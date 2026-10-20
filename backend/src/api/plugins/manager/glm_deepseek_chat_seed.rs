/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：GLM-5.2 / DeepSeek-V4-Pro 聊天方案。
//! 仅 playground_2026 注入。深度思考放在 IO 能力开关，不进 params。

use serde_json::{json, Value};

use super::doubao_seed_evolving_seed::{doubao_chat_scheme_io, DOUBAO_SEED_EVOLVING_SCHEME_ID};

pub const GLM_5_2_SCHEME_ID: &str = "glm_5_2";
pub const GLM_5_3_SCHEME_ID: &str = "glm_5_3";
pub const DEEPSEEK_V4_PRO_SCHEME_ID: &str = "deepseek_v4_pro";
pub const QWEN_3_7_MAX_SCHEME_ID: &str = "qwen_3_7_max";

pub fn is_thinking_on_chat_scheme_id(id: &str) -> bool {
    id == DOUBAO_SEED_EVOLVING_SCHEME_ID
        || id == GLM_5_2_SCHEME_ID
        || id == GLM_5_3_SCHEME_ID
        || id == DEEPSEEK_V4_PRO_SCHEME_ID
        || id == QWEN_3_7_MAX_SCHEME_ID
}

fn chat_scheme_shell(id: &str, name: &str, description: &str, params: Value) -> Value {
    let io = doubao_chat_scheme_io();
    json!({
        "id": id,
        "name": name,
        "type": "chat",
        "is_system": true,
        "description": description,
        "inputs": io.get("inputs").cloned().unwrap_or(json!([])),
        "outputs": io.get("outputs").cloned().unwrap_or(json!([])),
        "params": params
    })
}

pub fn glm_5_2_scheme() -> Value {
    chat_scheme_shell(
        GLM_5_2_SCHEME_ID,
        "GLM-5.2聊天方案",
        "智谱 GLM-5.2 及以下对话方案。thinking.type 默认 enabled，reasoning_effort 高=max、中=high。temperature 仅 0–1；勿与 top_p 同时猛调。max_tokens 上限 131072。GLM-5.3 不用这套。",
        json!([
            {"key": "temperature", "label": "创意度", "type": "slider", "default": 1.0, "min": 0.0, "max": 1.0, "step": 0.05, "hint": "官方范围 0–1，默认 1.0；不要和核采样一起猛调"},
            {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072], "default": 4096, "unit": "tokens", "hint": "输出上限 128K；思考与回答共用额度"},
            {"key": "stream", "label": "流式输出", "type": "switch", "default": true, "hint": "逐字输出回答，提升体验"},
            {"key": "top_p", "label": "核采样", "type": "slider", "default": 0.95, "min": 0.01, "max": 1.0, "step": 0.01, "hint": "官方范围 0.01–1，默认 0.95"}
        ]),
    )
}

pub fn deepseek_v4_pro_scheme() -> Value {
    chat_scheme_shell(
        DEEPSEEK_V4_PRO_SCHEME_ID,
        "DeepSeek-V4-Pro聊天方案",
        "DeepSeek-V4-Pro 对话方案。thinking.type 默认 enabled；reasoning_effort 为 low / high（官方默认）/ max（高=max，中=high，低=low）。思考开启时官方忽略 temperature / top_p。max_tokens 默认 4096，建议思考模式 ≥2048。",
        json!([
            {"key": "temperature", "label": "创意度", "type": "slider", "default": 1.0, "min": 0.0, "max": 2.0, "step": 0.1, "hint": "官方默认 1；深度思考开启时该参数无效"},
            {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072], "default": 4096, "unit": "tokens", "hint": "思考与回答共用额度，思考模式建议 ≥2048"},
            {"key": "stream", "label": "流式输出", "type": "switch", "default": true, "hint": "逐字输出回答，提升体验"},
            {"key": "top_p", "label": "核采样", "type": "slider", "default": 1.0, "min": 0.0, "max": 1.0, "step": 0.05, "hint": "官方默认 1；深度思考开启时该参数无效"}
        ]),
    )
}

pub fn qwen_3_7_max_scheme() -> Value {
    chat_scheme_shell(
        QWEN_3_7_MAX_SCHEME_ID,
        "Qwen3.7-Max聊天方案",
        "通义千问对话方案。深度思考走 enable_thinking（默认开）；3.7/3.5/3.6 高档不传 thinking_budget（官方默认上限），中/低为 16384/4096；3.8 用 reasoning_effort（高=xhigh，中=medium，低=low）。3.8 支持图/视频理解。temperature 0–2 默认 0.7。",
        json!([
            {"key": "temperature", "label": "创意度", "type": "slider", "default": 0.7, "min": 0.0, "max": 2.0, "step": 0.1, "hint": "官方范围 [0, 2)，默认 0.7；不要和核采样一起猛调"},
            {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072], "default": 4096, "unit": "tokens", "hint": "回答长度上限；思考另计 thinking_budget / reasoning_effort"},
            {"key": "stream", "label": "流式输出", "type": "switch", "default": true, "hint": "逐字输出回答，提升体验"},
            {"key": "top_p", "label": "核采样", "type": "slider", "default": 0.8, "min": 0.01, "max": 1.0, "step": 0.01, "hint": "官方 qwen-max 默认约 0.8"}
        ]),
    )
}

fn upsert_scheme(schemes: &mut Vec<Value>, next: Value) {
    let id = next.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(id))
    {
        schemes[pos] = next;
    } else {
        schemes.push(next);
    }
}

pub fn glm_5_3_scheme() -> Value {
    chat_scheme_shell(
        GLM_5_3_SCHEME_ID,
        "GLM-5.3聊天方案",
        "智谱 GLM-5.3 对话方案。不传 thinking。reasoning_effort 高=xhigh、中=high、关=none，不传 max。temperature 仅 0–1。max_tokens 上限 131072。",
        json!([
            {"key": "temperature", "label": "创意度", "type": "slider", "default": 1.0, "min": 0.0, "max": 1.0, "step": 0.05, "hint": "官方范围 0–1，默认 1.0；不要和核采样一起猛调"},
            {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072], "default": 4096, "unit": "tokens", "hint": "输出上限 128K；思考与回答共用额度"},
            {"key": "stream", "label": "流式输出", "type": "switch", "default": true, "hint": "逐字输出回答，提升体验"},
            {"key": "top_p", "label": "核采样", "type": "slider", "default": 0.95, "min": 0.01, "max": 1.0, "step": 0.01, "hint": "官方范围 0.01–1，默认 0.95"}
        ]),
    )
}

pub fn seed_glm_5_2_scheme(schemes: &mut Vec<Value>) {
    upsert_scheme(schemes, glm_5_2_scheme());
}

pub fn seed_glm_5_3_scheme(schemes: &mut Vec<Value>) {
    upsert_scheme(schemes, glm_5_3_scheme());
}

pub fn seed_deepseek_v4_pro_scheme(schemes: &mut Vec<Value>) {
    upsert_scheme(schemes, deepseek_v4_pro_scheme());
}

pub fn seed_qwen_3_7_max_scheme(schemes: &mut Vec<Value>) {
    upsert_scheme(schemes, qwen_3_7_max_scheme());
}

pub fn seed_glm_deepseek_chat_schemes(schemes: &mut Vec<Value>) {
    seed_glm_5_2_scheme(schemes);
    seed_glm_5_3_scheme(schemes);
    seed_deepseek_v4_pro_scheme(schemes);
    seed_qwen_3_7_max_scheme(schemes);
}
