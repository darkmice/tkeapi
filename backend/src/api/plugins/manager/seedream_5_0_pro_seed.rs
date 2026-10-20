/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：Seedream 5.0 Pro 图片生成方案种子。
//! 仅 playground_2026 注入，不进入旧创作中心默认方案列表。
//! 参数对齐 docs/DoubaoSeedream5pro.md（doubao-seedream-5-0-pro-260628）。
//! 与 lite 方案 `seedream_5_0` 并存：Pro 不支持组图 / 流式 / 联网搜索；支持图层拆分与交互编辑。

use serde_json::{json, Value};

pub const SEEDREAM_5_0_PRO_SCHEME_ID: &str = "seedream_5_0_pro";
pub const SEEDREAM_5_0_LITE_SCHEME_ID: &str = "seedream_5_0";

/// Pro 最多 10 张参考图；图层拆分时前端会再收成 1 张。
pub fn seedream_5_0_pro_scheme_io() -> Value {
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
            }
        ],
        "outputs": [
            {
                "key": "images",
                "label": "图片列表",
                "enabled": true,
                "modality": "image",
                "handle_prefix": "Image",
                "result_key": "image_url",
                "max": 1
            }
        ]
    })
}

pub fn seedream_5_0_pro_scheme() -> Value {
    let io = seedream_5_0_pro_scheme_io();
    let mut scheme = json!({
        "id": SEEDREAM_5_0_PRO_SCHEME_ID,
        "name": "Seedream 5.0 Pro 图片生成方案",
        "type": "image",
        "is_system": true,
        "description": "Doubao Seedream 5.0 Pro（doubao-seedream-5-0-pro-260628）：文生图、单/多图生图（最多 10 张参考图）；支持交互编辑与图层拆分。不支持文生组图、流式输出、联网搜索。分辨率 1K/1.5K/2K。",
        "max_reference_images": 10,
        "params": [
            {
                "key": "watermark",
                "label": "水印",
                "type": "switch",
                "data_type": "boolean",
                "default": false
            },
            {
                "key": "output_format",
                "label": "输出格式",
                "type": "radio",
                "data_type": "string",
                "options": ["png", "jpeg"],
                "option_labels": {
                    "png": "PNG",
                    "jpeg": "JPEG"
                },
                "default": "jpeg"
            },
            {
                "key": "background",
                "label": "背景",
                "type": "radio",
                "data_type": "string",
                "options": ["opaque", "transparent"],
                "option_labels": {
                    "opaque": "不透明",
                    "transparent": "透明"
                },
                "default": "opaque"
            },
            {
                "key": "layer_decomposition",
                "label": "自动拆分图层",
                "type": "switch",
                "data_type": "boolean",
                "default": false,
                "enabled": true
            },
            {
                "key": "optimize_prompt_mode",
                "label": "提示词优化",
                "type": "radio",
                "data_type": "string",
                "options": ["standard", "fast"],
                "option_labels": {
                    "standard": "标准",
                    "fast": "快速"
                },
                "default": "standard"
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
        obj.insert(
            "image_special_params".into(),
            seedream_5_0_pro_image_special_params(),
        );
    }
    scheme
}

/// 若列表中尚无 Pro 方案则插入到 lite 方案之后（无 lite 则追加）
pub fn seed_seedream_5_0_pro_scheme(schemes: &mut Vec<Value>) -> bool {
    let exists = schemes
        .iter()
        .any(|s| s.get("id").and_then(|v| v.as_str()) == Some(SEEDREAM_5_0_PRO_SCHEME_ID));
    if exists {
        return false;
    }
    let scheme = seedream_5_0_pro_scheme();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(SEEDREAM_5_0_LITE_SCHEME_ID))
    {
        schemes.insert(pos + 1, scheme);
    } else {
        schemes.push(scheme);
    }
    true
}

/// 是否为 Seedream 5.0 Pro（不匹配 lite）
pub fn is_seedream_5_0_pro_model(model_id: &str, name: &str) -> bool {
    let hay = format!("{model_id} {name}")
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace(' ', "-")
        .replace('.', "-");
    hay.contains("seedream-5-0-pro") || hay.contains("seedream-5-pro")
}

fn is_image_count_key(key: &str) -> bool {
    matches!(
        key,
        "n" | "batch_size" | "num_images" | "image_count" | "count" | "quantity" | "generate_count"
    )
}

/// 创作中心2026：Pro 模型 params 补上图层拆分开关（方案未绑 Pro 时也能看见）
pub fn ensure_layer_decomposition_param(params: Value) -> Value {
    let Some(arr) = params.as_array() else {
        return params;
    };
    if arr
        .iter()
        .any(|p| p.get("key").and_then(|v| v.as_str()) == Some("layer_decomposition"))
    {
        return params;
    }
    let mut next = arr.clone();
    next.push(json!({
        "key": "layer_decomposition",
        "label": "自动拆分图层",
        "type": "switch",
        "data_type": "boolean",
        "default": false,
        "enabled": true
    }));
    json!(next)
}

/// Pro 不支持组图：生成数量锁定为 1
pub fn lock_image_count_to_one(params: Value) -> Value {
    let Some(arr) = params.as_array() else {
        return params;
    };
    json!(arr
        .iter()
        .map(|p| {
            let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
            if !is_image_count_key(key) {
                return p.clone();
            }
            let mut next = p.clone();
            if let Some(obj) = next.as_object_mut() {
                obj.insert("options".into(), json!([1]));
                obj.insert("default".into(), json!(1));
            }
            next
        })
        .collect::<Vec<_>>())
}

const RATIO_OPTIONS: [&str; 9] = [
    "auto", "1:1", "4:3", "3:4", "16:9", "9:16", "3:2", "2:3", "21:9",
];

/// 火山教程「图像输出尺寸」方式 1：lite / 4.5 / 4.0 共用 2K 表（与 Pro 2K 像素不同）。
fn size_map_2k_standard() -> Value {
    json!({
        "1:1": "2048x2048",
        "4:3": "2304x1728",
        "3:4": "1728x2304",
        "16:9": "2848x1600",
        "9:16": "1600x2848",
        "3:2": "2496x1664",
        "2:3": "1664x2496",
        "21:9": "3136x1344"
    })
}

fn size_map_3k_lite() -> Value {
    json!({
        "1:1": "3072x3072",
        "4:3": "3456x2592",
        "3:4": "2592x3456",
        "16:9": "4096x2304",
        "9:16": "2304x4096",
        "3:2": "3744x2496",
        "2:3": "2496x3744",
        "21:9": "4704x2016"
    })
}

fn size_map_4k_standard() -> Value {
    json!({
        "1:1": "4096x4096",
        "4:3": "4704x3520",
        "3:4": "3520x4704",
        "16:9": "5504x3040",
        "9:16": "3040x5504",
        "3:2": "4992x3328",
        "2:3": "3328x4992",
        "21:9": "6240x2656"
    })
}

fn image_special_block(
    reso_options: &[&str],
    reso_default: &str,
    size_map: Value,
    min_pixels: i64,
    max_pixels: i64,
) -> Value {
    json!({
        "enabled": true,
        "aspect_ratio": {
            "enabled": true,
            "key": "ratio",
            "in_request": false,
            "options": RATIO_OPTIONS,
            "default": "auto"
        },
        "resolution": {
            "enabled": true,
            "key": "resolution",
            "in_request": false,
            "options": reso_options,
            "default": reso_default
        },
        "image_size": {
            "enabled": true,
            "key": "size",
            "in_request": true,
            "size_map": size_map,
            "custom": {
                "enabled": true,
                "min_pixels": min_pixels,
                "max_pixels": max_pixels,
                "min_aspect": "1:16",
                "max_aspect": "16:1",
                "step": 1
            }
        }
    })
}

/// Seedream 5.0 Pro：1K/1.5K/2K，默认 1.5K；指定宽高 [921600, 4624220]。
pub fn seedream_5_0_pro_image_special_params() -> Value {
    image_special_block(
        &["1K", "1.5K", "2K"],
        "1.5K",
        json!({
            "1K": {
                "1:1": "1024x1024",
                "4:3": "1152x864",
                "3:4": "864x1152",
                "16:9": "1424x800",
                "9:16": "800x1424",
                "3:2": "1248x832",
                "2:3": "832x1248",
                "21:9": "1568x672"
            },
            "1.5K": {
                "1:1": "1536x1536",
                "4:3": "1792x1344",
                "3:4": "1344x1792",
                "16:9": "2048x1152",
                "9:16": "1152x2048",
                "3:2": "1872x1248",
                "2:3": "1248x1872",
                "21:9": "2352x1008"
            },
            "2K": {
                "1:1": "2048x2048",
                "4:3": "2368x1776",
                "3:4": "1776x2368",
                "16:9": "2816x1584",
                "9:16": "1584x2816",
                "3:2": "2496x1664",
                "2:3": "1664x2496",
                "21:9": "3136x1344"
            }
        }),
        921600,
        4624220,
    )
}

/// Seedream 5.0 lite：2K/3K/4K，默认 2K；指定宽高 [3686400, 16777216]。
pub fn seedream_5_0_lite_image_special_params() -> Value {
    image_special_block(
        &["2K", "3K", "4K"],
        "2K",
        json!({
            "2K": size_map_2k_standard(),
            "3K": size_map_3k_lite(),
            "4K": size_map_4k_standard()
        }),
        3686400,
        16777216,
    )
}

/// Seedream 4.5：2K/4K，默认 2K；指定宽高与 lite 相同。
pub fn seedream_4_5_image_special_params() -> Value {
    image_special_block(
        &["2K", "4K"],
        "2K",
        json!({
            "2K": size_map_2k_standard(),
            "4K": size_map_4k_standard()
        }),
        3686400,
        16777216,
    )
}

/// Seedream 4.0：1K/2K/4K，默认 1K；1K 的 16:9 为 1312x736（不是 Pro 的 1424x800）。
pub fn seedream_4_0_image_special_params() -> Value {
    image_special_block(
        &["1K", "2K", "4K"],
        "1K",
        json!({
            "1K": {
                "1:1": "1024x1024",
                "4:3": "1152x864",
                "3:4": "864x1152",
                "16:9": "1312x736",
                "9:16": "736x1312",
                "3:2": "1248x832",
                "2:3": "832x1248",
                "21:9": "1568x672"
            },
            "2K": size_map_2k_standard(),
            "4K": size_map_4k_standard()
        }),
        921600,
        16777216,
    )
}

pub fn official_image_special_params_for(scheme_id: &str) -> Option<Value> {
    match scheme_id {
        SEEDREAM_5_0_PRO_SCHEME_ID => Some(seedream_5_0_pro_image_special_params()),
        SEEDREAM_5_0_LITE_SCHEME_ID => Some(seedream_5_0_lite_image_special_params()),
        "seedream_4_5" => Some(seedream_4_5_image_special_params()),
        "seedream_4_0" => Some(seedream_4_0_image_special_params()),
        _ => None,
    }
}

fn image_special_missing(s: &Value) -> bool {
    match s.get("image_special_params") {
        None => true,
        Some(v) if v.is_null() => true,
        Some(v) => v.as_object().map(|o| o.is_empty()).unwrap_or(true),
    }
}

/// 仅 playground_2026：四个 Seedream 图片方案在缺少专用参数时写入官方表，已有配置不覆盖。
pub fn seed_seedream_official_image_special(schemes: &mut [Value]) -> bool {
    let mut changed = false;
    for s in schemes.iter_mut() {
        let Some(id) = s.get("id").and_then(|v| v.as_str()).map(str::to_string) else {
            continue;
        };
        let Some(official) = official_image_special_params_for(&id) else {
            continue;
        };
        if image_special_missing(s) {
            s["image_special_params"] = official;
            changed = true;
        } else if id == SEEDREAM_5_0_PRO_SCHEME_ID {
            if let Some(isp) = s.get_mut("image_special_params") {
                if let Some(res) = isp.get_mut("resolution") {
                    if res.get("default").and_then(|v| v.as_str()) == Some("2K") {
                        res["default"] = json!("1.5K");
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

/// 仅四个 Seedream 图片方案：去掉与图片专用参数重复的 params.size。
pub fn strip_seedream_scheme_size_params(schemes: &mut [Value]) -> bool {
    let mut changed = false;
    for s in schemes.iter_mut() {
        let id = s
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if official_image_special_params_for(&id).is_none() {
            continue;
        }
        let Some(arr) = s.get_mut("params").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        let before = arr.len();
        arr.retain(|p| p.get("key").and_then(|v| v.as_str()) != Some("size"));
        if arr.len() != before {
            changed = true;
        }
    }
    changed
}

/// 仅 Seedream 5.0 Pro 图片方案：确保选项控件类型默认对齐为 radio 并补齐默认中文选项映射
pub fn seed_seedream_output_format_radio(schemes: &mut [Value]) -> bool {
    let mut changed = false;
    for s in schemes.iter_mut() {
        let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id != SEEDREAM_5_0_PRO_SCHEME_ID {
            continue;
        }
        let Some(arr) = s.get_mut("params").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for p in arr.iter_mut() {
            let key = p
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match key.as_str() {
                "output_format" => {
                    if p.get("type").and_then(|v| v.as_str()) != Some("radio") {
                        p["type"] = json!("radio");
                        changed = true;
                    }
                    if p.get("option_labels").is_none() {
                        p["option_labels"] = json!({
                            "png": "PNG",
                            "jpeg": "JPEG"
                        });
                        changed = true;
                    }
                }
                "background" => {
                    if p.get("type").and_then(|v| v.as_str()) != Some("radio") {
                        p["type"] = json!("radio");
                        changed = true;
                    }
                    if p.get("option_labels").is_none() {
                        p["option_labels"] = json!({
                            "opaque": "不透明",
                            "transparent": "透明"
                        });
                        changed = true;
                    }
                }
                "optimize_prompt_mode" => {
                    if p.get("type").and_then(|v| v.as_str()) != Some("radio") {
                        p["type"] = json!("radio");
                        changed = true;
                    }
                    if p.get("option_labels").is_none() {
                        p["option_labels"] = json!({
                            "standard": "标准",
                            "fast": "快速"
                        });
                        changed = true;
                    }
                }
                _ => {}
            }
            if matches!(
                key.as_str(),
                "output_format" | "background" | "layer_decomposition" | "optimize_prompt_mode"
            ) {
                if let Some(obj) = p.as_object_mut() {
                    if obj.remove("hint").is_some() {
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}
