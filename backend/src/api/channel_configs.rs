/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::error::AppError;
use crate::models::{
    ChannelCategory, ChannelCategoryRequest, ChannelConfig, ChannelConfigListResponse,
    ChannelConfigSafe, ChannelConfigSyncLog, ChannelConfigSyncLogDto, CreateChannelConfigRequest,
    UpdateChannelConfigRequest,
};
use crate::services::upstream_rate_sync::{
    applied_channel_rate, is_sync_due, newapi_pricing_url, parse_newapi_groups, UpstreamGroupRatio,
    SYSTEM_NEWAPI,
};
use crate::time_system::DbTs;
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use rand::Rng;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

fn sanitize_upstream_system(raw: &str) -> Result<String, AppError> {
    let value = raw.trim();
    if !crate::services::upstream_rate_sync::is_known_upstream_system(value) {
        return Err(AppError::BadRequest("不支持的上游系统".into()));
    }
    Ok(value.to_string())
}

fn sanitize_sync_interval(minutes: i64) -> i64 {
    minutes.clamp(0, 10_080)
}

fn sanitize_rate_add(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn sanitize_upstream_group(raw: &str) -> String {
    raw.trim().to_string()
}

fn sanitize_currency(raw: &str) -> String {
    raw.trim().to_string()
}

fn sanitize_rate(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        (value * 10000.0).round() / 10000.0
    } else {
        1.0
    }
}

fn sanitize_currency_rate(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

fn sanitize_category_ids(
    category_ids: Option<Vec<i64>>,
    category_id: Option<i64>,
) -> (Option<i64>, String) {
    let mut ids: Vec<i64> = match category_ids {
        Some(list) => list.into_iter().filter(|&id| id > 0).collect(),
        None => match category_id {
            Some(id) if id > 0 => vec![id],
            _ => Vec::new(),
        },
    };
    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(*id));
    let primary_id = ids.first().copied();
    let json_str = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
    (primary_id, json_str)
}

fn resolve_api_key(submitted: Option<&str>, stored: &str) -> String {
    match submitted {
        Some(key) if !key.trim().is_empty() && !key.contains("******") => key.trim().to_string(),
        _ => stored.to_string(),
    }
}

async fn fetch_newapi_groups(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<UpstreamGroupRatio>, AppError> {
    let url = newapi_pricing_url(base_url).map_err(AppError::BadRequest)?;
    let mut req = http.get(&url).header("Accept", "application/json");
    let key = api_key.trim();
    if !key.is_empty() {
        req = req.header("Authorization", format!("Bearer {key}"));
    }
    let resp = crate::services::http_client::with_timeout(req, Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| AppError::UpstreamError(format!("拉取上游分组倍率失败: {e}")))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppError::UpstreamError(format!(
            "上游定价接口 HTTP {}: {}",
            status.as_u16(),
            crate::relay::proxy::sanitize_error_message(&body)
        )));
    }
    parse_newapi_groups(&body).map_err(AppError::UpstreamError)
}

fn apply_selected_group(
    groups: &[UpstreamGroupRatio],
    group_name: &str,
    rate_add: f64,
    upstream_currency: &str,
    site_currency: &str,
    currency_rate: f64,
) -> Result<(f64, f64), AppError> {
    let found = groups
        .iter()
        .find(|g| g.name == group_name)
        .ok_or_else(|| AppError::BadRequest(format!("上游没有分组 {group_name}")))?;
    let applied = applied_channel_rate(
        found.ratio,
        rate_add,
        upstream_currency,
        site_currency,
        currency_rate,
    );
    Ok((found.ratio, applied))
}

pub async fn list_channel_configs(
    State(state): State<Arc<AppState>>,
    claims: Option<axum::Extension<crate::auth::Claims>>,
) -> Result<Json<ChannelConfigListResponse>, AppError> {
    let is_admin = claims.as_ref().map_or(false, |c| c.0.role == "admin");

    let configs: Vec<ChannelConfig> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM channel_configs ORDER BY sort_order DESC, id DESC"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    let total: i64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT COUNT(*) FROM channel_configs"),
    )
    .fetch_one(&state.db.pool)
    .await?;

    // 统计各上游预设绑定的模型渠道分组
    let channels: Vec<(
        i64,
        String,
        Option<String>,
        Option<i64>,
        String,
        String,
        i64,
    )> = sqlx::query_as(&state.db.format_query(
        "SELECT id, name, group_aid, preset_id, provider_type, config, status FROM channels",
    ))
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    let config_defaults: std::collections::HashMap<i64, (i64, i64)> = configs
        .iter()
        .map(|c| (c.id, (c.priority, c.weight)))
        .collect();

    let mut bindings_map: std::collections::HashMap<
        i64,
        Vec<crate::models::channel_config::BoundChannelSummary>,
    > = std::collections::HashMap::new();

    for (ch_id, name, group_aid, preset_id, provider_type, config_str, status) in channels {
        let is_ha = provider_type == "high_availability_group";
        let cfg_val: serde_json::Value =
            serde_json::from_str(&config_str).unwrap_or(serde_json::json!({}));

        let build_bound = |target_cid: i64, is_ha_flag: bool| {
            let (def_priority, def_weight) = config_defaults
                .get(&target_cid)
                .copied()
                .unwrap_or((0, 1));
            let ov = crate::models::channel::extract_upstream_override(&cfg_val, target_cid);
            let custom_priority = ov.as_ref().and_then(|o| o.priority);
            let custom_weight = ov.as_ref().and_then(|o| o.weight);
            let effective_priority = custom_priority.unwrap_or(def_priority);
            let effective_weight = custom_weight.unwrap_or(def_weight);
            let has_custom_schedule = custom_priority.is_some() || custom_weight.is_some();

            crate::models::channel_config::BoundChannelSummary {
                id: ch_id,
                name: name.clone(),
                group_aid: group_aid.clone(),
                is_ha: is_ha_flag,
                status,
                effective_priority,
                effective_weight,
                custom_priority,
                custom_weight,
                has_custom_schedule,
            }
        };

        if is_ha {
            if let Some(sub_ids) = cfg_val.get("sub_channels").and_then(|v| v.as_array()) {
                let mut seen = std::collections::HashSet::new();
                for sid_val in sub_ids {
                    if let Some(sid) = sid_val.as_i64() {
                        if seen.insert(sid) {
                            bindings_map
                                .entry(sid)
                                .or_default()
                                .push(build_bound(sid, true));
                        }
                    }
                }
            }
        } else if let Some(pid) = preset_id {
            bindings_map
                .entry(pid)
                .or_default()
                .push(build_bound(pid, false));
        }
    }

    let data: Vec<ChannelConfigSafe> = configs
        .into_iter()
        .map(|c| {
            let config_id = c.id;
            let mut safe = ChannelConfigSafe::from_with_role(c, is_admin);
            if let Some(bound) = bindings_map.remove(&config_id) {
                safe.bound_channel_count = bound.len() as i64;
                safe.bound_channels = bound;
            }
            safe
        })
        .collect();

    Ok(Json(ChannelConfigListResponse { data, total }))
}

async fn allocate_channel_config_yid(state: &AppState) -> Result<String, AppError> {
    // yid 仅展示；业务关联用 id。查重分配，格式 3 + 4 位（共 5 位，与 EID/PID 一致）
    for _ in 0..10 {
        let candidate = format!("3{:04}", rand::thread_rng().gen_range(0i64..10_000));
        let exists: i64 = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT COUNT(*) FROM channel_configs WHERE yid = ?"),
        )
        .bind(&candidate)
        .fetch_one(&state.db.pool)
        .await?;
        if exists == 0 {
            return Ok(candidate);
        }
    }
    Err(AppError::Internal("生成渠道配置 yid 失败，请重试".into()))
}

pub async fn create_channel_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateChannelConfigRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let yid = allocate_channel_config_yid(&state).await?;

    let status: i64 = if req.status == 0 { 0 } else { 1 };
    let daily_reset_hour = req.daily_reset_hour.unwrap_or(0).clamp(0, 23);
    let daily_reset_minute = req.daily_reset_minute.unwrap_or(0).clamp(0, 59);
    let daily_reset_cooldown_minutes = req.daily_reset_cooldown_minutes.unwrap_or(0).max(0);
    let upstream_system = sanitize_upstream_system(&req.upstream_system)?;
    let upstream_group = sanitize_upstream_group(&req.upstream_group);
    let upstream_sync_interval_minutes = sanitize_sync_interval(req.upstream_sync_interval_minutes);
    let upstream_sync_rate_add = sanitize_rate_add(req.upstream_sync_rate_add);
    let upstream_currency = sanitize_currency(&req.upstream_currency);
    let site_currency = sanitize_currency(&req.site_currency);
    let upstream_currency_rate = sanitize_currency_rate(req.upstream_currency_rate);
    let rate = sanitize_rate(req.rate);
    let (category_id, category_ids) = sanitize_category_ids(req.category_ids, req.category_id);

    let created: Option<ChannelConfig> = sqlx::query_as(
        &state.db.format_query(
            "INSERT INTO channel_configs (name, provider_type, base_url, api_key, remark, yid, sort_order, rate, priority, weight, quota_limit, daily_quota_limit, weekly_quota_limit, monthly_quota_limit, daily_reset_hour, daily_reset_minute, daily_reset_cooldown_minutes, status, category_id, category_ids, upstream_system, upstream_group, upstream_sync_interval_minutes, upstream_sync_rate_add, upstream_currency, site_currency, upstream_currency_rate) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *"
        )
    )
    .bind(&req.name)
    .bind(&req.provider_type)
    .bind(&req.base_url)
    .bind(&req.api_key)
    .bind(&req.remark)
    .bind(&yid)
    .bind(&req.sort_order)
    .bind(rate)
    .bind(&req.priority)
    .bind(&req.weight)
    .bind(req.quota_limit.unwrap_or(-1.0))
    .bind(req.daily_quota_limit.unwrap_or(-1.0))
    .bind(req.weekly_quota_limit.unwrap_or(-1.0))
    .bind(req.monthly_quota_limit.unwrap_or(-1.0))
    .bind(daily_reset_hour)
    .bind(daily_reset_minute)
    .bind(daily_reset_cooldown_minutes)
    .bind(status)
    .bind(category_id)
    .bind(&category_ids)
    .bind(&upstream_system)
    .bind(&upstream_group)
    .bind(upstream_sync_interval_minutes)
    .bind(upstream_sync_rate_add)
    .bind(&upstream_currency)
    .bind(&site_currency)
    .bind(upstream_currency_rate)
    .fetch_optional(&state.db.pool)
    .await?;

    crate::relay::router::invalidate_channel_configs_cache();

    if let Some(cfg) = created {
        if cfg.upstream_system == SYSTEM_NEWAPI && !cfg.upstream_group.is_empty() {
            let state_bg = state.clone();
            tokio::spawn(async move {
                let _ = sync_single_channel_config(&state_bg, &cfg, "auto", Utc::now()).await;
            });
        }
    }

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Debug, Default, Deserialize)]
pub struct CopyChannelConfigRequest {
    pub name: Option<String>,
}

fn resolve_copy_config_name(req: Option<&CopyChannelConfigRequest>, src_name: &str) -> String {
    req.and_then(|r| r.name.as_deref())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(src_name)
        .to_string()
}

pub async fn copy_channel_config(
    State(state): State<Arc<AppState>>,
    claims: Option<axum::Extension<crate::auth::Claims>>,
    Path(id): Path<i64>,
    req: Option<Json<CopyChannelConfigRequest>>,
) -> Result<Json<ChannelConfigSafe>, AppError> {
    let is_admin = claims.as_ref().map_or(false, |c| c.0.role == "admin");

    let src: ChannelConfig = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM channel_configs WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("上游渠道配置不存在".to_string()))?;

    let new_name = resolve_copy_config_name(req.as_ref().map(|Json(r)| r), &src.name);
    let yid = allocate_channel_config_yid(&state).await?;

    let created: ChannelConfig = sqlx::query_as(
        &state.db.format_query(
            "INSERT INTO channel_configs (name, provider_type, base_url, api_key, remark, yid, sort_order, rate, priority, weight, quota_limit, daily_quota_limit, weekly_quota_limit, monthly_quota_limit, daily_reset_hour, daily_reset_minute, daily_reset_cooldown_minutes, status, category_id, category_ids, upstream_system, upstream_group, upstream_sync_interval_minutes, upstream_sync_rate_add, upstream_currency, site_currency, upstream_currency_rate) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *"
        )
    )
    .bind(&new_name)
    .bind(&src.provider_type)
    .bind(&src.base_url)
    .bind(&src.api_key)
    .bind(&src.remark)
    .bind(&yid)
    .bind(src.sort_order)
    .bind(src.rate)
    .bind(src.priority)
    .bind(src.weight)
    .bind(src.quota_limit)
    .bind(src.daily_quota_limit)
    .bind(src.weekly_quota_limit)
    .bind(src.monthly_quota_limit)
    .bind(src.daily_reset_hour)
    .bind(src.daily_reset_minute)
    .bind(src.daily_reset_cooldown_minutes)
    .bind(src.status)
    .bind(src.category_id)
    .bind(&src.category_ids)
    .bind(&src.upstream_system)
    .bind(&src.upstream_group)
    .bind(src.upstream_sync_interval_minutes)
    .bind(src.upstream_sync_rate_add)
    .bind(&src.upstream_currency)
    .bind(&src.site_currency)
    .bind(src.upstream_currency_rate)
    .fetch_one(&state.db.pool)
    .await?;

    crate::relay::router::invalidate_channel_configs_cache();
    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    if created.upstream_system == SYSTEM_NEWAPI && !created.upstream_group.is_empty() {
        let state_bg = state.clone();
        let cfg_bg = created.clone();
        tokio::spawn(async move {
            let _ = sync_single_channel_config(&state_bg, &cfg_bg, "auto", Utc::now()).await;
        });
    }

    Ok(Json(ChannelConfigSafe::from_with_role(created, is_admin)))
}

pub async fn update_channel_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateChannelConfigRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut config: ChannelConfig = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM channel_configs WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Channel Config not found".to_string()))?;

    let prev_group = config.upstream_group.clone();
    let prev_interval = config.upstream_sync_interval_minutes;

    if let Some(name) = req.name {
        config.name = name;
    }
    if let Some(pt) = req.provider_type {
        config.provider_type = pt;
    }
    if let Some(bu) = req.base_url {
        config.base_url = bu;
    }
    if let Some(key) = req.api_key {
        // 【防护】含脱敏标记的值不覆盖原始密钥
        if !key.contains("******") {
            config.api_key = key;
        }
    }
    if let Some(rem) = req.remark {
        config.remark = Some(rem);
    }
    if let Some(so) = req.sort_order {
        config.sort_order = so;
    }
    if let Some(r) = req.rate {
        config.rate = sanitize_rate(r);
    }
    if let Some(p) = req.priority {
        config.priority = p;
    }
    if let Some(w) = req.weight {
        config.weight = w;
    }
    if let Some(q) = req.quota_limit {
        config.quota_limit = q;
    }
    if let Some(d) = req.daily_quota_limit {
        config.daily_quota_limit = d;
    }
    if let Some(w) = req.weekly_quota_limit {
        config.weekly_quota_limit = w;
    }
    if let Some(m) = req.monthly_quota_limit {
        config.monthly_quota_limit = m;
    }
    if let Some(h) = req.daily_reset_hour {
        config.daily_reset_hour = h.clamp(0, 23);
    }
    if let Some(m) = req.daily_reset_minute {
        config.daily_reset_minute = m.clamp(0, 59);
    }
    if let Some(c) = req.daily_reset_cooldown_minutes {
        config.daily_reset_cooldown_minutes = c.max(0);
    }
    if let Some(s) = req.status {
        config.status = if s == 0 { 0 } else { 1 };
    }
    if let Some(category_ids_opt) = req.category_ids {
        match category_ids_opt {
            Some(ids) => {
                let (cid, cids_str) = sanitize_category_ids(Some(ids), None);
                config.category_id = cid;
                config.category_ids = cids_str;
            }
            None => {
                config.category_id = None;
                config.category_ids = "[]".to_string();
            }
        }
    } else if let Some(category_id_opt) = req.category_id {
        match category_id_opt {
            Some(cid) => {
                let (cid, cids_str) = sanitize_category_ids(None, Some(cid));
                config.category_id = cid;
                config.category_ids = cids_str;
            }
            None => {
                config.category_id = None;
                config.category_ids = "[]".to_string();
            }
        }
    }
    if let Some(system) = req.upstream_system {
        config.upstream_system = sanitize_upstream_system(&system)?;
    }
    if let Some(group) = req.upstream_group {
        config.upstream_group = sanitize_upstream_group(&group);
    }
    if let Some(interval) = req.upstream_sync_interval_minutes {
        config.upstream_sync_interval_minutes = sanitize_sync_interval(interval);
    }
    if let Some(add) = req.upstream_sync_rate_add {
        config.upstream_sync_rate_add = sanitize_rate_add(add);
    }
    if let Some(uc) = req.upstream_currency {
        config.upstream_currency = sanitize_currency(&uc);
    }
    if let Some(sc) = req.site_currency {
        config.site_currency = sanitize_currency(&sc);
    }
    if let Some(ur) = req.upstream_currency_rate {
        config.upstream_currency_rate = sanitize_currency_rate(ur);
    }

    sqlx::query(
        &state.db.format_query(
            "UPDATE channel_configs SET name = ?, provider_type = ?, base_url = ?, api_key = ?, remark = ?, \
             sort_order = ?, rate = ?, priority = ?, weight = ?, quota_limit = ?, daily_quota_limit = ?, weekly_quota_limit = ?, monthly_quota_limit = ?, \
             daily_reset_hour = ?, daily_reset_minute = ?, daily_reset_cooldown_minutes = ?, status = ?, category_id = ?, category_ids = ?, \
             upstream_system = ?, upstream_group = ?, upstream_sync_interval_minutes = ?, upstream_sync_rate_add = ?, \
             upstream_currency = ?, site_currency = ?, upstream_currency_rate = ? \
             WHERE id = ?"
        )
    )
    .bind(&config.name)
    .bind(&config.provider_type)
    .bind(&config.base_url)
    .bind(&config.api_key)
    .bind(&config.remark)
    .bind(config.sort_order)
    .bind(config.rate)
    .bind(config.priority)
    .bind(config.weight)
    .bind(config.quota_limit)
    .bind(config.daily_quota_limit)
    .bind(config.weekly_quota_limit)
    .bind(config.monthly_quota_limit)
    .bind(config.daily_reset_hour)
    .bind(config.daily_reset_minute)
    .bind(config.daily_reset_cooldown_minutes)
    .bind(config.status)
    .bind(config.category_id)
    .bind(&config.category_ids)
    .bind(&config.upstream_system)
    .bind(&config.upstream_group)
    .bind(config.upstream_sync_interval_minutes)
    .bind(config.upstream_sync_rate_add)
    .bind(&config.upstream_currency)
    .bind(&config.site_currency)
    .bind(config.upstream_currency_rate)
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    // 上游渠道配置变更后清除关联的 HA 子渠道熔断记录
    // HA 子渠道的熔断 key 格式为 ha_group_{group_id}_config_{config_id}
    let config_suffix = format!("_config_{}", id);
    state
        .failed_channels
        .retain(|k, _| !k.ends_with(&config_suffix));

    // 如果该 config 被某个渠道作为 preset_id 引用，同步其生效的倍率/优先级/权重并清除熔断记录
    if let Ok(referring_channels) = sqlx::query_as::<_, crate::models::Channel>(
        &state
            .db
            .format_query("SELECT * FROM channels WHERE preset_id = ?"),
    )
    .bind(id)
    .fetch_all(&state.db.pool)
    .await
    {
        for referring_ch in referring_channels {
            if let Some(aid) = &referring_ch.group_aid {
                state.failed_channels.remove(aid);
            }
            if referring_ch.provider_type != "high_availability_group" {
                let resolved = referring_ch.resolve_upstream_config(&config);
                if (referring_ch.rate - resolved.rate).abs() > f64::EPSILON
                    || referring_ch.priority != resolved.priority
                    || referring_ch.weight != resolved.weight
                {
                    let _ = sqlx::query(&state.db.format_query(
                        "UPDATE channels SET rate = ?, priority = ?, weight = ? WHERE id = ?",
                    ))
                    .bind(resolved.rate)
                    .bind(resolved.priority)
                    .bind(resolved.weight)
                    .bind(referring_ch.id)
                    .execute(&state.db.pool)
                    .await;
                }
            }
        }
    }

    tracing::info!(
        "[ChannelConfig Update] 上游渠道配置 {} 已更新，已清除关联的熔断记录",
        id
    );

    crate::relay::router::invalidate_channel_configs_cache();
    crate::relay::router::invalidate_channel_cache();
    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    if config.upstream_system == SYSTEM_NEWAPI
        && !config.upstream_group.is_empty()
        && (config.upstream_synced_at.is_none()
            || config.upstream_group != prev_group
            || (prev_interval == 0 && config.upstream_sync_interval_minutes > 0))
    {
        let state_bg = state.clone();
        let cfg_bg = config.clone();
        tokio::spawn(async move {
            let _ = sync_single_channel_config(&state_bg, &cfg_bg, "auto", Utc::now()).await;
        });
    }

    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn delete_channel_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let count: i64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT COUNT(*) FROM channels WHERE preset_id = ?"),
    )
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    // 解绑引用该预设的单渠道分组，同时清空快照的 base_url / api_key，避免继续使用已删除上游的旧密钥转发
    if count > 0 {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE channels SET preset_id = NULL, base_url = '', api_key = '', updated_at = CURRENT_TIMESTAMP WHERE preset_id = ?"),
        )
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }

    // 同步清理高可用渠道组 config 中绑定的该子渠道 ID 及其独立调度/模型映射覆盖
    if let Ok(ha_rows) = sqlx::query_as::<_, (i64, String)>(
        &state
            .db
            .format_query("SELECT id, config FROM channels WHERE provider_type = 'high_availability_group'"),
    )
    .fetch_all(&state.db.pool)
    .await
    {
        let id_str = id.to_string();
        for (ch_id, cfg_str) in ha_rows {
            let Ok(mut cfg_val) = serde_json::from_str::<serde_json::Value>(&cfg_str) else {
                continue;
            };
            let mut changed = false;
            if let Some(subs) = cfg_val.get_mut("sub_channels").and_then(|v| v.as_array_mut()) {
                let before = subs.len();
                subs.retain(|v| v.as_i64() != Some(id));
                if subs.len() != before {
                    changed = true;
                }
            }
            if let Some(ovs) = cfg_val
                .get_mut("upstream_overrides")
                .and_then(|v| v.as_object_mut())
            {
                if ovs.remove(&id_str).is_some() {
                    changed = true;
                }
                if ovs.is_empty() {
                    cfg_val.as_object_mut().map(|o| o.remove("upstream_overrides"));
                }
            }
            if let Some(ha_map) = cfg_val
                .get_mut("ha_model_mapping")
                .and_then(|v| v.as_object_mut())
            {
                for sub_map in ha_map.values_mut() {
                    if let Some(obj) = sub_map.as_object_mut() {
                        if obj.remove(&id_str).is_some() {
                            changed = true;
                        }
                    }
                }
                ha_map.retain(|_, v| v.as_object().is_some_and(|o| !o.is_empty()));
                if ha_map.is_empty() {
                    cfg_val.as_object_mut().map(|o| o.remove("ha_model_mapping"));
                }
            }
            if let Some(res_map) = cfg_val
                .get_mut("res_model_mapping")
                .and_then(|v| v.as_object_mut())
            {
                for scopes in res_map.values_mut() {
                    if let Some(obj) = scopes.as_object_mut() {
                        if obj.remove(&id_str).is_some() {
                            changed = true;
                        }
                    }
                }
                res_map.retain(|_, v| v.as_object().is_some_and(|o| !o.is_empty()));
                if res_map.is_empty() {
                    cfg_val.as_object_mut().map(|o| o.remove("res_model_mapping"));
                }
            }
            if changed {
                if let Ok(new_cfg_str) = serde_json::to_string(&cfg_val) {
                    let _ = sqlx::query(
                        &state
                            .db
                            .format_query("UPDATE channels SET config = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"),
                    )
                    .bind(new_cfg_str)
                    .bind(ch_id)
                    .execute(&state.db.pool)
                    .await;
                }
            }
        }
    }

    let config_suffix = format!("_config_{}", id);
    state
        .failed_channels
        .retain(|k, _| !k.ends_with(&config_suffix));

    let _ = sqlx::query(
        &state
            .db
            .format_query("DELETE FROM channel_config_sync_logs WHERE config_id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await;

    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM channel_configs WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    crate::relay::channel_quota::clear_config_quota_buffer(id);
    crate::relay::router::invalidate_channel_configs_cache();
    crate::relay::router::invalidate_channel_cache();
    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(serde_json::json!({"success": true})))
}

/// 手动清零上游预设已用额度（总/日/月）
pub async fn reset_quota(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = sqlx::query(&state.db.format_query(
        &crate::models::channel_quota::reset_quota_sql("channel_configs"),
    ))
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("上游渠道配置不存在".into()));
    }

    crate::relay::channel_quota::clear_config_quota_buffer(id);
    crate::relay::router::invalidate_channel_configs_cache();

    tracing::info!(
        "[ChannelConfig Quota Reset] 管理员手动清零上游预设 {} 的已用额度",
        id
    );
    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Debug, Deserialize)]
pub struct FetchUpstreamGroupsRequest {
    pub config_id: Option<i64>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub upstream_system: Option<String>,
}

pub async fn fetch_upstream_groups(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FetchUpstreamGroupsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let system = sanitize_upstream_system(req.upstream_system.as_deref().unwrap_or(SYSTEM_NEWAPI))?;
    if system != SYSTEM_NEWAPI {
        return Err(AppError::BadRequest(
            "当前仅 newapi 支持拉取分组倍率".into(),
        ));
    }

    let mut base_url = req.base_url.unwrap_or_default();
    let mut api_key = req.api_key.unwrap_or_default();
    if let Some(id) = req.config_id {
        let stored: ChannelConfig = sqlx::query_as(
            &state
                .db
                .format_query("SELECT * FROM channel_configs WHERE id = ?"),
        )
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel Config not found".to_string()))?;
        if base_url.trim().is_empty() {
            base_url = stored.base_url.clone();
        }
        api_key = resolve_api_key(Some(&api_key), &stored.api_key);
    }

    if api_key.trim().is_empty() || api_key.contains("******") {
        return Err(AppError::BadRequest("请先填写请求鉴权密钥".into()));
    }

    let groups = fetch_newapi_groups(&state.http_client, &base_url, &api_key).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "data": groups.iter().map(|g| serde_json::json!({
            "name": g.name,
            "ratio": g.ratio,
            "label": g.label,
        })).collect::<Vec<_>>(),
    })))
}

async fn record_sync_log(
    state: &AppState,
    config: &ChannelConfig,
    upstream_ratio: f64,
    applied_rate: f64,
    trigger_type: &str,
    status: i64,
    error_message: &str,
    now: chrono::DateTime<Utc>,
) {
    let _ = sqlx::query(
        &state.db.format_query(
            "INSERT INTO channel_config_sync_logs (config_id, upstream_system, upstream_group, upstream_ratio, rate_add, upstream_currency, site_currency, currency_rate, previous_rate, applied_rate, sync_interval_minutes, trigger_type, status, error_message, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ),
    )
    .bind(config.id)
    .bind(&config.upstream_system)
    .bind(&config.upstream_group)
    .bind(upstream_ratio)
    .bind(config.upstream_sync_rate_add)
    .bind(&config.upstream_currency)
    .bind(&config.site_currency)
    .bind(config.upstream_currency_rate)
    .bind(config.rate)
    .bind(applied_rate)
    .bind(config.upstream_sync_interval_minutes)
    .bind(trigger_type)
    .bind(status)
    .bind(error_message)
    .bind(DbTs::from_utc(now))
    .execute(&state.db.pool)
    .await;

    let _ = sqlx::query(
        &state.db.format_query(
            "DELETE FROM channel_config_sync_logs WHERE config_id = ? AND id NOT IN (SELECT id FROM channel_config_sync_logs WHERE config_id = ? ORDER BY created_at DESC, id DESC LIMIT 200)",
        ),
    )
    .bind(config.id)
    .bind(config.id)
    .execute(&state.db.pool)
    .await;
}

async fn sync_single_channel_config(
    state: &AppState,
    config: &ChannelConfig,
    trigger_type: &str,
    now: chrono::DateTime<Utc>,
) -> Result<(f64, f64), AppError> {
    match fetch_newapi_groups(&state.http_client, &config.base_url, &config.api_key).await {
        Ok(groups) => match apply_selected_group(
            &groups,
            &config.upstream_group,
            config.upstream_sync_rate_add,
            &config.upstream_currency,
            &config.site_currency,
            config.upstream_currency_rate,
        ) {
            Ok((upstream_ratio, rate)) => {
                sqlx::query(
                    &state.db.format_query(
                        "UPDATE channel_configs SET rate = ?, upstream_synced_at = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                    ),
                )
                .bind(rate)
                .bind(DbTs::from_utc(now))
                .bind(config.id)
                .execute(&state.db.pool)
                .await?;

                let _ = sqlx::query(
                    &state.db.format_query(
                        "UPDATE channels SET rate = ? WHERE preset_id = ? AND provider_type != 'high_availability_group'",
                    ),
                )
                .bind(rate)
                .bind(config.id)
                .execute(&state.db.pool)
                .await;

                crate::relay::router::invalidate_channel_configs_cache();
                crate::relay::router::invalidate_channel_cache();
                record_sync_log(
                    state,
                    config,
                    upstream_ratio,
                    rate,
                    trigger_type,
                    1,
                    "",
                    now,
                )
                .await;
                tracing::info!(
                    "[UpstreamRateSync] 渠道 {} 分组 {} 倍率同步为 {} (trigger={})",
                    config.id,
                    config.upstream_group,
                    rate,
                    trigger_type
                );
                Ok((upstream_ratio, rate))
            }
            Err(e) => {
                let _ = sqlx::query(
                    &state.db.format_query(
                        "UPDATE channel_configs SET upstream_synced_at = ? WHERE id = ?",
                    ),
                )
                .bind(DbTs::from_utc(now))
                .bind(config.id)
                .execute(&state.db.pool)
                .await;
                record_sync_log(
                    state,
                    config,
                    0.0,
                    config.rate,
                    trigger_type,
                    0,
                    &e.to_string(),
                    now,
                )
                .await;
                tracing::warn!("[UpstreamRateSync] 渠道 {} 分组未命中: {e}", config.id);
                Err(e)
            }
        },
        Err(e) => {
            let _ = sqlx::query(
                &state.db.format_query(
                    "UPDATE channel_configs SET upstream_synced_at = ? WHERE id = ?",
                ),
            )
            .bind(DbTs::from_utc(now))
            .bind(config.id)
            .execute(&state.db.pool)
            .await;
            record_sync_log(
                state,
                config,
                0.0,
                config.rate,
                trigger_type,
                0,
                &e.to_string(),
                now,
            )
            .await;
            tracing::warn!("[UpstreamRateSync] 渠道 {} 拉取失败: {e}", config.id);
            Err(e)
        }
    }
}

pub async fn list_sync_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows: Vec<ChannelConfigSyncLog> = sqlx::query_as(
        &state.db.format_query(
            "SELECT * FROM channel_config_sync_logs WHERE config_id = ? ORDER BY created_at DESC, id DESC LIMIT 100",
        ),
    )
    .bind(id)
    .fetch_all(&state.db.pool)
    .await?;

    let data: Vec<ChannelConfigSyncLogDto> = rows
        .into_iter()
        .map(ChannelConfigSyncLogDto::from)
        .collect();
    Ok(Json(serde_json::json!({
        "success": true,
        "data": data,
    })))
}

pub async fn trigger_sync_rate(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config: ChannelConfig = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM channel_configs WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("上游渠道配置不存在".into()))?;

    if config.upstream_system != SYSTEM_NEWAPI || config.upstream_group.trim().is_empty() {
        return Err(AppError::BadRequest(
            "当前渠道未配置 newapi 分组倍率同步".into(),
        ));
    }

    let now = Utc::now();
    let (upstream_ratio, rate) =
        sync_single_channel_config(&state, &config, "manual", now).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "upstream_ratio": upstream_ratio,
        "rate": rate,
        "upstream_synced_at": DbTs::from_utc(now).into_string(),
    })))
}

pub async fn run_upstream_rate_sync_tick(state: Arc<AppState>) {
    let configs: Vec<ChannelConfig> = match sqlx::query_as(
        &state.db.format_query(
            "SELECT * FROM channel_configs WHERE upstream_system = ? AND upstream_group <> '' AND upstream_sync_interval_minutes > 0",
        ),
    )
    .bind(SYSTEM_NEWAPI)
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("[UpstreamRateSync] 查询待同步渠道失败: {e}");
            return;
        }
    };

    let now = Utc::now();
    for config in configs {
        let synced = config.upstream_synced_at.as_ref().map(|t| t.as_str());
        if !is_sync_due(synced, config.upstream_sync_interval_minutes, now) {
            continue;
        }
        let _ = sync_single_channel_config(&state, &config, "auto", now).await;
    }
}

pub async fn list_providers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ChannelCategory>>, AppError> {
    let providers = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM channel_providers ORDER BY sort_order DESC, id ASC"),
    )
    .fetch_all(&state.db.pool)
    .await?;
    Ok(Json(providers))
}

pub async fn create_provider(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<ChannelCategoryRequest>,
) -> Result<Json<ChannelCategory>, AppError> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(AppError::BadRequest("服务商名称不能为空".to_string()));
    }

    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM channel_providers WHERE name = ?"),
    )
    .bind(&req.name)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(AppError::Conflict("服务商名称已存在".to_string()));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let provider = sqlx::query_as(
        &state.db.format_query(
            "INSERT INTO channel_providers (name, name_en, sort_order, is_active) VALUES (?, ?, ?, ?) RETURNING *",
        ),
    )
    .bind(&req.name)
    .bind(name_en)
    .bind(req.sort_order)
    .bind(req.is_active)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(provider))
}

pub async fn update_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(mut req): Json<ChannelCategoryRequest>,
) -> Result<Json<ChannelCategory>, AppError> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(AppError::BadRequest("服务商名称不能为空".to_string()));
    }

    let old_name: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT name FROM channel_providers WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    let Some(old_name) = old_name else {
        return Err(AppError::NotFound("服务商不存在".to_string()));
    };

    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM channel_providers WHERE name = ? AND id != ?"),
    )
    .bind(&req.name)
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(AppError::Conflict("服务商名称与其他记录重复".to_string()));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let provider: ChannelCategory = sqlx::query_as(
        &state.db.format_query(
            "UPDATE channel_providers SET name = ?, name_en = ?, sort_order = ?, is_active = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? RETURNING *",
        ),
    )
    .bind(&req.name)
    .bind(name_en)
    .bind(req.sort_order)
    .bind(req.is_active)
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    if old_name != req.name {
        sqlx::query(
            &state.db.format_query(
                "UPDATE channel_configs SET provider_type = ?, updated_at = CURRENT_TIMESTAMP WHERE provider_type = ?",
            ),
        )
        .bind(&req.name)
        .bind(&old_name)
        .execute(&state.db.pool)
        .await?;
        crate::relay::router::invalidate_channel_configs_cache();
        crate::api::plugins::notify_marketplace_data_changed(&state).await;
    }

    Ok(Json(provider))
}

pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let old_name: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT name FROM channel_providers WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    if let Some(name) = old_name {
        sqlx::query(
            &state.db.format_query(
                "UPDATE channel_configs SET provider_type = '', updated_at = CURRENT_TIMESTAMP WHERE provider_type = ?",
            ),
        )
        .bind(&name)
        .execute(&state.db.pool)
        .await?;
    }

    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM channel_providers WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    crate::relay::router::invalidate_channel_configs_cache();
    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(serde_json::json!({ "success": true })))
}

