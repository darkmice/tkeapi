/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use super::super::media;
// ── 阿里百炼 DashScope 图像请求体构建器 ─────────────────────────
//
// 将 OpenAI 风格的参数转换为 DashScope /api/v1/services/aigc/text2image/image-synthesis 格式。
// 参考文档：https://help.aliyun.com/zh/model-studio/user-guide/wanx-v2-text-to-image-api-reference

pub(crate) fn build_dashscope_image_body(
    model: &str,
    body: &serde_json::Value,
) -> serde_json::Value {
    // ── 直通模式 ──
    if body.get("input").is_some() {
        let mut fwd = body.clone();
        fwd["model"] = serde_json::json!(model);
        return fwd;
    }

    // ── 转换模式 ──
    let mut input = serde_json::Map::new();

    // 优先处理 messages (支持多模态及万相 2.7/千问 2.0 格式)
    if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
        let mut dashscope_msgs = Vec::new();
        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let mut dash_content = Vec::new();

            if let Some(content) = msg.get("content") {
                if let Some(text) = content.as_str() {
                    dash_content.push(serde_json::json!({ "text": text }));
                } else if let Some(arr) = content.as_array() {
                    for item in arr {
                        if let Some(t) = item.get("type").and_then(|v| v.as_str()) {
                            if t == "text" {
                                if let Some(txt) = item.get("text").and_then(|v| v.as_str()) {
                                    dash_content.push(serde_json::json!({ "text": txt }));
                                }
                            } else if t == "image_url" {
                                if let Some(url) = item
                                    .get("image_url")
                                    .and_then(|v| v.get("url"))
                                    .and_then(|v| v.as_str())
                                {
                                    dash_content.push(serde_json::json!({ "image": url }));
                                }
                            }
                        } else if let Some(_url) = item.get("image").and_then(|v| v.as_str()) {
                            // 兼容阿里原生传入的 {"image": "..."}
                            dash_content.push(item.clone());
                        } else if let Some(_txt) = item.get("text").and_then(|v| v.as_str()) {
                            dash_content.push(item.clone());
                        }
                    }
                }
            }
            dashscope_msgs.push(serde_json::json!({
                "role": role,
                "content": dash_content
            }));
        }
        input.insert("messages".to_string(), serde_json::json!(dashscope_msgs));
    } else {
        // 强制包装为 messages 结构，不使用快捷 prompt 字段
        let mut dash_content = Vec::new();

        // 支持顶层 image / image_urls 参数，确保图片先于文本内容
        for url in &media::collect_image_urls(body, &["image", "image_urls"]) {
            dash_content.push(serde_json::json!({ "image": url }));
        }

        if let Some(prompt) = body
            .get("prompt")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            dash_content.push(serde_json::json!({ "text": prompt }));
        }

        input.insert(
            "messages".to_string(),
            serde_json::json!([
                {
                    "role": "user",
                    "content": dash_content
                }
            ]),
        );
    }

    let mut params = serde_json::Map::new();

    // negative_prompt 移入 parameters
    if let Some(np) = body.get("negative_prompt").and_then(|v| v.as_str()) {
        params.insert("negative_prompt".to_string(), serde_json::json!(np));
    }

    // n -> n
    if let Some(n) = body.get("n").and_then(|v| v.as_i64()) {
        params.insert("n".to_string(), serde_json::json!(n));
    }

    // size -> size (1024x1024 -> 1024*1024)
    if let Some(size) = body.get("size").and_then(|v| v.as_str()) {
        params.insert(
            "size".to_string(),
            serde_json::json!(size.replace("x", "*")),
        );
    }

    // style/quality/prompt_extend/watermark
    let passthrough = ["style", "quality", "prompt_extend", "watermark", "seed"];
    for &key in &passthrough {
        if let Some(val) = body.get(key) {
            params.insert(key.to_string(), val.clone());
        }
    }

    serde_json::json!({
        "model": model,
        "input": input,
        "parameters": params
    })
}

// ── 阿里百炼 DashScope 视频请求体构建器 ─────────────────────────
//
// 将 OpenAI 风格的扁平参数转换为 DashScope /api/v1/services/aigc/video-generation/video-synthesis 格式。
// 参考文档：https://help.aliyun.com/zh/model-studio/text-to-video-api-reference
//
// DashScope 请求格式：
//   { "model": "...", "input": { "prompt": "...", "media": [...] }, "parameters": { "resolution": "720P", ... } }
// media.type：first_frame / reference_image / reference_video（r2v）/ video（videoedit 等）

/// DashScope parameters 内的合法参数白名单
const DASHSCOPE_PARAM_KEYS: &[&str] = &[
    "resolution",
    "ratio",
    "duration",
    "prompt_extend",
    "watermark",
    "seed",
    "audio", // wan3.0：输出是否含音轨
];

/// 构建阿里百炼 DashScope 视频生成请求体
pub(crate) fn build_dashscope_video_body(
    model: &str,
    body: &serde_json::Value,
) -> serde_json::Value {
    // 已有 input：官方 DashScope 体直通，仅替换 model，不改用户参数
    if body.get("input").is_some() {
        let mut fwd = body.clone();
        fwd["model"] = serde_json::json!(model);
        return fwd;
    }

    // OpenAI 扁平参数 → input/parameters
    let prompt = body
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut input = serde_json::json!({ "prompt": prompt });
    if let Some(np) = body.get("negative_prompt").and_then(|v| v.as_str()) {
        input["negative_prompt"] = serde_json::json!(np);
    }
    // audio_url：官方字段直接透传
    if let Some(au) = body.get("audio_url").and_then(|v| v.as_str()) {
        input["audio_url"] = serde_json::json!(au);
    }

    // ── media 组装 ──────────────────────────────────────────────
    // 优先级：body.media（已是官方格式，直透）> OpenAI 扁平字段
    if let Some(media) = body.get("media").filter(|v| v.is_array()) {
        input["media"] = media.clone();
    } else {
        // 快乐小马（happyhorse）模型：视频 type 使用 "video"；其他模型统一使用 "reference_video"
        let is_happyhorse = model.to_ascii_lowercase().contains("happyhorse");
        let video_type = if is_happyhorse {
            "video"
        } else {
            "reference_video"
        };
        let mut media: Vec<serde_json::Value> = Vec::new();

        // 图片 → first_frame / last_frame / reference_image（按数量推断）
        let arr_images = media::collect_media_values(body, &["images", "image_urls"]);
        if !arr_images.is_empty() {
            let defaults = media::infer_image_default_roles(arr_images.len());
            for (i, item) in arr_images.iter().enumerate() {
                let default_role = defaults.get(i).copied().unwrap_or("reference_image");
                let (url, role) = media::parse_media_item(item, default_role);
                if let Some(u) = url.filter(|u| !u.is_empty()) {
                    media.push(serde_json::json!({ "type": role, "url": u }));
                }
            }
        }
        // 视频 → reference_video（其他模型）/ video（happyhorse）
        for item in &media::collect_media_values(body, &["videos"]) {
            let (url, role) = media::parse_media_item(item, video_type);
            if let Some(u) = url.filter(|u| !u.is_empty()) {
                media.push(serde_json::json!({ "type": role, "url": u }));
            }
        }

        // 音频 → reference_audio
        for item in &media::collect_media_values(body, &["audios"]) {
            let (url, role) = media::parse_media_item(item, "reference_audio");
            if let Some(u) = url.filter(|u| !u.is_empty()) {
                media.push(serde_json::json!({ "type": role, "url": u }));
            }
        }

        // 文件 → file（通用，模型自动识别意图）
        for item in &media::collect_media_values(body, &["files"]) {
            let (url, role) = media::parse_media_item(item, "file");
            if let Some(u) = url.filter(|u| !u.is_empty()) {
                media.push(serde_json::json!({ "type": role, "url": u }));
            }
        }

        // 网页链接 → webpage
        for item in &media::collect_media_values(body, &["links"]) {
            let (url, role) = media::parse_media_item(item, "link");
            if let Some(u) = url.filter(|u| !u.is_empty()) {
                media.push(serde_json::json!({ "type": role, "url": u }));
            }
        }

        if !media.is_empty() {
            input["media"] = serde_json::json!(media);
        }
    }

    let mut params = serde_json::Map::new();
    for key in DASHSCOPE_PARAM_KEYS {
        if let Some(v) = body.get(*key) {
            params.insert(key.to_string(), v.clone());
        }
    }
    // size → resolution；统一大写（DashScope 接受 720P/1080P）
    if !params.contains_key("resolution") {
        if let Some(size) = body.get("size").and_then(|v| v.as_str()) {
            params.insert(
                "resolution".to_string(),
                serde_json::json!(size.to_uppercase()),
            );
        }
    }
    params
        .entry("resolution".to_string())
        .or_insert(serde_json::json!("720P"));
    params
        .entry("duration".to_string())
        .or_insert(serde_json::json!(5));
    // generate_audio → parameters.audio（params 中无 audio 时生效，非 bool 强转）
    if !params.contains_key("audio") {
        if let Some(ga) = body.get("generate_audio") {
            let enabled = media::json_truthy(ga);
            params.insert("audio".to_string(), serde_json::json!(enabled));
        }
    }

    let mut result = serde_json::json!({ "model": model, "input": input });
    if !params.is_empty() {
        result["parameters"] = serde_json::Value::Object(params);
    }
    result
}

// ── ATP Token 视频请求体构建器 ─────────────────────────────────
//
// 将 OpenAI 扁平参数 或 阿里百炼 DashScope input/parameters 参数
// 转换为 ATP Token /omni/media/v1/contents/generations/tasks 所需格式。
// 参考文档：https://atptoken.ai/zh-cn/docs/media-video
//
// 兼容策略（三级）：
//   Level 3 — body 已有符合规范的 content[] → 直通
//   Level 2 — DashScope input/parameters 格式 → 提取标准字段
//   Level 1 — OpenAI 扁平格式 → 按字段语义映射

/// 构建 ATP Token 视频生成 API 请求体（omni media task 格式）
/// 兼容 OpenAI 和 阿里百炼 DashScope，复用已有公共媒体工具函数
/// 参考文档：https://atptoken.ai/zh-cn/docs/media-video
pub(crate) fn build_atp_video_body(model: &str, body: &serde_json::Value) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    result.insert("model".to_string(), serde_json::json!(model));

    // 检测 DashScope 格式（body 含 input/parameters 字段）
    let ds_input = body.get("input").and_then(|v| v.as_object());
    let ds_params = body.get("parameters").and_then(|v| v.as_object());

    // ── 构建 content[] ──
    let content_items: Vec<serde_json::Value> =
        if let Some(arr) = body.get("content").and_then(|v| v.as_array()) {
            // Level 3：已有 content 数组直通（ATP 原生格式）
            arr.iter().filter(|v| v.is_object()).cloned().collect()
        } else {
            let mut items = Vec::new();

            // prompt：DashScope: input.prompt；OpenAI: 顶层 prompt
            let prompt = ds_input
                .and_then(|i| i.get("prompt"))
                .or_else(|| body.get("prompt"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // 文本 prompt
            if !prompt.is_empty() {
                items.push(serde_json::json!({ "type": "text", "text": prompt }));
            }

            // DashScope 格式：input 和 input.media[] 同时存在才走此路径
            // OpenAI 格式：顶层 images/image_urls/video_url 字段
            if let Some(media_arr) = ds_input
                .and_then(|i| i.get("media"))
                .and_then(|v| v.as_array())
            {
                // DashScope input.media = [{type, url}, ...]
                // type：first_frame / last_frame / reference_image / video / reference_video
                for item in media_arr {
                    let media_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    if url.is_empty() {
                        continue;
                    }
                    match media_type {
                        "first_frame" | "last_frame" | "reference_image" => {
                            items.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": { "url": url },
                                "role": media_type
                            }));
                        }
                        "video" | "reference_video" => {
                            items.push(serde_json::json!({
                                "type": "video_url",
                                "video_url": { "url": url }
                            }));
                        }
                        _ => items.push(item.clone()),
                    }
                }
            } else {
                // OpenAI 格式：图片（images / image_urls），按数量推断 role
                let arr_images = media::collect_media_values(body, &["images", "image_urls"]);
                if !arr_images.is_empty() {
                    let defaults = media::infer_image_default_roles(arr_images.len());
                    for (idx, item) in arr_images.iter().enumerate() {
                        let default_role = defaults.get(idx).copied().unwrap_or("reference_image");
                        let (url, role) = media::parse_media_item(item, default_role);
                        if let Some(u) = url.filter(|u| !u.is_empty()) {
                            items.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": { "url": u },
                                "role": role
                            }));
                        }
                    }
                }

                // OpenAI 格式：视频（videos），不带 role
                for item in &media::collect_media_values(body, &["videos"]) {
                    let (url, _) = media::parse_media_item(item, "");
                    if let Some(u) = url.filter(|u| !u.is_empty()) {
                        items.push(
                            serde_json::json!({ "type": "video_url", "video_url": { "url": u } }),
                        );
                    }
                }
            }

            items
        };

    result.insert("content".to_string(), serde_json::json!(content_items));

    // ── 分辨率与比例（原值透传） ──
    if let Some(res) = ds_params
        .and_then(|p| p.get("resolution"))
        .or_else(|| body.get("resolution"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        result.insert("resolution".to_string(), serde_json::json!(res));
    }
    if let Some(rat) = ds_params
        .and_then(|p| p.get("ratio").or_else(|| p.get("aspect_ratio")))
        .or_else(|| body.get("ratio"))
        .or_else(|| body.get("aspect_ratio"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        result.insert("ratio".to_string(), serde_json::json!(rat));
    }

    // ── 控场参数（duration 必须存在，缺省兜底 5 保证计费正常） ──
    let duration = ds_params
        .and_then(|p| p.get("duration"))
        .or_else(|| body.get("duration"))
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        .unwrap_or(5);
    result.insert("duration".to_string(), serde_json::json!(duration));

    if let Some(wm) = ds_params
        .and_then(|p| p.get("watermark"))
        .or_else(|| body.get("watermark"))
        .and_then(|v| v.as_bool())
    {
        result.insert("watermark".to_string(), serde_json::json!(wm));
    }
    if let Some(s) = ds_params
        .and_then(|p| p.get("seed"))
        .or_else(|| body.get("seed"))
        .and_then(|v| v.as_i64())
    {
        result.insert("seed".to_string(), serde_json::json!(s));
    }
    if let Some(ga) = ds_params
        .and_then(|p| p.get("generate_audio"))
        .or_else(|| body.get("generate_audio"))
        .and_then(|v| v.as_bool())
    {
        result.insert("generate_audio".to_string(), serde_json::json!(ga));
    }

    serde_json::Value::Object(result)
}

// ── 火山方舟视频/图片请求体构建器 ──────────────────────────────
//

// 将 OpenAI 风格的扁平参数转换为火山方舟 /api/v3/contents/generations/tasks 所需的结构化格式。
// 参考文档：https://www.volcengine.com/docs/82379/1520757
//
// 设计原则（三级兼容）：
//   Level 3 — body 中已有 content 数组 → 原样直通，仅替换 model
//   Level 2 — images/videos/audios 元素为 {url, role} 对象 → 复用 role
//   Level 1 — images/videos/audios 元素为纯 URL 字符串 → 按数量智能推断 role
//
// 控制参数（resolution, duration, watermark 等）→ 顶层 key 数据驱动直通
// 新增参数只需在 PASSTHROUGH_KEYS 中追加，零侵入式扩展

/// 火山方舟视频/图片接口的「控制参数白名单」
/// 在用户请求体中出现即原样透传到火山上游请求体，扩展时追加一行即可。
const VOLCENGINE_CONTENT_PASSTHROUGH_KEYS: &[&str] = &[
    // 画面控制
    "ratio",      // 宽高比，如 "16:9", "4:3"
    "resolution", // 分辨率，如 "480p", "720p", "1080p"
    // 视频控制
    "duration",     // 视频时长（秒），如 5, 10
    "camera_fixed", // 是否固定摄像头
    "seed",         // 随机种子
    // 音频/水印/末帧
    "generate_audio",    // 是否生成音频 (bool)
    "return_last_frame", // 是否返回末帧 (bool)
    "watermark",         // 是否添加水印 (bool)
    // 流式/回调
    "stream",                   // 是否流式返回
    "callback_url",             // 回调地址
    "service_tier",             // 服务等级（如 flex 离线减半）
    "execution_expires_after",  // 任务超时失效时间 (秒)
    "draft",                    // 是否开启样片模式
    "tools",                    // 调用的工具
    "safety_identifier",        // 终端用户的唯一标识符
    "priority",                 // 请求的执行优先级
    "frames",                   // 生成视频的帧数
    "omni_reference_task_type", // auto / edit / extend
    "output_format",            // mp4 / mov
    "bitrate_mode",             // 控制输出视频的编码码率，文档不体现的内部参数
];

/// 构建火山方舟 /api/v3/contents/generations/tasks 请求体。
///
/// 支持三种输入格式，系统自动识别：
///
/// **简单模式** — images/videos/audios 为纯 URL 字符串数组：
/// ```json
/// {"model": "...", "prompt": "...", "images": ["url1"], "resolution": "720p"}
/// ```
///
/// **高级模式** — 带 role 的结构化对象数组：
/// ```json
/// {"model": "...", "prompt": "...", "images": [{"url": "url1", "role": "first_frame"}]}
/// ```
///
/// **直通模式** — 直接传入火山官方 content 数组：
/// ```json
/// {"model": "...", "content": [{"type": "text", "text": "..."}, ...]}
/// ```
// ── 提取出的公共方法：构建多模态 content 数组 ──
pub(crate) fn build_content_array(body: &serde_json::Value) -> serde_json::Value {
    // ── Level 3：直通模式 ──
    // body 中已包含 content 数组，原样使用
    let content = if let Some(c) = body.get("content").filter(|v| v.is_array()) {
        c.clone()
    } else {
        // ── Level 1 & 2：从 prompt + images/videos/audios 构建 content ──
        let mut parts: Vec<serde_json::Value> = Vec::new();

        // 有 prompt 才写 text，不伪造默认文案（空则交上游处理）
        if let Some(prompt) = body
            .get("prompt")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            parts.push(serde_json::json!({"type": "text", "text": prompt}));
        }

        // 图片：media::collect_media_values 统一收集；按数量推断 role
        // 1 张 → first_frame，2 张 → first_frame + last_frame，3+ 张 → reference_image
        let arr_images = media::collect_media_values(body, &["images", "image_urls"]);
        if !arr_images.is_empty() {
            let defaults = media::infer_image_default_roles(arr_images.len());
            for (i, item) in arr_images.iter().enumerate() {
                let (url, role) = media::parse_media_item(
                    item,
                    defaults.get(i).copied().unwrap_or("reference_image"),
                );
                if let Some(u) = url {
                    let mut entry = serde_json::json!({
                        "type": "image_url",
                        "image_url": {"url": u}
                    });
                    entry["role"] = serde_json::json!(role);
                    parts.push(entry);
                }
            }
        }

        // 视频：默认 role = reference_video
        let arr_videos = media::collect_media_values(body, &["videos"]);
        if !arr_videos.is_empty() {
            for item in &arr_videos {
                let (url, role) = media::parse_media_item(item, "reference_video");
                if let Some(u) = url {
                    let mut entry = serde_json::json!({
                        "type": "video_url",
                        "video_url": {"url": u}
                    });
                    entry["role"] = serde_json::json!(role);
                    parts.push(entry);
                }
            }
        }

        // 音频：默认 role = reference_audio
        let arr_audios = media::collect_media_values(body, &["audios"]);
        if !arr_audios.is_empty() {
            for item in &arr_audios {
                let (url, role) = media::parse_media_item(item, "reference_audio");
                if let Some(u) = url {
                    let mut entry = serde_json::json!({
                        "type": "audio_url",
                        "audio_url": {"url": u}
                    });
                    entry["role"] = serde_json::json!(role);
                    parts.push(entry);
                }
            }
        }

        serde_json::json!(parts)
    };

    // ── 后处理：多模态视频请求中，为缺失 role 的 image_url 自动补充 role ──
    // 火山方舟 API 要求：当 content 中同时包含 video_url 或 audio_url 时，
    // 所有 image_url 元素必须携带 role 字段（如 reference_image），否则提交任务会被拒绝。
    media::ensure_image_roles_for_multimodal(content)
}

pub(crate) fn build_volcengine_content_body(
    model: &str,
    body: &serde_json::Value,
) -> serde_json::Value {
    let content = build_content_array(body);

    // ── 组装请求体 ──
    let mut result = serde_json::json!({
        "model": model,
        "content": content,
    });

    // ── 数据驱动直通：遍历白名单，存在即透传 ──
    for key in VOLCENGINE_CONTENT_PASSTHROUGH_KEYS {
        if let Some(v) = body.get(*key) {
            result[*key] = v.clone();
        }
    }

    // ── 任务失效时间默认兜底 ──
    // 若用户未指定 execution_expires_after，则默认设为 1小时（3600 秒），防止队列任务长时间积压或一直处于排队中
    if result.get("execution_expires_after").is_none() {
        result["execution_expires_after"] = serde_json::json!(3600);
    }

    // ── 分辨率默认兜底 ──
    // 若用户未指定 resolution，则默认设为 "720p"；用户传参直接透传保持原样（部分上游对大小写敏感）
    if result
        .get("resolution")
        .and_then(|v| v.as_str())
        .map_or(true, |s| s.trim().is_empty())
    {
        result["resolution"] = serde_json::json!("720p");
    }

    result
}

// ── MiniMax V2 视频请求体构建器 ─────────────────────────────────

const MINIMAX_VIDEO_PASSTHROUGH_KEYS: &[&str] =
    &["resolution", "ratio", "duration", "callback_url"];

pub(crate) fn build_minimax_video_body(model: &str, body: &serde_json::Value) -> serde_json::Value {
    let content = build_content_array(body);
    let mut result = serde_json::json!({
        "model": model,
        "content": content,
    });
    for key in MINIMAX_VIDEO_PASSTHROUGH_KEYS {
        if let Some(v) = body.get(*key) {
            result[*key] = v.clone();
        }
    }
    // OpenAI watermark → MiniMax aigc_watermark（原生字段优先）
    if let Some(wm) = body.get("aigc_watermark").or_else(|| body.get("watermark")) {
        result["aigc_watermark"] = wm.clone();
    }
    result
}

// ── MiniMax 图片生成请求体构建器（文生图 / 图生图）────────────────
// 官方参数大多与 OpenAI 同名；仅少数需映射：size/ratio→aspect_ratio、
// watermark→aigc_watermark、b64_json→base64、image→subject_reference。

const MINIMAX_IMAGE_PASSTHROUGH_KEYS: &[&str] = &[
    "prompt",
    "aspect_ratio",
    "width",
    "height",
    "n",
    "seed",
    "style",
    "prompt_optimizer",
    "aigc_watermark",
    "subject_reference",
    "response_format",
];

pub(crate) fn build_minimax_image_body(model: &str, body: &serde_json::Value) -> serde_json::Value {
    let mut result = serde_json::json!({ "model": model });

    // 1. 官方字段透传（原生优先，支持官方参数直调）
    for key in MINIMAX_IMAGE_PASSTHROUGH_KEYS {
        if let Some(v) = body.get(*key) {
            if !v.is_null() {
                result[*key] = v.clone();
            }
        }
    }

    // 2. OpenAI size / ratio → aspect_ratio（官方 aspect_ratio 或 width+height 优先）
    let has_wh = result.get("width").is_some() && result.get("height").is_some();
    if result.get("aspect_ratio").is_none() && !has_wh {
        if let Some(r) = body
            .get("ratio")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            result["aspect_ratio"] = serde_json::json!(r);
        } else if let Some(size) = body.get("size").and_then(|v| v.as_str()) {
            if let Some(ar) = media::size_to_ratio(size) {
                result["aspect_ratio"] = serde_json::json!(ar);
            }
        }
    }

    // 3. OpenAI watermark → aigc_watermark（官方字段优先）
    if result.get("aigc_watermark").is_none() {
        if let Some(wm) = body.get("watermark") {
            result["aigc_watermark"] = wm.clone();
        }
    }

    // 4. response_format: OpenAI b64_json → MiniMax base64
    if result
        .get("response_format")
        .and_then(|v| v.as_str())
        .is_some_and(|rf| rf == "b64_json")
    {
        result["response_format"] = serde_json::json!("base64");
    }

    // 5. OpenAI image/image_urls → subject_reference（官方 subject_reference 优先）
    if result.get("subject_reference").is_none() {
        let urls = media::collect_image_urls(body, &["image", "image_urls", "image[]"]);
        if !urls.is_empty() {
            let refs: Vec<serde_json::Value> = urls
                .into_iter()
                .map(|u| {
                    serde_json::json!({
                        "type": "character",
                        "image_file": u
                    })
                })
                .collect();
            result["subject_reference"] = serde_json::Value::Array(refs);
        }
    }

    result
}

pub(crate) fn build_bytefor_video_body(model: &str, body: &serde_json::Value) -> serde_json::Value {
    let mut fwd = body.clone();

    // model 字段
    fwd["model"] = serde_json::json!(model);

    // duration 字段
    if let Some(dur_val) = body.get("duration") {
        if let Some(d_str) = dur_val.as_str() {
            let d_clean = d_str.to_lowercase();
            if d_clean.ends_with('s') {
                fwd["duration"] = serde_json::json!(d_clean);
            } else {
                fwd["duration"] = serde_json::json!(format!("{}s", d_clean));
            }
        } else if let Some(d_num) = dur_val.as_i64() {
            fwd["duration"] = serde_json::json!(format!("{}s", d_num));
        } else if let Some(d_f64) = dur_val.as_f64() {
            fwd["duration"] = serde_json::json!(format!("{}s", d_f64));
        }
    } else {
        fwd["duration"] = serde_json::json!("5s");
    }

    // aspectRatio 字段
    if fwd.get("aspectRatio").is_none() {
        let ratio = body
            .get("aspect_ratio")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("size").and_then(|v| v.as_str()));
        if let Some(r) = ratio {
            fwd["aspectRatio"] = serde_json::json!(r);
        } else {
            fwd["aspectRatio"] = serde_json::json!("16:9");
        }
    }
    if let Some(obj) = fwd.as_object_mut() {
        obj.remove("aspect_ratio");
        obj.remove("size");
    }

    // resolution 字段
    if fwd.get("resolution").is_none() {
        let res = if let Some(q) = body.get("quality").and_then(|v| v.as_str()) {
            let q_lower = q.to_lowercase();
            if q_lower == "hd" || q_lower == "high" {
                Some("1080P".to_string())
            } else if q_lower == "standard" || q_lower == "fast" {
                Some("720P".to_string())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(r) = res {
            fwd["resolution"] = serde_json::json!(r);
        } else {
            fwd["resolution"] = serde_json::json!("720P");
        }
    }
    if let Some(obj) = fwd.as_object_mut() {
        obj.remove("quality");
    }

    // referenceMode 字段：优先取 fwd 已有值，fallback 取 body.reference_mode，统一归一化为中文
    let raw_rm = fwd
        .get("referenceMode")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("reference_mode").and_then(|v| v.as_str()));
    if let Some(rm) = raw_rm {
        let final_rm = match rm {
            "all" | "全能" | "全能参考" => "全能参考",
            "subject" | "主体" | "主体参考" => "主体参考",
            other => other,
        };
        fwd["referenceMode"] = serde_json::json!(final_rm);
    }
    if let Some(obj) = fwd.as_object_mut() {
        obj.remove("reference_mode");
    }

    // images 字段 (合并图片、视频、音频 URL)
    let mut images = Vec::new();
    images.extend(media::collect_image_urls(body, &["images", "image_urls"]));
    images.extend(media::collect_image_urls(body, &["videos"]));
    images.extend(media::collect_image_urls(body, &["audios"]));
    if !images.is_empty() {
        fwd["images"] = serde_json::json!(images);
    }
    if let Some(obj) = fwd.as_object_mut() {
        obj.remove("image_urls");
        obj.remove("videos");
        obj.remove("audios");
    }

    // 清理 OpenAI 特有且 Bytefor 不支持的参数
    if let Some(obj) = fwd.as_object_mut() {
        media::remove_openai_compat_fields(obj);
        obj.remove("n");
    }

    fwd
}
