/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::models::{ApiToken, BillingRule, Channel, Model};
use crate::AppState;
use std::sync::Arc;

use axum::body::Bytes;
use axum::response::IntoResponse;
use futures::StreamExt;
use reqwest::Response as ReqwestResponse;
use tokio::sync::mpsc;

use super::upstream_headers;

/// 高效检测流式响应中是否包含上游业务错误帧（仅嗅探头尾关键行，高并发下长流 0 遍历开销）
pub fn detect_stream_error(raw_response_text: &str) -> Option<(u16, String)> {
    let s = raw_response_text.trim();
    if s.is_empty()
        || (!s.contains("\"error\"") && !s.contains("\"ErrorCode\"") && !s.contains("\"algo_msg\""))
    {
        return None;
    }

    // 1. 纯 JSON 报错响应（非 SSE）
    if s.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
            if crate::relay::response_formatter::is_upstream_error_response(&v) {
                return Some((
                    crate::relay::proxy::infer_error_status_code(&v),
                    crate::relay::proxy::extract_error_message(s),
                ));
            }
        }
    }

    // 2. 错误绝大多数只在首包或尾部；仅嗅探前 5 行与后 5 行，跳过中间成百上千行正常文本
    for line in s.lines().take(5).chain(s.lines().rev().take(5)) {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" || !data.contains("\"error\"") {
            continue;
        }
        let Some(start) = data.find('{') else {
            continue;
        };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data[start..]) {
            if crate::relay::response_formatter::is_upstream_error_response(&v) {
                let status = crate::relay::proxy::infer_error_status_code(&v);
                let msg = crate::relay::response_formatter::extract_error_message_from_value(&v)
                    .unwrap_or_else(|| crate::relay::proxy::extract_error_message(data));
                return Some((status, msg));
            }
        }
    }

    None
}

/// 流结束后统一结算：resolve_model → calculate_relay_cost → record_and_bill_inner
async fn settle_after_stream(
    state: &Arc<AppState>,
    token: &ApiToken,
    channel: &Channel,
    model: &str,
    db_model: Option<&Model>,
    db_rule: &mut Option<BillingRule>,
    ctx: &crate::relay::proxy::UserContext,
    mut usage: crate::relay::usage_extractor::UsageTokens,
    mut features: crate::relay::usage_extractor::ExtractedFeatures,
    detail_extra: Option<String>,
    start_time: std::time::Instant,
    entry_endpoint: &str,
    upstream_path: &str,
    pre_deducted: f64,
    pre_deduct_gift: f64,
    request_content: String,
    response_content: String,
    upstream_req_content: Option<String>,
    category: &str,
    pending_log_id: Option<i64>,
    stream_fail: Option<(u16, String)>,
    upstream_request_id: Option<String>,
) {
    let latency_ms = start_time.elapsed().as_millis() as u32;
    let ep = format!("{}|{}", entry_endpoint, upstream_path);

    let (status_code, error_msg, cost, detail) = if let Some((code, err)) = stream_fail {
        // 异常断流或上游业务报错：全额退还预扣款，用量清零不扣费
        usage.prompt = 0;
        usage.completion = 0;
        let detail_text = format!("流传输失败已退款 | {}", err);
        (code, Some(err), 0.0, detail_text)
    } else {
        let map_res = crate::relay::router::mapping_resolution(
            Some(category),
            features.resolution.as_deref(),
        );
        let (resolved_model, mapping_source) =
            crate::relay::router::resolve_model(channel, model, db_model, map_res);
        let (c, mut d) = crate::relay::calculate_relay_cost(
            state,
            db_model,
            db_rule.as_mut(),
            channel,
            ctx,
            &usage,
            &mut features,
            mapping_source.as_deref(),
            model,
            &resolved_model,
        )
        .await;
        if let Some(extra) = detail_extra {
            d.push_str(&extra);
        }
        (200, None, c, d)
    };

    crate::relay::proxy::record_and_bill_inner(crate::relay::proxy::BillRecord {
        ctx: crate::relay::ha::HaBillCtx::new(state, token, model, &ep)
            .category(category)
            .db(db_model),
        channel,
        log_id: pending_log_id,
        usage,
        cost,
        pre_deducted,
        pre_deduct_gift,
        latency_ms,
        is_stream: 1,
        status_code,
        error_msg: error_msg.as_deref(),
        request: Some(request_content),
        response: Some(response_content),
        upstream_req: upstream_req_content,
        detail: Some(detail),
        features: Some(features),
        time_multiplier: db_rule.as_ref().map(|r| r.applied_multiplier),
        plugin_tag: None,
        upstream_request_id,
    })
    .await;
}

/// SSE 原样透传：Anthropic Messages、Google Gemini 或 Responses（`/v1/responses`、`/api/v3/responses` 等）
fn sse_body_passthrough(entry: &str) -> bool {
    crate::relay::forward::is_anthropic_messages_path(entry)
        || crate::relay::forward::is_google_native_path(entry)
        || entry.ends_with("/responses")
}

/// 当客户端中途断开或上游未下发最终 usage 帧时，根据已接收的 SSE 增量文本估算 completion tokens
fn estimate_stream_completion_tokens(raw_response_text: &str) -> i32 {
    let mut total_bytes = 0usize;
    for line in raw_response_text.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
        // 1. OpenAI Chat Completions: choices[].delta.content / reasoning_content
        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(delta) = choice.get("delta") {
                    for key in ["content", "reasoning_content"] {
                        if let Some(s) = delta.get(key).and_then(|x| x.as_str()) {
                            total_bytes += s.len();
                        }
                    }
                }
            }
        }
        // 2. Anthropic Messages / Responses API: delta.text / delta.thinking / delta.partial_json 或字符串 delta
        if let Some(delta) = v.get("delta") {
            if let Some(s) = delta.as_str() {
                total_bytes += s.len();
            } else if delta.is_object() {
                for key in ["text", "thinking", "partial_json"] {
                    if let Some(s) = delta.get(key).and_then(|x| x.as_str()) {
                        total_bytes += s.len();
                    }
                }
            }
        }
        // 3. Google Gemini: candidates[].content.parts[].text
        if let Some(candidates) = v.get("candidates").and_then(|c| c.as_array()) {
            for cand in candidates {
                if let Some(parts) = cand
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    for part in parts {
                        if let Some(s) = part.get("text").and_then(|x| x.as_str()) {
                            total_bytes += s.len();
                        }
                    }
                }
            }
        }
    }
    if total_bytes > 0 {
        ((total_bytes as f64) / 4.0).ceil() as i32
    } else {
        0
    }
}

const MAX_STREAM_BUFFER_BYTES: usize = 4 * 1024 * 1024;
const STREAM_HEAD_KEEP_BYTES: usize = 1024 * 1024;

/// 限制单路流式响应内存累积上限（4MB），超出时按行边界保留首部（含 message_start 等起始帧）与最新尾部（含末尾 usage 帧），
/// 同时累加被裁剪中间段的估算 completion tokens，兼顾防 OOM 与计费零漏算。
fn append_stream_bounded(buf: &mut String, chunk: &str, overflow_tokens: &mut i32) {
    if buf.len().saturating_add(chunk.len()) <= MAX_STREAM_BUFFER_BYTES {
        buf.push_str(chunk);
        return;
    }
    buf.push_str(chunk);
    if buf.len() > MAX_STREAM_BUFFER_BYTES {
        let mut head_end = STREAM_HEAD_KEEP_BYTES.min(buf.len());
        while head_end < buf.len() && !buf.is_char_boundary(head_end) {
            head_end += 1;
        }
        if let Some(nl) = buf[head_end..].find('\n') {
            head_end += nl + 1;
        }
        let target_drop = buf.len().saturating_sub(MAX_STREAM_BUFFER_BYTES / 2);
        let mut drop_end = (head_end + target_drop).min(buf.len());
        while drop_end < buf.len() && !buf.is_char_boundary(drop_end) {
            drop_end += 1;
        }
        if let Some(nl) = buf[drop_end..].find('\n') {
            drop_end += nl + 1;
        }
        if drop_end > head_end && drop_end <= buf.len() {
            *overflow_tokens = overflow_tokens
                .saturating_add(estimate_stream_completion_tokens(&buf[head_end..drop_end]));
            buf.replace_range(head_end..drop_end, "\n");
        }
    }
}

/// Chat / Responses 流式
pub async fn handle_chat_stream(
    state: Arc<AppState>,
    token: ApiToken,
    channel: Channel,
    model: String,
    response: ReqwestResponse,
    ctx: crate::relay::proxy::UserContext,
    prompt_tokens: i32,
    request_content_str: String,
    start_time: std::time::Instant,
    target_type: String,
    upstream_path: String,
    upstream_req_content: Option<String>,
    pre_deducted: f64,
    pre_deduct_gift: f64,
    entry_endpoint: String,
    smart_router_ep: Option<String>,
    pending_log_id: Option<i64>,
    db_model: Option<Model>,
    mut db_rule: Option<BillingRule>,
) -> impl IntoResponse {
    let upstream_hdrs = response.headers().clone();
    let up_req_id = upstream_headers::extract_upstream_request_id(&upstream_hdrs);
    let (tx, rx) = mpsc::channel::<Result<String, axum::Error>>(100);
    let mut upstream_stream = response.bytes_stream();

    tokio::spawn(async move {
        let mut buffer = String::new();
        let mut raw_response_text = String::new();
        let mut overflow_tokens: i32 = 0;
        let mut stream_fail: Option<(u16, String)> = None;
        let passthrough = sse_body_passthrough(&entry_endpoint);

        let mut client_disconnected = false;
        while let Some(chunk_result) = upstream_stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    let chunk_str = String::from_utf8_lossy(&bytes);
                    append_stream_bounded(&mut raw_response_text, &chunk_str, &mut overflow_tokens);
                    if passthrough {
                        if tx.send(Ok(chunk_str.to_string())).await.is_err() {
                            client_disconnected = true;
                            break;
                        }
                    } else {
                        buffer.push_str(&chunk_str);
                        while let Some(index) = buffer.find('\n') {
                            let line = buffer.drain(..index + 1).collect::<String>();
                            let line = line.trim();
                            if line.is_empty() {
                                continue;
                            }
                            if let Some(transformed) = crate::relay::forward::transform_sse_line(
                                &target_type,
                                line,
                                &model,
                            ) {
                                if tx
                                    .send(Ok(format!("data: {}\n\n", transformed)))
                                    .await
                                    .is_err()
                                {
                                    client_disconnected = true;
                                    break;
                                }
                            }
                        }
                        if client_disconnected {
                            break;
                        }
                    }
                }
                Err(e) => {
                    stream_fail = Some((502, format!("上游流式传输中断: {}", e)));
                    break;
                }
            }
        }

        // 客户端提前断开连接时，短限时尝试排空上游尾部 chunk，尽量捕获末尾的 usage 统计帧
        if client_disconnected && stream_fail.is_none() {
            let mut drained_bytes = 0usize;
            let _ = tokio::time::timeout(std::time::Duration::from_millis(1500), async {
                while let Some(Ok(bytes)) = upstream_stream.next().await {
                    drained_bytes += bytes.len();
                    append_stream_bounded(
                        &mut raw_response_text,
                        &String::from_utf8_lossy(&bytes),
                        &mut overflow_tokens,
                    );
                    if drained_bytes >= 256 * 1024 {
                        break;
                    }
                }
            })
            .await;
        }

        if !passthrough && !client_disconnected {
            let _ = tx.send(Ok("data: [DONE]\n\n".to_string())).await;
        }

        if stream_fail.is_none() {
            stream_fail = detect_stream_error(&raw_response_text);
        }

        let mut usage = crate::relay::usage_extractor::UsageTokens {
            prompt: prompt_tokens,
            completion: 0,
            ..Default::default()
        };
        if !raw_response_text.is_empty() {
            let actual = crate::relay::usage_extractor::parse_usage(&raw_response_text);
            if actual.prompt > 0 || actual.completion > 0 {
                usage = actual;
                if usage.prompt == 0 && prompt_tokens > 0 {
                    usage.prompt = prompt_tokens;
                }
            }
            if usage.completion == 0 && stream_fail.is_none() {
                usage.completion = overflow_tokens
                    .saturating_add(estimate_stream_completion_tokens(&raw_response_text));
            }
        }
        if usage.prompt == 0 && usage.completion == 0 {
            usage.completion = 1;
        }

        let req_json = serde_json::from_str::<serde_json::Value>(&request_content_str)
            .unwrap_or(serde_json::json!({}));
        let mut features = crate::relay::usage_extractor::extract_request_features(&req_json);
        crate::relay::usage_extractor::enrich_features_from_usage(&mut features, &usage);
        settle_after_stream(
            &state,
            &token,
            &channel,
            &model,
            db_model.as_ref(),
            &mut db_rule,
            &ctx,
            usage,
            features,
            smart_router_ep
                .as_ref()
                .map(|ep| format!(" | 智能路由: {}", ep)),
            start_time,
            &entry_endpoint,
            &upstream_path,
            pre_deducted,
            pre_deduct_gift,
            request_content_str,
            raw_response_text,
            upstream_req_content,
            "聊天",
            pending_log_id,
            stream_fail,
            up_req_id,
        )
        .await;
    });

    upstream_headers::sse_with_upstream_headers(
        &upstream_hdrs,
        axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)),
    )
}

/// 图片流式
pub async fn handle_image_stream(
    state: Arc<AppState>,
    token: ApiToken,
    channel: Channel,
    model: String,
    response: ReqwestResponse,
    ctx: crate::relay::proxy::UserContext,
    request_content_str: String,
    start_time: std::time::Instant,
    upstream_path: String,
    upstream_req_content: Option<String>,
    pre_deducted: f64,
    pre_deduct_gift: f64,
    entry_endpoint: String,
    smart_router_ep: Option<String>,
    pending_log_id: Option<i64>,
    db_model: Option<Model>,
    mut db_rule: Option<BillingRule>,
) -> impl IntoResponse {
    let upstream_hdrs = response.headers().clone();
    let up_req_id = upstream_headers::extract_upstream_request_id(&upstream_hdrs);
    let (tx, rx) = mpsc::channel::<Result<Bytes, axum::Error>>(100);
    let mut upstream_stream = response.bytes_stream();

    tokio::spawn(async move {
        let mut full_response_text = String::new();
        let mut overflow_tokens: i32 = 0;
        let mut stream_fail: Option<(u16, String)> = None;

        while let Some(chunk_result) = upstream_stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    append_stream_bounded(
                        &mut full_response_text,
                        &String::from_utf8_lossy(&bytes),
                        &mut overflow_tokens,
                    );
                    if tx.send(Ok(bytes)).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    stream_fail = Some((502, format!("上游流式传输中断: {}", e)));
                    break;
                }
            }
        }

        if stream_fail.is_none() {
            stream_fail = detect_stream_error(&full_response_text);
        }

        let usage = crate::relay::usage_extractor::parse_usage(&full_response_text);
        let features = crate::relay::usage_extractor::features_from_exchange(
            Some(request_content_str.as_str()),
            upstream_req_content
                .as_deref()
                .filter(|uc| *uc != request_content_str.as_str()),
            Some(full_response_text.as_str()),
        );
        settle_after_stream(
            &state,
            &token,
            &channel,
            &model,
            db_model.as_ref(),
            &mut db_rule,
            &ctx,
            usage,
            features,
            smart_router_ep
                .as_ref()
                .map(|ep| format!(" | 智能路由: {}", ep)),
            start_time,
            &entry_endpoint,
            &upstream_path,
            pre_deducted,
            pre_deduct_gift,
            request_content_str,
            full_response_text,
            upstream_req_content,
            "图片",
            pending_log_id,
            stream_fail,
            up_req_id,
        )
        .await;
    });

    upstream_headers::sse_with_upstream_headers(
        &upstream_hdrs,
        axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)),
    )
}

