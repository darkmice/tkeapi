/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateUserRequest, LoginResponse, RechargeRequest, UpdateUserRequest, User, UserListResponse,
};
use crate::AppState;
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

fn collect_kyc_list_statuses(
    rows: impl IntoIterator<Item = (String, String, String)>,
) -> HashMap<String, (Option<String>, Option<String>)> {
    let mut flags: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for (user_id, kyc_type, status) in rows {
        if status == "none" || (kyc_type != "personal" && kyc_type != "enterprise") {
            continue;
        }
        let entry = flags.entry(user_id).or_insert((None, None));
        if kyc_type == "personal" {
            entry.0 = Some(status);
        } else {
            entry.1 = Some(status);
        }
    }
    flags
}

fn sync_user_remark(user: &mut User) {
    let admin_trimmed = user.admin_remark.as_deref().unwrap_or("").trim();
    if admin_trimmed.is_empty() {
        let referral_trimmed = user.remark.as_deref().unwrap_or("").trim();
        if !referral_trimmed.is_empty() {
            let synced = referral_trimmed.to_string();
            user.admin_remark = Some(synced.clone());
            user.remark = Some(synced);
        }
    } else if user.remark.as_deref().unwrap_or("").trim() != admin_trimmed {
        user.remark = Some(admin_trimmed.to_string());
    }
}

async fn attach_kyc_flags(state: &AppState, users: &mut [User]) -> AppResult<()> {
    if users.is_empty() {
        return Ok(());
    }
    // 直接扫描体量极小的有效 KYC 表并在内存匹配，免除数万 UUID 数组的分配与参数传输
    let rows: Vec<(String, String, String)> = sqlx::query_as(&state.db.format_query(
        "SELECT DISTINCT ON (user_id, kyc_type) user_id, kyc_type, status \
         FROM user_kyc \
         WHERE kyc_type IN ('personal', 'enterprise') AND status <> 'none' \
         ORDER BY user_id, kyc_type, COALESCE(submitted_at, updated_at) DESC, id DESC",
    ))
    .fetch_all(&state.db.pool)
    .await?;
    let flags = collect_kyc_list_statuses(rows);
    for user in users.iter_mut() {
        sync_user_remark(user);
        if let Some((personal, enterprise)) = flags.get(&user.id) {
            user.kyc_personal_status = personal.clone();
            user.kyc_enterprise_status = enterprise.clone();
            user.kyc_personal = personal.as_deref() == Some("approved");
            user.kyc_enterprise = enterprise.as_deref() == Some("approved");
        }
    }
    Ok(())
}

pub async fn list_users(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> AppResult<Json<UserListResponse>> {
    let keyword = query
        .get("keyword")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let mut users: Vec<User> = if let Some(kw) = keyword {
        let like_pattern = format!("%{}%", kw);
        let limit: i64 = query
            .get("limit")
            .and_then(|s| s.parse().ok())
            .unwrap_or(50)
            .clamp(1, 100);
        sqlx::query_as(&state.db.format_query(
            "SELECT u.*, ul.name as level_name, ul.id as level_id FROM users u \
             LEFT JOIN user_levels ul ON u.user_group = ul.group_key \
             WHERE u.role = 'user' AND (u.username LIKE ? OR u.uid LIKE ? OR u.email LIKE ? OR u.nickname LIKE ? OR COALESCE(u.mobile, '') LIKE ? OR COALESCE(u.wechat_id, '') LIKE ? OR COALESCE(u.wechat_name, '') LIKE ?) \
             ORDER BY u.created_at DESC LIMIT ?"
        ))
        .bind(&like_pattern)
        .bind(&like_pattern)
        .bind(&like_pattern)
        .bind(&like_pattern)
        .bind(&like_pattern)
        .bind(&like_pattern)
        .bind(&like_pattern)
        .bind(limit)
        .fetch_all(&state.db.pool)
        .await?
    } else {
        sqlx::query_as(&state.db.format_query(
            "SELECT u.*, ul.name as level_name, ul.id as level_id FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key ORDER BY u.created_at DESC"
        ))
        .fetch_all(&state.db.pool)
        .await?
    };

    attach_kyc_flags(&state, &mut users).await?;

    let total = users.len() as i64;
    Ok(Json(UserListResponse { data: users, total }))
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    axum::extract::Extension(claims): axum::extract::Extension<crate::auth::Claims>,
    axum::extract::Extension(ctx): axum::extract::Extension<crate::admin_permission::AdminContext>,
    Json(request): Json<CreateUserRequest>,
) -> AppResult<Json<User>> {
    let operator_name = claims.username.clone();
    let register_ip = crate::api::auth::extract_client_ip(&headers, &addr);
    crate::api::auth::validate_username(&request.username, false)?;
    let mut actual_email = crate::auth::contact::canonical_email(&request.email);
    if actual_email.is_empty() {
        use rand::Rng;
        let random_suffix: String = (0..8)
            .map(|_| rand::thread_rng().gen_range(0..10).to_string())
            .collect();
        actual_email = format!("u_{}@tokensbyte.local", random_suffix);
    }

    let role = request.role.as_deref().unwrap_or("user");
    crate::admin_permission::require_edit(&ctx, crate::admin_permission::users_write_perm(role))?;
    crate::admin_permission::assert_can_assign_admin_group(&state, request.admin_group_id, role)
        .await?;

    let exists: bool = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT EXISTS(SELECT 1 FROM users WHERE username = ?)"),
    )
    .bind(&request.username)
    .fetch_one(&state.db.pool)
    .await?;

    if exists {
        return Err(AppError::Conflict("请更换用户名尝试".to_string()));
    }

    let password_hash = auth::hash_password(&request.password)?;
    let user_id = uuid::Uuid::new_v4().to_string();
    let uid = state
        .db
        .generate_unique_uid()
        .await
        .map_err(AppError::from)?;

    let default_user_group: String;
    let user_group = match request
        .user_group
        .as_deref()
        .or(request.group.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(g) => g,
        None => {
            default_user_group = sqlx::query_scalar(&state.db.format_query(
                "SELECT group_key FROM user_levels WHERE is_default = 1 ORDER BY id ASC LIMIT 1",
            ))
            .fetch_optional(&state.db.pool)
            .await?
            .unwrap_or_else(|| "default".to_string());
            &default_user_group
        }
    };
    let admin_group_id = request.admin_group_id;
    let mut referred_by = request.referred_by.clone().or(request.aff.clone());

    // Resolve referred_by to ID if it's a UID or Username
    if let Some(ref ref_val) = referred_by {
        if !ref_val.trim().is_empty() {
            if ref_val == &request.username {
                return Err(AppError::BadRequest("不能设置自己为推荐人".to_string()));
            }

            let resolved: Option<(String, String)> = sqlx::query_as(&state.db.format_query(
                "SELECT id, role FROM users WHERE id = ? OR uid = ? OR username = ? LIMIT 1",
            ))
            .bind(ref_val)
            .bind(ref_val)
            .bind(ref_val)
            .fetch_optional(&state.db.pool)
            .await?;

            if let Some((id, ref_role)) = resolved {
                if role == "user" && ref_role == "admin" {
                    return Err(AppError::BadRequest("上级推荐人不能是管理员".to_string()));
                }
                referred_by = Some(id);
            }
        }
    }

    let referral_history = if let Some(ref inviter_id) = referred_by {
        let now = crate::time_system::utc_naive_string();
        let display_name = state.db.get_user_display_name(inviter_id).await;
        Some(format!("[{}] 通过 {} 邀请注册\n", now, display_name))
    } else {
        None
    };

    let balance = request.balance.unwrap_or(0.0);
    let gift_balance = request.gift_balance.unwrap_or(0.0);
    let mut first_name = None;
    let mut last_name = None;
    let mut birth_ym = None;
    crate::models::user::assign_profile_identity(
        &mut first_name,
        &mut last_name,
        &mut birth_ym,
        request.first_name,
        request.last_name,
        request.birth_ym,
    )
    .map_err(AppError::BadRequest)?;
    let pay_enabled = request.pay_enabled.unwrap_or(1);
    let settings = crate::api::settings::load_all_settings(&state).await?;

    let actual_mobile = request
        .mobile
        .as_deref()
        .map(crate::auth::contact::canonical_mobile_lenient)
        .filter(|s| !s.is_empty());
    let mut tx = state.db.pool.begin().await?;
    crate::auth::contact::assert_email_slot_tx(
        &mut tx,
        &state.db,
        &actual_email,
        None,
        crate::auth::contact::normalize_limit(settings.registration.max_accounts_per_email),
    )
    .await?;
    if let Some(ref mobile) = actual_mobile {
        crate::auth::contact::assert_mobile_slot_tx(
            &mut tx,
            &state.db,
            mobile,
            None,
            crate::auth::contact::normalize_limit(settings.registration.max_accounts_per_mobile),
        )
        .await?;
    }
    let actual_wechat_id = request
        .wechat_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    if let Some(ref wechat_id) = actual_wechat_id {
        crate::auth::contact::assert_wechat_slot_tx(
            &mut tx,
            &state.db,
            wechat_id,
            None,
            crate::auth::contact::normalize_limit(settings.registration.max_accounts_per_wechat),
        )
        .await?;
    }

    sqlx::query(
        &state.db.format_query(r#"INSERT INTO users (id, uid, username, email, mobile, wechat_id, password_hash, role, user_group, admin_group_id, balance, gift_balance, pay_enabled, is_active, referred_by, referral_history, register_ip, first_name, last_name, birth_ym)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?)"#)
    )
    .bind(&user_id)
    .bind(&uid)
    .bind(&request.username)
    .bind(&actual_email)
    .bind(&actual_mobile)
    .bind(&actual_wechat_id)
    .bind(&password_hash)
    .bind(role)
    .bind(user_group)
    .bind(admin_group_id)
    .bind(balance)
    .bind(gift_balance)
    .bind(pay_enabled)
    .bind(&referred_by)
    .bind(&referral_history)
    .bind(&register_ip)
    .bind(&first_name)
    .bind(&last_name)
    .bind(&birth_ym)
    .execute(&mut *tx)
    .await?;

    if balance > 0.0 {
        sqlx::query(
            &state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, operator, wallet_type) VALUES (?, ?, 'manual', ?, ?, 'system')")
        )
        .bind(&user_id)
        .bind(balance)
        .bind("管理员创建用户-系统余额")
        .bind(&operator_name)
        .execute(&mut *tx)
        .await?;
    }

    if gift_balance > 0.0 {
        let gift_order_no = crate::api::plugins::finance::generate_gift_order_no();
        sqlx::query(
            &state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, operator, wallet_type, order_no) VALUES (?, ?, 'manual', ?, ?, 'gift', ?)")
        )
        .bind(&user_id)
        .bind(gift_balance)
        .bind("管理员创建用户-赠送余额")
        .bind(&operator_name)
        .bind(&gift_order_no)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let user: User = sqlx::query_as(&state.db.format_query(
        "SELECT u.*, ul.name as level_name FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?"
    ))
    .bind(&user_id)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(user))
}

pub async fn get_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<User>> {
    let mut user: User = sqlx::query_as(&state.db.format_query(
        "SELECT u.*, ul.name as level_name FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ? OR u.uid = ?"
    ))
    .bind(&id)
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    sync_user_remark(&mut user);

    Ok(Json(user))
}

pub async fn update_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Extension(claims): axum::extract::Extension<crate::auth::Claims>,
    axum::extract::Extension(ctx): axum::extract::Extension<crate::admin_permission::AdminContext>,
    Json(request): Json<UpdateUserRequest>,
) -> AppResult<Json<User>> {
    let operator_name = claims.username.clone();
    let operator_id = claims.sub.clone();
    let mut user: User = sqlx::query_as(&state.db.format_query(
        "SELECT u.*, ul.name as level_name FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ? OR u.uid = ?"
    ))
    .bind(&id)
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    sync_user_remark(&mut user);

    let was_super_admin = crate::admin_permission::is_super_admin(&user.role, user.admin_group_id);
    let old_balance = user.balance;
    let old_gift_balance = user.gift_balance;
    let old_credit_limit = user.credit_limit;
    // 记录旧等级信息，用于变更时写日志
    let old_user_group = user.user_group.clone();
    let old_level_name: String = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT COALESCE(name, ?) FROM user_levels WHERE group_key = ?"),
    )
    .bind(&old_user_group)
    .bind(&old_user_group)
    .fetch_optional(&state.db.pool)
    .await?
    .flatten()
    .unwrap_or_else(|| old_user_group.clone());

    crate::admin_permission::assert_role_immutable(&user.role, request.role.as_deref())?;
    crate::admin_permission::assert_super_admin_immutable(
        &user.role,
        user.admin_group_id,
        request.role.as_deref(),
        request.admin_group_id,
    )?;
    crate::admin_permission::require_edit(
        &ctx,
        crate::admin_permission::users_write_perm(&user.role),
    )?;

    let old_email = user.email.clone();
    let old_mobile = user.mobile.clone();
    let old_wechat_id = user.wechat_id.clone();
    if let Some(username) = request.username.filter(|u| u != &user.username) {
        crate::api::auth::validate_username(&username, false)?;
        user.username = username;
    }
    if let Some(email) = request.email {
        let email = crate::auth::contact::canonical_email(&email);
        if email.is_empty() || !crate::auth::contact::is_real_email(&email) {
            if crate::auth::contact::is_real_email(&user.email) {
                user.email = format!("u_{}@tokensbyte.local", &user.uid);
            }
        } else {
            user.email = email;
        }
    }
    let mut password_changed = false;
    if let Some(password) = request.password {
        let trimmed = password.trim();
        if !trimmed.is_empty() {
            user.password_hash = auth::hash_password(trimmed)?;
            password_changed = true;
        }
    }
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
    if let Some(mobile) = request.mobile {
        let mobile = crate::auth::contact::canonical_mobile_lenient(&mobile);
        if mobile.is_empty() {
            user.mobile = None;
        } else {
            user.mobile = Some(mobile);
        }
    }
    if let Some(wechat_id) = request.wechat_id {
        let wechat_id = wechat_id.trim().to_string();
        let current = user.wechat_id.clone().unwrap_or_default();
        if wechat_id.is_empty() {
            user.wechat_id = None;
            user.wechat_name = None;
        } else {
            if wechat_id != current {
                user.wechat_name = None;
            }
            user.wechat_id = Some(wechat_id);
        }
    }
    if let Some(balance) = request.balance {
        user.balance = balance;
    }
    if let Some(gift_balance) = request.gift_balance {
        user.gift_balance = gift_balance;
    }
    if let Some(gift_used_quota) = request.gift_used_quota {
        user.gift_used_quota = gift_used_quota;
    }
    if let Some(user_group) = request.user_group {
        user.user_group = user_group;
    }
    if let Some(is_active) = request.is_active {
        user.is_active = is_active;
    }
    if let Some(admin_remark) = request.admin_remark {
        let trimmed = admin_remark.trim().to_string();
        user.admin_remark = Some(trimmed.clone());
        user.remark = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
    }
    if let Some(admin_group_opt) = request.admin_group_id {
        user.admin_group_id = admin_group_opt;
    }
    if user.role != "admin" {
        user.admin_group_id = None;
    } else if !was_super_admin {
        crate::admin_permission::assert_can_assign_admin_group(
            &state,
            user.admin_group_id,
            &user.role,
        )
        .await?;
    }
    // 用户模型单独折扣（空字符串视为清空，存 NULL；非空时校验 JSON 与数值 > 0）
    if let Some(ref md) = request.model_discounts {
        let trimmed = md.trim();
        if trimmed.is_empty() || trimmed == "{}" {
            user.model_discounts = None;
        } else {
            let map: HashMap<String, f64> = serde_json::from_str(trimmed).map_err(|_| {
                AppError::BadRequest("用户模型专属折扣格式无效".to_string())
            })?;
            for (mid, val) in &map {
                if mid.trim().is_empty() || !val.is_finite() || *val <= 0.0 || *val > 1000.0 {
                    return Err(AppError::BadRequest(
                        "用户模型专属折扣倍率必须大于 0 且不超过 1000".to_string(),
                    ));
                }
            }
            user.model_discounts = if map.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
    }
    if let Some(timezone) = request.timezone {
        user.timezone = Some(timezone);
    }
    if let Some(credit_limit) = request.credit_limit {
        user.credit_limit = credit_limit;
    }
    if let Some(pay_enabled) = request.pay_enabled {
        user.pay_enabled = pay_enabled;
    }
    let old_referred_by = user.referred_by.clone();

    if let Some(referred_by) = request.referred_by {
        let mut new_ref = if referred_by.trim().is_empty() {
            None
        } else {
            Some(referred_by.clone())
        };

        // Resolve referred_by to ID if it's a UID or Username
        if let Some(ref ref_val) = new_ref {
            let resolved: Option<(String, String)> = sqlx::query_as(&state.db.format_query(
                "SELECT id, role FROM users WHERE id = ? OR uid = ? OR username = ? LIMIT 1",
            ))
            .bind(ref_val)
            .bind(ref_val)
            .bind(ref_val)
            .fetch_optional(&state.db.pool)
            .await?;

            if let Some((id, ref_role)) = resolved {
                if user.role == "user" && ref_role == "admin" {
                    return Err(AppError::BadRequest("上级推荐人不能是管理员".to_string()));
                }
                new_ref = Some(id);
            }
        }

        // 检查不能设置自己为推荐人
        if let Some(ref ref_id) = new_ref {
            if ref_id == &user.id || ref_id == &user.uid || ref_id == &user.username {
                return Err(AppError::BadRequest("不能设置自己为推荐人".to_string()));
            }
        }

        if old_referred_by != new_ref {
            let now = crate::time_system::utc_naive_string();
            let old_str = state
                .db
                .get_user_display_name(&old_referred_by.unwrap_or_else(|| "无".to_string()))
                .await;
            let new_str = state
                .db
                .get_user_display_name(&new_ref.clone().unwrap_or_else(|| "无".to_string()))
                .await;
            let msg = format!("[{}] 推荐人从 {} 变更为 {}\n", now, old_str, new_str);
            let mut hist = user.referral_history.clone().unwrap_or_default();
            hist.push_str(&msg);
            user.referral_history = Some(hist);
        }
        user.referred_by = new_ref;
    }

    let email_changed = crate::auth::contact::is_real_email(&user.email)
        && !user.email.eq_ignore_ascii_case(&old_email);
    let mobile_changed = user
        .mobile
        .as_deref()
        .is_some_and(|m| !m.is_empty() && Some(m) != old_mobile.as_deref());
    let wechat_changed = user
        .wechat_id
        .as_deref()
        .is_some_and(|w| !w.is_empty() && Some(w) != old_wechat_id.as_deref());
    let reg_settings = if email_changed || mobile_changed || wechat_changed {
        Some(crate::api::settings::load_all_settings(&state).await?.registration)
    } else {
        None
    };

    let mut tx = state.db.pool.begin().await?;

    if let Some(ref reg) = reg_settings {
        if email_changed {
            crate::auth::contact::assert_email_slot_tx(
                &mut tx,
                &state.db,
                &user.email,
                Some(&user.id),
                crate::auth::contact::normalize_limit(reg.max_accounts_per_email),
            )
            .await?;
        }
        if let (true, Some(ref mobile)) = (mobile_changed, &user.mobile) {
            crate::auth::contact::assert_mobile_slot_tx(
                &mut tx,
                &state.db,
                mobile,
                Some(&user.id),
                crate::auth::contact::normalize_limit(reg.max_accounts_per_mobile),
            )
            .await?;
        }
        if let (true, Some(ref wechat_id)) = (wechat_changed, &user.wechat_id) {
            crate::auth::contact::assert_wechat_slot_tx(
                &mut tx,
                &state.db,
                wechat_id,
                Some(&user.id),
                crate::auth::contact::normalize_limit(reg.max_accounts_per_wechat),
            )
            .await?;
        }
    }

    sqlx::query(
        &state.db.format_query(r#"UPDATE users SET username = ?, email = ?, password_hash = ?, 
           nickname = ?, mobile = ?, wechat_id = ?, wechat_name = ?,
           role = ?, admin_group_id = ?, balance = ?, gift_balance = ?, gift_used_quota = ?, user_group = ?, is_active = ?, admin_remark = ?, remark = ?, referred_by = ?, referral_history = ?, model_discounts = ?, timezone = ?, credit_limit = ?, pay_enabled = ?, first_name = ?, last_name = ?, birth_ym = ?,
           password_changed_at = CASE WHEN ? THEN CURRENT_TIMESTAMP ELSE password_changed_at END,
           updated_at = CURRENT_TIMESTAMP
           WHERE id = ?"#)
    )
    .bind(&user.username)
    .bind(&user.email)
    .bind(&user.password_hash)
    .bind(&user.nickname)
    .bind(&user.mobile)
    .bind(&user.wechat_id)
    .bind(&user.wechat_name)
    .bind(&user.role)
    .bind(user.admin_group_id)
    .bind(user.balance)
    .bind(user.gift_balance)
    .bind(user.gift_used_quota)
    .bind(&user.user_group)
    .bind(user.is_active)
    .bind(&user.admin_remark)
    .bind(&user.remark)
    .bind(&user.referred_by)
    .bind(&user.referral_history)
    .bind(&user.model_discounts)
    .bind(&user.timezone)
    .bind(user.credit_limit)
    .bind(user.pay_enabled)
    .bind(&user.first_name)
    .bind(&user.last_name)
    .bind(&user.birth_ym)
    .bind(password_changed)
    .bind(&user.id)
    .execute(&mut *tx)
    .await?;

    // 系统钱包余额变动记录
    if (user.balance - old_balance).abs() > 1e-9 {
        let diff = user.balance - old_balance;
        let remark = if diff > 0.0 {
            "管理员调增系统余额"
        } else {
            "管理员调减系统余额"
        };
        sqlx::query(
            &state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, operator, wallet_type) VALUES (?, ?, 'manual', ?, ?, 'system')")
        )
        .bind(&user.id)
        .bind(diff)
        .bind(remark)
        .bind(&operator_name)
        .execute(&mut *tx)
        .await?;
    }

    // 赠送钱包余额变动记录
    if (user.gift_balance - old_gift_balance).abs() > 1e-9 {
        let diff = user.gift_balance - old_gift_balance;
        let remark = if diff > 0.0 {
            "管理员调增赠送余额"
        } else {
            "管理员调减赠送余额"
        };
        let gift_order_no = crate::api::plugins::finance::generate_gift_order_no();
        sqlx::query(
            &state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, operator, wallet_type, order_no) VALUES (?, ?, 'manual', ?, ?, 'gift', ?)")
        )
        .bind(&user.id)
        .bind(diff)
        .bind(remark)
        .bind(&operator_name)
        .bind(&gift_order_no)
        .execute(&mut *tx)
        .await?;
    }

    // 信控额度变动记录
    if (user.credit_limit - old_credit_limit).abs() > 1e-9 {
        let diff = user.credit_limit - old_credit_limit;
        let remark = if diff > 0.0 {
            "管理员调增信控额度"
        } else {
            "管理员调减信控额度"
        };
        sqlx::query(
            &state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, operator, wallet_type) VALUES (?, ?, 'manual', ?, ?, 'credit')")
        )
        .bind(&user.id)
        .bind(diff)
        .bind(remark)
        .bind(&operator_name)
        .execute(&mut *tx)
        .await?;
    }

    // 用户等级变更日志
    if user.user_group != old_user_group {
        let new_level_name: String = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT COALESCE(name, ?) FROM user_levels WHERE group_key = ?"),
        )
        .bind(&user.user_group)
        .bind(&user.user_group)
        .fetch_optional(&mut *tx)
        .await?
        .flatten()
        .unwrap_or_else(|| user.user_group.clone());

        sqlx::query(&state.db.format_query(
            "INSERT INTO user_level_logs (user_id, old_level, old_level_name, new_level, new_level_name, operator, operator_id, source) VALUES (?, ?, ?, ?, ?, ?, ?, 'admin')"
        ))
        .bind(&user.id)
        .bind(&old_user_group)
        .bind(&old_level_name)
        .bind(&user.user_group)
        .bind(&new_level_name)
        .bind(&operator_name)
        .bind(&operator_id)
        .execute(&mut *tx)
        .await?;
    }

    let balance_changed = (user.balance - old_balance).abs() > 1e-9
        || (user.gift_balance - old_gift_balance).abs() > 1e-9;

    tx.commit().await?;

    if balance_changed {
        crate::services::notification::spawn_low_balance_check(Arc::clone(&state), user.id.clone());
    }

    Ok(Json(user))
}

/// 管理员替换用户头像：写站点存储 `users/{uid}/avatar/`，成功后删除旧头像对象
pub async fn upload_user_avatar(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Extension(ctx): axum::extract::Extension<crate::admin_permission::AdminContext>,
    multipart: axum::extract::Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let (user_id, uid, role, old_avatar): (String, String, String, Option<String>) =
        sqlx::query_as(
            &state
                .db
                .format_query("SELECT id, uid, role, avatar FROM users WHERE id = ? OR uid = ?"),
        )
        .bind(&id)
        .bind(&id)
        .fetch_optional(&state.db.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    crate::admin_permission::require_edit(&ctx, crate::admin_permission::users_write_perm(&role))?;

    let avatar = store_user_avatar(&state, &user_id, &uid, old_avatar, multipart).await?;
    Ok(Json(serde_json::json!({ "avatar": avatar })))
}

/// 写站点存储 `users/{uid}/avatar/` 并更新 `users.avatar`，成功后删除旧头像对象
pub(crate) async fn store_user_avatar(
    state: &Arc<AppState>,
    user_id: &str,
    uid: &str,
    old_avatar: Option<String>,
    mut multipart: axum::extract::Multipart,
) -> AppResult<String> {
    use crate::services::object_store::upload_ext;

    let store = crate::relay::tos_persist::load_system_object_store(state)
        .await
        .ok_or_else(|| {
            AppError::BadRequest(
                "上传头像需要先在「站点设置 → 数据库/存储 → 存储设置」中配置对象存储".to_string(),
            )
        })?;

    let mut file: Option<(Vec<u8>, String, String)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("读取上传内容失败".to_string()))?
    {
        if field.name() == Some("file") {
            let mime = field.content_type().unwrap_or("").to_string();
            let name = field.file_name().unwrap_or("").to_string();
            let data = field
                .bytes()
                .await
                .map_err(|_| AppError::BadRequest("读取文件失败".to_string()))?;
            file = Some((data.to_vec(), mime, name));
        }
    }
    let (data, mime, name) =
        file.ok_or_else(|| AppError::BadRequest("请选择要上传的头像图片".to_string()))?;
    if !mime.starts_with("image/") {
        return Err(AppError::BadRequest("头像只支持图片格式".to_string()));
    }
    if data.len() > AVATAR_MAX_BYTES {
        return Err(AppError::BadRequest("头像图片不能超过 1MB".to_string()));
    }
    let ext = upload_ext(&name, "png");
    save_user_avatar(state, &store, user_id, uid, old_avatar, data, &mime, &ext).await
}

const AVATAR_MAX_BYTES: usize = 1024 * 1024;

#[allow(clippy::too_many_arguments)]
async fn save_user_avatar(
    state: &Arc<AppState>,
    store: &crate::services::object_store::ObjectStore,
    user_id: &str,
    uid: &str,
    old_avatar: Option<String>,
    data: Vec<u8>,
    mime: &str,
    ext: &str,
) -> AppResult<String> {
    use crate::services::object_store::UserFolder;

    let object_key = store.user_key(
        uid,
        UserFolder::Avatar,
        &format!("avatar_{}.{}", chrono::Utc::now().timestamp_millis(), ext),
    );
    let avatar = store
        .upload_file(&object_key, data, mime, None)
        .await
        .map_err(|e| {
            tracing::warn!("用户头像上传失败 {}: {}", object_key, e);
            AppError::Internal("头像上传失败，请稍后重试".to_string())
        })?;

    sqlx::query(
        &state.db.format_query(
            "UPDATE users SET avatar = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ),
    )
    .bind(&avatar)
    .bind(user_id)
    .execute(&state.db.pool)
    .await?;

    if let Some(old_key) = old_avatar
        .filter(|u| !u.is_empty() && *u != avatar)
        .and_then(|u| store.extract_object_key(&u))
    {
        if let Err(e) = store.delete_file(&old_key).await {
            tracing::warn!("旧头像删除失败 {}: {}", old_key, e);
        }
    }
    if let Err(e) = store
        .prune_user_folder(uid, UserFolder::Avatar, Some(&object_key))
        .await
    {
        tracing::warn!("用户 {} 旧头像目录清理失败: {}", uid, e);
    }

    Ok(avatar)
}

/// 三方登录/绑定带回的头像：用户还没有头像时后台导入，不阻塞登录跳转
pub(crate) fn spawn_oauth_avatar_import(
    state: &Arc<AppState>,
    user_id: &str,
    picture: Option<&str>,
) {
    let Some(url) = picture
        .map(str::trim)
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
    else {
        return;
    };
    let (state, user_id, url) = (state.clone(), user_id.to_string(), oauth_avatar_url(url));
    tokio::spawn(async move {
        if let Err(e) = import_oauth_avatar(&state, &user_id, &url).await {
            tracing::warn!("用户 {} 三方头像导入失败 {}: {}", user_id, url, e);
        }
    });
}

/// 谷歌头像默认 `=s96-c`，按头像统一尺寸取 256
fn oauth_avatar_url(url: &str) -> String {
    match url.rsplit_once("=s") {
        Some((base, size))
            if url.contains("googleusercontent.com")
                && size
                    .strip_suffix("-c")
                    .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) =>
        {
            format!("{base}=s256-c")
        }
        _ => url.to_string(),
    }
}

fn sniff_image(data: &[u8]) -> Option<(&'static str, &'static str)> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if data.starts_with(b"\x89PNG") {
        Some(("image/png", "png"))
    } else if data.starts_with(b"GIF8") {
        Some(("image/gif", "gif"))
    } else if data.len() > 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

async fn import_oauth_avatar(state: &Arc<AppState>, user_id: &str, url: &str) -> AppResult<()> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT uid, avatar FROM users WHERE id = ?"),
    )
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;
    let Some((uid, avatar)) = row else {
        return Ok(());
    };
    if avatar.is_some_and(|a| !a.trim().is_empty()) {
        return Ok(());
    }

    let Some(store) = crate::relay::tos_persist::load_system_object_store(state).await else {
        sqlx::query(&state.db.format_query(
            "UPDATE users SET avatar = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND (avatar IS NULL OR avatar = '')",
        ))
        .bind(url)
        .bind(user_id)
        .execute(&state.db.pool)
        .await?;
        return Ok(());
    };

    let data = crate::services::http_client::download_bytes(&state.http_client, url)
        .await
        .map_err(AppError::Internal)?;
    if data.len() > AVATAR_MAX_BYTES {
        return Err(AppError::Internal("三方头像超过 1MB".to_string()));
    }
    let (mime, ext) =
        sniff_image(&data).ok_or_else(|| AppError::Internal("三方头像不是图片".to_string()))?;
    save_user_avatar(state, &store, user_id, &uid, None, data, mime, ext).await?;
    Ok(())
}

/// 清空 `users.avatar`，并尽力删除站点存储里的旧头像对象
pub(crate) async fn clear_user_avatar(
    state: &Arc<AppState>,
    user_id: &str,
    uid: &str,
    old_avatar: Option<String>,
) -> AppResult<()> {
    sqlx::query(&state.db.format_query(
        "UPDATE users SET avatar = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    ))
    .bind(user_id)
    .execute(&state.db.pool)
    .await?;

    let Some(store) = crate::relay::tos_persist::load_system_object_store(state).await else {
        return Ok(());
    };
    if let Some(old_key) = old_avatar
        .filter(|u| !u.is_empty())
        .and_then(|u| store.extract_object_key(&u))
    {
        if let Err(e) = store.delete_file(&old_key).await {
            tracing::warn!("旧头像删除失败 {}: {}", old_key, e);
        }
    }
    if let Err(e) = store
        .prune_user_folder(uid, crate::services::object_store::UserFolder::Avatar, None)
        .await
    {
        tracing::warn!("用户 {} 头像目录清理失败: {}", uid, e);
    }
    Ok(())
}

async fn spawn_object_deletes(
    state: &AppState,
    plugin: &str,
    rows: Vec<(String, String)>,
    label: &'static str,
    tasks: &mut Vec<tokio::task::JoinHandle<()>>,
) {
    if rows.is_empty() {
        return;
    }
    use crate::services::object_store::StoreKind;
    let all = crate::api::plugins::collect_object_stores(state, plugin).await;
    for (key, provider) in rows {
        let kind = StoreKind::parse(&provider);
        let stores: Vec<_> = all
            .iter()
            .filter(|s| StoreKind::parse(s.provider()) == kind)
            .cloned()
            .collect();
        if stores.is_empty() {
            continue;
        }
        tasks.push(tokio::spawn(async move {
            for store in stores {
                match store.delete_file(&key).await {
                    Ok(()) => tracing::info!("同步清理用户数据: {label} 文件删除成功: {key}"),
                    Err(e) => {
                        tracing::warn!("同步清理用户数据: {label} 文件删除失败: {key} - {e}")
                    }
                }
            }
        }));
    }
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Extension(claims): axum::extract::Extension<crate::auth::Claims>,
    axum::extract::Extension(ctx): axum::extract::Extension<crate::admin_permission::AdminContext>,
) -> AppResult<Json<serde_json::Value>> {
    let target: Option<(String, String, String, Option<i64>)> =
        sqlx::query_as(&state.db.format_query(
            "SELECT id, uid, role, admin_group_id FROM users WHERE id = ? OR uid = ?",
        ))
        .bind(&id)
        .bind(&id)
        .fetch_optional(&state.db.pool)
        .await?;
    let (target_id, target_uid, role, gid) =
        target.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // 防止管理员删除自己
    if claims.sub == target_id {
        return Err(AppError::BadRequest(
            "不能删除当前登录的管理员账户".to_string(),
        ));
    }

    if crate::admin_permission::is_super_admin(&role, gid) {
        return Err(AppError::BadRequest("超级管理员不可删除".to_string()));
    }
    crate::admin_permission::require_edit(&ctx, crate::admin_permission::users_write_perm(&role))?;

    // 0. 在事务开启前，先查出该用户在 plugin_assets 与 playground_assets 中上传的全部云端文件，
    //    以进行同步且并发的物理清理，避免云端对象存储产生孤儿垃圾文件。
    //    注：不可在下面的数据库事务内执行网络 IO，以避免长时间占用数据库连接。
    let assets: Vec<(String, String, String)> = sqlx::query_as::<_, (String, String, String)>(
        &state
            .db
            .format_query("SELECT file_url, plugin_ns, COALESCE(storage_provider, 'tos') FROM plugin_assets WHERE user_id = ?"),
    )
    .bind(&target_id)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    let pg_assets: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        &state.db.format_query("SELECT tos_object_key, COALESCE(storage_provider, 'tos') FROM playground_assets WHERE user_id = ? AND tos_object_key IS NOT NULL AND tos_object_key != ''")
    )
    .bind(&target_id)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    let mut pg2026_all: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        &state.db.format_query("SELECT tos_object_key, COALESCE(storage_provider, 'tos') FROM playground_2026_project_assets WHERE user_id = ? AND tos_object_key IS NOT NULL AND tos_object_key != ''")
    )
    .bind(&target_id)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();
    let pg2026_lib: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        &state.db.format_query("SELECT tos_object_key, COALESCE(storage_provider, 'tos') FROM playground_2026_assets WHERE user_id = ? AND tos_object_key IS NOT NULL AND tos_object_key != ''")
    )
    .bind(&target_id)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();
    pg2026_all.extend(pg2026_lib);

    if !assets.is_empty() || !pg_assets.is_empty() || !pg2026_all.is_empty() {
        // 并发生成 TOS 文件删除任务
        let mut delete_tasks = Vec::new();

        // 1. 处理 plugin_assets 关联的 TOS 文件删除 (包含国内版与国际版素材管理等插件)
        if !assets.is_empty() {
            // 缓存不同 plugin_ns 的 TosConfig，避免重复查库
            let mut stores = std::collections::HashMap::new();
            for ns in assets
                .iter()
                .map(|(_, ns, _)| ns.as_str())
                .collect::<std::collections::HashSet<_>>()
            {
                stores.insert(
                    ns.to_string(),
                    crate::api::plugins::collect_object_stores(&state, ns).await,
                );
            }

            for (file_url, plugin_ns, provider) in assets {
                let kind = crate::services::object_store::StoreKind::parse(&provider);
                let Some(list) = stores.get(&plugin_ns) else {
                    continue;
                };
                let matched = list
                    .iter()
                    .find(|s| {
                        crate::services::object_store::StoreKind::parse(s.provider()) == kind
                            && s.extract_object_key(&file_url).is_some()
                    })
                    .cloned()
                    .or_else(|| {
                        list.iter()
                            .find(|s| {
                                crate::services::object_store::StoreKind::parse(s.provider())
                                    == kind
                            })
                            .cloned()
                    });
                if let Some(tos_config) = matched {
                    if let Some(object_key) = tos_config.extract_object_key(&file_url) {
                        let task = tokio::spawn(async move {
                            match tos_config.delete_file(&object_key).await {
                                Ok(()) => {
                                    tracing::info!(
                                        "同步清理用户数据: plugin_assets 文件删除成功: {}",
                                        object_key
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "同步清理用户数据: plugin_assets 文件删除失败: {} - {}",
                                        object_key,
                                        e
                                    );
                                }
                            }
                        });
                        delete_tasks.push(task);
                    }
                }
            }
        }

        spawn_object_deletes(
            &state,
            "playground",
            pg_assets,
            "playground_assets",
            &mut delete_tasks,
        )
        .await;
        spawn_object_deletes(
            &state,
            "playground_2026",
            pg2026_all,
            "playground_2026_project_assets",
            &mut delete_tasks,
        )
        .await;

        // 超时保障机制：使用 tokio::time::timeout 给并发执行 of tasks 设置一个 30 秒的最大时间上限。
        // 即使云端网络异常卡死，接口也将在 30 秒内强制返回并继续主流程，绝不阻塞用户删除接口的正常响应。
        if !delete_tasks.is_empty() {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                futures::future::join_all(delete_tasks),
            )
            .await;
        }
    }

    if let Some(store) = crate::relay::tos_persist::load_system_object_store(&state).await {
        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            store.prune_user_folder(
                &target_uid,
                crate::services::object_store::UserFolder::Avatar,
                None,
            ),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("删除用户 {} 头像目录清理失败: {}", target_uid, e),
            Err(_) => tracing::warn!("删除用户 {} 头像目录清理超时", target_uid),
        }
    }

    // 使用事务，按外键依赖顺序逐层清理关联数据
    let mut tx = state.db.pool.begin().await?;

    // 1. commissions 引用 recharge_records(id) 和 users(id)，必须最先删
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM commissions WHERE user_id = ? OR from_user_id = ?"),
    )
    .bind(&target_id)
    .bind(&target_id)
    .execute(&mut *tx)
    .await?;

    // 2. recharge_records 引用 users(id)
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM recharge_records WHERE user_id = ?"),
    )
    .bind(&target_id)
    .execute(&mut *tx)
    .await?;

    // 3. api_tokens 引用 users(id)
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM api_tokens WHERE user_id = ?"),
    )
    .bind(&target_id)
    .execute(&mut *tx)
    .await?;

    // 4. orders 引用 users(id)
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM orders WHERE user_id = ?"),
    )
    .bind(&target_id)
    .execute(&mut *tx)
    .await?;

    // 5. plugin_assets 引用 users(id)
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM plugin_assets WHERE user_id = ?"),
    )
    .bind(&target_id)
    .execute(&mut *tx)
    .await?;

    // 6. plugin_asset_groups 引用 users(id)
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM plugin_asset_groups WHERE user_id = ?"),
    )
    .bind(&target_id)
    .execute(&mut *tx)
    .await?;

    // 7. 无外键但需清理的业务数据
    sqlx::query(&state.db.format_query("DELETE FROM logs WHERE user_id = ?"))
        .bind(&target_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM plugin_api_logs WHERE user_id = ?"),
    )
    .bind(&target_id)
    .execute(&mut *tx)
    .await?;
    // marketing 关联（无外键但需清理）
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM marketing_team_leaders WHERE user_id = ?"),
    )
    .bind(&target_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM marketing_team_members WHERE user_id = ?"),
    )
    .bind(&target_id)
    .execute(&mut *tx)
    .await?;

    // 8. 最终删除用户主记录（playground_projects / playground_assets 已有 ON DELETE CASCADE）
    sqlx::query(&state.db.format_query("DELETE FROM users WHERE id = ?"))
        .bind(&target_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Json(serde_json::json!({ "success": true })))
}
pub async fn recharge_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Extension(claims): axum::extract::Extension<crate::auth::Claims>,
    axum::extract::Extension(ctx): axum::extract::Extension<crate::admin_permission::AdminContext>,
    Json(request): Json<RechargeRequest>,
) -> AppResult<Json<RechargeOutcome>> {
    if request.amount.is_nan() || request.amount.is_infinite() {
        return Err(AppError::BadRequest("无效的金额数值".to_string()));
    }
    let request_amount = crate::money::round_money(request.amount);

    let operator_name = claims.username.clone();
    let mut tx = state.db.pool.begin().await?;

    let user: User = sqlx::query_as(&state.db.format_query(
        "SELECT u.*, ul.name as level_name FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ? OR u.uid = ?"
    ))
    .bind(&id)
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    crate::admin_permission::require_edit(
        &ctx,
        crate::admin_permission::users_write_perm(&user.role),
    )?;

    let remark = request
        .remark
        .unwrap_or_else(|| "Administrator Adjustment".to_string());
    let is_gift = request.wallet_type == "gift";
    let is_credit = request.wallet_type == "credit";

    if is_credit {
        // 信控额度操作
        let new_credit = user.credit_limit + request_amount;
        if new_credit < 0.0 {
            return Err(AppError::BadRequest("信控额度不能为负数".to_string()));
        }
        sqlx::query(&state.db.format_query(
            "UPDATE users SET credit_limit = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ))
        .bind(new_credit)
        .bind(&user.id)
        .execute(&mut *tx)
        .await?;
    } else if is_gift {
        // 赠送钱包操作
        let new_gift = user.gift_balance + request_amount;
        sqlx::query(&state.db.format_query(
            "UPDATE users SET gift_balance = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ))
        .bind(new_gift)
        .bind(&user.id)
        .execute(&mut *tx)
        .await?;
    } else {
        // 系统钱包操作
        let new_balance = user.balance + request_amount;
        sqlx::query(&state.db.format_query(
            "UPDATE users SET balance = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ))
        .bind(new_balance)
        .bind(&user.id)
        .execute(&mut *tx)
        .await?;
    }

    let recharge_type = "manual";
    let wallet_type = if is_credit {
        "credit"
    } else if is_gift {
        "gift"
    } else {
        "system"
    };
    let order_no = if is_gift {
        crate::api::plugins::finance::generate_gift_order_no()
    } else {
        crate::api::plugins::finance::generate_system_order_no()
    };
    let recharge_id: i64 = sqlx::query_scalar::<_, i64>(&state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, operator, wallet_type, order_no) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id"))
        .bind(&user.id)
        .bind(request_amount)
        .bind(recharge_type)
        .bind(&remark)
        .bind(&operator_name)
        .bind(wallet_type)
        .bind(&order_no)
        .fetch_one(&mut *tx)
        .await?;

    let is_system = !is_gift && !is_credit;
    let mut commission_applied = 0.0;
    let commission_capped = false;
    let mut commission_from_wallet = 0.0;
    let mut commission_from_system = 0.0;
    if is_system && request_amount > 0.0 && request.apply_commission.unwrap_or(true) {
        match crate::services::affiliate::award_recharge_commission(
            &state.db,
            &mut tx,
            &user.id,
            recharge_id,
            request_amount,
        )
        .await
        {
            Ok(credited) => commission_applied = credited,
            Err(e) => {
                tracing::warn!(
                    "Failed to award commission for recharge {}: {}",
                    recharge_id,
                    e
                );
            }
        }
    }
    if is_system && request_amount < 0.0 {
        crate::services::affiliate::note_system_spend(
            &state.db,
            &mut tx,
            &user.id,
            -request_amount,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    }
    if is_system && request_amount < 0.0 && request.apply_commission.unwrap_or(false) {
        let system_order_no = crate::api::plugins::finance::generate_system_order_no();
        match crate::services::affiliate::clawback_admin_commission(
            &state.db,
            &mut tx,
            &user.id,
            recharge_id,
            -request_amount,
            &operator_name,
            &user.username,
            &system_order_no,
        )
        .await
        {
            Ok(cut) => {
                commission_from_wallet = cut.from_commission;
                commission_from_system = cut.from_system;
                commission_applied =
                    crate::money::round_money(-(cut.from_commission + cut.from_system));
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to claw back commission for recharge {}: {}",
                    recharge_id,
                    e
                );
            }
        }
    }

    tx.commit().await?;

    // 扣减可能触发提醒；充值回升需清除本轮标记
    crate::services::notification::spawn_low_balance_check(Arc::clone(&state), user.id.clone());

    let updated_user: User = sqlx::query_as(&state.db.format_query(
        "SELECT u.*, ul.name as level_name FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?"
    ))
    .bind(&user.id)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(RechargeOutcome {
        user: updated_user,
        commission_applied,
        commission_capped,
        commission_from_wallet,
        commission_from_system,
    }))
}

#[derive(Debug, Serialize)]
pub struct RechargeOutcome {
    #[serde(flatten)]
    pub user: User,
    pub commission_applied: f64,
    pub commission_capped: bool,
    /// 本次从上级佣金钱包扣回的正数。
    pub commission_from_wallet: f64,
    /// 本次从上级系统钱包扣回的正数。
    pub commission_from_system: f64,
}

#[derive(Debug, Deserialize)]
pub struct CommissionPreviewQuery {
    pub amount: f64,
}

#[derive(Debug, Serialize)]
pub struct CommissionPreviewResponse {
    pub eligible: bool,
    pub direction: String,
    pub inviter_username: String,
    pub inviter_uid: String,
    pub ratio: f64,
    pub basis: String,
    pub suggested: f64,
    pub inviter_commission_balance: f64,
    pub inviter_system_balance: f64,
    pub from_commission: f64,
    pub from_system: f64,
    pub actual: f64,
    pub default_apply: bool,
}

pub async fn preview_commission_adjust(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<CommissionPreviewQuery>,
    axum::extract::Extension(claims): axum::extract::Extension<crate::auth::Claims>,
) -> AppResult<Json<CommissionPreviewResponse>> {
    if query.amount.is_nan() || query.amount.is_infinite() {
        return Err(AppError::BadRequest("无效的金额数值".to_string()));
    }
    let user: User = sqlx::query_as(&state.db.format_query(
        "SELECT u.*, ul.name as level_name FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ? OR u.uid = ?",
    ))
    .bind(&id)
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    let ctx = crate::admin_permission::load_admin_context(&state, &claims.sub).await?;
    crate::admin_permission::require_edit(
        &ctx,
        crate::admin_permission::users_write_perm(&user.role),
    )?;

    let row: Option<(String, String, f64, f64, f64, String, i64)> = sqlx::query_as(&state.db.format_query(
        "SELECT inv.username, inv.uid, inv.commission_balance, inv.balance, ul.commission_ratio, COALESCE(NULLIF(ul.commission_basis, ''), 'consumption'), COALESCE(ul.marketing_enabled, 0)::bigint \
         FROM users u \
         JOIN users inv ON (inv.id = u.referred_by OR inv.uid = u.referred_by) \
         JOIN user_levels ul ON ul.group_key = inv.user_group \
         WHERE u.id = ? \
         ORDER BY CASE WHEN inv.id = u.referred_by THEN 0 ELSE 1 END \
         LIMIT 1",
    ))
    .bind(&user.id)
    .fetch_optional(&state.db.pool)
    .await?;

    let Some((username, uid, commission_balance, system_balance, ratio, basis, marketing_enabled)) =
        row
    else {
        return Ok(Json(CommissionPreviewResponse {
            eligible: false,
            direction: "none".to_string(),
            inviter_username: String::new(),
            inviter_uid: String::new(),
            ratio: 0.0,
            basis: "consumption".to_string(),
            suggested: 0.0,
            inviter_commission_balance: 0.0,
            inviter_system_balance: 0.0,
            from_commission: 0.0,
            from_system: 0.0,
            actual: 0.0,
            default_apply: false,
        }));
    };
    if query.amount < 0.0 {
        let recorded = crate::services::affiliate::quote_recorded_clawback(
            &state.db,
            &user.id,
            -query.amount,
            &basis,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        return Ok(Json(CommissionPreviewResponse {
            eligible: recorded.eligible,
            direction: if recorded.eligible {
                "clawback".to_string()
            } else {
                "none".to_string()
            },
            inviter_username: recorded.inviter_names,
            inviter_uid: uid,
            ratio,
            basis: crate::services::affiliate::normalize_commission_basis(&basis).to_string(),
            suggested: recorded.suggested,
            inviter_commission_balance: recorded.commission_balance,
            inviter_system_balance: recorded.system_balance,
            from_commission: recorded.from_commission,
            from_system: recorded.from_system,
            actual: recorded.actual,
            default_apply: recorded.default_apply,
        }));
    }
    let mut quote =
        crate::services::affiliate::quote_adjust(query.amount, ratio, &basis, commission_balance);
    if marketing_enabled != 1 {
        quote.eligible = false;
        quote.direction = "none";
        quote.suggested = 0.0;
        quote.actual = 0.0;
        quote.from_commission = 0.0;
        quote.from_system = 0.0;
        quote.default_apply = false;
    }
    Ok(Json(CommissionPreviewResponse {
        eligible: quote.eligible,
        direction: quote.direction.to_string(),
        inviter_username: username,
        inviter_uid: uid,
        ratio,
        basis: crate::services::affiliate::normalize_commission_basis(&basis).to_string(),
        suggested: quote.suggested,
        inviter_commission_balance: crate::money::round_money(commission_balance),
        inviter_system_balance: crate::money::round_money(system_balance),
        from_commission: quote.from_commission,
        from_system: quote.from_system,
        actual: quote.actual,
        default_apply: quote.default_apply,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CommissionLedgerQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CommissionLedgerItem {
    pub id: String,
    pub created_at: String,
    pub amount: f64,
    pub ratio: f64,
    pub source: String,
    pub source_label: String,
    pub from_username: String,
    pub from_uid: String,
}

#[derive(Debug, Serialize)]
pub struct CommissionLedgerResponse {
    pub commission_balance: f64,
    pub period_earned: f64,
    pub period_clawback: f64,
    pub period_transfer: f64,
    pub data: Vec<CommissionLedgerItem>,
}

fn commission_ledger_label(source_key: &str, amount: f64) -> (&'static str, &'static str) {
    if source_key.starts_with("adjustclaw:") || source_key.starts_with("claw:adjust:") {
        ("adjust", "余额调减扣回")
    } else if source_key.starts_with("claw:") {
        ("refund", "退款回冲")
    } else if source_key.starts_with("daycons:") || source_key.starts_with("log:") {
        ("consumption", "消费返佣")
    } else if source_key.starts_with("arkrefund:")
        || (amount < 0.0 && source_key.starts_with("ark:"))
    {
        ("refund", "退款回冲")
    } else if source_key.starts_with("ark:") {
        ("video", "视频返佣")
    } else if source_key.starts_with("recharge:") {
        ("recharge", "充值返佣")
    } else if amount < 0.0 {
        ("refund", "回冲")
    } else {
        ("other", "其他")
    }
}

fn append_ledger_range(
    sql: &mut String,
    binds: &mut Vec<String>,
    field: &str,
    start: Option<&str>,
    end: Option<&str>,
    tz: chrono_tz::Tz,
) {
    if let Some(start) = start.filter(|s| !s.trim().is_empty()) {
        crate::api::date_helper::push_timestamptz_bound(sql, binds, field, start, false, tz);
    }
    if let Some(end) = end.filter(|s| !s.trim().is_empty()) {
        crate::api::date_helper::push_timestamptz_bound(sql, binds, field, end, true, tz);
    }
}

/// 管理端钱包明细：佣金入账、回冲，以及划转到系统钱包的流出。
pub async fn list_user_commission_ledger(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<CommissionLedgerQuery>,
    axum::extract::Extension(claims): axum::extract::Extension<crate::auth::Claims>,
) -> AppResult<Json<CommissionLedgerResponse>> {
    crate::admin_permission::load_admin_context(&state, &claims.sub).await?;
    let user_id: String = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM users WHERE id = ? OR uid = ? LIMIT 1"),
    )
    .bind(&id)
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let commission_balance: f64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT commission_balance FROM users WHERE id = ?"),
    )
    .bind(&user_id)
    .fetch_one(&state.db.pool)
    .await?;

    let site_tz = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let tz = crate::time_system::parse_timedisplay(&site_tz);
    let start = query.start_date.as_deref();
    let end = query.end_date.as_deref();

    let mut comm_where = " WHERE c.user_id = ?".to_string();
    let mut comm_binds = vec![user_id.clone()];
    append_ledger_range(
        &mut comm_where,
        &mut comm_binds,
        "c.created_at",
        start,
        end,
        tz,
    );

    let sum_sql = state.db.format_query(&format!(
        "SELECT COALESCE(SUM(CASE WHEN c.amount > 0 THEN c.amount ELSE 0 END), 0)::float8, \
         COALESCE(SUM(CASE WHEN c.amount < 0 THEN -c.amount ELSE 0 END), 0)::float8 \
         FROM commissions c{comm_where}"
    ));
    let mut sum_q = sqlx::query_as::<_, (f64, f64)>(&sum_sql);
    for val in &comm_binds {
        sum_q = sum_q.bind(val);
    }
    let (period_earned, period_clawback) = sum_q.fetch_one(&state.db.pool).await?;

    #[derive(sqlx::FromRow)]
    struct CommissionRow {
        id: i64,
        amount: f64,
        ratio: f64,
        source_key: Option<String>,
        created_at: crate::time_system::DbTs,
        from_username: Option<String>,
        from_uid: Option<String>,
    }
    let list_sql = state.db.format_query(&format!(
        "SELECT c.id, c.amount, c.ratio, c.source_key, c.created_at, fu.username AS from_username, fu.uid AS from_uid \
         FROM commissions c \
         LEFT JOIN users fu ON fu.id = c.from_user_id \
         {comm_where} \
         ORDER BY c.created_at DESC, c.id DESC \
         LIMIT 500"
    ));
    let mut list_q = sqlx::query_as::<_, CommissionRow>(&list_sql);
    for val in &comm_binds {
        list_q = list_q.bind(val);
    }
    let commission_rows = list_q.fetch_all(&state.db.pool).await?;

    let mut xfer_where = " WHERE rr.user_id = ? AND rr.recharge_type = 'transfer'".to_string();
    let mut xfer_binds = vec![user_id.clone()];
    append_ledger_range(
        &mut xfer_where,
        &mut xfer_binds,
        "rr.created_at",
        start,
        end,
        tz,
    );
    let xfer_sum_sql = state.db.format_query(&format!(
        "SELECT COALESCE(SUM(rr.amount), 0)::float8 FROM recharge_records rr{xfer_where}"
    ));
    let mut xfer_sum_q = sqlx::query_scalar::<_, f64>(&xfer_sum_sql);
    for val in &xfer_binds {
        xfer_sum_q = xfer_sum_q.bind(val);
    }
    let period_transfer = xfer_sum_q.fetch_one(&state.db.pool).await?;

    #[derive(sqlx::FromRow)]
    struct TransferRow {
        id: i64,
        amount: f64,
        created_at: crate::time_system::DbTs,
    }
    let xfer_sql = state.db.format_query(&format!(
        "SELECT rr.id, rr.amount, rr.created_at FROM recharge_records rr{xfer_where} \
         ORDER BY rr.created_at DESC, rr.id DESC LIMIT 500"
    ));
    let mut xfer_q = sqlx::query_as::<_, TransferRow>(&xfer_sql);
    for val in &xfer_binds {
        xfer_q = xfer_q.bind(val);
    }
    let transfer_rows = xfer_q.fetch_all(&state.db.pool).await?;

    let money = crate::money::round_money;
    let mut data: Vec<CommissionLedgerItem> = commission_rows
        .into_iter()
        .map(|row| {
            let key = row.source_key.unwrap_or_default();
            let (source, source_label) = commission_ledger_label(&key, row.amount);
            CommissionLedgerItem {
                id: format!("c:{}", row.id),
                created_at: row.created_at.into_string(),
                amount: money(row.amount),
                ratio: row.ratio,
                source: source.to_string(),
                source_label: source_label.to_string(),
                from_username: row.from_username.unwrap_or_default(),
                from_uid: row.from_uid.unwrap_or_default(),
            }
        })
        .collect();
    data.extend(transfer_rows.into_iter().map(|row| CommissionLedgerItem {
        id: format!("t:{}", row.id),
        created_at: row.created_at.into_string(),
        amount: money(-row.amount),
        ratio: 0.0,
        source: "transfer".to_string(),
        source_label: "划转到系统钱包".to_string(),
        from_username: String::new(),
        from_uid: String::new(),
    }));
    data.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    data.truncate(500);

    Ok(Json(CommissionLedgerResponse {
        commission_balance: money(commission_balance),
        period_earned: money(period_earned),
        period_clawback: money(period_clawback),
        period_transfer: money(period_transfer),
        data,
    }))
}

pub async fn impersonate_user(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(claims): axum::extract::Extension<crate::auth::Claims>,
    axum::extract::Extension(ctx): axum::extract::Extension<crate::admin_permission::AdminContext>,
    Path(id): Path<String>,
) -> AppResult<Json<LoginResponse>> {
    // 防御纵深：路由层已有 auth + admin_middleware，handler 再强制校验管理员身份
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }
    crate::admin_permission::require_edit(&ctx, "users.list")?;

    let mut user: User = sqlx::query_as(&state.db.format_query(
        "SELECT u.*, ul.name as level_name FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ? OR u.uid = ?"
    ))
    .bind(&id)
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    if user.role != "user" {
        return Err(AppError::Forbidden(
            "Only normal users can be impersonated".to_string(),
        ));
    }

    if user.is_active == 0 {
        return Err(AppError::Forbidden("Account disabled".to_string()));
    }

    tracing::warn!(
        "[Impersonate] admin={} ({}) impersonating user={} ({})",
        claims.username,
        claims.sub,
        user.username,
        user.id
    );

    crate::api::auth::sanitize_user_for_client(&mut user);
    let token = auth::create_token(
        &user.id,
        &user.username,
        &user.role,
        &state.config.jwt_secret,
    )?;

    Ok(Json(LoginResponse { token, user }))
}

/// 管理员查询指定用户的等级变更历史
pub async fn get_user_level_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let real_user_id: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM users WHERE id = ? OR uid = ? LIMIT 1"),
    )
    .bind(&id)
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await?;
    let target_id = real_user_id.unwrap_or_else(|| id.clone());

    let logs = sqlx::query(
        &state.db.format_query(
            "SELECT id, user_id, old_level, old_level_name, new_level, new_level_name, operator, operator_id, source, remark, created_at FROM user_level_logs WHERE user_id = ? ORDER BY created_at DESC LIMIT 100"
        )
    )
    .bind(&target_id)
    .fetch_all(&state.db.pool)
    .await?;

    use sqlx::Row;
    let data: Vec<serde_json::Value> = logs.iter().map(|row| {
        serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "old_level": row.get::<String, _>("old_level"),
            "old_level_name": row.get::<String, _>("old_level_name"),
            "new_level": row.get::<String, _>("new_level"),
            "new_level_name": row.get::<String, _>("new_level_name"),
            "operator": row.get::<String, _>("operator"),
            "source": row.get::<String, _>("source"),
            "remark": row.get::<String, _>("remark"),
            "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        })
    }).collect();

    Ok(Json(
        serde_json::json!({ "data": data, "total": data.len() }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ConsumptionStatsBatchRequest {
    pub user_ids: Vec<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Serialize, Default, Clone)]
pub struct ConsumptionStatsBatchItem {
    pub system_cost: f64,
    pub gift_cost: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct ConsumptionStatsBatchRawRow {
    user_id: String,
    system_cost: f64,
    gift_cost: f64,
}

/// 批量查询用户消费合计（系统钱包 / 赠送钱包）。
/// 历史天走 usage_daily_stats，今日及碎片段走 logs，与财务/明细口径一致。
pub async fn query_consumption_stats_batch(
    state: &AppState,
    user_ids: &[String],
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> AppResult<HashMap<String, ConsumptionStatsBatchItem>> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let site_tz = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let tz: chrono_tz::Tz = site_tz.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let slices = crate::api::date_helper::calculate_query_slices(start_date, end_date, tz);

    let placeholders = user_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    let mut result: HashMap<String, ConsumptionStatsBatchItem> = HashMap::new();
    let merge_row = |map: &mut HashMap<String, ConsumptionStatsBatchItem>,
                     row: ConsumptionStatsBatchRawRow| {
        let entry = map.entry(row.user_id).or_default();
        entry.system_cost += row.system_cost;
        entry.gift_cost += row.gift_cost;
    };

    if slices.has_history_days {
        let sql = format!(
            "SELECT user_id, \
                COALESCE(SUM(GREATEST(total_cost - total_pre_deduct_gift - COALESCE(total_pre_deduct_voucher, 0.0), 0.0)), 0.0) as system_cost, \
                COALESCE(SUM(total_pre_deduct_gift), 0.0) as gift_cost \
             FROM usage_daily_stats \
             WHERE user_id IN ({}) AND {} \
             GROUP BY user_id",
            placeholders,
            slices.history_cond("stat_date")
        );
        let formatted_sql = state.db.format_query(&sql);
        let mut query = sqlx::query_as::<_, ConsumptionStatsBatchRawRow>(&formatted_sql);
        for id in user_ids {
            query = query.bind(id);
        }
        query = query
            .bind(slices.hist_start_date)
            .bind(slices.hist_end_date);
        let rows = query.fetch_all(&state.db.pool).await?;
        for row in rows {
            merge_row(&mut result, row);
        }
    }

    for r_slice in slices.realtime_slices() {
        let sql = format!(
            "SELECT user_id, \
                COALESCE(SUM(GREATEST(cost - pre_deduct_gift - COALESCE(pre_deduct_voucher, 0.0), 0.0)), 0.0) as system_cost, \
                COALESCE(SUM(pre_deduct_gift), 0.0) as gift_cost \
             FROM logs \
             WHERE user_id IN ({}) AND {} \
             GROUP BY user_id",
            placeholders,
            r_slice.sql_cond("created_at")
        );
        let formatted_sql = state.db.format_query(&sql);
        let mut query = sqlx::query_as::<_, ConsumptionStatsBatchRawRow>(&formatted_sql);
        for id in user_ids {
            query = query.bind(id);
        }
        query = query.bind(&r_slice.start).bind(&r_slice.end);
        let rows = query.fetch_all(&state.db.pool).await?;
        for row in rows {
            merge_row(&mut result, row);
        }
    }

    Ok(result)
}

/// 管理后台：批量查询用户消费合计。
pub async fn get_consumption_stats_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConsumptionStatsBatchRequest>,
) -> AppResult<Json<HashMap<String, ConsumptionStatsBatchItem>>> {
    let result = query_consumption_stats_batch(
        &state,
        &req.user_ids,
        req.start_date.as_deref(),
        req.end_date.as_deref(),
    )
    .await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct ContactBindQuery {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct PutContactBindRequest {
    pub kind: String,
    pub value: String,
    pub max_accounts: i64,
}

#[derive(Debug, Serialize)]
pub struct ContactBindResponse {
    pub kind: String,
    pub value: String,
    pub bound_count: i64,
    pub limit: i64,
    pub default_limit: i64,
    pub is_override: bool,
}

fn normalize_admin_contact(kind: &str, value: &str) -> AppResult<String> {
    let value = crate::auth::contact::canonical_contact_value(kind, value);
    if value.is_empty() {
        return Err(AppError::BadRequest("联系方式不能为空".to_string()));
    }
    if kind == "email" && !crate::auth::contact::is_real_email(&value) {
        return Err(AppError::BadRequest("占位邮箱不能设置绑定上限".to_string()));
    }
    Ok(value)
}

async fn load_contact_bind(
    state: &Arc<AppState>,
    kind: &str,
    value: &str,
) -> AppResult<ContactBindResponse> {
    let kind = crate::auth::contact::parse_contact_kind(kind)?;
    let value = normalize_admin_contact(kind, value)?;
    let settings = crate::api::settings::load_all_settings(state).await?;
    let default_limit = crate::auth::contact::normalize_limit(match kind {
        "email" => settings.registration.max_accounts_per_email,
        "wechat" => settings.registration.max_accounts_per_wechat,
        _ => settings.registration.max_accounts_per_mobile,
    });
    let occ = match kind {
        "email" => crate::auth::contact::email_occupancy(state, &value, None, default_limit).await?,
        "wechat" => {
            crate::auth::contact::wechat_occupancy(state, &value, None, default_limit).await?
        }
        _ => crate::auth::contact::mobile_occupancy(state, &value, None, default_limit).await?,
    };
    let is_override = crate::auth::contact::fetch_override_limit(state, kind, &value)
        .await?
        .is_some();
    Ok(ContactBindResponse {
        kind: kind.to_string(),
        value,
        bound_count: occ.bound_count,
        limit: occ.limit,
        default_limit,
        is_override,
    })
}

pub async fn get_contact_bind(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<ContactBindQuery>,
) -> AppResult<Json<ContactBindResponse>> {
    Ok(Json(
        load_contact_bind(&state, &query.kind, &query.value).await?,
    ))
}

pub async fn put_contact_bind(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(ctx): axum::extract::Extension<crate::admin_permission::AdminContext>,
    Json(request): Json<PutContactBindRequest>,
) -> AppResult<Json<ContactBindResponse>> {
    crate::admin_permission::require_edit(&ctx, crate::admin_permission::users_write_perm("user"))?;
    let kind = crate::auth::contact::parse_contact_kind(&request.kind)?;
    let value = normalize_admin_contact(kind, &request.value)?;
    let snapshot = load_contact_bind(&state, kind, &value).await?;
    if i64::from(request.max_accounts) < snapshot.bound_count {
        return Err(AppError::BadRequest(format!(
            "该联系方式已绑定 {} 个账号，上限不能低于当前绑定数",
            snapshot.bound_count
        )));
    }
    crate::auth::contact::upsert_override_limit(&state, kind, &value, request.max_accounts).await?;
    Ok(Json(load_contact_bind(&state, kind, &value).await?))
}

pub async fn delete_contact_bind(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(ctx): axum::extract::Extension<crate::admin_permission::AdminContext>,
    axum::extract::Query(query): axum::extract::Query<ContactBindQuery>,
) -> AppResult<Json<ContactBindResponse>> {
    crate::admin_permission::require_edit(&ctx, crate::admin_permission::users_write_perm("user"))?;
    let kind = crate::auth::contact::parse_contact_kind(&query.kind)?;
    let value = normalize_admin_contact(kind, &query.value)?;
    crate::auth::contact::delete_override_limit(&state, kind, &value).await?;
    Ok(Json(load_contact_bind(&state, kind, &value).await?))
}

#[cfg(test)]
mod tests {
    use super::{
        collect_kyc_list_statuses, commission_ledger_label, oauth_avatar_url, sniff_image,
    };

    #[test]
    fn oauth_avatar_url_upsizes_google_only() {
        assert_eq!(
            oauth_avatar_url("https://lh3.googleusercontent.com/a/ABC=s96-c"),
            "https://lh3.googleusercontent.com/a/ABC=s256-c"
        );
        let wx = "https://thirdwx.qlogo.cn/mmopen/vi_32/xyz/132";
        assert_eq!(oauth_avatar_url(wx), wx);
        let other = "https://example.com/a=sbig-c";
        assert_eq!(oauth_avatar_url(other), other);
    }

    #[test]
    fn sniff_image_detects_common_formats() {
        assert_eq!(
            sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(("image/jpeg", "jpg"))
        );
        assert_eq!(
            sniff_image(b"\x89PNG\r\n\x1a\n"),
            Some(("image/png", "png"))
        );
        assert_eq!(
            sniff_image(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(("image/webp", "webp"))
        );
        assert_eq!(sniff_image(b"<html>"), None);
    }

    #[test]
    fn commission_ledger_label_splits_adjust_and_sources() {
        assert_eq!(commission_ledger_label("adjustclaw:9", -5.0).0, "adjust");
        assert_eq!(commission_ledger_label("log:3", 1.2).0, "consumption");
        assert_eq!(
            commission_ledger_label("daycons:inv:u1:2026-10-05", 1.2).0,
            "consumption"
        );
        assert_eq!(
            commission_ledger_label("daycons:u1:2026-10-03", -1.0).0,
            "consumption"
        );
        assert_eq!(commission_ledger_label("recharge:4", 2.0).0, "recharge");
        assert_eq!(commission_ledger_label("ark:8", 1.0).0, "video");
        assert_eq!(commission_ledger_label("arkrefund:8", -1.0).0, "refund");
    }

    #[test]
    fn collect_kyc_list_statuses_keeps_pending_review() {
        let map = collect_kyc_list_statuses([
            ("u1".into(), "personal".into(), "pending".into()),
            ("u1".into(), "enterprise".into(), "approved".into()),
            ("u2".into(), "personal".into(), "none".into()),
            ("u3".into(), "other".into(), "pending".into()),
            ("u4".into(), "personal".into(), "rejected".into()),
        ]);
        assert_eq!(
            map.get("u1"),
            Some(&(Some("pending".into()), Some("approved".into())))
        );
        assert!(!map.contains_key("u2"));
        assert!(!map.contains_key("u3"));
        assert_eq!(map.get("u4"), Some(&(Some("rejected".into()), None)));
    }
}
