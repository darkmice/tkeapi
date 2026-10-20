/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! GET /v1/models & GET /api/v3/models
//! 返回系统模型列表，数据源为模型广场已启用的模型。
//! 响应格式兼容 OpenAI 标准（火山方舟 /api/v3/models 格式与 OpenAI 一致）。

use crate::api::plugins::{
    marketplace_viewer_for_user_id, mp_enabled, mp_visible_to, parse_mp_level_ids,
};
use crate::models::{ApiToken, Channel};
use crate::relay::router::get_cached_active_channels;
use crate::{error::AppResult, AppState};
use axum::{
    extract::{Extension, State},
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// GET /v1/models | /api/v3/models — 获取可用模型列表
pub async fn list_models(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
) -> AppResult<Json<serde_json::Value>> {
    // 1. 获取当前用户所属分组与等级
    let (user_group, level_id): (String, i64) = sqlx::query_as(
        &state.db.format_query(
            "SELECT u.user_group, COALESCE(ul.id, 0) FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?",
        ),
    )
    .bind(&token.user_id)
    .fetch_optional(&state.db.pool)
    .await?
    .unwrap_or_else(|| ("default".to_string(), 0));

    // 2. 获取站点活跃渠道（带内存缓存），并筛选当前用户可用的渠道
    let all_active_channels = get_cached_active_channels(&state.db).await?;
    let tz_name = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let (now_day, now_week, now_month) = crate::models::quota_period_keys(&tz_name);

    let ug_str = format!("\"{}\"", user_group);
    let level_str = format!("\"{}\"", level_id);

    let usable_channels: Vec<&Channel> = all_active_channels
        .iter()
        .filter(|c| {
            c.has_available_quota(&now_day, &now_week, &now_month)
                && c.matches_user_group(&ug_str, &level_str)
        })
        .collect();

    // 若当前用户没有任何可用渠道，则直接返回空列表
    if usable_channels.is_empty() {
        return Ok(Json(json!({
            "object": "list",
            "data": []
        })));
    }

    // 3. 统计可用渠道支持的模型范围（空模型渠道不作为通配渠道）
    let has_wildcard_channel = usable_channels.iter().any(|c| {
        let m = c.models.trim();
        m.contains("\"*\"")
    });

    let (supported_mids, supported_model_ids): (
        std::collections::HashSet<String>,
        std::collections::HashSet<String>,
    ) = if has_wildcard_channel {
        (
            std::collections::HashSet::new(),
            std::collections::HashSet::new(),
        )
    } else {
        let mut mids = std::collections::HashSet::new();
        let mut model_ids = std::collections::HashSet::new();
        for c in &usable_channels {
            for m in c.get_models() {
                mids.insert(m.clone());
                model_ids.insert(m);
            }
            for key in c.get_model_mapping().keys() {
                model_ids.insert(key.clone());
            }
        }
        (mids, model_ids)
    };

    // 4. 读取模型广场配置（若开启则兼容特定用户等级的可见性限制）
    let viewer = marketplace_viewer_for_user_id(&state, &token.user_id).await;
    let is_mp_enabled: bool = sqlx::query_scalar::<_, i64>(
        &state
            .db
            .format_query("SELECT is_enabled FROM plugins WHERE name = 'model_marketplace'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None)
        == Some(1);

    let configs = if is_mp_enabled {
        crate::api::plugins::load_plugin_configs_pub(&state, "model_marketplace")
            .await
            .unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };
    let display_mode = configs
        .get("mp_display_mode")
        .map(|s| s.as_str())
        .unwrap_or("blacklist");
    let is_blacklist = display_mode == "blacklist";

    // 5. 查询已激活且已上架的模型及其厂商信息
    let models: Vec<crate::models::Model> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM models WHERE is_active = 1 AND is_listed = 1 ORDER BY sort_order DESC, id ASC"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    let providers: Vec<crate::models::ModelProvider> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM model_providers WHERE is_active = 1"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    let mut data: Vec<serde_json::Value> = Vec::new();

    for m in &models {
        // A. 渠道运行支撑性校验：必须有通配渠道，或渠道明确配置支持该 mid / model_id
        let channel_supported = has_wildcard_channel
            || supported_mids.contains(&m.mid)
            || supported_model_ids.contains(&m.model_id);
        if !channel_supported {
            continue;
        }

        // B. API Token 模型白名单校验
        if !token.is_model_allowed(&m.model_id) {
            continue;
        }

        // C. 模型广场显式等级可见性校验（若单独配置）
        if is_mp_enabled {
            let config_key = format!("mp_model_id_{}", m.id);
            if let Some(conf_str) = configs.get(&config_key) {
                if let Ok(model_conf) = serde_json::from_str::<Value>(conf_str) {
                    let level_ids = parse_mp_level_ids(&model_conf);
                    let is_enabled = mp_enabled(&model_conf, is_blacklist);
                    if !mp_visible_to(is_enabled, &level_ids, viewer) {
                        continue;
                    }
                }
            }
        }

        // 解析 owned_by（取 provider 名称，兜底 "system"）
        let owned_by = m
            .provider_id
            .and_then(|pid| providers.iter().find(|p| p.id == pid))
            .map(|p| p.name_en.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                m.provider_id
                    .and_then(|pid| providers.iter().find(|p| p.id == pid))
                    .map(|p| p.name.as_str())
            })
            .unwrap_or("system");

        // 将 created_at 字符串解析为 Unix 时间戳（秒）
        let created = parse_timestamp(&m.created_at);

        data.push(json!({
            "id": m.model_id,
            "object": "model",
            "created": created,
            "owned_by": owned_by,
        }));
    }

    // 按 model_id 去重（同一 model_id 可能因不同定价方案存在多条记录）
    let mut seen = std::collections::HashSet::new();
    data.retain(|item| {
        let id = item["id"].as_str().unwrap_or("").to_string();
        seen.insert(id)
    });

    Ok(Json(json!({
        "object": "list",
        "data": data,
    })))
}

/// 将数据库时间字符串解析为 Unix 时间戳（秒）
/// 支持多种格式，解析失败时返回 0
fn parse_timestamp(s: &str) -> i64 {
    // 尝试 ISO 8601 / PostgreSQL 默认格式
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f") {
        return dt.and_utc().timestamp();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return dt.and_utc().timestamp();
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return dt.timestamp();
    }
    0
}
