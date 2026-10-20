/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    models::{Announcement, CreateAnnouncementReq, UpdateAnnouncementReq},
    time_system::DbTs,
    AppState,
};

const DEFAULT_FLAG_EXPIRE_DAYS: i64 = 5;
const PERMANENT_EXPIRE: &str = "permanent";
const DISPLAY_TIME_CREATED: &str = "created";
const DISPLAY_TIME_UPDATED: &str = "updated";
const MODULE_PAY_MODAL: &str = "pay_modal";
const MODULE_CONSOLE: &str = "console";
const MODULE_PLAYGROUND: &str = "playground_2026";
const MODULE_PORTAL: &str = "portal_pro";

#[derive(Debug, Deserialize)]
pub struct PublicAnnouncementQuery {
    pub module: Option<String>,
}

fn normalize_public_module(raw: Option<&str>) -> AppResult<String> {
    let candidate = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(MODULE_CONSOLE);
    match candidate {
        MODULE_PAY_MODAL | MODULE_CONSOLE | MODULE_PLAYGROUND | MODULE_PORTAL => {
            Ok(candidate.to_string())
        }
        _ => Err(AppError::BadRequest(
            "通知模块无效，请选择支付弹窗、控制台、创作中心或门户".to_string(),
        )),
    }
}

fn normalize_target_modules(raw: Option<&str>) -> AppResult<String> {
    let text = raw.map(str::trim).filter(|s| !s.is_empty());
    let Some(text) = text else {
        return Ok(MODULE_CONSOLE.to_string());
    };
    let mut pay_modal = false;
    let mut console = false;
    let mut playground = false;
    let mut portal = false;
    for part in text.split(',') {
        match part.trim() {
            "" => {}
            MODULE_PAY_MODAL => pay_modal = true,
            MODULE_CONSOLE => console = true,
            MODULE_PLAYGROUND => playground = true,
            MODULE_PORTAL => portal = true,
            _ => {
                return Err(AppError::BadRequest(
                    "通知模块无效，请选择支付弹窗、控制台、创作中心或门户".to_string(),
                ))
            }
        }
    }
    let mut out = Vec::new();
    if pay_modal {
        out.push(MODULE_PAY_MODAL);
    }
    if console {
        out.push(MODULE_CONSOLE);
    }
    if playground {
        out.push(MODULE_PLAYGROUND);
    }
    if portal {
        out.push(MODULE_PORTAL);
    }
    if out.is_empty() {
        return Err(AppError::BadRequest("请至少选择一个通知模块".to_string()));
    }
    Ok(out.join(","))
}

fn parse_banner_bg(raw: &str) -> AppResult<Option<String>> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let hex = s.strip_prefix('#').unwrap_or(s);
    if !matches!(hex.len(), 3 | 6 | 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest(
            "背景颜色格式无效，请使用 #RRGGBB".to_string(),
        ));
    }
    let hex = if hex.len() == 8 { &hex[..6] } else { hex };
    let full = if hex.len() == 3 {
        hex.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        hex.to_string()
    };
    Ok(Some(format!("#{}", full.to_ascii_lowercase())))
}

fn module_plugin_name(module: &str) -> Option<&'static str> {
    match module {
        MODULE_PLAYGROUND => Some("playground_2026"),
        MODULE_PORTAL => Some("site_portal_pro"),
        _ => None,
    }
}

async fn is_plugin_enabled(state: &Arc<AppState>, name: &str) -> AppResult<bool> {
    if !crate::api::plugins::is_plugin_compiled(name) {
        return Ok(false);
    }
    let enabled: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_enabled FROM plugins WHERE name = ?"),
    )
    .bind(name)
    .fetch_optional(&state.db.pool)
    .await?;
    Ok(enabled == Some(1))
}

fn module_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

async fn ensure_added_modules(state: &Arc<AppState>, next: &str, previous: &str) -> AppResult<()> {
    let prev = module_list(previous);
    for module in module_list(next) {
        if prev.iter().any(|item| item == &module) {
            continue;
        }
        ensure_module_selectable(state, &module).await?;
    }
    Ok(())
}

async fn ensure_module_selectable(state: &Arc<AppState>, module: &str) -> AppResult<()> {
    let Some(plugin) = module_plugin_name(module) else {
        return Ok(());
    };
    if is_plugin_enabled(state, plugin).await? {
        return Ok(());
    }
    Err(AppError::BadRequest(match module {
        MODULE_PLAYGROUND => "创作中心2026插件未开启，无法选择创作中心通知".to_string(),
        _ => "站点门户增强版插件未开启，无法选择门户通知".to_string(),
    }))
}

async fn module_is_publicly_available(state: &Arc<AppState>, module: &str) -> AppResult<bool> {
    match module_plugin_name(module) {
        Some(plugin) => is_plugin_enabled(state, plugin).await,
        None => Ok(true),
    }
}

fn normalize_display_time_mode(raw: Option<&str>, fallback: &str) -> AppResult<String> {
    let candidate = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback);
    let candidate = if candidate.is_empty() {
        DISPLAY_TIME_CREATED
    } else {
        candidate
    };
    match candidate {
        DISPLAY_TIME_CREATED | DISPLAY_TIME_UPDATED => Ok(candidate.to_string()),
        _ => Err(AppError::BadRequest(
            "发布时间选项无效，请选择新建时间或最后修改时间".to_string(),
        )),
    }
}

fn should_touch_updated_at(payload: &UpdateAnnouncementReq) -> bool {
    payload.title.is_some()
        || payload.content.is_some()
        || payload.is_pinned.is_some()
        || payload.is_popup.is_some()
        || payload.is_active.is_some()
        || payload.pin_expires_at.is_some()
        || payload.popup_expires_at.is_some()
        || payload.active_expires_at.is_some()
        || payload.display_time_mode.is_some()
        || payload.target_module.is_some()
        || payload.playground_banner_bg.is_some()
        || payload.portal_banner_bg.is_some()
}

fn default_flag_expires_at(now: DateTime<Utc>) -> DbTs {
    DbTs::from_utc(now + Duration::days(DEFAULT_FLAG_EXPIRE_DAYS))
}

fn is_permanent_expire(raw: &str) -> bool {
    let t = raw.trim();
    t.is_empty() || t.eq_ignore_ascii_case(PERMANENT_EXPIRE)
}

fn parse_flag_expires_at(raw: Option<&str>) -> Result<Option<DbTs>, String> {
    let Some(s) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    if is_permanent_expire(s) {
        return Ok(None);
    }
    let normalized = if s.len() == 10 {
        format!("{s}T23:59:59.000Z")
    } else {
        s.to_string()
    };
    let ts = DbTs::new(normalized);
    if ts.to_utc().is_none() {
        return Err("无效的有效期日期".to_string());
    }
    Ok(Some(ts))
}

fn flag_should_turn_off(is_on: i64, expires_at: Option<&DbTs>, now: DateTime<Utc>) -> bool {
    is_on == 1
        && expires_at
            .and_then(|t| t.to_utc())
            .is_some_and(|t| t <= now)
}

fn apply_flag_expiry(is_on: i64, expires_at: Option<&DbTs>, now: DateTime<Utc>) -> i64 {
    if flag_should_turn_off(is_on, expires_at, now) {
        0
    } else {
        is_on
    }
}

fn resolve_create_expires(
    is_on: i64,
    raw: Option<&str>,
    now: DateTime<Utc>,
) -> AppResult<Option<DbTs>> {
    match raw {
        Some(s) if is_permanent_expire(s) => Ok(None),
        Some(s) => parse_flag_expires_at(Some(s)).map_err(AppError::BadRequest),
        None if is_on == 1 => Ok(Some(default_flag_expires_at(now))),
        None => Ok(None),
    }
}

fn resolve_update_expires(
    is_on: i64,
    raw: Option<&str>,
    current: Option<DbTs>,
    now: DateTime<Utc>,
) -> AppResult<Option<DbTs>> {
    match raw {
        Some(s) if is_permanent_expire(s) => Ok(None),
        Some(s) => parse_flag_expires_at(Some(s)).map_err(AppError::BadRequest),
        None => {
            if current.is_some() {
                Ok(current)
            } else if is_on == 1 {
                Ok(Some(default_flag_expires_at(now)))
            } else {
                Ok(None)
            }
        }
    }
}

pub(crate) async fn apply_announcement_expiries(state: &Arc<AppState>) -> AppResult<()> {
    sqlx::query(
        "UPDATE announcements SET
            is_pinned = CASE
                WHEN is_pinned = 1 AND pin_expires_at IS NOT NULL AND pin_expires_at <= NOW() THEN 0
                ELSE is_pinned
            END,
            is_popup = CASE
                WHEN is_popup = 1 AND popup_expires_at IS NOT NULL AND popup_expires_at <= NOW() THEN 0
                ELSE is_popup
            END,
            is_active = CASE
                WHEN is_active = 1 AND active_expires_at IS NOT NULL AND active_expires_at <= NOW() THEN 0
                ELSE is_active
            END
         WHERE (is_pinned = 1 AND pin_expires_at IS NOT NULL AND pin_expires_at <= NOW())
            OR (is_popup = 1 AND popup_expires_at IS NOT NULL AND popup_expires_at <= NOW())
            OR (is_active = 1 AND active_expires_at IS NOT NULL AND active_expires_at <= NOW())",
    )
    .execute(&state.db.pool)
    .await?;
    Ok(())
}

pub async fn list_admin_announcements(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    apply_announcement_expiries(&state).await?;
    let announcements: Vec<Announcement> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM announcements ORDER BY sort_order DESC, id DESC"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    Ok(Json(json!({
        "success": true,
        "data": announcements
    })))
}

pub(crate) async fn load_public_announcements(
    state: &Arc<AppState>,
    module: &str,
) -> AppResult<Vec<Announcement>> {
    let module = normalize_public_module(Some(module))?;
    apply_announcement_expiries(state).await?;
    if !module_is_publicly_available(state, &module).await? {
        return Ok(Vec::new());
    }
    let announcements: Vec<Announcement> = sqlx::query_as(&state.db.format_query(
        "SELECT * FROM announcements WHERE is_active = 1 AND strpos(',' || target_module || ',', ',' || ? || ',') > 0 ORDER BY is_pinned DESC, sort_order DESC, id DESC LIMIT 10",
    ))
    .bind(&module)
    .fetch_all(&state.db.pool)
    .await?;
    Ok(announcements)
}

pub async fn get_public_announcements(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PublicAnnouncementQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let module = normalize_public_module(query.module.as_deref())?;
    let announcements = load_public_announcements(&state, &module).await?;

    Ok(Json(json!({
        "success": true,
        "data": announcements
    })))
}

pub async fn create_announcement(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateAnnouncementReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let now_dt = Utc::now();
    let now = DbTs::from_utc(now_dt);
    let is_popup = payload.is_popup.unwrap_or(0);
    let sort_order = payload.sort_order.unwrap_or(0);
    let pin_expires_at =
        resolve_create_expires(payload.is_pinned, payload.pin_expires_at.as_deref(), now_dt)?;
    let popup_expires_at =
        resolve_create_expires(is_popup, payload.popup_expires_at.as_deref(), now_dt)?;
    let active_expires_at = resolve_create_expires(
        payload.is_active,
        payload.active_expires_at.as_deref(),
        now_dt,
    )?;
    let is_pinned = apply_flag_expiry(payload.is_pinned, pin_expires_at.as_ref(), now_dt);
    let is_popup = apply_flag_expiry(is_popup, popup_expires_at.as_ref(), now_dt);
    let is_active = apply_flag_expiry(payload.is_active, active_expires_at.as_ref(), now_dt);
    let display_time_mode =
        normalize_display_time_mode(payload.display_time_mode.as_deref(), DISPLAY_TIME_CREATED)?;
    let target_module = normalize_target_modules(payload.target_module.as_deref())?;
    ensure_added_modules(&state, &target_module, "").await?;
    let playground_banner_bg =
        parse_banner_bg(payload.playground_banner_bg.as_deref().unwrap_or(""))?;
    let portal_banner_bg = parse_banner_bg(payload.portal_banner_bg.as_deref().unwrap_or(""))?;

    let announcement: Announcement = sqlx::query_as(&state.db.format_query(
        "INSERT INTO announcements (title, content, is_pinned, is_popup, is_active, sort_order, pin_expires_at, popup_expires_at, active_expires_at, display_time_mode, target_module, playground_banner_bg, portal_banner_bg, created_at, updated_at) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
    ))
    .bind(&payload.title)
    .bind(&payload.content)
    .bind(is_pinned)
    .bind(is_popup)
    .bind(is_active)
    .bind(sort_order)
    .bind(&pin_expires_at)
    .bind(&popup_expires_at)
    .bind(&active_expires_at)
    .bind(&display_time_mode)
    .bind(&target_module)
    .bind(&playground_banner_bg)
    .bind(&portal_banner_bg)
    .bind(&now)
    .bind(&now)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(json!({
        "success": true,
        "message": "通知创建成功",
        "data": announcement
    })))
}

pub async fn update_announcement(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateAnnouncementReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let current: Announcement = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM announcements WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("通知不存在".to_string()))?;

    let now_dt = Utc::now();
    let touch_updated = should_touch_updated_at(&payload);
    let display_time_mode = normalize_display_time_mode(
        payload.display_time_mode.as_deref(),
        &current.display_time_mode,
    )?;
    let target_module = normalize_target_modules(
        payload
            .target_module
            .as_deref()
            .or(Some(current.target_module.as_str())),
    )?;
    ensure_added_modules(&state, &target_module, &current.target_module).await?;
    let playground_banner_bg = match payload.playground_banner_bg.as_deref() {
        Some(raw) => parse_banner_bg(raw)?,
        None => current.playground_banner_bg.clone(),
    };
    let portal_banner_bg = match payload.portal_banner_bg.as_deref() {
        Some(raw) => parse_banner_bg(raw)?,
        None => current.portal_banner_bg.clone(),
    };
    let title = payload.title.unwrap_or(current.title);
    let content = payload.content.unwrap_or(current.content);
    let is_pinned = payload.is_pinned.unwrap_or(current.is_pinned);
    let is_popup = payload.is_popup.unwrap_or(current.is_popup);
    let is_active = payload.is_active.unwrap_or(current.is_active);
    let sort_order = payload.sort_order.unwrap_or(current.sort_order);
    let pin_expires_at = resolve_update_expires(
        is_pinned,
        payload.pin_expires_at.as_deref(),
        current.pin_expires_at,
        now_dt,
    )?;
    let popup_expires_at = resolve_update_expires(
        is_popup,
        payload.popup_expires_at.as_deref(),
        current.popup_expires_at,
        now_dt,
    )?;
    let active_expires_at = resolve_update_expires(
        is_active,
        payload.active_expires_at.as_deref(),
        current.active_expires_at,
        now_dt,
    )?;
    let is_pinned = apply_flag_expiry(is_pinned, pin_expires_at.as_ref(), now_dt);
    let is_popup = apply_flag_expiry(is_popup, popup_expires_at.as_ref(), now_dt);
    let is_active = apply_flag_expiry(is_active, active_expires_at.as_ref(), now_dt);
    let now = if touch_updated {
        DbTs::from_utc(now_dt)
    } else {
        current.updated_at
    };

    let updated: Announcement = sqlx::query_as(
        &state.db.format_query(
            "UPDATE announcements SET title = ?, content = ?, is_pinned = ?, is_popup = ?, is_active = ?, sort_order = ?, pin_expires_at = ?, popup_expires_at = ?, active_expires_at = ?, display_time_mode = ?, target_module = ?, playground_banner_bg = ?, portal_banner_bg = ?, updated_at = ? WHERE id = ? RETURNING *"
        )
    )
    .bind(&title)
    .bind(&content)
    .bind(is_pinned)
    .bind(is_popup)
    .bind(is_active)
    .bind(sort_order)
    .bind(&pin_expires_at)
    .bind(&popup_expires_at)
    .bind(&active_expires_at)
    .bind(&display_time_mode)
    .bind(&target_module)
    .bind(&playground_banner_bg)
    .bind(&portal_banner_bg)
    .bind(&now)
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(json!({
        "success": true,
        "message": "通知更新成功",
        "data": updated
    })))
}

pub async fn delete_announcement(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM announcements WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    Ok(Json(json!({
        "success": true,
        "message": "通知删除成功"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_modules_can_combine_and_reject_unknown() {
        assert_eq!(normalize_target_modules(None).unwrap(), MODULE_CONSOLE);
        assert_eq!(
            normalize_target_modules(Some("portal_pro, console, playground_2026")).unwrap(),
            "console,playground_2026,portal_pro"
        );
        assert_eq!(
            normalize_target_modules(Some("console,console")).unwrap(),
            MODULE_CONSOLE
        );
        assert_eq!(
            normalize_target_modules(Some("portal_pro, pay_modal, console")).unwrap(),
            "pay_modal,console,portal_pro"
        );
        assert_eq!(
            normalize_public_module(Some("pay_modal")).unwrap(),
            MODULE_PAY_MODAL
        );
        assert!(normalize_target_modules(Some("dashboard")).is_err());
        assert_eq!(parse_banner_bg("").unwrap(), None);
        assert_eq!(parse_banner_bg("#ABC").unwrap().as_deref(), Some("#aabbcc"));
        assert!(parse_banner_bg("red").is_err());
    }
}
