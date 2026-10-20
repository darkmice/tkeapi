/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! fal.ai 队列视频：请求体 + 轮询结果补齐。
//! 参考：https://fal.ai/models/minimax/h3-max/reference-to-video/api

use super::super::media;
use super::super::poll_enrich::PollEnrichErr;
use crate::relay::response_formatter::{
    extract_error_message_from_value, parse_raw_status_to_standard,
};

const FAL_VIDEO_KEYS: &[&str] = &[
    "prompt",
    "duration",
    "resolution",
    "seed",
    "enable_safety_checker",
    "sync_mode",
    "prompt_expansion_mode",
    "aspect_ratio",
    "reference_image_urls",
    "reference_video_urls",
    "reference_audio_urls",
];

const FLAT_REF_FIELDS: &[(&str, &[&str])] = &[
    ("reference_image_urls", &["images", "image_urls"]),
    ("reference_video_urls", &["videos"]),
    ("reference_audio_urls", &["audios"]),
];

/// COMPLETED 后再拉结果；结果 URL = status URL 去掉 `/status`。
pub(crate) async fn fal_enrich_queue_poll(
    http_client: &reqwest::Client,
    auth_headers: Vec<(String, String)>,
    status_url: &str,
    status_body: String,
) -> Result<(String, String), PollEnrichErr> {
    let status_json: serde_json::Value =
        serde_json::from_str(&status_body).unwrap_or(serde_json::json!({}));
    let raw = status_json
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    if parse_raw_status_to_standard(raw) != "completed" {
        return Ok((status_url.to_string(), status_body));
    }

    // COMPLETED + error → 业务失败体
    if status_json.get("error").is_some_and(|e| !e.is_null()) {
        let msg = extract_error_message_from_value(&status_json)
            .unwrap_or_else(|| "generation failed".into());
        return Ok((
            status_url.to_string(),
            fal_failed_body(status_json.get("request_id"), &msg, 0),
        ));
    }

    let result_url = {
        let trimmed = status_url.trim_end_matches('/');
        trimmed
            .strip_suffix("/status")
            .unwrap_or(trimmed)
            .to_string()
    };

    let mut builder = http_client
        .get(&result_url)
        .timeout(std::time::Duration::from_secs(30));
    for (k, v) in auth_headers {
        builder = builder.header(k, v);
    }
    let resp = builder.send().await.map_err(|e| PollEnrichErr {
        http_status: None,
        message: format!("请求渠道失败: {}", e),
    })?;
    if resp.status().as_u16() == 202 {
        return Ok((status_url.to_string(), status_body));
    }
    if !resp.status().is_success() {
        let http = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&err_body)
            .ok()
            .and_then(|v| extract_error_message_from_value(&v))
            .unwrap_or_else(|| format!("渠道返回错误状态码: {}", http));
        return Ok((
            result_url,
            fal_failed_body(status_json.get("request_id"), &message, http.as_u16()),
        ));
    }

    let result_text = resp.text().await.unwrap_or_default();
    let mut result_json: serde_json::Value =
        serde_json::from_str(&result_text).unwrap_or(serde_json::json!({}));
    if let Some(obj) = result_json.as_object_mut() {
        obj.entry("status")
            .or_insert_with(|| serde_json::json!("COMPLETED"));
        if !obj.contains_key("request_id") {
            if let Some(id) = status_json.get("request_id").cloned() {
                obj.insert("request_id".into(), id);
            }
        }
    }
    Ok((
        result_url,
        serde_json::to_string(&result_json).unwrap_or(result_text),
    ))
}

/// 业务失败体；`http` 为 4xx/5xx 时写入 `error.code`，供结算与轮询 HTTP 透出。
fn fal_failed_body(request_id: Option<&serde_json::Value>, message: &str, http: u16) -> String {
    let mut failed = serde_json::json!({
        "status": "FAILED",
        "error": { "message": message }
    });
    if (400..600).contains(&http) {
        failed["error"]["code"] = serde_json::json!(http);
    }
    if let Some(id) = request_id {
        failed["request_id"] = id.clone();
    }
    failed.to_string()
}

/// 将 OpenAI / content[] 转为 fal queue input；`duration` 默认 5，`prompt_expansion_mode` 默认 balanced。
pub(crate) fn build_fal_video_body(_model: &str, body: &serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::Map::new();

    for key in FAL_VIDEO_KEYS {
        if let Some(v) = body.get(*key).filter(|v| !v.is_null()) {
            out.insert((*key).into(), v.clone());
        }
    }

    fill_from_content(&mut out, body);

    for (dst, srcs) in FLAT_REF_FIELDS {
        if out.contains_key(*dst) {
            continue;
        }
        let urls = media::collect_image_urls(body, srcs);
        if !urls.is_empty() {
            out.insert((*dst).into(), serde_json::json!(urls));
        }
    }

    if !out.contains_key("aspect_ratio") {
        if let Some(r) = body
            .get("ratio")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            out.insert("aspect_ratio".into(), serde_json::json!(r));
        }
    }

    if !out.contains_key("duration") {
        out.insert("duration".into(), serde_json::json!(5));
    }
    if !out.contains_key("prompt_expansion_mode") {
        out.insert(
            "prompt_expansion_mode".into(),
            serde_json::json!("balanced"),
        );
    }

    // 官方枚举 480P / 768P
    if let Some(res) = out.get("resolution").and_then(|v| v.as_str()) {
        out.insert(
            "resolution".into(),
            serde_json::json!(res.to_ascii_uppercase()),
        );
    }

    serde_json::Value::Object(out)
}

fn fill_from_content(
    out: &mut serde_json::Map<String, serde_json::Value>,
    body: &serde_json::Value,
) {
    let Some(arr) = body.get("content").and_then(|c| c.as_array()) else {
        return;
    };

    let mut texts: Vec<&str> = Vec::new();
    let mut images = Vec::new();
    let mut videos = Vec::new();
    let mut audios = Vec::new();

    for item in arr {
        match item.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "text" => {
                if let Some(t) = item
                    .get("text")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    texts.push(t);
                }
            }
            ty @ ("image_url" | "video_url" | "audio_url") => {
                if let (Some(u), _) = media::parse_media_item(item, "") {
                    let u = u.to_string();
                    match ty {
                        "image_url" => images.push(u),
                        "video_url" => videos.push(u),
                        _ => audios.push(u),
                    }
                }
            }
            _ => {}
        }
    }

    if !out.contains_key("prompt") && !texts.is_empty() {
        out.insert("prompt".into(), serde_json::json!(texts.join("\n")));
    }
    insert_urls(out, "reference_image_urls", images);
    insert_urls(out, "reference_video_urls", videos);
    insert_urls(out, "reference_audio_urls", audios);
}

fn insert_urls(out: &mut serde_json::Map<String, serde_json::Value>, key: &str, urls: Vec<String>) {
    if !out.contains_key(key) && !urls.is_empty() {
        out.insert(key.into(), serde_json::json!(urls));
    }
}

/// 轮询成功后为 OpenAI 兼容响应体注入 fal 专属多模态消耗指标（total_seconds / input_seconds / output_seconds 等）
pub(crate) fn inject_fal_poll_usage(json_str: &str, billing_features: &str) -> String {
    if billing_features.is_empty() {
        return json_str.to_string();
    }
    let feat: crate::relay::usage_extractor::ExtractedFeatures =
        match serde_json::from_str(billing_features) {
            Ok(f) => f,
            Err(_) => return json_str.to_string(),
        };

    let in_sec = feat.video_ref_seconds.unwrap_or(0.0).round() as u64;
    let out_sec = feat.duration_seconds.unwrap_or(0.0).round() as u64;
    let total_sec = in_sec + out_sec;
    let img_cnt = feat.image_ref_count.or(feat.image_count).unwrap_or(0);
    let audio_sec = feat.audio_ref_seconds.unwrap_or(0.0).round() as u64;

    let has_features = feat.duration_seconds.is_some()
        || feat.video_ref_seconds.is_some()
        || feat.audio_ref_seconds.is_some()
        || feat.image_ref_count.is_some()
        || total_sec > 0;

    if !has_features {
        return json_str.to_string();
    }

    let mut val: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return json_str.to_string(),
    };

    let is_success = val
        .get("status")
        .and_then(|s| s.as_str())
        .is_some_and(|s| s == "completed" || s == "succeeded")
        || val.get("data").is_some();

    if !is_success {
        return json_str.to_string();
    }

    let mut usage = val
        .get("usage")
        .and_then(|u| u.as_object().cloned())
        .unwrap_or_default();

    if !usage.contains_key("prompt_tokens") {
        usage.insert("prompt_tokens".to_string(), serde_json::json!(0));
    }
    if !usage.contains_key("completion_tokens") {
        usage.insert("completion_tokens".to_string(), serde_json::json!(0));
    }
    if !usage.contains_key("total_tokens") {
        usage.insert("total_tokens".to_string(), serde_json::json!(0));
    }

    usage.insert("total_seconds".to_string(), serde_json::json!(total_sec));
    usage.insert("input_seconds".to_string(), serde_json::json!(in_sec));
    usage.insert("output_seconds".to_string(), serde_json::json!(out_sec));
    usage.insert("input_image_count".to_string(), serde_json::json!(img_cnt));
    usage.insert(
        "input_audio_seconds".to_string(),
        serde_json::json!(audio_sec),
    );

    val["usage"] = serde_json::Value::Object(usage);
    val.to_string()
}
