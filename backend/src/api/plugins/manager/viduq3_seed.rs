/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：Vidu Q3 四个视频方案。仅 playground_2026 注入。
//! 按官方接口拆开：Pro 无参考生，Turbo 多参考生，viduq3 与 viduq3-mix 共用参考生，漫剧单独一套。

use serde_json::{json, Value};

pub const VIDUQ3_PRO_SCHEME_ID: &str = "viduq3-pro";
pub const VIDUQ3_TURBO_SCHEME_ID: &str = "viduq3-turbo";
pub const VIDUQ3_REFERENCE_SCHEME_ID: &str = "viduq3-reference";
pub const VIDUQ3_DRAMA_SCHEME_ID: &str = "viduq3-drama";

const Q3_RES: &[&str] = &["540p", "720p", "1080p"];
const DRAMA_RES: &[&str] = &["720p", "1080p"];

fn prompt_input() -> Value {
    json!({
        "key": "prompt",
        "label": "提示词",
        "enabled": true,
        "modality": "text",
        "handle_prefix": "Prompt",
        "bind_key": "prompt",
        "accepts": ["prompt"],
        "required": true,
        "max": 1
    })
}

fn start_frame() -> Value {
    json!({
        "key": "start_frame",
        "label": "图生视频",
        "enabled": true,
        "modality": "image",
        "handle_prefix": "Start Frame",
        "bind_key": "image_url",
        "accepts": ["asset", "ai_image", "preview"],
        "accept_asset_kinds": ["image"],
        "max": 1
    })
}

fn end_frame() -> Value {
    json!({
        "key": "end_frame",
        "label": "尾帧",
        "enabled": true,
        "modality": "image",
        "handle_prefix": "End Frame",
        "bind_key": "end_image_url",
        "accepts": ["asset", "ai_image", "preview"],
        "accept_asset_kinds": ["image"],
        "max": 1
    })
}

fn reference_images(max: i64) -> Value {
    json!({
        "key": "reference_images",
        "label": "参考图",
        "enabled": true,
        "modality": "image",
        "handle_prefix": "Reference Images",
        "bind_key": "image_urls",
        "accepts": ["asset", "ai_image", "preview"],
        "accept_asset_kinds": ["image"],
        "max": max,
        "expandable": true,
        "default_count": 1
    })
}

fn video_output() -> Value {
    json!({
        "key": "video",
        "label": "视频",
        "enabled": true,
        "modality": "video",
        "handle_prefix": "Video",
        "result_key": "video_url",
        "max": 1
    })
}

fn duration_param(min: i64, max: i64, default: i64, hint: &str) -> Value {
    json!({
        "key": "duration",
        "label": "视频时长",
        "type": "slider",
        "data_type": "integer",
        "min": min,
        "max": max,
        "step": 1,
        "default": default,
        "unit": "秒",
        "hint": hint
    })
}

fn aspect_param(options: &[&str], default: &str, hint: &str) -> Value {
    json!({
        "key": "aspect_ratio",
        "label": "画面比例",
        "type": "radio",
        "data_type": "string",
        "options": options,
        "default": default,
        "quick": true,
        "hint": hint
    })
}

fn resolution_param(options: &[&str], default: &str) -> Value {
    json!({
        "key": "resolution",
        "label": "输出分辨率",
        "type": "select",
        "data_type": "string",
        "options": options,
        "default": default,
        "quick": true
    })
}

fn audio_param() -> Value {
    json!({
        "key": "audio",
        "label": "生成音频",
        "type": "switch",
        "data_type": "boolean",
        "default": true,
        "hint": "开启后输出带台词和音效的视频"
    })
}

fn watermark_param() -> Value {
    json!({
        "key": "watermark",
        "label": "水印",
        "type": "switch",
        "data_type": "boolean",
        "default": false,
        "hint": "开启后添加「AI生成」水印"
    })
}

fn scheme(
    id: &str,
    name: &str,
    description: &str,
    allow_text: bool,
    max_reference_images: i64,
    inputs: Vec<Value>,
    params: Vec<Value>,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "type": "video",
        "is_system": true,
        "allow_text_to_video": allow_text,
        "description": description,
        "max_reference_images": max_reference_images,
        "inputs": inputs,
        "outputs": [video_output()],
        "params": params
    })
}

pub fn viduq3_pro_scheme() -> Value {
    scheme(
        VIDUQ3_PRO_SCHEME_ID,
        "Vidu Q3 Pro 视频生成方案",
        "Vidu Q3 Pro：文生视频、图生视频（1 张首帧）、首尾帧。没有参考生。时长 1–16 秒。分辨率 540p/720p/1080p，默认 720p。画面比例用于文生视频。",
        true,
        0,
        vec![prompt_input(), start_frame(), end_frame()],
        vec![
            duration_param(1, 16, 5, "1–16 秒"),
            aspect_param(
                &["16:9", "9:16", "1:1", "4:3", "3:4"],
                "16:9",
                "文生视频生效。图生和首尾帧以画面为准",
            ),
            resolution_param(Q3_RES, "720p"),
            audio_param(),
            watermark_param(),
        ],
    )
}

pub fn viduq3_turbo_scheme() -> Value {
    scheme(
        VIDUQ3_TURBO_SCHEME_ID,
        "Vidu Q3 Turbo 视频生成方案",
        "Vidu Q3 Turbo：文生、图生（1 张首帧）、首尾帧，以及参考生（主体最多 7 个）。文生、图生、首尾帧时长 1–16 秒；参考生最短 3 秒。分辨率 540p/720p/1080p。参考生另支持比例 auto，与文生取交集后不含 auto。",
        true,
        7,
        vec![prompt_input(), start_frame(), end_frame(), reference_images(7)],
        vec![
            duration_param(1, 16, 5, "文生、图生、首尾帧 1–16 秒。参考生请选 3–16 秒"),
            aspect_param(
                &["16:9", "9:16", "1:1", "4:3", "3:4"],
                "16:9",
                "文生视频生效。参考生官方另有 auto，此处与文生取交集",
            ),
            resolution_param(Q3_RES, "720p"),
            audio_param(),
            watermark_param(),
        ],
    )
}

pub fn viduq3_reference_scheme() -> Value {
    scheme(
        VIDUQ3_REFERENCE_SCHEME_ID,
        "Vidu Q3 参考生视频方案",
        "Vidu Q3 与 Q3 Mix：只有参考生，主体最多 7 个。不支持文生、图生和首尾帧。时长 3–16 秒。分辨率 540p/720p/1080p，默认 720p。比例含 auto。",
        false,
        7,
        vec![prompt_input(), reference_images(7)],
        vec![
            duration_param(3, 16, 5, "3–16 秒"),
            aspect_param(
                &["16:9", "9:16", "1:1", "4:3", "3:4", "auto"],
                "16:9",
                "auto 按输入图自动推荐比例",
            ),
            resolution_param(Q3_RES, "720p"),
            audio_param(),
            watermark_param(),
        ],
    )
}

pub fn viduq3_drama_scheme() -> Value {
    scheme(
        VIDUQ3_DRAMA_SCHEME_ID,
        "Vidu Q3 漫剧参考生方案",
        "Vidu Q3 Drama：只有参考生，主体最多 7 个。时长 2–16 秒，默认 8 秒。分辨率只有 720p/1080p，默认 1080p。比例只有 16:9、9:16、auto。",
        false,
        7,
        vec![prompt_input(), reference_images(7)],
        vec![
            duration_param(2, 16, 8, "2–16 秒"),
            aspect_param(&["16:9", "9:16", "auto"], "16:9", "auto 按输入图自动推荐比例"),
            resolution_param(DRAMA_RES, "1080p"),
            audio_param(),
            watermark_param(),
        ],
    )
}

pub fn viduq3_schemes() -> [Value; 4] {
    [
        viduq3_pro_scheme(),
        viduq3_turbo_scheme(),
        viduq3_reference_scheme(),
        viduq3_drama_scheme(),
    ]
}

fn same_official_fields(saved: &Value, fresh: &Value) -> bool {
    saved.get("inputs") == fresh.get("inputs")
        && saved.get("params") == fresh.get("params")
        && saved.get("outputs") == fresh.get("outputs")
        && saved.get("allow_text_to_video") == fresh.get("allow_text_to_video")
        && saved.get("description") == fresh.get("description")
        && saved.get("max_reference_images") == fresh.get("max_reference_images")
        && saved.get("name") == fresh.get("name")
}

/// 没有方案时追加。已有方案与官方入口不一致时改回官方字段。
pub fn seed_viduq3_schemes(schemes: &mut Vec<Value>) -> bool {
    let mut changed = false;
    for fresh in viduq3_schemes() {
        let id = fresh.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(pos) = schemes
            .iter()
            .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(id))
        {
            if same_official_fields(&schemes[pos], &fresh) {
                continue;
            }
            if let Some(obj) = schemes[pos].as_object_mut() {
                for key in [
                    "name",
                    "description",
                    "allow_text_to_video",
                    "max_reference_images",
                    "inputs",
                    "outputs",
                    "params",
                ] {
                    if let Some(v) = fresh.get(key) {
                        obj.insert(key.to_string(), v.clone());
                    }
                }
            }
            changed = true;
        } else {
            schemes.push(fresh);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q3_schemes_follow_official_ports() {
        let pro = viduq3_pro_scheme();
        let turbo = viduq3_turbo_scheme();
        let reference = viduq3_reference_scheme();
        let drama = viduq3_drama_scheme();
        assert_eq!(pro["allow_text_to_video"], true);
        assert_eq!(turbo["allow_text_to_video"], true);
        assert_eq!(reference["allow_text_to_video"], false);
        assert_eq!(drama["allow_text_to_video"], false);
        assert!(pro["inputs"].as_array().unwrap().iter().all(|p| p["key"] != "reference_images"));
        assert!(turbo["inputs"].as_array().unwrap().iter().any(|p| p["key"] == "reference_images" && p["max"] == 7));
        assert!(reference["inputs"].as_array().unwrap().iter().all(|p| p["key"] != "start_frame" && p["key"] != "end_frame"));
        assert_eq!(drama["params"][2]["options"], json!(["720p", "1080p"]));
        assert_eq!(drama["params"][2]["default"], "1080p");
        assert_eq!(drama["params"][0]["min"], 2);
        assert_eq!(drama["params"][0]["default"], 8);
    }
}
