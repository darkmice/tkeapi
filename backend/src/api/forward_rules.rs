/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use axum::{
    extract::{Path, State},
    Json,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::api::plugins::{is_plugin_compiled, is_plugin_enabled};
use crate::error::{AppError, AppResult};
use crate::models::{CreateRuleRequest, ForwardRule, UpdateRuleRequest};
use crate::AppState;

/// 系统默认转发规则 → 依赖的站点插件（未编译或未启用则不展示）。
const PLUGIN_GATED_SYSTEM_RULES: &[(&str, &str)] = &[
    ("火山方舟 级联视频生成", "volcengine_enhance"),
    ("火山方舟 视频素材转换", "asset_manager"),
    ("火山方舟 视频素材转换(国际版)", "asset_manager_intl"),
    ("火山方舟 视频素材免审核转换(国际版)", "asset_manager_intl"),
    ("ComfyUI", "comfyui_bridge"),
];

const PLUGIN_GATED_PLUGIN_NAMES: &[&str] = &[
    "volcengine_enhance",
    "tencent_enhance",
    "asset_manager",
    "asset_manager_intl",
    "comfyui_bridge",
];

fn config_flag_on(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_i64() == Some(1),
        serde_json::Value::String(s) => s == "1" || s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// 级联规则依赖阶段二插件：`cascade_engine=tencent` 走腾讯云增强，其余走火山 MediaKit。
fn cascade_required_plugin(cfg: &serde_json::Value) -> Option<&'static str> {
    let on = cfg.get("is_cascade").is_some_and(config_flag_on);
    if !on {
        return None;
    }
    let engine = cfg
        .get("cascade_engine")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if engine.eq_ignore_ascii_case("tencent") {
        Some("tencent_enhance")
    } else {
        Some("volcengine_enhance")
    }
}

fn is_enhance_category(category: &str) -> bool {
    category == "封装"
}

/// 解析规则依赖的插件名；无依赖返回 `None`。
pub(crate) fn required_plugin_for_forward_rule(
    name: &str,
    category: &str,
    config_json: &str,
) -> Option<&'static str> {
    for (rule_name, plugin) in PLUGIN_GATED_SYSTEM_RULES {
        if name == *rule_name {
            return Some(*plugin);
        }
    }
    if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(config_json) {
        if let Some(plugin) = cascade_required_plugin(&cfg) {
            return Some(plugin);
        }
        if cfg.get("asset_convert_ns").and_then(|v| v.as_str()) == Some("asset_manager_intl") {
            return Some("asset_manager_intl");
        }
        if cfg.get("target_type").and_then(|v| v.as_str()) == Some("volcengine_media_enhance") {
            return Some("volcengine_enhance");
        }
    }
    if is_enhance_category(category) || name.contains("MediaKit") {
        return Some("volcengine_enhance");
    }
    None
}

/// `plugin_available(name)`：插件已编译且已启用时返回 true。
pub(crate) fn should_hide_plugin_gated_rule(
    name: &str,
    category: &str,
    config_json: &str,
    plugin_available: &HashMap<&str, bool>,
) -> bool {
    match required_plugin_for_forward_rule(name, category, config_json) {
        Some(plugin) => !plugin_available.get(plugin).copied().unwrap_or(false),
        None => false,
    }
}

async fn load_plugin_availability(state: &AppState) -> HashMap<&'static str, bool> {
    let mut map = HashMap::new();
    for &name in PLUGIN_GATED_PLUGIN_NAMES {
        let ok = is_plugin_compiled(name) && is_plugin_enabled(state, name).await;
        map.insert(name, ok);
    }
    map
}

pub async fn list_rules(State(state): State<Arc<AppState>>) -> AppResult<Json<Vec<ForwardRule>>> {
    let mut rules: Vec<ForwardRule> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM forward_rules ORDER BY sort_order DESC, id DESC"),
    )
    .fetch_all(&state.db.pool)
    .await?;
    let availability = load_plugin_availability(&state).await;
    rules.retain(|r| {
        !should_hide_plugin_gated_rule(&r.name, &r.category, &r.config_json, &availability)
    });
    Ok(Json(rules))
}

pub async fn create_rule(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateRuleRequest>,
) -> AppResult<Json<ForwardRule>> {
    req.name = req.name.trim().to_string();
    req.rule_type = req.rule_type.trim().to_string();

    if req.name.is_empty() || req.rule_type.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "规则名和类型不能为空".to_string(),
        ));
    }

    let config_json = req.config_json.unwrap_or_else(|| "{}".to_string());

    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM forward_rules WHERE name = ?"),
    )
    .bind(&req.name)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "规则名称已存在".to_string(),
        ));
    }

    let category_val = req.category.unwrap_or_else(|| "聊天".to_string());

    let mut eid_val = req.eid.as_deref().unwrap_or("").trim().to_string();
    if eid_val.is_empty() {
        eid_val = state.db.generate_unique_forward_eid().await?;
    } else if state.db.forward_eid_taken(&eid_val, None).await? {
        return Err(AppError::Conflict("转发规则 EID 已存在".to_string()));
    }

    let sort_order_val = req.sort_order.unwrap_or(0);

    let rule = sqlx::query_as(
        &state.db.format_query("INSERT INTO forward_rules (name, rule_type, category, description, config_json, is_active, is_system, eid, sort_order) VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?) RETURNING *")
    )
    .bind(&req.name)
    .bind(&req.rule_type)
    .bind(&category_val)
    .bind(&req.description)
    .bind(&config_json)
    .bind(req.is_active)
    .bind(&eid_val)
    .bind(sort_order_val)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(rule))
}

pub async fn update_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(mut req): Json<UpdateRuleRequest>,
) -> AppResult<Json<ForwardRule>> {
    let existing: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_system FROM forward_rules WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    if existing.is_none() {
        return Err(crate::error::AppError::NotFound("规则不存在".to_string()));
    }

    // Basic trimming if fields are provided
    if let Some(name) = &mut req.name {
        *name = name.trim().to_string();
        if name.is_empty() {
            return Err(crate::error::AppError::BadRequest(
                "规则名称不能为空".to_string(),
            ));
        }
    }

    if let Some(name) = &req.name {
        let exists: Option<i64> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT id FROM forward_rules WHERE name = ? AND id != ?"),
        )
        .bind(name)
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
                .format_query("UPDATE forward_rules SET name = ? WHERE id = ?"),
        )
        .bind(name)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }

    if let Some(rtype) = &req.rule_type {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE forward_rules SET rule_type = ? WHERE id = ?"),
        )
        .bind(rtype)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(cat) = &req.category {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE forward_rules SET category = ? WHERE id = ?"),
        )
        .bind(cat)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(config) = &req.config_json {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE forward_rules SET config_json = ? WHERE id = ?"),
        )
        .bind(config)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(desc) = &req.description {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE forward_rules SET description = ? WHERE id = ?"),
        )
        .bind(desc)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(active) = req.is_active {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE forward_rules SET is_active = ? WHERE id = ?"),
        )
        .bind(active)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(sort_order) = req.sort_order {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE forward_rules SET sort_order = ? WHERE id = ?"),
        )
        .bind(sort_order)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(eid) = &req.eid {
        let eid = eid.trim();
        if !eid.is_empty() && state.db.forward_eid_taken(eid, Some(id)).await? {
            return Err(AppError::Conflict("转发规则 EID 已存在".to_string()));
        }
        sqlx::query(
            &state
                .db
                .format_query("UPDATE forward_rules SET eid = ? WHERE id = ?"),
        )
        .bind(eid)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }

    sqlx::query(
        &state
            .db
            .format_query("UPDATE forward_rules SET updated_at = CURRENT_TIMESTAMP WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    let rule = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM forward_rules WHERE id = ?"),
    )
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(rule))
}

pub async fn delete_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let existing: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_system FROM forward_rules WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    if let Some(sys) = existing {
        if sys == 1 {
            return Err(crate::error::AppError::Forbidden(
                "系统内置规则禁止删除".to_string(),
            ));
        }
    } else {
        return Err(crate::error::AppError::NotFound("规则不存在".to_string()));
    }

    // Check if the rule is being used by any models by checking JSON structure
    // Usually handled logically, here we let it vanish, models matching parsing will gracefully fall back
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM forward_rules WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn avail(volc: bool, tencent: bool) -> HashMap<&'static str, bool> {
        let mut map = HashMap::new();
        map.insert("volcengine_enhance", volc);
        map.insert("tencent_enhance", tencent);
        map.insert("asset_manager", true);
        map.insert("asset_manager_intl", true);
        map.insert("comfyui_bridge", true);
        map
    }

    #[test]
    fn hides_volc_cascade_when_mediakit_off() {
        let cfg = r#"{"is_cascade":true,"target_type":"volcengine"}"#;
        assert!(should_hide_plugin_gated_rule(
            "自定义级联",
            "视频",
            cfg,
            &avail(false, true)
        ));
        assert!(!should_hide_plugin_gated_rule(
            "自定义级联",
            "视频",
            cfg,
            &avail(true, false)
        ));
    }

    #[test]
    fn hides_tencent_cascade_when_plugin_off() {
        let cfg = r#"{"is_cascade":true,"cascade_engine":"tencent"}"#;
        assert!(should_hide_plugin_gated_rule(
            "腾讯级联",
            "视频",
            cfg,
            &avail(true, false)
        ));
        assert!(!should_hide_plugin_gated_rule(
            "腾讯级联",
            "视频",
            cfg,
            &avail(false, true)
        ));
    }

    #[test]
    fn keeps_plain_rule_when_enhance_plugins_off() {
        let cfg = r#"{"target_type":"openai"}"#;
        assert!(!should_hide_plugin_gated_rule(
            "普通转发",
            "聊天",
            cfg,
            &avail(false, false)
        ));
    }

    #[test]
    fn hides_enhance_category_and_mediakit_rules_when_plugin_off() {
        let cfg = r#"{"target_type":"volcengine_media_enhance"}"#;
        assert!(should_hide_plugin_gated_rule(
            "火山 MediaKit 视频画质增强",
            "封装",
            cfg,
            &avail(false, true),
        ));
        assert!(should_hide_plugin_gated_rule(
            "自定义",
            "封装",
            "{}",
            &avail(false, true)
        ));
        assert!(!should_hide_plugin_gated_rule(
            "自定义",
            "封装",
            "{}",
            &avail(true, false)
        ));
    }
}
