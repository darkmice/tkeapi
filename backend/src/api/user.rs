/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::models::{
    BindEmailRequest, BindMobileRequest, ProfileUpdateRequest, RechargeRecord, UnbindRequest, User,
    WalletStats,
};
use crate::AppState;
use axum::{
    extract::{ConnectInfo, Extension, Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const PROFILE_USER_SQL: &str = "SELECT u.*, ul.name as level_name, ul.id as level_id, ul.allow_view_log_details, ul.invoice_enabled, ul.invoice_mode, ul.invoice_config, ul.marketing_enabled, ul.commission_ratio FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?";

pub(crate) async fn load_profile_user(
    state: &Arc<AppState>,
    claims: &auth::Claims,
) -> AppResult<User> {
    let mut user: User = sqlx::query_as(&state.db.format_query(PROFILE_USER_SQL))
        .bind(&claims.sub)
        .fetch_optional(&state.db.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Hide placeholder email from user-facing response
    if user.email.ends_with("@tokensbyte.local") {
        user.email = String::new();
    }

    crate::admin_permission::hydrate_user_admin_permissions(state, &mut user).await?;

    Ok(user)
}

pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<User>> {
    Ok(Json(load_profile_user(&state, &claims).await?))
}

pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Json(request): Json<ProfileUpdateRequest>,
) -> AppResult<Json<User>> {
    let mut user: User = sqlx::query_as(&state.db.format_query(PROFILE_USER_SQL))
        .bind(&claims.sub)
        .fetch_optional(&state.db.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    if let Some(ref nickname) = request.nickname {
        if nickname.chars().count() > 24 {
            return Err(AppError::BadRequest(
                "昵称长度最多不能超过 24 个字符".to_string(),
            ));
        }
        user.nickname = Some(nickname.clone());
    }
    crate::models::user::assign_profile_identity(
        &mut user.first_name,
        &mut user.last_name,
        &mut user.birth_ym,
        request.first_name,
        request.last_name,
        request.birth_ym,
    )
    .map_err(AppError::BadRequest)?;
    // 邮箱/手机/微信不得通过资料接口直接改写，必须走验证码绑定接口，防止未验证绑定被找回密码利用
    if request.email.is_some() || request.mobile.is_some() || request.wechat_id.is_some() {
        return Err(AppError::BadRequest(
            "请通过安全绑定流程修改邮箱、手机号或微信".to_string(),
        ));
    }
    if let Some(timezone) = request.timezone {
        user.timezone = Some(timezone);
    }
    if let Some(prefs) = request.notification_preferences {
        let merged = crate::services::notification::merge_user_prefs_json(
            user.notification_preferences.as_deref(),
            &prefs,
        );
        user.notification_preferences = Some(merged);
    }

    let mut password_changed = false;
    if let Some(password) = request.password {
        if !password.is_empty() {
            auth::validate_password(&password)?;
            if let Some(old_password) = request.old_password {
                if !auth::verify_password(&old_password, &user.password_hash).unwrap_or(false) {
                    return Err(AppError::BadRequest("原密码不正确".to_string()));
                }
            } else {
                return Err(AppError::BadRequest("修改密码需要验证原密码".to_string()));
            }
            user.password_hash = auth::hash_password(&password)?;
            password_changed = true;
        }
    }

    if password_changed {
        sqlx::query(&state.db.format_query(
            r#"UPDATE users SET email = ?, password_hash = ?, nickname = ?, mobile = ?, 
               wechat_id = ?, timezone = ?, notification_preferences = ?, first_name = ?, last_name = ?, birth_ym = ?, password_changed_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
               WHERE id = ?"#,
        ))
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.nickname)
        .bind(&user.mobile)
        .bind(&user.wechat_id)
        .bind(&user.timezone)
        .bind(&user.notification_preferences)
        .bind(&user.first_name)
        .bind(&user.last_name)
        .bind(&user.birth_ym)
        .bind(&user.id)
        .execute(&state.db.pool)
        .await?;
    } else {
        sqlx::query(&state.db.format_query(
            r#"UPDATE users SET email = ?, password_hash = ?, nickname = ?, mobile = ?, 
               wechat_id = ?, timezone = ?, notification_preferences = ?, first_name = ?, last_name = ?, birth_ym = ?, updated_at = CURRENT_TIMESTAMP
               WHERE id = ?"#,
        ))
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.nickname)
        .bind(&user.mobile)
        .bind(&user.wechat_id)
        .bind(&user.timezone)
        .bind(&user.notification_preferences)
        .bind(&user.first_name)
        .bind(&user.last_name)
        .bind(&user.birth_ym)
        .bind(&user.id)
        .execute(&state.db.pool)
        .await?;
    }

    crate::api::auth::sanitize_user_for_client(&mut user);
    crate::admin_permission::hydrate_user_admin_permissions(&state, &mut user).await?;

    Ok(Json(user))
}

async fn load_avatar_owner(
    state: &Arc<AppState>,
    claims: &auth::Claims,
) -> AppResult<(String, String, Option<String>)> {
    sqlx::query_as(
        &state
            .db
            .format_query("SELECT id, uid, avatar FROM users WHERE id = ?"),
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))
}

/// 用户上传自己的头像（客户端已裁成 1:1 小图）
pub async fn upload_avatar(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    multipart: axum::extract::Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let (user_id, uid, old_avatar) = load_avatar_owner(&state, &claims).await?;
    let avatar =
        crate::api::users::store_user_avatar(&state, &user_id, &uid, old_avatar, multipart).await?;
    Ok(Json(serde_json::json!({ "avatar": avatar })))
}

/// 用户移除自己的头像
pub async fn remove_avatar(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<serde_json::Value>> {
    let (user_id, uid, old_avatar) = load_avatar_owner(&state, &claims).await?;
    crate::api::users::clear_user_avatar(&state, &user_id, &uid, old_avatar).await?;
    Ok(Json(serde_json::json!({ "avatar": null })))
}

async fn ensure_client_ip_allowed(
    state: &Arc<AppState>,
    headers: &axum::http::HeaderMap,
    addr: std::net::SocketAddr,
) -> AppResult<()> {
    let settings = crate::api::settings::load_all_settings(state).await?;
    let ip = crate::api::auth::extract_client_ip(headers, &addr);
    crate::api::auth::check_ip_blacklist(&settings.site, &ip)
}

/// 绑定/换绑手机号
pub async fn bind_mobile(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Extension(claims): Extension<auth::Claims>,
    Json(request): Json<BindMobileRequest>,
) -> AppResult<Json<serde_json::Value>> {
    ensure_client_ip_allowed(&state, &headers, addr).await?;
    let user: User = sqlx::query_as(&state.db.format_query("SELECT * FROM users WHERE id = ?"))
        .bind(&claims.sub)
        .fetch_one(&state.db.pool)
        .await?;

    let settings = crate::api::settings::load_all_settings(&state).await?;
    let mobile = crate::auth::contact::normalize_mobile(
        &request.mobile,
        settings.registration.allow_international_mobile,
    )?;

    let old_mobile_opt = user
        .mobile
        .as_deref()
        .map(crate::auth::contact::canonical_mobile_lenient)
        .filter(|m| !m.is_empty());
    if old_mobile_opt.as_deref() == Some(mobile.as_str()) {
        return Err(AppError::BadRequest(
            "新手机号不能与当前已绑定手机号相同".to_string(),
        ));
    }

    // 预校验原手机验证码与新手机验证码（暂不消费，避免新手机号撞上限或新码输错导致旧码作废）
    if let Some(ref old_mobile) = old_mobile_opt {
        let old_code = request
            .old_code
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("换绑需验证原手机验证码".to_string()))?;
        crate::api::auth::peek_sms_code_pub(&state, old_mobile, old_code, "bind_mobile").await?;
    }
    crate::api::auth::peek_sms_code_pub(&state, &mobile, &request.code, "bind_mobile").await?;

    let limit =
        crate::auth::contact::normalize_limit(settings.registration.max_accounts_per_mobile);
    let mut tx = state.db.pool.begin().await?;
    crate::auth::contact::assert_mobile_slot_tx(
        &mut tx,
        &state.db,
        &mobile,
        Some(&claims.sub),
        limit,
    )
    .await?;

    if let Some(ref old_mobile) = old_mobile_opt {
        if let Some(ref old_code) = request.old_code {
            crate::api::auth::verify_sms_code_pub(&state, old_mobile, old_code, "bind_mobile")
                .await?;
        }
    }
    crate::api::auth::verify_sms_code_pub(&state, &mobile, &request.code, "bind_mobile").await?;

    sqlx::query(
        &state.db.format_query(
            "UPDATE users SET mobile = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ),
    )
    .bind(&mobile)
    .bind(&claims.sub)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(
        serde_json::json!({"success": true, "message": "手机号绑定成功"}),
    ))
}

/// 绑定/换绑邮箱
pub async fn bind_email(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Extension(claims): Extension<auth::Claims>,
    Json(request): Json<BindEmailRequest>,
) -> AppResult<Json<serde_json::Value>> {
    ensure_client_ip_allowed(&state, &headers, addr).await?;
    let user: User = sqlx::query_as(&state.db.format_query("SELECT * FROM users WHERE id = ?"))
        .bind(&claims.sub)
        .fetch_one(&state.db.pool)
        .await?;

    let settings = crate::api::settings::load_all_settings(&state).await?;
    let email = crate::auth::contact::canonical_email(&request.email);
    if email.is_empty() {
        return Err(AppError::BadRequest("邮箱不能为空".to_string()));
    }
    if !crate::auth::contact::is_real_email(&email) {
        return Err(AppError::BadRequest("无效的邮箱地址".to_string()));
    }
    crate::api::auth::validate_email(&settings.registration, &email)?;

    let old_email = crate::auth::contact::canonical_email(&user.email);
    let has_old_email = crate::auth::contact::is_real_email(&old_email);
    if has_old_email && old_email == email {
        return Err(AppError::BadRequest(
            "新邮箱不能与当前已绑定邮箱相同".to_string(),
        ));
    }

    // 预校验原邮箱验证码与新邮箱验证码（暂不消费，避免新邮箱撞上限或新码输错导致旧码作废）
    if has_old_email {
        let old_code = request
            .old_code
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("换绑需验证原邮箱验证码".to_string()))?;
        crate::api::auth::peek_email_code_pub(&state, &old_email, old_code, "bind_email").await?;
    }
    crate::api::auth::peek_email_code_pub(&state, &email, &request.code, "bind_email").await?;

    let limit = crate::auth::contact::normalize_limit(settings.registration.max_accounts_per_email);
    let mut tx = state.db.pool.begin().await?;
    crate::auth::contact::assert_email_slot_tx(
        &mut tx,
        &state.db,
        &email,
        Some(&claims.sub),
        limit,
    )
    .await?;

    if has_old_email {
        if let Some(ref old_code) = request.old_code {
            crate::api::auth::verify_email_code_pub(&state, &old_email, old_code, "bind_email")
                .await?;
        }
    }
    crate::api::auth::verify_email_code_pub(&state, &email, &request.code, "bind_email").await?;

    sqlx::query(
        &state.db.format_query(
            "UPDATE users SET email = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ),
    )
    .bind(&email)
    .bind(&claims.sub)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(
        serde_json::json!({"success": true, "message": "邮箱绑定成功"}),
    ))
}

/// 绑定/换绑微信 — 发起授权跳转
pub async fn bind_wechat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Extension(claims): Extension<auth::Claims>,
) -> Response {
    let result = (async {
        let settings = crate::api::settings::load_all_settings(&state).await?;
        let wechat = settings
            .wechat_oauth
            .ok_or_else(|| AppError::BadRequest("微信授权未配置".to_string()))?;
        let req_base_url =
            crate::api::auth::get_base_url_from_req(&headers, &state.config.base_url);
        let callback_base = wechat.callback_domain.trim();
        let redirect_base = if !callback_base.is_empty() {
            crate::api::auth::normalize_origin(callback_base)
        } else {
            req_base_url.clone()
        };
        let redirect_uri = format!("{}/api/v1/user/bind/wechat/callback", redirect_base);
        let return_origin = (redirect_base != req_base_url).then_some(req_base_url.as_str());
        let state_val = crate::api::auth::generate_oauth_bind_state(
            &state.config.jwt_secret,
            "bind",
            "wechat",
            &claims.sub,
            return_origin,
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

/// 为前端扫码绑定签发 HMAC state（需登录）
pub async fn bind_oauth_state(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Extension(claims): Extension<auth::Claims>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> AppResult<Json<serde_json::Value>> {
    let provider = query.get("provider").map(|s| s.as_str()).unwrap_or("");
    let action = query.get("action").map(|s| s.as_str()).unwrap_or("bind");
    if provider != "wechat" && provider != "google" {
        return Err(AppError::BadRequest(
            "provider 仅支持 wechat 或 google".to_string(),
        ));
    }
    if action != "bind" && action != "verify" {
        return Err(AppError::BadRequest(
            "action 仅支持 bind 或 verify".to_string(),
        ));
    }
    let req_base = crate::api::auth::get_base_url_from_req(&headers, &state.config.base_url);
    let origin = crate::api::auth::resolve_trusted_oauth_origin(
        query.get("origin").map(|s| s.as_str()).filter(|s| !s.is_empty()),
        &req_base,
        &state.config.base_url,
        None,
    )
    .unwrap_or(req_base);
    let state_val = crate::api::auth::generate_oauth_bind_state(
        &state.config.jwt_secret,
        action,
        provider,
        &claims.sub,
        Some(&origin),
    );
    Ok(Json(serde_json::json!({ "state": state_val })))
}

/// 微信绑定回调 — 统一处理验证旧微信 / 绑定新微信两种场景
/// state 必须为服务端 HMAC 签发：verify_wechat_{uid}_{ts}_{origin}_{sig} / bind_wechat_{uid}_{ts}_{origin}_{sig}
pub async fn bind_wechat_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<crate::api::auth::OAuthCallbackQuery>,
) -> Response {
    let result = (async {
        let code = query.code.ok_or_else(|| AppError::BadRequest("缺少 code".to_string()))?;
        let state_str = query.state.unwrap_or_default();
        let (action, provider, user_id, origin) = crate::api::auth::verify_oauth_bind_state(
            &state.config.jwt_secret,
            &state_str,
        )
        .ok_or_else(|| AppError::BadRequest("OAuth state 验证失败，请重新发起授权".to_string()))?;
        if provider != "wechat" {
            return Err(AppError::BadRequest("无效的 state 参数".to_string()));
        }

        let make_redirect = |param: &str| match origin.as_deref().filter(|s| !s.is_empty()) {
            Some(base) => format!("{}/profile?wechat_action={param}", base.trim_end_matches('/')),
            None => format!("/profile?wechat_action={param}"),
        };

        let settings = crate::api::settings::load_all_settings(&state).await?;
        let wechat = settings.wechat_oauth.ok_or_else(|| AppError::BadRequest("微信授权未配置".to_string()))?;
        let info = crate::services::oauth::OAuthService::wechat_exchange(&wechat.app_id, &wechat.app_secret, &code).await?;

        let wechat_identifier = info.unionid.as_deref().unwrap_or(&info.openid);
        let fallback_identifier = &info.openid;

        // ── 验证旧微信身份（换绑第一步） ───────────────────────
        if action == "verify" {
            let current_wechat: Option<String> = sqlx::query_scalar(
                &state.db.format_query("SELECT wechat_id FROM users WHERE id = ?")
            ).bind(&user_id).fetch_optional(&state.db.pool).await?.flatten();

            if current_wechat.as_deref() != Some(wechat_identifier) && current_wechat.as_deref() != Some(fallback_identifier) {
                return Ok(make_redirect("verify_failed"));
            }
            mark_oauth_rebind_verified(&user_id, "wechat");
            return Ok(make_redirect("verified"));
        }

        // ── 绑定新微信（首次绑定 / 换绑第二步） ─────────────────
        if action == "bind" {
            let current_wechat: Option<String> = sqlx::query_scalar(
                &state.db.format_query("SELECT wechat_id FROM users WHERE id = ?")
            ).bind(&user_id).fetch_optional(&state.db.pool).await?.flatten();
            let is_rebind = current_wechat.as_deref().is_some_and(|w| !w.trim().is_empty());
            if is_rebind && !has_oauth_rebind_verified(&user_id, "wechat") {
                return Ok(make_redirect("verify_failed"));
            }

            let mut tx = state.db.pool.begin().await?;
            if wechat_identifier != fallback_identifier {
                sqlx::query(&state.db.format_query(
                    "UPDATE users SET wechat_id = ?, wechat_name = ? WHERE wechat_id = ?",
                ))
                .bind(wechat_identifier)
                .bind(&info.nickname)
                .bind(fallback_identifier)
                .execute(&mut *tx)
                .await?;
            }
            let limit = crate::auth::contact::normalize_limit(
                settings.registration.max_accounts_per_wechat,
            );
            if crate::auth::contact::assert_wechat_slot_tx(
                &mut tx,
                &state.db,
                wechat_identifier,
                Some(&user_id),
                limit,
            )
            .await
            .is_err()
            {
                return Ok(make_redirect("bindconflict"));
            }
            if is_rebind && !consume_oauth_rebind_verified(&user_id, "wechat") {
                return Ok(make_redirect("verify_failed"));
            }
            sqlx::query(&state.db.format_query("UPDATE users SET wechat_id = ?, wechat_name = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"))
                .bind(wechat_identifier).bind(&info.nickname).bind(&user_id).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(make_redirect("bindok"));
        }

        Err(AppError::BadRequest("无效的 state 参数".to_string()))
    }).await;
    match result {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(err) => err.into_response(),
    }
}

static OAUTH_REBIND_VERIFIED: std::sync::LazyLock<
    dashmap::DashMap<(String, &'static str), std::time::Instant>,
> = std::sync::LazyLock::new(dashmap::DashMap::new);

const OAUTH_REBIND_TTL: std::time::Duration = std::time::Duration::from_secs(300);

fn mark_oauth_rebind_verified(user_id: &str, provider: &'static str) {
    let now = std::time::Instant::now();
    OAUTH_REBIND_VERIFIED.retain(|_, ts| now.duration_since(*ts) < OAUTH_REBIND_TTL);
    OAUTH_REBIND_VERIFIED.insert((user_id.to_string(), provider), now);
}

fn has_oauth_rebind_verified(user_id: &str, provider: &'static str) -> bool {
    if let Some(ts) = OAUTH_REBIND_VERIFIED.get(&(user_id.to_string(), provider)) {
        ts.elapsed() < OAUTH_REBIND_TTL
    } else {
        false
    }
}

fn consume_oauth_rebind_verified(user_id: &str, provider: &'static str) -> bool {
    if let Some((_, ts)) = OAUTH_REBIND_VERIFIED.remove(&(user_id.to_string(), provider)) {
        ts.elapsed() < OAUTH_REBIND_TTL
    } else {
        false
    }
}

/// 绑定/换绑谷歌 — 发起授权跳转
pub async fn bind_google(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Extension(claims): Extension<auth::Claims>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let result = (async {
        let settings = crate::api::settings::load_all_settings(&state).await?;
        let google = settings
            .google_oauth
            .ok_or_else(|| AppError::BadRequest("谷歌授权未配置".to_string()))?;
        let req_base_url =
            crate::api::auth::get_base_url_from_req(&headers, &state.config.base_url);
        let redirect_uri = format!("{}/api/v1/user/bind/google/callback", req_base_url);

        let action = query.get("action").map(|s| s.as_str()).unwrap_or("bind");
        let action = if action == "verify" { "verify" } else { "bind" };
        let return_origin = crate::api::auth::resolve_trusted_oauth_origin(
            query.get("origin").map(|s| s.as_str()).filter(|s| !s.is_empty()),
            &req_base_url,
            &state.config.base_url,
            None,
        );
        let state_val = crate::api::auth::generate_oauth_bind_state(
            &state.config.jwt_secret,
            action,
            "google",
            &claims.sub,
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

/// 谷歌绑定回调 — 统一处理验证旧谷歌 / 绑定新谷歌两种场景
pub async fn bind_google_callback(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(query): Query<crate::api::auth::OAuthCallbackQuery>,
) -> Response {
    let result = (async {
        let code = query.code.ok_or_else(|| AppError::BadRequest("缺少 code".to_string()))?;
        let state_str = query.state.unwrap_or_default();
        let (action, provider, user_id, origin) = crate::api::auth::verify_oauth_bind_state(
            &state.config.jwt_secret,
            &state_str,
        )
        .ok_or_else(|| AppError::BadRequest("OAuth state 验证失败，请重新发起授权".to_string()))?;
        if provider != "google" {
            return Err(AppError::BadRequest("无效的 state 参数".to_string()));
        }

        let make_redirect = |param: &str| match origin.as_deref().filter(|s| !s.is_empty()) {
            Some(base) => format!("{}/profile?google_action={param}", base.trim_end_matches('/')),
            None => format!("/profile?google_action={param}"),
        };

        let settings = crate::api::settings::load_all_settings(&state).await?;
        let google = settings.google_oauth.ok_or_else(|| AppError::BadRequest("谷歌授权未配置".to_string()))?;
        let req_base_url = crate::api::auth::get_base_url_from_req(&headers, &state.config.base_url);
        let redirect_uri = format!("{}/api/v1/user/bind/google/callback", req_base_url);
        let info = crate::services::oauth::OAuthService::google_exchange(&google.client_id, &google.client_secret, &code, &redirect_uri).await?;

        let google_display_name = info.name.clone().or_else(|| info.email.clone());
        let account: Option<(Option<String>, String)> = sqlx::query_as(
            &state.db.format_query("SELECT google_id, email FROM users WHERE id = ?"),
        )
        .bind(&user_id)
        .fetch_optional(&state.db.pool)
        .await?;
        let Some((current_google, current_email)) = account else {
            return Err(AppError::NotFound("用户不存在".to_string()));
        };

        if action == "verify" {
            if !crate::services::oauth::google_identity_matches(current_google.as_deref(), &info) {
                return Ok(make_redirect("verify_failed"));
            }
            mark_oauth_rebind_verified(&user_id, "google");
            return Ok(make_redirect("verified"));
        }

        if action == "bind" {
            let is_rebind = current_google.as_deref().is_some_and(|g| !g.trim().is_empty());
            if is_rebind && !has_oauth_rebind_verified(&user_id, "google") {
                return Ok(make_redirect("verify_failed"));
            }

            let Some(google_id) = info.account_key() else {
                return Ok(make_redirect("verify_failed"));
            };
            let exists: bool = sqlx::query_scalar(&state.db.format_query("SELECT EXISTS(SELECT 1 FROM users WHERE google_id = ? AND id != ?)"))
                .bind(&google_id).bind(&user_id).fetch_one(&state.db.pool).await?;
            if exists {
                return Ok(make_redirect("bindconflict"));
            }
            if is_rebind && !consume_oauth_rebind_verified(&user_id, "google") {
                return Ok(make_redirect("verify_failed"));
            }

            let bind_email = crate::api::auth::google_email_to_apply(
                &state,
                &settings.registration,
                &current_email,
                info.verified_email().as_deref(),
                Some(&user_id),
                true,
            )
            .await?;
            sqlx::query(&state.db.format_query(
                "UPDATE users SET google_id = ?, google_name = ?, email = COALESCE(?, email), updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            ))
            .bind(&google_id)
            .bind(&google_display_name)
            .bind(&bind_email)
            .bind(&user_id)
            .execute(&state.db.pool)
            .await?;

            return Ok(make_redirect("bindok"));
        }

        Err(AppError::BadRequest("无效的 state 参数".to_string()))
    }).await;
    match result {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(err) => err.into_response(),
    }
}

/// 解绑第三方（需密码校验）
pub async fn unbind_third_party(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(bind_type): Path<String>,
    Json(request): Json<UnbindRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user: User = sqlx::query_as(&state.db.format_query("SELECT * FROM users WHERE id = ?"))
        .bind(&claims.sub)
        .fetch_one(&state.db.pool)
        .await?;

    // 密码校验
    if !auth::verify_password(&request.password, &user.password_hash)? {
        return Err(AppError::AuthFailed("密码错误".to_string()));
    }

    match bind_type.as_str() {
        "wechat" => {
            sqlx::query(&state.db.format_query(
                "UPDATE users SET wechat_id = NULL, wechat_name = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            ))
            .bind(&claims.sub)
            .execute(&state.db.pool)
            .await?;
            Ok(Json(
                serde_json::json!({"success": true, "message": "微信已解绑"}),
            ))
        }
        "google" => {
            sqlx::query(&state.db.format_query(
                "UPDATE users SET google_id = NULL, google_name = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            ))
            .bind(&claims.sub)
            .execute(&state.db.pool)
            .await?;
            Ok(Json(
                serde_json::json!({"success": true, "message": "谷歌已解绑"}),
            ))
        }
        _ => Err(AppError::BadRequest("不支持的解绑类型".to_string())),
    }
}

pub async fn get_wallet_stats(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<WalletStats>> {
    let user_id = &claims.sub;

    let (balance, gift_balance, credit_limit, pay_enabled): (f64, f64, f64, i64) =
        sqlx::query_as(&state.db.format_query(
            "SELECT balance, gift_balance, credit_limit, pay_enabled FROM users WHERE id = ?",
        ))
        .bind(user_id)
        .fetch_one(&state.db.pool)
        .await?;

    let site_tz = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let timedisplay =
        crate::api::date_helper::resolve_user_timedisplay_name(&state.db, user_id, &site_tz).await;
    let tz: chrono_tz::Tz = timedisplay.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let bounds = crate::api::date_helper::get_timezone_time_bounds(tz);
    let today_date = bounds.today;
    let today_start_ts = bounds.today_start_ts;
    let history_end = today_date - chrono::Duration::days(1);

    let hist_stats: (Option<f64>, Option<i64>, Option<i64>) = sqlx::query_as(
        &state.db.format_query("SELECT SUM(total_cost), CAST(SUM(total_requests) AS BIGINT), CAST(SUM(success_count) AS BIGINT) FROM usage_daily_stats WHERE user_id = ? AND stat_date <= ?")
    )
    .bind(user_id)
    .bind(history_end)
    .fetch_one(&state.db.pool)
    .await?;

    let today_stats: (Option<f64>, Option<i64>, Option<i64>) = sqlx::query_as(
        &state.db.format_query("SELECT SUM(cost), COUNT(*), SUM(CASE WHEN status_code >= 200 AND status_code < 400 THEN 1 ELSE 0 END) FROM logs WHERE user_id = ? AND created_at >= ?::timestamptz")
    )
    .bind(user_id)
    .bind(&today_start_ts)
    .fetch_one(&state.db.pool)
    .await?;

    let total_consumption = hist_stats.0.unwrap_or(0.0) + today_stats.0.unwrap_or(0.0);
    let total_calls = hist_stats.1.unwrap_or(0) + today_stats.1.unwrap_or(0);
    let success_calls = hist_stats.2.unwrap_or(0) + today_stats.2.unwrap_or(0);

    let stats = (total_consumption, total_calls, success_calls);

    let affiliate_stats: (f64, i64) = sqlx::query_as(&state.db.format_query(
        r#"SELECT 
            commission_balance,
            (SELECT COUNT(*) FROM users WHERE referred_by = ?) as total_referred
           FROM users WHERE id = ?"#,
    ))
    .bind(user_id)
    .bind(user_id)
    .fetch_one(&state.db.pool)
    .await?;

    // 查询该用户等级的推广配置
    let level_marketing: Option<(i64, f64, f64, f64, String, f64, f64)> = sqlx::query_as(
        &state.db.format_query(
            "SELECT COALESCE(ul.marketing_enabled, 0), COALESCE(ul.commission_ratio, 0), COALESCE(ul.invite_reward_inviter, 0), COALESCE(ul.invite_reward_invitee, 0), COALESCE(NULLIF(ul.commission_basis, ''), 'consumption'), COALESCE(ul.commission_transfer_min, 0)::float8, COALESCE(ul.commission_transfer_max, 0)::float8 FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?"
        )
    )
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;

    let (
        marketing_enabled,
        commission_ratio,
        invite_reward_inviter,
        invite_reward_invitee,
        commission_basis,
        commission_transfer_min,
        commission_transfer_max,
    ) = match level_marketing {
        Some((m, c, ri, re, basis, tmin, tmax)) => (
            m == 1,
            c,
            ri,
            re,
            crate::services::affiliate::normalize_commission_basis(&basis).to_string(),
            tmin.max(0.0),
            tmax.max(0.0),
        ),
        None => (false, 0.0, 0.0, 0.0, "consumption".to_string(), 0.0, 0.0),
    };

    Ok(Json(WalletStats {
        balance,
        gift_balance,
        credit_limit,
        total_consumption: stats.0,
        total_calls: stats.1,
        success_calls: stats.2,
        commission_balance: affiliate_stats.0,
        total_referred: affiliate_stats.1,
        marketing_enabled,
        commission_ratio,
        invite_reward_inviter,
        invite_reward_invitee,
        commission_basis,
        commission_transfer_min,
        commission_transfer_max,
        pay_enabled: pay_enabled == 1,
    }))
}

#[derive(Debug, Deserialize)]
pub struct TransferCommissionRequest {
    pub amount: f64,
}

pub async fn transfer_commission(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Json(request): Json<TransferCommissionRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = &claims.sub;
    let mut tx = state.db.pool.begin().await?;

    let commission_balance: f64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT commission_balance FROM users WHERE id = ? FOR UPDATE"),
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;

    let limits: Option<(f64, f64)> = sqlx::query_as(&state.db.format_query(
        "SELECT COALESCE(ul.commission_transfer_min, 0), COALESCE(ul.commission_transfer_max, 0) \
         FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?",
    ))
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (transfer_min, transfer_max) = limits.unwrap_or((0.0, 0.0));
    if request.amount.is_nan() || request.amount.is_infinite() {
        return Err(AppError::BadRequest("请输入划转金额".to_string()));
    }
    let amount = match crate::services::affiliate::transfer_amount(
        request.amount,
        commission_balance,
        transfer_min,
        transfer_max,
    ) {
        Ok(amount) => amount,
        Err("below_min") => {
            return Err(AppError::BadRequest(format!(
                "单次最少划转 {}，且佣金钱包余额需达到该金额",
                crate::money::round_money(transfer_min)
            )));
        }
        Err("above_max") => {
            return Err(AppError::BadRequest(format!(
                "单次最多划转 {}",
                crate::money::round_money(transfer_max)
            )));
        }
        Err("over_balance") => {
            return Err(AppError::BadRequest(
                "划转金额不能超过佣金钱包余额".to_string(),
            ));
        }
        Err("conflict") => {
            return Err(AppError::BadRequest(
                "佣金钱包划转上限低于下限，请联系管理员".to_string(),
            ));
        }
        Err(_) => {
            return Err(AppError::BadRequest("请输入划转金额".to_string()));
        }
    };

    sqlx::query(&state.db.format_query(
        r#"UPDATE users SET 
            balance = balance + ?, 
            commission_balance = GREATEST(commission_balance - ?, 0), 
            updated_at = CURRENT_TIMESTAMP 
         WHERE id = ?"#,
    ))
    .bind(amount)
    .bind(amount)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        &state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark) VALUES (?, ?, 'transfer', ?)")
    ).bind(user_id).bind(amount).bind("佣金钱包划转到系统钱包")
    .execute(&mut *tx).await?;

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "amount": amount
    })))
}

#[derive(Debug, Deserialize)]
pub struct AffiliateOverviewQuery {
    pub referral_page: Option<i64>,
    pub referral_page_size: Option<i64>,
    pub ledger_page: Option<i64>,
    pub ledger_page_size: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AffiliateSummary {
    pub total_referred: i64,
    pub active_referred: i64,
    pub today_referred: i64,
    pub month_referred: i64,
    pub commission_earned: f64,
    pub commission_clawback: f64,
    pub commission_net: f64,
    pub today_net: f64,
    pub month_net: f64,
    pub source_consumption: f64,
    pub source_recharge: f64,
    pub source_video: f64,
    pub source_refund: f64,
    pub signup_reward: f64,
    pub signup_reward_count: i64,
    pub today_signup_reward: f64,
}

#[derive(Debug, Serialize)]
pub struct AffiliateReferral {
    pub uid: String,
    pub username: String,
    pub is_active: i64,
    pub created_at: String,
    pub recharge_total: f64,
    pub commission_net: f64,
    pub commission_count: i64,
}

#[derive(Debug, Serialize)]
pub struct AffiliateLedgerItem {
    pub id: i64,
    pub created_at: String,
    pub from_username: String,
    pub from_uid: String,
    pub source: String,
    pub ratio: f64,
    pub amount: f64,
    pub order_no: String,
}

#[derive(Debug, Serialize)]
pub struct AffiliatePage<T> {
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub items: Vec<T>,
}

#[derive(Debug, Serialize)]
pub struct AffiliateOverview {
    pub enabled: bool,
    pub marketing_enabled: bool,
    pub commission_ratio: f64,
    pub commission_basis: String,
    pub invite_reward_inviter: f64,
    pub invite_reward_invitee: f64,
    pub daily_invite_limit: i64,
    pub link_clicks: i64,
    pub commission_balance: f64,
    pub summary: AffiliateSummary,
    pub referrals: AffiliatePage<AffiliateReferral>,
    pub ledger: AffiliatePage<AffiliateLedgerItem>,
}

#[derive(Debug, sqlx::FromRow)]
struct AffiliateReferralRow {
    uid: String,
    username: String,
    is_active: i64,
    created_at: crate::time_system::DbTs,
    recharge_total: f64,
    commission_net: f64,
    commission_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct AffiliateLedgerRow {
    id: i64,
    amount: f64,
    ratio: f64,
    source_key: Option<String>,
    created_at: crate::time_system::DbTs,
    from_username: Option<String>,
    from_uid: Option<String>,
    order_no: Option<String>,
}

fn commission_source_kind(source_key: &str, amount: f64) -> &'static str {
    if source_key.starts_with("arkrefund:") || (amount < 0.0 && source_key.starts_with("ark")) {
        "refund"
    } else if source_key.starts_with("ark:") {
        "video"
    } else if source_key.starts_with("recharge:") {
        "recharge"
    } else if source_key.starts_with("log:") || source_key.starts_with("daycons:") {
        "consumption"
    } else if amount < 0.0 {
        "refund"
    } else {
        "other"
    }
}

fn empty_affiliate_overview() -> AffiliateOverview {
    AffiliateOverview {
        enabled: false,
        marketing_enabled: false,
        commission_ratio: 0.0,
        commission_basis: "consumption".to_string(),
        invite_reward_inviter: 0.0,
        invite_reward_invitee: 0.0,
        daily_invite_limit: 0,
        link_clicks: 0,
        commission_balance: 0.0,
        summary: AffiliateSummary {
            total_referred: 0,
            active_referred: 0,
            today_referred: 0,
            month_referred: 0,
            commission_earned: 0.0,
            commission_clawback: 0.0,
            commission_net: 0.0,
            today_net: 0.0,
            month_net: 0.0,
            source_consumption: 0.0,
            source_recharge: 0.0,
            source_video: 0.0,
            source_refund: 0.0,
            signup_reward: 0.0,
            signup_reward_count: 0,
            today_signup_reward: 0.0,
        },
        referrals: AffiliatePage {
            total: 0,
            page: 1,
            page_size: 20,
            items: Vec::new(),
        },
        ledger: AffiliatePage {
            total: 0,
            page: 1,
            page_size: 20,
            items: Vec::new(),
        },
    }
}

fn page_params(page: Option<i64>, page_size: Option<i64>) -> (i64, i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * page_size;
    (page, page_size, offset)
}

/// 邀请返利统计。等级开启专属推广，或返利比例大于 0 时才返回明细。
pub async fn get_affiliate_overview(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Query(query): Query<AffiliateOverviewQuery>,
) -> AppResult<Json<AffiliateOverview>> {
    let user_id = &claims.sub;
    let (referral_page, referral_page_size, referral_offset) =
        page_params(query.referral_page, query.referral_page_size);
    let (ledger_page, ledger_page_size, ledger_offset) =
        page_params(query.ledger_page, query.ledger_page_size);

    let identity: Option<(String, f64)> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT uid, commission_balance FROM users WHERE id = ?"),
    )
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;
    let Some((uid, commission_balance)) = identity else {
        return Ok(Json(empty_affiliate_overview()));
    };

    let level: Option<(i64, f64, f64, f64, i64, String)> = sqlx::query_as(&state.db.format_query(
        "SELECT COALESCE(ul.marketing_enabled, 0)::bigint, COALESCE(ul.commission_ratio, 0)::float8, COALESCE(ul.invite_reward_inviter, 0)::float8, COALESCE(ul.invite_reward_invitee, 0)::float8, COALESCE(ul.daily_invite_limit, 0)::bigint, COALESCE(NULLIF(ul.commission_basis, ''), 'consumption') FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?",
    ))
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;
    let Some((marketing_flag, ratio, reward_inviter, reward_invitee, daily_limit, basis_raw)) =
        level
    else {
        return Ok(Json(empty_affiliate_overview()));
    };
    let marketing_enabled = marketing_flag == 1;
    let commission_ratio = ratio.max(0.0);
    if !marketing_enabled && commission_ratio <= 0.0 {
        return Ok(Json(empty_affiliate_overview()));
    }
    let commission_basis =
        crate::services::affiliate::normalize_commission_basis(&basis_raw).to_string();

    let site_tz = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let timedisplay =
        crate::api::date_helper::resolve_user_timedisplay_name(&state.db, user_id, &site_tz).await;
    let tz: chrono_tz::Tz = timedisplay.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let now = chrono::Utc::now().with_timezone(&tz);
    let today = now.date_naive();
    let offset = now.format("%z").to_string();
    let stamp = |day: chrono::NaiveDate| format!("{} 00:00:00{}", day.format("%Y-%m-%d"), offset);
    let today_start = stamp(today);
    let tomorrow_start = stamp(today + chrono::Duration::days(1));
    let month_day =
        chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
    let month_start = stamp(month_day);

    let (total_referred, active_referred, today_referred, month_referred): (i64, i64, i64, i64) =
        sqlx::query_as(&state.db.format_query(
            "SELECT COUNT(*), COUNT(*) FILTER (WHERE is_active = 1), COUNT(*) FILTER (WHERE created_at >= ?::timestamptz AND created_at < ?::timestamptz), COUNT(*) FILTER (WHERE created_at >= ?::timestamptz AND created_at < ?::timestamptz) FROM users WHERE referred_by = ? OR referred_by = ?",
        ))
        .bind(&today_start)
        .bind(&tomorrow_start)
        .bind(&month_start)
        .bind(&tomorrow_start)
        .bind(user_id)
        .bind(&uid)
        .fetch_one(&state.db.pool)
        .await?;

    let (
        commission_earned,
        commission_clawback,
        commission_net,
        today_net,
        month_net,
        source_consumption,
        source_recharge,
        source_video,
        source_refund,
    ): (f64, f64, f64, f64, f64, f64, f64, f64, f64) = sqlx::query_as(&state.db.format_query(
        "SELECT \
            COALESCE(SUM(CASE WHEN amount > 0 THEN amount ELSE 0 END), 0)::float8, \
            COALESCE(SUM(CASE WHEN amount < 0 THEN -amount ELSE 0 END), 0)::float8, \
            COALESCE(SUM(amount), 0)::float8, \
            COALESCE(SUM(CASE WHEN created_at >= ?::timestamptz AND created_at < ?::timestamptz THEN amount ELSE 0 END), 0)::float8, \
            COALESCE(SUM(CASE WHEN created_at >= ?::timestamptz AND created_at < ?::timestamptz THEN amount ELSE 0 END), 0)::float8, \
            COALESCE(SUM(CASE WHEN source_key LIKE 'log:%' OR source_key LIKE 'daycons:%' THEN amount ELSE 0 END), 0)::float8, \
            COALESCE(SUM(CASE WHEN source_key LIKE 'recharge:%' THEN amount ELSE 0 END), 0)::float8, \
            COALESCE(SUM(CASE WHEN source_key LIKE 'ark:%' THEN amount ELSE 0 END), 0)::float8, \
            COALESCE(SUM(CASE WHEN source_key LIKE 'arkrefund:%' THEN amount ELSE 0 END), 0)::float8 \
         FROM commissions WHERE user_id = ?",
    ))
    .bind(&today_start)
    .bind(&tomorrow_start)
    .bind(&month_start)
    .bind(&tomorrow_start)
    .bind(user_id)
    .fetch_one(&state.db.pool)
    .await?;

    let (signup_reward, signup_reward_count, today_signup_reward): (f64, i64, f64) =
        sqlx::query_as(&state.db.format_query(
            "SELECT COALESCE(SUM(amount), 0)::float8, COUNT(*), COALESCE(SUM(CASE WHEN created_at >= ?::timestamptz AND created_at < ?::timestamptz THEN amount ELSE 0 END), 0)::float8 FROM recharge_records WHERE user_id = ? AND recharge_type = 'commission'",
        ))
        .bind(&today_start)
        .bind(&tomorrow_start)
        .bind(user_id)
        .fetch_one(&state.db.pool)
        .await?;

    let link_clicks: i64 = sqlx::query_scalar(&state.db.format_query(
        "SELECT COALESCE(SUM(click_count), 0)::bigint FROM marketing_link_click_stats WHERE link_type = 'invite' AND promoter_uid = ?",
    ))
    .bind(&uid)
    .fetch_one(&state.db.pool)
    .await?;

    let paid_recharge = "amount > 0 AND COALESCE(wallet_type, 'system') = 'system' AND recharge_type NOT IN ('gift', 'registration', 'commission', 'redemption', 'redemption_refund', 'model_voucher', 'coupon_purchase', 'coupon_pack', 'coupon_subscription', 'ark_video_consume', 'ark_video_refund')";
    let referrals: Vec<AffiliateReferralRow> = sqlx::query_as(&state.db.format_query(&format!(
        "SELECT u.uid, u.username, u.is_active, u.created_at, \
            COALESCE((SELECT SUM(rr.amount) FROM recharge_records rr WHERE rr.user_id = u.id AND {paid_recharge}), 0)::float8 AS recharge_total, \
            COALESCE((SELECT SUM(c.amount) FROM commissions c WHERE c.user_id = ? AND c.from_user_id = u.id), 0)::float8 AS commission_net, \
            COALESCE((SELECT COUNT(*) FROM commissions c WHERE c.user_id = ? AND c.from_user_id = u.id), 0) AS commission_count \
         FROM users u \
         WHERE u.referred_by = ? OR u.referred_by = ? \
         ORDER BY u.created_at DESC \
         LIMIT ? OFFSET ?"
    )))
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(&uid)
    .bind(referral_page_size)
    .bind(referral_offset)
    .fetch_all(&state.db.pool)
    .await?;

    let ledger_rows: Vec<AffiliateLedgerRow> = sqlx::query_as(&state.db.format_query(
        "SELECT c.id, c.amount, c.ratio, c.source_key, c.created_at, fu.username AS from_username, fu.uid AS from_uid, rr.order_no \
         FROM commissions c \
         LEFT JOIN users fu ON fu.id = c.from_user_id \
         LEFT JOIN recharge_records rr ON rr.id = c.recharge_id \
         WHERE c.user_id = ? \
         ORDER BY c.created_at DESC, c.id DESC \
         LIMIT ? OFFSET ?",
    ))
    .bind(user_id)
    .bind(ledger_page_size)
    .bind(ledger_offset)
    .fetch_all(&state.db.pool)
    .await?;

    let ledger_total: i64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT COUNT(*) FROM commissions WHERE user_id = ?"),
    )
    .bind(user_id)
    .fetch_one(&state.db.pool)
    .await?;

    let money = crate::money::round_money;
    Ok(Json(AffiliateOverview {
        enabled: true,
        marketing_enabled,
        commission_ratio,
        commission_basis,
        invite_reward_inviter: money(reward_inviter),
        invite_reward_invitee: money(reward_invitee),
        daily_invite_limit: daily_limit,
        link_clicks,
        commission_balance: money(commission_balance),
        summary: AffiliateSummary {
            total_referred,
            active_referred,
            today_referred,
            month_referred,
            commission_earned: money(commission_earned),
            commission_clawback: money(commission_clawback),
            commission_net: money(commission_net),
            today_net: money(today_net),
            month_net: money(month_net),
            source_consumption: money(source_consumption),
            source_recharge: money(source_recharge),
            source_video: money(source_video),
            source_refund: money(source_refund),
            signup_reward: money(signup_reward),
            signup_reward_count,
            today_signup_reward: money(today_signup_reward),
        },
        referrals: AffiliatePage {
            total: total_referred,
            page: referral_page,
            page_size: referral_page_size,
            items: referrals
                .into_iter()
                .map(|row| AffiliateReferral {
                    uid: row.uid,
                    username: row.username,
                    is_active: row.is_active,
                    created_at: row.created_at.into_string(),
                    recharge_total: money(row.recharge_total),
                    commission_net: money(row.commission_net),
                    commission_count: row.commission_count,
                })
                .collect(),
        },
        ledger: AffiliatePage {
            total: ledger_total,
            page: ledger_page,
            page_size: ledger_page_size,
            items: ledger_rows
                .into_iter()
                .map(|row| {
                    let key = row.source_key.unwrap_or_default();
                    AffiliateLedgerItem {
                        id: row.id,
                        created_at: row.created_at.into_string(),
                        from_username: row.from_username.unwrap_or_default(),
                        from_uid: row.from_uid.unwrap_or_default(),
                        source: commission_source_kind(&key, row.amount).to_string(),
                        ratio: row.ratio,
                        amount: money(row.amount),
                        order_no: row.order_no.unwrap_or_default(),
                    }
                })
                .collect(),
        },
    }))
}

pub async fn list_recharge_records(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<Vec<RechargeRecord>>> {
    let records: Vec<RechargeRecord> =
        sqlx::query_as(&state.db.format_query(
            "SELECT * FROM recharge_records WHERE user_id = ? ORDER BY created_at DESC",
        ))
        .bind(&claims.sub)
        .fetch_all(&state.db.pool)
        .await?;

    Ok(Json(records))
}

#[cfg(test)]
mod affiliate_overview_tests {
    use super::commission_source_kind;

    #[test]
    fn classifies_commission_sources() {
        assert_eq!(commission_source_kind("log:12", 1.0), "consumption");
        assert_eq!(
            commission_source_kind("daycons:u1:2026-10-03", 1.0),
            "consumption"
        );
        assert_eq!(
            commission_source_kind("daycons:inv:u1:2026-10-05", -1.0),
            "consumption"
        );
        assert_eq!(commission_source_kind("recharge:9", 1.0), "recharge");
        assert_eq!(commission_source_kind("ark:3", 1.0), "video");
        assert_eq!(commission_source_kind("arkrefund:4", -1.0), "refund");
        assert_eq!(commission_source_kind("manual", -0.5), "refund");
        assert_eq!(commission_source_kind("", 1.0), "other");
    }
}
