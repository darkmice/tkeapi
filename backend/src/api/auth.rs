/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::models::{
    AllSettings, CreateUserRequest, EmailRegisterRequest, LoginCandidate, LoginRequest,
    LoginResponse, LoginSelectResponse, MobileRegisterRequest, RegistrationSettings,
    ResetPasswordRequest, SendCodeRequest, SendSmsCodeRequest, SiteSettings, User,
};
use crate::services::email::EmailService;
use crate::time_system::DbTs;
use crate::AppState;
use axum::{
    extract::{ConnectInfo, Query, State},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use std::sync::Arc;

/// 验证码最大错误尝试次数（超过即作废）
const MAX_CODE_ATTEMPTS: i64 = 3;
/// OAuth state 有效期（秒）
const OAUTH_STATE_TTL_SECS: i64 = 600;
const USER_WITH_LEVEL_SQL: &str = "SELECT u.*, ul.name as level_name, ul.id as level_id, ul.allow_view_log_details, ul.invoice_enabled, ul.invoice_mode, ul.invoice_config, ul.marketing_enabled, ul.commission_ratio FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key";

/// 常量时间字符串比较，避免时序旁路
fn ct_eq_str(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes()
        .iter()
        .zip(b.as_bytes().iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// 指定 timedisplay 下「今日」UTC 半开区间，供 TIMESTAMPTZ 列范围查询。
fn today_bounds_db_ts(timedisplay: &str) -> (DbTs, DbTs) {
    let day = crate::time_system::local_day_bounds_utc(Utc::now(), timedisplay);
    (DbTs::from_utc(day.start_utc), DbTs::from_utc(day.end_utc))
}

fn site_timedisplay(settings: &crate::models::AllSettings) -> &str {
    let t = settings.site.default_timezone.trim();
    if t.is_empty() {
        crate::time_system::DEFAULT_TIMEDISPLAY
    } else {
        t
    }
}

/// 新用户个人时区：合法浏览器 IANA，否则 timesystem UTC。
fn new_user_timezone(requested: Option<&str>) -> String {
    requested
        .and_then(crate::time_system::try_iana_timezone_name)
        .unwrap_or_else(|| crate::time_system::TIMESYSTEM_TZ.to_string())
}

pub fn get_base_url_from_req(headers: &axum::http::HeaderMap, fallback: &str) -> String {
    crate::relay::vendor_callback::infer_base_url_with_fallback(headers, fallback)
}

/// 归一化 Origin 字符串：补齐 https://，去除尾部斜杠，仅提取合法的 scheme://host[:port]
pub fn normalize_origin(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let with_scheme = if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        format!("https://{}", trimmed)
    } else {
        trimmed.to_string()
    };
    if let Ok(parsed) = reqwest::Url::parse(&with_scheme) {
        let scheme = parsed.scheme();
        if (scheme == "http" || scheme == "https")
            && parsed.username().is_empty()
            && parsed.password().is_none()
        {
            if let Some(host) = parsed.host_str() {
                return match parsed.port() {
                    Some(port) => format!("{}://{}:{}", scheme, host, port),
                    None => format!("{}://{}", scheme, host),
                };
            }
        }
    }
    String::new()
}

fn origin_host(raw: &str) -> Option<String> {
    let norm = normalize_origin(raw);
    if norm.is_empty() {
        return None;
    }
    reqwest::Url::parse(&norm)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
}

/// 校验客户端传入的 OAuth return origin 是否属于受信任的站点域名（防 Open Redirect 劫持一次性登录兑换码）
pub fn resolve_trusted_oauth_origin(
    raw_origin: Option<&str>,
    req_base: &str,
    base_url: &str,
    callback_domain: Option<&str>,
) -> Option<String> {
    let candidate = normalize_origin(raw_origin?);
    if candidate.is_empty() {
        return None;
    }
    let Some(cand_host) = origin_host(&candidate) else {
        return None;
    };
    let matches_host = |trusted: &str| origin_host(trusted).is_some_and(|h| h == cand_host);
    if matches_host(req_base)
        || matches_host(base_url)
        || callback_domain.is_some_and(matches_host)
        || std::env::var("PUBLIC_FRONTEND_URL")
            .ok()
            .is_some_and(|v| matches_host(&v))
        || std::env::var("PUBLIC_API_URL")
            .ok()
            .is_some_and(|v| matches_host(&v))
        || std::env::var("CORS_ORIGINS").ok().is_some_and(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != "*")
                .any(matches_host)
        })
    {
        Some(candidate)
    } else {
        tracing::warn!(
            "[OAuth] 拦截不受信任的跨站 origin 重定向请求: candidate={}, req_base={}",
            candidate,
            req_base
        );
        let fallback = normalize_origin(req_base);
        (!fallback.is_empty()).then_some(fallback)
    }
}

pub(crate) fn sanitize_user_for_client(user: &mut User) {
    if is_placeholder_email(&user.email) {
        user.email = String::new();
    }
}

/// 用户登录 — 支持用户名/邮箱/手机号 + 密码（复用同一接口）
pub async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Response {
    let client_ip = extract_client_ip(&headers, &addr);
    let settings = match get_all_settings(&state).await {
        Ok(settings) => {
            if let Err(err) = check_ip_blacklist(&settings.site, &client_ip) {
                return err.into_response();
            }
            settings
        }
        Err(err) => return err.into_response(),
    };
    if request.login_type.as_deref() == Some("mobile") && !settings.login.enable_mobile_login {
        return AppError::Forbidden("手机号登录未开启".into()).into_response();
    }
    if request.login_type.as_deref() == Some("email") && !settings.login.enable_email_login {
        return AppError::Forbidden("邮箱登录未开启".into()).into_response();
    }
    if let Err(lock_msg) = state.rate_limiter.check_login_lock(&client_ip) {
        return AppError::Forbidden(lock_msg).into_response();
    }
    if !state.rate_limiter.check_login_ip(&client_ip, 30) {
        return AppError::Forbidden("登录尝试过于频繁，请稍后再试".into()).into_response();
    }
    if let Err(lock_msg) = state.rate_limiter.check_account_lock(&request.username) {
        return AppError::Forbidden(lock_msg).into_response();
    }
    let result = (async {
        let identifier = request.username.trim();
        let selected_uid = request
            .uid
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let users =
            load_site_login_users(&state, identifier, request.login_type.as_deref()).await?;
        if users.is_empty() {
            let msg = match request.login_type.as_deref() {
                Some("email") => "未找到该邮箱对应的账号",
                Some("mobile") => "未找到该手机号对应的账号",
                _ => "未找到此账号，请检查用户名或UID",
            };
            let msg = state.rate_limiter.record_login_failure(&client_ip, msg);
            return Err(AppError::AuthFailed(msg));
        }

        let mut password_matches = Vec::new();
        for user in users {
            if auth::verify_password(&request.password, &user.password_hash)? {
                password_matches.push(user);
            }
        }
        if password_matches.is_empty() {
            state.rate_limiter.record_account_failure(identifier);
            let msg = state
                .rate_limiter
                .record_login_failure(&client_ip, "密码输入错误，请重新尝试");
            return Err(AppError::AuthFailed(msg));
        }

        let active_matches: Vec<User> = password_matches
            .into_iter()
            .filter(|u| u.is_active != 0)
            .collect();
        if active_matches.is_empty() {
            let msg = state
                .rate_limiter
                .record_login_failure(&client_ip, "账号已被禁用");
            return Err(AppError::Forbidden(msg));
        }

        let mut user = if let Some(uid) = selected_uid {
            active_matches
                .into_iter()
                .find(|u| u.uid == uid)
                .ok_or_else(|| {
                    let msg = state
                        .rate_limiter
                        .record_login_failure(&client_ip, "密码输入错误，请重新尝试");
                    AppError::AuthFailed(msg)
                })?
        } else if active_matches.len() == 1 {
            let mut matches = active_matches;
            matches.remove(0)
        } else {
            let candidates = active_matches.iter().map(LoginCandidate::from).collect();
            return Ok(Json(LoginSelectResponse {
                need_select: true,
                candidates,
            })
            .into_response());
        };

        state.rate_limiter.clear_login_failures(&client_ip);
        state.rate_limiter.clear_account_failures(identifier);
        note_last_active_ip(&state, &user.id, &client_ip);

        if user.password_hash.starts_with("$2y$") || user.password_hash.starts_with("$2b$") {
            let new_hash = auth::hash_password(&request.password)?;
            sqlx::query(&state.db.format_query(
                "UPDATE users SET password_hash = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            ))
            .bind(&new_hash)
            .bind(&user.id)
            .execute(&state.db.pool)
            .await?;
        }

        if user.role == "admin" {
            crate::admin_permission::hydrate_user_admin_permissions(&state, &mut user).await?;
        }
        sanitize_user_for_client(&mut user);

        let token = auth::create_token(
            &user.id,
            &user.username,
            &user.role,
            &state.config.jwt_secret,
        )?;

        Ok(Json(LoginResponse { token, user }).into_response())
    })
    .await;

    match result {
        Ok(resp) => resp,
        Err(err) => err.into_response(),
    }
}

pub async fn admin_login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Response {
    let client_ip = extract_client_ip(&headers, &addr);
    if let Err(lock_msg) = state.rate_limiter.check_login_lock(&client_ip) {
        return AppError::Forbidden(lock_msg).into_response();
    }
    if !state.rate_limiter.check_login_ip(&client_ip, 30) {
        return AppError::Forbidden("登录尝试过于频繁，请稍后再试".into()).into_response();
    }
    if let Err(lock_msg) = state.rate_limiter.check_account_lock(&request.username) {
        return AppError::Forbidden(lock_msg).into_response();
    }
    let result = (async {
        let identifier = request.username.trim();
        let mut user: User =
            if let Some(found) = load_user_by_username_or_uid(&state, identifier).await? {
                found
            } else {
                let by_email = load_users_with_level_email(&state, identifier).await?;
                let admins: Vec<User> = by_email
                    .iter()
                    .filter(|u| u.role == "admin")
                    .cloned()
                    .collect();
                match admins.len() {
                    0 => {
                        if by_email.is_empty() {
                            let msg = state
                                .rate_limiter
                                .record_login_failure(&client_ip, "管理后台未查询到此账号");
                            return Err(AppError::AuthFailed(msg));
                        }
                        by_email
                            .into_iter()
                            .next()
                            .expect("non-empty email matches")
                    }
                    1 => {
                        let mut admins = admins;
                        admins.remove(0)
                    }
                    _ => {
                        return Err(AppError::BadRequest(
                            "该邮箱绑定了多个管理员账号，请使用用户名或 UID 登录".to_string(),
                        ));
                    }
                }
            };

        if user.role != "admin" {
            let msg = state
                .rate_limiter
                .record_login_failure(&client_ip, "Access denied: Not an administrator");
            return Err(AppError::Forbidden(msg));
        }

        if !auth::verify_password(&request.password, &user.password_hash)? {
            state.rate_limiter.record_account_failure(identifier);
            let msg = state
                .rate_limiter
                .record_login_failure(&client_ip, "管理员密码错误");
            return Err(AppError::AuthFailed(msg));
        }

        if user.is_active == 0 {
            let msg = state
                .rate_limiter
                .record_login_failure(&client_ip, "账号已被禁用");
            return Err(AppError::Forbidden(msg));
        }

        // 登录成功，重置失败与锁定记录
        state.rate_limiter.clear_login_failures(&client_ip);
        state.rate_limiter.clear_account_failures(identifier);
        note_last_active_ip(&state, &user.id, &client_ip);

        // 如果是bcrypt哈希，自动升级为Argon2
        if user.password_hash.starts_with("$2y$") || user.password_hash.starts_with("$2b$") {
            let new_hash = auth::hash_password(&request.password)?;
            sqlx::query(&state.db.format_query(
                "UPDATE users SET password_hash = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            ))
            .bind(&new_hash)
            .bind(&user.id)
            .execute(&state.db.pool)
            .await?;
        }

        crate::admin_permission::hydrate_user_admin_permissions(&state, &mut user).await?;

        let token = auth::create_token(
            &user.id,
            &user.username,
            &user.role,
            &state.config.jwt_secret,
        )?;

        Ok(Json(LoginResponse { token, user }))
    })
    .await;

    match result {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

#[derive(Debug, serde::Serialize)]
pub struct AdminInitStatusResponse {
    pub initialized: bool,
}

#[derive(Debug, serde::Deserialize)]
pub struct InitAdminRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub timezone: Option<String>,
}

pub async fn admin_init_status(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<AdminInitStatusResponse>> {
    let initialized: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE role = 'admin')")
            .fetch_one(&state.db.pool)
            .await?;

    Ok(Json(AdminInitStatusResponse { initialized }))
}

pub async fn init_admin(
    State(state): State<Arc<AppState>>,
    Json(request): Json<InitAdminRequest>,
) -> Response {
    let result: AppResult<Json<LoginResponse>> = (async {
        let exists_count: i64 = sqlx::query_scalar(
            &state.db.format_query("SELECT COUNT(*) FROM users WHERE role = 'admin'"),
        )
        .fetch_one(&state.db.pool)
        .await?;

        if exists_count > 0 {
            return Err(AppError::Forbidden("系统已初始化管理员账号，无法重复初始化".to_string()));
        }

        let username = request.username.trim();
        if username.is_empty() {
            return Err(AppError::BadRequest("管理员用户名不能为空".to_string()));
        }
        auth::validate_password(&request.password)?;

        crate::api::settings::apply_initial_site_timezone(&state, request.timezone.as_deref())
            .await?;

        let password_hash = auth::hash_password(&request.password)?;
        let id = uuid::Uuid::new_v4().to_string();
        let uid = state.db.generate_unique_uid().await?;
        let now = DbTs::now();

        sqlx::query(
            &state.db.format_query(r#"INSERT INTO users (id, uid, username, email, password_hash, role, balance, user_group, is_active, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, 'admin', 100.0, 'default', 1, ?, ?)"#)
        )
        .bind(&id)
        .bind(&uid)
        .bind(username)
        .bind(format!("{}@tokensbyte.local", username))
        .bind(&password_hash)
        .bind(&now)
        .bind(&now)
        .execute(&state.db.pool)
        .await?;

        let user: User = sqlx::query_as(
            &state.db.format_query("SELECT u.*, ul.name as level_name, ul.id as level_id, ul.allow_view_log_details, ul.invoice_enabled, ul.invoice_mode, ul.invoice_config, ul.marketing_enabled, ul.commission_ratio FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?")
        )
        .bind(&id)
        .fetch_one(&state.db.pool)
        .await?;

        let token = auth::create_token(&user.id, &user.username, &user.role, &state.config.jwt_secret)?;

        tracing::info!("Initial admin user '{}' (UID: {}) created successfully.", username, uid);

        Ok(Json(LoginResponse { token, user }))
    }).await;

    match result {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(request): Json<CreateUserRequest>,
) -> Response {
    let result = (async {
        let settings = get_all_settings(&state).await?;
        if !settings.registration.enable_username_registration {
            return Err(AppError::Forbidden("Username registration is disabled".to_string()));
        }
        let timezone = new_user_timezone(request.timezone.as_deref());

        // IP 黑名单与防刷检查
        let raw_ip = extract_client_ip(&headers, &addr);
        check_ip_blacklist(&settings.site, &raw_ip)?;
        if let Some(limit) = configured_username_ip_limit(&settings.registration) {
            check_username_ip_rate_limit(&state, &settings, &raw_ip, limit).await?;
        } else {
            check_ip_rate_limit(&state, &settings.registration, &raw_ip).await?;
        }

        // 用户名合规校验
        let username = request.username.trim();
        validate_username(username, true)?;
        auth::validate_password(&request.password)?;

        // 公开用户名注册不得写入未验证邮箱，否则可被用于邮箱找回接管
        let random_suffix: String = (0..8)
            .map(|_| rand::thread_rng().gen_range(0..10).to_string())
            .collect();
        let actual_email = format!("u_{}@tokensbyte.local", random_suffix);

        let exists: bool = sqlx::query_scalar(
            &state.db.format_query("SELECT EXISTS(SELECT 1 FROM users WHERE username = ?)")
        )
        .bind(username)
        .fetch_one(&state.db.pool)
        .await?;

        if exists {
            return Err(AppError::Forbidden("请更换用户名尝试".to_string()));
        }

        let password_hash = auth::hash_password(&request.password)?;
        let user_id = uuid::Uuid::new_v4().to_string();
        let uid = state.db.generate_unique_uid().await.map_err(AppError::from)?;

        let mut tx = state.db.pool.begin().await?;

        let mut referred_by: Option<String> = None;
        if let Some(ref aff_code) = request.aff {
            let aff_code = aff_code.trim();
            if !aff_code.is_empty() {
                let inviter_id: Option<String> =
                    sqlx::query_scalar(&state.db.format_query("SELECT id FROM users WHERE uid = ?"))
                        .bind(aff_code)
                        .fetch_optional(&mut *tx)
                        .await?;
                referred_by = inviter_id;
            }
        }

        let initial_balance = state.config.default_user_quota;
        let mut gift_amount = 0.0;
        let mut inviter_reward = 0.0;
        let mut marketing_override = false;
        let mut gift_remark = "注册赠送".to_string();

        if let Some(ref inv_id) = referred_by {
            lock_key_tx(&mut tx, &state.db, &format!("invite:{inv_id}")).await?;
            let inviter_group: Option<String> = sqlx::query_scalar(&state.db.format_query("SELECT user_group FROM users WHERE id = ?"))
                .bind(inv_id)
                .fetch_optional(&mut *tx)
                .await?;

            if let Some(group) = inviter_group {
                use sqlx::Row;
                let level_row_opt = sqlx::query(&state.db.format_query("SELECT marketing_enabled, invite_reward_inviter, invite_reward_invitee, daily_invite_limit FROM user_levels WHERE group_key = ?"))
                    .bind(&group)
                    .fetch_optional(&mut *tx)
                    .await?;

                if let Some(row) = level_row_opt {
                    let enabled: i64 = row.try_get::<i64, _>("marketing_enabled")
                        .unwrap_or_else(|_| row.try_get::<i64, _>("marketing_enabled").unwrap_or(0) as i64);

                    if enabled == 1 {
                        marketing_override = true;

                        let invitee_rew: f64 = row.try_get::<f64, _>("invite_reward_invitee").unwrap_or(0.0);
                        let inviter_rew: f64 = row.try_get::<f64, _>("invite_reward_inviter").unwrap_or(0.0);
                        let limit: i64 = row.try_get::<i64, _>("daily_invite_limit")
                            .unwrap_or_else(|_| row.try_get::<i64, _>("daily_invite_limit").unwrap_or(10) as i64);

                        gift_amount = invitee_rew;
                        gift_remark = "走专属链接注册特权赠送".to_string();

                        let mut can_reward = true;
                        if limit > 0 {
                            let (day_start, day_end) = today_bounds_db_ts(site_timedisplay(&settings));
                            let today_count: i64 = sqlx::query_scalar(&state.db.format_query(
                                "SELECT COUNT(*) FROM users WHERE referred_by = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz"
                            ))
                            .bind(inv_id)
                            .bind(&day_start)
                            .bind(&day_end)
                            .fetch_one(&mut *tx)
                            .await?;

                            if today_count >= limit {
                                can_reward = false;
                                gift_amount = 0.0;
                            }
                        }

                        if can_reward {
                            inviter_reward = inviter_rew;
                        }
                    }
                }
            }
        }

        if !marketing_override && settings.marketing.enable_registration_gift {
            gift_amount = calc_gift_amount(&settings.marketing);
        }

        // 用户名注册：若单独配置了用户名注册 IP 限制则按独立上限拦截；否则遵循站点安全 IP 防刷规则（发赠金/邀请奖励时未开开关按兜底值）
        if let Some(limit) = configured_username_ip_limit(&settings.registration) {
            enforce_username_ip_daily_limit_tx(&mut tx, &state.db, &settings, &raw_ip, limit).await?;
        } else if let Some(limit) = username_ip_daily_limit(
            &settings.registration,
            gift_amount > 0.0 || inviter_reward > 0.0,
        ) {
            enforce_ip_daily_limit_tx(&mut tx, &state.db, &settings, &raw_ip, limit).await?;
        }

        // 查询默认注册等级
        let default_group: String = sqlx::query_scalar(&state.db.format_query(
            "SELECT group_key FROM user_levels WHERE is_default = 1 LIMIT 1"
        ))
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or_else(|| "default".to_string());

        let referral_history = if let Some(ref inviter_id) = referred_by {
            let now = crate::time_system::utc_naive_string();
            let display_name = state.db.get_user_display_name(inviter_id).await;
            Some(format!("[{}] 通过 {} 邀请注册\n", now, display_name))
        } else {
            None
        };

        sqlx::query(
            &state.db.format_query(r#"INSERT INTO users (id, uid, username, email, password_hash, role, balance, gift_balance, is_active, referred_by, register_ip, user_group, referral_history, timezone)
               VALUES (?, ?, ?, ?, ?, 'user', ?, ?, 1, ?, ?, ?, ?, ?)"#)
        )
        .bind(&user_id)
        .bind(&uid)
        .bind(username)
        .bind(&actual_email)
        .bind(&password_hash)
        .bind(initial_balance)
        .bind(gift_amount)
        .bind(&referred_by)
        .bind(&raw_ip)
        .bind(&default_group)
        .bind(&referral_history)
        .bind(&timezone)
        .execute(&mut *tx)
        .await?;

        if gift_amount > 0.0 {
            let gift_order_no = crate::api::plugins::finance::generate_gift_order_no();
            sqlx::query(&state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'gift', ?, 'gift', ?)"))
                .bind(&user_id)
                .bind(gift_amount)
                .bind(&gift_remark)
                .bind(&gift_order_no)
                .execute(&mut *tx)
                .await?;
        }

        if inviter_reward > 0.0 {
            if let Some(ref inv_id) = referred_by {
                sqlx::query(&state.db.format_query("UPDATE users SET gift_balance = gift_balance + ? WHERE id = ?"))
                    .bind(inviter_reward)
                    .bind(inv_id)
                    .execute(&mut *tx)
                    .await?;

                let invite_order_no = crate::api::plugins::finance::generate_gift_order_no();
                sqlx::query(&state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'commission', '邀请成功奖励', 'gift', ?)"))
                    .bind(inv_id)
                    .bind(inviter_reward)
                    .bind(&invite_order_no)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;

        // 团队邀请码：注册后自动加入团队
        if let Some(ref team_code) = request.team {
            if !team_code.trim().is_empty() {
                #[cfg(all(feature = "commercial_plugins", plugin_team_marketing))]
                let _ = crate::api::plugins::team_marketing::add_user_to_team_by_invite_code(&state, &user_id, team_code.trim()).await;
            }
        }

        let mut user: User = sqlx::query_as(&state.db.format_query("SELECT * FROM users WHERE id = ?"))
            .bind(&user_id)
            .fetch_one(&state.db.pool)
            .await?;

        note_last_active_ip(&state, &user.id, &raw_ip);
        sanitize_user_for_client(&mut user);
        let token = auth::create_token(&user.id, &user.username, &user.role, &state.config.jwt_secret)?;

        Ok(Json(LoginResponse { token, user }))
    }).await;

    match result {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

/// 发送邮箱验证码（公开：仅注册 / 找回密码；登录用户：绑定邮箱）
pub async fn send_code(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(request): Json<SendCodeRequest>,
) -> Response {
    let result: AppResult<Json<serde_json::Value>> = (async {
        let settings = get_all_settings(&state).await?;
        let purpose = request.purpose.as_str();
        validate_code_purpose_and_auth(
            &headers,
            &state.config.jwt_secret,
            purpose,
            CodeChannel::Email,
        )?;

        let raw_ip = extract_client_ip(&headers, &addr);
        check_ip_blacklist(&settings.site, &raw_ip)?;

        dispatch_email_code(&state, &settings, &raw_ip, &request.email, purpose).await?;
        Ok(Json(serde_json::json!({ "success": true })))
    })
    .await;

    match result {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

/// 发送短信验证码（公开：仅注册 / 找回密码；登录用户：绑定手机）
pub async fn send_sms_code(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(request): Json<SendSmsCodeRequest>,
) -> Response {
    let result: AppResult<Json<serde_json::Value>> = (async {
        let settings = get_all_settings(&state).await?;
        let purpose = request.purpose.as_str();
        validate_code_purpose_and_auth(
            &headers,
            &state.config.jwt_secret,
            purpose,
            CodeChannel::Sms,
        )?;

        let raw_ip = extract_client_ip(&headers, &addr);
        check_ip_blacklist(&settings.site, &raw_ip)?;

        let mobile = auth::contact::normalize_mobile(
            &request.mobile,
            settings.registration.allow_international_mobile,
        )?;
        dispatch_sms_code(&state, &settings, &raw_ip, &mobile, purpose).await?;
        Ok(Json(serde_json::json!({ "success": true })))
    })
    .await;

    match result {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

/// 邮箱发码公共流程（公开发码与登录后绑定发码共用）：格式、用途开关、冷却、配额、落库、发送
pub(crate) async fn dispatch_email_code(
    state: &Arc<AppState>,
    settings: &AllSettings,
    client_ip: &str,
    email: &str,
    purpose: &str,
) -> AppResult<()> {
    let email_norm = auth::contact::canonical_email(email);
    let email = email_norm.as_str();
    if email.is_empty() {
        return Err(AppError::BadRequest("邮箱不能为空".to_string()));
    }

    // 格式与防刷对所有发码都生效；域名白名单只拦新邮箱注册
    validate_email(&settings.registration, email)?;
    if purpose == "register" {
        enforce_registration_email_whitelist(&settings.registration, email)?;
    }
    if is_placeholder_email(email) {
        return Err(AppError::BadRequest("无效的邮箱地址".to_string()));
    }

    if purpose == "register" && !settings.registration.enable_email_registration {
        return Err(AppError::Forbidden("邮箱注册未开启".to_string()));
    }
    if purpose == "reset_password" {
        if !settings.registration.enable_password_recovery {
            return Err(AppError::Forbidden(
                "Password recovery is disabled".to_string(),
            ));
        }
        // 仅向已真实绑定该邮箱的账号发送找回验证码，禁止任意邮箱刷码/接管未绑定账号
        if !email_is_bound_for_recovery(state, email).await? {
            return Err(AppError::BadRequest(
                "该邮箱未绑定账号，无法找回密码".to_string(),
            ));
        }
    }

    if settings.smtp.host.trim().is_empty() || settings.smtp.from_address.trim().is_empty() {
        return Err(AppError::BadRequest("邮箱通知未配置".to_string()));
    }
    let email_service = EmailService::new(&settings.smtp)?;

    check_code_send_cooldown(state, email, "", purpose).await?;
    state
        .rate_limiter
        .check_code_send(
            client_ip,
            &code_send_key(purpose, &email.to_lowercase()),
        )
        .map_err(AppError::TooManyRequests)?;

    let code = generate_code();
    save_verification_code(state, email, "", &code, purpose).await?;

    if let Err(err) = email_service
        .send_verification_code(email, &code, purpose)
        .await
    {
        let _ = sqlx::query(
            &state
                .db
                .format_query("DELETE FROM verification_codes WHERE email = ? AND purpose = ?"),
        )
        .bind(email)
        .bind(purpose)
        .execute(&state.db.pool)
        .await;
        return Err(err);
    }
    Ok(())
}

/// 短信发码公共流程；`mobile` 须已由调用方规范化
pub(crate) async fn dispatch_sms_code(
    state: &Arc<AppState>,
    settings: &AllSettings,
    client_ip: &str,
    mobile: &str,
    purpose: &str,
) -> AppResult<()> {
    let mobile = mobile.trim();
    if mobile.is_empty() {
        return Err(AppError::BadRequest("手机号不能为空".to_string()));
    }

    if purpose == "register" && !settings.registration.enable_mobile_registration {
        return Err(AppError::Forbidden("手机号注册未开启".to_string()));
    }
    if purpose == "reset_password" {
        if !settings.registration.enable_password_recovery {
            return Err(AppError::Forbidden(
                "Password recovery is disabled".to_string(),
            ));
        }
        if !mobile_is_bound_for_recovery(state, mobile).await? {
            return Err(AppError::BadRequest(
                "该手机号未绑定账号，无法找回密码".to_string(),
            ));
        }
    }

    let sms_settings = settings
        .sms
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("短信通知未配置".to_string()))?;
    if !sms_settings.credentials_configured() {
        return Err(AppError::BadRequest("短信通知未配置".to_string()));
    }

    check_code_send_cooldown(state, "", mobile, purpose).await?;
    state
        .rate_limiter
        .check_code_send(client_ip, &code_send_key(purpose, mobile))
        .map_err(AppError::TooManyRequests)?;

    let code = generate_code();
    save_verification_code(state, "", mobile, &code, purpose).await?;

    let sms_service = crate::services::sms::SmsService::new(sms_settings);
    sms_service.send_verification_code(mobile, &code).await?;
    Ok(())
}

/// 邮箱注册
pub async fn register_email(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(request): Json<EmailRegisterRequest>,
) -> Response {
    let result = (async {
        let settings = get_all_settings(&state).await?;
        if !settings.registration.enable_email_registration {
            return Err(AppError::Forbidden("邮箱注册未开启".to_string()));
        }
        let timezone = new_user_timezone(request.timezone.as_deref());

        // 邮箱校验（含域名白名单，仅新注册）
        let email_norm = auth::contact::canonical_email(&request.email);
        let email = email_norm.as_str();
        validate_email(&settings.registration, email)?;
        enforce_registration_email_whitelist(&settings.registration, email)?;
        auth::validate_password(&request.password)?;

        // IP 黑名单与防刷
        let raw_ip = extract_client_ip(&headers, &addr);
        check_ip_blacklist(&settings.site, &raw_ip)?;
        check_ip_rate_limit(&state, &settings.registration, &raw_ip).await?;

        verify_email_code(&state, email, &request.code, "register", false).await?;

        let email_limit = auth::contact::normalize_limit(settings.registration.max_accounts_per_email);
        let occ = auth::contact::email_occupancy(&state, email, None, email_limit).await?;
        auth::contact::gate_shared_contact(occ, request.confirm_new_account, "邮箱")?;

        verify_email_code(&state, email, &request.code, "register", true).await?;

        let user_id = uuid::Uuid::new_v4().to_string();
        let uid = state.db.generate_unique_uid().await.map_err(AppError::from)?;
        let username = generate_unique_username(&state, email).await?;
        let password_hash = auth::hash_password(&request.password)?;

        let mut tx = state.db.pool.begin().await?;
        auth::contact::assert_email_slot_tx(
            &mut tx,
            &state.db,
            email,
            None,
            email_limit,
        )
        .await?;

        let mut referred_by: Option<String> = None;
        if let Some(ref aff_code) = request.aff {
            let aff_code = aff_code.trim();
            if !aff_code.is_empty() {
                let inviter_id: Option<String> =
                    sqlx::query_scalar(&state.db.format_query("SELECT id FROM users WHERE uid = ?"))
                        .bind(aff_code)
                        .fetch_optional(&mut *tx)
                        .await?;
                referred_by = inviter_id;
            }
        }

        let initial_balance = state.config.default_user_quota;
        let mut gift_amount = 0.0;
        let mut inviter_reward = 0.0;
        let mut marketing_override = false;
        let mut gift_remark = "注册赠送".to_string();

        if let Some(ref inv_id) = referred_by {
            lock_key_tx(&mut tx, &state.db, &format!("invite:{inv_id}")).await?;
            let inviter_group: Option<String> = sqlx::query_scalar(&state.db.format_query("SELECT user_group FROM users WHERE id = ?"))
                .bind(inv_id)
                .fetch_optional(&mut *tx)
                .await?;

            if let Some(group) = inviter_group {
                use sqlx::Row;
                let level_row_opt = sqlx::query(&state.db.format_query("SELECT marketing_enabled, invite_reward_inviter, invite_reward_invitee, daily_invite_limit FROM user_levels WHERE group_key = ?"))
                    .bind(&group)
                    .fetch_optional(&mut *tx)
                    .await?;

                if let Some(row) = level_row_opt {
                    let enabled: i64 = row.try_get::<i64, _>("marketing_enabled")
                        .unwrap_or_else(|_| row.try_get::<i64, _>("marketing_enabled").unwrap_or(0) as i64);

                    if enabled == 1 {
                        marketing_override = true;

                        let invitee_rew: f64 = row.try_get::<f64, _>("invite_reward_invitee").unwrap_or(0.0);
                        let inviter_rew: f64 = row.try_get::<f64, _>("invite_reward_inviter").unwrap_or(0.0);
                        let limit: i64 = row.try_get::<i64, _>("daily_invite_limit")
                            .unwrap_or_else(|_| row.try_get::<i64, _>("daily_invite_limit").unwrap_or(10) as i64);

                        gift_amount = invitee_rew;
                        gift_remark = "走专属链接注册特权赠送".to_string();

                        let mut can_reward = true;
                        if limit > 0 {
                            let (day_start, day_end) = today_bounds_db_ts(site_timedisplay(&settings));
                            let today_count: i64 = sqlx::query_scalar(&state.db.format_query(
                                "SELECT COUNT(*) FROM users WHERE referred_by = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz"
                            ))
                            .bind(inv_id)
                            .bind(&day_start)
                            .bind(&day_end)
                            .fetch_one(&mut *tx)
                            .await?;

                            if today_count >= limit {
                                can_reward = false;
                                gift_amount = 0.0;
                            }
                        }

                        if can_reward {
                            inviter_reward = inviter_rew;
                        }
                    }
                }
            }
        }

        if !marketing_override && settings.marketing.enable_registration_gift {
            gift_amount = calc_gift_amount(&settings.marketing);
        }

        if settings.registration.ip_rate_limit_enabled {
            let limit = settings.registration.ip_daily_limit;
            enforce_ip_daily_limit_tx(&mut tx, &state.db, &settings, &raw_ip, limit).await?;
        }

        // 查询默认注册等级
        let default_group: String = sqlx::query_scalar(&state.db.format_query(
            "SELECT group_key FROM user_levels WHERE is_default = 1 LIMIT 1"
        ))
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or_else(|| "default".to_string());

        let referral_history = if let Some(ref inviter_id) = referred_by {
            let now = crate::time_system::utc_naive_string();
            let display_name = state.db.get_user_display_name(inviter_id).await;
            Some(format!("[{}] 通过 {} 邀请注册\n", now, display_name))
        } else {
            None
        };

        sqlx::query(
            &state.db.format_query(r#"INSERT INTO users (id, uid, username, email, password_hash, role, balance, gift_balance, is_active, referred_by, register_ip, user_group, referral_history, timezone)
               VALUES (?, ?, ?, ?, ?, 'user', ?, ?, 1, ?, ?, ?, ?, ?)"#)
        )
        .bind(&user_id).bind(&uid).bind(&username).bind(email)
        .bind(&password_hash).bind(initial_balance).bind(gift_amount).bind(&referred_by).bind(&raw_ip)
        .bind(&default_group).bind(&referral_history).bind(&timezone)
        .execute(&mut *tx)
        .await?;

        if gift_amount > 0.0 {
            let gift_order_no = crate::api::plugins::finance::generate_gift_order_no();
            sqlx::query(&state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'gift', ?, 'gift', ?)"))
                .bind(&user_id)
                .bind(gift_amount)
                .bind(&gift_remark)
                .bind(&gift_order_no)
                .execute(&mut *tx)
                .await?;
        }

        if inviter_reward > 0.0 {
            if let Some(ref inv_id) = referred_by {
                sqlx::query(&state.db.format_query("UPDATE users SET gift_balance = gift_balance + ? WHERE id = ?"))
                    .bind(inviter_reward)
                    .bind(inv_id)
                    .execute(&mut *tx)
                    .await?;

                let invite_order_no = crate::api::plugins::finance::generate_gift_order_no();
                sqlx::query(&state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'commission', '邀请成功奖励', 'gift', ?)"))
                    .bind(inv_id)
                    .bind(inviter_reward)
                    .bind(&invite_order_no)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;

        // 团队邀请码：注册后自动加入团队
        if let Some(ref team_code) = request.team {
            if !team_code.trim().is_empty() {
                #[cfg(all(feature = "commercial_plugins", plugin_team_marketing))]
                let _ = crate::api::plugins::team_marketing::add_user_to_team_by_invite_code(&state, &user_id, team_code.trim()).await;
            }
        }

        let mut user: User = sqlx::query_as(&state.db.format_query("SELECT * FROM users WHERE id = ?"))
            .bind(&user_id)
            .fetch_one(&state.db.pool)
            .await?;

        note_last_active_ip(&state, &user.id, &raw_ip);
        sanitize_user_for_client(&mut user);
        let token = auth::create_token(&user.id, &user.username, &user.role, &state.config.jwt_secret)?;

        Ok(Json(LoginResponse { token, user }))
    }).await;

    match result {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

/// 手机号注册
pub async fn register_mobile(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(request): Json<MobileRegisterRequest>,
) -> Response {
    let result = (async {
        let settings = get_all_settings(&state).await?;
        if !settings.registration.enable_mobile_registration {
            return Err(AppError::Forbidden("手机号注册未开启".to_string()));
        }
        let timezone = new_user_timezone(request.timezone.as_deref());

        // IP 提取与黑名单校验
        let raw_ip = extract_client_ip(&headers, &addr);
        check_ip_blacklist(&settings.site, &raw_ip)?;

        let mobile = auth::contact::normalize_mobile(
            &request.mobile,
            settings.registration.allow_international_mobile,
        )?;
        auth::validate_password(&request.password)?;

        // 验证短信验证码
        verify_sms_code(&state, &mobile, &request.code, "register", false).await?;

        let mobile_limit = auth::contact::normalize_limit(settings.registration.max_accounts_per_mobile);
        let occ = auth::contact::mobile_occupancy(&state, &mobile, None, mobile_limit).await?;
        auth::contact::gate_shared_contact(occ, request.confirm_new_account, "手机号")?;

        verify_sms_code(&state, &mobile, &request.code, "register", true).await?;

        let user_id = uuid::Uuid::new_v4().to_string();
        let uid = state.db.generate_unique_uid().await.map_err(AppError::from)?;
        let base_username = format!("m_{}", &mobile[mobile.len().saturating_sub(4)..]);
        // 确保用户名唯一
        let username = ensure_unique_username(&state, &base_username).await?;
        let password_hash = auth::hash_password(&request.password)?;
        let placeholder_email = format!("m_{}@tokensbyte.local", &uid);

        let mut tx = state.db.pool.begin().await?;
        auth::contact::assert_mobile_slot_tx(
            &mut tx,
            &state.db,
            &mobile,
            None,
            mobile_limit,
        )
        .await?;

        let mut referred_by: Option<String> = None;
        if let Some(ref aff_code) = request.aff {
            let aff_code = aff_code.trim();
            if !aff_code.is_empty() {
                let inviter_id: Option<String> =
                    sqlx::query_scalar(&state.db.format_query("SELECT id FROM users WHERE uid = ?"))
                        .bind(aff_code)
                        .fetch_optional(&mut *tx)
                        .await?;
                referred_by = inviter_id;
            }
        }

        let initial_balance = state.config.default_user_quota;
        let mut gift_amount = 0.0;
        let mut inviter_reward = 0.0;
        let mut marketing_override = false;
        let mut gift_remark = "注册赠送".to_string();

        if let Some(ref inv_id) = referred_by {
            lock_key_tx(&mut tx, &state.db, &format!("invite:{inv_id}")).await?;
            let inviter_group: Option<String> = sqlx::query_scalar(&state.db.format_query("SELECT user_group FROM users WHERE id = ?"))
                .bind(inv_id)
                .fetch_optional(&mut *tx)
                .await?;

            if let Some(group) = inviter_group {
                use sqlx::Row;
                let level_row_opt = sqlx::query(&state.db.format_query("SELECT marketing_enabled, invite_reward_inviter, invite_reward_invitee, daily_invite_limit FROM user_levels WHERE group_key = ?"))
                    .bind(&group)
                    .fetch_optional(&mut *tx)
                    .await?;

                if let Some(row) = level_row_opt {
                    let enabled: i64 = row.try_get::<i64, _>("marketing_enabled")
                        .unwrap_or_else(|_| row.try_get::<i64, _>("marketing_enabled").unwrap_or(0) as i64);

                    if enabled == 1 {
                        marketing_override = true;

                        let invitee_rew: f64 = row.try_get::<f64, _>("invite_reward_invitee").unwrap_or(0.0);
                        let inviter_rew: f64 = row.try_get::<f64, _>("invite_reward_inviter").unwrap_or(0.0);
                        let limit: i64 = row.try_get::<i64, _>("daily_invite_limit")
                            .unwrap_or_else(|_| row.try_get::<i64, _>("daily_invite_limit").unwrap_or(10) as i64);

                        gift_amount = invitee_rew;
                        gift_remark = "走专属链接注册特权赠送".to_string();

                        let mut can_reward = true;
                        if limit > 0 {
                            let (day_start, day_end) = today_bounds_db_ts(site_timedisplay(&settings));
                            let today_count: i64 = sqlx::query_scalar(&state.db.format_query(
                                "SELECT COUNT(*) FROM users WHERE referred_by = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz"
                            ))
                            .bind(inv_id)
                            .bind(&day_start)
                            .bind(&day_end)
                            .fetch_one(&mut *tx)
                            .await?;

                            if today_count >= limit {
                                can_reward = false;
                                gift_amount = 0.0;
                            }
                        }

                        if can_reward {
                            inviter_reward = inviter_rew;
                        }
                    }
                }
            }
        }

        if !marketing_override && settings.marketing.enable_registration_gift {
            gift_amount = calc_gift_amount(&settings.marketing);
        }

        // 查询默认注册等级
        let default_group: String = sqlx::query_scalar(&state.db.format_query(
            "SELECT group_key FROM user_levels WHERE is_default = 1 LIMIT 1"
        ))
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or_else(|| "default".to_string());

        let referral_history = if let Some(ref inviter_id) = referred_by {
            let now = crate::time_system::utc_naive_string();
            let display_name = state.db.get_user_display_name(inviter_id).await;
            Some(format!("[{}] 通过 {} 邀请注册\n", now, display_name))
        } else {
            None
        };

        sqlx::query(
            &state.db.format_query(r#"INSERT INTO users (id, uid, username, email, mobile, password_hash, role, balance, gift_balance, is_active, referred_by, register_ip, user_group, referral_history, timezone)
               VALUES (?, ?, ?, ?, ?, ?, 'user', ?, ?, 1, ?, ?, ?, ?, ?)"#)
        )
        .bind(&user_id).bind(&uid).bind(&username).bind(&placeholder_email)
        .bind(&mobile).bind(&password_hash).bind(initial_balance).bind(gift_amount)
        .bind(&referred_by).bind(&raw_ip)
        .bind(&default_group).bind(&referral_history).bind(&timezone)
        .execute(&mut *tx).await?;

        if gift_amount > 0.0 {
            let gift_order_no = crate::api::plugins::finance::generate_gift_order_no();
            sqlx::query(&state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'gift', ?, 'gift', ?)"))
                .bind(&user_id).bind(gift_amount).bind(&gift_remark).bind(&gift_order_no)
                .execute(&mut *tx).await?;
        }

        if inviter_reward > 0.0 {
            if let Some(ref inv_id) = referred_by {
                sqlx::query(&state.db.format_query("UPDATE users SET gift_balance = gift_balance + ? WHERE id = ?"))
                    .bind(inviter_reward)
                    .bind(inv_id)
                    .execute(&mut *tx)
                    .await?;

                let invite_order_no = crate::api::plugins::finance::generate_gift_order_no();
                sqlx::query(&state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'commission', '邀请成功奖励', 'gift', ?)"))
                    .bind(inv_id)
                    .bind(inviter_reward)
                    .bind(&invite_order_no)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;

        // 团队邀请码：注册后自动加入团队
        if let Some(ref team_code) = request.team {
            if !team_code.trim().is_empty() {
                #[cfg(all(feature = "commercial_plugins", plugin_team_marketing))]
                let _ = crate::api::plugins::team_marketing::add_user_to_team_by_invite_code(&state, &user_id, team_code.trim()).await;
            }
        }

        let mut user: User = sqlx::query_as(&state.db.format_query("SELECT * FROM users WHERE id = ?"))
            .bind(&user_id).fetch_one(&state.db.pool).await?;

        note_last_active_ip(&state, &user.id, &raw_ip);
        sanitize_user_for_client(&mut user);
        let token = auth::create_token(&user.id, &user.username, &user.role, &state.config.jwt_secret)?;
        Ok(Json(LoginResponse { token, user }))
    }).await;

    match result {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

pub async fn reset_password(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ResetPasswordRequest>,
) -> Response {
    let result = (async {
        let settings = get_all_settings(&state).await?;
        let raw_ip = extract_client_ip(&headers, &addr);
        check_ip_blacklist(&settings.site, &raw_ip)?;
        if !state.rate_limiter.check_login_ip(&raw_ip, 30) {
            return Err(AppError::Forbidden(
                "请求过于频繁，请稍后再试".to_string(),
            ));
        }
        if !settings.registration.enable_password_recovery {
            return Err(AppError::Forbidden("Password recovery is disabled".to_string()));
        }

        auth::validate_password(&request.new_password)?;

        let password_hash = auth::hash_password(&request.new_password)?;
        let selected_uid = request
            .uid
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        // 先验码（不消耗）再判断 UID 归属，避免无码探测某邮箱/手机绑定了哪些 UID
        let result = if let Some(raw_email) = &request.email {
            let email_norm = auth::contact::canonical_email(raw_email);
            let email = email_norm.as_str();
            if email.is_empty() || is_placeholder_email(email) {
                return Err(AppError::BadRequest(
                    "该邮箱未绑定账号，无法找回密码".to_string(),
                ));
            }
            let candidates = auth::contact::list_email_candidates(&state, email).await?;
            if candidates.is_empty() {
                return Err(AppError::BadRequest(
                    "该邮箱未绑定账号，无法找回密码".to_string(),
                ));
            }
            verify_email_code(&state, email, &request.code, "reset_password", false).await?;
            let target_uid = match (selected_uid, candidates.len()) {
                (Some(uid), _) => {
                    if candidates.iter().any(|c| c.uid == uid) {
                        uid.to_string()
                    } else {
                        return Err(AppError::BadRequest(
                            "所选账号与该邮箱不匹配".to_string(),
                        ));
                    }
                }
                (None, 1) => candidates[0].uid.clone(),
                (None, _) => {
                    return Err(auth::contact::account_select_error(candidates, "邮箱"));
                }
            };
            verify_email_code(&state, email, &request.code, "reset_password", true).await?;
            sqlx::query(&state.db.format_query(
                "UPDATE users SET password_hash = ?, password_changed_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE uid = ? AND LOWER(email) = LOWER(?)",
            ))
            .bind(&password_hash)
            .bind(&target_uid)
            .bind(email)
            .execute(&state.db.pool)
            .await?
        } else if let Some(raw_mobile) = &request.mobile {
            if raw_mobile.trim().is_empty() {
                return Err(AppError::BadRequest(
                    "该手机号未绑定账号，无法找回密码".to_string(),
                ));
            }
            let mobile = auth::contact::normalize_mobile(
                raw_mobile,
                settings.registration.allow_international_mobile,
            )?;
            let mobile = mobile.as_str();
            let candidates = auth::contact::list_mobile_candidates(&state, mobile).await?;
            if candidates.is_empty() {
                return Err(AppError::BadRequest(
                    "该手机号未绑定账号，无法找回密码".to_string(),
                ));
            }
            verify_sms_code(&state, mobile, &request.code, "reset_password", false).await?;
            let target_uid = match (selected_uid, candidates.len()) {
                (Some(uid), _) => {
                    if candidates.iter().any(|c| c.uid == uid) {
                        uid.to_string()
                    } else {
                        return Err(AppError::BadRequest(
                            "所选账号与该手机号不匹配".to_string(),
                        ));
                    }
                }
                (None, 1) => candidates[0].uid.clone(),
                (None, _) => {
                    return Err(auth::contact::account_select_error(candidates, "手机号"));
                }
            };
            verify_sms_code(&state, mobile, &request.code, "reset_password", true).await?;
            sqlx::query(&state.db.format_query(
                "UPDATE users SET password_hash = ?, password_changed_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE uid = ? AND mobile = ?",
            ))
            .bind(&password_hash)
            .bind(&target_uid)
            .bind(mobile)
            .execute(&state.db.pool)
            .await?
        } else {
            return Err(AppError::BadRequest("Email or mobile is required".to_string()));
        };

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound("User not found".to_string()));
        }

        Ok(Json(serde_json::json!({ "success": true })))
    }).await;

    match result {
        Ok(json) => json.into_response(),
        Err(err) => err.into_response(),
    }
}

// ======================== OAuth 回调 ========================

#[derive(Debug, serde::Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
}

/// 从 Cookie 头解析指定 key（与前端 inviteTracking 双存储中的 cookie 对应）
fn parse_cookie_value(cookie_header: &str, key: &str) -> Option<String> {
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(key) {
            if let Some(val) = rest.strip_prefix('=') {
                let decoded = urlencoding_decode(val);
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
        }
    }
    None
}

fn urlencoding_decode(s: &str) -> String {
    // 轻量 percent-decode，避免为单场景引入依赖
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Ok(h), Ok(l)) = (
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 2]).unwrap_or(""), 16),
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 2..i + 3]).unwrap_or(""), 16),
            ) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 前端 3 天邀请 cookie 名（与 frontend/src/utils/inviteTracking.ts 一致）
const COOKIE_AFF: &str = "tokensbyte_affiliate_code";
const COOKIE_TEAM: &str = "tokensbyte_team_invite";

fn invite_codes_from_headers(headers: &axum::http::HeaderMap) -> (Option<String>, Option<String>) {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let aff = parse_cookie_value(cookie, COOKIE_AFF);
    let team = parse_cookie_value(cookie, COOKIE_TEAM);
    (aff, team)
}

fn pick_nonempty_param(
    params: &std::collections::HashMap<String, String>,
    key: &str,
) -> Option<String> {
    params
        .get(key)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// query 优先，其次 cookie（用于发起 OAuth 时写入 state）
fn invite_from_query_or_cookie(
    headers: &axum::http::HeaderMap,
    params: &std::collections::HashMap<String, String>,
) -> (Option<String>, Option<String>) {
    let (c_aff, c_team) = invite_codes_from_headers(headers);
    (
        pick_nonempty_param(params, "aff").or(c_aff),
        pick_nonempty_param(params, "team").or(c_team),
    )
}

/// 邀请与中继载荷：hex(aff + '\n' + team + '\n' + timezone + '\n' + origin [+ '\n' + intent])，全空时为 "-"（避免 `_` 破坏 state 分段）
fn encode_invite_payload_with_intent(
    aff: Option<&str>,
    team: Option<&str>,
    timezone: Option<&str>,
    origin: Option<&str>,
    intent: Option<&str>,
) -> String {
    let a = aff.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("");
    let t = team.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("");
    let z = timezone
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let o = origin
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(normalize_origin)
        .unwrap_or_default();
    let i = intent
        .map(str::trim)
        .filter(|s| *s == "register")
        .unwrap_or("");
    if a.is_empty() && t.is_empty() && z.is_empty() && o.is_empty() && i.is_empty() {
        return "-".to_string();
    }
    if i.is_empty() {
        hex::encode(format!("{a}\n{t}\n{z}\n{o}"))
    } else {
        hex::encode(format!("{a}\n{t}\n{z}\n{o}\n{i}"))
    }
}

fn decode_invite_payload_with_intent(
    raw: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if raw.is_empty() || raw == "-" {
        return (None, None, None, None, None);
    }
    let Ok(bytes) = hex::decode(raw) else {
        return (None, None, None, None, None);
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return (None, None, None, None, None);
    };
    let mut parts = text.splitn(5, '\n');
    let a = parts.next().unwrap_or("").trim();
    let t = parts.next().unwrap_or("").trim();
    let z = parts.next().unwrap_or("").trim();
    let o = parts.next().unwrap_or("").trim();
    let i = parts.next().unwrap_or("").trim();
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    (opt(a), opt(t), opt(z), opt(o), opt(i))
}

struct OAuthInviteResolution {
    referred_by: Option<String>,
    gift_amount: f64,
    gift_remark: String,
    inviter_reward: f64,
    referral_history: Option<String>,
    team: Option<String>,
}

/// OAuth 自动注册时解析邀请关系：state 优先，cookie 兜底（对齐 3 天邀请规则）
async fn resolve_oauth_invite(
    state: &Arc<AppState>,
    settings: &AllSettings,
    headers: &axum::http::HeaderMap,
    state_aff: Option<String>,
    state_team: Option<String>,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> AppResult<OAuthInviteResolution> {
    let (cookie_aff, cookie_team) = invite_codes_from_headers(headers);
    let aff = state_aff.filter(|s| !s.trim().is_empty()).or(cookie_aff);
    let team = state_team.filter(|s| !s.trim().is_empty()).or(cookie_team);

    let mut referred_by: Option<String> = None;
    if let Some(ref aff_code) = aff {
        let inviter_id: Option<String> =
            sqlx::query_scalar(&state.db.format_query("SELECT id FROM users WHERE uid = ?"))
                .bind(aff_code.trim())
                .fetch_optional(&mut **tx)
                .await?;
        referred_by = inviter_id;
    }

    let mut gift_amount = 0.0;
    let mut inviter_reward = 0.0;
    let mut marketing_override = false;
    let mut gift_remark = "注册赠送".to_string();

    if let Some(ref inv_id) = referred_by {
        lock_key_tx(tx, &state.db, &format!("invite:{inv_id}")).await?;
        let inviter_group: Option<String> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT user_group FROM users WHERE id = ?"),
        )
        .bind(inv_id)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some(group) = inviter_group {
            use sqlx::Row;
            let level_row_opt = sqlx::query(&state.db.format_query(
                "SELECT marketing_enabled, invite_reward_inviter, invite_reward_invitee, daily_invite_limit FROM user_levels WHERE group_key = ?",
            ))
            .bind(&group)
            .fetch_optional(&mut **tx)
            .await?;

            if let Some(row) = level_row_opt {
                let enabled: i64 =
                    row.try_get::<i64, _>("marketing_enabled")
                        .unwrap_or_else(|_| {
                            row.try_get::<i64, _>("marketing_enabled").unwrap_or(0) as i64
                        });

                if enabled == 1 {
                    marketing_override = true;
                    let invitee_rew: f64 = row
                        .try_get::<f64, _>("invite_reward_invitee")
                        .unwrap_or(0.0);
                    let inviter_rew: f64 = row
                        .try_get::<f64, _>("invite_reward_inviter")
                        .unwrap_or(0.0);
                    let limit: i64 = row.try_get::<i64, _>("daily_invite_limit").unwrap_or(10);

                    gift_amount = invitee_rew;
                    gift_remark = "走专属链接注册特权赠送".to_string();

                    let mut can_reward = true;
                    if limit > 0 {
                        let (day_start, day_end) = today_bounds_db_ts(site_timedisplay(settings));
                        let today_count: i64 = sqlx::query_scalar(&state.db.format_query(
                            "SELECT COUNT(*) FROM users WHERE referred_by = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz",
                        ))
                        .bind(inv_id)
                        .bind(&day_start)
                        .bind(&day_end)
                        .fetch_one(&mut **tx)
                        .await?;

                        if today_count >= limit {
                            can_reward = false;
                            gift_amount = 0.0;
                        }
                    }

                    if can_reward {
                        inviter_reward = inviter_rew;
                    }
                }
            }
        }
    }

    if !marketing_override && settings.marketing.enable_registration_gift {
        gift_amount = calc_gift_amount(&settings.marketing);
    }

    let referral_history = if let Some(ref inviter_id) = referred_by {
        let now = crate::time_system::utc_naive_string();
        let display_name = state.db.get_user_display_name(inviter_id).await;
        Some(format!("[{}] 通过 {} 邀请注册\n", now, display_name))
    } else {
        None
    };

    Ok(OAuthInviteResolution {
        referred_by,
        gift_amount,
        gift_remark,
        inviter_reward,
        referral_history,
        team,
    })
}

async fn credit_inviter_reward_tx(
    state: &Arc<AppState>,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invite: &OAuthInviteResolution,
) -> AppResult<()> {
    if invite.inviter_reward > 0.0 {
        if let Some(ref inv_id) = invite.referred_by {
            sqlx::query(
                &state
                    .db
                    .format_query("UPDATE users SET gift_balance = gift_balance + ? WHERE id = ?"),
            )
            .bind(invite.inviter_reward)
            .bind(inv_id)
            .execute(&mut **tx)
            .await?;

            let invite_order_no = crate::api::plugins::finance::generate_gift_order_no();
            sqlx::query(&state.db.format_query(
                "INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'commission', '邀请成功奖励', 'gift', ?)",
            ))
            .bind(inv_id)
            .bind(invite.inviter_reward)
            .bind(&invite_order_no)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn finalize_oauth_invite_side_effects(
    state: &Arc<AppState>,
    user_id: &str,
    invite: &OAuthInviteResolution,
) -> AppResult<()> {
    #[cfg(not(all(feature = "commercial_plugins", plugin_team_marketing)))]
    let _ = (state, user_id);
    if let Some(ref team_code) = invite.team {
        if !team_code.trim().is_empty() {
            #[cfg(all(feature = "commercial_plugins", plugin_team_marketing))]
            let _ = crate::api::plugins::team_marketing::add_user_to_team_by_invite_code(
                state,
                user_id,
                team_code.trim(),
            )
            .await;
        }
    }
    Ok(())
}

#[derive(Clone)]
struct WechatPendingSession {
    wechat_id: String,
    openid: String,
    nickname: Option<String>,
    headimgurl: Option<String>,
    aff: Option<String>,
    team: Option<String>,
    timezone: Option<String>,
}

static WECHAT_OAUTH_SESSIONS: std::sync::LazyLock<
    dashmap::DashMap<String, (WechatPendingSession, std::time::Instant)>,
> = std::sync::LazyLock::new(dashmap::DashMap::new);

fn issue_wechat_session_code(session: WechatPendingSession) -> String {
    let code = uuid::Uuid::new_v4().simple().to_string();
    let now = std::time::Instant::now();
    let expires = now + std::time::Duration::from_secs(300);
    WECHAT_OAUTH_SESSIONS.retain(|_, (_, exp)| *exp > now);
    WECHAT_OAUTH_SESSIONS.insert(code.clone(), (session, expires));
    code
}

async fn create_wechat_user(
    state: &Arc<AppState>,
    settings: &AllSettings,
    headers: &axum::http::HeaderMap,
    session: &WechatPendingSession,
    raw_ip: &str,
) -> AppResult<User> {
    let wechat_limit =
        auth::contact::normalize_limit(settings.registration.max_accounts_per_wechat);
    let mut tx = state.db.pool.begin().await?;
    auth::contact::assert_wechat_slot_tx(
        &mut tx,
        &state.db,
        &session.wechat_id,
        None,
        wechat_limit,
    )
    .await?;
    let invite = resolve_oauth_invite(
        state,
        settings,
        headers,
        session.aff.clone(),
        session.team.clone(),
        &mut tx,
    )
    .await?;
    let user_id = uuid::Uuid::new_v4().to_string();
    let uid = state.db.generate_unique_uid().await.map_err(AppError::from)?;
    let fallback_nick: String = session.openid.chars().take(8).collect();
    let nickname = session.nickname.as_deref().unwrap_or(&fallback_nick);
    let rand_str: String = (0..8)
        .map(|_| {
            let idx = rand::thread_rng().gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + (idx - 10)) as char
            }
        })
        .collect();
    let base_username = format!("wx_{}", rand_str);
    let username = ensure_unique_username(state, &base_username).await?;
    let placeholder_email = format!("wx_{}@tokensbyte.local", &uid);
    let password_hash = auth::hash_password(&uuid::Uuid::new_v4().to_string())?;

    let default_group: String = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT group_key FROM user_levels WHERE is_default = 1 LIMIT 1"),
    )
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or_else(|| "default".to_string());

    let initial_balance = state.config.default_user_quota;
    let timezone = new_user_timezone(session.timezone.as_deref());
    sqlx::query(&state.db.format_query(
        r#"INSERT INTO users (id, uid, username, email, password_hash, nickname, wechat_id, wechat_name, role, balance, gift_balance, is_active, referred_by, user_group, referral_history, timezone, register_ip)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'user', ?, ?, 1, ?, ?, ?, ?, ?)"#,
    ))
    .bind(&user_id)
    .bind(&uid)
    .bind(&username)
    .bind(&placeholder_email)
    .bind(&password_hash)
    .bind(nickname)
    .bind(&session.wechat_id)
    .bind(&session.nickname)
    .bind(initial_balance)
    .bind(invite.gift_amount)
    .bind(&invite.referred_by)
    .bind(&default_group)
    .bind(&invite.referral_history)
    .bind(&timezone)
    .bind(raw_ip)
    .execute(&mut *tx)
    .await?;

    if invite.gift_amount > 0.0 {
        let gift_order_no = crate::api::plugins::finance::generate_gift_order_no();
        sqlx::query(&state.db.format_query(
            "INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'gift', ?, 'gift', ?)",
        ))
        .bind(&user_id)
        .bind(invite.gift_amount)
        .bind(&invite.gift_remark)
        .bind(&gift_order_no)
        .execute(&mut *tx)
        .await?;
    }

    credit_inviter_reward_tx(state, &mut tx, &invite).await?;
    tx.commit().await?;

    finalize_oauth_invite_side_effects(state, &user_id, &invite).await?;
    crate::api::users::spawn_oauth_avatar_import(state, &user_id, session.headimgurl.as_deref());

    let query = format!("{USER_WITH_LEVEL_SQL} WHERE u.id = ?");
    Ok(sqlx::query_as(&state.db.format_query(&query))
        .bind(&user_id)
        .fetch_one(&state.db.pool)
        .await?)
}

/// 微信 OAuth — 获取授权 URL 并重定向（兼容旧流程）
pub async fn oauth_wechat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let result = (async {
        let settings = get_all_settings(&state).await?;
        let wechat = settings
            .wechat_oauth
            .ok_or_else(|| AppError::BadRequest("微信授权登录未配置".to_string()))?;
        if wechat.app_id.is_empty() {
            return Err(AppError::BadRequest("微信授权登录未配置".to_string()));
        }
        let req_base_url = get_base_url_from_req(&headers, &state.config.base_url);
        let callback_base = wechat.callback_domain.trim();
        let redirect_base = if !callback_base.is_empty() {
            normalize_origin(callback_base)
        } else {
            req_base_url.clone()
        };
        let redirect_uri = format!("{}/api/v1/auth/oauth/wechat/callback", redirect_base);
        let (aff, team) = invite_from_query_or_cookie(&headers, &params);
        let timezone = pick_nonempty_param(&params, "timezone");
        let intent = pick_nonempty_param(&params, "intent");
        let return_origin = resolve_trusted_oauth_origin(
            params.get("origin").map(|s| s.as_str()).filter(|s| !s.is_empty()),
            &req_base_url,
            &state.config.base_url,
            Some(callback_base),
        )
        .or_else(|| (redirect_base != req_base_url).then(|| req_base_url.clone()));
        let state_val = generate_oauth_state_with_intent(
            &state.config.jwt_secret,
            "wechat",
            aff.as_deref(),
            team.as_deref(),
            timezone.as_deref(),
            return_origin.as_deref(),
            intent.as_deref(),
        );
        let url = crate::services::oauth::OAuthService::wechat_auth_url(
            &wechat.app_id,
            &redirect_uri,
            &state_val,
        );
        Ok::<_, AppError>(url)
    })
    .await;
    match result {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(err) => err.into_response(),
    }
}

/// 微信 OAuth 回调 — 自动注册/登录（支持多账号选择与注册绑定确认）
pub async fn oauth_wechat_callback(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Query(query): Query<OAuthCallbackQuery>,
) -> Response {
    let result = (async {
        let code = query.code.ok_or_else(|| AppError::BadRequest("缺少 code 参数".to_string()))?;
        // CSRF 防护：校验 state，并取出嵌入的邀请参数与中继源
        let state_param = query.state.as_deref().unwrap_or("");
        let Some((state_aff, state_team, state_tz, state_origin, state_intent)) =
            verify_oauth_state_with_intent(&state.config.jwt_secret, "wechat", state_param)
        else {
            return Err(AppError::BadRequest("OAuth state 验证失败，请重新发起授权".to_string()));
        };
        let settings = get_all_settings(&state).await?;
        if !settings.login.enable_wechat_login {
            return Err(AppError::Forbidden("微信登录未开启".to_string()));
        }
        let wechat = settings
            .wechat_oauth
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("微信授权未配置".to_string()))?;

        let info = crate::services::oauth::OAuthService::wechat_exchange(
            &wechat.app_id, &wechat.app_secret, &code
        ).await?;

        let wechat_identifier = info.unionid.as_deref().unwrap_or(&info.openid).to_string();
        let fallback_identifier = &info.openid;

        let raw_ip = extract_client_ip(&headers, &addr);
        check_ip_blacklist(&settings.site, &raw_ip)?;

        // 统一更新已绑定账号的微信标识（兼容旧 openid 升级为 unionid）与最新微信昵称
        sqlx::query(&state.db.format_query(
            "UPDATE users SET wechat_id = ?, wechat_name = ? WHERE wechat_id = ? OR wechat_id = ?",
        ))
        .bind(&wechat_identifier)
        .bind(&info.nickname)
        .bind(&wechat_identifier)
        .bind(fallback_identifier)
        .execute(&state.db.pool)
        .await?;

        let existing_users = load_users_with_level_wechat(&state, &wechat_identifier).await?;
        let session = WechatPendingSession {
            wechat_id: wechat_identifier.clone(),
            openid: info.openid.clone(),
            nickname: info.nickname.clone(),
            headimgurl: info.headimgurl.clone(),
            aff: state_aff,
            team: state_team,
            timezone: state_tz,
        };
        let base_prefix = state_origin
            .filter(|o| !o.is_empty())
            .map(|b| b.trim_end_matches('/').to_string())
            .unwrap_or_default();

        if state_intent.as_deref() == Some("register") && !existing_users.is_empty() {
            let ticket = issue_wechat_session_code(session);
            return Ok::<_, AppError>(format!(
                "{}/register?wechat_ticket={}",
                base_prefix, ticket
            ));
        }

        let user = if existing_users.is_empty() {
            create_wechat_user(&state, &settings, &headers, &session, &raw_ip).await?
        } else {
            let active_users: Vec<User> = existing_users
                .into_iter()
                .filter(|u| u.is_active != 0)
                .collect();
            if active_users.is_empty() {
                return Err(AppError::Forbidden("账号已被禁用".to_string()));
            }
            if active_users.len() > 1 {
                let ticket = issue_wechat_session_code(session);
                return Ok::<_, AppError>(format!(
                    "{}/login?code={}&type=wechat",
                    base_prefix, ticket
                ));
            }
            let mut active_users = active_users;
            active_users.remove(0)
        };

        note_last_active_ip(&state, &user.id, &raw_ip);
        let token = auth::create_token(&user.id, &user.username, &user.role, &state.config.jwt_secret)?;
        // 重定向到前端，附带一次性兑换码（避免 JWT 出现在 URL）
        let code = issue_login_code(&state, token);
        let redirect_url = format!("{}/login?code={}&type=wechat", base_prefix, code);
        Ok::<_, AppError>(redirect_url)
    }).await;

    match result {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(err) => err.into_response(),
    }
}

async fn fetch_user_by_google_id(
    state: &Arc<AppState>,
    google_id: &str,
) -> AppResult<Option<User>> {
    Ok(sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM users WHERE google_id = ?"),
    )
    .bind(google_id)
    .fetch_optional(&state.db.pool)
    .await?)
}

/// 先按谷歌 id 找人；只有已验证邮箱时再按 `email:{地址}` 找回上次「仅邮箱」登录的账号。
async fn google_login_identity(
    state: &Arc<AppState>,
    info: &crate::services::oauth::GoogleUserInfo,
) -> AppResult<(Option<User>, String)> {
    let real_id = {
        let id = info.id.trim();
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    };
    let email_key = info
        .verified_email()
        .map(|email| format!("email:{}", email.to_ascii_lowercase()));

    if let Some(id) = &real_id {
        if let Some(user) = fetch_user_by_google_id(state, id).await? {
            return Ok((Some(user), id.clone()));
        }
    }
    if let Some(key) = &email_key {
        if let Some(user) = fetch_user_by_google_id(state, key).await? {
            let store_id = real_id.unwrap_or_else(|| key.clone());
            return Ok((Some(user), store_id));
        }
    }
    match real_id.or(email_key) {
        Some(id) => Ok((None, id)),
        None => Err(AppError::BadRequest(
            "谷歌未返回已验证邮箱或账号标识".to_string(),
        )),
    }
}

/// 占位邮箱才接受谷歌邮箱。已绑定的真实邮箱不覆盖。
pub(crate) fn unbound_email_from_google(
    current_email: &str,
    incoming: Option<&str>,
) -> Option<String> {
    if !is_placeholder_email(current_email) {
        return None;
    }
    let email = incoming?.trim();
    if !auth::contact::is_real_email(email) {
        return None;
    }
    Some(email.to_string())
}

pub(crate) async fn google_email_to_apply(
    state: &Arc<AppState>,
    registration: &RegistrationSettings,
    current_email: &str,
    incoming: Option<&str>,
    exclude_user_id: Option<&str>,
    apply_whitelist: bool,
) -> AppResult<Option<String>> {
    let Some(email) = unbound_email_from_google(current_email, incoming) else {
        return Ok(None);
    };
    if apply_whitelist
        && registration.email_whitelist_enabled
        && enforce_registration_email_whitelist(registration, &email).is_err()
    {
        return Ok(None);
    }
    let limit = auth::contact::normalize_limit(registration.max_accounts_per_email);
    let occ = auth::contact::email_occupancy(state, &email, exclude_user_id, limit).await?;
    if occ.at_limit() {
        return Ok(None);
    }
    Ok(Some(email))
}

/// 谷歌 OAuth — 获取授权 URL 并重定向
pub async fn oauth_google(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let result = (async {
        let settings = get_all_settings(&state).await?;
        let google = settings
            .google_oauth
            .ok_or_else(|| AppError::BadRequest("谷歌授权登录未配置".to_string()))?;
        if google.client_id.is_empty() {
            return Err(AppError::BadRequest("谷歌授权登录未配置".to_string()));
        }

        let req_base_url = get_base_url_from_req(&headers, &state.config.base_url);
        let redirect_uri = format!("{}/api/v1/auth/oauth/google/callback", req_base_url);
        let (aff, team) = invite_from_query_or_cookie(&headers, &params);
        let timezone = pick_nonempty_param(&params, "timezone");
        let return_origin = resolve_trusted_oauth_origin(
            params.get("origin").map(|s| s.as_str()).filter(|s| !s.is_empty()),
            &req_base_url,
            &state.config.base_url,
            None,
        );
        let state_val = generate_oauth_state(
            &state.config.jwt_secret,
            "google",
            aff.as_deref(),
            team.as_deref(),
            timezone.as_deref(),
            return_origin.as_deref(),
        );
        let url = crate::services::oauth::OAuthService::google_auth_url(
            &google.client_id,
            &redirect_uri,
            &state_val,
        );
        Ok::<_, AppError>(url)
    })
    .await;

    match result {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(err) => err.into_response(),
    }
}

/// 谷歌 OAuth 回调 — 自动注册/登录
pub async fn oauth_google_callback(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Query(query): Query<OAuthCallbackQuery>,
) -> Response {
    let result = (async {
        let code = query.code.ok_or_else(|| AppError::BadRequest("缺少 code 参数".to_string()))?;
        // CSRF 防护：校验 state，并取出嵌入的邀请参数与中继源
        let state_param = query.state.as_deref().unwrap_or("");
        let Some((state_aff, state_team, state_tz, state_origin)) =
            verify_oauth_state(&state.config.jwt_secret, "google", state_param)
        else {
            return Err(AppError::BadRequest("OAuth state 验证失败，请重新发起授权".to_string()));
        };
        let settings = get_all_settings(&state).await?;
        if !settings.login.enable_google_login {
            return Err(AppError::Forbidden("谷歌登录未开启".to_string()));
        }
        let google = settings
            .google_oauth
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("谷歌授权未配置".to_string()))?;

        let req_base_url = get_base_url_from_req(&headers, &state.config.base_url);
        let redirect_uri = format!("{}/api/v1/auth/oauth/google/callback", req_base_url);

        let info = crate::services::oauth::OAuthService::google_exchange(
            &google.client_id, &google.client_secret, &code, &redirect_uri,
        ).await?;

        let (existing, google_id) = google_login_identity(&state, &info).await?;
        let google_display_name = info.name.clone().or_else(|| info.email.clone());
        let verified_email = info.verified_email();

        let raw_ip = extract_client_ip(&headers, &addr);
        check_ip_blacklist(&settings.site, &raw_ip)?;
        let user = if let Some(mut u) = existing {
            if u.is_active == 0 {
                return Err(AppError::Forbidden("账号已被禁用".to_string()));
            }
            let bind_email = google_email_to_apply(
                &state,
                &settings.registration,
                &u.email,
                verified_email.as_deref(),
                Some(&u.id),
                true,
            )
            .await?;
            let google_changed = u.google_id.as_deref() != Some(google_id.as_str());
            if bind_email.is_some() || google_changed {
                sqlx::query(&state.db.format_query(
                    "UPDATE users SET google_id = ?, google_name = ?, email = COALESCE(?, email), updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                ))
                .bind(&google_id)
                .bind(&google_display_name)
                .bind(&bind_email)
                .bind(&u.id)
                .execute(&state.db.pool)
                .await?;
                if let Some(email) = bind_email {
                    u.email = email;
                }
                u.google_id = Some(google_id.clone());
            } else {
                sqlx::query(&state.db.format_query("UPDATE users SET google_name = ? WHERE id = ?"))
                    .bind(&google_display_name).bind(&u.id).execute(&state.db.pool).await?;
            }
            u
        } else {
            let mut tx = state.db.pool.begin().await?;
            let invite =
                resolve_oauth_invite(&state, &settings, &headers, state_aff, state_team, &mut tx)
                    .await?;
            let user_id = uuid::Uuid::new_v4().to_string();
            let uid = state.db.generate_unique_uid().await.map_err(AppError::from)?;
            let id_prefix: String = info.id.chars().take(8).collect();
            let fallback_name = if id_prefix.is_empty() {
                verified_email.clone().unwrap_or_default()
            } else {
                id_prefix
            };
            let raw_name = info.name.as_deref().unwrap_or(&fallback_name);
            let mut clean_name: String = raw_name
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if clean_name.is_empty() {
                clean_name = "google_user".to_string();
            }
            while clean_name.len() < 5 {
                clean_name.push_str(&rand::thread_rng().gen_range(0..10).to_string());
            }
            let username = ensure_unique_username(&state, &clean_name).await?;
            let password_hash = auth::hash_password(&uuid::Uuid::new_v4().to_string())?;
            let actual_email = google_email_to_apply(
                &state,
                &settings.registration,
                "",
                verified_email.as_deref(),
                None,
                true,
            )
            .await?
            .unwrap_or_else(|| format!("g_{}@tokensbyte.local", &uid));

            // 查询默认注册等级
            let default_group: String = sqlx::query_scalar(&state.db.format_query(
                "SELECT group_key FROM user_levels WHERE is_default = 1 LIMIT 1"
            ))
            .fetch_optional(&state.db.pool)
            .await?
            .unwrap_or_else(|| "default".to_string());

            let initial_balance = state.config.default_user_quota;

            let timezone = new_user_timezone(state_tz.as_deref());
            sqlx::query(
                &state.db.format_query(r#"INSERT INTO users (id, uid, username, email, password_hash, nickname, google_id, google_name, role, balance, gift_balance, is_active, referred_by, user_group, referral_history, timezone, register_ip)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'user', ?, ?, 1, ?, ?, ?, ?, ?)"#)
            )
            .bind(&user_id).bind(&uid).bind(&username).bind(&actual_email)
            .bind(&password_hash).bind(raw_name).bind(&google_id).bind(&google_display_name)
            .bind(initial_balance).bind(invite.gift_amount)
            .bind(&invite.referred_by)
            .bind(&default_group)
            .bind(&invite.referral_history)
            .bind(&timezone)
            .bind(&raw_ip)
            .execute(&mut *tx).await?;

            if invite.gift_amount > 0.0 {
                let gift_order_no = crate::api::plugins::finance::generate_gift_order_no();
                sqlx::query(&state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'gift', ?, 'gift', ?)"))
                    .bind(&user_id).bind(invite.gift_amount).bind(&invite.gift_remark).bind(&gift_order_no)
                    .execute(&mut *tx).await?;
            }

            credit_inviter_reward_tx(&state, &mut tx, &invite).await?;
            tx.commit().await?;

            finalize_oauth_invite_side_effects(&state, &user_id, &invite).await?;
            crate::api::users::spawn_oauth_avatar_import(&state, &user_id, info.picture.as_deref());

            sqlx::query_as(&state.db.format_query("SELECT * FROM users WHERE id = ?"))
                .bind(&user_id).fetch_one(&state.db.pool).await?
        };

        note_last_active_ip(&state, &user.id, &extract_client_ip(&headers, &addr));
        let token = auth::create_token(&user.id, &user.username, &user.role, &state.config.jwt_secret)?;
        let code = issue_login_code(&state, token);
        let redirect_url = if let Some(ref base) = state_origin.filter(|o| !o.is_empty()) {
            format!("{}/login?code={}&type=google", base.trim_end_matches('/'), code)
        } else {
            format!("/login?code={}&type=google", code)
        };
        Ok::<_, AppError>(redirect_url)
    }).await;

    match result {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(err) => err.into_response(),
    }
}

// ======================== 内部工具函数 ========================

async fn load_user_by_username_or_uid(
    state: &Arc<AppState>,
    identifier: &str,
) -> AppResult<Option<User>> {
    if identifier.len() == 10 && identifier.chars().all(|c| c.is_ascii_digit()) {
        if let Some(u) = load_user_with_level(state, "u.uid = ?", identifier).await? {
            return Ok(Some(u));
        }
    }
    if let Some(u) = load_user_with_level(state, "u.username = ?", identifier).await? {
        return Ok(Some(u));
    }
    load_user_with_level(state, "u.uid = ?", identifier).await
}

async fn load_user_with_level(
    state: &Arc<AppState>,
    where_sql: &str,
    value: &str,
) -> AppResult<Option<User>> {
    let query = format!("{USER_WITH_LEVEL_SQL} WHERE {where_sql}");
    Ok(sqlx::query_as(&state.db.format_query(&query))
        .bind(value)
        .fetch_optional(&state.db.pool)
        .await?)
}

async fn load_users_with_level_email(state: &Arc<AppState>, email: &str) -> AppResult<Vec<User>> {
    let email = auth::contact::canonical_email(email);
    if !auth::contact::is_real_email(&email) {
        return Ok(Vec::new());
    }
    let query = format!(
        "{USER_WITH_LEVEL_SQL} WHERE LOWER(u.email) = LOWER(?) AND LOWER(u.email) NOT LIKE ? ORDER BY u.created_at ASC, u.uid ASC"
    );
    Ok(sqlx::query_as(&state.db.format_query(&query))
        .bind(&email)
        .bind("%@tokensbyte.local")
        .fetch_all(&state.db.pool)
        .await?)
}

async fn load_users_with_level_mobile(state: &Arc<AppState>, mobile: &str) -> AppResult<Vec<User>> {
    let mobile = auth::contact::canonical_mobile_lenient(mobile);
    if !auth::contact::is_real_mobile(&mobile) {
        return Ok(Vec::new());
    }
    let query = format!(
        "{USER_WITH_LEVEL_SQL} WHERE u.mobile = ? AND COALESCE(u.mobile, '') != '' ORDER BY u.created_at ASC, u.uid ASC"
    );
    Ok(sqlx::query_as(&state.db.format_query(&query))
        .bind(&mobile)
        .fetch_all(&state.db.pool)
        .await?)
}

async fn load_users_with_level_wechat(
    state: &Arc<AppState>,
    wechat_id: &str,
) -> AppResult<Vec<User>> {
    if !auth::contact::is_real_wechat(wechat_id) {
        return Ok(Vec::new());
    }
    let query = format!(
        "{USER_WITH_LEVEL_SQL} WHERE u.wechat_id = ? AND COALESCE(u.wechat_id, '') != '' ORDER BY u.created_at ASC, u.uid ASC"
    );
    Ok(sqlx::query_as(&state.db.format_query(&query))
        .bind(wechat_id)
        .fetch_all(&state.db.pool)
        .await?)
}

async fn load_site_login_users(
    state: &Arc<AppState>,
    identifier: &str,
    login_type: Option<&str>,
) -> AppResult<Vec<User>> {
    match login_type {
        Some("email") => load_users_with_level_email(state, identifier).await,
        Some("mobile") => load_users_with_level_mobile(state, identifier).await,
        _ => {
            if let Some(user) = load_user_with_level(state, "u.username = ?", identifier).await? {
                return Ok(vec![user]);
            }
            if let Some(user) = load_user_with_level(state, "u.uid = ?", identifier).await? {
                return Ok(vec![user]);
            }
            if identifier.contains('@') {
                return load_users_with_level_email(state, identifier).await;
            }
            Ok(Vec::new())
        }
    }
}

async fn get_all_settings(state: &Arc<AppState>) -> AppResult<AllSettings> {
    crate::api::settings::load_all_settings(state).await
}

/// 用户名合规校验：仅限英文字母和数字，至少6个字符，并且包含敏感词过滤
pub(crate) fn validate_username(username: &str, is_register: bool) -> AppResult<()> {
    let name = username.trim();

    if name.len() < 5 {
        return Err(AppError::BadRequest(
            "用户名长度不能少于 5 个字符".to_string(),
        ));
    }
    if name.len() > 48 {
        return Err(AppError::BadRequest(
            "正确输入用户名限制为 48 字".to_string(),
        ));
    }

    if name.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::BadRequest("用户名不能为纯数字".to_string()));
    }

    // 支持英文字母、数字、下划线，或合规邮箱格式（放开 '@'、'.'、'-'）
    let is_valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '.' | '-'));
    if !is_valid
        || (name.contains('@')
            && (!name.contains('.') || name.starts_with('@') || name.ends_with('@')))
    {
        return Err(AppError::BadRequest(
            "用户名只能包含英文字母、数字、下划线或合规邮箱地址".to_string(),
        ));
    }

    if is_register {
        // 包含即拒绝的敏感词/保留字（模糊匹配）
        const CONTAINS_RESERVED: &[&str] = &[
            "admin",
            "root",
            "system",
            "superadmin",
            "moderator",
            "support",
            "official",
            "anonymous",
            "tokensbyte",
            "security",
            "noreply",
            "select",
            "update",
            "delete",
            "insert",
            "drop",
            "database",
        ];

        // 精确匹配的保留字（较短的词，防止模糊匹配误伤正常单词）
        const EXACT_RESERVED: &[&str] = &[
            "sys",
            "super",
            "master",
            "operator",
            "mod",
            "staff",
            "help",
            "service",
            "test",
            "tester",
            "testing",
            "demo",
            "guest",
            "nobody",
            "null",
            "undefined",
            "api",
            "www",
            "mail",
            "ftp",
            "smtp",
            "pop",
            "imap",
            "dns",
            "ns",
            "server",
            "db",
            "mysql",
            "postgres",
            "redis",
            "mongo",
            "nginx",
            "apache",
            "proxy",
            "bot",
            "robot",
            "crawler",
            "spider",
            "info",
            "ceo",
            "cto",
            "cfo",
            "coo",
            "token",
        ];

        let lower = name.to_lowercase();

        for &word in CONTAINS_RESERVED {
            if lower.contains(word) {
                return Err(AppError::BadRequest(
                    "此用户名已被保留，无法使用".to_string(),
                ));
            }
        }

        for &word in EXACT_RESERVED {
            if lower == word {
                return Err(AppError::BadRequest(
                    "此用户名已被保留，无法使用".to_string(),
                ));
            }
        }
    }

    Ok(())
}

/// 生成 6 位数字验证码
fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    (0..6).map(|_| rng.gen_range(0..10).to_string()).collect()
}

#[derive(Clone, Copy)]
enum CodeChannel {
    Email,
    Sms,
}

/// 占位邮箱（用户名/手机/OAuth 注册产生），不可用于找回密码
pub(crate) fn is_placeholder_email(email: &str) -> bool {
    let email = email.trim();
    email.is_empty() || email.to_ascii_lowercase().ends_with("@tokensbyte.local")
}

fn validate_code_purpose(purpose: &str, channel: CodeChannel) -> AppResult<()> {
    let allowed = match channel {
        CodeChannel::Email => matches!(purpose, "register" | "reset_password" | "bind_email"),
        CodeChannel::Sms => matches!(purpose, "register" | "reset_password" | "bind_mobile"),
    };
    if !allowed {
        return Err(AppError::BadRequest("无效的验证码用途".to_string()));
    }
    Ok(())
}

fn extract_bearer_claims(
    headers: &axum::http::HeaderMap,
    jwt_secret: &str,
) -> AppResult<auth::Claims> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized)?;
    let token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .map(str::trim)
        .ok_or_else(|| AppError::Unauthorized)?;
    auth::validate_token(token, jwt_secret).map_err(|_| AppError::Unauthorized)
}

fn validate_code_purpose_and_auth(
    headers: &axum::http::HeaderMap,
    jwt_secret: &str,
    purpose: &str,
    channel: CodeChannel,
) -> AppResult<()> {
    validate_code_purpose(purpose, channel)?;
    if matches!(purpose, "bind_email" | "bind_mobile") {
        let _claims = extract_bearer_claims(headers, jwt_secret)?;
    }
    Ok(())
}

fn code_send_key(purpose: &str, target: &str) -> String {
    format!("{purpose}:{target}")
}

async fn lock_key_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    db: &crate::db::Database,
    key: &str,
) -> AppResult<()> {
    sqlx::query(&db.format_query("SELECT pg_advisory_xact_lock(hashtext(?))"))
        .bind(key)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn configured_username_ip_limit(reg: &RegistrationSettings) -> Option<i64> {
    reg.username_ip_daily_limit.filter(|&n| n >= 1)
}

fn username_ip_daily_limit(reg: &RegistrationSettings, has_reward: bool) -> Option<i64> {
    if let Some(limit) = configured_username_ip_limit(reg) {
        Some(limit)
    } else if reg.ip_rate_limit_enabled {
        Some(reg.ip_daily_limit.max(1))
    } else if has_reward {
        Some(crate::models::RegistrationSettings::default().ip_daily_limit.max(1))
    } else {
        None
    }
}

async fn enforce_username_ip_daily_limit_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    db: &crate::db::Database,
    settings: &AllSettings,
    ip: &str,
    limit: i64,
) -> AppResult<()> {
    if registration_ip_whitelisted(&settings.site, ip) {
        return Ok(());
    }
    lock_key_tx(tx, db, &format!("ip_reg:{ip}")).await?;
    let (day_start, day_end) = today_bounds_db_ts(site_timedisplay(settings));
    let count: i64 = sqlx::query_scalar(&db.format_query(
        "SELECT COUNT(*) FROM users WHERE register_ip = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz AND COALESCE(mobile, '') = '' AND COALESCE(wechat_id, '') = '' AND COALESCE(google_id, '') = '' AND email LIKE 'u_%@tokensbyte.local'",
    ))
    .bind(ip)
    .bind(&day_start)
    .bind(&day_end)
    .fetch_one(&mut **tx)
    .await?;

    if count >= limit {
        return Err(AppError::Forbidden(format!(
            "当前 IP 直接注册用户名已达上限 ({limit})，请采取其他注册方式",
        )));
    }
    Ok(())
}

async fn enforce_ip_daily_limit_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    db: &crate::db::Database,
    settings: &AllSettings,
    ip: &str,
    limit: i64,
) -> AppResult<()> {
    if registration_ip_whitelisted(&settings.site, ip) {
        return Ok(());
    }
    lock_key_tx(tx, db, &format!("ip_reg:{ip}")).await?;
    let (day_start, day_end) = today_bounds_db_ts(site_timedisplay(settings));
    let sql = if configured_username_ip_limit(&settings.registration).is_some() {
        "SELECT COUNT(*) FROM users WHERE register_ip = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz AND COALESCE(mobile, '') = '' AND COALESCE(wechat_id, '') = '' AND COALESCE(google_id, '') = '' AND email NOT LIKE 'u_%@tokensbyte.local'"
    } else {
        "SELECT COUNT(*) FROM users WHERE register_ip = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz AND COALESCE(mobile, '') = '' AND COALESCE(wechat_id, '') = '' AND COALESCE(google_id, '') = ''"
    };
    let count: i64 = sqlx::query_scalar(&db.format_query(sql))
        .bind(ip)
        .bind(&day_start)
        .bind(&day_end)
        .fetch_one(&mut **tx)
        .await?;

    if count >= limit {
        return Err(AppError::Forbidden(format!(
            "当日注册次数已达上限 ({limit})",
        )));
    }
    Ok(())
}

async fn check_code_send_cooldown(
    state: &Arc<AppState>,
    email: &str,
    phone: &str,
    purpose: &str,
) -> AppResult<()> {
    let since = DbTs::from_utc(Utc::now() - Duration::seconds(60));
    let count: i64 = if !email.is_empty() {
        let email = auth::contact::canonical_email(email);
        sqlx::query_scalar(&state.db.format_query(
            "SELECT COUNT(*) FROM verification_codes WHERE LOWER(email) = LOWER(?) AND purpose = ? AND created_at >= ?::timestamptz",
        ))
        .bind(&email)
        .bind(purpose)
        .bind(&since)
        .fetch_one(&state.db.pool)
        .await?
    } else {
        let phone = auth::contact::canonical_mobile_lenient(phone);
        sqlx::query_scalar(&state.db.format_query(
            "SELECT COUNT(*) FROM verification_codes WHERE phone = ? AND purpose = ? AND created_at >= ?::timestamptz",
        ))
        .bind(&phone)
        .bind(purpose)
        .bind(&since)
        .fetch_one(&state.db.pool)
        .await?
    };
    if count > 0 {
        return Err(AppError::BadRequest(
            "验证码发送过于频繁，请稍后再试".to_string(),
        ));
    }
    Ok(())
}

async fn email_is_bound_for_recovery(state: &Arc<AppState>, email: &str) -> AppResult<bool> {
    let email = auth::contact::canonical_email(email);
    if is_placeholder_email(&email) {
        return Ok(false);
    }
    let count: i64 = sqlx::query_scalar(&state.db.format_query(
        "SELECT COUNT(*) FROM users WHERE LOWER(email) = LOWER(?) AND LOWER(email) NOT LIKE ?",
    ))
    .bind(&email)
    .bind("%@tokensbyte.local")
    .fetch_one(&state.db.pool)
    .await?;
    Ok(count >= 1)
}

async fn mobile_is_bound_for_recovery(state: &Arc<AppState>, mobile: &str) -> AppResult<bool> {
    let mobile = auth::contact::canonical_mobile_lenient(mobile);
    if mobile.is_empty() {
        return Ok(false);
    }
    let count: i64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT COUNT(*) FROM users WHERE mobile = ? AND COALESCE(mobile, '') != ''"),
    )
    .bind(&mobile)
    .fetch_one(&state.db.pool)
    .await?;
    Ok(count >= 1)
}

/// 保存验证码到数据库（先清理同目标同用途旧码；有效期见 VERIFICATION_CODE_EXPIRY_MINUTES）
async fn save_verification_code(
    state: &Arc<AppState>,
    email: &str,
    phone: &str,
    code: &str,
    purpose: &str,
) -> AppResult<()> {
    let email = auth::contact::canonical_email(email);
    let phone = auth::contact::canonical_mobile_lenient(phone);
    if !email.is_empty() {
        sqlx::query(
            &state
                .db
                .format_query("DELETE FROM verification_codes WHERE LOWER(email) = LOWER(?) AND purpose = ?"),
        )
        .bind(&email)
        .bind(purpose)
        .execute(&state.db.pool)
        .await?;
    } else if !phone.is_empty() {
        sqlx::query(
            &state
                .db
                .format_query("DELETE FROM verification_codes WHERE phone = ? AND purpose = ?"),
        )
        .bind(&phone)
        .bind(purpose)
        .execute(&state.db.pool)
        .await?;
    }

    let expires_at = DbTs::from_utc(
        Utc::now()
            + Duration::minutes(crate::models::verification::VERIFICATION_CODE_EXPIRY_MINUTES),
    );
    sqlx::query(
        &state.db.format_query(
            "INSERT INTO verification_codes (email, phone, code, purpose, expires_at, attempts) VALUES (?, ?, ?, ?, ?, 0)",
        ),
    )
    .bind(&email)
    .bind(&phone)
    .bind(code)
    .bind(purpose)
    .bind(&expires_at)
    .execute(&state.db.pool)
    .await?;
    Ok(())
}

/// 校验并消费邮箱验证码（pub 版本供 user.rs 调用）
pub async fn verify_email_code_pub(
    state: &Arc<AppState>,
    email: &str,
    code: &str,
    purpose: &str,
) -> AppResult<()> {
    verify_email_code(state, email, code, purpose, true).await
}

/// 预检邮箱验证码（不消费，供换绑/预校验使用）
pub async fn peek_email_code_pub(
    state: &Arc<AppState>,
    email: &str,
    code: &str,
    purpose: &str,
) -> AppResult<()> {
    verify_email_code(state, email, code, purpose, false).await
}

/// 校验并消费短信验证码（pub 版本供 user.rs 调用）
pub async fn verify_sms_code_pub(
    state: &Arc<AppState>,
    phone: &str,
    code: &str,
    purpose: &str,
) -> AppResult<()> {
    verify_sms_code(state, phone, code, purpose, true).await
}

/// 预检短信验证码（不消费，供换绑/预校验使用）
pub async fn peek_sms_code_pub(
    state: &Arc<AppState>,
    phone: &str,
    code: &str,
    purpose: &str,
) -> AppResult<()> {
    verify_sms_code(state, phone, code, purpose, false).await
}

/// 校验邮箱验证码（失败累计 attempts，超过上限立即失效）
async fn verify_email_code(
    state: &Arc<AppState>,
    email: &str,
    code: &str,
    purpose: &str,
    consume: bool,
) -> AppResult<()> {
    let email = auth::contact::canonical_email(email);
    let row: Option<(i64, String, i64)> = sqlx::query_as(&state.db.format_query(
        "SELECT id, code, COALESCE(attempts, 0) FROM verification_codes \
             WHERE LOWER(email) = LOWER(?) AND purpose = ? AND expires_at > NOW() \
             ORDER BY created_at DESC LIMIT 1",
    ))
    .bind(&email)
    .bind(purpose)
    .fetch_optional(&state.db.pool)
    .await?;

    let Some((id, stored_code, attempts)) = row else {
        let exists: Option<(i64,)> = sqlx::query_as(&state.db.format_query(
            "SELECT 1 FROM verification_codes WHERE LOWER(email) = LOWER(?) AND purpose = ? LIMIT 1",
        ))
        .bind(&email)
        .bind(purpose)
        .fetch_optional(&state.db.pool)
        .await?;
        if exists.is_some() {
            return Err(AppError::BadRequest("邮箱验证码已过期".to_string()));
        }
        return Err(AppError::BadRequest("邮箱验证码无效".to_string()));
    };

    if attempts >= MAX_CODE_ATTEMPTS {
        sqlx::query(
            &state
                .db
                .format_query("DELETE FROM verification_codes WHERE id = ?"),
        )
        .bind(id)
        .execute(&state.db.pool)
        .await?;
        return Err(AppError::BadRequest(
            "验证码尝试次数过多，请重新获取".to_string(),
        ));
    }

    if !ct_eq_str(&stored_code, code) {
        let new_attempts: i64 = sqlx::query_scalar(
            &state.db.format_query(
                "UPDATE verification_codes SET attempts = COALESCE(attempts, 0) + 1 WHERE id = ? RETURNING attempts",
            ),
        )
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await?
        .unwrap_or(MAX_CODE_ATTEMPTS);
        if new_attempts >= MAX_CODE_ATTEMPTS {
            sqlx::query(
                &state
                    .db
                    .format_query("DELETE FROM verification_codes WHERE id = ?"),
            )
            .bind(id)
            .execute(&state.db.pool)
            .await?;
            return Err(AppError::BadRequest(
                "验证码尝试次数过多，请重新获取".to_string(),
            ));
        }
        return Err(AppError::BadRequest("邮箱验证码无效".to_string()));
    }

    if consume {
        let res = sqlx::query(
            &state.db.format_query(
                "DELETE FROM verification_codes WHERE id = ? AND COALESCE(attempts, 0) < ?",
            ),
        )
        .bind(id)
        .bind(MAX_CODE_ATTEMPTS)
        .execute(&state.db.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::BadRequest("邮箱验证码无效".to_string()));
        }
    }
    Ok(())
}

/// 校验短信验证码（失败累计 attempts，超过上限立即失效）
async fn verify_sms_code(
    state: &Arc<AppState>,
    phone: &str,
    code: &str,
    purpose: &str,
    consume: bool,
) -> AppResult<()> {
    let phone = auth::contact::canonical_mobile_lenient(phone);
    let row: Option<(i64, String, i64)> = sqlx::query_as(&state.db.format_query(
        "SELECT id, code, COALESCE(attempts, 0) FROM verification_codes \
             WHERE phone = ? AND purpose = ? AND expires_at > NOW() \
             ORDER BY created_at DESC LIMIT 1",
    ))
    .bind(&phone)
    .bind(purpose)
    .fetch_optional(&state.db.pool)
    .await?;

    let Some((id, stored_code, attempts)) = row else {
        let exists: Option<(i64,)> = sqlx::query_as(&state.db.format_query(
            "SELECT 1 FROM verification_codes WHERE phone = ? AND purpose = ? LIMIT 1",
        ))
        .bind(&phone)
        .bind(purpose)
        .fetch_optional(&state.db.pool)
        .await?;
        if exists.is_some() {
            return Err(AppError::BadRequest("短信验证码已过期".to_string()));
        }
        return Err(AppError::BadRequest("短信验证码无效".to_string()));
    };

    if attempts >= MAX_CODE_ATTEMPTS {
        sqlx::query(
            &state
                .db
                .format_query("DELETE FROM verification_codes WHERE id = ?"),
        )
        .bind(id)
        .execute(&state.db.pool)
        .await?;
        return Err(AppError::BadRequest(
            "验证码尝试次数过多，请重新获取".to_string(),
        ));
    }

    if !ct_eq_str(&stored_code, code) {
        let new_attempts: i64 = sqlx::query_scalar(
            &state.db.format_query(
                "UPDATE verification_codes SET attempts = COALESCE(attempts, 0) + 1 WHERE id = ? RETURNING attempts",
            ),
        )
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await?
        .unwrap_or(MAX_CODE_ATTEMPTS);
        if new_attempts >= MAX_CODE_ATTEMPTS {
            sqlx::query(
                &state
                    .db
                    .format_query("DELETE FROM verification_codes WHERE id = ?"),
            )
            .bind(id)
            .execute(&state.db.pool)
            .await?;
            return Err(AppError::BadRequest(
                "验证码尝试次数过多，请重新获取".to_string(),
            ));
        }
        return Err(AppError::BadRequest("短信验证码无效".to_string()));
    }

    if consume {
        let res = sqlx::query(
            &state.db.format_query(
                "DELETE FROM verification_codes WHERE id = ? AND COALESCE(attempts, 0) < ?",
            ),
        )
        .bind(id)
        .bind(MAX_CODE_ATTEMPTS)
        .execute(&state.db.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::BadRequest("短信验证码无效".to_string()));
        }
    }
    Ok(())
}

static LAST_ACTIVE_IP_SEEN: std::sync::LazyLock<dashmap::DashMap<String, String>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

/// 记下用户最近一次出现的客户端 IP。同一 IP 不重复写库。
pub(crate) fn note_last_active_ip(state: &Arc<AppState>, user_id: &str, ip: &str) {
    let ip = ip.trim();
    if user_id.is_empty() || ip.is_empty() {
        return;
    }
    if LAST_ACTIVE_IP_SEEN
        .get(user_id)
        .is_some_and(|prev| prev.as_str() == ip)
    {
        return;
    }
    LAST_ACTIVE_IP_SEEN.insert(user_id.to_string(), ip.to_string());
    let state = Arc::clone(state);
    let user_id = user_id.to_string();
    let ip = ip.to_string();
    tokio::spawn(async move {
        let result = sqlx::query(&state.db.format_query(
            "UPDATE users SET last_active_ip = ? WHERE id = ? AND last_active_ip <> ?",
        ))
        .bind(&ip)
        .bind(&user_id)
        .bind(&ip)
        .execute(&state.db.pool)
        .await;
        if let Err(err) = result {
            tracing::warn!("记录最后活跃 IP 失败 user={user_id}: {err}");
            LAST_ACTIVE_IP_SEEN.remove(&user_id);
        }
    });
}

fn header_ip<'a>(headers: &'a axum::http::HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.parse::<std::net::IpAddr>().is_ok())
}

fn forwarded_ips(headers: &axum::http::HeaderMap) -> Vec<&str> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.parse::<std::net::IpAddr>().is_ok())
                .collect()
        })
        .unwrap_or_default()
}

/// 判定是否为公网 IP（非私网、非回环、非链路本地、非广播/未指定）
fn is_public_ip(raw: &str) -> bool {
    let Ok(ip) = raw.parse::<std::net::IpAddr>() else {
        return false;
    };
    match to_canonical_ip(ip) {
        std::net::IpAddr::V4(v4) => {
            !v4.is_loopback()
                && !v4.is_unspecified()
                && !v4.is_link_local()
                && !v4.is_broadcast()
                && !v4.is_private()
        }
        std::net::IpAddr::V6(v6) => {
            !v6.is_loopback()
                && !v6.is_unspecified()
                && !v6.is_unicast_link_local()
                && (v6.octets()[0] & 0xfe != 0xfc)
        }
    }
}

/// 判定是否为局域网/内网环境可用的有效客户端 IP（排除 127.0.0.1、0.0.0.0、链路本地等纯回环或无效地址）
fn is_usable_lan_ip(raw: &str) -> bool {
    let Ok(ip) = raw.parse::<std::net::IpAddr>() else {
        return false;
    };
    match to_canonical_ip(ip) {
        std::net::IpAddr::V4(v4) => {
            !v4.is_loopback() && !v4.is_unspecified() && !v4.is_link_local() && !v4.is_broadcast()
        }
        std::net::IpAddr::V6(v6) => {
            !v6.is_loopback() && !v6.is_unspecified() && !v6.is_unicast_link_local()
        }
    }
}

fn trust_cf_connecting_ip() -> bool {
    std::env::var("TRUST_CF_CONNECTING_IP")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 从请求头中提取客户端 IP：
/// 1. 显式开启 TRUST_CF_CONNECTING_IP=true 时才信任 CF-Connecting-IP（且必须为合法公网 IP）；
/// 2. 公网生产环境第一优先：X-Real-IP 若是公网 IP 则直接采纳（外层反代设定的不可伪造客户端地址）；
/// 3. 从右向左遍历 X-Forwarded-For：自动跳过内部容器/反代私网跳板，取最右侧的真实公网 IP（且杜绝客户端伪造左侧）；
/// 4. 纯内网办公/局域网私有化环境（全链路无公网 IP）：X-Forwarded-For 最左侧即原始发起端客户端；否则回退至可用的 X-Real-IP；
/// 5. 极端兜底回退。
pub(crate) fn parse_client_ip_from_headers(headers: &axum::http::HeaderMap) -> Option<&str> {
    if trust_cf_connecting_ip() {
        let cf = header_ip(headers, "cf-connecting-ip");
        if cf.is_some_and(is_public_ip) {
            return cf;
        }
    }

    let real = header_ip(headers, "x-real-ip");
    let forwarded = forwarded_ips(headers);

    // 1. 公网优先：X-Real-IP 若是公网 IP 则直接采纳
    if let Some(ip) = real.filter(|s| is_public_ip(s)) {
        return Some(ip);
    }

    // 2. 从右向左寻找 X-Forwarded-For 中首个公网 IP（跳过所有 Docker/K8s/反代跳板，防左侧客户端伪造）
    if let Some(ip) = forwarded.iter().rev().copied().find(|s| is_public_ip(s)) {
        return Some(ip);
    }

    // 3. 纯内网办公/局域网私有化环境（全链路无公网 IP）：
    // 取 X-Forwarded-For 最左侧的原始局域网客户端；否则回退至可用的 X-Real-IP
    if let Some(ip) = forwarded.iter().copied().find(|s| is_usable_lan_ip(s)) {
        return Some(ip);
    }
    if let Some(ip) = real.filter(|s| is_usable_lan_ip(s)) {
        return Some(ip);
    }

    // 4. 极端兜底
    real.or_else(|| forwarded.last().copied())
}

/// 提取客户端 IP：当直连对端为本地/私网反代（或开启 CF 信任）时解析反代头，公网直连时使用 socket IP 防伪造。
pub(crate) fn extract_client_ip(
    headers: &axum::http::HeaderMap,
    addr: &std::net::SocketAddr,
) -> String {
    let peer_ip = to_canonical_ip(addr.ip());
    if is_public_ip(&peer_ip.to_string()) && !trust_cf_connecting_ip() {
        return peer_ip.to_string();
    }
    parse_client_ip_from_headers(headers)
        .and_then(|s| s.parse::<std::net::IpAddr>().ok())
        .map(to_canonical_ip)
        .unwrap_or(peer_ip)
        .to_string()
}

/// 签发一次性登录兑换码（默认 60 秒有效），避免 JWT 出现在 URL。
pub(crate) fn issue_login_code(state: &AppState, jwt: String) -> String {
    let code = uuid::Uuid::new_v4().simple().to_string();
    let expires = std::time::Instant::now() + std::time::Duration::from_secs(60);
    state.login_codes.insert(code.clone(), (jwt, expires));
    // 顺带清理少量过期项，避免无限增长
    if state.login_codes.len() > 2048 {
        let now = std::time::Instant::now();
        state.login_codes.retain(|_, (_, exp)| *exp > now);
    }
    code
}

#[derive(Debug, serde::Deserialize)]
pub struct ExchangeLoginCodeRequest {
    pub code: String,
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub confirm_new_account: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct ExchangeLoginCodeResponse {
    pub token: String,
}

/// 用一次性 code 换取 JWT（支持微信多账号选择与注册确认）
pub async fn exchange_login_code(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ExchangeLoginCodeRequest>,
) -> Response {
    let code = req.code.trim();
    if code.is_empty() {
        return AppError::BadRequest("兑换码不能为空".into()).into_response();
    }
    if let Some(entry) = WECHAT_OAUTH_SESSIONS.get(code).map(|r| r.clone()) {
        let (session, exp) = entry;
        if std::time::Instant::now() > exp {
            WECHAT_OAUTH_SESSIONS.remove(code);
            return AppError::AuthFailed("微信授权已过期，请重新扫码".into()).into_response();
        }
        let result = (async {
            let settings = get_all_settings(&state).await?;
            if !settings.login.enable_wechat_login {
                WECHAT_OAUTH_SESSIONS.remove(code);
                return Err(AppError::Forbidden("微信登录未开启".to_string()));
            }
            let raw_ip = extract_client_ip(&headers, &addr);
            check_ip_blacklist(&settings.site, &raw_ip)?;

            if req.intent.as_deref() == Some("register") {
                let wechat_limit = auth::contact::normalize_limit(
                    settings.registration.max_accounts_per_wechat,
                );
                let occ = auth::contact::wechat_occupancy(
                    &state,
                    &session.wechat_id,
                    None,
                    wechat_limit,
                )
                .await?;
                auth::contact::gate_shared_contact(occ, req.confirm_new_account, "微信")?;

                WECHAT_OAUTH_SESSIONS.remove(code);
                let mut user =
                    create_wechat_user(&state, &settings, &headers, &session, &raw_ip).await?;
                if user.role == "admin" {
                    crate::admin_permission::hydrate_user_admin_permissions(&state, &mut user)
                        .await?;
                }
                sanitize_user_for_client(&mut user);
                note_last_active_ip(&state, &user.id, &raw_ip);
                let token = auth::create_token(
                    &user.id,
                    &user.username,
                    &user.role,
                    &state.config.jwt_secret,
                )?;
                return Ok::<Response, AppError>(
                    Json(LoginResponse { token, user }).into_response(),
                );
            }

            let users = load_users_with_level_wechat(&state, &session.wechat_id).await?;
            let active_matches: Vec<User> =
                users.into_iter().filter(|u| u.is_active != 0).collect();
            if active_matches.is_empty() {
                WECHAT_OAUTH_SESSIONS.remove(code);
                return Err(AppError::Forbidden("账号已被禁用".to_string()));
            }
            let selected_uid = req
                .uid
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let mut user = if let Some(uid) = selected_uid {
                active_matches
                    .into_iter()
                    .find(|u| u.uid == uid)
                    .ok_or_else(|| AppError::BadRequest("所选账号与该微信不匹配".to_string()))?
            } else if active_matches.len() == 1 {
                let mut matches = active_matches;
                matches.remove(0)
            } else {
                let candidates = active_matches.iter().map(LoginCandidate::from).collect();
                return Ok(Json(LoginSelectResponse {
                    need_select: true,
                    candidates,
                })
                .into_response());
            };

            WECHAT_OAUTH_SESSIONS.remove(code);
            if user.role == "admin" {
                crate::admin_permission::hydrate_user_admin_permissions(&state, &mut user).await?;
            }
            sanitize_user_for_client(&mut user);
            note_last_active_ip(&state, &user.id, &raw_ip);
            let token = auth::create_token(
                &user.id,
                &user.username,
                &user.role,
                &state.config.jwt_secret,
            )?;
            Ok(Json(LoginResponse { token, user }).into_response())
        })
        .await;
        return match result {
            Ok(resp) => resp,
            Err(err) => err.into_response(),
        };
    }
    let Some((_, (jwt, exp))) = state.login_codes.remove(code) else {
        return AppError::AuthFailed("登录兑换码无效或已使用".into()).into_response();
    };
    if std::time::Instant::now() > exp {
        return AppError::AuthFailed("登录兑换码已过期".into()).into_response();
    }
    Json(ExchangeLoginCodeResponse { token: jwt }).into_response()
}

fn to_canonical_ip(ip: std::net::IpAddr) -> std::net::IpAddr {
    match ip {
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                std::net::IpAddr::V4(v4)
            } else {
                std::net::IpAddr::V6(v6)
            }
        }
        v4 => v4,
    }
}

fn client_ip_in_entries(client_ip: &str, entries: &[String]) -> bool {
    let parsed_ip: std::net::IpAddr = match client_ip.trim().parse() {
        Ok(addr) => addr,
        Err(_) => return false,
    };
    let client_ip_addr = to_canonical_ip(parsed_ip);
    entries.iter().any(|raw_item| {
        let item_str = raw_item.trim();
        if item_str.is_empty() {
            return false;
        }
        if let Some((ip_part, mask_part)) = item_str.split_once('/') {
            let parse_net: Result<std::net::IpAddr, _> = ip_part.trim().parse();
            let parse_prefix: Result<u8, _> = mask_part.trim().parse();
            if let (Ok(net_ip), Ok(prefix_len)) = (parse_net, parse_prefix) {
                return ip_in_cidr(client_ip_addr, to_canonical_ip(net_ip), prefix_len);
            }
            false
        } else {
            item_str
                .parse::<std::net::IpAddr>()
                .is_ok_and(|listed| client_ip_addr == to_canonical_ip(listed))
        }
    })
}

/// 注册 IP 白名单：命中后不受黑名单与当日注册次数限制
pub(crate) fn registration_ip_whitelisted(site_settings: &SiteSettings, client_ip: &str) -> bool {
    site_settings.ip_whitelist_enabled
        && !site_settings.ip_whitelist.is_empty()
        && client_ip_in_entries(client_ip, &site_settings.ip_whitelist)
}

/// 检查 IP 是否在站点 IP 黑名单中（支持单 IP 及 CIDR 网段）
pub(crate) fn check_ip_blacklist(site_settings: &SiteSettings, client_ip: &str) -> AppResult<()> {
    if registration_ip_whitelisted(site_settings, client_ip) {
        return Ok(());
    }
    if !site_settings.ip_blacklist_enabled || site_settings.ip_blacklist.is_empty() {
        return Ok(());
    }
    if client_ip_in_entries(client_ip, &site_settings.ip_blacklist) {
        return Err(AppError::Forbidden("当前 IP 已被限制".to_string()));
    }
    Ok(())
}

fn ip_in_cidr(ip: std::net::IpAddr, net_ip: std::net::IpAddr, prefix_len: u8) -> bool {
    match (ip, net_ip) {
        (std::net::IpAddr::V4(ip4), std::net::IpAddr::V4(net4)) => {
            if prefix_len > 32 {
                return false;
            }
            if prefix_len == 0 {
                return true;
            }
            let mask = u32::MAX.checked_shl(32 - prefix_len as u32).unwrap_or(0);
            (u32::from(ip4) & mask) == (u32::from(net4) & mask)
        }
        (std::net::IpAddr::V6(ip6), std::net::IpAddr::V6(net6)) => {
            if prefix_len > 128 {
                return false;
            }
            if prefix_len == 0 {
                return true;
            }
            let mask = u128::MAX.checked_shl(128 - prefix_len as u32).unwrap_or(0);
            (u128::from(ip6) & mask) == (u128::from(net6) & mask)
        }
        _ => false,
    }
}

/// 用户名直接注册独立 IP 限制预检（仅统计直接用户名注册，超限提示改用其他方式注册）
async fn check_username_ip_rate_limit(
    state: &Arc<AppState>,
    settings: &AllSettings,
    ip: &str,
    limit: i64,
) -> AppResult<()> {
    if registration_ip_whitelisted(&settings.site, ip) {
        return Ok(());
    }
    let (day_start, day_end) = today_bounds_db_ts(site_timedisplay(settings));
    let count: i64 = sqlx::query_scalar(&state.db.format_query(
        "SELECT COUNT(*) FROM users WHERE register_ip = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz AND COALESCE(mobile, '') = '' AND COALESCE(wechat_id, '') = '' AND COALESCE(google_id, '') = '' AND email LIKE 'u_%@tokensbyte.local'",
    ))
    .bind(ip)
    .bind(&day_start)
    .bind(&day_end)
    .fetch_one(&state.db.pool)
    .await?;

    if count >= limit {
        return Err(AppError::Forbidden(format!(
            "当前 IP 直接注册用户名已达上限 ({limit})，请采取其他注册方式"
        )));
    }
    Ok(())
}

/// IP 注册防刷：只统计、只拦截用户名和邮箱注册。手机号、微信、谷歌不占当日次数。
async fn check_ip_rate_limit(
    state: &Arc<AppState>,
    reg: &RegistrationSettings,
    ip: &str,
) -> AppResult<()> {
    if !reg.ip_rate_limit_enabled {
        return Ok(());
    }
    let settings = crate::api::settings::load_all_settings(state).await?;
    if registration_ip_whitelisted(&settings.site, ip) {
        return Ok(());
    }
    let (day_start, day_end) = today_bounds_db_ts(site_timedisplay(&settings));
    let sql = if configured_username_ip_limit(reg).is_some() {
        "SELECT COUNT(*) FROM users WHERE register_ip = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz AND COALESCE(mobile, '') = '' AND COALESCE(wechat_id, '') = '' AND COALESCE(google_id, '') = '' AND email NOT LIKE 'u_%@tokensbyte.local'"
    } else {
        "SELECT COUNT(*) FROM users WHERE register_ip = ? AND created_at >= ?::timestamptz AND created_at < ?::timestamptz AND COALESCE(mobile, '') = '' AND COALESCE(wechat_id, '') = '' AND COALESCE(google_id, '') = ''"
    };
    let count: i64 = sqlx::query_scalar(&state.db.format_query(sql))
        .bind(ip)
        .bind(&day_start)
        .bind(&day_end)
        .fetch_one(&state.db.pool)
        .await?;

    if count >= reg.ip_daily_limit {
        return Err(AppError::Forbidden(format!(
            "当日注册次数已达上限 ({})",
            reg.ip_daily_limit
        )));
    }
    Ok(())
}

/// 邮箱格式与防刷。找回密码、绑定/换绑也走这里，不看域名白名单。
pub(crate) fn validate_email(reg: &RegistrationSettings, email: &str) -> AppResult<()> {
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 {
        return Err(AppError::BadRequest("邮箱格式不正确".to_string()));
    }
    let local = parts[0];

    if reg.email_validation_strict {
        if local.len() > 25 {
            return Err(AppError::BadRequest("邮箱地址过长".to_string()));
        }
        if !local.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(AppError::BadRequest(
                "邮箱地址仅允许数字、字母和下划线".to_string(),
            ));
        }
    }
    Ok(())
}

/// 匹配单条邮箱白名单规则（支持纯域名 `qq.com`、带 `@` 域名 `@qq.com`、子域通配 `*.edu.cn` 以及指定完整邮箱 `user@example.com`）
fn email_matches_whitelist_rule(email: &str, domain: &str, rule: &str) -> bool {
    let r = rule.trim();
    if r.is_empty() {
        return false;
    }
    if let Some(stripped) = r.strip_prefix('@') {
        let rule_domain = stripped.trim();
        return !rule_domain.is_empty() && domain.eq_ignore_ascii_case(rule_domain);
    }
    if r.contains('@') {
        return email.eq_ignore_ascii_case(r);
    }
    if let Some(suffix) = r.strip_prefix("*.") {
        let suffix = suffix.trim();
        if suffix.is_empty() || domain.len() <= suffix.len() {
            return false;
        }
        let dot_suffix = format!(".{}", suffix);
        return domain
            .to_ascii_lowercase()
            .ends_with(&dot_suffix.to_ascii_lowercase());
    }
    domain.eq_ignore_ascii_case(r)
}

/// 域名/邮箱白名单只用于新邮箱注册。开关关闭时不拦截；开关打开且名单为空时，没有任何域名可通过。
fn enforce_registration_email_whitelist(reg: &RegistrationSettings, email: &str) -> AppResult<()> {
    if !reg.email_whitelist_enabled {
        return Ok(());
    }
    let email_trimmed = email.trim();
    let domain = email_trimmed.splitn(2, '@').nth(1).unwrap_or("").trim();
    if domain.is_empty()
        || !reg
            .email_whitelist
            .iter()
            .any(|d| email_matches_whitelist_rule(email_trimmed, domain, d))
    {
        return Err(AppError::BadRequest(format!(
            "不支持 @{} 域名的邮箱注册",
            domain
        )));
    }
    Ok(())
}

/// 计算注册赠送金额
fn calc_gift_amount(marketing: &crate::models::MarketingSettings) -> f64 {
    if !marketing.enable_registration_gift {
        return 0.0;
    }
    if marketing.gift_mode == "random" {
        let min = marketing.min_amount as i64;
        let max = marketing.max_amount as i64;
        if max > min {
            rand::thread_rng().gen_range(min..=max) as f64
        } else {
            min as f64
        }
    } else {
        marketing.fixed_amount
    }
}

/// 从邮箱前缀生成唯一用户名
async fn generate_unique_username(state: &Arc<AppState>, email: &str) -> AppResult<String> {
    let mut base = email.split('@').next().unwrap_or("user").to_string();
    // 过滤掉非字母数字和下划线的字符，确保合规
    base.retain(|c| c.is_ascii_alphanumeric() || c == '_');
    if base.is_empty() {
        base = "user".to_string();
    }
    if base.len() > 40 {
        base.truncate(40);
    }
    // 保证至少5位
    while base.len() < 5 {
        base.push_str(&rand::thread_rng().gen_range(0..10).to_string());
    }
    ensure_unique_username(state, &base).await
}

/// 确保用户名唯一（存在则追加随机后缀）
async fn ensure_unique_username(state: &Arc<AppState>, base: &str) -> AppResult<String> {
    let base_truncated: String = base.chars().take(40).collect();
    let mut current = base_truncated.clone();
    loop {
        let exists: bool = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT EXISTS(SELECT 1 FROM users WHERE username = ?)"),
        )
        .bind(&current)
        .fetch_one(&state.db.pool)
        .await?;
        if !exists {
            return Ok(current);
        }
        let suffix: String = (0..4)
            .map(|_| rand::thread_rng().gen_range(0..10).to_string())
            .collect();
        // 恢复下划线拼接
        current = format!("{}_{}", base_truncated, suffix);
    }
}

/// 生成 OAuth CSRF 防伪 state（HMAC 签名，可嵌入邀请参数）
/// 格式: {provider}_{unix_timestamp}_{invite_hex_or_-}_{hmac_hex16}
pub(crate) fn generate_oauth_state(
    secret: &str,
    provider: &str,
    aff: Option<&str>,
    team: Option<&str>,
    timezone: Option<&str>,
    origin: Option<&str>,
) -> String {
    generate_oauth_state_with_intent(secret, provider, aff, team, timezone, origin, None)
}

pub(crate) fn generate_oauth_state_with_intent(
    secret: &str,
    provider: &str,
    aff: Option<&str>,
    team: Option<&str>,
    timezone: Option<&str>,
    origin: Option<&str>,
    intent: Option<&str>,
) -> String {
    let ts = Utc::now().timestamp();
    let invite = encode_invite_payload_with_intent(aff, team, timezone, origin, intent);
    let payload = format!("oauth:{}:{}:{}", provider, ts, invite);
    type HmacSha256 = Hmac<Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key length is always valid");
    mac.update(payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("{}_{}_{}_{}", provider, ts, invite, &sig[..16])
}

/// 校验登录 OAuth state；成功返回 (aff, team, timezone, origin)。兼容旧版三段式 state（无邀请）。
pub(crate) fn verify_oauth_state(
    secret: &str,
    expected_provider: &str,
    state: &str,
) -> Option<(
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
)> {
    let (aff, team, tz, origin, _) =
        verify_oauth_state_with_intent(secret, expected_provider, state)?;
    Some((aff, team, tz, origin))
}

pub(crate) fn verify_oauth_state_with_intent(
    secret: &str,
    expected_provider: &str,
    state: &str,
) -> Option<(
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
)> {
    let parts: Vec<&str> = state.split('_').collect();
    let (provider, ts_str, invite, sig) = match parts.as_slice() {
        [p, ts, s] if s.len() == 16 => (*p, *ts, "-", *s), // 旧格式
        [p, ts, inv, s] if s.len() == 16 => (*p, *ts, *inv, *s),
        _ => return None,
    };
    if provider != expected_provider {
        return None;
    }
    let Ok(ts) = ts_str.parse::<i64>() else {
        return None;
    };
    let now = Utc::now().timestamp();
    if (now - ts).abs() > OAUTH_STATE_TTL_SECS {
        return None;
    }
    let payload = if invite == "-" && parts.len() == 3 {
        // 旧格式签名载荷
        format!("oauth:{}:{}", provider, ts)
    } else {
        format!("oauth:{}:{}:{}", provider, ts, invite)
    };
    type HmacSha256 = Hmac<Sha256>;
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return None;
    };
    mac.update(payload.as_bytes());
    let expected_sig = hex::encode(mac.finalize().into_bytes());
    if !ct_eq_str(sig, &expected_sig[..16]) {
        return None;
    }
    Some(decode_invite_payload_with_intent(invite))
}

/// 生成绑定/验证场景的 OAuth state
/// 格式: {action}_{provider}_{user_id}_{ts}_{origin_hex_or_-}_{hmac16}
/// 例: bind_wechat_<uuid>_1710000000_abcdef_0123456789abcdef
pub(crate) fn generate_oauth_bind_state(
    secret: &str,
    action: &str,
    provider: &str,
    user_id: &str,
    origin: Option<&str>,
) -> String {
    let ts = Utc::now().timestamp();
    let norm_origin = origin
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(normalize_origin)
        .unwrap_or_default();
    let origin_hex = if norm_origin.is_empty() {
        "-".to_string()
    } else {
        hex::encode(norm_origin.as_bytes())
    };
    let payload = format!("oauth:{}:{}:{}:{}:{}", action, provider, user_id, ts, origin_hex);
    type HmacSha256 = Hmac<Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key length is always valid");
    mac.update(payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("{}_{}_{}_{}_{}_{}", action, provider, user_id, ts, origin_hex, &sig[..16])
}

/// 校验绑定/验证 OAuth state，成功返回 (action, provider, user_id, origin)
pub(crate) fn verify_oauth_bind_state(
    secret: &str,
    state: &str,
) -> Option<(String, String, String, Option<String>)> {
    for action in ["bind", "verify"] {
        for provider in ["wechat", "google"] {
            let prefix = format!("{}_{}_", action, provider);
            let Some(rest) = state.strip_prefix(&prefix) else {
                continue;
            };
            let parts: Vec<&str> = rest.split('_').collect();
            let (user_id, ts_str, origin_hex, sig) = match parts.as_slice() {
                [u, ts, s] if s.len() == 16 => (*u, *ts, "-", *s),
                [u, ts, orig, s] if s.len() == 16 => (*u, *ts, *orig, *s),
                _ => continue,
            };
            if user_id.is_empty() || sig.len() != 16 {
                continue;
            }
            let Ok(ts) = ts_str.parse::<i64>() else {
                continue;
            };
            let now = Utc::now().timestamp();
            if (now - ts).abs() > OAUTH_STATE_TTL_SECS {
                continue;
            }
            let payload = if origin_hex == "-" && parts.len() == 3 {
                format!("oauth:{}:{}:{}:{}", action, provider, user_id, ts)
            } else {
                format!("oauth:{}:{}:{}:{}:{}", action, provider, user_id, ts, origin_hex)
            };
            type HmacSha256 = Hmac<Sha256>;
            let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
                continue;
            };
            mac.update(payload.as_bytes());
            let expected_sig = hex::encode(mac.finalize().into_bytes());
            if !ct_eq_str(sig, &expected_sig[..16]) {
                continue;
            }
            let origin = if origin_hex.is_empty() || origin_hex == "-" {
                None
            } else {
                hex::decode(origin_hex).ok().and_then(|b| String::from_utf8(b).ok())
            };
            return Some((
                action.to_string(),
                provider.to_string(),
                user_id.to_string(),
                origin,
            ));
        }
    }
    None
}

/// 公开接口：为前端扫码登录签发 HMAC OAuth state（禁止前端自造；可嵌入 aff/team/origin）
pub async fn oauth_state(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> AppResult<Json<serde_json::Value>> {
    let provider = params.get("provider").map(|s| s.as_str()).unwrap_or("");
    if provider != "wechat" && provider != "google" {
        return Err(AppError::BadRequest(
            "provider 仅支持 wechat 或 google".to_string(),
        ));
    }
    let aff = pick_nonempty_param(&params, "aff");
    let team = pick_nonempty_param(&params, "team");
    let timezone = pick_nonempty_param(&params, "timezone");
    let intent = pick_nonempty_param(&params, "intent");
    let req_base = get_base_url_from_req(&headers, &state.config.base_url);
    let origin = resolve_trusted_oauth_origin(
        params.get("origin").map(|s| s.as_str()).filter(|s| !s.is_empty()),
        &req_base,
        &state.config.base_url,
        None,
    )
    .unwrap_or(req_base);
    let state_val = generate_oauth_state_with_intent(
        &state.config.jwt_secret,
        provider,
        aff.as_deref(),
        team.as_deref(),
        timezone.as_deref(),
        Some(&origin),
        intent.as_deref(),
    );
    Ok(Json(serde_json::json!({ "state": state_val })))
}

#[cfg(test)]
mod registration_ip_list_tests {
    use super::{check_ip_blacklist, registration_ip_whitelisted};
    use crate::api::settings::default_site_settings;
    use crate::models::SiteSettings;

    fn site(
        blacklist: &[&str],
        whitelist: &[&str],
        black_on: bool,
        white_on: bool,
    ) -> SiteSettings {
        let mut settings = default_site_settings();
        settings.ip_blacklist_enabled = black_on;
        settings.ip_blacklist = blacklist.iter().map(|s| (*s).to_string()).collect();
        settings.ip_whitelist_enabled = white_on;
        settings.ip_whitelist = whitelist.iter().map(|s| (*s).to_string()).collect();
        settings
    }

    #[test]
    fn whitelist_exact_and_cidr_skip_blacklist() {
        let settings = site(
            &["10.0.0.0/8", "1.2.3.4"],
            &["10.1.2.3", "192.168.1.0/24"],
            true,
            true,
        );
        assert!(check_ip_blacklist(&settings, "10.1.2.3").is_ok());
        assert!(check_ip_blacklist(&settings, "192.168.1.50").is_ok());
        assert!(check_ip_blacklist(&settings, "10.9.9.9").is_err());
        assert!(check_ip_blacklist(&settings, "1.2.3.4").is_err());
    }

    #[test]
    fn whitelist_off_does_not_exempt() {
        let settings = site(&["10.0.0.8"], &["10.0.0.8"], true, false);
        assert!(check_ip_blacklist(&settings, "10.0.0.8").is_err());
    }

    #[test]
    fn builtin_whitelist_opens_last_octet_only() {
        let settings = crate::api::settings::default_site_settings();
        assert!(registration_ip_whitelisted(&settings, "10.0.0.8"));
        assert!(registration_ip_whitelisted(&settings, "192.168.1.8"));
        assert!(registration_ip_whitelisted(&settings, "172.16.0.8"));
        assert!(!registration_ip_whitelisted(&settings, "10.0.1.1"));
        assert!(!registration_ip_whitelisted(&settings, "192.168.2.1"));
        assert!(!registration_ip_whitelisted(&settings, "172.16.1.1"));
    }

    #[test]
    fn empty_whitelist_does_not_exempt() {
        let mut settings = crate::api::settings::default_site_settings();
        settings.ip_whitelist_enabled = true;
        settings.ip_whitelist.clear();
        assert!(!registration_ip_whitelisted(&settings, "192.168.1.20"));
        settings.ip_whitelist_enabled = false;
        assert!(!registration_ip_whitelisted(&settings, "10.0.0.8"));
    }

    #[test]
    fn disabled_whitelist_with_entries_stays_off() {
        let settings = site(&[], &["10.0.0.8"], false, false);
        assert!(!registration_ip_whitelisted(&settings, "10.0.0.8"));
    }

    #[test]
    fn mapped_ipv6_matches_v4_whitelist() {
        let settings = site(&[], &["127.0.0.1"], false, true);
        assert!(registration_ip_whitelisted(&settings, "::ffff:127.0.0.1"));
        assert!(!registration_ip_whitelisted(&settings, "10.0.0.1"));
    }

    #[test]
    fn username_standalone_ip_limit_overrides_or_falls_back() {
        use super::{configured_username_ip_limit, username_ip_daily_limit};
        use crate::models::RegistrationSettings;

        let mut reg = RegistrationSettings::default();
        assert_eq!(configured_username_ip_limit(&reg), None);
        assert_eq!(username_ip_daily_limit(&reg, false), None);

        reg.ip_rate_limit_enabled = true;
        reg.ip_daily_limit = 6;
        assert_eq!(configured_username_ip_limit(&reg), None);
        assert_eq!(username_ip_daily_limit(&reg, false), Some(6));

        reg.username_ip_daily_limit = Some(3);
        assert_eq!(configured_username_ip_limit(&reg), Some(3));
        assert_eq!(username_ip_daily_limit(&reg, false), Some(3));

        reg.username_ip_daily_limit = Some(0);
        assert_eq!(configured_username_ip_limit(&reg), None);
        assert_eq!(username_ip_daily_limit(&reg, false), Some(6));
    }
}

#[cfg(test)]
mod email_whitelist_tests {
    use super::{enforce_registration_email_whitelist, validate_email};
    use crate::models::RegistrationSettings;

    fn whitelist_on(domains: &[&str]) -> RegistrationSettings {
        let mut reg = RegistrationSettings::default();
        reg.email_whitelist_enabled = true;
        reg.email_whitelist = domains.iter().map(|s| (*s).to_string()).collect();
        reg
    }

    #[test]
    fn registration_rejects_domain_outside_list() {
        let reg = whitelist_on(&["qq.com", "163.com"]);
        assert!(enforce_registration_email_whitelist(&reg, "a@gmail.com").is_err());
        assert!(enforce_registration_email_whitelist(&reg, "a@qq.com").is_ok());
        assert!(enforce_registration_email_whitelist(&reg, "a@QQ.COM").is_ok());
    }

    #[test]
    fn existing_account_mail_ignores_whitelist() {
        let reg = whitelist_on(&["qq.com"]);
        assert!(validate_email(&reg, "old.user@gmail.com").is_ok());
    }

    #[test]
    fn google_email_binds_only_when_account_email_is_placeholder() {
        use super::unbound_email_from_google;
        assert_eq!(
            unbound_email_from_google("", Some("User@gmail.com")).as_deref(),
            Some("User@gmail.com")
        );
        assert_eq!(
            unbound_email_from_google("g_1@tokensbyte.local", Some("User@gmail.com")).as_deref(),
            Some("User@gmail.com")
        );
        assert_eq!(
            unbound_email_from_google("kept@qq.com", Some("User@gmail.com")).as_deref(),
            None
        );
        assert_eq!(unbound_email_from_google("", None).as_deref(), None);
        assert_eq!(
            unbound_email_from_google("", Some("g_9@tokensbyte.local")).as_deref(),
            None
        );
    }

    #[test]
    fn disabled_allows_any_domain_empty_list_blocks() {
        let mut reg = whitelist_on(&["qq.com"]);
        reg.email_whitelist_enabled = false;
        assert!(enforce_registration_email_whitelist(&reg, "a@gmail.com").is_ok());
        reg.email_whitelist_enabled = true;
        reg.email_whitelist.clear();
        assert!(enforce_registration_email_whitelist(&reg, "a@gmail.com").is_err());
    }

    #[test]
    fn supports_at_prefix_wildcard_and_exact_email() {
        let reg = whitelist_on(&["@163.com", "*.edu.cn", "vip@custom.org"]);
        assert!(enforce_registration_email_whitelist(&reg, "user@163.com").is_ok());
        assert!(enforce_registration_email_whitelist(&reg, "stu@tsinghua.edu.cn").is_ok());
        assert!(enforce_registration_email_whitelist(&reg, "stu@edu.cn").is_err());
        assert!(enforce_registration_email_whitelist(&reg, "VIP@custom.org").is_ok());
        assert!(enforce_registration_email_whitelist(&reg, "other@custom.org").is_err());
    }
}

#[cfg(test)]
mod client_ip_header_tests {
    use super::parse_client_ip_from_headers;
    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn inner_nginx_docker_hop_does_not_hide_visitor() {
        let h = headers(&[
            ("x-real-ip", "172.18.0.1"),
            ("x-forwarded-for", "203.0.113.8, 172.18.0.1"),
        ]);
        assert_eq!(parse_client_ip_from_headers(&h), Some("203.0.113.8"));
    }

    #[test]
    fn mapped_docker_hop_is_skipped() {
        let h = headers(&[
            ("x-real-ip", "::ffff:172.18.0.1"),
            ("x-forwarded-for", "203.0.113.8, ::ffff:172.18.0.1"),
        ]);
        assert_eq!(parse_client_ip_from_headers(&h), Some("203.0.113.8"));
    }

    #[test]
    fn public_x_real_ip_wins_over_spoofed_leftmost() {
        let h = headers(&[
            ("x-real-ip", "203.0.113.8"),
            ("x-forwarded-for", "1.2.3.4, 203.0.113.8"),
        ]);
        assert_eq!(parse_client_ip_from_headers(&h), Some("203.0.113.8"));
    }

    #[test]
    fn office_lan_behind_docker_hop_is_kept() {
        let h = headers(&[
            ("x-real-ip", "172.18.0.1"),
            ("x-forwarded-for", "192.168.1.20, 172.18.0.1"),
        ]);
        assert_eq!(parse_client_ip_from_headers(&h), Some("192.168.1.20"));
    }

    #[test]
    fn xff_rightmost_ignores_client_spoof() {
        let h = headers(&[("x-forwarded-for", "1.2.3.4, 203.0.113.9")]);
        assert_eq!(parse_client_ip_from_headers(&h), Some("203.0.113.9"));
    }

    #[test]
    fn single_proxy_office_ip_stays() {
        let h = headers(&[("x-real-ip", "10.0.0.8")]);
        assert_eq!(parse_client_ip_from_headers(&h), Some("10.0.0.8"));
    }

    #[test]
    fn untrusted_cf_header_does_not_override_real_ip() {
        let h = headers(&[
            ("cf-connecting-ip", "10.0.0.1"),
            ("x-real-ip", "203.0.113.8"),
        ]);
        assert_eq!(parse_client_ip_from_headers(&h), Some("203.0.113.8"));
    }

    #[test]
    fn docker_192_bridge_hop_extracts_real_public_ip() {
        let h = headers(&[
            ("x-real-ip", "192.168.96.1"),
            ("x-forwarded-for", "14.155.89.84, 192.168.96.1"),
        ]);
        assert_eq!(parse_client_ip_from_headers(&h), Some("14.155.89.84"));
    }

    #[test]
    fn docker_10_bridge_hop_extracts_real_public_ip() {
        let h = headers(&[
            ("x-real-ip", "10.0.0.1"),
            ("x-forwarded-for", "14.155.89.84, 10.0.0.1"),
        ]);
        assert_eq!(parse_client_ip_from_headers(&h), Some("14.155.89.84"));
    }
}

#[cfg(test)]
mod oauth_multidomain_state_tests {
    use super::{
        generate_oauth_bind_state, generate_oauth_state, normalize_origin, verify_oauth_bind_state,
        verify_oauth_state,
    };

    #[test]
    fn test_normalize_origin() {
        assert_eq!(normalize_origin("old.com"), "https://old.com");
        assert_eq!(normalize_origin("https://new.com/"), "https://new.com");
        assert_eq!(normalize_origin("http://127.0.0.1:3000/"), "http://127.0.0.1:3000");
        assert_eq!(normalize_origin("   "), "");
    }

    #[test]
    fn test_oauth_state_with_origin_relay() {
        let secret = "test_jwt_secret_1234567890abcdef";
        let state = generate_oauth_state(
            secret,
            "wechat",
            Some("aff123"),
            Some("team456"),
            Some("Asia/Shanghai"),
            Some("https://new.com"),
        );
        let (aff, team, tz, origin) = verify_oauth_state(secret, "wechat", &state).unwrap();
        assert_eq!(aff.as_deref(), Some("aff123"));
        assert_eq!(team.as_deref(), Some("team456"));
        assert_eq!(tz.as_deref(), Some("Asia/Shanghai"));
        assert_eq!(origin.as_deref(), Some("https://new.com"));
    }

    #[test]
    fn test_oauth_state_without_origin_fallback() {
        let secret = "test_jwt_secret_1234567890abcdef";
        let state = generate_oauth_state(secret, "wechat", None, None, None, None);
        let (aff, team, tz, origin) = verify_oauth_state(secret, "wechat", &state).unwrap();
        assert_eq!(aff, None);
        assert_eq!(team, None);
        assert_eq!(tz, None);
        assert_eq!(origin, None);
    }

    #[test]
    fn test_oauth_bind_state_with_origin() {
        let secret = "test_jwt_secret_1234567890abcdef";
        let uid = "018f3a8b-1111-7777-8888-999999999999";
        let state = generate_oauth_bind_state(secret, "bind", "wechat", uid, Some("https://new.com"));
        let (action, provider, user_id, origin) =
            verify_oauth_bind_state(secret, &state).unwrap();
        assert_eq!(action, "bind");
        assert_eq!(provider, "wechat");
        assert_eq!(user_id, uid);
        assert_eq!(origin.as_deref(), Some("https://new.com"));
    }
}
