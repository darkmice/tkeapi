/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! Relay: Native protocol passthrough (Gemini / Volcengine / DashScope).
//! Provides direct-path endpoints that mirror the vendor's own API surface,
//! while still running through the gateway's auth / billing / logging pipeline.

use super::proxy;
use super::upstream_headers;
use super::url_utils::join_url;
use crate::models::ApiToken;
use crate::{
    error::{AppError, AppResult},
    AppState,
};
use axum::{
    extract::{Extension, OriginalUri, Path, Query, State},
    response::Response,
    Json,
};
use std::collections::HashMap;
#[cfg(feature = "commercial_plugins")]
use std::collections::HashSet;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════
//  阿里百炼 DashScope Native:
//    POST /api/v1/services/aigc/multimodal-generation/generation
//    生图/多模态聊天共用官方路径；模型类型为聊天 → chat，否则 → image。
// ═══════════════════════════════════════════════════════════════

pub async fn dashscope_multimodal_generation(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    OriginalUri(uri): OriginalUri,
    request: axum::http::Request<axum::body::Body>,
) -> AppResult<Response> {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 50 * 1024 * 1024)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read request body: {e}")))?;
    let is_chat = if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        if let Some(model) = json["model"].as_str() {
            proxy::find_active_model_exact(&state, model, None, None)
                .await
                .and_then(|m| m.type_name)
                .is_some_and(|t| t == "聊天")
        } else {
            false
        }
    } else {
        false
    };
    let req = axum::http::Request::from_parts(parts, axum::body::Body::from(bytes));
    if is_chat {
        super::chat::chat_completions(State(state), Extension(token), OriginalUri(uri), req).await
    } else {
        super::image::image_generations(State(state), Extension(token), OriginalUri(uri), req).await
    }
}

// ═══════════════════════════════════════════════════════════════
//  Google Gemini Native Relay:
//    POST /v1/models/{model}:{action}
//    POST /v1beta/models/{model}:{action}
//  原生生图转入 image_generations，原生聊天转入 chat_completions；
//  原始请求体挂载至 Extensions，日志 100% 保持客户端纯净输入（零污染）。
// ═══════════════════════════════════════════════════════════════

/// 精准判定 Gemini 原生请求是否携带图片生成参数（支持 Google 原生、Interactions 与 OpenAI 兼容生图参数）
pub(crate) fn has_gemini_image_params(body: &serde_json::Value) -> bool {
    body.pointer("/generationConfig/imageConfig").is_some()
        || body.pointer("/generation_config/image_config").is_some()
        || body
            .pointer("/response_format/type")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("image"))
        || (body.get("prompt").is_some() && body.get("messages").is_none())
}

async fn dispatch_gemini_relay(
    state: Arc<AppState>,
    token: ApiToken,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    raw_content: String,
    body_json: serde_json::Value,
    model: &str,
) -> AppResult<Response> {
    // 优先按图片参数识别（支持 Google 原生、Interactions 与 OpenAI 参数）；无参数时按模型类型兜底
    let is_image = has_gemini_image_params(&body_json)
        || proxy::find_active_model_exact(&state, model, None, None)
            .await
            .and_then(|m| m.type_name)
            .is_some_and(|t| t == "图片");

    let (mut parts, _) = axum::http::Request::builder().body(()).unwrap().into_parts();
    parts.headers = headers;
    let mut req = axum::http::Request::from_parts(
        parts,
        axum::body::Body::from(serde_json::to_vec(&body_json).unwrap_or_default()),
    );
    req.extensions_mut()
        .insert(super::RawRequestContent(raw_content));

    if is_image {
        super::image::image_generations(State(state), Extension(token), OriginalUri(uri), req).await
    } else {
        super::chat::chat_completions(State(state), Extension(token), OriginalUri(uri), req).await
    }
}

pub async fn gemini_native_relay(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    Path(model_action): Path<String>,
    OriginalUri(uri): OriginalUri,
    headers: axum::http::HeaderMap,
    body_bytes: axum::body::Bytes,
) -> AppResult<Response> {
    let (model, action) = model_action
        .split_once(':')
        .ok_or_else(|| AppError::BadRequest("Invalid path: expected {model}:{action}".into()))?;

    let raw_content = String::from_utf8_lossy(&body_bytes).to_string();
    let mut body_json: serde_json::Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON: {}", e)))?;

    let is_stream = action.starts_with("streamGenerateContent");
    // 内部注入 model 与 stream 供下游模型权限校验与转发规则路由（不污染日志）
    body_json["model"] = serde_json::json!(model);
    if is_stream {
        body_json["stream"] = serde_json::json!(true);
    }

    dispatch_gemini_relay(state, token, uri, headers, raw_content, body_json, model).await
}

pub async fn gemini_interactions_relay(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    OriginalUri(uri): OriginalUri,
    headers: axum::http::HeaderMap,
    body_bytes: axum::body::Bytes,
) -> AppResult<Response> {
    let raw_content = String::from_utf8_lossy(&body_bytes).to_string();
    let mut body_json: serde_json::Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON: {}", e)))?;

    let model = body_json
        .get("model")
        .or_else(|| body_json.get("agent"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("Missing required parameter: model".into()))?
        .to_string();

    if body_json.get("model").and_then(|v| v.as_str()).is_none() {
        body_json["model"] = serde_json::json!(&model);
    }

    dispatch_gemini_relay(state, token, uri, headers, raw_content, body_json, &model).await
}

// 素材代理 API 白名单（单资源读写删做归属校验；列表按本地归属过滤，确保数据隔离）
const ARK_ASSET_ACTIONS: &[&str] = &[
    "CreateAsset",
    "GetAsset",
    "UpdateAsset",
    "ListAssets",
    "CreateAssetGroup",
    "GetAssetGroup",
    "UpdateAssetGroup",
    "ListAssetGroups",
    "DeleteAsset",
    "DeleteAssetGroup",
];

#[cfg(feature = "commercial_plugins")]
/// 加载当前用户在本插件命名空间下拥有的 Ark 资源 ID 集合
async fn load_owned_ark_ids(
    state: &AppState,
    table: &str,
    id_col: &str,
    user_id: &str,
    plugin_ns: &str,
) -> HashSet<String> {
    // table/id_col 仅来自内部常量，禁止外部输入
    let sql = state.db.format_query(&format!(
        "SELECT {id_col} FROM {table} WHERE user_id = ? AND plugin_ns = ? \
         AND {id_col} IS NOT NULL AND {id_col} != ''"
    ));
    let rows: Vec<(String,)> = sqlx::query_as(&sql)
        .bind(user_id)
        .bind(plugin_ns)
        .fetch_all(&state.db.pool)
        .await
        .unwrap_or_default();
    rows.into_iter().map(|(id,)| id).collect()
}

#[cfg(feature = "commercial_plugins")]
/// 将 List 请求的 Filter.GroupIds 收窄为当前用户拥有的组（与已有 GroupIds 取交集）
fn narrow_list_filter_group_ids(body: &mut serde_json::Value, owned_groups: &HashSet<String>) {
    if owned_groups.is_empty() {
        return;
    }
    let Some(obj) = body.as_object_mut() else {
        return;
    };
    let filter = obj
        .entry("Filter".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let Some(fobj) = filter.as_object_mut() else {
        return;
    };

    let narrowed: Vec<serde_json::Value> =
        if let Some(existing) = fobj.get("GroupIds").and_then(|v| v.as_array()) {
            existing
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|id| owned_groups.contains(*id))
                .map(|s| serde_json::json!(s))
                .collect()
        } else {
            owned_groups.iter().map(|s| serde_json::json!(s)).collect()
        };
    fobj.insert("GroupIds".to_string(), serde_json::Value::Array(narrowed));
}

#[cfg(feature = "commercial_plugins")]
/// 过滤 Ark List 响应：仅保留 Id 属于 owned 的条目，并校正 TotalCount
fn filter_ark_list_result(res: &mut serde_json::Value, owned: &HashSet<String>, owned_total: i64) {
    let Some(result) = res.get_mut("Result") else {
        return;
    };
    for key in ["Items", "Assets", "AssetGroups"] {
        if let Some(arr) = result.get_mut(key).and_then(|v| v.as_array_mut()) {
            arr.retain(|item| {
                item.get("Id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| owned.contains(id))
            });
            if let Some(obj) = result.as_object_mut() {
                obj.insert("TotalCount".to_string(), serde_json::json!(owned_total));
            }
            return;
        }
    }
}

#[cfg(feature = "commercial_plugins")]
/// 构造空的 List 成功响应（用户无本地归属时短路，避免泄露全量）
fn empty_ark_list_response(action: &str, version: &str, region: &str) -> Response {
    let body = serde_json::json!({
        "ResponseMetadata": {
            "Action": action,
            "Version": version,
            "Service": "ark",
            "Region": region,
        },
        "Result": {
            "Items": [],
            "TotalCount": 0,
        }
    });
    Response::builder()
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap()
}

#[cfg(feature = "commercial_plugins")]
/// 校验资源归属；无任何本地记录时补录当前用户（兼容历史 API 数据）
async fn ensure_ark_owned_or_claim(
    state: &AppState,
    kind: &str, // "asset" | "group"
    id: &str,
    user_id: &str,
    plugin_ns: &str,
) -> AppResult<()> {
    let (owned_sql, any_sql, claim_sql, forbidden_msg) = match kind {
        "asset" => (
            "SELECT COUNT(*) FROM plugin_assets WHERE asset_id = ? AND user_id = ? AND plugin_ns = ?",
            "SELECT COUNT(*) FROM plugin_assets WHERE asset_id = ? AND plugin_ns = ?",
            "INSERT INTO plugin_assets (user_id, asset_type, source, status, file_name, file_url, asset_id, category, plugin_ns) \
             VALUES (?, 'unknown', 'api_proxy', 'approved', 'unknown', '', ?, 'API素材', ?)",
            "无权操作此素材，仅可操作自己创建的素材",
        ),
        _ => (
            "SELECT COUNT(*) FROM plugin_asset_groups WHERE group_id = ? AND user_id = ? AND plugin_ns = ?",
            "SELECT COUNT(*) FROM plugin_asset_groups WHERE group_id = ? AND plugin_ns = ?",
            "INSERT INTO plugin_asset_groups (user_id, group_id, name, plugin_ns) VALUES (?, ?, '历史数据', ?)",
            "无权操作此素材组，仅可操作自己创建的素材组",
        ),
    };

    let owned: i64 = sqlx::query_scalar(&state.db.format_query(owned_sql))
        .bind(id)
        .bind(user_id)
        .bind(plugin_ns)
        .fetch_one(&state.db.pool)
        .await
        .unwrap_or(0);
    if owned > 0 {
        return Ok(());
    }

    let any_exists: i64 = sqlx::query_scalar(&state.db.format_query(any_sql))
        .bind(id)
        .bind(plugin_ns)
        .fetch_one(&state.db.pool)
        .await
        .unwrap_or(0);
    if any_exists > 0 {
        return Err(AppError::Forbidden(forbidden_msg.into()));
    }

    let _ = sqlx::query(&state.db.format_query(claim_sql))
        .bind(user_id)
        .bind(id)
        .bind(plugin_ns)
        .execute(&state.db.pool)
        .await;
    Ok(())
}

pub async fn ark_asset_proxy(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    Query(params): Query<HashMap<String, String>>,
    #[cfg_attr(not(feature = "commercial_plugins"), allow(unused_mut))] Json(mut body): Json<
        serde_json::Value,
    >,
) -> AppResult<Response> {
    #[cfg(feature = "commercial_plugins")]
    {
        // 1. 获取 Action (兼容 Action/action 和 Version/version)
        let action = params
            .get("Action")
            .or_else(|| params.get("action"))
            .cloned()
            .unwrap_or_default();
        let version = params
            .get("Version")
            .or_else(|| params.get("version"))
            .cloned()
            .unwrap_or_else(|| "2024-01-01".to_string());

        // 通过 ns 参数指定插件命名空间，默认 asset_manager（国内版）
        let plugin_ns = params
            .get("ns")
            .map(|s| s.as_str())
            .filter(|s| *s == "asset_manager" || *s == "asset_manager_intl")
            .unwrap_or("asset_manager");

        // 2. 白名单检查
        if !ARK_ASSET_ACTIONS.contains(&action.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Unsupported Ark Asset Action: {}",
                action
            )));
        }

        if !crate::api::plugins::is_plugin_compiled(plugin_ns) {
            return Err(AppError::Forbidden(format!(
                "素材资产管理插件({})未安装或未启用",
                plugin_ns
            )));
        }

        // 3. 用户及插件等级权限检查（仅投影鉴权必须的字段，避免查全表大文本与未使用的等级名）
        let (user_id, user_role, user_group, user_level_id): (String, String, String, Option<i64>) =
            sqlx::query_as(&state.db.format_query(
                "SELECT u.id, u.role, u.user_group, ul.id as level_id FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?"
            ))
            .bind(&token.user_id)
            .fetch_optional(&state.db.pool)
            .await?
            .ok_or_else(|| AppError::Unauthorized)?;

        if user_role != "admin" {
            // 检查目标插件是否开启并授权当前用户等级
            let plugin_info: Option<(i64, String)> = sqlx::query_as(&state.db.format_query(
                "SELECT is_enabled, allowed_levels FROM plugins WHERE name = ? AND is_enabled = 1",
            ))
            .bind(plugin_ns)
            .fetch_optional(&state.db.pool)
            .await?;

            if plugin_info.is_none() {
                return Err(AppError::Forbidden(format!(
                    "素材资产管理插件({})未安装或未启用",
                    plugin_ns
                )));
            }

            if let Some((_, ref allowed_levels)) = plugin_info {
                if allowed_levels != "all" {
                    let allowed: Vec<&str> = allowed_levels.split(',').collect();
                    let level_id_str = user_level_id.unwrap_or(0).to_string();
                    if !allowed.contains(&user_group.as_str())
                        && !allowed.contains(&level_id_str.as_str())
                    {
                        return Err(AppError::Forbidden(
                            "您当前的用户等级无权使用素材资产管理功能".to_string(),
                        ));
                    }
                }
            }

            // 进一步检查 API 接口开放状态（按等级 / 按用户）
            let configs = crate::api::plugins::load_plugin_configs_pub(&state, plugin_ns)
                .await
                .unwrap_or_default();
            if !crate::api::plugins::is_asset_api_enabled(
                &configs,
                &user_id,
                &user_group,
                user_level_id,
            ) {
                return Err(AppError::Forbidden(
                    "您当前无权使用素材 API 接口调用".to_string(),
                ));
            }
        }

        // 4. 获取火山引擎配置（根据 plugin_ns 加载对应的凭证）
        let volc_config = crate::api::plugins::get_volc_config(&state, plugin_ns)
            .await
            .ok_or_else(|| {
                AppError::BadRequest(format!("系统未配置素材管理凭证({})", plugin_ns))
            })?;

        // 强制设置 ProjectName 为系统配置项，覆盖用户可能传的值，确保资产数据在系统统一管理下
        if let Some(obj) = body.as_object_mut() {
            if !volc_config.project_name.is_empty() {
                obj.insert(
                    "ProjectName".to_string(),
                    serde_json::Value::String(volc_config.project_name.clone()),
                );
            } else {
                obj.insert(
                    "ProjectName".to_string(),
                    serde_json::Value::String("default".to_string()),
                );
            }
            // Create/Update Asset(Group) 的 Name 上限 64 字符
            if let Some(serde_json::Value::String(n)) = obj.get_mut("Name") {
                match action.as_str() {
                    "CreateAsset" | "UpdateAsset" => {
                        *n = crate::services::volcengine::clamp_create_asset_name(n);
                    }
                    "CreateAssetGroup" | "UpdateAssetGroup" => {
                        *n = crate::services::volcengine::clamp_ark_name(n);
                    }
                    _ => {}
                }
            }
        }

        // 预提取请求体关键字段（body 将在转发时被消费，需提前保存）
        let body_id = body
            .get("Id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let body_group_id = body
            .get("GroupId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let body_asset_type = body
            .get("AssetType")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase());
        let body_url = body
            .get("URL")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let body_name = body
            .get("Name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let body_desc = body
            .get("Description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 5. 用户归属校验 + List 预过滤准备（admin 跳过隔离）
        let mut list_owned_ids: Option<HashSet<String>> = None;
        let mut list_owned_total: i64 = 0;
        if user_role != "admin" {
            match action.as_str() {
                "GetAsset" | "UpdateAsset" | "DeleteAsset" => {
                    let id = body_id
                        .as_deref()
                        .ok_or_else(|| AppError::BadRequest("缺少必需参数 Id".into()))?;
                    ensure_ark_owned_or_claim(&state, "asset", id, &token.user_id, plugin_ns)
                        .await?;
                }
                "GetAssetGroup" | "UpdateAssetGroup" | "DeleteAssetGroup" => {
                    let id = body_id
                        .as_deref()
                        .ok_or_else(|| AppError::BadRequest("缺少必需参数 Id".into()))?;
                    ensure_ark_owned_or_claim(&state, "group", id, &token.user_id, plugin_ns)
                        .await?;
                }
                "CreateAsset" => {
                    // 只能写入自己拥有的素材组
                    let gid = body_group_id
                        .as_deref()
                        .ok_or_else(|| AppError::BadRequest("缺少必需参数 GroupId".into()))?;
                    ensure_ark_owned_or_claim(&state, "group", gid, &token.user_id, plugin_ns)
                        .await?;
                }
                "ListAssets" => {
                    let owned_assets = load_owned_ark_ids(
                        &state,
                        "plugin_assets",
                        "asset_id",
                        &token.user_id,
                        plugin_ns,
                    )
                    .await;
                    if owned_assets.is_empty() {
                        return Ok(empty_ark_list_response(
                            &action,
                            &version,
                            &volc_config.region,
                        ));
                    }
                    let owned_groups = load_owned_ark_ids(
                        &state,
                        "plugin_asset_groups",
                        "group_id",
                        &token.user_id,
                        plugin_ns,
                    )
                    .await;
                    narrow_list_filter_group_ids(&mut body, &owned_groups);
                    list_owned_total = owned_assets.len() as i64;
                    list_owned_ids = Some(owned_assets);
                }
                "ListAssetGroups" => {
                    let owned_groups = load_owned_ark_ids(
                        &state,
                        "plugin_asset_groups",
                        "group_id",
                        &token.user_id,
                        plugin_ns,
                    )
                    .await;
                    if owned_groups.is_empty() {
                        return Ok(empty_ark_list_response(
                            &action,
                            &version,
                            &volc_config.region,
                        ));
                    }
                    list_owned_total = owned_groups.len() as i64;
                    list_owned_ids = Some(owned_groups);
                }
                _ => {}
            }
        }

        let region = volc_config.region.clone();
        let client = crate::services::volcengine::VolcClient::new(volc_config)
            .with_logger(state.db.clone(), token.user_id.clone())
            .with_source("api_proxy")
            .with_plugin_name(plugin_ns);

        // 6. 转发请求并处理响应
        match client
            .call_api::<_, serde_json::Value>("ark", &region, &action, &version, body)
            .await
        {
            Ok(mut res) => {
                // List：按本地归属过滤，防止共享 ProjectName 下泄露他人素材
                if let Some(ref owned) = list_owned_ids {
                    filter_ark_list_result(&mut res, owned, list_owned_total);
                }

                let res_bytes = serde_json::to_vec(&res).unwrap_or_default();

                // 后置处理：Create 写入本地归属记录 / Delete 清理本地记录
                match action.as_str() {
                    "CreateAsset" => {
                        if let Some(aid) = res.pointer("/Result/Id").and_then(|v| v.as_str()) {
                            let fname = body_url
                                .as_deref()
                                .and_then(|url| url.rsplit('/').next())
                                .unwrap_or("unknown");
                            let at = body_asset_type.as_deref().unwrap_or("image");
                            let _ = sqlx::query(&state.db.format_query(
                                "INSERT INTO plugin_assets (user_id, asset_type, source, status, file_name, file_url, asset_id, category, plugin_ns) \
                                 VALUES (?, ?, 'api_proxy', 'approved', ?, ?, ?, 'API素材', ?)"
                            ))
                            .bind(&token.user_id)
                            .bind(at)
                            .bind(fname)
                            .bind(body_url.as_deref().unwrap_or(""))
                            .bind(aid)
                            .bind(plugin_ns)
                            .execute(&state.db.pool)
                            .await;
                        }
                    }
                    "CreateAssetGroup" => {
                        if let Some(gid) = res.pointer("/Result/Id").and_then(|v| v.as_str()) {
                            let name = body_name.as_deref().unwrap_or("未命名");
                            let _ = sqlx::query(&state.db.format_query(
                                "INSERT INTO plugin_asset_groups (user_id, group_id, name, description, plugin_ns) VALUES (?, ?, ?, ?, ?)"
                            ))
                            .bind(&token.user_id)
                            .bind(gid)
                            .bind(name)
                            .bind(body_desc.as_deref())
                            .bind(plugin_ns)
                            .execute(&state.db.pool)
                            .await;
                        }
                    }
                    "DeleteAsset" => {
                        if let Some(id) = body_id.as_deref() {
                            let _ = sqlx::query(&state.db.format_query(
                                "DELETE FROM plugin_assets WHERE asset_id = ? AND user_id = ? AND plugin_ns = ? AND source = 'api_proxy'"
                            ))
                            .bind(id)
                            .bind(&token.user_id)
                            .bind(plugin_ns)
                            .execute(&state.db.pool)
                            .await;
                        }
                    }
                    "DeleteAssetGroup" => {
                        if let Some(id) = body_id.as_deref() {
                            let _ = sqlx::query(&state.db.format_query(
                                "DELETE FROM plugin_asset_groups WHERE group_id = ? AND user_id = ? AND plugin_ns = ?"
                            ))
                            .bind(id)
                            .bind(&token.user_id)
                            .bind(plugin_ns)
                            .execute(&state.db.pool)
                            .await;
                        }
                    }
                    _ => {}
                }

                Ok(Response::builder()
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(res_bytes))
                    .unwrap())
            }
            Err(e) => {
                crate::relay_debug!(
                    "[ArkAssetProxy] {} 失败 命名空间={}: {}",
                    action,
                    plugin_ns,
                    e
                );
                Err(AppError::UpstreamError(e.to_string()))
            }
        }
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = state;
        let _ = token;
        let _ = params;
        let _ = body;
        Err(AppError::Forbidden("素材资产管理插件未安装".to_string()))
    }
}

// ═══════════════════════════════════════════════════════════════
//  Volcengine: DELETE /api/v3/contents/generations/tasks/{task_id}
//  取消/删除火山方舟视频生成任务（用户级鉴权隔离）
// ═══════════════════════════════════════════════════════════════

/// DELETE /api/v3/contents/generations/tasks/{task_id}
/// 通过 api_key_middleware 鉴权后，校验 task_id 归属当前用户，转发 DELETE 到上游火山方舟。
/// 若任务处于冻结状态（pending），成功取消后自动退还预扣费。
pub async fn volcengine_task_cancel(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    Path(task_id): Path<String>,
) -> AppResult<Response> {
    // 1. 从 logs 表查找任务记录，同时校验归属当前用户（含 channel_config_id 供 HA 子配还原）
    type NativeCancelLogRow = (
        i64,
        i64,
        Option<String>,
        String,
        Option<i64>,
        f64,
        f64,
        Option<i64>,
    );
    let log_row: Option<NativeCancelLogRow> =
        sqlx::query_as(&state.db.format_query(
            "SELECT id, channel_id, billing_detail, COALESCE(endpoint, ''), channel_config_id, cost, pre_deduct_gift, token_id \
             FROM logs WHERE task_id = ? AND user_id = ? \
             ORDER BY id DESC LIMIT 1",
        ))
        .bind(&task_id)
        .bind(&token.user_id)
        .fetch_optional(&state.db.pool)
        .await?;

    let (log_id, channel_id, billing_detail, endpoint, log_cfg_id, pre_deduction, pre_deduct_gift, token_id_opt) = match log_row {
        Some(r) => r,
        None => {
            return Err(AppError::Forbidden(
                "无权操作此任务或任务不存在".to_string(),
            ))
        }
    };

    // 2. 校验是否为火山方舟视频任务（endpoint 含火山原生路径标识）
    if !endpoint.contains("contents/generations") {
        return Err(AppError::BadRequest(
            "此任务不支持取消操作，仅火山方舟视频任务可取消".to_string(),
        ));
    }

    // 3. 与选渠/轮询同源水合（channel_config_id 还原 HA 子配）
    let channel = super::router::fetch_channel(&state, channel_id, log_cfg_id)
        .await
        .ok_or_else(|| AppError::BadRequest("任务对应的渠道不存在或已被删除".to_string()))?;

    // 4. 构建上游 DELETE 请求
    let url = join_url(
        &channel.base_url,
        &format!("/api/v3/contents/generations/tasks/{}", task_id),
    );
    crate::relay_debug!(
        "[Volcengine Cancel] 用户ID={} 任务ID={} URL={}",
        token.user_id,
        task_id,
        url
    );

    let resp = state
        .http_client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", channel.api_key))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| AppError::UpstreamError(format!("请求上游失败: {}", e)))?;

    let status = resp.status().as_u16();
    if !resp.status().is_success() && status != 404 {
        let err_body = resp.text().await.unwrap_or_default();
        crate::relay_debug!(
            "[Volcengine Cancel] 上游返回错误 状态码={} 响应体={}",
            status,
            err_body
        );
        // 提取上游错误信息供用户查看
        let detail = serde_json::from_str::<serde_json::Value>(&err_body)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .or(v.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("上游返回错误状态码: {}", status));
        return Err(AppError::UpstreamError(detail));
    }

    // 5. 取消成功后处理本地日志和退费
    let is_frozen = billing_detail
        .as_deref()
        .map_or(false, |d| d.contains("冻结"));
    if is_frozen {
        let failure_meta = super::task::SettleFailureMeta {
            cost: pre_deduction,
            pre_deduct_gift,
            user_id: &token.user_id,
            token_id: token_id_opt,
            channel_id: Some(channel_id),
            channel_config_id: log_cfg_id,
        };
        super::task::execute_refund_tx(
            &state,
            log_id,
            &failure_meta,
            "用户主动取消任务，预扣费已退回",
            499,
            Some("用户主动取消任务"),
        )
        .await;
        crate::relay_debug!(
            "[Volcengine Cancel] 预扣费已退还 日志ID={} 退款金额={:.6}",
            log_id,
            pre_deduction
        );
    } else {
        // 任务已结算（succeeded/failed），仅更新日志标记
        let _ =
            sqlx::query(&state.db.format_query(
                "UPDATE logs SET error_message = '用户已删除此任务记录' WHERE id = ?",
            ))
            .bind(log_id)
            .execute(&state.db.pool)
            .await;
    }

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({"message": "任务已取消"}).to_string(),
        ))
        .unwrap())
}

// ═══════════════════════════════════════════════════════════════
//  Volcengine: GET /api/v3/contents/generations/tasks
//  查询火山方舟视频任务列表（用户级鉴权隔离）
// ═══════════════════════════════════════════════════════════════

/// GET /api/v3/contents/generations/tasks
/// 先从 logs 表查出当前用户近 7 天的火山方舟视频 task_id，
/// 作为 filter.task_ids 附加到上游请求中，确保严格的用户隔离。
pub async fn volcengine_task_list(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Response> {
    // 1. 从 logs 表查出当前用户近 7 天的火山方舟视频 task_id
    // 使用 action_type = '视频' + upstream_url 含火山原生路径，严格限定火山方舟任务
    // 避免混入腾讯云等其他厂商的 task_id（它们的接口协议不同）
    let task_ids: Vec<String> = sqlx::query_scalar(&state.db.format_query(
        "SELECT DISTINCT task_id FROM logs \
             WHERE user_id = ? AND task_id IS NOT NULL AND task_id != '' \
             AND action_type = '视频' AND upstream_url LIKE '%contents/generations%' \
             AND created_at > CURRENT_TIMESTAMP - INTERVAL '7 days' \
             ORDER BY task_id DESC LIMIT 500",
    ))
    .bind(&token.user_id)
    .fetch_all(&state.db.pool)
    .await?;

    if task_ids.is_empty() {
        // 无任务记录，直接返回空列表
        return Ok(Response::builder()
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(
                serde_json::json!({
                    "items": [],
                    "total": 0,
                    "page_num": 1,
                    "page_size": 20
                })
                .to_string(),
            ))
            .unwrap());
    }

    // 2. 最近一条火山视频日志：channel_id + channel_config_id 还原 HA 子配
    let ch_row: Option<(i64, Option<i64>)> = sqlx::query_as(&state.db.format_query(
        "SELECT channel_id, channel_config_id FROM logs \
             WHERE user_id = ? AND task_id IS NOT NULL AND task_id != '' \
             AND action_type = '视频' AND upstream_url LIKE '%contents/generations%' \
             AND created_at > CURRENT_TIMESTAMP - INTERVAL '7 days' \
             ORDER BY id DESC LIMIT 1",
    ))
    .bind(&token.user_id)
    .fetch_optional(&state.db.pool)
    .await?;

    let (cid, log_cfg_id) =
        ch_row.ok_or_else(|| AppError::BadRequest("未找到可用的视频渠道".to_string()))?;
    let channel = super::router::fetch_channel(&state, cid, log_cfg_id)
        .await
        .ok_or_else(|| AppError::BadRequest("渠道不存在或已被删除".to_string()))?;

    // 3. 构建上游请求 URL（附加用户的 task_ids 作为过滤条件）
    let mut query_parts: Vec<String> = Vec::new();

    // 用户传入的查询参数透传（排除 task_ids，由系统控制；参数值 URL 编码防注入）
    for (k, v) in &params {
        if k != "filter.task_ids" {
            query_parts.push(format!("{}={}", k, urlencoding::encode(v)));
        }
    }

    // 注入当前用户的 task_ids（确保只能查到自己的任务）
    for tid in &task_ids {
        query_parts.push(format!("filter.task_ids={}", tid));
    }

    let qs = query_parts.join("&");
    let url = format!(
        "{}?{}",
        join_url(&channel.base_url, "/api/v3/contents/generations/tasks"),
        qs
    );
    crate::relay_debug!(
        "[Volcengine TaskList] 用户ID={} 任务数={} URL长度={}",
        token.user_id,
        task_ids.len(),
        url.len()
    );

    // 4. 转发到上游
    let resp = state
        .http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", channel.api_key))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| AppError::UpstreamError(format!("请求上游失败: {}", e)))?;

    let upstream_hdrs = resp.headers().clone();
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let err_body = resp.text().await.unwrap_or_default();
        return Err(proxy::upstream_fail(
            status,
            &err_body,
            Some(upstream_hdrs),
            "/api/v3/contents/generations/tasks",
        ));
    }

    let body = resp.bytes().await.unwrap_or_default();
    let log_id = upstream_headers::resolve_request_log_id(Some("任务"));
    Ok(upstream_headers::with_request_id(
        upstream_headers::json_with_upstream_headers(&upstream_hdrs, body),
        &log_id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_gemini_image_params() {
        // 1. 驼峰 imageConfig
        let camel = serde_json::json!({
            "generationConfig": {
                "imageConfig": { "aspectRatio": "16:9", "imageSize": "2K" }
            }
        });
        assert!(has_gemini_image_params(&camel));

        // 2. 下划线 image_config
        let snake = serde_json::json!({
            "generation_config": {
                "image_config": { "aspect_ratio": "16:9", "image_size": "2K" },
                "thinking_config": { "thinking_level": "MEDIUM" }
            }
        });
        assert!(has_gemini_image_params(&snake));

        // 3. OpenAI 兼容生图参数（带 prompt 且无 messages）
        let openai_img = serde_json::json!({
            "prompt": "Draw a cheeseburger",
            "size": "1024x1024"
        });
        assert!(has_gemini_image_params(&openai_img));

        // 4. OpenAI 兼容聊天参数（带 messages）不被误判为生图
        let openai_chat = serde_json::json!({
            "messages": [{"role": "user", "content": "hello"}]
        });
        assert!(!has_gemini_image_params(&openai_chat));

        // 5. 多模态对话（无 image_config）不被误判
        let chat_multimodal = serde_json::json!({
            "contents": [{"parts": [{"text": "hello"}]}],
            "generation_config": {
                "response_modalities": ["TEXT", "IMAGE"]
            }
        });
        assert!(!has_gemini_image_params(&chat_multimodal));

        // 6. 普通纯文本
        let chat_plain = serde_json::json!({
            "contents": [{"parts": [{"text": "hello"}]}]
        });
        assert!(!has_gemini_image_params(&chat_plain));
    }
}



