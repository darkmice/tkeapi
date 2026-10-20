/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! Shared proxy utilities — user context, billing, logging.
//! All relay handlers reuse these to avoid code duplication.

use super::router;
use crate::error::{AppError, AppResult};
use crate::models::ApiToken;
use crate::models::Channel;
use crate::AppState;
use futures::StreamExt;
use regex::Regex;
use std::sync::Arc;

// ── User Context ────────────────────────────────────────────────

#[derive(Clone)]
pub struct UserContext {
    pub user_id: String,
    pub user_group: String,
    pub level_id: String,
    pub balance: f64,
    pub discount: f64,
    pub discount_type: i64,
    /// 用户模型单独折扣(JSON: {"mid": discount})，优先于用户等级折扣
    pub model_discounts: Option<String>,
}

impl UserContext {
    /// 仅折扣字段参与计费时的轻量上下文（异步任务结算等）
    pub fn from_discounts(
        discount: f64,
        discount_type: i64,
        model_discounts: Option<String>,
    ) -> Self {
        Self {
            user_id: String::new(),
            user_group: String::new(),
            level_id: String::new(),
            balance: 0.0,
            discount,
            discount_type,
            model_discounts,
        }
    }
}

pub async fn get_user_context(state: &AppState, user_id: &str) -> AppResult<UserContext> {
    // 查询用户信息、等级折扣、折扣模式、模型单独折扣（用于计费时优先匹配用户模型折扣）
    let (mut g, mut l_id, b, gb, cl, mut d, mut dt, md): (
        String,
        i64,
        f64,
        f64,
        f64,
        f64,
        i64,
        Option<String>,
    ) = sqlx::query_as(&state.db.format_query(
        "SELECT u.user_group, COALESCE(ul.id, 0), u.balance, u.gift_balance, u.credit_limit, COALESCE(ul.discount, 1.0), COALESCE(ul.discount_type, 0), u.model_discounts \
         FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key \
         WHERE u.id = ?",
    ))
    .bind(user_id)
    .fetch_one(&state.db.pool)
    .await?;

    // 若用户归属等级已被删除或失配（l_id == 0），回退继承系统默认等级的折扣与掩码，防退化为 0 (DISC_ALL)
    if l_id == 0 {
        if let Ok(Some((def_id, def_group, def_d, def_dt))) =
            sqlx::query_as::<_, (i64, String, f64, i64)>(&state.db.format_query(
                "SELECT id, group_key, discount, discount_type FROM user_levels \
                 WHERE is_default = 1 OR group_key = 'default' \
                 ORDER BY is_default DESC, id ASC LIMIT 1",
            ))
            .fetch_optional(&state.db.pool)
            .await
        {
            l_id = def_id;
            g = def_group;
            d = def_d;
            dt = def_dt;
        }
    }

    Ok(UserContext {
        user_id: user_id.to_string(),
        user_group: g,
        level_id: l_id.to_string(),
        // 可用额 = 系统 + 赠送 + 信控；统一 6 位，避免浮点残渣误判为「有余额」
        balance: crate::money::round_money(b + gb + cl),
        discount: d,
        discount_type: dt,
        model_discounts: md,
    })
}

pub const DISC_LEVEL: i64 = 1;      // 1. 用户等级折扣
pub const DISC_USER_MODEL: i64 = 2; // 2. 用户模型折扣
pub const DISC_GLOBAL: i64 = 4;     // 3. 模型全站折扣
pub const DISC_CHANNEL: i64 = 8;    // 4. 上游渠道倍率
pub const DISC_ALL: i64 = 15;
pub const DISC_V2_FLAG: i64 = 16;

#[inline]
pub fn normalize_discount_mask(discount_type: i64) -> i64 {
    if discount_type >= DISC_V2_FLAG {
        discount_type & DISC_ALL
    } else {
        match discount_type {
            1 => DISC_USER_MODEL | DISC_GLOBAL,
            2 => DISC_LEVEL | DISC_USER_MODEL,
            _ => DISC_ALL,
        }
    }
}

/// 统一折扣策略：对勾选的折扣项（等级/用户模型/全站/渠道倍率）取最低值，再由 `clamp_discount_floor` 保底限价
pub fn resolve_discount(
    db_model: Option<&crate::models::Model>,
    level_discount: f64,
    user_model_discount: Option<f64>,
    channel_rate: f64,
    discount_type: i64,
) -> (f64, &'static str) {
    let mask = normalize_discount_mask(discount_type);
    let global_disc = db_model
        .filter(|m| m.global_discount_enabled == 1)
        .map(|m| m.global_discount);
    let candidates = [
        (DISC_LEVEL, Some(level_discount), "用户等级折扣"),
        (DISC_USER_MODEL, user_model_discount, "用户模型折扣"),
        (DISC_GLOBAL, global_disc, "全站折扣"),
        (DISC_CHANNEL, Some(channel_rate), "渠道倍率"),
    ];

    let mut min_discount = f64::MAX;
    let mut source = "默认倍率";
    for (bit, val_opt, label) in candidates {
        if (mask & bit) != 0 {
        if let Some(val) = val_opt.filter(|&v| {
            v.is_finite()
                && v < min_discount
                && if bit == DISC_GLOBAL { v >= 0.0 } else { v > 0.0 }
        }) {
            min_discount = val;
            source = label;
        }
        }
    }

    if min_discount == f64::MAX {
        (1.0, "默认倍率")
    } else {
        (min_discount, source)
    }
}

/// 折扣限价：有效倍率不得低于模型 `site_discount`（未建档模型默认以 1.0 保底防穿透）
#[inline]
pub fn clamp_discount_floor(
    effective: f64,
    db_model: Option<&crate::models::Model>,
) -> Option<f64> {
    let floor = match db_model {
        Some(m) if m.site_discount_enabled == 1 => m.site_discount,
        Some(_) => return None,
        None => 1.0,
    };
    (effective < floor - 1e-6).then_some(floor)
}

/// 从用户 model_discounts JSON 中提取指定模型(mid)的单独折扣
pub fn parse_user_model_discount(model_discounts: &Option<String>, mid: &str) -> Option<f64> {
    let json_str = model_discounts.as_ref()?;
    let map: std::collections::HashMap<String, f64> = serde_json::from_str(json_str).ok()?;
    map.get(mid).copied()
}

// ── Model Lookup (支持同名模型按类型区分) ────────────────────────

/// 请求内已锁定峰谷的计费规则；HA 环外建一份，`billing_rule_id` 未变则免重复查库。
pub type BillingRuleCache = Option<(i64, crate::models::BillingRule)>;

/// 查询模型关联计费规则并锁定峰谷倍率；`cache` 命中同 `billing_rule_id` 则直接复用。
pub async fn get_model_billing_rule(
    state: &AppState,
    model_id: &str,
    channel: Option<&crate::models::Channel>,
    db_model: Option<&crate::models::Model>,
    cache: &mut BillingRuleCache,
) -> Option<crate::models::BillingRule> {
    let rule_id = if let Some(m) = db_model {
        m.billing_rule_id?
    } else {
        find_active_model_exact(state, model_id, None, channel)
            .await?
            .billing_rule_id?
    };
    if let Some((_, rule)) = cache.as_ref().filter(|(id, _)| *id == rule_id) {
        return Some(rule.clone());
    }
    let mut rule: crate::models::BillingRule = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM billing_rules WHERE id = ? AND is_active = 1"),
    )
    .bind(rule_id)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None)?;

    let default_site_tz = super::relay_settings::get_cached_site_timezone(&state.db).await;
    rule.lock_time_multiplier(&default_site_tz);
    *cache = Some((rule_id, rule.clone()));
    Some(rule)
}

/// 按 model_id 查找活跃模型，可选传入 category 以区分同名但不同类型的模型。
/// category: Some("图片") / Some("视频") / Some("聊天") / None（不限类型）
pub async fn find_active_model_exact(
    state: &AppState,
    model_id: &str,
    category: Option<&str>,
    channel: Option<&crate::models::Channel>,
) -> Option<crate::models::Model> {
    // category 过滤条件（参数化绑定防止 SQL 注入）
    let cat_filter = if category.is_some() {
        " AND t.name = ?"
    } else {
        ""
    };

    // 1. 获取所有匹配的活跃模型候选（ORDER BY m.id 保证多候选时返回顺序确定性）
    let sql = format!(
        "SELECT m.*, t.name AS type_name FROM models m LEFT JOIN model_types t ON m.type_id = t.id WHERE m.model_id = ? AND m.is_active = 1 AND m.is_listed = 1{} ORDER BY m.id",
        cat_filter
    );
    let formatted_sql = state.db.format_query(&sql);
    let mut query = sqlx::query_as(&formatted_sql).bind(model_id);
    if let Some(cat) = category {
        query = query.bind(cat);
    }
    let mut candidates: Vec<crate::models::Model> =
        query.fetch_all(&state.db.pool).await.unwrap_or_default();

    if candidates.is_empty() && category.is_some() {
        let fallback_sql = "SELECT m.*, t.name AS type_name FROM models m LEFT JOIN model_types t ON m.type_id = t.id WHERE m.model_id = ? AND m.is_active = 1 AND m.is_listed = 1 ORDER BY m.id";
        candidates = sqlx::query_as(&state.db.format_query(fallback_sql))
            .bind(model_id)
            .fetch_all(&state.db.pool)
            .await
            .unwrap_or_default();
    }

    if candidates.is_empty() {
        return None;
    }

    // 2. 如果提供了渠道，且存在多个候选模型，尝试精确匹配渠道包含的 mid
    if let Some(ch) = channel {
        let ch_models = ch.get_models(); // 可能是 mid 数组，也可能是旧版的 model_id 数组
        if !ch_models.is_empty() {
            // 优先匹配 mid（精确锁定唯一模型记录及其关联计费规则）
            if let Some(exact) = candidates.iter().find(|m| ch_models.contains(&m.mid)) {
                return allow_volc_preset(state, exact.clone()).await;
            }
            // 兜底匹配 model_id
            if let Some(exact) = candidates.iter().find(|m| ch_models.contains(&m.model_id)) {
                return allow_volc_preset(state, exact.clone()).await;
            }
        }
    }

    // 3. 默认返回第一个（ORDER BY m.id 保证确定性）
    allow_volc_preset(state, candidates.into_iter().next().unwrap()).await
}

/// 插件关闭时，MediaKit 预置模型不能被当成已激活模型解析。
async fn allow_volc_preset(
    state: &AppState,
    model: crate::models::Model,
) -> Option<crate::models::Model> {
    #[cfg(feature = "plugin_volcengine_enhance")]
    if crate::api::plugins::volc_preset_blocked(state, &model.mid).await {
        return None;
    }
    #[cfg(not(feature = "plugin_volcengine_enhance"))]
    let _ = state;
    Some(model)
}

/// 根据 mid 查找处于激活状态的模型数据
pub async fn find_active_model_by_mid(state: &AppState, mid: &str) -> Option<crate::models::Model> {
    let sql = "SELECT m.*, t.name AS type_name FROM models m LEFT JOIN model_types t ON m.type_id = t.id WHERE (m.mid = ? OR m.library_mid = ?) AND m.is_active = 1 AND m.is_listed = 1 LIMIT 1";
    match sqlx::query_as(&state.db.format_query(sql))
        .bind(mid)
        .bind(mid)
        .fetch_optional(&state.db.pool)
        .await
        .unwrap_or(None)
    {
        Some(model) => allow_volc_preset(state, model).await,
        None => None,
    }
}

// ── Access Check ────────────────────────────────────────────────

/// 根据 category 推断标准 endpoint 路径（用于错误日志记录）
pub fn category_endpoint(category: Option<&str>) -> &'static str {
    match category {
        Some("图片") => "/v1/images/generations",
        Some("视频") | Some("封装") => "/v1/video/generations",
        Some("音频") => "/v1/audio/speech",
        Some("向量") => "/v1/embeddings",
        Some("排序") => "/v1/rerank",
        _ => "/v1/chat/completions",
    }
}

/// 入口类型与模型真实类型是否互通（视频 ↔ 封装）
#[inline]
fn category_compatible(expected: &str, resolved: &str) -> bool {
    expected == resolved || matches!((expected, resolved), ("视频", "封装") | ("封装", "视频"))
}

/// 类型隔离失败文案：真实类型 + 实际入口（action_type 另记入口，见 check_access_with_model）
#[inline]
fn type_mismatch_message(model: &str, resolved_cat: &str, expected_cat: &str) -> String {
    format!(
        "模型 '{}' 为 '{}' 类型，不支持当前 '{}' 接口请求",
        model, resolved_cat, expected_cat
    )
}

/// 路径→类型兜底（仅鉴权中间件 / 历史日志补全等「业务模块尚未透传」场景）。
/// 业务失败日志应优先透传模块已知的 category，勿依赖本函数。
pub fn action_type_from_path(endpoint: &str) -> Option<&'static str> {
    let ep = endpoint
        .split('|')
        .next()
        .unwrap_or(endpoint)
        .to_ascii_lowercase();
    // 更具体的规则在前
    const RULES: &[(&str, &str)] = &[
        ("enhance-video", "封装"),
        ("erase-video", "封装"),
        ("enhance-image", "图片"),
        ("remove-image-background", "图片"),
        ("contents/generations", "视频"),
        ("video-generation", "视频"),
        ("video-synthesis", "视频"),
        ("videos", "视频"),
        ("/video/", "视频"),
        ("multimodal-generation", "图片"),
        ("/images/", "图片"),
        ("/image/", "图片"),
        ("/audio/", "音频"),
        ("/tts", "音频"),
        ("/speech", "音频"),
        ("embedding", "向量"),
        ("rerank", "排序"),
        ("/chat/", "聊天"),
        ("/v1/messages", "聊天"),
        ("/responses", "聊天"),
        ("v1beta/models", "聊天"),
    ];
    RULES.iter().find(|(k, _)| ep.contains(k)).map(|(_, v)| *v)
}

/// Token 模型权限校验（渠道选择 **之前** 调用，快速拦截未授权模型）。
/// `action_type`: 调用方已知类别（如 "图片"），失败落库时透传，保证日志 Tab 定位正确。
pub async fn check_model_permission(
    state: &Arc<AppState>,
    token: &ApiToken,
    model: &str,
    endpoint: &str,
    action_type: Option<&str>,
) -> AppResult<()> {
    if !token.is_model_allowed(model) {
        let msg = format!("Model {} does not exist or you do not have access to it", model);
        record_error_log(
            state,
            token,
            None,
            model,
            404,
            endpoint,
            action_type.unwrap_or_default(),
            &msg,
            None,
        )
        .await;
        return Err(AppError::NotFound(msg));
    }
    Ok(())
}

/// 请求内按 channel.id 复用鉴权结果（同 HA 组换子配免再查 models）。
pub struct AccessMemo {
    channel_id: i64,
    pre_deduction: f64,
    db_model: Option<crate::models::Model>,
    resolved_cat: String,
}
pub type AccessCache = Option<AccessMemo>;

/// 类型安全隔离 + 预扣费余额检查。调用方须先 `check_model_permission`。
/// `pre_fetched_model` 有值则跳过查表；`cache` 命中同 `channel.id` 则整段复用。
pub async fn check_access_with_model(
    state: &Arc<AppState>,
    token: &ApiToken,
    model: &str,
    ctx: &UserContext,
    category: Option<&str>,
    channel: &crate::models::Channel,
    pre_fetched_model: Option<&crate::models::Model>,
    cache: &mut AccessCache,
) -> AppResult<(f64, Option<crate::models::Model>, String)> {
    if let Some(memo) = cache.as_ref().filter(|m| m.channel_id == channel.id) {
        return Ok((
            memo.pre_deduction,
            memo.db_model.clone(),
            memo.resolved_cat.clone(),
        ));
    }

    let db_model = if let Some(m) = pre_fetched_model {
        Some(m.clone())
    } else {
        find_active_model_exact(state, model, category, Some(channel)).await
    };

    // 获取真实分类（图像工具模型在协议域统一收敛为「图片」）
    let mut resolved_cat = if let Some(ref m) = db_model {
        m.type_name
            .clone()
            .unwrap_or_else(|| category.unwrap_or("").to_string())
    } else {
        category.unwrap_or("").to_string()
    };
    if db_model.as_ref().is_some_and(crate::api::plugins::is_plugin_image_tool) && resolved_cat == "封装" {
        resolved_cat = "图片".to_string();
    }

    let ep = category_endpoint(category);
    let ch_id = Some(channel.id);

    let up_url = Some(channel.base_url.as_str());

    // 类型安全隔离：action_type 记入口 expected（Tab=endpoint），文案带模型真实类型 resolved
    if let Some(expected_cat) = category {
        if db_model.is_some() && !category_compatible(expected_cat, &resolved_cat) {
            let msg = type_mismatch_message(model, &resolved_cat, expected_cat);
            record_error_log(
                state,
                token,
                ch_id,
                model,
                400,
                ep,
                expected_cat,
                &msg,
                up_url,
            )
            .await;
            return Err(AppError::BadRequest(msg));
        }
    }

    let pre_deduction =
        crate::money::round_money(db_model.as_ref().map(|m| m.pre_deduction).unwrap_or(0.0));
    // ctx.balance = 系统+赠送+信控（get_user_context 已 round）
    let avail = ctx.balance;
    let insufficient = if pre_deduction > 0.0 {
        avail < pre_deduction
    } else {
        avail <= 0.0
    };
    if insufficient {
        let msg = if pre_deduction > 0.0 {
            let currency_unit = crate::api::settings::get_currency_settings(state)
                .await
                .currency_unit;
            format!("账户余额不足{}{}", pre_deduction, currency_unit)
        } else {
            "余额不足".to_string()
        };
        record_error_log(
            state,
            token,
            ch_id,
            model,
            402,
            ep,
            &resolved_cat,
            &msg,
            up_url,
        )
        .await;
        return Err(AppError::PaymentRequired(msg));
    }

    // 低余额：按类别限制未完成任务路数（金额门禁已过；不二次扣在途预扣）
    let inflight_cat = if resolved_cat == "封装" {
        category.unwrap_or("封装")
    } else {
        resolved_cat.as_str()
    };
    if let Err(e) =
        super::relay_settings::enforce_inflight_gate(&state.db, &token.user_id, avail, inflight_cat)
            .await
    {
        if let AppError::TooManyRequests(msg) = &e {
            record_error_log(
                state,
                token,
                ch_id,
                model,
                429,
                ep,
                &resolved_cat,
                msg,
                up_url,
            )
            .await;
        }
        return Err(e);
    }

    *cache = Some(AccessMemo {
        channel_id: channel.id,
        pre_deduction,
        db_model: db_model.clone(),
        resolved_cat: resolved_cat.clone(),
    });
    Ok((pre_deduction, db_model, resolved_cat))
}

// ── Channel Selection ───────────────────────────────────────────

pub async fn select_channel_for_model(
    state: &Arc<AppState>,
    token: &ApiToken,
    model: &str,
    user_group: &str,
    level_id: &str,
    endpoint: &str,
    exclude_aids: &[String],
    log_miss: bool,
    action_type: Option<&str>,
    ha_pool: &mut Option<super::ha::HaPoolSnap>,
) -> AppResult<Channel> {
    select_channel_with_db(
        state,
        token,
        model,
        user_group,
        level_id,
        endpoint,
        None,
        exclude_aids,
        log_miss,
        action_type,
        ha_pool,
    )
    .await
}

/// 渠道选择（支持透传 Model 实体，提取其 mid 规避 select_channel 内部的查表动作）。
/// `log_miss`: 选渠失败时是否写入日志。failover 循环中若已有上游错误应传 false，终态为无可用渠道时传 true。
/// `action_type`: 调用方已知类别，选渠失败落库时透传。
pub async fn select_channel_with_db(
    state: &Arc<AppState>,
    token: &ApiToken,
    model: &str,
    user_group: &str,
    level_id: &str,
    endpoint: &str,
    db_model: Option<&crate::models::Model>,
    exclude_aids: &[String],
    log_miss: bool,
    action_type: Option<&str>,
    ha_pool: &mut Option<super::ha::HaPoolSnap>,
) -> AppResult<Channel> {
    let mids = db_model.map(|m| vec![m.mid.clone()]);
    match router::select_channel(
        state,
        model,
        user_group,
        level_id,
        exclude_aids,
        mids.as_deref(),
        ha_pool,
    )
    .await
    {
        Ok(ch) => Ok(ch),
        Err(e) => {
            if log_miss {
                let msg = if let AppError::NotFound(ref m) = e {
                    m.clone()
                } else {
                    e.to_string()
                };
                record_error_log(
                    state,
                    token,
                    None,
                    model,
                    404,
                    endpoint,
                    action_type.unwrap_or_default(),
                    &msg,
                    None,
                )
                .await;
            }
            Err(e)
        }
    }
}

// ── Record Usage & Billing ──────────────────────────────────────

use super::url_utils::join_url;

/// 事务化预扣费：全局对齐加锁顺序（先 logs 后 users），彻底杜绝与结算/退款事务死锁；
/// 持有 users 锁后极速完成扣除并立即 commit（锁持有时间 < 0.3ms）。
/// 保证崩溃后孤儿清理/启动恢复可按日志退款（避免「钱包已扣、日志 cost=0」）。
/// 返回赠送钱包实扣金额（供 `pre_deduct_gift` 落库）。
pub async fn pre_deduct(
    state: &Arc<AppState>,
    user_id: &str,
    amount: f64,
    pending_log_id: Option<i64>,
) -> Result<f64, sqlx::Error> {
    if amount <= 0.0 {
        return Ok(0.0);
    }
    let amount = crate::money::round_money(amount);
    let mut tx = state.db.pool.begin().await?;

    // 1. 若存在 pending 日志，先锁 logs 行（对齐全局锁顺序：先 logs 后 users，消除死锁）
    if let Some(log_id) = pending_log_id {
        let _exists: i64 = sqlx::query_scalar(&state.db.format_query(
            "SELECT id FROM logs WHERE id = ? FOR UPDATE",
        ))
        .bind(log_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| sqlx::Error::Protocol("pre_deduct: pending log missing".into()))?;
    }

    // 2. 锁定 users 行并校验可用余额
    let (bal, gift, credit): (f64, f64, f64) = sqlx::query_as(&state.db.format_query(
        "SELECT balance, gift_balance, credit_limit FROM users WHERE id = ? FOR UPDATE",
    ))
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;

    // 预扣只看账户（余额 + 赠送 + 信控）。模型卡券不参与预扣，结算时再承担并退回对应预扣。
    if crate::money::round_money(bal + gift + credit) < amount {
        tx.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    }
    let (gift_deducted, balance_deducted) = crate::money::split_gift_first(amount, gift);

    // 3. 写入 logs 预扣凭证（行已在阶段 1 锁定，HA 续试时可重置状态供清理命中）
    // coupon_deduct 预扣阶段恒为 0，真实结算才写入会员面值。
    if let Some(log_id) = pending_log_id {
        sqlx::query(&state.db.format_query(
            "UPDATE logs SET cost = ?, pre_deduct_gift = ?, coupon_deduct = 0, pre_deduct_voucher = 0, status_code = 0, is_completed = 0, \
             error_message = NULL, billing_detail = '请求处理中' WHERE id = ?",
        ))
        .bind(amount)
        .bind(gift_deducted)
        .bind(log_id)
        .execute(&mut *tx)
        .await?;
    }

    // 4. 扣减 users 钱包，紧随 commit 极速放锁（持有 users 锁 < 0.3ms）
    sqlx::query(&state.db.format_query(
        "UPDATE users SET balance = balance - ?, gift_balance = gift_balance - ? WHERE id = ?",
    ))
    .bind(balance_deducted)
    .bind(gift_deducted)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(gift_deducted)
}

/// 预扣费；余额不足时写 402 日志并返回 PaymentRequired（金额≤0 跳过；角色无豁免）
pub async fn pre_deduct_or_intercept(
    state: &Arc<AppState>,
    token: &ApiToken,
    channel: &crate::models::Channel,
    model: &str,
    pre_deduction: f64,
    ep: &str,
    start_time: std::time::Instant,
    is_stream: i64,
    request_content_str: &str,
    upstream_body_str: &str,
    ep_tag: Option<String>,
    pending_log_id: Option<i64>,
    db_model: Option<&crate::models::Model>,
    category: Option<&str>,
) -> AppResult<f64> {
    if pre_deduction <= 0.0 {
        return Ok(0.0);
    }
    match pre_deduct(state, &token.user_id, pre_deduction, pending_log_id).await {
        Ok(gift) => Ok(gift),
        Err(e) => {
            let err_msg = match &e {
                sqlx::Error::RowNotFound => "余额不足".to_string(),
                sqlx::Error::Protocol(msg) if msg.contains("pending log missing") => {
                    "预扣记账失败，请重试".to_string()
                }
                _ => format!("预扣费失败: {:?}", e),
            };
            let is_balance = matches!(e, sqlx::Error::RowNotFound);
            if is_balance {
                crate::relay_debug!("[PreDeduct] 余额不足 用户ID={}", token.user_id);
            } else {
                tracing::warn!("[PreDeduct] 预扣费失败 用户ID={}: {:?}", token.user_id, e);
            }
            let latency_ms = start_time.elapsed().as_millis() as u32;
            let status_code = if is_balance { 402 } else { 500 };
            let _ = record_zero_cost_fail(ZeroCostUpstreamFail {
                state,
                token,
                channel,
                model,
                prefer_http_status: Some(status_code),
                endpoint: ep,
                latency_ms,
                is_stream,
                request_content: request_content_str.to_string(),
                response_body: err_msg.clone(),
                response_content: Some(err_msg.clone()),
                upstream_req_content: Some(upstream_body_str.to_string()),
                billing_detail: ep_tag,
                hint_category: category,
                pending_log_id,
                db_model,
                client_msg: Some(&err_msg),
                pre_deducted: 0.0,
                pre_deduct_gift: 0.0,
                upstream_request_id: None,
            })
            .await;
            Err(if is_balance {
                AppError::PaymentRequired("余额不足".to_string())
            } else {
                AppError::Internal(err_msg)
            })
        }
    }
}

// ── 预记录日志（请求前写入） ────────────────────────────────────
//
// 【一条日志原则】每个模型请求全生命周期只产生一条日志记录：
//   1. 请求发送前：调用 record_pending_log 插入 status_code=0 的"处理中"日志
//   2. 请求完成后：调用 record_and_bill* 系列函数时传入 pending_log_id，
//      通过 UPDATE 更新该日志行的最终状态、响应、计费等信息
//   3. 其他开发者/AI 在新增模型请求端点时必须遵循此原则，不得额外 INSERT 日志
//

/// Base64 数据脱敏：将请求/响应内容中的 base64 长串替换为占位符，减少日志体积。
/// 供预记录和最终记录共用，保证数据处理一致性。
pub fn sanitize_base64(text: &str) -> String {
    static RE_URI: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static RE_RAW: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re_uri =
        RE_URI.get_or_init(|| Regex::new(r"data:[^;]+;base64,[A-Za-z0-9+/=]{100,}").unwrap());
    let text = re_uri.replace_all(text, "base64数据");
    let re_raw = RE_RAW.get_or_init(|| Regex::new(r#""[A-Za-z0-9+/]{200,}={0,2}""#).unwrap());
    re_raw.replace_all(&text, "\"base64数据\"").into_owned()
}

/// 预记录日志参数（命名字段，避免位置参数踩坑）
pub struct PendingLog<'a> {
    pub state: &'a Arc<AppState>,
    pub user_id: &'a str,
    pub token_id: i64,
    pub model: &'a str,
    pub endpoint: &'a str,
    pub is_stream: i64,
    pub request_content: Option<&'a str>,
    pub upstream_url: Option<&'a str>,
    pub channel: &'a crate::models::Channel,
    pub plugin_tag: Option<&'a str>,
    pub category: Option<&'a str>,
    pub db_model: Option<&'a crate::models::Model>,
    pub forward_eid: Option<&'a str>,
    pub requested_log_id: Option<&'a str>,
    /// 受理时就要能按这个号查询。空则保持空串，由后续结算从响应里提取。
    pub task_id: Option<&'a str>,
}

/// 在上游请求发送前预记录一条"处理中"日志（status_code=0），返回 logs 主键 id。
/// 使用户能立即在日志页面看到请求记录，而不必等待上游响应。
/// 存入的信息包括：用户信息、渠道、模型、请求参数、端点、流式标志等。
/// 预记录阶段不存储 upstream_req_content（上游请求参数），因为此时请求尚未真正发送给上游，
/// 该字段在请求完成后由 record_and_bill_inner UPDATE 写入。
/// 预记录阶段即执行 URL 密钥脱敏、Base64 脱敏和上下文开关控制，与最终日志保持数据安全一致性。
pub async fn record_pending_log(p: PendingLog<'_>) -> Option<i64> {
    let PendingLog {
        state,
        user_id,
        token_id,
        model,
        endpoint,
        is_stream,
        request_content,
        upstream_url,
        channel,
        plugin_tag,
        category,
        db_model,
        forward_eid,
        requested_log_id,
        task_id,
    } = p;
    let task_id = task_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let (meta_cat, billing_pid, enable_log) =
        resolve_model_meta(state, model, category, Some(channel), db_model).await;
    let action_type = category
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .unwrap_or(meta_cat);
    let generated_log_id = requested_log_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| super::upstream_headers::resolve_request_log_id(Some(&action_type)));
    let forward_eid: Option<String> = forward_eid.filter(|s| !s.is_empty()).map(|s| s.to_string());

    let channel_config_id = super::ha::resolve_log_config_id(state, channel).await;
    let is_ha = super::ha::channel_is_ha_flag(channel);
    let masked_url: Option<String> =
        upstream_url.map(|u| super::forward::mask_key_in_string(u, &channel.api_key));

    let stored_req: Option<String> = if enable_log > 0 {
        request_content.map(sanitize_base64)
    } else {
        None
    };

    let sql = state.db.format_query(
        "INSERT INTO logs (log_id, user_id, channel_id, token_id, model, prompt_tokens, completion_tokens, \
         cached_tokens, cost, status_code, endpoint, error_message, latency_ms, \
         request_content, response_content, is_stream, upstream_url, \
         billing_detail, task_id, action_type, billing_pid, forward_eid, plugin_tag, channel_config_id, is_ha) \
         VALUES (?, ?, ?, ?, ?, 0, 0, 0, 0.0, 0, ?, NULL, 0, ?, NULL, ?, ?, \
                 '请求处理中', ?, ?, ?, ?, ?, ?, ?) RETURNING id"
    );

    let sys_ep = endpoint.split_once('|').map(|(s, _)| s).unwrap_or(endpoint);

    let res = sqlx::query_scalar::<_, i64>(&sql)
        .bind(&generated_log_id)
        .bind(user_id)
        .bind(channel.id)
        .bind(token_id)
        .bind(model)
        .bind(sys_ep)
        .bind(stored_req.as_deref())
        .bind(is_stream)
        .bind(masked_url.as_deref())
        .bind(task_id)
        .bind(&action_type)
        .bind(&billing_pid)
        .bind(&forward_eid)
        .bind(plugin_tag.unwrap_or(""))
        .bind(channel_config_id)
        .bind(is_ha)
        .fetch_one(&state.db.pool)
        .await;

    super::relay_settings::release_pre_log_inflight(user_id, &action_type);

    match res {
        Ok(id) => {
            crate::relay_debug!(
                "[PendingLog] ID={} 日志号={} 模型={} 端点={}",
                id,
                generated_log_id,
                model,
                sys_ep
            );
            Some(id)
        }
        Err(e) => {
            tracing::warn!("[PendingLog] 预记录失败: {:?}", e);
            None
        }
    }
}

/// 从 models 表解析模型元信息（action_type / billing_pid / enable_log_content）
/// 供预记录和最终记录复用，避免代码重复。
/// channel: 可选渠道信息，用于同 model_id 多条记录时精确匹配渠道绑定的 mid，
///          确保 billing_pid 与实际计费规则一致。
/// db_model: 调用方已查询的 Model 记录（主键定位，极快且无歧义）。传 None 时按 model_id 查询。
/// forward_eid 不在此函数查询——各端点在 resolve_forward_rule 时已获取，直接透传给 record_pending_log。
async fn resolve_model_meta(
    state: &AppState,
    model_name: &str,
    hint_category: Option<&str>,
    channel: Option<&crate::models::Channel>,
    db_model: Option<&crate::models::Model>,
) -> (String, Option<String>, i64) {
    let mut action_type = String::new();
    let mut billing_pid: Option<String> = None;
    let mut enable_log: i64 = 0;

    // 统一 JOIN 查询，根据是否有 db_model 选择最优 WHERE 条件
    use sqlx::Row;
    let base_select = "SELECT m.mid, m.enable_log_content, \
         t.name as category_name, b.pid as billing_pid \
         FROM models m \
         LEFT JOIN model_types t ON m.type_id = t.id \
         LEFT JOIN billing_rules b ON m.billing_rule_id = b.id";

    let row = if let Some(m) = db_model {
        // 主键精确定位（一次查询、一行结果、无歧义）
        let sql = format!("{} WHERE m.id = ?", base_select);
        sqlx::query(&state.db.format_query(&sql))
            .bind(m.id)
            .fetch_optional(&state.db.pool)
            .await
            .unwrap_or(None)
    } else {
        // 按 model_id 查询 + 类别过滤 + 渠道精确匹配
        let cat_filter = if let Some(cat) = hint_category {
            format!(" AND t.name = '{}'", cat)
        } else {
            String::new()
        };
        let sql = format!(
            "{} WHERE m.model_id = ? AND m.is_active = 1 AND m.is_listed = 1{} ORDER BY m.id",
            base_select, cat_filter
        );
        let mut rows = sqlx::query(&state.db.format_query(&sql))
            .bind(model_name)
            .fetch_all(&state.db.pool)
            .await
            .unwrap_or_default();

        if rows.is_empty() && hint_category.is_some() {
            let fallback_sql = format!(
                "{} WHERE m.model_id = ? AND m.is_active = 1 AND m.is_listed = 1 ORDER BY m.id",
                base_select
            );
            rows = sqlx::query(&state.db.format_query(&fallback_sql))
                .bind(model_name)
                .fetch_all(&state.db.pool)
                .await
                .unwrap_or_default();
        }

        if rows.is_empty() {
            // 模型未入库时仍保留调用方类别提示，避免失败日志 action_type 为空
            if let Some(cat) = hint_category.filter(|c| !c.is_empty()) {
                action_type = cat.to_string();
            }
            return (action_type, billing_pid, enable_log);
        }

        // 优先通过渠道精确匹配，确保 billing_pid 与计费路径一致
        // 渠道 models 字段可能是 mid 或 model_id 格式，两种均尝试匹配
        let target_row = if let Some(ch) = channel {
            let ch_models = ch.get_models();
            if !ch_models.is_empty() {
                rows.iter()
                    .position(|r| {
                        let mid: String = r.try_get("mid").unwrap_or_default();
                        ch_models.contains(&mid)
                    })
                    .or_else(|| {
                        rows.iter()
                            .position(|_| ch_models.contains(&model_name.to_string()))
                    })
            } else {
                None
            }
        } else {
            None
        };
        let idx = target_row.unwrap_or(0);
        Some(rows.into_iter().nth(idx).unwrap())
    };

    let row = match row {
        Some(r) => r,
        None => {
            if let Some(cat) = hint_category.filter(|c| !c.is_empty()) {
                action_type = cat.to_string();
            }
            return (action_type, billing_pid, enable_log);
        }
    };

    action_type = row.try_get("category_name").unwrap_or_default();
    if action_type == "封装" {
        if let Some(cat) = hint_category.filter(|c| !c.is_empty() && *c != "封装") {
            action_type = cat.to_string();
        }
    } else if action_type.is_empty() {
        if let Some(cat) = hint_category.filter(|c| !c.is_empty()) {
            action_type = cat.to_string();
        }
    }
    billing_pid = row.try_get("billing_pid").unwrap_or(None);
    enable_log = row
        .try_get::<i64, _>("enable_log_content")
        .unwrap_or(0);

    crate::relay_debug!(
        "[ModelMeta] 模型={} 类别={} PID={} 日志内容开关={} 来源={}",
        model_name,
        action_type,
        billing_pid.as_deref().unwrap_or("-"),
        enable_log,
        if db_model.is_some() { "pk" } else { "query" }
    );

    (action_type, billing_pid, enable_log)
}

/// 统一记录 Relay 前置拦截与校验失败日志（极简高效，直接传 &token，区分 endpoint 与 upstream_url）
pub async fn record_error_log(
    state: &Arc<AppState>,
    token: &ApiToken,
    channel_id: Option<i64>,
    model: &str,
    status_code: u16,
    endpoint: &str,
    category: &str,
    error_msg: &str,
    upstream_url: Option<&str>,
) {
    let db_error_msg = extract_error_message(error_msg);
    let (real_ep, ep_up_url) = endpoint.split_once('|').unwrap_or((endpoint, ""));
    let actual_upstream = upstream_url.unwrap_or(ep_up_url);
    let cat = if !category.is_empty() {
        category
    } else {
        action_type_from_path(real_ep).unwrap_or("其它")
    };
    let prefix = if !cat.is_empty() && cat != "聊天" {
        "tsk_"
    } else {
        "log_"
    };
    let log_id = format!("{}{}", prefix, ulid::Ulid::new().to_string().to_lowercase());
    let sql = state.db.format_query(
        "INSERT INTO logs (log_id, user_id, channel_id, token_id, model, prompt_tokens, completion_tokens, cached_tokens, cost, status_code, endpoint, error_message, latency_ms, is_stream, upstream_url, action_type, is_completed) VALUES (?, ?, ?, ?, ?, 0, 0, 0, 0.0, ?, ?, ?, 0, 0, ?, ?, 1)"
    );
    let res = sqlx::query(&sql)
        .bind(&log_id)
        .bind(&token.user_id)
        .bind(channel_id.unwrap_or(0))
        .bind(token.id)
        .bind(model)
        .bind(status_code as i32)
        .bind(real_ep)
        .bind(&db_error_msg)
        .bind(actual_upstream)
        .bind(cat)
        .execute(&state.db.pool)
        .await;
    if let Err(e) = res {
        tracing::warn!("[ErrorLog] 记录错误日志失败: {:?}", e);
    }
}

/// 最终记账参数：`ctx` 复用 HA 记账上下文，`usage` 替代 prompt/completion/cached 三字段
pub struct BillRecord<'a> {
    pub ctx: crate::relay::ha::HaBillCtx<'a>,
    pub channel: &'a crate::models::Channel,
    pub log_id: Option<i64>,
    pub usage: crate::relay::usage_extractor::UsageTokens,
    pub cost: f64,
    pub pre_deducted: f64,
    pub pre_deduct_gift: f64,
    pub latency_ms: u32,
    pub is_stream: i64,
    pub status_code: u16,
    pub error_msg: Option<&'a str>,
    pub request: Option<String>,
    pub response: Option<String>,
    pub upstream_req: Option<String>,
    pub detail: Option<String>,
    pub features: Option<crate::relay::usage_extractor::ExtractedFeatures>,
    pub time_multiplier: Option<f64>,
    pub plugin_tag: Option<&'a str>,
    pub upstream_request_id: Option<String>,
}

/// 计费记录统一入口；log_id 有值 UPDATE 预记录行，无值 INSERT
pub async fn record_and_bill_inner(p: BillRecord<'_>) {
    let BillRecord {
        ctx,
        channel,
        log_id: pending_log_id,
        usage,
        cost,
        pre_deducted,
        pre_deduct_gift,
        latency_ms,
        is_stream,
        status_code,
        error_msg,
        request: request_content,
        response: response_content,
        upstream_req: upstream_req_content,
        detail: billing_detail,
        features: billed_features,
        time_multiplier: bill_time_multiplier,
        plugin_tag,
        upstream_request_id,
    } = p;
    let state = ctx.state;
    let token = ctx.token;
    let model_name = ctx.model;
    let hint_category = ctx.hint_category;
    let db_model = ctx.db_model;
    let endpoint = ctx.ep;
    let pre_deducted = crate::money::round_money(pre_deducted);
    let pre_deduct_gift = crate::money::round_money(pre_deduct_gift);
    crate::middleware::live_metrics::record_tokens(
        &token.user_id,
        token.id,
        (usage.prompt.max(0) as u64).saturating_add(usage.completion.max(0) as u64),
    );

    let extracted_error_msg = error_msg.map(|msg| extract_error_message(msg));
    let db_error_msg = extracted_error_msg.as_deref();
    let channel_id = channel.id;

    let (category, billing_pid, enable_log) =
        resolve_model_meta(state, model_name, hint_category, Some(channel), db_model).await;

    // HA: group_aid；物理: preset_id；内存 yid 补全 config_id
    let channel_config_id = super::ha::resolve_log_config_id(state, channel).await;
    let is_ha = super::ha::channel_is_ha_flag(channel);

    let filter_content = |content: Option<String>, respect_log_flag: bool| -> Option<String> {
        let text = content?;
        if respect_log_flag && enable_log == 0 {
            return None;
        }
        Some(sanitize_base64(&text))
    };

    // ── 计费特征快照（与扣费同源；已传入则复用）──
    let mut feat = crate::relay::usage_extractor::build_billing_features(
        request_content.as_deref(),
        upstream_req_content.as_deref(),
        response_content.as_deref(),
        billed_features,
        bill_time_multiplier,
    );
    // 级联：从 plugin_tag.cascade 补 version/resolution（不改用户入参）
    if let Some(tag) = plugin_tag {
        if feat.version.is_none() {
            if let Some(ver) = crate::relay::cascade::cascade_json_str(tag, "/cascade/version") {
                feat.version = Some(ver);
            }
        }
        if feat.resolution.is_none() {
            if let Some(res) = crate::relay::cascade::cascade_json_str(tag, "/cascade/resolution") {
                feat.resolution = Some(res);
            }
        }
    }
    // 异步任务等未先行调用 calculate_relay_cost 的路径：提交时刻锁定模型的时段折扣快照，防跨时段结算漂移
    if feat.locked_site_discount.is_none() || feat.locked_global_discount.is_none() {
        if let Some(m) = db_model.filter(|m| crate::models::discount_schedule_enabled(&m.discount_schedule)) {
            let mut m = m.clone();
            let site_tz = super::relay_settings::get_cached_site_timezone(&state.db).await;
            m.apply_discount_schedule_now(site_tz.as_ref());
            feat.locked_site_discount = Some(m.site_discount);
            feat.locked_global_discount = Some(m.global_discount);
        }
    }
    let has_video_val: i64 = if feat.has_video { 1 } else { 0 };
    let billing_features_json: Option<String> = serde_json::to_string(&feat).ok();
    let prompt_for_log = usage.context_input_tokens();

    let req_content = filter_content(request_content, true);
    let upstream_req = filter_content(upstream_req_content, true);

    let resp_content = if enable_log == 0 && category == "聊天" {
        // 关闭上下文记录的聊天类型：仅保留 usage 统计；失败时保留响应体以便排错
        if let Some(ref text) = response_content {
            crate::relay::usage_extractor::extract_usage_json_string(text).or_else(|| {
                if status_code != 200 {
                    Some(sanitize_base64(text))
                } else {
                    Some("[]".to_string())
                }
            })
        } else {
            None
        }
    } else {
        // 视频/图片/其他类别，或已开启上下文记录：完整保留响应体
        response_content.map(|t| sanitize_base64(&t))
    };

    // 直接复用已水合 Channel 的 base_url/api_key（含 HA 子配 / preset / volc 覆盖）
    let (system_endpoint, upstream_ep) = if endpoint.contains('|') {
        let parts: Vec<&str> = endpoint.splitn(2, '|').collect();
        (parts[0], parts[1])
    } else {
        (endpoint, endpoint)
    };

    let mut final_endpoint = upstream_ep.to_string();
    if !final_endpoint.starts_with("http") && !channel.base_url.is_empty() {
        final_endpoint = join_url(&channel.base_url, &final_endpoint);
    }
    if !channel.api_key.is_empty() {
        final_endpoint = super::forward::mask_key_in_string(&final_endpoint, &channel.api_key);
    }
    if !final_endpoint.starts_with("http") {
        if let Some(log_id) = pending_log_id {
            if let Ok(Some(prev)) = sqlx::query_scalar::<_, String>(
                &state
                    .db
                    .format_query("SELECT COALESCE(upstream_url, '') FROM logs WHERE id = ?"),
            )
            .bind(log_id)
            .fetch_optional(&state.db.pool)
            .await
            {
                if prev.starts_with("http") {
                    final_endpoint = prev;
                }
            }
        }
    }

    let res: Result<(), sqlx::Error> = async {
        let mut tx = state.db.pool.begin().await?;
        // 从响应体自动提取异步任务 ID（复用 response_formatter::extract_async_task_id 统一逻辑）
        // 提前解析提取，用于判断当前是否为异步任务预扣冻结阶段
        let task_id = resp_content
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .map(|v| super::response_formatter::extract_async_task_id(&v))
            .unwrap_or_default();

        // 异步任务预扣冻结判定：任务 ID 非空且计费详情中包含“冻结”
        let is_freeze = !task_id.is_empty() && billing_detail.as_deref().map_or(false, |d| d.contains("冻结"));

        // 异步节流更新令牌最后使用时间（彻底移出主结算事务，消除高并发下针对同一 token 的行排他锁争夺）
        super::token_quota::touch_token_last_used(state, token.id);

        let (settled_cost, _apply_balance) = crate::money::settlement_delta(cost, pre_deducted);
        let site_tz = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
        let tz = crate::api::date_helper::resolve_user_timedisplay_name(
            &state.db,
            &token.user_id,
            &site_tz,
        )
        .await;

        let db_post_response = if is_freeze {
            resp_content.clone()
        } else {
            None
        };

        let final_action_type = if let Some(cat) = hint_category.map(str::trim).filter(|c| !c.is_empty()) {
            cat.to_string()
        } else if !category.is_empty() {
            category.clone()
        } else {
            action_type_from_path(system_endpoint).unwrap_or("其它").to_string()
        };

        // 【倒序写入第一阶段：大日志落库与 CAS 防重校验】
        // 在持有 users 行锁之前，先完成耗时的大文本 UPDATE logs（PostgreSQL TOAST 耗时 20ms）。
        // 此时锁定的仅是 logs 表当前行主键，完全不占 users 表行锁，并发请求互不阻塞！
        // CAS status_code=0：避免孤儿清理/启动恢复已关单后退款后又被结算扣费覆盖
        let commission_log_id: i64 = if let Some(log_id) = pending_log_id {
            let touched = sqlx::query(&state.db.format_query(
                "UPDATE logs SET channel_id = ?, model = ?, \
                 prompt_tokens = ?, completion_tokens = ?, cached_tokens = ?, \
                 cost = ?, status_code = ?, endpoint = ?, error_message = ?, latency_ms = ?, \
                 request_content = ?, response_content = ?, post_response = ?, upstream_url = ?, \
                 upstream_req_content = ?, billing_detail = ?, \
                 task_id = CASE WHEN COALESCE(task_id, '') <> '' THEN task_id WHEN ? = '' OR ? IS NULL THEN task_id ELSE ? END, \
                 action_type = ?, billing_pid = ?, \
                 billing_features = ?, pre_deduct_gift = ?, is_completed = ?, \
                 channel_config_id = ?, is_ha = ?, \
                 plugin_tag = CASE WHEN ? = '' THEN plugin_tag ELSE ? END, \
                 upstream_request_id = CASE WHEN ? = '' OR ? IS NULL THEN upstream_request_id ELSE ? END, \
                 has_video = ? \
                 WHERE id = ? AND status_code = 0",
            ))
            .bind(channel_id)
            .bind(model_name)
            .bind(prompt_for_log)
            .bind(usage.completion)
            .bind(usage.cached)
            .bind(settled_cost)
            .bind(status_code as i32)
            .bind(system_endpoint)
            .bind(db_error_msg)
            .bind(latency_ms as i32)
            .bind(&req_content)
            .bind(&resp_content)
            .bind(&db_post_response)
            .bind(&final_endpoint)
            .bind(&upstream_req)
            .bind(&billing_detail)
            .bind(&task_id).bind(&task_id).bind(&task_id)
            .bind(&final_action_type)
            .bind(&billing_pid)
            .bind(&billing_features_json)
            .bind(pre_deduct_gift)
            .bind(if is_freeze { 0i16 } else { 1i16 })  // is_completed: 冻结任务=0(待结算), 同步请求=1(已完成)
            .bind(channel_config_id)
            .bind(is_ha)
            .bind(plugin_tag.unwrap_or(""))
            .bind(plugin_tag.unwrap_or(""))
            .bind(&upstream_request_id).bind(&upstream_request_id).bind(&upstream_request_id)
            .bind(has_video_val)
            .bind(log_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if touched == 0 {
                // 已被孤儿清理/启动恢复/并发结案；勿与预扣余额不足的 RowNotFound 混淆
                return Err(sqlx::Error::Protocol("pending_cas_miss".into()));
            }
            log_id
        } else {
            let fb_prefix = if !final_action_type.is_empty() && final_action_type != "聊天" { "tsk_" } else { "log_" };
            let fallback_log_id = format!("{}{}", fb_prefix, ulid::Ulid::new().to_string().to_lowercase());
            sqlx::query_scalar::<_, i64>(&state.db.format_query(
                "INSERT INTO logs (log_id, user_id, channel_id, token_id, model, prompt_tokens, completion_tokens, cached_tokens, cost, status_code, endpoint, error_message, latency_ms, request_content, response_content, post_response, is_stream, upstream_url, upstream_req_content, billing_detail, task_id, action_type, billing_pid, forward_eid, billing_features, pre_deduct_gift, plugin_tag, is_completed, channel_config_id, is_ha, upstream_request_id, has_video) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            ))
            .bind(&fallback_log_id)
            .bind(&token.user_id)
            .bind(channel_id)
            .bind(token.id)
            .bind(model_name)
            .bind(prompt_for_log)
            .bind(usage.completion)
            .bind(usage.cached)
            .bind(settled_cost)
            .bind(status_code as i32)
            .bind(system_endpoint)
            .bind(db_error_msg)
            .bind(latency_ms as i32)
            .bind(&req_content)
            .bind(&resp_content)
            .bind(&db_post_response)
            .bind(is_stream)
            .bind(&final_endpoint)
            .bind(&upstream_req)
            .bind(&billing_detail)
            .bind(&task_id)
            .bind(&final_action_type)
            .bind(&billing_pid)
            .bind::<Option<String>>(None)  // forward_eid: 预记录阶段已写入，无预记录时留空
            .bind(&billing_features_json)
            .bind(pre_deduct_gift)
            .bind(plugin_tag.unwrap_or(""))
            .bind(if is_freeze { 0i16 } else { 1i16 })  // is_completed: 冻结任务=0(待结算), 同步请求=1(已完成)
            .bind(channel_config_id)
            .bind(is_ha)
            .bind(&upstream_request_id)
            .bind(has_video_val)
            .fetch_one(&mut *tx)
            .await?
        };

        // 【倒序写入第二阶段：令牌与渠道配额消耗统计】
        if settled_cost > 0.0 || pre_deducted > 0.0 {
            if settled_cost > 0.0 {
                let _added = super::token_quota::consume_async_or_sync(
                    state,
                    &mut tx,
                    token,
                    settled_cost,
                    &tz,
                )
                .await?;
            }

            if channel_id > 0 && settled_cost > 0.0 {
                super::channel_quota::record_channel_quota(channel_id, settled_cost);
            }
            if let Some(cfg_id) = channel_config_id {
                if cfg_id > 0 && settled_cost > 0.0 {
                    super::channel_quota::record_config_quota(cfg_id as i64, settled_cost);
                }
            }

            // 【倒序写入第三阶段：用户钱包扣减/退补（锁持有时间压缩至 0.3ms 极速放锁）】
            // 钱包更新后只做同事务的小额返佣入账，然后立即 commit。不要在这里写大文本。
            // 冻结仍是预扣，不占卡券。同步请求在这里结清，才由卡券承担并退回已冻余额。
            let charge_ok = (200..400).contains(&status_code);
            let (voucher_final, voucher_held) = if commission_log_id > 0 && !is_freeze && charge_ok {
                super::voucher::retarget_for_log(
                    &state.db,
                    &mut tx,
                    &token.user_id,
                    commission_log_id,
                    model_name,
                    settled_cost,
                )
                .await?
            } else {
                (0.0, 0.0)
            };
            let (settled_rest, pre_rest, _) = crate::money::wallet_leg(
                settled_cost,
                pre_deducted,
                voucher_final,
                voucher_held,
            );
            #[cfg(feature = "commercial_plugins")]
            let (coupon_final, coupon_held) = if commission_log_id > 0 && !is_freeze && charge_ok {
                crate::api::plugins::marketing_membership::billing::retarget_for_log(
                    state,
                    &mut tx,
                    &token.user_id,
                    commission_log_id,
                    model_name,
                    settled_rest,
                )
                .await?
            } else {
                (0.0, 0.0)
            };
            #[cfg(not(feature = "commercial_plugins"))]
            let (coupon_final, coupon_held) = (0.0, 0.0);
            let (wallet_settled, wallet_pre, wallet_apply) = crate::money::wallet_leg(
                settled_rest,
                pre_rest,
                coupon_final,
                coupon_held,
            );
            let consumption_award = crate::services::affiliate::prepare_consumption_commission(
                &state.db,
                &mut tx,
                &token.user_id,
                wallet_settled,
                wallet_pre,
                pre_deduct_gift,
                is_freeze,
            )
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("commission: {e}")))?;
            if wallet_apply > 0.0 {
                sqlx::query(&state.db.format_query(
                    "UPDATE users SET
                     balance = CASE WHEN gift_balance >= ? THEN balance ELSE balance - (? - gift_balance) END,
                     gift_used_quota = gift_used_quota + ? + CASE WHEN gift_balance >= ? THEN ? ELSE gift_balance END,
                     gift_balance = CASE WHEN gift_balance >= ? THEN gift_balance - ? ELSE 0 END,
                     used_quota = used_quota + ?,
                     updated_at = CURRENT_TIMESTAMP
                     WHERE id = ?",
                ))
                .bind(wallet_apply).bind(wallet_apply)
                .bind(pre_deduct_gift).bind(wallet_apply).bind(wallet_apply)
                .bind(wallet_apply).bind(wallet_apply).bind(wallet_settled).bind(&token.user_id)
                .execute(&mut *tx)
                .await?;
            } else if wallet_apply < 0.0 {
                let refund = -wallet_apply;
                let gift_cost = wallet_settled.min(pre_deduct_gift);
                let gift_refund = pre_deduct_gift - gift_cost;
                let balance_refund = refund - gift_refund;
                sqlx::query(&state.db.format_query(
                    "UPDATE users SET balance = balance + ?, gift_balance = gift_balance + ?, used_quota = used_quota + ?, gift_used_quota = gift_used_quota + ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                ))
                .bind(balance_refund)
                .bind(gift_refund)
                .bind(wallet_settled)
                .bind(gift_cost)
                .bind(&token.user_id)
                .execute(&mut *tx)
                .await?;
            } else {
                // apply==0：冻结阶段不累加 used_quota；同步精确匹配则累加
                let (add_used, add_gift) = if is_freeze {
                    (0.0, 0.0)
                } else {
                    (wallet_settled, wallet_settled.min(pre_deduct_gift))
                };
                sqlx::query(&state.db.format_query(
                    "UPDATE users SET used_quota = used_quota + ?, gift_used_quota = gift_used_quota + ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                ))
                .bind(add_used)
                .bind(add_gift)
                .bind(&token.user_id)
                .execute(&mut *tx)
                .await?;
            }
            if let Some(award) = consumption_award {
                crate::services::affiliate::credit_consumption_commission(
                    &state.db,
                    &mut tx,
                    &token.user_id,
                    &format!("log:{commission_log_id}"),
                    &award,
                )
                .await
                .map_err(|e| sqlx::Error::Protocol(format!("commission: {e}")))?;
            }
        }

        tx.commit().await?;
        Ok(())
    }
    .await;
    if let Err(e) = res {
        tracing::warn!("[RelayUsage] 记录使用日志失败: {:?}", e);
        // 结算事务失败：若已预扣且日志仍为处理中，立即 CAS 退款（不必等孤儿任务）
        // CAS 未命中说明他处已关单/退款，禁止再补偿
        let cas_miss = matches!(&e, sqlx::Error::Protocol(m) if m == "pending_cas_miss");
        if pre_deducted > 0.0 && !cas_miss {
            const DETAIL: &str = "计费落库失败，预扣费已退回";
            if let Some(log_id) = pending_log_id {
                let _ = close_pending_and_refund(state, log_id, 500, DETAIL, DETAIL).await;
            } else if let Err(e) = refund_wallet_sql(
                &state.db,
                &state.db.pool,
                &token.user_id,
                pre_deducted,
                pre_deduct_gift,
            )
            .await
            {
                tracing::warn!(
                    "[BillCompensate] 无 pending 日志钱包退款失败 user={} amount={:.6}: {:?}",
                    token.user_id,
                    pre_deducted,
                    e
                );
            }
        }
    } else if crate::money::round_money(cost) > 0.0 {
        // 异步检查低余额提醒，不阻塞计费路径
        crate::services::notification::spawn_low_balance_check(
            Arc::clone(state),
            token.user_id.clone(),
        );
    }
}

/// 锁定仍为 pending 的日志预扣凭证；无行则 Ok(None)。
async fn lock_pending_prepay(
    db: &crate::db::Database,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    log_id: i64,
) -> Result<Option<(String, f64, f64)>, sqlx::Error> {
    sqlx::query_as(&db.format_query(
        "SELECT user_id, cost, pre_deduct_gift FROM logs \
         WHERE id = ? AND status_code = 0 FOR UPDATE",
    ))
    .bind(log_id)
    .fetch_optional(&mut **tx)
    .await
}

/// `close=None`：仅退预扣并清零 cost，保持 pending；`close=Some`：关单并退预扣。
async fn settle_pending_prepay(
    state: &Arc<AppState>,
    log_id: i64,
    close: Option<(i32, &str, &str)>,
) -> bool {
    let tag = if close.is_some() {
        "PendingClose"
    } else {
        "HA"
    };
    let mut tx = match state.db.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!("[{tag}] 开启事务失败 日志ID={}: {:?}", log_id, e);
            return false;
        }
    };

    let row = match lock_pending_prepay(&state.db, &mut tx, log_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("[{tag}] 锁定日志失败 日志ID={}: {:?}", log_id, e);
            let _ = tx.rollback().await;
            return false;
        }
    };
    let Some((user_id, cost, pre_deduct_gift)) = row else {
        let _ = tx.rollback().await;
        return false;
    };
    let cost = crate::money::round_money(cost);
    let pre_deduct_gift = crate::money::round_money(pre_deduct_gift);

    if close.is_none() && cost <= 0.0 && pre_deduct_gift <= 0.0 {
        let _ = tx.rollback().await;
        return false;
    }

    let upd = match close {
        Some((status_code, error_message, billing_detail)) => {
            sqlx::query(&state.db.format_query(
                "UPDATE logs SET status_code = ?, cost = 0.0, pre_deduct_gift = 0.0, \
             error_message = ?, billing_detail = ?, is_completed = 1 \
             WHERE id = ? AND status_code = 0",
            ))
            .bind(status_code)
            .bind(error_message)
            .bind(billing_detail)
            .bind(log_id)
            .execute(&mut *tx)
            .await
        }
        None => sqlx::query(&state.db.format_query(
            "UPDATE logs SET cost = 0.0, pre_deduct_gift = 0.0 WHERE id = ? AND status_code = 0",
        ))
        .bind(log_id)
        .execute(&mut *tx)
        .await,
    };
    if let Err(e) = upd {
        tracing::warn!("[{tag}] 更新日志失败 日志ID={}: {:?}", log_id, e);
        let _ = tx.rollback().await;
        return false;
    }

    let voucher_refunded = match crate::relay::voucher::release_holds(&state.db, &mut tx, &user_id, log_id).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[{tag}] 退回模型卡券失败 日志ID={}: {:?}", log_id, e);
            let _ = tx.rollback().await;
            return false;
        }
    };
    #[cfg(feature = "commercial_plugins")]
    let coupon_refunded = match crate::api::plugins::marketing_membership::billing::refund_coupon_prepay(
        state, &mut tx, &user_id, log_id,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[{tag}] 退回卡券失败 日志ID={}: {:?}", log_id, e);
            let _ = tx.rollback().await;
            return false;
        }
    };
    #[cfg(not(feature = "commercial_plugins"))]
    let coupon_refunded = 0.0;
    let (balance_refund, gift_refund) = crate::money::wallet_refund_parts(
        cost,
        pre_deduct_gift,
        voucher_refunded + coupon_refunded,
    );
    if let Err(e) = refund_wallet_sql(
        &state.db,
        &mut *tx,
        &user_id,
        balance_refund + gift_refund,
        gift_refund,
    )
    .await
    {
        tracing::warn!(
            "[{tag}] 退款失败 用户ID={} 日志ID={}: {:?}",
            user_id,
            log_id,
            e
        );
        let _ = tx.rollback().await;
        return false;
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("[{tag}] 提交事务失败 日志ID={}: {:?}", log_id, e);
        return false;
    }

    if let Some((status_code, _, _)) = close {
        crate::relay_debug!(
            "[PendingClose] 已关闭 日志ID={} 状态码={} 用户ID={} 金额={:.6} 赠送={:.6}",
            log_id,
            status_code,
            user_id,
            cost,
            pre_deduct_gift
        );
    } else {
        crate::relay_debug!(
            "[HA] 续试退预扣 日志ID={} 金额={:.6} 赠送={:.6}",
            log_id,
            cost,
            pre_deduct_gift
        );
    }
    true
}

/// CAS 关闭 status_code=0 的 pending：同行锁读 cost/pre_deduct_gift 后退双钱包。
async fn close_pending_and_refund(
    state: &Arc<AppState>,
    log_id: i64,
    status_code: i32,
    error_message: &str,
    billing_detail: &str,
) -> bool {
    settle_pending_prepay(
        state,
        log_id,
        Some((status_code, error_message, billing_detail)),
    )
    .await
}

async fn refund_wallet_sql<'e, E>(
    db: &crate::db::Database,
    executor: E,
    user_id: &str,
    cost: f64,
    pre_deduct_gift: f64,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    if cost <= 0.0 && pre_deduct_gift <= 0.0 {
        return Ok(());
    }
    // 赠送退回不超过 cost，避免脏数据导致 balance 被扣成负向
    let (balance_refund, gift_refund) =
        crate::money::wallet_refund_parts(cost, pre_deduct_gift, 0.0);
    sqlx::query(&db.format_query(
        "UPDATE users SET balance = balance + ?, gift_balance = gift_balance + ?, \
         updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    ))
    .bind(balance_refund)
    .bind(gift_refund)
    .bind(user_id)
    .execute(executor)
    .await?;
    Ok(())
}

/// 清理孤儿预记录日志（status_code=0 且超过 1 小时）
pub async fn cleanup_orphan_pending_logs(state: &Arc<AppState>) {
    let orphans: Vec<i64> = match sqlx::query_scalar(&state.db.format_query(
        "SELECT id FROM logs \
             WHERE is_completed = 0 AND status_code = 0 \
             AND created_at < CURRENT_TIMESTAMP - INTERVAL '1 hour'",
    ))
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("[OrphanCleanup] 查询孤儿日志失败: {:?}", e);
            return;
        }
    };
    if orphans.is_empty() {
        return;
    }
    tracing::info!(
        "[OrphanCleanup] 发现 {} 条孤儿日志，开始清理",
        orphans.len()
    );
    for log_id in orphans {
        let _ = close_pending_and_refund(
            state,
            log_id,
            408,
            "请求处理超时或连接中断",
            "孤儿日志清理，预扣费已退回",
        )
        .await;
    }
}

/// 启动时恢复中断遗留的处理中日志（分批并发平滑处理，防拖死数据库与主线程）
pub async fn recover_interrupted_logs(state: Arc<AppState>) {
    let orphans: Vec<i64> = match sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM logs WHERE is_completed = 0 AND status_code = 0"),
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("[StartupRecover] 查询中断日志失败: {:?}", e);
            return;
        }
    };
    if orphans.is_empty() {
        return;
    }
    let total = orphans.len();
    tracing::info!(
        "[StartupRecover] 发现 {} 条上次中断遗留的处理中日志，开始并发恢复退费...",
        total
    );
    futures::stream::iter(orphans)
        .for_each_concurrent(16, |log_id| {
            let state = state.clone();
            async move {
                let _ = close_pending_and_refund(
                    &state,
                    log_id,
                    503,
                    "服务升级重启，请求被中断",
                    "服务升级中断，预扣费已退回",
                )
                .await;
            }
        })
        .await;
    tracing::info!("[StartupRecover] 全部 {} 条中断日志已恢复处理完毕", total);
}

/// 错误信息敏感词脱敏：URL 域名替换为 ***（保留协议和路径），密钥替换为 ***。
/// 仅用于普通用户端返回和日志展示，管理员端保留原始信息用于排查。
pub fn sanitize_error_message(msg: &str) -> String {
    // URL 域名脱敏：https://api.example.com/v1/... → https://***/v1/...
    let re_url = Regex::new(r"(https?://)([^/\s)\]},]+)").unwrap();
    let result = re_url.replace_all(msg, "${1}***").to_string();
    // API 密钥脱敏：sk-xxxx 等格式
    let re_key = Regex::new(r"\bsk-[a-zA-Z0-9]{8,}\b").unwrap();
    re_key.replace_all(&result, "***").to_string()
}

/// 上游失败对外 HTTP 状态：仅保留 4xx/5xx，其余按 502（含非 HTTP 业务码）。
#[inline]
pub fn normalize_error_http_status(status: u16) -> u16 {
    if (400..600).contains(&status) {
        status
    } else {
        502
    }
}

/// 异步轮询传输层可重试 HTTP（业务失败走 body.status=failed，不经此判定）。
#[inline]
pub fn is_poll_transport_retryable(status: u16) -> bool {
    matches!(
        normalize_error_http_status(status),
        429 | 500 | 502 | 503 | 504
    )
}

/// 上游失败对外错误：根据客户端请求路径决定是否格式化为 OpenAI 错误
/// - OpenAI 兼容路由：经 norm_err_msg 收成标准 OpenAI 错误结构
/// - 官方原生路由：直接透传上游原始错误响应体，绝不擅自包装
pub fn upstream_fail(
    status: u16,
    raw_msg: &str,
    headers: Option<axum::http::HeaderMap>,
    raw_path: &str,
) -> crate::error::AppError {
    let msg = if super::response_formatter::is_openai_compatible_path(raw_path) {
        sanitize_error_message(&norm_err_msg(raw_msg))
    } else {
        raw_msg.to_string()
    };
    crate::error::AppError::UpstreamHttpError(normalize_error_http_status(status), msg, headers)
}

/// 零费用/失败结算参数（检测仍由调用方完成；本结构负责记账，可选再包成 `upstream_fail`）。
///
/// - `prefer_http_status = Some(s)`：传输失败 / HTTP 非 2xx / 调用方已定码，状态用 `s`，日志文案用 `upstream_error_text`
/// - `prefer_http_status = None`：HTTP 200 业务失败，状态码与日志文案从 `response_body` 推断/提取
/// - `client_msg`：客户端文案；`None` 时与日志 `error_msg` 相同
/// - `pre_deducted` / `pre_deduct_gift`：预扣后失败退费场景传入已预扣额；上游失败尚未预扣时保持 `0.0`
pub struct ZeroCostUpstreamFail<'a> {
    pub state: &'a Arc<AppState>,
    pub token: &'a ApiToken,
    pub channel: &'a crate::models::Channel,
    pub model: &'a str,
    pub prefer_http_status: Option<u16>,
    pub endpoint: &'a str,
    pub latency_ms: u32,
    pub is_stream: i64,
    pub request_content: String,
    pub response_body: String,
    pub response_content: Option<String>,
    pub upstream_req_content: Option<String>,
    pub billing_detail: Option<String>,
    pub hint_category: Option<&'a str>,
    pub pending_log_id: Option<i64>,
    pub db_model: Option<&'a crate::models::Model>,
    pub client_msg: Option<&'a str>,
    pub pre_deducted: f64,
    pub pre_deduct_gift: f64,
    pub upstream_request_id: Option<String>,
}

/// 失败记账（cost=0）：返回 `(status_code, client_msg)`，由调用方决定 `upstream_fail` / `BadRequest` 等。
/// `status_code` 已经过 `normalize_error_http_status`，与后续 `upstream_fail` / HA first_fail 一致。
pub async fn record_zero_cost_fail(p: ZeroCostUpstreamFail<'_>) -> (u16, String) {
    let raw_status = p
        .prefer_http_status
        .unwrap_or_else(|| infer_error_status_code_from_str(&p.response_body));
    let status_code = normalize_error_http_status(raw_status);
    let error_msg = match p.prefer_http_status {
        Some(_) => upstream_error_text(status_code, &p.response_body),
        None => extract_error_message(&p.response_body),
    };
    let client_owned = p.client_msg.unwrap_or(&error_msg).to_string();
    record_and_bill_inner(BillRecord {
        ctx: crate::relay::ha::HaBillCtx {
            state: p.state,
            token: p.token,
            model: p.model,
            ep: p.endpoint,
            hint_category: p.hint_category,
            db_model: p.db_model,
        },
        channel: p.channel,
        log_id: p.pending_log_id,
        usage: Default::default(),
        cost: 0.0,
        pre_deducted: p.pre_deducted,
        pre_deduct_gift: p.pre_deduct_gift,
        latency_ms: p.latency_ms,
        is_stream: p.is_stream,
        status_code,
        error_msg: Some(&error_msg),
        request: Some(p.request_content),
        response: p.response_content,
        upstream_req: p.upstream_req_content,
        detail: p.billing_detail,
        features: None,
        time_multiplier: None,
        plugin_tag: None,
        upstream_request_id: p.upstream_request_id,
    })
    .await;
    (status_code, client_owned)
}

/// HA 续试：按行内预扣凭证退钱包并清零 cost，保持 pending（status_code=0）供下一子渠结算
pub async fn refund_pending(state: &Arc<AppState>, log_id: i64) {
    let _ = settle_pending_prepay(state, log_id, None).await;
}

/// JSON 则走统一 OpenAI 错误规范化；非 JSON 原样返回
fn norm_err_msg(raw_msg: &str) -> String {
    let raw = raw_msg.find('{').map(|i| &raw_msg[i..]).unwrap_or(raw_msg);
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| super::response_formatter::format_as_openai_error(&v))
        .unwrap_or_else(|| raw_msg.to_string())
}

/// 上游错误文案：空 body 时补默认句，供日志与 `upstream_fail` 共用
#[inline]
pub fn upstream_error_text(status: u16, body: &str) -> String {
    if body.trim().is_empty() {
        format!("Upstream HTTP error {}", status)
    } else {
        body.to_string()
    }
}

/// 从错误响应体中提取最核心的错误文本信息（兼容 JSON、HTML 网页与纯文本，单行规整与 500 字符安全截断）
pub fn extract_error_message(resp_body: &str) -> String {
    let s = resp_body.trim();
    if s.is_empty() {
        return String::new();
    }
    // 兼容 HTML 错误页提取 <title>
    let raw = if s.starts_with('<') || s.contains("<html") || s.contains("<HTML") {
        s.to_ascii_lowercase()
            .find("<title>")
            .and_then(|i| s[i + 7..].split_once("</title>"))
            .map(|(t, _)| t.trim())
            .unwrap_or("HTML error page")
    } else {
        s
    };
    // 兼容 JSON 提取业务报错
    let msg = raw
        .find('{')
        .and_then(|i| serde_json::from_str::<serde_json::Value>(&raw[i..]).ok())
        .and_then(|v| super::response_formatter::extract_error_message_from_value(&v))
        .unwrap_or_else(|| raw.to_string());

    // 压缩空格单行化与 500 字符安全截断
    let clean = if msg.contains('\n') || msg.contains('\r') {
        msg.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        msg
    };
    if clean.chars().count() > 500 {
        format!("{} ...[截断]", clean.chars().take(500).collect::<String>())
    } else {
        clean
    }
}

/// 文案中的显式 HTTP 码（`status 404`）。只认 3 位 4xx/5xx，避免厂商业务码如 40004。
fn http_status_in_text(s: &str) -> Option<u16> {
    let lower = s.to_ascii_lowercase();
    let mut rest = lower.as_str();
    while let Some(i) = rest.find("status") {
        let after = rest[i + 6..].trim_start_matches([' ', '\t', ':', '=', '-']);
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.len() == 3 {
            if let Ok(n) = digits.parse::<u16>() {
                if (400..600).contains(&n) {
                    return Some(n);
                }
            }
        }
        rest = &rest[i + 6..];
    }
    None
}

fn is_content_policy(s: &str) -> bool {
    s.contains("sensitive")
        || s.contains("policy")
        || s.contains("violation")
        || s.contains("copyright")
        || s.contains("safety")
        || s.contains("moderation")
        || s.contains("censor")
}

/// 推断业务 HTTP 状态码：文案里的 `status 404` 优先于厂商 `InternalError` 等笼统码。
pub fn infer_error_status_code(body: &serde_json::Value) -> u16 {
    let msg = super::response_formatter::extract_error_message_from_value(body).unwrap_or_default();
    let raw = http_status_in_text(&msg)
        .or_else(|| {
            super::response_formatter::extract_error_code_from_value(body)
                .and_then(|c| classify_error_code(&c))
        })
        .unwrap_or_else(|| classify_error_text(&msg));
    normalize_error_http_status(raw)
}

/// 文本错误快捷推断：尝试 JSON，否则文案关键词；结果保证 4xx/5xx。
pub fn infer_error_status_code_from_str(err: &str) -> u16 {
    let raw = err.find('{').map(|i| &err[i..]).unwrap_or(err);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        return infer_error_status_code(&v);
    }
    normalize_error_http_status(
        http_status_in_text(err).unwrap_or_else(|| classify_error_text(err)),
    )
}

/// 按 error.code 字符串分类 HTTP 状态码。
/// 数字仅在合法 HTTP 错误区间（400–599）时采纳；厂商业务码（如 MiniMax 2013）返回 None，
/// 由调用方按 message 再分级。
fn classify_error_code(code: &str) -> Option<u16> {
    if let Ok(n) = code.parse::<u16>() {
        return if (400..=599).contains(&n) {
            Some(n)
        } else {
            None
        };
    }
    let c = code.to_lowercase();
    // 402：欠费/余额不足（须在权限/风控之前）
    if c.contains("overdue")
        || c.contains("balance")
        || c.contains("payment")
        || c.contains("insufficient_quota")
        || c.contains("insufficient_fund")
        || c.contains("billing")
    {
        return Some(402);
    }
    // 400：内容风控（用户内容被拒，不是没权限调接口）
    if is_content_policy(&c) || c.contains("block") {
        return Some(400);
    }
    // 403：令牌/IP/模型白名单等权限不足
    if c.contains("permission") || c.contains("forbidden") || c.contains("access_denied") {
        return Some(403);
    }
    // 鉴权/身份认证失败
    if c.contains("auth")
        || c.contains("unauthorized")
        || c.contains("invalid_key")
        || c.contains("credential")
        || c.contains("unauthenticated")
        || c.contains("revoked")
    {
        return Some(401);
    }
    // 超时/不可用（优先于限流判定，避免 deadline_exceeded 等被 exceeded 误判）
    if c.contains("timeout")
        || c.contains("deadline")
        || c.contains("gateway")
        || c.contains("unavailable")
        || c.contains("overload")
    {
        return Some(504);
    }
    // 限流/超额
    if c.contains("rate")
        || c.contains("limit")
        || c.contains("quota")
        || c.contains("throttl")
        || c.contains("quota_exceeded")
        || c.contains("rate_limit_exceeded")
        || c.contains("limit_exceeded")
        || c.contains("concurrency_limit")
    {
        return Some(429);
    }
    // 上游服务内部错误
    if c.contains("internal") || c.contains("server_error") || c.contains("service_error") {
        return Some(500);
    }
    // 有语义 code 但未命中以上分类 → 客户端类业务错误
    Some(400)
}

/// message 文本关键词分类（无 error.code 时）。`status 404` 已在入口先判。
fn classify_error_text(msg: &str) -> u16 {
    let m = msg.to_lowercase();
    // 402：欠费/余额不足（须在权限之前）
    if m.contains("overdue")
        || m.contains("out of budget")
        || m.contains("insufficient_balance")
        || m.contains("insufficient_quota")
        || m.contains("insufficient fund")
        || m.contains("insufficient_quota")
        || m.contains("insufficient quota")
        || m.contains("payment required")
        || m.contains("balance")
        || m.contains("payment")
        || m.contains("欠费")
        || m.contains("余额不足")
        || m.contains("余额不够")
    {
        return 402;
    }
    // 400：内容风控与客户端请求错误（须在 403 之前）
    if is_content_policy(&m)
        || m.contains("content_filter")
        || m.contains("敏感")
        || m.contains("违规")
        || m.contains("版权")
        || m.contains("failed to download media")
        || m.contains("check if the link is accessible")
        || m.contains("素材转换失败")
        || m.contains("素材处理失败")
    {
        return 400;
    }
    // 403：权限不足（permission 须在 auth 之前："not authorized" 含 auth 子串）
    if m.contains("permission")
        || m.contains("forbidden")
        || m.contains("not authorized")
        || m.contains("access denied")
        || m.contains("无权限")
        || m.contains("没有权限")
    {
        return 403;
    }
    // 401：鉴权/授权失败
    if m.contains("auth")
        || m.contains("unauthorized")
        || m.contains("api_key")
        || m.contains("api key")
        || m.contains("credential")
        || m.contains("invalid_key")
        || m.contains("invalid key")
        || m.contains("bad_key")
        || m.contains("bad key")
        || m.contains("revoked")
        || m.contains("unauthenticated")
        || m.contains("鉴权")
        || m.contains("密钥")
        || m.contains("授权")
    {
        return 401;
    }
    // 504：超时/网关/连接中断（优先于 429 匹配，避免 deadline exceeded 被误判为限流）
    if m.contains("timeout")
        || m.contains("timed out")
        || m.contains("timedout")
        || m.contains("deadline exceeded")
        || m.contains("context deadline")
        || m.contains("read body err")
        || m.contains("gateway")
        || m.contains("connect")
        || m.contains("disconnect")
        || m.contains("abort")
        || m.contains("unreachable")
        || m.contains("超时")
        || m.contains("网关")
        || m.contains("中断")
    {
        return 504;
    }
    // 429：限流/超额（收敛专有短语，杜绝泛词误伤）
    if m.contains("too many requests")
        || m.contains("too_many_requests")
        || m.contains("rate limit")
        || m.contains("rate_limit")
        || m.contains("ratelimit")
        || m.contains("quota exceeded")
        || m.contains("limit exceeded")
        || m.contains("concurrency limit")
        || m.contains("quota")
        || m.contains("throttl")
        || m.contains("限流")
        || m.contains("频率限制")
        || m.contains("并发限制")
        || m.contains("并发超限")
    {
        return 429;
    }
    // 500：上游服务器内部故障
    if m.contains("internal")
        || m.contains("server")
        || m.contains("failed")
        || m.contains("error")
        || m.contains("bug")
        || m.contains("crash")
        || m.contains("故障")
        || m.contains("服务器错误")
        || m.contains("执行失败")
        || m.contains("异常")
    {
        return 500;
    }
    400
}

/// 并发获取所有远程多媒体 URL (视频/音频) 的时长之和（8 秒全局超时兜底，流式 Range 解析）
/// 若媒体资源无法访问 (如 404/网络异常)，返回明确错误提示，避免无谓转给上游浪费资源
pub async fn sum_remote_media_duration(
    client: &reqwest::Client,
    urls: &[String],
) -> Result<f64, String> {
    if urls.is_empty() {
        return Ok(0.0);
    }
    let probe = async {
        let futs = urls
            .iter()
            .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
            .map(|u| probe_media_duration(client, u));
        let durations = futures::future::try_join_all(futs).await?;
        Ok(durations.into_iter().sum())
    };
    tokio::time::timeout(std::time::Duration::from_secs(8), probe)
        .await
        .map_err(|_| "探测参考媒体资源超时，请确保资源可正常公开访问".to_string())?
}

/// 局部辅助：流式获取指定 Range 的数据，一旦解析出时长立刻返回，支持 UA 伪装防拦截
async fn fetch_and_parse(
    client: &reqwest::Client,
    url: &str,
    range: &str,
) -> Result<(f64, Option<u64>, Vec<u8>), String> {
    use futures::StreamExt;

    let resp = client
        .get(url)
        .header("Range", range)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .timeout(std::time::Duration::from_secs(4))
        .send()
        .await
        .map_err(|e| format!("参考媒体资源连接失败: {} ({})", url, e))?;

    let status = resp.status().as_u16();
    if status != 200 && status != 206 {
        crate::relay_debug!(
            "[MediaDuration] HTTP 状态码非预期: {} 状态码={}",
            url,
            status
        );
        return Err(format!("参考媒体资源无法访问 (HTTP {}): {}", status, url));
    }

    let total = resp
        .headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|cr| cr.split('/').last()?.trim().parse::<u64>().ok());

    let mut buf = Vec::with_capacity(8192);
    let mut stream = resp.bytes_stream();
    while let Some(chunk_res) = stream.next().await {
        let chunk = match chunk_res {
            Ok(c) => c,
            Err(e) => return Err(format!("参考媒体资源读取中断: {} ({})", url, e)),
        };
        buf.extend_from_slice(&chunk);
        if let Some(d) = parse_media_duration(&buf) {
            return Ok((d, total, buf));
        }
        if buf.len() >= 32768 {
            break;
        }
    }
    Ok((0.0, total, buf))
}

/// HTTP Range 流式探测单个远程多媒体(视频/音频)时长，解析出 duration 立即终止连接
async fn probe_media_duration(client: &reqwest::Client, url: &str) -> Result<f64, String> {
    let start = std::time::Instant::now();

    // 1. 发起头部 Range 请求，拉取并流式解析前 32KB
    let (dur, total_size, head_buf) = match fetch_and_parse(client, url, "bytes=0-32767").await {
        Ok(res) => res,
        Err(err) => {
            crate::relay_debug!("[MediaDuration] 头部请求异常: {}", err);
            return Err(err);
        }
    };

    if dur > 0.0 {
        crate::relay_debug!(
            "[MediaDuration] 头部解析成功: {} 时长={} 耗时={:?}",
            url,
            dur,
            start.elapsed()
        );
        return Ok(dur);
    }

    // 2. 如果头部未找到且知道总大小，发起第二个 Range 请求，拉取并流式解析尾部 32KB (处理非 faststart 视频)
    if let Some(total) = total_size {
        if total > 32768 {
            let range = format!("bytes={}-{}", total - 32768, total - 1);
            if let Ok((tail_dur, _, _)) = fetch_and_parse(client, url, &range).await {
                if tail_dur > 0.0 {
                    crate::relay_debug!(
                        "[MediaDuration] 尾部解析成功: {} 时长={} 耗时={:?}",
                        url,
                        tail_dur,
                        start.elapsed()
                    );
                    return Ok(tail_dur);
                }
            }
        }
    }

    // 3. 兜底处理 (链接正常连通但未从元数据识别出时长，返回 0.0)
    crate::relay_debug!(
        "[MediaDuration] 探测完成(未解析到元数据时长): {} 大小={} 字节 总大小={:?} 耗时={:?}",
        url,
        head_buf.len(),
        total_size,
        start.elapsed()
    );
    Ok(0.0)
}

/// 通用多媒体时长解析入口，支持 MP4/MOV、WEBM/MKV、AVI、FLV、WAV、MP3
fn parse_media_duration(data: &[u8]) -> Option<f64> {
    parse_mp4_duration(data)
        .or_else(|| parse_webm_duration(data))
        .or_else(|| parse_avi_duration(data))
        .or_else(|| parse_flv_duration(data))
        .or_else(|| parse_wav_duration(data))
        .or_else(|| parse_mp3_duration(data))
        .filter(|d| *d > 0.0 && d.is_finite() && *d < 86400.0)
}

/// 解析 MP4/MOV 提取视频时长（秒）
fn parse_mp4_duration(data: &[u8]) -> Option<f64> {
    let moov_pos = data.windows(4).position(|w| w == b"moov")?;
    let moov_body = &data[moov_pos + 4..];
    let mvhd_pos = moov_body.windows(4).position(|w| w == b"mvhd")?;
    let mvhd_body = &moov_body[mvhd_pos + 4..];
    let (ts_off, dur_off, dur_len) = if mvhd_body.first().copied()? == 0 {
        (12, 16, 4)
    } else {
        (20, 24, 8)
    };
    if mvhd_body.len() < dur_off + dur_len {
        return None;
    }
    let timescale = u32::from_be_bytes(mvhd_body[ts_off..ts_off + 4].try_into().ok()?) as f64;
    let duration = if dur_len == 4 {
        u32::from_be_bytes(mvhd_body[dur_off..dur_off + 4].try_into().ok()?) as f64
    } else {
        u64::from_be_bytes(mvhd_body[dur_off..dur_off + 8].try_into().ok()?) as f64
    };
    if timescale > 0.0 {
        Some(duration / timescale)
    } else {
        None
    }
}

/// 解析 WEBM/MKV (EBML 容器) 提取视频时长（秒）
fn parse_webm_duration(data: &[u8]) -> Option<f64> {
    data.windows(4)
        .position(|w| w == &[0x1A, 0x45, 0xDF, 0xA3])?;
    let info_pos = data
        .windows(4)
        .position(|w| w == &[0x15, 0x49, 0xA9, 0x66])?;
    let info_body = &data[info_pos + 4..];

    let ts_pos = info_body
        .windows(3)
        .position(|w| w == &[0x2A, 0xD7, 0xB1])?;
    let (timescale, _) = parse_ebml_vint(&info_body[ts_pos + 3..])?;

    let dur_pos = info_body.windows(2).position(|w| w == &[0x44, 0x89])?;
    let dur_body = &info_body[dur_pos + 2..];
    let (dur_size, dur_size_len) = parse_ebml_vint(dur_body)?;
    if dur_body.len() < dur_size_len + dur_size as usize {
        return None;
    }

    let val_bytes = &dur_body[dur_size_len..dur_size_len + dur_size as usize];
    let duration_ms = if dur_size == 4 {
        f32::from_be_bytes(val_bytes.try_into().ok()?) as f64
    } else if dur_size == 8 {
        f64::from_be_bytes(val_bytes.try_into().ok()?) as f64
    } else {
        return None;
    };

    if timescale > 0 {
        Some((duration_ms * timescale as f64) / 1_000_000_000.0)
    } else {
        Some(duration_ms / 1000.0)
    }
}

/// 解析 AVI (RIFF 容器) 提取视频时长（秒）
fn parse_avi_duration(data: &[u8]) -> Option<f64> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"AVI " {
        return None;
    }
    let avih_pos = data.windows(4).position(|w| w == b"avih")?;
    let avih_body = &data[avih_pos + 8..];
    if avih_body.len() < 20 {
        return None;
    }
    let us_per_frame = u32::from_le_bytes(avih_body[0..4].try_into().ok()?) as f64;
    let total_frames = u32::from_le_bytes(avih_body[16..20].try_into().ok()?) as f64;
    if us_per_frame > 0.0 {
        Some((us_per_frame * total_frames) / 1_000_000.0)
    } else {
        None
    }
}

/// 解析 FLV (AMF 容器) 提取视频时长（秒）
fn parse_flv_duration(data: &[u8]) -> Option<f64> {
    if data.len() < 4 || &data[0..3] != b"FLV" {
        return None;
    }
    let dur_pos = data.windows(8).position(|w| w == b"duration")?;
    let val_type_pos = dur_pos + 8;
    if data.len() >= val_type_pos + 9 && data[val_type_pos] == 0x00 {
        let duration =
            f64::from_be_bytes(data[val_type_pos + 1..val_type_pos + 9].try_into().ok()?);
        if duration > 0.0 && duration.is_finite() {
            return Some(duration);
        }
    }
    None
}

/// 解析 EBML VINT (可变长度整数)
fn parse_ebml_vint(data: &[u8]) -> Option<(u64, usize)> {
    let first = *data.first()?;
    let zeros = first.leading_zeros() as usize;
    if zeros >= 8 {
        return None;
    }
    let len = zeros + 1;
    if data.len() < len {
        return None;
    }
    let mut val = (first & (0xFF >> len)) as u64;
    for i in 1..len {
        val = (val << 8) | data[i] as u64;
    }
    Some((val, len))
}

/// 高效精确：复用系统特征识别结构来提取输入视频 URL 列表
pub fn extract_request_video_urls(body: &serde_json::Value) -> Vec<String> {
    let mut urls = Vec::new();

    // 1. 顶层 videos 数组
    if let Some(arr) = body.get("videos").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(s) = item.as_str().filter(|s| !s.is_empty()) {
                urls.push(s.to_string());
            } else if let Some(u) = item
                .get("url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                urls.push(u.to_string());
            } else if let Some(u) = item
                .get("video_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                urls.push(u.to_string());
            }
        }
    }

    // 2. 火山方舟 content[].video_url 结构
    if let Some(content) = body.get("content").and_then(|c| c.as_array()) {
        for item in content {
            if let Some(t) = item.get("type").and_then(|v| v.as_str()) {
                if t.contains("video") {
                    if let Some(video_obj) = item.get("video_url") {
                        if let Some(u) = video_obj.as_str().filter(|s| !s.is_empty()) {
                            urls.push(u.to_string());
                        } else if let Some(u) = video_obj
                            .get("url")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                        {
                            urls.push(u.to_string());
                        }
                    }
                }
            }
        }
    }

    // 3. fal reference_video_urls 纯字符串数组
    urls.extend(extract_string_array_urls(body, "reference_video_urls"));

    urls.sort();
    urls.dedup();
    urls
}

/// 辅助提取 JSON 字段中的字符串 URL 列表（支持纯字符串数组与非空过滤）
fn extract_string_array_urls(body: &serde_json::Value, field: &str) -> Vec<String> {
    body.get(field)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// 提取请求体中的参考音频 URLs（纯一维字符串数组）
pub fn extract_request_audio_urls(body: &serde_json::Value) -> Vec<String> {
    let mut urls = extract_string_array_urls(body, "reference_audio_urls");
    if urls.is_empty() {
        urls = extract_string_array_urls(body, "audios");
    }
    if urls.is_empty() {
        urls = extract_string_array_urls(body, "sounds");
    }
    urls.sort();
    urls.dedup();
    urls
}

/// 解析 WAV 提取音频时长（秒）
fn parse_wav_duration(data: &[u8]) -> Option<f64> {
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    let mut byte_rate: Option<u32> = None;
    let mut data_size: Option<u32> = None;
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let len = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().ok()?) as usize;
        if id == b"fmt " && pos + 8 + 16 <= data.len() {
            byte_rate = Some(u32::from_le_bytes(
                data[pos + 16..pos + 20].try_into().ok()?,
            ));
        } else if id == b"data" {
            data_size = Some(len as u32);
        }
        if let (Some(br), Some(ds)) = (byte_rate, data_size) {
            if br > 0 {
                let dur = ds as f64 / br as f64;
                if dur > 0.0 && dur.is_finite() && dur < 86400.0 {
                    return Some(dur);
                }
            }
        }
        match pos.checked_add(8).and_then(|p| p.checked_add(len)) {
            Some(next) => pos = next,
            None => break,
        }
    }
    None
}

/// 解析 MP3 提取音频时长（秒）
fn parse_mp3_duration(data: &[u8]) -> Option<f64> {
    let mut offset = 0;
    if data.len() >= 10 && &data[0..3] == b"ID3" {
        let tag_size = ((data[6] as usize & 0x7F) << 21)
            | ((data[7] as usize & 0x7F) << 14)
            | ((data[8] as usize & 0x7F) << 7)
            | (data[9] as usize & 0x7F);
        offset = 10 + tag_size;
    }
    if offset + 4 > data.len() {
        return None;
    }
    let slice = &data[offset..];
    let sync_pos = slice
        .windows(2)
        .position(|w| w[0] == 0xFF && (w[1] & 0xE0) == 0xE0)?;
    let frame = &slice[sync_pos..];
    if frame.len() < 4 {
        return None;
    }
    let ver_bits = (frame[1] >> 3) & 0x03;
    let layer_bits = (frame[1] >> 1) & 0x03;
    if layer_bits != 1 {
        return None;
    }
    let bitrate_idx = (frame[2] >> 4) & 0x0F;
    let srate_idx = (frame[2] >> 2) & 0x03;
    if bitrate_idx == 0 || bitrate_idx == 15 || srate_idx == 3 {
        return None;
    }
    let sample_rate = match (ver_bits, srate_idx) {
        (3, 0) => 44100,
        (3, 1) => 48000,
        (3, 2) => 32000,
        (2, 0) => 22050,
        (2, 1) => 24000,
        (2, 2) => 16000,
        _ => return None,
    };
    if let Some(pos) = frame
        .windows(4)
        .take(200)
        .position(|w| w == b"Xing" || w == b"Info")
    {
        let xing_body = &frame[pos + 4..];
        if xing_body.len() >= 8 && (xing_body[3] & 0x01 != 0) {
            let frames = u32::from_be_bytes(xing_body[4..8].try_into().ok()?) as f64;
            let dur = frames * 1152.0 / sample_rate as f64;
            if dur > 0.0 && dur.is_finite() && dur < 86400.0 {
                return Some(dur);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_error_status_code_accuracy() {
        // 1. 素材转换与下载失败（应归为客户端 400 错误，杜绝误判为 429）
        let user_err = "素材转换失败: [Failed to download media] tos: request error: Message=tos: read body err:context deadline exceeded";
        assert_eq!(infer_error_status_code_from_str(user_err), 400);

        // 2. 纯网络超时（应归为 504 网关超时，杜绝误判为 429）
        let timeout_err = "tos: request error: Message=tos: read body err:context deadline exceeded";
        assert_eq!(infer_error_status_code_from_str(timeout_err), 504);

        // 3. 真实限流（必须 100% 保持识别为 429）
        assert_eq!(infer_error_status_code_from_str("rate limit exceeded"), 429);
        assert_eq!(infer_error_status_code_from_str("too many requests"), 429);
        assert_eq!(infer_error_status_code_from_str("触发并发限制，请稍后重试"), 429);
        assert_eq!(infer_error_status_code_from_str("quota exceeded for this model"), 429);
        assert_eq!(infer_error_status_code_from_str("API rate throttled"), 429);

        // 4. 欠费/余额不足（402）
        assert_eq!(infer_error_status_code_from_str("insufficient_quota"), 402);
        assert_eq!(infer_error_status_code_from_str("账户余额不足，请充值"), 402);

        // 5. 鉴权失败（401）
        assert_eq!(infer_error_status_code_from_str("Incorrect API key provided"), 401);

        // 6. 权限不足（403）
        assert_eq!(infer_error_status_code_from_str("Access denied for current resource"), 403);

        // 7. 内容风控（400）
        assert_eq!(infer_error_status_code_from_str("content sensitive policy violation"), 400);

        // 8. 显式带有数字 code 的 JSON（优先采用数字 code）
        let json_400 = serde_json::json!({
            "error": {
                "code": "400",
                "message": "素材转换失败: context deadline exceeded"
            }
        });
        assert_eq!(infer_error_status_code(&json_400), 400);

        let json_429 = serde_json::json!({
            "error": {
                "code": "429",
                "message": "Too Many Requests"
            }
        });
        assert_eq!(infer_error_status_code(&json_429), 429);

        // 9. 阿里百炼/DashScope 异步任务失败与 degenerate 文本（必须为 400，杜绝误判为 429）
        let dashscope_degenerate_json = serde_json::json!({
            "output": {
                "task_id": "0f9774f7",
                "task_status": "FAILED",
                "code": "InvalidParameter",
                "message": "Input Video url https://oss.aliyun.com/sample.mp4 contains degenerate frame timestamps"
            }
        });
        assert_eq!(infer_error_status_code(&dashscope_degenerate_json), 400);
        assert_eq!(
            infer_error_status_code_from_str("Input Video url contains degenerate frame timestamps"),
            400
        );
    }

    #[test]
    fn test_upstream_fail_passthrough() {
        let raw_vidu_err = r#"{"code":40001,"message":"invalid parameters","detail":"image prompt required"}"#;

        // 1. 官方原生路由：原始错误体 100% 原样透传，严禁转为 OpenAI 格式
        let err_official = upstream_fail(400, raw_vidu_err, None, "/ent/v2/img2video");
        match &err_official {
            crate::error::AppError::UpstreamHttpError(status, msg, _) => {
                assert_eq!(*status, 400);
                assert_eq!(msg, raw_vidu_err);
                assert!(!msg.contains("\"type\":\"upstream_error\""));
            }
            _ => panic!("Expected UpstreamHttpError"),
        }
        // 验证进入 IntoResponse 后，输出的 JSON 就是原样 Vidu 错误
        use axum::response::IntoResponse;
        let resp = err_official.into_response();
        assert_eq!(resp.status().as_u16(), 400);

        // 1.1 复合路由测试（含 upstream_path，如 /ent/v2/img2video|/ent/v2/img2video）
        let err_composite = upstream_fail(400, raw_vidu_err, None, "/ent/v2/img2video|/ent/v2/img2video");
        match err_composite {
            crate::error::AppError::UpstreamHttpError(status, msg, _) => {
                assert_eq!(status, 400);
                assert_eq!(msg, raw_vidu_err);
            }
            _ => panic!("Expected UpstreamHttpError"),
        }

        // 2. OpenAI 兼容路由：错误体转换为 OpenAI 标准 {"error": {...}}
        let err_openai = upstream_fail(400, raw_vidu_err, None, "/v1/video/generations");
        match err_openai {
            crate::error::AppError::UpstreamHttpError(status, msg, _) => {
                assert_eq!(status, 400);
                let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
                assert!(v.get("error").is_some());
                assert_eq!(v["error"]["code"], "40001");
                assert_eq!(v["error"]["message"], "image prompt required");
            }
            _ => panic!("Expected UpstreamHttpError"),
        }
    }

    #[test]
    fn test_discount_mask_and_price_floor_clamping() {
        // 1. 兼容旧版枚举与新版位掩码 (16 | mask)
        assert_eq!(normalize_discount_mask(0), DISC_ALL);
        assert_eq!(normalize_discount_mask(1), DISC_USER_MODEL | DISC_GLOBAL);
        assert_eq!(normalize_discount_mask(2), DISC_LEVEL | DISC_USER_MODEL);
        assert_eq!(normalize_discount_mask(DISC_V2_FLAG), 0);
        assert_eq!(
            normalize_discount_mask(DISC_V2_FLAG | DISC_LEVEL | DISC_CHANNEL),
            DISC_LEVEL | DISC_CHANNEL
        );

        // 2. 勾选项最低值匹配与全不选回退 1.0x 原价
        assert_eq!(
            resolve_discount(None, 0.8, Some(0.7), 0.6, DISC_V2_FLAG | DISC_ALL),
            (0.6, "渠道倍率")
        );
        assert_eq!(
            resolve_discount(
                None,
                0.8,
                Some(0.7),
                0.6,
                DISC_V2_FLAG | DISC_LEVEL | DISC_USER_MODEL
            ),
            (0.7, "用户模型折扣")
        );
        assert_eq!(
            resolve_discount(None, 0.8, Some(0.7), 0.6, DISC_V2_FLAG),
            (1.0, "默认倍率")
        );

        // 3. 未建档模型 (None) 默认以 1.0 保底防亏损
        assert_eq!(clamp_discount_floor(0.6, None), Some(1.0));
        assert_eq!(clamp_discount_floor(1.0, None), None);

        // 4. 全站折扣 0 是免费；等级和渠道的 0 仍忽略
        assert_eq!(
            resolve_discount(None, 0.0, None, 0.0, DISC_V2_FLAG | DISC_LEVEL | DISC_CHANNEL),
            (1.0, "默认倍率")
        );
        let free = discount_model(0.0, 1, 0.0, 1);
        assert_eq!(
            resolve_discount(Some(&free), 0.8, Some(0.7), 0.6, DISC_V2_FLAG | DISC_ALL),
            (0.0, "全站折扣")
        );
        assert_eq!(clamp_discount_floor(0.0, Some(&free)), None);
        let floored = discount_model(0.0, 1, 0.5, 1);
        assert_eq!(clamp_discount_floor(0.0, Some(&floored)), Some(0.5));
    }

    fn discount_model(global: f64, global_on: i64, site: f64, site_on: i64) -> crate::models::Model {
        crate::models::Model {
            id: 1,
            mid: "000001".into(),
            name: "t".into(),
            model_id: "t".into(),
            original_id: String::new(),
            model_id_alias: String::new(),
            provider_id: None,
            type_id: None,
            api_provider_id: None,
            group_ratios: "{}".into(),
            billing_rule_id: None,
            pre_deduction: 1.0,
            is_active: 1,
            forward_rule_ids: None,
            enable_log_content: 0,
            site_discount: site,
            site_discount_enabled: site_on,
            global_discount: global,
            global_discount_enabled: global_on,
            discount_schedule: String::new(),
            logo: None,
            remark: None,
            description: None,
            feature_attributes: None,
            is_system: 0,
            library_mid: None,
            is_listed: 1,
            listed_count: 0,
            library_kind: None,
            sort_order: 0,
            created_at: crate::time_system::DbTs::default(),
            updated_at: crate::time_system::DbTs::default(),
            type_name: None,
        }
    }
}

