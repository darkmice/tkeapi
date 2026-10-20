/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! Relay Header 透传：上游响应头过滤；客户端请求头黑名单过滤后注入上游。
//! 网关会重建 body，故不可原样转发 content-length / content-encoding。
//! 鉴权头由调用方在本模块之后写入，保证渠道 key 覆盖客户端 sk。

use axum::http::header::{self, HeaderName, HeaderValue};
use axum::http::HeaderMap;
use axum::response::Response;

/// 是否应将上游响应头转发给客户端。
#[inline]
fn should_forward_response_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-connection"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
            | "content-encoding"
            | "content-type"
            | "set-cookie"
            // 上游追踪与请求 ID 头：严禁透传给下游，避免暴露上游真实请求 ID
            | "request-id"
            | "x-request-id"
            | "x-tt-logid"
            | "x-bce-request-id"
            | "x-tc-request-id"
            | "x-acs-request-id"
            | "x-amzn-requestid"
    )
}

/// 读取上游响应头字符串值。
#[inline]
pub fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// 上游 Content-Type 是否为 SSE。
#[inline]
pub fn is_sse(headers: &HeaderMap) -> bool {
    header_str(headers, "content-type").is_some_and(|s| s.contains("text/event-stream"))
}

/// 上游 Content-Type 是否为可流式消费（SSE / NDJSON）。
#[inline]
pub fn is_stream_content_type(headers: &HeaderMap) -> bool {
    header_str(headers, "content-type")
        .is_some_and(|s| s.contains("text/event-stream") || s.contains("application/x-ndjson"))
}

/// 将上游可转发头挂到 Response builder。
fn apply_upstream_response_headers(
    mut builder: axum::http::response::Builder,
    upstream: &HeaderMap,
) -> axum::http::response::Builder {
    for (name, value) in upstream
        .iter()
        .filter(|(n, _)| should_forward_response_header(n))
    {
        builder = builder.header(name, value);
    }
    builder
}

/// 将上游可转发头写入已构建 Response 的 HeaderMap（错误路径等）。
pub fn merge_upstream_response_headers(dest: &mut HeaderMap, upstream: &HeaderMap) {
    for (name, value) in upstream
        .iter()
        .filter(|(n, _)| should_forward_response_header(n))
    {
        dest.append(name.clone(), value.clone());
    }
}

/// 透传上游诊断头并设置指定 Content-Type。
pub fn with_content_type(
    upstream: &HeaderMap,
    content_type: impl AsRef<str>,
    body: impl Into<axum::body::Body>,
) -> Response {
    apply_upstream_response_headers(Response::builder(), upstream)
        .header(header::CONTENT_TYPE, content_type.as_ref())
        .body(body.into())
        .unwrap()
}

/// 成功 JSON 响应：透传上游诊断头 + `application/json`。
#[inline]
pub fn json_with_upstream_headers(
    upstream: &HeaderMap,
    body: impl Into<axum::body::Body>,
) -> Response {
    with_content_type(upstream, "application/json", body)
}

/// SSE 响应：透传上游诊断头 + 网关流式固定头（insert 覆盖，避免与上游 Cache-Control 重复）。
pub fn sse_with_upstream_headers(
    upstream: &HeaderMap,
    body: impl Into<axum::body::Body>,
) -> Response {
    let mut resp = with_content_type(upstream, "text/event-stream", body);
    let h = resp.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    h.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    resp
}

/// 向响应头统一注入平台公认标准的 x-request-id（值为本请求的 log_id）。
/// 遵循 OpenAI、OneAPI / NewAPI 等主流 AI 生态公认规范，杜绝冗余多字段与泄露上游真实凭据。
pub fn inject_request_id(headers: &mut HeaderMap, log_id: &str) {
    let trimmed = log_id.trim();
    if trimmed.is_empty() {
        return;
    }
    let Ok(v) = HeaderValue::try_from(trimmed) else {
        return;
    };

    headers.insert(HeaderName::from_static("x-request-id"), v);
}

/// 确保 Response 具备平台统一追踪请求头（x-request-id）。
#[inline]
pub fn with_request_id(mut resp: Response, log_id: &str) -> Response {
    inject_request_id(resp.headers_mut(), log_id);
    resp
}

/// 从上游响应头中提取各厂商的请求唯一追踪 ID（如 x-tt-logid / x-request-id 等）。
/// 优先认厂商专属强特异性追踪头（避免被网关层通用 x-request-id 遮蔽真实凭据），
/// 再由行业标准及通用 request-id 兜底（OpenAI、Claude、DeepSeek、Moonshot 等）。
/// 必须是非空、合理长度（1..=256）且全为合法可见 ASCII 字符。
pub fn extract_upstream_request_id(headers: &HeaderMap) -> Option<String> {
    const CANDIDATES: &[&str] = &[
        // 1. 厂商强特异性专属头（优先提取，避免被网关通用头遮蔽）
        "x-tt-logid", // 字节跳动 / 火山引擎（即梦、豆包、方舟 Ark）官方排查唯一凭证
        "x-bce-request-id", // 百度智能云 / 文心千帆
        "x-tc-request-id", // 腾讯云 / 腾讯混元
        "x-acs-request-id", // 阿里云 / 通义千问 / 百炼
        "x-amzn-requestid", // AWS Bedrock / Amazon API
        // 2. 行业标准与通用兜底头（OpenAI、Claude、DeepSeek、Moonshot 及各类标准模型服务）
        "x-request-id",
        "request-id",
    ];
    for key in CANDIDATES {
        if let Some(val) = headers.get(*key).and_then(|v| v.to_str().ok()) {
            // 支持多层网关逗号拼接 Header（如 "req-111, req-222"），提取首个核心 ID
            let raw_id = val.split(',').next().unwrap_or(val);
            let trimmed = raw_id.trim();
            if !trimmed.is_empty()
                && trimmed.len() <= 256
                && trimmed.bytes().all(|b| (32..=126).contains(&b))
            {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// 生成本请求的系统平台统一日志 ID（`log_` / `tsk_` + ULID）。
/// 纯系统自主生成，不受客户端传入的外部追踪 ID 污染，保证全平台日志主键格式一致。
pub fn resolve_request_log_id(category: Option<&str>) -> String {
    let prefix = if category.is_some_and(|c| !c.is_empty() && c != "聊天") {
        "tsk_"
    } else {
        "log_"
    };
    format!("{}{}", prefix, ulid::Ulid::new().to_string().to_lowercase())
}

/// 客户端 → 上游：黑名单内不透传（鉴权 / hop-by-hop / body 绑定 / 流式易被带偏的 Accept·UA / 客户端 IP）。
/// 其余原样透传（Codex `x-codex-*`/`originator`、Claude `anthropic-beta`、追踪 id 等无需逐厂商登记）。
#[inline]
fn should_block_client_request_header(name: &str) -> bool {
    // 反代链路身份：勿把终端用户 IP/Host 打到上游（风控/地域/回调异常）
    if name == "forwarded" || name.starts_with("x-forwarded-") {
        return true;
    }
    matches!(
        name,
        // 鉴权 / Anthropic 版本：由渠道 auth 写入，禁止把平台 sk 打到上游
        "authorization"
            | "proxy-authorization"
            | "x-api-key"
            | "x-goog-api-key"
            | "anthropic-version"
            // hop-by-hop / 连接
            | "host"
            | "connection"
            | "keep-alive"
            | "proxy-connection"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
            // body 由网关 json() 重建
            | "content-length"
            | "content-encoding"
            | "content-type"
            // 压缩协商交给 reqwest
            | "accept-encoding"
            // Cookie 不应进上游
            | "cookie"
            | "cookie2"
            // 浏览器 Accept: application/json 会带偏 SSE；UA 亦勿透传
            | "accept"
            | "user-agent"
            // 客户端真实 IP（非 x-forwarded-* 前缀的常见变体）
            | "x-real-ip"
            | "x-client-ip"
            | "cf-connecting-ip"
            | "true-client-ip"
            | "fastly-client-ip"
            | "x-cluster-client-ip"
            | "x-original-forwarded-for"
            // request-id 由 apply_client_request_headers 统一在末尾写入对齐的 log_id，避免重复头
            | "x-request-id"
            | "request-id"
            | "x-log-id"
    )
}

/// 将黑名单外的客户端请求头注入上游 RequestBuilder，同时将当前请求的 log_id 注入为 x-request-id 请求头。
/// 遵循行业标准，调用方须在其后写入渠道鉴权与 Content-Type，保证覆盖。
pub fn apply_client_request_headers(
    mut builder: reqwest::RequestBuilder,
    headers: &HeaderMap,
    log_id: &str,
) -> reqwest::RequestBuilder {
    for (name, value) in headers.iter() {
        if !should_block_client_request_header(name.as_str()) {
            builder = builder.header(name, value);
        }
    }
    let trimmed = log_id.trim();
    if !trimmed.is_empty() {
        builder = builder.header("x-request-id", trimmed);
    }
    builder
}
