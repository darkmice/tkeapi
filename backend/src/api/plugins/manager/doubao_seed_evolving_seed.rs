/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：Doubao-Seed-Evolving 聊天方案。
//! 仅 playground_2026 注入。深度思考放在 IO 能力开关，不进 params。

use serde_json::{json, Value};

pub const DOUBAO_SEED_EVOLVING_SCHEME_ID: &str = "doubao_seed_evolving";

fn chat_prompt_port() -> Value {
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

fn chat_thinking_port(enabled: bool) -> Value {
    json!({
        "key": "thinking",
        "label": "深度思考",
        "enabled": enabled,
        "modality": "text",
        "handle_prefix": "Thinking",
        "bind_key": "feature:thinking",
        "max": 0
    })
}

fn chat_message_port() -> Value {
    json!({
        "key": "message",
        "label": "回复",
        "enabled": true,
        "modality": "text",
        "handle_prefix": "Message",
        "result_key": "content",
        "max": 1
    })
}

/// 通用聊天 IO：深度思考默认关（模型 IO 可单独打开）。
pub fn default_chat_scheme_io() -> Value {
    json!({
        "inputs": [chat_prompt_port(), chat_thinking_port(false)],
        "outputs": [chat_message_port()]
    })
}

/// 豆包聊天 IO：深度思考默认开。
pub fn doubao_chat_scheme_io() -> Value {
    json!({
        "inputs": [chat_prompt_port(), chat_thinking_port(true)],
        "outputs": [chat_message_port()]
    })
}

pub fn strip_legacy_thinking_param(scheme: &mut Value) {
    if let Some(arr) = scheme.get_mut("params").and_then(|v| v.as_array_mut()) {
        arr.retain(|p| p.get("key").and_then(|v| v.as_str()) != Some("thinking"));
    }
}

pub fn doubao_seed_evolving_scheme() -> Value {
    let io = doubao_chat_scheme_io();
    json!({
        "id": DOUBAO_SEED_EVOLVING_SCHEME_ID,
        "name": "Doubao-Seed-Evolving聊天方案",
        "type": "chat",
        "is_system": true,
        "description": "豆包 Seed / Seed Evolving 对话方案。深度思考在 IO 中开关；2.0 / 1.8 思考默认 medium，Evolving / 2.1 默认 high。temperature 官方默认 1.0。",
        "inputs": io.get("inputs").cloned().unwrap_or(json!([])),
        "outputs": io.get("outputs").cloned().unwrap_or(json!([])),
        "params": [
            {"key": "temperature", "label": "创意度", "type": "slider", "default": 1.0, "min": 0.0, "max": 2.0, "step": 0.1, "hint": "官方范围 0–2，默认 1.0"},
            {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072], "default": 4096, "unit": "tokens", "hint": "只限制最终回答，不含思维链。勿与 max_completion_tokens 同时传"},
            {"key": "stream", "label": "流式输出", "type": "switch", "default": true, "hint": "逐字输出回答，提升体验"},
            {"key": "top_p", "label": "核采样", "type": "slider", "default": 1.0, "min": 0.0, "max": 1.0, "step": 0.05}
        ]
    })
}

pub fn seed_doubao_seed_evolving_scheme(schemes: &mut Vec<Value>) {
    let next = doubao_seed_evolving_scheme();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(DOUBAO_SEED_EVOLVING_SCHEME_ID))
    {
        schemes[pos] = next;
    } else {
        schemes.push(next);
    }
}
