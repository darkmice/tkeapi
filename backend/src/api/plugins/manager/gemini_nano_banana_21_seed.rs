/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：Gemini Nano Banana 2.1 图片生成方案。
//! 仅 playground_2026 注入。上游收的是档位 `1K`/`2K`/`4K` 和比例，不收像素尺寸。

use serde_json::{json, Value};

pub const GEMINI_NANO_BANANA_21_SCHEME_ID: &str = "gemini-nano-banana-2.1";

const DESCRIPTION: &str = "Gemini Nano Banana 2.1 图片生成方案：支持文生图与参考图编辑。请求传比例，以及分辨率档位 1K/2K/4K，不传像素尺寸。另可配置变体数量（1-4）。";

/// Nano Banana 2 参考图上限。
const MAX_REFERENCE_IMAGES: i64 = 14;

const RATIO_OPTIONS: &[&str] = &["1:1", "3:4", "4:3", "2:3", "3:2", "9:16", "16:9", "21:9"];
const RESO_OPTIONS: &[&str] = &["1K", "2K", "4K"];

fn size_map_for(tier: &str) -> Value {
    let mut map = serde_json::Map::new();
    for ratio in RATIO_OPTIONS {
        map.insert((*ratio).to_string(), json!(tier));
    }
    Value::Object(map)
}

fn image_special_params() -> Value {
    json!({
        "enabled": true,
        "aspect_ratio": {
            "enabled": true,
            "key": "ratio",
            "in_request": true,
            "options": RATIO_OPTIONS,
            "default": "1:1"
        },
        "resolution": {
            "enabled": true,
            "key": "resolution",
            "in_request": false,
            "options": RESO_OPTIONS,
            "default": "1K"
        },
        "image_size": {
            "enabled": true,
            "key": "size",
            "in_request": true,
            "size_map": {
                "1K": size_map_for("1K"),
                "2K": size_map_for("2K"),
                "4K": size_map_for("4K")
            },
            "custom": {
                "enabled": false
            }
        }
    })
}

pub fn gemini_nano_banana_21_scheme() -> Value {
    json!({
        "id": GEMINI_NANO_BANANA_21_SCHEME_ID,
        "name": "Gemini Nano Banana 2.1 图片生成方案",
        "type": "image",
        "is_system": true,
        "description": DESCRIPTION,
        "max_reference_images": MAX_REFERENCE_IMAGES,
        "image_special_params": image_special_params(),
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
                "max": MAX_REFERENCE_IMAGES,
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
                "max": 4
            }
        ],
        "params": [
            {
                "key": "variations",
                "label": "变体数量",
                "type": "slider",
                "data_type": "integer",
                "min": 1,
                "max": 4,
                "step": 1,
                "default": 1,
                "display_style": "pill",
                "group": "更多选项",
                "hint": "单次生成的图片变体数量 (1-4)"
            }
        ]
    })
}

fn size_map_uses_pixels(scheme: &Value) -> bool {
    let Some(map) = scheme.pointer("/image_special_params/image_size/size_map") else {
        return true;
    };
    let Some(obj) = map.as_object() else {
        return true;
    };
    obj.values().any(|tier| {
        tier.as_object().is_some_and(|ratios| {
            ratios
                .values()
                .any(|v| v.as_str().is_some_and(|s| s.to_ascii_lowercase().contains('x')))
        })
    })
}

/// 没有该方案时追加。已有方案若仍在传像素尺寸，改成档位加比例。
pub fn seed_gemini_nano_banana_21_scheme(schemes: &mut Vec<Value>) -> bool {
    if let Some(pos) = schemes.iter().position(|s| {
        s.get("id").and_then(|v| v.as_str()) == Some(GEMINI_NANO_BANANA_21_SCHEME_ID)
    }) {
        if !size_map_uses_pixels(&schemes[pos]) {
            return false;
        }
        if let Some(obj) = schemes[pos].as_object_mut() {
            obj.insert("image_special_params".into(), image_special_params());
            obj.insert("description".into(), json!(DESCRIPTION));
        }
        return true;
    }
    schemes.push(gemini_nano_banana_21_scheme());
    true
}

#[cfg(test)]
mod tests {
    use super::size_map_for;
    use crate::relay::usage_extractor::parse_pixel_resolution;

    #[test]
    fn size_maps_are_resolution_tiers() {
        for tier in ["1K", "2K", "4K"] {
            let obj = size_map_for(tier).as_object().cloned().expect(tier);
            for (ratio, size) in obj {
                let raw = size.as_str().unwrap_or("");
                assert_eq!(raw, tier, "{ratio}");
                assert_eq!(parse_pixel_resolution(raw).as_deref(), Some(tier.to_ascii_lowercase().as_str()));
            }
        }
    }
}
