/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 渠道级 TOS 资源持久化模块
//! 将响应中的 base64/URL 媒体资源上传到 TOS，返回替换后的响应字符串。
//! 兼容 OpenAI 标准格式和各厂商原生格式（可灵、火山方舟、阿里百炼、Gemini 等）。
//! 供 image.rs、video.rs、task.rs 共同调用。

use crate::services::object_store::{ObjectStore, SiteFolder, StoreKind};
use crate::time_system::DbTs;
use crate::AppState;
use sha2::Digest;

/// 从系统存储设置加载 ObjectStore。
/// `prefer` 为渠道指定厂商（tos/cos）；空则用站点默认；指定未配置时回退默认。
pub async fn load_object_store(state: &AppState, prefer: Option<&str>) -> Option<ObjectStore> {
    let s = load_storage_settings(state).await?;
    if let Some(p) = prefer.map(str::trim).filter(|p| !p.is_empty()) {
        let kind = StoreKind::parse(p);
        if let Some(store) = ObjectStore::from_settings_for(&s, kind) {
            return Some(store);
        }
        let default_kind = StoreKind::parse(&s.default_provider);
        if kind != default_kind {
            crate::relay_debug!(
                "[TosPersist] 渠道指定存储 {} 未配置，回退站点默认 {}",
                p,
                default_kind.as_str()
            );
        }
    }
    ObjectStore::from_settings(&s)
}

/// 从系统存储设置加载当前默认 ObjectStore
pub async fn load_system_object_store(state: &AppState) -> Option<ObjectStore> {
    load_object_store(state, None).await
}

pub(crate) async fn load_storage_settings(
    state: &AppState,
) -> Option<crate::models::StorageSettings> {
    let val: String = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = 'storage_settings'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    .ok()??;
    serde_json::from_str(&val).ok()
}

pub(crate) fn response_format_from_request(request_content: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(request_content)
        .ok()?
        .get("response_format")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 渠道已开启转存时上传媒体并替换 URL；未开启则原样返回。
/// 若用户请求显式指定 response_format（如 "url" 或 "b64_json"），优先遵循用户意图。
pub async fn persist_for_channel(
    state: &AppState,
    channel: &crate::models::Channel,
    response_str: &str,
    category: &str,
    user_response_format: Option<&str>,
) -> String {
    let Some((channel_id, days, provider)) = channel.storage_persist() else {
        return response_str.to_string();
    };
    let kind = if category.contains("视频") || category == "封装" {
        "video"
    } else {
        "image"
    };
    let persist_b64 = match user_response_format.map(str::trim) {
        Some(rf) if rf.eq_ignore_ascii_case("url") => true,
        Some(rf) if rf.eq_ignore_ascii_case("b64_json") => false,
        _ => channel.tos_storage_b64(),
    };
    persist_response_resources(
        state,
        response_str,
        channel_id,
        days,
        Some(kind),
        provider.as_deref(),
        persist_b64,
    )
    .await
}

/// 单 URL 落桶（包装为 OpenAI `data[0].url`），成功返回新 URL。
pub async fn persist_url_for_storage(
    state: &AppState,
    url: &str,
    (channel_id, days, provider): (i64, i64, Option<String>),
    media_kind: &str,
) -> Option<String> {
    let mini = serde_json::json!({ "data": [{ "url": url }] }).to_string();
    let out = persist_response_resources(
        state,
        &mini,
        channel_id,
        days,
        Some(media_kind),
        provider.as_deref(),
        true,
    )
    .await;
    serde_json::from_str::<serde_json::Value>(&out)
        .ok()?
        .pointer("/data/0/url")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 将响应中的媒体 URL 和 base64 上传到对象存储并替换为永久 URL。
/// - URL 数据：只要开启了渠道转存就必须转存
/// - base64 数据：由 persist_b64 控制（开则转存，关则原样直接返回不受转存影响）
/// 兼容 OpenAI 标准格式和各厂商原生格式。
pub(crate) async fn persist_response_resources(
    state: &AppState,
    response_str: &str,
    channel_id: i64,
    storage_days: i64,
    fallback_type: Option<&str>,
    storage_provider: Option<&str>,
    persist_b64: bool,
) -> String {
    let store = match load_object_store(state, storage_provider).await {
        Some(c) => c,
        None => {
            crate::relay_debug!("[TosPersist] 系统存储设置未配置，跳过渠道对象存储");
            return response_str.to_string();
        }
    };

    let mut root: serde_json::Value = match serde_json::from_str(response_str) {
        Ok(v) => v,
        Err(_) => return response_str.to_string(),
    };

    let mut changed = false;

    // 策略一：OpenAI 标准格式（data[].url / b64_json）
    if let Some(items) = root.get_mut("data").and_then(|d| d.as_array_mut()) {
        for item in items.iter_mut() {
            if persist_openai_item(
                state,
                &store,
                item,
                channel_id,
                storage_days,
                fallback_type,
                persist_b64,
            )
            .await
            {
                changed = true;
            }
        }
    }

    // 策略二：非 OpenAI 格式 — 复用 response_formatter::find_urls 提取各厂商媒体 URL 和 base64
    if !changed {
        let urls = super::response_formatter::find_urls(&root);
        if !urls.is_empty() {
            // 构建 原始URL → TOS URL 映射表
            let mut url_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for found in &urls {
                if found.starts_with("data:") {
                    // base64 数据：受 persist_b64 开关控制
                    if !persist_b64 {
                        continue;
                    }
                    // Gemini base64：解码上传 TOS
                    let raw_b64 = super::forward::b64_data(found);
                    let file_data = match base64_decode(found) {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    let ext = detect_image_ext(&file_data);
                    if let Some(tos_url) = upload_and_record(
                        state,
                        &store,
                        &file_data,
                        &ext,
                        channel_id,
                        storage_days,
                        Some("base64_data"),
                    )
                    .await
                    {
                        url_map.insert(raw_b64.to_string(), tos_url);
                    }
                } else if found.starts_with("http://") || found.starts_with("https://") {
                    // URL 数据：url 就一定要转存因为开启了渠道转存
                    if store.extract_object_key(found).is_some() {
                        continue;
                    }
                    let (file_data, ext) = match download_url(&state.http_client, found).await {
                        Ok(data) => (data, guess_ext(found, fallback_type.unwrap_or("image"))),
                        Err(e) => {
                            crate::relay_debug!("[TosPersist] 下载失败 url={}: {}", found, e);
                            continue;
                        }
                    };
                    if let Some(tos_url) = upload_and_record(
                        state,
                        &store,
                        &file_data,
                        &ext,
                        channel_id,
                        storage_days,
                        Some(found),
                    )
                    .await
                    {
                        url_map.insert(found.clone(), tos_url);
                    }
                }
            }
            if !url_map.is_empty() {
                // 基于 JSON 结构精确替换，避免全局字符串替换导致输入字段中的 URL 被污染
                replace_urls_in_json(&mut root, &url_map);
                changed = true;
            }
        }
    }

    if changed {
        serde_json::to_string(&root).unwrap_or_else(|_| response_str.to_string())
    } else {
        response_str.to_string()
    }
}

/// 仅当客户端显式要求 b64_json 时，将 data[].url 转换为 b64_json。
/// 未指定（或其它格式）一律原样返回，上游返回什么就是什么。
pub async fn align_response_format(
    state: &AppState,
    response_str: &str,
    response_format: Option<&str>,
) -> String {
    if !response_format.is_some_and(|rf| rf.trim().eq_ignore_ascii_case("b64_json")) {
        return response_str.to_string();
    }

    let mut root: serde_json::Value = match serde_json::from_str(response_str) {
        Ok(v) => v,
        Err(_) => return response_str.to_string(),
    };
    let mut changed = false;

    if let Some(items) = root.get_mut("data").and_then(|d| d.as_array_mut()) {
        for item in items.iter_mut() {
            let b64 = item.get("b64_json").and_then(|v| v.as_str()).unwrap_or("");
            let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");

            if b64.is_empty() && !url.is_empty() && url != "base64数据" {
                let data = match download_url(&state.http_client, url).await {
                    Ok(d) => d,
                    Err(e) => {
                        crate::relay_debug!("[TosPersist] url 转换为 base64 失败: {}", e);
                        continue;
                    }
                };
                use base64::Engine;
                item["b64_json"] =
                    serde_json::json!(base64::engine::general_purpose::STANDARD.encode(&data));
                item.as_object_mut().map(|obj| obj.remove("url"));
                changed = true;
            }
        }
    }

    if changed {
        serde_json::to_string(&root).unwrap_or_else(|_| response_str.to_string())
    } else {
        response_str.to_string()
    }
}

/// 将 OpenAI data[] item 的 base64/URL 资源上传到 TOS，替换为 TOS URL
async fn persist_openai_item(
    state: &AppState,
    store: &ObjectStore,
    item: &mut serde_json::Value,
    channel_id: i64,
    storage_days: i64,
    fallback_type: Option<&str>,
    persist_b64: bool,
) -> bool {
    let b64 = item.get("b64_json").and_then(|v| v.as_str()).unwrap_or("");
    let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");

    if b64.is_empty() && url.is_empty() {
        return false;
    }
    if b64 == "base64数据" {
        return false;
    }
    if !url.is_empty() && store.extract_object_key(url).is_some() {
        return false;
    }

    let is_base64_data = !b64.is_empty() || url.starts_with("data:");
    // base64 数据且未开启 base64 转存时：直接返回不受开启转存影响
    if is_base64_data && !persist_b64 {
        return false;
    }

    let (file_data, ext) = if !b64.is_empty() {
        match base64_decode(b64) {
            Ok(data) => {
                let ext = detect_image_ext(&data);
                (data, ext)
            }
            Err(e) => {
                crate::relay_debug!("[TosPersist] base64 解码失败: {}", e);
                return false;
            }
        }
    } else if url.starts_with("data:") {
        match base64_decode(url) {
            Ok(data) => {
                let ext = detect_image_ext(&data);
                (data, ext)
            }
            Err(e) => {
                crate::relay_debug!("[TosPersist] url base64 解码失败: {}", e);
                return false;
            }
        }
    } else {
        // URL 数据：url 就一定要转存因为开启了渠道转存
        match download_url(&state.http_client, url).await {
            Ok(data) => (data, guess_ext(url, fallback_type.unwrap_or("image"))),
            Err(e) => {
                crate::relay_debug!("[TosPersist] 下载失败 url={}: {}", url, e);
                return false;
            }
        }
    };

    let source = if !url.is_empty() { url } else { "base64_data" };
    let tos_url = match upload_and_record(
        state,
        store,
        &file_data,
        &ext,
        channel_id,
        storage_days,
        Some(source),
    )
    .await
    {
        Some(url) => url,
        None => return false,
    };

    item.as_object_mut().map(|obj| obj.remove("b64_json"));
    item["url"] = serde_json::json!(tos_url);
    true
}

/// 上传文件到 TOS 并记录过期追踪，返回 TOS URL
/// source_url: 原始资源地址，用于日志输出追溯（base64 来源传 "base64_data"）
async fn upload_and_record(
    state: &AppState,
    store: &ObjectStore,
    file_data: &[u8],
    ext: &str,
    channel_id: i64,
    storage_days: i64,
    source_url: Option<&str>,
) -> Option<String> {
    let hash = &format!("{:x}", sha2::Sha256::digest(file_data))[..8];
    let timestamp = chrono::Utc::now().timestamp();
    let filename = format!("{}_{}.{}", timestamp, hash, ext);
    let object_key = store.site_key(
        SiteFolder::ChannelCache,
        &format!("{}/{}", channel_id, filename),
    );
    let content_type = ext_to_mime(ext);

    let tos_url = match store
        .upload_file(&object_key, file_data.to_vec(), content_type, None)
        .await
    {
        Ok(url) => url,
        Err(e) => {
            crate::relay_debug!("[TosPersist] 对象存储上传失败 key={}: {}", object_key, e);
            return None;
        }
    };

    if storage_days > 0 {
        let expire_at =
            DbTs::from_utc(chrono::Utc::now() + chrono::Duration::days(storage_days));
        let _ = sqlx::query(
            "INSERT INTO tos_temp_files (object_key, channel_id, source, expire_at, storage_provider) VALUES ($1, $2, 'channel', $3, $4)"
        )
        .bind(&object_key)
        .bind(channel_id)
        .bind(&expire_at)
        .bind(store.provider())
        .execute(&state.db.pool)
        .await;
    }

    crate::relay_debug!(
        "[TosPersist] {} => {}",
        source_url.unwrap_or("unknown"),
        tos_url
    );
    Some(tos_url)
}

fn ext_to_mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// 清理过期的临时对象（循环批处理，每批 100 条，直到全部清完）
pub async fn cleanup_expired_files(state: &AppState) {
    let (tos_store, cos_store) = match load_storage_settings(state).await {
        Some(s) => (
            ObjectStore::from_settings_for(&s, StoreKind::Tos),
            ObjectStore::from_settings_for(&s, StoreKind::Cos),
        ),
        None => (None, None),
    };
    if tos_store.is_none() && cos_store.is_none() {
        tracing::info!("[TosCleanup] 系统存储设置未配置，跳过过期文件清理");
        return;
    }

    let mut total_cleaned: u64 = 0;
    let mut total_failed: u64 = 0;

    loop {
        let rows: Vec<(i64, String, String)> = match sqlx::query_as(
            "SELECT id, object_key, COALESCE(storage_provider, 'tos') FROM tos_temp_files WHERE expire_at <= NOW() LIMIT 100",
        )
        .fetch_all(&state.db.pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("[TosCleanup] 查询过期文件失败: {}", e);
                return;
            }
        };

        if rows.is_empty() {
            break;
        }

        let batch_size = rows.len();
        let mut batch_cleaned: usize = 0;
        for (id, object_key, provider) in &rows {
            let store = match StoreKind::parse(provider) {
                StoreKind::Tos => tos_store.as_ref(),
                StoreKind::Cos => cos_store.as_ref(),
            };
            let Some(store) = store else {
                tracing::warn!("[TosCleanup] {} 未配置，跳过 key={}", provider, object_key);
                total_failed += 1;
                continue;
            };
            if let Err(e) = store.delete_file(object_key).await {
                let err_lower = e.to_lowercase();
                let is_not_found = err_lower.contains("404")
                    || err_lower.contains("nosuchkey")
                    || err_lower.contains("not found")
                    || err_lower.contains("no such key");
                if !is_not_found {
                    tracing::warn!("[TosCleanup] 删除失败 key={}: {}", object_key, e);
                    total_failed += 1;
                    continue;
                }
                tracing::info!(
                    "[TosCleanup] 对象已不存在 key={}，清理数据库记录",
                    object_key
                );
            }
            let _ = sqlx::query("DELETE FROM tos_temp_files WHERE id = $1")
                .bind(id)
                .execute(&state.db.pool)
                .await;
            total_cleaned += 1;
            batch_cleaned += 1;
        }

        if batch_size < 100 || batch_cleaned == 0 {
            break;
        }
    }

    if total_cleaned > 0 || total_failed > 0 {
        tracing::info!(
            "[TosCleanup] 过期文件清理完成: 成功={}, 失败={}",
            total_cleaned,
            total_failed
        );
    }
}

/// 基于 JSON 结构精确替换 URL 值（替代全局字符串替换，防止输入字段中的 URL 被污染）。
/// 跳过 input/request/task_input/original_input 等请求输入相关字段。
fn replace_urls_in_json(
    v: &mut serde_json::Value,
    url_map: &std::collections::HashMap<String, String>,
) {
    match v {
        serde_json::Value::String(s) => {
            // 精确匹配：整个字符串值是映射中的 key
            if let Some(tos_url) = url_map.get(s.as_str()) {
                *s = tos_url.clone();
            } else if s.starts_with("data:") {
                // 精确匹配：纯 base64 部分匹配 url_map，避免前缀残留
                let raw_b64 = super::forward::b64_data(s);
                if raw_b64 != s.trim() {
                    if let Some(tos_url) = url_map.get(raw_b64) {
                        *s = tos_url.clone();
                    }
                }
            } else {
                // 子串替换：字符串中嵌入了映射中的 key（如 base64 内容、Markdown 文本）
                let mut replaced = s.clone();
                // 长度降序排序替换，先替换长键，防止先替换短子串造成前缀等信息残留
                let mut sorted_keys: Vec<&String> = url_map.keys().collect();
                sorted_keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
                for old in sorted_keys {
                    if let Some(new) = url_map.get(old) {
                        if replaced.contains(old.as_str()) {
                            replaced = replaced.replace(old.as_str(), new);
                        }
                    }
                }
                // 书虫格式清理：替换后 ![...](data:...;base64,TOS_URL) → TOS_URL
                if replaced.contains("data:") && replaced.contains(";base64,http") {
                    let re_md =
                        regex::Regex::new(r"!\[.*?\]\(data:[^;]+;base64,(https?://[^\)]+)\)")
                            .unwrap();
                    replaced = re_md.replace_all(&replaced, "$1").to_string();
                }
                if replaced != *s {
                    *s = replaced;
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                replace_urls_in_json(item, url_map);
            }
        }
        serde_json::Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                // 跳过请求输入相关字段，避免将用户原始图片替换为 TOS 地址
                if k == "request" || k == "input" || k == "task_input" || k == "original_input" {
                    continue;
                }
                replace_urls_in_json(val, url_map);
            }
        }
        _ => {}
    }
}

// ── 辅助函数 ──────────────────────────────────────────────────

/// 下载远程文件
async fn download_url(http_client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    crate::services::http_client::download_bytes(http_client, url).await
}

/// Base64 解码（支持 data:xxx;base64, 前缀）
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let data = super::forward::b64_data(input);
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("Base64 解码失败: {}", e))
}

/// 从文件头字节检测图片格式
fn detect_image_ext(data: &[u8]) -> String {
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "png".to_string()
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg".to_string()
    } else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
        "webp".to_string()
    } else if data.starts_with(b"GIF8") {
        "gif".to_string()
    } else {
        "png".to_string()
    }
}

/// 从 URL 推断文件扩展名
fn guess_ext(url: &str, fallback_type: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    if let Some(ext) = path.rsplit('.').next() {
        let ext = ext.to_lowercase();
        if ["png", "jpg", "jpeg", "webp", "gif", "mp4", "webm", "mov"].contains(&ext.as_str()) {
            return ext;
        }
    }
    match fallback_type {
        "video" => "mp4".to_string(),
        _ => "png".to_string(),
    }
}
