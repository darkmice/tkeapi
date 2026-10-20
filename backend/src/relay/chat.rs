/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

// ── 聊天 & Responses API 处理 ──────────────────────────────────
// 统一管理 Chat Completions 和 Responses API 的请求处理逻辑

use super::{
    forward, proxy, response_formatter, router, stream, upstream_headers, usage_extractor,
};
use crate::error::{AppError, AppResult};
use crate::models::ApiToken;
use crate::AppState;
use axum::{
    extract::{Extension, OriginalUri, State},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

// ── Chat Completions (/v1/chat/completions) ──────────────────

pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    OriginalUri(uri): OriginalUri,
    request: axum::http::Request<axum::body::Body>,
) -> AppResult<Response> {
    relay_chat(state, token, uri, request, false).await
}

// ── Responses API (/v1/responses, /api/v3/responses) ─────────

pub async fn responses_create(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    OriginalUri(uri): OriginalUri,
    request: axum::http::Request<axum::body::Body>,
) -> AppResult<Response> {
    relay_chat(state, token, uri, request, true).await
}

/// 统一 Chat Completions / Responses 中继：共用渠道选择与重试逻辑；`is_responses` 区分格式转换、流式处理与日志前缀。
async fn relay_chat(
    state: Arc<AppState>,
    token: ApiToken,
    uri: axum::http::Uri,
    request: axum::http::Request<axum::body::Body>,
    is_responses: bool,
) -> AppResult<Response> {
    let req_start = std::time::Instant::now();
    let (parts, body_stream) = request.into_parts();
    let client_headers = parts.headers;
    let raw_from_ext = parts
        .extensions
        .get::<super::RawRequestContent>()
        .map(|r| r.0.clone());

    let bytes = axum::body::to_bytes(body_stream, 50 * 1024 * 1024)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read request body: {}", e)))?;

    let raw_path = uri.path();
    let request_content_str = raw_from_ext
        .unwrap_or_else(|| String::from_utf8_lossy(&bytes).into_owned());
    let body: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON: {}", e)))?;

    let model = body["model"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("Missing required parameter: model".to_string()))?;
    let is_stream = body["stream"].as_bool().unwrap_or(false);
    let log_id = upstream_headers::resolve_request_log_id(Some("聊天"));

    let ctx = proxy::get_user_context(&state, &token.user_id).await?;
    proxy::check_model_permission(&state, &token, model, raw_path, Some("聊天")).await?;

    // 【一条日志原则】HA 重试复用同一条 pending，避免产生多条
    let mut ha = crate::relay::ha::HaAttempt::begin(&state, token.high_availability, req_start).await;
    let mut billing_rule_cache = None;
    let mut access_cache = None;

    while ha.cont() {
        let attempt_start = std::time::Instant::now();

        // 1. 选择渠道
        let mut ha_pool = None;
        let channel = match proxy::select_channel_for_model(
            &state,
            &token,
            model,
            &ctx.user_group,
            &ctx.level_id,
            raw_path,
            &ha.exclude_aids,
            !ha.had_upstream,
            Some("聊天"),
            &mut ha_pool,
        )
        .await
        {
            Ok(c) => {
                ha.note_pool(ha_pool);
                ha.on_channel(&c);
                c
            }
            Err(e) => {
                ha.note_pool(ha_pool);
                ha.on_select_err(e);
                break;
            }
        };

        // 2. 预扣费检查（带 channel 精确匹配同名模型；同时获取 Model 供下游复用）
        let (pre_deduction, db_model, resolved_cat) = match proxy::check_access_with_model(
            &state,
            &token,
            model,
            &ctx,
            Some("聊天"),
            &channel,
            None,
            &mut access_cache,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                ha.on_access_err(e);
                break;
            }
        };

        // 模型映射：聊天/Responses 无分辨率档，跳过 body 解析
        let (resolved_model, mapping_source) =
            router::resolve_model(&channel, model, db_model.as_ref(), None);

        // 3. 解析转发规则（复用 db_model 避免重查 models 表）
        let resolved = match forward::resolve_forward_rule(
            &state,
            model,
            &resolved_cat,
            raw_path,
            Some(&channel),
            db_model.as_ref(),
        )
        .await
        {
            Some(r) => r,
            None => {
                if raw_path == "/api/v1/services/aigc/multimodal-generation/generation" {
                    // 本路径即官方上游，原样透传（勿套用 /v1/chat/completions 兼容规则）
                    forward::default_openai_forward(raw_path)
                } else if forward::model_has_forward_rules(&state, model).await {
                    let err = forward::record_unsupported_forward_error(
                        &state, &token, &channel, model, raw_path, "聊天",
                    )
                    .await;
                    ha.on_access_err(err);
                    break;
                } else if is_responses {
                    // 未配置规则的模型，默认按 OpenAI Responses 规范原样透传
                    forward::default_openai_forward(raw_path)
                } else {
                    forward::infer_forward_from_base_url(
                        &channel.base_url,
                        &resolved_cat,
                        db_model.as_ref(),
                    )
                }
            }
        };

        let mut db_rule = proxy::get_model_billing_rule(
            &state,
            model,
            Some(&channel),
            db_model.as_ref(),
            &mut billing_rule_cache,
        )
        .await;

        // 上游请求体：统一走格式转换与后处理（涵盖 Chat、Responses 及三协议原生透传）
        let mut upstream_body: serde_json::Value = forward::transform_request_body(
            &resolved,
            &resolved_model,
            &body,
            "聊天",
            db_rule.as_ref(),
            Some(&state.http_client),
        )
        .await;

        let mut url = forward::build_upstream_url(
            &channel.base_url,
            &resolved,
            &resolved_model,
            &channel.api_key,
        );
        let auth_headers = forward::build_auth_headers(&resolved, &channel.api_key, true);
        let mut resolved_upstream_path =
            resolved.upstream_path.replace("${model}", &resolved_model);

        let target_type = resolved.target_type.clone();
        let log_tag = if is_responses {
            "[Responses]"
        } else {
            "[Chat]"
        };
        // Completions：转发规则含 streamGenerateContent 时上游始终 SSE
        let is_stream = is_stream
            || (!is_responses && resolved.upstream_path.contains("streamGenerateContent"));

        // Gemini Completions 流式走 URL :streamGenerateContent?alt=sse，body 不带 stream（Interactions 路由除外）
        let gemini_stream = is_stream
            && !is_responses
            && target_type == "gemini"
            && !forward::is_google_interactions_path(&resolved_upstream_path);
        if is_stream && !gemini_stream {
            upstream_body["stream"] = serde_json::json!(true);
        }
        if gemini_stream {
            resolved_upstream_path = resolved_upstream_path
                .replace(":generateContent", ":streamGenerateContent")
                + "?alt=sse";
            url = super::url_utils::join_url(&channel.base_url, &resolved_upstream_path);
            if resolved.auth_type == "query_key" {
                if url.contains('?') {
                    url = format!("{}&key={}", url, channel.api_key);
                } else {
                    url = format!("{}?key={}", url, channel.api_key);
                }
            }
        }

        crate::relay_debug!(
            "{} 尝试={} 模型={} 目标类型={} 鉴权={} 地址={} 渠道id={}",
            log_tag,
            ha.attempt,
            model,
            target_type,
            resolved.auth_type,
            url,
            channel.id
        );

        let masked_url = forward::mask_key_in_string(&url, &channel.api_key);
        let ep = format!("{}|{}", raw_path, masked_url);
        let stream_log_path = if is_stream {
            masked_url.clone()
        } else {
            resolved_upstream_path.clone()
        };

        if ha.pending_log_id.is_none() {
            ha.set_pending(
                proxy::record_pending_log(proxy::PendingLog {
                    state: &state,
                    user_id: &token.user_id,
                    token_id: token.id,
                    model,
                    endpoint: &ep,
                    is_stream: if is_stream { 1 } else { 0 },
                    request_content: Some(&request_content_str),
                    upstream_url: Some(&url),
                    channel: &channel,
                    plugin_tag: None,
                    category: Some("聊天"),
                    db_model: db_model.as_ref(),
                    forward_eid: Some(&resolved.eid),
                    requested_log_id: Some(&log_id),
                    task_id: None,
                })
                .await,
            );
        }

        let bill_ctx = crate::relay::ha::HaBillCtx::new(&state, &token, model, &ep)
            .category("聊天")
            .db(db_model.as_ref());
        let pending_log_id = ha.pending_log_id;
        let timeout_ctx = ha.timeout_ctx();
        let fail_buf = ha.buf();
        let prompt_tokens = estimate_prompt_tokens(&body);

        // 【连接保护】打上游 → 预扣 → 记账；拼响应体在 task 外
        let result_rx = super::spawn_protected({
            let (state, token, ctx) = (state.clone(), token.clone(), ctx.clone());
            let (channel, url, ep, db_model) =
                (channel.clone(), url.clone(), ep.clone(), db_model.clone());
            let (model, raw_path, target_type) =
                (model.to_string(), raw_path.to_string(), target_type.clone());
            let (body, client_headers, log_id) =
                (body.clone(), client_headers.clone(), log_id.clone());
            let request_content_str = request_content_str.clone();
            async move {
                // 客户端安全头 → 渠道鉴权 → Content-Type（后写覆盖，禁止 sk 泄漏）
                let builder = upstream_headers::apply_client_request_headers(
                    state.http_client.post(&url),
                    &client_headers,
                    &log_id,
                );
                let builder = auth_headers
                    .into_iter()
                    .fold(builder, |b, (k, v)| b.header(k, v));
                let builder = builder.header("Content-Type", "application/json");
                let builder = crate::services::http_client::with_timeout_if(
                    builder,
                    !is_stream,
                    timeout_ctx.resolve(),
                );
                let resp = match timeout_ctx
                    .send(builder.json(&upstream_body), is_stream)
                    .await
                {
                    Ok(r) => r,
                    Err(err_msg) => {
                        crate::relay_debug!("{} 连接错误: {}", log_tag, err_msg);
                        let bill = crate::relay::ha::FailBill::transport(
                            attempt_start.elapsed().as_millis() as u32,
                            err_msg.clone(),
                            &request_content_str,
                            upstream_body.to_string(),
                        )
                        .content(None)
                        .client(err_msg)
                        .stream(if is_stream { 1 } else { 0 });
                        return Err(crate::relay::ha::HaAttempt::park(&fail_buf, bill, None));
                    }
                };

                if !resp.status().is_success() {
                    let status = resp.status().as_u16();
                    let upstream_hdrs = resp.headers().clone();
                    let err = resp.text().await.unwrap_or_default();
                    let display_err = proxy::upstream_error_text(status, &err);
                    crate::relay_debug!("{} 上游错误 {}: {}", log_tag, status, display_err);
                    let bill = crate::relay::ha::FailBill::http(
                        attempt_start.elapsed().as_millis() as u32,
                        status,
                        err.clone(),
                        &request_content_str,
                        upstream_body.to_string(),
                    )
                    .body(display_err.clone())
                    .content(Some(err))
                    .client(display_err)
                    .stream(if is_stream { 1 } else { 0 });
                    return Err(crate::relay::ha::HaAttempt::park(
                        &fail_buf,
                        bill,
                        Some(upstream_hdrs),
                    ));
                }

                let as_stream =
                    is_stream || upstream_headers::is_stream_content_type(resp.headers());
                if as_stream {
                    let pre_deduct_gift = proxy::pre_deduct_or_intercept(
                        &state,
                        &token,
                        &channel,
                        &model,
                        pre_deduction,
                        &ep,
                        req_start,
                        1,
                        &request_content_str,
                        &upstream_body.to_string(),
                        None,
                        pending_log_id,
                        db_model.as_ref(),
                        Some("聊天"),
                    )
                    .await?;
                    return Ok(super::ProtectOut::Live(
                        stream::handle_chat_stream(
                            state,
                            token,
                            channel,
                            model,
                            resp,
                            ctx,
                            prompt_tokens,
                            request_content_str,
                            req_start,
                            target_type,
                            stream_log_path,
                            Some(upstream_body.to_string()),
                            pre_deduction,
                            pre_deduct_gift,
                            raw_path,
                            None,
                            pending_log_id,
                            db_model,
                            db_rule,
                        )
                        .await
                        .into_response(),
                    ));
                }

                let headers = resp.headers().clone();
                let mut body_str = resp.text().await.unwrap_or_default();
                let (converted, post_err) = forward::check_upstream_post_error(
                    &target_type,
                    &body_str,
                    resolved_cat.as_str(),
                    false,
                );
                body_str = converted;
                if post_err.is_some() {
                    let err_text = proxy::extract_error_message(&body_str);
                    crate::relay_debug!("{} 上游响应体错误: {}", log_tag, err_text);
                    let bill = crate::relay::ha::FailBill::biz(
                        attempt_start.elapsed().as_millis() as u32,
                        body_str.clone(),
                        err_text,
                        &request_content_str,
                        upstream_body.to_string(),
                    )
                    .stream(0);
                    return Err(crate::relay::ha::HaAttempt::park(
                        &fail_buf,
                        bill,
                        Some(headers),
                    ));
                }

                let usage_tokens = usage_extractor::parse_usage(&body_str);
                let mut features =
                    usage_extractor::features_from_values(Some(&body), None, Some(&body_str));
                let pre_deduct_gift = proxy::pre_deduct_or_intercept(
                    &state,
                    &token,
                    &channel,
                    &model,
                    pre_deduction,
                    &ep,
                    req_start,
                    0,
                    &request_content_str,
                    &upstream_body.to_string(),
                    None,
                    pending_log_id,
                    db_model.as_ref(),
                    Some("聊天"),
                )
                .await?;
                let (quota_used, detail) = super::calculate_relay_cost(
                    &state,
                    db_model.as_ref(),
                    db_rule.as_mut(),
                    &channel,
                    &ctx,
                    &usage_tokens,
                    &mut features,
                    mapping_source.as_deref(),
                    &model,
                    &resolved_model,
                )
                .await;
                proxy::record_and_bill_inner(proxy::BillRecord {
                    ctx: crate::relay::ha::HaBillCtx::new(&state, &token, &model, &ep)
                        .category("聊天")
                        .db(db_model.as_ref()),
                    channel: &channel,
                    log_id: pending_log_id,
                    usage: usage_tokens,
                    cost: quota_used,
                    pre_deducted: pre_deduction,
                    pre_deduct_gift,
                    latency_ms: req_start.elapsed().as_millis() as u32,
                    is_stream: 0,
                    status_code: 200,
                    error_msg: None,
                    request: Some(request_content_str),
                    response: Some(body_str.clone()),
                    upstream_req: Some(upstream_body.to_string()),
                    detail: Some(detail),
                    features: Some(features),
                    time_multiplier: db_rule.as_ref().map(|r| r.applied_multiplier),
                    plugin_tag: None,
                    upstream_request_id:
                        crate::relay::upstream_headers::extract_upstream_request_id(&headers),
                })
                .await;
                Ok(super::ProtectOut::Raw(super::UpstreamRaw::new(
                    headers, body_str,
                )))
            }
        });

        match super::join_protected(&mut ha, result_rx, &bill_ctx, &channel, Some(&url)).await {
            super::ProtectJoin::Ok(super::ProtectOut::Raw(raw)) => {
                let ms = attempt_start.elapsed().as_millis() as u32;
                ha.ok(&state, &channel, &url, ms).await;
                let final_body = if is_responses
                    || forward::is_anthropic_messages_path(raw_path)
                    || forward::is_google_native_path(raw_path)
                {
                    raw.body
                } else {
                    transform_chat_response(&raw.body, &target_type, model)
                };
                let resp = upstream_headers::json_with_upstream_headers(&raw.headers, final_body);
                return Ok(upstream_headers::with_request_id(resp, &log_id));
            }
            super::ProtectJoin::Ok(super::ProtectOut::Live(resp)) => {
                let ms = attempt_start.elapsed().as_millis() as u32;
                ha.ok(&state, &channel, &url, ms).await;
                return Ok(upstream_headers::with_request_id(resp, &log_id));
            }
            super::ProtectJoin::Retry => {
                ha.bump();
                continue;
            }
            super::ProtectJoin::Stop => break,
        }
    }

    Err(ha
        .finish(&crate::relay::ha::HaBillCtx::new(&state, &token, model, raw_path).category("聊天"))
        .await)
}

// ── 公共辅助函数 ──────────────────────────────────────────────

fn count_content_chars(val: &serde_json::Value) -> usize {
    if let Some(s) = val.as_str() {
        return s.len();
    }
    if let Some(arr) = val.as_array() {
        return arr
            .iter()
            .map(|item| {
                if let Some(s) = item.as_str() {
                    s.len()
                } else if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                    t.len()
                } else if let Some(c) = item.get("content") {
                    count_content_chars(c)
                } else {
                    0
                }
            })
            .sum();
    }
    0
}

/// 粗略估算 prompt tokens（兼容 Chat 的 messages、Anthropic 的 system 和 Responses 的 input）
pub fn estimate_prompt_tokens(body: &serde_json::Value) -> i32 {
    let mut total_chars = 0;
    // Chat Completions / Anthropic Messages: messages 数组（支持 string 与多模态 array content）
    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            if let Some(c) = msg.get("content") {
                total_chars += count_content_chars(c);
            }
        }
    }
    // Anthropic 顶层 system 字段（string 或 content block array）
    if let Some(sys) = body.get("system") {
        total_chars += count_content_chars(sys);
    }
    // Responses API: input 字段（string 或 array）
    if let Some(input) = body.get("input") {
        total_chars += count_content_chars(input);
    }
    // instructions 字段
    if let Some(s) = body.get("instructions").and_then(|i| i.as_str()) {
        total_chars += s.len();
    }
    (total_chars as f64 / 4.0).ceil() as i32
}

/// 将上游非 OpenAI 格式响应转换为 OpenAI 格式
fn transform_chat_response(response: &str, target_type: &str, model: &str) -> String {
    match target_type {
        "anthropic" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
                let content = v
                    .get("content")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| {
                        arr.iter()
                            .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("text"))
                            .map(|c| c.get("text").and_then(|t| t.as_str()).unwrap_or(""))
                            .next()
                    })
                    .unwrap_or("");
                let usage_tokens = usage_extractor::parse_usage(response);
                let mut usage = serde_json::json!({
                    "prompt_tokens": usage_tokens.prompt,
                    "completion_tokens": usage_tokens.completion,
                    "total_tokens": usage_tokens.total,
                    "cache_creation": v.get("usage").and_then(|c| c.get("cache_creation"))
                });
                // 映射 Anthropic 缓存字段到 OpenAI prompt_tokens_details
                if usage_tokens.cached > 0 || usage_tokens.cache_creation > 0 {
                    usage["prompt_tokens_details"] = serde_json::json!({
                        "cached_tokens": usage_tokens.cached,
                        "cache_creation_tokens": usage_tokens.cache_creation
                    });
                }
                return serde_json::to_string(&serde_json::json!({
                    "id": v.get("id").and_then(|i| i.as_str()).unwrap_or(""),
                    "object": "chat.completion",
                    "created": chrono::Utc::now().timestamp(),
                    "model": model,
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
                    "usage": usage
                })).unwrap_or_else(|_| response.to_string());
            }
            response.to_string()
        }
        "gemini" | "gemini_image" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
                let mut parts_out = Vec::new();
                if let Some(parts) = v
                    .pointer("/candidates/0/content/parts")
                    .and_then(|p| p.as_array())
                {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()).filter(|s| !s.is_empty()) {
                            parts_out.push(text.to_string());
                        } else if let Some(inline) = part.get("inlineData").or_else(|| part.get("inline_data")) {
                            if let Some(data) = inline.get("data").and_then(|d| d.as_str()).filter(|s| !s.is_empty()) {
                                let mime = inline
                                    .get("mimeType")
                                    .or_else(|| inline.get("mime_type"))
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("image/png");
                                parts_out.push(format!("![image](data:{};base64,{})", mime, data));
                            }
                        }
                    }
                }
                let content = parts_out.join("\n\n");
                let finish = response_formatter::extract_finish_reason(&v).unwrap_or("stop");
                let usage = usage_extractor::gemini_usage_metadata_to_openai(
                    v.get("usageMetadata").unwrap_or(&serde_json::Value::Null),
                );
                return serde_json::to_string(&serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "object": "chat.completion",
                    "created": chrono::Utc::now().timestamp(),
                    "model": model,
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": finish}],
                    "usage": usage
                })).unwrap_or_else(|_| response.to_string());
            }
            response.to_string()
        }
        _ => {
            // 兜底：检测上游是否返回了 Anthropic 原生格式（type:"message"），自动转为 OpenAI 格式
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
                if v.get("type").and_then(|t| t.as_str()) == Some("message") {
                    return transform_chat_response(response, "anthropic", model);
                }
            }
            response.to_string()
        }
    }
}
