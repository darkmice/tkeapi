/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：claude-fable-5 聊天方案。
//! 仅 playground_2026 注入。claude-opus-4-8 / claude-opus-4-7 共用。
//! 不传 top_p：opus-4-7 上游会拒。深度思考默认关：fable 不接受 thinking.type=enabled。

use serde_json::{json, Value};

use super::doubao_seed_evolving_seed::default_chat_scheme_io;

pub const CLAUDE_FABLE_5_SCHEME_ID: &str = "claude_fable_5";

pub fn claude_fable_5_scheme() -> Value {
    let io = default_chat_scheme_io();
    json!({
        "id": CLAUDE_FABLE_5_SCHEME_ID,
        "name": "claude-fable-5聊天方案",
        "type": "chat",
        "is_system": true,
        "description": "适用于 claude-fable-5、claude-opus-4-8、claude-opus-4-7。请求走 Anthropic Messages。temperature 范围 0–1。不传 top_p，opus-4-7 的上游会拒。深度思考默认关闭，fable 不接受 thinking.type=enabled。",
        "max_reference_images": 0,
        "inputs": io.get("inputs").cloned().unwrap_or(json!([])),
        "outputs": io.get("outputs").cloned().unwrap_or(json!([])),
        "params": [
            {"key": "temperature", "label": "创意度", "type": "slider", "default": 1.0, "min": 0.0, "max": 1.0, "step": 0.05, "hint": "Anthropic 范围 0–1，默认 1。三个 Claude 聊天模型共用"},
            {"key": "max_tokens", "label": "最大回复长度", "type": "select", "options": [1024, 2048, 4096, 8192, 16384, 32000], "default": 4096, "unit": "tokens", "hint": "输出上限。默认 4096，三家都能接受"},
            {"key": "stream", "label": "流式输出", "type": "switch", "default": true, "hint": "逐字输出回答"}
        ]
    })
}

pub fn seed_claude_fable_chat_scheme(schemes: &mut Vec<Value>) {
    let next = claude_fable_5_scheme();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(CLAUDE_FABLE_5_SCHEME_ID))
    {
        schemes[pos] = next;
    } else {
        schemes.push(next);
    }
}
