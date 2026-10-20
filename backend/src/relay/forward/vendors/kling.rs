/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use super::super::media;

// ── 可灵 AI 请求体构建器 ──────────────────────────────────────
//
// 将 OpenAI 兼容格式转换为可灵官方 API 格式。
// - model → model_name（可灵官方字段名）
// - 白名单驱动参数透传，扩展时追加一行即可
// - 直通模式：body 已含 model_name → 原样透传，仅补充默认值
// 参考文档：https://klingai.com/document-api/apiReference

/// 可灵图片接口透传参数白名单
const KLING_IMAGE_PASSTHROUGH_KEYS: &[&str] = &[
    "model_name",
    "prompt",
    "negative_prompt",
    "n",
    "aspect_ratio",
    "resolution",
    "image",
    "image_list",
    "element_list",
    "subject_image_list",
    "image_fidelity",
    "series_amount",
    "callback_url",
    "external_task_id",
    "image_reference",
    "result_type",
    "watermark_info",
];

pub(crate) fn build_kling_body(
    model: &str,
    body: &serde_json::Value,
    _category: &str,
    upstream_path: &str,
) -> serde_json::Value {
    let is_omni = upstream_path.contains("omni-image");

    let mut result = serde_json::Map::new();

    // model_name：优先使用请求体中已有的（官方原生调用），否则从 model 转换
    if let Some(mn) = body.get("model_name").and_then(|v| v.as_str()) {
        result.insert("model_name".to_string(), serde_json::json!(mn));
    } else {
        result.insert("model_name".to_string(), serde_json::json!(model));
    }

    // 白名单驱动透传（跳过 model_name，已处理）
    for &key in KLING_IMAGE_PASSTHROUGH_KEYS {
        if key == "model_name" {
            continue;
        }
        if let Some(val) = body.get(key) {
            result.insert(key.to_string(), val.clone());
        }
    }

    // 兼容 OpenAI 的 ratio -> aspect_ratio (可灵官方参数名为 aspect_ratio)
    if !result.contains_key("aspect_ratio") {
        if let Some(ratio) = body.get("ratio") {
            result.insert("aspect_ratio".to_string(), ratio.clone());
        }
    }

    // 图片 resolution：优先可灵官方 resolution → OpenAI size → 兜底 1k
    if !result.contains_key("resolution") {
        let fallback = body.get("size").and_then(|v| v.as_str()).unwrap_or("1k");
        result.insert("resolution".to_string(), serde_json::json!(fallback));
    }

    // OpenAI 兼容：media::collect_image_urls 统一收集 image/image_urls
    // omni-image：多图 → image_list[].image
    // 非 omni：单图 → image，多图 → subject_image_list[].subject_image
    if !result.contains_key("subject_image_list") && !result.contains_key("image_list") {
        let urls = media::collect_image_urls(body, &["image", "image_urls"]);
        result.remove("image");
        result.remove("image_urls");
        if is_omni {
            // omni-image：全部走 image_list[].image
            if !urls.is_empty() {
                let list: Vec<serde_json::Value> = urls
                    .iter()
                    .map(|u| serde_json::json!({ "image": u }))
                    .collect();
                result.insert("image_list".to_string(), serde_json::json!(list));
            }
        } else {
            // 非 omni：单图 → image，多图 → subject_image_list[].subject_image
            match urls.len() {
                0 => {}
                1 => {
                    result.insert("image".to_string(), serde_json::json!(&urls[0]));
                }
                _ => {
                    let list: Vec<serde_json::Value> = urls
                        .iter()
                        .map(|u| serde_json::json!({ "subject_image": u }))
                        .collect();
                    result.insert("subject_image_list".to_string(), serde_json::json!(list));
                }
            }
        }
    }

    serde_json::Value::Object(result)
}

fn kling_v3_content_item(type_name: &str, url: &str) -> serde_json::Value {
    serde_json::json!({ "type": type_name, "url": url })
}

fn kling_v3_image_type(role: &str, idx: usize, pair_plain: bool) -> &'static str {
    if pair_plain {
        return if idx == 0 {
            "first_frame"
        } else {
            "last_frame"
        };
    }
    match role.to_ascii_lowercase().as_str() {
        "first_frame" | "first" => "first_frame",
        "last_frame" | "end_frame" | "last" | "tail" => "last_frame",
        "refer_image" | "reference_image" => "refer_image",
        _ if idx == 0 => "first_frame",
        _ => "refer_image",
    }
}

fn kling_v3_build_settings(body: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut settings = serde_json::Map::new();
    let resolution = body
        .pointer("/settings/resolution")
        .or_else(|| body.get("resolution"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("720p");
    settings.insert("resolution".into(), serde_json::json!(resolution));
    if let Some(d) = body
        .pointer("/settings/duration")
        .or_else(|| body.get("duration"))
    {
        settings.insert("duration".into(), d.clone());
    }
    // settings.audio / audio / OpenAI generate_audio → off|native
    let audio = if let Some(a) = body
        .pointer("/settings/audio")
        .or_else(|| body.get("audio"))
        .and_then(|v| v.as_str())
    {
        match a.to_ascii_lowercase().as_str() {
            "off" | "false" => "off",
            _ => "native",
        }
    } else if let Some(ga) = body.get("generate_audio") {
        let on = media::json_truthy(ga);
        if on {
            "native"
        } else {
            "off"
        }
    } else {
        "off"
    };
    settings.insert("audio".into(), serde_json::json!(audio));
    if let Some(r) = body
        .pointer("/settings/aspect_ratio")
        .or_else(|| body.get("aspect_ratio"))
        .or_else(|| body.get("ratio"))
    {
        settings.insert("aspect_ratio".into(), r.clone());
    }
    if let Some(ms) = body
        .pointer("/settings/multi_shot")
        .or_else(|| body.get("multi_shot"))
    {
        settings.insert("multi_shot".into(), ms.clone());
    }
    settings
}

fn kling_v3_build_options(body: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut options = body
        .get("options")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    for key in ["callback_url", "external_task_id", "watermark_info"] {
        if !options.contains_key(key) {
            if let Some(v) = body.get(key) {
                options.insert(key.into(), v.clone());
            }
        }
    }
    options
}

/// 可灵 3.0：contents / settings / options（模型在 URL）
/// - 文生：顶层 `prompt`，无 contents
/// - 图生 / Omni：`contents`（含 prompt 条目 + 媒体；Omni 的 prompt 一律在 contents）
pub(crate) fn build_kling_v3_body(
    body: &serde_json::Value,
    upstream_path: &str,
) -> serde_json::Value {
    let is_omni = upstream_path.contains("omni-video");

    // 已含 contents 透传；文生 prompt+settings 也可透传。Omni 禁止仅靠顶层 prompt 透传。
    if body.get("contents").is_some()
        || (!is_omni && body.get("prompt").is_some() && body.get("settings").is_some())
    {
        let mut out = body.clone();
        if let Some(obj) = out.as_object_mut() {
            obj.remove("model");
            obj.remove("model_name");
            // Omni：顶层 prompt 迁入 contents（已有 prompt 条目则只去掉顶层）
            if is_omni {
                if let Some(p) = obj.remove("prompt") {
                    let text = p.as_str().unwrap_or("").trim();
                    if !text.is_empty() {
                        let mut contents = obj
                            .remove("contents")
                            .and_then(|v| v.as_array().cloned())
                            .unwrap_or_default();
                        let has_prompt = contents
                            .iter()
                            .any(|c| c.get("type").and_then(|t| t.as_str()) == Some("prompt"));
                        if !has_prompt {
                            contents
                                .insert(0, serde_json::json!({ "type": "prompt", "text": text }));
                        }
                        obj.insert("contents".into(), serde_json::Value::Array(contents));
                    }
                }
            }
            let mut settings = obj
                .remove("settings")
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            for (k, v) in kling_v3_build_settings(body) {
                settings.entry(k).or_insert(v);
            }
            obj.insert("settings".into(), serde_json::Value::Object(settings));
            let options = kling_v3_build_options(body);
            if !options.is_empty() {
                obj.insert("options".into(), serde_json::Value::Object(options));
            }
        }
        return out;
    }

    // 媒体 → contents（prompt 稍后按文/非文分流）；仅新协议 images/image_urls/videos，不读旧 image/image_tail
    let mut media: Vec<serde_json::Value> = Vec::new();

    let images = media::collect_media_values(body, &["images", "image_urls"]);
    let parsed_imgs: Vec<(String, String)> = images
        .iter()
        .filter_map(|item| {
            let (url_opt, role) = media::parse_media_item(item, "");
            url_opt
                .filter(|u| !u.is_empty())
                .map(|u| (u.to_string(), role.to_string()))
        })
        .collect();
    let pair_plain = parsed_imgs.len() == 2 && parsed_imgs.iter().all(|(_, r)| r.is_empty());
    for (idx, (url, role)) in parsed_imgs.into_iter().enumerate() {
        media.push(kling_v3_content_item(
            kling_v3_image_type(&role, idx, pair_plain),
            &url,
        ));
    }

    if is_omni || body.get("videos").is_some() {
        for item in media::collect_media_values(body, &["videos"]) {
            let (url_opt, role) = media::parse_media_item(&item, "");
            let Some(url) = url_opt.filter(|u| !u.is_empty()) else {
                continue;
            };
            let ty = match role.to_ascii_lowercase().as_str() {
                "feature" | "feature_video" => "feature_video",
                _ => "base_video",
            };
            media.push(kling_v3_content_item(ty, url));
        }
    }

    // 文生：顶层 prompt；图生 / Omni：prompt 进 contents
    let is_text = !is_omni && media.is_empty();
    let mut result = serde_json::Map::new();

    if is_text {
        if let Some(p) = body.get("prompt") {
            result.insert("prompt".into(), p.clone());
        }
    } else {
        let mut contents = Vec::with_capacity(media.len() + 1);
        if let Some(p) = body
            .get("prompt")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            contents.push(serde_json::json!({ "type": "prompt", "text": p }));
        }
        contents.extend(media);
        result.insert("contents".into(), serde_json::Value::Array(contents));
    }

    result.insert(
        "settings".into(),
        serde_json::Value::Object(kling_v3_build_settings(body)),
    );
    let options = kling_v3_build_options(body);
    if !options.is_empty() {
        result.insert("options".into(), serde_json::Value::Object(options));
    }

    serde_json::Value::Object(result)
}

// ── 可灵 JWT 自动生成（仅旧 target_type=kling）────────────────
//
// 渠道 api_key 格式："{access_key}:{secret_key}"
// 使用 HS256 算法生成 30 分钟有效期的 JWT Token。
// 如果 api_key 不含 ":" 分隔符，视为已生成的 Token / 普通 Bearer，由调用方直传。
// 新标准 kling_video 使用官方 API Key + Authorization: Bearer，不走本函数。

pub(crate) fn generate_kling_jwt(api_key: &str) -> Option<String> {
    // 不含 ":" 时视为已生成的 JWT 或普通 Bearer Token，直接使用
    let (ak, sk) = api_key.split_once(':')?;
    if ak.is_empty() || sk.is_empty() {
        return None;
    }

    let now = chrono::Utc::now().timestamp() as usize;
    let claims = serde_json::json!({
        "iss": ak,
        "exp": now + 1800,
        "nbf": now - 5
    });

    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
    let key = jsonwebtoken::EncodingKey::from_secret(sk.as_bytes());

    jsonwebtoken::encode(&header, &claims, &key).ok()
}
