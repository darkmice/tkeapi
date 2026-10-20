/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! Google Gemini 官方多模态生图与聊天适配器。
//! 遵循 Google 原生 API 规范：https://ai.google.dev/api/generate-content?hl=zh-cn
//! 遵循“优先官方 generationConfig，否则从 OpenAI 标准参数转换”原则。

use super::super::media;
use crate::relay::usage_extractor;

/// Google Gemini 官方原生根级透传参数列表（官方原生优先，有则直接透传给上游）
const GEMINI_PASSTHROUGH_KEYS: &[&str] = &[
    "contents",
    "generationConfig",
    "generation_config",
    "systemInstruction",
    "system_instruction",
    "tools",
    "toolConfig",
    "tool_config",
    "safetySettings",
    "safety_settings",
    "cachedContent",
    "cached_content",
    "labels",
];

/// 官方 tools 缺失时的联网搜索开关映射
fn resolve_tools(body: &serde_json::Value, result: &mut serde_json::Map<String, serde_json::Value>) {
    if !result.contains_key("tools") {
        if body.get("web_search").and_then(|v| v.as_bool()).unwrap_or(false)
            || body.get("google_search").and_then(|v| v.as_bool()).unwrap_or(false)
            || body.get("google_image_search").and_then(|v| v.as_bool()).unwrap_or(false)
        {
            result.insert("tools".to_string(), serde_json::json!([{"google_search": {}}]));
        }
    }
}

/// 构建 Google Gemini 原生生图请求体 (target_type == "gemini_image")
pub(crate) async fn build_gemini_image_body(
    body: &serde_json::Value,
    http_client: Option<&reqwest::Client>,
) -> serde_json::Value {
    let mut result = serde_json::Map::new();

    // 1. 官方原生根级参数直接透传（白名单驱动，与 vidu.rs 完全一致）
    for &key in GEMINI_PASSTHROUGH_KEYS {
        if let Some(val) = body.get(key) {
            result.insert(key.to_string(), val.clone());
        }
    }

    // 2. generationConfig 处理：确保包含出图模态；官方配置优先，缺失时从 OpenAI 参数规范补充
    let is_snake_gen_cfg = result.contains_key("generation_config");
    let mut gen_config = result
        .remove("generationConfig")
        .or_else(|| result.remove("generation_config"))
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    if !gen_config.contains_key("responseModalities") && !gen_config.contains_key("response_modalities") {
        gen_config.insert(
            "responseModalities".to_string(),
            serde_json::json!(["IMAGE"]),
        );
    }
    if !gen_config.contains_key("candidateCount") && !gen_config.contains_key("candidate_count") {
        if let Some(n) = body.get("n").and_then(|v| v.as_i64()) {
            if n > 1 {
                gen_config.insert("candidateCount".to_string(), serde_json::json!(n));
            }
        }
    }
    if !gen_config.contains_key("seed") {
        if let Some(s) = body.get("seed") {
            gen_config.insert("seed".to_string(), s.clone());
        }
    }
    if !gen_config.contains_key("temperature") {
        if let Some(t) = body.get("temperature") {
            gen_config.insert("temperature".to_string(), t.clone());
        }
    }

    // 图片配置：用户已传 imageConfig / image_config 则原样透传；未传时从 ratio/size 规范生成 imageConfig
    if !gen_config.contains_key("imageConfig") && !gen_config.contains_key("image_config") {
        let mut img_cfg = serde_json::Map::new();
        let aspect_ratio = body
            .get("ratio")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("size").and_then(|v| v.as_str()).and_then(media::size_to_ratio));
        if let Some(ar) = aspect_ratio {
            img_cfg.insert("aspectRatio".to_string(), serde_json::json!(ar));
        }
        if let Some(res) = usage_extractor::extract_resolution(body) {
            img_cfg.insert(
                "imageSize".to_string(),
                serde_json::json!(res.to_ascii_uppercase()),
            );
        }
        if !img_cfg.is_empty() {
            gen_config.insert(
                "imageConfig".to_string(),
                serde_json::Value::Object(img_cfg),
            );
        }
    }
    let gen_config_key = if is_snake_gen_cfg { "generation_config" } else { "generationConfig" };
    result.insert(gen_config_key.to_string(), serde_json::Value::Object(gen_config));

    // 3. contents 组装：官方 contents 优先，缺省从 OpenAI prompt 与参考图构建
    if !result.contains_key("contents") {
        let all_urls = media::collect_image_urls(body, &["image", "image_urls"]);
        let resolved = media::resolve_image_urls(http_client, &all_urls).await;
        let mut parts: Vec<serde_json::Value> = Vec::new();

        if let Some(prompt) = body
            .get("prompt")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            parts.push(serde_json::json!({ "text": prompt }));
        }
        for v in resolved.into_iter().flatten() {
            parts.push(serde_json::json!({ "inline_data": v }));
        }

        result.insert(
            "contents".to_string(),
            serde_json::json!([{ "parts": parts, "role": "user" }]),
        );
    }

    // 4. 搜索工具兜底
    resolve_tools(body, &mut result);

    serde_json::Value::Object(result)
}

/// 构建 Google Gemini 原生聊天对话请求体 (target_type == "gemini")
pub(crate) fn build_gemini_chat_body(body: &serde_json::Value) -> serde_json::Value {
    let mut result = serde_json::Map::new();

    // 1. 官方原生根级参数直接透传（白名单驱动，与 vidu.rs 完全一致）
    for &key in GEMINI_PASSTHROUGH_KEYS {
        if let Some(val) = body.get(key) {
            result.insert(key.to_string(), val.clone());
        }
    }

    // 2. 仅在官方 contents 缺失时，从 OpenAI messages 组装并按需分离 systemInstruction
    if !result.contains_key("contents") {
        let mut contents = Vec::new();
        if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
            for msg in messages {
                let role = msg["role"].as_str().unwrap_or("user");
                let text = match &msg["content"] {
                    serde_json::Value::String(s) => s.clone(),
                    v if !v.is_null() => v.to_string(),
                    _ => continue,
                };
                if role == "system" {
                    if !result.contains_key("systemInstruction") && !result.contains_key("system_instruction") {
                        result.insert(
                            "systemInstruction".to_string(),
                            serde_json::json!({ "parts": [{"text": text}] }),
                        );
                    }
                } else {
                    let gemini_role = if role == "assistant" { "model" } else { "user" };
                    contents.push(serde_json::json!({
                        "role": gemini_role,
                        "parts": [{"text": text}]
                    }));
                }
            }
        }
        result.insert("contents".to_string(), serde_json::json!(contents));
    }

    // 3. 仅在官方 generationConfig 缺失时，从 OpenAI 标准参数组装
    if !result.contains_key("generationConfig") && !result.contains_key("generation_config") {
        let mut gen_config = serde_json::Map::new();
        if let Some(t) = body.get("temperature") { gen_config.insert("temperature".to_string(), t.clone()); }
        if let Some(t) = body.get("top_p") { gen_config.insert("topP".to_string(), t.clone()); }
        let max_tokens = body.get("max_tokens").or_else(|| body.get("max_completion_tokens"));
        if let Some(t) = max_tokens { gen_config.insert("maxOutputTokens".to_string(), t.clone()); }
        if let Some(s) = body.get("seed") { gen_config.insert("seed".to_string(), s.clone()); }
        if let Some(stop) = body.get("stop") {
            let arr = if let Some(s) = stop.as_str() { serde_json::json!([s]) } else { stop.clone() };
            gen_config.insert("stopSequences".to_string(), arr);
        }
        if !gen_config.is_empty() {
            result.insert("generationConfig".to_string(), serde_json::Value::Object(gen_config));
        }
    }

    // 4. 搜索工具兜底
    resolve_tools(body, &mut result);

    if let Some(opts) = body.get("stream_options") {
        result.insert("stream_options".to_string(), opts.clone());
    }

    serde_json::Value::Object(result)
}

/// Google Gemini Interactions API (`/v1beta/interactions` & `/v1/interactions`) 官方根级透传参数列表
const INTERACTIONS_PASSTHROUGH_KEYS: &[&str] = &[
    "input",
    "system_instruction",
    "tools",
    "response_format",
    "response_mime_type",
    "stream",
    "store",
    "background",
    "generation_config",
    "agent",
    "agent_config",
    "previous_interaction_id",
    "service_tier",
    "webhook_config",
];

/// 构建 Google Gemini Interactions API 请求体（直达 `/v1beta/interactions` 或 `/v1/interactions`）
pub(crate) async fn build_gemini_interactions_body(
    model: &str,
    body: &serde_json::Value,
    category: &str,
    http_client: Option<&reqwest::Client>,
) -> serde_json::Value {
    let mut result = serde_json::Map::new();

    // 1. 官方 Interactions 根级参数直接透传
    for &key in INTERACTIONS_PASSTHROUGH_KEYS {
        if let Some(val) = body.get(key) {
            result.insert(key.to_string(), val.clone());
        }
    }

    // 2. model 与 agent 互斥（CreateInteraction 规范：指定 model 或 agent 其一）
    let target_key = if result.contains_key("agent") { "agent" } else { "model" };
    result.insert(target_key.to_string(), serde_json::json!(model));

    // 3. input 组装：官方 input 优先，缺失时从 prompt/image 或 messages 转换为 Interactions 格式
    if !result.contains_key("input") {
        if category == "图片" || body.get("prompt").is_some() {
            let urls = media::collect_image_urls(body, &["image", "image_urls"]);
            let resolved = media::resolve_image_urls(http_client, &urls).await;
            let prompt = body.get("prompt").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
            let mut items: Vec<serde_json::Value> = resolved
                .into_iter()
                .flatten()
                .map(|mut v| {
                    v["type"] = serde_json::json!("image");
                    v
                })
                .collect();

            if items.is_empty() {
                if let Some(p) = prompt {
                    result.insert("input".to_string(), serde_json::json!(p));
                }
            } else {
                if let Some(p) = prompt {
                    items.insert(0, serde_json::json!({ "type": "text", "text": p }));
                }
                result.insert("input".to_string(), serde_json::Value::Array(items));
            }
        } else if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
            let mut turns = Vec::new();
            for msg in messages {
                let role = msg["role"].as_str().unwrap_or("user");
                let text = match &msg["content"] {
                    serde_json::Value::String(s) => s.clone(),
                    v if !v.is_null() => v.to_string(),
                    _ => continue,
                };
                if role == "system" {
                    result
                        .entry("system_instruction".to_string())
                        .or_insert_with(|| serde_json::json!(text));
                } else {
                    let turn_role = if role == "assistant" { "model" } else { "user" };
                    turns.push(serde_json::json!({ "role": turn_role, "content": text }));
                }
            }
            let input_val = if turns.len() == 1 && turns[0]["role"] == "user" {
                turns[0]["content"].take()
            } else {
                serde_json::Value::Array(turns)
            };
            result.insert("input".to_string(), input_val);
        }
    }

    // 4. 图片 response_format 补齐（统一使用 Interactions 规范格式）
    if category == "图片" && !result.contains_key("response_format") {
        let mut rf = serde_json::Map::new();
        if let Some(ar) = body.get("ratio").and_then(|v| v.as_str()).or_else(|| {
            body.get("size")
                .and_then(|v| v.as_str())
                .and_then(media::size_to_ratio)
        }) {
            rf.insert("aspect_ratio".to_string(), serde_json::json!(ar));
        }
        if let Some(res) = usage_extractor::extract_resolution(body) {
            rf.insert(
                "image_size".to_string(),
                serde_json::json!(res.to_ascii_uppercase()),
            );
        }
        if !rf.is_empty() {
            rf.insert("type".to_string(), serde_json::json!("image"));
            result.insert("response_format".to_string(), serde_json::Value::Object(rf));
        }
    }

    // 5. 搜索工具兜底（Interactions 规范为 [{"type": "google_search"}]）
    if !result.contains_key("tools")
        && ["web_search", "google_search"]
            .iter()
            .any(|k| body.get(*k).and_then(|v| v.as_bool()).unwrap_or(false))
    {
        result.insert(
            "tools".to_string(),
            serde_json::json!([{"type": "google_search"}]),
        );
    }

    serde_json::Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gemini_image_openai_params() {
        let body = serde_json::json!({
            "prompt": "Draw a banana astronaut",
            "size": "1024x1024",
            "n": 2,
            "seed": 9999,
            "temperature": 0.8
        });
        let res = build_gemini_image_body(&body, None).await;

        assert_eq!(res["contents"][0]["parts"][0]["text"], "Draw a banana astronaut");
        assert_eq!(res["generationConfig"]["responseModalities"][0], "IMAGE");
        assert_eq!(res["generationConfig"]["imageConfig"]["aspectRatio"], "1:1");
        assert_eq!(res["generationConfig"]["imageConfig"]["imageSize"], "1K");
        assert_eq!(res["generationConfig"]["candidateCount"], 2);
        assert_eq!(res["generationConfig"]["seed"], 9999);
        assert_eq!(res["generationConfig"]["temperature"], 0.8);
    }

    #[tokio::test]
    async fn test_gemini_image_user_image_config_passthrough() {
        // 用户显式自传 imageConfig，原样透传保留
        let body = serde_json::json!({
            "prompt": "High res photo",
            "generationConfig": {
                "imageConfig": {
                    "aspectRatio": "16:9",
                    "imageSize": "2K"
                },
                "seed": 1234
            }
        });
        let res = build_gemini_image_body(&body, None).await;

        assert_eq!(res["generationConfig"]["imageConfig"]["aspectRatio"], "16:9");
        assert_eq!(res["generationConfig"]["imageConfig"]["imageSize"], "2K");
        assert_eq!(res["generationConfig"]["seed"], 1234);
    }

    #[test]
    fn test_gemini_chat_conversion() {
        let body = serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are a helpful assistant"},
                {"role": "user", "content": "Hello"}
            ],
            "temperature": 0.7,
            "max_tokens": 100,
            "seed": 42,
            "stop": ["END"]
        });
        let res = build_gemini_chat_body(&body);

        assert_eq!(res["systemInstruction"]["parts"][0]["text"], "You are a helpful assistant");
        assert_eq!(res["contents"][0]["role"], "user");
        assert_eq!(res["contents"][0]["parts"][0]["text"], "Hello");
        assert_eq!(res["generationConfig"]["temperature"], 0.7);
        assert_eq!(res["generationConfig"]["maxOutputTokens"], 100);
        assert_eq!(res["generationConfig"]["seed"], 42);
        assert_eq!(res["generationConfig"]["stopSequences"], serde_json::json!(["END"]));
    }

    #[test]
    fn test_gemini_chat_with_max_completion_tokens() {
        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "Hi"}],
            "max_completion_tokens": 512
        });
        let res = build_gemini_chat_body(&body);
        assert_eq!(res["generationConfig"]["maxOutputTokens"], 512);
    }

    #[tokio::test]
    async fn test_gemini_passthrough_all_official_fields() {
        let official_body = serde_json::json!({
            "contents": [{"parts": [{"text": "Explain quantum computing"}], "role": "user"}],
            "generationConfig": {
                "temperature": 0.2,
                "topP": 0.95
            },
            "systemInstruction": {"parts": [{"text": "You are a physicist"}]},
            "tools": [{"google_search": {}}],
            "toolConfig": {
                "functionCallingConfig": {"mode": "AUTO"}
            },
            "safetySettings": [
                {"category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "BLOCK_LOW_AND_ABOVE"}
            ],
            "cachedContent": "cachedContents/sample123",
            "labels": {"environment": "production"}
        });

        // 验证聊天透传
        let chat_res = build_gemini_chat_body(&official_body);
        assert_eq!(chat_res["contents"][0]["parts"][0]["text"], "Explain quantum computing");
        assert_eq!(chat_res["generationConfig"]["temperature"], 0.2);
        assert_eq!(chat_res["systemInstruction"]["parts"][0]["text"], "You are a physicist");
        assert_eq!(chat_res["toolConfig"]["functionCallingConfig"]["mode"], "AUTO");
        assert_eq!(chat_res["safetySettings"][0]["category"], "HARM_CATEGORY_HATE_SPEECH");
        assert_eq!(chat_res["cachedContent"], "cachedContents/sample123");
        assert_eq!(chat_res["labels"]["environment"], "production");

        // 验证生图透传
        let img_res = build_gemini_image_body(&official_body, None).await;
        assert_eq!(img_res["generationConfig"]["responseModalities"][0], "IMAGE");
        assert_eq!(img_res["generationConfig"]["temperature"], 0.2);
        assert_eq!(img_res["toolConfig"]["functionCallingConfig"]["mode"], "AUTO");
        assert_eq!(img_res["safetySettings"][0]["category"], "HARM_CATEGORY_HATE_SPEECH");
        assert_eq!(img_res["cachedContent"], "cachedContents/sample123");
        assert_eq!(img_res["labels"]["environment"], "production");
    }

    #[tokio::test]
    async fn test_gemini_snake_case_passthrough() {
        let snake_body = serde_json::json!({
            "contents": {
                "role": "user",
                "parts": {
                    "text": "Generate a cheeseburger"
                }
            },
            "generation_config": {
                "response_modalities": ["TEXT", "IMAGE"],
                "image_config": {
                    "aspect_ratio": "16:9",
                    "image_size": "2K"
                },
                "thinking_config": {
                    "thinking_level": "MEDIUM",
                    "include_thoughts": true
                }
            },
            "system_instruction": {"parts": [{"text": "You are a master chef"}]},
            "tool_config": {"function_calling_config": {"mode": "AUTO"}},
            "safety_settings": [{"category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "BLOCK_NONE"}],
            "cached_content": "cachedContents/snake123"
        });

        // 验证生图透传
        let img_res = build_gemini_image_body(&snake_body, None).await;
        assert!(img_res.get("generationConfig").is_none());
        let gen_cfg = &img_res["generation_config"];
        assert_eq!(gen_cfg["response_modalities"][0], "TEXT");
        assert_eq!(gen_cfg["response_modalities"][1], "IMAGE");
        assert_eq!(gen_cfg["image_config"]["aspect_ratio"], "16:9");
        assert_eq!(gen_cfg["image_config"]["image_size"], "2K");
        assert_eq!(gen_cfg["thinking_config"]["thinking_level"], "MEDIUM");
        assert_eq!(gen_cfg["thinking_config"]["include_thoughts"], true);
        assert_eq!(img_res["contents"]["parts"]["text"], "Generate a cheeseburger");
        assert_eq!(img_res["cached_content"], "cachedContents/snake123");

        // 验证聊天透传
        let chat_res = build_gemini_chat_body(&snake_body);
        assert!(chat_res.get("generationConfig").is_none());
        assert_eq!(chat_res["generation_config"]["thinking_config"]["thinking_level"], "MEDIUM");
        assert_eq!(chat_res["system_instruction"]["parts"][0]["text"], "You are a master chef");
        assert_eq!(chat_res["cached_content"], "cachedContents/snake123");
    }

    #[tokio::test]
    async fn test_gemini_interactions_body_passthrough_and_conversion() {
        // 1. 原生 Interactions 生图透传
        let native_img = serde_json::json!({
            "model": "gemini-3.1-flash-image-preview",
            "input": "A futuristic city at sunset",
            "response_format": {
                "type": "image",
                "aspect_ratio": "16:9",
                "image_size": "2K"
            }
        });
        let res = build_gemini_interactions_body(
            "gemini-3.1-flash-image-preview",
            &native_img,
            "图片",
            None,
        )
        .await;
        assert!(res.get("contents").is_none());
        assert!(res.get("generation_config").is_none());
        assert_eq!(res["model"], "gemini-3.1-flash-image-preview");
        assert_eq!(res["input"], "A futuristic city at sunset");
        assert_eq!(res["response_format"]["type"], "image");
        assert_eq!(res["response_format"]["aspect_ratio"], "16:9");
        assert_eq!(res["response_format"]["image_size"], "2K");

        // 2. OpenAI 兼容参数走 Interactions 生图
        let compat_img = serde_json::json!({
            "prompt": "A cute cat",
            "size": "1024x1024",
            "resolution": "4k"
        });
        let res2 = build_gemini_interactions_body(
            "gemini-3.1-flash-image-preview",
            &compat_img,
            "图片",
            None,
        )
        .await;
        assert_eq!(res2["input"], "A cute cat");
        assert!(res2.get("response_modalities").is_none());
        assert!(res2.get("generation_config").is_none());
        assert_eq!(res2["response_format"]["type"], "image");
        assert_eq!(res2["response_format"]["aspect_ratio"], "1:1");
        assert_eq!(res2["response_format"]["image_size"], "4K");

        // 3. 原生 Interactions 聊天（含 thinking_level 与 previous_interaction_id）
        let native_chat = serde_json::json!({
            "model": "gemini-3-flash-preview",
            "input": "Hello!",
            "previous_interaction_id": "v1_abc123",
            "generation_config": {
                "thinking_level": "medium",
                "thinking_summaries": "auto"
            }
        });
        let res3 = build_gemini_interactions_body(
            "gemini-3-flash-preview",
            &native_chat,
            "聊天",
            None,
        )
        .await;
        assert!(res3.get("contents").is_none());
        assert_eq!(res3["input"], "Hello!");
        assert_eq!(res3["previous_interaction_id"], "v1_abc123");
        assert_eq!(res3["generation_config"]["thinking_level"], "medium");
    }
}
