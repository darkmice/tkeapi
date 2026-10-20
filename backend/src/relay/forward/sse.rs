/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

// ── SSE 流式转换 ──────────────────────────────────────────────

/// 根据 target_type 转换 SSE 数据行为 OpenAI 格式
pub fn transform_sse_line(target_type: &str, line: &str, model: &str) -> Option<String> {
    if !line.starts_with("data: ") {
        return None;
    }
    let data = &line[6..];
    if data == "[DONE]" {
        return None;
    }

    match target_type {
        "anthropic" => transform_anthropic_sse(data, model),
        "gemini" => transform_gemini_sse(data, model),
        _ => Some(data.to_string()), // OpenAI 兼容直接透传
    }
}

fn transform_anthropic_sse(data: &str, model: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    let event_type = v.get("type")?.as_str()?;
    if event_type == "content_block_delta" {
        let text = v.get("delta")?.get("text")?.as_str()?;
        return serde_json::to_string(&create_openai_chunk(text, model)).ok();
    }
    None
}

fn transform_gemini_sse(data: &str, model: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;

    // 文字/图片内容 chunk
    let text_chunk = v
        .pointer("/candidates/0/content/parts")
        .and_then(|p| p.as_array())
        .and_then(|parts| {
            for part in parts {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()).filter(|s| !s.is_empty()) {
                    return Some(text.to_string());
                } else if let Some(inline) = part.get("inlineData").or_else(|| part.get("inline_data")) {
                    if let Some(data) = inline.get("data").and_then(|d| d.as_str()).filter(|s| !s.is_empty()) {
                        let mime = inline
                            .get("mimeType")
                            .or_else(|| inline.get("mime_type"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("image/png");
                        return Some(format!("\n\n![image](data:{};base64,{})\n\n", mime, data));
                    }
                }
            }
            None
        })
        .and_then(|content| serde_json::to_string(&create_openai_chunk(&content, model)).ok());

    // usage chunk（usageMetadata → OpenAI usage，含 reasoning / cached）
    let usage_chunk = v.get("usageMetadata").and_then(|meta| {
        let usage = crate::relay::usage_extractor::gemini_usage_metadata_to_openai(meta);
        serde_json::to_string(&serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "object": "chat.completion.chunk",
            "created": chrono::Utc::now().timestamp(),
            "model": model,
            "choices": [],
            "usage": usage
        }))
        .ok()
    });

    match (text_chunk, usage_chunk) {
        (Some(t), Some(u)) => Some(format!("{}\n\ndata: {}", t, u)),
        (Some(t), None) => Some(t),
        (None, Some(u)) => Some(u),
        (None, None) => None,
    }
}

fn create_openai_chunk(text: &str, model: &str) -> serde_json::Value {
    serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "object": "chat.completion.chunk",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {"role": "assistant", "content": text},
            "finish_reason": null
        }]
    })
}
