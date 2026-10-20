/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：gpt-6-astra 聊天方案。
//! 仅 playground_2026 注入。gpt-5.6-sol / luna / terra、gpt-5.5 共用。
//! 思考档只发 low / medium / high：astra 不接受 none，gpt-5.5 不接受 max。
//! 长度走 max_completion_tokens，不传 temperature / top_p。

use serde_json::{json, Value};

use super::doubao_seed_evolving_seed::doubao_chat_scheme_io;

pub const GPT_6_ASTRA_SCHEME_ID: &str = "gpt_6_astra";

pub fn is_gpt_6_astra_scheme_id(id: &str) -> bool {
    id == GPT_6_ASTRA_SCHEME_ID
}

pub fn gpt_6_astra_scheme() -> Value {
    let io = doubao_chat_scheme_io();
    json!({
        "id": GPT_6_ASTRA_SCHEME_ID,
        "name": "gpt-6-astra聊天方案",
        "type": "chat",
        "is_system": true,
        "description": "适用于 gpt-6-astra、gpt-5.6-sol、gpt-5.6-luna、gpt-5.6-terra、gpt-5.5。Chat Completions。思考档低/中/高对应 reasoning_effort low/medium/high，默认中。不传 none 与 max。长度用 max_completion_tokens。不传 temperature / top_p。输入为文本和图片。",
        "max_reference_images": 4,
        "inputs": io.get("inputs").cloned().unwrap_or(json!([])),
        "outputs": io.get("outputs").cloned().unwrap_or(json!([])),
        "params": [
            {"key": "max_completion_tokens", "label": "最大回复长度", "type": "select", "options": [1024, 2048, 4096, 8192, 16384, 32768, 65536, 128000], "default": 4096, "unit": "tokens", "hint": "含思考 token。默认 4096，上限 128000"},
            {"key": "stream", "label": "流式输出", "type": "switch", "default": true, "hint": "逐字输出回答"}
        ]
    })
}

pub fn seed_gpt_6_astra_chat_scheme(schemes: &mut Vec<Value>) {
    let next = gpt_6_astra_scheme();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(GPT_6_ASTRA_SCHEME_ID))
    {
        schemes[pos] = next;
    } else {
        schemes.push(next);
    }
}
