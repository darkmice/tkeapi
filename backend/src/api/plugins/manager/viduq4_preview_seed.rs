/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：Vidu Q4 Preview 视频方案。
//! 仅 playground_2026 注入。对齐官方 img2video / reference2video：
//! 首帧 1 张，或参考图 1–15 张加参考音频 0–3 个。不支持纯文生与首尾帧。

use serde_json::{json, Value};

pub const VIDUQ4_PREVIEW_SCHEME_ID: &str = "viduq4-preview";

const DESCRIPTION: &str = "Vidu Q4 Preview：图生视频（1 张首帧）或参考生视频（参考图 1–15 张，参考音频 0–3 个）。不支持纯文生和首尾帧。时长 3–16 秒。分辨率 540p/720p/1080p/2K/4K，默认 720p。画面比例用于参考生视频（16:9、9:16、1:1、4:3、3:4），图生视频以首帧为准。首帧与参考图不要同时使用。";

const RESOLUTION_OPTIONS: &[&str] = &["540p", "720p", "1080p", "2K", "4K"];

pub fn viduq4_preview_scheme() -> Value {
    json!({
        "id": VIDUQ4_PREVIEW_SCHEME_ID,
        "name": "Vidu Q4 Preview 视频生成方案",
        "type": "video",
        "is_system": true,
        "allow_text_to_video": false,
        "description": DESCRIPTION,
        "max_reference_images": 15,
        "inputs": [
            {
                "key": "prompt",
                "label": "提示词",
                "enabled": true,
                "modality": "text",
                "handle_prefix": "Prompt",
                "bind_key": "prompt",
                "accepts": ["prompt"],
                "required": true,
                "max": 1
            },
            {
                "key": "start_frame",
                "label": "图生视频",
                "enabled": true,
                "modality": "image",
                "handle_prefix": "Start Frame",
                "bind_key": "image_url",
                "accepts": ["asset", "ai_image", "preview"],
                "accept_asset_kinds": ["image"],
                "max": 1
            },
            {
                "key": "reference_images",
                "label": "参考图",
                "enabled": true,
                "modality": "image",
                "handle_prefix": "Reference Images",
                "bind_key": "image_urls",
                "accepts": ["asset", "ai_image", "preview"],
                "accept_asset_kinds": ["image"],
                "max": 15,
                "expandable": true,
                "default_count": 1
            },
            {
                "key": "reference_audio",
                "label": "参考音频",
                "enabled": true,
                "modality": "audio",
                "handle_prefix": "Reference Audio",
                "bind_key": "sounds",
                "accepts": ["asset"],
                "accept_asset_kinds": ["audio"],
                "max": 3,
                "expandable": true,
                "default_count": 1
            }
        ],
        "outputs": [
            {
                "key": "video",
                "label": "视频",
                "enabled": true,
                "modality": "video",
                "handle_prefix": "Video",
                "result_key": "video_url",
                "max": 1
            }
        ],
        "params": [
            {
                "key": "duration",
                "label": "视频时长",
                "type": "slider",
                "data_type": "integer",
                "min": 3,
                "max": 16,
                "step": 1,
                "default": 5,
                "unit": "秒",
                "hint": "3–16 秒"
            },
            {
                "key": "aspect_ratio",
                "label": "画面比例",
                "type": "radio",
                "data_type": "string",
                "options": ["16:9", "9:16", "1:1", "4:3", "3:4"],
                "default": "16:9",
                "quick": true,
                "hint": "参考生视频生效。图生视频以首帧画面为准"
            },
            {
                "key": "resolution",
                "label": "输出分辨率",
                "type": "select",
                "data_type": "string",
                "options": RESOLUTION_OPTIONS,
                "default": "720p",
                "quick": true
            },
            {
                "key": "audio",
                "label": "生成音频",
                "type": "switch",
                "data_type": "boolean",
                "default": true,
                "hint": "开启后输出带台词和音效的视频"
            },
            {
                "key": "watermark",
                "label": "水印",
                "type": "switch",
                "data_type": "boolean",
                "default": false,
                "hint": "开启后添加「AI生成」水印"
            }
        ]
    })
}

fn allows_text_to_video(scheme: &Value) -> bool {
    scheme
        .get("allow_text_to_video")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

fn resolution_needs_billing_options(scheme: &Value) -> bool {
    let Some(params) = scheme.get("params").and_then(|v| v.as_array()) else {
        return true;
    };
    let Some(param) = params
        .iter()
        .find(|p| p.get("key").and_then(|v| v.as_str()) == Some("resolution"))
    else {
        return true;
    };
    let options = param
        .get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let expected: Vec<String> = RESOLUTION_OPTIONS.iter().map(|s| (*s).to_string()).collect();
    options != expected || param.get("default").and_then(|v| v.as_str()) != Some("720p")
}

/// 没有该方案时追加。已有方案的分辨率或文生标记与官方不一致时改回。
pub fn seed_viduq4_preview_scheme(schemes: &mut Vec<Value>) -> bool {
    if let Some(pos) = schemes.iter().position(|s| {
        s.get("id").and_then(|v| v.as_str()) == Some(VIDUQ4_PREVIEW_SCHEME_ID)
    }) {
        if !resolution_needs_billing_options(&schemes[pos]) && !allows_text_to_video(&schemes[pos])
        {
            return false;
        }
        let fresh = viduq4_preview_scheme();
        if let Some(obj) = schemes[pos].as_object_mut() {
            if let Some(params) = fresh.get("params") {
                obj.insert("params".into(), params.clone());
            }
            obj.insert("description".into(), json!(DESCRIPTION));
            obj.insert("allow_text_to_video".into(), json!(false));
        }
        return true;
    }
    schemes.push(viduq4_preview_scheme());
    true
}
