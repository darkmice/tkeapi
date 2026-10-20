/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! Relay: POST /v1/video/generations（对外主入口）；POST /v1/videos 与 POST /v1/videos/generations 为 OpenAI 兼容别名
//! OpenAI-compatible video generation task endpoint with forward-rule-driven protocol adaptation.

use super::cascade::{cascade_check_resolution, cascade_resolve_base};
use super::{forward, proxy, router, upstream_headers};
#[cfg(feature = "plugin_volcengine_enhance")]
use crate::api::plugins::{
    apply_volc_enhance_path, link_volcengine_enhance_log, lock_volc_cascade_s2_channel,
    resolve_volc_video_model, volc_cascade_s1_tag,
};
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

const KLING_NATIVE_PREFIXES: [&str; 3] = ["/text-to-video/", "/image-to-video/", "/omni-video/"];

/// POST /v1/video/generations — Submit a video generation task
pub async fn video_generations(
    State(state): State<Arc<AppState>>,
    Extension(token): Extension<ApiToken>,
    OriginalUri(uri): OriginalUri,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Response> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    // 计数在任务入队前加上，关闭流程不会把「已受理、还没跑到第一行」的提交看成空闲
    let guard = state.video_submit_drain.enter();
    tokio::spawn(async move {
        let _guard = guard;
        video_generations_job(state, token, uri, headers, body, tx).await;
    });
    match rx.await {
        Ok(r) => r,
        Err(_) => Err(AppError::Internal("视频受理中断".into())),
    }
}

async fn video_generations_job(
    state: Arc<AppState>,
    token: ApiToken,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    mut body: serde_json::Value,
    ack_tx: tokio::sync::oneshot::Sender<AppResult<Response>>,
) {
    let mut ack_tx = Some(ack_tx);
    macro_rules! bail {
        ($e:expr) => {{
            if let Some(tx) = ack_tx.take() {
                let _ = tx.send(Err($e));
            }
            return;
        }};
    }
    let req_start = std::time::Instant::now();
    let log_id = headers
        .get("x-log-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| upstream_headers::resolve_request_log_id(Some("视频")));
    let client_task_id = super::response_formatter::new_cgt_task_id();
    let raw_path = uri.path();
    let is_kling_native = KLING_NATIVE_PREFIXES
        .iter()
        .any(|p| raw_path.starts_with(p));
    let is_vidu_native = raw_path.starts_with("/ent/v2/");

    // 归一化到兼容入口，便于匹配转发规则 path_rewrite.old=/v1/video/generations：
    // - OpenAI Videos：/v1/videos
    // - 可灵官方 3.0 / Omni 原生路径：/text-to-video/*、/image-to-video/*、/omni-video/*
    // - Vidu 官方路径：/ent/v2/*
    let entry_path = if raw_path == "/v1/videos"
        || raw_path.starts_with("/v1/videos/")
        || is_kling_native
        || is_vidu_native
    {
        "/v1/video/generations".to_string()
    } else {
        raw_path.to_string()
    };
    let request_content_str = serde_json::to_string(&body).unwrap_or_default();
    let mut model_opt = body["model"]
        .as_str()
        .or_else(|| body["model_name"].as_str())
        .map(|s| s.to_string());

    // 当访问可灵官方原生路由时，自动从 URL 路径中提取可变模型 ID (${model_id})
    if model_opt.is_none() && is_kling_native {
        for prefix in KLING_NATIVE_PREFIXES {
            if let Some(rest) = raw_path.strip_prefix(prefix) {
                let m = rest.split('/').next().unwrap_or("").trim();
                if !m.is_empty() {
                    model_opt = Some(m.to_string());
                    body["model"] = serde_json::json!(m);
                    break;
                }
            }
        }
    }

    #[cfg_attr(not(feature = "plugin_volcengine_enhance"), allow(unused_mut))]
    let mut category = "视频";
    let mut db_model = None;

    #[cfg(feature = "plugin_volcengine_enhance")]
    if model_opt.is_none() {
        match resolve_volc_video_model(&state, &token, raw_path, body["tool_version"].as_str())
            .await
        {
            Ok(Some(model_data)) => {
                model_opt = Some(model_data.model_id.clone());
                db_model = Some(model_data);
                category = "封装";
            }
            Ok(None) => {}
            Err(e) => bail!(e),
        }
    }

    let Some(model_str) = model_opt else {
        bail!(AppError::BadRequest(
            "Missing required parameter: model".to_string()
        ));
    };
    let model = model_str.as_str();

    // 1. Token 模型权限校验（渠道选择前快速拦截）
    if let Err(e) =
        proxy::check_model_permission(&state, &token, model, &entry_path, Some(category)).await
    {
        bail!(e);
    }

    let ctx = match proxy::get_user_context(&state, &token.user_id).await {
        Ok(v) => v,
        Err(e) => bail!(e),
    };

    let mut billing_rule_cache = None;
    let mut access_cache = None;

    let mut ha = crate::relay::ha::HaAttempt::begin(&state, token.high_availability, req_start).await;

    while ha.cont() {
        let attempt_start = std::time::Instant::now();
        // 2. 渠道选择
        let mut ha_pool = None;
        let channel = match proxy::select_channel_with_db(
            &state,
            &token,
            model,
            &ctx.user_group,
            &ctx.level_id,
            &entry_path,
            db_model.as_ref(),
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

        // 3. 预扣费检查（带 channel 精确匹配同名模型的预扣费金额，同时获取 Model 供下游复用）
        let (mut pre_deduction, ch_model, resolved_cat) = match proxy::check_access_with_model(
            &state,
            &token,
            model,
            &ctx,
            Some(category),
            &channel,
            db_model.as_ref(),
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
        if ch_model.is_some() {
            db_model = ch_model;
        }

        // 转发规则（复用 db_model 避免重查 models 表）
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
                        &raw_path,
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
        // 根据渠道 base_url 修正 target_type（如 APIMart 需从 "openai" 覆盖为 "apimart"）
        forward::refine_target_type(&mut resolved, &channel.base_url);
        forward::apply_channel_provider(&mut resolved, &channel);
        #[cfg(feature = "plugin_volcengine_enhance")]
        if resolved.target_type == "volcengine_media_enhance" {
            apply_volc_enhance_path(&mut resolved, model);
        }

        // 模型映射：视频走分辨率档（resolve_model_body）；依赖本轮 Channel/子配，不可环外缓存
        let (final_resolved_model, mapping_source) =
            router::resolve_model_body(&channel, model, db_model.as_ref(), Some(&body));

        let db_rule = proxy::get_model_billing_rule(
            &state,
            model,
            Some(&channel),
            db_model.as_ref(),
            &mut billing_rule_cache,
        )
        .await;
        let mut upstream_body: serde_json::Value = forward::transform_request_body(
            &resolved,
            &final_resolved_model,
            &body,
            &resolved_cat,
            db_rule.as_ref(),
            Some(&state.http_client),
        )
        .await;

        // 视频模型动态预扣费：自预测（<=0 如火山-1）按10秒预扣防透支，缺省5秒，正数按实际秒数
        let req_features = crate::relay::usage_extractor::extract_request_features(&upstream_body);
        let duration_seconds = req_features
            .duration_seconds
            .map_or(5.0, |d| if d <= 0.0 { 10.0 } else { d.max(1.0) });
        let unit_pre = pre_deduction;
        if unit_pre > 0.0 {
            pre_deduction = crate::money::round_money(unit_pre * duration_seconds);
            if ctx.balance < pre_deduction {
                let currency_unit = crate::api::settings::get_currency_settings(&state)
                    .await
                    .currency_unit;
                let err_msg = format!(
                    "账户余额不足，视频生成按秒预扣冻结：{}{}/秒 × {:.0}秒 = {}{}",
                    crate::money::format_clean_money(unit_pre),
                    currency_unit,
                    duration_seconds,
                    crate::money::format_clean_money(pre_deduction),
                    currency_unit
                );
                proxy::record_error_log(
                    &state,
                    &token,
                    Some(channel.id),
                    model,
                    402,
                    &raw_path,
                    &resolved_cat,
                    &err_msg,
                    Some(&channel.base_url),
                )
                .await;
                ha.on_access_err(AppError::PaymentRequired(err_msg));
                break;
            }
        }

        macro_rules! log_video_pre_err {
            ($msg:expr) => {
                proxy::record_error_log(
                    &state,
                    &token,
                    Some(channel.id),
                    model,
                    400,
                    &raw_path,
                    &resolved_cat,
                    $msg,
                    Some(&channel.base_url),
                )
                .await
            };
        }

        // 通用视频模型分辨率校验（有分辨率计费配置且未开启时拦截，非分辨率计费全放行）
        if !resolved.is_cascade {
            if let Some(res_str) = body
                .get("resolution")
                .or_else(|| upstream_body.get("resolution"))
                .and_then(|v| v.as_str())
            {
                if let Err(e) = cascade_check_resolution(db_rule.as_ref(), "", res_str) {
                    let msg = e.to_string();
                    log_video_pre_err!(&msg);
                    ha.on_access_err(e);
                    break;
                }
            }
        }

        // 级联超分：增强档优先转发规则 res_enhance，缺省标准版；忽略请求体 version
        let mut cascade_tag_json = None;
        if resolved.is_cascade {
            let is_req_mov = body
                .get("output_format")
                .or_else(|| upstream_body.get("output_format"))
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.trim().eq_ignore_ascii_case("mov"));

            if is_req_mov && resolved.cascade_engine == "tencent" {
                #[cfg(feature = "plugin_tencent_enhance")]
                {
                    let mov_check = match crate::api::plugins::tencent_enhance::load_ready_config(&state).await {
                        Ok(cfg) if cfg.template_transcode_mov > 0 => Ok(()),
                        Ok(_) => crate::api::plugins::tencent_enhance::ensure_mov_transcode_template(&state)
                            .await
                            .map(|_| ())
                            .map_err(|_| "未开启 mov格式".to_string()),
                        Err(e) => Err(e),
                    };
                    if let Err(msg) = mov_check {
                        log_video_pre_err!(&msg);
                        ha.on_access_err(AppError::BadRequest(msg));
                        break;
                    }
                }
            }

            let target_key = body
                .get("resolution")
                .and_then(|v| v.as_str())
                .or_else(|| upstream_body.get("resolution").and_then(|r| r.as_str()))
                .unwrap_or("720p")
                .trim()
                .to_ascii_lowercase();
            let enhance_enabled = resolved.is_res_enhance_enabled(&target_key);
            let mut cascade_val: serde_json::Value = if !enhance_enabled {
                serde_json::json!({
                    "resolution": target_key,
                    "skip_s2": true,
                })
            } else if resolved.cascade_engine == "tencent" {
                #[cfg(feature = "plugin_tencent_enhance")]
                {
                    match crate::api::plugins::tencent_cascade_s1_tag(&state, &target_key, &resolved.res_limit).await {
                        Ok(v) => v,
                        Err(e) => {
                            let msg = e.to_string();
                            log_video_pre_err!(&msg);
                            ha.on_access_err(e);
                            break;
                        }
                    }
                }
                #[cfg(not(feature = "plugin_tencent_enhance"))]
                {
                    let msg = "增强插件未编译";
                    log_video_pre_err!(msg);
                    ha.on_access_err(AppError::BadRequest(msg.to_string()));
                    break;
                }
            } else {
                #[cfg(not(feature = "plugin_volcengine_enhance"))]
                {
                    let msg = "增强插件未编译";
                    log_video_pre_err!(msg);
                    ha.on_access_err(AppError::BadRequest(msg.to_string()));
                    break;
                }
                #[cfg(feature = "plugin_volcengine_enhance")]
                {
                    volc_cascade_s1_tag(&target_key, &resolved.res_enhance, &resolved.res_scene, &resolved.res_limit, &resolved.res_limit_fallback)
                }
            };
            if is_req_mov {
                cascade_val["output_format"] = serde_json::json!("mov");
            }
            let cascade_version = cascade_val
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if let Err(e) = cascade_check_resolution(
                db_rule.as_ref(),
                if resolved.cascade_engine == "tencent" || !enhance_enabled {
                    ""
                } else {
                    &cascade_version
                },
                &target_key,
            ) {
                let msg = e.to_string();
                log_video_pre_err!(&msg);
                ha.on_access_err(e);
                break;
            }
            let base_res = match cascade_resolve_base(&target_key, &resolved.res_base) {
                Ok(b) => b,
                Err(e) => {
                    let msg = e.to_string();
                    log_video_pre_err!(&msg);
                    ha.on_access_err(e);
                    break;
                }
            };
            upstream_body["resolution"] = serde_json::json!(base_res);
            if !enhance_enabled {
                crate::relay_debug!(
                    "[Cascade 直出] 模型: {}, 目标: {}, 底座: {}, 跳过S2增强(最终按倍率计费)",
                    model,
                    target_key,
                    base_res
                );
            } else if resolved.cascade_engine == "tencent" {
                crate::relay_debug!(
                    "[Cascade] 模型: {}, 引擎: tencent, 目标: {}, 底座: {}",
                    model,
                    target_key,
                    base_res
                );
            } else {
                crate::relay_debug!(
                    "[Cascade] 模型: {}, 增强: {}({}), 目标: {}, 底座: {}, 场景: {}",
                    model,
                    cascade_version,
                    cascade_val
                        .get("mid")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    target_key,
                    base_res,
                    cascade_val
                        .get("scene")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-")
                );
            }
            #[cfg(feature = "plugin_volcengine_enhance")]
            if enhance_enabled && resolved.cascade_engine != "tencent" {
                match lock_volc_cascade_s2_channel(&state, &token, &ctx, &entry_path, cascade_val)
                    .await
                {
                    Ok(v) => cascade_val = v,
                    Err(e) => {
                        let msg = e.to_string();
                        log_video_pre_err!(&msg);
                        ha.on_access_err(e);
                        break;
                    }
                }
            }
            cascade_tag_json = Some(cascade_val);
        }

        // 可灵动态路径：根据请求体内容调整实际端点（text-to-video/image-to-video）
        forward::resolve_kling_dynamic_path(&mut resolved, &upstream_body);

        // Vidu 动态路径选路：官方原生端点直接复用，OpenAI/非官方路由基于前置计费特征动态推导
        if resolved.target_type == "vidu" {
            if is_vidu_native {
                resolved.upstream_path = raw_path.to_string();
            } else {
                forward::resolve_vidu_dynamic_path(&mut resolved, &body, Some(&req_features));
            }
        }

        let url = forward::build_upstream_url(
            &channel.base_url,
            &resolved,
            &final_resolved_model,
            &channel.api_key,
        );

        // 【一条日志原则】请求前预记录日志（model 记录用户请求的第1个模型 ID）
        let ep = format!(
            "{}|{}",
            raw_path,
            resolved
                .upstream_path
                .replace("${model}", &final_resolved_model)
        );

        // plugin_tag：级联 / 客户端 callback（上游体根级 callback_url；与入口是否 OpenAI 无关）
        // 火山等官方参数可经 OpenAI 路由透传到 upstream_body
        let client_cb = super::vendor_callback::extract_client_callback_url(&upstream_body);
        let plugin_tag: Option<String> = {
            let mut tag_json = serde_json::json!({});
            tag_json["la"] = serde_json::json!(1);
            if let Some(cascade_val) = cascade_tag_json {
                tag_json["cascade"] = cascade_val;
            }
            if let Some(ref u) = client_cb {
                super::vendor_callback::stash_cb_in_plugin_tag(&mut tag_json, u);
            }
            Some(tag_json.to_string())
        };

        if resolved.upstream_asset_convert && resolved.upstream_asset_binding_id.is_none() {
            let msg = "转发规则缺少 upstream_asset_binding_id";
            log_video_pre_err!(msg);
            ha.on_access_err(AppError::BadRequest(msg.to_string()));
            break;
        }

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
                    plugin_tag: plugin_tag.as_deref(),
                    category: Some(resolved_cat.as_str()),
                    db_model: db_model.as_ref(),
                    forward_eid: Some(&resolved.eid),
                    requested_log_id: Some(&log_id),
                    task_id: (resolved.fast_response || resolved.is_cascade)
                        .then_some(client_task_id.as_str()),
                })
                .await,
            );
            #[cfg(feature = "plugin_volcengine_enhance")]
            if resolved.target_type == "volcengine_media_enhance" {
                if let Some(pk) = ha.pending_log_id {
                    link_volcengine_enhance_log(&state, pk).await;
                }
            }
        }

        // 上游体根级有 callback_url：改写为系统地址（logs 主键 id）；原地址已存 plugin_tag.cb
        if let (Some(_), Some(pk)) = (&client_cb, ha.pending_log_id) {
            let sys = super::vendor_callback::system_callback_url(pk, &headers);
            super::vendor_callback::rewrite_upstream_callback(&mut upstream_body, &sys);
        }

        if resolved.fast_response && ha.pending_log_id.is_some() {
            if let Some(tx) = ack_tx.take() {
                let ack_body = super::forward::route_accept_body(raw_path, &client_task_id, model);
                let ack = upstream_headers::with_request_id(
                    axum::response::Response::builder()
                        .status(200)
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(ack_body))
                        .unwrap(),
                    &log_id,
                );
                let _ = tx.send(Ok(ack));
            }
        }

        // 【连接保护】素材转换+上游请求+预扣+落库放独立 task，客户端断开后仍能完成
        let pending_log_id = ha.pending_log_id;
        let mapping_detail: Option<String> =
            mapping_source.map(|src| format!("{}: {} ➞ {}", src, model, final_resolved_model));
        let timeout_ctx = ha.timeout_ctx();
        let fail_buf = ha.buf();

        let result_rx = super::spawn_protected({
            let state = state.clone();
            let token = token.clone();
            #[cfg_attr(not(feature = "plugin_comfyui"), allow(unused_mut))]
            let mut channel = channel.clone();
            let ep = ep.clone();
            let mut upstream_body = upstream_body.clone();
            let url = url.clone();
            let mut plugin_tag = plugin_tag.clone();
            let model = model.to_string();
            let raw_path = raw_path.to_string();
            let resolved_cat = resolved_cat.clone();
            let request_content_str = request_content_str.clone();
            let dm = db_model.clone();
            let resolved = resolved.clone();
            let db_rule = db_rule.clone();
            let body = body.clone();
            let client_task_id = client_task_id.clone();
            async move {
                macro_rules! open_fail {
                    ($status:expr, $body:expr, $detail:expr) => {{
                        let _ = proxy::record_zero_cost_fail(proxy::ZeroCostUpstreamFail {
                            state: &state,
                            token: &token,
                            channel: &channel,
                            model: &model,
                            prefer_http_status: Some($status),
                            endpoint: &ep,
                            latency_ms: req_start.elapsed().as_millis() as u32,
                            is_stream: 0,
                            request_content: request_content_str.clone(),
                            response_body: $body,
                            response_content: None,
                            upstream_req_content: None,
                            billing_detail: $detail,
                            hint_category: Some(resolved_cat.as_str()),
                            pending_log_id,
                            db_model: dm.as_ref(),
                            client_msg: None,
                            pre_deducted: 0.0,
                            pre_deduct_gift: 0.0,
                            upstream_request_id: None,
                        })
                        .await;
                    }};
                }
                if resolved.is_cascade
                    && db_rule
                        .as_ref()
                        .is_some_and(|r| r.billing_rule == "volc_enhance_cascade")
                {
                    let video_urls = proxy::extract_request_video_urls(&body);
                    if !video_urls.is_empty() {
                        match proxy::sum_remote_media_duration(&state.http_client, &video_urls)
                            .await
                        {
                            Ok(dur) if dur > 0.0 => {
                                if let Some(ref mut tag) = plugin_tag {
                                    if let Ok(mut v) =
                                        serde_json::from_str::<serde_json::Value>(tag)
                                    {
                                        if let Some(c) = v.get_mut("cascade") {
                                            c["input_duration"] = serde_json::json!(dur);
                                        }
                                        *tag = v.to_string();
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                open_fail!(400, e.clone(), None);
                                return Err(AppError::BadRequest(e));
                            }
                        }
                    }
                }
                let pre_features = if db_rule.as_ref().is_some_and(|r| {
                    r.billing_rule == "fal_ref_video"
                        || r.billing_rule == "minimax_h3"
                        || r.billing_rule == "video_seconds_io"
                        || r.billing_rule == "video_seconds_ref"
                }) {
                    let mut feat =
                        crate::relay::usage_extractor::extract_request_features(&upstream_body);
                    let video_urls = proxy::extract_request_video_urls(&upstream_body);
                    let audio_urls = proxy::extract_request_audio_urls(&upstream_body);
                    if !video_urls.is_empty() || !audio_urls.is_empty() {
                        match tokio::try_join!(
                            proxy::sum_remote_media_duration(&state.http_client, &video_urls),
                            proxy::sum_remote_media_duration(&state.http_client, &audio_urls)
                        ) {
                            Ok((v_dur, a_dur)) => {
                                if v_dur > 0.0 {
                                    feat.video_ref_seconds = Some(v_dur);
                                }
                                if a_dur > 0.0 {
                                    feat.audio_ref_seconds = Some(a_dur);
                                }
                            }
                            Err(e) => {
                                open_fail!(400, e.clone(), None);
                                return Err(AppError::BadRequest(e));
                            }
                        }
                    }
                    Some(feat)
                } else {
                    None
                };
                // 素材转换：上游渠道转换优先；否则走现有插件凭证转换
                let mut asset_convert_log: Option<String> = None;
                if resolved.upstream_asset_convert {
                    if let Some(binding_id) = resolved.upstream_asset_binding_id {
                        let (convert_logs, convert_errors) =
                            super::asset_convert::convert_content_urls_via_upstream(
                                &state,
                                &token.user_id,
                                binding_id,
                                &mut upstream_body,
                            )
                            .await;
                        if !convert_logs.is_empty() {
                            asset_convert_log =
                                Some(format!("上游素材转换: {}", convert_logs.join(" | ")));
                        }
                        if !convert_errors.is_empty() {
                            let full_err = convert_errors.join("; ");
                            let user_msg = convert_errors
                                .iter()
                                .map(|e| proxy::extract_error_message(e))
                                .collect::<Vec<_>>()
                                .join("; ");
                            let status_code = match proxy::infer_error_status_code_from_str(&full_err) {
                                s if (400..500).contains(&s) => s,
                                _ => 400,
                            };
                            open_fail!(status_code, full_err, asset_convert_log.clone());
                            return Err(AppError::BadRequest(user_msg));
                        }
                    }
                } else if resolved.asset_convert {
                    let (convert_logs, convert_errors) =
                        super::asset_convert::convert_content_urls(
                            &state,
                            &token.user_id,
                            &resolved.asset_convert_ns,
                            &mut upstream_body,
                            resolved.asset_moderation,
                        )
                        .await;
                    if !convert_logs.is_empty() {
                        asset_convert_log = Some(format!("素材转换: {}", convert_logs.join(" | ")));
                    }
                    if !convert_errors.is_empty() {
                        let full_err = convert_errors.join("; ");
                        let user_msg = convert_errors
                            .iter()
                            .map(|e| proxy::extract_error_message(e))
                            .collect::<Vec<_>>()
                            .join("; ");
                        let status_code = match proxy::infer_error_status_code_from_str(&full_err) {
                            s if (400..500).contains(&s) => s,
                            _ => 400,
                        };
                        open_fail!(status_code, full_err, asset_convert_log.clone());
                        return Err(AppError::BadRequest(format!("素材转换失败: {}", user_msg)));
                    }
                }

                #[cfg(feature = "plugin_comfyui")]
                let mut comfy_prompt_json: Option<String> = None;
                let (mut response_content_str, upstream_hdrs) = if resolved.target_type == "comfyui"
                {
                    #[cfg(feature = "plugin_comfyui")]
                    {
                        let target =
                            match crate::api::plugins::comfyui_bridge::resolve_submit_target(
                                &state,
                                &upstream_body,
                                &channel,
                            )
                            .await
                            {
                                Ok(v) => v,
                                Err(e) => {
                                    let msg = e.message();
                                    let bill = crate::relay::ha::FailBill::biz(
                                        attempt_start.elapsed().as_millis() as u32,
                                        msg.clone(),
                                        msg,
                                        request_content_str.clone(),
                                        String::new(),
                                    );
                                    return Err(crate::relay::ha::HaAttempt::park(
                                        &fail_buf, bill, None,
                                    ));
                                }
                            };
                        match crate::api::plugins::comfyui_bridge::submit_video(
                            Arc::clone(&state),
                            &upstream_body,
                            &target,
                            pending_log_id,
                            Some(token.user_id.as_str()),
                        )
                        .await
                        {
                            Ok(ack) => {
                                channel.base_url = ack.base_url.clone();
                                let body = ack.openai_body();
                                comfy_prompt_json = Some(ack.prompt_json);
                                (body, axum::http::HeaderMap::new())
                            }
                            Err(e) => {
                                let msg = e.error.message();
                                let bill = crate::relay::ha::FailBill::http(
                                    attempt_start.elapsed().as_millis() as u32,
                                    e.error.http_status(),
                                    msg.clone(),
                                    request_content_str.clone(),
                                    e.prompt_json,
                                )
                                .client(msg);
                                return Err(crate::relay::ha::HaAttempt::park(
                                    &fail_buf, bill, None,
                                ));
                            }
                        }
                    }
                    #[cfg(not(feature = "plugin_comfyui"))]
                    {
                        let bill = crate::relay::ha::FailBill::biz(
                            attempt_start.elapsed().as_millis() as u32,
                            "当前服务暂不可用",
                            "当前服务暂不可用",
                            request_content_str.clone(),
                            String::new(),
                        );
                        return Err(crate::relay::ha::HaAttempt::park(&fail_buf, bill, None));
                    }
                } else {
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
                            )
                            .detail_opt(asset_convert_log.clone());
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
                        )
                        .detail_opt(asset_convert_log.clone());
                        return Err(crate::relay::ha::HaAttempt::park(
                            &fail_buf,
                            bill,
                            Some(upstream_hdrs),
                        ));
                    }

                    let upstream_hdrs = resp.headers().clone();
                    let data = resp.bytes().await.unwrap_or_default();
                    let mut body_str = String::from_utf8_lossy(&data).to_string();
                    let (converted, mut post_err) = forward::check_upstream_post_error(
                        &resolved.target_type,
                        &body_str,
                        resolved_cat.as_str(),
                        crate::relay::response_formatter::is_openai_compatible_path(&raw_path),
                    );
                    body_str = converted;
                    if post_err.is_none() && resolved.fast_response {
                        let has_tid = serde_json::from_str::<serde_json::Value>(&body_str)
                            .is_ok_and(|v| !crate::relay::response_formatter::extract_async_task_id(&v).is_empty());
                        if !has_tid {
                            post_err = Some(body_str.clone());
                        }
                    }
                    if let Some(mut err_response) = post_err {
                        if forward::is_official_route(&raw_path) && forward::is_tencent_target(&resolved.target_type) {
                            err_response = forward::wrap_official_client(
                                &raw_path,
                                &err_response,
                                &model,
                                &request_content_str,
                            );
                            body_str = err_response.clone();
                        }
                        let latency_ms = attempt_start.elapsed().as_millis() as u32;
                        let bill = crate::relay::ha::FailBill::biz(
                            latency_ms,
                            body_str,
                            err_response,
                            request_content_str,
                            upstream_body.to_string(),
                        )
                        .detail("请求失败");
                        return Err(crate::relay::ha::HaAttempt::park(
                            &fail_buf,
                            bill,
                            Some(upstream_hdrs),
                        ));
                    }
                    (body_str, upstream_hdrs)
                };

                #[cfg(feature = "plugin_comfyui")]
                let upstream_req = comfy_prompt_json.unwrap_or_else(|| upstream_body.to_string());
                #[cfg(not(feature = "plugin_comfyui"))]
                let upstream_req = upstream_body.to_string();

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
                    &upstream_req,
                    None,
                    pending_log_id,
                    dm.as_ref(),
                    Some(resolved_cat.as_str()),
                )
                .await?;

                let latency_ms = req_start.elapsed().as_millis() as u32;
                let freeze_title = if unit_pre > 0.0 {
                    let currency_unit = crate::api::settings::get_currency_settings(&state)
                        .await
                        .currency_unit;
                    format!(
                        "异步任务预扣费冻结(按秒) | 预扣: {}{}/秒 × {:.0}秒 = 冻结: {}{}",
                        crate::money::format_clean_money(unit_pre),
                        currency_unit,
                        duration_seconds,
                        crate::money::format_clean_money(pre_deduction),
                        currency_unit
                    )
                } else {
                    "异步任务处理中(冻结)".to_string()
                };
                let mut billing_detail = match &asset_convert_log {
                    Some(acl) => format!("{} | {}", freeze_title, acl),
                    None => freeze_title,
                };
                if let Some(ref md) = mapping_detail {
                    billing_detail.push_str(&format!(" | {}", md));
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&response_content_str) {
                    let tid = crate::relay::response_formatter::extract_async_task_id(&v);
                    if !tid.is_empty() {
                        super::forward::remember_upstream_task(&mut plugin_tag, &tid);
                        if resolved.is_cascade {
                            crate::relay::cascade::cascade_remember_s1(&mut plugin_tag, &tid);
                        }
                        if resolved.fast_response || resolved.is_cascade {
                            crate::relay::response_formatter::force_json_task_id(
                                &mut response_content_str,
                                &client_task_id,
                            );
                        }
                    }
                }
                let mut final_response_str = crate::relay::response_formatter::apply_format(
                    &raw_path,
                    &resolved_cat,
                    &response_content_str,
                    false,
                    None,
                );
                let is_tencent = forward::is_tencent_target(&resolved.target_type);
                if forward::is_official_route(&raw_path)
                    && is_tencent
                    && !final_response_str.contains("\"error\":")
                {
                    final_response_str = forward::wrap_official_client(
                        &raw_path,
                        &final_response_str,
                        &model,
                        &request_content_str,
                    );
                }
                proxy::record_and_bill_inner(proxy::BillRecord {
                    ctx: crate::relay::ha::HaBillCtx::new(&state, &token, &model, &ep)
                        .category(resolved_cat.as_str())
                        .db(dm.as_ref()),
                    channel: &channel,
                    log_id: pending_log_id,
                    usage: Default::default(),
                    cost: pre_deduction,
                    pre_deducted: pre_deduction,
                    pre_deduct_gift,
                    latency_ms,
                    is_stream: 0,
                    status_code: 200,
                    error_msg: None,
                    request: Some(request_content_str),
                    response: Some(final_response_str.clone()),
                    upstream_req: Some(upstream_req),
                    detail: Some(billing_detail),
                    features: pre_features,
                    time_multiplier: db_rule.as_ref().map(|r| r.applied_multiplier),
                    plugin_tag: plugin_tag.as_deref(),
                    upstream_request_id: upstream_headers::extract_upstream_request_id(
                        &upstream_hdrs,
                    ),
                })
                .await;

                Ok(super::UpstreamRaw::new(upstream_hdrs, final_response_str))
            }
        });

        let bill_ctx = crate::relay::ha::HaBillCtx::new(&state, &token, &model, &ep)
            .category(resolved_cat.as_str())
            .db(db_model.as_ref());
        match super::join_protected(&mut ha, result_rx, &bill_ctx, &channel, Some(&url)).await {
            super::ProtectJoin::Ok(raw) => {
                let ms = attempt_start.elapsed().as_millis() as u32;
                ha.ok(&state, &channel, &url, ms).await;
                if let Some(tx) = ack_tx.take() {
                    let _ = tx.send(Ok(upstream_headers::with_request_id(
                        upstream_headers::json_with_upstream_headers(&raw.headers, raw.body),
                        &log_id,
                    )));
                }
                return;
            }
            super::ProtectJoin::Retry => {
                ha.bump();
                continue;
            }
            super::ProtectJoin::Stop => break,
        }
    }

    if ack_tx.is_none() && !ha.had_upstream {
        if let Some(id) = ha.pending_log_id {
            let msg = ha.last_err.message();
            let _ = sqlx::query(&state.db.format_query(
                "UPDATE logs SET status_code = ?, is_completed = 1, error_message = ? \
                 WHERE id = ? AND status_code = 0 AND is_completed = 0",
            ))
            .bind(ha.last_err.http_status() as i32)
            .bind(&msg)
            .bind(id)
            .execute(&state.db.pool)
            .await;
        }
    }
    let err = ha
        .finish(
            &crate::relay::ha::HaBillCtx::new(&state, &token, model, raw_path)
                .category(category)
                .db(db_model.as_ref()),
        )
        .await;
    if let Some(tx) = ack_tx.take() {
        let _ = tx.send(Err(err));
    }
}
