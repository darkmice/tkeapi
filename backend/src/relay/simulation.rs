/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::error::{AppError, AppResult};
use crate::models::{BillingRule, Channel, ChannelConfig, UserLevel};
use crate::relay::proxy::{
    clamp_discount_floor, find_active_model_exact, parse_user_model_discount, resolve_discount,
};
use crate::relay::usage_extractor::{ExtractedFeatures, UsageTokens};
use crate::AppState;
use chrono::{Datelike, Timelike};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SimulateUsage {
    #[serde(default)]
    pub prompt_tokens: i64,
    #[serde(default)]
    pub completion_tokens: i64,
    #[serde(default)]
    pub cached_tokens: i64,
    #[serde(default)]
    pub cache_write_tokens: i64,
    #[serde(default)]
    pub audio_tokens: i64,
    #[serde(default)]
    pub audio_cached_tokens: i64,
    #[serde(default)]
    pub image_tokens: i64,
    #[serde(default)]
    pub output_image_tokens: i64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SimulateFeatures {
    #[serde(default)]
    pub web_search: Option<i32>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub duration_seconds: Option<f64>,
    #[serde(default)]
    pub image_count: Option<i32>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub text_characters: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SimulateBillingRequest {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub sub_channel_id: Option<i64>,
    #[serde(default)]
    pub user_level_id: Option<i64>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub custom_discount: Option<f64>,
    #[serde(default)]
    pub custom_upstream_rate: Option<f64>,
    #[serde(default)]
    pub simulated_time: Option<String>,
    #[serde(default)]
    pub usage: Option<SimulateUsage>,
    #[serde(default)]
    pub features: Option<SimulateFeatures>,
    #[serde(default)]
    pub mock_user_balance: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTraceStep {
    pub step: u32,
    pub stage: String,
    pub title: String,
    pub formula: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    pub status: String, // "normal" | "pass" | "warn" | "danger"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementSummary {
    pub pre_deducted: f64,
    pub actual_cost: f64,
    pub refund_amount: f64,
    pub additional_deduct: f64,
    pub billing_detail_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAndMarginAnalysis {
    pub upstream_name: String,
    pub upstream_rate: f64,
    pub upstream_cost: f64,
    pub gross_profit: f64,
    pub margin_percent: f64,
    pub is_loss_making: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSimInfo {
    pub channel_id: i64,
    pub channel_name: String,
    pub channel_rate: f64,
    pub provider_type: String,
    pub group_aid: Option<String>,
    pub status: i64,
    pub is_disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingSimInfo {
    pub requested_model: String,
    pub resolved_model: String,
    pub model_mapping_hit: bool,
    pub mapping_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingRuleSimInfo {
    pub rule_id: Option<i64>,
    pub rule_name: String,
    pub pricing_type: String,
    pub billing_type: String,
    pub is_default_fallback: bool,
    pub rates: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubChannelOption {
    pub id: i64,
    pub name: String,
    pub rate: f64,
    pub status: i64,
    pub provider_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateBillingResponse {
    pub success: bool,
    pub channel_info: ChannelSimInfo,
    pub available_sub_channels: Vec<SubChannelOption>,
    pub routing: RoutingSimInfo,
    pub billing_rule: BillingRuleSimInfo,
    pub pipeline_trace: Vec<PipelineTraceStep>,
    pub settlement: SettlementSummary,
    pub cost_and_margin: CostAndMarginAnalysis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// 执行渠道调用计费全链路内存仿真试算
pub async fn simulate_channel_billing(
    state: &AppState,
    channel_id: i64,
    req: SimulateBillingRequest,
) -> AppResult<SimulateBillingResponse> {
    // 1. 查询目标渠道
    let mut channel: Channel =
        sqlx::query_as(&state.db.format_query("SELECT * FROM channels WHERE id = ?"))
            .bind(channel_id)
            .fetch_optional(&state.db.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("指定的渠道分组不存在".into()))?;

    // 2. 检查渠道可用性与配额（只告警，不拦截沙盒推演）
    let tz_name = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let (now_day, now_week, now_month) = crate::models::quota_period_keys(&tz_name);
    let mut channel_quota_warn: Option<String> = None;
    if let Err(msg) = channel.check_quota_limits(&now_day, &now_week, &now_month) {
        channel_quota_warn = Some(format!("该渠道{}，线上真实调用将被拦截", msg));
    }

    let is_disabled = channel.status != 1;
    let mut general_warning: Option<String> = None;
    if is_disabled {
        general_warning =
            Some("当前渠道处于禁用状态，线上调用将被拦截；此处仅演算理论计费。".into());
    } else if let Some(ref q_warn) = channel_quota_warn {
        general_warning = Some(format!("{}；此处仅演算理论计费。", q_warn));
    }

    // 3. 上游与采购费率推导（支持高可用组子渠道下拉及自定义覆盖）
    let mut available_sub_channels = Vec::new();
    let mut resolved_upstream_name = "自定义上游".to_string();
    let mut resolved_upstream_rate = 1.0;

    if channel.provider_type == "high_availability_group" {
        let config: serde_json::Value =
            serde_json::from_str(&channel.config).unwrap_or(serde_json::json!({}));
        let sub_ids: Vec<i64> = config
            .get("sub_channels")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();

        if !sub_ids.is_empty() {
            let placeholders = sub_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT id, name, rate, status, provider_type, priority FROM channel_configs WHERE id IN ({})",
                placeholders
            );
            let formatted = state.db.format_query(&sql);
            let mut query = sqlx::query_as::<_, (i64, String, f64, i64, String, i64)>(&formatted);
            for sid in &sub_ids {
                query = query.bind(sid);
            }
            if let Ok(rows) = query.fetch_all(&state.db.pool).await {
                let mut rows_with_order: Vec<(i64, usize, SubChannelOption)> = rows
                    .into_iter()
                    .map(|(id, name, rate, status, provider_type, def_priority)| {
                        let eff_priority =
                            crate::models::channel::extract_upstream_override(&config, id)
                                .and_then(|ov| ov.priority)
                                .unwrap_or(def_priority);
                        let pos = sub_ids
                            .iter()
                            .position(|&sid| sid == id)
                            .unwrap_or(usize::MAX);
                        (
                            eff_priority,
                            pos,
                            SubChannelOption {
                                id,
                                name,
                                rate,
                                status,
                                provider_type,
                            },
                        )
                    })
                    .collect();
                rows_with_order.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                for (_, _, opt) in rows_with_order {
                    available_sub_channels.push(opt);
                }
            }
        }

        // 高可用下支持下拉任意切换子渠道测试，默认按最高优先级首项
        let mut selected_sub_id: Option<i64> = None;
        if let Some(target_sub_id) = req.sub_channel_id {
            if let Some(matched) = available_sub_channels
                .iter()
                .find(|s| s.id == target_sub_id)
            {
                resolved_upstream_name = format!("HA子渠道: {}", matched.name);
                resolved_upstream_rate = matched.rate;
                channel.rate = matched.rate;
                selected_sub_id = Some(matched.id);
            }
        } else if let Some(first_sub) = available_sub_channels.first() {
            resolved_upstream_name = format!("HA子渠道(默认首项): {}", first_sub.name);
            resolved_upstream_rate = first_sub.rate;
            channel.rate = first_sub.rate;
            selected_sub_id = Some(first_sub.id);
        }
        if let Some(sid) = selected_sub_id {
            crate::relay::router::apply_ha_model_mapping(&mut channel, sid);
        }
    } else if let Some(preset_id) = channel.preset_id {
        if let Ok(Some(preset)) = sqlx::query_as::<_, ChannelConfig>(
            &state
                .db
                .format_query("SELECT * FROM channel_configs WHERE id = ?"),
        )
        .bind(preset_id)
        .fetch_optional(&state.db.pool)
        .await
        {
            let resolved = channel.resolve_upstream_config(&preset);
            channel.rate = resolved.rate;
            channel.priority = resolved.priority;
            channel.weight = resolved.weight;
            resolved_upstream_name = format!("上游预设: {}", preset.name);
            resolved_upstream_rate = resolved.rate;
        }
    }

    // 若请求显式传入自定义上游费率（覆盖默认），以管理员指定为准，并同步更新 channel.rate 供折扣比对使用
    if let Some(custom_rate) = req.custom_upstream_rate {
        if custom_rate >= 0.0 {
            resolved_upstream_rate = custom_rate;
            channel.rate = custom_rate;
            resolved_upstream_name = format!("{} (手动指定倍率)", resolved_upstream_name);
        }
    }

    // 4. 路由与模型映射推导 (Single Source of Truth: router::resolve_model)
    // 允许传入 model_id 或 mid；若为空则兜底取渠道首个可用模型
    let raw_model = if req.model.trim().is_empty() {
        channel
            .get_models()
            .into_iter()
            .next()
            .unwrap_or_else(|| "gpt-4o".into())
    } else {
        req.model.trim().to_string()
    };

    // 优先尝试按 model_id 精确查找，其次尝试按 6 位系统 mid 查找
    let db_model = if let Some(m) =
        find_active_model_exact(state, &raw_model, None, Some(&channel)).await
    {
        Some(m)
    } else if let Some(m) = crate::relay::proxy::find_active_model_by_mid(state, &raw_model).await {
        Some(m)
    } else {
        None
    };

    // 若传入的是 mid，将请求模型名称规范化为真实 model_id
    let requested_model = if let Some(ref m) = db_model {
        if raw_model == m.mid && !m.model_id.is_empty() {
            m.model_id.clone()
        } else {
            raw_model
        }
    } else {
        raw_model
    };

    let orig_group_aid = channel.group_aid.clone();
    if channel.provider_type == "high_availability_group" {
        let active_sid = req
            .sub_channel_id
            .filter(|id| available_sub_channels.iter().any(|s| s.id == *id))
            .or_else(|| available_sub_channels.first().map(|s| s.id));
        if let Some(sid) = active_sid {
            channel.group_aid = Some(format!("ha_group_{}_config_{}", channel.id, sid));
        }
    }
    let sim_res = req
        .features
        .as_ref()
        .and_then(|f| f.resolution.as_deref().or(f.size.as_deref()))
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let (resolved_model, map_src_opt) = crate::relay::router::resolve_model(
        &channel,
        &requested_model,
        db_model.as_ref(),
        sim_res,
    );
    channel.group_aid = orig_group_aid;

    let mapping_map = channel.get_model_mapping();
    let (mapping_hit, mapping_source) = match map_src_opt.as_deref() {
        Some(src) if src.starts_with("分辨率映射@") => {
            (true, format!("渠道按{}映射", src))
        }
        Some("渠道映射") => {
            if mapping_map.contains_key(&requested_model) {
                (true, "渠道模型映射(精确匹配)".to_string())
            } else {
                (true, "渠道模型映射(正则规则匹配)".to_string())
            }
        }
        Some("模型映射") => (true, "全局模型库别名映射".to_string()),
        Some(other) => (true, other.to_string()),
        None => (false, "无映射(直通模型名)".to_string()),
    };

    let mut active_model = if db_model.is_some() {
        db_model
    } else if requested_model != resolved_model {
        find_active_model_exact(state, &resolved_model, None, Some(&channel)).await
    } else {
        None
    };

    let mut db_rule: Option<BillingRule> = None;
    if let Some(ref m) = active_model {
        if let Some(rule_id) = m.billing_rule_id {
            db_rule = sqlx::query_as(
                &state
                    .db
                    .format_query("SELECT * FROM billing_rules WHERE id = ?"),
            )
            .bind(rule_id)
            .fetch_optional(&state.db.pool)
            .await
            .unwrap_or(None);
        }
    }

    // 6. 解析模拟时段并计算峰谷波段倍率 (单点真理: multiplier_at_local_time)
    let site_tz_parsed: Tz = tz_name.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let (sim_local_time, sim_weekday) = if let Some(ref time_str) = req.simulated_time {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(time_str) {
            let local = dt.with_timezone(&site_tz_parsed);
            (local.time(), local.weekday())
        } else if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M:%S")
        {
            (dt.time(), dt.weekday())
        } else if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M") {
            (dt.time(), dt.weekday())
        } else if let Ok(t) = chrono::NaiveTime::parse_from_str(time_str, "%H:%M") {
            (
                t,
                chrono::Utc::now().with_timezone(&site_tz_parsed).weekday(),
            )
        } else {
            let now = chrono::Utc::now().with_timezone(&site_tz_parsed);
            (now.time(), now.weekday())
        }
    } else {
        let now = chrono::Utc::now().with_timezone(&site_tz_parsed);
        (now.time(), now.weekday())
    };
    let mut site_changed = false;
    let mut global_changed = false;
    if let Some(model) = active_model.as_mut() {
        let before_site = model.site_discount;
        let before_global = model.global_discount;
        model.apply_discount_schedule_at(sim_local_time, sim_weekday);
        site_changed = (model.site_discount - before_site).abs() > 1e-6;
        global_changed = (model.global_discount - before_global).abs() > 1e-6;
    }

    let time_multiplier = if let Some(ref rule) = db_rule {
        rule.multiplier_at_local_datetime(sim_local_time, sim_weekday)
    } else {
        1.0
    };

    // 7. 用户身份折扣与余额上下文推导
    let mut resolved_level_discount = 1.0;
    let mut resolved_discount_type = 0;
    let mut user_model_discounts: Option<String> = None;
    let mut user_context_balance = 0.0;
    let mut user_identity_desc = "默认标准用户".to_string();
    let mut resolved_real_user_id = String::new();

    let default_level_opt: Option<UserLevel> = sqlx::query_as(&state.db.format_query(
        "SELECT * FROM user_levels WHERE is_default = 1 OR group_key = 'default' ORDER BY is_default DESC, id ASC LIMIT 1",
    ))
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    if let Some(ref def_lvl) = default_level_opt {
        resolved_level_discount = def_lvl.discount;
        resolved_discount_type = def_lvl.discount_type;
        user_identity_desc = format!("默认等级: {} (折扣: {:.2}x)", def_lvl.name, def_lvl.discount);
    }

    if let Some(ref uid_or_name) = req.user_id {
        if !uid_or_name.trim().is_empty() {
            let u_row: Option<(String, String, f64, f64, Option<String>, i64, String, f64, i64)> = sqlx::query_as(
                &state.db.format_query(
                    "SELECT u.id, u.user_group, u.balance, u.gift_balance, u.model_discounts, \
                     COALESCE(ul.id, 0), COALESCE(ul.name, ''), COALESCE(ul.discount, 1.0), COALESCE(ul.discount_type, 0) \
                     FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key \
                     WHERE u.id = ? OR u.uid = ? OR u.username = ?"
                )
            )
            .bind(uid_or_name.trim())
            .bind(uid_or_name.trim())
            .bind(uid_or_name.trim())
            .fetch_optional(&state.db.pool)
            .await
            .unwrap_or(None);

            if let Some((uid, u_grp, bal, gift, md, lid, l_name, d, dt)) = u_row {
                resolved_real_user_id = uid;
                user_context_balance = bal + gift;
                user_model_discounts = md;
                if lid > 0 {
                    resolved_level_discount = d;
                    resolved_discount_type = dt;
                    user_identity_desc = format!(
                        "真实用户: {} (分组: {}, 等级: {})",
                        uid_or_name, u_grp, l_name
                    );
                } else if let Some(ref def_lvl) = default_level_opt {
                    resolved_level_discount = def_lvl.discount;
                    resolved_discount_type = def_lvl.discount_type;
                    user_identity_desc = format!(
                        "真实用户: {} (原分组 {} 已失效，回退默认等级: {})",
                        uid_or_name, u_grp, def_lvl.name
                    );
                } else {
                    resolved_level_discount = d;
                    resolved_discount_type = dt;
                    user_identity_desc = format!("真实用户: {} (分组: {})", uid_or_name, u_grp);
                }
            }
        }
    } else if let Some(lid) = req.user_level_id {
        let l_row: Option<UserLevel> = sqlx::query_as(
            &state
                .db
                .format_query("SELECT * FROM user_levels WHERE id = ?"),
        )
        .bind(lid)
        .fetch_optional(&state.db.pool)
        .await
        .unwrap_or(None);

        if let Some(level) = l_row {
            resolved_level_discount = level.discount;
            resolved_discount_type = level.discount_type;
            user_identity_desc = format!("用户等级: {} (折扣: {:.2}x)", level.name, level.discount);
        }
    }

    if let Some(custom_d) = req.custom_discount {
        if custom_d > 0.0 {
            resolved_level_discount = custom_d;
            resolved_discount_type = crate::relay::proxy::DISC_V2_FLAG
                | crate::relay::proxy::normalize_discount_mask(resolved_discount_type)
                | crate::relay::proxy::DISC_LEVEL;
            user_identity_desc = format!("{} [手动折率: {:.2}x]", user_identity_desc, custom_d);
        }
    }

    if let Some(mock_bal) = req.mock_user_balance {
        user_context_balance = mock_bal;
    }

    // 8. 构造算力特征参数与 UsageTokens
    let usage_req = req.usage.unwrap_or_default();
    let features_req = req.features.unwrap_or_default();

    let usage_tokens = UsageTokens {
        prompt: usage_req.prompt_tokens as i32,
        completion: usage_req.completion_tokens as i32,
        total: (usage_req.prompt_tokens + usage_req.completion_tokens) as i32,
        has_total_tokens: true,
        cached: usage_req.cached_tokens as i32,
        cache_write: usage_req.cache_write_tokens as i32,
        cache_creation: 0,
        audio_tokens: usage_req.audio_tokens as i32,
        audio_cached_tokens: usage_req.audio_cached_tokens as i32,
        image_tokens: usage_req.image_tokens as i32,
        output_image_tokens: usage_req.output_image_tokens as i32,
        reasoning_tokens: 0,
        web_search: features_req.web_search.unwrap_or(0),
    };

    let eff_resolution = features_req.resolution.clone();
    let eff_size = features_req.size.clone().or_else(|| eff_resolution.clone());

    let extracted_features = ExtractedFeatures {
        has_video: features_req.duration_seconds.is_some(),
        has_audio: usage_req.audio_tokens > 0 || features_req.text_characters.is_some(),
        has_image_ref: false,
        duration_seconds: features_req.duration_seconds,
        resolution: eff_resolution,
        image_count: features_req.image_count,
        service_tier: None,
        prompt_extend: false,
        layer_decomposition: false,
        sound: None,
        image_ref_count: None,
        size: eff_size,
        quality: features_req.quality.clone(),
        text_characters: features_req.text_characters,
        fps: None,
        version: None,
        web_search: features_req.web_search,
        time_multiplier: Some(time_multiplier),
        locked_site_discount: None,
        locked_global_discount: None,
        video_ref_seconds: None,
        audio_ref_seconds: None,
    };

    // 9. 核心基准计费运算 (Single Source of Truth: compute_cost_raw)
    // 第一遍以 discount = 1.0 计算出纯净基准原始费用
    let (base_cost_raw, _base_detail) = crate::relay::compute_cost_raw(
        active_model.as_ref(),
        db_rule.as_ref(),
        &usage_tokens,
        1.0,
        &extracted_features,
    );

    // 10. 用户身份与渠道倍率统一折扣判定 (Single Source of Truth: resolve_discount)
    let user_mid_discount = active_model
        .as_ref()
        .and_then(|m| parse_user_model_discount(&user_model_discounts, &m.mid));
    let (base_src_discount, base_source_name) = resolve_discount(
        active_model.as_ref(),
        resolved_level_discount,
        user_mid_discount,
        channel.rate,
        resolved_discount_type,
    );
    let coupon_discount = if !resolved_real_user_id.is_empty() {
        let voucher_d = crate::relay::voucher::active_vouchers_for_model(
            &state.db,
            &resolved_real_user_id,
            active_model.as_ref(),
            &requested_model,
            &resolved_model,
        )
        .await
        .first()
        .map(|v| v.discount);
        if voucher_d.is_some() {
            voucher_d
        } else {
            #[cfg(feature = "commercial_plugins")]
            {
                crate::api::plugins::marketing_membership::billing::active_coupon_credits_for_model(
                    state,
                    &resolved_real_user_id,
                    active_model.as_ref(),
                    &requested_model,
                    &resolved_model,
                )
                .await
                .first()
                .map(|c| c.discount)
            }
            #[cfg(not(feature = "commercial_plugins"))]
            {
                None
            }
        }
    } else {
        None
    };

    let (src_discount, discount_source_name) = crate::relay::resolve_coupon_or_base_discount(
        base_src_discount,
        base_source_name,
        coupon_discount,
    );

    // 11. 保底限价校验 (Single Source of Truth: clamp_discount_floor)
    let floor_clamped = clamp_discount_floor(src_discount, active_model.as_ref());
    let (billing_discount, is_floor_hit) = match floor_clamped {
        Some(floor_val) => (floor_val, true),
        None => (src_discount, false),
    };

    // 12. 最终算力计费与时段倍率折算
    let (mut cost_after_discount, mut detail_str) = crate::relay::compute_cost_raw(
        active_model.as_ref(),
        db_rule.as_ref(),
        &usage_tokens,
        billing_discount,
        &extracted_features,
    );

    // 替换货币占位符为站点实际货币配置
    let currency_settings = crate::api::settings::get_currency_settings(state).await;
    let c_unit = &currency_settings.currency_unit;
    let c_sym = &currency_settings.currency_symbol;
    detail_str = detail_str.replace('¤', c_unit);

    let discount_label = if is_floor_hit {
        format!("{:.2}倍率(保底限价生效)", billing_discount)
    } else {
        format!("{:.2}倍率({})", src_discount, discount_source_name)
    };
    detail_str = detail_str.replacen(&format!("{:.2}倍率", billing_discount), &discount_label, 1);

    // 峰谷时段倍率叠加
    if (time_multiplier - 1.0).abs() > 0.00001 {
        cost_after_discount *= time_multiplier;
        detail_str = format!("{} * {:.2}倍(时段倍率)", detail_str, time_multiplier);
    }

    if is_floor_hit {
        detail_str.push_str(&format!(
            " | 原{:.2}({})低于保底限价",
            src_discount, discount_source_name
        ));
    }

    if mapping_hit {
        detail_str.push_str(&format!(
            " | 渠道映射: {} ➞ {}",
            requested_model, resolved_model
        ));
    }
    if (is_floor_hit && site_changed) || (discount_source_name == "全站折扣" && global_changed)
    {
        detail_str.push_str(" | 时段折扣");
    }

    let actual_cost = crate::money::round_money(cost_after_discount);

    // 13. 预扣费冻结与差额退补（对齐生产环境：视频分类模型按秒动态预扣，防止透支）
    let is_video_model = active_model.as_ref().map_or(false, |m| {
        m.type_name.as_deref().unwrap_or("").contains("视频")
    }) || extracted_features.has_video;

    let unit_pre = active_model
        .as_ref()
        .map(|m| m.pre_deduction)
        .unwrap_or(0.0);
    let dur = extracted_features.duration_seconds.unwrap_or(5.0).max(1.0);
    let (pre_deducted, is_by_second) = if is_video_model && unit_pre > 0.0 {
        (crate::money::round_money(unit_pre * dur), true)
    } else {
        (crate::money::round_money(unit_pre), false)
    };

    let refund_amount = if pre_deducted > actual_cost {
        crate::money::round_money(pre_deducted - actual_cost)
    } else {
        0.0
    };
    let additional_deduct = if actual_cost > pre_deducted {
        crate::money::round_money(actual_cost - pre_deducted)
    } else {
        0.0
    };

    // 14. 上游成本与毛利率核算
    let upstream_cost = crate::money::round_money(base_cost_raw * resolved_upstream_rate);
    let gross_profit = crate::money::round_money(actual_cost - upstream_cost);
    let margin_percent = if actual_cost.abs() > 0.000001 {
        ((actual_cost - upstream_cost) / actual_cost * 10000.0).round() / 100.0
    } else {
        0.0
    };
    let is_loss_making = actual_cost < upstream_cost;
    let margin_warning_message = if is_loss_making {
        Some(format!(
            "⚠️ 严重风险：当前终端收费 {} {:.6} 低于上游采购成本 {} {:.6}，单次调用亏损 {} {:.6}（毛利率 {:.2}%）。请立即提高渠道倍率或提高模型保底限价！",
            c_sym, actual_cost, c_sym, upstream_cost, c_sym, (upstream_cost - actual_cost).abs(), margin_percent
        ))
    } else {
        None
    };

    // 15. 组装 Pipeline Trace 全链路分步演化详情
    let mut steps = Vec::new();

    // Step 1: 路由与别名解析
    steps.push(PipelineTraceStep {
        step: 1,
        stage: "routing".into(),
        title: "路由与模型映射解析".into(),
        formula: format!("{} ➞ {}", requested_model, resolved_model),
        description: if mapping_hit {
            format!(
                "成功命中{}，实际将请求上游真实模型名 {}",
                mapping_source, resolved_model
            )
        } else {
            "未配置模型重命名映射，直接按请求模型名直通转发上游".into()
        },
        value: None,
        status: "normal".into(),
    });

    // Step 2: 预扣费冻结校验
    let pre_deduct_pass = user_context_balance >= pre_deducted;
    let step2_formula = if is_by_second {
        format!(
            "按秒预扣冻结: {} {}/秒 × {:.0}秒 = {} {}",
            crate::money::format_clean_money(unit_pre),
            c_unit,
            dur,
            crate::money::format_clean_money(pre_deducted),
            c_unit
        )
    } else {
        format!("预扣冻结: {:.6} {}", pre_deducted, c_unit)
    };
    let step2_desc = if pre_deducted <= 0.0 {
        "该模型未配置固定或按秒预扣金额，请求前免冻结直接透传".into()
    } else if !pre_deduct_pass {
        if is_by_second {
            format!(
                "模拟用户可用余额 {:.4} {} 不足，无法满足按秒预扣要求 {:.4} {}（{} {}/秒 × {:.0}秒），线上将被拦截",
                user_context_balance, c_unit, pre_deducted, c_unit, crate::money::format_clean_money(unit_pre), c_unit, dur
            )
        } else {
            format!(
                "模拟用户可用余额 {:.4} {} 不足，无法满足预扣 {:.4} {}，线上将被拦截",
                user_context_balance, c_unit, pre_deducted, c_unit
            )
        }
    } else if is_by_second {
        format!(
            "检测到当前为视频生成任务，系统严格执行「每秒预扣单价 × 视频时长」动态预扣机制。模拟用户钱包可用余额 {:.4} {} ≥ 预扣要求 {:.4} {}，预扣冻结校验通过",
            user_context_balance, c_unit, pre_deducted, c_unit
        )
    } else {
        format!(
            "模拟用户钱包可用余额 {:.4} {} ≥ 预扣要求 {:.4} {}，预扣冻结校验通过",
            user_context_balance, c_unit, pre_deducted, c_unit
        )
    };
    steps.push(PipelineTraceStep {
        step: 2,
        stage: "pre_deduct".into(),
        title: if is_by_second {
            "预扣费冻结校验 (按秒预扣)".into()
        } else {
            "预扣费冻结校验".into()
        },
        formula: step2_formula,
        description: step2_desc,
        value: Some(pre_deducted),
        status: if pre_deduct_pass {
            "pass".into()
        } else {
            "danger".into()
        },
    });

    // Step 3: 基准算力费用
    steps.push(PipelineTraceStep {
        step: 3,
        stage: "base_cost".into(),
        title: "基准未折算算力费用".into(),
        formula: format!("基础算力原价 = {:.6} {}", base_cost_raw, c_unit),
        description: if let Some(ref rule) = db_rule {
            format!(
                "采用计费规则 [{}] 进行基础计费（Prompt: {}/M, Completion: {}/M, 缓存: {}/M）",
                rule.name, rule.prompt_rate, rule.completion_rate, rule.cached_rate
            )
        } else {
            "未关联计费规则，触发系统统一默认兜底公式（1M Tokens = 1.0 货币单位）".into()
        },
        value: Some(base_cost_raw),
        status: if db_rule.is_some() {
            "normal".into()
        } else {
            "warn".into()
        },
    });

    // Step 4: 多维折扣策略判定
    let sim_mask = crate::relay::proxy::normalize_discount_mask(resolved_discount_type);
    let level_disc_str = if (sim_mask & crate::relay::proxy::DISC_LEVEL) != 0 {
        format!("{:.2}x", resolved_level_discount)
    } else {
        format!("{:.2}x(未勾选)", resolved_level_discount)
    };
    let umd_str = match user_mid_discount {
        Some(d) if (sim_mask & crate::relay::proxy::DISC_USER_MODEL) != 0 => format!("{:.2}x", d),
        Some(d) => format!("{:.2}x(未勾选)", d),
        None => "无".into(),
    };
    let global_str = match active_model
        .as_ref()
        .filter(|m| m.global_discount_enabled == 1)
    {
        Some(m) if (sim_mask & crate::relay::proxy::DISC_GLOBAL) != 0 => {
            format!("{:.2}x", m.global_discount)
        }
        Some(m) => format!("{:.2}x(未勾选)", m.global_discount),
        None => "未启用".into(),
    };
    let ch_rate_str = if (sim_mask & crate::relay::proxy::DISC_CHANNEL) != 0 {
        format!("{:.2}x", channel.rate)
    } else {
        format!("{:.2}x(未勾选)", channel.rate)
    };
    steps.push(PipelineTraceStep {
        step: 4,
        stage: "discount".into(),
        title: "多维折扣比对判定".into(),
        formula: format!(
            "最低匹配折扣 = {:.2}x（来源: {}）",
            src_discount, discount_source_name
        ),
        description: format!(
            "用户上下文: {}；从已勾选折扣项中匹配最低值 MIN(等级折扣: {}, 专享折扣: {}, 全站折扣: {}, 渠道倍率: {}) = {:.2}x",
            user_identity_desc,
            level_disc_str,
            umd_str,
            global_str,
            ch_rate_str,
            src_discount
        ),
        value: Some(src_discount),
        status: "normal".into(),
    });

    // Step 5: 保底限价拦截
    steps.push(PipelineTraceStep {
        step: 5,
        stage: "channel_rate_and_floor".into(),
        title: "折扣保底限价校验".into(),
        formula: if is_floor_hit {
            format!(
                "触碰保底: 匹配折扣 {:.2}x ({}) < 保底 {:.2}x ➞ 强制保底 {:.2}x",
                src_discount, discount_source_name, billing_discount, billing_discount
            )
        } else {
            format!(
                "最终有效倍率 = {:.2}x（{}）",
                billing_discount, discount_source_name
            )
        },
        description: if is_floor_hit {
            format!(
                "⚠️ 触及模型设置的保底限价（{:.2}x），系统已自动触发安全拦截兜底，保护平台成本不倒贴",
                billing_discount
            )
        } else {
            "折扣倍率未低于模型保底限价，按匹配到的最低折扣倍率结算".into()
        },
        value: Some(billing_discount),
        status: if is_floor_hit { "warn".into() } else { "pass".into() },
    });

    // Step 6: 峰谷时段倍率折算
    steps.push(PipelineTraceStep {
        step: 6,
        stage: "time_multiplier".into(),
        title: "峰谷时段倍率折算".into(),
        formula: format!(
            "时段倍率 = {:.2}x（模拟时钟: {:02}:{:02}:{:02}）",
            time_multiplier,
            sim_local_time.hour(),
            sim_local_time.minute(),
            sim_local_time.second()
        ),
        description: if (time_multiplier - 1.0).abs() > 0.00001 {
            format!(
                "当前模拟时间匹配到计费规则的时段波段系数，金额乘以 {:.2} 倍",
                time_multiplier
            )
        } else {
            "当前时段处于基准常价格区间或未启用时段倍率，无额外时段乘数".into()
        },
        value: Some(time_multiplier),
        status: "normal".into(),
    });

    // Step 7: 最终结算与退补对账
    let settlement_prefix = if is_by_second {
        "[按秒预扣结算] "
    } else {
        "[预扣结算] "
    };
    steps.push(PipelineTraceStep {
        step: 7,
        stage: "settlement".into(),
        title: if is_by_second {
            "最终结算与差额退补 (按秒结算)".into()
        } else {
            "最终结算与差额退补".into()
        },
        formula: format!(
            "{}实扣金额 = {:.6} {}（预扣差额: {}{:.6} {}）",
            settlement_prefix,
            actual_cost,
            c_unit,
            if refund_amount > 0.0 { "+" } else { "-" },
            if refund_amount > 0.0 {
                refund_amount
            } else {
                additional_deduct
            },
            c_unit
        ),
        description: if refund_amount > 0.0 {
            format!(
                "{}预扣 {:.6} {} 大于实际结算消费，将向用户原路退回余额 {:.6} {}",
                settlement_prefix, pre_deducted, c_unit, refund_amount, c_unit
            )
        } else if additional_deduct > 0.0 {
            format!(
                "{}实际结算金额超出预扣，将从用户余额中追加补扣 {:.6} {}",
                settlement_prefix, additional_deduct, c_unit
            )
        } else {
            format!(
                "{}预扣金额与实际消费完全一致，无需额外退补",
                settlement_prefix
            )
        },
        value: Some(actual_cost),
        status: "normal".into(),
    });

    // Step 8: 上游成本与毛利核算
    steps.push(PipelineTraceStep {
        step: 8,
        stage: "cost_and_margin".into(),
        title: "上游采购成本与毛利核算".into(),
        formula: format!(
            "平台实收 {:.6} - 上游成本 {:.6} = 毛利润 {}{:.6} {} (毛利率 {:.2}%)",
            actual_cost,
            upstream_cost,
            if gross_profit >= 0.0 { "+" } else { "" },
            gross_profit,
            c_unit,
            margin_percent
        ),
        description: if is_loss_making {
            format!(
                "⚠️ 出现负毛利倒贴！采购成本超出终端收取 {:.6} {}，建议调整渠道倍率至 ≥ {:.2}x",
                (upstream_cost - actual_cost).abs(),
                c_unit,
                if base_cost_raw > 0.0 {
                    upstream_cost / base_cost_raw
                } else {
                    1.0
                }
            )
        } else {
            format!(
                "平台盈利健康，当前单次调用毛利为 +{:.6} {}，利润率处于合理安全空间",
                gross_profit, c_unit
            )
        },
        value: Some(gross_profit),
        status: if is_loss_making {
            "danger".into()
        } else {
            "pass".into()
        },
    });

    // 16. 构建响应体
    let rates_json = if let Some(ref r) = db_rule {
        serde_json::json!({
            "prompt_rate": r.prompt_rate,
            "completion_rate": r.completion_rate,
            "cached_rate": r.cached_rate,
            "fixed_rate": r.fixed_rate,
            "duration_rate": r.duration_rate,
        })
    } else {
        serde_json::json!({
            "default_rate": 1.0,
            "per_million": 1.0
        })
    };

    Ok(SimulateBillingResponse {
        success: true,
        channel_info: ChannelSimInfo {
            channel_id: channel.id,
            channel_name: channel.name,
            channel_rate: channel.rate,
            provider_type: channel.provider_type,
            group_aid: channel.group_aid,
            status: channel.status,
            is_disabled,
            quota_warning: channel_quota_warn,
        },
        available_sub_channels,
        routing: RoutingSimInfo {
            requested_model,
            resolved_model,
            model_mapping_hit: mapping_hit,
            mapping_source,
        },
        billing_rule: BillingRuleSimInfo {
            rule_id: db_rule.as_ref().map(|r| r.id),
            rule_name: db_rule
                .as_ref()
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "系统默认兜底规则".into()),
            pricing_type: db_rule
                .as_ref()
                .map(|r| r.pricing_type.clone())
                .unwrap_or_else(|| "token".into()),
            billing_type: db_rule
                .as_ref()
                .map(|r| r.billing_type.clone())
                .unwrap_or_else(|| "tokens".into()),
            is_default_fallback: db_rule.is_none(),
            rates: rates_json,
        },
        pipeline_trace: steps,
        settlement: SettlementSummary {
            pre_deducted,
            actual_cost,
            refund_amount,
            additional_deduct,
            billing_detail_text: detail_str,
        },
        cost_and_margin: CostAndMarginAnalysis {
            upstream_name: resolved_upstream_name,
            upstream_rate: resolved_upstream_rate,
            upstream_cost,
            gross_profit,
            margin_percent,
            is_loss_making,
            warning_message: margin_warning_message,
        },
        warning: general_warning,
    })
}
