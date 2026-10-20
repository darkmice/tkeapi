/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 轮询后处理扩展点：厂商 status 与结果体分离时在此补齐。
//! 新队列类厂商：在 `enrich_poll_response` 增加 match arm，逻辑放 `vendors/`。
//! 轮询 URL 仍由 `task` 按规则 `poll_path` / 原有默认逻辑解析。

use super::{build_auth_headers, vendors, ResolvedForward};
use crate::models::Channel;

/// 轮询后处理失败（由 task 映射为 PollReqErr）
pub struct PollEnrichErr {
    pub http_status: Option<u16>,
    pub message: String,
}

/// 客户端轮询出口补齐所需的上下文（解耦持久层 `TaskRelayLogRow`）
#[derive(Clone, Copy, Default)]
pub struct ClientPollCtx<'a> {
    pub path: &'a str,
    pub model: &'a str,
    pub request_content: &'a str,
    pub billing_features: &'a str,
    pub task_id: &'a str,
}

/// 轮询 GET 成功后的厂商补齐。默认原样返回。
pub async fn enrich_poll_response(
    resolved: &ResolvedForward,
    http_client: &reqwest::Client,
    channel: &Channel,
    poll_url: &str,
    body: String,
) -> Result<(String, String), PollEnrichErr> {
    match resolved.target_type.as_str() {
        "fal_video" => {
            let auth = build_auth_headers(resolved, &channel.api_key, false);
            vendors::fal_enrich_queue_poll(http_client, auth, poll_url, body).await
        }
        _ => Ok((poll_url.to_string(), body)),
    }
}

/// 客户端轮询出口补齐：fal 注入消耗；腾讯云与 fal 按官方路由组装信封。
pub fn enrich_client_poll(target_type: &str, json_str: &str, ctx: ClientPollCtx<'_>) -> String {
    let p = ctx.path.trim_end_matches('/');
    let is_minimax_query =
        p.starts_with("/v2/query/video_generation/") || p == "/v2/query/video_generation";

    if target_type == "fal_video" {
        if is_minimax_query {
            let fallback_id = (!ctx.task_id.is_empty()).then_some(ctx.task_id);
            let openai_json = crate::relay::response_formatter::format_openai(
                "视频",
                json_str,
                true,
                fallback_id,
            );
            let with_usage = vendors::inject_fal_poll_usage(&openai_json, ctx.billing_features);
            super::wrap_official_client(ctx.path, &with_usage, ctx.model, ctx.request_content)
        } else {
            vendors::inject_fal_poll_usage(json_str, ctx.billing_features)
        }
    } else if super::is_official_route(ctx.path) && super::is_tencent_target(target_type) {
        super::wrap_official_client(ctx.path, json_str, ctx.model, ctx.request_content)
    } else {
        json_str.to_string()
    }
}
