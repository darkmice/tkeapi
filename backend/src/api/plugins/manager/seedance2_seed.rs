/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：Seedance 方案对齐官方视频生成 API。
//! 仅 playground_2026 覆写，不改旧创作中心默认方案。
//! 编辑/延长是全能参考子能力开关，不是独立上传口。
//! https://www.volcengine.com/docs/82379/1520757

use serde_json::{json, Value};

pub const SEEDANCE2_SCHEME_ID: &str = "seedance2.0";

/// 官方能力：文生 / 图生（首帧）/ 首尾帧 / 全能参考；编辑与延长为全能参考子能力。
/// IO 面板可选 2.0 或 2.5 上限。首尾帧与全能参考互斥。
pub fn seedance2_video_scheme_io() -> Value {
    json!({
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
                "key": "end_frame",
                "label": "尾帧",
                "enabled": true,
                "modality": "image",
                "handle_prefix": "End Frame",
                "bind_key": "end_image_url",
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
                "max": 9,
                "expandable": true,
                "default_count": 1
            },
            {
                "key": "reference_videos",
                "label": "参考视频",
                "enabled": true,
                "modality": "video",
                "handle_prefix": "Reference Videos",
                "bind_key": "video_urls",
                "accepts": ["asset", "ai_video", "preview"],
                "accept_asset_kinds": ["video"],
                "max": 3,
                "expandable": true,
                "default_count": 1
            },
            {
                "key": "reference_audio",
                "label": "参考音频",
                "enabled": true,
                "modality": "audio",
                "handle_prefix": "Reference Audio",
                "bind_key": "audio_urls",
                "accepts": ["asset"],
                "accept_asset_kinds": ["audio"],
                "max": 3,
                "expandable": true,
                "default_count": 1
            },
            {
                "key": "edit_video",
                "label": "编辑视频",
                "enabled": true,
                "modality": "video",
                "handle_prefix": "Edit Video",
                "bind_key": "feature:edit_video",
                "max": 0
            },
            {
                "key": "extend_video",
                "label": "延长视频",
                "enabled": true,
                "modality": "video",
                "handle_prefix": "Extend Video",
                "bind_key": "feature:extend_video",
                "max": 0
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
        ]
    })
}

pub fn seedance2_scheme() -> Value {
    let io = seedance2_video_scheme_io();
    let mut scheme = json!({
        "id": SEEDANCE2_SCHEME_ID,
        "name": "Seedance 2.0 方案",
        "type": "video",
        "is_system": true,
        "description": "火山 Seedance：文生、图生（首帧）、首尾帧、全能参考（图/视频/音频）。编辑与延长是全能参考子能力（默认开）：编辑保持宽高比与时长，延长保持宽高比。IO 面板可选 2.0（图≤9 视频≤3 音频≤3，无参考按文生视频，参考音频须搭配参考图）或 2.5（图≤30 视频≤10 音频≤10，可任意搭配，时长最长 30 秒）。",
        "max_reference_images": 9,
        "params": [
            {
                "key": "duration",
                "label": "视频时长",
                "type": "slider",
                "data_type": "integer",
                "min": 4,
                "max": 15,
                "step": 1,
                "default": 5,
                "unit": "秒",
                "hint": "2.0 文生/图生/全能参考 4–15 秒，2.5 为 4–30 秒。仅编辑由模型自选时长（请求传 -1）；延长视频可选手动秒数"
            },
            {
                "key": "ratio",
                "label": "画面比例",
                "type": "radio",
                "data_type": "string",
                "options": ["21:9", "16:9", "4:3", "1:1", "3:4", "9:16", "adaptive"],
                "default": "16:9",
                "quick": true,
                "hint": "文生、全能参考可选固定比例。首帧/首尾帧、编辑、延长必须 adaptive"
            },
            {
                "key": "resolution",
                "label": "输出分辨率",
                "type": "select",
                "data_type": "string",
                "options": ["480p", "720p", "1080p", "4K"],
                "default": "480p",
                "quick": true
            },
            {
                "key": "watermark",
                "label": "水印",
                "type": "switch",
                "data_type": "boolean",
                "default": false
            },
            {
                "key": "return_last_frame",
                "label": "返回最后一帧",
                "type": "switch",
                "data_type": "boolean",
                "default": false
            },
            {
                "key": "generate_audio",
                "label": "生成音频",
                "type": "switch",
                "data_type": "boolean",
                "default": false,
                "hint": "原生有声视频"
            },
            {
                "key": "web_search",
                "label": "联网搜索",
                "type": "switch",
                "data_type": "boolean",
                "default": false,
                "hint": "仅文生视频生效"
            }
        ]
    });
    if let Some(obj) = scheme.as_object_mut() {
        if let Some(inputs) = io.get("inputs").cloned() {
            obj.insert("inputs".into(), inputs);
        }
        if let Some(outputs) = io.get("outputs").cloned() {
            obj.insert("outputs".into(), outputs);
        }
    }
    scheme
}

/// 旧种子只有提示词 + 参考图/视频/音频，缺首尾帧与源视频
pub fn saved_io_is_legacy_multimodal_only(saved: &Value) -> bool {
    let Some(inputs) = saved.get("inputs").and_then(|v| v.as_array()) else {
        return true;
    };
    if inputs.is_empty() {
        return true;
    }
    let mut has_ref = false;
    let mut has_frames = false;
    for p in inputs {
        if p.get("enabled") == Some(&Value::Bool(false)) {
            continue;
        }
        match p.get("key").and_then(|v| v.as_str()).unwrap_or("") {
            "reference_images" | "reference_videos" | "reference_audio" => has_ref = true,
            "start_frame" | "end_frame" => has_frames = true,
            _ => {}
        }
    }
    has_ref && !has_frames
}

/// 覆写列表中的 seedance2.0；若不存在则追加
pub fn seed_seedance2_scheme(schemes: &mut Vec<Value>) {
    let next = seedance2_scheme();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(SEEDANCE2_SCHEME_ID))
    {
        schemes[pos] = next;
    } else {
        schemes.push(next);
    }
}

/// Seedance：去掉反向提示词；编辑/延长改为全能参考子能力开关
pub fn sanitize_seedance2_saved_io(saved: &mut Value) {
    if let Some(inputs) = saved.get_mut("inputs").and_then(|v| v.as_array_mut()) {
        inputs.retain(|p| {
            let key = p.get("key").and_then(|v| v.as_str());
            let bind_key = p.get("bind_key").and_then(|v| v.as_str());
            key != Some("negative_prompt") && bind_key != Some("negative_prompt")
        });
    }
    ensure_edit_extend_feature_toggles(saved);
}

fn port_key(p: &Value) -> &str {
    p.get("key").and_then(|v| v.as_str()).unwrap_or("")
}

fn feature_edit_port() -> Value {
    json!({
        "key": "edit_video",
        "label": "编辑视频",
        "enabled": true,
        "modality": "video",
        "handle_prefix": "Edit Video",
        "bind_key": "feature:edit_video",
        "max": 0
    })
}

fn feature_extend_port() -> Value {
    json!({
        "key": "extend_video",
        "label": "延长视频",
        "enabled": true,
        "modality": "video",
        "handle_prefix": "Extend Video",
        "bind_key": "feature:extend_video",
        "max": 0
    })
}

fn coerce_feature_toggle(port: &mut Value, bind_key: &str) {
    let enabled = port.get("enabled").cloned().unwrap_or(json!(true));
    if let Some(obj) = port.as_object_mut() {
        obj.insert("bind_key".into(), json!(bind_key));
        obj.insert("max".into(), json!(0));
        obj.insert("enabled".into(), enabled);
        obj.remove("accepts");
        obj.remove("accept_asset_kinds");
        obj.remove("expandable");
        obj.remove("required");
    }
}

fn ensure_edit_extend_feature_toggles(saved: &mut Value) {
    let Some(inputs) = saved.get_mut("inputs").and_then(|v| v.as_array_mut()) else {
        return;
    };
    let mut has_edit = false;
    let mut has_extend = false;
    for p in inputs.iter_mut() {
        match port_key(p) {
            "edit_video" => {
                has_edit = true;
                coerce_feature_toggle(p, "feature:edit_video");
            }
            "extend_video" => {
                has_extend = true;
                coerce_feature_toggle(p, "feature:extend_video");
            }
            _ => {}
        }
    }
    if !has_edit {
        inputs.push(feature_edit_port());
    }
    if !has_extend {
        inputs.push(feature_extend_port());
    }
}
