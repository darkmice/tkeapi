/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! Vidu 官方视频模型专属适配器：请求体构建与动态端点选路。
//! 支持文生视频、首帧图生视频、首尾帧生视频与多主体参考生视频。
//! 参考文档：https://platform.vidu.cn/docs/api-reference/video-models/vidu-q4-preview/image-to-video

use super::super::media;
use crate::relay::forward::ResolvedForward;
use crate::relay::usage_extractor::ExtractedFeatures;

/// Vidu 官方视频接口透传参数白名单（官方优先原则，images 经下方专用流程标准化提取纯 URL 数组）
const VIDU_VIDEO_PASSTHROUGH_KEYS: &[&str] = &[
    "prompt",
    "duration",
    "resolution",
    "aspect_ratio",
    "audio",
    "is_rec",
    "seed",
    "off_peak",
    "watermark",
    "wm_position",
    "wm_url",
    "payload",
    "meta_data",
    "callback_url",
];

/// 构建 Vidu 官方视频请求体
pub(crate) fn build_vidu_video_body(model: &str, body: &serde_json::Value) -> serde_json::Value {
    let mut result = serde_json::Map::new();

    // 1. 模型名：优先使用请求体自选的 model，缺省回退路由匹配值
    let model_name = body.get("model").and_then(|v| v.as_str()).unwrap_or(model);
    result.insert("model".to_string(), serde_json::json!(model_name));

    // 2. 官方原生参数直接透传（白名单驱动，原样无损传递，不设多余限制）
    for &key in VIDU_VIDEO_PASSTHROUGH_KEYS {
        if let Some(val) = body.get(key) {
            result.insert(key.to_string(), val.clone());
        }
    }

    // 3. 图像与参考音频输入处理：统一提取纯字符串 URL 列表（兼容官方 sounds/images 与 OpenAI audios/image_urls 字符串或对象数组）
    let sounds = media::collect_image_urls(body, &["sounds", "audios"]);
    if !sounds.is_empty() {
        result.insert("sounds".to_string(), serde_json::json!(sounds));
    }

    let media_items = media::collect_media_values(body, &["images", "image_urls"]);
    if !media_items.is_empty() {
        let mut first_frame = None;
        let mut end_frame = None;
        let mut others = Vec::new();

        for item in &media_items {
            let (url_opt, role) = media::parse_media_item(item, "");
            if let Some(u) = url_opt.filter(|s| !s.is_empty()) {
                match role {
                    "first_frame" if first_frame.is_none() => first_frame = Some(u.to_string()),
                    "last_frame" if end_frame.is_none() => end_frame = Some(u.to_string()),
                    _ => others.push(u.to_string()),
                }
            }
        }

        let mut final_images = Vec::with_capacity(media_items.len());
        if let Some(f) = first_frame.or_else(|| {
            if end_frame.is_some() && !others.is_empty() {
                Some(others.remove(0))
            } else {
                None
            }
        }) {
            final_images.push(f);
        }
        if let Some(e) = end_frame {
            final_images.push(e);
        }
        final_images.extend(others);

        if !final_images.is_empty() {
            result.insert("images".to_string(), serde_json::json!(final_images));
        }
    }

    // 4. 仅在官方参数缺失时，映射对应的 OpenAI 视频规范参数与默认兜底
    if !result.contains_key("aspect_ratio") {
        if let Some(r) = body.get("ratio") {
            result.insert("aspect_ratio".to_string(), r.clone());
        }
    }
    if !result.contains_key("audio") {
        if let Some(ga) = body.get("generate_audio") {
            result.insert("audio".to_string(), ga.clone());
        }
    }
    if !result.contains_key("resolution") {
        result.insert("resolution".to_string(), serde_json::json!("540p"));
    }
    if !result.contains_key("off_peak")
        && body.get("service_tier").and_then(|v| v.as_str()) == Some("flex")
    {
        result.insert("off_peak".to_string(), serde_json::json!(true));
    }

    serde_json::Value::Object(result)
}

/// Vidu 动态路径选路：针对 OpenAI 兼容路由，复用前置计费特征与 role/type 智能推导
pub fn resolve_vidu_dynamic_path(
    resolved: &mut ResolvedForward,
    req_body: &serde_json::Value,
    features: Option<&ExtractedFeatures>,
) {
    let path = resolved.upstream_path.trim_end_matches('/');
    // 规则已显式指定具体端点，尊重用户配置
    if path.contains("text2video")
        || path.contains("img2video")
        || path.contains("start-end2video")
        || path.contains("reference2video")
    {
        return;
    }

    let images_arr = req_body
        .get("images")
        .or_else(|| req_body.get("image_urls"))
        .and_then(|v| v.as_array());

    // 优先复用前置计费特征中的图片数，避免二次解析
    let images_count = features
        .and_then(|f| f.image_ref_count)
        .unwrap_or_else(|| images_arr.map_or(0, |a| a.len() as i32));

    let has_sounds = !media::collect_image_urls(req_body, &["sounds", "audios"]).is_empty();

    // 单次循环同时提取首尾帧角色与参考图角色
    let (mut has_end_role, mut has_ref_role) = (false, false);
    if let Some(arr) = images_arr {
        for item in arr {
            let (_, role) = media::parse_media_item(item, "");
            match role {
                "last_frame" => has_end_role = true,
                "reference_image" | "reference" => has_ref_role = true,
                _ => {}
            }
        }
    }

    // ── 智能选路判定（表达式赋值，简洁优雅）──
    resolved.upstream_path = if (images_count == 2 && !has_ref_role && !has_sounds) || has_end_role {
        "/ent/v2/start-end2video"
    } else if has_sounds || has_ref_role || images_count > 2 {
        "/ent/v2/reference2video"
    } else if images_count == 1 {
        "/ent/v2/img2video"
    } else {
        "/ent/v2/text2video"
    }
    .to_string();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_build_vidu_video_body_official_passthrough() {
        let input = json!({
            "model": "viduq4-preview",
            "prompt": "sunset over ocean",
            "duration": 5,
            "resolution": "1080p",
            "aspect_ratio": "16:9",
            "audio": true,
            "seed": 123456,
            "images": ["https://example.com/1.png"],
            "sounds": ["https://example.com/bgm.mp3"]
        });
        let res = build_vidu_video_body("viduq4-preview", &input);
        assert_eq!(res["model"], "viduq4-preview");
        assert_eq!(res["prompt"], "sunset over ocean");
        assert_eq!(res["duration"], 5);
        assert_eq!(res["resolution"], "1080p");
        assert_eq!(res["aspect_ratio"], "16:9");
        assert_eq!(res["audio"], true);
        assert_eq!(res["seed"], 123456);
        assert_eq!(res["images"], json!(["https://example.com/1.png"]));
        assert_eq!(res["sounds"], json!(["https://example.com/bgm.mp3"]));
    }

    #[test]
    fn test_build_vidu_video_body_openai_mapping_and_frame_sort() {
        let input = json!({
            "model": "viduq4-preview",
            "prompt": "a flying eagle",
            "ratio": "9:16",
            "generate_audio": true,
            "service_tier": "flex",
            "audios": ["https://example.com/ref.mp3"],
            "images": [
                {"url": "https://example.com/tail.png", "role": "last_frame"},
                {"url": "https://example.com/head.png", "role": "first_frame"}
            ]
        });
        let res = build_vidu_video_body("viduq4-preview", &input);
        assert_eq!(res["prompt"], "a flying eagle");
        assert_eq!(res["aspect_ratio"], "9:16");
        assert_eq!(res["audio"], true);
        assert_eq!(res["resolution"], "540p");
        assert_eq!(res["off_peak"], true);
        assert_eq!(res["sounds"], json!(["https://example.com/ref.mp3"]));
        // 首尾帧排序保证：首帧排第1，尾帧排第2
        assert_eq!(
            res["images"],
            json!(["https://example.com/head.png", "https://example.com/tail.png"])
        );
    }

    #[test]
    fn test_resolve_vidu_dynamic_path() {
        let mut forward =
            crate::relay::forward::make_forward("vidu", "/v1/video/generations", "token");

        // 1. 文生视频 (0 图)
        forward.upstream_path = "/v1/video/generations".to_string();
        let body_text = json!({"prompt": "cat"});
        resolve_vidu_dynamic_path(&mut forward, &body_text, None);
        assert_eq!(forward.upstream_path, "/ent/v2/text2video");

        // 2. 首帧图生视频 (1 图 + generate_audio: true 不应误判为 reference2video)
        forward.upstream_path = "/v1/video/generations".to_string();
        let body_img = json!({"prompt": "cat", "images": ["https://example.com/1.png"], "generate_audio": true});
        let mut features_img = ExtractedFeatures::default();
        features_img.image_ref_count = Some(1);
        features_img.has_audio = true;
        resolve_vidu_dynamic_path(&mut forward, &body_img, Some(&features_img));
        assert_eq!(forward.upstream_path, "/ent/v2/img2video");

        // 3. 首尾帧生视频 (2 图无 ref/sound)
        forward.upstream_path = "/v1/video/generations".to_string();
        let body_start_end = json!({
            "prompt": "cat",
            "images": ["https://example.com/1.png", "https://example.com/2.png"]
        });
        let mut features_2 = ExtractedFeatures::default();
        features_2.image_ref_count = Some(2);
        resolve_vidu_dynamic_path(&mut forward, &body_start_end, Some(&features_2));
        assert_eq!(forward.upstream_path, "/ent/v2/start-end2video");

        // 4. 参考生视频 (3 图主体参考)
        forward.upstream_path = "/v1/video/generations".to_string();
        let body_ref_3 = json!({
            "prompt": "cat",
            "images": ["https://example.com/1.png", "https://example.com/2.png", "https://example.com/3.png"]
        });
        let mut features_3 = ExtractedFeatures::default();
        features_3.image_ref_count = Some(3);
        resolve_vidu_dynamic_path(&mut forward, &body_ref_3, Some(&features_3));
        assert_eq!(forward.upstream_path, "/ent/v2/reference2video");

        // 5. 参考生视频 (单图且带 sounds / audios 参考音频)
        for audio_key in ["sounds", "audios"] {
            forward.upstream_path = "/v1/video/generations".to_string();
            let body_ref_audio = json!({
                "prompt": "cat",
                "images": ["https://example.com/1.png"],
                audio_key: ["https://example.com/sound.mp3"]
            });
            let mut features_sound = ExtractedFeatures::default();
            features_sound.image_ref_count = Some(1);
            features_sound.has_audio = true;
            resolve_vidu_dynamic_path(&mut forward, &body_ref_audio, Some(&features_sound));
            assert_eq!(forward.upstream_path, "/ent/v2/reference2video");
        }

        // 6. 原生端点已指定时，直接保持原样
        forward.upstream_path = "/ent/v2/start-end2video".to_string();
        let body_keep = json!({"prompt": "cat"});
        resolve_vidu_dynamic_path(&mut forward, &body_keep, None);
        assert_eq!(forward.upstream_path, "/ent/v2/start-end2video");
    }
}
