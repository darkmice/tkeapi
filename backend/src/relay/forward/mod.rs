/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */
//! 统一转发规则解析器：规则解析、鉴权、URL、厂商 body 分发。
//!
//! ## 目录
//! - `mod.rs` — `ResolvedForward`、规则解析、鉴权、URL、usage/res_mul、mask_key
//! - `transform.rs` — `transform_request_body` 按 target_type 分发
//! - `media.rs` — 图片/multipart/URL/base64 共享 helper
//! - `official_client.rs` — 客户端官方路由（如 MiniMax /v2）协议适配与信封封装
//! - `poll_enrich.rs` — 轮询后处理扩展点（fal 队列等）
//! - `sse.rs` — SSE 流式转换
//! - `vendors/` — 各厂商请求体（kling / tencent / jimeng / fal / standard）
//!
//! ## 扩展
//! 新厂商 → `vendors/{name}.rs`；新 target_type → `transform.rs` 加 match arm。
//! 队列类轮询补齐 → `poll_enrich.rs` + vendor 内实现，勿堆在 `task.rs`。
//! 新规则字段 → 只改 `ResolvedForward` + `parse_forward_config`。
//! 共享媒体逻辑放 `media.rs`；鉴权差异只进 `build_auth_headers` / `apply_request_auth`。
//!
//! ## 契约
//! `crate::relay::forward::*` 路径与签名不可破坏。

use super::url_utils::join_url;
use crate::AppState;
use std::collections::HashMap;

mod media;
mod official_client;
mod poll_enrich;
mod sse;
mod transform;
mod vendors;

pub use media::*;
pub use official_client::*;
pub use poll_enrich::{enrich_client_poll, enrich_poll_response, ClientPollCtx};
pub use sse::*;
pub use transform::*;
pub use vendors::*;

use vendors::generate_kling_jwt;
// ── 解析后的转发配置 ──────────────────────────────────────────

/// 转发规则解析结果
#[derive(Debug, Clone)]
pub struct ResolvedForward {
    /// 目标协议类型: "openai", "volcengine", "volcengine_chat", "gemini", "gemini_image", "anthropic", "kling", "kling_video", "jimeng_image", "jimeng_video", "minimax_image", "minimax_video", "fal_video", "tencent_vod_image", "tencent_vod_video", "tencent_mps_image", "tencent_mps_create_image"
    pub target_type: String,
    /// 上游路径 e.g. "/api/v3/chat/completions"
    pub upstream_path: String,
    /// 鉴权方式: "bearer", "query_key", "x-goog-api-key", "x-api-key", "key"（Authorization: Key）
    pub auth_type: String,
    /// 是否启用素材 URL→素材ID 自动转换（火山方舟视频素材专用）
    pub asset_convert: bool,
    /// 素材转换使用的插件命名空间（默认 asset_manager，国际版可设为 asset_manager_intl）
    pub asset_convert_ns: String,
    /// 是否启用上游渠道素材转换（与 asset_convert 正交，默认关闭）
    pub upstream_asset_convert: bool,
    /// 上游素材绑定 ID（upstream_asset_bindings.id）
    pub upstream_asset_binding_id: Option<i64>,
    /// 异步任务轮询路径 (可选)，如果规则里配置了则优先使用
    pub poll_path: Option<String>,
    /// 是否启用免审核策略
    pub asset_moderation: bool,
    /// 转发规则 EID（供日志记录，避免二次查库）
    pub eid: String,
    /// 关联的数据库模型唯一标识 (系统内持久不可变的唯一 mid，如 vve-sd/vve-pf 等)
    pub mid: Option<String>,
    /// 是否为级联转发模型（二阶段级联执行）
    pub is_cascade: bool,
    /// 级联阶段二引擎：`volc`（缺省）或 `tencent`
    pub cascade_engine: String,
    /// 级联：目标 720p 且底座为 480p 时，是否 MediaKit 居中裁成标准 480p；缺省 true（兼容现网）；其它分辨率忽略
    pub crop_480p: bool,
    /// 级联：裁剪时机，"pre" (超分前 480p) 或 "post" (超分后 720p)；缺省 "pre"
    pub crop_timing: String,
    /// 级联：480p 裁剪坐标（比例 "16:9" / "9:16" → (tlx, tly, brx, bry)）；缺省为 seedance2.0 标准
    pub crop_coords_480p: HashMap<String, (i32, i32, i32, i32)>,
    /// 级联：720p 裁剪坐标（比例 "16:9" / "9:16" → (tlx, tly, brx, bry)）；缺省为 seedance2.5 标准
    pub crop_coords_720p: HashMap<String, (i32, i32, i32, i32)>,
    /// 是否将 content 字段提取为 prompt（针对火山视频某些上游通道特判兼容）
    pub content_to_prompt: bool,
    /// 级联分辨率倍率表（config_json.res_mul）；阶段二：有 usage 则乘入 token，否则乘费用；空表=1.0
    pub res_mul: HashMap<String, f64>,
    /// 级联：各分辨率是否启用增强（缺省/未配置默认为 true；关闭后不走 S2 增强但 usage/cost 仍按 res_mul 放大）
    pub res_enhance_enabled: HashMap<String, bool>,
    /// 级联：目标分辨率 → 增强版本（fast|standard|pro|ai）；ai 支持 720p/1080p/2k/4k；缺 key/非法 → 标准版
    pub res_enhance: HashMap<String, String>,
    /// 级联：目标分辨率 → 阶段一座底分辨率；缺 key / 非法则用默认一级底座
    pub res_base: HashMap<String, String>,
    /// 级联：目标分辨率 → 标准版增强场景（common|ugc|short_series|aigc|old_film）；仅 standard 生效
    pub res_scene: HashMap<String, String>,
    /// 级联：目标分辨率 → 短边像素限制（resolution_limit，仅标准版/专业版生效）；0 或未配置则不限制
    pub res_limit: HashMap<String, i64>,
    /// 级联：大模型增强遇短边像素限制时的回退档位（pro|standard|fast）；缺省 pro
    pub res_limit_fallback: String,
    /// path_passthrough 命中：入口路径原样上游，请求体仅替换 model
    #[allow(dead_code)]
    pub body_passthrough: bool,
    /// 是否启用异步任务优先快返（先 200 返回受理，后台异步转素材并调用上游；缺省 false 保持同步上游等待）
    pub fast_response: bool,
}

impl Default for ResolvedForward {
    fn default() -> Self {
        Self {
            target_type: "openai".to_string(),
            upstream_path: String::new(),
            auth_type: "bearer".to_string(),
            asset_convert: false,
            asset_convert_ns: "asset_manager".to_string(),
            upstream_asset_convert: false,
            upstream_asset_binding_id: None,
            poll_path: None,
            asset_moderation: false,
            eid: String::new(),
            mid: None,
            is_cascade: false,
            cascade_engine: "volc".to_string(),
            crop_480p: true,
            crop_timing: "pre".to_string(),
            crop_coords_480p: HashMap::new(),
            crop_coords_720p: HashMap::new(),
            content_to_prompt: false,
            res_mul: HashMap::new(),
            res_enhance_enabled: HashMap::new(),
            res_enhance: HashMap::new(),
            res_base: HashMap::new(),
            res_scene: HashMap::new(),
            res_limit: HashMap::new(),
            res_limit_fallback: "pro".to_string(),
            body_passthrough: false,
            fast_response: false,
        }
    }
}

impl ResolvedForward {
    /// 目标分辨率是否启用阶段二画质增强（缺省或未配置时默认为 true）
    #[inline]
    pub fn is_res_enhance_enabled(&self, res: &str) -> bool {
        self.res_enhance_enabled
            .get(&normalize_res_mul_key(res))
            .copied()
            .unwrap_or(true)
    }
}

/// 路径比较（忽略首部 `/`，允许 req 以 candidate 结尾，支持 ${model} 占位宏匹配）— 用于 path_rewrite 评分
fn path_matches(req: &str, candidate: &str) -> bool {
    let req = req.trim_start_matches('/');
    let cand = candidate.trim_start_matches('/');
    if cand.is_empty() {
        return false;
    }
    if req == cand || req.ends_with(cand) {
        return true;
    }
    // 支持包含 ${model} 占位宏的模式匹配（如 /v1/models/${model}:generateContent）
    if let Some((prefix, suffix)) = cand.split_once("${model}") {
        if req.starts_with(prefix)
            && req.ends_with(suffix)
            && req.len() >= prefix.len() + suffix.len()
        {
            return true;
        }
    }
    false
}

/// 路径精确相等（忽略首部 `/`）— 用于 path_passthrough 入口列表
fn path_eq(a: &str, b: &str) -> bool {
    a.trim_start_matches('/') == b.trim_start_matches('/')
}

/// Anthropic Messages 入口：官方 `/v1/messages`、MiniMax `/anthropic/v1/messages` 或火山方舟 `/api/compatible/v1/messages`
#[inline]
pub(crate) fn is_anthropic_messages_path(path: &str) -> bool {
    path_eq(path, "/v1/messages")
        || path_eq(path, "/anthropic/v1/messages")
        || path_eq(path, "/api/compatible/v1/messages")
}

/// Google Gemini Interactions API 官方路由：`/v1/interactions` 或 `/v1beta/interactions`
#[inline]
pub(crate) fn is_google_interactions_path(path: &str) -> bool {
    let clean = path.split(['|', '?']).next().unwrap_or(path).trim_matches('/');
    clean == "v1/interactions" || clean == "v1beta/interactions"
}

/// Google Gemini 官方原生入口：`/v1(beta)/models/*` 或 `/v1(beta)/interactions`
#[inline]
pub(crate) fn is_google_native_path(path: &str) -> bool {
    let clean = path.split(['|', '?']).next().unwrap_or(path).trim_start_matches('/');
    clean.starts_with("v1/models/")
        || clean.starts_with("v1beta/models/")
        || is_google_interactions_path(clean)
}

#[inline]
fn parse_rewrite_pair(item: &serde_json::Value) -> Option<(&str, &str)> {
    let o = item.get("old").and_then(|v| v.as_str()).unwrap_or("");
    let n = item.get("new").and_then(|v| v.as_str()).unwrap_or("");
    if !o.is_empty() || !n.is_empty() {
        Some((o, n))
    } else {
        None
    }
}

/// 提取规则中的路径重写列表（优先读取 `path_rewrites` 数组，回退到单个 `path_rewrite` 对象）
pub(crate) fn collect_rewrites(config: &serde_json::Value) -> Vec<(&str, &str)> {
    if let Some(arr) = config.get("path_rewrites").and_then(|v| v.as_array()) {
        arr.iter().filter_map(parse_rewrite_pair).collect()
    } else if let Some(pair) = config.get("path_rewrite").and_then(parse_rewrite_pair) {
        vec![pair]
    } else {
        vec![]
    }
}

/// config_json 字符串数组字段是否包含当前请求路径（精确相等）
fn path_list_hit(config: &serde_json::Value, key: &str, request_path: &str) -> bool {
    config
        .get(key)
        .and_then(|v| v.as_array())
        .is_some_and(|arr| {
            arr.iter()
                .any(|p| p.as_str().is_some_and(|s| path_eq(request_path, s)))
        })
}

/// path_passthrough：命中则上游路径=入口且 body 仅换 model
fn path_passthrough_hit(config: &serde_json::Value, request_path: &str) -> bool {
    path_list_hit(config, "path_passthrough", request_path)
}

/// path_accept：命中则规则可匹配，仍走 path_rewrite.new（不做入口原样上游）
fn path_accept_hit(config: &serde_json::Value, request_path: &str) -> bool {
    path_list_hit(config, "path_accept", request_path)
}

/// 按目标分辨率查级联倍率；无表/无 key/非法值 → 1.0
pub fn lookup_res_mul(map: &HashMap<String, f64>, resolution: &str) -> f64 {
    let key = normalize_res_mul_key(resolution);
    map.get(&key).copied().filter(|&v| v > 0.0).unwrap_or(1.0)
}

fn scale_json_num_field(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    mult: f64,
) {
    let Some(v) = obj.get_mut(key) else { return };
    let Some(n) = v.as_i64() else { return };
    if n != 0 {
        *v = serde_json::json!(((n as f64) * mult).round() as i64);
    }
}

fn scale_usage_token_object(obj: &mut serde_json::Map<String, serde_json::Value>, mult: f64) {
    for key in [
        "prompt_tokens",
        "completion_tokens",
        "total_tokens",
        "input_tokens",
        "output_tokens",
        "image_tokens",
        "output_image_tokens",
    ] {
        scale_json_num_field(obj, key, mult);
    }
}

/// 级联阶段二：放大响应中的 usage token（返回 / 落库 / 结算共用）
pub fn scale_usage_in_json(root: &mut serde_json::Value, mult: f64) {
    if (mult - 1.0).abs() <= 1e-9 {
        return;
    }
    for ptr in ["/usage", "/data/usage"] {
        if let Some(serde_json::Value::Object(obj)) = root.pointer_mut(ptr) {
            scale_usage_token_object(obj, mult);
        }
    }
}

fn normalize_res_mul_key(resolution: &str) -> String {
    let s = resolution.trim().to_lowercase();
    match s.as_str() {
        "480" | "480p" => "480p".to_string(),
        "540" | "540p" => "540p".to_string(),
        "720" | "720p" => "720p".to_string(),
        "768" | "768p" => "768p".to_string(),
        "1080" | "1080p" => "1080p".to_string(),
        "2k" | "2kp" => "2k".to_string(),
        "4k" | "4kp" => "4k".to_string(),
        _ => s,
    }
}

fn parse_res_mul(config: &serde_json::Value) -> HashMap<String, f64> {
    let Some(obj) = config.get("res_mul").and_then(|v| v.as_object()) else {
        return HashMap::new();
    };
    obj.iter()
        .filter_map(|(k, v)| {
            let rate = v.as_f64().filter(|&r| r > 0.0)?;
            Some((normalize_res_mul_key(k), rate))
        })
        .collect()
}

/// 解析级联字符串映射（res_enhance / res_base / res_scene）；非空值转小写，合法性在 cascade_resolve_* 校验
fn parse_res_str_map(config: &serde_json::Value, field: &str) -> HashMap<String, String> {
    let Some(obj) = config.get(field).and_then(|v| v.as_object()) else {
        return HashMap::new();
    };
    obj.iter()
        .filter_map(|(k, v)| {
            let val = v.as_str()?.trim();
            if val.is_empty() {
                return None;
            }
            Some((normalize_res_mul_key(k), val.to_ascii_lowercase()))
        })
        .collect()
}

/// 解析级联布尔映射（res_enhance_enabled，各分辨率是否启用增强）
fn parse_res_bool_map(config: &serde_json::Value, field: &str) -> HashMap<String, bool> {
    let Some(obj) = config.get(field).and_then(|v| v.as_object()) else {
        return HashMap::new();
    };
    obj.iter()
        .filter_map(|(k, v)| {
            let b = v.as_bool()?;
            Some((normalize_res_mul_key(k), b))
        })
        .collect()
}

fn normalize_res_limit_key(key: &str) -> String {
    let s = key.trim().to_lowercase();
    if let Some((res, ratio)) = s.split_once('@') {
        format!("{}@{}", normalize_res_mul_key(res), ratio.trim())
    } else {
        normalize_res_mul_key(&s)
    }
}

/// 解析级联整型映射（res_limit，短边像素限制，必须为正整数）
fn parse_res_i64_map(config: &serde_json::Value, field: &str) -> HashMap<String, i64> {
    let Some(obj) = config.get(field).and_then(|v| v.as_object()) else {
        return HashMap::new();
    };
    obj.iter()
        .filter_map(|(k, v)| {
            let n = v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))?;
            if n <= 0 {
                return None;
            }
            Some((normalize_res_limit_key(k), n))
        })
        .collect()
}

/// 解析裁剪矩形坐标（支持 [x1, y1, x2, y2] 或 "x1, y1, x2, y2"）
fn parse_rect_coords(val: &serde_json::Value) -> Option<(i32, i32, i32, i32)> {
    if let Some(arr) = val.as_array() {
        if arr.len() == 4 {
            let x1 = arr[0].as_i64()? as i32;
            let y1 = arr[1].as_i64()? as i32;
            let x2 = arr[2].as_i64()? as i32;
            let y2 = arr[3].as_i64()? as i32;
            if x2 > x1 && y2 > y1 {
                return Some((x1, y1, x2, y2));
            }
        }
    } else if let Some(s) = val.as_str() {
        let parts: Vec<i32> = s
            .split(&[',', ' ', ';'][..])
            .filter(|p| !p.trim().is_empty())
            .filter_map(|p| p.trim().parse::<i32>().ok())
            .collect();
        if parts.len() == 4 && parts[2] > parts[0] && parts[3] > parts[1] {
            return Some((parts[0], parts[1], parts[2], parts[3]));
        }
    }
    None
}

/// 解析级联裁剪坐标映射（crop_coords_480p / crop_coords_720p）
fn parse_crop_coords(
    config: &serde_json::Value,
    field: &str,
) -> HashMap<String, (i32, i32, i32, i32)> {
    let mut map = HashMap::new();
    let Some(obj) = config.get(field).and_then(|v| v.as_object()) else {
        return map;
    };
    for (k, v) in obj {
        let ratio = k.trim();
        if let Some(coords) = parse_rect_coords(v) {
            map.insert(ratio.to_string(), coords);
        }
    }
    map
}

// ── 转发规则解析 ──────────────────────────────────────────────

/// 根据模型 ID、请求类别、入口路径，从 DB 查找匹配的转发规则。
///
/// 逻辑：
/// 1. 查 models 表取 forward_rule_ids（JSON 数组如 [1,5,8]）
/// 2. 查 forward_rules 表，筛选 category 匹配且 is_active=1
/// 3. 如果有多条同类别规则，从 config_json.path_rewrite.old 匹配入口路径
/// 4. 找不到 → 返回 None，调用方按 OpenAI 格式透传
/// db_model: 调用方已查询的模型记录（如来自 check_access_with_model），避免重复查 models 表。
///           传 None 时内部自行查询。
pub async fn resolve_forward_rule(
    state: &AppState,
    model_id: &str,
    category: &str,
    request_path: &str,
    channel: Option<&crate::models::Channel>,
    db_model: Option<&crate::models::Model>,
) -> Option<ResolvedForward> {
    // 根据模型类别定义标准的 OpenAI 基准路径
    let openai_path = super::proxy::category_endpoint(Some(category));

    // 1. 复用调用方已查询的 Model，或自行查库
    let owned_model;
    let model = if let Some(m) = db_model {
        m
    } else {
        owned_model =
            super::proxy::find_active_model_exact(state, model_id, Some(category), channel).await;
        match owned_model.as_ref() {
            Some(m) => m,
            None => {
                crate::relay_debug!("[Forward] 未找到模型 模型='{}' 类别={}", model_id, category);
                return None;
            }
        }
    };

    crate::relay_debug!(
        "[Forward] 命中模型 模型='{}' 真实MID='{}' 类别='{}'",
        model_id,
        model.mid,
        category
    );

    let rule_ids_str = model.forward_rule_ids.as_deref().unwrap_or("[]");
    let rule_ids: Vec<i64> = serde_json::from_str(rule_ids_str).unwrap_or_default();

    // 2. 查所有关联的转发规则
    let mut rules: Vec<crate::models::ForwardRule> = Vec::new();
    if !rule_ids.is_empty() {
        crate::relay_debug!(
            "[Forward] 绑定规则 模型='{}' 规则IDs={:?} 类别={} 路径={}",
            model_id,
            rule_ids,
            category,
            request_path
        );
        let placeholders: Vec<String> = rule_ids.iter().map(|_| "?".to_string()).collect();
        let query_str = format!(
            "SELECT * FROM forward_rules WHERE id IN ({}) AND is_active = 1",
            placeholders.join(",")
        );
        let formatted = state.db.format_query(&query_str);
        let mut q = sqlx::query_as::<_, crate::models::ForwardRule>(&formatted);
        for id in &rule_ids {
            q = q.bind(id);
        }
        rules = q.fetch_all(&state.db.pool).await.unwrap_or_default();
        if rules.is_empty() {
            crate::relay_debug!("[Forward] 未找到规则 规则IDs={:?}", rule_ids);
        }
    } else {
        crate::relay_debug!("[Forward] 未绑定规则 模型='{}' (按协议回退)", model_id);
    }

    // 3. 按 category 筛选（兼容绑定了「封装」分类规则的图片/视频工具模型）
    let category_matched: Vec<&crate::models::ForwardRule> = rules
        .iter()
        .filter(|r| r.category == category || r.category == "封装")
        .collect();

    let candidates = if category_matched.is_empty() {
        rules.iter().collect::<Vec<_>>()
    } else {
        category_matched
    };

    // 4. 按精确度打分匹配：当模型绑定了多条规则时，优先选择与请求路径最精确匹配的规则
    //    评分策略：
    //    3 分 — path_passthrough / path_accept 命中，或请求路径精确匹配 path_rewrite.new/old
    //    2 分 — OpenAI 请求 + 规则为纯透传(old==new)
    //    1 分 — OpenAI 请求 + 规则为转换型(old≠new)
    let mut best: Option<&crate::models::ForwardRule> = None;
    let mut best_score: u8 = 0;
    for rule in &candidates {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&rule.config_json) {
            let score = if path_passthrough_hit(&config, request_path)
                || path_accept_hit(&config, request_path)
            {
                3
            } else {
                let rewrites = collect_rewrites(&config);
                let req_clean = request_path.trim_start_matches('/');
                let openai_clean = openai_path.trim_start_matches('/');
                let is_openai_req = req_clean == openai_clean || req_clean.ends_with(openai_clean);

                let mut max_s = 0u8;
                for (old_path, new_path) in rewrites {
                    let old_clean = old_path.trim_start_matches('/');
                    let new_clean = new_path.trim_start_matches('/');

                    let rule_supports_openai = old_clean.is_empty()
                        || old_clean == openai_clean
                        || openai_clean.ends_with(old_clean);

                    let match_new = path_matches(request_path, new_path);
                    let match_old = path_matches(request_path, old_path);

                    let s = if !is_openai_req && (match_new || match_old) {
                        3
                    } else if is_openai_req && rule_supports_openai && old_clean == new_clean {
                        2
                    } else if is_openai_req && rule_supports_openai {
                        1
                    } else {
                        0
                    };
                    if s > max_s {
                        max_s = s;
                        if max_s == 3 {
                            break;
                        }
                    }
                }
                max_s
            };

            if score > best_score {
                best_score = score;
                best = Some(rule);
                if score == 3 {
                    break;
                }
            }
        }
    }
    // 如果未找到与请求路径严格匹配的规则，则拒绝匹配（不再兜底回落到第一条规则）
    let rule = best?;

    // 5. 统一通过 parse_forward_config 解析 config_json → ResolvedForward
    let config: serde_json::Value = serde_json::from_str(&rule.config_json).unwrap_or_default();
    let resolved = parse_forward_config(
        &config,
        &openai_path,
        request_path,
        &rule.eid,
        Some(model.mid.clone()),
    );
    let asset_convert_label = if resolved.upstream_asset_convert {
        format!(
            "上游(绑定#{})",
            resolved.upstream_asset_binding_id.unwrap_or(0)
        )
    } else if resolved.asset_convert {
        format!("true({})", resolved.asset_convert_ns)
    } else {
        "false".to_string()
    };
    crate::relay_debug!(
        "[Forward] 命中规则 名称='{}' EID={}: 目标类型={} 上游路径={} 鉴权类型={} \
         素材转换={} 轮询路径={:?} 素材审核={} \
         级联={} 快返={} 裁剪480={} 裁剪时机={} 提示词转换={} 关联MID='{}'",
        rule.name,
        rule.eid,
        resolved.target_type,
        resolved.upstream_path,
        resolved.auth_type,
        asset_convert_label,
        resolved.poll_path,
        resolved.asset_moderation,
        resolved.is_cascade,
        resolved.fast_response,
        resolved.crop_480p,
        resolved.crop_timing,
        resolved.content_to_prompt,
        model.mid
    );
    Some(resolved)
}

/// 按 EID 直接查询转发规则（任务在途轮询快照优先机制）。
/// 当任务已记录 forward_eid 时优先调用，避免模型/子渠热切换对在途任务造成干扰。
pub async fn resolve_forward_rule_by_eid(
    state: &AppState,
    eid: &str,
    category: &str,
    request_path: &str,
    db_model: Option<&crate::models::Model>,
) -> Option<ResolvedForward> {
    let eid_trimmed = eid.trim();
    if eid_trimmed.is_empty() {
        return None;
    }
    let query_str = state.db.format_query(
        "SELECT name, eid, config_json FROM forward_rules WHERE eid = ? AND is_active = 1 LIMIT 1",
    );
    let (rule_name, rule_eid, config_json): (String, String, String) = sqlx::query_as(&query_str)
        .bind(eid_trimmed)
        .fetch_optional(&state.db.pool)
        .await
        .ok()??;

    let openai_path = super::proxy::category_endpoint(Some(category));
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap_or_default();
    let resolved = parse_forward_config(
        &config,
        &openai_path,
        request_path,
        &rule_eid,
        db_model.map(|m| m.mid.clone()),
    );
    crate::relay_debug!(
        "[ForwardByEID] 命中快照规则 名称='{}' EID={}: 目标类型={} 上游路径={} 鉴权类型={} 级联={}",
        rule_name,
        rule_eid,
        resolved.target_type,
        resolved.upstream_path,
        resolved.auth_type,
        resolved.is_cascade
    );
    Some(resolved)
}

/// 快速检查模型是否绑定了转发规则（不做路径匹配）。
/// 配合 resolve_forward_rule 使用：当 resolve 返回 None 时，
/// 若此函数返回 true 说明模型绑定了规则但入口路径不匹配，应拒绝请求。
pub async fn model_has_forward_rules(state: &AppState, model_id: &str) -> bool {
    let ids: Option<String> =
        sqlx::query_scalar(&state.db.format_query(
            "SELECT forward_rule_ids FROM models WHERE model_id = ? AND is_active = 1 AND is_listed = 1",
        ))
        .bind(model_id)
        .fetch_optional(&state.db.pool)
        .await
        .unwrap_or(None);

    match ids {
        Some(s) => {
            let arr: Vec<i64> = serde_json::from_str(&s).unwrap_or_default();
            !arr.is_empty()
        }
        None => false,
    }
}

/// 记录转发规则不支持并返回 400 错误（统一收敛各路由，杜绝重复代码）
pub async fn record_unsupported_forward_error(
    state: &std::sync::Arc<AppState>,
    token: &crate::models::ApiToken,
    channel: &crate::models::Channel,
    model: &str,
    raw_path: &str,
    category: &str,
) -> crate::error::AppError {
    let msg = format!("模型 '{}' 不支持当前接口，请检查模型对应的转发规则", model);
    super::proxy::record_error_log(
        state,
        token,
        Some(channel.id),
        model,
        400,
        raw_path,
        category,
        &msg,
        Some(&channel.base_url),
    )
    .await;
    crate::error::AppError::BadRequest(msg)
}

/// 根据渠道 base_url 修正已解析的 target_type。
/// 当转发规则返回默认的 "openai" 但实际渠道有特殊约束时，覆盖为精确的 target_type。
/// 例如 APIMart（api.apimart.ai）的 size 仅接受比例格式，需识别为 "apimart"。
pub fn refine_target_type(resolved: &mut ResolvedForward, base_url: &str) {
    if resolved.target_type == "openai" {
        let url_lower = base_url.to_lowercase();
        if url_lower.contains("apimart.ai") {
            resolved.target_type = "apimart".to_string();
        }
    }
}

/// 渠道已绑定插件上游时覆盖转发目标（选流细节在插件内）。
pub fn apply_channel_provider(resolved: &mut ResolvedForward, ch: &crate::models::Channel) {
    #[cfg(feature = "plugin_comfyui")]
    crate::api::plugins::comfyui_bridge::apply_forward_provider(resolved, ch);
    #[cfg(not(feature = "plugin_comfyui"))]
    let _ = (resolved, ch);
}

// ── URL 构建 ──────────────────────────────────────────────────

/// 构建上游完整 URL，支持 ${model} 变量替换。
/// 可灵模型会根据已转换的请求体动态调整路径。
pub fn build_upstream_url(
    base_url: &str,
    resolved: &ResolvedForward,
    model: &str,
    api_key: &str,
) -> String {
    // 即梦AI：端点固定拼接 域名+Action+Version
    if resolved.target_type.starts_with("jimeng_") {
        return format!(
            "{}/?Action=CVSync2AsyncSubmitTask&Version=2022-08-31",
            base_url.trim_end_matches('/')
        );
    }

    let path = resolved.upstream_path.replace("${model}", model);

    if resolved.auth_type == "query_key" {
        // Gemini 风格: URL 中带 key 参数
        format!("{}?key={}", join_url(base_url, &path), api_key)
    } else {
        join_url(base_url, &path)
    }
}

/// 可灵动态路径解析：根据已转换的上游请求体内容动态调整端点路径。
/// - `kling`（图片）：有 `subject_image_list` 时自动改写为多图生图端点 `/v1/images/multi-image2image`
/// - `kling_video`（视频 3.0）：有 `contents` → `/image-to-video/${model}`，否则 `/text-to-video/${model}`；Omni 固定
pub fn resolve_kling_dynamic_path(
    resolved: &mut ResolvedForward,
    upstream_body: &serde_json::Value,
) {
    let tt = resolved.target_type.as_str();
    if tt != "kling" && tt != "kling_video" {
        return;
    }
    let path = resolved.upstream_path.as_str();

    // Omni 端点由转发规则直接指定，不做动态调整
    if path.contains("omni-video") || path.contains("omni-image") {
        return;
    }

    // 新协议：文/图共用一条规则，有非空 contents → image-to-video，否则 text-to-video
    if tt == "kling_video"
        && (path.contains("/text-to-video/") || path.contains("/image-to-video/"))
    {
        let suffix = path.rsplit('/').next().unwrap_or("");
        if suffix.is_empty() {
            return;
        }
        let has_contents = upstream_body
            .get("contents")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty());
        let prefix = if has_contents {
            "image-to-video"
        } else {
            "text-to-video"
        };
        resolved.upstream_path = format!("/{}/{}", prefix, suffix);
        return;
    }

    // 旧协议图片动态路由
    if path.contains("images") {
        if upstream_body
            .get("subject_image_list")
            .and_then(|v| v.as_array())
            .map_or(false, |a| !a.is_empty())
        {
            resolved.upstream_path = "/v1/images/multi-image2image".to_string();
        }
    }
}
// ── 鉴权 Header 构建 ──────────────────────────────────────────

/// 根据 auth_type 构建请求 Headers
pub fn build_auth_headers(
    resolved: &ResolvedForward,
    api_key: &str,
    is_post: bool,
) -> Vec<(String, String)> {
    let mut headers = match resolved.auth_type.as_str() {
        "x-api-key" => vec![
            ("x-api-key".to_string(), api_key.to_string()),
            ("anthropic-version".to_string(), "2023-06-01".to_string()),
        ],
        "query_key" => vec![], // key 已在 URL 中
        // Google 原生 API Key Header 鉴权: x-goog-api-key
        "x-goog-api-key" => vec![("x-goog-api-key".to_string(), api_key.trim().to_string())],
        // 火山方舟语音合成 TTS V3：X-Api-Key 鉴权 + X-Api-Resource-Id 指定模型版本
        "volcengine_tts" => vec![("X-Api-Key".to_string(), api_key.to_string())],
        // Authorization: Key <api_key>（fal 等队列厂商通用）
        "key" => vec![(
            "Authorization".to_string(),
            format!("Key {}", api_key.trim()),
        )],
        // Authorization: Token <api_key>（Vidu 等厂商规范）
        "token" => vec![(
            "Authorization".to_string(),
            format!("Token {}", api_key.trim()),
        )],
        _ => {
            // 可灵旧协议(kling)：AccessKey:SecretKey → JWT；新协议(kling_video)：官方 API Key 直传 Bearer
            if resolved.target_type == "kling" {
                if let Some(token) = generate_kling_jwt(api_key) {
                    vec![("Authorization".to_string(), format!("Bearer {}", token))]
                } else {
                    crate::relay_debug!(
                        "[Kling Auth] JWT 生成失败，将直接使用 api_key 作为 Bearer Token"
                    );
                    vec![("Authorization".to_string(), format!("Bearer {}", api_key))]
                }
            } else {
                vec![("Authorization".to_string(), format!("Bearer {}", api_key))]
            }
        }
    };
    // DashScope 异步视频任务需要 X-DashScope-Async: enable，且只能在 POST 提交阶段加入，GET 轮询阶段加入会报错
    if resolved.target_type == "dashscope" && is_post {
        headers.push(("X-DashScope-Async".to_string(), "enable".to_string()));
    }
    headers
}

/// 统一为上游 POST 请求应用鉴权头并设置请求体。
/// 封装所有厂商的认证差异（Bearer/JWT/TC3-HMAC-SHA256/火山引擎 V4 等），
/// 调用方只需传入 builder，无需关心具体协议实现。
///
/// 签名类厂商（腾讯云/即梦）：内部用 .body() 发送已签名 body，确保签名一致。
/// 其他厂商：内部用 .json() 序列化 body。
pub fn apply_request_auth(
    mut builder: reqwest::RequestBuilder,
    resolved: &ResolvedForward,
    api_key: &str,
    upstream_body: &mut serde_json::Value,
    base_url: &str,
) -> reqwest::RequestBuilder {
    match resolved.target_type.as_str() {
        "tencent_vod_image" | "tencent_vod_video" => {
            let action = if resolved.target_type == "tencent_vod_image" {
                "CreateAigcImageTask"
            } else {
                "CreateAigcVideoTask"
            };
            let (ak, sk, sub_app_id) = parse_tencent_vod_key(api_key);
            upstream_body["SubAppId"] = serde_json::json!(sub_app_id);
            let signed_body = serde_json::to_string(upstream_body).unwrap_or_default();
            for (k, v) in build_tencent_vod_headers(ak, sk, action, &signed_body) {
                builder = builder.header(k, v);
            }
            builder.body(signed_body)
        }
        "tencent_mps_image" | "tencent_mps_create_image" => {
            let (ak, sk) = parse_tencent_mps_key(api_key);
            let signed_body = serde_json::to_string(upstream_body).unwrap_or_default();
            for (k, v) in build_tencent_mps_headers(ak, sk, "ProcessImage", &signed_body) {
                builder = builder.header(k, v);
            }
            builder.body(signed_body)
        }
        "jimeng_image" | "jimeng_video" => {
            let (ak, sk) = parse_jimeng_key(api_key);
            let signed_body = serde_json::to_string(upstream_body).unwrap_or_default();
            for (k, v) in
                build_jimeng_headers(ak, sk, "CVSync2AsyncSubmitTask", &signed_body, base_url)
            {
                builder = builder.header(k, v);
            }
            builder.body(signed_body)
        }
        _ => {
            let auth_headers = build_auth_headers(resolved, api_key, true);
            for (k, v) in &auth_headers {
                builder = builder.header(k, v);
            }
            builder.json(upstream_body)
        }
    }
}

/// 目标类型是否为 OpenAI / GPT 协议（仅这两个目标端点向上游转发 multipart 图像编辑）
#[inline]
pub fn supports_multipart_edits(target_type: &str) -> bool {
    matches!(target_type, "openai" | "gpt")
}

/// 统一检测上游 POST 响应的 body 级错误（HTTP 200 但业务失败）。
/// 腾讯云/即梦等厂商 HTTP 状态码始终返回 200，错误信息在 body 中。
/// 返回 Some((错误响应JSON字符串, 转换后的response_content_str)) 表示有错误，None 表示正常。
///
/// 此函数在预扣费之前调用，避免"先扣费再退款"的冗余流程。
pub fn check_upstream_post_error(
    _target_type: &str,
    response_body: &str,
    category: &str,
    is_openai_compat: bool,
) -> (String, Option<String>) {
    let v: serde_json::Value = serde_json::from_str(response_body).unwrap_or_default();
    if super::response_formatter::is_upstream_error_response(&v) {
        if is_openai_compat {
            // OpenAI 兼容请求：通过 format_openai 把错误转换为标准 OpenAI 格式 JSON 返回
            let formatted =
                super::response_formatter::format_openai(category, response_body, false, None);
            (formatted.clone(), Some(formatted))
        } else {
            // 官方原生请求：报错格式原样返回不能转为 OpenAI，但仍需提供 Some(response_body) 触发拦截退费
            (response_body.to_string(), Some(response_body.to_string()))
        }
    } else {
        (response_body.to_string(), None)
    }
}

// ── 默认转发配置（无规则时的 OpenAI 透传）─────────────────────

/// 构建 ResolvedForward，仅需指定 target_type / upstream_path / auth_type，
/// 其余字段自动继承 Default 值（asset_convert=false, poll_path=None 等）。
/// 新增字段只需更新 Default impl，无需修改此函数。
pub fn make_forward(target_type: &str, upstream_path: &str, auth_type: &str) -> ResolvedForward {
    ResolvedForward {
        target_type: target_type.to_string(),
        upstream_path: upstream_path.to_string(),
        auth_type: auth_type.to_string(),
        ..Default::default()
    }
}

/// 获取默认的 OpenAI 格式转发配置
pub fn default_openai_forward(entry_path: &str) -> ResolvedForward {
    make_forward("openai", entry_path, "bearer")
}

/// 从转发规则的 config JSON 解析出 ResolvedForward。
///
/// 这是唯一需要随 ResolvedForward 字段新增而修改的解析入口。
/// 供 resolve_forward_rule() 与 test_channel 等场景共用。
///
/// - `category_path` : 类别 OpenAI 基准路径，供 path_rewrite old→new
/// - `request_path`  : 真实入口；path_passthrough 命中时作为上游路径
pub fn parse_forward_config(
    config: &serde_json::Value,
    category_path: &str,
    request_path: &str,
    eid: &str,
    mid: Option<String>,
) -> ResolvedForward {
    let is_passthrough = path_passthrough_hit(config, request_path);
    // Anthropic Messages 透传（含 MiniMax /anthropic/v1/messages）：统一 anthropic + x-api-key
    let (default_tt, default_auth) = if is_passthrough && is_anthropic_messages_path(request_path) {
        ("anthropic", "x-api-key")
    } else {
        ("openai", "bearer")
    };

    let upstream_path = if is_passthrough {
        request_path.to_string()
    } else {
        let rewrites = collect_rewrites(config);
        if let Some((_, new)) = rewrites
            .iter()
            .find(|(old, new)| path_matches(request_path, new) || path_matches(request_path, old))
        {
            new.to_string()
        } else if let Some((old, new)) = rewrites
            .iter()
            .find(|(old, _)| !old.is_empty() && category_path.contains(*old))
        {
            category_path.replace(old, new)
        } else if let Some((_, new)) = rewrites.iter().find(|(_, new)| !new.is_empty()) {
            new.to_string()
        } else {
            category_path.to_string()
        }
    };

    ResolvedForward {
        target_type: config
            .get("target_type")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(default_tt)
            .to_string(),
        upstream_path,
        auth_type: config
            .get("auth_type")
            .and_then(|v| v.as_str())
            .unwrap_or(default_auth)
            .to_string(),
        asset_convert: config
            .get("asset_convert")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        asset_convert_ns: config
            .get("asset_convert_ns")
            .and_then(|v| v.as_str())
            .unwrap_or("asset_manager")
            .to_string(),
        upstream_asset_convert: config
            .get("upstream_asset_convert")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        upstream_asset_binding_id: config
            .get("upstream_asset_binding_id")
            .and_then(|v| v.as_i64()),
        poll_path: config
            .get("poll_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        asset_moderation: config
            .get("moderation")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        eid: eid.to_string(),
        mid,
        is_cascade: config
            .get("is_cascade")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        cascade_engine: config
            .get("cascade_engine")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| s == "tencent")
            .unwrap_or_else(|| "volc".to_string()),
        crop_480p: config
            .get("crop_480p")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        crop_timing: config
            .get("crop_timing")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_lowercase())
            .filter(|s| s == "post")
            .unwrap_or_else(|| "pre".to_string()),
        crop_coords_480p: parse_crop_coords(config, "crop_coords_480p"),
        crop_coords_720p: parse_crop_coords(config, "crop_coords_720p"),
        content_to_prompt: config
            .get("content_to_prompt")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        res_mul: parse_res_mul(config),
        res_enhance_enabled: parse_res_bool_map(config, "res_enhance_enabled"),
        res_enhance: parse_res_str_map(config, "res_enhance"),
        res_base: parse_res_str_map(config, "res_base"),
        res_scene: parse_res_str_map(config, "res_scene"),
        res_limit: parse_res_i64_map(config, "res_limit"),
        res_limit_fallback: config
            .get("res_limit_fallback")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| matches!(s.as_str(), "pro" | "standard" | "fast"))
            .unwrap_or_else(|| "pro".to_string()),
        body_passthrough: is_passthrough,
        fast_response: config
            .get("fast_response")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

// ── 域名智能推断（无转发规则时的自动识别）─────────────────────

/// 根据 channel base_url 域名自动推断正确的转发配置。
/// 当模型未绑定转发规则时使用，避免把火山/Google/Anthropic 请求
/// 错误地按 OpenAI 路径透传。
pub fn infer_forward_from_base_url(
    base_url: &str,
    category: &str,
    db_model: Option<&crate::models::Model>,
) -> ResolvedForward {
    #[cfg(not(feature = "plugin_volcengine_enhance"))]
    let _ = db_model;
    let url_lower = base_url.to_lowercase();

    // 阿里百炼 DashScope
    if url_lower.contains("dashscope") {
        return match category {
            "视频" => {
                let mut r = make_forward(
                    "dashscope",
                    "/api/v1/services/aigc/video-generation/video-synthesis",
                    "bearer",
                );
                r.poll_path = Some("/api/v1/tasks/${task_id}".to_string());
                r
            }
            "图片" => make_forward(
                "dashscope_image",
                "/api/v1/services/aigc/multimodal-generation/generation",
                "bearer",
            ),
            "向量" => make_forward("openai", "/compatible-mode/v1/embeddings", "bearer"),
            "排序" => make_forward("openai", "/compatible-api/v1/reranks", "bearer"),
            _ => default_openai_forward(match category {
                "聊天" => "/v1/chat/completions",
                "图片" => "/v1/images/generations",
                _ => "/v1/chat/completions",
            }),
        };
    }

    #[cfg(feature = "plugin_volcengine_enhance")]
    if let Some(r) = crate::api::plugins::infer_volc_media_enhance(&url_lower, db_model) {
        return r;
    }

    // 火山豆包语音合成（openspeech.bytedance.com，独立域名）
    // 必须优先于通用 volcengine 匹配
    if url_lower.contains("openspeech.bytedance.com") {
        return make_forward(
            "volcengine_tts",
            "/api/v3/tts/unidirectional/sse",
            "volcengine_tts",
        );
    }

    // 即梦AI（火山引擎 CV 视觉服务）
    // 注意：visual.volcengineapi.com 包含 "volcengine" 子串，必须优先于火山方舟通用匹配
    if url_lower.contains("visual.volcengineapi.com") {
        return match category {
            "图片" => make_forward("jimeng_image", "/", "jimeng"),
            "视频" => make_forward("jimeng_video", "/", "jimeng"),
            _ => default_openai_forward("/v1/chat/completions"),
        };
    }

    if url_lower.contains("volces.com") || url_lower.contains("volcengine") {
        match category {
            "图片" => make_forward("volcengine_image", "/api/v3/images/generations", "bearer"),
            "视频" => make_forward("volcengine", "/api/v3/contents/generations/tasks", "bearer"),
            "向量" => make_forward("openai", "/api/v3/embeddings/multimodal", "bearer"),
            _ => make_forward("volcengine_chat", "/api/v3/chat/completions", "bearer"),
        }
    } else if url_lower.contains("googleapis.com") || url_lower.contains("generativelanguage") {
        let tt = if category == "图片" {
            "gemini_image"
        } else {
            "gemini"
        };
        make_forward(tt, "/v1beta/models/${model}:generateContent", "query_key")
    } else if url_lower.contains("anthropic.com") {
        make_forward("anthropic", "/v1/messages", "x-api-key")
    } else if url_lower.contains("klingai.com") {
        match category {
            "视频" => make_forward("kling_video", "/text-to-video/${model}", "bearer"),
            "图片" => make_forward("kling", "/v1/images/generations", "bearer"),
            _ => default_openai_forward("/v1/chat/completions"),
        }
    } else if url_lower.contains("vidu.") {
        match category {
            "视频" => {
                let mut r = make_forward("vidu", "/ent/v2/img2video", "token");
                r.poll_path = Some("/ent/v2/tasks/${task_id}/creations".to_string());
                r
            }
            _ => default_openai_forward("/v1/chat/completions"),
        }
    } else if url_lower.contains("minimaxi.com") {
        match category {
            "视频" => {
                let mut r = make_forward("minimax_video", "/v2/video_generation", "bearer");
                r.poll_path = Some("/v2/query/video_generation/${task_id}".to_string());
                r
            }
            "图片" => make_forward("minimax_image", "/v1/image_generation", "bearer"),
            _ => default_openai_forward("/v1/chat/completions"),
        }
    } else if url_lower.contains("atptoken.ai") {
        if category == "视频" {
            let mut r = make_forward(
                "atp_video",
                "/omni/media/v1/contents/generations/tasks",
                "bearer",
            );
            r.poll_path = Some("/omni/media/v1/contents/generations/tasks/${task_id}".to_string());
            r
        } else {
            default_openai_forward(super::proxy::category_endpoint(Some(category)))
        }
    } else if url_lower.contains("mps.tencentcloudapi.com") {
        match category {
            "图片" => make_forward("tencent_mps_image", "/", "tencent_mps"),
            _ => default_openai_forward("/v1/chat/completions"),
        }
    } else if url_lower.contains("tencentcloudapi.com") {
        match category {
            "图片" => make_forward("tencent_vod_image", "/", "tencent_vod"),
            "视频" => make_forward("tencent_vod_video", "/", "tencent_vod"),
            _ => default_openai_forward("/v1/chat/completions"),
        }
    } else if url_lower.contains("queue.fal.run") {
        // 无规则时与内置 MiniMax 参考生视频路径一致
        if category == "视频" {
            let mut r = make_forward("fal_video", "/minimax/${model}/reference-to-video", "key");
            r.poll_path = Some("/minimax/${model}/requests/${task_id}/status".to_string());
            r
        } else {
            default_openai_forward(super::proxy::category_endpoint(Some(category)))
        }
    } else {
        default_openai_forward(super::proxy::category_endpoint(Some(category)))
    }
}
// ── 通用密钥脱敏 ──────────────────────────────────────────────

/// 对字符串中的 API 密钥进行脱敏处理
pub fn mask_key_in_string(text: &str, api_key: &str) -> String {
    if api_key.is_empty() || !text.contains(api_key) {
        return text.to_string();
    }
    let masked = {
        let cc = api_key.chars().count();
        if cc > 8 {
            let p: String = api_key.chars().take(4).collect();
            let s: String = api_key.chars().skip(cc - 4).collect();
            format!("{}******{}", p, s)
        } else {
            "******".to_string()
        }
    };
    text.replace(api_key, &masked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forward_rule_fast_response_default_and_explicit() {
        let empty_cfg = serde_json::json!({});
        let resolved_default = parse_forward_config(
            &empty_cfg,
            "/v1/video/generations",
            "/v1/video/generations",
            "eid-1",
            None,
        );
        assert!(!resolved_default.fast_response);

        let explicit_false = serde_json::json!({
            "target_type": "volcengine",
            "fast_response": false
        });
        let resolved_false = parse_forward_config(
            &explicit_false,
            "/v1/video/generations",
            "/v1/video/generations",
            "eid-2",
            None,
        );
        assert!(!resolved_false.fast_response);

        let explicit_true = serde_json::json!({
            "target_type": "volcengine",
            "asset_convert": true,
            "fast_response": true
        });
        let resolved_true = parse_forward_config(
            &explicit_true,
            "/v1/video/generations",
            "/v1/video/generations",
            "eid-3",
            None,
        );
        assert!(resolved_true.fast_response);
        assert!(resolved_true.asset_convert);

        // 级联规则未显式声明 fast_response 时，默认不自动开启快返（快返遵循字段 fast_response 为 true 才执行）
        let cascade_implicit = serde_json::json!({
            "target_type": "volcengine",
            "is_cascade": true
        });
        let resolved_cascade_implicit = parse_forward_config(
            &cascade_implicit,
            "/v1/video/generations",
            "/v1/video/generations",
            "eid-4",
            None,
        );
        assert!(!resolved_cascade_implicit.fast_response);
        assert!(resolved_cascade_implicit.is_cascade);

        // 级联规则显式指定 fast_response: true 时，执行快返
        let cascade_explicit_true = serde_json::json!({
            "target_type": "volcengine",
            "is_cascade": true,
            "fast_response": true
        });
        let resolved_cascade_true = parse_forward_config(
            &cascade_explicit_true,
            "/v1/video/generations",
            "/v1/video/generations",
            "eid-5",
            None,
        );
        assert!(resolved_cascade_true.fast_response);
        assert!(resolved_cascade_true.is_cascade);
    }

    #[test]
    fn test_forward_auth_type_google_header() {
        let cfg = serde_json::json!({
            "target_type": "gemini_image",
            "path_rewrite": {
                "old": "/v1/images/generations",
                "new": "/v1beta/models/${model}:generateContent"
            },
            "auth_type": "x-goog-api-key"
        });
        let resolved = parse_forward_config(
            &cfg,
            "/v1/images/generations",
            "/v1/images/generations",
            "eid-goog",
            None,
        );
        assert_eq!(resolved.target_type, "gemini_image");
        assert_eq!(resolved.auth_type, "x-goog-api-key");

        // 验证 URL 构建不携带 ?key=
        let upstream_url = build_upstream_url(
            "https://generativelanguage.googleapis.com",
            &resolved,
            "imagen-3.0-generate-002",
            "AIzaSyD-dummy-key",
        );
        assert_eq!(
            upstream_url,
            "https://generativelanguage.googleapis.com/v1beta/models/imagen-3.0-generate-002:generateContent"
        );
        assert!(!upstream_url.contains("key="));

        // 验证 Header 正确生成 x-goog-api-key
        let headers = build_auth_headers(&resolved, "  AIzaSyD-dummy-key  ", true);
        assert_eq!(
            headers,
            vec![("x-goog-api-key".to_string(), "AIzaSyD-dummy-key".to_string())]
        );
    }

    #[test]
    fn test_forward_path_matches_with_model_macro() {
        assert!(path_matches(
            "/v1/models/gemini-nano-banana-2.1:generateContent",
            "/v1/models/${model}:generateContent"
        ));
        assert!(path_matches(
            "/v1/models/gemini-2.0-flash:streamGenerateContent",
            "/v1/models/${model}:streamGenerateContent"
        ));
        assert!(path_matches(
            "/v1beta/models/gemini-2.5-flash:generateContent",
            "/v1beta/models/${model}:generateContent"
        ));

        // 验证多路由 path_rewrites 解析能够根据真实请求路径精准匹配
        let cfg = serde_json::json!({
            "target_type": "gemini_image",
            "path_rewrites": [
                { "old": "/v1/images/generations", "new": "/v1beta/models/${model}:generateContent" },
                { "old": "/v1/models/${model}:generateContent", "new": "/v1/models/${model}:generateContent" },
                { "old": "/v1beta/models/${model}:generateContent", "new": "/v1beta/models/${model}:generateContent" }
            ],
            "auth_type": "x-goog-api-key"
        });

        // 场景 A: 客户端调用新官方路由
        let req_a = "/v1/models/gemini-nano-banana-2.1:generateContent";
        let resolved_a = parse_forward_config(&cfg, "/v1/images/generations", req_a, "eid-goog-a", None);
        assert_eq!(resolved_a.upstream_path, "/v1/models/${model}:generateContent");
        let upstream_a = build_upstream_url(
            "https://generativelanguage.googleapis.com",
            &resolved_a,
            "gemini-nano-banana-2.1",
            "key123",
        );
        assert_eq!(
            upstream_a,
            "https://generativelanguage.googleapis.com/v1/models/gemini-nano-banana-2.1:generateContent"
        );

        // 场景 B: 客户端调用 OpenAI 兼容生图 (默认映射至最具兼容性的 /v1beta)
        let req_b = "/v1/images/generations";
        let resolved_b = parse_forward_config(&cfg, "/v1/images/generations", req_b, "eid-goog-b", None);
        assert_eq!(resolved_b.upstream_path, "/v1beta/models/${model}:generateContent");

        // 场景 C: 客户端调用老官方路由
        let req_c = "/v1beta/models/imagen-3.0-generate-002:generateContent";
        let resolved_c = parse_forward_config(&cfg, "/v1/images/generations", req_c, "eid-goog-c", None);
        assert_eq!(resolved_c.upstream_path, "/v1beta/models/${model}:generateContent");
    }

    #[test]
    fn test_is_google_native_path() {
        assert!(is_google_native_path("/v1/models/gemini-3-pro-image:generateContent"));
        assert!(is_google_native_path("/v1beta/models/gemini-2.5-flash:generateContent"));
        assert!(is_google_native_path("/v1/models/gemini-2.0-flash:streamGenerateContent"));
        assert!(is_google_native_path("v1/models/imagen-3.0:generateContent?key=abc"));
        assert!(is_google_native_path("/v1beta/models/imagen-3.0:generateContent|/upstream"));

        // OpenAI 与其它路由应为 false
        assert!(!is_google_native_path("/v1/chat/completions"));
        assert!(!is_google_native_path("/v1/images/generations"));
        assert!(!is_google_native_path("/v1/messages"));
    }

    #[test]
    fn test_volc_mediakit_merged_forward_rules_and_paths() {
        use crate::db::migrations::helpers::volc_mediakit_image::{
            SUBTITLE_ERASE_FORWARD_JSON, VIDEO_ENHANCE_FORWARD_JSON,
        };
        let sub_cfg: serde_json::Value =
            serde_json::from_str(SUBTITLE_ERASE_FORWARD_JSON).expect("valid subtitle json");
        let enh_cfg: serde_json::Value =
            serde_json::from_str(VIDEO_ENHANCE_FORWARD_JSON).expect("valid enhance json");

        // 1. 字幕擦除规则同时匹配 /v1/video/generations、/api/v1/tools/erase-video-subtitle 与 -pro
        let sub_rewrites = collect_rewrites(&sub_cfg);
        for path in [
            "/v1/video/generations",
            "/api/v1/tools/erase-video-subtitle",
            "/api/v1/tools/erase-video-subtitle-pro",
        ] {
            assert!(
                sub_rewrites
                    .iter()
                    .any(|(old, new)| path_matches(path, old) || path_matches(path, new)),
                "subtitle rule should match {path}"
            );
        }

        // 2. 合并后的视频画质增强规则同时匹配 OpenAI 与 3 个官方端点
        let enh_rewrites = collect_rewrites(&enh_cfg);
        for path in [
            "/v1/video/generations",
            "/api/v1/tools/enhance-video",
            "/api/v1/tools/enhance-video-fast",
            "/api/v1/tools/enhance-video-generative",
        ] {
            assert!(
                enh_rewrites
                    .iter()
                    .any(|(old, new)| path_matches(path, old) || path_matches(path, new)),
                "video enhance rule should match {path}"
            );
        }

        // 3. 验证 apply_volc_enhance_path 按 mid 与 model_id 精准覆写真实上游路径
        #[cfg(feature = "plugin_volcengine_enhance")]
        {
            let mut fwd_pro = parse_forward_config(
                &sub_cfg,
                "/v1/video/generations",
                "/v1/video/generations",
                "eid-sub",
                Some("vvs-ep".to_string()),
            );
            crate::api::plugins::apply_volc_enhance_path(
                &mut fwd_pro,
                "volc_video_subtitle_erase_pro",
            );
            assert_eq!(fwd_pro.upstream_path, "/api/v1/tools/erase-video-subtitle-pro");

            let mut fwd_fast = parse_forward_config(
                &enh_cfg,
                "/v1/video/generations",
                "/v1/video/generations",
                "eid-enh",
                None,
            );
            crate::api::plugins::apply_volc_enhance_path(&mut fwd_fast, "volc_video_enhance_fast");
            assert_eq!(fwd_fast.upstream_path, "/api/v1/tools/enhance-video-fast");
        }
    }

    #[test]
    fn test_res_limit_parsing() {
        let json_str = r#"{
            "is_cascade": true,
            "res_limit": {
                "720p": 960,
                "720p@1:1": 960,
                "720@3:4": 834,
                "1080p": 1440,
                "invalid": 0
            }
        }"#;
        let val: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let map = parse_res_i64_map(&val, "res_limit");
        assert_eq!(map.get("720p"), Some(&960));
        assert_eq!(map.get("720p@1:1"), Some(&960));
        assert_eq!(map.get("720p@3:4"), Some(&834));
        assert_eq!(map.get("1080p"), Some(&1440));
        assert_eq!(map.get("invalid"), None);
    }
}
