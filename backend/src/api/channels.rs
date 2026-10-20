/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::error::{AppError, AppResult};
use crate::models::{
    Channel, ChannelListResponse, ChannelSafe, CreateChannelRequest, UpdateChannelRequest,
};
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

async fn ensure_ha_channel_rule(
    state: &AppState,
    provider_type: &str,
    config: &serde_json::Value,
) -> AppResult<()> {
    if provider_type != "high_availability_group" {
        return Ok(());
    }
    let Some(id) = config
        .get("rule")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    else {
        return Ok(());
    };
    let bundle = crate::relay::relay_settings::get_cached_ha_rules(&state.db).await;
    if bundle.has_id(id) {
        Ok(())
    } else {
        Err(AppError::BadRequest("所选高可用规则不存在".into()))
    }
}

fn validate_channel_upstream(
    provider_type: &str,
    preset_id: Option<i64>,
    config: &serde_json::Value,
) -> AppResult<()> {
    let has_preset = preset_id.is_some_and(|id| id > 0);
    let valid = match provider_type {
        "high_availability_group" => config
            .get("sub_channels")
            .and_then(|v| v.as_array())
            .map_or(false, |a| !a.is_empty()),
        "volcengine" => {
            config
                .get("volcengine_enhance_credential_id")
                .is_some_and(|v| {
                    v.as_str().is_some_and(|s| !s.trim().is_empty())
                        || v.as_i64().is_some_and(|id| id > 0)
                })
                || has_preset
        }
        "comfyui" => {
            config
                .get("comfyui_server_ids")
                .and_then(|v| v.as_array())
                .map_or(false, |a| !a.is_empty())
                || config
                    .get("comfyui_server_id")
                    .and_then(|v| v.as_i64())
                    .is_some_and(|id| id > 0)
                || has_preset
        }
        _ => has_preset,
    };
    if !valid {
        return Err(AppError::BadRequest("请选择上游渠道".to_string()));
    }
    Ok(())
}

async fn hydrate_channel_preset_schedule(state: &AppState, ch: &mut Channel) {
    if ch.provider_type == "high_availability_group" {
        return;
    }
    let Some(pid) = ch.preset_id else {
        return;
    };
    if let Ok(configs_map) = crate::relay::router::get_cached_channel_configs(&state.db).await {
        if let Some(preset) = configs_map.get(&pid) {
            let resolved = ch.resolve_upstream_config(preset);
            ch.rate = resolved.rate;
            ch.priority = resolved.priority;
            ch.weight = resolved.weight;
        }
    }
}

pub async fn list_channels(
    State(state): State<Arc<AppState>>,
    claims: Option<axum::Extension<crate::auth::Claims>>,
) -> AppResult<Json<ChannelListResponse>> {
    let is_admin = claims.as_ref().map_or(false, |c| c.0.role == "admin");

    let mut channels: Vec<Channel> = sqlx::query_as(&state.db.format_query(
        "SELECT * FROM channels ORDER BY status DESC, sort_order DESC, priority DESC, id DESC",
    ))
    .fetch_all(&state.db.pool)
    .await?;

    if let Ok(configs_map) = crate::relay::router::get_cached_channel_configs(&state.db).await {
        for ch in &mut channels {
            if ch.provider_type != "high_availability_group" {
                if let Some(pid) = ch.preset_id {
                    if let Some(preset) = configs_map.get(&pid) {
                        let resolved = ch.resolve_upstream_config(preset);
                        ch.rate = resolved.rate;
                        ch.priority = resolved.priority;
                        ch.weight = resolved.weight;
                    }
                }
            }
        }
        channels.sort_by(|a, b| {
            b.status
                .cmp(&a.status)
                .then_with(|| b.sort_order.cmp(&a.sort_order))
                .then_with(|| b.priority.cmp(&a.priority))
                .then_with(|| b.id.cmp(&a.id))
        });
    }

    let safe_channels: Vec<ChannelSafe> = channels
        .into_iter()
        .map(|ch| ChannelSafe::from_with_role(ch, is_admin))
        .collect();
    let total = safe_channels.len() as i64;

    Ok(Json(ChannelListResponse {
        data: safe_channels,
        total,
    }))
}

pub async fn create_channel(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateChannelRequest>,
) -> AppResult<Json<ChannelSafe>> {
    if request.name.trim().is_empty() {
        return Err(AppError::BadRequest("渠道名称不能为空".to_string()));
    }
    let config_val = request.config.clone().unwrap_or_else(|| serde_json::json!({}));
    validate_channel_upstream(&request.provider_type, request.preset_id, &config_val)?;

    let models_json = serde_json::to_string(&request.models).unwrap_or_else(|_| "[]".to_string());
    let mapping_json = serde_json::to_string(&request.model_mapping.unwrap_or_default())
        .unwrap_or_else(|_| "{}".to_string());
    ensure_ha_channel_rule(&state, &request.provider_type, &config_val).await?;
    let config_json = serde_json::to_string(&config_val).unwrap_or_else(|_| "{}".to_string());
    let groups_json = serde_json::to_string(&request.user_groups.unwrap_or_default())
        .unwrap_or_else(|_| "[]".to_string());
    let exclude_groups_json =
        serde_json::to_string(&request.exclude_user_groups.unwrap_or_default())
            .unwrap_or_else(|_| "[]".to_string());

    // Auto-generate unique 4-digit group_aid
    let mut group_aid_val = String::new();
    use rand::Rng;
    for _ in 0..10 {
        let aid: i64 = rand::thread_rng().gen_range(1000..10000);
        let aid_str = aid.to_string();
        let exists: i64 = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT COUNT(*) FROM channels WHERE group_aid = ?"),
        )
        .bind(&aid_str)
        .fetch_one(&state.db.pool)
        .await?;
        if exists == 0 {
            group_aid_val = aid_str;
            break;
        }
    }
    if group_aid_val.is_empty() {
        group_aid_val = rand::thread_rng().gen_range(1000i64..10000).to_string(); // fallback
    }

    let sql = r#"INSERT INTO channels (name, provider_type, base_url, api_key, models, model_mapping, user_groups, exclude_user_groups, group_aid, preset_id, category_id, sort_order, priority, weight, status, max_rps, quota_limit, quota_used, daily_quota_limit, weekly_quota_limit, monthly_quota_limit, config, rate)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id"#;

    let id: i64 = sqlx::query_scalar::<_, i64>(&state.db.format_query(sql))
        .bind(&request.name)
        .bind(&request.provider_type)
        .bind(&request.base_url)
        .bind(&request.api_key)
        .bind(&models_json)
        .bind(&mapping_json)
        .bind(&groups_json)
        .bind(&exclude_groups_json)
        .bind(&group_aid_val)
        .bind(request.preset_id)
        .bind(request.category_id)
        .bind(request.sort_order.unwrap_or(0))
        .bind(request.priority.unwrap_or(0))
        .bind(request.weight.unwrap_or(1))
        .bind(request.max_rps.unwrap_or(0))
        .bind(request.quota_limit.unwrap_or(-1.0))
        .bind(request.quota_used.unwrap_or(0.0))
        .bind(request.daily_quota_limit.unwrap_or(-1.0))
        .bind(request.weekly_quota_limit.unwrap_or(-1.0))
        .bind(request.monthly_quota_limit.unwrap_or(-1.0))
        .bind(&config_json)
        .bind(request.rate)
        .fetch_one(&state.db.pool)
        .await?;

    let mut channel: Channel =
        sqlx::query_as(&state.db.format_query("SELECT * FROM channels WHERE id = ?"))
            .bind(id)
            .fetch_one(&state.db.pool)
            .await?;

    hydrate_channel_preset_schedule(&state, &mut channel).await;

    crate::relay::router::invalidate_channel_cache();
    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(ChannelSafe::from_with_role(channel, true)))
}

pub async fn update_channel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(request): Json<UpdateChannelRequest>,
) -> AppResult<Json<ChannelSafe>> {
    // Current channel for partial updates
    let mut channel: Channel =
        sqlx::query_as(&state.db.format_query("SELECT * FROM channels WHERE id = ?"))
            .bind(id)
            .fetch_one(&state.db.pool)
            .await?;

    if let Some(name) = request.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest("渠道名称不能为空".to_string()));
        }
        channel.name = trimmed.to_string();
    }
    if let Some(provider_type) = request.provider_type {
        channel.provider_type = provider_type;
    }
    if let Some(base_url) = request.base_url {
        channel.base_url = base_url;
    }
    if let Some(api_key) = request.api_key {
        // 【防护】空值或含脱敏标记的值不覆盖原始密钥
        if !api_key.is_empty() && !api_key.contains("******") {
            channel.api_key = api_key;
        }
    }
    let old_models = channel.get_models();
    if let Some(ref models) = request.models {
        channel.models = serde_json::to_string(models).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(mapping) = request.model_mapping {
        channel.model_mapping =
            serde_json::to_string(&mapping).unwrap_or_else(|_| "{}".to_string());
    } else if let Some(ref new_models) = request.models {
        // 局部解绑模型时（如通过列表气泡或模型渠道显示矩阵移除模型），自动清理已移除模型的孤儿别名与分辨率映射
        let new_set: std::collections::HashSet<&str> =
            new_models.iter().map(|s| s.as_str()).collect();
        let removed_keys: Vec<String> = old_models
            .into_iter()
            .filter(|m| !new_set.contains(m.as_str()))
            .collect();
        if !removed_keys.is_empty() {
            let mut drop_keys: std::collections::HashSet<String> =
                removed_keys.iter().cloned().collect();
            let placeholders = removed_keys
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT mid, model_id FROM models WHERE mid IN ({}) OR model_id IN ({})",
                placeholders, placeholders
            );
            let formatted = state.db.format_query(&sql);
            let mut query = sqlx::query_as::<_, (String, String)>(&formatted);
            for k in &removed_keys {
                query = query.bind(k);
            }
            for k in &removed_keys {
                query = query.bind(k);
            }
            if let Ok(rows) = query.fetch_all(&state.db.pool).await {
                for (mid, model_id) in rows {
                    drop_keys.insert(mid);
                    drop_keys.insert(model_id);
                }
            }
            if let Ok(mut map) =
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
                    &channel.model_mapping,
                )
            {
                let before = map.len();
                map.retain(|k, _| !drop_keys.contains(k));
                if map.len() != before {
                    channel.model_mapping =
                        serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string());
                }
            }
            if request.config.is_none() {
                if let Ok(mut cfg_val) =
                    serde_json::from_str::<serde_json::Value>(&channel.config)
                {
                    let mut cfg_changed = false;
                    for field in ["ha_model_mapping", "res_model_mapping"] {
                        if let Some(obj) = cfg_val.get_mut(field).and_then(|v| v.as_object_mut()) {
                            let before = obj.len();
                            obj.retain(|k, _| !drop_keys.contains(k));
                            if obj.len() != before {
                                cfg_changed = true;
                            }
                            if obj.is_empty() {
                                cfg_val.as_object_mut().map(|o| o.remove(field));
                                cfg_changed = true;
                            }
                        }
                    }
                    if cfg_changed {
                        channel.config =
                            serde_json::to_string(&cfg_val).unwrap_or_else(|_| "{}".to_string());
                    }
                }
            }
        }
    }
    if let Some(user_groups) = request.user_groups {
        channel.user_groups =
            serde_json::to_string(&user_groups).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(exclude_user_groups) = request.exclude_user_groups {
        channel.exclude_user_groups =
            serde_json::to_string(&exclude_user_groups).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(group_aid) = request.group_aid {
        channel.group_aid = Some(group_aid);
    }
    if let Some(sort_order) = request.sort_order {
        channel.sort_order = sort_order;
    }
    if let Some(priority) = request.priority {
        channel.priority = priority;
    }
    if let Some(weight) = request.weight {
        channel.weight = weight;
    }
    if let Some(status) = request.status {
        channel.status = status;
    }
    if let Some(max_rps) = request.max_rps {
        channel.max_rps = Some(max_rps);
    }
    if let Some(quota_limit) = request.quota_limit {
        channel.quota_limit = quota_limit;
    }
    if let Some(quota_used) = request.quota_used {
        channel.quota_used = quota_used;
    }
    if let Some(daily) = request.daily_quota_limit {
        channel.daily_quota_limit = daily;
    }
    if let Some(weekly) = request.weekly_quota_limit {
        channel.weekly_quota_limit = weekly;
    }
    if let Some(monthly) = request.monthly_quota_limit {
        channel.monthly_quota_limit = monthly;
    }
    if let Some(ref config) = request.config {
        ensure_ha_channel_rule(&state, &channel.provider_type, config).await?;
        channel.config = serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());
    }
    if let Some(rate) = request.rate {
        channel.rate = rate;
    }
    if let Some(category_id) = request.category_id {
        channel.category_id = category_id;
    }

    if let Some(preset_id) = request.preset_id {
        channel.preset_id = preset_id;
    }

    if request.preset_id.is_some() || request.config.is_some() {
        let config_val: serde_json::Value =
            serde_json::from_str(&channel.config).unwrap_or_default();
        validate_channel_upstream(&channel.provider_type, channel.preset_id, &config_val)?;
    }

    let mut group_aid_val = channel.group_aid.clone().unwrap_or_default();
    if group_aid_val.is_empty() {
        use rand::Rng;
        for _ in 0..10 {
            let aid: i64 = rand::thread_rng().gen_range(1000..10000);
            let aid_str = aid.to_string();
            let exists: i64 = sqlx::query_scalar(
                &state
                    .db
                    .format_query("SELECT COUNT(*) FROM channels WHERE group_aid = ?"),
            )
            .bind(&aid_str)
            .fetch_one(&state.db.pool)
            .await?;
            if exists == 0 {
                group_aid_val = aid_str;
                break;
            }
        }
        if group_aid_val.is_empty() {
            group_aid_val = rand::thread_rng().gen_range(1000i64..10000).to_string();
            // fallback
        }
    }

    // 诊断日志：记录关键字段变更，帮助排查上游切换后持续报错的问题
    {
        let old_ch: Option<(Option<i64>, String)> = sqlx::query_as(
            &state
                .db
                .format_query("SELECT preset_id, base_url FROM channels WHERE id = ?"),
        )
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await
        .ok()
        .flatten();
        let (old_preset, old_base) = old_ch.unwrap_or((None, String::new()));
        let sibling_count: i64 = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT COUNT(*) FROM channels WHERE group_aid = ? AND id != ?"),
        )
        .bind(&group_aid_val)
        .bind(id)
        .fetch_one(&state.db.pool)
        .await
        .unwrap_or(0);
        tracing::info!(
            "[Channel Update Debug] id={}, group_aid={}, preset_id: {:?} -> {:?}, base_url: '{}' -> '{}', 同组其他渠道数: {}",
            id, group_aid_val, old_preset, channel.preset_id, old_base, channel.base_url, sibling_count
        );
        if sibling_count > 0 {
            tracing::warn!(
                "[Channel Update Debug] ⚠️ 渠道 {} 所在分组 group_aid={} 存在 {} 条其他渠道记录，修改上游不会同步到它们！",
                id, group_aid_val, sibling_count
            );
        }
    }

    sqlx::query(
        &state.db.format_query(r#"UPDATE channels SET name = ?, provider_type = ?, base_url = ?, api_key = ?, models = ?, 
           model_mapping = ?, user_groups = ?, exclude_user_groups = ?, preset_id = ?, category_id = ?, sort_order = ?, priority = ?, weight = ?, status = ?, max_rps = ?, quota_limit = ?, quota_used = ?, daily_quota_limit = ?, weekly_quota_limit = ?, monthly_quota_limit = ?, config = ?, group_aid = ?, rate = ?, updated_at = CURRENT_TIMESTAMP
           WHERE id = ?"#)
    )
    .bind(&channel.name)
    .bind(&channel.provider_type)
    .bind(&channel.base_url)
    .bind(&channel.api_key)
    .bind(&channel.models)
    .bind(&channel.model_mapping)
    .bind(&channel.user_groups)
    .bind(&channel.exclude_user_groups)
    .bind(channel.preset_id)
    .bind(channel.category_id)
    .bind(channel.sort_order)
    .bind(channel.priority)
    .bind(channel.weight)
    .bind(channel.status)
    .bind(channel.max_rps)
    .bind(channel.quota_limit)
    .bind(channel.quota_used)
    .bind(channel.daily_quota_limit)
    .bind(channel.weekly_quota_limit)
    .bind(channel.monthly_quota_limit)
    .bind(&channel.config)
    .bind(&group_aid_val)
    .bind(channel.rate)
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    if !group_aid_val.is_empty() {
        sqlx::query(&state.db.format_query(
            "UPDATE channels SET sort_order = ?, category_id = ? WHERE group_aid = ? AND id != ?",
        ))
        .bind(channel.sort_order)
        .bind(channel.category_id)
        .bind(&group_aid_val)
        .bind(id)
        .execute(&state.db.pool)
        .await
        .ok();
    }

    // 渠道配置变更后清除熔断记录，防止旧熔断阻止修正后的渠道被选中
    if !group_aid_val.is_empty() {
        state.failed_channels.remove(&group_aid_val);
        // 同时清除 HA 子渠道的熔断记录（前缀匹配 ha_group_{channel_id}_）
        let ha_prefix = format!("ha_group_{}_", id);
        state
            .failed_channels
            .retain(|k, _| !k.starts_with(&ha_prefix));
        tracing::info!(
            "[Channel Update] 渠道 {} (group_aid={}) 配置已更新，已清除关联的熔断记录",
            id,
            group_aid_val
        );
    }

    hydrate_channel_preset_schedule(&state, &mut channel).await;

    crate::relay::router::invalidate_channel_cache();
    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(ChannelSafe::from_with_role(channel, true)))
}

pub async fn delete_channel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    sqlx::query(&state.db.format_query("DELETE FROM channels WHERE id = ?"))
        .bind(id)
        .execute(&state.db.pool)
        .await?;

    crate::relay::channel_quota::clear_channel_quota_buffer(id);
    crate::relay::router::invalidate_channel_cache();
    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(serde_json::json!({ "success": true })))
}

fn merge_custom_body(base: &mut serde_json::Value, custom: Option<&serde_json::Value>) {
    if let Some(serde_json::Value::Object(custom_obj)) = custom {
        if let Some(base_obj) = base.as_object_mut() {
            for (k, v) in custom_obj {
                base_obj.insert(k.clone(), v.clone());
            }
        }
    }
}

fn build_default_test_body(
    category: &str,
    model: &str,
    scenario: Option<&str>,
    resolution: &str,
    rule_is_stream: bool,
    is_image_tool: bool,
) -> serde_json::Value {
    let is_image = category == "图片" || is_image_tool;
    let is_video = category == "视频" || (category == "封装" && !is_image);

    if is_image {
        if scenario == Some("image_i2i") {
            serde_json::json!({
                "model": model,
                "prompt": "A cute white cat wearing sunglasses",
                "image": "https://dummyimage.com/1024x1024/333/fff.png",
                "n": 1
            })
        } else {
            serde_json::json!({
                "model": model,
                "prompt": "Draw a picture of a cute white cat. 请画一只可爱的白色小猫。",
                "n": 1
            })
        }
    } else if is_video {
        let mut v_body = serde_json::json!({
            "model": model,
            "prompt": "A short video of a cute white cat walking",
            "resolution": if resolution.is_empty() { "480p" } else { resolution },
            "duration": 5
        });
        if scenario == Some("video_i2v") {
            v_body["image_url"] =
                serde_json::json!("https://dummyimage.com/1024x1024/333/fff.png");
        }
        v_body
    } else if category == "音频" {
        if model.to_lowercase().contains("whisper") || scenario == Some("stt") {
            serde_json::json!({
                "model": model,
                "file": "https://dummyimage.com/test.mp3"
            })
        } else {
            serde_json::json!({
                "model": model,
                "input": "你好，TokensByte 渠道拨测中。",
                "voice": "alloy"
            })
        }
    } else if category == "向量" {
        serde_json::json!({
            "model": model,
            "input": "TokensByte embedding test prompt"
        })
    } else if category == "排序" {
        serde_json::json!({
            "model": model,
            "query": "TokensByte test query",
            "documents": ["TokensByte ranking doc 1", "TokensByte ranking doc 2"]
        })
    } else {
        let st = rule_is_stream || scenario == Some("stream");
        if scenario == Some("openai_responses") {
            serde_json::json!({
                "model": model,
                "input": "hi",
                "stream": st
            })
        } else {
            serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": "hi"}],
                "stream": st
            })
        }
    }
}

pub async fn test_channel(
    State(state): State<Arc<AppState>>,
    options_claims: Option<axum::Extension<crate::auth::Claims>>,
    Path(id): Path<i64>,
    Json(req): Json<crate::models::TestChannelRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let mut channel = crate::relay::router::fetch_channel(&state, id, None)
        .await
        .ok_or_else(|| crate::error::AppError::NotFound("渠道不存在".to_string()))?;

    let mut channel_config_id: Option<i64> = None;

    if channel.provider_type == "high_availability_group" {
        let target_sub_id = if let Some(sub_id) = req.sub_channel_id {
            Some(sub_id)
        } else {
            let cfg_val: serde_json::Value =
                serde_json::from_str(&channel.config).unwrap_or_default();
            let sub_ids: Vec<i64> = cfg_val
                .get("sub_channels")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
                .unwrap_or_default();
            if let Ok(configs_map) = crate::relay::router::get_cached_channel_configs(&state.db).await
            {
                let mut candidates: Vec<(i64, usize, i64)> = sub_ids
                    .iter()
                    .enumerate()
                    .filter_map(|(pos, sid)| {
                        let cfg = configs_map.get(sid)?;
                        if cfg.status != 1 {
                            return None;
                        }
                        let resolved = channel.resolve_upstream_config(cfg);
                        Some((resolved.priority, pos, *sid))
                    })
                    .collect();
                candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                candidates.first().map(|c| c.2).or_else(|| sub_ids.first().copied())
            } else {
                sub_ids.first().copied()
            }
        };

        if let Some(sub_id) = target_sub_id {
            let sub_config: Option<crate::models::ChannelConfig> = sqlx::query_as(
                &state
                    .db
                    .format_query("SELECT * FROM channel_configs WHERE id = ?"),
            )
            .bind(sub_id)
            .fetch_optional(&state.db.pool)
            .await
            .unwrap_or(None);

            if let Some(cfg) = sub_config {
                // 测试指定子上游时校验其额度（站点时区，与线上扣费一致）
                let site_tz =
                    crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
                let (_, now_week, now_month) = crate::models::quota_period_keys(&site_tz);
                if cfg.status != 1 {
                    return Err(crate::error::AppError::Forbidden(
                        "指定的上游渠道配置已禁用，测试请求被拦截".into(),
                    ));
                }
                if !cfg.has_available_quota(&site_tz, &now_week, &now_month) {
                    return Err(crate::error::AppError::Forbidden(
                        "指定的上游预设额度已耗尽，测试请求被拦截".into(),
                    ));
                }
                crate::relay::router::apply_ha_sub_mapped(&mut channel, &cfg);
                crate::relay::router::apply_volcengine_credential(&state, &mut channel).await;
                crate::relay::router::apply_comfyui_channel(&state, &mut channel).await;
                channel_config_id = Some(sub_id);
            } else {
                return Err(crate::error::AppError::NotFound(
                    "指定的高可用子上游渠道不存在".into(),
                ));
            }
        } else {
            return Err(crate::error::AppError::BadRequest(
                "高可用渠道组未绑定任何可用子渠道".into(),
            ));
        }
    }

    let tz_name = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let (now_day, now_week, now_month) = crate::models::quota_period_keys(&tz_name);
    if let Err(msg) = channel.check_quota_limits(&now_day, &now_week, &now_month) {
        return Err(crate::error::AppError::Forbidden(format!(
            "该渠道{}，测试请求被拦截",
            msg
        )));
    }

    // 绑定上游预设时同步校验预设状态与额度
    if let Some(pid) = channel.preset_id {
        if let Ok(Some(preset)) = sqlx::query_as::<_, crate::models::ChannelConfig>(
            &state
                .db
                .format_query("SELECT * FROM channel_configs WHERE id = ?"),
        )
        .bind(pid)
        .fetch_optional(&state.db.pool)
        .await
        {
            if preset.status != 1 {
                return Err(crate::error::AppError::Forbidden(
                    "绑定的上游渠道配置已禁用，测试请求被拦截".into(),
                ));
            }
            if !preset.has_available_quota(&tz_name, &now_week, &now_month) {
                return Err(crate::error::AppError::Forbidden(
                    "绑定的上游预设额度已耗尽，测试请求被拦截".into(),
                ));
            }
        } else {
            return Err(crate::error::AppError::NotFound(
                "绑定的上游渠道配置不存在，测试请求被拦截".into(),
            ));
        }
    }

    // preset / volc 已由 fetch_channel（及上方 HA 子配）水合，勿再查空父行覆盖
    let test_model = if let Some(m) = req.model {
        m
    } else {
        let model_mids = channel.get_models();
        if !model_mids.is_empty() {
            // models 存储的是 mid，需要反查 model_id
            let mid = &model_mids[0];
            let model_id: Option<String> = sqlx::query_scalar(
                &state
                    .db
                    .format_query("SELECT model_id FROM models WHERE mid = ?"),
            )
            .bind(mid)
            .fetch_optional(&state.db.pool)
            .await
            .unwrap_or(None);
            model_id.unwrap_or_else(|| mid.clone())
        } else {
            "gpt-3.5-turbo".to_string()
        }
    };

    // ── 获取该测试模型关联的计费规则（复用 relay 模块已有的模型查找逻辑） ──
    let db_model =
        crate::relay::proxy::find_active_model_exact(&state, &test_model, None, Some(&channel))
            .await;

    // 计算映射后的上游模型 ID（基础映射，进入循环后按 resolution 动态微调）
    let (mut resolved_model, _) =
        crate::relay::router::resolve_model(&channel, &test_model, db_model.as_ref(), None);

    let start = std::time::Instant::now();

    // ── 获取该测试模型关联的计费规则（请求开始锁定峰谷倍率） ──
    let db_rule = crate::relay::proxy::get_model_billing_rule(
        &state,
        &test_model,
        Some(&channel),
        db_model.as_ref(),
        &mut None,
    )
    .await;

    // ── 解析转发规则 ──
    let mut resolved: Option<crate::relay::forward::ResolvedForward> = None;
    let mut category = db_model
        .as_ref()
        .and_then(|m| m.type_name.clone())
        .unwrap_or_else(|| "聊天".to_string());
    let is_image_tool = db_model.as_ref().is_some_and(crate::api::plugins::is_plugin_image_tool);
    if is_image_tool {
        category = "图片".to_string();
    }
    let mut rule_is_stream = false;
    let mut db_forward_rule: Option<crate::models::ForwardRule> = None;

    if let Some(fid) = req.forward_rule_id {
        let rule: Option<crate::models::ForwardRule> = sqlx::query_as(
            &state
                .db
                .format_query("SELECT * FROM forward_rules WHERE id = ?"),
        )
        .bind(fid)
        .fetch_optional(&state.db.pool)
        .await?;
        if let Some(r) = rule {
            if !is_image_tool {
                category = r.category.clone();
            }
            if r.name.contains("流式") || r.name.to_lowercase().contains("stream") {
                rule_is_stream = true;
            }
            db_forward_rule = Some(r);
        }
    }

    // 确定真实的测试入口路径（显式入参优先，其次按场景与类别推导，确保分类隔离与 path_rewrite 精准命中）
    let entry_path = if let Some(ref ep) = req.entry_path.as_deref().filter(|s| !s.trim().is_empty()) {
        ep.trim().to_string()
    } else {
        match req.scenario.as_deref() {
            Some("anthropic_messages") if category == "聊天" => "/v1/messages".to_string(),
            Some("gemini_native") if category == "聊天" => "/v1beta/models/${model}:generateContent".to_string(),
            Some("openai_responses") if category == "聊天" => "/v1/responses".to_string(),
            _ => crate::relay::proxy::category_endpoint(Some(&category)).to_string(),
        }
    };

    if let Some(ref r) = db_forward_rule {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&r.config_json) {
            // 统一通过 parse_forward_config 解析，传入真实的 entry_path 供 path_rewrite / path_passthrough 匹配
            let category_path = crate::relay::proxy::category_endpoint(Some(&category));
            let mid = db_model.as_ref().map(|m| m.mid.clone());
            let res = crate::relay::forward::parse_forward_config(
                &config,
                &category_path,
                &entry_path,
                &r.eid,
                mid,
            );
            resolved = Some(res);
        }
    } else {
        // 如果没有指定 forward_rule_id，自动根据真实 entry_path 精准打分匹配
        resolved = crate::relay::forward::resolve_forward_rule(
            &state,
            &test_model,
            &category,
            &entry_path,
            Some(&channel),
            db_model.as_ref(),
        )
        .await;
    }

    let mut fwd = resolved.unwrap_or_else(|| {
        crate::relay::forward::infer_forward_from_base_url(
            &channel.base_url,
            &category,
            db_model.as_ref(),
        )
    });

    // ── 修正并校正 target_type 与 MediaKit 专向上游路径 ──
    crate::relay::forward::refine_target_type(&mut fwd, &channel.base_url);
    crate::relay::forward::apply_channel_provider(&mut fwd, &channel);
    #[cfg(feature = "plugin_volcengine_enhance")]
    crate::api::plugins::apply_volc_enhance_path(&mut fwd, &resolved_model);

    let is_image = category == "图片" || is_image_tool;
    let is_video = category == "视频" || (category == "封装" && !is_image);

    // 视频拨测：默认 480p，若自定义入参已指定则单轮执行；失败时支持升 720p 重试
    let video_resolutions: &[&str] = if is_video {
        if let Some(ref custom) = req.custom_body {
            if custom.get("resolution").is_some() {
                &[""]
            } else {
                &["480p", "720p"]
            }
        } else if req.scenario.as_deref() == Some("video_720p") {
            &["720p"]
        } else {
            &["480p", "720p"]
        }
    } else {
        &[""]
    };

    let mut openai_body = serde_json::Value::Null;
    let mut request_data = serde_json::Value::Null;
    let mut masked_endpoint = String::new();
    let mut curl_cmd = String::new();
    let mut request_json = String::new();
    let mut raw_response_text = String::new();
    let mut response_res: Result<serde_json::Value, crate::error::AppError> =
        Ok(serde_json::json!({}));
    #[cfg(feature = "plugin_comfyui")]
    let mut comfy_ack: Option<crate::api::plugins::comfyui_bridge::ComfySubmitAck> = None;

    for (attempt_idx, resolution) in video_resolutions.iter().enumerate() {
        let cur_resolution = if is_video {
            if resolution.is_empty() { "480p" } else { *resolution }
        } else {
            ""
        };

        // 构造贴合场景的标准化基础请求体，并应用用户自定义微调参数
        let mut cur_body = build_default_test_body(
            &category,
            &test_model,
            req.scenario.as_deref(),
            cur_resolution,
            rule_is_stream,
            is_image_tool,
        );
        merge_custom_body(&mut cur_body, req.custom_body.as_ref());

        // 从当前实际请求体提取分辨率，精确执行渠道高级分分辨率别名映射
        let (cur_resolved, _) = crate::relay::router::resolve_model_body(
            &channel,
            &test_model,
            db_model.as_ref(),
            Some(&cur_body),
        );
        cur_body["model"] = serde_json::json!(&cur_resolved);
        resolved_model = cur_resolved;
        openai_body = cur_body;

        request_data = crate::relay::forward::transform_request_body(
            &fwd,
            &resolved_model,
            &openai_body,
            &category,
            db_rule.as_ref(),
            Some(&state.http_client),
        )
        .await;

        // 可灵视频/图片动态路由路径匹配
        crate::relay::forward::resolve_kling_dynamic_path(&mut fwd, &request_data);

        if fwd.target_type == "comfyui" {
            #[cfg(feature = "plugin_comfyui")]
            {
                let target = crate::api::plugins::comfyui_bridge::resolve_submit_target(
                    &state,
                    &openai_body,
                    &channel,
                )
                .await?;
                request_json = serde_json::to_string(&openai_body).unwrap_or_default();
                curl_cmd = format!(
                    "# 本地视频上游 workflow={} servers={:?}",
                    target.workflow_id, target.eligible_server_ids
                );
                match crate::api::plugins::comfyui_bridge::submit_video(
                    Arc::clone(&state),
                    &openai_body,
                    &target,
                    None,
                    None,
                )
                .await
                {
                    Ok(ack) => {
                        channel.base_url = ack.base_url.clone();
                        raw_response_text = ack.openai_body();
                        response_res = Ok(serde_json::from_str(&raw_response_text)
                            .unwrap_or_else(|_| serde_json::json!({"id": ack.task_id})));
                        request_json = ack.prompt_json.clone();
                        comfy_ack = Some(ack);
                    }
                    Err(e) => {
                        let msg = e.error.to_string();
                        if !e.prompt_json.is_empty() {
                            request_json = e.prompt_json;
                        }
                        raw_response_text = msg.clone();
                        response_res = Ok(serde_json::json!({
                            "_upstream_status": e.error.http_status(),
                            "_upstream_error": {"error": msg}
                        }));
                    }
                }
                masked_endpoint = format!("{}/prompt", channel.base_url.trim_end_matches('/'));
                let send_failed = match &response_res {
                    Ok(v) => v.get("_upstream_error").is_some(),
                    Err(_) => true,
                };
                if !is_video || !send_failed || attempt_idx + 1 >= video_resolutions.len() {
                    break;
                }
                continue;
            }
            #[cfg(not(feature = "plugin_comfyui"))]
            {
                return Err(crate::error::AppError::BadRequest(
                    "当前服务暂不可用".into(),
                ));
            }
        }

        // ── 构建 URL ──
        let raw_base = channel.base_url.trim();
        if !raw_base.starts_with("http://") && !raw_base.starts_with("https://") {
            return Err(crate::error::AppError::BadRequest(
                "渠道 Base URL 格式无效（必须以 http:// 或 https:// 开头）".into(),
            ));
        }

        let endpoint = crate::relay::forward::build_upstream_url(
            &channel.base_url,
            &fwd,
            &resolved_model,
            &channel.api_key,
        );

        let builder = crate::services::http_client::with_upstream_timeout(
            crate::relay::forward::apply_request_auth(
                state
                    .http_client
                    .post(&endpoint)
                    .header("Content-Type", "application/json"),
                &fwd,
                &channel.api_key,
                &mut request_data,
                &channel.base_url,
            ),
        );

        // ── 生成 cURL 命令（在 apply_request_auth 之后，确保 body 包含可能注入的 SubAppId 等字段）──
        masked_endpoint = crate::relay::forward::mask_key_in_string(&endpoint, &channel.api_key);
        let auth_headers_for_curl =
            crate::relay::forward::build_auth_headers(&fwd, &channel.api_key, true);
        curl_cmd = format!("curl -X POST '{}' \\\n", masked_endpoint);
        curl_cmd.push_str("  -H 'Content-Type: application/json' \\\n");
        for (k, v) in &auth_headers_for_curl {
            let masked_v = crate::relay::forward::mask_key_in_string(v, &channel.api_key);
            curl_cmd.push_str(&format!("  -H '{}: {}' \\\n", k, masked_v));
        }
        request_json = serde_json::to_string(&request_data).unwrap_or_default();
        curl_cmd.push_str(&format!("  -d '{}'", request_json.replace("'", "\\'")));

        response_res = match builder.send().await {
            Ok(r) => {
                let status = r.status();
                let headers = r.headers().clone();
                let is_stream_resp = headers
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.contains("text/event-stream"))
                    .unwrap_or(false);

                if status.is_success() {
                    let text = r.text().await.unwrap_or_default();
                    raw_response_text = text.clone();
                    if (is_stream_resp || text.contains("data: "))
                        && text.lines().any(|l| l.trim().starts_with("data: "))
                    {
                        let mut accumulated_text = String::new();
                        let mut sse_usage = None;
                        for line in text.lines() {
                            let line = line.trim();
                            if let Some(chunk_json_str) = crate::relay::forward::transform_sse_line(
                                &fwd.target_type,
                                line,
                                &resolved_model,
                            ) {
                                if let Ok(chunk_val) =
                                    serde_json::from_str::<serde_json::Value>(&chunk_json_str)
                                {
                                    if let Some(delta) = chunk_val
                                        .pointer("/choices/0/delta/content")
                                        .and_then(|c| c.as_str())
                                    {
                                        accumulated_text.push_str(delta);
                                    }
                                    if let Some(u) = chunk_val.get("usage") {
                                        sse_usage = Some(u.clone());
                                    }
                                }
                            }
                        }
                        let mut resp_obj = serde_json::json!({
                            "status": "success",
                            "stream": true,
                            "text": accumulated_text,
                        });
                        if let Some(u) = sse_usage {
                            resp_obj["usage"] = u;
                        }
                        Ok(resp_obj)
                    } else {
                        match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(v) => Ok(v),
                            Err(_) => Ok(serde_json::json!({
                                "status": "success",
                                "raw": text.chars().take(500).collect::<String>()
                            })),
                        }
                    }
                } else {
                    let status_val = status.as_u16();
                    let err_body = r.text().await.unwrap_or_default();
                    raw_response_text = err_body.clone();
                    let upstream_json = serde_json::from_str::<serde_json::Value>(&err_body)
                        .unwrap_or_else(|_| serde_json::json!({"raw_error": err_body}));
                    Ok(serde_json::json!({
                        "_upstream_status": status_val,
                        "_upstream_error": upstream_json
                    }))
                }
            }
            Err(e) => {
                raw_response_text = format!(
                    "{{\"connection_error\":\"{}\"}}",
                    e.to_string().replace('"', "\\\"")
                );
                Ok(serde_json::json!({
                    "_upstream_status": 0,
                    "_upstream_error": {"connection_error": e.to_string()}
                }))
            }
        };

        let send_failed = match &response_res {
            Ok(v) => v.get("_upstream_error").is_some(),
            Err(_) => true,
        };
        // 非视频只跑一轮；视频 480p 失败则升 720p 再试
        if !is_video || !send_failed || attempt_idx + 1 >= video_resolutions.len() {
            break;
        }
        tracing::warn!(
            "[TestChannel] 视频拨测 resolution={} 失败，改用 {} 重试",
            resolution,
            video_resolutions[attempt_idx + 1]
        );
    }

    // ── 响应格式转换（与 relay 保持一致，确保 task_id 可被正确提取）──
    let response_content_for_log = if crate::relay::forward::is_tencent_target(&fwd.target_type) {
        let (converted, _) =
            crate::relay::task::convert_tencent_post_response(&raw_response_text, &category);
        converted
    } else {
        raw_response_text
    };

    // ── 记录日志（字段与 relay 模块对齐，确保后续功能正常读取）──
    let latency_ms = start.elapsed().as_millis() as i64;
    let mut status_code: i64 = 200;
    let mut p_tokens: i64 = 0;
    let mut c_tokens: i64 = 0;
    let mut err_msg: Option<String> = None;

    match &response_res {
        Ok(v) => {
            if v.get("_upstream_error").is_some() {
                status_code = v
                    .get("_upstream_status")
                    .and_then(|s| s.as_i64())
                    .filter(|&s| s >= 400 && s < 600)
                    .unwrap_or(502);
                err_msg = Some(serde_json::to_string(&v["_upstream_error"]).unwrap_or_default());
            } else if let Some(usage) = v.get("usage") {
                p_tokens = usage
                    .get("prompt_tokens")
                    .and_then(|t| t.as_i64())
                    .unwrap_or(0);
                c_tokens = usage
                    .get("completion_tokens")
                    .and_then(|t| t.as_i64())
                    .unwrap_or(0);
            }
        }
        Err(e) => {
            status_code = 500;
            err_msg = Some(e.to_string());
        }
    }

    let user_id_str = match &options_claims {
        Some(axum::Extension(c)) => c.sub.clone(),
        None => "0".to_string(),
    };

    // endpoint 格式与 relay 一致: "入口|上游路径"，便于正确推断类别
    let ep = format!(
        "test|{}",
        fwd.upstream_path.replace("${model}", &resolved_model)
    );
    let openai_body_str = serde_json::to_string(&openai_body).unwrap_or_default();
    let task_id =
        crate::relay::task::extract_task_id(&response_content_for_log).unwrap_or_default();

    // ── 预先计算渠道测试的计费明细依据（但不扣费） ──
    let preview_usage = crate::relay::usage_extractor::UsageTokens {
        prompt: p_tokens as i32,
        completion: c_tokens as i32,
        ..Default::default()
    };
    let (_, calc_detail) = crate::relay::compute_cost(
        db_model.as_ref(),
        db_rule.as_ref(),
        &preview_usage,
        if channel.rate > 0.0 {
            channel.rate
        } else {
            1.0
        },
        &crate::relay::usage_extractor::ExtractedFeatures::default(),
    );
    let calc_detail = calc_detail.replace(
        '¤',
        &crate::api::settings::get_currency_settings(&state)
            .await
            .currency_unit,
    );
    let final_billing_detail = format!("[测试渠道，不扣费] {}", calc_detail);
    let billing_pid = db_rule.as_ref().map(|r| r.pid.clone());
    let forward_eid = db_forward_rule.as_ref().map(|r| r.eid.clone()).or_else(|| {
        if fwd.eid.is_empty() {
            None
        } else {
            Some(fwd.eid.clone())
        }
    });

    // ── 存入日志前对内容进行 base64 脱敏（复用 relay 模块统一脱敏逻辑） ──
    let sanitized_req = crate::relay::proxy::sanitize_base64(&openai_body_str);
    let sanitized_resp = crate::relay::proxy::sanitize_base64(&response_content_for_log);
    let sanitized_upstream_req = crate::relay::proxy::sanitize_base64(&request_json);

    // 与 relay::record_pending_log 一致：任务类用 tsk_，其余用 log_
    let log_id_str = {
        let prefix = if !category.is_empty() && category != "聊天" {
            "tsk_"
        } else {
            "log_"
        };
        format!("{}{}", prefix, ulid::Ulid::new().to_string().to_lowercase())
    };

    // 异步图/视频测试：先写「冻结」进行中态，供任务列表展示刷新按钮；
    // is_completed=1 避免被 TaskPoller 捡起重复轮询（本函数内同步 poll）。
    let will_poll = !task_id.is_empty() && (is_image || is_video) && status_code == 200;
    let insert_billing_detail = if will_poll {
        "[测试渠道，不扣费] 异步任务处理中(冻结)".to_string()
    } else {
        final_billing_detail.clone()
    };

    // INSERT 返回数字主键 id；同时写入业务 log_id（tsk_/log_）供列表展示与检索
    let db_row_id = sqlx::query_scalar::<_, i64>(&state.db.format_query(
        "INSERT INTO logs (log_id, user_id, channel_id, token_id, model, prompt_tokens, completion_tokens, \
         cost, latency_ms, status_code, endpoint, error_message, upstream_url, \
         request_content, response_content, upstream_req_content, billing_detail, action_type, task_id, \
         billing_pid, forward_eid, channel_config_id, is_ha, is_completed) \
         VALUES (?, ?, ?, 0, ?, ?, ?, 0.0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1) RETURNING id"
    ))
        .bind(&log_id_str).bind(&user_id_str).bind(id).bind(&test_model)
        .bind(p_tokens).bind(c_tokens).bind(latency_ms)
        .bind(status_code).bind(&ep).bind(&err_msg).bind(&masked_endpoint)
        .bind(&sanitized_req).bind(&sanitized_resp)
        .bind(&sanitized_upstream_req).bind(&insert_billing_detail).bind(&category).bind(&task_id)
        .bind(billing_pid).bind(forward_eid).bind(channel_config_id)
        .bind(crate::relay::ha::channel_is_ha_flag(&channel))
        .fetch_optional(&state.db.pool).await.unwrap_or(None);

    #[cfg(feature = "plugin_comfyui")]
    if let (Some(lid), Some(ack)) = (db_row_id, comfy_ack.as_ref()) {
        crate::api::plugins::comfyui_bridge::link_job(&state, lid, ack).await?;
    }

    // ── 异步任务自动轮询（统一支持所有厂商：火山/腾讯云/DashScope/可灵等）──
    let mut response_res = response_res;
    if will_poll {
        if let Some(final_body) = crate::relay::task::poll_task_result(
            &state,
            &channel,
            &fwd,
            &task_id,
            crate::relay::task::PollTaskOpts {
                model: &resolved_model,
                category: &category,
                timeout_secs: 180,
                jimeng_ctx: None,
            },
        )
        .await
        .into_terminal_body()
        {
            // 更新日志：终态响应 + 最终计费明细（去掉冻结标记）
            if let Some(lid) = db_row_id {
                let sanitized_final = crate::relay::proxy::sanitize_base64(&final_body);
                let _ = sqlx::query(&state.db.format_query(
                    "UPDATE logs SET response_content = ?, billing_detail = ?, is_completed = 1 WHERE id = ?",
                ))
                .bind(&sanitized_final)
                .bind(&final_billing_detail)
                .bind(lid)
                .execute(&state.db.pool)
                .await;
            }
            // 更新前端展示的响应数据为终态结果
            response_res = match serde_json::from_str::<serde_json::Value>(&final_body) {
                Ok(v) => Ok(v),
                Err(_) => {
                    Ok(serde_json::json!({"raw": final_body.chars().take(500).collect::<String>()}))
                }
            };
        } else if let Some(lid) = db_row_id {
            // 轮询超时/无终态：保留响应，去掉冻结以免任务列表永久「进行中」
            let _ =
                sqlx::query(&state.db.format_query(
                    "UPDATE logs SET billing_detail = ?, is_completed = 1 WHERE id = ?",
                ))
                .bind(&final_billing_detail)
                .bind(lid)
                .execute(&state.db.pool)
                .await;
        }
    }

    let matched_rule = serde_json::json!({
        "eid": fwd.eid,
        "target_type": fwd.target_type,
        "upstream_path": fwd.upstream_path,
        "auth_type": fwd.auth_type,
        "is_cascade": fwd.is_cascade,
        "cascade_engine": fwd.cascade_engine,
        "asset_convert": fwd.asset_convert || fwd.upstream_asset_convert,
        "fast_response": fwd.fast_response,
    });

    let cascade_preflight = if fwd.is_cascade {
        let mut check_msg = format!("阶段二引擎: {}", fwd.cascade_engine);
        let mut is_ready = true;
        if fwd.cascade_engine == "tencent" {
            #[cfg(feature = "plugin_tencent_enhance")]
            {
                if let Ok(cfg) = crate::api::plugins::tencent_enhance::load_ready_config(&state).await {
                    check_msg.push_str(&format!(" (MPS 存储桶: {})", cfg.mps_cos_bucket));
                } else {
                    check_msg.push_str(" (警告: 腾讯云增强配置未就绪)");
                    is_ready = false;
                }
            }
            #[cfg(not(feature = "plugin_tencent_enhance"))]
            {
                check_msg.push_str(" (警告: 腾讯云增强特性未编译启用)");
                is_ready = false;
            }
        } else {
            #[cfg(feature = "plugin_volcengine_enhance")]
            {
                let volc_on = crate::api::plugins::is_plugin_enabled(&state, "volcengine_enhance").await;
                if volc_on {
                    check_msg.push_str(" (火山 MediaKit 插件已就绪)");
                } else {
                    check_msg.push_str(" (警告: 火山增强插件未启用)");
                    is_ready = false;
                }
            }
        }
        Some(serde_json::json!({
            "engine": fwd.cascade_engine,
            "ready": is_ready,
            "message": check_msg,
            "crop_480p": fwd.crop_480p,
            "crop_timing": fwd.crop_timing,
        }))
    } else {
        None
    };

    // ── 返回结果 ──
    match response_res {
        Ok(response_value) => {
            let is_upstream_err = response_value.get("_upstream_error").is_some();
            if is_upstream_err {
                let upstream_status = response_value
                    .get("_upstream_status")
                    .and_then(|s| s.as_i64())
                    .filter(|&s| s > 0)
                    .unwrap_or(502);
                Ok(Json(serde_json::json!({
                    "success": false, "err_msg": format!("上游返回 HTTP {}", upstream_status),
                    "latency": latency_ms, "channel_id": id, "curl_command": curl_cmd,
                    "request_data": request_data, "response_data": response_value,
                    "matched_rule": matched_rule, "cascade_preflight": cascade_preflight,
                    "entry_path": entry_path, "scenario": req.scenario,
                })))
            } else {
                Ok(Json(serde_json::json!({
                    "success": true, "latency": latency_ms, "channel_id": id,
                    "curl_command": curl_cmd, "request_data": request_data, "response_data": response_value,
                    "matched_rule": matched_rule, "cascade_preflight": cascade_preflight,
                    "entry_path": entry_path, "scenario": req.scenario,
                })))
            }
        }
        Err(e) => {
            let err_str = e.to_string();
            Ok(Json(serde_json::json!({
                "success": false, "err_msg": err_str.clone(), "latency": latency_ms,
                "channel_id": id, "curl_command": curl_cmd, "request_data": request_data,
                "response_data": { "error": err_str },
                "matched_rule": matched_rule, "cascade_preflight": cascade_preflight,
                "entry_path": entry_path, "scenario": req.scenario,
            })))
        }
    }
}

// ── 熔断状态查询与重置 ────────────────────────────────────────────

/// 查询渠道及其 HA 子渠道的熔断状态
pub async fn get_meltdown_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let channel: Channel =
        sqlx::query_as(&state.db.format_query("SELECT * FROM channels WHERE id = ?"))
            .bind(id)
            .fetch_one(&state.db.pool)
            .await?;

    let now = std::time::Instant::now();
    // 顺带清掉已过期熔断项，避免管理端展示与内存堆积
    crate::relay::ha::scrub_failed_channels(&state);
    let group_aid = channel.group_aid.clone().unwrap_or_default();

    // 1. 查询渠道自身的熔断状态
    let channel_meltdown = if !group_aid.is_empty() {
        if let Some(blocked_until) = state.failed_channels.get(&group_aid) {
            if *blocked_until.value() > now {
                let remaining = blocked_until.value().duration_since(now).as_secs();
                serde_json::json!({ "is_melted": true, "remaining_seconds": remaining, "key": group_aid })
            } else {
                serde_json::json!({ "is_melted": false, "remaining_seconds": 0 })
            }
        } else {
            serde_json::json!({ "is_melted": false, "remaining_seconds": 0 })
        }
    } else {
        serde_json::json!({ "is_melted": false, "remaining_seconds": 0 })
    };

    // 2. 查询 HA 子渠道的熔断状态
    let mut sub_channels = Vec::new();
    if channel.provider_type == "high_availability_group" {
        let config: serde_json::Value =
            serde_json::from_str(&channel.config).unwrap_or(serde_json::json!({}));

        let sub_ids: Vec<i64> = config
            .get("sub_channels")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();

        // 批量查询子渠道名称
        let mut sub_names: std::collections::HashMap<i64, String> =
            std::collections::HashMap::new();
        if !sub_ids.is_empty() {
            let placeholders: Vec<String> = sub_ids.iter().map(|_| "?".to_string()).collect();
            let sql = format!(
                "SELECT id, name FROM channel_configs WHERE id IN ({})",
                placeholders.join(",")
            );
            let formatted_sql = state.db.format_query(&sql);
            let mut query = sqlx::query_as::<_, (i64, String)>(&formatted_sql);
            for sid in &sub_ids {
                query = query.bind(sid);
            }
            if let Ok(rows) = query.fetch_all(&state.db.pool).await {
                for (sid, name) in rows {
                    sub_names.insert(sid, name);
                }
            }
        }

        for sub_id in &sub_ids {
            let ha_key = crate::relay::ha::ha_sub_channel_key(id, *sub_id);
            let name = sub_names
                .get(sub_id)
                .cloned()
                .unwrap_or_else(|| format!("渠道 {}", sub_id));

            if let Some(blocked_until) = state.failed_channels.get(&ha_key) {
                if *blocked_until.value() > now {
                    let remaining = blocked_until.value().duration_since(now).as_secs();
                    sub_channels.push(serde_json::json!({
                        "config_id": sub_id,
                        "name": name,
                        "is_melted": true,
                        "remaining_seconds": remaining,
                        "key": ha_key,
                    }));
                } else {
                    sub_channels.push(serde_json::json!({
                        "config_id": sub_id,
                        "name": name,
                        "is_melted": false,
                        "remaining_seconds": 0,
                    }));
                }
            } else {
                sub_channels.push(serde_json::json!({
                    "config_id": sub_id,
                    "name": name,
                    "is_melted": false,
                    "remaining_seconds": 0,
                }));
            }
        }
    }

    Ok(Json(serde_json::json!({
        "channel_meltdown": channel_meltdown,
        "sub_channels": sub_channels,
    })))
}

/// 手动重置渠道及其 HA 子渠道的熔断状态
pub async fn reset_meltdown(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let channel: Channel =
        sqlx::query_as(&state.db.format_query("SELECT * FROM channels WHERE id = ?"))
            .bind(id)
            .fetch_one(&state.db.pool)
            .await?;

    let group_aid = channel.group_aid.clone().unwrap_or_default();
    let mut cleared_count: i64 = 0;

    // 1. 清除渠道自身的熔断记录
    if !group_aid.is_empty() {
        if state.failed_channels.remove(&group_aid).is_some() {
            cleared_count += 1;
        }
    }

    // 2. 清除所有 HA 子渠道的熔断记录
    let ha_prefix = format!("ha_group_{}_", id);
    let keys_to_remove: Vec<String> = state
        .failed_channels
        .iter()
        .filter(|entry| entry.key().starts_with(&ha_prefix))
        .map(|entry| entry.key().clone())
        .collect();

    for key in &keys_to_remove {
        state.failed_channels.remove(key);
        cleared_count += 1;
    }

    tracing::info!(
        "[Meltdown Reset] 管理员手动重置渠道 {} (group_aid={}) 的熔断状态，共清除 {} 条记录",
        id,
        group_aid,
        cleared_count
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "cleared_count": cleared_count,
    })))
}

#[derive(serde::Deserialize)]
pub struct SetSubMeltdownRequest {
    pub config_id: i64,
    pub melted: bool,
}

/// 手动熔断或恢复高可用组内单个上游
pub async fn set_sub_meltdown(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<SetSubMeltdownRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let channel: Channel =
        sqlx::query_as(&state.db.format_query("SELECT * FROM channels WHERE id = ?"))
            .bind(id)
            .fetch_one(&state.db.pool)
            .await?;

    if channel.provider_type != "high_availability_group" {
        return Err(AppError::BadRequest(
            "仅高可用渠道组支持子渠道熔断操作".into(),
        ));
    }

    let config: serde_json::Value =
        serde_json::from_str(&channel.config).unwrap_or(serde_json::json!({}));
    let sub_ids: Vec<i64> = config
        .get("sub_channels")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default();
    if !sub_ids.contains(&req.config_id) {
        return Err(AppError::BadRequest("该上游不在此高可用组内".into()));
    }

    let key = crate::relay::ha::ha_sub_channel_key(id, req.config_id);
    if req.melted {
        let bundle = crate::relay::relay_settings::get_cached_ha_rules(&state.db).await;
        let rule =
            bundle.resolve_arc(crate::relay::ha_rule::parse_rule_id(&channel.config).as_deref());
        let secs = match rule.melt_secs(503) {
            0 => 300,
            n => n,
        };
        let until = std::time::Instant::now() + std::time::Duration::from_secs(secs as u64);
        state.failed_channels.insert(key.clone(), until);
        tracing::info!(
            "[Meltdown] 管理员手动熔断渠道 {} 子渠 {}，冷却 {} 秒",
            id,
            req.config_id,
            secs
        );
        Ok(Json(serde_json::json!({
            "success": true,
            "melted": true,
            "remaining_seconds": secs,
            "key": key,
        })))
    } else {
        let cleared = state.failed_channels.remove(&key).is_some();
        tracing::info!(
            "[Meltdown] 管理员手动恢复渠道 {} 子渠 {}",
            id,
            req.config_id
        );
        Ok(Json(serde_json::json!({
            "success": true,
            "melted": false,
            "cleared": cleared,
            "key": key,
        })))
    }
}

/// 手动清零渠道分组已用额度（总/日/月）
pub async fn reset_quota(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query(
        &state
            .db
            .format_query(&crate::models::channel_quota::reset_quota_sql("channels")),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(crate::error::AppError::NotFound("渠道不存在".into()));
    }

    crate::relay::channel_quota::clear_channel_quota_buffer(id);
    crate::relay::router::invalidate_channel_cache();
    tracing::info!("[Channel Quota Reset] 管理员手动清零渠道 {} 的已用额度", id);
    Ok(Json(serde_json::json!({ "success": true })))
}

/// 渠道调用计费全链路模拟仿真试算
pub async fn simulate_channel_billing(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<crate::relay::simulation::SimulateBillingRequest>,
) -> AppResult<Json<crate::relay::simulation::SimulateBillingResponse>> {
    let result = crate::relay::simulation::simulate_channel_billing(&state, id, req).await?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::validate_channel_upstream;
    use serde_json::json;

    #[test]
    fn validate_upstream_accepts_volcengine_string_credential_id() {
        let cfg = json!({ "volcengine_enhance_credential_id": "k3j9a2x1l8z" });
        assert!(validate_channel_upstream("volcengine", None, &cfg).is_ok());

        let empty_cfg = json!({ "volcengine_enhance_credential_id": "   " });
        assert!(validate_channel_upstream("volcengine", None, &empty_cfg).is_err());

        assert!(validate_channel_upstream("volcengine", Some(5), &json!({})).is_ok());
    }

    #[test]
    fn validate_upstream_checks_ha_comfyui_and_preset() {
        assert!(validate_channel_upstream(
            "high_availability_group",
            None,
            &json!({ "sub_channels": [1, 2] })
        )
        .is_ok());
        assert!(validate_channel_upstream(
            "high_availability_group",
            None,
            &json!({ "sub_channels": [] })
        )
        .is_err());

        assert!(
            validate_channel_upstream("comfyui", None, &json!({ "comfyui_server_ids": [1] }))
                .is_ok()
        );
        assert!(validate_channel_upstream("comfyui", None, &json!({})).is_err());

        assert!(validate_channel_upstream("custom", Some(1), &json!({})).is_ok());
        assert!(validate_channel_upstream("custom", None, &json!({})).is_err());
    }
}
