/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use super::media;
use super::vendors;
use super::ResolvedForward;
// ── 请求体转换 ─────────────────────────────────────────────────

/// 将 OpenAI 格式请求体转换为目标上游格式。
///
/// 数据驱动映射：
/// - dashscope（视频）: prompt → input.prompt + parameters 格式
/// - volcengine（图片/视频）: prompt → content[{type:"text",text:...}]
/// - volcengine_chat / volcengine（聊天）: 保持 OpenAI 格式（火山兼容）
/// - gemini / gemini_image: messages/prompt → contents[{parts:[{text:...}]}]
/// - anthropic: messages → Anthropic Messages 格式
/// - openai / 其他: 直接透传
pub async fn transform_request_body(
    resolved: &ResolvedForward,
    model: &str,
    body: &serde_json::Value,
    category: &str,
    // billing_rule: 模型关联的计费规则实体上下文，
    // 通过判断 billing_type 是否为 "tokens" 决定是否跳过视频/图片的 resolution/duration 注入；
    // 通过判断 billing_rule 是否为 "image_resolution" 决定图片是否启用分辨率自适应提取与兜底
    billing_rule: Option<&crate::models::BillingRule>,
    // http_client: 图片模型用于下载 HTTP 图片 URL 转 base64
    http_client: Option<&reqwest::Client>,
) -> serde_json::Value {
    let mut result = match resolved.target_type.as_str() {
        // 火山引擎 AI MediaKit 画质增强与字幕擦除：重构并生成符合火山官方 API 规范的请求体
        "volcengine_media_enhance" => {
            #[cfg(feature = "plugin_volcengine_enhance")]
            {
                let match_key = resolved.mid.as_deref().unwrap_or("");
                crate::api::plugins::build_volcengine_media_enhance_body(match_key, body)
            }
            #[cfg(not(feature = "plugin_volcengine_enhance"))]
            {
                body.clone()
            }
        }

        // Bytefor 视频生成：将 OpenAI 兼容格式转换为 Bytefor 视频生成 API 格式
        "bytefor_video" => vendors::build_bytefor_video_body(model, body),

        // Vidu 官方视频生成：支持文生/首帧图生/首尾帧/多参考生视频
        "vidu" => vendors::build_vidu_video_body(model, body),

        // ATP Token 视频生成：将 OpenAI / 阿里百炼 格式请求转换为 ATP Token omni media tasks 格式
        // 参考文档：https://atptoken.ai/zh-cn/docs/media-video
        "atp_video" => vendors::build_atp_video_body(model, body),

        // 可灵 AI 图片生成
        "kling" => vendors::build_kling_body(model, body, category, &resolved.upstream_path),
        // 可灵 3.0 推荐视频：URL 含模型，body 为 contents/settings/options
        "kling_video" => vendors::build_kling_v3_body(body, &resolved.upstream_path),

        // 阿里百炼 DashScope 视频：OpenAI → input/parameters 格式
        // 参考文档：https://help.aliyun.com/zh/model-studio/text-to-video-api-reference
        "dashscope" => vendors::build_dashscope_video_body(model, body),

        // 火山方舟图片（/api/v3/images/generations）: 保持 OpenAI 兼容格式
        // 参考 Seedream 5.0 API: https://www.volcengine.com/docs/82379/1541523
        "volcengine_image" => {
            let mut fwd = body.clone();
            fwd["model"] = serde_json::json!(model);

            // 优化与精简：当 size 不存在时取 resolution 参数，若两者均不存在则兜底为 "2k" 并赋值给 size；处理完毕后删除 resolution 字段，防止冗余或非法参数传到火山上游
            if fwd.get("size").is_none() {
                let size_val = fwd
                    .get("resolution")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("2k")
                    .to_string();
                fwd["size"] = serde_json::json!(size_val);
            }
            if let Some(obj) = fwd.as_object_mut() {
                obj.remove("resolution");
            }

            // 火山方舟 image 原生支持字符串和数组，用 media::collect_image_urls 统一收集
            let img_urls = media::collect_image_urls(body, &["image", "image_urls"]);
            if let Some(obj) = fwd.as_object_mut() {
                obj.remove("image");
                obj.remove("image_urls");
                match img_urls.len() {
                    0 => {}
                    1 => {
                        obj.insert("image".to_string(), serde_json::json!(&img_urls[0]));
                    }
                    _ => {
                        obj.insert("image".to_string(), serde_json::json!(img_urls));
                    }
                }
            }

            // n > 1 → 启用组图: sequential_image_generation = "auto"
            let n = body
                .get("n")
                .and_then(|v| {
                    v.as_i64()
                        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                })
                .unwrap_or(1)
                .clamp(1, 15);
            if n > 1 {
                fwd["sequential_image_generation"] = serde_json::json!("auto");
                fwd["sequential_image_generation_options"] = serde_json::json!({
                    "max_images": n
                });
            }
            // n 已转换为官方参数，删除避免冗余传到上游
            if let Some(obj) = fwd.as_object_mut() {
                obj.remove("n");
            }
            // watermark 直接透传（火山方舟原生支持，默认 true）
            fwd
        }

        // 阿里百炼图像生成: prompt → input.prompt
        "dashscope_image" => vendors::build_dashscope_image_body(model, body),

        // 火山方舟视频（/api/v3/contents/generations/tasks）: prompt → content 格式
        // 参考火山引擎 Seedance 2.0 官方 API：https://www.volcengine.com/docs/82379/1520757
        "volcengine" => vendors::build_volcengine_content_body(model, body),

        // MiniMax 视频生成（/v2/video_generation）
        "minimax_video" => vendors::build_minimax_video_body(model, body),

        // fal.ai 队列视频（路径如 /minimax/${model}/reference-to-video）
        "fal_video" => vendors::build_fal_video_body(model, body),

        // MiniMax 图片生成（/v1/image_generation）：文生图 + 图生图
        // 参考：https://platform.minimaxi.com/docs/api-reference/image-generation-t2i
        //       https://platform.minimaxi.com/docs/api-reference/image-generation-i2i
        "minimax_image" => vendors::build_minimax_image_body(model, body),

        // 火山方舟聊天：保持 OpenAI 格式（火山完全兼容 OpenAI）
        "volcengine_chat" => {
            let mut fwd = body.clone();
            fwd["model"] = serde_json::json!(model);
            fwd
        }

        // Gemini 图片与聊天：若上游路由为 /v1(beta)/interactions 则构建 Interactions 请求体，否则构建标准 Models contents 请求体
        "gemini" | "gemini_image"
            if super::is_google_interactions_path(&resolved.upstream_path) =>
        {
            vendors::build_gemini_interactions_body(model, body, category, http_client).await
        }
        "gemini_image" => vendors::build_gemini_image_body(body, http_client).await,
        "gemini" => vendors::build_gemini_chat_body(body),

        // Anthropic 聊天：OpenAI messages → Anthropic 格式
        // system 优先级：请求体顶层 system 参数 > messages 中 role=system 的内容
        "anthropic" => {
            let mut messages = Vec::new();
            let mut system_from_messages: Option<serde_json::Value> = None;

            if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
                for msg in msgs {
                    let role = msg["role"].as_str().unwrap_or("user");
                    if role == "system" {
                        // 仅在顶层 system 未提供时，才从 messages 中提取
                        if body.get("system").is_none() {
                            let content = &msg["content"];
                            system_from_messages = match content {
                                serde_json::Value::String(s) => Some(serde_json::json!(s)),
                                v if !v.is_null() => Some(v.clone()),
                                _ => None,
                            };
                        }
                    } else {
                        // user/assistant 消息：保留原始 content 结构（字符串或多模态数组）
                        messages.push(
                            serde_json::json!({"role": role, "content": msg["content"].clone()}),
                        );
                    }
                }
            }

            let mut result = serde_json::json!({
                "model": model,
                "messages": messages,
                "max_tokens": body.get("max_tokens").and_then(|v| v.as_i64()).unwrap_or(4096),
            });

            // system 参数：顶层优先（支持字符串和数组格式），其次 messages 中提取
            if let Some(sys) = body.get("system") {
                result["system"] = sys.clone();
            } else if let Some(sys) = system_from_messages {
                result["system"] = sys;
            }

            // 透传 Anthropic 原生参数
            for key in &[
                "temperature",
                "top_p",
                "top_k",
                "stream",
                "stop_sequences",
                "metadata",
                "tools",
                "tool_choice",
                "thinking",
                "reasoning_effort",
            ] {
                if let Some(v) = body.get(*key) {
                    result[*key] = v.clone();
                }
            }
            result
        }

        // 腾讯云 VOD AIGC / MPS AI 换装：独立构建请求体
        "tencent_vod_image" => vendors::build_tencent_vod_image_body(model, body),
        "tencent_vod_video" => vendors::build_tencent_vod_video_body(model, body),
        "tencent_mps_image" => vendors::build_tencent_mps_tryon_body(model, body),
        "tencent_mps_create_image" => vendors::build_tencent_mps_create_image_body(model, body),
        // 腾讯云 TokenHub 混元生图 3.0 / 3.5 预览版
        "tencent_tokenhub_image" => vendors::build_tokenhub_image_v3_body(model, body),
        "tencent_tokenhub_v35_image" => vendors::build_tokenhub_image_v35_body(model, body),

        // 即梦AI（火山引擎 CV 视觉服务）：OpenAI → 即梦格式
        "jimeng_image" => vendors::build_jimeng_image_body(model, body),
        "jimeng_video" => vendors::build_jimeng_video_body(model, body),

        // GPT 官方图片：image/image_urls → GPT 官方 images 数组
        // 参考文档：https://developers.openai.com/api/reference/resources/images/methods/edit
        // edits 端点要求 images 数组，元素格式: { image_url: "data:...;base64,..." } 或 { image_url: "https://..." }
        "gpt" if category == "图片" => {
            let mut fwd = body.clone();
            fwd["model"] = serde_json::json!(model);
            // 提取各种图片参数中的图片地址（自动去重），不下载转 base64，直接统一构建为 images 数组直传
            let all_urls =
                media::collect_image_urls(body, &["image", "image_urls", "images", "image[]"]);
            let entries: Vec<serde_json::Value> = all_urls
                .iter()
                .filter(|url| !url.is_empty())
                .map(|url| serde_json::json!({ "image_url": url }))
                .collect();
            if let Some(obj) = fwd.as_object_mut() {
                obj.remove("image");
                obj.remove("images");
                obj.remove("image_urls");
                obj.remove("image[]");
                if !entries.is_empty() {
                    obj.insert("images".to_string(), serde_json::json!(entries));
                }
            }
            fwd
        }

        // 火山方舟语音合成 TTS V3 SSE：OpenAI /v1/audio/speech → 火山 V3 SSE 格式
        // 官方请求体: { user, req_params: { text, speaker, audio_params: { format, sample_rate }, mix_speaker? } }
        // 参考文档：https://www.volcengine.com/docs/6561/1598757
        "volcengine_tts" => {
            // 官方格式优先：如果已包含 req_params，直接使用（透传原生参数）
            if body.get("req_params").is_some() {
                let mut fwd = body.clone();
                // 确保 user.uid 存在
                if fwd.get("user").is_none() {
                    fwd["user"] = serde_json::json!({ "uid": "tokensbyte" });
                }
                return fwd;
            }

            // OpenAI 格式转换：input/voice/response_format/speed → req_params
            let text = body.get("input").and_then(|v| v.as_str()).unwrap_or("");
            let speaker = body.get("voice").and_then(|v| v.as_str()).unwrap_or("");
            let format = body
                .get("response_format")
                .and_then(|v| v.as_str())
                .unwrap_or("mp3");
            let sample_rate = body
                .get("sample_rate")
                .and_then(|v| v.as_i64())
                .unwrap_or(24000);
            let speed = body
                .get("speed")
                .filter(|v| v.is_number())
                .cloned()
                .unwrap_or_else(|| serde_json::json!(0));

            let mut req_params = serde_json::json!({
                "text": text,
                "speaker": speaker,
                "audio_params": {
                    "format": format,
                    "sample_rate": sample_rate,
                    "speech_rate": speed
                }
            });
            // 透传混音配置（mix_speaker）
            if let Some(mix) = body.get("mix_speaker") {
                req_params["mix_speaker"] = mix.clone();
            }

            serde_json::json!({
                "user": { "uid": "tokensbyte" },
                "req_params": req_params
            })
        }

        _ => {
            let mut fwd = body.clone();
            fwd["model"] = serde_json::json!(model);
            fwd
        }
    };

    // 视频模型默认参数兜底（确保上游数据与计费一致）
    // 跳过条件：仅对标准的 OpenAI 兼容渠道注入，其他已定制渠道在各自构建函数中管理，无需在此注入以防报错
    let is_token_billing = billing_rule.as_ref().map(|r| r.billing_type.as_str()) == Some("tokens");
    if category == "视频"
        && !is_token_billing
        && matches!(resolved.target_type.as_str(), "openai" | "apimart")
    {
        if result.get("resolution").is_none() {
            result["resolution"] = serde_json::json!("720p");
        }
        if result.get("duration").is_none() {
            result["duration"] = serde_json::json!(5);
        }
    }

    // APIMart 渠道特判：当入参包含 size 且为 k 结尾，且包含 ratio 时，将 ratio 的值赋给 size 并删除 ratio
    if resolved.target_type == "apimart" {
        if let Some(size_val) = result.get("size").and_then(|v| v.as_str()) {
            let size_lower = size_val.trim().to_lowercase();
            if size_lower.ends_with('k') && result.get("ratio").is_some() {
                if let Some(ratio_val) = result.get("ratio").cloned() {
                    result["size"] = ratio_val;
                    if let Some(obj) = result.as_object_mut() {
                        obj.remove("ratio");
                    }
                }
            }
        }
    }

    // 统一后处理：web_search 联网搜索参数转换
    convert_web_search(&mut result, body, &resolved.target_type);

    // 统一后处理：对所有聊天模型的流式请求，设置 stream_options.include_usage = true
    // 确保流式聊天时能获取到 token 使用量进行计费
    if category == "聊天" {
        // 检查是否是流式请求
        let is_stream = body
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Responses 协议（含 input 字段且无 messages）流式在 response.done 原生自带 usage，无需亦不可注入 Chat 专属 stream_options
        let is_responses = body.get("input").is_some() && body.get("messages").is_none();

        if is_stream && !is_responses {
            // 明确白名单：对所有使用 OpenAI 格式的模型（含 target_type 为空/未设置时默认 OpenAI 兼容）应用此设置
            let is_openai_format = resolved.target_type.trim().is_empty()
                || matches!(resolved.target_type.as_str(), "openai" | "volcengine_chat");

            if is_openai_format {
                if let Some(obj) = result.as_object_mut() {
                    if let Some(options) = obj
                        .get_mut("stream_options")
                        .and_then(|v| v.as_object_mut())
                    {
                        // 如果已有 stream_options，确保 include_usage = true
                        options.insert("include_usage".to_string(), serde_json::json!(true));
                    } else {
                        // 否则，添加 stream_options
                        obj.insert(
                            "stream_options".to_string(),
                            serde_json::json!({ "include_usage": true }),
                        );
                    }
                }
            }
        }
    }

    if resolved.content_to_prompt {
        apply_content_to_prompt(&mut result);
    }

    result
}

/// 将 OpenAI 风格的 `web_search: true` 转换为目标平台的联网搜索参数。
/// 火山方舟统一使用 `tools: [{"type": "web_search"}]` 格式。
fn convert_web_search(
    result: &mut serde_json::Value,
    original: &serde_json::Value,
    target_type: &str,
) {
    if !original
        .get("web_search")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return;
    }
    match target_type {
        "volcengine" | "volcengine_image" => {
            result["tools"] = serde_json::json!([{"type": "web_search"}]);
            if let Some(obj) = result.as_object_mut() {
                obj.remove("web_search");
            }
        }
        "gemini" | "gemini_image" => {
            if result.get("tools").is_none() {
                result["tools"] = serde_json::json!([{"google_search": {}}]);
            }
            if let Some(obj) = result.as_object_mut() {
                obj.remove("web_search");
            }
        }
        _ => {}
    }
}
// ── 统一后处理 content_to_prompt 提取与覆盖辅助 ──

/// 对最终拼装的请求体进行 inplace 修改，当没有定义 prompt 且包含 content 时提取 content 文本字段赋值给 prompt
fn apply_content_to_prompt(result: &mut serde_json::Value) {
    if result.get("prompt").is_none() {
        if let Some(content) = result.get("content") {
            let text_opt = match content {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Array(arr) => arr
                    .iter()
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .next()
                    .map(|t| t.to_string()),
                _ => None,
            };
            if let Some(text) = text_opt {
                result["prompt"] = serde_json::json!(text);
            }
        }
    }
}
