/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：将内置 `dashscope_video` 对齐万相 3.0 官方请求字段。
//! 仅 playground_2026 覆写，不改旧创作中心默认方案。
//! 参数/素材 type 对齐 docs/wan3.md（DashScope video-synthesis）。

use serde_json::{json, Value};

pub const DASHSCOPE_VIDEO_SCHEME_ID: &str = "dashscope_video";
pub const WAN3_SCHEME_ID: &str = "wan3.0";

fn port_key(p: &Value) -> &str {
    p.get("key").and_then(|v| v.as_str()).unwrap_or("")
}

fn port_max(p: &Value) -> Option<i64> {
    p.get("max").and_then(|v| v.as_i64())
}

pub fn is_wan3_family_scheme_id(id: &str) -> bool {
    id == WAN3_SCHEME_ID || id == DASHSCOPE_VIDEO_SCHEME_ID
}

/// 万相 3.0 `input.media` 口：prompt + 图生视频（首帧）/ 尾帧 + 参考图/视频/音频；编辑/延长为全能参考子能力开关
pub fn dashscope_video_wan3_scheme_io() -> Value {
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
                "max": 10,
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
                "max": 5,
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
                "max": 5,
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

pub fn dashscope_video_wan3_scheme() -> Value {
    let io = dashscope_video_wan3_scheme_io();
    let mut scheme = json!({
        "id": DASHSCOPE_VIDEO_SCHEME_ID,
        "name": "阿里云 (DashScope) 视频生成方案",
        "type": "video",
        "is_system": true,
        "description": "阿里云百炼万相 3.0（wan3.0-video）All-in-One：文生/图生视频（首帧）/尾帧/全模态参考（图≤10、视频≤5、音频≤5，可任意组合）。编辑与延长是全能参考子能力，默认开启。parameters 对齐官方 resolution/ratio/duration/prompt_extend/audio。首尾帧与参考素材互斥。",
        "max_reference_images": 10,
        "params": [
            {
                "key": "duration",
                "label": "视频时长",
                "type": "slider",
                "data_type": "integer",
                "min": -1,
                "max": 30,
                "step": 1,
                "default": 5,
                "unit": "秒",
                "hint": "-1 为智能时长；有视频输入时输入+输出总时长≤30秒"
            },
            {
                "key": "ratio",
                "label": "画面比例",
                "type": "radio",
                "data_type": "string",
                "options": ["adaptive", "16:9", "4:3", "1:1", "3:4", "9:16"],
                "default": "adaptive",
                "quick": true,
                "hint": "有参考素材时建议 adaptive"
            },
            {
                "key": "resolution",
                "label": "输出分辨率",
                "type": "select",
                "data_type": "string",
                "options": ["480P", "720P", "1080P"],
                "default": "1080P",
                "quick": true
            },
            {
                "key": "prompt_extend",
                "label": "提示词扩写",
                "type": "switch",
                "data_type": "boolean",
                "default": false
            },
            {
                "key": "audio",
                "label": "生成音频",
                "type": "switch",
                "data_type": "boolean",
                "default": true,
                "hint": "有声视频（台词/BGM/音效）；设为关闭对应 parameters.audio=false"
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

/// 旧 dashscope_video 默认 IO 只有 prompt / negative_prompt，应被万相 3.0 口替换
pub fn saved_io_is_legacy_text_only(saved: &Value) -> bool {
    match saved.get("inputs").and_then(|v| v.as_array()) {
        None => true,
        Some(inputs) if inputs.is_empty() => true,
        Some(inputs) => inputs.iter().all(|p| {
            matches!(
                p.get("key").and_then(|v| v.as_str()),
                Some("prompt") | Some("negative_prompt")
            )
        }),
    }
}

/// 覆写列表中的 dashscope_video；若不存在则追加
pub fn seed_dashscope_video_wan3(schemes: &mut Vec<Value>) {
    let next = dashscope_video_wan3_scheme();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(DASHSCOPE_VIDEO_SCHEME_ID))
    {
        schemes[pos] = next;
    } else {
        schemes.push(next);
    }
}

fn io_has_port(saved: &Value, key: &str) -> bool {
    saved
        .get("inputs")
        .and_then(|v| v.as_array())
        .is_some_and(|inputs| inputs.iter().any(|p| port_key(p) == key))
}

/// 缺参考图/视频/音频口：旧 multimodal 上限或纯文生，需换成万相 3.0 全模态 IO
pub fn saved_io_needs_wan3_omni_seed(saved: &Value) -> bool {
    if saved_io_is_legacy_text_only(saved) {
        return true;
    }
    !io_has_port(saved, "reference_images")
        || !io_has_port(saved, "reference_videos")
        || !io_has_port(saved, "reference_audio")
        || !io_has_port(saved, "start_frame")
        || !io_has_port(saved, "end_frame")
}

fn strip_negative_prompt_ports(saved: &mut Value) {
    if let Some(inputs) = saved.get_mut("inputs").and_then(|v| v.as_array_mut()) {
        inputs.retain(|p| port_key(p) != "negative_prompt");
    }
}

fn bump_wan3_reference_max(saved: &mut Value) {
    let Some(inputs) = saved.get_mut("inputs").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for p in inputs {
        let Some(obj) = p.as_object_mut() else {
            continue;
        };
        let key = obj.get("key").and_then(|v| v.as_str()).unwrap_or("");
        let max = obj.get("max").and_then(|v| v.as_i64());
        match key {
            "reference_images" if matches!(max, Some(7 | 9)) => {
                obj.insert("max".into(), json!(10));
            }
            "reference_videos" | "reference_audio" if matches!(max, Some(3)) => {
                obj.insert("max".into(), json!(5));
            }
            _ => {}
        }
    }
    let img_max = saved
        .get("inputs")
        .and_then(|v| v.as_array())
        .and_then(|inputs| {
            inputs.iter().find_map(|p| {
                (port_key(p) == "reference_images")
                    .then(|| port_max(p))
                    .flatten()
            })
        });
    if matches!(img_max, Some(10)) {
        if let Some(obj) = saved.as_object_mut() {
            obj.insert("max_reference_images".into(), json!(10));
        }
    }
}

/// 万相 3.0 / DashScope 视频：去掉反向提示词，参考口上限对齐官方（图10/视5/音5）；编辑/延长改为全能参考子能力开关
pub fn sanitize_wan3_saved_io(saved: &mut Value) {
    strip_negative_prompt_ports(saved);
    bump_wan3_reference_max(saved);
    ensure_edit_extend_feature_toggles(saved);
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

/// 创作中心2026：给 `wan3.0` 铺上与官方全模态参考一致的 IO，保留方案参数
pub fn seed_wan3_scheme(schemes: &mut Vec<Value>) {
    let io = dashscope_video_wan3_scheme_io();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(WAN3_SCHEME_ID))
    {
        if let Some(obj) = schemes[pos].as_object_mut() {
            if let Some(inputs) = io.get("inputs").cloned() {
                obj.insert("inputs".into(), inputs);
            }
            if let Some(outputs) = io.get("outputs").cloned() {
                obj.insert("outputs".into(), outputs);
            }
            obj.insert("max_reference_images".into(), json!(10));
            obj.insert(
                "description".into(),
                json!("阿里云百炼万相 3.0（wan3.0-video）全模态参考：文生/图生（首帧）/首尾帧/参考图≤10、参考视频≤5、参考音频≤5（可任意组合）。编辑与延长是全能参考子能力，默认开启。首尾帧与参考素材互斥。"),
            );
        }
        return;
    }
    let mut next = dashscope_video_wan3_scheme();
    if let Some(obj) = next.as_object_mut() {
        obj.insert("id".into(), json!(WAN3_SCHEME_ID));
        obj.insert("name".into(), json!("万相 3.0 视频方案"));
    }
    schemes.push(next);
}
