/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：GPT-Image-2 图片生成独立创作方案种子。
//! 仅 playground_2026 注入。比例 / 尺寸 / 分辨率走图片专用参数；方案参数只保留变体数量与画质。

use serde_json::{json, Value};

pub const GPT_IMAGE_2_SCHEME_ID: &str = "gpt-image-2";

const GPT_IMAGE_2_DESCRIPTION: &str = "GPT-Image-2 旗舰图片生成方案：支持文生图与图生图；图片专用参数提供比例、尺寸与分辨率；另可配置变体数量（1-4）与画质（常用画质/高画质/高精细画质）。";

const REDUNDANT_PARAM_KEYS: &[&str] = &["ratio", "prompt_enhancer", "resolution"];

/// GPT-Image-2 工作流 IO：文生图 + 图生图（参考图编辑）
pub fn gpt_image_2_scheme_io() -> Value {
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
                "max": 16,
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
        ]
    })
}

/// OpenAI GPT Image 2 / 2.5：`size=auto` 或 WxH；边长 ≤3840 且为 16 倍数，长短边 ≤3:1，总像素 [655360, 8294400]。
const GPT_RATIO_OPTIONS: &[&str] = &[
    "auto", "9:16", "3:4", "2:3", "1:1", "4:3", "3:2", "16:9", "21:9",
];
const GPT_RESO_OPTIONS: &[&str] = &["1K", "2K", "4K"];
const GPT_MAX_EDGE: u32 = 3840;
const GPT_MIN_PIXELS: u64 = 655_360;
const GPT_MAX_PIXELS: u64 = 8_294_400;

fn gpt_size_map_1k() -> Value {
    json!({
        "1:1": "1024x1024",
        "4:3": "1024x768",
        "3:4": "768x1024",
        "16:9": "1280x720",
        "9:16": "720x1280",
        "3:2": "1536x1024",
        "2:3": "1024x1536",
        "21:9": "1344x576"
    })
}

fn gpt_size_map_2k() -> Value {
    json!({
        "1:1": "2048x2048",
        "4:3": "2048x1536",
        "3:4": "1536x2048",
        "16:9": "2048x1152",
        "9:16": "1152x2048",
        "3:2": "2048x1360",
        "2:3": "1360x2048",
        "21:9": "2016x864"
    })
}

fn gpt_size_map_4k() -> Value {
    json!({
        "1:1": "2880x2880",
        "4:3": "3264x2448",
        "3:4": "2448x3264",
        "16:9": "3840x2160",
        "9:16": "2160x3840",
        "3:2": "3504x2336",
        "2:3": "2336x3504",
        "21:9": "3840x1648"
    })
}

fn gpt_image_2_image_special_params() -> Value {
    json!({
        "enabled": true,
        "smart_size": "auto",
        "aspect_ratio": {
            "enabled": true,
            "key": "ratio",
            "in_request": false,
            "options": GPT_RATIO_OPTIONS,
            "default": "auto"
        },
        "resolution": {
            "enabled": true,
            "key": "resolution",
            "in_request": false,
            "options": GPT_RESO_OPTIONS,
            "default": "1K"
        },
        "image_size": {
            "enabled": true,
            "key": "size",
            "in_request": true,
            "size_map": {
                "1K": gpt_size_map_1k(),
                "2K": gpt_size_map_2k(),
                "4K": gpt_size_map_4k()
            },
            "custom": {
                "enabled": true,
                "min_pixels": GPT_MIN_PIXELS,
                "max_pixels": GPT_MAX_PIXELS,
                "min_aspect": "1:3",
                "max_aspect": "3:1",
                "step": 16
            }
        }
    })
}

fn parse_wxh(raw: &str) -> Option<(u32, u32)> {
    let s = raw.trim().to_ascii_lowercase();
    let (a, b) = s.split_once('x')?;
    let w: u32 = a.trim().parse().ok()?;
    let h: u32 = b.trim().parse().ok()?;
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h))
}

fn gpt_size_ok(raw: &str) -> bool {
    let Some((w, h)) = parse_wxh(raw) else {
        return false;
    };
    if w % 16 != 0 || h % 16 != 0 || w > GPT_MAX_EDGE || h > GPT_MAX_EDGE {
        return false;
    }
    let pixels = u64::from(w) * u64::from(h);
    if pixels < GPT_MIN_PIXELS || pixels > GPT_MAX_PIXELS {
        return false;
    }
    let long = w.max(h);
    let short = w.min(h);
    short > 0 && u64::from(long) <= u64::from(short) * 3
}

fn str_list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn insert_unique(opts: &mut Vec<String>, val: &str, at: usize) -> bool {
    if opts.iter().any(|x| x.eq_ignore_ascii_case(val)) {
        return false;
    }
    opts.insert(at.min(opts.len()), val.to_string());
    true
}

fn ensure_section_object(parent: &mut Value, key: &str) -> bool {
    if parent.get(key).map(|v| v.is_object()).unwrap_or(false) {
        return false;
    }
    parent[key] = json!({});
    true
}

fn merge_gpt_size_maps(isp: &mut Value) -> bool {
    let official = json!({
        "1K": gpt_size_map_1k(),
        "2K": gpt_size_map_2k(),
        "4K": gpt_size_map_4k()
    });
    let Some(official_obj) = official.as_object() else {
        return false;
    };
    let mut changed = ensure_section_object(isp, "image_size");
    let existing = isp["image_size"]
        .get("size_map")
        .cloned()
        .unwrap_or(json!({}));
    let mut out = if existing.is_object() {
        existing
    } else {
        changed = true;
        json!({})
    };
    for (tier, ratios) in official_obj {
        let Some(official_ratios) = ratios.as_object() else {
            continue;
        };
        if !out.get(tier).map(|v| v.is_object()).unwrap_or(false) {
            out[tier] = json!({});
            changed = true;
        }
        for (ratio, official_size) in official_ratios {
            let cur = out[tier]
                .get(ratio)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let official_s = official_size.as_str().unwrap_or("");
            if cur.is_empty() || !gpt_size_ok(&cur) {
                if cur != official_s {
                    out[tier][ratio] = json!(official_s);
                    changed = true;
                }
            }
        }
    }
    if changed {
        isp["image_size"]["size_map"] = out;
    }
    changed
}

/// 强制开启并按官方规则补齐 auto / 4K / 尺寸表；已有合法自定义格子保留。
fn ensure_gpt_image_2_image_special(scheme: &mut Value) -> bool {
    if image_special_missing(scheme) {
        scheme["image_special_params"] = gpt_image_2_image_special_params();
        return true;
    }
    let official = gpt_image_2_image_special_params();
    let mut changed = false;
    {
        let isp = &mut scheme["image_special_params"];
        if isp.get("enabled") != Some(&json!(true)) {
            isp["enabled"] = json!(true);
            changed = true;
        }
        if isp.get("smart_size").and_then(|v| v.as_str()) != Some("auto") {
            isp["smart_size"] = json!("auto");
            changed = true;
        }
        changed |= ensure_section_object(isp, "aspect_ratio");
        changed |= ensure_section_object(isp, "resolution");
        changed |= ensure_section_object(isp, "image_size");
        if isp["aspect_ratio"].get("enabled") == Some(&json!(false)) {
            isp["aspect_ratio"]["enabled"] = json!(true);
            changed = true;
        }
        if isp["resolution"].get("enabled") == Some(&json!(false)) {
            isp["resolution"]["enabled"] = json!(true);
            changed = true;
        }
        if isp["image_size"].get("enabled") == Some(&json!(false)) {
            isp["image_size"]["enabled"] = json!(true);
            changed = true;
        }
        if isp["aspect_ratio"]
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        {
            isp["aspect_ratio"]["key"] = official["aspect_ratio"]["key"].clone();
            changed = true;
        }
        if isp["resolution"]
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        {
            isp["resolution"]["key"] = official["resolution"]["key"].clone();
            changed = true;
        }
        if isp["image_size"]
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        {
            isp["image_size"]["key"] = official["image_size"]["key"].clone();
            changed = true;
        }
        if isp["image_size"].get("in_request").is_none() {
            isp["image_size"]["in_request"] = json!(true);
            changed = true;
        }
        if isp["aspect_ratio"].get("in_request") != Some(&json!(false)) {
            isp["aspect_ratio"]["in_request"] = json!(false);
            changed = true;
        }
        if isp["resolution"].get("in_request") != Some(&json!(false)) {
            isp["resolution"]["in_request"] = json!(false);
            changed = true;
        }

        let mut ratios = str_list(&isp["aspect_ratio"]["options"]);
        let added_auto;
        if ratios.is_empty() {
            ratios = GPT_RATIO_OPTIONS.iter().map(|s| (*s).to_string()).collect();
            added_auto = true;
            changed = true;
        } else {
            added_auto = insert_unique(&mut ratios, "auto", 0);
            changed |= added_auto;
            if let Some(pos) = opts_index(&ratios, "3:4") {
                changed |= insert_unique(&mut ratios, "2:3", pos + 1);
            } else {
                changed |= insert_unique(&mut ratios, "2:3", 1);
            }
        }
        if isp["aspect_ratio"]["options"] != json!(ratios) {
            isp["aspect_ratio"]["options"] = json!(ratios.clone());
            changed = true;
        }
        let def_ratio = isp["aspect_ratio"]
            .get("default")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if def_ratio.is_empty()
            || !ratios.iter().any(|x| x == &def_ratio)
            || (added_auto && def_ratio == "1:1")
        {
            isp["aspect_ratio"]["default"] = json!("auto");
            changed = true;
        }

        let mut resos = str_list(&isp["resolution"]["options"]);
        if resos.is_empty() {
            resos = GPT_RESO_OPTIONS.iter().map(|s| (*s).to_string()).collect();
            changed = true;
        } else {
            let reso_len = resos.len();
            changed |= insert_unique(&mut resos, "4K", reso_len);
        }
        if isp["resolution"]["options"] != json!(resos) {
            isp["resolution"]["options"] = json!(resos.clone());
            changed = true;
        }
        let def_reso = isp["resolution"]
            .get("default")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if def_reso.is_empty() || !resos.iter().any(|x| x.eq_ignore_ascii_case(&def_reso)) {
            isp["resolution"]["default"] = json!("1K");
            changed = true;
        }

        let custom = isp["image_size"]
            .get("custom")
            .cloned()
            .unwrap_or(json!({}));
        let max_px = custom
            .get("max_pixels")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let min_px = custom
            .get("min_pixels")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let step = custom.get("step").and_then(|v| v.as_i64()).unwrap_or(0);
        let aspect_wide = custom
            .get("max_aspect")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !custom.is_object()
            || max_px != GPT_MAX_PIXELS
            || min_px != GPT_MIN_PIXELS
            || step != 16
            || aspect_wide != "3:1"
        {
            let enabled = custom
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            isp["image_size"]["custom"] = json!({
                "enabled": enabled,
                "min_pixels": GPT_MIN_PIXELS,
                "max_pixels": GPT_MAX_PIXELS,
                "min_aspect": "1:3",
                "max_aspect": "3:1",
                "step": 16
            });
            changed = true;
        }
    }
    changed |= merge_gpt_size_maps(&mut scheme["image_special_params"]);
    changed
}

fn opts_index(opts: &[String], val: &str) -> Option<usize> {
    opts.iter().position(|x| x == val)
}

fn image_special_missing(s: &Value) -> bool {
    match s.get("image_special_params") {
        None => true,
        Some(v) if v.is_null() => true,
        Some(v) => v.as_object().map(|o| o.is_empty()).unwrap_or(true),
    }
}

/// 官方 Images edits 最多 16 张参考图。旧种子 max=1 升级；管理员已改成 2–16 的保留。
fn ensure_gpt_image_2_io(scheme: &mut Value) -> bool {
    let official = gpt_image_2_scheme_io();
    let mut changed = false;
    if !scheme.get("inputs").map(|v| v.is_array()).unwrap_or(false) {
        scheme["inputs"] = official["inputs"].clone();
        changed = true;
    }
    if !scheme.get("outputs").map(|v| v.is_array()).unwrap_or(false) {
        scheme["outputs"] = official["outputs"].clone();
        changed = true;
    }
    if let Some(inputs) = scheme.get_mut("inputs").and_then(|v| v.as_array_mut()) {
        if let Some(port) = inputs
            .iter_mut()
            .find(|p| p.get("key").and_then(|v| v.as_str()) == Some("reference_images"))
        {
            let max = port.get("max").and_then(|v| v.as_u64()).unwrap_or(0);
            if max == 1 {
                port["max"] = json!(16);
                changed = true;
            }
            if port.get("expandable") != Some(&json!(true)) {
                port["expandable"] = json!(true);
                changed = true;
            }
        }
    }
    let legacy = scheme
        .get("max_reference_images")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if legacy == 1 {
        scheme["max_reference_images"] = json!(16);
        changed = true;
    }
    changed
}

fn strip_redundant_params(params: &mut Vec<Value>) -> bool {
    let before = params.len();
    params.retain(|p| {
        !REDUNDANT_PARAM_KEYS.contains(&p.get("key").and_then(|v| v.as_str()).unwrap_or(""))
    });
    params.len() != before
}

pub fn gpt_image_2_scheme() -> Value {
    let io = gpt_image_2_scheme_io();
    let mut scheme = json!({
        "id": GPT_IMAGE_2_SCHEME_ID,
        "name": "GPT-Image-2 图片生成方案",
        "type": "image",
        "is_system": true,
        "description": GPT_IMAGE_2_DESCRIPTION,
        "max_reference_images": 16,
        "image_special_params": gpt_image_2_image_special_params(),
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
            },
            {
                "key": "quality",
                "label": "画质",
                "type": "radio",
                "data_type": "string",
                "options": ["Low", "Medium", "High"],
                "option_labels": {
                    "Low": "常用画质",
                    "Medium": "高画质",
                    "High": "高精细画质"
                },
                "default": "Low",
                "display_style": "radio",
                "group": "更多选项",
                "hint": "设置生成画面的精细度质量 (常用画质/高画质/高精细画质)"
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

/// 确保列表中有 gpt-image-2 方案；若已存在则去掉与图片专用参数重复的项，并同步变体/画质文案
pub fn seed_gpt_image_2_scheme(schemes: &mut Vec<Value>) -> bool {
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(GPT_IMAGE_2_SCHEME_ID))
    {
        let mut changed = false;
        if let Some(params) = schemes[pos]
            .get_mut("params")
            .and_then(|v| v.as_array_mut())
        {
            if strip_redundant_params(params) {
                changed = true;
            }
            for p in params.iter_mut() {
                let key = p
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                match key.as_str() {
                    "variations" => {
                        if p.get("label").and_then(|v| v.as_str()) != Some("变体数量") {
                            p["label"] = json!("变体数量");
                            changed = true;
                        }
                        if p.get("group").and_then(|v| v.as_str()) != Some("更多选项") {
                            p["group"] = json!("更多选项");
                            changed = true;
                        }
                        if p.get("hint").and_then(|v| v.as_str())
                            != Some("单次生成的图片变体数量 (1-4)")
                        {
                            p["hint"] = json!("单次生成的图片变体数量 (1-4)");
                            changed = true;
                        }
                        if p.get("default").and_then(|v| v.as_i64()) == Some(4)
                            || p.get("default").is_none()
                        {
                            p["default"] = json!(1);
                            changed = true;
                        }
                    }
                    "quality" => {
                        if p.get("label").and_then(|v| v.as_str()) != Some("画质") {
                            p["label"] = json!("画质");
                            changed = true;
                        }
                        if p.get("group").and_then(|v| v.as_str()) != Some("更多选项") {
                            p["group"] = json!("更多选项");
                            changed = true;
                        }
                        let hint_zh = "设置生成画面的精细度质量 (常用画质/高画质/高精细画质)";
                        if p.get("hint").and_then(|v| v.as_str()) != Some(hint_zh) {
                            p["hint"] = json!(hint_zh);
                            changed = true;
                        }
                        if p.get("option_labels").is_none() {
                            p["option_labels"] = json!({
                                "Low": "常用画质",
                                "Medium": "高画质",
                                "High": "高精细画质"
                            });
                            changed = true;
                        }
                    }
                    _ => {
                        if p.get("group").and_then(|v| v.as_str()) == Some("More options") {
                            p["group"] = json!("更多选项");
                            changed = true;
                        }
                    }
                }
            }
        }
        if ensure_gpt_image_2_image_special(&mut schemes[pos]) {
            changed = true;
        }
        if ensure_gpt_image_2_io(&mut schemes[pos]) {
            changed = true;
        }
        if schemes[pos].get("description").and_then(|v| v.as_str()) != Some(GPT_IMAGE_2_DESCRIPTION)
        {
            let desc = schemes[pos]
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if desc.contains("提示词增强") || desc.contains("画面比例网格") {
                schemes[pos]["description"] = json!(GPT_IMAGE_2_DESCRIPTION);
                changed = true;
            }
        }
        return changed;
    }

    let scheme = gpt_image_2_scheme();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some("openai_image"))
    {
        schemes.insert(pos + 1, scheme);
    } else {
        schemes.push(scheme);
    }
    true
}
