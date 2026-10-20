/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：MiniMax-H3 视频生成方案种子。
//! 仅 playground_2026 注入，不进入旧创作中心默认方案列表。
//! 参数对齐 https://platform.minimax.cn/docs/api-reference/video-generation-v2-create

use serde_json::{json, Value};

pub const MINIMAX_H3_SCHEME_ID: &str = "minimax-h3";

/// MiniMax H3 工作流 IO：文生 + 首尾帧 + 多模态参考（图/视频/音频）
pub fn minimax_h3_video_scheme_io() -> Value {
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

pub fn minimax_h3_scheme() -> Value {
    let io = minimax_h3_video_scheme_io();
    let mut scheme = json!({
        "id": MINIMAX_H3_SCHEME_ID,
        "name": "MiniMax H3 视频生成方案",
        "type": "video",
        "is_system": true,
        "description": "MiniMax Hailuo-03 视频生成。MiniMax-H3：文生 / 图生（首尾帧）/ 多模态参考（图≤9、视频≤3、音频≤3），分辨率 768P/2K，时长 4–15 秒。MiniMax-H3-Max：仅文生与图生（首尾帧），不支持多模态参考，分辨率 480P/768P，时长 5–15 秒。IO 配置可切换版本。首尾帧与参考素材互斥。",
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
                "hint": "MiniMax-H3：4–15 秒；MiniMax-H3-Max：5–15 秒"
            },
            {
                "key": "ratio",
                "label": "画面比例",
                "type": "radio",
                "data_type": "string",
                "options": ["21:9", "16:9", "4:3", "1:1", "3:4", "9:16", "adaptive"],
                "default": "16:9",
                "quick": true,
                "hint": "文生视频必填且不能为 adaptive；图生视频由输入图决定（adaptive，传入其他值会被忽略）；参考生可选"
            },
            {
                "key": "resolution",
                "label": "输出分辨率",
                "type": "select",
                "data_type": "string",
                "options": ["768P", "2K"],
                "default": "2K",
                "quick": true,
                "hint": "MiniMax-H3：768P / 2K；MiniMax-H3-Max：480P / 768P"
            },
            {
                "key": "watermark",
                "label": "水印",
                "type": "switch",
                "data_type": "boolean",
                "default": false,
                "hint": "对应官方 aigc_watermark"
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

/// 已有方案：补齐多模态口；H3（含 2K）时长下限从旧的 5 秒改为官方 4 秒
pub fn sanitize_minimax_h3_saved_io(saved: &mut Value) {
    ensure_omni_ports(saved);
    bump_h3_duration_min(saved);
    if let Some(obj) = saved.as_object_mut() {
        obj.insert(
            "description".into(),
            json!("MiniMax Hailuo-03 视频生成。MiniMax-H3：文生 / 图生（首尾帧）/ 多模态参考（图≤9、视频≤3、音频≤3），分辨率 768P/2K，时长 4–15 秒。MiniMax-H3-Max：仅文生与图生（首尾帧），不支持多模态参考，分辨率 480P/768P，时长 5–15 秒。IO 配置可切换版本。首尾帧与参考素材互斥。"),
        );
    }
}

fn port_key(p: &Value) -> &str {
    p.get("key").and_then(|v| v.as_str()).unwrap_or("")
}

fn ensure_omni_ports(saved: &mut Value) {
    let seed_inputs = minimax_h3_video_scheme_io()
        .get("inputs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let Some(inputs) = saved.get_mut("inputs").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for port in seed_inputs {
        let key = port_key(&port).to_string();
        if key.is_empty() || inputs.iter().any(|p| port_key(p) == key) {
            continue;
        }
        inputs.push(port);
    }
}

fn resolution_looks_like_h3(saved: &Value) -> bool {
    saved
        .get("params")
        .and_then(|v| v.as_array())
        .and_then(|params| {
            params
                .iter()
                .find(|p| p.get("key").and_then(|k| k.as_str()) == Some("resolution"))
        })
        .and_then(|p| p.get("options").and_then(|v| v.as_array()))
        .is_some_and(|opts| opts.iter().any(|o| o.as_str() == Some("2K")))
}

fn bump_h3_duration_min(saved: &mut Value) {
    if !resolution_looks_like_h3(saved) {
        return;
    }
    let Some(params) = saved.get_mut("params").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for p in params {
        if p.get("key").and_then(|v| v.as_str()) != Some("duration") {
            continue;
        }
        let min = p.get("min").and_then(|v| v.as_i64());
        if min == Some(5) {
            if let Some(obj) = p.as_object_mut() {
                obj.insert("min".into(), json!(4));
            }
        }
    }
}

/// 若列表中尚无 MiniMax H3 方案则追加；已有则补齐口与 H3 时长下限
pub fn seed_minimax_h3_scheme(schemes: &mut Vec<Value>) -> bool {
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(MINIMAX_H3_SCHEME_ID))
    {
        sanitize_minimax_h3_saved_io(&mut schemes[pos]);
        return true;
    }
    schemes.push(minimax_h3_scheme());
    true
}
