/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心：schemes 种子/IO、配置与公开 API。插件中心 UI 相关改动放此文件，见 `mod.rs` 模块注释。

use crate::{
    auth,
    error::{AppError, AppResult},
    AppState,
};
use axum::{
    extract::{Extension, Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::{
    claude_fable_chat_seed, dashscope_video_seed, doubao_seed_evolving_seed, doubao_tts_2_seed,
    duration_slider_seed, gemini_nano_banana_21_seed, glm_deepseek_chat_seed,
    gpt_6_astra_chat_seed, gpt_image_2_seed, grok_4_6_chat_seed, is_plugin_enabled,
    load_plugin_configs, minimax_h3_seed, quick_bar_seed, seedance2_seed, seedream_5_0_pro_seed,
    upsert_config, viduq3_seed, viduq4_preview_seed,
};

/// 模型管理或模型广场改了页面排序后，创作中心2026配置里的 sort_order 写成同一个数。
/// 没有该模型的配置行时不新建。
pub async fn sync_playground_2026_page_sort(
    state: &AppState,
    model_id: i64,
    sort_order: i64,
) -> Result<(), sqlx::Error> {
    let config_key = format!("pg_model_id_{model_id}");
    let existing: Option<String> = sqlx::query_scalar(&state.db.format_query(
        "SELECT config_value FROM plugin_configs WHERE plugin_name = 'playground_2026' AND config_key = ?",
    ))
    .bind(&config_key)
    .fetch_optional(&state.db.pool)
    .await?;
    let Some(raw) = existing else {
        return Ok(());
    };
    let mut val: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    let Some(obj) = val.as_object_mut() else {
        return Ok(());
    };
    if obj.get("sort_order").and_then(|v| v.as_i64()) == Some(sort_order) {
        return Ok(());
    }
    obj.insert("sort_order".to_string(), json!(sort_order));
    upsert_config(state, "playground_2026", &config_key, &val.to_string()).await?;
    Ok(())
}

// ========== 体验中心配置 (Playground) ==========

/// 系统内置体验方案默认种子（仅当 DB 中无自定义方案时用作初始化）
fn get_default_schemes() -> Vec<serde_json::Value> {
    let mut schemes = vec![
        json!({
            "id": "seedance2.0",
            "name": "Seedance 2.0 方案",
            "type": "video",
            "is_system": true,
            "description": "支持多种分辨率和时长，适合高品质视频生成",
            "params": [
                {"key": "duration", "label": "视频时长", "type": "slider", "data_type": "integer", "min": 4, "max": 15, "step": 1, "default": 5, "unit": "秒"},
                {"key": "ratio", "label": "画面比例", "type": "radio", "data_type": "string", "options": ["21:9","16:9","4:3","1:1","3:4","9:16","adaptive"], "default": "16:9"},
                {"key": "resolution", "label": "输出分辨率", "type": "select", "data_type": "string", "options": ["480p","720p","1080p","4K"], "default": "480p"},
                {"key": "watermark", "label": "水印", "type": "switch", "data_type": "boolean", "default": false},
                {"key": "return_last_frame", "label": "返回最后一帧", "type": "switch", "data_type": "boolean", "default": false},
                {"key": "generate_audio", "label": "生成音频", "type": "switch", "data_type": "boolean", "default": false},
                {"key": "web_search", "label": "联网搜索", "type": "switch", "data_type": "boolean", "default": false}
            ]
        }),
        json!({
            "id": "seedream_5_0",
            "name": "Seedream 5.0 图片生成方案",
            "type": "image",
            "is_system": true,
            "description": "高质量 AI 图片生成，支持 doubao-seedream-5.0-lite 模型",
            "params": [
                {"key": "size", "label": "图片尺寸", "type": "radio", "options": ["2048x2048", "3072x3072", "1728x2304", "2592x3456", "2304x1728", "3456x2592", "2848x1600", "4096x2304", "1600x2848", "2304x4096", "2496x1664", "3744x2496", "1664x2496", "2496x3744", "3136x1344", "4704x2016", "2K", "3K", "4K"], "default": "2048x2048"},
                {"key": "n", "label": "生成数量", "type": "select", "options": [1,2,4], "default": 1, "unit": "张"},
                {"key": "watermark", "label": "水印", "type": "switch", "default": false},
                {"key": "output_format", "label": "输出格式", "type": "select", "options": ["png","jpeg"], "default": "jpeg"},
                {"key": "web_search", "label": "联网搜索", "type": "switch", "default": false}
            ]
        }),
        json!({
            "id": "seedream_4_5",
            "name": "Seedream 4.5 图片生成方案",
            "type": "image",
            "is_system": true,
            "description": "高质量 AI 图片生成，支持 doubao-seedream-4.5 模型",
            "params": [
                {"key": "size", "label": "图片尺寸", "type": "radio", "options": ["2048x2048", "4096x4096", "2304x1728", "1728x2304", "3520x4704", "2304x1728", "4704x3520", "2848x1600", "5504x3040", "1600x2848", "3040x5504", "2496x1664", "4992x3328", "1664x2496", "3328x4992", "3136x1344", "6240x2656", "2K", "4K"], "default": "2048x2048"},
                {"key": "n", "label": "生成数量", "type": "select", "options": [1,2,4], "default": 1, "unit": "张"},
                {"key": "watermark", "label": "水印", "type": "switch", "default": false}
            ]
        }),
        json!({
            "id": "seedream_4_0",
            "name": "Seedream 4.0 图片生成方案",
            "type": "image",
            "is_system": true,
            "description": "高质量 AI 图片生成，支持 doubao-seedream-4.0 模型",
            "params": [
                {"key": "size", "label": "图片尺寸", "type": "radio", "options": ["1024x1024", "2048x2048", "4096x4096", "864x1152", "1728x2304", "3520x4704", "1152x864", "2304x1728", "4704x3520", "1312x736", "2848x1600", "5504x3040", "736x1312", "1600x2848", "3040x5504", "832x1248", "1664x2496", "3328x4992", "1248x832", "2496x1664", "4992x3328", "1568x672", "3136x1344", "6240x2656", "1K", "2K", "4K"], "default": "1024x1024"},
                {"key": "n", "label": "生成数量", "type": "select", "options": [1,2,4], "default": 1, "unit": "张"},
                {"key": "watermark", "label": "水印", "type": "switch", "default": false}
            ]
        }),
        json!({
            "id": "seedance1.5pro",
            "name": "Seedance 1.5 Pro 方案",
            "type": "video",
            "is_system": true,
            "description": "支持文生视频和图生视频，可生成音频，适用于 doubao-seedance-1-0-pro 系列模型",
            "params": [
                {"key": "duration", "label": "视频时长", "type": "select", "options": [-1,5,10,12], "default": 5, "unit": "秒", "hint": "-1 表示由模型智能选择"},
                {"key": "ratio", "label": "画面比例", "type": "radio", "options": ["21:9","16:9","4:3","1:1","3:4","9:16","adaptive"], "default": "16:9"},
                {"key": "resolution", "label": "输出分辨率", "type": "select", "options": ["480p","720p","1080p"], "default": "720p"},
                {"key": "seed", "label": "随机种子", "type": "number", "default": -1, "min": -1, "max": 4294967295_i64, "hint": "-1 表示随机"},
                {"key": "generate_audio", "label": "生成音频", "type": "switch", "default": true},
                {"key": "camera_fixed", "label": "固定摄像头", "type": "switch", "default": false},
                {"key": "return_last_frame", "label": "返回尾帧图像", "type": "switch", "default": false},
                {"key": "watermark", "label": "水印", "type": "switch", "default": false}
            ]
        }),
        json!({
            "id": "openai_image",
            "name": "OpenAI 图片生成方案",
            "type": "image",
            "is_system": true,
            "description": "OpenAI gpt-image / DALL·E 兼容图片方案，支持文生图与图生图（画布参考图自动走 /v1/images/edits）；含尺寸、画质、背景、审核与输出格式等官方参数",
            "params": [
                {"key": "size", "label": "图片尺寸", "type": "radio", "options": ["auto", "1024x1024", "1536x1024", "1024x1536", "1792x1024", "1024x1792", "1536x864", "864x1536", "2560x1440", "1440x2560", "3840x2160", "2160x3840"], "default": "1024x1024"},
                {"key": "n", "label": "生成数量", "type": "radio", "options": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], "default": 1, "unit": "张"}
            ]
        }),
        json!({
            "id": "openai_video",
            "name": "OpenAI 视频生成方案",
            "type": "video",
            "is_system": true,
            "description": "OpenAI 兼容视频通道（含 Sora 等），支持文生视频与图生视频；画布参考图自动写入 images。字段对齐本站 /v1/video/generations 透传协议",
            "params": [
                {"key": "duration", "label": "视频时长", "type": "radio", "options": [4, 5, 8, 10, 12, 15], "default": 4, "unit": "秒", "hint": "本站 OpenAI 兼容协议使用 duration"},
                {"key": "ratio", "label": "画面比例", "type": "radio", "options": ["16:9", "9:16", "1:1"], "default": "16:9"},
                {"key": "resolution", "label": "输出分辨率", "type": "radio", "options": ["480p", "720p", "1080p"], "default": "480p", "hint": "兼容通道兜底字段，与计费对齐"},
            ],
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
                    "accepts": ["asset", "ai_image", "preview", "director"],
                    "accept_asset_kinds": ["image"],
                    "max": 1
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
        }),
        json!({
            "id": "gemini_flash_image",
            "name": "Gemini 3.1 Flash 图片生成方案",
            "type": "image",
            "is_system": true,
            "description": "Google Gemini 原生多模态图像生成，支持文生图和图生图(最多14张参考图)，最高 4K 分辨率，支持极端宽高比和 Google 搜索增强",
            "params": [
                {"key": "size", "label": "画面比例", "type": "radio", "options": ["1:1","3:2","2:3","4:3","3:4","16:9","9:16","5:4","4:5","21:9","1:4","4:1","1:8","8:1"], "default": "1:1"},
                {"key": "resolution", "label": "输出分辨率", "type": "select", "options": ["1k","2k","4k"], "default": "1k"},
                {"key": "n", "label": "生成数量", "type": "select", "options": [1,2,3,4], "default": 1, "unit": "张"},
                {"key": "google_search", "label": "搜索增强", "type": "switch", "default": false, "description": "搜索网络文字信息辅助生成图片"},
                {"key": "google_image_search", "label": "图片搜索增强", "type": "switch", "default": false, "description": "搜索参考图片辅助生成，适合需要视觉参考的场景"}
            ]
        }),
        json!({
            "id": "dashscope_image",
            "name": "阿里云 (DashScope) 图片生成方案",
            "type": "image",
            "is_system": true,
            "description": "阿里云通义万相系列原生/代理通道配置，支持多尺寸、多样式图像生成及提示词扩写功能",
            "params": [
                {"key": "size", "label": "图片尺寸", "type": "radio", "options": ["1280*1280", "1104*1472", "1472*1104", "960*1696", "1696*960", "2048*2048"], "default": "1280*1280"},
                {"key": "n", "label": "生成数量", "type": "select", "options": [1,2,3,4], "default": 1, "unit": "张"},
                {"key": "watermark", "label": "水印", "type": "switch", "default": false},
                {"key": "prompt_extend", "label": "提示词扩写", "type": "switch", "default": false, "description": "由模型自动丰富提示词细节以获得更好的生成效果"},
            ]
        }),
        json!({
            "id": "dashscope_video",
            "name": "阿里云 (DashScope) 视频生成方案",
            "type": "video",
            "is_system": true,
            "description": "阿里云通义万相视频生成配置，支持多种画面尺寸，适用于视频生成模型",
            "params": [
                {"key": "duration", "label": "视频时长", "type": "select", "options": [3, 5, 10, 15], "default": 5, "unit": "秒"},
                {"key": "ratio", "label": "画面尺寸", "type": "radio", "options": ["1:1", "16:9", "9:16", "4:3", "3:4"], "default": "1:1"},
                {"key": "resolution", "label": "输出分辨率", "type": "select", "options": ["720P","1080P"], "default": "720P"},
                {"key": "prompt_extend", "label": "提示词扩写", "type": "switch", "default": false},
                {"key": "watermark", "label": "水印", "type": "switch", "default": false}
            ]
        }),
        json!({
            "id": "wan3.0",
            "name": "万相 3.0 视频方案",
            "type": "video",
            "is_system": true,
            "description": "阿里云百炼万相 3.0（wan3.0-video）全能参考视频：文生/首尾帧/参考图视频音频。注意：首尾帧与参考素材互斥，不可同请求混用。",
            "params": [
                {"key": "duration", "label": "视频时长", "type": "select", "options": [-1, 2, 3, 5, 10, 15, 20, 25, 30], "default": 5, "unit": "秒", "data_type": "integer", "description": "-1 为智能时长"},
                {"key": "ratio", "label": "画面比例", "type": "radio", "options": ["adaptive", "16:9", "4:3", "1:1", "3:4", "9:16"], "default": "adaptive", "data_type": "string"},
                {"key": "resolution", "label": "输出分辨率", "type": "select", "options": ["480P", "720P", "1080P"], "default": "720P", "data_type": "string"},
                {"key": "audio", "label": "生成音频", "type": "switch", "default": true, "data_type": "boolean"},
                {"key": "watermark", "label": "水印", "type": "switch", "default": false, "data_type": "boolean"}
            ]
        }),
        json!({
            "id": "kling_image",
            "name": "可灵 (Kling) 图片生成方案",
            "type": "image",
            "is_system": true,
            "description": "快手可灵原生/代理图像生成配置，支持多比例及参考图",
            "params": [
                {"key": "ratio", "label": "画面比例", "type": "radio", "options": ["1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3", "21:9"], "default": "1:1"},
                {"key": "resolution", "label": "输出分辨率", "type": "select", "options": ["1k","2k","4k"], "default": "1k"},
                {"key": "n", "label": "生成数量", "type": "select", "options": [1,2,3,4,5,6,7,8,9], "default": 1, "unit": "张"},
            ]
        }),
        json!({
            "id": "kling_video",
            "name": "可灵 (Kling) 视频生成方案",
            "type": "video",
            "is_system": true,
            "description": "快手可灵原生/代理视频生成配置，支持文生视频与图生视频，包含多种模式、时长及音频控制",
            "params": [
                {"key": "duration", "label": "视频时长", "type": "select", "options": [3, 5, 10, 15], "default": 5, "unit": "秒"},
                {"key": "ratio", "label": "画面比例", "type": "radio", "options": ["16:9","9:16","1:1"], "default": "16:9", "hint": "文生视频时生效"},
                {"key": "mode", "label": "生成模式", "type": "select", "options": ["std", "pro", "4k"], "default": "std", "hint": "std:标准 pro:专业 4k:超高清 (不同模式计费可能不同)"},
                {"key": "sound", "label": "音频效果", "type": "select", "options": ["off", "on"], "default": "off", "description": "是否同时生成匹配画面的音频"},
            ]
        }),
        // ── 聊天对话方案 ──
        json!({
            "id": "chat_standard",
            "name": "标准对话方案",
            "type": "chat",
            "is_system": true,
            "description": "通用 AI 对话方案，支持温度、最大回复长度、流式输出等核心参数，适用于所有聊天类模型",
            "params": [
                {"key": "temperature", "label": "创意度", "type": "slider", "default": 0.7, "min": 0.0, "max": 2.0, "step": 0.1, "hint": "值越高回答越有创意，越低越精确"},
                {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [256, 512, 1024, 2048, 4096, 8192], "default": 4096, "unit": "tokens"},
                {"key": "stream", "label": "流式输出", "type": "switch", "default": true, "hint": "逐字输出回答，提升体验"},
                {"key": "top_p", "label": "核采样", "type": "slider", "default": 1.0, "min": 0.0, "max": 1.0, "step": 0.05}
            ]
        }),
        json!({
            "id": "chat_creative",
            "name": "创意写作方案",
            "type": "chat",
            "is_system": true,
            "description": "适用于创意写作、故事生成等场景，预设较高创意度和更长回复",
            "params": [
                {"key": "temperature", "label": "创意度", "type": "slider", "default": 1.2, "min": 0.0, "max": 2.0, "step": 0.1},
                {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [1024, 2048, 4096, 8192, 16384], "default": 8192, "unit": "tokens"},
                {"key": "stream", "label": "流式输出", "type": "switch", "default": true},
                {"key": "top_p", "label": "核采样", "type": "slider", "default": 0.95, "min": 0.0, "max": 1.0, "step": 0.05}
            ]
        }),
        json!({
            "id": "chat_precise",
            "name": "精准问答方案",
            "type": "chat",
            "is_system": true,
            "description": "适用于代码生成、数据分析、精确问答等场景，预设低创意度确保回答准确",
            "params": [
                {"key": "temperature", "label": "创意度", "type": "slider", "default": 0.1, "min": 0.0, "max": 2.0, "step": 0.1},
                {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [256, 512, 1024, 2048, 4096], "default": 2048, "unit": "tokens"},
                {"key": "stream", "label": "流式输出", "type": "switch", "default": true},
                {"key": "top_p", "label": "核采样", "type": "slider", "default": 1.0, "min": 0.0, "max": 1.0, "step": 0.05}
            ]
        }),
        json!({
            "id": "tencent_image",
            "name": "腾讯云 (AIGC) 图片生成方案",
            "type": "image",
            "is_system": true,
            "description": "基于腾讯云点播/媒体处理 AIGC 图像生成服务，支持多模型和自定义输出配置 (AigcImageOutputConfig)",
            "params": [
                {"key": "size", "label": "画面比例", "type": "radio", "options": ["1024x1024", "2048x2048", "2304x1728", "2496x1664", "2560x1440", "3024x1296", "4096x4096", "4693x3520", "4992x3328", "5404x3040", "6197x2656"], "default": "1024x1024"},
                {"key": "force_single", "label": "强制单张生成", "type": "switch", "default": false, "hint": "强制生成单张图片"},
            ]
        }),
        json!({
            "id": "tencent_video",
            "name": "腾讯云 (AIGC) 视频生成方案",
            "type": "video",
            "is_system": true,
            "description": "基于腾讯云点播/媒体处理 AIGC 视频生成服务，支持可灵、Vidu等模型和自定义输出配置 (AigcVideoOutputConfig)",
            "params": [
                {"key": "duration", "label": "视频时长", "type": "select", "options": [5, 10], "default": 5, "unit": "秒", "hint": "生成视频的目标时长"},
                {"key": "ratio", "label": "画面比例", "type": "radio", "options": ["1:1", "16:9", "9:16", "4:3", "3:4", "21:9"], "default": "1:1"},
                {"key": "seed", "label": "随机种子", "type": "number", "default": -1, "min": -1, "max": 4294967295_i64, "hint": "-1 表示随机"},
            ]
        }),
    ];
    for s in &mut schemes {
        ensure_scheme_max_reference_images(s);
        ensure_scheme_io(s, "");
    }
    schemes
}

/// 创作中心2026：视频方案 IO 去掉参考文件/网页链接/源视频，首帧改名为图生视频；OpenAI / Seedance 2.0 / 万相 3.0 视频方案去掉反向提示词
fn sanitize_pg2026_video_scheme_io(s: &mut serde_json::Value) {
    if s.get("type").and_then(|v| v.as_str()) != Some("video") {
        return;
    }
    let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let is_no_negative_prompt = id == "openai_video"
        || id == seedance2_seed::SEEDANCE2_SCHEME_ID
        || id == "seedance2"
        || dashscope_video_seed::is_wan3_family_scheme_id(id);
    let Some(inputs) = s.get_mut("inputs").and_then(|v| v.as_array_mut()) else {
        return;
    };
    inputs.retain(|p| {
        let key = p.get("key").and_then(|v| v.as_str());
        let bind_key = p.get("bind_key").and_then(|v| v.as_str());
        if is_no_negative_prompt
            && (key == Some("negative_prompt") || bind_key == Some("negative_prompt"))
        {
            return false;
        }
        !matches!(
            key,
            Some("reference_file") | Some("webpage_link") | Some("source_video")
        )
    });
    for p in inputs.iter_mut() {
        if p.get("key").and_then(|v| v.as_str()) == Some("start_frame") {
            if let Some(obj) = p.as_object_mut() {
                obj.insert("label".into(), json!("图生视频"));
            }
        }
    }
}

fn get_default_schemes_for(plugin_name: &str) -> Vec<serde_json::Value> {
    let mut schemes = get_default_schemes();
    if plugin_name == "playground_2026" {
        schemes.retain(|s| {
            !matches!(
                s.get("id").and_then(|v| v.as_str()),
                Some("seedance1.5pro" | "chat_standard" | "chat_creative" | "chat_precise")
            )
        });
        minimax_h3_seed::seed_minimax_h3_scheme(&mut schemes);
        dashscope_video_seed::seed_dashscope_video_wan3(&mut schemes);
        dashscope_video_seed::seed_wan3_scheme(&mut schemes);
        seedance2_seed::seed_seedance2_scheme(&mut schemes);
        quick_bar_seed::seed_imagine_quick_bar_schemes(&mut schemes);
        seedream_5_0_pro_seed::seed_seedream_5_0_pro_scheme(&mut schemes);
        seedream_5_0_pro_seed::seed_seedream_official_image_special(&mut schemes);
        seedream_5_0_pro_seed::strip_seedream_scheme_size_params(&mut schemes);
        seedream_5_0_pro_seed::seed_seedream_output_format_radio(&mut schemes);
        gpt_image_2_seed::seed_gpt_image_2_scheme(&mut schemes);
        gemini_nano_banana_21_seed::seed_gemini_nano_banana_21_scheme(&mut schemes);
        viduq4_preview_seed::seed_viduq4_preview_scheme(&mut schemes);
        viduq3_seed::seed_viduq3_schemes(&mut schemes);
        doubao_tts_2_seed::seed_doubao_tts_2_scheme(&mut schemes);
        doubao_seed_evolving_seed::seed_doubao_seed_evolving_scheme(&mut schemes);
        glm_deepseek_chat_seed::seed_glm_deepseek_chat_schemes(&mut schemes);
        claude_fable_chat_seed::seed_claude_fable_chat_scheme(&mut schemes);
        gpt_6_astra_chat_seed::seed_gpt_6_astra_chat_scheme(&mut schemes);
        grok_4_6_chat_seed::seed_grok_4_6_chat_scheme(&mut schemes);
        duration_slider_seed::seed_video_duration_sliders(&mut schemes);
        for s in &mut schemes {
            sanitize_pg2026_video_scheme_io(s);
            #[cfg(feature = "commercial_plugins")]
            crate::api::plugins::playground_2026::prompt_optimize::seed_scheme_prompt_optimize(s);
            #[cfg(feature = "commercial_plugins")]
            crate::api::plugins::playground_2026::voice_library::seed_scheme_voice_library(s);
        }
    }
    schemes
}

/// 创作中心2026：保持管理员已保存的参数顺序与自定义配置，种子新增 key 追加至末尾。
fn merge_saved_scheme_params(
    seed: &serde_json::Value,
    saved: &serde_json::Value,
) -> serde_json::Value {
    let Some(seed_arr) = seed.as_array() else {
        return saved.clone();
    };
    let saved_arr = saved.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
    if saved_arr.is_empty() {
        return seed.clone();
    }
    let mut used = HashSet::<String>::new();
    let mut out = Vec::with_capacity(saved_arr.len() + seed_arr.len());
    for p in saved_arr {
        let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if !key.is_empty() {
            used.insert(key.to_string());
        }
        let mut item = p.clone();
        if item.get("option_labels").is_none() {
            if let Some(seed_p) = seed_arr
                .iter()
                .find(|sp| sp.get("key").and_then(|v| v.as_str()) == Some(key))
            {
                if let Some(labels) = seed_p.get("option_labels") {
                    item["option_labels"] = labels.clone();
                }
            }
        }
        out.push(item);
    }
    for p in seed_arr {
        let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if !key.is_empty() && !used.contains(key) {
            used.insert(key.to_string());
            out.push(p.clone());
        }
    }
    serde_json::Value::Array(out)
}

/// 方案固定字段：最大参考图数量（与 name / description 同级）
/// 缺省：Gemini 图生 14；MiniMax H3 9；dashscope_video / Seedream 5.0 Pro 10；对话 0；其余图/视频 7
fn ensure_scheme_max_reference_images(s: &mut serde_json::Value) {
    if s.get("max_reference_images").is_some() {
        return;
    }
    let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let ty = s.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let n = if id == "gemini_flash_image" {
        14
    } else if id == minimax_h3_seed::MINIMAX_H3_SCHEME_ID {
        9
    } else if id == dashscope_video_seed::DASHSCOPE_VIDEO_SCHEME_ID
        || id == dashscope_video_seed::WAN3_SCHEME_ID
        || id == seedream_5_0_pro_seed::SEEDREAM_5_0_PRO_SCHEME_ID
    {
        10
    } else if ty == "chat" || ty == "audio" {
        0
    } else {
        7
    };
    if let Some(obj) = s.as_object_mut() {
        obj.insert("max_reference_images".into(), json!(n));
    }
}

/// 图片方案默认工作流 IO（对齐前端 schemeIo.ts defaultImageSchemeIo）
fn default_image_scheme_io() -> serde_json::Value {
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
                "handle_prefix": "Reference Image",
                "bind_key": "image_urls",
                "accepts": ["asset", "ai_image", "preview"],
                "accept_asset_kinds": ["image"],
                "max": 7,
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

/// OpenAI 兼容视频方案工作流 IO：文生视频 + 图生视频（首帧），无反向提示词
fn openai_video_scheme_io() -> serde_json::Value {
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
                "accepts": ["asset", "ai_image", "preview", "director"],
                "accept_asset_kinds": ["image"],
                "max": 1
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

/// 视频方案默认工作流 IO（对齐前端 defaultVideoSchemeIo）
fn default_video_scheme_io() -> serde_json::Value {
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
                "key": "negative_prompt",
                "label": "反向提示词",
                "enabled": true,
                "modality": "text",
                "handle_prefix": "Negative Prompt",
                "bind_key": "negative_prompt",
                "accepts": ["prompt"],
                "max": 1
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

/// 首尾帧视频方案 IO（对齐前端 video_start_end 模板）
fn start_end_video_scheme_io() -> serde_json::Value {
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
                "label": "首帧",
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

/// Seedance / 多模态视频方案 IO（对齐前端 multimodalVideoSchemeIo）
fn multimodal_video_scheme_io() -> serde_json::Value {
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

fn scheme_io_arrays_missing_or_empty(s: &serde_json::Value, field: &str) -> bool {
    match s.get(field) {
        None => true,
        Some(v) => v.as_array().map(|a| a.is_empty()).unwrap_or(true),
    }
}

fn apply_reference_images_max_from_scheme(io: &mut serde_json::Value, max_ref: &serde_json::Value) {
    let Some(inputs) = io.get_mut("inputs").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for port in inputs {
        if port.get("key").and_then(|v| v.as_str()) == Some("reference_images") {
            if let Some(obj) = port.as_object_mut() {
                obj.insert("max".into(), max_ref.clone());
            }
        }
    }
}

/// 若方案缺少 inputs/outputs（或为空数组），按类型填充默认工作流 IO
fn ensure_scheme_io(s: &mut serde_json::Value, plugin_name: &str) {
    let fill_inputs = scheme_io_arrays_missing_or_empty(s, "inputs");
    let fill_outputs = scheme_io_arrays_missing_or_empty(s, "outputs");
    if !fill_inputs && !fill_outputs {
        return;
    }

    let ty = s.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let mut io = match ty {
        "image" => default_image_scheme_io(),
        "video" => {
            if id == "seedance1.5pro" || id == "kling_video" {
                start_end_video_scheme_io()
            } else if id == minimax_h3_seed::MINIMAX_H3_SCHEME_ID {
                minimax_h3_seed::minimax_h3_video_scheme_io()
            } else if plugin_name == "playground_2026" && id == seedance2_seed::SEEDANCE2_SCHEME_ID
            {
                seedance2_seed::seedance2_video_scheme_io()
            } else if plugin_name == "playground_2026" && id == dashscope_video_seed::WAN3_SCHEME_ID
            {
                dashscope_video_seed::dashscope_video_wan3_scheme_io()
            } else if id == "wan3.0" || id == "seedance2.0" || id.contains("seedance") {
                multimodal_video_scheme_io()
            } else if id == "openai_video" {
                openai_video_scheme_io()
            } else {
                default_video_scheme_io()
            }
        }
        "audio" => doubao_tts_2_seed::doubao_tts_2_scheme_io(),
        "chat" if plugin_name == "playground_2026" => {
            if glm_deepseek_chat_seed::is_thinking_on_chat_scheme_id(id)
                || gpt_6_astra_chat_seed::is_gpt_6_astra_scheme_id(id)
            {
                doubao_seed_evolving_seed::doubao_chat_scheme_io()
            } else {
                doubao_seed_evolving_seed::default_chat_scheme_io()
            }
        }
        _ => return,
    };

    if matches!(ty, "image" | "video") {
        if let Some(max_ref) = s.get("max_reference_images") {
            apply_reference_images_max_from_scheme(&mut io, max_ref);
        }
    }

    if let Some(obj) = s.as_object_mut() {
        if fill_inputs {
            if let Some(inputs) = io.get("inputs").cloned() {
                obj.insert("inputs".into(), inputs);
            }
        }
        if fill_outputs {
            if let Some(outputs) = io.get("outputs").cloned() {
                obj.insert("outputs".into(), outputs);
            }
        }
    }
}

/// 从 DB 加载方案列表（优先使用 DB 存储，DB 为空时 fallback 到内置默认）
/// 同时自动合并新增的内置系统方案（is_system=true），确保新增种子方案无需手动操作即可出现
async fn load_schemes_from_db(state: &AppState, plugin_name: &str) -> Vec<serde_json::Value> {
    let configs = load_plugin_configs(state, plugin_name)
        .await
        .unwrap_or_default();
    if let Some(schemes_str) = configs.get("pg_schemes") {
        if let Ok(mut schemes) = serde_json::from_str::<Vec<serde_json::Value>>(schemes_str) {
            if !schemes.is_empty() {
                // 自动合并新增的内置方案，并清理已废弃的系统方案
                let defaults = get_default_schemes_for(plugin_name);
                let default_ids: std::collections::HashSet<String> = defaults
                    .iter()
                    .filter_map(|d| d.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect();

                // 移除在数据库中但已从代码默认配置中移除的系统方案
                schemes.retain(|s| {
                    let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if plugin_name == "playground_2026"
                        && matches!(
                            id,
                            "chat_standard" | "chat_creative" | "chat_precise"
                        )
                    {
                        return false;
                    }
                    if s.get("is_system")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        if !id.is_empty() {
                            return default_ids.contains(id);
                        }
                    }
                    true
                });

                let existing_ids: std::collections::HashSet<String> = schemes
                    .iter()
                    .filter_map(|s| s.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect();
                for d in defaults {
                    if let Some(id) = d.get("id").and_then(|v| v.as_str()) {
                        if d.get("is_system")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                        {
                            if !existing_ids.contains(id) {
                                schemes.push(d);
                            } else {
                                // 替换为最新的系统内置方案以同步参数配置；
                                // 保留管理员已配置的 IO 与 max_reference_images
                                if let Some(pos) = schemes
                                    .iter()
                                    .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(id))
                                {
                                    let saved = schemes[pos].clone();
                                    let saved_max = saved.get("max_reference_images").cloned();
                                    let saved_inputs = saved.get("inputs").cloned();
                                    let saved_outputs = saved.get("outputs").cloned();
                                    let saved_params = saved.get("params").cloned();
                                    let migrate_dashscope_io = plugin_name == "playground_2026"
                                        && id == dashscope_video_seed::DASHSCOPE_VIDEO_SCHEME_ID
                                        && dashscope_video_seed::saved_io_is_legacy_text_only(
                                            &saved,
                                        );
                                    let migrate_wan3_io = plugin_name == "playground_2026"
                                        && id == dashscope_video_seed::WAN3_SCHEME_ID
                                        && dashscope_video_seed::saved_io_needs_wan3_omni_seed(
                                            &saved,
                                        );
                                    let migrate_seedance2_io = plugin_name == "playground_2026"
                                        && id == seedance2_seed::SEEDANCE2_SCHEME_ID
                                        && seedance2_seed::saved_io_is_legacy_multimodal_only(
                                            &saved,
                                        );
                                    let migrate_io = migrate_dashscope_io
                                        || migrate_wan3_io
                                        || migrate_seedance2_io;
                                    let strip_doubao_thinking =
                                        glm_deepseek_chat_seed::is_thinking_on_chat_scheme_id(id);
                                    let mut merged = d;
                                    if plugin_name == "playground_2026" {
                                        // 库内字段（含 IO、image_special_params）为准；params 按 key 合并，种子只补新项
                                        if let (Some(saved_obj), Some(merged_obj)) =
                                            (saved.as_object(), merged.as_object_mut())
                                        {
                                            for (k, v) in saved_obj {
                                                if k == "params" {
                                                    continue;
                                                }
                                                if migrate_io
                                                    && matches!(
                                                        k.as_str(),
                                                        "inputs"
                                                            | "outputs"
                                                            | "max_reference_images"
                                                    )
                                                {
                                                    continue;
                                                }
                                                merged_obj.insert(k.clone(), v.clone());
                                            }
                                        }
                                        if let Some(saved_params) = saved_params.as_ref() {
                                            let seed_params = merged
                                                .get("params")
                                                .cloned()
                                                .unwrap_or_else(|| json!([]));
                                            merged["params"] = merge_saved_scheme_params(
                                                &seed_params,
                                                saved_params,
                                            );
                                            if let Some(merged_params) = merged.get_mut("params") {
                                                quick_bar_seed::overlay_saved_quick_flags(
                                                    saved_params,
                                                    merged_params,
                                                );
                                            }
                                        }
                                        if strip_doubao_thinking {
                                            doubao_seed_evolving_seed::strip_legacy_thinking_param(
                                                &mut merged,
                                            );
                                        }
                                    } else if let Some(obj) = merged.as_object_mut() {
                                        if let Some(v) = saved_max {
                                            obj.insert("max_reference_images".into(), v);
                                        }
                                        if let Some(v) = saved_inputs {
                                            if v.as_array().map(|a| !a.is_empty()).unwrap_or(false)
                                            {
                                                obj.insert("inputs".into(), v);
                                            }
                                        }
                                        if let Some(v) = saved_outputs {
                                            if v.as_array().map(|a| !a.is_empty()).unwrap_or(false)
                                            {
                                                obj.insert("outputs".into(), v);
                                            }
                                        }
                                    }
                                    ensure_scheme_max_reference_images(&mut merged);
                                    ensure_scheme_io(&mut merged, plugin_name);
                                    schemes[pos] = merged;
                                }
                            }
                        }
                    }
                }
                for s in &mut schemes {
                    ensure_scheme_max_reference_images(s);
                    ensure_scheme_io(s, plugin_name);
                    if plugin_name == "playground_2026" {
                        if s.get("id")
                            .and_then(|v| v.as_str())
                            .is_some_and(dashscope_video_seed::is_wan3_family_scheme_id)
                        {
                            dashscope_video_seed::sanitize_wan3_saved_io(s);
                        }
                        if s.get("id").and_then(|v| v.as_str())
                            == Some(seedance2_seed::SEEDANCE2_SCHEME_ID)
                        {
                            seedance2_seed::sanitize_seedance2_saved_io(s);
                        }
                        if s.get("id").and_then(|v| v.as_str())
                            == Some(minimax_h3_seed::MINIMAX_H3_SCHEME_ID)
                        {
                            minimax_h3_seed::sanitize_minimax_h3_saved_io(s);
                        }
                    }
                }
                if plugin_name == "playground_2026" {
                    duration_slider_seed::seed_video_duration_sliders(&mut schemes);
                    seedream_5_0_pro_seed::strip_seedream_scheme_size_params(&mut schemes);
                    seedream_5_0_pro_seed::seed_seedream_official_image_special(&mut schemes);
                    seedream_5_0_pro_seed::seed_seedream_output_format_radio(&mut schemes);
                    gpt_image_2_seed::seed_gpt_image_2_scheme(&mut schemes);
                    gemini_nano_banana_21_seed::seed_gemini_nano_banana_21_scheme(&mut schemes);
                    viduq4_preview_seed::seed_viduq4_preview_scheme(&mut schemes);
                    viduq3_seed::seed_viduq3_schemes(&mut schemes);
                    for s in &mut schemes {
                        sanitize_pg2026_video_scheme_io(s);
                        #[cfg(feature = "commercial_plugins")]
                        crate::api::plugins::playground_2026::prompt_optimize::seed_scheme_prompt_optimize(s);
                        #[cfg(feature = "commercial_plugins")]
                        crate::api::plugins::playground_2026::voice_library::seed_scheme_voice_library(s);
                    }
                }
                return schemes;
            }
        }
    }
    // DB 中没有或解析失败，返回内置默认
    get_default_schemes_for(plugin_name)
}

/// 每个模型的体验配置（启用状态 + 绑定方案）
#[derive(Deserialize)]
pub(crate) struct PlaygroundModelConfig {
    pub id: i64,
    pub enabled: bool,
    pub scheme_id: Option<String>,
    pub param_overrides: Option<serde_json::Value>,
    pub io_overrides: Option<serde_json::Value>,
    pub sort_order: Option<i64>,
    /// 仅 playground_2026：兼容旧客户端；优先使用 feature_attributes
    #[serde(default)]
    pub feature_keys: Option<Vec<String>>,
    /// 仅 playground_2026：站点模型二级功能属性（与模型管理同步）
    #[serde(default)]
    pub feature_attributes: Option<Vec<String>>,
    /// 仅 playground_2026：图片专用参数（模型级，覆盖方案）
    #[serde(default)]
    pub image_special_params: Option<serde_json::Value>,
    /// 仅 playground_2026：优化指令覆写（null=继承方案）
    #[serde(default)]
    pub prompt_optimize: Option<serde_json::Value>,
    /// 仅 playground_2026：音频音色库覆写（null=继承方案）
    #[serde(default)]
    pub voice_library: Option<serde_json::Value>,
    /// 仅 playground_2026 聊天：输入协议预设 ID
    #[serde(default)]
    pub input_protocol: Option<String>,
    /// 仅 playground_2026 聊天：思考预设 ID
    #[serde(default)]
    pub thinking_profile: Option<String>,
    /// 仅 playground_2026：同步模型广场展示状态
    #[serde(default)]
    pub mp_enabled: Option<bool>,
    /// 仅 playground_2026：同步模型广场等级可见性配置
    #[serde(
        default,
        deserialize_with = "super::marketplace_visibility::deserialize_opt_vec_i64_flexible"
    )]
    pub mp_level_ids: Option<Vec<i64>>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub(crate) struct AdvancedNodesConfig {
    pub enabled: bool,
    pub preview_enabled: bool,
    pub volc_enhance_enabled: bool,
    #[serde(default)]
    pub director_enabled: bool,
    pub prompt_enabled: bool,
    pub ai_video_enabled: bool,
    pub ai_image_enabled: bool,
    pub agent_enabled: bool,
    pub agent_mode_enabled: Option<bool>,
    pub agent_video_mode: Option<String>,
    pub agent_welcome_title: Option<String>,
    pub agent_welcome_desc: Option<String>,
    pub agent_preset_prompts: Option<serde_json::Value>,
    pub agent_system_prompt: Option<String>,
    pub agent_chat_models: Option<Vec<String>>,
    pub unified_limit_enabled: Option<bool>,
    pub unified_limit_value: Option<i64>,
    pub preview_limit: Option<i64>,
    pub prompt_limit: Option<i64>,
    pub ai_video_limit: Option<i64>,
    pub ai_image_limit: Option<i64>,
    pub agent_limit: Option<i64>,
    pub volc_enhance_limit: Option<i64>,
    pub instance_limit: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct PlaygroundConfigRequest {
    pub models: Vec<PlaygroundModelConfig>,
    pub default_model_mids: Option<serde_json::Value>, // {"chat": "mid1", "image": "mid2", "video": "mid3"}
    pub advanced_nodes: Option<AdvancedNodesConfig>,
}

fn pg2026_feature_catalog(types: &[crate::models::ModelType]) -> serde_json::Value {
    #[cfg(feature = "commercial_plugins")]
    {
        let video_attrs = types
            .iter()
            .find(|t| t.name == "视频")
            .map(|t| {
                crate::api::plugins::playground_2026::model_features::parse_attr_list(
                    t.default_features.as_deref(),
                )
            })
            .unwrap_or_default();
        crate::api::plugins::playground_2026::model_features::catalog_json_from_type_defaults(
            &video_attrs,
        )
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = types;
        json!({ "groups": [] })
    }
}

fn pg2026_parse_feature_keys(value: Option<&serde_json::Value>) -> Vec<String> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::model_features::parse_keys(value)
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = value;
        Vec::new()
    }
}

fn pg2026_sanitize_feature_keys(keys: &[String], type_name: &str) -> Vec<String> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::model_features::sanitize_keys(keys, type_name)
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = (keys, type_name);
        Vec::new()
    }
}

fn pg2026_parse_attr_list(raw: Option<&str>) -> Vec<String> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::model_features::parse_attr_list(raw)
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = raw;
        Vec::new()
    }
}

fn pg2026_sanitize_attr_list(attrs: &[String]) -> Vec<String> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::model_features::sanitize_attr_list(attrs)
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = attrs;
        Vec::new()
    }
}

fn pg2026_keys_from_attrs(attrs: &[String], type_name: &str) -> Vec<String> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::model_features::keys_from_attributes(attrs, type_name)
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = (attrs, type_name);
        Vec::new()
    }
}

fn pg2026_attrs_from_keys(keys: &[String]) -> Vec<String> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::model_features::attributes_from_keys(keys)
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = keys;
        Vec::new()
    }
}

fn pg2026_type_has_feature_attrs(type_name: &str) -> bool {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::model_features::kind_from_type_name(type_name)
            .is_some()
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = type_name;
        false
    }
}

fn pg2026_is_chat_type(type_name: &str) -> bool {
    type_name.contains("聊天") || type_name.contains("对话")
}

fn pg2026_sanitize_input_protocol(raw: Option<&str>) -> Option<String> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::chat_protocol::sanitize_input_protocol(raw)
            .map(|s| s.to_string())
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = raw;
        None
    }
}

fn pg2026_sanitize_thinking_profile(raw: Option<&str>) -> Option<String> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::chat_protocol::sanitize_thinking_profile(raw)
            .map(|s| s.to_string())
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = raw;
        None
    }
}

fn pg2026_chat_protocol_catalog() -> serde_json::Value {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::chat_protocol::catalog_json()
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        serde_json::json!({})
    }
}

/// 管理员：获取体验中心配置（返回全部模型 + 每个模型的启用/方案信息）
pub(crate) async fn get_playground_config(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<serde_json::Value>> {
    let role: String =
        sqlx::query_scalar(&state.db.format_query("SELECT role FROM users WHERE id = ?"))
            .bind(&claims.sub)
            .fetch_one(&state.db.pool)
            .await?;
    if role != "admin" {
        return Err(AppError::Forbidden("需要管理员权限".to_string()));
    }

    let configs = load_plugin_configs(&state, &name).await?;
    let mp_configs = load_plugin_configs(&state, "model_marketplace").await?;
    let mp_display_mode = mp_configs
        .get("mp_display_mode")
        .map(|s| s.as_str())
        .unwrap_or("blacklist");
    let mp_default_enabled = mp_display_mode == "blacklist";

    // 查出全部模型及其 type 信息
    let models: Vec<crate::models::Model> = sqlx::query_as(&state.db.format_query(
        "SELECT * FROM models WHERE is_listed = 1 ORDER BY sort_order DESC, id DESC",
    ))
    .fetch_all(&state.db.pool)
    .await?;

    let types: Vec<crate::models::ModelType> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM model_types ORDER BY sort_order DESC, id ASC"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    // 为每个模型附加启用和方案配置
    let mut model_list = Vec::new();
    for m in &models {
        let new_key = format!("pg_model_id_{}", m.id);
        let old_key = format!("pg_model_{}", m.mid);
        let model_conf: serde_json::Value = configs
            .get(&new_key)
            .or_else(|| configs.get(&old_key))
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(json!({"enabled": false, "scheme_id": null}));

        let mp_config_key = format!("mp_model_id_{}", m.id);
        let mp_model_conf: serde_json::Value = mp_configs
            .get(&mp_config_key)
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| json!({}));
        let mp_is_enabled =
            super::marketplace_visibility::mp_enabled(&mp_model_conf, mp_default_enabled);
        let mp_level_ids = super::marketplace_visibility::parse_mp_level_ids(&mp_model_conf);

        let type_row = m.type_id.and_then(|tid| types.iter().find(|t| t.id == tid));
        let type_name = type_row.map(|t| t.name.clone()).unwrap_or_default();
        let type_default_features = if name == "playground_2026" {
            json!(pg2026_parse_attr_list(
                type_row.and_then(|t| t.default_features.as_deref())
            ))
        } else {
            serde_json::Value::Null
        };

        let (pg_feature_keys, feature_attributes) = if name == "playground_2026" {
            let plugin_keys = pg2026_parse_feature_keys(model_conf.get("feature_keys"));
            let mut attrs = pg2026_parse_attr_list(m.feature_attributes.as_deref());
            if attrs.is_empty() {
                attrs = pg2026_attrs_from_keys(&plugin_keys);
            }
            let keys = {
                let mapped = pg2026_keys_from_attrs(&attrs, &type_name);
                if mapped.is_empty() {
                    pg2026_sanitize_feature_keys(&plugin_keys, &type_name)
                } else {
                    mapped
                }
            };
            (json!(keys), json!(attrs))
        } else {
            (serde_json::Value::Null, serde_json::Value::Null)
        };

        model_list.push(json!({
            "id": m.id,
            "mid": m.mid,
            "name": m.name,
            "model_id": m.model_id,
            "model_id_alias": m.model_id_alias,
            "remark": m.remark,
            "type_id": m.type_id,
            "type_name": type_name,
            "provider_id": m.provider_id,
            "api_provider_id": m.api_provider_id,
            "is_active": m.is_active,
            "global_discount": m.global_discount,
            "global_discount_enabled": m.global_discount_enabled,
            "mp_enabled": mp_is_enabled,
            "mp_level_ids": mp_level_ids,
            "mp_description": mp_model_conf.get("description").and_then(|v| v.as_str()).unwrap_or(""),
            "mp_description_en": mp_model_conf.get("description_en").and_then(|v| v.as_str()).unwrap_or(""),
            "pg_enabled": model_conf.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
            "pg_scheme_id": model_conf.get("scheme_id").and_then(|v| v.as_str()).unwrap_or(""),
            "pg_param_overrides": model_conf.get("param_overrides").cloned().unwrap_or(serde_json::Value::Null),
            "pg_io_overrides": model_conf.get("io_overrides").cloned().unwrap_or(serde_json::Value::Null),
            "pg_sort_order": m.sort_order as i64,
            "pg_feature_keys": pg_feature_keys,
            "feature_attributes": feature_attributes,
            "type_default_features": type_default_features,
            "pg_image_special_params": model_conf.get("image_special_params").cloned().unwrap_or(serde_json::Value::Null),
            "pg_prompt_optimize": model_conf.get("prompt_optimize").cloned().unwrap_or(serde_json::Value::Null),
            "pg_voice_library": model_conf.get("voice_library").cloned().unwrap_or(serde_json::Value::Null),
            "pg_input_protocol": if name == "playground_2026" && pg2026_is_chat_type(&type_name) {
                json!(model_conf.get("input_protocol").and_then(|v| v.as_str()).unwrap_or(""))
            } else {
                serde_json::Value::Null
            },
            "pg_thinking_profile": if name == "playground_2026" && pg2026_is_chat_type(&type_name) {
                json!(model_conf.get("thinking_profile").and_then(|v| v.as_str()).unwrap_or(""))
            } else {
                serde_json::Value::Null
            },
        }));
    }

    // 读取每个类型的默认模型
    let default_model_mids: serde_json::Value = configs
        .get("pg_default_model_mids")
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(json!({}));

    let adv_nodes_enabled = configs
        .get("pg_advanced_nodes_enabled")
        .map(|s| s == "true")
        .unwrap_or(false);
    let adv_node_preview = configs
        .get("pg_advanced_node_preview_enabled")
        .map(|s| s == "true")
        .unwrap_or(true);
    let adv_node_volc = configs
        .get("pg_advanced_node_volc_enhance_enabled")
        .map(|s| s == "true")
        .unwrap_or(false);
    let adv_node_prompt = configs
        .get("pg_advanced_node_prompt_enabled")
        .map(|s| s == "true")
        .unwrap_or(true);
    let adv_node_ai_video = configs
        .get("pg_advanced_node_ai_video_enabled")
        .map(|s| s == "true")
        .unwrap_or(true);
    let adv_node_ai_image = configs
        .get("pg_advanced_node_ai_image_enabled")
        .map(|s| s == "true")
        .unwrap_or(true);
    let adv_node_agent = if name == "playground_2026" {
        false
    } else {
        configs
            .get("pg_advanced_node_agent_enabled")
            .map(|s| s == "true")
            .unwrap_or(false)
    };
    let is_pg2026 = name == "playground_2026";
    let agent_mode_enabled = if is_pg2026 {
        false
    } else {
        configs
            .get("pg_agent_mode_enabled")
            .map(|s| s == "true")
            .unwrap_or(false)
    };
    let agent_video_mode = if is_pg2026 {
        "track".to_string()
    } else {
        configs
            .get("pg_agent_video_mode")
            .cloned()
            .unwrap_or_else(|| "track".to_string())
    };
    let agent_welcome_title = if is_pg2026 {
        None
    } else {
        configs.get("pg_agent_welcome_title").cloned()
    };
    let agent_welcome_desc = if is_pg2026 {
        None
    } else {
        configs.get("pg_agent_welcome_desc").cloned()
    };
    let agent_preset_prompts: Option<serde_json::Value> = if is_pg2026 {
        None
    } else {
        configs
            .get("pg_agent_preset_prompts")
            .and_then(|s| serde_json::from_str(s).ok())
    };
    let agent_system_prompt = if is_pg2026 {
        None
    } else {
        configs.get("pg_agent_system_prompt").cloned()
    };
    let agent_chat_models: Option<Vec<String>> = if is_pg2026 {
        None
    } else {
        configs
            .get("pg_agent_chat_models")
            .and_then(|s| serde_json::from_str(s).ok())
    };
    let volc_enhance_plugin_active = is_plugin_enabled(&state, "volcengine_enhance").await;

    let mut advanced_nodes = json!({
        "enabled": adv_nodes_enabled,
        "preview_enabled": adv_node_preview,
        "volc_enhance_enabled": adv_node_volc,
        "volc_enhance_plugin_active": volc_enhance_plugin_active,
        "prompt_enabled": adv_node_prompt,
        "ai_video_enabled": adv_node_ai_video,
        "ai_image_enabled": adv_node_ai_image,
        "agent_enabled": adv_node_agent,
    });
    if is_pg2026 {
        advanced_nodes["director_enabled"] = json!(configs
            .get("pg_advanced_node_director_enabled")
            .map(|s| s == "true")
            .unwrap_or(false));
    }
    if !is_pg2026 {
        advanced_nodes["agent_mode_enabled"] = json!(agent_mode_enabled);
        advanced_nodes["agent_video_mode"] = json!(agent_video_mode);
        advanced_nodes["agent_welcome_title"] = json!(agent_welcome_title);
        advanced_nodes["agent_welcome_desc"] = json!(agent_welcome_desc);
        advanced_nodes["agent_preset_prompts"] = json!(agent_preset_prompts);
        advanced_nodes["agent_system_prompt"] = json!(agent_system_prompt);
        advanced_nodes["agent_chat_models"] = json!(agent_chat_models);
    }

    let mut body = json!({
        "models": model_list,
        "schemes": load_schemes_from_db(&state, &name).await,
        "default_model_mids": default_model_mids,
        "advanced_nodes": advanced_nodes,
    });
    if name == "playground_2026" {
        body["feature_catalog"] = pg2026_feature_catalog(&types);
        body["chat_protocol"] = pg2026_chat_protocol_catalog();
        body["model_types"] = json!(types
            .iter()
            .map(|t| json!({
                "id": t.id,
                "name": t.name,
                "default_features": pg2026_parse_attr_list(t.default_features.as_deref()),
            }))
            .collect::<Vec<_>>());
        #[cfg(feature = "commercial_plugins")]
        {
            body["voice_catalogs"] =
                crate::api::plugins::playground_2026::voice_library::catalogs_json();
        }
    }
    Ok(Json(body))
}

/// 管理员：保存体验中心配置（按模型逐个保存启用状态和方案绑定）
pub(crate) async fn save_playground_config(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Extension(claims): Extension<auth::Claims>,
    Json(payload): Json<PlaygroundConfigRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let role: String =
        sqlx::query_scalar(&state.db.format_query("SELECT role FROM users WHERE id = ?"))
            .bind(&claims.sub)
            .fetch_one(&state.db.pool)
            .await?;
    if role != "admin" {
        return Err(AppError::Forbidden("需要管理员权限".to_string()));
    }

    // 保存每个类型的默认模型
    if let Some(ref mids) = payload.default_model_mids {
        upsert_config(&state, &name, "pg_default_model_mids", &mids.to_string()).await?;
    }

    if let Some(ref adv) = payload.advanced_nodes {
        upsert_config(
            &state,
            &name,
            "pg_advanced_nodes_enabled",
            &adv.enabled.to_string(),
        )
        .await?;
        upsert_config(
            &state,
            &name,
            "pg_advanced_node_preview_enabled",
            &adv.preview_enabled.to_string(),
        )
        .await?;
        upsert_config(
            &state,
            &name,
            "pg_advanced_node_volc_enhance_enabled",
            &adv.volc_enhance_enabled.to_string(),
        )
        .await?;
        if name == "playground_2026" {
            upsert_config(
                &state,
                &name,
                "pg_advanced_node_director_enabled",
                &adv.director_enabled.to_string(),
            )
            .await?;
        }
        upsert_config(
            &state,
            &name,
            "pg_advanced_node_prompt_enabled",
            &adv.prompt_enabled.to_string(),
        )
        .await?;
        upsert_config(
            &state,
            &name,
            "pg_advanced_node_ai_video_enabled",
            &adv.ai_video_enabled.to_string(),
        )
        .await?;
        upsert_config(
            &state,
            &name,
            "pg_advanced_node_ai_image_enabled",
            &adv.ai_image_enabled.to_string(),
        )
        .await?;
        let agent_enabled = if name == "playground_2026" {
            false
        } else {
            adv.agent_enabled
        };
        upsert_config(
            &state,
            &name,
            "pg_advanced_node_agent_enabled",
            &agent_enabled.to_string(),
        )
        .await?;
        if name != "playground_2026" {
            if let Some(ref val) = adv.agent_mode_enabled {
                upsert_config(&state, &name, "pg_agent_mode_enabled", &val.to_string()).await?;
            }
            if let Some(ref val) = adv.agent_video_mode {
                upsert_config(&state, &name, "pg_agent_video_mode", val).await?;
            }
            if let Some(ref val) = adv.agent_welcome_title {
                upsert_config(&state, &name, "pg_agent_welcome_title", val).await?;
            }
            if let Some(ref val) = adv.agent_welcome_desc {
                upsert_config(&state, &name, "pg_agent_welcome_desc", val).await?;
            }
            if let Some(ref val) = adv.agent_preset_prompts {
                upsert_config(&state, &name, "pg_agent_preset_prompts", &val.to_string()).await?;
            }
            if let Some(ref val) = adv.agent_system_prompt {
                upsert_config(&state, &name, "pg_agent_system_prompt", val).await?;
            }
            if let Some(ref val) = adv.agent_chat_models {
                upsert_config(
                    &state,
                    &name,
                    "pg_agent_chat_models",
                    &serde_json::to_string(val).unwrap_or_default(),
                )
                .await?;
            }
        }
    }

    let mut type_name_by_model_id: HashMap<i64, String> = HashMap::new();
    let mut existing_feature_keys: HashMap<i64, Vec<String>> = HashMap::new();
    let mut existing_image_special: HashMap<i64, serde_json::Value> = HashMap::new();
    let mut existing_prompt_optimize: HashMap<i64, serde_json::Value> = HashMap::new();
    let mut existing_voice_library: HashMap<i64, serde_json::Value> = HashMap::new();
    let mut existing_input_protocol: HashMap<i64, String> = HashMap::new();
    let mut existing_thinking_profile: HashMap<i64, String> = HashMap::new();
    if name == "playground_2026" {
        let models: Vec<crate::models::Model> = sqlx::query_as(
            &state
                .db
                .format_query("SELECT * FROM models WHERE is_listed = 1 ORDER BY id DESC"),
        )
        .fetch_all(&state.db.pool)
        .await?;
        let types: Vec<crate::models::ModelType> = sqlx::query_as(
            &state
                .db
                .format_query("SELECT * FROM model_types ORDER BY id ASC"),
        )
        .fetch_all(&state.db.pool)
        .await?;
        for m in &models {
            let type_name = m
                .type_id
                .and_then(|tid| types.iter().find(|t| t.id == tid))
                .map(|t| t.name.clone())
                .unwrap_or_default();
            type_name_by_model_id.insert(m.id, type_name);
        }
        if let Ok(cfgs) = load_plugin_configs(&state, &name).await {
            for (key, raw) in cfgs {
                let Some(id_str) = key.strip_prefix("pg_model_id_") else {
                    continue;
                };
                let Ok(id) = id_str.parse::<i64>() else {
                    continue;
                };
                let parsed: serde_json::Value =
                    serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
                existing_feature_keys
                    .insert(id, pg2026_parse_feature_keys(parsed.get("feature_keys")));
                if let Some(isp) = parsed.get("image_special_params") {
                    existing_image_special.insert(id, isp.clone());
                }
                if let Some(po) = parsed.get("prompt_optimize") {
                    existing_prompt_optimize.insert(id, po.clone());
                }
                if let Some(vl) = parsed.get("voice_library") {
                    existing_voice_library.insert(id, vl.clone());
                }
                if let Some(p) = parsed.get("input_protocol").and_then(|v| v.as_str()) {
                    if let Some(id_ok) = pg2026_sanitize_input_protocol(Some(p)) {
                        existing_input_protocol.insert(id, id_ok);
                    }
                }
                if let Some(p) = parsed.get("thinking_profile").and_then(|v| v.as_str()) {
                    if let Some(id_ok) = pg2026_sanitize_thinking_profile(Some(p)) {
                        existing_thinking_profile.insert(id, id_ok);
                    }
                }
            }
        }
    }

    for mc in &payload.models {
        let config_key = format!("pg_model_id_{}", mc.id);
        let mut val = json!({
            "enabled": mc.enabled,
            "scheme_id": mc.scheme_id,
            "sort_order": mc.sort_order.unwrap_or(0),
        });
        // 仅在有覆写数据时才写入，保持数据精简
        if let Some(ref overrides) = mc.param_overrides {
            val["param_overrides"] = overrides.clone();
        }
        if let Some(ref overrides) = mc.io_overrides {
            let mut o = overrides.clone();
            let scheme_id_str = mc.scheme_id.as_deref().unwrap_or("");
            if scheme_id_str == "openai_video"
                || scheme_id_str == seedance2_seed::SEEDANCE2_SCHEME_ID
                || scheme_id_str == "seedance2"
                || dashscope_video_seed::is_wan3_family_scheme_id(scheme_id_str)
            {
                if let Some(inputs) = o.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                    if let Some(modify) = inputs.get_mut("modify").and_then(|v| v.as_object_mut()) {
                        modify.remove("negative_prompt");
                    }
                }
            }
            val["io_overrides"] = o;
        }
        if name == "playground_2026" {
            if let Some(sort_order) = mc.sort_order {
                sqlx::query(
                    &state.db.format_query(
                        "UPDATE models SET sort_order = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                    ),
                )
                .bind(sort_order)
                .bind(mc.id)
                .execute(&state.db.pool)
                .await?;
                crate::api::plugins::sync_marketplace_page_sort(&state, mc.id, sort_order).await?;
            }
            if mc.mp_enabled.is_some() || mc.mp_level_ids.is_some() {
                crate::api::plugins::sync_marketplace_model_visibility(
                    &state,
                    mc.id,
                    mc.mp_enabled,
                    mc.mp_level_ids.as_deref(),
                )
                .await?;
            }
            let type_name = type_name_by_model_id
                .get(&mc.id)
                .map(|s| s.as_str())
                .unwrap_or("");
            let keys = if let Some(attrs) = mc.feature_attributes.as_deref() {
                if pg2026_type_has_feature_attrs(type_name) {
                    let sanitized = pg2026_sanitize_attr_list(attrs);
                    let attrs_json =
                        serde_json::to_string(&sanitized).unwrap_or_else(|_| "[]".to_string());
                    sqlx::query(
                        &state
                            .db
                            .format_query("UPDATE models SET feature_attributes = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"),
                    )
                    .bind(&attrs_json)
                    .bind(mc.id)
                    .execute(&state.db.pool)
                    .await?;
                    pg2026_keys_from_attrs(&sanitized, type_name)
                } else {
                    pg2026_sanitize_feature_keys(
                        existing_feature_keys
                            .get(&mc.id)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                        type_name,
                    )
                }
            } else {
                match mc.feature_keys.as_deref() {
                    Some(k) => pg2026_sanitize_feature_keys(k, type_name),
                    None => pg2026_sanitize_feature_keys(
                        existing_feature_keys
                            .get(&mc.id)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                        type_name,
                    ),
                }
            };
            val["feature_keys"] = json!(keys);
            match mc.image_special_params.as_ref() {
                Some(v) if v.is_object() => {
                    val["image_special_params"] = v.clone();
                }
                _ => {
                    if let Some(old) = existing_image_special.get(&mc.id) {
                        val["image_special_params"] = old.clone();
                    }
                }
            }
            match mc.prompt_optimize.as_ref() {
                Some(v) if v.is_null() => {}
                Some(v) if v.is_object() => {
                    val["prompt_optimize"] = v.clone();
                }
                _ => {
                    if let Some(old) = existing_prompt_optimize.get(&mc.id) {
                        val["prompt_optimize"] = old.clone();
                    }
                }
            }
            match mc.voice_library.as_ref() {
                Some(v) if v.is_null() => {}
                Some(v) if v.is_object() => {
                    val["voice_library"] = v.clone();
                }
                _ => {
                    if let Some(old) = existing_voice_library.get(&mc.id) {
                        val["voice_library"] = old.clone();
                    }
                }
            }
            if pg2026_is_chat_type(type_name) {
                match pg2026_sanitize_input_protocol(mc.input_protocol.as_deref()) {
                    Some(id) => val["input_protocol"] = json!(id),
                    None => {
                        if let Some(old) = existing_input_protocol.get(&mc.id) {
                            val["input_protocol"] = json!(old);
                        }
                    }
                }
                match pg2026_sanitize_thinking_profile(mc.thinking_profile.as_deref()) {
                    Some(id) => val["thinking_profile"] = json!(id),
                    None => {
                        if let Some(old) = existing_thinking_profile.get(&mc.id) {
                            val["thinking_profile"] = json!(old);
                        }
                    }
                }
            }
        }
        upsert_config(&state, &name, &config_key, &val.to_string()).await?;
    }

    Ok(Json(json!({ "message": "模型创作中心配置已保存" })))
}

/// 管理员：获取体验方案列表（从 DB 加载，含内置 + 自定义）
pub(crate) async fn get_playground_schemes(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<serde_json::Value>> {
    // Bug 3 修复：补冲缺失的管理员权限校验
    let role: String =
        sqlx::query_scalar(&state.db.format_query("SELECT role FROM users WHERE id = ?"))
            .bind(&claims.sub)
            .fetch_one(&state.db.pool)
            .await?;
    if role != "admin" {
        return Err(AppError::Forbidden("需要管理员权限".to_string()));
    }
    let schemes = load_schemes_from_db(&state, &name).await;
    #[cfg_attr(not(feature = "commercial_plugins"), allow(unused_mut))]
    let mut body = json!({ "schemes": schemes, "defaults": get_default_schemes_for(&name) });
    if name == "playground_2026" {
        #[cfg(feature = "commercial_plugins")]
        {
            body["voice_catalogs"] =
                crate::api::plugins::playground_2026::voice_library::catalogs_json();
        }
    }
    Ok(Json(body))
}

/// 管理员：保存体验方案列表（全量覆盖）
pub(crate) async fn save_playground_schemes(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Extension(claims): Extension<auth::Claims>,
    Json(payload): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let role: String =
        sqlx::query_scalar(&state.db.format_query("SELECT role FROM users WHERE id = ?"))
            .bind(&claims.sub)
            .fetch_one(&state.db.pool)
            .await?;
    if role != "admin" {
        return Err(AppError::Forbidden("需要管理员权限".to_string()));
    }

    let mut schemes = payload
        .get("schemes")
        .ok_or_else(|| AppError::BadRequest("缺少 schemes 字段".to_string()))?
        .clone();

    if name == "playground_2026" {
        if let Some(arr) = schemes.as_array_mut() {
            arr.retain(|s| s.get("id").and_then(|v| v.as_str()) != Some("seedance1.5pro"));
            for s in arr {
                sanitize_pg2026_video_scheme_io(s);
                #[cfg(feature = "commercial_plugins")]
                crate::api::plugins::playground_2026::prompt_optimize::seed_scheme_prompt_optimize(
                    s,
                );
                #[cfg(feature = "commercial_plugins")]
                crate::api::plugins::playground_2026::voice_library::seed_scheme_voice_library(s);
            }
        }
    }

    let schemes_str = serde_json::to_string(&schemes)
        .map_err(|_| AppError::BadRequest("方案数据序列化失败".to_string()))?;

    upsert_config(&state, &name, "pg_schemes", &schemes_str).await?;

    Ok(Json(json!({ "message": "体验方案已保存" })))
}

/// 将模型级参数覆写（delta）与预设方案参数合并
/// overrides 格式: { "modify": {"key": {patch}}, "remove": ["key"], "add": [{param}] }
fn merge_param_overrides(
    base_params: serde_json::Value,
    overrides: Option<serde_json::Value>,
) -> serde_json::Value {
    let overrides = match overrides {
        Some(v) if v.is_object() => v,
        _ => return base_params,
    };
    let base_arr = match base_params.as_array() {
        Some(a) => a.clone(),
        None => return base_params,
    };

    // 收集需删除的 key
    let removes: std::collections::HashSet<String> = overrides
        .get("remove")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // 收集需修改的 key -> patch
    let modifies: std::collections::HashMap<String, &serde_json::Value> = overrides
        .get("modify")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v)).collect())
        .unwrap_or_default();

    // 过滤 + 合并
    let mut result: Vec<serde_json::Value> = base_arr
        .into_iter()
        .filter(|p| {
            let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
            !removes.contains(key)
        })
        .map(|mut p| {
            let key = p
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(patch) = modifies.get(&key) {
                // 浅合并：patch 中的字段覆盖 base
                if let (Some(base_obj), Some(patch_obj)) = (p.as_object_mut(), patch.as_object()) {
                    for (k, v) in patch_obj {
                        base_obj.insert(k.clone(), v.clone());
                    }
                }
            }
            p
        })
        .filter(|p| p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true))
        .collect();

    // 追加新增参数
    if let Some(adds) = overrides.get("add").and_then(|v| v.as_array()) {
        result.extend(adds.clone());
    }

    json!(result)
}

/// 管理端模型 IO 编辑器用的完整口目录（对齐 frontend schemeIo.ts full*Catalog）
/// 视频未出现在方案里的口默认 enabled=false，避免铺齐后误开。
fn full_io_catalog(scheme_type: &str) -> Option<(serde_json::Value, serde_json::Value)> {
    match scheme_type {
        "image" => {
            let io = default_image_scheme_io();
            Some((
                io.get("inputs").cloned().unwrap_or(json!([])),
                io.get("outputs").cloned().unwrap_or(json!([])),
            ))
        }
        "video" => Some((
            json!([
                {
                    "key": "prompt",
                    "label": "提示词",
                    "enabled": false,
                    "modality": "text",
                    "handle_prefix": "Prompt",
                    "bind_key": "prompt",
                    "accepts": ["prompt"],
                    "required": true,
                    "max": 1
                },
                {
                    "key": "negative_prompt",
                    "label": "反向提示词",
                    "enabled": false,
                    "modality": "text",
                    "handle_prefix": "Negative Prompt",
                    "bind_key": "negative_prompt",
                    "accepts": ["prompt"],
                    "max": 1
                },
                {
                    "key": "start_frame",
                    "label": "首帧",
                    "enabled": false,
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
                    "enabled": false,
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
                    "enabled": false,
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
                    "enabled": false,
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
                    "enabled": false,
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
                    "key": "source_video",
                    "label": "源视频",
                    "enabled": false,
                    "modality": "video",
                    "handle_prefix": "Source Video",
                    "bind_key": "video_url",
                    "accepts": ["asset", "ai_video", "preview"],
                    "accept_asset_kinds": ["video"],
                    "max": 1
                }
            ]),
            json!([
                {
                    "key": "video",
                    "label": "视频",
                    "enabled": false,
                    "modality": "video",
                    "handle_prefix": "Video",
                    "result_key": "video_url",
                    "max": 1
                },
                {
                    "key": "last_frame_image",
                    "label": "最后一帧",
                    "enabled": false,
                    "modality": "image",
                    "handle_prefix": "Last Frame",
                    "result_key": "last_frame_image_url",
                    "max": 1
                }
            ]),
        )),
        "audio" => {
            let io = doubao_tts_2_seed::doubao_tts_2_scheme_io();
            Some((
                io.get("inputs").cloned().unwrap_or(json!([])),
                io.get("outputs").cloned().unwrap_or(json!([])),
            ))
        }
        "chat" => {
            let io = doubao_seed_evolving_seed::default_chat_scheme_io();
            Some((
                io.get("inputs").cloned().unwrap_or(json!([])),
                io.get("outputs").cloned().unwrap_or(json!([])),
            ))
        }
        _ => None,
    }
}

/// 用完整目录铺齐口；方案已有字段覆盖目录，目录多出口补上（默认关闭）
fn merge_ports_with_catalog(
    catalog: &serde_json::Value,
    current: serde_json::Value,
) -> serde_json::Value {
    let Some(cat_arr) = catalog.as_array() else {
        return current;
    };
    let cur_arr = current.as_array().cloned().unwrap_or_default();
    let mut by_key = std::collections::HashMap::new();
    for p in &cur_arr {
        if let Some(k) = p.get("key").and_then(|v| v.as_str()) {
            by_key.insert(k.to_string(), p.clone());
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut result: Vec<serde_json::Value> = cat_arr
        .iter()
        .map(|base| {
            let key = base.get("key").and_then(|v| v.as_str()).unwrap_or("");
            seen.insert(key.to_string());
            let Some(cur) = by_key.get(key) else {
                return base.clone();
            };
            let mut next = base.clone();
            if let (Some(obj), Some(cur_obj)) = (next.as_object_mut(), cur.as_object()) {
                for (k, v) in cur_obj {
                    if k != "key" && !v.is_null() {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }
            next
        })
        .collect();
    for p in cur_arr {
        let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if !key.is_empty() && !seen.contains(key) {
            result.push(p);
        }
    }
    json!(result)
}

fn coerce_io_overrides(raw: Option<serde_json::Value>) -> Option<serde_json::Value> {
    match raw {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => serde_json::from_str(&s).ok(),
        Some(v) => Some(v),
    }
}

/// 将模型级 IO 覆写与方案 inputs/outputs 合并
/// 先按管理端同一套完整口目录铺齐，再 apply modify —— 否则方案里没有的口（参考视频/首帧等）微调会被丢掉
/// overrides 格式: { "inputs": { "modify": {"portKey": {patch}} }, "outputs": { "modify": {...} } }
fn merge_io_overrides(
    base_inputs: serde_json::Value,
    base_outputs: serde_json::Value,
    overrides: Option<serde_json::Value>,
    scheme_type: &str,
) -> (serde_json::Value, serde_json::Value) {
    fn apply_port_modify(
        base: serde_json::Value,
        modify: Option<&serde_json::Value>,
    ) -> serde_json::Value {
        const ALLOWED: &[&str] = &[
            "enabled",
            "max",
            "min",
            "expandable",
            "default_count",
            "accepts",
            "accept_asset_kinds",
            "label",
            "required",
        ];
        let Some(arr) = base.as_array() else {
            return base;
        };
        let Some(modify_obj) = modify.and_then(|v| v.as_object()) else {
            return json!(arr.clone());
        };
        let result: Vec<serde_json::Value> = arr
            .iter()
            .map(|p| {
                let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let Some(patch) = modify_obj.get(key).and_then(|v| v.as_object()) else {
                    return p.clone();
                };
                let mut next = p.clone();
                if let Some(obj) = next.as_object_mut() {
                    for (k, v) in patch {
                        if ALLOWED.contains(&k.as_str()) {
                            obj.insert(k.clone(), v.clone());
                        }
                    }
                }
                next
            })
            .collect();
        json!(result)
    }

    let (base_inputs, base_outputs) = if let Some((cat_in, cat_out)) = full_io_catalog(scheme_type)
    {
        (
            merge_ports_with_catalog(&cat_in, base_inputs),
            merge_ports_with_catalog(&cat_out, base_outputs),
        )
    } else {
        (base_inputs, base_outputs)
    };

    let overrides = coerce_io_overrides(overrides);
    let input_modify = overrides
        .as_ref()
        .and_then(|o| o.get("inputs"))
        .and_then(|i| i.get("modify"));
    let output_modify = overrides
        .as_ref()
        .and_then(|o| o.get("outputs"))
        .and_then(|o| o.get("modify"));

    (
        apply_port_modify(base_inputs, input_modify),
        apply_port_modify(base_outputs, output_modify),
    )
}

/// 公开：获取体验中心配置供前端用户使用
/// 返回已启用的模型列表 + 各模型绑定的方案参数
pub(crate) async fn get_playground_public_config(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    claims: Option<Extension<auth::Claims>>,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let configs = load_plugin_configs(&state, &name).await?;

    let adv_nodes_enabled = configs
        .get("pg_advanced_nodes_enabled")
        .map(|s| s == "true")
        .unwrap_or(false);
    let adv_node_preview = configs
        .get("pg_advanced_node_preview_enabled")
        .map(|s| s == "true")
        .unwrap_or(true);
    let adv_node_volc = configs
        .get("pg_advanced_node_volc_enhance_enabled")
        .map(|s| s == "true")
        .unwrap_or(false);
    let adv_node_prompt = configs
        .get("pg_advanced_node_prompt_enabled")
        .map(|s| s == "true")
        .unwrap_or(true);
    let adv_node_ai_video = configs
        .get("pg_advanced_node_ai_video_enabled")
        .map(|s| s == "true")
        .unwrap_or(true);
    let adv_node_ai_image = configs
        .get("pg_advanced_node_ai_image_enabled")
        .map(|s| s == "true")
        .unwrap_or(true);
    let adv_node_agent = if name == "playground_2026" {
        false
    } else {
        configs
            .get("pg_advanced_node_agent_enabled")
            .map(|s| s == "true")
            .unwrap_or(false)
    };
    let is_pg2026 = name == "playground_2026";
    let agent_mode_enabled = if is_pg2026 {
        false
    } else {
        configs
            .get("pg_agent_mode_enabled")
            .map(|s| s == "true")
            .unwrap_or(false)
    };
    let agent_video_mode = if is_pg2026 {
        "track".to_string()
    } else {
        configs
            .get("pg_agent_video_mode")
            .cloned()
            .unwrap_or_else(|| "track".to_string())
    };
    let agent_welcome_title = if is_pg2026 {
        None
    } else {
        configs.get("pg_agent_welcome_title").cloned()
    };
    let agent_welcome_desc = if is_pg2026 {
        None
    } else {
        configs.get("pg_agent_welcome_desc").cloned()
    };
    let agent_preset_prompts: Option<serde_json::Value> = if is_pg2026 {
        None
    } else {
        configs
            .get("pg_agent_preset_prompts")
            .and_then(|s| serde_json::from_str(s).ok())
    };
    let agent_system_prompt = if is_pg2026 {
        None
    } else {
        configs.get("pg_agent_system_prompt").cloned()
    };
    let agent_chat_models: Option<Vec<String>> = if is_pg2026 {
        None
    } else {
        configs
            .get("pg_agent_chat_models")
            .and_then(|s| serde_json::from_str(s).ok())
    };
    let volc_enhance_plugin_active = is_plugin_enabled(&state, "volcengine_enhance").await;
    let schemes = load_schemes_from_db(&state, &name).await;
    let (mp_viewer, mp_configs, mp_default_enabled) = if is_pg2026 {
        let viewer = if let Some(Extension(ref c)) = claims {
            super::marketplace::marketplace_viewer_for_user_id(&state, &c.sub).await
        } else {
            super::marketplace::marketplace_viewer_from_headers(&state, &headers).await
        };
        let mp_cfgs = load_plugin_configs(&state, "model_marketplace").await?;
        let default_enabled = mp_cfgs
            .get("mp_display_mode")
            .map(|s| s.as_str())
            .unwrap_or("blacklist")
            == "blacklist";
        (viewer, mp_cfgs, default_enabled)
    } else {
        (
            super::marketplace_visibility::MarketplaceViewer::guest(),
            HashMap::new(),
            true,
        )
    };

    // 查出全部模型及其 type 信息
    let models: Vec<crate::models::Model> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM models WHERE is_active = 1 AND is_listed = 1 ORDER BY sort_order DESC, id DESC"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    let types: Vec<crate::models::ModelType> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM model_types ORDER BY sort_order DESC, id ASC"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    let providers: Vec<crate::models::ModelProvider> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM model_providers ORDER BY sort_order DESC, id ASC"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    let billing_rules: Vec<crate::models::BillingRule> =
        sqlx::query_as(&state.db.format_query("SELECT * FROM billing_rules"))
            .fetch_all(&state.db.pool)
            .await?;

    // 读取每个类型的默认模型配置并构建集合
    let default_model_mids: serde_json::Value = configs
        .get("pg_default_model_mids")
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(json!({}));

    let mut default_mids_set = std::collections::HashSet::new();
    if let Some(arr) = default_model_mids.as_array() {
        for v in arr {
            if let Some(s) = v.as_str() {
                default_mids_set.insert(s.to_string());
            }
        }
    } else if let Some(obj) = default_model_mids.as_object() {
        for (_k, v) in obj {
            if let Some(s) = v.as_str() {
                default_mids_set.insert(s.to_string());
            }
        }
    }

    let mut enabled_models = Vec::new();
    for m in &models {
        let new_key = format!("pg_model_id_{}", m.id);
        let old_key = format!("pg_model_{}", m.mid);
        let model_conf: serde_json::Value = configs
            .get(&new_key)
            .or_else(|| configs.get(&old_key))
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(json!({"enabled": false, "scheme_id": null}));

        let is_enabled = model_conf
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_agent_chat = agent_chat_models
            .as_ref()
            .map_or(false, |list| list.contains(&m.mid));
        #[cfg(feature = "plugin_volcengine_enhance")]
        if !volc_enhance_plugin_active && crate::api::plugins::is_volc_preset_mid(&m.mid) {
            continue;
        }
        #[cfg(feature = "plugin_volcengine_enhance")]
        let include_pg2026_image_tool = is_pg2026
            && volc_enhance_plugin_active
            && (crate::api::plugins::is_image_tool_mid(&m.mid)
                || crate::api::plugins::is_image_tool_mid(&m.model_id));
        #[cfg(not(feature = "plugin_volcengine_enhance"))]
        let include_pg2026_image_tool = false;
        if !is_enabled && !is_agent_chat && !include_pg2026_image_tool {
            continue;
        }
        if is_pg2026 && !include_pg2026_image_tool {
            let mp_config_key = format!("mp_model_id_{}", m.id);
            let mp_model_conf: serde_json::Value = mp_configs
                .get(&mp_config_key)
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_else(|| json!({}));
            let mp_is_enabled =
                super::marketplace_visibility::mp_enabled(&mp_model_conf, mp_default_enabled);
            let mp_level_ids = super::marketplace_visibility::parse_mp_level_ids(&mp_model_conf);
            if !super::marketplace_visibility::mp_visible_to(
                mp_is_enabled,
                &mp_level_ids,
                mp_viewer,
            ) {
                continue;
            }
        }

        let sort_order = m.sort_order as i64;

        let scheme_id = model_conf
            .get("scheme_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let mut scheme = schemes
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some(scheme_id));

        let type_name = m
            .type_id
            .and_then(|tid| types.iter().find(|t| t.id == tid))
            .map(|t| t.name.clone())
            .unwrap_or_default();

        let provider = m
            .provider_id
            .and_then(|pid| providers.iter().find(|p| p.id == pid));
        let provider_name = provider
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let provider_sort_order = provider.map(|p| p.sort_order);

        // 如果未绑定方案或方案不存在，按模型类型自动匹配第一个同类方案
        if scheme.is_none() && !type_name.is_empty() {
            let type_key = if include_pg2026_image_tool {
                "image"
            } else if type_name.contains("视频") || type_name.contains("封装") {
                "video"
            } else if type_name.contains("图片") {
                "image"
            } else if type_name.contains("聊天") && !is_pg2026 {
                "chat"
            } else if is_pg2026 && (type_name.contains("音频") || type_name.contains("语音")) {
                "audio"
            } else {
                ""
            };
            if !type_key.is_empty() {
                scheme = schemes
                    .iter()
                    .find(|s| s.get("type").and_then(|v| v.as_str()) == Some(type_key));
            }
        }

        let scheme_type = scheme
            .and_then(|s| s.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let billing_info = m
            .billing_rule_id
            .and_then(|bid| billing_rules.iter().find(|b| b.id == bid))
            .map(|b| {
                json!({
                    "billing_type": b.billing_type,
                    "name": b.name,
                    "prompt_rate": b.prompt_rate,
                    "completion_rate": b.completion_rate,
                    "cached_rate": b.cached_rate,
                    "claude_cache_creation_rate": b.claude_cache_creation_rate,
                    "claude_cache_read_rate": b.claude_cache_read_rate,
                    "fixed_rate": b.fixed_rate,
                    "duration_rate": b.duration_rate,
                    "pricing_tiers": b.pricing_tiers,
                    "billing_rule": b.billing_rule,
                    "extended_config": b.extended_config,
                })
            })
            .unwrap_or(json!(null));

        let (mut merged_inputs, merged_outputs) = merge_io_overrides(
            scheme
                .and_then(|s| s.get("inputs"))
                .cloned()
                .unwrap_or(json!([])),
            scheme
                .and_then(|s| s.get("outputs"))
                .cloned()
                .unwrap_or(json!([])),
            model_conf.get("io_overrides").cloned(),
            scheme_type,
        );

        let scheme_id_str = scheme
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if scheme_id_str == "openai_video"
            || scheme_id_str == seedance2_seed::SEEDANCE2_SCHEME_ID
            || scheme_id_str == "seedance2"
            || dashscope_video_seed::is_wan3_family_scheme_id(scheme_id_str)
        {
            if let Some(arr) = merged_inputs.as_array_mut() {
                arr.retain(|p| {
                    let k = p.get("key").and_then(|v| v.as_str());
                    let b = p.get("bind_key").and_then(|v| v.as_str());
                    k != Some("negative_prompt") && b != Some("negative_prompt")
                });
            }
        }

        let max_reference_images = merged_inputs
            .as_array()
            .and_then(|arr| {
                arr.iter().find(|p| {
                    let enabled = p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                    if !enabled {
                        return false;
                    }
                    p.get("key").and_then(|v| v.as_str()) == Some("reference_images")
                        || matches!(
                            p.get("bind_key").and_then(|v| v.as_str()),
                            Some("image_urls") | Some("reference_urls")
                        )
                })
            })
            .and_then(|p| p.get("max").and_then(|v| v.as_i64()))
            .filter(|n| *n >= 0)
            .or_else(|| {
                scheme
                    .and_then(|s| s.get("max_reference_images"))
                    .and_then(|v| v.as_i64())
            })
            .unwrap_or(7);
        let mut final_params = merge_param_overrides(
            scheme
                .and_then(|s| s.get("params"))
                .cloned()
                .unwrap_or(json!([])),
            model_conf.get("param_overrides").cloned(),
        );
        if is_pg2026 && scheme_type == "audio" {
            #[cfg(feature = "commercial_plugins")]
            crate::api::plugins::playground_2026::voice_library::strip_voice_from_params(
                &mut final_params,
            );
        }
        if is_pg2026
            && seedream_5_0_pro_seed::is_seedream_5_0_pro_model(
                &format!("{} {}", m.model_id, m.model_id_alias),
                &m.name,
            )
        {
            final_params = seedream_5_0_pro_seed::lock_image_count_to_one(final_params);
            final_params = seedream_5_0_pro_seed::ensure_layer_decomposition_param(final_params);
        }

        let is_default = default_mids_set.contains(&m.mid);
        let mut row = json!({
            "id": m.id,
            "mid": m.mid,
            "name": m.name,
            "model_id": m.model_id,
            "remark": m.remark,
            "description": m.description,
            "logo": m.logo,
            "type_name": type_name,
            "is_default": is_default,
            "provider_id": m.provider_id,
            "provider_name": provider_name,
            "scheme_id": scheme_id,
            "scheme_name": scheme.and_then(|s| s.get("name")).and_then(|v| v.as_str()).unwrap_or(""),
            "scheme_type": scheme_type,
            "max_reference_images": max_reference_images,
            "endpoint": scheme.and_then(|s| s.get("endpoint")).and_then(|v| v.as_str()).unwrap_or(""),
            "poll_endpoint": scheme.and_then(|s| s.get("poll_endpoint")).and_then(|v| v.as_str()).unwrap_or(""),
            "billing": billing_info,
            "sort_order": sort_order,
            "global_discount": m.global_discount,
            "global_discount_enabled": m.global_discount_enabled,
            "params": final_params,
            "inputs": merged_inputs,
            "outputs": merged_outputs,
        });
        if is_pg2026 {
            row["allow_text_to_video"] = scheme
                .and_then(|s| s.get("allow_text_to_video"))
                .cloned()
                .unwrap_or(json!(true));
            if let Some(order) = provider_sort_order {
                row["provider_sort_order"] = json!(order);
            }
            let attrs = pg2026_parse_attr_list(m.feature_attributes.as_deref());
            let mapped = pg2026_keys_from_attrs(&attrs, &type_name);
            let keys = if mapped.is_empty() {
                pg2026_sanitize_feature_keys(
                    &pg2026_parse_feature_keys(model_conf.get("feature_keys")),
                    &type_name,
                )
            } else {
                mapped
            };
            row["feature_keys"] = json!(keys);
            if pg2026_is_chat_type(&type_name) {
                if let Some(p) = model_conf.get("input_protocol").and_then(|v| v.as_str()) {
                    if let Some(id) = pg2026_sanitize_input_protocol(Some(p)) {
                        row["input_protocol"] = json!(id);
                    }
                }
                if let Some(p) = model_conf.get("thinking_profile").and_then(|v| v.as_str()) {
                    if let Some(id) = pg2026_sanitize_thinking_profile(Some(p)) {
                        row["thinking_profile"] = json!(id);
                    }
                }
            }
            if scheme_type == "image" {
                let from_model = model_conf.get("image_special_params").cloned();
                let from_scheme = scheme.and_then(|s| s.get("image_special_params")).cloned();
                let model_on = from_model
                    .as_ref()
                    .and_then(|v| v.get("enabled"))
                    .and_then(|v| v.as_bool())
                    == Some(true);
                let model_off = from_model
                    .as_ref()
                    .and_then(|v| v.get("enabled"))
                    .and_then(|v| v.as_bool())
                    == Some(false);
                if model_on {
                    row["image_special_params"] = from_model.unwrap();
                } else if !model_off {
                    if let Some(isp) = from_scheme {
                        row["image_special_params"] = isp;
                    }
                }
            }
            if scheme_type == "image" || scheme_type == "video" {
                #[cfg(feature = "commercial_plugins")]
                {
                    row["prompt_optimize"] = crate::api::plugins::playground_2026::prompt_optimize::public_body_json(
                        crate::api::plugins::playground_2026::prompt_optimize::resolve_model_body(
                            Some(&model_conf),
                            scheme,
                        ),
                    );
                }
            }
            if scheme_type == "audio" {
                #[cfg(feature = "commercial_plugins")]
                {
                    row["voice_library"] = crate::api::plugins::playground_2026::voice_library::public_library_json(
                        crate::api::plugins::playground_2026::voice_library::resolve_model_library(
                            Some(&model_conf),
                            scheme,
                        ),
                    );
                }
            }
        }
        enabled_models.push(row);
    }

    // 根据 sort_order 对已启用的模型进行降序排序
    enabled_models.sort_by(|a, b| {
        let sa = a.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0);
        let sb = b.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0);
        sb.cmp(&sa)
    });

    let mut advanced_nodes = json!({
        "enabled": adv_nodes_enabled,
        "preview_enabled": adv_node_preview,
        "volc_enhance_enabled": adv_node_volc,
        "volc_enhance_plugin_active": volc_enhance_plugin_active,
        "prompt_enabled": adv_node_prompt,
        "ai_video_enabled": adv_node_ai_video,
        "ai_image_enabled": adv_node_ai_image,
        "agent_enabled": adv_node_agent,
    });
    if is_pg2026 {
        advanced_nodes["director_enabled"] = json!(configs
            .get("pg_advanced_node_director_enabled")
            .map(|s| s == "true")
            .unwrap_or(false));
    }
    if !is_pg2026 {
        advanced_nodes["agent_mode_enabled"] = json!(agent_mode_enabled);
        advanced_nodes["agent_video_mode"] = json!(agent_video_mode);
        advanced_nodes["agent_welcome_title"] = json!(agent_welcome_title);
        advanced_nodes["agent_welcome_desc"] = json!(agent_welcome_desc);
        advanced_nodes["agent_preset_prompts"] = json!(agent_preset_prompts);
        advanced_nodes["agent_system_prompt"] = json!(agent_system_prompt);
        advanced_nodes["agent_chat_models"] = json!(agent_chat_models);
    }

    let default_model_mids = if is_pg2026 {
        let visible_mids: std::collections::HashSet<&str> = enabled_models
            .iter()
            .filter_map(|m| m.get("mid").and_then(|v| v.as_str()))
            .collect();
        match default_model_mids {
            serde_json::Value::Array(arr) => serde_json::Value::Array(
                arr.into_iter()
                    .filter(|v| v.as_str().map(|s| visible_mids.contains(s)).unwrap_or(false))
                    .collect(),
            ),
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.into_iter()
                    .filter(|(_, v)| v.as_str().map(|s| visible_mids.contains(s)).unwrap_or(false))
                    .collect(),
            ),
            other => other,
        }
    } else {
        default_model_mids
    };

    let mut body = json!({
        "models": enabled_models,
        "default_model_mids": default_model_mids,
        "advanced_nodes": advanced_nodes,
    });
    if is_pg2026 {
        body["feature_catalog"] = pg2026_feature_catalog(&types);
        #[cfg(feature = "commercial_plugins")]
        {
            body["error_mappings"] =
                crate::api::plugins::playground_2026::error_mappings::mappings_from_configs(
                    &configs,
                );
            let po_enabled = crate::api::plugins::playground_2026::prompt_optimize::config_enabled(
                configs
                    .get(crate::api::plugins::playground_2026::prompt_optimize::ENABLED_KEY)
                    .map(|s| s.as_str()),
            );
            body["prompt_optimize_enabled"] = json!(po_enabled);
            body["prompt_optimize_llm"] = if po_enabled {
                let preferred = configs
                    .get(crate::api::plugins::playground_2026::prompt_optimize::CONFIG_KEY)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                crate::api::plugins::playground_2026::prompt_optimize::public_llm_json(
                    &state, preferred,
                )
                .await
            } else {
                serde_json::Value::Null
            };
            body["voice_catalogs"] =
                crate::api::plugins::playground_2026::voice_library::catalogs_json();
        }
    }
    Ok(Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playground_2026_drops_seedance_1_5_pro_scheme() {
        let pg2026 = get_default_schemes_for("playground_2026");
        assert!(pg2026
            .iter()
            .all(|s| s.get("id").and_then(|v| v.as_str()) != Some("seedance1.5pro")));
        let legacy = get_default_schemes_for("playground");
        assert!(legacy
            .iter()
            .any(|s| s.get("id").and_then(|v| v.as_str()) == Some("seedance1.5pro")));
    }

    #[test]
    fn playground_2026_has_doubao_seed_evolving_chat_scheme() {
        let pg2026 = get_default_schemes_for("playground_2026");
        let scheme = pg2026
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some("doubao_seed_evolving"));
        assert!(scheme.is_some());
        let params = scheme.unwrap().get("params").and_then(|v| v.as_array());
        let keys: Vec<&str> = params
            .unwrap()
            .iter()
            .filter_map(|p| p.get("key").and_then(|v| v.as_str()))
            .collect();
        assert!(!keys.contains(&"thinking"));
        assert!(keys.contains(&"temperature"));
        assert!(keys.contains(&"max_tokens"));
        let io_keys: Vec<&str> = scheme
            .unwrap()
            .get("inputs")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|p| p.get("key").and_then(|v| v.as_str()))
            .collect();
        assert!(io_keys.contains(&"thinking"));
        let thinking_on = scheme
            .unwrap()
            .get("inputs")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .find(|p| p.get("key").and_then(|v| v.as_str()) == Some("thinking"))
            .and_then(|p| p.get("enabled"))
            .and_then(|v| v.as_bool());
        assert_eq!(thinking_on, Some(true));
        let temp = scheme
            .unwrap()
            .get("params")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .find(|p| p.get("key").and_then(|v| v.as_str()) == Some("temperature"))
            .unwrap();
        assert_eq!(temp.get("default").and_then(|v| v.as_f64()), Some(1.0));
        let max_tokens = scheme
            .unwrap()
            .get("params")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .find(|p| p.get("key").and_then(|v| v.as_str()) == Some("max_tokens"))
            .unwrap();
        let opts = max_tokens
            .get("options")
            .and_then(|v| v.as_array())
            .unwrap();
        assert!(opts.iter().any(|v| v.as_i64() == Some(131072)));
        let legacy = get_default_schemes_for("playground");
        assert!(legacy
            .iter()
            .all(|s| s.get("id").and_then(|v| v.as_str()) != Some("doubao_seed_evolving")));
    }

    #[test]
    fn playground_2026_has_doubao_tts_voice_library() {
        let pg2026 = get_default_schemes_for("playground_2026");
        let scheme = pg2026
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some("doubao_tts_2_0"))
            .expect("doubao_tts_2_0");
        assert_eq!(scheme.get("type").and_then(|v| v.as_str()), Some("audio"));
        let keys: Vec<&str> = scheme
            .get("params")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|p| p.get("key").and_then(|v| v.as_str()))
            .collect();
        assert!(!keys.contains(&"voice"));
        assert!(keys.contains(&"speed"));
        assert_eq!(
            scheme
                .get("voice_library")
                .and_then(|v| v.get("enabled"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            scheme
                .get("voice_library")
                .and_then(|v| v.get("catalog"))
                .and_then(|v| v.as_str()),
            Some("doubao_tts_2_0")
        );
        assert_eq!(
            scheme
                .get("voice_library")
                .and_then(|v| v.get("default"))
                .and_then(|v| v.as_str()),
            Some("zh_female_vv_uranus_bigtts")
        );
    }

    #[test]
    fn playground_2026_drops_generic_chat_schemes() {
        let pg2026 = get_default_schemes_for("playground_2026");
        for id in ["chat_standard", "chat_creative", "chat_precise"] {
            assert!(
                pg2026
                    .iter()
                    .all(|s| s.get("id").and_then(|v| v.as_str()) != Some(id)),
                "{id} should be removed from playground_2026"
            );
        }
        let legacy = get_default_schemes_for("playground");
        for id in ["chat_standard", "chat_creative", "chat_precise"] {
            assert!(
                legacy
                    .iter()
                    .any(|s| s.get("id").and_then(|v| v.as_str()) == Some(id)),
                "{id} should remain on legacy playground"
            );
        }
    }

    #[test]
    fn playground_2026_has_grok_4_6_chat_scheme() {
        let pg2026 = get_default_schemes_for("playground_2026");
        assert!(
            pg2026
                .iter()
                .any(|s| s.get("id").and_then(|v| v.as_str()) == Some("grok_4_6")),
            "grok_4_6 should be injected for playground_2026"
        );
        let legacy = get_default_schemes_for("playground");
        assert!(
            legacy
                .iter()
                .all(|s| s.get("id").and_then(|v| v.as_str()) != Some("grok_4_6")),
            "grok_4_6 must not be injected into legacy playground"
        );
    }

    #[test]
    fn playground_2026_has_glm_and_deepseek_chat_schemes() {
        let pg2026 = get_default_schemes_for("playground_2026");
        let glm = pg2026
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some("glm_5_2"))
            .expect("glm_5_2");
        let glm53 = pg2026
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some("glm_5_3"))
            .expect("glm_5_3");
        let ds = pg2026
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some("deepseek_v4_pro"))
            .expect("deepseek_v4_pro");
        let qwen = pg2026
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some("qwen_3_7_max"))
            .expect("qwen_3_7_max");
        for scheme in [glm, glm53, ds, qwen] {
            let keys: Vec<&str> = scheme
                .get("params")
                .and_then(|v| v.as_array())
                .unwrap()
                .iter()
                .filter_map(|p| p.get("key").and_then(|v| v.as_str()))
                .collect();
            assert!(!keys.contains(&"thinking"));
            let thinking_on = scheme
                .get("inputs")
                .and_then(|v| v.as_array())
                .unwrap()
                .iter()
                .find(|p| p.get("key").and_then(|v| v.as_str()) == Some("thinking"))
                .and_then(|p| p.get("enabled"))
                .and_then(|v| v.as_bool());
            assert_eq!(thinking_on, Some(true));
        }
        let temp = glm
            .get("params")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .find(|p| p.get("key").and_then(|v| v.as_str()) == Some("temperature"))
            .unwrap();
        assert_eq!(temp.get("max").and_then(|v| v.as_f64()), Some(1.0));
        let legacy = get_default_schemes_for("playground");
        assert!(legacy.iter().all(|s| {
            !matches!(
                s.get("id").and_then(|v| v.as_str()),
                Some("glm_5_2" | "glm_5_3" | "deepseek_v4_pro" | "qwen_3_7_max" | "claude_fable_5" | "gpt_6_astra")
            )
        }));
    }

    #[test]
    fn playground_2026_has_claude_fable_chat_scheme() {
        let pg2026 = get_default_schemes_for("playground_2026");
        let scheme = pg2026
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some("claude_fable_5"))
            .expect("claude_fable_5");
        assert_eq!(scheme.get("type").and_then(|v| v.as_str()), Some("chat"));
        assert_eq!(
            scheme.get("is_system").and_then(|v| v.as_bool()),
            Some(true)
        );
        let keys: Vec<&str> = scheme
            .get("params")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|p| p.get("key").and_then(|v| v.as_str()))
            .collect();
        assert!(keys.contains(&"temperature"));
        assert!(keys.contains(&"max_tokens"));
        assert!(!keys.contains(&"top_p"));
        assert!(!keys.contains(&"thinking"));
        let thinking_on = scheme
            .get("inputs")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .find(|p| p.get("key").and_then(|v| v.as_str()) == Some("thinking"))
            .and_then(|p| p.get("enabled"))
            .and_then(|v| v.as_bool());
        assert_eq!(thinking_on, Some(false));
    }

    #[test]
    fn playground_2026_has_gpt_6_astra_chat_scheme() {
        let pg2026 = get_default_schemes_for("playground_2026");
        let scheme = pg2026
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some("gpt_6_astra"))
            .expect("gpt_6_astra");
        assert_eq!(scheme.get("type").and_then(|v| v.as_str()), Some("chat"));
        let keys: Vec<&str> = scheme
            .get("params")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|p| p.get("key").and_then(|v| v.as_str()))
            .collect();
        assert!(keys.contains(&"max_completion_tokens"));
        assert!(!keys.contains(&"temperature"));
        assert!(!keys.contains(&"top_p"));
        assert!(!keys.contains(&"max_tokens"));
        let thinking_on = scheme
            .get("inputs")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .find(|p| p.get("key").and_then(|v| v.as_str()) == Some("thinking"))
            .and_then(|p| p.get("enabled"))
            .and_then(|v| v.as_bool());
        assert_eq!(thinking_on, Some(true));
        let legacy = get_default_schemes_for("playground");
        assert!(legacy
            .iter()
            .all(|s| s.get("id").and_then(|v| v.as_str()) != Some("gpt_6_astra")));
    }
}
