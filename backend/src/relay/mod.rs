/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#[cfg(feature = "commercial_plugins")]
pub mod asset_convert;
pub mod billing_pipeline;
pub mod cascade;
pub mod channel_quota;
pub mod forward;
pub mod ha;
pub mod ha_rule;
pub mod image;
pub mod native;
pub mod proxy;
pub mod quota_memory;
pub mod relay_settings;
pub mod router;
pub mod stream;
pub mod task;
pub mod token_quota;
pub mod upstream_headers;
pub mod url_utils;
pub mod usage_extractor;
pub mod usage_stats;
pub mod voucher;
pub mod vendor_callback;
pub mod video;

#[cfg(not(feature = "commercial_plugins"))]
pub mod asset_convert {
    use crate::AppState;
    pub async fn convert_content_urls(
        _state: &AppState,
        _user_id: &str,
        _plugin_ns: &str,
        _body: &mut serde_json::Value,
        _moderation: bool,
    ) -> (Vec<String>, Vec<String>) {
        (Vec::new(), Vec::new())
    }

    pub async fn convert_content_urls_via_upstream(
        _state: &AppState,
        _user_id: &str,
        _binding_id: i64,
        _body: &mut serde_json::Value,
    ) -> (Vec<String>, Vec<String>) {
        (Vec::new(), Vec::new())
    }
}
pub mod audio;
pub mod balance;
pub mod chat;
pub mod generic;
pub mod model_list;
pub mod response_formatter;
pub mod simulation;
pub mod tos_persist;

/// 挂载在 Request Extensions 上的原始纯净请求体文本（用于日志防污染，避免反向注入 model/stream 等字段被写入日志）
#[derive(Clone, Debug)]
pub struct RawRequestContent(pub String);

use std::future::Future;
use std::sync::Arc;

/// 视频受理后的后台提交计数。生产关闭等到这些任务落账，不随 HTTP 连接取消。
pub struct VideoSubmitDrain {
    tx: tokio::sync::watch::Sender<usize>,
}

pub struct VideoSubmitGuard {
    tx: tokio::sync::watch::Sender<usize>,
}

impl VideoSubmitDrain {
    pub fn new() -> Arc<Self> {
        let (tx, _rx) = tokio::sync::watch::channel(0);
        Arc::new(Self { tx })
    }

    pub fn enter(self: &Arc<Self>) -> VideoSubmitGuard {
        self.tx.send_modify(|n| *n += 1);
        VideoSubmitGuard {
            tx: self.tx.clone(),
        }
    }

    pub async fn wait_idle(&self) {
        let mut rx = self.tx.subscribe();
        while *rx.borrow() > 0 {
            if rx.changed().await.is_err() {
                return;
            }
        }
    }
}

impl Drop for VideoSubmitGuard {
    fn drop(&mut self) {
        self.tx.send_modify(|n| *n = n.saturating_sub(1));
    }
}

/// 连接保护任务产出：上游原文 + 头（格式转换 / 拼 Response 在 task 外）
pub struct UpstreamRaw {
    pub headers: axum::http::HeaderMap,
    pub body: String,
    /// 提交响应里提取的任务 id，供外层 `apply_format` 兜底；其它入口空
    pub task_id: String,
}

impl UpstreamRaw {
    #[inline]
    pub fn new(headers: axum::http::HeaderMap, body: String) -> Self {
        Self {
            headers,
            body,
            task_id: String::new(),
        }
    }
}

/// 同步原文，或流式已启动泵的 Response
pub enum ProtectOut {
    Raw(UpstreamRaw),
    Live(axum::response::Response),
}

/// 连接保护 oneshot 收口：成功 / HA 续试 / 停切
pub enum ProtectJoin<T> {
    Ok(T),
    Retry,
    Stop,
}

/// 连接保护：独立 task 执行 `fut`，结果经 oneshot 回传；客户端断开不影响 fut 跑完
#[inline]
pub fn spawn_protected<T: Send + 'static>(
    fut: impl Future<Output = T> + Send + 'static,
) -> tokio::sync::oneshot::Receiver<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = tx.send(fut.await);
    });
    rx
}

/// 收连接保护结果；拼响应体由调用方在 Ok 分支做
pub async fn join_protected<T>(
    ha: &mut ha::HaAttempt,
    rx: tokio::sync::oneshot::Receiver<Result<T, crate::error::AppError>>,
    ctx: &ha::HaBillCtx<'_>,
    channel: &crate::models::Channel,
    url: Option<&str>,
) -> ProtectJoin<T> {
    match rx.await {
        Ok(Ok(v)) => ProtectJoin::Ok(v),
        Ok(Err(e)) => {
            if ha.fail(ctx, channel, e, url).await {
                ProtectJoin::Retry
            } else {
                ProtectJoin::Stop
            }
        }
        Err(_) => {
            ha.last_err = crate::error::AppError::Internal("请求处理任务异常终止".into());
            ProtectJoin::Stop
        }
    }
}

/// 底座费用 × res_mul；倍率为 1 时原样返回（无 token 时的时长等计费兜底）
pub fn scale_cost_by_res_mul(
    cost: f64,
    detail: String,
    map: &std::collections::HashMap<String, f64>,
    resolution: &str,
) -> (f64, String) {
    let mult = forward::lookup_res_mul(map, resolution);
    if (mult - 1.0).abs() <= 1e-9 {
        return (crate::money::round_money(cost), detail);
    }
    let factor = format!(" * {:.2}倍(级联res_mul:{})", mult, resolution);
    let detail = match detail.find(" | ") {
        Some(idx) => format!("{}{}{}", &detail[..idx], factor, &detail[idx..]),
        None => format!("{}{}", detail, factor),
    };
    (crate::money::round_money(cost * mult), detail)
}

/// 解析卡券模型折扣与常规模型折扣的优先候选（优先采用卡券折扣；若卡券折扣高于常规模型折扣则按常规模型折扣）。
pub fn resolve_coupon_or_base_discount(
    base_src_d: f64,
    base_src_name: &'static str,
    coupon_discount: Option<f64>,
) -> (f64, &'static str) {
    if let Some(cd) = coupon_discount {
        if cd.is_finite() && cd > 0.0 && cd < base_src_d - 1e-6 {
            return (cd, "卡券模型折扣");
        }
    }
    (base_src_d, base_src_name)
}

/// 当单次请求按卡券折扣计费的金额超出首张卡券剩余面值时，按各张有效卡券剩余面值分段折算，超出部分回到常规折扣，确保平台零亏损。
pub fn blend_coupon_partial_cost(
    undiscounted_cost: f64,
    base_src_d: f64,
    db_model: Option<&crate::models::Model>,
    credits: &[(f64, f64)],
) -> f64 {
    if undiscounted_cost <= 0.0 {
        return 0.0;
    }
    let mut uncovered_share = 1.0_f64;
    let mut total_cost = 0.0_f64;
    for &(remaining, credit_discount) in credits {
        if uncovered_share <= 1e-9 || remaining <= 0.0 {
            continue;
        }
        let cand = if credit_discount.is_finite() && credit_discount > 0.0 {
            credit_discount.min(base_src_d)
        } else {
            base_src_d
        };
        let eff_d = crate::relay::proxy::clamp_discount_floor(cand, db_model).unwrap_or(cand);
        let full_cost_at_eff = undiscounted_cost * eff_d;
        if full_cost_at_eff <= 1e-9 {
            uncovered_share = 0.0;
            break;
        }
        let needed_cost = uncovered_share * full_cost_at_eff;
        if remaining + 1e-9 >= needed_cost {
            total_cost += needed_cost;
            uncovered_share = 0.0;
            break;
        } else {
            total_cost += remaining;
            let covered_share = (remaining / full_cost_at_eff).clamp(0.0, uncovered_share);
            uncovered_share -= covered_share;
        }
    }
    if uncovered_share > 1e-9 {
        let normal_eff_d =
            crate::relay::proxy::clamp_discount_floor(base_src_d, db_model).unwrap_or(base_src_d);
        total_cost += uncovered_share * undiscounted_cost * normal_eff_d;
    }
    crate::money::round_money(total_cost)
}

/// 计算最终的消费金额和计费详情文本（包含后置时间段倍率折算，解耦高可用插件并提炼冗余逻辑）
pub async fn calculate_relay_cost(
    state: &crate::AppState,
    db_model: Option<&crate::models::Model>,
    db_rule: Option<&mut crate::models::BillingRule>,
    channel: &crate::models::Channel,
    ctx: &crate::relay::proxy::UserContext,
    usage: &usage_extractor::UsageTokens,
    features: &mut usage_extractor::ExtractedFeatures,
    mapping_source: Option<&str>,
    model_name: &str,
    resolved_model: &str,
) -> (f64, String) {
    let mut priced = db_model.cloned();
    let mut site_changed = false;
    let mut global_changed = false;
    if let Some(model) = priced.as_mut() {
        let before_site = model.site_discount;
        let before_global = model.global_discount;
        if let (Some(site), Some(global)) = (
            features.locked_site_discount,
            features.locked_global_discount,
        ) {
            model.site_discount = site;
            model.global_discount = global;
        } else if crate::models::discount_schedule_enabled(&model.discount_schedule) {
            let tz = relay_settings::get_cached_site_timezone(&state.db).await;
            model.apply_discount_schedule_now(tz.as_ref());
            features.locked_site_discount = Some(model.site_discount);
            features.locked_global_discount = Some(model.global_discount);
        }
        site_changed = (model.site_discount - before_site).abs() > 1e-6;
        global_changed = (model.global_discount - before_global).abs() > 1e-6;
    }
    let db_model = priced.as_ref();

    let umd = db_model
        .and_then(|m| crate::relay::proxy::parse_user_model_discount(&ctx.model_discounts, &m.mid));
    let (base_src_d, base_src_name) = crate::relay::proxy::resolve_discount(
        db_model,
        ctx.discount,
        umd,
        channel.rate,
        ctx.discount_type,
    );
    let mut coupon_credits: Vec<(f64, f64)> = crate::relay::voucher::active_vouchers_for_model(
        &state.db,
        &ctx.user_id,
        db_model,
        model_name,
        resolved_model,
    )
    .await
    .into_iter()
    .map(|v| (v.remaining, v.discount))
    .collect();
    #[cfg(feature = "commercial_plugins")]
    {
        coupon_credits.extend(
            crate::api::plugins::marketing_membership::billing::active_coupon_credits_for_model(
                state,
                &ctx.user_id,
                db_model,
                model_name,
                resolved_model,
            )
            .await
            .into_iter()
            .map(|c| (c.remaining, c.discount)),
        );
    }

    let (src_d, src_name) = resolve_coupon_or_base_discount(
        base_src_d,
        base_src_name,
        coupon_credits.first().map(|c| c.1),
    );
    let floor = crate::relay::proxy::clamp_discount_floor(src_d, db_model);
    let bill_d = floor.unwrap_or(src_d);
    let normal_bill_d =
        crate::relay::proxy::clamp_discount_floor(base_src_d, db_model).unwrap_or(base_src_d);
    let schedule_label =
        (floor.is_some() && site_changed) || (src_name == "全站折扣" && global_changed);

    // 时段倍率 = 模型当前价（只取一档，不与折扣 MIN）；限价只约束候选折扣，不含时段。
    let (mut cost, mut detail) =
        compute_cost_raw(db_model, db_rule.as_deref(), usage, bill_d, features);
    detail = detail.replace(
        '¤',
        &crate::api::settings::get_currency_settings(state)
            .await
            .currency_unit,
    );
    let label = match floor {
        Some(f) => format!("{:.2}倍率(折扣限价)", f),
        None => format!("{:.2}倍率({})", src_d, src_name),
    };
    detail = detail.replacen(&format!("{:.2}倍率", bill_d), &label, 1);

    if let Some(rule) = db_rule {
        let (c, d) = apply_locked_time_multiplier(cost, detail, rule, features);
        cost = c;
        detail = d;
    }

    if floor.is_some() {
        detail.push_str(&format!(" | 原{:.2}({})低于限价", src_d, src_name));
    }

    if bill_d < normal_bill_d - 1e-6 {
        if let Some(&(first_rem, _)) = coupon_credits.first() {
            if cost > first_rem + 1e-6 && bill_d > 1e-9 {
                let undiscounted_cost = cost / bill_d;
                cost = blend_coupon_partial_cost(
                    undiscounted_cost,
                    base_src_d,
                    db_model,
                    &coupon_credits,
                );
                detail.push_str(" | 卡券面值部分抵扣折算");
            }
        }
    }

    if let Some(src) = mapping_source {
        detail.push_str(&format!(" | {}: {} ➞ {}", src, model_name, resolved_model));
    }
    if schedule_label {
        detail.push_str(" | 时段折扣");
    }

    (crate::money::round_money(cost), detail)
}

/// 应用已锁定的峰谷时段倍率（请求开始 / billing_features 快照），防重复乘价与重复明细
pub fn apply_locked_time_multiplier(
    mut cost: f64,
    mut detail: String,
    rule: &mut crate::models::BillingRule,
    features: &usage_extractor::ExtractedFeatures,
) -> (f64, String) {
    // 快照优先（异步任务结算），否则用请求开始时写入的 applied_multiplier
    let time_multiplier = features.time_multiplier.unwrap_or(rule.applied_multiplier);
    rule.applied_multiplier = time_multiplier;

    if (time_multiplier - 1.0).abs() <= 0.00001 {
        return (cost, detail);
    }
    if rule.is_multiplier_applied {
        // 已乘过价：不再改金额，也不再追加明细文案
        return (cost, detail);
    }
    cost *= time_multiplier;
    rule.is_multiplier_applied = true;
    detail = format!("{} * {:.2}倍(时段倍率)", detail, time_multiplier);
    (cost, detail)
}

/// 统一计费逻辑（返回金额已 round 到 6 位小数）
/// 若 `db_rule.applied_multiplier` 已在请求开始锁定（或 features.time_multiplier 有快照），会后置乘时段倍率
/// 明细中的货币单位占位符 `¤` 由调用方换成站点 `currency_unit`
pub fn compute_cost(
    db_model: Option<&crate::models::Model>,
    db_rule: Option<&crate::models::BillingRule>,
    usage: &usage_extractor::UsageTokens,
    discount: f64,
    features: &usage_extractor::ExtractedFeatures,
) -> (f64, String) {
    let (cost, detail) = compute_cost_raw(db_model, db_rule, usage, discount, features);
    let (cost, detail) = if let Some(rule) = db_rule {
        let tm = features.time_multiplier.unwrap_or(rule.applied_multiplier);
        if (tm - 1.0).abs() > 0.00001 {
            (cost * tm, format!("{} * {:.2}倍(时段倍率)", detail, tm))
        } else {
            (cost, detail)
        }
    } else {
        (cost, detail)
    };
    (crate::money::round_money(cost), detail)
}

/// 统一计费逻辑（未舍入，供内部组合后再 round）
/// 单价处写 `¤` 占位货币单位，由 `calculate_relay_cost` / 渠道测试换成站点 `currency_unit`
pub fn compute_cost_raw(
    _db_model: Option<&crate::models::Model>,
    db_rule: Option<&crate::models::BillingRule>,
    usage: &usage_extractor::UsageTokens,
    discount: f64,
    features: &usage_extractor::ExtractedFeatures,
) -> (f64, String) {
    // 从 UsageTokens 解构所需字段（简化后续计算代码）
    let prompt_tokens = usage.prompt;
    let completion_tokens = usage.completion;
    let cached_tokens = usage.cached;
    let cache_write_tokens = usage.cache_write;
    let audio_tokens = usage.audio_tokens;
    let audio_cached_tokens = usage.audio_cached_tokens;
    let rule = match db_rule {
        Some(r) => r,
        None => {
            // 没有配置计费规则，走默认基础计费 1M 万字 = 1美金 等价
            let total = prompt_tokens + completion_tokens;
            let cost = (total as f64 / 1_000_000.0) * discount;
            return (
                cost,
                format!(
                    "无规则默认计费: {}总Tokens * 1/1M * {:.2}倍率",
                    total, discount
                ),
            );
        }
    };

    let apply_web_search = |token_cost_raw: f64, token_detail_raw: String| -> (f64, String) {
        let web_search_count = features.web_search.unwrap_or(0) as f64;
        let web_search_rate =
            if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                ext.get("web_search_rate")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
            } else {
                0.0
            };
        if web_search_count > 0.0 && web_search_rate > 0.0 {
            let ws_cost_raw = web_search_count * web_search_rate / 1000.0;
            let cost = (token_cost_raw + ws_cost_raw) * discount;
            let detail = format!(
                "[ {} + 联网搜索: {}次*{}¤/千次 ] * {:.2}倍率",
                token_detail_raw, web_search_count, web_search_rate, discount
            );
            (cost, detail)
        } else {
            let cost = token_cost_raw * discount;
            let detail = format!("{} * {:.2}倍率", token_detail_raw, discount);
            (cost, detail)
        }
    };

    // 供解析分辨率费率使用的辅助结构体
    #[derive(serde::Deserialize)]
    struct ResolutionTier {
        pub resolution: String,
        pub rate: f64,
        #[serde(default = "crate::relay::default_tier_enabled")]
        pub enabled: bool,
        /// 图片：有图（图生图）倍率，仅 image_resolution 使用
        #[serde(default)]
        pub image_ref_multiplier: Option<f64>,
        /// 图片输入单价，仅 image_resolution_io 使用
        #[serde(default)]
        pub input_rate: f64,
        /// 无参考视频：生成秒单价，仅 video_seconds_ref 区分参考视频时使用
        #[serde(default)]
        pub rate_no_ref: Option<f64>,
        /// 有参考视频：生成秒单价，仅 video_seconds_ref 区分参考视频时使用
        #[serde(default)]
        pub rate_ref: Option<f64>,
        /// 有参考视频：输入秒单价，仅 video_seconds_ref 区分参考视频时使用
        #[serde(default)]
        pub input_rate_ref: Option<f64>,
    }

    /// 供视频画质（分辨率及帧率阶梯）计费使用的辅助结构体
    #[derive(serde::Deserialize)]
    struct VideoQualityTier {
        pub resolution: String,
        pub fps_range: String,
        pub rate: f64,
        #[serde(default = "crate::relay::default_tier_enabled")]
        pub enabled: bool,
    }

    /// 供按分辨率像素计费使用的辅助结构体
    #[derive(serde::Deserialize)]
    struct SizeTier {
        pub size: String,
        #[serde(default)]
        pub rate: f64,
        #[serde(default = "crate::relay::default_tier_enabled")]
        pub enabled: bool,
        /// 有图（图生图）倍率
        #[serde(default)]
        pub image_ref_multiplier: Option<f64>,
        /// 是否启用画质独立定价
        #[serde(default)]
        pub quality_pricing: bool,
        /// 低画质单价（quality_pricing 开启时生效）
        #[serde(default)]
        pub rate_low: Option<f64>,
        /// 中画质单价（quality_pricing 开启时生效）
        #[serde(default)]
        pub rate_medium: Option<f64>,
        /// 高画质单价（quality_pricing 开启时生效）
        #[serde(default)]
        pub rate_high: Option<f64>,
    }

    match rule.billing_type.as_str() {
        "requests" => {
            let mut rate = rule.fixed_rate;
            let mut count = 1.0;
            let mut detail_desc = "固定按次计费".to_string();

            if rule.billing_rule == "per_image" {
                // 按张计费：严格使用从响应提取的实际图片数量；无有效图则不计费（由上层判失败）
                count = match features.image_count {
                    Some(c) if c > 0 => c as f64,
                    _ => {
                        crate::relay_debug!(
                            "[Billing] per_image 规则未获取到有效图片数量，按 0 张不计费"
                        );
                        0.0
                    }
                };
                detail_desc = "按张返回计费".to_string();

                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    // 提示词扩写倍率
                    if features.prompt_extend {
                        if let Some(m) =
                            ext.get("prompt_extend_multiplier").and_then(|v| v.as_f64())
                        {
                            rate *= m;
                            detail_desc.push_str(&format!(" [提示词扩写 x{}]", m));
                        }
                    }
                    // 有图倍率（区分文生图/图生图）
                    if features.has_image_ref {
                        if let Some(m) = ext.get("image_ref_multiplier").and_then(|v| v.as_f64()) {
                            rate *= m;
                            detail_desc.push_str(&format!(" [图生图 x{}]", m));
                        }
                    }
                }
            } else if rule.billing_rule == "image_resolution" {
                count = features.image_count.map(|c| c.max(1) as f64).unwrap_or(1.0);
                detail_desc = format!("分辨率匹配计费(默认单价: {})", rate);
                let res = features.resolution.as_deref().unwrap_or("1k");
                if let Ok(tiers) = serde_json::from_str::<Vec<ResolutionTier>>(&rule.pricing_tiers)
                {
                    let mut matched = false;
                    let mut max_rate: Option<f64> = None;
                    let mut max_res = String::new();
                    for tier in &tiers {
                        if !tier.enabled {
                            continue;
                        }
                        if max_rate.map_or(true, |mr| tier.rate > mr) {
                            max_rate = Some(tier.rate);
                            max_res = tier.resolution.clone();
                        }
                        if tier.resolution.eq_ignore_ascii_case(res) {
                            rate = tier.rate;
                            detail_desc = format!(
                                "命中分辨率阶梯 {} 单价: {}",
                                res,
                                crate::money::format_clean_money(rate)
                            );
                            if features.has_image_ref {
                                if let Some(m) = tier.image_ref_multiplier.filter(|&m| m != 1.0) {
                                    rate *= m;
                                    detail_desc.push_str(&format!(" [图生图 x{}]", m));
                                }
                            }
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        if let Some(mr) = max_rate {
                            rate = mr;
                            detail_desc = format!(
                                "分辨率{}未命中,兜底最高阶梯({} 单价:{})",
                                res,
                                max_res,
                                crate::money::format_clean_money(mr)
                            );
                        }
                    }
                }

                // 提示词扩写倍率（全局，非 per-tier）
                if features.prompt_extend {
                    if let Ok(ext) =
                        serde_json::from_str::<serde_json::Value>(&rule.extended_config)
                    {
                        if let Some(m) =
                            ext.get("prompt_extend_multiplier").and_then(|v| v.as_f64())
                        {
                            rate *= m;
                            detail_desc.push_str(&format!(" [提示词扩写 x{}]", m));
                        }
                    }
                }
            } else if rule.billing_rule == "image_resolution_io" {
                let output_n = features.image_count.map(|c| c.max(1) as f64).unwrap_or(1.0);
                let input_n = features.image_ref_count.unwrap_or(0) as f64;
                let res = features.resolution.as_deref().unwrap_or("1k");
                let mut input_rate = 0.0;
                let mut output_rate = rule.fixed_rate;
                if let Ok(tiers) = serde_json::from_str::<Vec<ResolutionTier>>(&rule.pricing_tiers)
                {
                    let picked = tiers
                        .iter()
                        .filter(|t| t.enabled)
                        .find(|t| t.resolution.eq_ignore_ascii_case(res))
                        .or_else(|| {
                            tiers.iter().filter(|t| t.enabled).max_by(|a, b| {
                                a.rate
                                    .partial_cmp(&b.rate)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                        });
                    if let Some(t) = picked {
                        input_rate = t.input_rate;
                        output_rate = t.rate;
                    }
                }
                let cost = (input_n * input_rate + output_n * output_rate) * discount;
                return (
                    cost,
                    format!(
                        "分辨率K输入/生成 -> (输入:{}张*{} + 生成:{}张*{}) * {:.2}倍率",
                        input_n as i32, input_rate, output_n as i32, output_rate, discount
                    ),
                );
            } else if rule.billing_rule == "image_size_pixel" {
                // 按分辨率像素计费：匹配 size 参数（如 1024x1024），支持画质独立定价
                count = features.image_count.map(|c| c.max(1) as f64).unwrap_or(1.0);
                detail_desc = format!("分辨率像素匹配计费(默认单价: {})", rate);
                let raw_size = features.size.as_deref().unwrap_or("");
                let normalized = normalize_pixel_size(raw_size);

                /// 从匹配到的档位中提取最终费率：画质独立定价时按 quality 选取，否则使用 rate
                fn resolve_tier_rate(tier: &SizeTier, quality: &Option<String>) -> (f64, String) {
                    if tier.quality_pricing {
                        let q = quality.as_deref().unwrap_or("medium");
                        let r = match q {
                            "low" => tier.rate_low.unwrap_or(0.0),
                            "high" => tier.rate_high.unwrap_or(0.0),
                            _ => tier.rate_medium.unwrap_or(0.0), // medium 或未知画质默认取中画质
                        };
                        (r, format!("画质:{}", q))
                    } else {
                        (tier.rate, String::new())
                    }
                }

                /// 取所有启用档位中的最高费率（用于未匹配时兜底）
                fn max_tier_rate(
                    tiers: &[SizeTier],
                    quality: &Option<String>,
                ) -> Option<(f64, String)> {
                    tiers
                        .iter()
                        .filter(|t| t.enabled)
                        .map(|t| {
                            let (r, _) = resolve_tier_rate(t, quality);
                            (r, t.size.clone())
                        })
                        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                }

                if !normalized.is_empty() {
                    if let Ok(tiers) = serde_json::from_str::<Vec<SizeTier>>(&rule.pricing_tiers) {
                        let mut matched = false;
                        for tier in &tiers {
                            if !tier.enabled {
                                continue;
                            }
                            if normalize_pixel_size(&tier.size) == normalized {
                                let (r, q_desc) = resolve_tier_rate(tier, &features.quality);
                                rate = r;
                                detail_desc = if q_desc.is_empty() {
                                    format!(
                                        "命中分辨率像素 {} 单价: {}",
                                        normalized,
                                        crate::money::format_clean_money(rate)
                                    )
                                } else {
                                    format!(
                                        "命中分辨率像素 {} {} 单价: {}",
                                        normalized,
                                        q_desc,
                                        crate::money::format_clean_money(rate)
                                    )
                                };
                                if features.has_image_ref {
                                    if let Some(m) = tier.image_ref_multiplier.filter(|&m| m != 1.0)
                                    {
                                        rate *= m;
                                        detail_desc.push_str(&format!(" [图生图 x{}]", m));
                                    }
                                }
                                matched = true;
                                break;
                            }
                        }
                        if !matched {
                            if let Some((mr, ms)) = max_tier_rate(&tiers, &features.quality) {
                                rate = mr;
                                detail_desc = format!(
                                    "分辨率像素{}未命中,兜底最高阶梯({} 单价:{})",
                                    normalized,
                                    ms,
                                    crate::money::format_clean_money(mr)
                                );
                            }
                        }
                    }
                } else {
                    // size 不是有效像素格式，兜底最高价
                    if let Ok(tiers) = serde_json::from_str::<Vec<SizeTier>>(&rule.pricing_tiers) {
                        if let Some((mr, ms)) = max_tier_rate(&tiers, &features.quality) {
                            rate = mr;
                            detail_desc = format!(
                                "size({})无法解析,兜底最高阶梯({} 单价:{})",
                                raw_size,
                                ms,
                                crate::money::format_clean_money(mr)
                            );
                        }
                    }
                }
                // 提示词扩写倍率
                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    if features.prompt_extend {
                        if let Some(m) =
                            ext.get("prompt_extend_multiplier").and_then(|v| v.as_f64())
                        {
                            rate *= m;
                            detail_desc.push_str(&format!(" [提示词扩写 x{}]", m));
                        }
                    }
                }
            } else if rule.billing_rule == "vidu_image" {
                // 腾讯云 Vidu 图片精确查表：属性×分辨率 → 元/张
                count = features.image_count.map(|c| c.max(1) as f64).unwrap_or(1.0);
                detail_desc = format!("Vidu图片查表(默认单价: {})", rate);
                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    let res = features.resolution.as_deref().unwrap_or("1k");
                    let ref_count = features.image_ref_count.unwrap_or(0);
                    let attr = if !features.has_image_ref {
                        "text"
                    } else if ref_count <= 1 {
                        "img2img"
                    } else if ref_count <= 3 {
                        "ref_1_3"
                    } else {
                        "ref_4_7"
                    };
                    let table_key = format!("{}|{}", attr, res);
                    if let Some(tr) = lookup_price_table(&ext, &table_key) {
                        rate = tr;
                        detail_desc = format!(
                            "Vidu图片查表({} 单价:{})",
                            translate_billing_key(&table_key),
                            rate
                        );
                    } else {
                        // 未命中：兜底使用 price_table 中同属性的最高价
                        if let Some(pt) = ext.get("price_table").and_then(|p| p.as_object()) {
                            let prefix = format!("{}|", attr);
                            let max_price = pt
                                .iter()
                                .filter(|(k, _)| {
                                    k.to_lowercase().starts_with(&prefix.to_lowercase())
                                })
                                .filter_map(|(k, v)| v.as_f64().map(|p| (k.clone(), p)))
                                .max_by(|a, b| {
                                    a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                                });
                            if let Some((mk, mp)) = max_price {
                                rate = mp;
                                detail_desc = format!(
                                    "Vidu图片{}未命中,兜底最高价({} 单价:{})",
                                    translate_billing_key(&table_key),
                                    translate_billing_key(&mk),
                                    mp
                                );
                            }
                        }
                    }
                }
            } else if rule.billing_rule == "characters" {
                // 按字符数计费（语音合成等）：text_characters / 10000 * fixed_rate * discount
                // fixed_rate 单位：元/万字符（如 2.8 表示 2.8元/万字符）
                let chars = features.text_characters.unwrap_or(0) as f64;
                count = chars / 10000.0;
                detail_desc = format!("按字符计费({}字符)", chars as i32);
            } else if rule.billing_rule == "volc_seedream_pro" {
                // 输入图超额（free_image_count，缺省 1）+ 输出按像素档×张数
                // layer_decomposition=true 且阶梯填了图层单价则走图层价，否则单图价
                let free_images = free_image_count_from_ext(&rule.extended_config, 1);
                let input_images = features.image_ref_count.unwrap_or(0);
                let billable_inputs = (input_images - free_images).max(0);
                let input_rate = rule.prompt_rate;
                let input_cost = billable_inputs as f64 * input_rate;

                let image_count = features.image_count.map(|c| c.max(1) as f64).unwrap_or(1.0);
                let total_pixels_wan =
                    pixels_wan_from_size(features.size.as_deref().unwrap_or("1024x1024"));

                #[derive(serde::Deserialize)]
                struct VolcSeedreamTier {
                    pub max_pixels_wan: f64,
                    pub rate: f64,
                    #[serde(default)]
                    pub layer_rate: f64,
                    #[serde(default = "crate::relay::default_tier_enabled")]
                    pub enabled: bool,
                }

                let mut output_rate = rule.fixed_rate;
                let mut match_desc = "默认输出单价".to_string();

                if let Ok(mut tiers) =
                    serde_json::from_str::<Vec<VolcSeedreamTier>>(&rule.pricing_tiers)
                {
                    tiers.sort_by(|a, b| {
                        a.max_pixels_wan
                            .partial_cmp(&b.max_pixels_wan)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let mut matched: Option<&VolcSeedreamTier> = None;
                    for tier in &tiers {
                        if tier.enabled && total_pixels_wan <= tier.max_pixels_wan {
                            matched = Some(tier);
                            break;
                        }
                    }
                    if matched.is_none() {
                        matched = tiers.iter().filter(|t| t.enabled).last();
                    }
                    if let Some(tier) = matched {
                        let layer = features.layer_decomposition && tier.layer_rate > 0.0;
                        output_rate = if layer { tier.layer_rate } else { tier.rate };
                        match_desc = format!(
                            "命中阶梯(<= {:.0}万像素 {} 单价: {})",
                            tier.max_pixels_wan,
                            if layer { "图层" } else { "单图" },
                            crate::money::format_clean_money(output_rate)
                        );
                    }
                }

                let output_cost = image_count * output_rate;
                let total_cost = (input_cost + output_cost) * discount;

                let detail = format!(
                    "火山SeedreamPro计费 -> (输入图超额:{}张(共{}张,免费{}张)*{}¤/张 + 输出图: {}张*[总像素: {:.2}万, {}])*{:.2}倍率",
                    billable_inputs,
                    input_images,
                    free_images,
                    input_rate,
                    image_count,
                    total_pixels_wan,
                    match_desc,
                    discount
                );
                return (total_cost, detail);
            }
            let cost = count * rate * discount;
            (
                cost,
                format!(
                    "{} -> ({}量 * {}单价 * {:.2}倍率)",
                    detail_desc,
                    count,
                    crate::money::format_clean_money(rate),
                    discount
                ),
            )
        }
        "duration" => {
            let dur = features.total_video_seconds();
            let mut rate = rule.duration_rate;
            let mut detail_desc = "固定按秒时长计费".to_string();

            // 分辨率阶梯匹配（video_resolution / minimax_h3 共用）
            let match_res_rate = |resolution: Option<&str>,
                                  allow_max_without_res: bool|
             -> (f64, String) {
                if resolution.is_none() && !allow_max_without_res {
                    return (rate, format!("默认单价: {}", rate));
                }
                let Ok(tiers) = serde_json::from_str::<Vec<ResolutionTier>>(&rule.pricing_tiers)
                else {
                    return (rate, format!("默认单价: {}", rate));
                };
                let mut max_rate: Option<f64> = None;
                let mut max_res = String::new();
                for tier in &tiers {
                    if !tier.enabled {
                        continue;
                    }
                    if max_rate.map_or(true, |mr| tier.rate > mr) {
                        max_rate = Some(tier.rate);
                        max_res = tier.resolution.clone();
                    }
                    if let Some(res) = resolution {
                        if tier.resolution.eq_ignore_ascii_case(res) {
                            return (tier.rate, format!("命中分辨率 {} 单价: {}", res, tier.rate));
                        }
                    }
                }
                if let Some(mr) = max_rate {
                    let desc = match resolution {
                        Some(res) => {
                            format!("分辨率{}未命中，兜底最高阶梯({} 单价:{})", res, max_res, mr)
                        }
                        None => format!("无分辨率，兜底最高阶梯({} 单价:{})", max_res, mr),
                    };
                    return (mr, desc);
                }
                (rate, format!("默认单价: {}", rate))
            };

            if rule.billing_rule == "video_resolution" {
                detail_desc = format!("视频分辨率阶梯找寻(默认单价: {})", rate);
                if let Some(res) = features.resolution.as_deref() {
                    let (r, desc) = match_res_rate(Some(res), false);
                    rate = r;
                    detail_desc = desc;
                }
            } else if rule.billing_rule == "minimax_h3" {
                // 分辨率秒单价 × total_seconds + 输入图超额（free_image_count，缺省 5）
                let (r, desc) = match_res_rate(features.resolution.as_deref(), true);
                rate = r;
                detail_desc = format!("视频秒价+输入图 {}", desc);

                let free_images = free_image_count_from_ext(&rule.extended_config, 5);
                let input_images = features.image_ref_count.unwrap_or(0).max(0);
                let billable_images = (input_images - free_images).max(0);
                let image_rate = rule.prompt_rate;
                let total_cost = (dur * rate + billable_images as f64 * image_rate) * discount;
                return (
                    total_cost,
                    format!(
                        "{} -> ({:.2}秒*{} + 输入图超额:{}张(共{}张,免费{}张)*{}¤/张)*{:.2}倍率",
                        detail_desc,
                        dur,
                        rate,
                        billable_images,
                        input_images,
                        free_images,
                        image_rate,
                        discount
                    ),
                );
            } else if rule.billing_rule == "video_seconds_io" {
                // 输出生成秒数 × 生成秒单价 + 输入参考秒数 × 输入秒单价 + 输入图超额（free_image_count，缺省 5）
                let out_dur = features.duration_seconds.unwrap_or(0.0);
                let in_dur = features.video_ref_seconds.unwrap_or(0.0);
                let res = features.resolution.as_deref();

                let mut in_rate = 0.0;
                let mut out_rate = rate;
                let match_desc;

                if let Ok(tiers) = serde_json::from_str::<Vec<ResolutionTier>>(&rule.pricing_tiers)
                {
                    let enabled_tiers: Vec<&ResolutionTier> =
                        tiers.iter().filter(|t| t.enabled).collect();
                    let picked = if let Some(r) = res {
                        enabled_tiers
                            .iter()
                            .find(|t| t.resolution.eq_ignore_ascii_case(r))
                            .copied()
                    } else {
                        None
                    };
                    if let Some(t) = picked {
                        in_rate = t.input_rate;
                        out_rate = t.rate;
                        match_desc = format!("命中分辨率 {}", t.resolution);
                    } else if let Some(t) = enabled_tiers.iter().max_by(|a, b| {
                        a.rate
                            .partial_cmp(&b.rate)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    }) {
                        in_rate = t.input_rate;
                        out_rate = t.rate;
                        match_desc = match res {
                            Some(r) => format!("分辨率{}未命中，兜底最高阶梯({})", r, t.resolution),
                            None => format!("无分辨率，兜底最高阶梯({})", t.resolution),
                        };
                    } else {
                        match_desc = format!("默认单价: {}", rate);
                    }
                } else {
                    match_desc = format!("默认单价: {}", rate);
                }

                detail_desc = format!("视频秒价(输入/生成)+输入图 {}", match_desc);

                let free_images = free_image_count_from_ext(&rule.extended_config, 5);
                let input_images = features.image_ref_count.unwrap_or(0).max(0);
                let billable_images = (input_images - free_images).max(0);
                let image_rate = rule.prompt_rate;
                let total_cost =
                    (out_dur * out_rate + in_dur * in_rate + billable_images as f64 * image_rate)
                        * discount;
                return (
                    total_cost,
                    format!(
                        "{} -> (生成:{:.2}秒*{} + 输入:{:.2}秒*{} + 输入图超额:{}张(共{}张,免费{}张)*{}¤/张)*{:.2}倍率",
                        detail_desc,
                        out_dur,
                        out_rate,
                        in_dur,
                        in_rate,
                        billable_images,
                        input_images,
                        free_images,
                        image_rate,
                        discount
                    ),
                );
            } else if rule.billing_rule == "video_seconds_ref" {
                // 区分参考视频：输出生成秒数 × 生成秒单价 + 输入参考秒数 × 输入秒单价 + 输入图超额（free_image_count，缺省 5）
                let out_dur = features.duration_seconds.unwrap_or(0.0);
                let in_dur = features.video_ref_seconds.unwrap_or(0.0);
                let has_ref_video = in_dur > 0.0 || features.has_video;
                let res = features.resolution.as_deref();

                let mut in_rate = 0.0;
                let mut out_rate = rate;
                let match_desc;

                let is_ref_aware_enabled =
                    serde_json::from_str::<serde_json::Value>(&rule.extended_config)
                        .ok()
                        .and_then(|v| v.get("enable_video_ref").and_then(|b| b.as_bool()))
                        .unwrap_or(true);

                if let Ok(tiers) = serde_json::from_str::<Vec<ResolutionTier>>(&rule.pricing_tiers)
                {
                    let enabled_tiers: Vec<&ResolutionTier> =
                        tiers.iter().filter(|t| t.enabled).collect();
                    let picked = if let Some(r) = res {
                        enabled_tiers
                            .iter()
                            .find(|t| {
                                t.resolution
                                    .split('/')
                                    .any(|part| part.trim().eq_ignore_ascii_case(r))
                                    || t.resolution.eq_ignore_ascii_case(r)
                            })
                            .copied()
                    } else {
                        None
                    };

                    let pick_rates = |t: &ResolutionTier| -> (f64, f64) {
                        if is_ref_aware_enabled {
                            if has_ref_video {
                                (
                                    t.input_rate_ref.unwrap_or(t.input_rate),
                                    t.rate_ref.unwrap_or(t.rate),
                                )
                            } else {
                                (0.0, t.rate_no_ref.unwrap_or(t.rate))
                            }
                        } else {
                            (t.input_rate, t.rate)
                        }
                    };

                    if let Some(t) = picked {
                        let (ir, or) = pick_rates(t);
                        in_rate = ir;
                        out_rate = or;
                        match_desc = format!("命中分辨率 {}", t.resolution);
                    } else if let Some(t) = enabled_tiers.iter().max_by(|a, b| {
                        a.rate
                            .partial_cmp(&b.rate)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    }) {
                        let (ir, or) = pick_rates(t);
                        in_rate = ir;
                        out_rate = or;
                        match_desc = match res {
                            Some(r) => format!("分辨率{}未命中，兜底最高阶梯({})", r, t.resolution),
                            None => format!("无分辨率，兜底最高阶梯({})", t.resolution),
                        };
                    } else {
                        match_desc = format!("默认单价: {}", rate);
                    }
                } else {
                    match_desc = format!("默认单价: {}", rate);
                }

                let mode_desc = if is_ref_aware_enabled {
                    if has_ref_video {
                        "有参考视频"
                    } else {
                        "无参考视频"
                    }
                } else {
                    "标准双向秒价"
                };

                detail_desc = format!("双向秒价+参考视频({}): {}", mode_desc, match_desc);

                let free_images = free_image_count_from_ext(&rule.extended_config, 5);
                let input_images = features.image_ref_count.unwrap_or(0).max(0);
                let billable_images = (input_images - free_images).max(0);
                let image_rate = rule.prompt_rate;
                let total_cost =
                    (out_dur * out_rate + in_dur * in_rate + billable_images as f64 * image_rate)
                        * discount;
                return (
                    total_cost,
                    format!(
                        "{} -> (生成:{:.2}秒*{} + 输入:{:.2}秒*{} + 输入图超额:{}张(共{}张,免费{}张)*{}¤/张)*{:.2}倍率",
                        detail_desc,
                        out_dur,
                        out_rate,
                        in_dur,
                        in_rate,
                        billable_images,
                        input_images,
                        free_images,
                        image_rate,
                        discount
                    ),
                );
            } else if rule.billing_rule == "fal_ref_video" {
                // fal MiniMax H3 Max 官方公式计费：成片秒价 + 参考素材 Token 池超额
                let out_dur = features.duration_seconds.unwrap_or(0.0);
                let output_cost = out_dur * rule.duration_rate;

                let ext: serde_json::Value =
                    serde_json::from_str(&rule.extended_config).unwrap_or_default();
                let free_ref_tokens = ext
                    .get("free_ref_tokens")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(4096.0);
                let ref_token_rate_per_1k = ext
                    .get("ref_token_rate_per_1k")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.02);
                let image_tokens_default = ext
                    .get("image_tokens_default")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1024.0);
                let audio_tokens_per_sec = ext
                    .get("audio_ref_tokens_per_sec")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(80.0);

                let is_480p = features
                    .resolution
                    .as_deref()
                    .is_some_and(|r| r.to_lowercase().contains("480"));
                let video_tokens_per_sec = if is_480p {
                    ext.pointer("/video_ref_tokens_per_sec/480p")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(2886.0)
                } else {
                    ext.pointer("/video_ref_tokens_per_sec/768p")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(7459.0)
                };

                let image_count = features.image_ref_count.unwrap_or(0).max(0) as f64;
                let image_tokens = image_count * image_tokens_default;

                let video_ref_sec = features.video_ref_seconds.unwrap_or(0.0).max(0.0);
                let video_tokens = (video_ref_sec * video_tokens_per_sec).round();

                let audio_ref_sec = features.audio_ref_seconds.unwrap_or(0.0).max(0.0);
                let audio_tokens = (audio_ref_sec * audio_tokens_per_sec).round();

                let total_ref_tokens = image_tokens + video_tokens + audio_tokens;
                let billable_ref_tokens = (total_ref_tokens - free_ref_tokens).max(0.0);
                let ref_cost = (billable_ref_tokens / 1000.0) * ref_token_rate_per_1k;

                let total_cost = (output_cost + ref_cost) * discount;

                let mut ref_parts = Vec::new();
                if image_count > 0.0 {
                    ref_parts.push(format!("图:{}张({:.0}Token)", image_count, image_tokens));
                }
                if video_ref_sec > 0.0 {
                    ref_parts.push(format!(
                        "视频:{:.1}秒({:.0}Token@{})",
                        video_ref_sec,
                        video_tokens,
                        if is_480p { "480p" } else { "768p" }
                    ));
                }
                if audio_ref_sec > 0.0 {
                    ref_parts.push(format!(
                        "音频:{:.1}秒({:.0}Token)",
                        audio_ref_sec, audio_tokens
                    ));
                }
                let ref_desc = if ref_parts.is_empty() {
                    "无参考素材".to_string()
                } else {
                    ref_parts.join("+")
                };

                let detail = format!(
                    "fal H3-MAX 视频 -> (成片:{:.2}秒*{}¤ + 参考Token:[{} -> 总{:.0}-免{:.0}=计费{:.0}]*{}¤/千Token)*{:.2}倍率",
                    out_dur,
                    rule.duration_rate,
                    ref_desc,
                    total_ref_tokens,
                    free_ref_tokens,
                    billable_ref_tokens,
                    ref_token_rate_per_1k,
                    discount
                );

                return (total_cost, detail);
            } else if rule.billing_rule == "video_quality" {
                detail_desc = format!("视频画质阶梯找寻(默认单价: {})", rate);
                if let Some(res) = &features.resolution {
                    if let Ok(tiers) =
                        serde_json::from_str::<Vec<VideoQualityTier>>(&rule.pricing_tiers)
                    {
                        let fps_val = features.fps.unwrap_or(30.0);
                        let is_high_fps = fps_val > 30.0;

                        let mut target_res = "unknown";
                        let mut short_side: Option<i32> = None;
                        let res_normalized = res.to_lowercase().replace("*", "x");

                        if res_normalized.contains("720p") || res_normalized == "720" {
                            target_res = "720p";
                            short_side = Some(720);
                        } else if res_normalized.contains("1080p") || res_normalized == "1080" {
                            target_res = "1080p";
                            short_side = Some(1080);
                        } else if res_normalized.contains("2k") {
                            target_res = "2k";
                            short_side = Some(1440);
                        } else if res_normalized.contains("4k") {
                            target_res = "4k";
                            short_side = Some(2160);
                        } else {
                            let parts: Vec<&str> = res_normalized.split('x').collect();
                            let mut side: Option<i32> = None;
                            if parts.len() == 2 {
                                if let (Ok(w), Ok(h)) = (
                                    parts[0].trim().parse::<i32>(),
                                    parts[1].trim().parse::<i32>(),
                                ) {
                                    side = Some(std::cmp::min(w, h));
                                }
                            }
                            if side.is_none() {
                                let num_str: String = res_normalized
                                    .chars()
                                    .filter(|c| c.is_ascii_digit())
                                    .collect();
                                if let Ok(num) = num_str.parse::<i32>() {
                                    if num > 100 {
                                        side = Some(num);
                                    }
                                }
                            }

                            if let Some(s) = side {
                                short_side = Some(s);
                                target_res = if s <= 720 {
                                    "720p"
                                } else if s <= 1080 {
                                    "1080p"
                                } else if s <= 1440 {
                                    "2k"
                                } else {
                                    "4k"
                                };
                            }
                        }

                        let mut matched = false;
                        let mut max_rate: Option<f64> = None;
                        let mut max_tier_desc = String::new();

                        for tier in &tiers {
                            if !tier.enabled {
                                continue;
                            }
                            if max_rate.map_or(true, |mr| tier.rate > mr) {
                                max_rate = Some(tier.rate);
                                max_tier_desc = format!("{} {}", tier.resolution, tier.fps_range);
                            }

                            let res_match = tier.resolution.eq_ignore_ascii_case(target_res);
                            let fps_match = if is_high_fps {
                                tier.fps_range.contains(">30") || tier.fps_range.contains("> 30")
                            } else {
                                tier.fps_range.contains("<30")
                                    || tier.fps_range.contains("<=30")
                                    || tier.fps_range.contains("<= 30")
                            };

                            if res_match && fps_match {
                                rate = tier.rate;
                                if let Some(side) = short_side {
                                    detail_desc = format!(
                                        "命中视频画质阶梯 [{}] [帧率: {}] (解析短边: {}) 单价: {}",
                                        target_res, tier.fps_range, side, rate
                                    );
                                } else {
                                    detail_desc = format!(
                                        "命中视频画质阶梯 [{}] [帧率: {}] 单价: {}",
                                        target_res, tier.fps_range, rate
                                    );
                                }
                                matched = true;
                                break;
                            }
                        }

                        if !matched {
                            if let Some(mr) = max_rate {
                                rate = mr;
                                if let Some(side) = short_side {
                                    detail_desc = format!("视频画质阶梯未命中 [{}] (解析短边: {}, 帧率: {})，兜底最高阶梯({} 单价:{})", target_res, side, fps_val, max_tier_desc, mr);
                                } else {
                                    detail_desc = format!("视频画质阶梯未命中 [{}] (帧率: {})，兜底最高阶梯({} 单价:{})", target_res, fps_val, max_tier_desc, mr);
                                }
                            }
                        }
                    }
                }
            } else if rule.billing_rule == "kling_video" {
                // 可灵视频按秒计费：price_table 精确查表优先，倍率乘法兜底
                detail_desc = format!("可灵视频按秒计费(基准: {})", rate);
                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    let mapped_mode;
                    let mode_key = if ext.get("enable_mode").and_then(|v| v.as_bool()).unwrap_or(true) {
                        match features.resolution.as_deref() {
                            Some(r) if r.eq_ignore_ascii_case("720p") => "std",
                            Some(r) if r.eq_ignore_ascii_case("1080p") => "pro",
                            Some(r) => {
                                mapped_mode = r.to_ascii_lowercase();
                                &mapped_mode
                            }
                            None => "std",
                        }
                    } else {
                        "std"
                    };

                    let sound_key = if ext.get("enable_sound").and_then(|v| v.as_bool()).unwrap_or(true) {
                        features.sound.as_deref().unwrap_or("off")
                    } else {
                        "off"
                    };

                    let video_key = if features.has_video && ext.get("enable_video_ref").and_then(|v| v.as_bool()).unwrap_or(false) {
                        "yes"
                    } else {
                        "no"
                    };

                    let table_key = format!("{}|{}|{}", mode_key, sound_key, video_key);
                    if let Some(tr) = lookup_price_table(&ext, &table_key) {
                        rate = tr;
                        detail_desc = format!("可灵视频查表({} 单价:{})", translate_billing_key(&table_key), rate);
                    } else {
                        let m_mult = ext.get("mode_multipliers").and_then(|m| m.get(mode_key)).and_then(|v| v.as_f64()).unwrap_or(1.0);
                        let s_mult = ext.get("sound_multipliers").and_then(|m| m.get(sound_key)).and_then(|v| v.as_f64()).unwrap_or(1.0);
                        rate *= m_mult * s_mult;
                        detail_desc = format!("可灵视频倍率(mode:{}x{} sound:{}x{} 单价:{})", mode_key, m_mult, sound_key, s_mult, rate);
                    }
                }
            } else if rule.billing_rule == "vidu_video" {
                // 腾讯云 Vidu 视频精确查表：属性×分辨率 → 元/秒
                detail_desc = format!("Vidu视频查表(基准: {})", rate);
                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    let res = features.resolution.as_deref().unwrap_or("540p");
                    let is_ref = features.has_video
                        || features.has_audio
                        || features.image_ref_count.unwrap_or(0) > 2;
                    let attr = if is_ref {
                        "ref"
                    } else if features.has_image_ref {
                        "image"
                    } else {
                        "text"
                    };
                    let table_key = format!("{}|{}", attr, res);
                    if let Some(tr) = lookup_price_table(&ext, &table_key) {
                        rate = tr;
                        detail_desc = format!(
                            "Vidu视频查表({} 单价:{})",
                            translate_billing_key(&table_key),
                            rate
                        );
                    } else if let Some((mk, mp)) = lookup_price_table_prefix_max(&ext, attr) {
                        rate = mp;
                        detail_desc = format!(
                            "Vidu视频{}未命中,兜底最高价({} 单价:{})",
                            translate_billing_key(&table_key),
                            translate_billing_key(&mk),
                            mp
                        );
                    }
                    // 错峰模式：优先检查请求体的 OutputConfig.OffPeak，其次 service_tier=flex
                    let is_offpeak = features.service_tier.as_deref() == Some("flex");
                    if is_offpeak {
                        let offpeak_discount = ext
                            .get("offpeak_discount")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0);
                        if offpeak_discount < 1.0 {
                            rate *= offpeak_discount;
                            detail_desc.push_str(&format!(" [错峰 x{}]", offpeak_discount));
                        }
                    }
                }
            } else if rule.billing_rule == "volc_enhance_cascade" {
                // 级联封装查表：version(fast|standard|pro|ai，优先规则 res_enhance，缺省 standard)×分辨率×是否有视频输入
                detail_desc = format!("火山级联封装查表(基准: {})", rate);
                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    let raw_version = features.version.as_deref().unwrap_or("standard");
                    // 与级联 cascade.resolution / 阶段二目标缺省一致
                    let raw_res = features.resolution.as_deref().unwrap_or("720p");
                    let raw_video = if features.has_video { "yes" } else { "no" };

                    let table_key = format!("{}|{}|{}", raw_version, raw_res, raw_video);
                    if let Some(tr) = lookup_price_table(&ext, &table_key) {
                        rate = tr;
                        detail_desc = format!(
                            "火山级联封装查表({} 单价:{})",
                            translate_billing_key(&table_key),
                            rate
                        );
                    } else if let Some((mk, mp)) = lookup_price_table_prefix_max(&ext, raw_version) {
                        rate = mp;
                        detail_desc = format!(
                            "火山级联封装{}未命中,兜底最高价({} 单价:{})",
                            translate_billing_key(&table_key),
                            translate_billing_key(&mk),
                            mp
                        );
                    }
                }
            }

            let cost = dur * rate * discount;
            (
                cost,
                format!(
                    "{} -> ({:.2}秒 * {}单价 * {:.2}倍率)",
                    detail_desc, dur, rate, discount
                ),
            )
        }
        _ => {
            // tokens 计费
            let mut p_rate = rule.prompt_rate;
            let mut c_rate = rule.completion_rate;
            let mut is_overridden = false;
            let is_glm_5_3 = rule.billing_rule == "glm_5_3";
            let mut detail_desc = if is_glm_5_3 {
                "glm 5.3".to_string()
            } else {
                "标准 Tokens 计费".to_string()
            };

            if rule.billing_rule == "seedance2.0" {
                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    if let Some(rates) = ext.get("resolution_rates") {
                        let res_key = features.resolution.as_deref().unwrap_or("720p");
                        let tier = rates.as_object().and_then(|obj| {
                            obj.iter()
                                .find(|(k, _)| k.eq_ignore_ascii_case(res_key))
                                .or_else(|| {
                                    obj.iter().find(|(k, _)| k.eq_ignore_ascii_case("720p"))
                                })
                                .map(|(_, v)| v)
                        });
                        let (rate_field, video_label) = if features.has_video {
                            ("with_video", "含视频")
                        } else {
                            ("without_video", "无视频")
                        };
                        if let Some(rate) = tier
                            .and_then(|t| t.get(rate_field))
                            .and_then(|v| v.as_f64())
                        {
                            p_rate = rate;
                            c_rate = rate;
                            is_overridden = true;
                            detail_desc = format!(
                                "Seedance2规则({}|{}|基本单价:{})",
                                res_key, video_label, rate
                            );
                        }
                    }
                }
            } else if rule.billing_rule == "seedance1.5pro" {
                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    let mut rate = None;
                    let mut desc = String::new();
                    if features.has_audio {
                        if let Some(ar) = ext.get("audio_rate").and_then(|v| v.as_f64()) {
                            rate = Some(ar);
                            desc = format!("Seedance1.5规则(含语音单价:{})", ar);
                        }
                    } else {
                        if let Some(br) = ext.get("base_rate").and_then(|v| v.as_f64()) {
                            rate = Some(br);
                            desc = format!("Seedance1.5规则(无语音单价:{})", br);
                        }
                    }
                    if let Some(mut r) = rate {
                        if features.service_tier.as_deref() == Some("flex") {
                            let discount = ext
                                .get("offline_discount")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.5);
                            r *= discount;
                            desc.push_str(" [离线推理]");
                        }
                        p_rate = r;
                        c_rate = r;
                        is_overridden = true;
                        detail_desc = desc;
                    }
                }
            } else if rule.billing_rule == "seedance1.0" {
                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    if features.service_tier.as_deref() == Some("flex") {
                        if let Some(off_rate) = ext.get("offline_rate").and_then(|v| v.as_f64()) {
                            p_rate = off_rate;
                            c_rate = off_rate;
                            is_overridden = true;
                            detail_desc = format!("Seedance1规则(离线推理单价:{})", off_rate);
                        }
                    } else {
                        if let Some(on_rate) = ext.get("online_rate").and_then(|v| v.as_f64()) {
                            p_rate = on_rate;
                            c_rate = on_rate;
                            is_overridden = true;
                            detail_desc = format!("Seedance1规则(在线推理单价:{})", on_rate);
                        }
                    }
                }
            }

            let mut is_cached_rate_set = false;
            let mut cached_r = rule.cached_rate;
            if cached_r > 0.0 {
                is_cached_rate_set = true;
            }
            // 仅阶梯档位可配置写入费率；未配置时写入量并入未缓存输入（保持旧行为）
            let mut is_cache_write_rate_set = false;
            let mut cache_write_r = 0.0;

            if !is_overridden && rule.billing_rule == "tiered" {
                let mut tiers: Vec<crate::models::PricingTier> =
                    serde_json::from_str(&rule.pricing_tiers).unwrap_or_default();
                tiers.sort_by(|a, b| {
                    a.max_prompt_tokens
                        .partial_cmp(&b.max_prompt_tokens)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                // 前端界限单位为千 (K)
                let p_k = prompt_tokens as f64 / 1000.0;
                let c_k = completion_tokens as f64 / 1000.0;
                let matched = tiers.iter().find(|t| {
                    p_k <= t.max_prompt_tokens
                        && t.max_completion_tokens.map_or(true, |mc| c_k <= mc)
                });
                let (tier, desc) = if let Some(t) = matched {
                    let desc = match t.max_completion_tokens {
                        Some(mc) => format!("阶梯(≤{}K入|≤{}K出)", t.max_prompt_tokens, mc),
                        None => format!("阶梯(≤{}K入)", t.max_prompt_tokens),
                    };
                    (Some(t), desc)
                } else if let Some(last) = tiers.last() {
                    (
                        Some(last),
                        format!("阶梯(超限·最高档{}K入)", last.max_prompt_tokens),
                    )
                } else {
                    (None, detail_desc.clone())
                };
                if let Some(tier) = tier {
                    p_rate = tier.prompt_rate;
                    c_rate = tier.completion_rate;
                    // 阶梯未设置缓存费率时，不继承全局规则
                    is_cached_rate_set = tier.cached_rate > 0.0;
                    if is_cached_rate_set {
                        cached_r = tier.cached_rate;
                    }
                    is_cache_write_rate_set = tier.cache_write_rate > 0.0;
                    if is_cache_write_rate_set {
                        cache_write_r = tier.cache_write_rate;
                    }
                    detail_desc = desc;
                }
            } else if !is_overridden && rule.billing_rule == "doubao_chat" {
                // 豆包聊天阶梯计费：分离音频/非音频独立计价，对齐官方公式
                let mut tiers: Vec<crate::models::PricingTier> =
                    serde_json::from_str(&rule.pricing_tiers).unwrap_or_default();
                tiers.sort_by(|a, b| {
                    a.max_prompt_tokens
                        .partial_cmp(&b.max_prompt_tokens)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let p_k = prompt_tokens as f64 / 1000.0;
                let c_k = completion_tokens as f64 / 1000.0;

                let matched_tier = tiers
                    .iter()
                    .find(|t| {
                        p_k <= t.max_prompt_tokens
                            && t.max_completion_tokens.map_or(true, |mc| c_k <= mc)
                    })
                    .or(tiers.last());

                if let Some(tier) = matched_tier {
                    let is_fast = features.service_tier.as_deref() == Some("fast");
                    let tier_desc = match tier.max_completion_tokens {
                        Some(mc) => format!("<={}K_P|<={}K_C", tier.max_prompt_tokens, mc),
                        None => format!("<={}K_P", tier.max_prompt_tokens),
                    };

                    // 分离音频/非音频 token（缓存是 prompt 子集，需拆分）
                    // 注：audio_tokens 在豆包语境下指"非缓存的音频输入 token"，audio_cached_tokens 是"已缓存的音频 token"
                    let eff_prompt = (prompt_tokens - cached_tokens).max(0);
                    let non_audio_prompt = (eff_prompt - audio_tokens).max(0);
                    let non_audio_cached = (cached_tokens - audio_cached_tokens).max(0);

                    // 选取费率组：service_tier=fast → 低延迟（未设置降级常规），否则常规
                    let (pr, cr, cache_r, audio_pr, audio_cache_r) = if is_fast {
                        (
                            if tier.fast_prompt_rate > 0.0 {
                                tier.fast_prompt_rate
                            } else {
                                tier.prompt_rate
                            },
                            if tier.fast_completion_rate > 0.0 {
                                tier.fast_completion_rate
                            } else {
                                tier.completion_rate
                            },
                            if tier.fast_cached_rate > 0.0 {
                                tier.fast_cached_rate
                            } else {
                                tier.cached_rate
                            },
                            if tier.fast_audio_prompt_rate > 0.0 {
                                tier.fast_audio_prompt_rate
                            } else {
                                tier.audio_prompt_rate
                            },
                            if tier.fast_audio_cached_rate > 0.0 {
                                tier.fast_audio_cached_rate
                            } else {
                                tier.audio_cached_rate
                            },
                        )
                    } else {
                        (
                            tier.prompt_rate,
                            tier.completion_rate,
                            tier.cached_rate,
                            tier.audio_prompt_rate,
                            tier.audio_cached_rate,
                        )
                    };

                    let cost_raw = (non_audio_prompt as f64 * pr
                        + audio_tokens as f64 * audio_pr
                        + non_audio_cached as f64 * cache_r
                        + audio_cached_tokens as f64 * audio_cache_r
                        + completion_tokens as f64 * cr)
                        / 1_000_000.0;

                    let mode_label = if is_fast { "低延迟" } else { "常规" };
                    let d_raw = format!("豆包阶梯[{}]({}) -> ({}非音P*{} + {}音P*{} + {}非音C*{} + {}音C*{} + {}Out*{})/1M",
                        mode_label, tier_desc,
                        non_audio_prompt, pr, audio_tokens, audio_pr,
                        non_audio_cached, cache_r, audio_cached_tokens, audio_cache_r,
                        completion_tokens, cr);
                    let (cost, d) = apply_web_search(cost_raw, d_raw);
                    return (cost, d);
                }
            } else if !is_overridden && rule.billing_rule == "multimodal" {
                // 多模态 tokens 全双工分类计价：
                // 输入端：文本输入按 prompt_rate，图片输入按 image_prompt_rate（从 prompt_tokens 拆分）
                // 输出端：文本与思考输出按 completion_rate，图片输出按 image_completion_rate（从 completion_tokens 拆分）
                let mut img_prompt_rate = rule.prompt_rate;
                let mut img_completion_rate = rule.completion_rate;
                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    if let Some(r) = ext.get("image_prompt_rate").and_then(|v| v.as_f64()) {
                        img_prompt_rate = r;
                    }
                    if let Some(r) = ext.get("image_completion_rate").and_then(|v| v.as_f64()) {
                        img_completion_rate = r;
                    }
                }

                // 计算公式：文本输入 + 图片输入 + 文本输出 + 思考输出 + 图片输出
                // prompt_tokens 是上游返回的总输入 tokens（包含文本和图片），纯文本需扣除图片部分，防止双重计费
                let text_prompt = (prompt_tokens - usage.image_tokens).max(0);
                let text_completion = (completion_tokens - usage.output_image_tokens).max(0);
                let thoughts = usage.reasoning_tokens.min(text_completion);
                let pure_text = text_completion - thoughts;

                let cost_raw = (text_prompt as f64 * p_rate
                    + usage.image_tokens as f64 * img_prompt_rate
                    + text_completion as f64 * c_rate
                    + usage.output_image_tokens as f64 * img_completion_rate)
                    / 1_000_000.0;

                let mut parts = Vec::new();
                if text_prompt > 0 || (usage.image_tokens == 0 && prompt_tokens > 0) {
                    parts.push(format!("{}文本入*{}", text_prompt, p_rate));
                }
                if usage.image_tokens > 0 {
                    parts.push(format!("{}图片入*{}", usage.image_tokens, img_prompt_rate));
                }
                if thoughts > 0 {
                    if pure_text > 0 {
                        parts.push(format!("{}文本出*{}", pure_text, c_rate));
                    }
                    parts.push(format!("{}思考*{}", thoughts, c_rate));
                } else if text_completion > 0 || (usage.output_image_tokens == 0 && completion_tokens > 0) {
                    parts.push(format!("{}输出*{}", text_completion, c_rate));
                }
                if usage.output_image_tokens > 0 {
                    parts.push(format!("{}图片出*{}", usage.output_image_tokens, img_completion_rate));
                }
                if parts.is_empty() {
                    parts.push(format!("{}总入*{}", prompt_tokens, p_rate));
                }

                let d_raw = format!("多模态计费 -> ({})/1M", parts.join(" + "));
                let (cost, d) = apply_web_search(cost_raw, d_raw);
                return (cost, d);
            } else if !is_overridden && rule.billing_rule == "gpt_billing" {
                // GPT 官方计费逻辑：对输入文本、输入图片、输出图片、输入文本缓存、输入图片缓存等分别计费，每一项可开启关闭，开启即使用
                let mut cost_raw = 0.0;
                let mut detail_parts = Vec::new();

                if let Ok(ext) = serde_json::from_str::<serde_json::Value>(&rule.extended_config) {
                    if let Some(gpt_config) = ext.get("gpt_config") {
                        // 推导各项 tokens 消耗
                        let total_input = prompt_tokens;
                        let img_input = usage.image_tokens;
                        let text_input = (total_input - img_input).max(0);
                        let total_cached = cached_tokens;

                        // 根据请求特征是否包含图片直接区分文本缓存和图片缓存
                        let (img_cached, text_cached) = if total_cached > 0 {
                            if features.has_image_ref {
                                (total_cached, 0)
                            } else {
                                (0, total_cached)
                            }
                        } else {
                            (0, 0)
                        };

                        let non_cached_text = (text_input - text_cached).max(0);
                        let non_cached_image = (img_input - img_cached).max(0);
                        let img_output = completion_tokens; // 直接复用 completion

                        // 循环遍历计费项，减少冗余代码
                        let items = [
                            ("input_text", "文本输入", non_cached_text),
                            ("input_image", "图片输入", non_cached_image),
                            ("cached_input_text", "文本缓存", text_cached),
                            ("cached_input_image", "图片缓存", img_cached),
                            ("output_image", "图片输出", img_output),
                        ];

                        for &(key, label, tokens) in &items {
                            let enabled = gpt_config
                                .pointer(&format!("/{}/enabled", key))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            if enabled {
                                let rate = gpt_config
                                    .pointer(&format!("/{}/rate", key))
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                                cost_raw += tokens as f64 * rate;
                                detail_parts.push(format!("{}:{}*{}", label, tokens, rate));
                            }
                        }
                        cost_raw /= 1_000_000.0;
                    }
                }

                let d_raw = if detail_parts.is_empty() {
                    "GPT图片计费(未启用任何计费项)".to_string()
                } else {
                    format!("GPT图片计费 -> ({})/1M", detail_parts.join(" + "))
                };

                let cost = cost_raw * discount;
                let detail = format!("{} * {:.2}倍率", d_raw, discount);
                return (cost, detail);
            }

            // 常规单价规则：prompt_tokens 通常已包含多模态图片输入（如 OpenAI、火山方舟等规范），
            // 仅当 prompt_tokens 小于 image_tokens 时（极少数非标上游未计入图片）才补充差额，防止双重计费
            let prompt_tokens = if usage.image_tokens > prompt_tokens {
                usage.image_tokens
            } else {
                prompt_tokens
            };

            // 补充兜底的 flex 离线折扣检查（当使用 standard / tiered 或者其他策略不匹配导致回退时）
            // 确保不与其他已经自带离线的策略(如已被拦截处理过的描述中包含了"离线")产生重复折扣
            // 注意：火山的 Seedance 2.0/2.0 fast 已不支持 flex 离线降级
            if features.service_tier.as_deref() == Some("flex")
                && !detail_desc.contains("离线")
                && rule.billing_rule != "seedance2.0"
            {
                let off_discount = if let Ok(ext) =
                    serde_json::from_str::<serde_json::Value>(&rule.extended_config)
                {
                    ext.get("offline_discount")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.5)
                } else {
                    0.5
                };
                p_rate *= off_discount;
                c_rate *= off_discount;
                if is_cached_rate_set {
                    cached_r *= off_discount;
                }
                if is_cache_write_rate_set {
                    cache_write_r *= off_discount;
                }
                detail_desc = format!("{} [叠加Flex离线折扣: {}倍]", detail_desc, off_discount);
            }

            // 缓存独立计价仅在对应费率开关开启（费率>0）时参与；关闭则不计、不回落到输入价
            let cc = usage.cache_creation;
            let cc_rate = rule.claude_cache_creation_rate;
            let cr_rate = rule.claude_cache_read_rate;
            let bill_cc = !is_glm_5_3 && cc > 0 && cc_rate > 0.0;
            let bill_cr = !is_glm_5_3 && cached_tokens > 0 && cr_rate > 0.0;
            let is_claude = bill_cc || bill_cr;

            // 上游显式 total_tokens 且 total==prompt+completion → OpenAI 子集语义（回填 total 不算）
            let is_openai_format = is_glm_5_3
                || (usage.has_total_tokens && usage.total == prompt_tokens + completion_tokens);

            let (cost, detail_str) = if is_claude {
                let sub_cc = if bill_cc { cc } else { 0 };
                let sub_cr = if bill_cr { cached_tokens } else { 0 };
                let effective_prompt = if is_openai_format && prompt_tokens >= sub_cc + sub_cr {
                    prompt_tokens - sub_cc - sub_cr
                } else {
                    prompt_tokens
                };
                let creation = if bill_cc { cc as f64 * cc_rate } else { 0.0 };
                let read = if bill_cr {
                    cached_tokens as f64 * cr_rate
                } else {
                    0.0
                };
                let cost_raw = (effective_prompt as f64 * p_rate
                    + completion_tokens as f64 * c_rate
                    + creation
                    + read)
                    / 1_000_000.0;
                let mut d = format!(
                    "{} -> ({}P*{} + {}C*{}",
                    detail_desc, effective_prompt, p_rate, completion_tokens, c_rate
                );
                if bill_cc {
                    d.push_str(&format!(" + {}创建@{}", cc, cc_rate));
                }
                if bill_cr {
                    d.push_str(&format!(" + {}读取@{}", cached_tokens, cr_rate));
                }
                d.push_str(")/1M");
                (cost_raw, d)
            } else {
                let (effective_prompt, bill_cache, bill_write) = split_prompt_subsets(
                    prompt_tokens,
                    cached_tokens,
                    cache_write_tokens,
                    is_cached_rate_set,
                    is_cache_write_rate_set,
                    is_openai_format,
                );

                let cost_raw = (effective_prompt as f64 * p_rate
                    + completion_tokens as f64 * c_rate
                    + bill_cache as f64 * cached_r
                    + bill_write as f64 * cache_write_r)
                    / 1_000_000.0;

                let mut d = format!(
                    "{} -> ({:.0}入*{} + {:.0}出*{}",
                    detail_desc, effective_prompt, p_rate, completion_tokens, c_rate
                );
                if bill_cache > 0 {
                    d.push_str(&format!(" + {:.0}读*{}", bill_cache, cached_r));
                }
                if bill_write > 0 {
                    d.push_str(&format!(" + {:.0}写*{}", bill_write, cache_write_r));
                }
                d.push_str(")/1M");
                if is_openai_format && cached_tokens > 0 && bill_cache == 0 {
                    d.push_str(&format!(" [含{:.0}读·按输入价]", cached_tokens));
                }
                if is_openai_format && cache_write_tokens > 0 && bill_write == 0 {
                    d.push_str(&format!(" [含{:.0}写·按输入价]", cache_write_tokens));
                }
                (cost_raw, d)
            };
            let (cost, d) = if is_glm_5_3 {
                (
                    cost * discount,
                    format!("{} * {:.2}倍率", detail_str, discount),
                )
            } else {
                apply_web_search(cost, detail_str)
            };
            (cost, d)
        }
    }
}

/// OpenAI（缓存是 prompt 子集）：独立费率则从 prompt 拆出，否则已含在输入价内。
/// Anthropic（缓存独立于 input）：仅开启的费率参与；关闭则不计、不并入输入。
fn split_prompt_subsets(
    prompt: i32,
    cached: i32,
    cache_write: i32,
    bill_cached: bool,
    bill_write: bool,
    is_openai_format: bool,
) -> (i32, i32, i32) {
    let cached = cached.max(0);
    let cache_write = cache_write.max(0);
    let bill_c = if bill_cached { cached } else { 0 };
    let bill_w = if bill_write { cache_write } else { 0 };
    if is_openai_format && prompt >= cached + cache_write {
        (prompt - bill_c - bill_w, bill_c, bill_w)
    } else {
        (prompt, bill_c, bill_w)
    }
}

/// price_table 查表辅助：精确匹配优先，fallback 大小写不敏感遍历
/// - 被 price_table_disabled 标记的 key 视为未配置（返回 None），防止 0 值覆盖默认费率
/// - 大小写不敏感 fallback 兼容 features.resolution 统一转小写后与后台 key 不一致的情况
fn lookup_price_table(ext: &serde_json::Value, table_key: &str) -> Option<f64> {
    // 检查 disabled 列表（大小写不敏感）
    if let Some(disabled) = ext.get("price_table_disabled").and_then(|v| v.as_array()) {
        let lower_key = table_key.to_lowercase();
        if disabled
            .iter()
            .any(|k| k.as_str().map_or(false, |s| s.to_lowercase() == lower_key))
        {
            return None;
        }
    }
    let pt = ext.get("price_table")?;
    // 精确匹配
    if let Some(v) = pt.get(table_key).and_then(|v| v.as_f64()) {
        return Some(v);
    }
    // 大小写不敏感 fallback
    let lower_key = table_key.to_lowercase();
    pt.as_object()?
        .iter()
        .find(|(k, _)| k.to_lowercase() == lower_key)
        .and_then(|(_, v)| v.as_f64())
}

/// price_table 前缀匹配求最高价兜底辅助：用于按属性/版本分组查表时，未命中具体规格则兜底同前缀下的最高单价
fn lookup_price_table_prefix_max(ext: &serde_json::Value, prefix: &str) -> Option<(String, f64)> {
    let pt = ext.get("price_table")?.as_object()?;
    let prefix_lower = format!("{}|", prefix.to_lowercase());
    pt.iter()
        .filter(|(k, _)| k.to_lowercase().starts_with(&prefix_lower))
        .filter_map(|(k, v)| v.as_f64().map(|p| (k.clone(), p)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

/// 将 price_table 中用 | 拼接的多维度 key 翻译成友好中文格式显示
pub fn translate_billing_key(key: &str) -> String {
    let parts: Vec<String> = key
        .split('|')
        .map(|part| {
            let part_trimmed = part.trim().to_lowercase();
            match part_trimmed.as_str() {
                "std" => "标准".to_string(),
                "pro" => "专业".to_string(),
                "fast" => "极速".to_string(),
                "standard" => "标准".to_string(),
                "ai" => "大模型".to_string(),
                "text" => "文生".to_string(),
                "image" => "图生".to_string(),
                "ref" => "视频参考".to_string(),
                "on" => "有声".to_string(),
                "off" => "无声".to_string(),
                "yes" => "含视频".to_string(),
                "no" => "无视频".to_string(),
                "img2img" => "图生图".to_string(),
                "ref_1_3" => "1-3张参考".to_string(),
                "ref_4_7" => "4-7张参考".to_string(),
                other => other.to_string(),
            }
        })
        .collect();
    parts.join("|")
}

/// 阶梯 enabled 缺省视为启用（与前端 Switch / RateDisplay 语义一致）
pub(crate) fn default_tier_enabled() -> bool {
    true
}

/// 从 extended_config 读取输入图免费张数（缺省用 default，且不为负）
fn free_image_count_from_ext(extended_config: &str, default: i32) -> i32 {
    serde_json::from_str::<serde_json::Value>(extended_config)
        .ok()
        .and_then(|ext| {
            ext.get("free_image_count")
                .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        })
        .unwrap_or(default as i64)
        .max(0) as i32
}

/// Seedream 输出总像素（万）：走 normalize_pixel_size，解析失败按 1024×1024
fn pixels_wan_from_size(size: &str) -> f64 {
    normalize_pixel_size(size)
        .split_once('x')
        .and_then(|(w, h)| {
            let w = w.parse::<f64>().ok()?;
            let h = h.parse::<f64>().ok()?;
            (w > 0.0 && h > 0.0).then_some((w * h) / 10000.0)
        })
        .unwrap_or(104.8576)
}

/// 将 size 参数统一为像素分辨率格式（小写 x 分隔）
/// 支持：1024x1024、1024*1024、1024×1024、1024X1024、2:3
/// K 等级：1k/1.5k/2k/4k → 边长 n×1024
fn normalize_pixel_size(size: &str) -> String {
    let s = size.trim().to_lowercase();
    if let Some(k) = s.strip_suffix('k') {
        if let Ok(n) = k.parse::<f64>() {
            if n > 0.0 {
                let px = (n * 1024.0).round() as u32;
                return format!("{}x{}", px, px);
            }
        }
    }
    // 像素格式：支持 x、*、×（Unicode 乘号）、:（比例）分隔
    let normalized = s
        .replace('*', "x")
        .replace('\u{00d7}', "x")
        .replace(':', "x");
    let parts: Vec<&str> = normalized.split('x').collect();
    if parts.len() == 2 {
        if parts[0].trim().parse::<u32>().is_ok() && parts[1].trim().parse::<u32>().is_ok() {
            return format!("{}x{}", parts[0].trim(), parts[1].trim());
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BillingRule;
    use crate::relay::usage_extractor::{ExtractedFeatures, UsageTokens};

    #[test]
    fn test_kling_video_resolution_billing_modes() {
        let rule = serde_json::from_value::<BillingRule>(serde_json::json!({
            "id": 1, "name": "kling", "billing_type": "duration",
            "prompt_rate": 0.0, "completion_rate": 0.0, "cached_rate": 0.0,
            "claude_cache_creation_rate": 0.0, "claude_cache_read_rate": 0.0,
            "fixed_rate": 0.0, "duration_rate": 0.2, "billing_rule": "kling_video",
            "pricing_tiers": "[]",
            "extended_config": r#"{"mode_multipliers":{"std":1.0,"pro":1.5,"4k":2.5}}"#,
            "is_active": 1, "is_system": 0, "pid": "p", "pricing_type": "duration",
            "sort_order": 0, "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "applied_multiplier": 1.0,
        }))
        .unwrap();

        let cases = [("720p", 1.0), ("1080P", 1.5), ("4K", 2.5), ("", 1.0)];
        for (res, expected) in cases {
            let feat = ExtractedFeatures {
                duration_seconds: Some(5.0),
                resolution: (!res.is_empty()).then(|| res.to_string()),
                ..Default::default()
            };
            let (cost, _) = compute_cost_raw(None, Some(&rule), &UsageTokens::default(), 1.0, &feat);
            assert!((cost - expected).abs() < 1e-6, "Failed for res: {}", res);
        }
    }

    #[test]
    fn test_standard_token_billing_hunyuan_total_tokens() {
        let rule = serde_json::from_value::<BillingRule>(serde_json::json!({
            "id": 2, "name": "hunyuan_rule", "billing_type": "tokens",
            "prompt_rate": 1.0, "completion_rate": 2.0, "cached_rate": 0.0,
            "claude_cache_creation_rate": 0.0, "claude_cache_read_rate": 0.0,
            "fixed_rate": 0.0, "duration_rate": 0.0, "billing_rule": "standard",
            "pricing_tiers": "[]", "extended_config": "{}",
            "is_active": 1, "is_system": 0, "pid": "p2", "pricing_type": "tokens",
            "sort_order": 0, "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "applied_multiplier": 1.0,
        }))
        .unwrap();

        // 混元 3.5 响应：经过 usage_extractor 逆向回填后 completion = 15000, prompt = 0, total = 15000
        let u = UsageTokens {
            prompt: 0,
            completion: 15000,
            total: 15000,
            has_total_tokens: true,
            ..Default::default()
        };
        let feat = ExtractedFeatures::default();
        let (cost, detail) = compute_cost_raw(None, Some(&rule), &u, 1.0, &feat);
        // (0入*1 + 15000出*2)/1M = 0.03
        assert!((cost - 0.03).abs() < 1e-6);
        assert!(detail.contains("0入*1 + 15000出*2"));
    }

    #[test]
    fn test_coupon_discount_priority_floor_and_partial_blend() {
        // 1. 卡券折扣低于常规模型折扣时优先用卡券折扣；高于常规模型折扣时仍按常规模型折扣
        assert_eq!(
            resolve_coupon_or_base_discount(0.8, "全站折扣", Some(0.6)),
            (0.6, "卡券模型折扣")
        );
        assert_eq!(
            resolve_coupon_or_base_discount(0.7, "用户等级", Some(0.85)),
            (0.7, "用户等级")
        );
        assert_eq!(
            resolve_coupon_or_base_discount(0.8, "全站折扣", None),
            (0.8, "全站折扣")
        );

        let model_with_floor = crate::models::Model {
            id: 1,
            mid: "000001".into(),
            name: "gpt-4o".into(),
            model_id: "gpt-4o".into(),
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
            site_discount: 0.6,
            site_discount_enabled: 1,
            global_discount: 0.9,
            global_discount_enabled: 1,
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
        };

        // 2. 兜底始终不能超过模型最低限价 (site_discount = 0.6)
        let (cand, _) = resolve_coupon_or_base_discount(0.9, "全站折扣", Some(0.4));
        assert_eq!(
            crate::relay::proxy::clamp_discount_floor(cand, Some(&model_with_floor)),
            Some(0.6)
        );

        // 3. 部分抵扣折算：原价 10.0，常规折扣 1.0，卡券折扣 0.5 但仅剩 2.0 面值（抵扣 40% 请求），剩余 60% 按 1.0 计费 -> 2.0 + 6.0 = 8.0
        let no_floor_model = crate::models::Model {
            site_discount_enabled: 0,
            ..model_with_floor.clone()
        };
        let blended = blend_coupon_partial_cost(10.0, 1.0, Some(&no_floor_model), &[(2.0, 0.5)]);
        assert!((blended - 8.0).abs() < 1e-6);

        // 4. 部分抵扣受最低限价约束：卡券设 0.4 但模型最低限价 0.6（满额需 6.0），卡券剩 3.0（抵扣 50% 请求），剩余 50% 按常规 0.9 计费 -> 3.0 + 4.5 = 7.5
        let blended_floored =
            blend_coupon_partial_cost(10.0, 0.9, Some(&model_with_floor), &[(3.0, 0.4)]);
        assert!((blended_floored - 7.5).abs() < 1e-6);
    }
}
