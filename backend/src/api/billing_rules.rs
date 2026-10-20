/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use axum::{
    extract::{Path, Query, State},
    Json,
};
use sqlx::Row;
use std::sync::Arc;

use crate::error::AppResult;
use crate::models::{BillingRule, CreateBillingRuleRequest, UpdateBillingRuleRequest};
use crate::AppState;

#[derive(serde::Deserialize, Default)]
pub struct BillingRuleListQuery {
    pub include_pid: Option<String>,
}

pub async fn list_rules(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BillingRuleListQuery>,
) -> AppResult<Json<Vec<BillingRule>>> {
    let plugin_on = crate::api::plugins::is_plugin_compiled("volcengine_enhance")
        && crate::api::plugins::is_plugin_enabled(&state, "volcengine_enhance").await;

    let rules: Vec<BillingRule> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM billing_rules ORDER BY sort_order DESC, id DESC"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    let include_key = query.include_pid.as_deref();
    let rules = rules
        .into_iter()
        .filter(|rule| {
            crate::db::migrations::helpers::volc_mediakit_image::billing_rule_listed(
                &rule.name,
                &rule.pid,
                &rule.billing_rule,
                rule.id,
                plugin_on,
                include_key,
            )
        })
        .collect();
    Ok(Json(rules))
}

pub async fn create_rule(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateBillingRuleRequest>,
) -> AppResult<Json<BillingRule>> {
    req.name = req.name.trim().to_string();

    if req.name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "规则名称不能为空".to_string(),
        ));
    }

    let pricing_tiers_str = serde_json::to_string(&req.pricing_tiers.unwrap_or_default())
        .unwrap_or_else(|_| "[]".to_string());
    let extended_config_str = serde_json::to_string(&req.extended_config.unwrap_or_default())
        .unwrap_or_else(|_| "{}".to_string());
    if let Err(msg) = crate::models::validate_time_multiplier_config(&extended_config_str) {
        return Err(crate::error::AppError::BadRequest(msg));
    }

    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM billing_rules WHERE name = ?"),
    )
    .bind(&req.name)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "该费用规则名称已存在".to_string(),
        ));
    }

    let mut pid_val = req.pid.as_ref().map(|s| s.trim()).unwrap_or("").to_string();
    if pid_val.is_empty() {
        use rand::Rng;
        loop {
            let temp_pid = format!("6{:04}", rand::thread_rng().gen_range(0..10000));
            let exists: Option<i64> = sqlx::query_scalar(
                &state
                    .db
                    .format_query("SELECT id FROM billing_rules WHERE pid = ?"),
            )
            .bind(&temp_pid)
            .fetch_optional(&state.db.pool)
            .await?;
            if exists.is_none() {
                pid_val = temp_pid;
                break;
            }
        }
    } else {
        if pid_val.len() != 5 || !pid_val.chars().all(|c| c.is_ascii_digit()) {
            return Err(crate::error::AppError::BadRequest(
                "计费规则 PID 必须是 5 位数字字符串".to_string(),
            ));
        }
        if !pid_val.starts_with('6') {
            return Err(crate::error::AppError::BadRequest(
                "手动添加的计费规则 PID 必须是以 6 开头".to_string(),
            ));
        }
        let exists: Option<i64> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT id FROM billing_rules WHERE pid = ?"),
        )
        .bind(&pid_val)
        .fetch_optional(&state.db.pool)
        .await?;
        if exists.is_some() {
            return Err(crate::error::AppError::Conflict(
                "计费规则 PID 已存在".to_string(),
            ));
        }
    }

    let id_i64 = sqlx::query(
        &state.db.format_query(r#"INSERT INTO billing_rules 
            (name, billing_type, prompt_rate, completion_rate, cached_rate, claude_cache_creation_rate, claude_cache_read_rate, fixed_rate, duration_rate, billing_rule, pricing_tiers, extended_config, is_active, pid, provider_id, type_id, pricing_type, sort_order) 
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id"#)
    )
    .bind(&req.name)
    .bind(&req.billing_type)
    .bind(req.prompt_rate)
    .bind(req.completion_rate)
    .bind(req.cached_rate)
    .bind(req.claude_cache_creation_rate)
    .bind(req.claude_cache_read_rate)
    .bind(req.fixed_rate)
    .bind(req.duration_rate)
    .bind(&req.billing_rule)
    .bind(&pricing_tiers_str)
    .bind(&extended_config_str)
    .bind(req.is_active)
    .bind(&pid_val)
    .bind(req.provider_id)
    .bind(req.type_id)
    .bind(&req.pricing_type)
    .bind(req.sort_order.unwrap_or(0))
    .fetch_one(&state.db.pool)
    .await?
    .get::<i64, _>("id");

    let rule = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM billing_rules WHERE id = ?"),
    )
    .bind(id_i64)
    .fetch_one(&state.db.pool)
    .await?;

    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(rule))
}

pub async fn update_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(mut req): Json<UpdateBillingRuleRequest>,
) -> AppResult<Json<BillingRule>> {
    if let Some(name) = &mut req.name {
        *name = name.trim().to_string();
        if name.is_empty() {
            return Err(crate::error::AppError::BadRequest(
                "规则名称不能为空".to_string(),
            ));
        }
        let exists: Option<i64> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT id FROM billing_rules WHERE name = ? AND id != ?"),
        )
        .bind(&*name)
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await?;
        if exists.is_some() {
            return Err(crate::error::AppError::Conflict(
                "规则名称已经被占用".to_string(),
            ));
        }
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET name = ? WHERE id = ?"),
        )
        .bind(&*name)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }

    if let Some(val) = &req.billing_type {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET billing_type = ? WHERE id = ?"),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = req.prompt_rate {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET prompt_rate = ? WHERE id = ?"),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = req.completion_rate {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET completion_rate = ? WHERE id = ?"),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = req.cached_rate {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET cached_rate = ? WHERE id = ?"),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = req.claude_cache_creation_rate {
        sqlx::query(
            &state.db.format_query(
                "UPDATE billing_rules SET claude_cache_creation_rate = ? WHERE id = ?",
            ),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = req.claude_cache_read_rate {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET claude_cache_read_rate = ? WHERE id = ?"),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = req.fixed_rate {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET fixed_rate = ? WHERE id = ?"),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = req.duration_rate {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET duration_rate = ? WHERE id = ?"),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = &req.billing_rule {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET billing_rule = ? WHERE id = ?"),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = &req.pricing_tiers {
        let str_val = serde_json::to_string(val).unwrap_or_else(|_| "[]".to_string());
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET pricing_tiers = ? WHERE id = ?"),
        )
        .bind(&str_val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = &req.extended_config {
        let str_val = serde_json::to_string(val).unwrap_or_else(|_| "{}".to_string());
        if let Err(msg) = crate::models::validate_time_multiplier_config(&str_val) {
            return Err(crate::error::AppError::BadRequest(msg));
        }
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET extended_config = ? WHERE id = ?"),
        )
        .bind(&str_val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(active) = req.is_active {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET is_active = ? WHERE id = ?"),
        )
        .bind(active)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(pid) = &req.pid {
        let trimmed_pid = pid.trim().to_string();
        if !trimmed_pid.is_empty() {
            if trimmed_pid.len() != 5 || !trimmed_pid.chars().all(|c| c.is_ascii_digit()) {
                return Err(crate::error::AppError::BadRequest(
                    "计费规则 PID 必须是 5 位数字字符串".to_string(),
                ));
            }
            let is_system: i64 = sqlx::query_scalar(
                &state
                    .db
                    .format_query("SELECT is_system FROM billing_rules WHERE id = ?"),
            )
            .bind(id)
            .fetch_one(&state.db.pool)
            .await?;
            if is_system == 1 {
                if !trimmed_pid.starts_with('7') {
                    return Err(crate::error::AppError::BadRequest(
                        "系统计费规则 PID 必须是以 7 开头".to_string(),
                    ));
                }
            } else {
                if !trimmed_pid.starts_with('6') {
                    return Err(crate::error::AppError::BadRequest(
                        "手动添加的计费规则 PID 必须是以 6 开头".to_string(),
                    ));
                }
            }
            let exists: Option<i64> = sqlx::query_scalar(
                &state
                    .db
                    .format_query("SELECT id FROM billing_rules WHERE pid = ? AND id != ?"),
            )
            .bind(&trimmed_pid)
            .bind(id)
            .fetch_optional(&state.db.pool)
            .await?;
            if exists.is_some() {
                return Err(crate::error::AppError::Conflict(
                    "计费规则 PID 已存在".to_string(),
                ));
            }
            sqlx::query(
                &state
                    .db
                    .format_query("UPDATE billing_rules SET pid = ? WHERE id = ?"),
            )
            .bind(trimmed_pid)
            .bind(id)
            .execute(&state.db.pool)
            .await?;
        }
    }
    if let Some(provider_id) = req.provider_id {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET provider_id = ? WHERE id = ?"),
        )
        .bind(provider_id)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(type_id) = req.type_id {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET type_id = ? WHERE id = ?"),
        )
        .bind(type_id)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(pricing_type) = &req.pricing_type {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET pricing_type = ? WHERE id = ?"),
        )
        .bind(pricing_type)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(val) = req.sort_order {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE billing_rules SET sort_order = ? WHERE id = ?"),
        )
        .bind(val)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }

    sqlx::query(
        &state
            .db
            .format_query("UPDATE billing_rules SET updated_at = CURRENT_TIMESTAMP WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    let rule = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM billing_rules WHERE id = ?"),
    )
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(rule))
}

pub async fn delete_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let rule: Option<BillingRule> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM billing_rules WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    if let Some(r) = rule {
        if r.is_system == 1 {
            return Err(crate::error::AppError::Forbidden(
                "系统内置计费规则不允许删除".to_string(),
            ));
        }
    }

    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM billing_rules WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    sqlx::query(
        &state
            .db
            .format_query("UPDATE models SET billing_rule_id = NULL WHERE billing_rule_id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(serde_json::json!({ "success": true })))
}

pub struct BillingRuleDefault {
    pub billing_type: &'static str,
    pub prompt_rate: f64,
    pub completion_rate: f64,
    pub cached_rate: f64,
    pub claude_cache_creation_rate: f64,
    pub claude_cache_read_rate: f64,
    pub fixed_rate: f64,
    pub duration_rate: f64,
    pub billing_rule: &'static str,
    pub pricing_tiers: &'static str,
    pub extended_config: &'static str,
}

fn get_default_by_name(name: &str) -> Option<BillingRuleDefault> {
    match name {
        "标准1M万字计费 (1)" => Some(BillingRuleDefault {
            billing_type: "tokens",
            prompt_rate: 1.0,
            completion_rate: 2.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.0,
            billing_rule: "standard",
            pricing_tiers: "[]",
            extended_config: "{}",
        }),
        "单次请求扣费 (0.1)" => Some(BillingRuleDefault {
            billing_type: "requests",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.1,
            duration_rate: 0.0,
            billing_rule: "standard",
            pricing_tiers: "[]",
            extended_config: "{}",
        }),
        "Seedance2.0官方计费" => Some(BillingRuleDefault {
            billing_type: "tokens",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.0,
            billing_rule: "seedance2.0",
            pricing_tiers: "[]",
            extended_config: r#"{"resolution_rates":{"1080p":{"with_video":31,"without_video":51},"480p":{"with_video":28,"without_video":46},"4k":{"with_video":16,"without_video":26},"720p":{"with_video":28,"without_video":46}}}"#,
        }),
        "Seedance2.0Fast官方计费" => Some(BillingRuleDefault {
            billing_type: "tokens",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.0,
            billing_rule: "seedance2.0",
            pricing_tiers: "[]",
            extended_config: r#"{"resolution_rates":{"480p":{"with_video":22,"without_video":37},"720p":{"with_video":22,"without_video":37}}}"#,
        }),
        "Seedance2.0mini官方计费" => Some(BillingRuleDefault {
            billing_type: "tokens",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.0,
            billing_rule: "seedance2.0",
            pricing_tiers: "[]",
            extended_config: r#"{"resolution_rates":{"480p":{"with_video":14,"without_video":23},"720p":{"with_video":14,"without_video":23}}}"#,
        }),
        "Seedance2.5官方计费" => Some(BillingRuleDefault {
            billing_type: "tokens",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.0,
            billing_rule: "seedance2.0",
            pricing_tiers: "[]",
            extended_config: r#"{"enable_time_multipliers":false,"resolution_rates":{"480p":{"with_video":42,"without_video":70},"720p":{"with_video":42,"without_video":70},"1080p":{"with_video":46,"without_video":77}},"time_multipliers":[]}"#,
        }),
        "语音合成按字符计费 (2.8元/万字符)" => Some(BillingRuleDefault {
            billing_type: "requests",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 2.8,
            duration_rate: 0.0,
            billing_rule: "characters",
            pricing_tiers: "[]",
            extended_config: "{}",
        }),
        "文本向量标准计费" => Some(BillingRuleDefault {
            billing_type: "tokens",
            prompt_rate: 0.7,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.0,
            billing_rule: "standard",
            pricing_tiers: "[]",
            extended_config: "{}",
        }),
        "排序模型多模态计费" => Some(BillingRuleDefault {
            billing_type: "tokens",
            prompt_rate: 0.35,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.0,
            billing_rule: "multimodal",
            pricing_tiers: "[]",
            extended_config: r#"{"image_prompt_rate": 0.35}"#,
        }),
        "火山 MediaKit 官方视频超分计费" => Some(BillingRuleDefault {
            billing_type: "duration",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.20,
            billing_rule: "video_quality",
            pricing_tiers: r#"[{"resolution":"720p","fps_range":"<=30","rate":0.10,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.15,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.20,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.30,"enabled":true},{"resolution":"4k","fps_range":"<=30","rate":0.50,"enabled":true},{"resolution":"4k","fps_range":">30","rate":0.80,"enabled":true}]"#,
            extended_config: "{}",
        }),
        "火山 MediaKit 视频画质增强 (标准版)" => Some(BillingRuleDefault {
            billing_type: "duration",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.0125,
            billing_rule: "video_quality",
            pricing_tiers: r#"[{"resolution":"720p","fps_range":"<=30","rate":0.0125,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.025,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.025,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.05,"enabled":true},{"resolution":"2k","fps_range":"<=30","rate":0.05,"enabled":true},{"resolution":"2k","fps_range":">30","rate":0.10,"enabled":true},{"resolution":"4k","fps_range":"<=30","rate":0.10,"enabled":true},{"resolution":"4k","fps_range":">30","rate":0.20,"enabled":true}]"#,
            extended_config: "{}",
        }),
        "火山 MediaKit 视频画质增强 (专业版)" => Some(BillingRuleDefault {
            billing_type: "duration",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.125,
            billing_rule: "video_quality",
            pricing_tiers: r#"[{"resolution":"720p","fps_range":"<=30","rate":0.125,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.25,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.25,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.50,"enabled":true},{"resolution":"2k","fps_range":"<=30","rate":0.50,"enabled":true},{"resolution":"2k","fps_range":">30","rate":1.00,"enabled":true},{"resolution":"4k","fps_range":"<=30","rate":1.00,"enabled":true},{"resolution":"4k","fps_range":">30","rate":2.00,"enabled":true}]"#,
            extended_config: "{}",
        }),
        "火山 MediaKit 视频画质增强 (极速版)" => Some(BillingRuleDefault {
            billing_type: "duration",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.00333333,
            billing_rule: "video_quality",
            pricing_tiers: r#"[{"resolution":"720p","fps_range":"<=30","rate":0.00333333,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.00666667,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.00666667,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.01333333,"enabled":true},{"resolution":"2k","fps_range":"<=30","rate":0.01333333,"enabled":true},{"resolution":"2k","fps_range":">30","rate":0.02666667,"enabled":true},{"resolution":"4k","fps_range":"<=30","rate":0.02666667,"enabled":true},{"resolution":"4k","fps_range":">30","rate":0.05333333,"enabled":true}]"#,
            extended_config: "{}",
        }),
        "火山 MediaKit 视频画质增强 (大模型版)" => Some(BillingRuleDefault {
            billing_type: "duration",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.04166667,
            billing_rule: "video_quality",
            pricing_tiers: r#"[{"resolution":"720p","fps_range":"<=30","rate":0.04166667,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.08333333,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.08333333,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.16666667,"enabled":true}]"#,
            extended_config: "{}",
        }),
        "火山 MediaKit 视频字幕擦除 (标准版)" => Some(BillingRuleDefault {
            billing_type: "duration",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.00666667,
            billing_rule: "standard",
            pricing_tiers: "[]",
            extended_config: "{}",
        }),
        "火山 MediaKit 图像画质增强" | "火山 MediaKit 图像背景移除" => {
            Some(BillingRuleDefault {
                billing_type: "requests",
                prompt_rate: 0.0,
                completion_rate: 0.0,
                cached_rate: 0.0,
                claude_cache_creation_rate: 0.0,
                claude_cache_read_rate: 0.0,
                fixed_rate: 0.006,
                duration_rate: 0.0,
                billing_rule: "standard",
                pricing_tiers: "[]",
                extended_config: "{}",
            })
        }
        "火山 MediaKit 视频字幕擦除 (精细版)" => Some(BillingRuleDefault {
            billing_type: "duration",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.01666667,
            billing_rule: "standard",
            pricing_tiers: "[]",
            extended_config: "{}",
        }),
        "火山级联封装默认计费" => Some(BillingRuleDefault {
            billing_type: "duration",
            prompt_rate: 0.0,
            completion_rate: 0.0,
            cached_rate: 0.0,
            claude_cache_creation_rate: 0.0,
            claude_cache_read_rate: 0.0,
            fixed_rate: 0.0,
            duration_rate: 0.0,
            billing_rule: "volc_enhance_cascade",
            pricing_tiers: "[]",
            extended_config: r#"{"price_table": {"fast|720p|no": 0.80, "fast|1080p|no": 1.80, "fast|2k|no": 3.20, "fast|4k|no": 7.20, "standard|720p|no": 1.00, "standard|1080p|no": 2.24, "standard|2k|no": 4.00, "standard|4k|no": 8.94, "pro|720p|no": 1.20, "pro|1080p|no": 2.70, "pro|2k|no": 4.80, "pro|4k|no": 10.70, "ai|720p|no": 1.40, "ai|1080p|no": 3.16, "fast|720p|yes": 0.84, "fast|1080p|yes": 1.88, "fast|2k|yes": 3.40, "fast|4k|yes": 7.40, "standard|720p|yes": 1.06, "standard|1080p|yes": 2.36, "standard|2k|yes": 4.30, "standard|4k|yes": 9.24, "pro|720p|yes": 1.28, "pro|1080p|yes": 2.86, "pro|2k|yes": 5.20, "pro|4k|yes": 11.10, "ai|720p|yes": 1.60, "ai|1080p|yes": 3.51}}"#,
        }),
        _ => None,
    }
}

/// 插件重新开启时，把八条预置计费规则的单价和启用状态写回系统默认。
pub async fn restore_volc_preset_billing(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    for (_, name, _) in crate::db::migrations::helpers::volc_mediakit_image::PRESET_BILLING {
        let Some(default_val) = get_default_by_name(name) else {
            continue;
        };
        sqlx::query(
            r#"UPDATE billing_rules SET
                billing_type = $1,
                prompt_rate = $2,
                completion_rate = $3,
                cached_rate = $4,
                claude_cache_creation_rate = $5,
                claude_cache_read_rate = $6,
                fixed_rate = $7,
                duration_rate = $8,
                billing_rule = $9,
                pricing_tiers = $10,
                extended_config = $11,
                is_active = 1,
                updated_at = CURRENT_TIMESTAMP
               WHERE name = $12"#,
        )
        .bind(default_val.billing_type)
        .bind(default_val.prompt_rate)
        .bind(default_val.completion_rate)
        .bind(default_val.cached_rate)
        .bind(default_val.claude_cache_creation_rate)
        .bind(default_val.claude_cache_read_rate)
        .bind(default_val.fixed_rate)
        .bind(default_val.duration_rate)
        .bind(default_val.billing_rule)
        .bind(default_val.pricing_tiers)
        .bind(default_val.extended_config)
        .bind(*name)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn restore_default_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<BillingRule>> {
    let current: Option<(String, i64)> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT name, is_system FROM billing_rules WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    let (name, is_system) = match current {
        Some(c) => c,
        None => {
            return Err(crate::error::AppError::NotFound(
                "计费规则不存在".to_string(),
            ))
        }
    };

    if is_system != 1 {
        return Err(crate::error::AppError::BadRequest(
            "仅系统计费规则支持恢复默认".to_string(),
        ));
    }

    let default_val = match get_default_by_name(&name) {
        Some(d) => d,
        None => {
            return Err(crate::error::AppError::BadRequest(
                "未找到该系统计费规则的默认配置数据".to_string(),
            ))
        }
    };

    sqlx::query(&state.db.format_query(
        "UPDATE billing_rules SET \
         billing_type = ?, \
         prompt_rate = ?, \
         completion_rate = ?, \
         cached_rate = ?, \
         claude_cache_creation_rate = ?, \
         claude_cache_read_rate = ?, \
         fixed_rate = ?, \
         duration_rate = ?, \
         billing_rule = ?, \
         pricing_tiers = ?, \
         extended_config = ?, \
         updated_at = CURRENT_TIMESTAMP \
         WHERE id = ?",
    ))
    .bind(default_val.billing_type)
    .bind(default_val.prompt_rate)
    .bind(default_val.completion_rate)
    .bind(default_val.cached_rate)
    .bind(default_val.claude_cache_creation_rate)
    .bind(default_val.claude_cache_read_rate)
    .bind(default_val.fixed_rate)
    .bind(default_val.duration_rate)
    .bind(default_val.billing_rule)
    .bind(default_val.pricing_tiers)
    .bind(default_val.extended_config)
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    let rule = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM billing_rules WHERE id = ?"),
    )
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(rule))
}

/// 当开启火山引擎 AI MediaKit 插件时，系统自动注入火山级联封装默认计费 (PID 78242) 及相关预设规则
pub async fn ensure_volcengine_enhance_system_rules(state: &AppState) -> AppResult<()> {
    let pool = &state.db.pool;
    let _ = crate::db::migrations::helpers::volc_mediakit_image::seed(pool).await;

    let volc_provider_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM model_providers WHERE name = '火山引擎' LIMIT 1")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

    let _volc_api_provider_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM model_api_providers WHERE name ILIKE '%火山%' OR name ILIKE '%volcengine%' LIMIT 1"
    ).fetch_optional(pool).await.unwrap_or(None);

    let enhance_type_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM model_types WHERE name = '封装' LIMIT 1")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

    let video_type_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM model_types WHERE name = '视频' LIMIT 1")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

    // 1. 注入/保障「火山级联封装默认计费」(PID 78242)
    let cascade_exists: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM billing_rules WHERE name = '火山级联封装默认计费' OR pid = '78242' LIMIT 1"
    ).fetch_optional(pool).await.unwrap_or(None);

    if let Some(id) = cascade_exists {
        let _ = sqlx::query(
            "UPDATE billing_rules SET name = '火山级联封装默认计费', pid = '78242', is_system = 1, provider_id = COALESCE(provider_id, $1), type_id = COALESCE(type_id, $2) WHERE id = $3"
        )
        .bind(volc_provider_id)
        .bind(video_type_id)
        .bind(id)
        .execute(pool).await;
    } else {
        let _ = sqlx::query(
            r#"INSERT INTO billing_rules (
                name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate,
                billing_rule, extended_config, is_system, pid, pricing_type, provider_id, type_id
            ) VALUES (
                '火山级联封装默认计费', 'duration', 0.0, 0.0, 0.0, 0.0,
                'volc_enhance_cascade',
                '{"price_table": {"fast|720p|no": 0.80, "fast|1080p|no": 1.80, "fast|2k|no": 3.20, "fast|4k|no": 7.20, "standard|720p|no": 1.00, "standard|1080p|no": 2.24, "standard|2k|no": 4.00, "standard|4k|no": 8.94, "pro|720p|no": 1.20, "pro|1080p|no": 2.70, "pro|2k|no": 4.80, "pro|4k|no": 10.70, "ai|720p|no": 1.40, "ai|1080p|no": 3.16, "fast|720p|yes": 0.84, "fast|1080p|yes": 1.88, "fast|2k|yes": 3.40, "fast|4k|yes": 7.40, "standard|720p|yes": 1.06, "standard|1080p|yes": 2.36, "standard|2k|yes": 4.30, "standard|4k|yes": 9.24, "pro|720p|yes": 1.28, "pro|1080p|yes": 2.86, "pro|2k|yes": 5.20, "pro|4k|yes": 11.10, "ai|720p|yes": 1.60, "ai|1080p|yes": 3.51}}',
                1, '78242', 'official', $1, $2
            )"#
        )
        .bind(volc_provider_id)
        .bind(video_type_id)
        .execute(pool).await;
    }

    // 2. 保障 MediaKit 视频画质增强及字幕擦除细分计费规则
    let mediakit_rules = [
        (
            "火山 MediaKit 官方视频超分计费",
            0.20f64,
            "video_quality",
            r#"[{"resolution":"720p","fps_range":"<=30","rate":0.10,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.15,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.20,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.30,"enabled":true},{"resolution":"4k","fps_range":"<=30","rate":0.50,"enabled":true},{"resolution":"4k","fps_range":">30","rate":0.80,"enabled":true}]"#,
        ),
        (
            "火山 MediaKit 视频画质增强 (标准版)",
            0.0125,
            "video_quality",
            r#"[{"resolution":"720p","fps_range":"<=30","rate":0.0125,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.025,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.025,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.05,"enabled":true},{"resolution":"2k","fps_range":"<=30","rate":0.05,"enabled":true},{"resolution":"2k","fps_range":">30","rate":0.10,"enabled":true},{"resolution":"4k","fps_range":"<=30","rate":0.10,"enabled":true},{"resolution":"4k","fps_range":">30","rate":0.20,"enabled":true}]"#,
        ),
        (
            "火山 MediaKit 视频画质增强 (专业版)",
            0.125,
            "video_quality",
            r#"[{"resolution":"720p","fps_range":"<=30","rate":0.125,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.25,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.25,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.50,"enabled":true},{"resolution":"2k","fps_range":"<=30","rate":0.50,"enabled":true},{"resolution":"2k","fps_range":">30","rate":1.00,"enabled":true},{"resolution":"4k","fps_range":"<=30","rate":1.00,"enabled":true},{"resolution":"4k","fps_range":">30","rate":2.00,"enabled":true}]"#,
        ),
        (
            "火山 MediaKit 视频画质增强 (极速版)",
            0.00333333,
            "video_quality",
            r#"[{"resolution":"720p","fps_range":"<=30","rate":0.00333333,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.00666667,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.00666667,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.01333333,"enabled":true},{"resolution":"2k","fps_range":"<=30","rate":0.01333333,"enabled":true},{"resolution":"2k","fps_range":">30","rate":0.02666667,"enabled":true},{"resolution":"4k","fps_range":"<=30","rate":0.02666667,"enabled":true},{"resolution":"4k","fps_range":">30","rate":0.05333333,"enabled":true}]"#,
        ),
        (
            "火山 MediaKit 视频画质增强 (大模型版)",
            0.04166667,
            "video_quality",
            r#"[{"resolution":"720p","fps_range":"<=30","rate":0.04166667,"enabled":true},{"resolution":"720p","fps_range":">30","rate":0.08333333,"enabled":true},{"resolution":"1080p","fps_range":"<=30","rate":0.08333333,"enabled":true},{"resolution":"1080p","fps_range":">30","rate":0.16666667,"enabled":true}]"#,
        ),
        (
            "火山 MediaKit 视频字幕擦除 (标准版)",
            0.00666667,
            "standard",
            "[]",
        ),
        (
            "火山 MediaKit 视频字幕擦除 (精细版)",
            0.01666667,
            "standard",
            "[]",
        ),
    ];

    for (name, dur_rate, brule, ptiers) in mediakit_rules {
        let _ = sqlx::query(
            "INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, pricing_tiers, extended_config, is_system, provider_id, type_id) \
             SELECT $1, 'duration', 0.0, 0.0, 0.0, $2, $3, $4, '{}', 1, $5, $6 \
             WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = $1)"
        )
        .bind(name)
        .bind(dur_rate)
        .bind(brule)
        .bind(ptiers)
        .bind(volc_provider_id)
        .bind(enhance_type_id)
        .execute(pool).await;
    }

    // 3. 保障级联视频与 Vidu 转发规则（MediaKit 转发规则统一由 volc_mediakit_image::seed 维护）
    let preset_forward_rules = [
        (
            "火山方舟 级联视频生成",
            "volcengine",
            "供视频生成级联封装调用的火山方舟专属转发规则",
            r#"{"target_type":"volcengine","is_cascade":true,"fast_response":true,"res_mul":{"480p":1.5,"720p":2.15,"1080p":2.25,"2k":2.5,"4k":4.0},"path_rewrite":{"old":"/v1/video/generations","new":"/api/v3/contents/generations/tasks"},"auth_type":"bearer"}"#,
            "视频"
        ),
        (
            "Vidu 官方旗舰视频生成",
            "vidu",
            "供 Vidu 官方 Q 系列视频模型（viduq4-preview / viduq3-pro 等）调用的专属转发规则，支持 OpenAI 与 Vidu 官方 2 种方式调用，支持文生视频、首帧图生视频、首尾帧生视频与多主体参考生视频自适应选路，支持音视频直出与异步任务轮询。",
            r#"{"target_type":"vidu","path_rewrite":{"old":"/v1/video/generations","new":"/ent/v2/img2video"},"auth_type":"token","poll_path":"/ent/v2/tasks/${task_id}/creations"}"#,
            "视频"
        )
    ];

    for (name, rtype, desc, config, cat) in preset_forward_rules {
        let eid = match state.db.generate_unique_forward_eid().await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let _ = sqlx::query(
            "INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid) \
             SELECT $1, $2, $3, $4, $5, 1, $6 \
             WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = $1)"
        )
        .bind(name).bind(rtype).bind(desc).bind(config).bind(cat).bind(&eid)
        .execute(pool).await;
    }

    let _ = crate::db::migrations::helpers::volc_mediakit_image::ensure_enhance_library_catalog(
        pool,
    )
    .await;
    let _ = crate::db::migrations::helpers::volc_mediakit_image::backfill_preset_billing_pids(pool)
        .await;

    Ok(())
}
