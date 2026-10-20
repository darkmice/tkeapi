/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 响应格式化引擎 (Response Formatter)
//! 将各厂商上游返回格式归一化为 OpenAI 标准规范格式。
//! 仅对 OpenAI 兼容路由（/v1/images/generations、/v1/images/edits、
//! /v1/video/generations、/v1/videos、/v1/videos/generations、/v1/tasks/）生效。
//! 设计原则：采用递归扫描模式，确保无论上游结构如何变化，都能准确抓取 ID、状态和媒体 URL。

use crate::relay::usage_extractor;
use regex::Regex;
use serde_json::{json, Value};

/// 判定请求路径是否属于标准的 OpenAI 兼容 API 路径（排除厂商官方原生接口，如 /ent/v2/、/v2/、/api/v3/、/text-to-video/ 等）
pub fn is_openai_compatible_path(raw_path: &str) -> bool {
    let clean = raw_path
        .split(['|', '?'])
        .next()
        .unwrap_or(raw_path)
        .trim_end_matches('/');

    // 阿里百炼兼容模式
    if clean.starts_with("/compatible-mode/v1/") || clean.starts_with("/compatible-api/v1/") {
        return true;
    }

    // 排除 Google Gemini 官方原生路由（/v1/models/、/v1beta/models/）
    if crate::relay::forward::is_google_native_path(clean) {
        return false;
    }

    // 必须以 /v1/ 开头
    let Some(sub) = clean.strip_prefix("/v1/") else {
        return false;
    };

    // 排除可灵官方原生挂载图片路径与腾讯云官方原生生图路径
    if sub.starts_with("images/omni-image")
        || sub.starts_with("images/multi-image2image")
        || sub.starts_with("wand/hunyuan-image")
    {
        return false;
    }

    true
}

/// 统一格式化入口：对 OpenAI 兼容路由自动转换响应格式。
/// - `is_poll`: true=异步轮询/结案（必须带正确终态 status）；false=同步或异步 POST
/// - `fallback_id`: 轮询缺 id 时兜底；同步成功时可写入 id；不用于伪造 POST pending
pub fn apply_format(
    raw_path: &str,
    category: &str,
    raw_response: &str,
    is_poll: bool,
    fallback_id: Option<&str>,
) -> String {
    // 仅对 OpenAI 兼容路由（/v1/...）启用格式转换
    if !is_openai_compatible_path(raw_path) {
        return raw_response.to_string();
    }

    format_openai(category, raw_response, is_poll, fallback_id)
}

/// OpenAI 格式化核心逻辑
pub fn format_openai(
    category: &str,
    raw: &str,
    is_poll: bool,
    fallback_id: Option<&str>,
) -> String {
    let v: Value = match serde_json::from_str(raw) {
        Ok(val) => val,
        Err(_) => return raw.to_string(),
    };

    // 1. 异步轮询/结案：一律 poll 骨架（正确 status；error+success → failed）
    if is_poll {
        return build_openai_poll(category, &v, fallback_id);
    }

    // 同步 / 异步 POST：业务错误 → 纯 error（无 status）
    if let Some(formatted) = format_as_openai_error(&v) {
        return formatted;
    }

    // 上游已是 OpenAI 同步成功体
    if v.get("created").is_some()
        && v.get("data").and_then(|d| d.as_array()).is_some()
        && v.get("code").is_none()
    {
        // 已有 OpenAI usage → 原样；仅有根级 usageMetadata → 注入转换结果
        if openai_usage_node(&v).is_some() {
            return raw.to_string();
        }
        if let Some(meta) = v.get("usageMetadata") {
            let usage = crate::relay::usage_extractor::gemini_usage_metadata_to_openai(meta);
            let mut out = v;
            out["usage"] = usage;
            return to_json(&out);
        }
        return raw.to_string();
    }

    let urls = find_urls(&v);
    let id = find_id(&v);

    let raw_status = extract_raw_status(&v);
    let std_status = parse_raw_status_to_standard(&raw_status);

    // 同步完成：具备媒体 URL，且无 task_id、任务状态明确完成、或图片类别无未完成状态
    if !urls.is_empty()
        && (id.is_empty()
            || std_status == "completed"
            || (raw_status.is_empty() && category == "图片"))
    {
        return build_openai_sync(category, &v, urls, fallback_id);
    }

    // 同步被拦截但已结算用量：无有效媒体，且非 STOP（如 IMAGE_SAFETY、content_filter）
    if urls.is_empty() {
        if let Some(reason) = extract_finish_reason(&v).filter(|f| !f.eq_ignore_ascii_case("stop")) {
            let mut resp = json!({
                "created": resolve_created(&v),
                "data": [],
                "finish_reason": reason,
            });
            if let Some(fid) = fallback_id.filter(|s| !s.is_empty()) {
                resp["id"] = json!(fid);
            }
            if let Some(u) = resolve_client_usage(&v) {
                resp["usage"] = u;
            }
            return to_json(&resp);
        }
    }

    // 6. 异步 POST 提交：具备有效 task_id
    if !id.is_empty() {
        return build_openai_submit(category, &v, &id);
    }

    raw.to_string()
}

// ── ID 提取（公共方法，供 task.rs / image.rs / proxy.rs 复用） ──

/// 从任意厂商响应 JSON 中提取任务 ID（兼容 task_id / id / data.task_id 等多种路径）
/// 优先级：真实任务号字段优先；根级 `request_id` 仅作 fal 等无 task_id 厂商的最后兜底
///（DashScope 等同体常同时有 `request_id` + `output.task_id`，不可先取 request_id）。
pub fn find_id(v: &Value) -> String {
    v.get("task_id")
        .or_else(|| v.get("TaskId"))
        .or_else(|| v.get("id"))
        .or_else(|| v.pointer("/task/id")) // MiniMax H3: { task: { id, status } }
        .or_else(|| v.pointer("/data/taskCode"))
        .or_else(|| v.pointer("/data/task_id"))
        .or_else(|| v.pointer("/data/id"))
        .or_else(|| v.pointer("/data/0/task_id"))
        .or_else(|| v.pointer("/data/0/id")) // 可灵 3.0 按任务ID查询: data[].id
        .or_else(|| v.pointer("/output/task_id")) // DashScope / 阿里百炼
        .or_else(|| v.pointer("/data/task/id"))
        .or_else(|| v.pointer("/Response/TaskId"))
        .or_else(|| usage_extractor::tencent_aigc_task(v).and_then(|t| t.get("TaskId")))
        .or_else(|| v.get("request_id").filter(|_| !is_upstream_error_response(v))) // fal.ai queue: { request_id, status }；非错误体兜底
        .and_then(|val| {
            // 兼容字符串和数字类型的 task_id（如火山方舟返回数字 ID）
            val.as_str()
                .map(|s| s.to_string())
                .or_else(|| val.as_i64().map(|n| n.to_string()))
                .or_else(|| Some(val.to_string()))
        })
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

/// 统一提取异步任务 ID。自动过滤同步响应（包含 choices / candidates / steps 字段）的通用会话 ID。
pub fn extract_async_task_id(v: &Value) -> String {
    if v.get("choices").is_some() || v.get("candidates").is_some() || v.get("steps").is_some() {
        return String::new();
    }
    find_id(v)
}

// ── 异步结案失败体 / 状态提取 ──

/// 异步结案失败最小任务体（默认 HTTP 200；体含数字 error.code 时轮询出口透出）
pub fn async_task_failed_body(task_id: &str, message: &str) -> String {
    let code = match crate::relay::proxy::infer_error_status_code_from_str(message) {
        s if (400..600).contains(&s) => s.to_string(),
        _ => "400".to_string(),
    };
    json!({
        "id": task_id,
        "status": "failed",
        "error": {
            "code": code,
            "message": crate::relay::proxy::sanitize_error_message(message),
            "type": "api_error"
        }
    })
    .to_string()
}

/// 生成统一对外异步任务号：`cgt-{YYYYMMDDHHmmss}-{5位随机}`
/// 格式规范：cgt- + 14位年月日时分秒 + 5位全局唯一ULID随机码
pub fn new_cgt_task_id() -> String {
    let ts = chrono::Local::now().format("%Y%m%d%H%M%S");
    let u = ulid::Ulid::new().to_string().to_lowercase();
    format!("cgt-{}-{}", ts, &u[21..26])
}

/// 强制对外任务号：原位同步全部上游任务号（百炼 output.task_id、可灵 data.task_id 等）。避免 `Value` 重排键。
pub fn force_json_task_id(s: &mut String, task_id: &str) {
    if task_id.is_empty() {
        return;
    }
    if let Ok(v) = serde_json::from_str::<Value>(s) {
        let old_id = find_id(&v);
        if !old_id.is_empty() && old_id != task_id {
            json_replace_str(s, &old_id, task_id);
        }
    }
}

#[derive(Clone, Copy)]
struct JsonMember {
    key_lo: usize,
    val_lo: usize,
    val_hi: usize,
}

fn json_skip_ws(s: &str, i: &mut usize) {
    let b = s.as_bytes();
    while *i < b.len() && b[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn json_skip_string(s: &str, i: &mut usize) -> Option<()> {
    let b = s.as_bytes();
    if b.get(*i) != Some(&b'"') {
        return None;
    }
    *i += 1;
    while *i < b.len() {
        match b[*i] {
            b'\\' => *i = (*i + 2).min(b.len()),
            b'"' => {
                *i += 1;
                return Some(());
            }
            _ => *i += 1,
        }
    }
    None
}

fn json_skip_balanced(s: &str, i: &mut usize, open: u8, close: u8) -> Option<()> {
    let b = s.as_bytes();
    if b.get(*i) != Some(&open) {
        return None;
    }
    let mut depth = 1usize;
    let mut in_str = false;
    let mut esc = false;
    *i += 1;
    while *i < b.len() && depth > 0 {
        let c = b[*i];
        *i += 1;
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
        }
    }
    (depth == 0).then_some(())
}

fn json_skip_value(s: &str, i: &mut usize) -> Option<(usize, usize)> {
    json_skip_ws(s, i);
    let start = *i;
    match s.as_bytes().get(*i)? {
        b'"' => json_skip_string(s, i)?,
        b'{' => json_skip_balanced(s, i, b'{', b'}')?,
        b'[' => json_skip_balanced(s, i, b'[', b']')?,
        _ => {
            let b = s.as_bytes();
            while *i < b.len() && !matches!(b[*i], b',' | b'}' | b']') {
                *i += 1;
            }
        }
    }
    Some((start, *i))
}

fn json_root_open(s: &str) -> Option<usize> {
    let mut i = 0;
    json_skip_ws(s, &mut i);
    (s.as_bytes().get(i) == Some(&b'{')).then_some(i)
}

fn json_for_each_member(
    s: &str,
    obj_open: usize,
    mut f: impl FnMut(&str, JsonMember) -> bool,
) -> Option<()> {
    let b = s.as_bytes();
    if b.get(obj_open) != Some(&b'{') {
        return None;
    }
    let mut i = obj_open + 1;
    json_skip_ws(s, &mut i);
    if b.get(i) == Some(&b'}') {
        return Some(());
    }
    loop {
        json_skip_ws(s, &mut i);
        let key_lo = i;
        json_skip_string(s, &mut i)?;
        let key = serde_json::from_str::<String>(&s[key_lo..i]).ok()?;
        json_skip_ws(s, &mut i);
        if b.get(i) != Some(&b':') {
            return None;
        }
        i += 1;
        let (val_lo, val_hi) = json_skip_value(s, &mut i)?;
        if !f(
            &key,
            JsonMember {
                key_lo,
                val_lo,
                val_hi,
            },
        ) {
            return Some(());
        }
        json_skip_ws(s, &mut i);
        match b.get(i) {
            Some(b',') => i += 1,
            Some(b'}') => return Some(()),
            _ => return None,
        }
    }
}

fn json_find_root(s: &str, key: &str) -> Option<JsonMember> {
    let mut found = None;
    json_for_each_member(s, json_root_open(s)?, |k, m| {
        if k == key {
            found = Some(m);
            false
        } else {
            true
        }
    })?;
    found
}

fn json_walk_objects(s: &str, obj_open: usize, f: &mut impl FnMut(&str, JsonMember)) {
    let _ = json_for_each_member(s, obj_open, |k, m| {
        f(k, m);
        let mut i = m.val_lo;
        json_skip_ws(s, &mut i);
        if s.as_bytes().get(i) == Some(&b'{') {
            json_walk_objects(s, i, f);
        }
        true
    });
}

pub(crate) fn json_root_raw_value<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    json_find_root(s, key).map(|m| &s[m.val_lo..m.val_hi])
}

/// 根字段已存在则原位替换值，不重排其它键。无此键 / 非对象 → false
pub(crate) fn json_root_set(s: &mut String, key: &str, raw_val: &str) -> bool {
    let Some(m) = json_find_root(s, key) else {
        return false;
    };
    s.replace_range(m.val_lo..m.val_hi, raw_val);
    true
}

pub(crate) fn json_root_remove(s: &mut String, key: &str) {
    let Some(m) = json_find_root(s, key) else {
        return;
    };
    let b = s.as_bytes();
    let mut lo = m.key_lo;
    let mut hi = m.val_hi;
    let mut k = hi;
    json_skip_ws(s, &mut k);
    if b.get(k) == Some(&b',') {
        hi = k + 1;
    } else {
        let mut p = lo;
        while p > 0 && b[p - 1].is_ascii_whitespace() {
            p -= 1;
        }
        if p > 0 && b[p - 1] == b',' {
            lo = p - 1;
        }
    }
    s.replace_range(lo..hi, "");
}

pub(crate) fn json_replace_str(s: &mut String, old: &str, new: &str) {
    if old.is_empty() || old == new {
        return;
    }
    if let (Ok(old_lit), Ok(new_lit)) = (serde_json::to_string(old), serde_json::to_string(new)) {
        if s.contains(&old_lit) {
            // 99.99% 主路径：带双引号的标准字符串 ID，单次极速替换，零多余内存分配
            *s = s.replace(&old_lit, &new_lit);
        } else {
            // 容错路径：原生未带引号的数字 ID（如 : 12345），原位替换为带引号的目标字符串
            for p in [format!(": {old}"), format!(":{old}")] {
                if s.contains(&p) {
                    *s = s.replace(&p, &p.replace(old, &new_lit));
                    break;
                }
            }
        }
    }
}

/// 已有同名字段（含嵌套对象）原位改值，不追加
pub(crate) fn json_replace_fields(s: &mut String, kv: &[(&str, &str)]) {
    let Some(open) = json_root_open(s) else {
        return;
    };
    let mut hits: Vec<(usize, usize, &str)> = Vec::new();
    json_walk_objects(s, open, &mut |k, m| {
        if let Some((_, raw)) = kv.iter().find(|(n, _)| *n == k) {
            hits.push((m.val_lo, m.val_hi, raw));
        }
    });
    for (lo, hi, raw) in hits.into_iter().rev() {
        s.replace_range(lo..hi, raw);
    }
}

/// 结案失败客户端体：统一封装为合法失败响应 JSON（OpenAI 兼容端点或官方原生路由）
pub fn format_async_task_failed(
    raw_path: &str,
    category: &str,
    task_id: &str,
    message: &str,
) -> String {
    let body = async_task_failed_body(task_id, message);
    if crate::relay::forward::is_official_route(raw_path) {
        return crate::relay::forward::wrap_official_client(raw_path, &body, "", "");
    }
    if !is_openai_compatible_path(raw_path) {
        return body;
    }
    let mut s = apply_format(raw_path, category, &body, true, Some(task_id));
    force_json_task_id(&mut s, task_id);
    s
}

/// 统一从响应体中提取 finish_reason（兼容 Google 原生 candidates、OpenAI choices 与顶层扩展格式）
pub fn extract_finish_reason(v: &Value) -> Option<&str> {
    v.pointer("/candidates/0/finishReason")
        .or_else(|| v.pointer("/candidates/0/finish_reason"))
        .or_else(|| v.pointer("/choices/0/finish_reason"))
        .or_else(|| v.pointer("/choices/0/finishReason"))
        .or_else(|| v.pointer("/finish_reason"))
        .or_else(|| v.pointer("/finishReason"))
        .and_then(|f| f.as_str())
}

/// 从任意厂商响应 JSON 中提取原始状态字，并自动应用特定平台的校验（如腾讯云 ErrCode、即梦 code）
pub fn extract_raw_status(v: &Value) -> String {
    // 腾讯云 DescribeTaskDetail：Status="FINISH" 时校验任务节点 ErrCode，非 0 视为 FAILED
    if let Some(resp) = v.get("Response") {
        if let Some(status) = resp.get("Status").and_then(|s| s.as_str()) {
            if status.eq_ignore_ascii_case("FINISH") {
                if usage_extractor::tencent_aigc_task(resp)
                    .and_then(|t| t.get("ErrCode"))
                    .and_then(|c| c.as_i64())
                    .is_some_and(|c| c != 0)
                {
                    return "FAILED".to_string();
                }
                if resp
                    .pointer("/ProcedureTask/ErrCode")
                    .and_then(|c| c.as_i64())
                    .is_some_and(|c| c != 0)
                {
                    return "FAILED".to_string();
                }
                // MPS DescribeImageTaskDetail：FINISH 但 ErrMsg 非空即失败（可无 ResultSet）
                if resp
                    .get("ErrMsg")
                    .and_then(|s| s.as_str())
                    .is_some_and(|s| !s.is_empty())
                {
                    return "FAILED".to_string();
                }
            }
            return status.to_string();
        }
    }

    // 即梦AI：data.status="done" 时需检查外层 code。10000 为成功，否则失败。
    if let Some(status) = v.pointer("/data/status").and_then(|s| s.as_str()) {
        if status == "done" {
            let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            return if code == 10000 {
                "done".to_string()
            } else {
                "FAILED".to_string()
            };
        }
    }

    v.get("status")
        .or_else(|| v.get("task_status"))
        .or_else(|| v.get("state"))
        .or_else(|| v.pointer("/task/status")) // MiniMax H3 v2: { task: { status, content } }
        .or_else(|| v.pointer("/data/status"))
        .or_else(|| v.pointer("/data/task_status"))
        .or_else(|| v.pointer("/data/0/status")) // 可灵 3.0: data[].status
        .or_else(|| v.pointer("/data/task/status"))
        .or_else(|| v.pointer("/output/task_status"))
        .or_else(|| v.pointer("/Response/Status"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string()
}

/// 将任意厂商的状态字符串统一归一化为标准的异步任务状态
pub fn parse_raw_status_to_standard(raw: &str) -> &'static str {
    match raw.to_lowercase().trim() {
        "completed" | "succeeded" | "succeed" | "success" | "finish" | "done" => "completed",
        "failed" | "canceled" | "cancelled" | "error" | "timeout" | "unknown" | "fail"
        | "abort" | "not_found" | "expired" => "failed",
        "processing" | "running" | "active" | "generating" | "waiting" | "in_queue"
        | "in_progress" => "in_progress",
        "submitted" | "pending" | "queueing" | "queued" | "created" => "pending",
        _ => "unknown",
    }
}

/// JSON 体（或已格式化串）是否已是标准 `status=failed`
pub fn is_failed_task_status(raw: &str) -> bool {
    serde_json::from_str::<Value>(raw)
        .ok()
        .is_some_and(|v| parse_raw_status_to_standard(&extract_raw_status(&v)) == "failed")
}

/// URL 提取：优先从标准字段路径直接提取，递归扫描兜底（供 tos_persist 复用）
pub fn find_urls(v: &Value) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();

    // 1. OpenAI: data[].url；可灵 3.0: data[].outputs[].url
    if let Some(arr) = v.get("data").and_then(|d| d.as_array()) {
        for item in arr {
            if let Some(u) = item.get("url").and_then(|u| u.as_str()) {
                push_unique(&mut urls, u);
            }
            if let Some(outputs) = item.get("outputs").and_then(|a| a.as_array()) {
                for out in outputs {
                    if let Some(u) = out.get("url").and_then(|u| u.as_str()) {
                        push_unique(&mut urls, u);
                    }
                }
            }
        }
    }

    // 2. 可灵图片协议: data.task_result.images[].url
    for path in &[
        "/data/task_result/images",
        "/data/task/task_result/images",
    ] {
        if let Some(arr) = v.pointer(path).and_then(|a| a.as_array()) {
            for item in arr {
                if let Some(u) = item.get("url").and_then(|u| u.as_str()) {
                    push_unique(&mut urls, u);
                }
            }
        }
    }

    // 即梦AI / MiniMax: data.image_urls[] (字符串数组，非对象数组)
    if let Some(arr) = v.pointer("/data/image_urls").and_then(|a| a.as_array()) {
        for item in arr {
            if let Some(u) = item.as_str() {
                push_unique(&mut urls, u);
            }
        }
    }

    // 即梦 binary_data_base64 / MiniMax image_base64（与 image_urls 互斥）
    if urls.is_empty() {
        for path in ["/data/binary_data_base64", "/data/image_base64"] {
            if let Some(arr) = v.pointer(path).and_then(|a| a.as_array()) {
                for item in arr {
                    if let Some(b64) = item.as_str().filter(|s| !s.is_empty()) {
                        if b64.starts_with("data:") {
                            push_unique(&mut urls, b64);
                        } else {
                            push_unique(&mut urls, &format!("data:image/png;base64,{}", b64));
                        }
                    }
                }
                if !urls.is_empty() {
                    break;
                }
            }
        }
    }

    // 即梦AI: data.video_url
    if let Some(u) = v.pointer("/data/video_url").and_then(|u| u.as_str()) {
        push_unique(&mut urls, u);
    }

    // 3. 火山方舟: content.video_url；MiniMax H3 v2: task.content.url；fal.ai: video.url
    for path in &["/content/video_url", "/task/content/url", "/video/url"] {
        if let Some(u) = v.pointer(path).and_then(|u| u.as_str()) {
            push_unique(&mut urls, u);
        }
    }
    if let Some(u) = v.get("video_url").and_then(|u| u.as_str()) {
        push_unique(&mut urls, u);
    }
    // 火山引擎 AI MediaKit：视频 /result/video_url，图像 /result/image_url
    if let Some(u) = v.pointer("/result/video_url").and_then(|u| u.as_str()) {
        push_unique(&mut urls, u);
    }
    if let Some(u) = v.pointer("/result/image_url").and_then(|u| u.as_str()) {
        push_unique(&mut urls, u);
    }
    if let Some(u) = v
        .pointer("/Response/ProcedureTask/MediaProcessResultSet/0/TranscodeTask/Output/Url")
        .and_then(|u| u.as_str())
    {
        push_unique(&mut urls, u);
    }
    if let Some(u) = v
        .pointer(
            "/Response/ProcedureTask/MediaProcessResultSet/0/CoverBySnapshotTask/Output/CoverUrl",
        )
        .and_then(|u| u.as_str())
    {
        push_unique(&mut urls, u);
    }

    // 3b. Vidu 官方视频生成: creations[].url
    if let Some(arr) = v.get("creations").and_then(|a| a.as_array()) {
        for item in arr {
            if let Some(u) = item.get("url").and_then(|u| u.as_str()) {
                push_unique(&mut urls, u);
            }
        }
    }

    // 4. 阿里 DashScope: output.results[].url / output.video_url
    if let Some(arr) = v.pointer("/output/results").and_then(|a| a.as_array()) {
        for item in arr {
            let u = item
                .get("url")
                .or_else(|| item.get("video_url"))
                .and_then(|u| u.as_str());
            if let Some(u) = u {
                push_unique(&mut urls, u);
            }
        }
    }
    if let Some(u) = v.pointer("/output/video_url").and_then(|u| u.as_str()) {
        push_unique(&mut urls, u);
    }

    // 4b. 阿里 DashScope chat 格式: output.choices[].message.content[].image
    if let Some(choices) = v.pointer("/output/choices").and_then(|c| c.as_array()) {
        for choice in choices {
            if let Some(parts) = choice
                .pointer("/message/content")
                .and_then(|c| c.as_array())
            {
                for part in parts {
                    if let Some(u) = part.get("image").and_then(|u| u.as_str()) {
                        push_unique(&mut urls, u);
                    }
                }
            }
        }
    }

    // 5. APIMart: data.result.images/videos[].url
    for path in &["/data/result/images", "/data/result/videos"] {
        if let Some(arr) = v.pointer(path).and_then(|a| a.as_array()) {
            for item in arr {
                if let Some(u) = item.get("url").and_then(|u| u.as_str()) {
                    push_unique(&mut urls, u);
                } else if let Some(arr_url) = item.get("url").and_then(|u| u.as_array()) {
                    for u in arr_url {
                        if let Some(s) = u.as_str() {
                            push_unique(&mut urls, s);
                        }
                    }
                }
            }
        }
    }

    // 5b. Bytefor: data.files[].fileUrl 或 data.files[].file_url
    if let Some(arr) = v.pointer("/data/files").and_then(|a| a.as_array()) {
        for item in arr {
            if let Some(u) = item
                .get("fileUrl")
                .or_else(|| item.get("file_url"))
                .and_then(|u| u.as_str())
            {
                push_unique(&mut urls, u);
            }
        }
    }

    // 6. Gemini: candidates[].content.parts[].inlineData → data URI
    if let Some(candidates) = v.get("candidates").and_then(|c| c.as_array()) {
        for cand in candidates {
            if let Some(parts) = cand.pointer("/content/parts").and_then(|p| p.as_array()) {
                for part in parts {
                    if let Some(inline) = part.get("inlineData").or_else(|| part.get("inline_data")) {
                        push_inline_media(&mut urls, inline);
                    }
                    // 书虫格式/Gemini文本格式：text 中的 Markdown 图片 或嵌入的 HTTP URL
                    else if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        // 1. 匹配 Markdown 中的 base64 图片格式 ![...](data:image/...;base64,...)
                        let base64_re =
                            Regex::new(r"data:([^;]+);base64,([a-zA-Z0-9+/=]+)").unwrap();
                        for cap in base64_re.captures_iter(text) {
                            push_unique(&mut urls, &format!("data:{};base64,{}", &cap[1], &cap[2]));
                        }
                        // 2. 匹配文本或 Markdown 中的 HTTP/HTTPS 链接（自动剔除右侧的括号或方括号）
                        let url_re = Regex::new(r"https?://[^\s)\]]+").unwrap();
                        for mat in url_re.find_iter(text) {
                            let url_str = mat.as_str();
                            let lower = url_str.to_lowercase();
                            let path_part = lower.split('?').next().unwrap_or(&lower);
                            let is_media = path_part.ends_with(".png")
                                || path_part.ends_with(".jpg")
                                || path_part.ends_with(".jpeg")
                                || path_part.ends_with(".webp")
                                || path_part.ends_with(".gif")
                                || path_part.ends_with(".mp4")
                                || path_part.ends_with(".mov")
                                || path_part.ends_with(".webm")
                                || lower.contains("/image")
                                || lower.contains("/video")
                                || lower.contains("x-oss-process")
                                || lower.contains("tos-cn-")
                                || lower.contains("volccdn.com")
                                || lower.contains("volces.com")
                                || lower.contains("klingai.com")
                                || lower.contains("aliyuncs.com");
                            if is_media {
                                push_unique(&mut urls, url_str);
                            }
                        }
                    }
                }
            }
        }
    }

    // 6a. Google Gemini Interactions: steps[].content[] (type == "image")
    if let Some(steps) = v.get("steps").and_then(|s| s.as_array()) {
        for item in steps
            .iter()
            .filter_map(|s| s.get("content").and_then(|c| c.as_array()))
            .flatten()
        {
            if item
                .get("type")
                .and_then(|t| t.as_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("image"))
            {
                push_inline_media(&mut urls, item);
            }
        }
    }

    // 6b. 腾讯云 VOD FileInfos；MPS SignedUrl（跳过 last_frame_url 尾帧图）
    if let Some(resp) = v.get("Response") {
        if let Some(task) = usage_extractor::tencent_aigc_task(resp) {
            if let Some(arr) = task.pointer("/Output/FileInfos").and_then(|a| a.as_array()) {
                for item in arr {
                    if item.get("UsageType").and_then(|u| u.as_str()) == Some("last_frame_url") {
                        continue;
                    }
                    if let Some(u) = item.get("FileUrl").and_then(|u| u.as_str()) {
                        push_unique(&mut urls, u);
                    }
                }
            }
        }
        if let Some(arr) = resp
            .get("ImageProcessTaskResultSet")
            .and_then(|a| a.as_array())
        {
            for item in arr {
                if let Some(u) = item.pointer("/Output/SignedUrl").and_then(|u| u.as_str()) {
                    push_unique(&mut urls, u);
                }
            }
        }
    }

    // 6c. 腾讯云 TokenHub 混元生图 3.5: choices[].delta.image.url
    if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            if let Some(u) = choice.pointer("/delta/image/url").and_then(|u| u.as_str()) {
                push_unique(&mut urls, u);
            }
        }
    }

    // 7. 递归扫描兜底（捕获非标准位置的媒体 URL）
    if urls.is_empty() {
        scan_value_for_urls(v, &mut urls);
    }

    urls
}

fn push_unique(urls: &mut Vec<String>, url: &str) {
    if !url.is_empty() && !urls.iter().any(|u| u == url) {
        urls.push(url.to_string());
    }
}

fn push_inline_media(urls: &mut Vec<String>, node: &Value) {
    if let Some(u) = node.get("uri").and_then(|u| u.as_str()).filter(|s| !s.is_empty()) {
        push_unique(urls, u);
    } else if let Some(data) = node.get("data").and_then(|d| d.as_str()).filter(|s| !s.is_empty()) {
        if data.starts_with("http://") || data.starts_with("https://") {
            push_unique(urls, data);
        } else {
            let mime = node
                .get("mimeType")
                .or_else(|| node.get("mime_type"))
                .and_then(|m| m.as_str())
                .unwrap_or("image/png");
            push_unique(urls, &format!("data:{};base64,{}", mime, data));
        }
    }
}

fn scan_value_for_urls(v: &Value, urls: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            if (s.starts_with("http://") || s.starts_with("https://"))
                && !urls.iter().any(|u| u == s)
            {
                let lower = s.to_lowercase();
                // 截取 ? 前的路径部分，解决 OSS 签名 URL 带查询参数导致扩展名匹配失败
                let path_part = lower.split('?').next().unwrap_or(&lower);
                let is_media = path_part.ends_with(".png")
                    || path_part.ends_with(".jpg")
                    || path_part.ends_with(".jpeg")
                    || path_part.ends_with(".webp")
                    || path_part.ends_with(".gif")
                    || path_part.ends_with(".mp4")
                    || path_part.ends_with(".mov")
                    || path_part.ends_with(".webm")
                    || lower.contains("/image")
                    || lower.contains("/video")
                    || lower.contains("x-oss-process")
                    || lower.contains("tos-cn-")
                    || lower.contains("volccdn.com")
                    || lower.contains("volces.com")
                    || lower.contains("klingai.com")
                    || lower.contains("aliyuncs.com");
                if is_media {
                    urls.push(s.clone());
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                scan_value_for_urls(item, urls);
            }
        }
        Value::Object(map) => {
            let is_task_obj = map.contains_key("task_id") || map.contains_key("TaskId");
            for (k, val) in map {
                // 排除请求输入相关字段，避免将用户提交的原始图片误识别为响应媒体
                if matches!(
                    k.as_str(),
                    "request" | "input" | "task_input" | "original_input" | "task_data" | "InputInfo"
                ) || (is_task_obj && matches!(k.as_str(), "images" | "image" | "image_urls" | "sounds"))
                {
                    continue;
                }
                scan_value_for_urls(val, urls);
            }
        }
        _ => {}
    }
}

fn find_ts(v: &Value, keys: &[&str]) -> i64 {
    for key in keys {
        let val = v
            .get(*key)
            .or_else(|| v.pointer(&format!("/task/{}", key))) // MiniMax H3
            .or_else(|| v.pointer(&format!("/data/{}", key)))
            .or_else(|| v.pointer(&format!("/output/{}", key)));
        if let Some(t) = val {
            if let Some(n) = t.as_i64() {
                return n;
            }
            if let Some(s) = t.as_str() {
                if let Ok(n) = s.parse::<i64>() {
                    return n;
                }
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                    return dt.timestamp();
                }
            }
        }
    }
    0
}

/// 提交/轮询骨架共用的 created 时间戳
fn resolve_created(v: &Value) -> i64 {
    let ts = find_ts(v, &["created_at", "created", "submit_time"]);
    if ts > 0 {
        ts
    } else {
        chrono::Utc::now().timestamp()
    }
}

fn to_json(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

// ══════════════════════════════════════════════════════════════════════
// OpenAI 规范格式
// ══════════════════════════════════════════════════════════════════════

fn openai_object(category: &str) -> &'static str {
    if category.contains("视频") || category == "封装" || category == "video" {
        "video.generation"
    } else {
        "image.generation"
    }
}

fn openai_status(v: &Value, urls: &[String]) -> String {
    let raw = extract_raw_status(v);
    let std = if raw.is_empty() {
        if is_upstream_error_response(v) {
            return "failed".to_string();
        }
        if urls.is_empty() {
            "in_progress"
        } else {
            "completed"
        }
    } else {
        parse_raw_status_to_standard(&raw)
    };
    // error + 成功类 status → failed（计费安全）
    if std == "completed" && is_upstream_error_response(v) {
        return "failed".to_string();
    }
    match std {
        "completed" | "failed" | "in_progress" => std.to_string(),
        // OpenAI 轮询无 pending：上游 queueing/pending 统一为 in_progress
        "pending" => "in_progress".to_string(),
        _ => "in_progress".to_string(),
    }
}

fn openai_usage_node(v: &Value) -> Option<&Value> {
    v.get("usage")
        .or_else(|| v.pointer("/task/usage"))
        .or_else(|| v.pointer("/data/usage"))
}

/// OpenAI usage 原样；Gemini `usageMetadata` 或 腾讯云 `Output.Usage` → OpenAI 字段
fn resolve_client_usage(v: &Value) -> Option<Value> {
    if let Some(u) = openai_usage_node(v) {
        return Some(u.clone());
    }
    if let Some(meta) = v.get("usageMetadata") {
        return Some(usage_extractor::gemini_usage_metadata_to_openai(meta));
    }
    if let Some(usage) = usage_extractor::tencent_output_usage(v) {
        return Some(usage_extractor::tencent_usage_to_openai(usage));
    }
    None
}

// ── URL/Base64 → OpenAI data item 统一转换（build_openai_sync 和 build_openai_poll 共用）──
fn build_data_item(u: &str) -> Value {
    if u.starts_with("data:") {
        // data:image/png;base64,xxx → b64_json；TOS 替换后 payload 为 http → url
        let payload = crate::relay::forward::b64_data(u);
        if payload == u.trim() {
            json!({"url": u})
        } else if payload.starts_with("http://") || payload.starts_with("https://") {
            json!({"url": payload})
        } else {
            json!({"b64_json": payload})
        }
    } else {
        json!({"url": u})
    }
}

// ── 同步完成 ──
fn build_openai_sync(
    _category: &str,
    v: &Value,
    urls: Vec<String>,
    fallback_id: Option<&str>,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let created = v.get("created").and_then(|c| c.as_i64()).unwrap_or(now);

    let extra = scan_extra_metadata(v);
    let items: Vec<Value> = urls
        .iter()
        .map(|u| {
            let mut item = build_data_item(u);
            for (k, ev) in &extra {
                item[k] = ev.clone();
            }
            item
        })
        .collect();

    let mut resp = json!({"created": created, "data": items});
    if let Some(fid) = fallback_id {
        if !fid.is_empty() {
            resp["id"] = json!(fid);
        }
    }
    // OpenAI usage 原样 / Gemini usageMetadata 转换；无则不挂字段（避免无用量厂商多出零值）
    if let Some(usage) = resolve_client_usage(v) {
        resp["usage"] = usage;
    }
    to_json(&resp)
}

// ── 异步 POST 提交 ack（带 status:pending；终态 status 只在轮询响应）──
fn build_openai_submit(category: &str, v: &Value, id: &str) -> String {
    to_json(&json!({
        "id": id,
        "object": openai_object(category),
        "status": "pending",
        "created": resolve_created(v)
    }))
}

// ── 异步轮询 ──
/// fallback_id: 客户端轮询钥匙（path / logs.task_id）；优先于上游体内 id（级联 cgt 等）
fn build_openai_poll(category: &str, v: &Value, fallback_id: Option<&str>) -> String {
    let urls = find_urls(v);
    let status = openai_status(v, &urls);
    let id = fallback_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| find_id(v));

    let mut resp = json!({
        "id": id,
        "object": openai_object(category),
        "status": status,
        "created": resolve_created(v)
    });

    if status == "completed" {
        if !urls.is_empty() {
            let extra = scan_extra_metadata(v);
            let items: Vec<Value> = urls
                .iter()
                .map(|u| {
                    let mut item = build_data_item(u);
                    for (k, ev) in &extra {
                        item[k] = ev.clone();
                    }
                    item
                })
                .collect();
            resp["data"] = json!(items);
        }
        resp["usage"] = resolve_client_usage(v).unwrap_or_else(
            || json!({"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}),
        );
    }

    if status == "failed" {
        resp["error"] = openai_error_object(v);
    }

    to_json(&resp)
}

/// 统一从 Value 中提取最核心的错误文本信息（集合了所有的已知厂商指针路径）
pub fn extract_error_message_from_value(v: &Value) -> Option<String> {
    // fal / FastAPI：{"detail":[{"msg":"..."}, ...]} 或 {"detail":"..."}
    if let Some(msg) = fastapi_detail_message(v) {
        return Some(msg);
    }

    v.pointer("/data/error/message")
        .or_else(|| v.pointer("/data/error"))
        .or_else(|| v.pointer("/error/message"))
        .or_else(|| v.pointer("/error"))
        .or_else(|| v.pointer("/task/error/message")) // MiniMax H3: { task: { error: { message } } }
        .or_else(|| v.pointer("/base_resp/status_msg")) // MiniMax 图片/部分接口
        .or_else(|| v.pointer("/data/task/task_status_msg"))
        .or_else(|| v.pointer("/data/0/message")) // 可灵 3.0 失败说明
        .or_else(|| v.pointer("/data/errorMsg"))
        .or_else(|| v.get("message"))
        .or_else(|| v.pointer("/output/message"))
        .or_else(|| v.pointer("/Response/Error/Message"))
        .or_else(|| v.pointer("/ResponseMetadata/Error/Message"))
        // 方舟/智算等扁平错误：{"ErrorCode":"...","ErrorMessage":"..."}
        .or_else(|| v.get("ErrorMessage"))
        // 腾讯云任务级：优先 Response.{TaskType}.Message，无 TaskType 时回退已知 Aigc* 节点
        .or_else(|| tencent_task_message(v))
        .or_else(|| {
            v.pointer("/Response/ProcedureTask/Message")
                .filter(|m| m.as_str().is_some_and(|s| !s.is_empty()))
        })
        .or_else(|| {
            v.pointer("/Response/Message")
                .filter(|m| m.as_str().is_some_and(|s| !s.is_empty()))
        })
        // 即梦 CV 网关错误体：[{"algo_msg":"..."}]
        .or_else(|| v.pointer("/0/algo_msg"))
        .and_then(|val| {
            if val.is_object() {
                val.get("message")
                    .or_else(|| val.get("msg"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| Some(val.to_string()))
            } else {
                val.as_str()
                    .map(|s| s.to_string())
                    .or_else(|| Some(val.to_string()))
            }
        })
}

/// fal / FastAPI 校验错误：`detail` 为对象数组（取 msg）或纯字符串
fn fastapi_detail_message(v: &Value) -> Option<String> {
    let detail = v.get("detail")?;
    if let Some(arr) = detail.as_array() {
        let msgs: Vec<&str> = arr
            .iter()
            .filter_map(|item| item.get("msg").and_then(|m| m.as_str()))
            .filter(|s| !s.is_empty())
            .collect();
        (!msgs.is_empty()).then(|| msgs.join("; "))
    } else {
        detail
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }
}

/// 腾讯云 VOD/混元任务节点 Message：复用 [`usage_extractor::tencent_aigc_task`]
fn tencent_task_message(v: &Value) -> Option<&Value> {
    let msg = usage_extractor::tencent_aigc_task(v.get("Response")?)?.get("Message")?;
    match msg.as_str() {
        Some(s) if !s.is_empty() => Some(msg),
        _ => None,
    }
}

/// 从任意厂商响应 JSON 中提取错误消息（兼容可灵、APIMart、DashScope 等）
/// 供各 relay 模块复用，避免硬编码通用错误文本
pub fn extract_error_message(v: &Value) -> String {
    extract_error_message_from_value(v).unwrap_or_else(|| "generation failed".to_string())
}

/// 从响应 JSON 中提取结构化错误分类码，供 HTTP 状态码推断和错误格式化使用
/// 优先精确路径（error.code / data.error.code / 腾讯云 / 即梦火山网关 / ErrorCode），兜底根节点 code
/// 接受字符串（如 "PolicyViolation" / "PERMISSION_ERROR"）和数字（如 429），统一转为字符串返回
/// 注：仅在已确认为错误响应时调用，根节点 code 不会出现成功值（10000/200/0）
pub fn extract_error_code_from_value(v: &Value) -> Option<String> {
    v.pointer("/error/code")
        .or_else(|| v.pointer("/data/error/code"))
        .or_else(|| v.pointer("/task/error/code")) // MiniMax H3
        .or_else(|| v.pointer("/base_resp/status_code")) // MiniMax 图片/部分接口
        .or_else(|| v.pointer("/Response/Error/Code"))
        .or_else(|| v.pointer("/ResponseMetadata/Error/Code"))
        .or_else(|| v.pointer("/output/code")) // 阿里百炼/DashScope 异步任务输出 code
        // 方舟/智算等扁平错误：{"ErrorCode":"PERMISSION_ERROR","ErrorMessage":"..."}
        .or_else(|| v.get("ErrorCode"))
        .or_else(|| v.get("code")) // APIMart/即梦根节点数字或字符串 code
        .and_then(|c| {
            c.as_str()
                .map(|s| s.to_string())
                .or_else(|| c.as_i64().map(|n| n.to_string()))
        })
}

/// 尾帧图：`content.last_frame_url`、顶层 `last_frame_url`、OpenAI `data[0].last_frame_url`、腾讯云 VOD UsageType
pub fn find_last_frame_url(v: &Value) -> Option<&str> {
    if let Some(url) = v
        .pointer("/content/last_frame_url")
        .or_else(|| v.get("last_frame_url"))
        .or_else(|| v.pointer("/data/0/last_frame_url"))
        .and_then(|u| u.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(url);
    }

    // 腾讯云 AIGC 任务 Output.FileInfos: UsageType 为 last_frame_url，地址为 FileUrl
    let arr = usage_extractor::tencent_aigc_task(v)
        .and_then(|t| t.pointer("/Output/FileInfos"))
        .and_then(|a| a.as_array())?;
    for item in arr {
        if item.get("UsageType").and_then(|u| u.as_str()) == Some("last_frame_url") {
            return item
                .get("FileUrl")
                .and_then(|u| u.as_str())
                .filter(|s| !s.is_empty());
        }
    }
    None
}

/// 从上游响应中扫描厂商特有的重要附加字段
fn scan_extra_metadata(v: &Value) -> serde_json::Map<String, Value> {
    let mut meta = serde_json::Map::new();
    if let Some(url) = find_last_frame_url(v) {
        meta.insert("last_frame_url".to_string(), json!(url));
    }
    meta
}

/// 辅助函数：判断响应体是否是上游业务报错状态（聚合所有已知厂商的报错标识，如腾讯云、即梦、火山MediaKit、Bytefor等）
pub fn is_upstream_error_response(v: &Value) -> bool {
    // 1. 腾讯云 API 级别错误
    if v.pointer("/Response/Error").is_some() {
        return true;
    }
    if v.pointer("/Response/ErrMsg")
        .and_then(|s| s.as_str())
        .is_some_and(|s| !s.is_empty())
    {
        return true;
    }
    // 2. 即梦/火山网关错误
    if v.pointer("/ResponseMetadata/Error").is_some() {
        return true;
    }
    // 3. 方舟/智算等扁平错误：{"ErrorCode":"PERMISSION_ERROR","ErrorMessage":"..."}
    //    仅认非空 PascalCase ErrorCode，避免与业务成功体中的小写 code/message 混淆
    if v.get("ErrorCode")
        .and_then(|c| c.as_str())
        .is_some_and(|c| !c.is_empty())
    {
        return true;
    }
    // 4. 含有 error 节点（且 error 节点内含有 message/msg 或 code，或者 error 本身是字符串）
    if let Some(err) = v.get("error") {
        if err.is_string()
            || err.get("message").is_some()
            || err.get("msg").is_some()
            || err.get("code").is_some()
        {
            return true;
        }
    }
    if let Some(err) = v.pointer("/data/error") {
        if err.is_string()
            || err.get("message").is_some()
            || err.get("msg").is_some()
            || err.get("code").is_some()
        {
            return true;
        }
    }
    // 5. 常见的 code 错误指示（排除 0 和 200, 10000 等正常成功值）
    if let Some(code_val) = v.get("code") {
        if let Some(code) = code_val.as_i64() {
            if code != 0 && code != 200 && code != 10000 {
                return true;
            }
        } else if let Some(code_str) = code_val.as_str() {
            if code_str != "0"
                && code_str != "200"
                && code_str != "10000"
                && code_str != "success"
                && code_str != "ok"
            {
                return true;
            }
        }
    }
    // 6. 火山 MediaKit success 字段指示
    if let Some(success) = v.get("success").and_then(|s| s.as_bool()) {
        if !success {
            return true;
        }
    }
    // 7. MiniMax: base_resp.status_code != 0（图片等同步接口 HTTP 仍可能 200）
    if let Some(code) = v.pointer("/base_resp/status_code").and_then(|c| c.as_i64()) {
        if code != 0 {
            return true;
        }
    }
    // 8. fal / FastAPI：{"detail":"..."} 或 {"detail":[{"msg":"..."}]}
    if fastapi_detail_message(v).is_some() {
        return true;
    }
    // 9. 根级 status_code 且非 200/0
    if let Some(sc) = v.get("status_code").and_then(|s| s.as_i64()) {
        if sc != 0 && sc != 200 {
            return true;
        }
    }
    false
}

/// 标准 OpenAI `error` 对象（poll failed / 纯 error 体共用）
fn openai_error_object(v: &Value) -> Value {
    // 优先 OpenAI 路径 message，避免 extract 抢先命中 /data/error/message
    let msg = v
        .pointer("/error/message")
        .and_then(|m| m.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| extract_error_message(v));

    let raw_type = v
        .pointer("/error/type")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let code = extract_error_code_from_value(v)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if raw_type.is_empty() {
                "upstream_error".to_string()
            } else {
                raw_type.to_string()
            }
        });

    let err_type = if code.to_lowercase().contains("permission") {
        "permission_error"
    } else if raw_type.is_empty() || raw_type.chars().all(|c| c.is_ascii_digit()) {
        "upstream_error"
    } else {
        raw_type
    };

    json!({
        "message": crate::relay::proxy::sanitize_error_message(&msg),
        "type": err_type,
        "code": code
    })
}

/// 将上游错误 Value 转为标准 OpenAI error JSON：`{"error":{"message","type","code"}}`。
/// - 厂商扁平错误（ErrorCode 等）→ 转换
/// - 已有 `error.message` 但夹带 `success` / 数字 type 等 → 规范化
/// - 非错误体 → `None`（调用方透传原文）
pub fn format_as_openai_error(v: &Value) -> Option<String> {
    if !is_upstream_error_response(v) {
        return None;
    }
    Some(to_json(&json!({ "error": openai_error_object(v) })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_force_json_task_id_all_vendors() {
        let sys_id = "tsk_20260928_sys";

        // 1. 阿里百炼 / DashScope 原生：仅替换 output.task_id，根级保持纯净无多余 id
        let mut ali =
            r#"{"request_id":"req-1","output":{"task_id":"ali-up-123","task_status":"SUCCEEDED"}}"#
                .to_string();
        force_json_task_id(&mut ali, sys_id);
        let ali_v: Value = serde_json::from_str(&ali).unwrap();
        assert_eq!(ali_v["output"]["task_id"], sys_id);
        assert!(ali_v.get("id").is_none());

        // 2. 可灵旧版原生：仅替换 data.task_id，根级无多余 id
        let mut kling_v1 =
            r#"{"code":0,"data":{"task_id":"kling-up-456","task_status":"succeed"}}"#.to_string();
        force_json_task_id(&mut kling_v1, sys_id);
        let k1_v: Value = serde_json::from_str(&kling_v1).unwrap();
        assert_eq!(k1_v["data"]["task_id"], sys_id);
        assert!(k1_v.get("id").is_none());

        // 3. 可灵新版 v3 轮询原生：仅替换 data[0].id，根级无多余 id
        let mut kling_v3 =
            r#"{"code":0,"data":[{"id":"kling-up-789","status":"succeeded"}]}"#.to_string();
        force_json_task_id(&mut kling_v3, sys_id);
        let k3_v: Value = serde_json::from_str(&kling_v3).unwrap();
        assert_eq!(k3_v["data"][0]["id"], sys_id);
        assert!(k3_v.get("id").is_none());

        // 4. 原生带根级 id 的协议（OpenAI 兼容 / 火山方舟）：原位替换为 sys_id
        let mut openai = r#"{"id":"up-openai-001","status":"completed"}"#.to_string();
        force_json_task_id(&mut openai, sys_id);
        let op_v: Value = serde_json::from_str(&openai).unwrap();
        assert_eq!(op_v["id"], sys_id);

        // 5. 验证 OpenAI 兼容出口流水线：原生百炼体经 apply_format 后，必然具备标准根级 id
        let openai_out = apply_format("/v1/video/generations", "视频", &ali, false, Some(sys_id));
        let openai_v: Value = serde_json::from_str(&openai_out).unwrap();
        assert_eq!(openai_v["id"], sys_id);
        assert_eq!(openai_v["status"], "pending");

        // 6. 原生数字型任务 ID：精准替换为带双引号统一字符串任务 ID
        let mut volc_num = r#"{"code":0,"data":{"task_id":182938491823912,"task_status":"succeed"}}"#.to_string();
        force_json_task_id(&mut volc_num, sys_id);
        let num_v: Value = serde_json::from_str(&volc_num).unwrap();
        assert_eq!(num_v["data"]["task_id"], sys_id);
        assert!(num_v.get("id").is_none());

        // 7. 错误响应体：find_id 不误识别 request_id
        let err_resp: Value = serde_json::from_str(r#"{"code":"InvalidApiKey","message":"error","request_id":"req-999"}"#).unwrap();
        assert_eq!(find_id(&err_resp), "");

        // 8. fal.ai 正常排队响应：find_id 正常识别 request_id 兜底
        let fal_resp: Value = serde_json::from_str(r#"{"request_id":"fal-123","status":"IN_QUEUE"}"#).unwrap();
        assert_eq!(find_id(&fal_resp), "fal-123");

        // 9. 统一对外 cgt 任务 ID 规范测试：前缀为 cgt- 且长度为 30 (cgt-14位时间戳-5位随机)
        let cgt_id = new_cgt_task_id();
        assert!(cgt_id.starts_with("cgt-"));
        assert_eq!(cgt_id.len(), 4 + 14 + 1 + 5);
    }

    #[test]
    fn test_vidu_official_video_response_to_openai() {
        // 1. Vidu 官方视频创建异步响应：包含 task_id、state: created、输入图片 images 回显
        let vidu_raw = r#"{
            "task_id": "1006087621681618944",
            "type": "img2video",
            "state": "created",
            "model": "viduq4",
            "style": "general",
            "prompt": "让小狗跑起来",
            "images": [
                "https://tokensbyte-1251015623.cos.ap-guangzhou.myqcloud.com/uploads/p1009873912/91222949/images/1788513904_ff8a0aaf.jpeg"
            ],
            "duration": 3,
            "seed": 532460788,
            "aspect_ratio": "",
            "resolution": "540p",
            "movement_amplitude": "auto",
            "created_at": "2026-10-09T10:24:56.366782159Z",
            "credits": 27
        }"#;

        // POST 提交响应：必须转为标准 OpenAI 异步提交响应（带 id 与 status: pending，严禁被误识别为同步完成并返回输入图）
        let out = apply_format("/v1/video/generations", "视频", vidu_raw, false, None);
        let out_v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(out_v["id"], "1006087621681618944");
        assert_eq!(out_v["object"], "video.generation");
        assert_eq!(out_v["status"], "pending");
        assert_eq!(out_v["created"], 1791541496);
        assert!(out_v.get("data").is_none());

        // 2. 轮询处理中（state: processing）：保持 OpenAI in_progress
        let vidu_poll_proc = r#"{
            "task_id": "1006087621681618944",
            "state": "processing",
            "created_at": "2026-10-09T10:24:56.366782159Z"
        }"#;
        let poll_out = apply_format("/v1/video/generations/1006087621681618944", "视频", vidu_poll_proc, true, Some("1006087621681618944"));
        let poll_v: Value = serde_json::from_str(&poll_out).unwrap();
        assert_eq!(poll_v["id"], "1006087621681618944");
        assert_eq!(poll_v["status"], "in_progress");

        // 3. 轮询成功（state: success）：提取 creations[0].url，且不被输入图 images 污染
        let vidu_poll_succ = r#"{
            "task_id": "1006087621681618944",
            "state": "success",
            "creations": [
                {
                    "url": "https://api.vidu.cn/video/final_output.mp4",
                    "cover_url": "https://api.vidu.cn/cover.jpg"
                }
            ],
            "images": [
                "https://tokensbyte-1251015623.cos.ap-guangzhou.myqcloud.com/uploads/p1009873912/91222949/images/1788513904_ff8a0aaf.jpeg"
            ],
            "created_at": "2026-10-09T10:24:56.366782159Z"
        }"#;
        let succ_out = apply_format("/v1/video/generations/1006087621681618944", "视频", vidu_poll_succ, true, Some("1006087621681618944"));
        let succ_v: Value = serde_json::from_str(&succ_out).unwrap();
        assert_eq!(succ_v["id"], "1006087621681618944");
        assert_eq!(succ_v["status"], "completed");
        let data_arr = succ_v["data"].as_array().unwrap();
        assert_eq!(data_arr.len(), 1);
        assert_eq!(data_arr[0]["url"], "https://api.vidu.cn/video/final_output.mp4");
    }

    #[test]
    fn test_is_openai_compatible_path_distinction() {
        // OpenAI 路由（返回 true）
        assert!(is_openai_compatible_path("/v1/video/generations"));
        assert!(is_openai_compatible_path("/v1/videos"));
        assert!(is_openai_compatible_path("/v1/videos/generations"));
        assert!(is_openai_compatible_path("/v1/videos/task_abc123"));
        assert!(is_openai_compatible_path("/v1/tasks/task_abc123"));
        assert!(is_openai_compatible_path("/v1/chat/completions"));
        assert!(is_openai_compatible_path("/v1/images/generations"));
        assert!(is_openai_compatible_path("/compatible-mode/v1/embeddings"));
        assert!(is_openai_compatible_path("/v1/video/generations?stream=true"));
        assert!(is_openai_compatible_path("/v1/video/generations|/ent/v2/img2video"));

        // 官方原生路由（必须返回 false）
        assert!(!is_openai_compatible_path("/ent/v2/img2video"));
        assert!(!is_openai_compatible_path("/ent/v2/text2video"));
        assert!(!is_openai_compatible_path("/ent/v2/tasks/task_123/creations"));
        assert!(!is_openai_compatible_path("/v2/video_generation"));
        assert!(!is_openai_compatible_path("/v2/query/video_generation/task_123"));
        assert!(!is_openai_compatible_path("/api/v3/contents/generations/tasks"));
        assert!(!is_openai_compatible_path("/api/v3/chat/completions"));
        assert!(!is_openai_compatible_path("/text-to-video/kling-v1"));
        assert!(!is_openai_compatible_path("/image-to-video/kling-v1"));
        assert!(!is_openai_compatible_path("/omni-video/kling-v1"));
        assert!(!is_openai_compatible_path("/v1/images/omni-image"));
        assert!(!is_openai_compatible_path("/v1/wand/hunyuan-image/v3-generation"));
        assert!(!is_openai_compatible_path("/v1/wand/hunyuan-image/v35-generation"));
        assert!(!is_openai_compatible_path("/ent/v2/img2video|/ent/v2/img2video"));
        // 谷歌官方原生路由（/v1/models/ 与 /v1beta/models/ 必须返回 false）
        assert!(!is_openai_compatible_path("/v1/models/gemini-3-pro-image:generateContent"));
        assert!(!is_openai_compatible_path("/v1beta/models/gemini-2.5-flash:generateContent"));
        assert!(!is_openai_compatible_path("/v1/models/gemini-2.0-flash:streamGenerateContent"));
    }

    #[test]
    fn test_format_async_task_failed_route_awareness() {
        let msg = "the parameter duration specified in the request must be greater than or equal to 2";

        // 1. 官方原生路由：包装为官方标准失败响应，绝不生成 OpenAI 假体，且保证为合法 JSON
        let off_res = format_async_task_failed(
            "/api/v3/contents/generations/tasks/cgt-123",
            "视频",
            "cgt-123",
            msg,
        );
        let off_v: Value = serde_json::from_str(&off_res).expect("官方路由必须返回合法 JSON 响应");
        assert_eq!(off_v["id"], "cgt-123");
        assert_eq!(off_v["status"], "failed");
        assert_eq!(off_v["error"]["message"], msg);
        assert!(!off_res.contains("\"object\":\"video.generation\""));

        // 2. OpenAI 兼容路由：规范生成包含 id、status=failed、error 的 OpenAI 失败体
        let oai_res = format_async_task_failed(
            "/v1/video/generations/cgt-123",
            "视频",
            "cgt-123",
            msg,
        );
        let v: Value = serde_json::from_str(&oai_res).unwrap();
        assert_eq!(v["id"], "cgt-123");
        assert_eq!(v["status"], "failed");
        assert_eq!(v["error"]["message"], msg);
        assert_eq!(v["error"]["type"], "api_error");
    }

    #[test]
    fn test_is_upstream_error_response_coverage() {
        let v_err: Value = serde_json::from_str(r#"{"error":"prompt content violates policy"}"#).unwrap();
        assert!(is_upstream_error_response(&v_err));

        let v_status_code_err: Value = serde_json::from_str(r#"{"status_code":400,"message":"bad request"}"#).unwrap();
        assert!(is_upstream_error_response(&v_status_code_err));

        let v_ok: Value = serde_json::from_str(r#"{"id":"task-123","status":"in_progress"}"#).unwrap();
        assert!(!is_upstream_error_response(&v_ok));
    }

    #[test]
    fn test_extract_finish_reason_and_interception() {
        // 1. Google 官方格式
        let google_safety: Value = serde_json::from_str(r#"{"candidates":[{"finishReason":"IMAGE_SAFETY"}]}"#).unwrap();
        assert_eq!(extract_finish_reason(&google_safety), Some("IMAGE_SAFETY"));

        // 2. Google 成功格式
        let google_stop: Value = serde_json::from_str(r#"{"candidates":[{"finishReason":"STOP"}]}"#).unwrap();
        assert_eq!(extract_finish_reason(&google_stop), Some("STOP"));

        // 3. OpenAI Chat 格式
        let oai_safety: Value = serde_json::from_str(r#"{"choices":[{"finish_reason":"content_filter"}]}"#).unwrap();
        assert_eq!(extract_finish_reason(&oai_safety), Some("content_filter"));

        let oai_stop: Value = serde_json::from_str(r#"{"choices":[{"finish_reason":"stop"}]}"#).unwrap();
        assert_eq!(extract_finish_reason(&oai_stop), Some("stop"));

        // 4. 顶层兼容格式
        let top_level: Value = serde_json::from_str(r#"{"finish_reason":"SAFETY"}"#).unwrap();
        assert_eq!(extract_finish_reason(&top_level), Some("SAFETY"));

        // 5. 无 finish_reason 的空包
        let empty: Value = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(extract_finish_reason(&empty), None);
    }

    #[test]
    fn test_format_openai_intercepted_gemini_response() {
        let raw = r#"{
            "candidates": [
                {
                    "content": { "role": "model" },
                    "finishReason": "IMAGE_SAFETY"
                }
            ],
            "usageMetadata": {
                "promptTokenCount": 3979,
                "totalTokenCount": 3979,
                "trafficType": "ON_DEMAND",
                "promptTokensDetails": [
                    { "modality": "TEXT", "tokenCount": 619 },
                    { "modality": "IMAGE", "tokenCount": 3360 }
                ]
            }
        }"#;

        let res = apply_format("/v1/images/generations", "图片", raw, false, None);
        let val: Value = serde_json::from_str(&res).unwrap();
        assert_eq!(val["data"].as_array().unwrap().len(), 0);
        assert_eq!(val["finish_reason"], "IMAGE_SAFETY");
        assert!(val.get("error").is_none());
        assert_eq!(val["usage"]["prompt_tokens"], 3979);
        assert_eq!(val["usage"]["total_tokens"], 3979);
    }

    #[test]
    fn test_format_openai_already_formed_intercepted_passthrough() {
        let raw = r#"{
            "created": 1791797375,
            "data": [],
            "finish_reason": "IMAGE_SAFETY",
            "id": "tsk_01m4qq3xdd19e4agqe1cb81sth",
            "usage": {
                "completion_tokens": 253,
                "completion_tokens_details": {
                    "reasoning_tokens": 253
                },
                "image_tokens": 3360,
                "prompt_tokens": 3979,
                "prompt_tokens_details": {
                    "image_tokens": 3360
                },
                "total_tokens": 4232
            }
        }"#;

        let res = apply_format("/v1/images/generations", "图片", raw, false, None);
        let val: Value = serde_json::from_str(&res).unwrap();
        assert_eq!(val["created"], 1791797375);
        assert_eq!(val["data"].as_array().unwrap().len(), 0);
        assert_eq!(val["finish_reason"], "IMAGE_SAFETY");
        assert!(val.get("error").is_none());
        assert_eq!(val["id"], "tsk_01m4qq3xdd19e4agqe1cb81sth");
        assert_eq!(val["usage"]["prompt_tokens"], 3979);
        assert_eq!(val["usage"]["completion_tokens"], 253);
        assert_eq!(val["usage"]["total_tokens"], 4232);

        // 验证用量解析器也能无损提取
        let u = crate::relay::usage_extractor::parse_usage(raw);
        assert_eq!(u.prompt, 3979);
        assert_eq!(u.completion, 253);
        assert_eq!(u.reasoning_tokens, 253);
        assert_eq!(u.image_tokens, 3360);
        assert_eq!(u.total, 4232);
    }

    #[test]
    fn test_format_tokenhub_hunyuan_35_image_response() {
        let raw = r#"{
            "id": "1374200352-WandImage-085edfe2367d4a688f68e813af3665a5",
            "object": "image.chat.completion.chunk",
            "created": 1789720599,
            "model": "HY-Image-3.5-preview-4090-Tob-v1.2",
            "choices": [
                {
                    "index": 0,
                    "delta": {
                        "type": "image",
                        "image": {
                            "url": "https://aigc-output-image-file-1326893053.cos.ap-guangzhou.myqcloud.com/xxx/main.png",
                            "width": 4096,
                            "height": 4096
                        }
                    },
                    "finish_reason": null
                }
            ],
            "usage": { "total_tokens": 20000 },
            "request_id": "a1c00d07-041b-4daa-bae0-b7eabf2bc33a"
        }"#;

        let res = apply_format("/v1/images/generations", "图片", raw, false, None);
        let val: Value = serde_json::from_str(&res).unwrap();
        assert_eq!(val["created"], 1789720599);
        let data = val["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(
            data[0]["url"],
            "https://aigc-output-image-file-1326893053.cos.ap-guangzhou.myqcloud.com/xxx/main.png"
        );
    }
}

