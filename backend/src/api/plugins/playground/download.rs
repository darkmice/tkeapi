/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 同源下载中转：CDN 无 CORS 时浏览器无法 fetch→blob 强制下载。

use axum::{
    body::Body,
    extract::{Extension, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    auth,
    error::{AppError, AppResult},
    AppState,
};

#[derive(Debug, Deserialize)]
struct DownloadFileQuery {
    url: String,
    filename: Option<String>,
}

fn sanitize_filename(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let mut t = s.trim().to_string();
    if t.to_lowercase().ends_with(".jpeg") {
        t = format!("{}.jpg", &t[..t.len() - 5]);
    }
    if t.is_empty() {
        "file.bin".into()
    } else {
        t.chars().take(120).collect()
    }
}

fn validate_download_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("url 不能为空".into()));
    }
    if trimmed.starts_with("blob:") || trimmed.starts_with("data:") {
        return Err(AppError::BadRequest("不支持 blob/data 地址".into()));
    }
    let parsed =
        reqwest::Url::parse(trimmed).map_err(|_| AppError::BadRequest("url 无效".into()))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(AppError::BadRequest(format!(
                "不支持的协议: {}（仅 http/https）",
                other
            )));
        }
    }
    if parsed.host_str().is_none() {
        return Err(AppError::BadRequest("url 缺少主机名".into()));
    }
    Ok(trimmed.to_string())
}

fn content_disposition(filename: &str) -> String {
    let ascii: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let ascii_fallback = if ascii.chars().all(|c| c == '_' || c == '.') {
        let ext = filename
            .rsplit('.')
            .next()
            .filter(|e| {
                !e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric())
            })
            .unwrap_or("bin");
        format!("file.{}", ext.to_lowercase())
    } else {
        ascii
    };
    format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        ascii_fallback,
        urlencoding::encode(filename)
    )
}

async fn download_file(
    State(state): State<Arc<AppState>>,
    Extension(_claims): Extension<auth::Claims>,
    Query(q): Query<DownloadFileQuery>,
) -> AppResult<Response> {
    let url = validate_download_url(&q.url)?;
    let filename = sanitize_filename(q.filename.as_deref().unwrap_or("file.bin"));
    let bytes = crate::services::http_client::download_bytes(&state.http_client, &url)
        .await
        .map_err(|e| AppError::BadRequest(format!("拉取文件失败: {e}")))?;
    if bytes.is_empty() {
        return Err(AppError::BadRequest("文件内容为空".into()));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, content_disposition(&filename))
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(Body::from(bytes))
        .map_err(|e| AppError::Internal(format!("构建响应失败: {e}")))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/download-file", get(download_file))
}
