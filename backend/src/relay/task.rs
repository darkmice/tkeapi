/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! Relay: 通用异步任务轮询网关 + 后台定时轮询器
//! 统一处理视频、图片等带有 task_id 的异步模型轮询和计费结算。
//!
//! 路由入口：
//!   - GET /v1/video/generations/{task_id} — 标准视频轮询（对外主入口）
//!   - GET /v1/videos/{task_id} — OpenAI Videos 同级别名
//!   - GET /v1/videos/generations/{task_id} — OpenAI 兼容别名
//!   - GET /v1/tasks/{task_id}            — 兼容 apimart 等图片模型的异步轮询地址
//!   - DELETE 同上路径 — 取消尚未完成的异步任务（本站 ComfyUI 队列；火山方舟走原生 DELETE）
//!   GET 视频/任务轮询均执行本文件的 task_status 函数，逻辑完全一致。
//!
//! 后台定时器按 RelaySettings.poll_tick_secs（缓存）自动检查未完成计费的异步任务，确保计费正确落地。

use super::cascade::{
    cascade_apply_s1_res_mul, cascade_combine_stages, cascade_format_completed_resp,
    cascade_is_skip_s2, cascade_on_s2_succeeded, cascade_persist_s2_fail,
    cascade_plugin_tag_present, cascade_prepare_poll, cascade_s1_raw_from_log,
    cascade_s2_client_processing, cascade_s2_poll, cascade_scrub_and_update_log_tag,
    cascade_stage2_submit, CascadePollPlan, CascadeS2PollErr, CascadeS2PollOk, CascadeS2SubmitCtx,
};
use super::response_formatter::{
    extract_raw_status, force_json_task_id, format_async_task_failed,
    is_failed_task_status, parse_raw_status_to_standard,
};
use super::url_utils::join_url;
use super::{forward, proxy};
use crate::models::{ApiToken, Channel};
use crate::{
    error::{AppError, AppResult},
    AppState,
};
use axum::{
    extract::{Extension, OriginalUri, Path, Query, State},
    response::Response,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

/// 轮询/后台共用 logs 行（FromRow；加字段同步改 SELECT）。
#[derive(Debug, Clone, sqlx::FromRow)]
struct TaskRelayLogRow {
    id: i64,
    channel_id: i64,
    model: String,
    response_content: String,
    request_content: String,
    /// ← logs.action_type
    category: String,
    plugin_tag: String,
    upstream_req_content: String,
    post_response: String,
    is_completed: i64,
    status_code: i64,
    channel_config_id: Option<i64>,
    task_id: String,
    user_id: String,
    billing_features: String,
    cost: f64,
    pre_deduct_gift: f64,
    token_id: Option<i64>,
    billing_detail: Option<String>,
    error_message: Option<String>,
    forward_eid: String,
    endpoint: String,
}

/// 退款/失败结算时复用内存元数据，避免重复回表
#[derive(Debug, Clone, Copy)]
pub(crate) struct SettleFailureMeta<'a> {
    pub cost: f64,
    pub pre_deduct_gift: f64,
    pub user_id: &'a str,
    pub token_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub channel_config_id: Option<i64>,
}

impl TaskRelayLogRow {
    /// 转换为阶段二提交入参，避免调用方手动展开多个零散字段
    fn to_s2_submit_ctx(
        &self,
        channel: &Channel,
        base_video_url: &str,
        stage1_response: &str,
        resolved: &forward::ResolvedForward,
    ) -> CascadeS2SubmitCtx {
        CascadeS2SubmitCtx {
            task_id: self.task_id.clone(),
            log_id: self.id,
            post_response: self.post_response.clone(),
            request_content: self.request_content.clone(),
            upstream_req: self.upstream_req_content.clone(),
            channel: channel.clone(),
            base_video_url: base_video_url.to_string(),
            plugin_tag: self.plugin_tag.clone(),
            stage1_response: stage1_response.to_string(),
            resolved: resolved.clone(),
        }
    }

    /// 提取内存中的计费与预扣元数据，供失败退费直接复用
    fn to_failure_meta(&self) -> SettleFailureMeta<'_> {
        SettleFailureMeta {
            cost: self.cost,
            pre_deduct_gift: self.pre_deduct_gift,
            user_id: &self.user_id,
            token_id: self.token_id,
            channel_id: Some(self.channel_id),
            channel_config_id: self.channel_config_id,
        }
    }

    /// 提取客户端轮询所需的只读上下文，解耦底层持久化实体
    fn to_poll_ctx<'a>(&'a self, path: &'a str) -> forward::ClientPollCtx<'a> {
        forward::ClientPollCtx {
            path,
            model: &self.model,
            request_content: &self.request_content,
            billing_features: &self.billing_features,
            task_id: &self.task_id,
        }
    }
}

/// 与 [`TaskRelayLogRow`] 列对齐
const TASK_RELAY_LOG_COLS: &str = "\
id, channel_id, model, \
COALESCE(response_content, '') AS response_content, \
COALESCE(request_content, '') AS request_content, \
COALESCE(action_type, '') AS category, \
COALESCE(plugin_tag, '') AS plugin_tag, \
COALESCE(upstream_req_content, '') AS upstream_req_content, \
COALESCE(post_response, '') AS post_response, \
is_completed, status_code, channel_config_id, \
COALESCE(task_id, '') AS task_id, \
user_id, \
COALESCE(billing_features, '') AS billing_features, \
cost, pre_deduct_gift, token_id, billing_detail, error_message, \
COALESCE(forward_eid, '') AS forward_eid, \
COALESCE(endpoint, '') AS endpoint";

#[inline]
fn format_task_relay_sql(state: &AppState, where_clause: &str) -> String {
    state.db.format_query(&format!(
        "SELECT {TASK_RELAY_LOG_COLS} FROM logs WHERE {where_clause}"
    ))
}

/// 未结案才写轮询体；`err=None` 时 COALESCE 保留原 error_message（如 POLL_FAIL）。
async fn persist_open_poll_response(state: &AppState, log_id: i64, body: &str, err: Option<&str>) {
    let sql = state.db.format_query(
        "UPDATE logs SET response_content = ?, error_message = COALESCE(?, error_message) \
         WHERE id = ? AND is_completed = 0",
    );
    let stored = super::proxy::sanitize_base64(body);
    let _ = sqlx::query(&sql)
        .bind(&stored)
        .bind(err)
        .bind(log_id)
        .execute(&state.db.pool)
        .await;
}

/// GET / TaskPoller 中间态落库；S2 仍 combine 便于读；若内容未变且无错则跳过写入以降低 DB 压力。
async fn persist_open_poll_progress(
    state: &AppState,
    log_id: i64,
    cascade_stage: u8,
    s1_raw: &str,
    store_body: &str,
    current_content: &str,
) {
    if cascade_stage == 2 {
        let body = cascade_combine_stages(s1_raw, store_body);
        if body != current_content {
            persist_open_poll_response(state, log_id, &body, None).await;
        }
    } else if store_body != current_content {
        persist_open_poll_response(state, log_id, store_body, None).await;
    }
}

/// 单次上游轮询结果（手动 GET / 后台共用）
struct UpstreamPollOk {
    url: String,
    body: String,
    resp_json: serde_json::Value,
    task_status: String,
}

enum UpstreamPollFail {
    /// 临时错：HTTP 映射失败 / 后台计入 POLL_FAIL
    Retryable { status: u16, message: String },
    /// 终态错：已 `refund_poll_terminal`
    Settled {
        #[allow(dead_code)]
        status: u16,
        message: String,
    },
}

impl From<CascadeS2PollOk> for UpstreamPollOk {
    fn from(ok: CascadeS2PollOk) -> Self {
        Self {
            url: ok.url,
            body: ok.body,
            resp_json: ok.resp_json,
            task_status: ok.task_status,
        }
    }
}

impl From<CascadeS2PollErr> for UpstreamPollFail {
    fn from(e: CascadeS2PollErr) -> Self {
        match e {
            CascadeS2PollErr::Retryable { status, message } => Self::Retryable { status, message },
            #[cfg(not(all(
                feature = "plugin_tencent_enhance",
                feature = "plugin_volcengine_enhance"
            )))]
            CascadeS2PollErr::Settled { status, message } => Self::Settled { status, message },
        }
    }
}

/// jimeng ctx + send_poll + 清 POLL_FAIL + 状态归一
async fn run_upstream_poll(
    state: &AppState,
    log: &TaskRelayLogRow,
    cascade_stage: u8,
    target_type: &str,
    poll: &super::cascade::CascadePollTarget<'_>,
) -> Result<UpstreamPollOk, UpstreamPollFail> {
    if cascade_stage == 2 {
        return cascade_s2_poll(state, &log.plugin_tag, poll)
            .await
            .map(Into::into)
            .map_err(Into::into);
    }

    let mut jimeng_fb = None;
    let jimeng_ctx = build_jimeng_poll_ctx(
        target_type,
        &log.upstream_req_content,
        &log.request_content,
        &log.plugin_tag,
        &mut jimeng_fb,
    );
    let (url, body) = match send_poll_request(
        state,
        poll.channel.as_ref(),
        poll.resolved.as_ref(),
        poll.task_id.as_ref(),
        poll.model.as_ref(),
        jimeng_ctx,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let (status, retryable) = e.classify();
            if retryable {
                return Err(UpstreamPollFail::Retryable {
                    status,
                    message: e.message,
                });
            }
            refund_poll_terminal(
                state,
                log.id,
                cascade_stage,
                &e.message,
                status,
                &log.task_id,
            )
            .await;
            return Err(UpstreamPollFail::Settled {
                status,
                message: e.message,
            });
        }
    };
    if log
        .error_message
        .as_deref()
        .is_some_and(|m| m.starts_with("[POLL_FAIL:"))
    {
        clear_poll_fail(state, log.id).await;
    }
    if forward::is_tencent_target(target_type) {
        crate::relay_debug!(
            "[PollTask] 腾讯云原始响应 任务ID={}, 响应体={}",
            &log.task_id,
            body
        );
    }
    let resp_json = serde_json::from_str::<serde_json::Value>(&body).unwrap_or_default();
    let raw_status = super::response_formatter::extract_raw_status(&resp_json);
    let mut task_status = if resp_json.is_object() {
        normalize_task_status(&raw_status)
    } else {
        "failed"
    };
    if task_status != "succeeded" && super::response_formatter::is_upstream_error_response(&resp_json) {
        task_status = "failed";
    }
    let task_status = task_status.to_string();
    Ok(UpstreamPollOk {
        url,
        body,
        resp_json,
        task_status,
    })
}

/// 结案失败对外体：已有 `status:failed` 则保留并固定 id；否则补齐约定体
fn ensure_client_async_failed(
    raw_path: &str,
    category: &str,
    task_id: &str,
    formatted: &str,
    err_src: &str,
) -> String {
    // 仅当已有合法失败 JSON（且已包含 failed 状态）时保留透传并固定 id
    let is_json_obj = serde_json::from_str::<serde_json::Value>(formatted)
        .is_ok_and(|v| v.is_object());
    if is_json_obj && is_failed_task_status(formatted) {
        let mut s = formatted.to_string();
        force_json_task_id(&mut s, task_id);
        return s;
    }
    let err = proxy::extract_error_message(err_src);
    format_async_task_failed(
        raw_path,
        category,
        task_id,
        if err.is_empty() { "已失败" } else { &err },
    )
}

fn json_poll_response(body: String) -> Response {
    Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap()
}

/// logs 可服务客户端则 Some；否则 None 打上游。未完成级联只回处理中（防 S1 成片）。
async fn try_client_poll_from_logs(
    state: &AppState,
    raw_path: &str,
    task_id: &str,
    log: &TaskRelayLogRow,
    target_type: &str,
) -> Option<Response> {
    let completed = log.is_completed == 1;
    // 未完成且允许打上游 → 不走 logs 短路（默认 true）
    if !completed
        && crate::relay::relay_settings::get_cached_relay_settings(&state.db)
            .await
            .manual_poll_upstream
    {
        return None;
    }

    let category = log.category.as_str();
    let content = log.response_content.as_str();
    let cached = match serde_json::from_str::<serde_json::Value>(content) {
        Ok(v) if v.as_object().is_some_and(|m| !m.is_empty()) => v,
        _ => {
            if completed && log.status_code != 200 {
                crate::relay_debug!("[TaskPoll] {} 已失败无缓存", task_id);
                let raw_err = if !content.trim().is_empty() {
                    content
                } else {
                    log.error_message.as_deref().unwrap_or("已失败")
                };
                let body = ensure_client_async_failed(raw_path, category, task_id, raw_err, raw_err);
                return Some(json_poll_response(
                    forward::enrich_client_poll(target_type, &body, log.to_poll_ctx(raw_path)),
                ));
            }
            crate::relay_debug!(
                "[TaskPoll] {} {}→上游",
                task_id,
                if completed { "无缓存" } else { "无status" }
            );
            return None;
        }
    };

    // 未完成且无 status → 先打上游（攒数据，便于后续级联）
    if !completed && extract_raw_status(&cached).is_empty() {
        crate::relay_debug!("[TaskPoll] {} 无status→上游", task_id);
        return None;
    }

    let is_cascade = cached.get("stage1").is_some() || cached.get("stage2").is_some();
    // 未完成级联：有 stage1 用 stage1，否则整份响应（早期扁平体即整包）
    if !completed && (is_cascade || cascade_plugin_tag_present(&log.plugin_tag)) {
        let s1_raw = cascade_s1_raw_from_log(content);
        crate::relay_debug!(
            "[TaskPoll] {} 级联处理中 status={}",
            task_id,
            log.status_code
        );
        return Some(json_poll_response(cascade_s2_client_processing(
            raw_path,
            category,
            &s1_raw,
            task_id,
            &log.plugin_tag,
            &log.model,
        )));
    }

    // completed，或未完成非级联且带 status
    let body = if is_cascade {
        cascade_format_completed_resp(
            raw_path,
            category,
            content,
            &log.plugin_tag,
            task_id,
            &log.model,
            "增强失败",
        )
    } else {
        let mut formatted = crate::relay::response_formatter::apply_format(
            raw_path,
            category,
            content,
            true,
            Some(task_id),
        );
        if completed && log.status_code != 200 {
            formatted =
                ensure_client_async_failed(raw_path, category, task_id, &formatted, content);
        }
        if category.contains("图片") {
            let rf = super::tos_persist::response_format_from_request(&log.request_content);
            formatted =
                super::tos_persist::align_response_format(state, &formatted, rf.as_deref()).await;
        }
        if !log.model.is_empty() {
            super::response_formatter::json_root_set(
                &mut formatted,
                "model",
                &serde_json::json!(log.model).to_string(),
            );
        }
        formatted
    };
    crate::relay_debug!(
        "[TaskPoll] {} 走缓存 completed={} status={}",
        task_id,
        completed,
        log.status_code
    );
    let body = forward::enrich_client_poll(target_type, &body, log.to_poll_ctx(raw_path));
    Some(json_poll_response(body))
}

/// 即梦轮询 ctx（无 request 时从 plugin_tag.jimeng_poll 恢复）
fn build_jimeng_poll_ctx<'a>(
    target_type: &str,
    log_upstream_req: &'a str,
    log_request_content: &'a str,
    plugin_tag: &str,
    fallback_buf: &'a mut Option<String>,
) -> Option<(&'a str, &'a str)> {
    if !target_type.starts_with("jimeng_") {
        return None;
    }
    let req: &str = if log_request_content.is_empty() {
        *fallback_buf = serde_json::from_str::<serde_json::Value>(plugin_tag)
            .ok()
            .and_then(|pt| pt.get("jimeng_poll").map(|jp| jp.to_string()));
        fallback_buf.as_deref().unwrap_or("")
    } else {
        log_request_content
    };
    let upstream = if log_upstream_req.is_empty() {
        ""
    } else {
        log_upstream_req
    };
    Some((upstream, req))
}

/// 类别 → 默认入口路径
fn category_to_entry_path(category: &str) -> &'static str {
    match category {
        "视频" | "封装" => "/v1/video/generations",
        "图片" => "/v1/images/generations",
        _ => "/v1/tasks",
    }
}

/// 非空类别给选模/计费作 type hint
#[inline]
fn category_hint(category: &str) -> Option<&str> {
    (!category.is_empty()).then_some(category)
}

// ── DELETE /v1/video/generations/{task_id} | /v1/tasks/{task_id} ──

/// DELETE 异步任务（校验归属；节点确认取消成功后才退冻结预扣，已出片则取消失败并照常扣费）
pub async fn task_cancel(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    Path(task_id): Path<String>,
) -> AppResult<Response> {
    if task_id.is_empty() {
        return Err(AppError::BadRequest("任务不存在".into()));
    }
    let log_id: Option<i64> = sqlx::query_scalar(&state.db.format_query(
        "SELECT id FROM logs WHERE task_id = ? AND user_id = ? ORDER BY id DESC LIMIT 1",
    ))
    .bind(&task_id)
    .bind(&token.user_id)
    .fetch_optional(&state.db.pool)
    .await?;
    let Some(log_id) = log_id else {
        return Err(AppError::Forbidden(
            "无权操作此任务或任务不存在".to_string(),
        ));
    };

    let cancelled = {
        #[cfg(feature = "plugin_comfyui")]
        {
            crate::api::plugins::comfyui_bridge::cancel_video(&state, &task_id, log_id).await?
        }
        #[cfg(not(feature = "plugin_comfyui"))]
        {
            let _ = log_id;
            false
        }
    };
    if cancelled {
        return Ok(Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(
                serde_json::json!({ "message": "任务已取消" }).to_string(),
            ))
            .unwrap());
    }

    Err(AppError::BadRequest("此任务不支持取消操作".to_string()))
}

// ── GET /tasks?task_ids=${task_id} ──

/// GET /tasks?task_ids=${task_id} — 可灵 3.0 / Omni 官方轮询入口
/// 仅支持可灵官方参数 task_ids
pub async fn kling_tasks_query(
    state: State<Arc<AppState>>,
    token: Extension<ApiToken>,
    uri: OriginalUri,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Response> {
    let task_id = params
        .get("task_ids")
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("Missing required query parameter: task_ids".into()))?;

    task_status(
        state,
        token,
        uri,
        axum::extract::Path(task_id),
        Query(params),
    )
    .await
}

// ── GET /v1/video/generations/{task_id} | /v1/tasks/{task_id} ──

/// GET 异步任务状态（/v1/video/generations|tasks/{id}）
pub async fn task_status(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    OriginalUri(uri): OriginalUri,
    Path(task_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Response> {
    let raw_path = uri.path();

    if task_id.is_empty() {
        return Err(AppError::BadRequest("任务不存在".into()));
    }
    // 先按 (task_id, id) 取主键，再回表拿大字段，避免 Bitmap Heap 展开 TOAST 后排序
    let mut log = sqlx::query_as::<_, TaskRelayLogRow>(&format_task_relay_sql(
        &state,
        "id = (SELECT id FROM logs WHERE task_id = ? AND task_id <> '' AND user_id = ? ORDER BY id DESC LIMIT 1)",
    ))
    .bind(&task_id)
    .bind(&token.user_id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("任务不存在".into()))?;
    // model：logs 优先，否则用 query
    if log.model.is_empty() {
        if let Some(m) = params.get("model") {
            log.model.clone_from(m);
        }
    }
    if log.is_completed == 0
        && forward::awaiting_video_submit(&log.response_content, &log.plugin_tag)
    {
        return Ok(json_poll_response(forward::route_accept_body(
            raw_path, &task_id, &log.model,
        )));
    }

    // 与选渠同源水合（channel_config_id 还原 HA 子配）
    let channel = super::router::fetch_channel(&state, log.channel_id, log.channel_config_id).await;

    // 模型一次查出，转发规则与结算共用
    let db_model = match (&channel, log.model.is_empty()) {
        (Some(c), false) => {
            super::proxy::find_active_model_exact(
                &state,
                &log.model,
                category_hint(&log.category),
                Some(c),
            )
            .await
        }
        _ => None,
    };

    let entry_path = category_to_entry_path(&log.category);
    let resolved_by_eid = if !log.forward_eid.is_empty() {
        forward::resolve_forward_rule_by_eid(
            &state,
            &log.forward_eid,
            &log.category,
            entry_path,
            db_model.as_ref(),
        )
        .await
    } else {
        None
    };

    let mut resolved = match resolved_by_eid {
        Some(r) => r,
        None => forward::resolve_forward_rule(
            &state,
            &log.model,
            &log.category,
            entry_path,
            channel.as_ref(),
            db_model.as_ref(),
        )
        .await
        .unwrap_or_else(|| match &channel {
            Some(c) => forward::infer_forward_from_base_url(&c.base_url, &log.category, None),
            None => forward::ResolvedForward::default(),
        }),
    };
    if let Some(c) = &channel {
        forward::refine_target_type(&mut resolved, &c.base_url);
        forward::apply_channel_provider(&mut resolved, c);
    }

    // 级联基因守卫：若任务历史日志中无级联标记，则强制纠偏为非级联任务，严防误触发 S2 提交流程导致退款
    if resolved.is_cascade && !cascade_plugin_tag_present(&log.plugin_tag) {
        crate::relay_debug!(
            "[TaskPoll Client] 任务缺少级联基因，强制纠偏为普通单阶段任务 id={}",
            task_id
        );
        resolved.is_cascade = false;
    }

    if let Some(resp) =
        try_client_poll_from_logs(&state, raw_path, &task_id, &log, &resolved.target_type).await
    {
        return Ok(resp);
    }

    if log.model.is_empty() {
        return Err(AppError::BadRequest("缺少 model".into()));
    }
    let channel = channel.ok_or_else(|| AppError::BadRequest("渠道不存在".into()))?;

    // 与 image 同步轮询一致：传入映射后的上游模型 id（复用已查 db_model）
    let poll_model = super::router::resolve_model(&channel, &log.model, db_model.as_ref(), None).0;
    let CascadePollPlan {
        stage: cascade_stage,
        post_resp_json,
        poll,
    } = match cascade_prepare_poll(
        resolved.is_cascade,
        &log.post_response,
        &channel,
        &resolved,
        &log.plugin_tag,
        &task_id,
        &poll_model,
    ) {
        Ok(plan) => plan,
        Err((err_text, status)) => {
            // S2 提交失败已落库无 id；此处 CAS 结案（status 已从 stage2 推断）
            settle_failure(
                &state,
                log.id,
                &err_text,
                status,
                2,
                Some(log.to_failure_meta()),
            )
            .await;
            return Ok(json_poll_response(format_async_task_failed(
                raw_path,
                &log.category,
                &task_id,
                &err_text,
            )));
        }
    };

    let UpstreamPollOk {
        url,
        body: get_resp_str,
        resp_json,
        task_status,
    } = match run_upstream_poll(&state, &log, cascade_stage, &resolved.target_type, &poll).await {
        Ok(r) => r,
        Err(UpstreamPollFail::Retryable { status, message }) => {
            return Err(proxy::upstream_fail(status, &message, None, raw_path));
        }
        Err(UpstreamPollFail::Settled { message, .. }) => {
            let body = format_async_task_failed(raw_path, &log.category, &task_id, &message);
            return Ok(json_poll_response(body));
        }
    };
    crate::relay_debug!(
        "[TaskPoll] id={} 模型={} 类别={} 状态={} 阶段={} len={}",
        task_id,
        log.model,
        log.category,
        task_status,
        cascade_stage,
        get_resp_str.len()
    );
    let mut get_resp_str = get_resp_str;
    if forward::tag_is_video_accept(&log.plugin_tag) {
        force_json_task_id(&mut get_resp_str, &task_id);
    }

    // 组落库体基准：腾讯云转 OpenAI 规整化；若发起端点是官方路由且成功，提前转为官方响应体（前置于级联增强）
    let is_tencent = forward::is_tencent_target(&resolved.target_type);
    let mut store_body = if is_tencent {
        super::response_formatter::format_openai(&log.category, &get_resp_str, true, Some(&task_id))
    } else {
        get_resp_str.clone()
    };
    if forward::is_official_route(raw_path) && is_tencent {
        store_body =
            forward::wrap_official_client(raw_path, &store_body, &log.model, &log.request_content);
    }

    // S1 成功 → 提交 S2（若配置跳过 S2，则直接出片并按 res_mul 放大倍率结案）
    let is_skip_s2 = cascade_is_skip_s2(&log.plugin_tag);
    if cascade_stage == 1 && task_status == "succeeded" {
        if !is_skip_s2 {
            let base_video_url = super::response_formatter::find_urls(&resp_json)
                .into_iter()
                .next()
                .unwrap_or_default();
            crate::relay_debug!("[Cascade S1] 成功→S2 id={}", task_id);
            cascade_stage2_submit(
                &state,
                log.to_s2_submit_ctx(&channel, &base_video_url, &store_body, &resolved),
            )
            .await;
            // 用刚拿到的已规整/官方格式轮询体（勿用瘦 POST ack）；前裁/提交在 cascade 后台
            let mut s1_raw = store_body.clone();
            force_json_task_id(&mut s1_raw, &task_id);
            return Ok(json_poll_response(cascade_s2_client_processing(
                raw_path,
                &log.category,
                &s1_raw,
                &task_id,
                &log.plugin_tag,
                &log.model,
            )));
        } else {
            let mul = cascade_apply_s1_res_mul(&mut store_body, &log.plugin_tag, &log.request_content, &resolved.res_mul);
            crate::relay_debug!("[Cascade S1 直出] id={} 倍率={:.2} 跳过S2直接出片", task_id, mul);
        }
    }

    if resolved.is_cascade && cascade_stage == 1 {
        force_json_task_id(&mut store_body, &task_id);
    } else if !resolved.is_cascade && !log.model.is_empty() {
        super::response_formatter::json_root_set(
            &mut store_body,
            "model",
            &serde_json::json!(log.model).to_string(),
        );
    }

    let mut s1_raw = if cascade_stage == 2 {
        cascade_s1_raw_from_log(&log.response_content).to_string()
    } else {
        String::new()
    };
    // S2 成功：后裁/抽帧；未就绪或忙则回处理中，避免重复处理
    let mut _s2_guard = None;
    if cascade_stage == 2 && task_status == "succeeded" {
        _s2_guard = cascade_on_s2_succeeded(
            &state,
            log.id,
            &poll,
            &mut s1_raw,
            &mut store_body,
            &log.plugin_tag,
            &log.request_content,
        )
        .await;
        if _s2_guard.is_none() {
            return Ok(json_poll_response(cascade_s2_client_processing(
                raw_path,
                &log.category,
                &s1_raw,
                &task_id,
                &log.plugin_tag,
                &log.model,
            )));
        }
    }
    // TOS：成功且非级联 S1（若跳过 S2 则 S1 即终态）。Comfy 渠道 TOS 已在 poll_video 落桶，此处跳过避免二次 persist
    if task_status == "succeeded"
        && (cascade_stage != 1 || is_skip_s2)
        && resolved.target_type != "comfyui"
    {
        let user_rf = super::tos_persist::response_format_from_request(&log.request_content);
        store_body = super::tos_persist::persist_for_channel(
            &state,
            &channel,
            &store_body,
            &log.category,
            user_rf.as_deref(),
        )
        .await;
    }
    if cascade_stage == 2 && task_status == "succeeded" {
        // S2 成功叠 S1+S2 落库
        store_body = cascade_combine_stages(&s1_raw, &store_body);
    }
    // 终态：清理级联 plugin_tag 敏感字段
    if task_status == "succeeded" || task_status == "failed" {
        cascade_scrub_and_update_log_tag(&state, log.id, &log.plugin_tag).await;
    }

    match task_status.as_str() {
        "succeeded" => {
            // 结算 CAS 同写终态 body，避免倍率 usage 被未结案重试再乘、或被 pending 覆盖
            settle_success(
                &state,
                &log,
                &store_body,
                &resp_json,
                &url,
                &channel,
                cascade_stage,
                db_model.as_ref(),
                &resolved.res_mul,
            )
            .await;
            crate::services::notification::spawn_low_balance_check(
                Arc::clone(&state),
                token.user_id.clone(),
            );
            crate::relay_debug!(
                "[TaskBilling] log_id={} 模型={} 阶段={} url={}",
                log.id,
                log.model,
                cascade_stage,
                url
            );
        }
        "failed" => {
            let err_text = proxy::extract_error_message(&store_body);
            if cascade_stage == 2 {
                cascade_persist_s2_fail(
                    &state,
                    log.id,
                    &post_resp_json,
                    &s1_raw,
                    &store_body,
                    &err_text,
                    "Cascade S2",
                )
                .await;
            } else {
                persist_open_poll_response(&state, log.id, &store_body, Some(&err_text)).await;
            }
            let status_code = proxy::infer_error_status_code_from_str(&store_body);
            settle_failure(
                &state,
                log.id,
                &url,
                status_code,
                cascade_stage,
                Some(log.to_failure_meta()),
            )
            .await;
            crate::relay_debug!(
                "[TaskRefund] log_id={} 模型={} 阶段={} url={} code={}",
                log.id,
                log.model,
                cascade_stage,
                url,
                status_code
            );
        }
        _ => {
            persist_open_poll_progress(
                &state,
                log.id,
                cascade_stage,
                &s1_raw,
                &store_body,
                &log.response_content,
            )
            .await;
        }
    }

    // 对外：S2 成功叠 URL / 失败 failed / 进行中处理中；其余腾讯或 apply_format
    let mut out = match (cascade_stage, task_status.as_str()) {
        (2, "succeeded") => cascade_format_completed_resp(
            raw_path,
            &log.category,
            &store_body,
            &log.plugin_tag,
            &task_id,
            &log.model,
            &proxy::extract_error_message(&store_body),
        ),
        (2, "failed") => format_async_task_failed(
            raw_path,
            &log.category,
            &task_id,
            &proxy::extract_error_message(&store_body),
        ),
        (2, _) => cascade_s2_client_processing(
            raw_path,
            &log.category,
            &s1_raw,
            &task_id,
            &log.plugin_tag,
            &log.model,
        ),
        // live 结案失败也须 status:failed（与缓存路径一致）
        (_, "failed") => {
            let formatted = if is_tencent {
                store_body
            } else {
                crate::relay::response_formatter::apply_format(
                    raw_path,
                    &log.category,
                    &store_body,
                    true,
                    Some(&task_id),
                )
            };
            ensure_client_async_failed(raw_path, &log.category, &task_id, &formatted, &formatted)
        }
        (1, _) => cascade_s2_client_processing(
            raw_path,
            &log.category,
            &store_body,
            &task_id,
            &log.plugin_tag,
            &log.model,
        ),
        _ if is_tencent => store_body,
        _ => crate::relay::response_formatter::apply_format(
            raw_path,
            &log.category,
            &store_body,
            true,
            Some(&task_id),
        ),
    };
    // 仅图片双向对齐（视频只返 URL，跳过大 JSON）
    if log.category.contains("图片") {
        let rf = super::tos_persist::response_format_from_request(&log.request_content);
        out = super::tos_persist::align_response_format(&state, &out, rf.as_deref()).await;
    }
    // 级联对外统一 cgt，防上游 poll 体 id 泄漏
    if resolved.is_cascade {
        force_json_task_id(&mut out, &task_id);
    }
    // S2 终态：转发客户端 callback
    if cascade_stage == 2 && (task_status == "succeeded" || task_status == "failed") {
        super::vendor_callback::forward_official_to_client(&state, &log.plugin_tag, &out).await;
    }
    let out = forward::enrich_client_poll(&resolved.target_type, &out, log.to_poll_ctx(raw_path));
    Ok(json_poll_response(out))
}

// ── 后台定时轮询器 ──────────────────────────────────────────────

/// 启动后台轮询定时任务（支持优雅关闭：收到 shutdown 信号后完成当前轮询再退出）
/// 周期读 [`RelaySettings::poll_tick_secs`]（缓存，默认 30s）；客户端主动 GET 仍即时。
pub fn start(
    state: Arc<AppState>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // 启动后等待 30 秒再开始第一次轮询，让系统初始化完毕
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {},
            _ = shutdown.changed() => {
                tracing::info!("[TaskPoller] 初始化期间收到关闭信号，退出");
                return;
            }
        }
        loop {
            if let Err(e) = poll_pending_tasks(&state).await {
                tracing::warn!("[TaskPoller] 轮询异常: {}", e);
            }
            let tick_secs = crate::relay::relay_settings::get_cached_relay_settings(&state.db)
                .await
                .poll_tick_secs;
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(tick_secs)) => {},
                _ = shutdown.changed() => {
                    tracing::info!("[TaskPoller] 收到关闭信号，退出轮询");
                    return;
                }
            }
        }
    })
}

/// 活跃轮询窗口与单批大小；超窗外仍冻结的任务走 `refund_stale_freezes` 兜底退款。
const POLL_ACTIVE_INTERVAL: &str = "2 days";
const POLL_BATCH: i64 = 100;
const POLL_MAX_BATCHES_PER_TICK: u32 = 5;
/// 后台轮询并发度控制在 8（兼顾高吞吐并严格处于火山/Seedance 上游 QPS 频控安全线内）
const CONCURRENT_POLL_LIMIT: usize = 8;
/// 跨 Tick 环形滚动游标，保证在途大批量任务绝对公平调度，杜绝头部垄断与长尾饥饿
static TASK_POLL_CURSOR: AtomicI64 = AtomicI64::new(0);
/// `logs.latency_ms` 为 INT4；超龄冻结直接 CAST 会 22003，先按 bigint 钳到上限。
const LATENCY_MS_SQL: &str = "LEAST(2147483647, GREATEST(0, \
    (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - created_at)) * 1000)::bigint))::integer";

/// 连续请求失败上限（任务状态轮询 / 后台 TaskPoller 共用）
/// 取 100：上游故障、高峰排队或网络抖动时给予充裕容错窗口，配合 2 小时绝对生存门禁避免提前误退款。
const POLL_FAIL_LIMIT: u32 = 100;

/// 轮询失败达到上限时，必须同时满足任务生存时间超过该窗口（秒），才允许终态退款（默认 2 小时 = 7200s）。
/// 只要在 2 小时内，哪怕上游服务故障连续报错达到上限，也绝不提前退款，严防上游后续成功出片导致平台亏损。
const POLL_FAIL_MIN_AGE_SECS: i64 = 7200;

/// 查询前倒序休眠 4→4→3→此后 2s；若休眠会越过 `deadline` 则返回 false。
pub(crate) async fn poll_wait_before_query(attempt: u32, deadline: tokio::time::Instant) -> bool {
    let delay = 6u64.saturating_sub(attempt.max(2) as u64).max(2);
    if tokio::time::Instant::now() + std::time::Duration::from_secs(delay) > deadline {
        return false;
    }
    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
    true
}

fn pending_freeze_filter(cmp: &str) -> String {
    format!(
        "is_completed = 0 AND status_code = 200 \
         AND created_at {cmp} CURRENT_TIMESTAMP - INTERVAL '{interval}'",
        cmp = cmp,
        interval = POLL_ACTIVE_INTERVAL
    )
}

/// 查询未结算异步任务并轮询上游；连续失败达到 [`POLL_FAIL_LIMIT`] 且超过 2 小时生存期则退款终结。
/// 按全局游标 `id > ?` 环形分页分批，杜绝头部死循环垄断与长尾任务饥饿；采用受控并发流避免串行 I/O 阻塞。
async fn poll_pending_tasks(state: &Arc<AppState>) -> anyhow::Result<()> {
    refund_stale_freezes(state).await;

    let filter = pending_freeze_filter(">");
    for _ in 0..POLL_MAX_BATCHES_PER_TICK {
        let current_cursor = TASK_POLL_CURSOR.load(Ordering::Relaxed);
        let rows: Vec<(i64, String, Option<String>, String, i64)> =
            sqlx::query_as(&state.db.format_query(&format!(
                "SELECT id, model, error_message, COALESCE(task_id, ''), \
                 EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - created_at))::bigint FROM logs \
             WHERE {filter} AND id > ? ORDER BY id ASC LIMIT ?",
                filter = filter
            )))
            .bind(current_cursor)
            .bind(POLL_BATCH)
            .fetch_all(&state.db.pool)
            .await?;

        if rows.is_empty() {
            TASK_POLL_CURSOR.store(0, Ordering::Relaxed);
            break;
        }
        let batch_len = rows.len();
        if let Some(last_row) = rows.last() {
            TASK_POLL_CURSOR.store(last_row.0, Ordering::Relaxed);
        }
        crate::relay_debug!(
            "[TaskPoller] 本批待轮询 {} 条 (游标: {} -> {})",
            batch_len,
            current_cursor,
            TASK_POLL_CURSOR.load(Ordering::Relaxed)
        );

        futures::stream::iter(rows)
            .for_each_concurrent(
                CONCURRENT_POLL_LIMIT,
                |(log_id, model, error_message, db_task_id, age_secs)| {
                    let state = Arc::clone(state);
                    async move {
                        let (prev_fail_count, last_fail_status) =
                            parse_poll_fail_meta(error_message.as_deref());
                        if prev_fail_count >= POLL_FAIL_LIMIT && age_secs >= POLL_FAIL_MIN_AGE_SECS {
                            // 已达上限且生存时间满 2 小时：补退费（CAS 幂等，防上次 settle 失败后永久挂住）
                            let clean_err = error_message
                                .as_deref()
                                .and_then(|m| m.splitn(4, ':').nth(3))
                                .map(|s| s.trim_end_matches(']'))
                                .unwrap_or("达到最大轮询失败次数");
                            settle_failure(
                                &state,
                                log_id,
                                &format!("auto_poll_fail:{}", clean_err),
                                last_fail_status,
                                0,
                                None,
                            )
                            .await;
                            return;
                        }

                        crate::relay_debug!(
                            "[TaskPoller] 开始轮询 日志ID={}, 模型={}, 任务ID={}",
                            log_id,
                            model,
                            db_task_id
                        );
                        if let Err(e) = sync_single_task(&state, log_id).await {
                            let raw = e.to_string();
                            // 仅统计可重试轮询传输错；其它 anyhow（级联等）不计入，避免误退费
                            let Some((status, err_msg)) = split_poll_retry_err(&raw) else {
                                crate::relay_debug!(
                                    "[TaskPoller] 日志ID={} 轮询异常(不计入 POLL_FAIL): {}",
                                    log_id,
                                    raw
                                );
                                return;
                            };
                            let fail_count = prev_fail_count + 1;
                            crate::relay_debug!(
                                "[TaskPoller] 日志ID={} 自动轮询失败 ({}/{}): {}",
                                log_id,
                                fail_count,
                                POLL_FAIL_LIMIT,
                                err_msg
                            );
                            let _ = sqlx::query(
                                &state
                                    .db
                                    .format_query("UPDATE logs SET error_message = ? WHERE id = ?"),
                            )
                            .bind(&format_poll_fail_tag(fail_count, status, err_msg))
                            .bind(log_id)
                            .execute(&state.db.pool)
                            .await;

                            if fail_count >= POLL_FAIL_LIMIT && age_secs >= POLL_FAIL_MIN_AGE_SECS {
                                settle_failure(
                                    &state,
                                    log_id,
                                    &format!("auto_poll_fail:{}", err_msg),
                                    status,
                                    0,
                                    None,
                                )
                                .await;
                                tracing::warn!(
                                    "[TaskPoller] 日志ID={} 连续 {} 次轮询失败且超过生存时间 {}s，已终止并退款",
                                    log_id,
                                    fail_count,
                                    POLL_FAIL_MIN_AGE_SECS
                                );
                            }
                        }
                    }
                },
            )
            .await;

        if (batch_len as i64) < POLL_BATCH {
            TASK_POLL_CURSOR.store(0, Ordering::Relaxed);
            break;
        }
    }

    Ok(())
}

/// 超过活跃窗口仍未完成的冻结任务：退款终结，防止预扣永久挂住。
async fn refund_stale_freezes(state: &Arc<AppState>) {
    let filter = pending_freeze_filter("<=");
    let stale: Vec<i64> = match sqlx::query_scalar(&state.db.format_query(&format!(
        "SELECT id FROM logs WHERE {filter} ORDER BY id ASC LIMIT 50",
        filter = filter
    )))
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!("[TaskPoller] 查询僵死冻结失败: {:?}", e);
            return;
        }
    };
    if stale.is_empty() {
        return;
    }
    tracing::warn!(
        "[TaskPoller] 发现 {} 条超过 {} 仍冻结，执行兜底退款",
        stale.len(),
        POLL_ACTIVE_INTERVAL
    );
    for log_id in stale {
        settle_failure(state, log_id, "stale_freeze_timeout", 408, 0, None).await;
    }
}

// ── sync_single_task ────────────────────────────────────────────

/// 后台/手动同步单条任务（同 task_status 的 SELECT 列与级联主流程）
pub async fn sync_single_task(state: &Arc<AppState>, log_id: i64) -> anyhow::Result<String> {
    let log = sqlx::query_as::<_, TaskRelayLogRow>(&format_task_relay_sql(state, "id = ?"))
        .bind(log_id)
        .fetch_optional(&state.db.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;

    // 已完成无需再轮询
    if log.is_completed == 1 {
        return Ok("已完成".to_string());
    }
    if forward::awaiting_video_submit(&log.response_content, &log.plugin_tag) {
        return Ok("受理中".to_string());
    }

    let channel = super::router::fetch_channel(state, log.channel_id, log.channel_config_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("渠道不存在"))?;

    let entry_path = category_to_entry_path(&log.category);

    // 模型一次查出，转发规则与结算共用
    let db_model = super::proxy::find_active_model_exact(
        state,
        &log.model,
        category_hint(&log.category),
        Some(&channel),
    )
    .await;

    let resolved_by_eid = if !log.forward_eid.is_empty() {
        forward::resolve_forward_rule_by_eid(
            state,
            &log.forward_eid,
            &log.category,
            entry_path,
            db_model.as_ref(),
        )
        .await
    } else {
        None
    };

    let mut resolved = match resolved_by_eid {
        Some(r) => r,
        None => forward::resolve_forward_rule(
            state,
            &log.model,
            &log.category,
            entry_path,
            Some(&channel),
            db_model.as_ref(),
        )
        .await
        .unwrap_or_else(|| {
            forward::infer_forward_from_base_url(&channel.base_url, &log.category, None)
        }),
    };
    forward::refine_target_type(&mut resolved, &channel.base_url);
    forward::apply_channel_provider(&mut resolved, &channel);

    // 级联基因守卫：若任务历史日志中无级联标记，则强制纠偏为非级联任务，严防误触发 S2 提交流程导致退款
    if resolved.is_cascade && !cascade_plugin_tag_present(&log.plugin_tag) {
        crate::relay_debug!(
            "[TaskPoll BG] 任务缺少级联基因，强制纠偏为普通单阶段任务 id={}",
            log.task_id
        );
        resolved.is_cascade = false;
    }

    let poll_model = super::router::resolve_model(&channel, &log.model, db_model.as_ref(), None).0;
    let CascadePollPlan {
        stage: cascade_stage,
        post_resp_json,
        poll,
    } = match cascade_prepare_poll(
        resolved.is_cascade,
        &log.post_response,
        &channel,
        &resolved,
        &log.plugin_tag,
        &log.task_id,
        &poll_model,
    ) {
        Ok(plan) => plan,
        Err((err_text, status)) => {
            // 正常已结案；CAS 补洞（status 已从 stage2 推断）
            settle_failure(
                state,
                log_id,
                &err_text,
                status,
                2,
                Some(log.to_failure_meta()),
            )
            .await;
            return Ok(format!("S2 失败: {}", err_text));
        }
    };

    let UpstreamPollOk {
        url,
        body,
        resp_json,
        task_status,
    } = match run_upstream_poll(state, &log, cascade_stage, &resolved.target_type, &poll).await {
        Ok(r) => r,
        Err(UpstreamPollFail::Retryable { status, message }) => {
            return Err(anyhow::anyhow!("[poll:{}] {}", status, message));
        }
        Err(UpstreamPollFail::Settled { message, .. }) => {
            return Ok(format!("上游终态失败: {}", message));
        }
    };
    let mut body = body;
    if forward::tag_is_video_accept(&log.plugin_tag) {
        force_json_task_id(&mut body, &log.task_id);
    }

    // 组落库体与 GET 同源：腾讯云转 OpenAI 规整化；若发起端点是官方路由且成功，提前转为官方响应体（前置于级联增强）
    let is_tencent = forward::is_tencent_target(&resolved.target_type);
    let tencent_cat = if log.category.contains("视频") {
        "视频"
    } else {
        "图片"
    };
    let mut store_body = if is_tencent {
        super::response_formatter::format_openai(tencent_cat, &body, true, Some(&log.task_id))
    } else {
        body.clone()
    };
    if forward::is_official_route(&log.endpoint) && is_tencent {
        let query_path = forward::to_official_query_path(&log.endpoint, &log.task_id);
        store_body = forward::wrap_official_client(
            &query_path,
            &store_body,
            &log.model,
            &log.request_content,
        );
    }

    // S1 成功 → 提交 S2（前裁在 cascade 后台；若跳过 S2 则直接结案并放大倍率）
    let is_skip_s2 = cascade_is_skip_s2(&log.plugin_tag);
    if cascade_stage == 1 && task_status == "succeeded" {
        if !is_skip_s2 {
            let base_video_url = super::response_formatter::find_urls(&resp_json)
                .into_iter()
                .next()
                .unwrap_or_default();
            crate::relay_debug!("[Cascade S1 BG] 成功→S2 id={}", log.task_id);
            cascade_stage2_submit(
                state,
                log.to_s2_submit_ctx(&channel, &base_video_url, &store_body, &resolved),
            )
            .await;
            return Ok("S2 提交中".to_string());
        } else {
            let mul = cascade_apply_s1_res_mul(&mut store_body, &log.plugin_tag, &log.request_content, &resolved.res_mul);
            crate::relay_debug!("[Cascade S1 BG 直出] id={} 倍率={:.2} 跳过S2直接结案", log.task_id, mul);
        }
    }

    // 级联 S1：落库体统一 cgt；非级联任务：若上游响应含 model 根字段，覆写回请求模型
    if resolved.is_cascade && cascade_stage == 1 {
        force_json_task_id(&mut store_body, &log.task_id);
    } else if !resolved.is_cascade && !log.model.is_empty() {
        super::response_formatter::json_root_set(
            &mut store_body,
            "model",
            &serde_json::json!(log.model).to_string(),
        );
    }

    let mut s1_raw = if cascade_stage == 2 {
        cascade_s1_raw_from_log(&log.response_content).to_string()
    } else {
        String::new()
    };
    if task_status != "succeeded" && task_status != "failed" {
        persist_open_poll_progress(
            state,
            log_id,
            cascade_stage,
            &s1_raw,
            &store_body,
            &log.response_content,
        )
        .await;
        return Ok(format!("status={}", task_status));
    }

    // S2 成功：后裁/抽帧；未就绪或忙则回处理中，避免重复处理
    let mut _s2_guard = None;
    if cascade_stage == 2 && task_status == "succeeded" {
        _s2_guard = cascade_on_s2_succeeded(
            state,
            log_id,
            &poll,
            &mut s1_raw,
            &mut store_body,
            &log.plugin_tag,
            &log.request_content,
        )
        .await;
        if _s2_guard.is_none() {
            return Ok("S2 后处理中".to_string());
        }
    }

    // S2 客户端 callback：有 URL 才组包
    let s2_cb_url = (cascade_stage == 2)
        .then(|| super::vendor_callback::cb_from_plugin_tag(&log.plugin_tag))
        .flatten();
    let mut s2_cb_body: Option<String> = None;

    if task_status == "succeeded" {
        // TOS → 结算（级联 S2 此时已后处理完）
        if resolved.target_type != "comfyui" {
            let user_rf = super::tos_persist::response_format_from_request(&log.request_content);
            store_body = super::tos_persist::persist_for_channel(
                state,
                &channel,
                &store_body,
                &log.category,
                user_rf.as_deref(),
            )
            .await;
        }
        if cascade_stage == 2 {
            store_body = cascade_combine_stages(&s1_raw, &store_body);
            if s2_cb_url.is_some() {
                s2_cb_body = Some(cascade_format_completed_resp(
                    entry_path,
                    &log.category,
                    &store_body,
                    &log.plugin_tag,
                    &log.task_id,
                    &log.model,
                    &proxy::extract_error_message(&store_body),
                ));
            }
        }
        settle_success(
            state,
            &log,
            &store_body,
            &resp_json,
            &url,
            &channel,
            cascade_stage,
            db_model.as_ref(),
            &resolved.res_mul,
        )
        .await;
        if !log.user_id.is_empty() {
            crate::services::notification::spawn_low_balance_check(
                Arc::clone(state),
                log.user_id.clone(),
            );
        }
    } else {
        // 失败：先写日志（S2 含 post_response.stage2），再退费
        let err_text = proxy::extract_error_message(&store_body);
        let status = proxy::infer_error_status_code_from_str(&store_body);
        if cascade_stage == 2 {
            cascade_persist_s2_fail(
                state,
                log_id,
                &post_resp_json,
                &s1_raw,
                &store_body,
                &err_text,
                "Cascade S2 BG",
            )
            .await;
            if s2_cb_url.is_some() {
                s2_cb_body = Some(format_async_task_failed(
                    entry_path,
                    &log.category,
                    &log.task_id,
                    &err_text,
                ));
            }
        } else {
            persist_open_poll_response(state, log_id, &store_body, Some(&err_text)).await;
        }
        settle_failure(
            state,
            log_id,
            &url,
            status,
            cascade_stage,
            Some(log.to_failure_meta()),
        )
        .await;
    }

    // 清理级联 plugin_tag 敏感字段
    cascade_scrub_and_update_log_tag(state, log_id, &log.plugin_tag).await;
    if let (Some(u), Some(cb)) = (s2_cb_url.as_deref(), s2_cb_body.as_deref()) {
        super::vendor_callback::notify_client(state, u, cb).await;
    }
    Ok(if task_status == "succeeded" {
        "成功已计费".to_string()
    } else {
        "失败已退费".to_string()
    })
}

// ── 结算辅助函数 ────────────────────────────────────────────────

/// 任务成功：提取 token、计费、余额结算
/// cascade_stage: 级联阶段（0=非级联, 1=阶段一, 2=阶段二）
/// log_plugin_tag: 日志 plugin_tag，级联阶段二从 cascade.input_duration 获取预缓存的输入视频时长
/// res_mul: 级联分辨率倍率（stage2：有 tokens 则已乘入用量，否则乘费用）
async fn settle_success(
    state: &AppState,
    log: &TaskRelayLogRow,
    body: &str,
    resp_json: &serde_json::Value,
    poll_url: &str,
    channel: &crate::models::Channel,
    cascade_stage: u8,
    caller_model: Option<&crate::models::Model>,
    res_mul: &std::collections::HashMap<String, f64>,
) {
    let model_name = &log.model;
    let category = &log.category;

    // 级联阶段二用量取自 stage1（成功路径 usage 已 × res_mul）
    let usage_str: String;
    let usage = if cascade_stage == 2 {
        let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::json!({}));
        let s1 = parsed.get("stage1").cloned().unwrap_or(parsed.clone());
        usage_str = s1.to_string();
        super::usage_extractor::parse_usage(&usage_str)
    } else {
        super::usage_extractor::parse_usage(body)
    };

    // 复用调用方已查询的 Model，避免重复查询 models 表
    let owned_model;
    let db_model: Option<&crate::models::Model> = if let Some(m) = caller_model {
        Some(m)
    } else {
        let cat_hint = if category.is_empty() {
            None
        } else {
            Some(category.as_str())
        };
        owned_model =
            super::proxy::find_active_model_exact(state, model_name, cat_hint, Some(channel)).await;
        owned_model.as_ref()
    };

    let mut db_rule = super::proxy::get_model_billing_rule(
        state,
        model_name,
        Some(channel),
        db_model,
        &mut None,
    )
    .await;

    // 直接复用内存中已有的预扣费、billing_detail 与 billing_features，无需二次回表查询 logs
    let mut pre_deduction = log.cost;
    let mut pre_deduct_gift = log.pre_deduct_gift;
    let b_detail = log.billing_detail.as_deref();

    // 退款后重新成功：预扣费已退回用户，视为 0（全额从余额扣除）
    if b_detail.is_some_and(|d| d.contains("退回")) {
        pre_deduction = 0.0;
        pre_deduct_gift = 0.0;
    } else {
        pre_deduction = crate::money::round_money(pre_deduction);
        pre_deduct_gift = crate::money::round_money(pre_deduct_gift);
    }

    // 获取用户折扣上下文（复用 get_user_context，避免重复拼装 discount 查询）
    let ctx = if log.user_id.is_empty() {
        proxy::UserContext::from_discounts(1.0, 0, None)
    } else {
        proxy::get_user_context(state, &log.user_id)
            .await
            .unwrap_or_else(|_| proxy::UserContext::from_discounts(1.0, 0, None))
    };

    // 计费特征恢复：复用 build_poll_settlement_features 统一逻辑（内部已含 image_count 提取）
    let billing_features_str = if log.billing_features.is_empty() {
        None
    } else {
        Some(log.billing_features.clone())
    };
    let mut features =
        build_poll_settlement_features(&billing_features_str, resp_json, body, category);

    // 级联阶段二：一次解析 plugin_tag，复用 cascade 节点（时长叠加）
    let plugin_tag_val = if cascade_stage == 2 {
        serde_json::from_str::<serde_json::Value>(&log.plugin_tag).ok()
    } else {
        None
    };
    let cascade = plugin_tag_val.as_ref().and_then(|v| v.get("cascade"));
    if let Some(cascade) = cascade {
        let in_dur = cascade
            .get("input_duration")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if in_dur > 0.0 {
            if let Some(dur) = features.duration_seconds.as_mut() {
                *dur += in_dur;
            } else {
                features.duration_seconds = Some(in_dur);
            }
        }
    }

    // 映射记录：仅图片/视频沿用请求分辨率；其它类别跳过
    let map_res =
        crate::relay::router::mapping_resolution(Some(category), features.resolution.as_deref());
    let (resolved_model, mapping_source) =
        crate::relay::router::resolve_model(channel, model_name, db_model, map_res);

    let (mut cost, mut detail) = super::calculate_relay_cost(
        state,
        db_model,
        db_rule.as_mut(),
        channel,
        &ctx,
        &usage,
        &mut features,
        mapping_source.as_deref(),
        model_name,
        &resolved_model,
    )
    .await;

    // 级联阶段二或跳过增强直出：有 P/C tokens 则倍率已在用量中；无则乘底座费用（分辨率优先用 cascade 原始目标）
    let is_skip_s2 = cascade
        .and_then(|c| c.get("skip_s2")?.as_bool())
        .unwrap_or(false);
    if cascade_stage == 2 || is_skip_s2 {
        let has_tokens = usage.prompt > 0 || usage.completion > 0;
        if !has_tokens {
            let target_res = cascade
                .and_then(|c| c.get("resolution").and_then(|r| r.as_str()))
                .or(features.resolution.as_deref())
                .unwrap_or("720p");
            (cost, detail) = super::scale_cost_by_res_mul(cost, detail, res_mul, target_res);
        }
    }

    // 任务模型成功结算明确预扣判定与退补留痕（按秒视频或通用异步任务预扣）
    if pre_deduction > 0.0 {
        let is_by_second =
            category.contains("视频") || b_detail.is_some_and(|d| d.contains("按秒"));
        detail.push_str(
            &crate::money::format_settlement_note(state, cost, pre_deduction, None, is_by_second)
                .await,
        );
    }

    let updated_bf = serde_json::to_string(&features).ok();
    execute_settlement_tx(
        state,
        log,
        SettlementOutcome {
            prompt_tokens: usage.prompt as i64,
            completion_tokens: usage.completion as i64,
            cost,
            pre_deduction,
            pre_deduct_gift,
            detail: &detail,
            billing_features: updated_bf.as_deref(),
            response_content: body,
        },
    )
    .await;
    crate::relay_debug!(
        "[TaskPoller Billing] log_id={} 模型={} 费用={:.6} 预扣={:.6} tok={}+{}={} 图={:?} url={}",
        log.id,
        model_name,
        cost,
        pre_deduction,
        usage.prompt,
        usage.completion,
        usage.total,
        features.image_count,
        poll_url
    );
}

/// 任务失败：按预扣费钱包来源精准退还
/// cascade_stage: 级联阶段（0=非级联, 2=阶段二）
pub(crate) async fn settle_failure(
    state: &AppState,
    log_id: i64,
    poll_url: &str,
    status_code: u16,
    cascade_stage: u8,
    meta: Option<SettleFailureMeta<'_>>,
) {
    let fallback_uid: String;
    let failure_meta = match meta {
        Some(m) => m,
        None => {
            type SettleFailureFallbackRow =
                (f64, f64, String, Option<i64>, Option<i64>, Option<i64>);
            let log_data: Option<SettleFailureFallbackRow> =
                sqlx::query_as(&state.db.format_query(
                    "SELECT cost, pre_deduct_gift, user_id, token_id, channel_id, channel_config_id FROM logs WHERE id = ?",
                ))
                .bind(log_id)
                .fetch_optional(&state.db.pool)
                .await
                .unwrap_or(None);
            let (cost, gift, uid, tid, cid, cfg_id) =
                log_data.unwrap_or((0.0, 0.0, String::new(), None, None, None));
            fallback_uid = uid;
            SettleFailureMeta {
                cost,
                pre_deduct_gift: gift,
                user_id: &fallback_uid,
                token_id: tid,
                channel_id: cid,
                channel_config_id: cfg_id,
            }
        }
    };

    let detail = if cascade_stage == 2 {
        if failure_meta.cost > 0.0 {
            "增强失败已退费"
        } else {
            "增强失败"
        }
    } else if failure_meta.cost > 0.0 {
        "失败已退费"
    } else {
        "失败（未预扣）"
    };

    let resolved_err: Option<String> = if poll_url == "stale_freeze_timeout" {
        Some("异步任务超时未完成，系统已自动取消并退款".to_string())
    } else if let Some(rest) = poll_url.strip_prefix("auto_poll_fail:") {
        Some(super::proxy::extract_error_message(&format!(
            "多次轮询上游失败: {}",
            rest
        )))
    } else if !poll_url.starts_with("http") && !poll_url.is_empty() {
        Some(super::proxy::extract_error_message(poll_url))
    } else {
        None
    };

    execute_refund_tx(
        state,
        log_id,
        &failure_meta,
        detail,
        status_code,
        resolved_err.as_deref(),
    )
    .await;
    crate::relay_debug!(
        "[TaskPoller Failure] log_id={} 阶段={} 退款={:.6} url={} code={}",
        log_id,
        cascade_stage,
        failure_meta.cost,
        poll_url,
        status_code
    );
}

// ── 公共结算工具函数 ────────────────────────────────────────────

/// 从提交响应 JSON 字符串中提取 task_id（复用 response_formatter::extract_async_task_id 统一搜索路径）
pub fn extract_task_id(response: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(response).ok()?;
    let id = super::response_formatter::extract_async_task_id(&v);
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

// ── 状态归一化 ──────────────────────────────────────────────────

/// 统一任务状态归一化：将各厂商返回的状态字符串映射为 succeeded / failed / 原值
pub(super) fn normalize_task_status(raw: &str) -> &str {
    let standard = parse_raw_status_to_standard(raw);
    match standard {
        "completed" => "succeeded",
        "failed" => "failed",
        "pending" => "pending",
        "in_progress" => "in_progress",
        _ => {
            if raw.is_empty() {
                "pending"
            } else {
                raw
            }
        }
    }
}

// ── 腾讯云转换函数 ──────────────────────────────────────────────

/// 腾讯云 POST 响应统一转换：原始响应 → OpenAI 格式（复用 response_formatter::format_openai 统一逻辑）
/// 返回 (转换后的响应字符串, 是否为错误)
pub fn convert_tencent_post_response(raw_response: &str, category: &str) -> (String, bool) {
    let formatted = super::response_formatter::format_openai(category, raw_response, false, None);
    let is_error = formatted.contains("\"error\":");
    (formatted, is_error)
}

// ── 用户请求轮询共用的结算辅助函数 ──

/// 构建异步任务终态结算所需的计费特征
/// billing_features 快照为优先（POST 阶段保存），然后叠加终态响应中的实际数据
pub(super) fn build_poll_settlement_features(
    billing_features_str: &Option<String>,
    resp_json: &serde_json::Value,
    store_body: &str,
    category: &str,
) -> super::usage_extractor::ExtractedFeatures {
    let mut features = if let Some(ref bf_str) = billing_features_str {
        serde_json::from_str::<super::usage_extractor::ExtractedFeatures>(bf_str)
            .unwrap_or_default()
    } else {
        super::usage_extractor::ExtractedFeatures::default()
    };
    features.merge_settlement_response(resp_json, store_body, category);
    features
}

// ── 成功结算事务 ────────────────────────────────────────────────

/// 任务终态结算的产出结果（计算属性）
struct SettlementOutcome<'a> {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost: f64,
    pub pre_deduction: f64,
    pub pre_deduct_gift: f64,
    pub detail: &'a str,
    pub billing_features: Option<&'a str>,
    pub response_content: &'a str,
}

/// 成功结算事务：更新日志计费、用户余额差额、令牌配额、渠道配额；
/// 与终态 `response_content` 同 CAS，防结算后被 pending 覆盖或进程中断留下过程态。
async fn execute_settlement_tx(
    state: &crate::AppState,
    log: &TaskRelayLogRow,
    outcome: SettlementOutcome<'_>,
) {
    match state.db.pool.begin().await {
        Ok(mut tx) => {
            let (settled_cost, apply_balance) = crate::money::settlement_delta(outcome.cost, outcome.pre_deduction);

            // 原子 CAS：仅当 billing_detail 含"冻结"（首次结算）或"退回"（退款后重新结算）时才更新，
            // 且排除用户已取消(499)的记录，防止取消后轮询到 succeeded 覆盖状态码
            // 同时将 status_code 恢复为 200（退款后重新成功场景需要从 400 恢复）
            // COALESCE(?, billing_features)：None 则保留原特征快照
            let result = sqlx::query(&state.db.format_query(&format!(
                "UPDATE logs SET status_code = 200, prompt_tokens = ?, completion_tokens = ?, cached_tokens = 0, \
                 cost = ?, billing_detail = ?, billing_features = COALESCE(?, billing_features), \
                 error_message = NULL, response_content = ?, is_completed = 1, latency_ms = {latency} \
                 WHERE id = ? AND (billing_detail LIKE '%冻结%' OR billing_detail LIKE '%退回%') AND status_code != 499",
                latency = LATENCY_MS_SQL
            )))
            .bind(outcome.prompt_tokens)
            .bind(outcome.completion_tokens)
            .bind(settled_cost)
            .bind(outcome.detail)
            .bind(outcome.billing_features)
            .bind(crate::relay::proxy::sanitize_base64(outcome.response_content))
            .bind(log.id)
            .execute(&mut *tx)
            .await;

            let affected = match &result {
                Ok(r) => r.rows_affected(),
                Err(e) => {
                    tracing::warn!("[Settlement] 更新日志错误，事务回滚: {:?}", e);
                    let _ = tx.rollback().await;
                    return;
                }
            };

            if affected == 0 {
                // 另一方已抢先结算，回滚事务避免重复扣费
                let _ = tx.rollback().await;
                crate::relay_debug!("[Settlement] log_id={} 已结算，跳过", log.id);
                return;
            }

            // 更新用户账户余额、令牌配额和渠道配额，任一步骤失败都会导致整个事务回滚，确保计费落地一致性
            let res: Result<(), sqlx::Error> = async {
                // 直接复用内存中 log.model，消除事务持锁期间额外的 SELECT logs 查询与数据库往返
                let bill_model = &log.model;
                let (voucher_final, voucher_held) = crate::relay::voucher::retarget_for_log(
                    &state.db,
                    &mut tx,
                    &log.user_id,
                    log.id,
                    bill_model,
                    settled_cost,
                )
                .await?;
                let (settled_rest, pre_rest, _) = crate::money::wallet_leg(
                    settled_cost,
                    outcome.pre_deduction,
                    voucher_final,
                    voucher_held,
                );
                #[cfg(feature = "commercial_plugins")]
                let (coupon_final, coupon_held) =
                    crate::api::plugins::marketing_membership::billing::retarget_for_log(
                        state,
                        &mut tx,
                        &log.user_id,
                        log.id,
                        bill_model,
                        settled_rest,
                    )
                    .await?;
                #[cfg(not(feature = "commercial_plugins"))]
                let (coupon_final, coupon_held) = (0.0, 0.0);
                let (wallet_settled, wallet_pre, wallet_apply) = crate::money::wallet_leg(
                    settled_rest,
                    pre_rest,
                    coupon_final,
                    coupon_held,
                );
                let consumption_award = crate::services::affiliate::prepare_consumption_commission(
                    &state.db,
                    &mut tx,
                    &log.user_id,
                    wallet_settled,
                    wallet_pre,
                    outcome.pre_deduct_gift,
                    false,
                )
                .await
                .map_err(|e| sqlx::Error::Protocol(format!("commission: {e}")))?;
                if wallet_apply > 0.0 {
                    sqlx::query(&state.db.format_query(
                        "UPDATE users SET \
                         balance = CASE WHEN gift_balance >= ? THEN balance ELSE balance - (? - gift_balance) END, \
                         gift_used_quota = gift_used_quota + ? + CASE WHEN gift_balance >= ? THEN ? ELSE gift_balance END, \
                         gift_balance = CASE WHEN gift_balance >= ? THEN gift_balance - ? ELSE 0 END, \
                         used_quota = used_quota + ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"
                    ))
                    .bind(wallet_apply).bind(wallet_apply)
                    .bind(outcome.pre_deduct_gift).bind(wallet_apply).bind(wallet_apply)
                    .bind(wallet_apply).bind(wallet_apply)
                    .bind(wallet_settled)
                    .bind(&log.user_id)
                    .execute(&mut *tx).await?;
                } else if wallet_apply < 0.0 {
                    let refund = -wallet_apply;
                    let gift_cost = wallet_settled.min(outcome.pre_deduct_gift);
                    let gift_refund = outcome.pre_deduct_gift - gift_cost;
                    let balance_refund = refund - gift_refund;
                    sqlx::query(&state.db.format_query(
                        "UPDATE users SET balance = balance + ?, gift_balance = gift_balance + ?, \
                         used_quota = used_quota + ?, gift_used_quota = gift_used_quota + ?, \
                         updated_at = CURRENT_TIMESTAMP WHERE id = ?"
                    )).bind(balance_refund).bind(gift_refund)
                    .bind(wallet_settled).bind(gift_cost).bind(&log.user_id)
                    .execute(&mut *tx).await?;
                } else {
                    sqlx::query(&state.db.format_query(
                        "UPDATE users SET used_quota = used_quota + ?, gift_used_quota = gift_used_quota + ?, \
                         updated_at = CURRENT_TIMESTAMP WHERE id = ?"
                    ))
                    .bind(wallet_settled)
                    .bind(wallet_settled.min(outcome.pre_deduct_gift))
                    .bind(&log.user_id)
                    .execute(&mut *tx)
                    .await?;
                }

                if apply_balance != 0.0 {
                    // 渠道/预设：站点时区；令牌：所属用户 timedisplay（与 proxy/中间件一致）
                    let site_tz = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
                    if let Some(tid) = log.token_id {
                        let user_td = crate::api::date_helper::resolve_user_timedisplay_name(
                            &state.db, &log.user_id, &site_tz,
                        )
                        .await;
                        crate::relay::token_quota::apply_delta_with_memory(
                            state, &mut tx, tid, apply_balance, &user_td,
                        )
                        .await?;
                        crate::relay::token_quota::touch_token_last_used(state, tid);
                    }
                    if log.channel_id > 0 {
                        if apply_balance > 0.0 {
                            crate::relay::channel_quota::consume_channel(
                                &state.db, &mut tx, log.channel_id, apply_balance, &site_tz,
                            )
                            .await?;
                        } else {
                            crate::relay::channel_quota::refund_channel(
                                &state.db, &mut tx, log.channel_id, -apply_balance, &site_tz,
                            )
                            .await?;
                        }
                    }
                    // 复用内存传入的上游预设 ID，无需在事务内再次 SELECT logs
                    if let Some(cfg_id) = log.channel_config_id {
                        if cfg_id > 0 {
                            if apply_balance > 0.0 {
                                crate::relay::channel_quota::consume_config(
                                    &state.db, &mut tx, cfg_id, apply_balance, &site_tz,
                                )
                                .await?;
                            } else {
                                crate::relay::channel_quota::refund_config(
                                    &state.db, &mut tx, cfg_id, -apply_balance, &site_tz,
                                )
                                .await?;
                            }
                        }
                    }
                }
                if let Some(award) = consumption_award {
                    crate::services::affiliate::credit_consumption_commission(
                        &state.db,
                        &mut tx,
                        &log.user_id,
                        &format!("log:{}", log.id),
                        &award,
                    )
                    .await
                    .map_err(|e| sqlx::Error::Protocol(format!("commission: {e}")))?;
                }
                Ok(())
            }
            .await;

            if let Err(e) = res {
                tracing::warn!("[Settlement] 更新余额或配额失败，事务回滚: {:?}", e);
                let _ = tx.rollback().await;
            } else {
                if let Err(e) = tx.commit().await {
                    tracing::warn!("[Settlement] 提交事务失败: {:?}", e);
                } else {
                    crate::relay_debug!(
                        "[Settlement] log_id={}, cost={:.6}, applied={:.6}",
                        log.id,
                        settled_cost,
                        apply_balance
                    );
                    // 异步任务结算成功：补记实时 TPM（该路径不经 record_and_bill_inner）
                    if let Some(tid) = log.token_id {
                        let live_total = (outcome.prompt_tokens.max(0) as u64)
                            .saturating_add(outcome.completion_tokens.max(0) as u64);
                        crate::middleware::live_metrics::record_tokens(&log.user_id, tid, live_total);
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("[Settlement] 启动事务失败: {:?}", e);
        }
    }
}

// ── 失败退款事务 ────────────────────────────────────────

/// 用户主动取消：退还冻结预扣，并把日志结成 499，避免后续轮询再结算成功
pub(crate) async fn settle_user_cancelled_log(
    state: &crate::AppState,
    log_id: i64,
    task_id: &str,
    message: &str,
) {
    if log_id <= 0 {
        return;
    }
    type CancelUserLogRow =
        (String, f64, f64, Option<i64>, Option<i64>, Option<i64>, Option<String>, i64);
    let row: Option<CancelUserLogRow> =
        match sqlx::query_as(&state.db.format_query(
            "SELECT user_id, cost, pre_deduct_gift, token_id, channel_id, channel_config_id, billing_detail, status_code \
             FROM logs WHERE id = ?",
        ))
        .bind(log_id)
        .fetch_optional(&state.db.pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(
                    "[Task] 标记失败时读取日志遇到数据库异常 log_id={}: {:?}",
                    log_id,
                    e
                );
                return;
            }
        };
    let Some((
        user_id,
        pre_deduction,
        pre_deduct_gift,
        token_id,
        channel_id,
        channel_config_id,
        billing_detail,
        status_code,
    )) = row
    else {
        return;
    };
    if status_code == 499 {
        return;
    }
    let body = format_async_task_failed("/v1/video/generations", "视频", task_id, message);
    let _ = sqlx::query(&state.db.format_query(
        "UPDATE logs SET error_message = ?, response_content = ? WHERE id = ? AND is_completed = 0",
    ))
    .bind(message)
    .bind(&body)
    .bind(log_id)
    .execute(&state.db.pool)
    .await;
    let frozen = billing_detail
        .as_deref()
        .is_some_and(|d| d.contains("冻结"));
    if frozen {
        let meta = SettleFailureMeta {
            cost: pre_deduction,
            pre_deduct_gift,
            user_id: &user_id,
            token_id,
            channel_id,
            channel_config_id,
        };
        execute_refund_tx(
            state,
            log_id,
            &meta,
            "用户主动取消任务，预扣费已退回",
            499,
            Some(message),
        )
        .await;
    } else {
        let _ = sqlx::query(&state.db.format_query(
            "UPDATE logs SET status_code = 499, is_completed = 1 WHERE id = ? AND is_completed = 0",
        ))
        .bind(log_id)
        .execute(&state.db.pool)
        .await;
    }
}

/// 失败退款事务：按预扣费钱包来源精准退还余额、令牌配额、渠道配额
pub(crate) async fn execute_refund_tx(
    state: &crate::AppState,
    log_id: i64,
    meta: &SettleFailureMeta<'_>,
    detail: &str,
    status_code: u16,
    error_msg: Option<&str>,
) {
    match state.db.pool.begin().await {
        Ok(mut tx) => {
            // 原子 CAS：仅当 billing_detail 仍含"冻结"且未被用户取消(499)时才更新，防止并发双重退款
            let result = sqlx::query(&state.db.format_query(&format!(
                "UPDATE logs SET status_code = ?, cost = 0.0, pre_deduct_gift = 0.0, billing_detail = ?, error_message = COALESCE(?, error_message), is_completed = 1, latency_ms = {latency} WHERE id = ? AND billing_detail LIKE '%冻结%' AND status_code != 499",
                latency = LATENCY_MS_SQL
            )))
            .bind(status_code as i64)
            .bind(detail)
            .bind(error_msg)
            .bind(log_id)
            .execute(&mut *tx).await;

            let affected = match &result {
                Ok(r) => r.rows_affected(),
                Err(e) => {
                    tracing::warn!("[Refund] 更新日志错误，事务回滚: {:?}", e);
                    let _ = tx.rollback().await;
                    return;
                }
            };

            if affected == 0 {
                let _ = tx.rollback().await;
                crate::relay_debug!("[Refund] log_id={} 已处理，跳过", log_id);
                return;
            }

            // 更新用户退款余额、令牌配额和渠道已用额度，任何异常都会触发事务安全回滚
            let res: Result<(), sqlx::Error> =
                async {
                    let pre_deduction = meta.cost;
                    let pre_deduct_gift = meta.pre_deduct_gift;
                    let user_id = meta.user_id;
                    let token_id = meta.token_id;
                    let channel_id = meta.channel_id;
                    let channel_config_id = meta.channel_config_id;
                    if pre_deduction > 0.0 {
                        let voucher_back = crate::relay::voucher::release_holds(
                            &state.db, &mut tx, user_id, log_id,
                        )
                        .await?;
                        #[cfg(feature = "commercial_plugins")]
                        let coupon_back = crate::api::plugins::marketing_membership::billing::refund_coupon_prepay(
                            state, &mut tx, user_id, log_id,
                        )
                        .await?;
                        #[cfg(not(feature = "commercial_plugins"))]
                        let coupon_back = 0.0;
                        let (balance_refund, gift_refund) = crate::money::wallet_refund_parts(
                            pre_deduction,
                            pre_deduct_gift,
                            voucher_back + coupon_back,
                        );
                        sqlx::query(&state.db.format_query(
                        "UPDATE users SET balance = balance + ?, gift_balance = gift_balance + ?, \
                         updated_at = CURRENT_TIMESTAMP WHERE id = ?"
                    )).bind(balance_refund).bind(gift_refund).bind(user_id)
                    .execute(&mut *tx).await?;

                        let site_tz =
                            crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
                        if let Some(tid) = token_id {
                            let user_td = crate::api::date_helper::resolve_user_timedisplay_name(
                                &state.db, user_id, &site_tz,
                            )
                            .await;
                            crate::relay::token_quota::refund(
                                &state.db,
                                &mut tx,
                                tid,
                                pre_deduction,
                                &user_td,
                            )
                            .await?;
                            state.quota_memory.apply_refund(tid, pre_deduction);
                        }
                        if let Some(cid) = channel_id {
                            crate::relay::channel_quota::refund_channel(
                                &state.db,
                                &mut tx,
                                cid,
                                pre_deduction,
                                &site_tz,
                            )
                            .await?;
                        }
                        // 复用内存传入的上游预设 ID，无需在事务内再次 SELECT logs
                        if let Some(cfg_id) = channel_config_id {
                            if cfg_id > 0 {
                                crate::relay::channel_quota::refund_config(
                                    &state.db,
                                    &mut tx,
                                    cfg_id,
                                    pre_deduction,
                                    &site_tz,
                                )
                                .await?;
                            }
                        }
                    }
                    Ok(())
                }
                .await;

            if let Err(e) = res {
                tracing::warn!("[Refund] 更新退款余额或配额失败，事务回滚: {:?}", e);
                let _ = tx.rollback().await;
            } else {
                if let Err(e) = tx.commit().await {
                    tracing::warn!("[Refund] 提交事务失败: {:?}", e);
                } else {
                    crate::relay_debug!("[Refund] 日志ID={}, 退款={:.6}", log_id, meta.cost);
                }
            }
        }
        Err(e) => {
            tracing::warn!("[Refund] 启动事务失败: {:?}", e);
        }
    }
}

/// 轮询业务终态：写 error_message + failed 响应体并退费。GET / 后台共用。
async fn refund_poll_terminal(
    state: &AppState,
    log_id: i64,
    cascade_stage: u8,
    err: &str,
    status: u16,
    task_id: &str,
) {
    let body = super::response_formatter::async_task_failed_body(task_id, err);
    let _ = sqlx::query(&state.db.format_query(
        "UPDATE logs SET error_message = ?, response_content = ? WHERE id = ? AND is_completed = 0",
    ))
    .bind(err)
    .bind(&body)
    .bind(log_id)
    .execute(&state.db.pool)
    .await;
    settle_failure(state, log_id, err, status, cascade_stage, None).await;
}

/// 轮询 HTTP 已成功：清掉 POLL_FAIL 累计，避免恢复后仍被旧计数退费。
async fn clear_poll_fail(state: &AppState, log_id: i64) {
    let _ = sqlx::query(&state.db.format_query(
        "UPDATE logs SET error_message = NULL WHERE id = ? AND error_message LIKE '[POLL_FAIL:%'",
    ))
    .bind(log_id)
    .execute(&state.db.pool)
    .await;
}

/// 解析 `[poll:status] msg`；非该前缀返回 None（不计入 POLL_FAIL）。
fn split_poll_retry_err(raw: &str) -> Option<(u16, &str)> {
    let rest = raw.strip_prefix("[poll:")?;
    let (code, msg) = rest.split_once(']')?;
    let status = proxy::normalize_error_http_status(code.parse().ok()?);
    Some((status, msg.trim_start()))
}

/// 解析 `[POLL_FAIL:count]` / `[POLL_FAIL:count:status]` → (次数, 状态码)；旧格式无 status 时默认 502。
fn parse_poll_fail_meta(error_message: Option<&str>) -> (u32, u16) {
    let Some(m) = error_message.and_then(|m| m.strip_prefix("[POLL_FAIL:")) else {
        return (0, 502);
    };
    let Some((tag, _)) = m.split_once(']') else {
        return (0, 502);
    };
    let mut parts = tag.split(':');
    let count = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    let status = parts
        .next()
        .and_then(|n| n.parse().ok())
        .map(proxy::normalize_error_http_status)
        .unwrap_or(502);
    (count, status)
}

fn format_poll_fail_tag(count: u32, status: u16, msg: &str) -> String {
    format!("[POLL_FAIL:{}:{}] {}", count, status, msg)
}

// ── 轮询请求 ────────────────────────────────────────────────────

/// 轮询请求失败：HTTP 非 2xx 带真实 status；连接失败无 status（一律可重试）。
#[derive(Debug)]
pub(crate) struct PollReqErr {
    http_status: Option<u16>,
    pub(crate) message: String,
}

impl PollReqErr {
    fn transport(message: String) -> Self {
        Self {
            http_status: None,
            message,
        }
    }

    fn from_http_parts(http: u16, err_body: String) -> Self {
        let detail = proxy::extract_error_message(&err_body);
        // 有上游文案则原样；勿拼 HTTP reason phrase（会污染 OpenAI message）
        let message = if detail.is_empty() {
            format!("渠道返回错误状态码: {}", http)
        } else {
            detail
        };
        Self {
            http_status: Some(http),
            message,
        }
    }

    /// (规范化状态码, 是否可重试) — 直接用 HTTP 码，不反解析文案；500 与 429/5xx 网关同属可重试。
    fn classify(&self) -> (u16, bool) {
        match self.http_status {
            Some(s) => {
                let s = proxy::normalize_error_http_status(s);
                (s, proxy::is_poll_transport_retryable(s))
            }
            None => (proxy::infer_error_status_code_from_str(&self.message), true),
        }
    }
}

/// 轮询非 2xx：识别显式业务终态与老网关伪 500；429 与 5xx 瞬时抖动严格走传输错重试。
fn poll_non_2xx_result(
    url: String,
    http: u16,
    body: String,
) -> Result<(String, String), PollReqErr> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
        // 1. 显式带有 failed 业务终态（无论 HTTP 是 400 还是老网关透出的 500），直接结案
        if super::response_formatter::parse_raw_status_to_standard(
            &super::response_formatter::extract_raw_status(&v),
        ) == "failed" {
            return Ok((url, body));
        }

        // 2. 业务错误体且不可传输重试（或 5xx 内嵌 4xx 业务码），补充 failed 终态结案；429/5xx 瞬时抖动严禁误判
        let is_terminal_err = !proxy::is_poll_transport_retryable(http)
            || super::response_formatter::extract_error_code_from_value(&v)
                .and_then(|c| c.trim().parse::<u16>().ok())
                .is_some_and(|code| (400..500).contains(&code) && code != 429);

        if is_terminal_err && super::response_formatter::is_upstream_error_response(&v) {
            let mut v = v;
            v["status"] = serde_json::json!("failed");
            return Ok((url, v.to_string()));
        }
    }

    // 3. 瞬时网络/限流错误（429/5xx）或纯文本错误，统一走传输重试
    Err(PollReqErr::from_http_parts(http, body))
}

/// 构建轮询 URL + 鉴权 + 发送（GET/后台/`poll_task_result` 共用）。
/// 成功返回 (poll_url, body)；失败返回结构化 [`PollReqErr`]。
/// `model` 须为映射后的上游 id（与 image 同步轮询一致）。
pub(crate) async fn send_poll_request(
    state: &AppState,
    channel: &crate::models::Channel,
    resolved: &super::forward::ResolvedForward,
    task_id: &str,
    model: &str,
    jimeng_ctx: Option<(&str, &str)>, // (upstream_req_content, request_content)
) -> Result<(String, String), PollReqErr> {
    if resolved.target_type == "comfyui" {
        #[cfg(feature = "plugin_comfyui")]
        {
            let channel_tos = channel.storage_persist();
            return crate::api::plugins::comfyui_bridge::poll_video(state, task_id, channel_tos)
                .await
                .map_err(|e| PollReqErr {
                    http_status: Some(e.http_status()),
                    message: proxy::sanitize_error_message(&e.message()),
                });
        }
        #[cfg(not(feature = "plugin_comfyui"))]
        {
            return Err(PollReqErr {
                http_status: Some(400),
                message: "当前服务暂不可用".into(),
            });
        }
    }

    let http_client = &state.http_client;
    let is_tencent = forward::is_tencent_target(&resolved.target_type);
    let is_tencent_mps = resolved.target_type.starts_with("tencent_mps");
    let is_jimeng = resolved.target_type.starts_with("jimeng_");

    // 即梦AI：POST 轮询（req_key + task_id + 可选 req_json）
    if is_jimeng {
        let (ak, sk) = forward::parse_jimeng_key(&channel.api_key);
        // req_key 从 upstream_req_content 提取
        let (upstream_req_str, request_content_str) = jimeng_ctx.unwrap_or(("", ""));
        let jimeng_req_key = serde_json::from_str::<serde_json::Value>(upstream_req_str)
            .ok()
            .and_then(|v| {
                v.get("req_key")
                    .and_then(|r| r.as_str().map(|s| s.to_string()))
            })
            .unwrap_or_else(|| model.to_string());
        let mut poll_body = serde_json::json!({
            "req_key": jimeng_req_key,
            "task_id": task_id
        });
        // req_json 组装：优先用户原始请求(request_content)中的 req_json > OpenAI 参数转换 + 兜底
        // 注意：火山引擎 CV API 要求 req_json 为 JSON 字符串格式，非嵌套对象
        if let Ok(req) = serde_json::from_str::<serde_json::Value>(request_content_str) {
            if let Some(rj) = req.get("req_json") {
                // 用户直接传入的 req_json：确保为字符串格式
                poll_body["req_json"] = if rj.is_string() {
                    rj.clone()
                } else {
                    serde_json::json!(serde_json::to_string(rj).unwrap_or_default())
                };
            } else {
                let mut assembled = serde_json::Map::new();
                // return_url：即梦火山引擎原生参数。显式指定 b64_json 则返回 base64，其他情况遵循即梦默认行为返回 URL
                let return_url = if let Some(rf) = req
                    .get("response_format")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    rf != "b64_json"
                } else {
                    true // 即梦原生默认返回 URL
                };
                assembled.insert("return_url".to_string(), serde_json::json!(return_url));
                if req
                    .get("watermark")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    assembled.insert(
                        "logo_info".to_string(),
                        serde_json::json!({"add_logo": true}),
                    );
                }
                poll_body["req_json"] = serde_json::json!(serde_json::to_string(
                    &serde_json::json!(assembled)
                )
                .unwrap_or_default());
            }
        } else {
            // request_content 解析失败时仍兜底 return_url
            poll_body["req_json"] = serde_json::json!("{\"return_url\":true}");
        }
        let body_str = serde_json::to_string(&poll_body).unwrap_or_default();
        let headers = forward::build_jimeng_headers(
            ak,
            sk,
            "CVSync2AsyncGetResult",
            &body_str,
            &channel.base_url,
        );
        let poll_url = format!(
            "{}/?Action=CVSync2AsyncGetResult&Version=2022-08-31",
            channel.base_url.trim_end_matches('/')
        );
        crate::relay_debug!("[PollTask] 轮询 URL={}", poll_url);
        let mut builder = http_client
            .post(&poll_url)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(30));
        for (k, v) in headers {
            builder = builder.header(k, v);
        }
        // 使用已签名的 body_str 发送，避免 .json() 重新序列化导致签名不匹配
        let resp = builder.body(body_str).send().await.map_err(|e| {
            PollReqErr::transport(proxy::sanitize_error_message(&format!(
                "请求渠道失败: {}",
                e
            )))
        })?;
        if !resp.status().is_success() {
            let http = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return poll_non_2xx_result(poll_url, http, body);
        }
        let body = resp.text().await.unwrap_or_default();
        return Ok((poll_url, body));
    }

    let poll_path = if is_tencent {
        "/".to_string()
    } else if let Some(ref custom_path) = resolved.poll_path {
        custom_path
            .replace("${task_id}", task_id)
            .replace("${model}", model)
    } else if resolved.target_type.is_empty()
        || resolved.target_type == "openai"
        || resolved.target_type == "apimart"
    {
        format!("/v1/tasks/{}", task_id)
    } else {
        let path = resolved.upstream_path.replace("${model}", model);
        format!("{}/{}", path.trim_end_matches('/'), task_id)
    };

    let url = if is_tencent_mps {
        "https://mps.tencentcloudapi.com".to_string()
    } else if is_tencent {
        "https://vod.tencentcloudapi.com".to_string()
    } else {
        join_url(&channel.base_url, &poll_path)
    };
    crate::relay_debug!("[PollTask] 轮询 URL={}", url);

    let resp = if is_tencent {
        let (body_str, tc_headers) = if is_tencent_mps {
            let (ak, sk) = forward::parse_tencent_mps_key(&channel.api_key);
            let body_str = serde_json::to_string(&serde_json::json!({ "TaskId": task_id }))
                .unwrap_or_default();
            let headers =
                forward::build_tencent_mps_headers(ak, sk, "DescribeImageTaskDetail", &body_str);
            (body_str, headers)
        } else {
            let (ak, sk, sub_app_id) = forward::parse_tencent_vod_key(&channel.api_key);
            let body_str = serde_json::to_string(&serde_json::json!({
                "TaskId": task_id,
                "SubAppId": sub_app_id
            }))
            .unwrap_or_default();
            let headers =
                forward::build_tencent_vod_headers(ak, sk, "DescribeTaskDetail", &body_str);
            (body_str, headers)
        };
        let mut builder = http_client
            .post(&url)
            .timeout(std::time::Duration::from_secs(30));
        for (k, v) in tc_headers {
            builder = builder.header(k, v);
        }
        // 使用已签名的 body_str 发送，避免 .json() 重新序列化导致签名不匹配
        builder.body(body_str).send().await
    } else {
        let auth = forward::build_auth_headers(resolved, &channel.api_key, false);
        let mut builder = http_client
            .get(&url)
            .timeout(std::time::Duration::from_secs(30));
        for (k, v) in auth {
            builder = builder.header(k, v);
        }
        builder.send().await
    };

    let resp = resp.map_err(|e| {
        PollReqErr::transport(proxy::sanitize_error_message(&format!(
            "请求渠道失败: {}",
            e
        )))
    })?;
    let http = resp.status().as_u16();
    let ok = resp.status().is_success();
    let body = resp.text().await.unwrap_or_default();
    let (url, body) = if ok {
        (url, body)
    } else {
        poll_non_2xx_result(url, http, body)?
    };
    forward::enrich_poll_response(resolved, http_client, channel, &url, body)
        .await
        .map_err(|e| PollReqErr {
            http_status: e.http_status,
            message: proxy::sanitize_error_message(&e.message),
        })
}

// ── 通用轮询 ────────────────────────────────────────────────────

/// [`poll_task_result`] 可选参数；缺省适合 MediaKit 等无 model/即梦上下文场景。
#[derive(Clone, Copy)]
pub struct PollTaskOpts<'a> {
    pub model: &'a str,
    pub category: &'a str,
    pub timeout_secs: u64,
    /// `(upstream_req_content, request_content)`，即梦等厂商需要
    pub jimeng_ctx: Option<(&'a str, &'a str)>,
}

impl Default for PollTaskOpts<'_> {
    fn default() -> Self {
        Self {
            model: "",
            category: "",
            timeout_secs: 300,
            jimeng_ctx: None,
        }
    }
}

/// 同步轮询统一失败面：文案与状态码已在轮询侧解析完毕。
#[derive(Debug, Clone)]
pub struct PollFail {
    pub message: String,
    pub http_status: u16,
    /// 仅任务终态失败有上游响应体；请求失败 / 超时为 None
    pub body: Option<String>,
    pub timed_out: bool,
}

/// 异步任务通用轮询结果（通道测试 / 同步图 / MediaKit 共用）。
#[derive(Debug, Clone)]
pub enum PollOutcome {
    Succeeded(String),
    Failed(PollFail),
}

impl PollOutcome {
    /// 任务终态响应体（成功或 status=failed）；请求失败/超时为 None。
    pub fn into_terminal_body(self) -> Option<String> {
        match self {
            Self::Succeeded(b) => Some(b),
            Self::Failed(f) => f.body,
        }
    }
}

/// 异步任务通用轮询（供通道测试 / 同步图 / 裁剪等场景复用）。
/// 仅轮询上游获取终态响应，不执行计费结算。
///
/// 策略：每次查询前倒序休眠 4→4→3→此后 2s；可重试错误连续达 [`POLL_FAIL_LIMIT`] 终止；
/// 不可重试（如 400）立即失败并保留上游文案，不伪装成超时。
pub async fn poll_task_result(
    state: &AppState,
    channel: &crate::models::Channel,
    resolved: &super::forward::ResolvedForward,
    task_id: &str,
    opts: PollTaskOpts<'_>,
) -> PollOutcome {
    let is_tencent = forward::is_tencent_target(&resolved.target_type);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(opts.timeout_secs);
    let mut consecutive_errors: u32 = 0;
    let mut attempt: u32 = 0;

    crate::relay_debug!(
        "[PollTask] 开始轮询 任务ID={}, 超时={}秒",
        task_id,
        opts.timeout_secs
    );

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        attempt += 1;
        if !poll_wait_before_query(attempt, deadline).await {
            break;
        }

        match send_poll_request(
            state,
            channel,
            resolved,
            task_id,
            opts.model,
            opts.jimeng_ctx,
        )
        .await
        {
            Ok((_url, body)) => {
                consecutive_errors = 0;
                let resp_json: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or(serde_json::json!({}));
                let raw_status = super::response_formatter::extract_raw_status(&resp_json);
                let task_status = normalize_task_status(&raw_status);

                crate::relay_debug!(
                    "[PollTask] 轮询第 {} 次, 任务ID={}, 状态={}",
                    attempt,
                    task_id,
                    task_status
                );

                match task_status {
                    "succeeded" | "failed" => {
                        crate::relay_debug!(
                            "[PollTask] 终态 任务ID={}, 状态={}, 响应长度={}",
                            task_id,
                            task_status,
                            body.len()
                        );
                        let store_body = if is_tencent {
                            crate::relay_debug!(
                                "[PollTask] 腾讯云原始响应 任务ID={}, 响应体={}",
                                task_id,
                                body
                            );
                            super::response_formatter::format_openai(
                                opts.category,
                                &body,
                                true,
                                Some(task_id),
                            )
                        } else {
                            body
                        };
                        if task_status == "succeeded" {
                            return PollOutcome::Succeeded(store_body);
                        }
                        return PollOutcome::Failed(PollFail {
                            message: super::response_formatter::extract_error_message(&resp_json),
                            http_status: proxy::infer_error_status_code(&resp_json),
                            body: Some(store_body),
                            timed_out: false,
                        });
                    }
                    _ => {}
                }
            }
            Err(e) => {
                let (status, retryable) = e.classify();
                if !retryable {
                    crate::relay_debug!(
                        "[PollTask] 不可重试错误，放弃轮询 任务ID={}, status={}, err={}",
                        task_id,
                        status,
                        e.message
                    );
                    return PollOutcome::Failed(PollFail {
                        message: e.message,
                        http_status: status,
                        body: None,
                        timed_out: false,
                    });
                }
                consecutive_errors += 1;
                crate::relay_debug!(
                    "[PollTask] 轮询请求失败 ({}/{}): {} (任务ID={})",
                    consecutive_errors,
                    POLL_FAIL_LIMIT,
                    e.message,
                    task_id
                );
                if consecutive_errors >= POLL_FAIL_LIMIT {
                    crate::relay_debug!(
                        "[PollTask] 连续 {} 次请求失败，放弃轮询 任务ID={}",
                        POLL_FAIL_LIMIT,
                        task_id
                    );
                    return PollOutcome::Failed(PollFail {
                        message: e.message,
                        http_status: status,
                        body: None,
                        timed_out: false,
                    });
                }
            }
        }
    }

    // deadline：任务长期非终态（与请求失败 / 达上限不同）
    crate::relay_debug!(
        "[PollTask] 轮询超时 任务ID={}, 已尝试 {} 次",
        task_id,
        attempt
    );
    PollOutcome::Failed(PollFail {
        message: format!("任务处理超时，请稍后查询结果，任务ID: {}", task_id),
        http_status: 504,
        body: None,
        timed_out: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poll_429_json_rate_limit_must_retry() {
        let body = r#"{"error":{"message":"Rate limit reached","type":"rate_limit_error","code":"rate_limit_exceeded"}}"#;
        let res = poll_non_2xx_result("http://api.test/poll".to_string(), 429, body.to_string());
        assert!(res.is_err(), "429 速率限制必须走传输重试（Err），不得误判为业务终态结案");
        let (code, retryable) = res.unwrap_err().classify();
        assert_eq!(code, 429);
        assert!(retryable);
    }

    #[test]
    fn test_poll_5xx_json_transient_error_must_retry() {
        let body = r#"{"error":{"code":500,"message":"internal server error"}}"#;
        let res = poll_non_2xx_result("http://api.test/poll".to_string(), 500, body.to_string());
        assert!(res.is_err(), "500 临时抖动必须走传输重试（Err），不得提前终态结案退款");
        let (code, retryable) = res.unwrap_err().classify();
        assert_eq!(code, 500);
        assert!(retryable);
    }

    #[test]
    fn test_poll_explicit_failed_status_settles_even_on_5xx() {
        let body = r#"{"id":"task-1","status":"failed","error":{"message":"content violation"}}"#;
        let res = poll_non_2xx_result("http://api.test/poll".to_string(), 500, body.to_string());
        assert!(res.is_ok(), "显式带 status: failed 的响应必须直接作为终态成功结案返回");
        let (_url, out_body) = res.unwrap();
        assert!(out_body.contains(r#""status":"failed""#));
    }

    #[test]
    fn test_poll_5xx_with_embedded_4xx_business_code_settles() {
        let body = r#"{"error":{"code":"400","message":"prompt must not be empty"}}"#;
        let res = poll_non_2xx_result("http://api.test/poll".to_string(), 500, body.to_string());
        assert!(res.is_ok(), "老网关以 500 透出的 400 业务错误必须作为终态结案退款，避免死锁");
        let (_url, out_body) = res.unwrap();
        assert!(out_body.contains(r#""status":"failed""#));
    }

    #[test]
    fn test_poll_4xx_business_error_still_settles() {
        let body = r#"{"error":{"message":"task not found","code":"not_found"}}"#;
        let res = poll_non_2xx_result("http://api.test/poll".to_string(), 404, body.to_string());
        assert!(res.is_ok(), "常规 404 业务错误必须作为终态结案退款");
        let (_url, out_body) = res.unwrap();
        assert!(out_body.contains(r#""status":"failed""#));
    }
}
