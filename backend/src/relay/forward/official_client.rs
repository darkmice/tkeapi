/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::relay::response_formatter::{
    extract_error_code_from_value, extract_error_message_from_value, find_id, find_last_frame_url,
    find_urls,
};
use serde_json::{json, Value};

/// 客户端走厂商官方路由（MiniMax、可灵 AI、火山方舟）时组装/规范化官方信封格式。
/// 适用于腾讯云、fal、EdgeOne 等对接各类官方路由的厂商通道。
/// 仅当命中对应官方路由（新旧创建及轮询）时转换，其它路径原样返回。
pub fn wrap_official_client(
    path: &str,
    json_str: &str,
    model: &str,
    request_content: &str,
) -> String {
    let p = path
        .split(['|', '?'])
        .next()
        .unwrap_or(path)
        .trim_end_matches('/');

    // 1. MiniMax 官方路由规范
    if p == "/v2/video_generation" {
        return wrap_minimax_v2_create(json_str);
    }
    if p.starts_with("/v2/query/video_generation/") || p == "/v2/query/video_generation" {
        return wrap_minimax_v2_query(json_str, model, request_content);
    }

    // 2. 火山引擎(方舟)官方视频路由规范
    let clean = p.strip_prefix("/v1").unwrap_or(p);
    if clean == "/api/v3/contents/generations/tasks" {
        return wrap_volcengine_create(json_str);
    }
    if clean.starts_with("/api/v3/contents/generations/tasks/") {
        return wrap_volcengine_query(json_str, model, request_content);
    }

    // 3. 可灵 3.0 / Omni 官方原生路由（注册于顶层根路径）
    // 创建：/text-to-video/{model}, /image-to-video/{model}, /omni-video/{model} (兼容带 /v1)
    if clean.starts_with("/text-to-video/")
        || clean.starts_with("/image-to-video/")
        || clean.starts_with("/omni-video/")
    {
        return wrap_kling_create(json_str);
    }
    // 轮询：GET /tasks?task_ids=... (兼容带 /v1 前缀)
    if clean == "/tasks" {
        return wrap_kling_query(json_str, request_content);
    }

    // 4. Vidu 官方视频路由规范
    // 创建：/ent/v2/text2video, /ent/v2/img2video, /ent/v2/start-end2video, /ent/v2/reference2video
    if clean == "/ent/v2/text2video"
        || clean == "/ent/v2/img2video"
        || clean == "/ent/v2/start-end2video"
        || clean == "/ent/v2/reference2video"
    {
        return wrap_vidu_create(clean, json_str, model, request_content);
    }
    // 轮询：GET /ent/v2/tasks/{task_id}/creations
    if clean.starts_with("/ent/v2/tasks/") && clean.ends_with("/creations") {
        return wrap_vidu_query(clean, json_str, model);
    }

    json_str.to_string()
}

/// 将官方创建路径映射为其对应的轮询/查询路径；非官方路径则原样返回
pub fn to_official_query_path(path: &str, task_id: &str) -> String {
    let p = path
        .split(['|', '?'])
        .next()
        .unwrap_or(path)
        .trim_end_matches('/');
    let clean = p.strip_prefix("/v1").unwrap_or(p);
    if clean == "/v2/video_generation" {
        return "/v2/query/video_generation".to_string();
    }
    if clean == "/api/v3/contents/generations/tasks" {
        let tid = if task_id.is_empty() { "poll" } else { task_id };
        return format!("{}/{}", clean, tid);
    }
    if clean.starts_with("/text-to-video/")
        || clean.starts_with("/image-to-video/")
        || clean.starts_with("/omni-video/")
    {
        return "/tasks".to_string();
    }
    if clean.starts_with("/ent/v2/") && !clean.ends_with("/creations") {
        let tid = if task_id.is_empty() { "poll" } else { task_id };
        return format!("/ent/v2/tasks/{}/creations", tid);
    }
    path.to_string()
}

/// 判断给定的请求路径是否属于厂商官方原生路由（MiniMax、可灵、火山方舟等）
#[inline]
pub fn is_official_route(path: &str) -> bool {
    let clean = path
        .split(['|', '?'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    clean == "/v2/video_generation"
        || clean.starts_with("/v2/query/video_generation")
        || clean == "/api/v3/contents/generations/tasks"
        || clean.starts_with("/api/v3/contents/generations/tasks/")
        || clean == "/tasks"
        || clean.starts_with("/text-to-video/")
        || clean.starts_with("/image-to-video/")
        || clean.starts_with("/omni-video/")
        || clean.starts_with("/ent/v2/")
}

fn wrap_kling_create(raw: &str) -> String {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };
    if v.get("code")
        .and_then(Value::as_i64)
        .is_some_and(|c| c != 0)
    {
        return raw.to_string();
    }
    // 已具备纯净新版官方结构则原样返回
    if v.pointer("/data/id").is_some() && v.pointer("/data/task_id").is_none() {
        return raw.to_string();
    }

    if crate::relay::response_formatter::is_upstream_error_response(&v) {
        let msg = extract_error_message_from_value(&v).unwrap_or_else(|| "request failed".to_string());
        let req_id = v
            .pointer("/Response/RequestId")
            .or_else(|| v.get("request_id"))
            .or_else(|| v.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        return json!({
            "code": 400,
            "message": msg,
            "request_id": req_id
        })
        .to_string();
    }

    let id = find_id(&v);
    if id.is_empty() {
        return raw.to_string();
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let status = match crate::relay::response_formatter::parse_raw_status_to_standard(
        &crate::relay::response_formatter::extract_raw_status(&v),
    ) {
        "completed" => "succeeded",
        "in_progress" => "processing",
        "failed" => "failed",
        _ => "submitted",
    };
    let create_time = v
        .get("created")
        .and_then(Value::as_i64)
        .map(|t| if t < 10_000_000_000 { t * 1000 } else { t })
        .unwrap_or(now_ms);
    let update_time = v
        .get("updated")
        .and_then(Value::as_i64)
        .map(|t| if t < 10_000_000_000 { t * 1000 } else { t })
        .unwrap_or(create_time);

    let mut data = json!({
        "id": id,
        "status": status,
        "create_time": create_time,
        "update_time": update_time,
    });
    if let Some(ext) = v.get("external_id").and_then(Value::as_str) {
        data["external_id"] = json!(ext);
    }

    json!({
        "code": 0,
        "message": v.get("message").and_then(Value::as_str).unwrap_or("SUCCEED"),
        "request_id": v.get("request_id").or_else(|| v.get("id")).and_then(Value::as_str).unwrap_or(&id),
        "data": data
    })
    .to_string()
}

fn wrap_kling_query(raw: &str, request_content: &str) -> String {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };
    if v.get("code")
        .and_then(Value::as_i64)
        .is_some_and(|c| c != 0)
    {
        return raw.to_string();
    }
    // 已具备新版纯净结构则原样返回
    if let Some(arr) = v.get("data").and_then(Value::as_array) {
        if !arr.is_empty() && arr[0].get("id").is_some() && arr[0].get("task_id").is_none() {
            return raw.to_string();
        }
    }

    let id = find_id(&v);
    if id.is_empty() {
        return raw.to_string();
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let status = match crate::relay::response_formatter::parse_raw_status_to_standard(
        &crate::relay::response_formatter::extract_raw_status(&v),
    ) {
        "completed" => "succeeded",
        "in_progress" => "processing",
        "failed" => "failed",
        _ => "submitted",
    };
    let message = extract_error_message_from_value(&v).unwrap_or_default();
    let create_time = v
        .get("created")
        .and_then(Value::as_i64)
        .map(|t| if t < 10_000_000_000 { t * 1000 } else { t })
        .unwrap_or(now_ms);
    let update_time = v
        .get("updated")
        .and_then(Value::as_i64)
        .map(|t| if t < 10_000_000_000 { t * 1000 } else { t })
        .unwrap_or(create_time);

    let outputs = if status == "succeeded" {
        if let Some(arr) = v
            .pointer("/data/0/outputs")
            .and_then(Value::as_array)
        {
            arr.clone()
        } else {
            let duration = v
                .pointer("/usage/output_seconds")
                .and_then(|d| {
                    if let Some(n) = d.as_i64().filter(|&n| n > 0) {
                        Some(n.to_string())
                    } else if let Some(f) = d.as_f64().filter(|&f| f > 0.0) {
                        Some(if f.fract() == 0.0 {
                            (f as i64).to_string()
                        } else {
                            f.to_string()
                        })
                    } else {
                        d.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
                    }
                })
                .unwrap_or_else(|| {
                    let req: Value = serde_json::from_str(request_content).unwrap_or(Value::Null);
                    req.pointer("/settings/duration")
                        .or_else(|| req.get("duration"))
                        .and_then(|d| {
                            if let Some(s) = d.as_str() {
                                Some(s.to_string())
                            } else {
                                d.as_i64().map(|n| n.to_string())
                            }
                        })
                        .unwrap_or_else(|| "5".to_string())
                });

            find_urls(&v)
                .into_iter()
                .enumerate()
                .map(|(idx, u)| {
                    json!({
                        "type": "video",
                        "id": format!("{}_{}", id, idx),
                        "url": u,
                        "duration": duration
                    })
                })
                .collect()
        }
    } else {
        Vec::new()
    };

    let mut item = json!({
        "id": id,
        "status": status,
        "message": message,
        "create_time": create_time,
        "update_time": update_time,
    });
    if status == "succeeded" {
        item["outputs"] = json!(outputs);
    }
    if let Some(ext) = v.get("external_id").and_then(Value::as_str) {
        item["external_id"] = json!(ext);
    }

    json!({
        "code": 0,
        "message": "SUCCEED",
        "request_id": v.get("request_id").or_else(|| v.get("id")).and_then(Value::as_str).unwrap_or(&id),
        "data": [item]
    })
    .to_string()
}

fn wrap_minimax_v2_create(raw: &str) -> String {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };
    if v.get("error").is_some() || v.pointer("/Response/Error").is_some() {
        return raw.to_string();
    }
    if v.get("task_id").is_some() && v.get("object").is_none() && v.get("task").is_none() {
        return raw.to_string();
    }
    let id = find_id(&v);
    if id.is_empty() {
        return raw.to_string();
    }
    json!({ "task_id": id }).to_string()
}

fn wrap_minimax_v2_query(raw: &str, model: &str, request_content: &str) -> String {
    let mut v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };
    if let Some(task) = v.get_mut("task").and_then(Value::as_object_mut) {
        if model.is_empty() || task.get("model").and_then(Value::as_str) == Some(model) {
            return raw.to_string();
        }
        task.insert("model".to_string(), json!(model));
        return v.to_string();
    }

    let status = match v.get("status").and_then(Value::as_str).unwrap_or("") {
        "completed" | "succeeded" => "succeeded",
        "failed" => "failed",
        "cancelled" => "cancelled",
        "pending" | "queued" => "queued",
        "in_progress" | "processing" | "running" => "running",
        _ if v.get("error").is_some() => "failed",
        _ => "running",
    };

    let req = serde_json::from_str::<Value>(request_content).unwrap_or(Value::Null);
    let mut task = json!({
        "id": find_id(&v),
        "model": model,
        "status": status,
        "task_type": "generation",
        "modality": "video",
    });
    if let Some(r) = req
        .get("resolution")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        task["resolution"] = json!(r);
    }
    if let Some(d) = req.get("duration").and_then(Value::as_i64) {
        task["duration"] = json!(d);
    }
    if let Some(r) = req
        .get("ratio")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        task["ratio"] = json!(r);
    }

    if status == "succeeded" {
        let url = v
            .pointer("/data/0/url")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| find_urls(&v).into_iter().next());
        if let Some(u) = url {
            task["content"] = json!({ "url": u });
        }
        if let Some(usage) = minimax_task_usage(v.get("usage"), req.get("duration")) {
            task["usage"] = usage;
        }
    } else if status == "failed" {
        let code = extract_error_code_from_value(&v).unwrap_or_else(|| "1026".to_string());
        let msg =
            extract_error_message_from_value(&v).unwrap_or_else(|| "generation failed".to_string());
        task["error"] = json!({ "code": code, "message": msg });
        task["usage"] = json!({});
    }

    json!({ "task": task }).to_string()
}

fn minimax_task_usage(usage: Option<&Value>, req_duration: Option<&Value>) -> Option<Value> {
    let mut out = usage
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(d) = req_duration.and_then(Value::as_i64) {
        out.entry("output_seconds".to_string()).or_insert(json!(d));
    }

    out.entry("prompt_tokens".to_string()).or_insert(json!(0));
    out.entry("completion_tokens".to_string())
        .or_insert(json!(0));
    out.entry("input_seconds".to_string()).or_insert(json!(0));
    out.entry("output_seconds".to_string()).or_insert(json!(0));

    let num = |k: &str| -> f64 {
        out.get(k)
            .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64)))
            .unwrap_or(0.0)
    };
    let total_sec = num("output_seconds") + num("input_seconds");
    let total_sec_val = if total_sec.fract() == 0.0 {
        json!(total_sec as i64)
    } else {
        json!(total_sec)
    };
    out.insert("total_seconds".to_string(), total_sec_val);

    let p = out
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let c = out
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    out.insert("total_tokens".to_string(), json!(p + c));

    Some(Value::Object(out))
}

fn wrap_volcengine_create(raw: &str) -> String {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };
    if v.get("error").is_some() || v.pointer("/Response/Error").is_some() {
        return raw.to_string();
    }
    if v.get("id").is_some() && v.get("object").is_none() && v.get("status").is_none() {
        return raw.to_string();
    }
    let id = find_id(&v);
    if id.is_empty() {
        return raw.to_string();
    }
    json!({ "id": id }).to_string()
}

#[derive(serde::Serialize)]
struct VolcengineQueryResponse<'a> {
    id: &'a str,
    model: &'a str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
    created_at: i64,
    updated_at: i64,
    service_tier: &'a str,
    execution_expires_after: i64,
    generate_audio: bool,
    draft: bool,
    priority: i64,
    output_format: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Value>,
}

fn wrap_volcengine_query(raw: &str, model: &str, request_content: &str) -> String {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };

    if v.get("id").is_some()
        && v.get("status").is_some()
        && v.get("service_tier").is_some()
        && v.get("object").is_none()
        && (model.is_empty() || v.get("model").and_then(Value::as_str) == Some(model))
    {
        return raw.to_string();
    }

    let raw_status = crate::relay::response_formatter::extract_raw_status(&v);
    let standard = crate::relay::response_formatter::parse_raw_status_to_standard(&raw_status);
    let status = match standard {
        "completed" => "succeeded",
        "failed" => "failed",
        "cancelled" => "cancelled",
        "queued" => "queued",
        _ => "running",
    };

    let id = find_id(&v);
    let now_sec = chrono::Utc::now().timestamp();
    let create_time = v
        .get("created_at")
        .or_else(|| v.get("created"))
        .and_then(Value::as_i64)
        .map(|t| if t > 10_000_000_000 { t / 1000 } else { t })
        .unwrap_or(now_sec);
    let update_time = v
        .get("updated_at")
        .or_else(|| v.get("updated"))
        .and_then(Value::as_i64)
        .map(|t| if t > 10_000_000_000 { t / 1000 } else { t })
        .unwrap_or(now_sec);

    let req: Value = serde_json::from_str(request_content).unwrap_or(Value::Null);

    let final_model = if !model.is_empty() {
        model
    } else {
        v.get("model")
            .or_else(|| req.get("model"))
            .and_then(Value::as_str)
            .unwrap_or("")
    };

    let service_tier = v
        .get("service_tier")
        .or_else(|| req.get("service_tier"))
        .and_then(Value::as_str)
        .unwrap_or("default");
    let execution_expires_after = v
        .get("execution_expires_after")
        .or_else(|| req.get("execution_expires_after"))
        .and_then(Value::as_i64)
        .unwrap_or(3600);
    let generate_audio = v
        .get("generate_audio")
        .or_else(|| req.get("generate_audio"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let draft = v
        .get("draft")
        .or_else(|| req.get("draft"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let priority = v
        .get("priority")
        .or_else(|| req.get("priority"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output_format = v
        .get("output_format")
        .or_else(|| req.get("output_format"))
        .and_then(Value::as_str)
        .unwrap_or("mp4");

    let (content, seed, resolution, ratio, duration, usage, error) = if status == "succeeded" {
        let content = v
            .get("content")
            .cloned()
            .map(|mut c| {
                if c.get("last_frame_url").is_none() {
                    if let Some(lf) = find_last_frame_url(&v) {
                        c["last_frame_url"] = json!(lf);
                    }
                }
                c
            })
            .or_else(|| {
                let u = v
                    .pointer("/data/0/url")
                    .and_then(Value::as_str)
                    .map(String::from)
                    .or_else(|| find_urls(&v).into_iter().next())?;
                let mut c = json!({ "video_url": u });
                if let Some(lf) = find_last_frame_url(&v) {
                    c["last_frame_url"] = json!(lf);
                }
                Some(c)
            });

        let s = v
            .get("seed")
            .or_else(|| req.get("seed"))
            .and_then(Value::as_i64);
        let res = v
            .get("resolution")
            .or_else(|| req.get("resolution"))
            .and_then(Value::as_str)
            .map(String::from);
        let rat = v
            .get("ratio")
            .or_else(|| req.get("ratio"))
            .and_then(Value::as_str)
            .map(String::from);
        let dur = v
            .get("duration")
            .and_then(Value::as_f64)
            .or_else(|| v.pointer("/usage/output_seconds").and_then(Value::as_f64))
            .or_else(|| {
                v.pointer("/Response/AigcVideoTask/Output/FileInfos/0/MetaData/Duration")
                    .and_then(Value::as_f64)
            })
            .or_else(|| req.get("duration").and_then(Value::as_f64))
            .map(|f| f.round() as i64);

        let u = v.get("usage").cloned();

        (content, s, res, rat, dur, u, None)
    } else if status == "failed" {
        let err = v.get("error").cloned().unwrap_or_else(|| {
            let code = extract_error_code_from_value(&v)
                .unwrap_or_else(|| "generation_failed".to_string());
            let msg = extract_error_message_from_value(&v)
                .unwrap_or_else(|| "generation failed".to_string());
            json!({ "code": code, "message": msg })
        });
        (None, None, None, None, None, None, Some(err))
    } else {
        (None, None, None, None, None, None, None)
    };

    let resp = VolcengineQueryResponse {
        id: &id,
        model: final_model,
        status,
        error,
        created_at: create_time,
        updated_at: update_time,
        service_tier,
        execution_expires_after,
        generate_audio,
        draft,
        priority,
        output_format,
        content,
        seed,
        resolution,
        ratio,
        duration,
        usage,
    };

    serde_json::to_string(&resp).unwrap_or_else(|_| raw.to_string())
}

fn infer_vidu_type(path: &str) -> &'static str {
    let clean = path
        .split(['|', '?'])
        .next()
        .unwrap_or(path)
        .trim_end_matches('/');
    if clean.contains("text2video") {
        "text2video"
    } else if clean.contains("start-end2video") {
        "start-end2video"
    } else if clean.contains("reference2video") {
        "reference2video"
    } else {
        "img2video"
    }
}

fn wrap_vidu_create(path: &str, raw: &str, model: &str, request_content: &str) -> String {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };

    // 已具备纯净 Vidu 官方创建结构则原样返回
    if v.get("task_id").is_some()
        && v.get("state").is_some()
        && v.get("type").is_some()
        && v.get("object").is_none()
    {
        return raw.to_string();
    }

    if crate::relay::response_formatter::is_upstream_error_response(&v) {
        let msg = extract_error_message_from_value(&v).unwrap_or_else(|| "request failed".to_string());
        let code = extract_error_code_from_value(&v)
            .and_then(|c| c.parse::<i64>().ok())
            .unwrap_or(400);
        return json!({
            "code": code,
            "message": msg
        })
        .to_string();
    }

    let id = find_id(&v);
    if id.is_empty() {
        return raw.to_string();
    }

    let vidu_type = infer_vidu_type(path);

    let created_at = v
        .get("created_at")
        .and_then(Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));

    let final_model = if !model.is_empty() {
        model
    } else {
        v.get("model").and_then(Value::as_str).unwrap_or("")
    };

    let mut out = json!({
        "task_id": id,
        "type": vidu_type,
        "state": "created",
        "created_at": created_at
    });
    if !final_model.is_empty() {
        out["model"] = json!(final_model);
    }

    // 从请求体同步基础参数（style, prompt, images, duration, seed, aspect_ratio, resolution, movement_amplitude）
    if let Ok(req) = serde_json::from_str::<Value>(request_content) {
        for key in [
            "style",
            "prompt",
            "images",
            "duration",
            "seed",
            "aspect_ratio",
            "resolution",
            "movement_amplitude",
        ] {
            if let Some(val) = req.get(key) {
                out[key] = val.clone();
            }
        }
    }
    if let Some(c) = v.get("credits") {
        out["credits"] = c.clone();
    }

    out.to_string()
}

fn wrap_vidu_query(path: &str, raw: &str, model: &str) -> String {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };

    let id = find_id(&v);
    if id.is_empty() {
        return raw.to_string();
    }

    let vidu_type = infer_vidu_type(path);

    // 已是纯净 Vidu 官方查询结构（使用 id 且含 state、type、err_code 且无 OpenAI 标识）则原样返回
    if v.get("id").is_some()
        && v.get("state").is_some()
        && v.get("type").is_some()
        && v.get("err_code").is_some()
        && v.get("object").is_none()
    {
        return raw.to_string();
    }

    let raw_status = crate::relay::response_formatter::extract_raw_status(&v);
    let standard = crate::relay::response_formatter::parse_raw_status_to_standard(&raw_status);

    let final_model = if !model.is_empty() {
        model
    } else {
        v.get("model").and_then(Value::as_str).unwrap_or("")
    };

    if standard == "completed" {
        let urls = find_urls(&v);
        let cover_url = find_last_frame_url(&v);
        let creations: Vec<Value> = if let Some(arr) = v.get("creations").and_then(Value::as_array) {
            arr.clone()
        } else {
            urls.into_iter()
                .enumerate()
                .map(|(idx, u)| {
                    let cid = format!("{}_{}", id, idx);
                    let mut item = json!({
                        "id": cid,
                        "url": u
                    });
                    if let Some(ref c) = cover_url {
                        item["cover_url"] = json!(c);
                    }
                    item
                })
                .collect()
        };

        let mut out = json!({
            "id": id,
            "type": vidu_type,
            "state": "success",
            "err_code": "",
            "err_msg": "",
            "creations": creations
        });
        if !final_model.is_empty() {
            out["model"] = json!(final_model);
        }
        if let Some(c) = v.get("credits") {
            out["credits"] = c.clone();
        }
        out.to_string()
    } else if standard == "failed" || crate::relay::response_formatter::is_upstream_error_response(&v) {
        let code = extract_error_code_from_value(&v).unwrap_or_else(|| "1000".to_string());
        let msg = extract_error_message_from_value(&v).unwrap_or_else(|| "generation failed".to_string());
        let mut out = json!({
            "id": id,
            "type": vidu_type,
            "state": "failed",
            "err_code": code,
            "err_msg": msg,
            "creations": []
        });
        if !final_model.is_empty() {
            out["model"] = json!(final_model);
        }
        out.to_string()
    } else {
        let state = if standard == "pending" {
            "queueing"
        } else {
            "processing"
        };
        let mut out = json!({
            "id": id,
            "type": vidu_type,
            "state": state,
            "err_code": "",
            "err_msg": ""
        });
        if !final_model.is_empty() {
            out["model"] = json!(final_model);
        }
        out.to_string()
    }
}

/// 日志还没有上游响应时，按本次请求路径组装受理体。
pub fn route_accept_body(path: &str, task_id: &str, model: &str) -> String {
    let now = chrono::Utc::now().timestamp();
    let seed = json!({
        "id": task_id,
        "task_id": task_id,
        "status": "pending",
        "created": now,
    })
    .to_string();
    let p = path
        .split(['|', '?'])
        .next()
        .unwrap_or(path)
        .trim_end_matches('/');
    if crate::relay::response_formatter::is_openai_compatible_path(p) {
        return crate::relay::response_formatter::format_openai(
            "视频",
            &seed,
            false,
            Some(task_id),
        );
    }
    if p.contains("/video-generation/video-synthesis") || p.starts_with("/api/v1/tasks/") {
        return json!({
            "request_id": task_id,
            "output": { "task_id": task_id, "task_status": "PENDING" }
        })
        .to_string();
    }
    wrap_official_client(p, &seed, model, "")
}

/// 本次视频受理（POST 已返回系统任务号，上游提交在后台）。
pub fn tag_is_video_accept(tag: &str) -> bool {
    serde_json::from_str::<Value>(tag)
        .ok()
        .and_then(|v| v.get("la").and_then(Value::as_i64))
        == Some(1)
}

/// 新受理且响应仍为空、上游号还没写下：轮询只回受理体。
pub fn awaiting_video_submit(response: &str, tag: &str) -> bool {
    if !response.trim().is_empty() {
        return false;
    }
    let Ok(v) = serde_json::from_str::<Value>(tag) else {
        return false;
    };
    v.get("la").and_then(Value::as_i64) == Some(1)
        && !v
            .get("upstream_task")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
}

pub fn remember_upstream_task(tag: &mut Option<String>, upstream_id: &str) {
    let id = upstream_id.trim();
    if id.is_empty() {
        return;
    }
    let mut v: Value = tag
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| json!({}));
    let Some(obj) = v.as_object_mut() else {
        return;
    };
    obj.insert("la".into(), json!(1));
    obj.insert("upstream_task".into(), json!(id));
    *tag = Some(v.to_string());
}

#[cfg(test)]
mod route_accept_tests {
    use super::*;

    #[test]
    fn awaiting_only_when_new_accept_has_no_result() {
        assert!(awaiting_video_submit("", r#"{"la":1}"#));
        assert!(!awaiting_video_submit("", r#"{}"#));
        assert!(!awaiting_video_submit(r#"{"id":"x"}"#, r#"{"la":1}"#));
        assert!(!awaiting_video_submit(
            "",
            r#"{"la":1,"upstream_task":"up-1"}"#
        ));
    }

    #[test]
    fn route_bodies_use_system_task_id() {
        let id = "tsk_abc";
        let openai = route_accept_body("/v1/video/generations", id, "m");
        assert!(openai.contains(id) && openai.contains("pending"));
        let volc = route_accept_body("/api/v3/contents/generations/tasks", id, "m");
        assert!(volc.contains(id));
        let ali = route_accept_body(
            "/api/v1/services/aigc/video-generation/video-synthesis",
            id,
            "m",
        );
        assert!(ali.contains("PENDING") && ali.contains(id));
        let poll = route_accept_body("/v1/video/generations/tsk_abc", id, "m");
        assert!(poll.contains(id) && poll.contains("pending"));
    }

    #[test]
    fn wrap_kling_create_transforms_tencent_post_error() {
        let tencent_err = r#"{"Response":{"Error":{"Code":"MissingParameter","Message":"The request is missing a required parameter `ModelVersion`."},"RequestId":"61b9fc3a-1234"}}"#;
        let wrapped = wrap_official_client("/omni-video/kling-v3-omni", tencent_err, "kling-v3-omni", "");
        let v: Value = serde_json::from_str(&wrapped).unwrap();
        assert_eq!(v["code"], 400);
        assert_eq!(
            v["message"],
            "The request is missing a required parameter `ModelVersion`."
        );
        assert_eq!(v["request_id"], "61b9fc3a-1234");
    }

    #[test]
    fn test_wrap_vidu_create_success() {
        let tencent_create = r#"{"Response":{"TaskId":"1251015623-vidu999","RequestId":"req-v1"}}"#;
        let req_body = r#"{"prompt":"让小狗跑起来","duration":3,"resolution":"540p"}"#;
        let wrapped = wrap_official_client("/ent/v2/img2video", tencent_create, "viduq4", req_body);
        let v: Value = serde_json::from_str(&wrapped).unwrap();
        assert_eq!(v["task_id"], "1251015623-vidu999");
        assert_eq!(v["type"], "img2video");
        assert_eq!(v["state"], "created");
        assert_eq!(v["model"], "viduq4");
        assert_eq!(v["prompt"], "让小狗跑起来");
        assert_eq!(v["duration"], 3);
        assert_eq!(v["resolution"], "540p");
        assert!(v["created_at"].as_str().is_some());

        // 验证其他 3 种路由的 type 区分
        let wrap_text = wrap_official_client("/ent/v2/text2video", tencent_create, "viduq4", "");
        assert_eq!(serde_json::from_str::<Value>(&wrap_text).unwrap()["type"], "text2video");

        let wrap_se = wrap_official_client("/ent/v2/start-end2video", tencent_create, "viduq4", "");
        assert_eq!(serde_json::from_str::<Value>(&wrap_se).unwrap()["type"], "start-end2video");

        let wrap_ref = wrap_official_client("/ent/v2/reference2video", tencent_create, "viduq4", "");
        assert_eq!(serde_json::from_str::<Value>(&wrap_ref).unwrap()["type"], "reference2video");
    }

    #[test]
    fn test_wrap_vidu_create_error() {
        let tencent_err = r#"{"Response":{"Error":{"Code":"FailedOperation","Message":"Balance insufficient."},"RequestId":"req-err"}}"#;
        let wrapped = wrap_official_client("/ent/v2/text2video", tencent_err, "viduq4", "");
        let v: Value = serde_json::from_str(&wrapped).unwrap();
        assert_eq!(v["code"], 400);
        assert_eq!(v["message"], "Balance insufficient.");
    }

    #[test]
    fn test_wrap_vidu_query_success() {
        let openai_poll_succ = r#"{
            "id": "1251015623-vidu999",
            "object": "video.generation",
            "status": "completed",
            "created": 1791541496,
            "data": [
                {
                    "url": "https://example.com/output.mp4",
                    "last_frame_url": "https://example.com/cover.jpg"
                }
            ]
        }"#;
        let req_body = r#"{"type":"img2video","prompt":"dog running"}"#;
        let wrapped = wrap_official_client("/ent/v2/tasks/1251015623-vidu999/creations", openai_poll_succ, "viduq4", req_body);
        let v: Value = serde_json::from_str(&wrapped).unwrap();
        // 关键断言：任务 id 参数名为 id 而不是 task_id
        assert_eq!(v["id"], "1251015623-vidu999");
        assert!(v.get("task_id").is_none());
        assert_eq!(v["type"], "img2video");
        assert_eq!(v["state"], "success");
        assert_eq!(v["err_code"], "");
        assert_eq!(v["err_msg"], "");
        let creations = v["creations"].as_array().unwrap();
        assert_eq!(creations.len(), 1);
        assert_eq!(creations[0]["id"], "1251015623-vidu999_0");
        assert_eq!(creations[0]["url"], "https://example.com/output.mp4");
        assert_eq!(creations[0]["cover_url"], "https://example.com/cover.jpg");
        assert!(v.get("created_at").is_none());
    }

    #[test]
    fn test_wrap_vidu_query_processing() {
        let openai_poll_proc = r#"{
            "id": "1251015623-vidu999",
            "object": "video.generation",
            "status": "in_progress",
            "created": 1791541496
        }"#;
        let wrapped = wrap_official_client("/ent/v2/tasks/1251015623-vidu999/creations", openai_poll_proc, "viduq4", "");
        let v: Value = serde_json::from_str(&wrapped).unwrap();
        assert_eq!(v["id"], "1251015623-vidu999");
        assert!(v.get("task_id").is_none());
        assert_eq!(v["type"], "img2video");
        assert_eq!(v["state"], "processing");
        assert_eq!(v["err_code"], "");
        assert_eq!(v["err_msg"], "");
    }

    #[test]
    fn test_wrap_vidu_query_failed() {
        let openai_poll_fail = r#"{
            "id": "1251015623-vidu999",
            "object": "video.generation",
            "status": "failed",
            "error": {
                "code": "AuditFailed",
                "message": "Sensitive content detected"
            }
        }"#;
        let wrapped = wrap_official_client("/ent/v2/tasks/1251015623-vidu999/creations", openai_poll_fail, "viduq4", "");
        let v: Value = serde_json::from_str(&wrapped).unwrap();
        assert_eq!(v["id"], "1251015623-vidu999");
        assert!(v.get("task_id").is_none());
        assert_eq!(v["type"], "img2video");
        assert_eq!(v["state"], "failed");
        assert_eq!(v["err_code"], "AuditFailed");
        assert_eq!(v["err_msg"], "Sensitive content detected");
    }

    #[test]
    fn test_wrap_vidu_query_passthrough() {
        // 已具备纯净官方查询结构，直接原样透传
        let raw_clean = r#"{"id":"task-123","state":"success","type":"img2video","err_code":"","creations":[]}"#;
        let wrapped = wrap_official_client("/ent/v2/tasks/task-123/creations", raw_clean, "viduq4", "");
        assert_eq!(wrapped, raw_clean);
    }

    #[test]
    fn test_to_official_query_path_vidu() {
        let path = to_official_query_path("/ent/v2/img2video", "task_abc");
        assert_eq!(path, "/ent/v2/tasks/task_abc/creations");

        let path_text = to_official_query_path("/ent/v2/text2video", "task_xyz");
        assert_eq!(path_text, "/ent/v2/tasks/task_xyz/creations");
    }
}
