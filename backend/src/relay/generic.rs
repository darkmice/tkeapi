/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! Relay: 通用透传处理器
//! 处理 Embedding（向量）和 Rerank（排序）模型请求。
//! 请求体直接透传，仅替换 model 字段；响应直接返回上游 JSON。
//! 遵循与 audio.rs 一致的 7 步流水线模式。

use super::{forward, proxy, router, upstream_headers, usage_extractor};
use crate::models::ApiToken;
use crate::{
    error::{AppError, AppResult},
    AppState,
};
use axum::{
    extract::{Extension, OriginalUri, State},
    response::Response,
    Json,
};
use std::sync::Arc;

// ── 类别推断 ────────────────────────────────────────────────────

/// 本 handler 仅服务向量/排序两类
fn infer_category(path: &str) -> &'static str {
    if path.contains("rerank") {
        "排序"
    } else {
        "向量"
    }
}

// ── 主处理函数 ──────────────────────────────────────────────────

/// 通用透传处理器 — Embedding / Rerank
pub async fn generic_relay(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    OriginalUri(uri): OriginalUri,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Response> {
    let req_start = std::time::Instant::now();
    let raw_path = uri.path();
    let entry_path = raw_path.to_string();
    let category = infer_category(raw_path);
    let log_id = upstream_headers::resolve_request_log_id(Some(category));
    let request_content_str = serde_json::to_string(&body).unwrap_or_default();

    let model = body["model"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("Missing required parameter: model".to_string()))?;

    // ── 1. Token 模型权限校验（渠道选择前快速拦截） ──
    proxy::check_model_permission(&state, &token, model, &entry_path, Some(category)).await?;

    // ── 2. 用户上下文 ──
    let ctx = proxy::get_user_context(&state, &token.user_id).await?;

    // ── 3. 渠道选择 + HA failover ──
    let mut ha = crate::relay::ha::HaAttempt::begin(&state, token.high_availability, req_start).await;
    let mut billing_rule_cache = None;
    let mut access_cache = None;

    while ha.cont() {
        let attempt_start = std::time::Instant::now();
        let mut ha_pool = None;
        let channel = match proxy::select_channel_for_model(
            &state,
            &token,
            model,
            &ctx.user_group,
            &ctx.level_id,
            &entry_path,
            &ha.exclude_aids,
            !ha.had_upstream,
            Some(category),
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

        // ── 4. 预扣费检查 ──
        let (pre_deduction, db_model, resolved_cat) = match proxy::check_access_with_model(
            &state,
            &token,
            model,
            &ctx,
            Some(category),
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

        // ── 5. 转发规则解析 ──
        let mut resolved = match forward::resolve_forward_rule(
            &state,
            model,
            &resolved_cat,
            &entry_path,
            Some(&channel),
            db_model.as_ref(),
        )
        .await
        {
            Some(r) => r,
            None => {
                if forward::model_has_forward_rules(&state, model).await {
                    let err = forward::record_unsupported_forward_error(
                        &state,
                        &token,
                        &channel,
                        model,
                        raw_path,
                        &resolved_cat,
                    )
                    .await;
                    ha.on_access_err(err);
                    break;
                }
                forward::infer_forward_from_base_url(
                    &channel.base_url,
                    &resolved_cat,
                    db_model.as_ref(),
                )
            }
        };
        forward::refine_target_type(&mut resolved, &channel.base_url);

        // 模型映射：向量/排序无分辨率档，跳过 body 解析
        let (final_resolved_model, mapping_source) =
            router::resolve_model(&channel, model, db_model.as_ref(), None);

        // 查询计费规则（同 billing_rule_id 复用，供计费阶段使用）
        let mut db_rule = proxy::get_model_billing_rule(
            &state,
            model,
            Some(&channel),
            db_model.as_ref(),
            &mut billing_rule_cache,
        )
        .await;

        // ── 6. 请求体透传（仅替换 model 字段） ──
        let mut upstream_body = body.clone();
        upstream_body["model"] = serde_json::json!(&final_resolved_model);

        let url = forward::build_upstream_url(
            &channel.base_url,
            &resolved,
            &final_resolved_model,
            &channel.api_key,
        );

        // 【一条日志原则】请求前预记录日志
        let ep = format!(
            "{}|{}",
            raw_path,
            resolved
                .upstream_path
                .replace("${model}", &final_resolved_model)
        );

        crate::relay_debug!(
            "[Generic] 模型={} 类别={} 目标类型={} URL={}",
            model,
            category,
            resolved.target_type,
            url
        );

        if ha.pending_log_id.is_none() {
            ha.set_pending(
                proxy::record_pending_log(proxy::PendingLog {
                    state: &state,
                    user_id: &token.user_id,
                    token_id: token.id,
                    model,
                    endpoint: &ep,
                    is_stream: 0,
                    request_content: Some(&request_content_str),
                    upstream_url: Some(&url),
                    channel: &channel,
                    plugin_tag: None,
                    category: Some(resolved_cat.as_str()),
                    db_model: db_model.as_ref(),
                    forward_eid: Some(&resolved.eid),
                    requested_log_id: Some(&log_id),
                    task_id: None,
                })
                .await,
            );
        }

        // 【连接保护】上游请求+预扣+落库放独立 task，客户端断开后仍能完成
        let pending_log_id = ha.pending_log_id;
        let timeout_ctx = ha.timeout_ctx();
        let fail_buf = ha.buf();

        let result_rx = super::spawn_protected({
            let state = state.clone();
            let token = token.clone();
            let channel = channel.clone();
            let model = model.to_string();
            let request_content_str = request_content_str.clone();
            let ctx = ctx.clone();
            let url = url.clone();
            let ep = ep.clone();
            let db_model = db_model.clone();
            let resolved_cat = resolved_cat.clone();
            async move {
                // 构建并发送上游请求（统一鉴权 + 设置请求体）；预扣在业务成功后再执行（对齐 chat）
                let builder = state
                    .http_client
                    .post(&url)
                    .header("Content-Type", "application/json");
                let builder = crate::services::http_client::with_timeout(
                    forward::apply_request_auth(
                        builder,
                        &resolved,
                        &channel.api_key,
                        &mut upstream_body,
                        &channel.base_url,
                    ),
                    timeout_ctx.resolve(),
                );
                let resp = match timeout_ctx.send(builder, false).await {
                    Ok(resp) => resp,
                    Err(err_msg) => {
                        let latency_ms = attempt_start.elapsed().as_millis() as u32;
                        let bill = crate::relay::ha::FailBill::transport(
                            latency_ms,
                            err_msg,
                            &request_content_str,
                            upstream_body.to_string(),
                        );
                        return Err(crate::relay::ha::HaAttempt::park(&fail_buf, bill, None));
                    }
                };

                let status = resp.status().as_u16();
                if !resp.status().is_success() {
                    let upstream_hdrs = resp.headers().clone();
                    let err = resp.text().await.unwrap_or_default();
                    let latency_ms = attempt_start.elapsed().as_millis() as u32;
                    let bill = crate::relay::ha::FailBill::http(
                        latency_ms,
                        status,
                        err,
                        &request_content_str,
                        upstream_body.to_string(),
                    );
                    return Err(crate::relay::ha::HaAttempt::park(
                        &fail_buf,
                        bill,
                        Some(upstream_hdrs),
                    ));
                }

                // 读取响应体文本
                let upstream_hdrs = resp.headers().clone();
                let mut resp_text = resp.text().await.unwrap_or_default();

                // 上游 body 级错误检测（HTTP 200 但业务失败，在预扣费之前拦截）
                let (converted, post_err) = forward::check_upstream_post_error(
                    &resolved.target_type,
                    &resp_text,
                    resolved_cat.as_str(),
                    false,
                );
                resp_text = converted;
                if let Some(err_response) = post_err {
                    let latency_ms = attempt_start.elapsed().as_millis() as u32;
                    let bill = crate::relay::ha::FailBill::biz(
                        latency_ms,
                        resp_text,
                        err_response,
                        request_content_str,
                        upstream_body.to_string(),
                    );
                    return Err(crate::relay::ha::HaAttempt::park(
                        &fail_buf,
                        bill,
                        Some(upstream_hdrs),
                    ));
                }

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
                    Some(resolved_cat.as_str()),
                )
                .await?;

                // 提取 usage tokens
                let mut usage = usage_extractor::parse_usage(&resp_text);
                // 向量化/重排端点以输入文本为主：若仅返回 total_tokens（被 parse_usage 归入 completion），在此调换为 prompt
                if usage.prompt == 0 && usage.completion > 0 {
                    usage.prompt = usage.completion;
                    usage.completion = 0;
                }
                let known_sum =
                    (usage.prompt + usage.completion).max(usage.image_tokens + usage.completion);
                if usage.total > known_sum {
                    usage.prompt = (usage.total - usage.completion).max(usage.prompt);
                }

                // ── 计费结算 ──
                let latency_ms = req_start.elapsed().as_millis() as u32;
                let mut features = usage_extractor::ExtractedFeatures::default();

                let (cost, billing_detail) = crate::relay::calculate_relay_cost(
                    &state,
                    db_model.as_ref(),
                    db_rule.as_mut(),
                    &channel,
                    &ctx,
                    &usage,
                    &mut features,
                    mapping_source.as_deref(),
                    &model,
                    &final_resolved_model,
                )
                .await;

                proxy::record_and_bill_inner(proxy::BillRecord {
                    ctx: crate::relay::ha::HaBillCtx::new(&state, &token, &model, &ep)
                        .category(resolved_cat.as_str())
                        .db(db_model.as_ref()),
                    channel: &channel,
                    log_id: pending_log_id,
                    usage,
                    cost,
                    pre_deducted: pre_deduction,
                    pre_deduct_gift,
                    latency_ms,
                    is_stream: 0,
                    status_code: 200,
                    error_msg: None,
                    request: Some(request_content_str),
                    response: Some(resp_text.clone()),
                    upstream_req: Some(upstream_body.to_string()),
                    detail: Some(billing_detail),
                    features: Some(features),
                    time_multiplier: db_rule.as_ref().map(|r| r.applied_multiplier),
                    plugin_tag: None,
                    upstream_request_id: upstream_headers::extract_upstream_request_id(
                        &upstream_hdrs,
                    ),
                })
                .await;

                Ok(super::UpstreamRaw::new(upstream_hdrs, resp_text))
            }
        });

        let bill_ctx = crate::relay::ha::HaBillCtx::new(&state, &token, model, &ep)
            .category(resolved_cat.as_str())
            .db(db_model.as_ref());
        match super::join_protected(&mut ha, result_rx, &bill_ctx, &channel, Some(&url)).await {
            super::ProtectJoin::Ok(raw) => {
                let ms = attempt_start.elapsed().as_millis() as u32;
                ha.ok(&state, &channel, &url, ms).await;
                return Ok(upstream_headers::with_request_id(
                    upstream_headers::json_with_upstream_headers(&raw.headers, raw.body),
                    &log_id,
                ));
            }
            super::ProtectJoin::Retry => {
                ha.bump();
                continue;
            }
            super::ProtectJoin::Stop => break,
        }
    } // end while

    Err(ha
        .finish(
            &crate::relay::ha::HaBillCtx::new(&state, &token, model, &entry_path)
                .category(category),
        )
        .await)
}
