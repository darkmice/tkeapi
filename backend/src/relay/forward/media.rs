/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

pub(crate) fn parse_image_data(trimmed_url: &str) -> Option<(Vec<u8>, &str)> {
    use base64::Engine;
    let trimmed_url = trimmed_url.trim();
    if trimmed_url.is_empty() {
        return None;
    }

    // 统一的 Base64 解码闭包，支持带填充与无填充
    let decode_b64 = |s: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(s))
    };

    // 1. Data URI：必须含 ;base64,（与历史 GPT edits 一致）
    if let Some(rest) = trimmed_url.strip_prefix("data:") {
        let (meta, payload) = rest.split_once(";base64,")?;
        if let Ok(bytes) = decode_b64(payload) {
            return Some((bytes, meta));
        }
        return None;
    }

    // 2. 排除 URL/路径类型（Base64 字符集不含 '.' 或 ':'），随后尝试纯 Base64 解码
    if !trimmed_url.contains('.') && !trimmed_url.contains(':') {
        if let Ok(bytes) = decode_b64(trimmed_url) {
            if bytes.len() > 10 {
                // 利用 Rust 声明式切片模式匹配判定图片格式，编译器将生成极佳的汇编跳转
                let mime = match bytes.as_slice() {
                    [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, ..] => "image/png",
                    [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
                    [0x47, 0x49, 0x46, 0x38, ..] => "image/gif",
                    [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => {
                        "image/webp"
                    }
                    _ => "image/png",
                };
                return Some((bytes, mime));
            }
        }
    }

    None
}

/// 将已转换的图片 JSON body 构建为 reqwest::multipart::Form。
/// - images 数组中的每个元素 → 根据图片数量映射为 image 或 image[] part（支持 data URI/纯 base64/并发异步下载 URL 转换为二进制，若失败则回退直传）
/// - mask 字段 → mask part（支持 data URI/纯 base64/并发异步下载 URL 转换为二进制，若失败则回退直传）
/// - 其他字段 → text part（model、prompt、size 等）
///
/// 参考文档：https://developers.openai.com/api/reference/resources/images/methods/edit
pub async fn build_edits_multipart(
    client: Option<&reqwest::Client>,
    upstream_body: &serde_json::Value,
) -> reqwest::multipart::Form {
    let mut form = reqwest::multipart::Form::new();
    let obj = match upstream_body.as_object() {
        Some(o) => o,
        None => return form,
    };

    // 辅助闭包：快速提取某个 JSON 节点中的所有图片 URL/Base64（兼容单字符串、对象数组等多种格式，避免两处提取逻辑的重复书写）
    let get_urls =
        |v: &serde_json::Value| collect_image_urls(&serde_json::json!({ "t": v }), &["t"]);

    // 1. 函数式迭代提取所有图片及 mask URL 列表
    let all_urls: Vec<String> = obj
        .iter()
        .filter_map(|(k, v)| {
            if matches!(k.as_str(), "images" | "image" | "image_urls" | "image[]") {
                Some(get_urls(v))
            } else if k == "mask" {
                v.as_str()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| vec![s.to_string()])
            } else {
                None
            }
        })
        .flatten()
        .collect();

    // 2. 并发下载并建立缓存映射
    let resolved = resolve_image_urls(client, &all_urls).await;
    use std::collections::HashMap;
    let url_to_data: HashMap<_, _> = all_urls
        .into_iter()
        .zip(resolved)
        .filter_map(|(url, opt)| opt.map(|d| (url.trim().to_string(), d)))
        .collect();

    // 3. 辅助闭包：构建二进制文件 Part（使用组合子扁平化解析逻辑）
    let add_file_part = |form_ref: reqwest::multipart::Form,
                         part_name: &str,
                         image_url: &str,
                         default_filename: &str|
     -> reqwest::multipart::Form {
        let trimmed = image_url.trim();
        if trimmed.is_empty() {
            return form_ref;
        }

        use base64::Engine;
        let resolved_bytes = url_to_data
            .get(trimmed)
            .and_then(|resolved_val| {
                let mime = resolved_val.get("mime_type")?.as_str()?;
                let b64 = resolved_val.get("data")?.as_str()?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64))
                    .ok()?;
                Some((bytes, mime.to_string()))
            })
            .or_else(|| parse_image_data(trimmed).map(|(b, m)| (b, m.to_string())));

        if let Some((bytes, mime)) = resolved_bytes {
            let ext = mime
                .split('/')
                .last()
                .unwrap_or("png")
                .split(';')
                .next()
                .unwrap_or("png")
                .trim();
            let filename = format!("{}.{}", default_filename, ext);

            // 打印二进制文件 Part 属性信息
            crate::relay_debug!(
                "[Multipart File Part] 键: {}, 文件名: {}, MIME: {}, 大小: {} 字节",
                part_name,
                filename,
                mime,
                bytes.len()
            );

            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(filename)
                .mime_str(&mime)
                .unwrap_or_else(|_| reqwest::multipart::Part::bytes(Vec::new()));
            form_ref.part(part_name.to_string(), part)
        } else {
            // 打印文本类型的图片/链接 Part 信息
            crate::relay_debug!(
                "[Multipart Text File Part] 键: {}, 值(URL): {}",
                part_name,
                trimmed
            );
            form_ref.text(part_name.to_string(), trimmed.to_string())
        }
    };

    // 4. 遍历组装表单项
    for (key, value) in obj {
        if matches!(key.as_str(), "images" | "image" | "image_urls" | "image[]") {
            // 兼容多种图片键名，提取对应的图片 URL 或 Base64 列表
            let urls = get_urls(value);
            // 单图使用 "image" 以保持对 dall-e-2 及单图场景的广泛兼容，多图使用 "image[]" 以契合多图模型规范
            let part_name = if urls.len() == 1 { "image" } else { "image[]" };
            for url in urls {
                form = add_file_part(form, part_name, &url, "image");
            }
        } else if key == "mask" {
            // mask 字段特殊处理，支持 base64 解码为二进制文件 Part，以便契合图片编辑的 mask 格式要求
            let mask_val = value.as_str().unwrap_or("");
            form = add_file_part(form, "mask", mask_val, "mask");
        } else {
            // 其他字段：序列化为文本 Part（支持字符串/数值/布尔等）
            let text_val = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => continue,
                other => other.to_string(),
            };

            // 打印普通文本 Part
            crate::relay_debug!("[Multipart Text Part] 键: {}, 值: {}", key, text_val);

            form = form.text(key.clone(), text_val);
        }
    }

    form
}

// ── 辅助函数 ──────────────────────────────────────────────────

/// 是否 base64：`data:`，或非 `http` 且 len≥min（1=即梦/腾讯，101=谷歌/GPT 原 len>100）
pub(crate) fn is_b64(s: &str, min: usize) -> bool {
    let s = s.trim();
    !s.is_empty() && (s.starts_with("data:") || (!s.starts_with("http") && s.len() >= min))
}

/// 去 `data:...,` 前缀得纯 base64；非 data URI 原样。各模块解码共用。
pub(crate) fn b64_data(s: &str) -> &str {
    let s = s.trim();
    if s.starts_with("data:") {
        s.find(',').map(|i| &s[i + 1..]).unwrap_or(s)
    } else {
        s
    }
}

/// 腾讯 FileInfos 一项；extra 如 Usage/Category。
pub(crate) fn tc_file(media: &str, extra: &[(&str, serde_json::Value)]) -> serde_json::Value {
    let mut v = if is_b64(media, 1) {
        serde_json::json!({ "Type": "Base64", "Base64": b64_data(media) })
    } else {
        serde_json::json!({ "Type": "Url", "Url": media.trim() })
    };
    if let Some(obj) = v.as_object_mut() {
        for (k, val) in extra {
            obj.insert((*k).into(), val.clone());
        }
    }
    v
}

/// 从请求体中按字段优先级收集媒体对象数组（保留 role 等原始元数据）。
/// 每个字段兼容：字符串、纯字符串数组、对象数组。
/// 如果是字符串形式，会自动包装成包含该字符串的单个 JSON 串元素数组。
pub(crate) fn collect_media_values(
    body: &serde_json::Value,
    fields: &[&str],
) -> Vec<serde_json::Value> {
    for field in fields {
        if let Some(val) = body.get(*field) {
            let mut list = Vec::new();
            if val.is_string() {
                list.push(val.clone());
            } else if let Some(arr) = val.as_array() {
                list.extend(arr.iter().cloned());
            }
            if !list.is_empty() {
                return list;
            }
        }
    }
    Vec::new()
}

/// 媒体 URL 值：纯字符串，或嵌套 `{"url":"..."}`。
pub(crate) fn as_media_url(v: &serde_json::Value) -> Option<&str> {
    v.as_str().filter(|s| !s.is_empty()).or_else(|| {
        v.get("url")
            .and_then(|u| u.as_str())
            .filter(|s| !s.is_empty())
    })
}

/// 从请求体中按字段优先级收集图片/视频 URL（提取为纯 String 列表）。
/// 每个字段兼容字符串、纯字符串数组、{url: "..."} 对象数组三种格式。
/// 默认字段优先级: image → image_urls（图片模型通用），调用方可自定义。
pub(crate) fn collect_image_urls(body: &serde_json::Value, fields: &[&str]) -> Vec<String> {
    let elements = collect_media_values(body, fields);
    let mut urls = Vec::new();
    for item in elements {
        if let Some(s) = as_media_url(&item) {
            urls.push(s.to_string());
        } else if let Some(u) = item
            .get("image")
            .or_else(|| item.get("image_url"))
            .or_else(|| item.get("video_url"))
            .or_else(|| item.get("audio_url"))
            .or_else(|| item.get("url"))
            .and_then(as_media_url)
        {
            urls.push(u.to_string());
        }
    }
    urls
}

/// 从数组元素中提取 (url, role/type)。
/// 兼容三种输入格式：
///   - 纯字符串 `"https://..."` → 使用 default_role
///   - 对象 `{"url": "https://...", "role": "first_frame"}` → 优先 role
///   - 对象 `{"url": "https://...", "type": "first_frame"}` → type 回退
pub(crate) fn parse_media_item<'a>(
    item: &'a serde_json::Value,
    default_role: &'a str,
) -> (Option<&'a str>, &'a str) {
    match item {
        serde_json::Value::String(s) if !s.is_empty() => (Some(s.as_str()), default_role),
        serde_json::Value::Object(obj) => {
            let url = obj
                .get("image")
                .or_else(|| obj.get("image_url"))
                .or_else(|| obj.get("video_url"))
                .or_else(|| obj.get("audio_url"))
                .or_else(|| obj.get("url"))
                .and_then(as_media_url);
            let role = obj
                .get("role")
                .or_else(|| obj.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or(default_role);
            (url, role)
        }
        _ => (None, default_role),
    }
}

/// 根据 images 数组长度推断默认 role 列表：
///   1 张 → ["first_frame"]
///   2 张 → ["first_frame", "last_frame"]（首尾帧）
///   3+ 张 → 全部 "reference_image"（多模态参考）
pub(crate) fn infer_image_default_roles(count: usize) -> Vec<&'static str> {
    match count {
        1 => vec!["first_frame"],
        2 => vec!["first_frame", "last_frame"],
        _ => vec!["reference_image"; count],
    }
}

/// 火山方舟多模态视频生成 role 修正。
/// 当 content 数组中同时包含 video_url 或 audio_url（参考媒体）时，
/// 火山 API 明确禁止 first_frame / last_frame 与参考媒体混用，
/// 所有 image_url 的 role 必须统一为 reference_image。
/// 此函数会：
///   1. 为缺失 role 的 image_url 补充 role = "reference_image"
///   2. 将错误的 first_frame / last_frame 纠正为 reference_image
pub(crate) fn ensure_image_roles_for_multimodal(content: serde_json::Value) -> serde_json::Value {
    let arr = match content.as_array() {
        Some(a) => a,
        None => return content,
    };

    // 检测是否包含 video/audio 参考媒体
    let has_video_or_audio = arr.iter().any(|item| {
        let t = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        t == "video_url" || t == "audio_url"
    });
    if !has_video_or_audio {
        return content;
    }

    // 收集需要修正 role 的 image_url 索引：
    // - 缺失 role
    // - role 为 first_frame / last_frame（与参考媒体冲突）
    let fix_indices: Vec<usize> = arr
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            if item.get("type").and_then(|v| v.as_str()) != Some("image_url") {
                return false;
            }
            // 在有多模态视频/音频参考的场景下，图片只能作为 reference_image
            // 无论它原本是不是 first_frame / last_frame，都必须强制覆盖
            true
        })
        .map(|(i, _)| i)
        .collect();

    if fix_indices.is_empty() {
        return content;
    }

    let mut patched = arr.clone();
    for &idx in &fix_indices {
        patched[idx]["role"] = serde_json::json!("reference_image");
    }

    serde_json::json!(patched)
}

/// OpenAI 布尔兼容：`true` 或字符串 `"true"`。
#[inline]
pub(crate) fn json_truthy(v: &serde_json::Value) -> bool {
    v.as_bool().unwrap_or(false) || v.as_str() == Some("true")
}

/// 清理 OpenAI 视频/图片请求里厂商不认识的字段。
pub(crate) fn remove_openai_compat_fields(obj: &mut serde_json::Map<String, serde_json::Value>) {
    for key in ["response_format", "watermark"] {
        obj.remove(key);
    }
}

/// OpenAI `size`（`16:9` 或 `1024x1024`）→ 最近标准宽高比（腾讯云 / MiniMax 等共用）。
/// 分辨率等级（如 1k/2k/4k）或无法解析的格式安全返回 None。
pub(crate) fn size_to_ratio(size: &str) -> Option<&str> {
    let s = size.trim();
    if s.is_empty() {
        return None;
    }
    // 显式比例（如 "16:9"、"1:1"）或自适应 "auto" / "adaptive" 直接识别
    if s.contains(':') || s.eq_ignore_ascii_case("auto") || s.eq_ignore_ascii_case("adaptive") {
        return Some(s);
    }
    // 像素尺寸（如 "1024x1024"）→ 计算宽高比并匹配最近标准比例
    let (w, h) = s
        .split_once('x')
        .or_else(|| s.split_once('X'))
        .or_else(|| s.split_once('*'))
        .or_else(|| s.split_once('×'))
        .and_then(|(a, b)| {
            let w = a.trim().parse::<f64>().ok()?;
            let h = b.trim().parse::<f64>().ok()?;
            (w > 0.0 && h > 0.0).then_some((w, h))
        })?;
    let r = w / h;
    const CANDIDATES: &[(&str, f64)] = &[
        ("1:1", 1.0),
        ("3:2", 1.5),
        ("4:3", 4.0 / 3.0),
        ("16:9", 16.0 / 9.0),
        ("21:9", 21.0 / 9.0),
        ("2:3", 2.0 / 3.0),
        ("3:4", 0.75),
        ("9:16", 9.0 / 16.0),
    ];
    CANDIDATES
        .iter()
        .min_by(|a, b| {
            (r - a.1)
                .abs()
                .partial_cmp(&(r - b.1).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|c| c.0)
}

/// 解析 data URI 为 Gemini inline_data 格式 `{mime_type, data}`。
pub(crate) fn parse_data_uri_to_inline_data(input: &str) -> Option<serde_json::Value> {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix("data:") {
        let (meta, data) = rest.split_once(',')?;
        let mime = meta
            .split(';')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("image/png");
        Some(serde_json::json!({ "mime_type": mime, "data": data }))
    } else if is_b64(input, 101) {
        Some(serde_json::json!({ "mime_type": "image/png", "data": input }))
    } else {
        None
    }
}

/// 异步下载 HTTP 图片并转为 base64 inline_data（最多重试 2 次）。
pub(crate) async fn download_image_to_base64(
    client: &reqwest::Client,
    url: &str,
) -> Option<serde_json::Value> {
    if !crate::services::http_client::is_safe_outbound_url(url).await {
        crate::relay_debug!("[DownloadImage] 目标 URL 安全校验未通过（禁止访问私有内网）: {}", url);
        return None;
    }
    for attempt in 0..2 {
        let resp = match crate::services::http_client::with_download_timeout(client.get(url))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                crate::relay_debug!(
                    "[DownloadImage] HTTP状态码={} URL={} (重试第{}次)",
                    r.status(),
                    url,
                    attempt + 1
                );
                continue;
            }
            Err(e) => {
                crate::relay_debug!(
                    "[DownloadImage] 请求失败 URL={} (重试第{}次): {}",
                    url,
                    attempt + 1,
                    e
                );
                continue;
            }
        };
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/png")
            .to_string();
        let mime = content_type.split(';').next().unwrap_or("image/png").trim();
        match resp.bytes().await {
            Ok(bytes) => {
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                return Some(serde_json::json!({
                    "mime_type": mime,
                    "data": b64
                }));
            }
            Err(e) => {
                crate::relay_debug!(
                    "[DownloadImage] 读取响应体失败 URL={} (重试第{}次): {}",
                    url,
                    attempt + 1,
                    e
                );
                continue;
            }
        }
    }
    crate::relay_debug!("[DownloadImage] 所有重试均已失败 URL={}", url);
    None
}

/// 批量解析图片 URL → inline_data；HTTP 并发下载且去重。
pub(crate) async fn resolve_image_urls(
    client: Option<&reqwest::Client>,
    urls: &[String],
) -> Vec<Option<serde_json::Value>> {
    use std::collections::HashMap;
    let mut results: Vec<Option<serde_json::Value>> = vec![None; urls.len()];
    let mut http_tasks: Vec<(usize, String)> = Vec::new();
    for (i, url) in urls.iter().enumerate() {
        let url = url.trim();
        if is_b64(url, 101) {
            results[i] = parse_data_uri_to_inline_data(url);
        } else if url.starts_with("http") {
            http_tasks.push((i, url.to_string()));
        }
    }
    if let Some(client) = client {
        if !http_tasks.is_empty() {
            let mut unique: Vec<String> = Vec::new();
            let mut url_idx: HashMap<String, usize> = HashMap::new();
            for (_, url) in &http_tasks {
                url_idx.entry(url.clone()).or_insert_with(|| {
                    unique.push(url.clone());
                    unique.len() - 1
                });
            }
            let dl = futures::future::join_all(
                unique.iter().map(|u| download_image_to_base64(client, u)),
            )
            .await;
            for (i, url) in http_tasks {
                results[i] = dl[url_idx[&url]].clone();
            }
        }
    }
    results
}
