/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 厂商 callback 代理：上游体含根级 `callback_url` 时改写为系统地址。
//! 本入口只查 logs、不写库；非级联则原文转发客户端；级联交由后台轮询结案后再通知。

use crate::error::AppResult;
use crate::AppState;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};

const CB_KEY: &str = "callback_url";

static LAST_BROWSER_ORIGIN: RwLock<Option<String>> = RwLock::new(None);

/// 从上游请求 JSON 根级提取 `callback_url`
pub fn extract_client_callback_url(v: &Value) -> Option<String> {
    v.get(CB_KEY)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 将上游 body 根级 `callback_url` 改写为系统地址
pub fn rewrite_upstream_callback(body: &mut Value, system_url: &str) {
    if body
        .get(CB_KEY)
        .and_then(|x| x.as_str())
        .is_some_and(|s| !s.trim().is_empty())
    {
        body[CB_KEY] = json!(system_url);
    }
}

fn host_of(s: &str) -> &str {
    s.split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(s)
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .trim()
}

fn extract_header_origin(headers: &axum::http::HeaderMap) -> Option<String> {
    let hdr = |k| {
        headers
            .get(k)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "null")
    };
    if let Some(o) = hdr("origin") {
        return Some(o.trim_end_matches('/').to_string());
    }
    let u = reqwest::Url::parse(hdr("referer")?).ok()?;
    Some(u.origin().ascii_serialization()).filter(|s| s != "null")
}

/// 记录控制台浏览器请求带显式端口的 Origin（如 IP:端口），供反代剥离端口时的 API 请求兜底还原
pub fn note_browser_origin(headers: &axum::http::HeaderMap) {
    let Some(origin) = extract_header_origin(headers) else { return };
    if let Some(raw_host) = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !host_of(raw_host).eq_ignore_ascii_case(host_of(&origin)) {
            return;
        }
    }
    if origin.split_once("://").is_some_and(|(_, hp)| hp.contains(':'))
        && !LAST_BROWSER_ORIGIN
            .read()
            .ok()
            .is_some_and(|g| g.as_deref() == Some(origin.as_str()))
    {
        if let Ok(mut w) = LAST_BROWSER_ORIGIN.write() {
            *w = Some(origin);
        }
    }
}

/// 从请求头推断服务根地址（与支付回调逻辑相同）。
/// 优先级：`PUBLIC_API_URL` 环境变量 → 同源 `Origin`/`Referer` 头 → `X-Forwarded-Host` / `Host` 头（含端口补齐）
pub fn infer_base_url(headers: &axum::http::HeaderMap) -> String {
    infer_base_url_with_fallback(headers, "http://localhost:3000")
}

pub fn infer_base_url_with_fallback(headers: &axum::http::HeaderMap, fallback: &str) -> String {
    let hdr = |k| {
        headers
            .get(k)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let raw_host = match (hdr("x-forwarded-host"), hdr("host")) {
        (Some(a), Some(b)) if !a.contains(':') && b.contains(':') => Some(b),
        (a, b) => a.or(b),
    };
    std::env::var("PUBLIC_API_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let rh = raw_host?;
            let origin = extract_header_origin(headers)?;
            if !host_of(rh).eq_ignore_ascii_case(host_of(&origin)) {
                return None;
            }
            note_browser_origin(headers);
            Some(origin)
        })
        .or_else(|| {
            let raw_host = raw_host?;
            let mut host = raw_host.to_string();
            if !raw_host.contains(':') {
                if let Some(p) = hdr("x-forwarded-port").filter(|&p| p != "80" && p != "443") {
                    host = format!("{raw_host}:{p}");
                } else if let Some(saved) = LAST_BROWSER_ORIGIN.read().ok().and_then(|g| g.clone())
                {
                    if let Some((_, hp)) = saved.split_once("://").filter(|(_, hp)| {
                        hp.strip_prefix(raw_host)
                            .is_some_and(|rest| rest.starts_with(':'))
                    }) {
                        if hdr("x-forwarded-proto").is_none() {
                            return Some(saved);
                        }
                        host = hp.to_string();
                    }
                }
            }
            let is_ip_or_local = host.contains("localhost")
                || host_of(&host).parse::<std::net::IpAddr>().is_ok();
            let scheme =
                hdr("x-forwarded-proto").unwrap_or(if is_ip_or_local && !host.ends_with(":443") {
                    "http"
                } else {
                    "https"
                });
            Some(format!("{scheme}://{host}"))
        })
        .unwrap_or_else(|| fallback.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn sign_vendor_callback_id(db_log_id: i64, secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key length is always valid");
    mac.update(format!("vendor_cb:{db_log_id}").as_bytes());
    let full = hex::encode(mac.finalize().into_bytes());
    full[..16].to_string()
}

fn default_callback_secret() -> String {
    std::env::var("JWT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "tokensbyte-vendor-callback-secret".to_string())
}

/// 构造系统回调地址，自动识别当前服务域名并附带 HMAC 防伪签名
pub fn system_callback_url(db_log_id: i64, headers: &axum::http::HeaderMap) -> String {
    let sig = sign_vendor_callback_id(db_log_id, &default_callback_secret());
    format!(
        "{}/api/v1/relay/vendor-callback/{}?sig={}",
        infer_base_url(headers),
        db_log_id,
        sig
    )
}

pub fn stash_cb_in_plugin_tag(tag: &mut Value, client_url: &str) {
    tag["cb"] = json!(client_url);
}

pub fn cb_from_plugin_tag(plugin_tag: &str) -> Option<String> {
    let v: Value = serde_json::from_str(plugin_tag).ok()?;
    v.get("cb")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

async fn post_official(state: &AppState, client_url: &str, official: &str) {
    if !crate::services::http_client::is_safe_outbound_url(client_url).await {
        tracing::warn!(
            "[VendorCallback] 非法或内网 callback_url，已阻断: {}",
            client_url
        );
        return;
    }
    match state
        .http_client
        .post(client_url)
        .header("Content-Type", "application/json")
        .body(official.to_string())
        .send()
        .await
    {
        Ok(resp) => crate::relay_debug!(
            "[VendorCallback] 已转发 url={} status={}",
            client_url,
            resp.status()
        ),
        Err(e) => crate::relay_debug!("[VendorCallback] 转发失败 url={} err={}", client_url, e),
    }
}

/// 直接向已知客户端 callback URL 转发
pub async fn notify_client(state: &AppState, client_url: &str, official: &str) {
    post_official(state, client_url, official).await;
}

/// 有客户端 callback 时转发；`plugin_tag` 由调用方传入
pub async fn forward_official_to_client(state: &AppState, plugin_tag: &str, official: &str) {
    let Some(url) = cb_from_plugin_tag(plugin_tag) else {
        return;
    };
    post_official(state, &url, official).await;
}

/// 厂商 callback：只读 logs；级联不转发；非级联原文转客户端
pub async fn vendor_callback(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: Bytes,
) -> AppResult<impl IntoResponse> {
    if id <= 0 {
        return Ok((StatusCode::BAD_REQUEST, "无效 id").into_response());
    }
    let provided_sig = params.get("sig").map(String::as_str).unwrap_or("");
    let expected_cfg = sign_vendor_callback_id(id, &state.config.jwt_secret);
    let expected_env = sign_vendor_callback_id(id, &default_callback_secret());
    if provided_sig.is_empty() || (provided_sig != expected_cfg && provided_sig != expected_env) {
        return Ok((StatusCode::FORBIDDEN, "回调签名校验失败").into_response());
    }

    let plugin_tag: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT COALESCE(plugin_tag,'') FROM logs WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await
    .ok()
    .flatten();

    let Some(plugin_tag) = plugin_tag else {
        return Ok((StatusCode::NOT_FOUND, "日志不存在").into_response());
    };

    // 级联：S1 上游回调只 ack，用户通知由后台轮询 S2 结案后处理
    if super::cascade::cascade_plugin_tag_present(&plugin_tag) {
        crate::relay_debug!("[VendorCallback] 级联跳过 id={}", id);
        return Ok((StatusCode::OK, "ok").into_response());
    }

    if let Some(url) = cb_from_plugin_tag(&plugin_tag) {
        let official = String::from_utf8_lossy(&body).to_string();
        let state2 = state.clone();
        tokio::spawn(async move {
            post_official(&state2, &url, &official).await;
        });
    }

    Ok((StatusCode::OK, "ok").into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn test_infer_base_url_preserves_ip_port_and_domain() {
        // 1. Host 含 IP:端口
        let mut h1 = HeaderMap::new();
        h1.insert("host", HeaderValue::from_static("115.190.151.94:9000"));
        h1.insert("x-forwarded-proto", HeaderValue::from_static("http"));
        assert_eq!(infer_base_url(&h1), "http://115.190.151.94:9000");

        // 2. X-Forwarded-Host 被剥离端口，但 Host 带有端口时优先保留端口
        let mut h2 = HeaderMap::new();
        h2.insert(
            "x-forwarded-host",
            HeaderValue::from_static("115.190.151.94"),
        );
        h2.insert("host", HeaderValue::from_static("115.190.151.94:9000"));
        h2.insert("x-forwarded-proto", HeaderValue::from_static("http"));
        assert_eq!(infer_base_url(&h2), "http://115.190.151.94:9000");

        // 3. Host 与 X-Forwarded-Host 都被反代剥离端口，但 X-Forwarded-Port 存在
        let mut h3 = HeaderMap::new();
        h3.insert("host", HeaderValue::from_static("115.190.151.94"));
        h3.insert("x-forwarded-port", HeaderValue::from_static("9000"));
        h3.insert("x-forwarded-proto", HeaderValue::from_static("http"));
        assert_eq!(infer_base_url(&h3), "http://115.190.151.94:9000");

        // 4. 浏览器访问控制台记录了 Origin/Referer 后，API 请求即使被反代剥掉端口也能自动补齐
        let mut browser_headers = HeaderMap::new();
        browser_headers.insert(
            "referer",
            HeaderValue::from_static("http://115.190.151.94:9000/tokens"),
        );
        note_browser_origin(&browser_headers);

        let mut api_headers = HeaderMap::new();
        api_headers.insert("host", HeaderValue::from_static("115.190.151.94"));
        api_headers.insert("x-forwarded-proto", HeaderValue::from_static("http"));
        assert_eq!(infer_base_url(&api_headers), "http://115.190.151.94:9000");

        // 5. 常规域名访问不受影响（即使未带 x-forwarded-proto 也默认 https）
        let mut domain_headers = HeaderMap::new();
        domain_headers.insert("host", HeaderValue::from_static("api.example.com"));
        assert_eq!(infer_base_url(&domain_headers), "https://api.example.com");

        // 6. OAuth 回调场景：Edge 浏览器跨域跳转携带第三方 Referer（如 accounts.google.com），绝不能篡改本站 Base URL
        let mut oauth_headers = HeaderMap::new();
        oauth_headers.insert("host", HeaderValue::from_static("ai.artsmcp.com"));
        oauth_headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        oauth_headers.insert(
            "referer",
            HeaderValue::from_static("https://accounts.google.com/"),
        );
        assert_eq!(infer_base_url(&oauth_headers), "https://ai.artsmcp.com");

        // 7. OAuth 回调场景（IP:端口）：跨域跳转携带第三方 Referer，保持本站 IP 和端口
        let mut ip_oauth_headers = HeaderMap::new();
        ip_oauth_headers.insert("host", HeaderValue::from_static("115.190.151.94:9000"));
        ip_oauth_headers.insert("x-forwarded-proto", HeaderValue::from_static("http"));
        ip_oauth_headers.insert(
            "referer",
            HeaderValue::from_static("https://accounts.google.com/"),
        );
        assert_eq!(infer_base_url(&ip_oauth_headers), "http://115.190.151.94:9000");

        // 8. OAuth 回调场景（IP 反代剥离端口）：前序有控制台访问记录，回调虽带第三方 Referer，依然能精确还原端口
        let mut ip_stripped_oauth = HeaderMap::new();
        ip_stripped_oauth.insert("host", HeaderValue::from_static("115.190.151.94"));
        ip_stripped_oauth.insert("x-forwarded-proto", HeaderValue::from_static("http"));
        ip_stripped_oauth.insert(
            "referer",
            HeaderValue::from_static("https://accounts.google.com/"),
        );
        assert_eq!(infer_base_url(&ip_stripped_oauth), "http://115.190.151.94:9000");

        // 9. 异常请求（完全无 Host / X-Forwarded-Host）携带第三方 Referer：绝不盲信外域，回退至 fallback
        let mut no_host_headers = HeaderMap::new();
        no_host_headers.insert(
            "referer",
            HeaderValue::from_static("https://accounts.google.com/"),
        );
        assert_eq!(
            infer_base_url_with_fallback(&no_host_headers, "https://ai.artsmcp.com"),
            "https://ai.artsmcp.com"
        );
    }
}
