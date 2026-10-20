/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::error::AppResult;
use crate::models::{ChannelCategory, ChannelCategoryRequest};
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

pub async fn list_categories(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<ChannelCategory>>> {
    let categories = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM channel_categories ORDER BY sort_order DESC, id ASC"),
    )
    .fetch_all(&state.db.pool)
    .await?;
    Ok(Json(categories))
}

pub async fn create_category(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<ChannelCategoryRequest>,
) -> AppResult<Json<ChannelCategory>> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "名称不能为空".to_string(),
        ));
    }

    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM channel_categories WHERE name = ?"),
    )
    .bind(&req.name)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "分类名称已存在".to_string(),
        ));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let category = sqlx::query_as(
        &state.db.format_query(
            "INSERT INTO channel_categories (name, name_en, sort_order, is_active) VALUES (?, ?, ?, ?) RETURNING *"
        )
    )
    .bind(&req.name)
    .bind(name_en)
    .bind(req.sort_order)
    .bind(req.is_active)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(category))
}

pub async fn update_category(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(mut req): Json<ChannelCategoryRequest>,
) -> AppResult<Json<ChannelCategory>> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "名称不能为空".to_string(),
        ));
    }

    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM channel_categories WHERE name = ? AND id != ?"),
    )
    .bind(&req.name)
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "分类名称与其他记录重复".to_string(),
        ));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let category = sqlx::query_as(
        &state.db.format_query(
            "UPDATE channel_categories SET name = ?, name_en = ?, sort_order = ?, is_active = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? RETURNING *"
        )
    )
    .bind(&req.name)
    .bind(name_en)
    .bind(req.sort_order)
    .bind(req.is_active)
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(category))
}

pub async fn delete_category(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let is_sys: i64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_system FROM channel_categories WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .unwrap_or(0);

    if is_sys == 1 {
        return Err(crate::error::AppError::BadRequest(
            "系统内置分类不可删除".to_string(),
        ));
    }

    sqlx::query(
        &state
            .db
            .format_query("UPDATE channels SET category_id = NULL WHERE category_id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    if let Ok(cfg_rows) = sqlx::query_as::<_, (i64, Option<i64>, String)>(
        &state
            .db
            .format_query("SELECT id, category_id, category_ids FROM channel_configs WHERE category_id = ? OR category_ids <> '[]'"),
    )
    .bind(id)
    .fetch_all(&state.db.pool)
    .await
    {
        for (cfg_id, primary_id, cids_str) in cfg_rows {
            let mut ids: Vec<i64> = serde_json::from_str(&cids_str).unwrap_or_default();
            let before = ids.len();
            ids.retain(|&cid| cid != id);
            if primary_id == Some(id) || ids.len() != before {
                let next_primary = ids.first().copied();
                let next_json = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
                let _ = sqlx::query(
                    &state
                        .db
                        .format_query("UPDATE channel_configs SET category_id = ?, category_ids = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"),
                )
                .bind(next_primary)
                .bind(next_json)
                .bind(cfg_id)
                .execute(&state.db.pool)
                .await;
            }
        }
    }

    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM channel_categories WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    crate::relay::router::invalidate_channel_cache();
    crate::relay::router::invalidate_channel_configs_cache();

    Ok(Json(serde_json::json!({ "success": true })))
}
