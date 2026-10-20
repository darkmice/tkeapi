/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#![allow(dead_code)]
use crate::error::{AppError, AppResult};
use crate::models::{Channel, ChannelConfig};
use crate::AppState;
use rand::Rng;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

/// 活跃渠道短 TTL 读缓存（5 秒自动过期，写路径主动清空；杜绝数据库全表读写冲撞）
const ACTIVE_CHANNELS_TTL: Duration = Duration::from_secs(5);
/// 上游渠道配置短 TTL 读缓存（5 秒自动过期，写路径主动清空；杜绝高可用选子渠道穿透查库）
const CHANNEL_CONFIGS_TTL: Duration = Duration::from_secs(5);

struct ChannelsTtlCell {
    channels: Arc<Vec<Channel>>,
    at: Instant,
}

struct ChannelConfigsTtlCell {
    configs: Arc<HashMap<i64, ChannelConfig>>,
    at: Instant,
}

static ACTIVE_CHANNELS_CACHE: OnceLock<RwLock<Option<ChannelsTtlCell>>> = OnceLock::new();
static CHANNEL_CONFIGS_CACHE: OnceLock<RwLock<Option<ChannelConfigsTtlCell>>> = OnceLock::new();

fn get_cache_lock() -> &'static RwLock<Option<ChannelsTtlCell>> {
    ACTIVE_CHANNELS_CACHE.get_or_init(|| RwLock::new(None))
}

fn get_configs_cache_lock() -> &'static RwLock<Option<ChannelConfigsTtlCell>> {
    CHANNEL_CONFIGS_CACHE.get_or_init(|| RwLock::new(None))
}

/// 主动使渠道缓存失效（管理端增删改启停时调用）
pub fn invalidate_channel_cache() {
    if let Ok(mut guard) = get_cache_lock().write() {
        *guard = None;
    }
}

/// 主动使渠道配置缓存失效（管理端修改上游配置时调用）
pub fn invalidate_channel_configs_cache() {
    if let Ok(mut guard) = get_configs_cache_lock().write() {
        *guard = None;
    }
}

pub async fn get_cached_channel_configs(
    db: &crate::db::Database,
) -> Result<Arc<HashMap<i64, ChannelConfig>>, sqlx::Error> {
    if let Ok(guard) = get_configs_cache_lock().read() {
        if let Some(ref cell) = *guard {
            if cell.at.elapsed() < CHANNEL_CONFIGS_TTL {
                return Ok(cell.configs.clone());
            }
        }
    }

    let rows: Vec<ChannelConfig> =
        sqlx::query_as(&db.format_query("SELECT * FROM channel_configs"))
            .fetch_all(&db.pool)
            .await?;

    let map: HashMap<i64, ChannelConfig> = rows.into_iter().map(|c| (c.id, c)).collect();
    let arc_configs = Arc::new(map);
    if let Ok(mut guard) = get_configs_cache_lock().write() {
        *guard = Some(ChannelConfigsTtlCell {
            configs: arc_configs.clone(),
            at: Instant::now(),
        });
    }
    Ok(arc_configs)
}

pub async fn get_cached_active_channels(
    db: &crate::db::Database,
) -> Result<Arc<Vec<Channel>>, sqlx::Error> {
    if let Ok(guard) = get_cache_lock().read() {
        if let Some(ref cell) = *guard {
            if cell.at.elapsed() < ACTIVE_CHANNELS_TTL {
                return Ok(cell.channels.clone());
            }
        }
    }

    let channels: Vec<Channel> = sqlx::query_as(
        &db.format_query("SELECT * FROM channels WHERE status = 1 ORDER BY priority DESC, id DESC"),
    )
    .fetch_all(&db.pool)
    .await?;

    let arc_channels = Arc::new(channels);
    if let Ok(mut guard) = get_cache_lock().write() {
        *guard = Some(ChannelsTtlCell {
            channels: arc_channels.clone(),
            at: Instant::now(),
        });
    }
    Ok(arc_channels)
}

/// 同档加权随机：`weight` 越大被选概率越高；负权重按 0 计，合计 ≤0 时取首项。
fn pick_weighted_by<T, F>(items: &[T], weight_of: F) -> &T
where
    F: Fn(&T) -> i64,
{
    debug_assert!(!items.is_empty());
    let weight = |item: &T| weight_of(item).max(0);
    let total_weight: i64 = items.iter().map(|i| weight(i)).sum();
    if total_weight <= 0 {
        return &items[0];
    }
    let mut rng = rand::rngs::OsRng;
    let random_value = rng.gen_range(0..total_weight);
    let mut current_sum = 0;
    for item in items {
        current_sum += weight(item);
        if random_value < current_sum {
            return item;
        }
    }
    &items[0]
}

/// Select the best channel for a given model based on priority and load balancing.
pub async fn select_channel(
    state: &Arc<AppState>,
    model: &str,
    user_group: &str,
    level_id: &str,
    exclude_aids: &[String],
    mids: Option<&[String]>,
    ha_pool: &mut Option<super::ha::HaPoolSnap>,
) -> AppResult<Channel> {
    crate::relay_debug!(
        "[SelectChannel] 开始 模型={} 分组={} 等级={} 已排除={:?}",
        model,
        user_group,
        level_id,
        exclude_aids
    );

    // 1. 一次查出该 model_id 的 mid / 是否在模型表；调用方已传入 mids 则免查
    let owned_mids: Vec<String>;
    let model_in_table;
    let mids_ref = if let Some(m) = mids {
        model_in_table = true;
        m
    } else {
        let rows: Vec<(String, i64)> = sqlx::query_as(&state.db.format_query(
            "SELECT mid, is_active FROM models WHERE model_id = ? AND is_listed = 1",
        ))
        .bind(model)
        .fetch_all(&state.db.pool)
        .await
        .unwrap_or_default();
        model_in_table = !rows.is_empty();
        owned_mids = rows
            .into_iter()
            .filter(|(_, on)| *on == 1)
            .map(|(mid, _)| mid)
            .collect();
        &owned_mids
    };

    // exclude_aids：仅过滤物理渠 group_aid；ha_group_* 由内存过滤/子渠逻辑处理
    let sql_excludes: Vec<&str> = exclude_aids
        .iter()
        .filter(|a| !a.starts_with("ha_group_"))
        .map(|s| s.as_str())
        .collect();

    // 2. 从 5 秒短 TTL 内存缓存获取全部活跃渠道与上游配置（避免高并发下每秒数十次全表扫描冲撞 DB）
    let all_active_channels = get_cached_active_channels(&state.db).await?;
    let all_channel_configs = get_cached_channel_configs(&state.db).await?;
    let tz_name = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let (now_day, now_week, now_month) = crate::models::quota_period_keys(&tz_name);

    let pattern_model = format!("\"{}\"", model);
    let pattern_mids: Vec<String> = mids_ref.iter().map(|m| format!("\"{}\"", m)).collect();
    let ug_str = format!("\"{}\"", user_group);
    let level_str = format!("\"{}\"", level_id);

    // 内存迭代过滤（耗时 < 5 微秒，0 数据库 I/O，保持 priority DESC 顺序）
    let channels: Vec<Channel> = all_active_channels
        .iter()
        .filter(|c| {
            if let Some(ref aid) = c.group_aid {
                if sql_excludes.contains(&aid.as_str()) {
                    return false;
                }
            }
            c.has_available_quota(&now_day, &now_week, &now_month)
                && c.matches_model(&pattern_model, &pattern_mids)
                && c.matches_user_group(&ug_str, &level_str)
        })
        .cloned()
        .collect();

    crate::relay_debug!(
        "[SelectChannel] 内存候选数={} 渠道id={:?}",
        channels.len(),
        channels.iter().map(|c| c.id).collect::<Vec<i64>>()
    );

    if channels.is_empty() {
        crate::relay_debug!(
            "[SelectChannel] 未命中 模型={} 分组={} 等级={}",
            model,
            user_group,
            level_id
        );
        let bound = all_active_channels
            .iter()
            .any(|c| c.matches_model(&pattern_model, &pattern_mids));
        if !bound && !model_in_table {
            return Err(AppError::NotFound(format!("模型不存在: {}", model)));
        }
        if !bound {
            return Err(AppError::NotFound(format!("模型未配置渠道: {}", model)));
        }
        return Err(no_usable_channels(model));
    }

    // 预加载非 HA 渠道绑定的上游预设，过滤禁用 / 额度耗尽的预设（直接从内存缓存检索，0 I/O）
    let mut unusable_presets: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for c in &channels {
        if c.provider_type != "high_availability_group" {
            if let Some(pid) = c.preset_id {
                let unusable = all_channel_configs.get(&pid).map_or(true, |p| {
                    p.status != 1 || !p.has_available_quota(&tz_name, &now_week, &now_month)
                });
                if unusable {
                    unusable_presets.insert(pid);
                }
            }
        }
    }

    // 过滤：熔断 / 黑名单 / 不允许 HA / 整组 exclude（ha_group_{id}）/ 预设禁用或额度耗尽
    let mut channels: Vec<Channel> = channels
        .into_iter()
        .filter(|c| {
            if c.provider_type == "high_availability_group" {
                let group_key = format!("ha_group_{}", c.id);
                if exclude_aids.iter().any(|a| a == &group_key) {
                    return false;
                }
            } else if let Some(pid) = c.preset_id {
                if unusable_presets.contains(&pid) {
                    return false;
                }
            } else if c.provider_type != "volcengine"
                && c.provider_type != "comfyui"
                && c.base_url.trim().is_empty()
            {
                return false;
            }
            let excludes = c.get_exclude_user_groups();
            if !excludes.is_empty()
                && (excludes.contains(&user_group.to_string())
                    || excludes.contains(&level_id.to_string()))
            {
                return false;
            }
            if let Some(ref aid) = c.group_aid {
                if crate::relay::ha::is_melted_down(state, aid) {
                    return false;
                }
            }
            true
        })
        .collect();

    crate::relay_debug!(
        "[SelectChannel] 过滤后={} 渠道id={:?}",
        channels.len(),
        channels.iter().map(|c| c.id).collect::<Vec<i64>>()
    );

    if channels.is_empty() {
        return Err(no_usable_channels(model));
    }

    // 针对绑定上游预设的非 HA 渠道，按当前渠道分组的独立覆盖参数（未设置则回退上游默认）解析最终 priority/weight/rate 并重排
    for c in &mut channels {
        if c.provider_type != "high_availability_group" {
            if let Some(pid) = c.preset_id {
                if let Some(preset) = all_channel_configs.get(&pid) {
                    let resolved = c.resolve_upstream_config(preset);
                    c.priority = resolved.priority;
                    c.weight = resolved.weight;
                    c.rate = resolved.rate;
                }
            }
        }
    }
    channels.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| b.id.cmp(&a.id)));

    // 物理渠 / HA 子渠：最高 priority 档内按 weight 比例随机分流
    // 顺序不变量：HA 子配注入并清 preset_id →（仅剩 preset 时）查 preset → 最后 volc
    let mut ch = loop {
        let highest_priority = channels[0].priority;
        let top_tier: Vec<Channel> = channels
            .iter()
            .take_while(|c| c.priority == highest_priority)
            .cloned()
            .collect();
        let picked = pick_weighted_by(&top_tier, |c| c.weight).clone();

        if picked.provider_type != "high_availability_group" {
            break picked;
        }

        let sub_channel_ids: Vec<i64> = serde_json::from_str::<serde_json::Value>(&picked.config)
            .ok()
            .and_then(|v| v.get("sub_channels").cloned())
            .and_then(|v| serde_json::from_value::<Vec<i64>>(v).ok())
            .unwrap_or_default();

        if sub_channel_ids.is_empty() {
            *ha_pool = Some(super::ha::HaPoolSnap::default());
            return Err(AppError::NotFound(format!(
                "高可用虚拟渠道组 (ID: {}) 配置异常，未绑定任何上游子渠道",
                picked.id
            )));
        }

        let rows: Vec<crate::models::ChannelConfig> = sub_channel_ids
            .iter()
            .filter_map(|id| {
                all_channel_configs
                    .get(id)
                    .map(|cfg| picked.resolve_upstream_config(cfg))
            })
            .collect();

        let group_id = picked.id;
        let (sub_configs, pool) = live_ha_subs(
            &sub_channel_ids,
            rows,
            group_id,
            exclude_aids,
            state,
            &tz_name,
            &now_week,
            &now_month,
        );
        *ha_pool = Some(pool);

        if sub_configs.is_empty() {
            // 整组不可用：剔除后重选，避免误报盖住其它可用物理渠
            channels.retain(|c| c.id != group_id);
            if channels.is_empty() {
                crate::relay_debug!("[SelectChannel] HA组 {} 子渠已耗尽", group_id);
                return Err(no_usable_channels(model));
            }
            continue;
        }

        // 统一加权负载均衡：最高 priority 档；同档按 weight 比例随机分流（彻底杜绝普通令牌单点打爆首个子渠道）
        let highest_sub_priority = sub_configs.iter().map(|c| c.priority).max().unwrap_or(0);
        let top_subs: Vec<&_> = sub_channel_ids
            .iter()
            .filter_map(|&id| {
                sub_configs
                    .iter()
                    .find(|c| c.id == id && c.priority == highest_sub_priority)
            })
            .collect();
        if top_subs.is_empty() {
            channels.retain(|c| c.id != group_id);
            if channels.is_empty() {
                return Err(no_usable_channels(model));
            }
            continue;
        }
        let selected_sub = *pick_weighted_by(&top_subs, |c| c.weight);

        let mut resolved = picked;
        apply_ha_sub_mapped(&mut resolved, selected_sub);
        break resolved;
    };

    // 4. Resolve preset (channel config template)
    if let Some(pid) = ch.preset_id {
        if let Some(preset) = all_channel_configs.get(&pid) {
            if preset.status != 1 {
                crate::relay_debug!("[SelectChannel] 预设已禁用 渠道={} 预设id={}", ch.id, pid);
                return Err(AppError::NotFound(format!(
                    "上游渠道配置已禁用 (preset_id={})",
                    pid
                )));
            }
            crate::relay_debug!(
                "[SelectChannel] 套用预设 渠道={} 子渠标识={:?} 预设id={} {} -> {}",
                ch.id,
                ch.group_aid,
                pid,
                ch.base_url,
                preset.base_url
            );
            apply_config_base(&mut ch, preset);
        } else {
            crate::relay_debug!(
                "[SelectChannel] 预设缺失 渠道={} 子渠标识={:?} 预设id={}",
                ch.id,
                ch.group_aid,
                pid
            );
        }
    }

    // 5. 画质增强凭证集成：从 config 中的凭证 ID 实时查询最新密钥（保证插件端修改凭证后渠道分组数据一致）
    apply_volcengine_credential(state, &mut ch).await;
    apply_comfyui_channel(state, &mut ch).await;

    crate::relay_debug!(
        "[SelectChannel] 选中 渠道id={} 上游YID={} 名称='{}' 类型={} 子渠标识={:?} 地址={}",
        ch.id,
        crate::relay::ha::yid_label(ch.yid.as_deref()),
        ch.name,
        ch.provider_type,
        ch.group_aid,
        ch.base_url
    );

    Ok(ch)
}

fn no_usable_channels(model: &str) -> AppError {
    AppError::NotFound(format!("模型无可用渠道: {}", model))
}

fn live_ha_subs(
    bound_ids: &[i64],
    rows: Vec<crate::models::ChannelConfig>,
    group_id: i64,
    exclude_aids: &[String],
    state: &AppState,
    tz_name: &str,
    now_week: &str,
    now_month: &str,
) -> (Vec<crate::models::ChannelConfig>, super::ha::HaPoolSnap) {
    let mut by_id: std::collections::HashMap<i64, crate::models::ChannelConfig> =
        rows.into_iter().map(|c| (c.id, c)).collect();
    let mut live = Vec::new();
    let mut off = 0u16;
    let mut melt = 0u16;
    let mut quota = 0u16;
    let mut excl = 0u16;
    let mut gone = 0u16;
    for id in bound_ids {
        let Some(sub) = by_id.remove(id) else {
            gone += 1;
            continue;
        };
        if sub.status != 1 {
            off += 1;
            continue;
        }
        let key = format!("ha_group_{}_config_{}", group_id, sub.id);
        if exclude_aids.contains(&key) {
            excl += 1;
            continue;
        }
        if crate::relay::ha::is_melted_down(state, &key) {
            melt += 1;
            continue;
        }
        if !sub.has_available_quota(tz_name, now_week, now_month) {
            quota += 1;
            continue;
        }
        live.push(sub);
    }
    let n = live.len() as u16;
    (
        live,
        super::ha::HaPoolSnap {
            bind: bound_ids.len() as u16,
            live: n,
            off,
            melt,
            quota,
            excl,
            gone,
        },
    )
}

/// 将 channel_configs 的 base/key/rate/priority/weight/yid 写入内存 Channel（选渠 / 重载共用，自动叠加分组内独立覆盖参数）
#[inline]
fn apply_config_base(ch: &mut crate::models::Channel, cfg: &crate::models::ChannelConfig) {
    let resolved = ch.resolve_upstream_config(cfg);
    ch.base_url = resolved.base_url;
    ch.api_key = resolved.api_key;
    ch.rate = resolved.rate;
    ch.priority = resolved.priority;
    ch.weight = resolved.weight;
    ch.yid = Some(resolved.yid);
}

/// HA 子配注入：写 base/key/rate/yid/name/provider/group_aid，清 preset_id
/// `name` 用子配名（内存态，供 ha_usage_logs）；父渠 DB 名不变，logs JOIN 仍显示组名
#[inline]
fn apply_ha_sub(ch: &mut crate::models::Channel, cfg: &crate::models::ChannelConfig) {
    let group_id = ch.id;
    apply_config_base(ch, cfg);
    ch.name = cfg.name.clone();
    ch.provider_type = cfg.provider_type.clone();
    ch.group_aid = Some(format!("ha_group_{}_config_{}", group_id, cfg.id));
    ch.preset_id = None; // 防止后续父行 preset 覆盖子配
}

/// HA 子配 + `ha_model_mapping`（选渠 / 重载 / 渠道测试唯一入口，避免漏映射）
#[inline]
pub(crate) fn apply_ha_sub_mapped(
    ch: &mut crate::models::Channel,
    cfg: &crate::models::ChannelConfig,
) {
    apply_ha_sub(ch, cfg);
    apply_ha_model_mapping(ch, cfg.id);
}

/// 按子配 id 叠加父渠 `ha_model_mapping`（保证异步 settle / 测试与同步选渠一致）
#[inline]
pub(crate) fn apply_ha_model_mapping(ch: &mut crate::models::Channel, sub_id: i64) {
    let Ok(config_val) = serde_json::from_str::<serde_json::Value>(&ch.config) else {
        return;
    };
    let Some(ha_mapping) = config_val
        .get("ha_model_mapping")
        .and_then(|v| v.as_object())
    else {
        return;
    };
    let mut base_mapping: std::collections::HashMap<String, String> =
        serde_json::from_str(&ch.model_mapping).unwrap_or_default();
    let sub_id_str = sub_id.to_string();
    for (model_id, sub_map) in ha_mapping {
        if let Some(alias) = sub_map.get(&sub_id_str).and_then(|v| v.as_str()) {
            if !alias.is_empty() {
                base_mapping.insert(model_id.clone(), alias.to_string());
            }
        }
    }
    ch.model_mapping = serde_json::to_string(&base_mapping).unwrap_or_else(|_| "{}".to_string());
}

/// 将已查到的子配/预设写入内存 Channel（HA 走 apply_ha_sub_mapped；物理走 base）
#[inline]
fn apply_reload_cfg(ch: &mut crate::models::Channel, cfg: &crate::models::ChannelConfig) {
    if ch.provider_type == "high_availability_group" {
        apply_ha_sub_mapped(ch, cfg);
    } else {
        apply_config_base(ch, cfg);
        ch.preset_id = Some(cfg.id);
    }
}

/// 按 ID 加载并水合渠道，产出与 `select_channel` 一致的运行时 Channel（含最终 base_url/api_key/yid/rate/映射）。
/// `channel_config_id`：日志中的子配快照；HA 异步轮询必传，以还原当时选中的子配。
pub async fn fetch_channel(
    state: &crate::AppState,
    channel_id: i64,
    channel_config_id: Option<i64>,
) -> Option<Channel> {
    let mut ch: Channel =
        sqlx::query_as(&state.db.format_query("SELECT * FROM channels WHERE id = ?"))
            .bind(channel_id)
            .fetch_optional(&state.db.pool)
            .await
            .ok()??;

    hydrate_for_reload(state, &mut ch, channel_config_id).await;
    Some(ch)
}

/// 重载路径水合：channel_config_id → 父行 preset_id → volc。
/// 与 select 不同：此处无 HA 加权选子，只能靠日志快照还原当时子配。
async fn hydrate_for_reload(
    state: &crate::AppState,
    ch: &mut crate::models::Channel,
    channel_config_id: Option<i64>,
) {
    if let Some(cid) = channel_config_id {
        if let Ok(Some(cfg)) = sqlx::query_as::<_, crate::models::ChannelConfig>(
            &state
                .db
                .format_query("SELECT * FROM channel_configs WHERE id = ?"),
        )
        .bind(cid)
        .fetch_optional(&state.db.pool)
        .await
        {
            apply_reload_cfg(ch, &cfg);
            apply_volcengine_credential(state, ch).await;
            apply_comfyui_channel(state, ch).await;
            return;
        }
    }
    if let Some(pid) = ch.preset_id {
        if let Ok(Some(preset)) = sqlx::query_as::<_, crate::models::ChannelConfig>(
            &state
                .db
                .format_query("SELECT * FROM channel_configs WHERE id = ?"),
        )
        .bind(pid)
        .fetch_optional(&state.db.pool)
        .await
        {
            apply_config_base(ch, &preset);
        }
    }
    apply_volcengine_credential(state, ch).await;
    apply_comfyui_channel(state, ch).await;
}

/// 解析最终上游模型名（全站唯一入口）。
/// 优先级：分辨率映射 > 渠道映射（含 HA 子渠明文）> 模型表别名 > 原始 id。
/// `resolution`：已规范化更佳；`None` 则跳过分辨率映射。
pub fn resolve_model(
    channel: &Channel,
    requested_model: &str,
    db_model: Option<&crate::models::Model>,
    resolution: Option<&str>,
) -> (String, Option<String>) {
    if let Some(res) = resolution {
        if let Some((alias, res_key)) = resolve_res_alias(channel, requested_model, res) {
            let src = format!("分辨率映射@{}", res_key);
            crate::relay_debug!("[ModelMap] {} {} → {}", src, requested_model, alias);
            return (alias, Some(src));
        }
    }
    let resolved = channel.resolve_model(requested_model);
    if resolved != requested_model {
        return (resolved, Some("渠道映射".into()));
    }
    if let Some(m) = db_model {
        if !m.model_id_alias.is_empty() {
            return (m.model_id_alias.clone(), Some("模型映射".into()));
        }
    }
    (resolved, None)
}

/// 图片/视频请求入口：从 body 抽分辨率后走 [`resolve_model`]（聊天/语音等勿用）
#[inline]
pub fn resolve_model_body(
    channel: &Channel,
    requested_model: &str,
    db_model: Option<&crate::models::Model>,
    body: Option<&serde_json::Value>,
) -> (String, Option<String>) {
    let res = body.and_then(crate::relay::usage_extractor::extract_resolution);
    resolve_model(channel, requested_model, db_model, res.as_deref())
}

/// 结算等场景：仅图片/视频类别才带上已提取分辨率做映射
#[inline]
pub fn mapping_resolution<'a>(
    category: Option<&str>,
    resolution: Option<&'a str>,
) -> Option<&'a str> {
    category
        .filter(|c| c.contains("图片") || c.contains("视频"))
        .and(resolution)
}

/// `config.res_model_mapping`: `{ model_id: { "default"|sub_id: { "480p"|"1k": alias } } }`
/// 回退：子渠该档 → 默认别名该档 →（未命中则走明文别名）
fn resolve_res_alias(
    channel: &Channel,
    requested_model: &str,
    resolution: &str,
) -> Option<(String, String)> {
    let config: serde_json::Value = serde_json::from_str(&channel.config).ok()?;
    let scopes = config
        .get("res_model_mapping")?
        .get(requested_model)?
        .as_object()?;
    let res_key = crate::relay::usage_extractor::normalize_resolution_label(resolution);
    if res_key.is_empty() {
        return None;
    }
    let pick = |scope: &str| -> Option<String> {
        let map = scopes.get(scope)?.as_object()?;
        res_alias_in_map(map, &res_key)
    };
    if let Some(sid) = crate::relay::ha::ha_config_id_from_aid(channel) {
        if let Some(alias) = pick(&sid.to_string()) {
            return Some((alias, res_key));
        }
    }
    pick("default").map(|alias| (alias, res_key))
}

#[inline]
fn res_alias_in_map(
    map: &serde_json::Map<String, serde_json::Value>,
    res_key: &str,
) -> Option<String> {
    if let Some(a) = map
        .get(res_key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(a.to_string());
    }
    // 兼容历史未规范 key（如 "480" / "720P"）
    map.iter().find_map(|(k, v)| {
        if crate::relay::usage_extractor::normalize_resolution_label(k) != res_key {
            return None;
        }
        v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
    })
}

/// 画质增强凭证集成：根据 config 中的凭证 ID 实时查询 plugin_configs 表获取最新密钥并覆盖到渠道
/// （供 select_channel / fetch_channel / 渠道测试共用）
pub(crate) async fn apply_volcengine_credential(
    state: &crate::AppState,
    ch: &mut crate::models::Channel,
) {
    if ch.provider_type != "volcengine" {
        return;
    }
    let cred_id = match serde_json::from_str::<serde_json::Value>(&ch.config)
        .ok()
        .and_then(|cfg| {
            cfg.get("volcengine_enhance_credential_id")?
                .as_str()
                .map(|s| s.to_string())
        }) {
        Some(id) => id,
        None => return,
    };
    let keys_str: Option<String> = sqlx::query_scalar(
        &state.db.format_query(
            "SELECT config_value FROM plugin_configs WHERE plugin_name = 'volcengine_enhance' AND config_key = 'keys'"
        )
    ).fetch_optional(&state.db.pool).await.ok().flatten();
    if let Some(ref ks) = keys_str {
        if let Ok(keys) = serde_json::from_str::<Vec<serde_json::Value>>(ks) {
            if let Some(k) = keys
                .iter()
                .find(|k| k.get("id").and_then(|v| v.as_str()) == Some(&cred_id))
            {
                ch.api_key = k
                    .get("api_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let bu = k
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                ch.base_url = if bu.is_empty() {
                    "https://mediakit.cn-beijing.volces.com".to_string()
                } else {
                    bu.to_string()
                };
            }
        }
    }
}

#[cfg(feature = "plugin_comfyui")]
pub(crate) use crate::api::plugins::comfyui_bridge::apply_channel_url as apply_comfyui_channel;

#[cfg(not(feature = "plugin_comfyui"))]
pub(crate) async fn apply_comfyui_channel(_: &crate::AppState, _: &mut crate::models::Channel) {}
