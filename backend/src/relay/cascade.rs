/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use super::proxy;
use super::task::normalize_task_status;
use crate::error::{AppError, AppResult};
use crate::models::{BillingRule, Channel};
use crate::relay::{forward, response_formatter};
use crate::AppState;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[cfg(feature = "plugin_volcengine_enhance")]
use crate::api::plugins::is_volc_enhance_version;
#[cfg(not(feature = "plugin_volcengine_enhance"))]
fn is_volc_enhance_version(_: &str) -> bool {
    false
}

fn cascade_is_res(s: &str) -> bool {
    let norm = crate::relay::usage_extractor::normalize_resolution_label(s);
    !norm.is_empty() && norm.chars().any(|c| c.is_ascii_digit())
}

/// 有分辨率计费时返回已启用集合；非分辨率计费或无配置则返回 None
fn cascade_billing_enabled_resolutions(
    rule: &BillingRule,
    cascade_version: &str,
) -> Option<HashSet<String>> {
    // requests (按次计费) 不参与分辨率拦截
    if rule.billing_type.eq_ignore_ascii_case("requests") {
        return None;
    }
    if (rule.extended_config.is_empty() || rule.extended_config == "{}")
        && (rule.pricing_tiers.is_empty() || rule.pricing_tiers == "[]")
    {
        return None;
    }

    let ext: serde_json::Value = serde_json::from_str(&rule.extended_config).unwrap_or_default();
    let mut has_res_billing = false;
    let mut enabled = HashSet::new();

    if let Some(rates) = ext.get("resolution_rates").and_then(|v| v.as_object()) {
        has_res_billing = true;
        for k in rates.keys().filter(|k| cascade_is_res(k)) {
            enabled.insert(k.to_ascii_lowercase());
        }
    }

    if let Some(pt) = ext.get("price_table").and_then(|v| v.as_object()) {
        let disabled: HashSet<String> = ext
            .get("price_table_disabled")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_ascii_lowercase()))
                    .collect()
            })
            .unwrap_or_default();
        let ver_filter = cascade_version.trim();
        for key in pt.keys() {
            let lower = key.to_ascii_lowercase();
            let parts: Vec<&str> = lower.split('|').collect();
            let res = match parts.as_slice() {
                [ver, res, ..] if is_volc_enhance_version(ver) && cascade_is_res(res) => {
                    has_res_billing = true;
                    if ver_filter.is_empty() || ver.eq_ignore_ascii_case(ver_filter) {
                        Some(*res)
                    } else {
                        None
                    }
                }
                [attr, res] if !is_volc_enhance_version(attr) && cascade_is_res(res) => {
                    has_res_billing = true;
                    Some(*res)
                }
                _ => None,
            };
            if let Some(res) = res {
                if !disabled.contains(&lower) {
                    enabled.insert(res.to_string());
                }
            }
        }
    }

    if !rule.pricing_tiers.is_empty() && rule.pricing_tiers != "[]" {
        if let Ok(tiers) = serde_json::from_str::<Vec<serde_json::Value>>(&rule.pricing_tiers) {
            for tier in tiers {
                let Some(res) = tier.get("resolution").and_then(|v| v.as_str()) else {
                    continue;
                };
                if !cascade_is_res(res) {
                    continue;
                }
                has_res_billing = true;
                if tier
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
                {
                    enabled.insert(res.trim().to_ascii_lowercase());
                }
            }
        }
    }

    has_res_billing.then_some(enabled)
}

/// 格式与计费启用校验：仅当模型存在生效的分辨率计费配置时才进行开启拦截；非分辨率计费则直接放行。
pub(crate) fn cascade_check_resolution(
    db_rule: Option<&BillingRule>,
    cascade_version: &str,
    res_str: &str,
) -> AppResult<()> {
    let Some(rule) = db_rule else {
        return Ok(());
    };
    let Some(enabled) = cascade_billing_enabled_resolutions(rule, cascade_version) else {
        return Ok(());
    };
    let key = res_str.trim().to_ascii_lowercase();
    if enabled.contains(&key) {
        return Ok(());
    }
    Err(AppError::BadRequest(format!(
        "当前分辨率 {} 不支持",
        res_str.trim()
    )))
}

/// 解析级联底座分辨率：预设档位安全回退默认一级底座，自定义档位未配置底座则抛错避免无意义调用
pub(crate) fn cascade_resolve_base(
    target: &str,
    res_base: &HashMap<String, String>,
) -> AppResult<String> {
    let key = crate::relay::usage_extractor::normalize_resolution_label(target);
    let configured = res_base
        .get(&key)
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty());

    match key.as_str() {
        "480p" => Ok(configured.unwrap_or("480p").into()),
        "540p" => Ok(configured.filter(|&b| b == "480p" || b == "540p").unwrap_or("540p").into()),
        "720p" => Ok(configured.filter(|&b| b == "480p" || b == "720p").unwrap_or("480p").into()),
        "1080p" => Ok(configured.filter(|&b| matches!(b, "720p" | "480p" | "1080p")).unwrap_or("720p").into()),
        "2k" | "4k" => Ok(configured.filter(|&b| matches!(b, "1080p" | "720p" | "480p")).unwrap_or("1080p").into()),
        _ => configured
            .map(|b| b.trim().to_string())
            .ok_or_else(|| AppError::BadRequest(format!("级联规则未配置分辨率 {} 的底座", target.trim()))),
    }
}

/// 根据目标分辨率与成片宽高比（如 "1:1", "3:4" 等），从规则中解析短边限制像素
pub(crate) fn resolve_cascade_res_limit(
    res_limit: &HashMap<String, i64>,
    target_res: &str,
    ratio_opt: Option<&str>,
) -> Option<i64> {
    let norm_res = target_res.trim().to_ascii_lowercase();
    // 1. 优先匹配指定比例（如 "720p@1:1"）
    if let Some(r) = ratio_opt {
        let norm_ratio = r.trim().to_ascii_lowercase();
        let key = format!("{}@{}", norm_res, norm_ratio);
        if let Some(&limit) = res_limit.get(&key).filter(|&&l| (128..=4320).contains(&l)) {
            return Some(limit);
        }
    }
    // 2. 兜底匹配该分辨率全局短边（如 "720p"）
    res_limit.get(&norm_res).copied().filter(|&l| (128..=4320).contains(&l))
}

/// 480→720 且 ratio∈{16:9,9:16} 的裁剪角点。仅信 S1 回显分辨率；ratio 优先 S1，缺则 hints。
fn cascade_crop_rect(
    stage1_resp: &serde_json::Value,
    target_resolution: &str,
    field_hints: &[&serde_json::Value],
    resolved: &forward::ResolvedForward,
    post: bool,
) -> Option<(i32, i32, i32, i32)> {
    if !target_resolution.eq_ignore_ascii_case("720p") {
        return None;
    }
    if cascade_json_root_str(stage1_resp, "resolution")
        .is_some_and(|r| !r.eq_ignore_ascii_case("480p"))
    {
        return None;
    }
    let ratio = cascade_json_root_str(stage1_resp, "ratio").or_else(|| {
        field_hints
            .iter()
            .find_map(|h| cascade_json_root_str(h, "ratio"))
    })?;
    let is_tencent = resolved.cascade_engine.eq_ignore_ascii_case("tencent");
    if post {
        match ratio {
            "16:9" => Some(
                resolved
                    .crop_coords_720p
                    .get("16:9")
                    .copied()
                    .unwrap_or((1, 0, 1281, 720)),
            ),
            "9:16" => Some(
                resolved
                    .crop_coords_720p
                    .get("9:16")
                    .copied()
                    .unwrap_or((0, 1, 720, 1281)),
            ),
            _ => None,
        }
    } else {
        match ratio {
            "16:9" => Some(resolved.crop_coords_480p.get("16:9").copied().unwrap_or(
                if is_tencent {
                    (0, 5, 864, 491)
                } else {
                    (2, 6, 862, 490)
                },
            )),
            "9:16" => Some(resolved.crop_coords_480p.get("9:16").copied().unwrap_or(
                if is_tencent {
                    (5, 0, 491, 864)
                } else {
                    (6, 2, 490, 862)
                },
            )),
            _ => None,
        }
    }
}

fn cascade_engine(plugin_tag: &str) -> String {
    cascade_json_ptr(plugin_tag, "/cascade/engine", true).unwrap_or_else(|| "volc".into())
}

async fn cascade_maybe_crop(
    state: &AppState,
    engine: &str,
    video_url: &str,
    s1: &serde_json::Value,
    target_res: &str,
    hints: &[&serde_json::Value],
    resolved: &forward::ResolvedForward,
    post: bool,
    ch: &Channel,
    auth_type: &str,
    relay_log_id: i64,
) -> String {
    let Some((tlx, tly, brx, bry)) = cascade_crop_rect(s1, target_res, hints, resolved, post)
    else {
        return video_url.to_string();
    };
    let cropped = match engine {
        "tencent" => {
            #[cfg(feature = "plugin_tencent_enhance")]
            {
                crate::api::plugins::crop_tencent_cascade_video(
                    state,
                    video_url,
                    tlx,
                    tly,
                    brx,
                    bry,
                    Some(relay_log_id),
                )
                .await
                .ok()
            }
            #[cfg(not(feature = "plugin_tencent_enhance"))]
            {
                let _ = (state, tlx, tly, brx, bry, relay_log_id);
                None
            }
        }
        _ => {
            #[cfg(feature = "plugin_volcengine_enhance")]
            {
                crate::api::plugins::crop_volc_cascade_video(
                    state,
                    ch,
                    auth_type,
                    video_url,
                    tlx,
                    tly,
                    brx,
                    bry,
                    Some(relay_log_id),
                )
                .await
                .ok()
            }
            #[cfg(not(feature = "plugin_volcengine_enhance"))]
            {
                let _ = (ch, auth_type, tlx, tly, brx, bry);
                None
            }
        }
    };
    cropped.unwrap_or_else(|| video_url.to_string())
}

pub(crate) fn cascade_is_skip_s2(plugin_tag: &str) -> bool {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(plugin_tag) else {
        return false;
    };
    v.pointer("/cascade/skip_s2")
        .and_then(|b| b.as_bool())
        .unwrap_or(false)
}

/// 级联跳过 S2 时：直接对 S1 响应中的 usage 按 res_mul 分辨率倍率放大，并返回实际生效倍率
pub(crate) fn cascade_apply_s1_res_mul(
    body: &mut String,
    plugin_tag: &str,
    req_content: &str,
    res_mul: &HashMap<String, f64>,
) -> f64 {
    let res = cascade_json_str(plugin_tag, "/cascade/resolution")
        .or_else(|| cascade_json_str(req_content, "/resolution"))
        .unwrap_or_else(|| "720p".into());
    let mul = forward::lookup_res_mul(res_mul, &res);
    if (mul - 1.0).abs() > 1e-9 {
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(body) {
            forward::scale_usage_in_json(&mut v, mul);
            *body = v.to_string();
        }
    }
    mul
}

fn cascade_s2_ready(plugin_tag: &str) -> Option<(String, String)> {
    let v = serde_json::from_str::<serde_json::Value>(plugin_tag).ok()?;
    let r = v.get("cascade")?.get("s2_ready")?;
    Some((
        r.get("s1")?.as_str()?.to_string(),
        r.get("s2")?.as_str()?.to_string(),
    ))
}

fn s2_first_video_url(s2: &serde_json::Value) -> Option<String> {
    response_formatter::find_urls(s2)
        .into_iter()
        .next()
        .filter(|s| !s.is_empty())
}

fn set_s2_video_url(s2: &mut serde_json::Value, url: &str) {
    if let Some(res_obj) = s2.pointer_mut("/result").and_then(|v| v.as_object_mut()) {
        res_obj.insert("video_url".into(), serde_json::json!(url));
    }
}

/// 倍率 + 后裁 + 抽尾帧。失败跳过，不整单失败。
async fn cascade_s2_postprocess(
    state: &AppState,
    poll: &CascadePollTarget<'_>,
    s1_raw: &mut String,
    s2_raw: &mut String,
    plugin_tag: &str,
    request_content: &str,
    relay_log_id: i64,
) {
    let s2_ch = poll.channel.as_ref();
    let resolved = poll.resolved.as_ref();
    let s2_auth = resolved.auth_type.as_str();
    let engine = cascade_engine(plugin_tag);
    let mut s1: serde_json::Value = serde_json::from_str(s1_raw).unwrap_or(serde_json::json!({}));
    let res = cascade_json_str(plugin_tag, "/cascade/resolution")
        .or_else(|| cascade_json_str(request_content, "/resolution"))
        .unwrap_or_else(|| "720p".into());
    let mul = forward::lookup_res_mul(&resolved.res_mul, &res);
    if (mul - 1.0).abs() > 1e-9 {
        forward::scale_usage_in_json(&mut s1, mul);
        if let Some(u) = s1.get("usage") {
            response_formatter::json_root_set(s1_raw, "usage", &u.to_string());
        }
    }

    let mut s2: serde_json::Value = serde_json::from_str(s2_raw).unwrap_or(serde_json::json!({}));
    let mut s2_modified = false;

    // 增强后裁剪：crop_480p 且时机 post 时，把 S2 的 720p 裁到标准画布
    if resolved.crop_480p
        && resolved.crop_timing == "post"
        && s2.get("cropped_720p").and_then(|v| v.as_bool()) != Some(true)
    {
        if let Some(video_url) = s2_first_video_url(&s2) {
            let req_hint: serde_json::Value =
                serde_json::from_str(request_content).unwrap_or(serde_json::json!({}));
            let cropped_url = cascade_maybe_crop(
                state,
                &engine,
                &video_url,
                &s1,
                &res,
                &[&req_hint],
                resolved,
                true,
                s2_ch,
                s2_auth,
                relay_log_id,
            )
            .await;
            if cropped_url != video_url {
                set_s2_video_url(&mut s2, &cropped_url);
                if let Some(obj) = s2.as_object_mut() {
                    obj.insert("cropped_720p".into(), serde_json::json!(true));
                }
                s2_modified = true;
            }
        }
    }

    let output_format = cascade_json_str(plugin_tag, "/cascade/output_format")
        .or_else(|| cascade_json_str(request_content, "/output_format"))
        .unwrap_or_default();

    if output_format.eq_ignore_ascii_case("mov") {
        if let Some(video_url) = s2_first_video_url(&s2) {
            let mov_res: Result<String, String> = match engine.as_str() {
                "tencent" => {
                    #[cfg(feature = "plugin_tencent_enhance")]
                    {
                        crate::api::plugins::transcode_tencent_mov(
                            state,
                            &video_url,
                            Some(relay_log_id),
                        )
                        .await
                    }
                    #[cfg(not(feature = "plugin_tencent_enhance"))]
                    {
                        let _ = state;
                        Err("增强插件未编译".into())
                    }
                }
                _ => {
                    #[cfg(feature = "plugin_volcengine_enhance")]
                    {
                        crate::api::plugins::transcode_volc_mov(
                            state,
                            s2_ch,
                            s2_auth,
                            &video_url,
                            Some(relay_log_id),
                        )
                        .await
                    }
                    #[cfg(not(feature = "plugin_volcengine_enhance"))]
                    {
                        Err("增强插件未编译".into())
                    }
                }
            };
            match mov_res {
                Ok(mov_url) => {
                    set_s2_video_url(&mut s2, &mov_url);
                    s2_modified = true;
                }
                Err(e) => {
                    crate::relay_debug!("[Cascade S2] MOV 转码失败: {}", e);
                }
            }
        }
    }

    if response_formatter::find_last_frame_url(&s1).is_some() {
        if let Some(video_url) = s2_first_video_url(&s2) {
            let frame: Option<String> = match engine.as_str() {
                "tencent" => {
                    #[cfg(feature = "plugin_tencent_enhance")]
                    {
                        crate::api::plugins::extract_tencent_last_frame(
                            state,
                            &video_url,
                            Some(relay_log_id),
                        )
                        .await
                        .ok()
                    }
                    #[cfg(not(feature = "plugin_tencent_enhance"))]
                    {
                        let _ = state;
                        None
                    }
                }
                _ => {
                    #[cfg(feature = "plugin_volcengine_enhance")]
                    {
                        crate::api::plugins::extract_volc_last_frame(
                            state,
                            s2_ch,
                            s2_auth,
                            &video_url,
                            Some(relay_log_id),
                        )
                        .await
                        .ok()
                    }
                    #[cfg(not(feature = "plugin_volcengine_enhance"))]
                    {
                        None
                    }
                }
            };
            if let Some(frame) = frame {
                if let Some(obj) = s2.as_object_mut() {
                    obj.insert("last_frame_url".into(), serde_json::json!(frame));
                }
                s2_modified = true;
            } else {
                crate::relay_debug!("[Cascade S2] 尾帧跳过");
            }
        }
    }

    if s2_modified {
        *s2_raw = s2.to_string();
    }
}

/// S2 成功落库前：进程内防并发；usage×res_mul；按需后裁；S1 有尾帧则抽帧写入 `s2.last_frame_url`。
/// 已有 `s2_ready` 则立刻交 Guard 给结算；否则后台做后处理，本次回 None（调用方回处理中）。
pub(crate) async fn cascade_on_s2_succeeded<'a>(
    state: &'a Arc<AppState>,
    log_id: i64,
    poll: &CascadePollTarget<'_>,
    s1_raw: &mut String,
    s2_raw: &mut String,
    plugin_tag: &str,
    request_content: &str,
) -> Option<CascadeS2InflightGuard<'a>> {
    if let Some((s1, s2)) = cascade_s2_ready(plugin_tag) {
        *s1_raw = s1;
        *s2_raw = s2;
        return CascadeS2InflightGuard::try_acquire(&state.cascade_s2_inflight, log_id);
    }
    if state.cascade_s2_inflight.contains_key(&log_id) {
        return None;
    }
    let state_bg = Arc::clone(state);
    let poll = poll.to_owned();
    let mut s1 = s1_raw.clone();
    let mut s2 = s2_raw.clone();
    let plugin_tag = plugin_tag.to_string();
    let request_content = request_content.to_string();
    tokio::spawn(async move {
        let Some(_g) = CascadeS2InflightGuard::try_acquire(&state_bg.cascade_s2_inflight, log_id)
        else {
            return;
        };
        cascade_s2_postprocess(
            &state_bg,
            &poll,
            &mut s1,
            &mut s2,
            &plugin_tag,
            &request_content,
            log_id,
        )
        .await;
        let mut v =
            serde_json::from_str::<serde_json::Value>(&plugin_tag).unwrap_or(serde_json::json!({}));
        v["cascade"]["s2_ready"] = serde_json::json!({ "s1": s1, "s2": s2 });
        let t = v.to_string();
        let _ = sqlx::query(
            &state_bg
                .db
                .format_query("UPDATE logs SET plugin_tag = ? WHERE id = ?"),
        )
        .bind(&t)
        .bind(log_id)
        .execute(&state_bg.db.pool)
        .await;
    });
    None
}

/// 阶段一出参 + 阶段二增强请求；`s1_raw` 空（未开 enable_log）→ None。
fn cascade_upstream_req_combined(s1_raw: &str, s2: &serde_json::Value) -> Option<String> {
    if s1_raw.is_empty() {
        return None;
    }
    let s1: serde_json::Value = serde_json::from_str(s1_raw).unwrap_or(serde_json::json!({}));
    Some(serde_json::json!({ "stage1": s1, "stage2": s2 }).to_string())
}

/// JSON 指针取非空字符串；`lower=true` 时转小写（分辨率等），任务 id 等保持原样
fn cascade_json_ptr(json: &str, pointer: &str, lower: bool) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| {
            v.pointer(pointer)?.as_str().map(|s| {
                let t = s.trim();
                if lower {
                    t.to_ascii_lowercase()
                } else {
                    t.to_string()
                }
            })
        })
        .filter(|s| !s.is_empty())
}

#[inline]
pub(crate) fn cascade_json_str(json: &str, pointer: &str) -> Option<String> {
    cascade_json_ptr(json, pointer, true)
}

fn cascade_json_root_str<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

async fn cascade_s2_maybe_pre_crop(
    state: &AppState,
    ctx: &CascadeS2SubmitCtx,
    s1_json: &serde_json::Value,
    target_resolution: &str,
    engine: &str,
    ch: &Channel,
    auth_type: &str,
) -> String {
    if !(ctx.resolved.crop_480p && ctx.resolved.crop_timing != "post") {
        return ctx.base_video_url.clone();
    }
    let req_hint: serde_json::Value =
        serde_json::from_str(&ctx.request_content).unwrap_or(serde_json::json!({}));
    let up_hint: serde_json::Value =
        serde_json::from_str(&ctx.upstream_req).unwrap_or(serde_json::json!({}));
    cascade_maybe_crop(
        state,
        engine,
        &ctx.base_video_url,
        s1_json,
        target_resolution,
        &[&up_hint, &req_hint],
        &ctx.resolved,
        false,
        ch,
        auth_type,
        ctx.log_id,
    )
    .await
}

pub(crate) struct CascadeS2PollOk {
    pub url: String,
    pub body: String,
    pub resp_json: serde_json::Value,
    pub task_status: String,
}

pub(crate) enum CascadeS2PollErr {
    Retryable {
        status: u16,
        message: String,
    },
    /// 插件未编译：不回落通用 GET。两家都编译时不会构造。
    #[cfg(not(all(
        feature = "plugin_tencent_enhance",
        feature = "plugin_volcengine_enhance"
    )))]
    Settled {
        status: u16,
        message: String,
    },
}

/// S2 单次查询：按引擎走插件；未编译则 Settled，不回落通用 GET。
#[cfg_attr(
    not(any(
        feature = "plugin_tencent_enhance",
        feature = "plugin_volcengine_enhance"
    )),
    allow(unreachable_code, unused_variables)
)]
pub(crate) async fn cascade_s2_poll(
    state: &AppState,
    plugin_tag: &str,
    poll: &CascadePollTarget<'_>,
) -> Result<CascadeS2PollOk, CascadeS2PollErr> {
    let task_id = poll.task_id.as_ref();
    let r = if cascade_engine(plugin_tag) == "tencent" {
        #[cfg(not(feature = "plugin_tencent_enhance"))]
        {
            let _ = state;
            return Err(CascadeS2PollErr::Settled {
                status: 400,
                message: "增强插件未编译".into(),
            });
        }
        #[cfg(feature = "plugin_tencent_enhance")]
        crate::api::plugins::poll_tencent_cascade_stage2(state, task_id).await
    } else {
        #[cfg(not(feature = "plugin_volcengine_enhance"))]
        {
            let _ = poll;
            return Err(CascadeS2PollErr::Settled {
                status: 400,
                message: "增强插件未编译".into(),
            });
        }
        #[cfg(feature = "plugin_volcengine_enhance")]
        crate::api::plugins::poll_volc_cascade_stage2(
            state,
            poll.channel.as_ref(),
            poll.resolved.as_ref(),
            task_id,
            poll.model.as_ref(),
        )
        .await
    };
    match r {
        Ok(resp_json) => {
            let task_status =
                normalize_task_status(&response_formatter::extract_raw_status(&resp_json))
                    .to_string();
            Ok(CascadeS2PollOk {
                url: format!("cascade_s2:{task_id}"),
                body: resp_json.to_string(),
                resp_json,
                task_status,
            })
        }
        Err(message) => Err(CascadeS2PollErr::Retryable {
            status: 502,
            message,
        }),
    }
}

/// 写入 `cascade.s1_task_id`（仅内部）。对外任务号沿用 POST 受理时的系统号。
pub(crate) fn cascade_remember_s1(plugin_tag: &mut Option<String>, upstream_s1_id: &str) {
    let upstream_s1_id = upstream_s1_id.trim();
    if upstream_s1_id.is_empty() {
        return;
    }
    let mut v: serde_json::Value = plugin_tag
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let Some(obj) = v.as_object_mut() else {
        return;
    };
    let cascade = obj
        .entry("cascade")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(c) = cascade.as_object_mut() {
        c.insert("s1_task_id".into(), serde_json::json!(upstream_s1_id));
        *plugin_tag = Some(v.to_string());
    }
}

/// 级联成功对外：S1 原文骨架叠 S2 产物 URL / 分辨率 / 帧率 / 尾帧；原位改字符串
fn cascade_s1_with_s2_url(s1_raw: &str, s2: &serde_json::Value, plugin_tag: &str) -> String {
    let s1: serde_json::Value = serde_json::from_str(s1_raw).unwrap_or(serde_json::json!({}));
    let new_url = response_formatter::find_urls(s2)
        .into_iter()
        .next()
        .unwrap_or_default();
    let mut out = s1_raw.to_string();
    if new_url.is_empty() {
        return out;
    }
    let old_url = response_formatter::find_urls(&s1)
        .into_iter()
        .next()
        .unwrap_or_default();
    let res_lit = serde_json::json!(
        cascade_json_str(plugin_tag, "/cascade/resolution").unwrap_or_else(|| "720p".into())
    )
    .to_string();
    let fps_lit = s2
        .pointer("/result/fps")
        .or_else(|| s2.get("fps"))
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().map(|u| u as i64))
                .or_else(|| v.as_f64().map(|f| f as i64))
        })
        .filter(|&f| f > 0)
        .unwrap_or(24)
        .to_string();
    let url_lit = serde_json::json!(new_url).to_string();
    if !old_url.is_empty() {
        response_formatter::json_replace_str(&mut out, &old_url, &new_url);
    }
    let mut field_replacements = vec![
        ("video_url", url_lit.as_str()),
        ("resolution", res_lit.as_str()),
        ("framespersecond", fps_lit.as_str()),
        ("fps", fps_lit.as_str()),
    ];
    let frame_lit =
        response_formatter::find_last_frame_url(s2).map(|f| serde_json::json!(f).to_string());
    if let Some(ref fl) = frame_lit {
        field_replacements.push(("last_frame_url", fl.as_str()));
    }
    response_formatter::json_replace_fields(&mut out, &field_replacements);
    out
}

/// 列表/仪表盘/终态落库：去掉 plugin_tag.cascade 中的密钥、上游渠道细节与内部握手 `s2_ready`。
/// 返回是否发生了字段删除（无变更则不改写字符串）。
pub(crate) fn cascade_scrub_plugin_tag_for_user(plugin_tag: &mut Option<String>) -> bool {
    let Some(raw) = plugin_tag.as_deref() else {
        return false;
    };
    if !raw.contains("\"cascade\"") {
        return false;
    }
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return false;
    };
    let Some(obj) = v.get_mut("cascade").and_then(|c| c.as_object_mut()) else {
        return false;
    };
    let mut changed = false;
    for key in ["api_key", "base_url", "ch_name", "ch_id", "mid", "s2_ready"] {
        if obj.remove(key).is_some() {
            changed = true;
        }
    }
    if obj.remove("crop").is_some() {
        changed = true;
    }
    if changed {
        *plugin_tag = Some(v.to_string());
    }
    changed
}

/// 用户日志级联脱敏（仅 response/post）：
/// 未完成→处理中；成功叠 S2 URL；失败不露 S1；非级联仅在有 cascade 时补 resolution
pub(crate) fn cascade_sanitize_for_user(
    response: &mut Option<String>,
    post_resp: &mut Option<String>,
    plugin_tag: Option<&str>,
    is_completed: bool,
    task_id: &str,
    log_model: &str,
    status_code: i64,
) {
    fn fold_post(raw: String) -> String {
        response_formatter::json_root_raw_value(&raw, "stage1")
            .map(str::to_string)
            .unwrap_or(raw)
    }
    fn take_map(slot: &mut Option<String>, f: impl FnOnce(String) -> String) {
        if let Some(raw) = slot.take() {
            *slot = Some(f(raw));
        }
    }

    let has_cascade = plugin_tag
        .map(|t| t.contains("\"cascade\""))
        .unwrap_or(false);
    let combined = |s: &str| response_formatter::json_root_raw_value(s, "stage1").is_some();
    let cascade_inflight = !is_completed
        && (has_cascade
            || post_resp.as_deref().is_some_and(combined)
            || response.as_deref().is_some_and(combined));

    if cascade_inflight {
        let s1_ack = [response.as_deref(), post_resp.as_deref()]
            .into_iter()
            .flatten()
            .map(cascade_s1_raw_from_log)
            .find(|s| s.contains('"'))
            .unwrap_or("{}");
        let tid_buf = task_id.is_empty().then(|| {
            response_formatter::json_root_raw_value(s1_ack, "id")
                .and_then(|v| serde_json::from_str::<String>(v).ok())
                .unwrap_or_default()
        });
        let tid = tid_buf.as_deref().unwrap_or(task_id);
        let mut user_resp = s1_ack.to_string();
        cascade_apply_processing_status(&mut user_resp, tid, false);
        cascade_overlay_client_identity(&mut user_resp, plugin_tag.unwrap_or(""), log_model);
        *response = Some(user_resp);
        take_map(post_resp, |raw| {
            let mut folded = fold_post(raw);
            response_formatter::force_json_task_id(&mut folded, tid);
            cascade_overlay_client_identity(&mut folded, plugin_tag.unwrap_or(""), log_model);
            folded
        });
        return;
    }

    if is_completed {
        take_map(response, |raw| {
            let mut out = if let (Some(s1), Some(s2_raw)) = (
                response_formatter::json_root_raw_value(&raw, "stage1"),
                response_formatter::json_root_raw_value(&raw, "stage2"),
            ) {
                let s2: serde_json::Value =
                    serde_json::from_str(s2_raw).unwrap_or(serde_json::json!({}));
                if status_code != 200 || response_formatter::find_urls(&s2).is_empty() {
                    let err = cascade_stage2_err_text(&s2, "增强失败");
                    serde_json::json!({
                        "id": task_id,
                        "status": "failed",
                        "error": { "message": err }
                    })
                    .to_string()
                } else {
                    let mut merged = cascade_s1_with_s2_url(s1, &s2, plugin_tag.unwrap_or(""));
                    cascade_overlay_client_identity(
                        &mut merged,
                        plugin_tag.unwrap_or(""),
                        log_model,
                    );
                    merged
                }
            } else if status_code != 200 && has_cascade {
                serde_json::json!({
                    "id": task_id,
                    "status": "failed",
                    "error": { "message": "增强失败" }
                })
                .to_string()
            } else if has_cascade {
                let mut out = raw;
                cascade_overlay_client_identity(&mut out, plugin_tag.unwrap_or(""), log_model);
                out
            } else {
                raw
            };
            if has_cascade {
                response_formatter::force_json_task_id(&mut out, task_id);
            }
            out
        });
    }
    take_map(post_resp, |raw| {
        let mut folded = fold_post(raw);
        if has_cascade {
            response_formatter::force_json_task_id(&mut folded, task_id);
            cascade_overlay_client_identity(&mut folded, plugin_tag.unwrap_or(""), log_model);
        }
        folded
    });
}

/// 官方路径：S1 已有字段才替换（不追加）。model=logs.model，resolution=级联目标
pub(crate) fn cascade_overlay_client_identity(s: &mut String, plugin_tag: &str, log_model: &str) {
    response_formatter::json_root_set(s, "model", &serde_json::json!(log_model).to_string());
    response_formatter::json_root_set(
        s,
        "resolution",
        &serde_json::json!(
            cascade_json_str(plugin_tag, "/cascade/resolution").unwrap_or_else(|| "720p".into())
        )
        .to_string(),
    );
}

/// 写入对外任务号，进行中统一为 in_progress/running；无成片则去掉 content/usage
fn cascade_apply_processing_status(s: &mut String, task_id: &str, openai_compatible: bool) {
    response_formatter::force_json_task_id(s, task_id);
    let st = response_formatter::json_root_raw_value(s, "status")
        .and_then(|raw| serde_json::from_str::<String>(raw).ok())
        .unwrap_or_default();
    let norm = normalize_task_status(&st);
    let processing = if openai_compatible {
        "in_progress"
    } else {
        "running"
    };
    if st.is_empty() || matches!(norm, "succeeded" | "failed") || norm == "pending" {
        response_formatter::json_root_set(s, "status", &serde_json::json!(processing).to_string());
    }
    response_formatter::json_root_remove(s, "content");
    response_formatter::json_root_remove(s, "usage");
    if openai_compatible {
        response_formatter::json_root_remove(s, "data");
    }
}

/// 级联处理中对外：OpenAI 只保留进度字段；官方路径覆盖 logs.model 与目标分辨率
pub(crate) fn cascade_s2_client_processing(
    raw_path: &str,
    category: &str,
    stage1_raw: &str,
    task_id: &str,
    plugin_tag: &str,
    log_model: &str,
) -> String {
    let openai = response_formatter::is_openai_compatible_path(raw_path);
    let mut s =
        response_formatter::apply_format(raw_path, category, stage1_raw, openai, Some(task_id));
    cascade_apply_processing_status(&mut s, task_id, openai);
    if !openai {
        cascade_overlay_client_identity(&mut s, plugin_tag, log_model);
    }
    s
}

/// 级联落库：stage1 + stage2 原始串直接拼进 combined，不经 Value 重排
pub(crate) fn cascade_combine_stages(s1_raw: &str, s2_raw: &str) -> String {
    let s1 = if s1_raw.trim().is_empty() {
        "{}"
    } else {
        s1_raw
    };
    let s2 = match serde_json::from_str::<serde_json::Value>(s2_raw) {
        Ok(_) => s2_raw.to_string(),
        Err(_) => serde_json::json!(s2_raw).to_string(),
    };
    format!(r#"{{"stage1":{s1},"stage2":{s2}}}"#)
}

/// S2 时从 response_content 取 stage1 原文（无 stage1 或空对象则整包即扁平 S1）
pub(crate) fn cascade_s1_raw_from_log(content: &str) -> &str {
    if content.is_empty() {
        return "{}";
    }
    match response_formatter::json_root_raw_value(content, "stage1") {
        Some(s1) if s1.trim_start().starts_with('{') && s1.contains('"') => s1,
        _ => content,
    }
}

/// plugin_tag 含 cascade（S1 扁平体时靠此识别）
#[inline]
pub(crate) fn cascade_plugin_tag_present(plugin_tag: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(plugin_tag)
        .ok()
        .is_some_and(|v| v.get("cascade").is_some())
}

/// 从级联 stage2 节点提取失败文案（字符串 / 错误体 / 兜底）
pub(crate) fn cascade_stage2_err_text(stage2: &serde_json::Value, fallback: &str) -> String {
    let raw = stage2
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            response_formatter::extract_error_message_from_value(stage2).filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| fallback.to_string());
    proxy::sanitize_error_message(&raw)
}

/// 上游轮询目标（手动 GET / 后台轮询共用）
pub(crate) struct CascadePollTarget<'a> {
    pub channel: std::borrow::Cow<'a, Channel>,
    pub resolved: std::borrow::Cow<'a, forward::ResolvedForward>,
    pub task_id: std::borrow::Cow<'a, str>,
    pub model: std::borrow::Cow<'a, str>,
}

impl CascadePollTarget<'_> {
    fn to_owned(&self) -> CascadePollTarget<'static> {
        CascadePollTarget {
            channel: std::borrow::Cow::Owned(self.channel.clone().into_owned()),
            resolved: std::borrow::Cow::Owned(self.resolved.clone().into_owned()),
            task_id: std::borrow::Cow::Owned(self.task_id.clone().into_owned()),
            model: std::borrow::Cow::Owned(self.model.clone().into_owned()),
        }
    }
}

/// 阶段二成功对外体：S1 骨架换 S2 URL → apply_format → 固定用户侧 task_id
pub(crate) fn cascade_format_s2_succeeded(
    raw_path: &str,
    category: &str,
    plugin_tag: &str,
    s1_raw: &str,
    s2: &serde_json::Value,
    task_id: &str,
    log_model: &str,
) -> String {
    let new_stage1 = cascade_s1_with_s2_url(s1_raw, s2, plugin_tag);
    let mut formatted =
        response_formatter::apply_format(raw_path, category, &new_stage1, true, Some(task_id));
    response_formatter::force_json_task_id(&mut formatted, task_id);
    if !response_formatter::is_openai_compatible_path(raw_path) {
        cascade_overlay_client_identity(&mut formatted, plugin_tag, log_model);
    }
    formatted
}

/// 级联终态对外格式化：有成片 URL 则替换原 stage1；否则返回失败 JSON
pub(crate) fn cascade_format_completed_resp(
    raw_path: &str,
    category: &str,
    combined_body: &str,
    plugin_tag: &str,
    task_id: &str,
    log_model: &str,
    fallback_err: &str,
) -> String {
    let s1_raw = response_formatter::json_root_raw_value(combined_body, "stage1").unwrap_or("{}");
    let s2: serde_json::Value = response_formatter::json_root_raw_value(combined_body, "stage2")
        .and_then(|x| serde_json::from_str(x).ok())
        .unwrap_or(serde_json::json!({}));
    if !response_formatter::find_urls(&s2).is_empty() {
        cascade_format_s2_succeeded(
            raw_path, category, plugin_tag, s1_raw, &s2, task_id, log_model,
        )
    } else {
        let err = cascade_stage2_err_text(&s2, fallback_err);
        response_formatter::format_async_task_failed(raw_path, category, task_id, &err)
    }
}

/// 级联轮询决策与目标（供 GET 状态查询与后台 Poller 一体化调用）
pub(crate) struct CascadePollPlan<'a> {
    pub stage: u8,
    pub post_resp_json: serde_json::Value,
    pub poll: CascadePollTarget<'a>,
}

/// 统一解析 post_response、计算级联阶段并组装轮询目标
pub(crate) fn cascade_prepare_poll<'a>(
    is_cascade: bool,
    post_response: &str,
    channel: &'a Channel,
    resolved: &'a forward::ResolvedForward,
    plugin_tag: &str,
    task_id: &'a str,
    poll_model: &'a str,
) -> Result<CascadePollPlan<'a>, (String, u16)> {
    // 级联基因守卫：任务历史中必须持有级联上下文（plugin_tag 含 cascade），否则物理拦截降为单阶段，坚决杜绝误判
    let is_cascade = is_cascade && cascade_plugin_tag_present(plugin_tag);
    let post_resp_json: serde_json::Value = if is_cascade {
        serde_json::from_str(post_response).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    let stage = if !is_cascade {
        0
    } else if post_resp_json.get("stage2").is_some() {
        2
    } else {
        1
    };

    let poll = if stage == 2 {
        let stage2_val = &post_resp_json["stage2"];
        let s2_id = response_formatter::find_id(stage2_val);
        if s2_id.is_empty() {
            let msg = cascade_stage2_err_text(stage2_val, "S2 无任务 ID");
            let status = match stage2_val {
                serde_json::Value::String(s) if !s.trim().is_empty() => {
                    proxy::infer_error_status_code_from_str(s)
                }
                serde_json::Value::Object(m) if !m.is_empty() => {
                    proxy::infer_error_status_code(stage2_val)
                }
                serde_json::Value::Array(a) if !a.is_empty() => {
                    proxy::infer_error_status_code(stage2_val)
                }
                _ => 500,
            };
            return Err((msg, status));
        }
        if cascade_engine(plugin_tag) == "tencent" {
            CascadePollTarget {
                channel: std::borrow::Cow::Borrowed(channel),
                resolved: std::borrow::Cow::Borrowed(resolved),
                task_id: std::borrow::Cow::Owned(s2_id),
                model: std::borrow::Cow::Borrowed(poll_model),
            }
        } else {
            #[cfg(not(feature = "plugin_volcengine_enhance"))]
            {
                let _ = (channel, resolved, poll_model);
                return Err(("增强插件未编译".into(), 400));
            }
            #[cfg(feature = "plugin_volcengine_enhance")]
            {
                let cascade_info = serde_json::from_str::<serde_json::Value>(plugin_tag)
                    .ok()
                    .and_then(|v| v.get("cascade").cloned())
                    .unwrap_or(serde_json::json!({}));
                let (ch, mut res, model) =
                    crate::api::plugins::volc_s2_from_tag(channel, &cascade_info);
                res.res_mul.clone_from(&resolved.res_mul);
                CascadePollTarget {
                    channel: std::borrow::Cow::Owned(ch),
                    resolved: std::borrow::Cow::Owned(res),
                    task_id: std::borrow::Cow::Owned(s2_id),
                    model: std::borrow::Cow::Owned(model),
                }
            }
        }
    } else {
        let poll_task_id = cascade_json_ptr(plugin_tag, "/cascade/s1_task_id", false)
            .or_else(|| cascade_json_ptr(plugin_tag, "/upstream_task", false))
            .map(std::borrow::Cow::Owned)
            .unwrap_or(std::borrow::Cow::Borrowed(task_id));
        CascadePollTarget {
            channel: std::borrow::Cow::Borrowed(channel),
            resolved: std::borrow::Cow::Borrowed(resolved),
            task_id: poll_task_id,
            model: std::borrow::Cow::Borrowed(poll_model),
        }
    };

    Ok(CascadePollPlan {
        stage,
        post_resp_json,
        poll,
    })
}

/// S2 失败落库（GET/后台共用）；仅未结案可写。
pub(crate) async fn cascade_persist_s2_fail(
    state: &AppState,
    log_id: i64,
    post_resp_json: &serde_json::Value,
    s1_raw: &str,
    store_body: &str,
    err_text: &str,
    log_prefix: &str,
) {
    crate::relay_debug!("[{}] S2失败 log_id={} err={}", log_prefix, log_id, err_text);
    let updated = serde_json::json!({
        "stage1": post_resp_json["stage1"],
        "stage2": err_text
    })
    .to_string();
    let resp_content = cascade_combine_stages(s1_raw, store_body);
    let _ = sqlx::query(&state.db.format_query(
        "UPDATE logs SET response_content = ?, error_message = ?, post_response = ? \
         WHERE id = ? AND is_completed = 0",
    ))
    .bind(&resp_content)
    .bind(err_text)
    .bind(&updated)
    .bind(log_id)
    .execute(&state.db.pool)
    .await;
}

/// 终态清理 plugin_tag 中的密钥等敏感字段并落库（若无变更则不执行 update）
pub(crate) async fn cascade_scrub_and_update_log_tag(
    state: &AppState,
    log_id: i64,
    plugin_tag: &str,
) {
    let mut tag = Some(plugin_tag.to_string());
    if cascade_scrub_plugin_tag_for_user(&mut tag) {
        if let Some(t) = tag {
            let _ = sqlx::query(
                &state
                    .db
                    .format_query("UPDATE logs SET plugin_tag = ? WHERE id = ?"),
            )
            .bind(&t)
            .bind(log_id)
            .execute(&state.db.pool)
            .await;
        }
    }
}

/// 进程内互斥：占位成功则持有，Drop 时自动 remove（S2 提交 / 终态收敛结算共用）
pub(crate) struct CascadeS2InflightGuard<'a> {
    map: &'a dashmap::DashMap<i64, ()>,
    id: i64,
}

impl<'a> CascadeS2InflightGuard<'a> {
    pub(crate) fn try_acquire(map: &'a dashmap::DashMap<i64, ()>, id: i64) -> Option<Self> {
        if map.insert(id, ()).is_some() {
            return None;
        }
        Some(Self { map, id })
    }
}

impl Drop for CascadeS2InflightGuard<'_> {
    fn drop(&mut self) {
        self.map.remove(&self.id);
    }
}

/// S2 提交入参（手动 GET / 后台共用，避免 10+ 散参）
pub(crate) struct CascadeS2SubmitCtx {
    pub task_id: String,
    pub log_id: i64,
    pub post_response: String,
    pub request_content: String,
    pub upstream_req: String,
    pub channel: Channel,
    pub base_video_url: String,
    pub plugin_tag: String,
    pub stage1_response: String,
    /// 转发规则配置（含 crop_480p, crop_timing 等）
    pub resolved: forward::ResolvedForward,
}

/// S2 提交：立刻返回，前裁+提交在后台做。失败只落库，退费由下次 prepare_poll / settle。
pub(crate) async fn cascade_stage2_submit(state: &Arc<AppState>, ctx: CascadeS2SubmitCtx) {
    if state.cascade_s2_inflight.contains_key(&ctx.log_id) {
        crate::relay_debug!("[Cascade S2] 跳过 log_id={}（忙）", ctx.log_id);
        return;
    }
    let state = Arc::clone(state);
    tokio::spawn(async move {
        cascade_stage2_submit_run(&state, &ctx).await;
    });
}

async fn cascade_stage2_submit_run(state: &Arc<AppState>, ctx: &CascadeS2SubmitCtx) {
    let Some(_guard) = CascadeS2InflightGuard::try_acquire(&state.cascade_s2_inflight, ctx.log_id)
    else {
        crate::relay_debug!("[Cascade S2] 跳过 log_id={}（忙）", ctx.log_id);
        return;
    };

    let post_resp: serde_json::Value =
        serde_json::from_str(&ctx.post_response).unwrap_or(serde_json::json!({}));

    let mut updated_tag_opt: Option<String> = None;
    let cascade_snap = if ctx.plugin_tag.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str::<serde_json::Value>(&ctx.plugin_tag) {
            Ok(mut pt) => {
                let snap = pt.get("cascade").cloned().unwrap_or(serde_json::json!({}));
                if let Some(cascade) = pt.get_mut("cascade").and_then(|v| v.as_object_mut()) {
                    if cascade.remove("api_key").is_some() {
                        updated_tag_opt = Some(pt.to_string());
                    }
                }
                snap
            }
            Err(_) => serde_json::json!({}),
        }
    };
    // S1 轮询成功体：根 id 换成用户侧 cgt 再落库（后续 combine/展示同源）
    let mut s1_body = ctx.stage1_response.clone();
    response_formatter::force_json_task_id(&mut s1_body, &ctx.task_id);
    let s1_json: serde_json::Value =
        serde_json::from_str(&s1_body).unwrap_or(serde_json::json!({}));

    let write_error = |state: &Arc<AppState>,
                       err_msg: &str,
                       post_resp_json: &serde_json::Value,
                       s1_raw: &str,
                       s2_raw: &str,
                       tag: &Option<String>,
                       upstream: Option<String>| {
        let state = state.clone();
        let err = err_msg.to_string();
        let updated = serde_json::json!({"stage1": post_resp_json, "stage2": s2_raw}).to_string();
        let resp_content = cascade_combine_stages(s1_raw, s2_raw);
        let tag = tag.clone();
        let db_id = ctx.log_id;
        async move {
            let _ = sqlx::query(&state.db.format_query(
                "UPDATE logs SET post_response = ?, response_content = ?, error_message = ?, plugin_tag = COALESCE(?, plugin_tag), upstream_req_content = COALESCE(?, upstream_req_content) WHERE id = ?"
            )).bind(&updated).bind(&resp_content).bind(&err).bind(&tag).bind(&upstream).bind(db_id).execute(&state.db.pool).await;
        }
    };

    if ctx.base_video_url.is_empty() {
        // 底座无直链：非上游 HTTP 体，无法推断 → 500
        let err_msg = "底座视频生成成功但未能获取到视频直链地址";
        write_error(
            state,
            err_msg,
            &post_resp,
            &s1_body,
            err_msg,
            &updated_tag_opt,
            None,
        )
        .await;
        return;
    }

    let engine = cascade_engine(&ctx.plugin_tag);
    let target_resolution = cascade_snap
        .get("resolution")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .or_else(|| cascade_json_str(&ctx.request_content, "/resolution"))
        .unwrap_or_else(|| "720p".into());

    let req_hint: serde_json::Value =
        serde_json::from_str(&ctx.request_content).unwrap_or(serde_json::json!({}));
    let up_hint: serde_json::Value =
        serde_json::from_str(&ctx.upstream_req).unwrap_or(serde_json::json!({}));
    let ratio_opt = cascade_json_root_str(&s1_json, "ratio")
        .or_else(|| cascade_json_root_str(&up_hint, "ratio"))
        .or_else(|| cascade_json_root_str(&req_hint, "ratio"));
    #[cfg_attr(
        not(any(
            feature = "plugin_tencent_enhance",
            feature = "plugin_volcengine_enhance"
        )),
        allow(unused_variables)
    )]
    let target_limit = resolve_cascade_res_limit(&ctx.resolved.res_limit, &target_resolution, ratio_opt)
        .or_else(|| {
            cascade_snap
                .get("res_limit")
                .and_then(|v| v.as_i64())
                .filter(|&l| (128..=4320).contains(&l))
        });

    if engine == "tencent" {
        #[cfg(not(feature = "plugin_tencent_enhance"))]
        {
            let err_msg = "增强插件未编译";
            write_error(
                state,
                err_msg,
                &post_resp,
                &s1_body,
                err_msg,
                &updated_tag_opt,
                None,
            )
            .await;
            return;
        }

        #[cfg(feature = "plugin_tencent_enhance")]
        {
            let video_url = cascade_s2_maybe_pre_crop(
                state,
                ctx,
                &s1_json,
                &target_resolution,
                &engine,
                &ctx.channel,
                "",
            )
            .await;
            match crate::api::plugins::submit_tencent_cascade_stage2(
                state,
                ctx.log_id,
                &video_url,
                &target_resolution,
                target_limit,
            )
            .await
            {
                Ok((stage2_id, post_json, tencent_payload)) => {
                    let updated =
                        serde_json::json!({"stage1": post_resp, "stage2": post_json}).to_string();
                    let upstream_combined =
                        cascade_upstream_req_combined(&ctx.upstream_req, &tencent_payload);
                    let _ = sqlx::query(&state.db.format_query(
                        "UPDATE logs SET post_response = ?, response_content = ?, plugin_tag = COALESCE(?, plugin_tag), upstream_req_content = COALESCE(?, upstream_req_content) WHERE id = ?",
                    ))
                    .bind(&updated)
                    .bind(&s1_body)
                    .bind(&updated_tag_opt)
                    .bind(&upstream_combined)
                    .bind(ctx.log_id)
                    .execute(&state.db.pool)
                    .await;
                    crate::relay_debug!(
                        "[Cascade S2] tencent 提交成功 日志ID={} 阶段1={} 阶段2={} 分辨率={}",
                        ctx.log_id,
                        ctx.task_id,
                        stage2_id,
                        target_resolution
                    );
                    return;
                }
                Err((err_msg, _)) => {
                    write_error(
                        state,
                        &err_msg,
                        &post_resp,
                        &s1_body,
                        &err_msg,
                        &updated_tag_opt,
                        None,
                    )
                    .await;
                    return;
                }
            }
        }
    }

    #[cfg(not(feature = "plugin_volcengine_enhance"))]
    {
        let err_msg = "增强插件未编译";
        write_error(
            state,
            err_msg,
            &post_resp,
            &s1_body,
            err_msg,
            &updated_tag_opt,
            None,
        )
        .await;
        return;
    }
    #[cfg(feature = "plugin_volcengine_enhance")]
    {
        let (enhance_ch, mut volc_resolved, final_model) =
            crate::api::plugins::volc_s2_from_tag(&ctx.channel, &cascade_snap);
        if let Some(limit) = target_limit {
            volc_resolved.res_limit.insert(target_resolution.trim().to_ascii_lowercase(), limit);
        }
        let volc_model_mid = volc_resolved.mid.clone().unwrap_or_default();

        let base_video_url = cascade_s2_maybe_pre_crop(
            state,
            ctx,
            &s1_json,
            &target_resolution,
            &engine,
            &enhance_ch,
            volc_resolved.auth_type.as_str(),
        )
        .await;

        let scene = cascade_snap
            .get("scene")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());
        match crate::api::plugins::submit_volc_cascade_stage2(
            state,
            &enhance_ch,
            &volc_resolved,
            &final_model,
            &base_video_url,
            &target_resolution,
            target_limit,
            &volc_model_mid,
            scene.as_deref(),
            ctx.log_id,
        )
        .await
        {
            Ok((stage2_id, post_json, volc_payload)) => {
                let updated =
                    serde_json::json!({"stage1": post_resp, "stage2": post_json}).to_string();
                let upstream_combined =
                    cascade_upstream_req_combined(&ctx.upstream_req, &volc_payload);
                let _ = sqlx::query(&state.db.format_query("UPDATE logs SET post_response = ?, response_content = ?, upstream_req_content = COALESCE(?, upstream_req_content) WHERE id = ?"))
                    .bind(&updated).bind(&s1_body).bind(&upstream_combined).bind(ctx.log_id).execute(&state.db.pool).await;

                crate::relay_debug!(
                    "[Cascade S2] 级联提交成功 日志ID={} 阶段1={} 阶段2={} MID={} 分辨率={} 渠道={}",
                    ctx.log_id,
                    ctx.task_id,
                    stage2_id,
                    volc_model_mid,
                    target_resolution,
                    enhance_ch.name
                );
            }
            Err((err_msg, _, raw_text, volc_payload)) => {
                write_error(
                    state,
                    &err_msg,
                    &post_resp,
                    &s1_body,
                    &raw_text,
                    &updated_tag_opt,
                    cascade_upstream_req_combined(&ctx.upstream_req, &volc_payload),
                )
                .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_cascade_res_limit() {
        let mut map = HashMap::new();
        map.insert("720p".to_string(), 720);
        map.insert("720p@1:1".to_string(), 960);
        map.insert("720p@3:4".to_string(), 834);
        map.insert("1080p@21:9".to_string(), 946);
        map.insert("invalid@1:1".to_string(), 50);

        // 优先命中比例
        assert_eq!(resolve_cascade_res_limit(&map, "720p", Some("1:1")), Some(960));
        assert_eq!(resolve_cascade_res_limit(&map, "720p", Some("3:4")), Some(834));
        assert_eq!(resolve_cascade_res_limit(&map, "1080p", Some("21:9")), Some(946));

        // 比例未配置时回退到全局
        assert_eq!(resolve_cascade_res_limit(&map, "720p", Some("16:9")), Some(720));
        assert_eq!(resolve_cascade_res_limit(&map, "720p", None), Some(720));

        // 未配置或非法范围返回 None
        assert_eq!(resolve_cascade_res_limit(&map, "4k", Some("1:1")), None);
        assert_eq!(resolve_cascade_res_limit(&map, "invalid", Some("1:1")), None);
    }
}
