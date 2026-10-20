/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtractedFeatures {
    pub has_video: bool,
    pub has_audio: bool,
    /// 请求是否包含参考图（用于区分文生图/图生图计费，如可灵）
    pub has_image_ref: bool,
    /// 生成输出的视频时长（秒），用于按输出时长或视频分辨率阶梯计费
    pub duration_seconds: Option<f64>,
    pub resolution: Option<String>,
    /// 图片数量（用于按张计费）：请求阶段取 n，响应阶段取实际返回数量
    pub image_count: Option<i32>,
    /// 服务等级（用于离线推理等特定计费，如 flex）
    pub service_tier: Option<String>,
    /// 提示词扩写（DashScope 等图片模型，可能影响计费）
    pub prompt_extend: bool,
    /// 火山 Seedream 图层拆分（请求 layer_decomposition=true）
    pub layer_decomposition: bool,
    /// 可灵视频有声/无声（on/off），影响计费倍率，默认 off
    pub sound: Option<String>,
    /// 参考图数量（用于腾讯云 Vidu 图片计费区分 ref_1_3 / ref_4_7）
    pub image_ref_count: Option<i32>,
    /// 原始 size 参数（如 "1024x1024"），用于按分辨率像素计费
    pub size: Option<String>,
    /// 画质等级（如 "low"、"medium"、"high"），用于画质倍率计费
    pub quality: Option<String>,
    /// 文本字符数（语音合成按万字符计费）
    pub text_characters: Option<i32>,
    /// 视频帧率（用于画质计费等按分辨率+帧率计费场景）
    pub fps: Option<f64>,
    /// 级联画质增强计费档位（fast|standard，由级联模型 Id 推导），与可灵 mode 解耦
    pub version: Option<String>,
    /// 联网搜索次数（Responses API 联网搜索计费）
    pub web_search: Option<i32>,
    /// 峰谷时段倍率快照（请求开始/冻结时锁定；异步结算优先使用，避免跨时段变价）
    pub time_multiplier: Option<f64>,
    /// 请求计费时锁定的折扣限价（仅时段规划开启时写入）
    pub locked_site_discount: Option<f64>,
    /// 请求计费时锁定的全站折扣（仅时段规划开启时写入）
    pub locked_global_discount: Option<f64>,
    /// 输入/参考视频总时长（秒），用于输入参考视频计费、双向秒价或按总时长计费
    pub video_ref_seconds: Option<f64>,
    /// 参考音频总时长（秒），用于 fal 等模型参考音频 Token 计费
    pub audio_ref_seconds: Option<f64>,
}

/// content[].type 是否包含指定关键字
#[inline]
fn type_contains(item: &Value, needle: &str) -> bool {
    item.get("type")
        .and_then(|v| v.as_str())
        .is_some_and(|t| t.contains(needle))
}

#[inline]
fn nonempty_str_field(obj: &Value, key: &str) -> bool {
    obj.get(key)
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

#[inline]
fn nonempty_array_field(obj: &Value, key: &str) -> bool {
    obj.get(key)
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty())
}

/// 媒体字段是否有有效载荷（非空字符串 / 对象 / 非空数组）
#[inline]
fn media_payload_present(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.is_empty())
        || v.is_object()
        || v.as_array().is_some_and(|a| !a.is_empty())
}

#[inline]
fn field_media_present(body: &Value, key: &str) -> bool {
    body.get(key).is_some_and(media_payload_present)
}

fn set_opt_if_none<T>(dst: &mut Option<T>, src: Option<T>) {
    if dst.is_none() {
        *dst = src;
    }
}

fn overwrite_if_some<T>(dst: &mut Option<T>, src: Option<T>) {
    if src.is_some() {
        *dst = src;
    }
}

/// SSE 行 → JSON 载荷；空行 / [DONE] 返回 None
#[inline]
fn sse_json_payload(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.ends_with("[DONE]") {
        return None;
    }
    Some(if let Some(rest) = line.strip_prefix("data: ") {
        rest
    } else if let Some(rest) = line.strip_prefix("data:") {
        rest
    } else {
        line
    })
}

#[inline]
fn json_i32(v: &Value) -> i32 {
    v.as_i64().unwrap_or(0) as i32
}

#[inline]
fn json_field_i32(obj: &Value, key: &str) -> i32 {
    obj.get(key).map(json_i32).unwrap_or(0)
}

/// on/off：true→on，false→off
#[inline]
fn on_off(enabled: bool) -> String {
    if enabled { "on" } else { "off" }.to_string()
}

/// 扫描 content 数组：可分别开关 video/audio（Responses input 仅检 audio）
fn scan_typed_content(
    content: Option<&Value>,
    video: bool,
    audio: bool,
    has_video: &mut bool,
    has_audio: &mut bool,
) {
    if (!video || *has_video) && (!audio || *has_audio) {
        return;
    }
    let Some(arr) = content.and_then(|v| v.as_array()) else {
        return;
    };
    for item in arr {
        if video && !*has_video && type_contains(item, "video") {
            *has_video = true;
        }
        if audio && !*has_audio && type_contains(item, "audio") {
            *has_audio = true;
        }
        if (!video || *has_video) && (!audio || *has_audio) {
            break;
        }
    }
}

/// 扫描 contents、Interactions input 或 MiniMax content 中的参考视频与输入图数量
fn scan_content_references(arr: &[Value]) -> (bool, i32) {
    let mut has_video = false;
    let mut image_n = 0i32;
    for c in arr {
        match c.get("type").and_then(|t| t.as_str()) {
            Some("base_video" | "feature_video" | "video_url" | "video") => has_video = true,
            Some("first_frame" | "last_frame" | "refer_image" | "image_url" | "image") => {
                image_n += 1
            }
            _ => {}
        }
        if let Some(sub) = c.get("content").and_then(|v| v.as_array()) {
            let (sv, si) = scan_content_references(sub);
            has_video |= sv;
            image_n += si;
        }
    }
    (has_video, image_n)
}

fn count_image_field(body: &Value) -> i32 {
    body.get("image")
        .map(|v| {
            if v.as_str().filter(|s| !s.is_empty()).is_some() {
                1
            } else {
                v.as_array().map(|a| a.len() as i32).unwrap_or(0)
            }
        })
        .unwrap_or(0)
}

fn count_image_list_fields(body: &Value) -> i32 {
    body.get("images")
        .or_else(|| body.get("image_urls"))
        .or_else(|| body.get("image_list"))
        .or_else(|| body.get("reference_image_urls"))
        .and_then(|v| v.as_array())
        .map(|a| a.len() as i32)
        .unwrap_or(0)
}

pub fn extract_request_features(body: &Value) -> ExtractedFeatures {
    let mut has_video = false;
    let mut has_audio = false;

    // service_tier：根 / parameters；腾讯云 OffPeak=Enabled → flex；Vidu off_peak=true → flex
    let service_tier = body
        .get("service_tier")
        .or_else(|| body.get("parameters").and_then(|p| p.get("service_tier")))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| {
            body.get("OutputConfig")
                .and_then(|oc| oc.get("OffPeak"))
                .and_then(|v| v.as_str())
                .filter(|s| s.eq_ignore_ascii_case("Enabled"))
                .map(|_| "flex".to_string())
        })
        .or_else(|| {
            let is_off_peak = body.get("off_peak").is_some_and(|v| {
                v.as_bool() == Some(true) || v.as_str() == Some("true")
            });
            is_off_peak.then(|| "flex".to_string())
        });

    has_audio |= body
        .get("modalities")
        .and_then(|m| m.as_array())
        .is_some_and(|mods| mods.iter().any(|m| m.as_str() == Some("audio")));
    has_audio |= body.get("generate_audio").and_then(|v| v.as_bool()) == Some(true);

    if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in msgs {
            scan_typed_content(
                msg.get("content"),
                true,
                true,
                &mut has_video,
                &mut has_audio,
            );
            if has_video && has_audio {
                break;
            }
        }
    }
    has_video |=
        nonempty_array_field(body, "videos") || nonempty_array_field(body, "reference_video_urls");
    has_audio |= nonempty_array_field(body, "reference_audio_urls")
        || nonempty_array_field(body, "sounds")
        || nonempty_array_field(body, "audios");

    let (content_video, content_images) = body
        .get("content")
        .or_else(|| body.get("contents"))
        .or_else(|| body.get("input"))
        .and_then(|v| v.as_array())
        .map(|a| scan_content_references(a))
        .unwrap_or((false, 0));
    has_video |= content_video;

    scan_typed_content(
        body.get("content"),
        true,
        true,
        &mut has_video,
        &mut has_audio,
    );
    // Responses / 方舟 response：input[].content 仅检 audio（保持原语义）
    if let Some(input_arr) = body.get("input").and_then(|i| i.as_array()) {
        for item in input_arr {
            scan_typed_content(
                item.get("content"),
                false,
                true,
                &mut has_video,
                &mut has_audio,
            );
            if has_audio {
                break;
            }
        }
    }
    // DashScope input.media：仅检 video
    scan_typed_content(
        body.pointer("/input/media"),
        true,
        false,
        &mut has_video,
        &mut has_audio,
    );

    let mut video_ref_seconds = None;

    // duration：根、task、parameters、OutputConfig、settings（分辨率见 extract_resolution）
    let mut duration_seconds = body
        .get("duration")
        .or_else(|| body.pointer("/task/duration"))
        .or_else(|| body.pointer("/parameters/duration"))
        .or_else(|| body.pointer("/OutputConfig/Duration"))
        .or_else(|| body.pointer("/settings/duration"))
        .and_then(parse_json_f64);

    let prompt_extend = body
        .get("prompt_extend")
        .or_else(|| body.pointer("/parameters/prompt_extend"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let layer_decomposition =
        body.get("layer_decomposition").and_then(|v| v.as_bool()) == Some(true);

    // generate_audio（OpenAI 兼容参数）优先于 settings.audio（可灵 3.0 规范，native→on）
    let sound = if let Some(ga) = body.get("generate_audio") {
        let enabled = ga.as_bool().unwrap_or(false) || ga.as_str() == Some("true");
        Some(on_off(enabled))
    } else {
        body.pointer("/settings/audio")
            .and_then(|v| v.as_str())
            .map(|a| {
                let a = a.to_ascii_lowercase();
                on_off(!(a == "off" || a == "false"))
            })
    };

    let (tencent_refs, tencent_video) = {
        let mut refs = 0i32;
        let mut vid = false;
        if let Some(arr) = body.get("FileInfos").and_then(|v| v.as_array()) {
            for item in arr {
                let usage = item.get("Usage").and_then(|u| u.as_str()).unwrap_or("");
                let is_video = item.get("Category").and_then(|c| c.as_str()) == Some("Video");
                if is_video {
                    if usage.eq_ignore_ascii_case("Reference") {
                        vid = true;
                    }
                } else if usage.eq_ignore_ascii_case("FirstFrame")
                    || usage.eq_ignore_ascii_case("LastFrame")
                    || usage.eq_ignore_ascii_case("Reference")
                {
                    refs += 1;
                }
            }
        }
        if nonempty_str_field(body, "LastFrameUrl") {
            refs += 1;
        }
        (refs, vid)
    };
    has_video |= tencent_video;

    let mut has_image_ref = field_media_present(body, "image")
        || nonempty_array_field(body, "images")
        || nonempty_array_field(body, "image_urls")
        || nonempty_array_field(body, "image_list")
        || nonempty_array_field(body, "subject_image_list")
        || nonempty_array_field(body, "reference_image_urls")
        || body.get("image_reference").is_some_and(|v| !v.is_null())
        || tencent_refs > 0
        || content_images > 0;

    let mut image_ref_count = {
        let flat = count_image_field(body);
        let count = if flat > 0 {
            flat
        } else {
            let from_lists = count_image_list_fields(body);
            if from_lists > 0 {
                from_lists
            } else if content_images > 0 {
                content_images
            } else {
                tencent_refs
            }
        };
        (count > 0).then_some(count)
    };

    // usage / task.usage / tencent Output.Usage 覆盖时长、输入图数（分辨率统一走 extract_resolution）
    let mut input_images = None;
    let usage_candidate = body
        .get("usage")
        .or_else(|| body.get("task").and_then(|t| t.get("usage")))
        .or_else(|| tencent_output_usage(body));
    if let Some(usage) = usage_candidate {
        if let Some(in_sec) = usage
            .get("input_video_duration")
            .or_else(|| usage.get("input_seconds"))
            .or_else(|| usage.get("InputSeconds"))
            .and_then(parse_json_f64)
            .filter(|&d| d > 0.0)
        {
            video_ref_seconds = Some(in_sec);
        }
        // 输出生成视频时长：
        // 1. 优先提取明确的纯输出时长（阿里 output_video_duration / MiniMax output_seconds / 腾讯 OutputSeconds）
        // 2. 次选通用时长或总时长（duration / total_seconds / TotalSeconds）：
        //    若已识别输入时长且总时长大于等于输入时长，扣除输入时长得出纯输出时长，避免将总时长误当成纯输出时长
        let explicit_out = usage
            .get("output_video_duration")
            .or_else(|| usage.get("output_seconds"))
            .or_else(|| usage.get("OutputSeconds"))
            .and_then(parse_json_f64)
            .filter(|&d| d > 0.0);

        if let Some(out_sec) = explicit_out {
            duration_seconds = Some(out_sec);
        } else if let Some(total_dur) = usage
            .get("duration")
            .or_else(|| usage.get("total_seconds"))
            .or_else(|| usage.get("TotalSeconds"))
            .and_then(parse_json_f64)
            .filter(|&d| d > 0.0)
        {
            let in_dur = video_ref_seconds.unwrap_or(0.0);
            let net_out = if in_dur > 0.0 && total_dur >= in_dur {
                total_dur - in_dur
            } else {
                total_dur
            };
            if net_out > 0.0 {
                duration_seconds = Some(net_out);
            }
        }
        if let Some(ii) = usage
            .get("input_images")
            .or_else(|| usage.get("input_image_count"))
            .or_else(|| usage.get("InputImageCount"))
            .or_else(|| usage.get("image_count"))
            .and_then(|v| v.as_i64())
            .filter(|&c| c > 0)
        {
            input_images = Some(ii as i32);
        }
    }
    if input_images.is_none() {
        input_images = body
            .get("input_images")
            .or_else(|| body.get("input_image_count"))
            .or_else(|| body.get("parameters").and_then(|p| p.get("input_images")))
            .and_then(|v| v.as_i64())
            .map(|ii| ii as i32);
    }

    // 即梦等：frames → 时长
    if duration_seconds.is_none() {
        if let Some(frames) = body
            .get("frames")
            .and_then(|f| f.as_f64().or_else(|| f.as_i64().map(|i| i as f64)))
        {
            if frames > 0.0 {
                duration_seconds = Some(if (frames - 121.0).abs() < 1e-3 {
                    5.0
                } else if (frames - 241.0).abs() < 1e-3 {
                    10.0
                } else {
                    (frames - 1.0) / 24.0
                });
            }
        }
    }

    let size = body
        .get("size")
        .or_else(|| body.pointer("/data/0/size"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let quality = body
        .get("quality")
        .or_else(|| body.pointer("/parameters/quality"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());
    let text_characters = body
        .get("input")
        .or_else(|| body.pointer("/req_params/text"))
        .or_else(|| body.pointer("/request/text"))
        .and_then(|v| v.as_str())
        .map(|s| s.chars().count() as i32);
    let image_count = body
        .get("n")
        .or_else(|| body.pointer("/OutputConfig/OutputImageCount"))
        .and_then(|v| v.as_i64())
        .map(|v| v.max(1) as i32);
    let fps = body.get("fps").and_then(|v| v.as_f64());
    let version = body
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());

    let resolution = extract_resolution(body);
    if let Some(ii) = input_images {
        image_ref_count = Some(ii);
        if ii > 0 {
            has_image_ref = true;
        }
    }

    ExtractedFeatures {
        has_video,
        has_audio,
        has_image_ref,
        duration_seconds,
        resolution,
        image_count,
        service_tier,
        prompt_extend,
        layer_decomposition,
        sound,
        image_ref_count,
        size,
        quality,
        text_characters,
        fps,
        version,
        web_search: None,
        time_multiplier: None,
        locked_site_discount: None,
        locked_global_discount: None,
        video_ref_seconds,
        audio_ref_seconds: None,
    }
}

impl ExtractedFeatures {
    /// 视频计费总时长（秒）：输出生成时长 + 输入参考时长。
    /// 用于所有未区分输入/输出的统一时长计费场景（如固定秒价、分辨率阶梯等），确保计费不漏算输入时长；
    /// 仅在显式配置了输入/输出分离单价（如 video_seconds_io / fal_ref_video）时才分别使用 duration_seconds 与 video_ref_seconds。
    pub fn total_video_seconds(&self) -> f64 {
        self.duration_seconds.unwrap_or(0.0) + self.video_ref_seconds.unwrap_or(0.0)
    }

    /// 严谨合并另外一个特征结构体中的字段（用于出参特征与入参特征融合，保障计费一致性）
    pub fn merge(&mut self, other: ExtractedFeatures) {
        overwrite_if_some(&mut self.duration_seconds, other.duration_seconds);
        set_opt_if_none(&mut self.resolution, other.resolution);
        set_opt_if_none(&mut self.sound, other.sound);
        set_opt_if_none(&mut self.version, other.version);
        self.has_video |= other.has_video;
        self.has_audio |= other.has_audio;
        self.has_image_ref |= other.has_image_ref;
        overwrite_if_some(&mut self.web_search, other.web_search);
        overwrite_if_some(&mut self.image_ref_count, other.image_ref_count);
        overwrite_if_some(&mut self.size, other.size);
        overwrite_if_some(&mut self.image_count, other.image_count);
        // 峰谷快照：已锁定的值不被后续 merge 冲掉
        set_opt_if_none(&mut self.time_multiplier, other.time_multiplier);
        set_opt_if_none(&mut self.locked_site_discount, other.locked_site_discount);
        set_opt_if_none(
            &mut self.locked_global_discount,
            other.locked_global_discount,
        );
        overwrite_if_some(&mut self.video_ref_seconds, other.video_ref_seconds);
        overwrite_if_some(&mut self.audio_ref_seconds, other.audio_ref_seconds);
    }

    /// 轮询结算：叠加终态响应特征，并应用厂商覆盖（顺序与历史一致，不可打乱）
    pub fn merge_settlement_response(
        &mut self,
        resp_json: &Value,
        store_body: &str,
        category: &str,
    ) {
        // 火山 MediaKit 终态 result（时长/分辨率/帧率）；fps 不经 merge，需先写入
        if let Some(result) = resp_json.get("result") {
            if let Some(duration) = result.get("duration").and_then(|v| v.as_f64()) {
                self.duration_seconds = Some(duration);
            }
            if let Some(res) = result.get("resolution").and_then(|v| v.as_str()) {
                self.resolution = Some(res.to_string());
            }
            if let Some(fps) = result.get("fps").and_then(|v| v.as_f64()) {
                self.fps = Some(fps);
            } else if let Some(fps) = result.get("fps").and_then(|v| v.as_i64()) {
                self.fps = Some(fps as f64);
            }
        }
        // 合并终态通用特征（如 input_images / size）；已有 resolution 不被覆盖
        self.merge(extract_request_features(resp_json));
        // 厂商终态覆盖放在 merge 之后，确保不被冲掉
        if let Some(d) = extract_kling_video_duration(resp_json) {
            self.duration_seconds = Some(d);
        }
        let (tc_dur, tc_res) = extract_tencent_vod_video_settlement(resp_json);
        if let Some(d) = tc_dur {
            self.duration_seconds = Some(d);
        }
        set_opt_if_none(&mut self.resolution, tc_res);
        if category.contains("视频") && self.duration_seconds.is_none() {
            self.duration_seconds = Some(5.0);
        }
        if let Some(n) =
            count_images_from_value(resp_json).or_else(|| count_response_images(store_body))
        {
            self.image_count = Some(n);
        }
    }
}

/// 从接口响应中提取实际返回的图片数量。
/// 支持 OpenAI/火山方舟 `data` 数组、Google Gemini `candidates.content.parts` 中的图片，
/// 以及 SSE 流式缓冲后的文本（逐行解析 `data: {...}` 提取图片数组）。
/// 返回 None 表示响应中无法识别图片数组（非图片类接口）。
pub fn count_response_images(response: &str) -> Option<i32> {
    // 尝试整体 JSON 解析（非流式响应）
    if let Ok(v) = serde_json::from_str::<Value>(response) {
        if let Some(count) = count_images_from_value(&v) {
            return Some(count);
        }
    }

    // SSE 流式缓冲回落：逐行解析 data: {...} 中的图片数量
    let mut accumulated_from_arrays = 0i32;
    let mut usage_total: Option<i32> = None;

    for line in response.lines() {
        let Some(json_str) = sse_json_payload(line) else {
            continue;
        };

        if let Ok(v) = serde_json::from_str::<Value>(json_str) {
            // 优先官方总计（MiniMax metadata.success_count / 火山 usage.*）
            if let Some(c) = official_image_count(&v) {
                usage_total = Some(c);
            }

            // 累加数组中的实体数
            if let Some(count) = count_images_from_arrays(&v) {
                accumulated_from_arrays += count;
            }
        }
    }

    // 如果流式数据中包含 usage 统计总数，则优先使用该总数（通常流的最后一条包含准确总计）
    if usage_total.is_some() {
        return usage_total;
    }

    if accumulated_from_arrays > 0 {
        Some(accumulated_from_arrays)
    } else {
        None
    }
}

/// 从单个 JSON Value 中提取图片数量
pub fn count_images_from_value(v: &Value) -> Option<i32> {
    // 官方计数字段优先（MiniMax metadata.success_count / 火山 usage.*）
    if let Some(c) = official_image_count(v) {
        return Some(c);
    }
    count_images_from_arrays(v)
}

/// 解析 JSON 中的非负计数值（兼容整数与数字字符串，如 MiniMax 示例 `"3"`）
#[inline]
fn json_nonneg_count(v: &Value) -> Option<i32> {
    v.as_i64()
        .or_else(|| v.as_u64().map(|n| n as i64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
        .filter(|&n| n >= 0)
        .map(|n| n as i32)
}

/// 上游官方成功张数：MiniMax `metadata.success_count` → usage.generated_images / output_image_count / image_count
fn official_image_count(v: &Value) -> Option<i32> {
    v.pointer("/metadata/success_count")
        .and_then(json_nonneg_count)
        .or_else(|| {
            let usage = v.get("usage")?;
            usage
                .get("generated_images")
                .or_else(|| usage.get("output_image_count"))
                .or_else(|| usage.get("image_count"))
                .and_then(json_nonneg_count)
        })
}

/// 内部辅助函数：深度遍历各种嵌套的 data/results 数组结构提取数量
fn count_images_from_arrays(v: &Value) -> Option<i32> {
    let mut total_count = 0i32;

    #[inline]
    fn count_url_val(url: &Value) -> i32 {
        if let Some(arr) = url.as_array() {
            arr.iter()
                .filter(|u| u.as_str().is_some_and(|s| !s.is_empty()))
                .count() as i32
        } else if url.as_str().is_some_and(|s| !s.is_empty()) {
            1
        } else {
            0
        }
    }
    #[inline]
    fn has_media(item: &Value) -> bool {
        nonempty_str_field(item, "b64_json")
            || nonempty_str_field(item, "image")
            || nonempty_str_field(item, "url")
    }

    // 1. OpenAI / 火山方舟: { "data": [{"url"|"b64_json"|"image": "..."}, ...] }
    if let Some(data) = v.get("data").and_then(|d| d.as_array()) {
        for item in data {
            let from_url = item.get("url").map(count_url_val).unwrap_or(0);
            if from_url > 0 {
                total_count += from_url;
            } else if nonempty_str_field(item, "b64_json") || nonempty_str_field(item, "image") {
                total_count += 1;
            }
        }
    }

    // 2. 异步终态: data.result.images / data.task_result.images / result.images / images
    if total_count == 0 {
        let images_node = v
            .pointer("/data/result/images")
            .or_else(|| v.pointer("/data/task_result/images"))
            .or_else(|| v.pointer("/result/images"))
            .or_else(|| v.pointer("/task_result/images"))
            .or_else(|| v.get("images"));
        if let Some(images) = images_node.and_then(|i| i.as_array()) {
            for img in images {
                if let Some(url) = img.get("url") {
                    total_count += count_url_val(url);
                } else if nonempty_str_field(img, "b64_json") || nonempty_str_field(img, "image") {
                    total_count += 1;
                }
            }
        }
    }

    // 2b. 火山 MediaKit 同步图像: { "result": { "image_url": "..." } }
    // image.rs 同步闸口用本函数，不能只靠 find_urls 认这个字段
    if total_count == 0
        && v.pointer("/result/image_url")
            .and_then(|u| u.as_str())
            .is_some_and(|s| !s.is_empty())
    {
        total_count = 1;
    }

    // 3. Google Gemini: candidates[].content.parts[]
    if total_count == 0 {
        if let Some(candidates) = v.get("candidates").and_then(|c| c.as_array()) {
            for candidate in candidates {
                if let Some(parts) = candidate
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    for part in parts {
                        if part.get("inline_data").is_some() || part.get("inlineData").is_some() {
                            total_count += 1;
                        } else if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            if text.contains("](data:") {
                                total_count += text.matches("](data:").count() as i32;
                            } else if text.contains("http://") || text.contains("https://") {
                                for word in text.split_whitespace() {
                                    if word.starts_with("http://") || word.starts_with("https://") {
                                        total_count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 3b. Google Gemini Interactions: 非流式 steps[].content[] 与流式 step.delta (delta)
    if total_count == 0 {
        let is_interaction_image = |item: &Value| {
            item.get("type")
                .and_then(|t| t.as_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("image"))
                && (nonempty_str_field(item, "data") || nonempty_str_field(item, "uri"))
        };
        if let Some(steps) = v.get("steps").and_then(|s| s.as_array()) {
            total_count += steps
                .iter()
                .filter_map(|s| s.get("content").and_then(|c| c.as_array()))
                .flatten()
                .filter(|item| is_interaction_image(item))
                .count() as i32;
        } else if v.get("delta").is_some_and(is_interaction_image) {
            total_count += 1;
        }
    }

    // 4. DashScope: output.results[]（须含有效媒体字段）
    if total_count == 0 {
        if let Some(results) = v.pointer("/output/results").and_then(|r| r.as_array()) {
            total_count = results.iter().filter(|item| has_media(item)).count() as i32;
        }
    }

    // 4b. DashScope multimodal: output.choices[].message.content[].image
    if total_count == 0 {
        if let Some(choices) = v.pointer("/output/choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(parts) = choice
                    .pointer("/message/content")
                    .and_then(|c| c.as_array())
                {
                    total_count += parts
                        .iter()
                        .filter(|p| nonempty_str_field(p, "image"))
                        .count() as i32;
                }
            }
        }
    }

    // 5. 即梦 / MiniMax: data.image_urls[] / data.binary_data_base64[] / data.image_base64[]
    if total_count == 0 {
        for path in [
            "/data/image_urls",
            "/data/binary_data_base64",
            "/data/image_base64",
        ] {
            if let Some(arr) = v.pointer(path).and_then(|a| a.as_array()) {
                let n = arr
                    .iter()
                    .filter(|item| item.as_str().is_some_and(|s| !s.is_empty()))
                    .count() as i32;
                if n > 0 {
                    total_count = n;
                    break;
                }
            }
        }
    }

    // 6. 腾讯云 TokenHub 混元生图 3.5: choices[].delta.image.url
    if total_count == 0 {
        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if choice
                    .pointer("/delta/image/url")
                    .and_then(|u| u.as_str())
                    .is_some_and(|s| !s.is_empty())
                {
                    total_count += 1;
                }
            }
        }
    }

    (total_count > 0).then_some(total_count)
}

#[derive(Debug, Clone, Default)]
pub struct UsageTokens {
    pub prompt: i32,
    pub completion: i32,
    pub total: i32,
    /// 上游是否显式返回了 total_tokens（parse_usage 回填的不算；供 is_openai_format 判定）
    pub has_total_tokens: bool,
    /// 缓存命中 Token（OpenAI：prompt 子集；Anthropic：独立于 input_tokens）
    pub cached: i32,
    /// 缓存写入 Token（OpenAI GPT-5.6+；Chat: prompt_tokens_details / Responses: input_tokens_details）
    pub cache_write: i32,
    /// Claude 缓存创建 Token 数量（5m+1h 合并，不属于 prompt 子集）
    pub cache_creation: i32,
    /// 音频输入 Token 数量（属于 prompt 的子集，用于豆包聊天分离计价）
    pub audio_tokens: i32,
    /// 音频缓存命中 Token 数量（属于 cached 的子集）
    pub audio_cached_tokens: i32,
    /// 图片输入 Token 数量（用于多模态 tokens 分类计价）
    pub image_tokens: i32,
    /// 图片输出 Token 数量（用于多模态/生图模型输出图片分类计价）
    pub output_image_tokens: i32,
    /// 思考/推理 Token 数量（OpenAI completion_tokens_details / Gemini thoughtsTokenCount）
    pub reasoning_tokens: i32,
    /// 联网搜索次数
    pub web_search: i32,
}

impl UsageTokens {
    /// 供日志记录与用量报表展示的“总输入上下文”（包含缓存）
    /// - Claude 等独立缓存语义（上游未回显式 total_tokens 且有缓存读写）：返回 input + cache_read + cache_creation
    /// - OpenAI / DeepSeek 等原生包含缓存语义：直接返回 prompt
    #[inline]
    pub fn context_input_tokens(&self) -> i32 {
        if !self.has_total_tokens && (self.cached > 0 || self.cache_creation > 0) {
            self.prompt
                .saturating_add(self.cached)
                .saturating_add(self.cache_creation)
        } else {
            self.prompt
        }
    }
}

/// Chat Completions 与 Responses 的 input/prompt details 双路径取较大值
fn max_usage_detail_i32(usage: &Value, key: &str) -> i32 {
    let from = |obj: &str| {
        usage
            .get(obj)
            .and_then(|d| d.get(key))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32
    };
    from("prompt_tokens_details").max(from("input_tokens_details"))
}

/// Chat Completions 与 Responses 的 output/completion details 双路径取较大值
fn max_completion_detail_i32(usage: &Value, key: &str) -> i32 {
    let from = |obj: &str| {
        usage
            .get(obj)
            .and_then(|d| d.get(key))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32
    };
    from("completion_tokens_details").max(from("output_tokens_details"))
}

#[inline]
fn assign_max(dst: &mut i32, v: i32) {
    if v > *dst {
        *dst = v;
    }
}

/// 记录上游显式 total（回填不算）；供计费 is_openai_format 判定
#[inline]
fn assign_upstream_total(u: &mut UsageTokens, total: i32) {
    if total > 0 {
        assign_max(&mut u.total, total);
        u.has_total_tokens = true;
    }
}

/// 将 usage 中需持久到 billing_features 的附加项写回（流式/非流式共用）
pub fn enrich_features_from_usage(features: &mut ExtractedFeatures, usage: &UsageTokens) {
    if usage.web_search > 0 {
        features.web_search = Some(usage.web_search);
    }
}

/// 统一构建计费特征（落库与计费同源）。
/// - `billed`：调用方已用于 `calculate_relay_cost` 的特征 → 直接复用，保证库表与扣费一致
/// - 未传：从请求/上游/响应抽取，并补响应真实张数与 usage 附加项
pub fn build_billing_features(
    request: Option<&str>,
    upstream: Option<&str>,
    response: Option<&str>,
    billed: Option<ExtractedFeatures>,
    time_multiplier: Option<f64>,
) -> ExtractedFeatures {
    let mut feat = match billed {
        Some(b) => b,
        None => features_from_exchange(request, upstream, response),
    };
    set_opt_if_none(&mut feat.time_multiplier, time_multiplier);
    feat
}

/// 已解析 JSON 的请求体场景（同步图片等）：与落库同一套抽取
pub fn features_from_values(
    request: Option<&Value>,
    upstream: Option<&Value>,
    response: Option<&str>,
) -> ExtractedFeatures {
    let mut f = match request {
        Some(v) => extract_request_features(v),
        None => ExtractedFeatures::default(),
    };
    if let Some(v) = upstream {
        f.merge(extract_request_features(v));
    }
    if let Some(resp) = response {
        if let Ok(v) = serde_json::from_str::<Value>(resp) {
            // 快速路径：标准单对象 JSON，单次解析共享使用（减少 66%~75% JSON 反序列化开销）
            f.merge(extract_request_features(&v));
            let mut u = UsageTokens::default();
            if parse_usage_from_value(&mut u, &v) {
                enrich_features_from_usage(&mut f, &u);
            }
            if let Some(n) = count_images_from_value(&v) {
                f.image_count = Some(n);
            }
        } else {
            // 回落路径：SSE 流式响应等逐行解析
            enrich_features_from_usage(&mut f, &parse_usage(resp));
            if let Some(n) = count_response_images(resp) {
                f.image_count = Some(n);
            }
        }
    }
    f
}

/// 从请求/上游 body + 响应文本抽取特征（含响应真实图片张数）
pub fn features_from_exchange(
    request: Option<&str>,
    upstream: Option<&str>,
    response: Option<&str>,
) -> ExtractedFeatures {
    let req = request.and_then(|s| serde_json::from_str(s).ok());
    let up = upstream.and_then(|s| serde_json::from_str(s).ok());
    features_from_values(req.as_ref(), up.as_ref(), response)
}

/// Gemini `usageMetadata` → OpenAI `usage`（忽略 rawPromptTokenCount）
pub fn gemini_usage_metadata_to_openai(u: &Value) -> Value {
    let n = |k: &str| u.get(k).and_then(Value::as_i64).unwrap_or(0);
    let prompt = n("promptTokenCount");
    let thoughts = n("thoughtsTokenCount").max(n("thoughtTokenCount"));
    let completion = n("candidatesTokenCount") + thoughts;

    let extract_modality_tokens = |field: &str, target_modality: &str| -> i64 {
        u.get(field)
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter(|item| {
                        item.get("modality")
                            .and_then(Value::as_str)
                            .is_some_and(|m| m.eq_ignore_ascii_case(target_modality))
                    })
                    .map(|item| item.get("tokenCount").and_then(Value::as_i64).unwrap_or(0))
                    .sum()
            })
            .unwrap_or(0)
    };

    let output_image_tokens = extract_modality_tokens("candidatesTokensDetails", "IMAGE");
    let input_image_tokens = extract_modality_tokens("promptTokensDetails", "IMAGE");

    let mut usage = json!({
        "prompt_tokens": prompt,
        "completion_tokens": completion,
        "total_tokens": prompt + completion,
    });
    if thoughts > 0 || output_image_tokens > 0 {
        let mut comp_details = json!({});
        if thoughts > 0 {
            comp_details["reasoning_tokens"] = json!(thoughts);
        }
        if output_image_tokens > 0 {
            comp_details["image_tokens"] = json!(output_image_tokens);
        }
        usage["completion_tokens_details"] = comp_details;
    }
    let cached = n("cachedContentTokenCount");
    if cached > 0 || input_image_tokens > 0 {
        let mut prompt_details = json!({});
        if cached > 0 {
            prompt_details["cached_tokens"] = json!(cached);
        }
        if input_image_tokens > 0 {
            prompt_details["image_tokens"] = json!(input_image_tokens);
        }
        usage["prompt_tokens_details"] = prompt_details;
    }
    if input_image_tokens > 0 {
        usage["image_tokens"] = json!(input_image_tokens);
    }
    if output_image_tokens > 0 {
        usage["output_image_tokens"] = json!(output_image_tokens);
    }
    usage
}

/// 腾讯云 AIGC 任务节点（TaskType 动态键优先，AigcVideoTask / AigcImageTask 兜底）
pub fn tencent_aigc_task(resp: &Value) -> Option<&Value> {
    let r = resp.get("Response").unwrap_or(resp);
    r.get("TaskType")
        .and_then(|t| t.as_str())
        .and_then(|tt| r.get(tt))
        .or_else(|| r.get("AigcVideoTask").or_else(|| r.get("AigcImageTask")))
}

/// 腾讯云任务 Output.Usage 提取节点（高复用，直接从任务节点提取）
#[inline]
pub fn tencent_output_usage(resp: &Value) -> Option<&Value> {
    tencent_aigc_task(resp).and_then(|t| t.pointer("/Output/Usage"))
}

/// 腾讯云 Usage 转换为标准 OpenAI `usage` 格式（兼容 Token、时长与图片数量等计量特征）
pub fn tencent_usage_to_openai(u: &Value) -> Value {
    let n_i64 = |k: &str| u.get(k).and_then(Value::as_i64).unwrap_or(0);
    let prompt_tokens = n_i64("InputTokens");
    let completion_tokens = n_i64("ThoughtTokens");

    let in_sec = u
        .get("InputSeconds")
        .and_then(parse_json_f64)
        .unwrap_or(0.0);
    let out_sec = u
        .get("OutputSeconds")
        .and_then(parse_json_f64)
        .unwrap_or(0.0);
    let sec_total = in_sec + out_sec;

    let mut usage = json!({
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "total_tokens": prompt_tokens + completion_tokens,
    });

    if completion_tokens > 0 {
        usage["completion_tokens_details"] = json!({ "reasoning_tokens": completion_tokens });
    }
    if u.get("InputSeconds").is_some() || u.get("OutputSeconds").is_some() || sec_total > 0.0 {
        let total_sec = if sec_total.fract() == 0.0 {
            json!(sec_total as i64)
        } else {
            json!(sec_total)
        };
        usage["input_seconds"] = u.get("InputSeconds").cloned().unwrap_or(json!(0));
        usage["output_seconds"] = u.get("OutputSeconds").cloned().unwrap_or(json!(0));
        usage["total_seconds"] = total_sec;
    }
    if let Some(c) = u
        .get("InputImageCount")
        .and_then(Value::as_i64)
        .filter(|&c| c > 0)
    {
        usage["input_images"] = json!(c);
    }

    usage
}

fn sum_modality_tokens(usage: &Value, field: &str, modality: &str) -> i32 {
    usage
        .get(field)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|m| {
                    m.get("modality")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s.eq_ignore_ascii_case(modality))
                })
                .map(|m| json_field_i32(m, "tokens"))
                .sum()
        })
        .unwrap_or(0)
}

/// 从 usage JSON 对象提取 token 字段，取大值写入 UsageTokens（初始 0 时等同赋值，
/// SSE 多事件场景防止后续缺失字段覆盖已提取值，如 Anthropic message_start 提供
/// input_tokens、message_delta 提供 output_tokens）
fn apply_usage_max(u: &mut UsageTokens, usage: &Value) {
    // 独立提取两组字段名后取较大值，避免上游同时返回 prompt_tokens=0 和 input_tokens>0
    // 时 or_else 回退链不触发导致漏取（如 gpt-image-2 上游返回 prompt_tokens:0 + input_tokens:1136）
    let p = json_field_i32(usage, "prompt_tokens")
        .max(json_field_i32(usage, "input_tokens"))
        .max(json_field_i32(usage, "total_input_tokens"));
    let img_tokens = json_field_i32(usage, "image_tokens")
        .max(max_usage_detail_i32(usage, "image_tokens"))
        .max(sum_modality_tokens(usage, "input_tokens_by_modality", "image"));
    let out_img_tokens = json_field_i32(usage, "output_image_tokens")
        .max(max_completion_detail_i32(usage, "image_tokens"))
        .max(sum_modality_tokens(usage, "output_tokens_by_modality", "image"));
    let c = json_field_i32(usage, "completion_tokens")
        .max(json_field_i32(usage, "output_tokens"))
        .max(
            json_field_i32(usage, "total_output_tokens")
                + json_field_i32(usage, "total_thought_tokens"),
        );
    assign_max(&mut u.prompt, p);
    assign_max(&mut u.completion, c);
    assign_max(&mut u.image_tokens, img_tokens);
    assign_max(&mut u.output_image_tokens, out_img_tokens);
    assign_max(
        &mut u.reasoning_tokens,
        max_completion_detail_i32(usage, "reasoning_tokens")
            .max(json_field_i32(usage, "total_thought_tokens")),
    );
    assign_upstream_total(u, json_field_i32(usage, "total_tokens"));
    // Chat Completions: prompt_tokens_details；Responses: input_tokens_details；Interactions: total_cached_tokens
    assign_max(
        &mut u.cached,
        max_usage_detail_i32(usage, "cached_tokens")
            .max(json_field_i32(usage, "total_cached_tokens")),
    );
    assign_max(
        &mut u.cache_write,
        max_usage_detail_i32(usage, "cache_write_tokens"),
    );
    assign_max(
        &mut u.audio_tokens,
        max_usage_detail_i32(usage, "audio_tokens"),
    );
    assign_max(
        &mut u.audio_cached_tokens,
        max_usage_detail_i32(usage, "audio_cached_tokens"),
    );
    // Claude 缓存创建（APImart: 5m+1h 合并，兜底: Claude 原生 cache_creation_input_tokens）
    let cc_5m = json_field_i32(usage, "claude_cache_creation_5_m_tokens");
    let cc_1h = json_field_i32(usage, "claude_cache_creation_1_h_tokens");
    let cc = if cc_5m + cc_1h > 0 {
        cc_5m + cc_1h
    } else {
        json_field_i32(usage, "cache_creation_input_tokens")
    };
    assign_max(&mut u.cache_creation, cc);
    // GLM 差异 usage：输入含缓存 = hit + miss。其余根级命中：Claude cache_read / DeepSeek hit
    if usage.get("prompt_cache_miss_tokens").is_some() {
        let hit = json_field_i32(usage, "prompt_cache_hit_tokens");
        let miss = json_field_i32(usage, "prompt_cache_miss_tokens");
        assign_max(&mut u.cached, hit);
        assign_max(&mut u.prompt, hit + miss);
    } else if u.cached == 0 {
        u.cached = json_field_i32(usage, "cache_read_input_tokens")
            .max(json_field_i32(usage, "prompt_cache_hit_tokens"));
    }
    // 联网搜索次数提取（Responses API 等）
    assign_max(
        &mut u.web_search,
        usage
            .get("tool_usage")
            .and_then(|t| t.get("web_search"))
            .map(json_i32)
            .unwrap_or(0),
    );
}

/// 从已解析的 JSON Value 中提取 usage，累加更新到 u 中
pub fn parse_usage_from_value(u: &mut UsageTokens, v: &Value) -> bool {
    let mut found = false;
    // 1. OpenAI / Volcengine / Anthropic / TokenHub（根级 usage 或 tokenhub_usage）
    if let Some(usage) = v.get("usage").or_else(|| v.get("tokenhub_usage")) {
        apply_usage_max(u, usage);
        found = true;
    }
    // 1b. Anthropic message_start：usage 嵌套在 message 对象内
    if let Some(usage) = v.get("message").and_then(|m| m.get("usage")) {
        apply_usage_max(u, usage);
        found = true;
    }
    // 1c. Responses API (response.completed) / Gemini Interactions (interaction.completed)
    if let Some(usage) = v
        .get("response")
        .or_else(|| v.get("interaction"))
        .and_then(|r| r.get("usage"))
    {
        apply_usage_max(u, usage);
        found = true;
    }
    // Google Gemini
    if let Some(usage) = v.get("usageMetadata") {
        apply_usage_max(u, &gemini_usage_metadata_to_openai(usage));
        found = true;
    }
    // 腾讯云任务 Output.Usage
    if !found {
        if let Some(usage) = tencent_output_usage(v) {
            apply_usage_max(u, &tencent_usage_to_openai(usage));
            found = true;
        }
    }
    // 3. 包裹格式: { code, data: { usage: {...} } }
    if !found {
        if let Some(usage) = v.get("data").and_then(|d| d.get("usage")) {
            apply_usage_max(u, usage);
            found = true;
        }
    }
    found
}

pub fn parse_usage(response: &str) -> UsageTokens {
    let mut u = UsageTokens::default();

    if let Ok(v) = serde_json::from_str::<Value>(response) {
        parse_usage_from_value(&mut u, &v);
    } else {
        // SSE流的情况下按行解析（兼容有无 data: 前缀的情况）
        for line in response.lines() {
            let Some(json_str) = sse_json_payload(line) else {
                continue;
            };
            if let Ok(v) = serde_json::from_str::<Value>(json_str) {
                parse_usage_from_value(&mut u, &v);
            }
        }
    }

    // 下游仍依赖 total 与 completion：缺省 total 时由输入输出正向汇总；若仅有 total（如混元生图/视频等），总量归入 completion
    if u.total == 0 {
        u.total = u.prompt + u.completion;
    } else if u.prompt == 0 && u.completion == 0 {
        u.completion = u.total;
    }

    u
}

pub fn extract_usage_json_string(response: &str) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<Value>(response) {
        // 仅提取 usage 节点，不返回完整响应体（避免存入 choices 等大量聊天内容）
        if let Some(usage) = v.get("usage") {
            return Some(serde_json::json!({ "usage": usage }).to_string());
        }
        if let Some(usage) = v.get("tokenhub_usage") {
            return Some(serde_json::json!({ "usage": usage }).to_string());
        }
        if let Some(usage) = v.get("usageMetadata") {
            return Some(serde_json::json!({ "usageMetadata": usage }).to_string());
        }
        // Responses API: response.usage
        if let Some(usage) = v.get("response").and_then(|r| r.get("usage")) {
            return Some(serde_json::json!({ "usage": usage }).to_string());
        }
        if let Some(usage) = v.get("data").and_then(|d| d.get("usage")) {
            return Some(serde_json::json!({ "usage": usage }).to_string());
        }
        if let Some(usage) = tencent_output_usage(&v) {
            return Some(
                serde_json::json!({ "usage": tencent_usage_to_openai(usage) }).to_string(),
            );
        }
    } else {
        // SSE 模式下，寻找最后一条包含 usage 字段的 chunk，仅提取 usage 部分
        let mut last_usage_json = None;
        for line in response.lines() {
            let Some(json_str) = sse_json_payload(line) else {
                continue;
            };

            if let Ok(v) = serde_json::from_str::<Value>(json_str) {
                if let Some(usage) = v.get("usage") {
                    last_usage_json = Some(serde_json::json!({ "usage": usage }).to_string());
                } else if let Some(usage) = v
                    .get("response")
                    .or_else(|| v.get("interaction"))
                    .and_then(|r| r.get("usage"))
                {
                    last_usage_json = Some(serde_json::json!({ "usage": usage }).to_string());
                } else if let Some(usage) = v.get("usageMetadata") {
                    last_usage_json =
                        Some(serde_json::json!({ "usageMetadata": usage }).to_string());
                }
            }
        }
        if last_usage_json.is_some() {
            return last_usage_json;
        }
    }
    None
}

/// 从可灵官方 3.0 视频终态响应中提取实际生成时长（秒）：data[0].outputs[0].duration
fn extract_kling_video_duration(resp: &Value) -> Option<f64> {
    resp.pointer("/data/0/outputs/0/duration")
        .and_then(parse_json_f64)
        .filter(|&d| d > 0.0)
}

/// 腾讯 VOD 生视频终态结算：产出 `MetaData.Duration/Width/Height`（短边定档），
/// 且在 MetaData 无时长时使用 Output.Usage.OutputSeconds 兜底。
/// 不读 `Resolution`（终态常为空，且请求侧可能非法）。
fn extract_tencent_vod_video_settlement(resp: &Value) -> (Option<f64>, Option<String>) {
    let task = tencent_aigc_task(resp);
    let meta = task.and_then(|t| t.pointer("/Output/FileInfos/0/MetaData"));
    let duration = meta
        .and_then(|m| m.get("Duration"))
        .and_then(parse_json_f64)
        .filter(|&d| d > 0.0)
        .or_else(|| {
            tencent_output_usage(resp)
                .and_then(|u| u.get("OutputSeconds"))
                .and_then(parse_json_f64)
                .filter(|&d| d > 0.0)
        });
    let resolution = match meta {
        Some(m) => match (
            m.get("Width").and_then(parse_json_u64),
            m.get("Height").and_then(parse_json_u64),
        ) {
            (Some(w), Some(h)) if w > 0 && h > 0 => {
                let short = w.min(h);
                Some(match short {
                    1440 => "2k".to_string(),
                    2160 => "4k".to_string(),
                    _ => format!("{}p", short),
                })
            }
            _ => None,
        },
        None => None,
    };
    (duration, resolution)
}

fn parse_json_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_i64().filter(|&i| i >= 0).map(|i| i as u64))
        .or_else(|| {
            v.as_f64()
                .filter(|&f| f.is_finite() && f >= 0.0)
                .map(|f| f as u64)
        })
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn parse_json_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .filter(|f| f.is_finite())
}

/// 分辨率标签规范化：`720P`→`720p`，`1K`→`1k`，纯数字 `720`→`720p`。
pub fn normalize_resolution_label(raw: &str) -> String {
    let mut res = raw.trim().to_lowercase().replace('*', "x");
    if !res.is_empty() && res.chars().all(|c| c.is_ascii_digit()) {
        res.push('p');
    }
    res
}

#[inline]
fn json_res_string(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s.to_string())
    } else {
        v.as_i64()
            .or_else(|| v.as_u64().map(|n| n as i64))
            .filter(|&n| n > 0)
            .map(|n| n.to_string())
    }
}

/// 从请求体提取规范化分辨率（模型别名路由与请求特征提取共用）。
/// 精准支持：根级 resolution、parameters/settings、MiniMax OutputConfig、
/// 腾讯云 AiTryOnConfig、谷歌 generationConfig/imageConfig/response_format，以及 OpenAI 根级 size。
pub fn extract_resolution(body: &Value) -> Option<String> {
    let raw = body
        .get("resolution")
        .and_then(json_res_string)
        .or_else(|| body.pointer("/parameters/resolution").and_then(json_res_string))
        .or_else(|| body.pointer("/settings/resolution").and_then(json_res_string))
        .or_else(|| body.pointer("/OutputConfig/Resolution").and_then(json_res_string))
        .or_else(|| body.pointer("/ImageTask/AiTryOnConfig/Resolution").and_then(json_res_string))
        .or_else(|| body.pointer("/generationConfig/imageConfig/imageSize").and_then(json_res_string))
        .or_else(|| body.pointer("/generation_config/image_config/image_size").and_then(json_res_string))
        .or_else(|| body.pointer("/response_format/image_size").and_then(json_res_string));

    if let Some(r) = raw {
        let normalized = normalize_resolution_label(&r);
        if !normalized.is_empty() {
            return Some(normalized);
        }
    }

    body.get("size")
        .and_then(Value::as_str)
        .and_then(parse_pixel_resolution)
}

/// 从像素尺寸字符串识别分辨率等级（从 forward.rs 移入本特征提取模块）。
/// 支持像素格式（如 "1024x1024"、"2048×1536"）和已有的等级格式（如 "1k"、"2k"、"4k"）。
/// 按最小边长严格判定区间（图片质量由短边决定）：
///   - ≤1024  → "1k"（标准，如 512x512, 1024x1024, 2048x1024）
///   - ≤2048  → "2k"（高清，如 2048x1536, 2048x2048）
///   - >2048  → "4k"（超高清，如 3840x2160, 4096x2304）
/// 比例格式（如 "1:1"）和无法解析的输入返回 None。
pub fn parse_pixel_resolution(size: &str) -> Option<String> {
    let s = size.trim().to_lowercase();
    // 已经是分辨率等级格式，直接返回其小写
    if s.ends_with('k') {
        return Some(s);
    }
    // 比例格式（含 ':'）不属于像素分辨率，返回 None
    if s.contains(':') {
        return None;
    }
    // 解析像素尺寸：支持 x、*、× (Unicode 乘号) 分隔符
    let (w_str, h_str) = s
        .split_once('x')
        .or_else(|| s.split_once('*'))
        .or_else(|| s.split_once('×'))?;
    let w = w_str.trim().parse::<u32>().ok()?;
    let h = h_str.trim().parse::<u32>().ok()?;
    let min_edge = w.min(h);
    // 按最小边长判定分辨率等级区间
    Some(if min_edge <= 1024 {
        "1k".to_string()
    } else if min_edge <= 2048 {
        "2k".to_string()
    } else {
        "4k".to_string() // >2048 统一归为 4k
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_input_tokens_claude() {
        let u = UsageTokens {
            prompt: 100,
            completion: 50,
            cached: 200,
            cache_creation: 300,
            has_total_tokens: false,
            ..Default::default()
        };
        // Claude 独立缓存语义：上下文总输入 = prompt + cached + cache_creation
        assert_eq!(u.context_input_tokens(), 600);
    }

    #[test]
    fn test_context_input_tokens_openai() {
        let u = UsageTokens {
            prompt: 500,
            completion: 50,
            cached: 200,
            cache_creation: 0,
            has_total_tokens: true,
            ..Default::default()
        };
        // OpenAI 原生包含缓存语义：返回 prompt 自身
        assert_eq!(u.context_input_tokens(), 500);
    }

    #[test]
    fn test_gemini_nano_banana_usage_and_billing() {
        let raw_gemini_usage = serde_json::json!({
            "promptTokenCount": 257,
            "candidatesTokenCount": 2388,
            "totalTokenCount": 3955,
            "promptTokensDetails": [
                { "modality": "TEXT", "tokenCount": 257 }
            ],
            "candidatesTokensDetails": [
                { "modality": "IMAGE", "tokenCount": 1680 }
            ],
            "thoughtsTokenCount": 1310,
            "serviceTier": "standard"
        });

        let openai_usage = super::gemini_usage_metadata_to_openai(&raw_gemini_usage);
        assert_eq!(openai_usage["prompt_tokens"], 257);
        assert_eq!(openai_usage["completion_tokens"], 3698);
        assert_eq!(openai_usage["completion_tokens_details"]["reasoning_tokens"], 1310);
        assert_eq!(openai_usage["completion_tokens_details"]["image_tokens"], 1680);
        assert_eq!(openai_usage["output_image_tokens"], 1680);

        let mut u = UsageTokens::default();
        super::apply_usage_max(&mut u, &openai_usage);
        assert_eq!(u.prompt, 257);
        assert_eq!(u.completion, 3698);
        assert_eq!(u.output_image_tokens, 1680);
        assert_eq!(u.reasoning_tokens, 1310);

        // 验证全双工多模态计费公式计算结果
        let text_prompt = (u.prompt - u.image_tokens).max(0);
        let thoughts = u.reasoning_tokens;
        let text_completion = (u.completion - u.output_image_tokens - thoughts).max(0);
        let p_rate = 1.5;
        let c_rate = 7.5;
        let img_comp_rate = 30.0;
        let cost_raw = (text_prompt as f64 * p_rate
            + (text_completion + thoughts) as f64 * c_rate
            + u.output_image_tokens as f64 * img_comp_rate)
            / 1_000_000.0;
        assert!((cost_raw - 0.0659205).abs() < 1e-6);
    }

    #[test]
    fn test_gemini_multimodal_billing_with_thoughts_and_image() {
        let raw_gemini_usage = serde_json::json!({
            "promptTokenCount": 6,
            "candidatesTokenCount": 1209,
            "totalTokenCount": 1354,
            "promptTokensDetails": [
                { "modality": "TEXT", "tokenCount": 6 }
            ],
            "candidatesTokensDetails": [
                { "modality": "IMAGE", "tokenCount": 1120 }
            ],
            "thoughtsTokenCount": 139,
            "serviceTier": "standard"
        });

        let openai_usage = super::gemini_usage_metadata_to_openai(&raw_gemini_usage);
        let mut u = UsageTokens::default();
        super::apply_usage_max(&mut u, &openai_usage);

        assert_eq!(u.prompt, 6);
        assert_eq!(u.output_image_tokens, 1120);
        assert_eq!(u.reasoning_tokens, 139);

        let text_prompt = (u.prompt - u.image_tokens).max(0);
        let text_completion = (u.completion - u.output_image_tokens).max(0);
        let thoughts = u.reasoning_tokens.min(text_completion);
        let pure_text = text_completion - thoughts;
        assert_eq!(text_prompt, 6);
        assert_eq!(pure_text, 89);
        assert_eq!(thoughts, 139);
        assert_eq!(text_prompt + pure_text + thoughts + u.output_image_tokens, 1354);

        let p_rate = 0.1;
        let c_rate = 0.05;
        let img_comp_rate = 0.4;
        let cost_raw = (text_prompt as f64 * p_rate
            + text_completion as f64 * c_rate
            + u.output_image_tokens as f64 * img_comp_rate)
            / 1_000_000.0;
        assert!((cost_raw - 0.00046).abs() < 1e-8);
    }

    #[test]
    fn test_extract_resolution_comprehensive() {
        use super::extract_resolution;

        // 1. 根级字符串与数字型 resolution
        assert_eq!(
            extract_resolution(&serde_json::json!({ "resolution": "1080p" })),
            Some("1080p".into())
        );
        assert_eq!(
            extract_resolution(&serde_json::json!({ "resolution": 720 })),
            Some("720p".into())
        );

        // 2. 阿里 DashScope parameters & 快手可灵 settings
        assert_eq!(
            extract_resolution(&serde_json::json!({ "parameters": { "resolution": "720P" } })),
            Some("720p".into())
        );
        assert_eq!(
            extract_resolution(&serde_json::json!({ "settings": { "resolution": "1080p" } })),
            Some("1080p".into())
        );

        // 3. MiniMax OutputConfig 与腾讯云 AiTryOnConfig
        assert_eq!(
            extract_resolution(&serde_json::json!({ "OutputConfig": { "Resolution": "1080P" } })),
            Some("1080p".into())
        );
        assert_eq!(
            extract_resolution(
                &serde_json::json!({ "ImageTask": { "AiTryOnConfig": { "Resolution": "1K" } } })
            ),
            Some("1k".into())
        );

        // 4. 谷歌官方 imageConfig（同时支持驼峰与下划线）
        assert_eq!(
            extract_resolution(&serde_json::json!({
                "generationConfig": { "imageConfig": { "imageSize": "2K" } }
            })),
            Some("2k".into())
        );
        assert_eq!(
            extract_resolution(&serde_json::json!({
                "generation_config": { "image_config": { "image_size": "2K" } }
            })),
            Some("2k".into())
        );

        // 5. OpenAI 根级 size
        assert_eq!(
            extract_resolution(&serde_json::json!({ "size": "1024x1024" })),
            Some("1k".into())
        );
        assert_eq!(
            extract_resolution(&serde_json::json!({ "size": "3840x2160" })),
            Some("4k".into())
        );
    }

    #[test]
    fn test_tokenhub_hunyuan_image_count_and_usage() {
        let v3_resp = r#"{
            "created": 1785125529,
            "data": [{ "url": "https://example.com/v3.png" }],
            "tokenhub_usage": { "total_tokens": 1024 }
        }"#;
        assert_eq!(count_response_images(v3_resp), Some(1));
        let u3 = parse_usage(v3_resp);
        assert_eq!(u3.total, 1024);
        assert_eq!(u3.completion, 1024);
        assert_eq!(u3.prompt, 0);

        let v35_resp = r#"{
            "choices": [{
                "delta": {
                    "type": "image",
                    "image": { "url": "https://example.com/v35.png" }
                }
            }],
            "usage": { "total_tokens": 20000 }
        }"#;
        assert_eq!(count_response_images(v35_resp), Some(1));
        let u35 = parse_usage(v35_resp);
        assert_eq!(u35.total, 20000);
        assert_eq!(u35.completion, 20000);
        assert_eq!(u35.prompt, 0);
    }

    #[test]
    fn test_google_interactions_usage_and_images() {
        let resp = r#"{
            "id": "v1_inter_123",
            "object": "interaction",
            "status": "completed",
            "steps": [{
                "type": "model_output",
                "content": [
                    { "type": "text", "text": "Here is your image" },
                    { "type": "image", "data": "iVBORw0KGgoAAAANSUhEUg==", "mime_type": "image/png" }
                ]
            }],
            "usage": {
                "total_input_tokens": 300,
                "total_output_tokens": 1290,
                "total_thought_tokens": 110,
                "total_cached_tokens": 50,
                "total_tokens": 1700,
                "input_tokens_by_modality": [{ "modality": "image", "tokens": 258 }],
                "output_tokens_by_modality": [{ "modality": "image", "tokens": 1290 }]
            }
        }"#;
        assert_eq!(count_response_images(resp), Some(1));
        let u = parse_usage(resp);
        assert_eq!(u.prompt, 300);
        assert_eq!(u.completion, 1400);
        assert_eq!(u.reasoning_tokens, 110);
        assert_eq!(u.cached, 50);
        assert_eq!(u.image_tokens, 258);
        assert_eq!(u.output_image_tokens, 1290);
        assert_eq!(u.total, 1700);

        let req = serde_json::json!({
            "model": "gemini-3.1-flash-image-preview",
            "input": [
                { "type": "text", "text": "Edit this image" },
                { "type": "image", "data": "abc", "mime_type": "image/png" }
            ],
            "response_format": { "type": "image", "image_size": "2K" }
        });
        let feats = extract_request_features(&req);
        assert_eq!(feats.image_ref_count, Some(1));
        assert_eq!(extract_resolution(&req), Some("2k".into()));

        // 验证 SSE 流式下 step.delta 图片计数与 interaction.completed 用量提取
        let sse_resp = concat!(
            "event: step.start\n",
            "data: {\"index\":1,\"step\":{\"type\":\"model_output\"},\"event_type\":\"step.start\"}\n\n",
            "event: step.delta\n",
            "data: {\"index\":1,\"delta\":{\"mime_type\":\"image/jpeg\",\"data\":\"/9j/4AAQSkZJRg==\",\"type\":\"image\"},\"event_type\":\"step.delta\"}\n\n",
            "event: step.stop\n",
            "data: {\"index\":1,\"event_type\":\"step.stop\"}\n\n",
            "event: interaction.completed\n",
            "data: {\"interaction\":{\"id\":\"v1_inter_sse\",\"status\":\"completed\",\"usage\":{\"total_tokens\":1401,\"total_input_tokens\":3,\"total_output_tokens\":1398,\"output_tokens_by_modality\":[{\"modality\":\"image\",\"tokens\":1290}]}}}\n\n",
            "data: [DONE]\n\n"
        );
        assert_eq!(count_response_images(sse_resp), Some(1));
        let sse_u = parse_usage(sse_resp);
        assert_eq!(sse_u.prompt, 3);
        assert_eq!(sse_u.completion, 1398);
        assert_eq!(sse_u.output_image_tokens, 1290);
        assert_eq!(sse_u.total, 1401);
    }
}

