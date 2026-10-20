/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::error::AppResult;
use crate::models::{
    CreateUserLevelRequest, UpdateUserLevelRequest, UserLevel, UserLevelListResponse,
};
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use sqlx::Row;
use std::sync::Arc;

pub async fn list_user_levels(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<UserLevelListResponse>> {
    let levels: Vec<UserLevel> = sqlx::query_as(&state.db.format_query(
        "
        SELECT 
            ul.*, 
            (SELECT COUNT(*) FROM users u WHERE u.user_group = ul.group_key) as user_count 
        FROM user_levels ul 
        ORDER BY ul.sort_order DESC, ul.discount DESC
    ",
    ))
    .fetch_all(&state.db.pool)
    .await?;

    let total: i64 = sqlx::query_scalar(&state.db.format_query("SELECT COUNT(*) FROM user_levels"))
        .fetch_one(&state.db.pool)
        .await?;

    Ok(Json(UserLevelListResponse {
        data: levels,
        total,
    }))
}

pub async fn create_user_level(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateUserLevelRequest>,
) -> AppResult<Json<UserLevel>> {
    let group_key = req.group_key.trim();
    if group_key.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "等级标志 ID 不能为空".to_string(),
        ));
    }
    if !req.discount.is_finite() || req.discount <= 0.0 || req.discount > 1000.0 {
        return Err(crate::error::AppError::BadRequest(
            "计费倍率必须大于 0 且不超过 1000".to_string(),
        ));
    }

    let exists: bool = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT EXISTS(SELECT 1 FROM user_levels WHERE group_key = ?)"),
    )
    .bind(group_key)
    .fetch_one(&state.db.pool)
    .await?;

    if exists {
        return Err(crate::error::AppError::Conflict(
            "该等级标志 ID 已存在，请使用其他的标志".to_string(),
        ));
    }

    // 如果设为默认，先清除其他默认
    if req.is_default.unwrap_or(0) == 1 {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET is_default = 0 WHERE is_default = 1"),
        )
        .execute(&state.db.pool)
        .await?;
    }

    let id = sqlx::query(
        &state.db.format_query(r#"INSERT INTO user_levels (name, group_key, discount, discount_type, commission_ratio, commission_basis, invite_reward_inviter, invite_reward_invitee, daily_invite_limit, marketing_enabled, is_default, max_token_count, allow_view_log_details, description, sort_order, invoice_enabled, invoice_mode, invoice_config, commission_transfer_min, commission_transfer_max, purchasable)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           RETURNING id"#)
    )
    .bind(req.name.trim())
    .bind(group_key)
    .bind(req.discount)
    .bind(req.discount_type.unwrap_or(0))
    .bind(req.commission_ratio.unwrap_or(0.0))
    .bind(crate::services::affiliate::normalize_commission_basis(
        req.commission_basis.as_deref().unwrap_or("consumption"),
    ))
    .bind(req.invite_reward_inviter.unwrap_or(0.0))
    .bind(req.invite_reward_invitee.unwrap_or(0.0))
    .bind(req.daily_invite_limit.unwrap_or(10))
    .bind(req.marketing_enabled.unwrap_or(0))
    .bind(req.is_default.unwrap_or(0))
    .bind(req.max_token_count.unwrap_or(10))
    .bind(req.allow_view_log_details.unwrap_or(1))
    .bind(req.description.unwrap_or_default())
    .bind(req.sort_order.unwrap_or(0))
    .bind(req.invoice_enabled.unwrap_or(0))
    .bind(req.invoice_mode.as_deref().unwrap_or("international"))
    .bind(req.invoice_config.as_deref().unwrap_or("{}"))
    .bind(req.commission_transfer_min.unwrap_or(0.0))
    .bind(req.commission_transfer_max.unwrap_or(0.0))
    .bind(req.purchasable.unwrap_or(0))
    .fetch_one(&state.db.pool)
    .await?
    .get::<i64, _>("id");

    let level = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM user_levels WHERE id = ?"),
    )
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(level))
}

pub async fn update_user_level(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateUserLevelRequest>,
) -> AppResult<Json<UserLevel>> {
    let (old_group_key, old_is_default): (String, i64) = sqlx::query_as(
        &state
            .db
            .format_query("SELECT group_key, is_default FROM user_levels WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| crate::error::AppError::NotFound("未找到该用户等级".to_string()))?;

    if let Some(discount) = req.discount {
        if !discount.is_finite() || discount <= 0.0 || discount > 1000.0 {
            return Err(crate::error::AppError::BadRequest(
                "计费倍率必须大于 0 且不超过 1000".to_string(),
            ));
        }
    }

    if let Some(name) = &req.name {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET name = ? WHERE id = ?"),
        )
        .bind(name.trim())
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(raw_group_key) = &req.group_key {
        let group_key = raw_group_key.trim();
        if group_key.is_empty() {
            return Err(crate::error::AppError::BadRequest(
                "等级标志 ID 不能为空".to_string(),
            ));
        }
        if group_key != old_group_key {
            if old_group_key == "default" {
                return Err(crate::error::AppError::BadRequest(
                    "系统内置默认等级标识 default 不可修改".to_string(),
                ));
            }
            let exists: bool = sqlx::query_scalar(&state.db.format_query(
                "SELECT EXISTS(SELECT 1 FROM user_levels WHERE group_key = ? AND id != ?)",
            ))
            .bind(group_key)
            .bind(id)
            .fetch_one(&state.db.pool)
            .await?;

            if exists {
                return Err(crate::error::AppError::Conflict(
                    "该等级标志 ID 已存在，请使用其他的标志".to_string(),
                ));
            }

            sqlx::query(
                &state
                    .db
                    .format_query("UPDATE user_levels SET group_key = ? WHERE id = ?"),
            )
            .bind(group_key)
            .bind(id)
            .execute(&state.db.pool)
            .await?;

            // 级联同步更新存量用户的 user_group，防止修改 group_key 后用户与等级解绑
            sqlx::query(
                &state
                    .db
                    .format_query("UPDATE users SET user_group = ? WHERE user_group = ?"),
            )
            .bind(group_key)
            .bind(&old_group_key)
            .execute(&state.db.pool)
            .await?;
        }
    }
    if let Some(discount) = req.discount {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET discount = ? WHERE id = ?"),
        )
        .bind(discount)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(discount_type) = req.discount_type {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET discount_type = ? WHERE id = ?"),
        )
        .bind(discount_type)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(description) = &req.description {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET description = ? WHERE id = ?"),
        )
        .bind(description)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(commission_ratio) = req.commission_ratio {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET commission_ratio = ? WHERE id = ?"),
        )
        .bind(commission_ratio)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(commission_basis) = &req.commission_basis {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET commission_basis = ? WHERE id = ?"),
        )
        .bind(crate::services::affiliate::normalize_commission_basis(
            commission_basis,
        ))
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(commission_transfer_min) = req.commission_transfer_min {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET commission_transfer_min = ? WHERE id = ?"),
        )
        .bind(commission_transfer_min.max(0.0))
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(commission_transfer_max) = req.commission_transfer_max {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET commission_transfer_max = ? WHERE id = ?"),
        )
        .bind(commission_transfer_max.max(0.0))
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(invite_reward_inviter) = req.invite_reward_inviter {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET invite_reward_inviter = ? WHERE id = ?"),
        )
        .bind(invite_reward_inviter)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(invite_reward_invitee) = req.invite_reward_invitee {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET invite_reward_invitee = ? WHERE id = ?"),
        )
        .bind(invite_reward_invitee)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(daily_invite_limit) = req.daily_invite_limit {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET daily_invite_limit = ? WHERE id = ?"),
        )
        .bind(daily_invite_limit)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(marketing_enabled) = req.marketing_enabled {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET marketing_enabled = ? WHERE id = ?"),
        )
        .bind(marketing_enabled)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(purchasable) = req.purchasable {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET purchasable = ? WHERE id = ?"),
        )
        .bind(if purchasable == 0 { 0 } else { 1 })
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(is_default) = req.is_default {
        if is_default == 1 {
            // 先清除所有默认
            sqlx::query(
                &state
                    .db
                    .format_query("UPDATE user_levels SET is_default = 0 WHERE is_default = 1"),
            )
            .execute(&state.db.pool)
            .await?;
        }
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET is_default = ? WHERE id = ?"),
        )
        .bind(if is_default == 1 { 1 } else { 0 })
        .bind(id)
        .execute(&state.db.pool)
        .await?;

        // 若关闭了原本唯一的默认等级，自动兜底将 default 等级（或首个等级）恢复为默认，防止全系统无默认注册等级
        if is_default != 1 && old_is_default == 1 {
            let has_default: bool = sqlx::query_scalar(
                &state
                    .db
                    .format_query("SELECT EXISTS(SELECT 1 FROM user_levels WHERE is_default = 1)"),
            )
            .fetch_one(&state.db.pool)
            .await?;
            if !has_default {
                sqlx::query(&state.db.format_query(
                    "UPDATE user_levels SET is_default = 1 WHERE id = COALESCE( \
                        (SELECT id FROM user_levels WHERE group_key = 'default' LIMIT 1), \
                        (SELECT id FROM user_levels ORDER BY id ASC LIMIT 1) \
                    )",
                ))
                .execute(&state.db.pool)
                .await?;
            }
        }
    }
    if let Some(max_token_count) = req.max_token_count {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET max_token_count = ? WHERE id = ?"),
        )
        .bind(max_token_count)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(allow_view_log_details) = req.allow_view_log_details {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET allow_view_log_details = ? WHERE id = ?"),
        )
        .bind(allow_view_log_details)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(sort_order) = req.sort_order {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET sort_order = ? WHERE id = ?"),
        )
        .bind(sort_order)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(invoice_enabled) = req.invoice_enabled {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET invoice_enabled = ? WHERE id = ?"),
        )
        .bind(invoice_enabled)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(ref invoice_mode) = req.invoice_mode {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET invoice_mode = ? WHERE id = ?"),
        )
        .bind(invoice_mode)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(ref invoice_config) = req.invoice_config {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE user_levels SET invoice_config = ? WHERE id = ?"),
        )
        .bind(invoice_config)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }

    sqlx::query(
        &state
            .db
            .format_query("UPDATE user_levels SET updated_at = CURRENT_TIMESTAMP WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    let level = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM user_levels WHERE id = ?"),
    )
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(level))
}

pub async fn delete_user_level(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    // 禁止删除内置 default 等级或当前默认注册等级 (is_default = 1)
    let (group_key, is_default): (String, i64) = sqlx::query_as(
        &state
            .db
            .format_query("SELECT group_key, is_default FROM user_levels WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| crate::error::AppError::NotFound("未找到该用户等级".to_string()))?;

    if group_key == "default" || is_default == 1 {
        return Err(crate::error::AppError::BadRequest(
            "默认注册等级不可删除，请先将其他等级设为默认注册等级".to_string(),
        ));
    }

    let fallback_group: String = sqlx::query_scalar(&state.db.format_query(
        "SELECT group_key FROM user_levels WHERE id != ? AND (is_default = 1 OR group_key = 'default') ORDER BY is_default DESC, id ASC LIMIT 1",
    ))
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .unwrap_or_else(|| "default".to_string());

    let mut tx = state.db.pool.begin().await?;

    // 将被删等级下的存量用户自动迁移回默认等级，避免产生孤儿 user_group 导致折扣掩码失配
    sqlx::query(
        &state
            .db
            .format_query("UPDATE users SET user_group = ? WHERE user_group = ?"),
    )
    .bind(&fallback_group)
    .bind(&group_key)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM user_levels WHERE id = ?"),
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(serde_json::json!({ "success": true })))
}
