/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：grok-4.6 聊天方案。仅 playground_2026 注入。

use serde_json::{json, Value};

use super::doubao_seed_evolving_seed::default_chat_scheme_io;

pub const GROK_4_6_SCHEME_ID: &str = "grok_4_6";

pub fn grok_4_6_scheme() -> Value {
    let io = default_chat_scheme_io();
    json!({
        "id": GROK_4_6_SCHEME_ID,
        "name": "grok-4.6聊天方案",
        "type": "chat",
        "is_system": true,
        "description": "仅用于 grok-4.6。请求走 OpenAI 兼容聊天接口。参数与原先绑定的标准对话方案一致：创意度 0–2、核采样、最大回复长度、流式输出。深度思考默认关闭。",
        "max_reference_images": 0,
        "inputs": io.get("inputs").cloned().unwrap_or(json!([])),
        "outputs": io.get("outputs").cloned().unwrap_or(json!([])),
        "params": [
            {"key": "temperature", "label": "创意度", "type": "slider", "default": 0.7, "min": 0.0, "max": 2.0, "step": 0.1, "hint": "值越高回答越有创意，越低越精确"},
            {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [256, 512, 1024, 2048, 4096, 8192], "default": 4096, "unit": "tokens"},
            {"key": "stream", "label": "流式输出", "type": "switch", "default": true, "hint": "逐字输出回答"},
            {"key": "top_p", "label": "核采样", "type": "slider", "default": 1.0, "min": 0.0, "max": 1.0, "step": 0.05}
        ]
    })
}

pub fn seed_grok_4_6_chat_scheme(schemes: &mut Vec<Value>) {
    let next = grok_4_6_scheme();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(GROK_4_6_SCHEME_ID))
    {
        schemes[pos] = next;
    } else {
        schemes.push(next);
    }
}
