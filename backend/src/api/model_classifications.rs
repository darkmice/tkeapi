/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::api::models::{class_filter_bind, class_filter_sql};
use crate::error::AppResult;
use crate::models::{
    ClassificationRequest, ClassificationsResponse, ModelProvider, ModelType, SourceModelCounts,
};
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

async fn is_volcengine_enhance_active(state: &AppState) -> bool {
    #[cfg(feature = "plugin_volcengine_enhance")]
    {
        crate::api::plugins::is_plugin_enabled(state, "volcengine_enhance").await
    }
    #[cfg(not(feature = "plugin_volcengine_enhance"))]
    {
        let _ = state;
        false
    }
}

// --- Providers ---

pub async fn list_providers(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<ModelProvider>>> {
    let query_str = "SELECT p.*, COUNT(m.id) as model_count FROM model_providers p LEFT JOIN models m ON p.id = m.provider_id AND m.is_listed = 1 GROUP BY p.id, p.name, p.name_en, p.sort_order, p.is_active, p.is_system, p.remark, p.logo, p.created_at, p.updated_at ORDER BY p.sort_order DESC, p.id ASC";
    let providers = sqlx::query_as(&state.db.format_query(query_str))
        .fetch_all(&state.db.pool)
        .await?;
    Ok(Json(providers))
}

pub async fn create_provider(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<ClassificationRequest>,
) -> AppResult<Json<ModelProvider>> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "名称不能为空".to_string(),
        ));
    }

    // Check for duplicate name
    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM model_providers WHERE name = ?"),
    )
    .bind(&req.name)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "服务商名称已存在".to_string(),
        ));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let provider = sqlx::query_as(
        &state.db.format_query("INSERT INTO model_providers (name, name_en, sort_order, is_active, remark, logo) VALUES (?, ?, ?, ?, ?, ?) RETURNING *")
    )
    .bind(&req.name)
    .bind(name_en)
    .bind(req.sort_order)
    .bind(req.is_active)
    .bind(&req.remark)
    .bind(&req.logo)
    .fetch_one(&state.db.pool)
    .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(provider))
}

pub async fn update_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(mut req): Json<ClassificationRequest>,
) -> AppResult<Json<ModelProvider>> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "名称不能为空".to_string(),
        ));
    }

    // Check for duplicate name (excluding itself)
    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM model_providers WHERE name = ? AND id != ?"),
    )
    .bind(&req.name)
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "服务商名称与其他记录重复".to_string(),
        ));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let provider = sqlx::query_as(
        &state.db.format_query("UPDATE model_providers SET name = ?, name_en = ?, sort_order = ?, is_active = ?, remark = ?, logo = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? RETURNING *")
    )
    .bind(&req.name)
    .bind(name_en)
    .bind(req.sort_order)
    .bind(req.is_active)
    .bind(&req.remark)
    .bind(&req.logo)
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(provider))
}

pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let is_sys: i64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_system FROM model_providers WHERE id = ?"),
    )
    .bind(id)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(0);

    if is_sys == 1 {
        return Err(crate::error::AppError::BadRequest(
            "系统内置服务商不允许删除".to_string(),
        ));
    }

    // NULL out references in models table
    sqlx::query(
        &state
            .db
            .format_query("UPDATE models SET provider_id = NULL WHERE provider_id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    // NULL out references in billing_rules table
    sqlx::query(
        &state
            .db
            .format_query("UPDATE billing_rules SET provider_id = NULL WHERE provider_id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM model_providers WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(serde_json::json!({ "success": true })))
}

// --- API Providers ---

pub async fn list_api_providers(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<ModelProvider>>> {
    let query_str = "SELECT p.*, COUNT(m.id) as model_count FROM model_api_providers p LEFT JOIN models m ON p.id = m.api_provider_id AND m.is_listed = 1 GROUP BY p.id, p.name, p.name_en, p.sort_order, p.is_active, p.is_system, p.remark, p.logo, p.created_at, p.updated_at ORDER BY p.sort_order DESC, p.id ASC";
    let providers = sqlx::query_as(&state.db.format_query(query_str))
        .fetch_all(&state.db.pool)
        .await?;
    Ok(Json(providers))
}

pub async fn create_api_provider(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<ClassificationRequest>,
) -> AppResult<Json<ModelProvider>> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "名称不能为空".to_string(),
        ));
    }

    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM model_api_providers WHERE name = ?"),
    )
    .bind(&req.name)
    .fetch_optional(&state.db.pool)
    .await?;
    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "API服务商名称已存在".to_string(),
        ));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let provider = sqlx::query_as(&state.db.format_query("INSERT INTO model_api_providers (name, name_en, sort_order, is_active, remark, logo) VALUES (?, ?, ?, ?, ?, ?) RETURNING *"))
        .bind(&req.name).bind(name_en).bind(req.sort_order).bind(req.is_active).bind(&req.remark).bind(&req.logo)
        .fetch_one(&state.db.pool).await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(provider))
}

pub async fn update_api_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(mut req): Json<ClassificationRequest>,
) -> AppResult<Json<ModelProvider>> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "名称不能为空".to_string(),
        ));
    }

    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM model_api_providers WHERE name = ? AND id != ?"),
    )
    .bind(&req.name)
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;
    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "API服务商名称与其他记录重复".to_string(),
        ));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let provider = sqlx::query_as(&state.db.format_query("UPDATE model_api_providers SET name = ?, name_en = ?, sort_order = ?, is_active = ?, remark = ?, logo = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? RETURNING *"))
        .bind(&req.name).bind(name_en).bind(req.sort_order).bind(req.is_active).bind(&req.remark).bind(&req.logo).bind(id)
        .fetch_one(&state.db.pool).await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(provider))
}

pub async fn delete_api_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let is_sys: i64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_system FROM model_api_providers WHERE id = ?"),
    )
    .bind(id)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(0);

    if is_sys == 1 {
        return Err(crate::error::AppError::BadRequest(
            "系统内置API服务商不允许删除".to_string(),
        ));
    }

    sqlx::query(
        &state
            .db
            .format_query("UPDATE models SET api_provider_id = NULL WHERE api_provider_id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM model_api_providers WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(serde_json::json!({ "success": true })))
}

// --- Types ---

pub async fn list_types(State(state): State<Arc<AppState>>) -> AppResult<Json<Vec<ModelType>>> {
    let volc_active = is_volcengine_enhance_active(&state).await;
    let where_clause = if volc_active {
        ""
    } else {
        "WHERE t.name <> '封装'"
    };

    let query_str = format!(
        "SELECT t.*, COUNT(m.id) as model_count FROM model_types t LEFT JOIN models m ON t.id = m.type_id AND m.is_listed = 1 {where_clause} GROUP BY t.id, t.name, t.name_en, t.sort_order, t.is_active, t.is_system, t.logo, t.default_features, t.created_at, t.updated_at ORDER BY t.sort_order DESC, t.id ASC"
    );

    let types = sqlx::query_as(&state.db.format_query(&query_str))
        .fetch_all(&state.db.pool)
        .await?;
    Ok(Json(types))
}

pub async fn create_type(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<ClassificationRequest>,
) -> AppResult<Json<ModelType>> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "名称不能为空".to_string(),
        ));
    }

    // Check for duplicate name
    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM model_types WHERE name = ?"),
    )
    .bind(&req.name)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "类型名称已存在".to_string(),
        ));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let model_type = sqlx::query_as(
        &state.db.format_query("INSERT INTO model_types (name, name_en, sort_order, is_active, logo) VALUES (?, ?, ?, ?, ?) RETURNING *")
    )
    .bind(&req.name)
    .bind(name_en)
    .bind(req.sort_order)
    .bind(req.is_active)
    .bind(&req.logo)
    .fetch_one(&state.db.pool)
    .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(model_type))
}

pub async fn update_type(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(mut req): Json<ClassificationRequest>,
) -> AppResult<Json<ModelType>> {
    req.name = req.name.trim().to_string();
    if req.name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "名称不能为空".to_string(),
        ));
    }

    // Check for duplicate name (excluding itself)
    let exists: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM model_types WHERE name = ? AND id != ?"),
    )
    .bind(&req.name)
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    if exists.is_some() {
        return Err(crate::error::AppError::Conflict(
            "类型名称与其他记录重复".to_string(),
        ));
    }

    let name_en = req.name_en.unwrap_or_default().trim().to_string();
    let video_preset_features =
        r#"["全能参考生视频","文生视频","图生视频","首尾帧生视频","编辑视频","延长视频"]"#;
    let model_type = sqlx::query_as(
        &state.db.format_query("UPDATE model_types SET name = ?, name_en = ?, sort_order = ?, is_active = ?, logo = ?, default_features = CASE WHEN ? = '视频' THEN ? ELSE default_features END, updated_at = CURRENT_TIMESTAMP WHERE id = ? RETURNING *")
    )
    .bind(&req.name)
    .bind(name_en)
    .bind(req.sort_order)
    .bind(req.is_active)
    .bind(&req.logo)
    .bind(&req.name)
    .bind(video_preset_features)
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(model_type))
}

pub async fn delete_type(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let is_sys: i64 = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_system FROM model_types WHERE id = ?"),
    )
    .bind(id)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(0);

    if is_sys == 1 {
        return Err(crate::error::AppError::BadRequest(
            "系统内置类型不允许删除".to_string(),
        ));
    }

    // NULL out references in models table
    sqlx::query(
        &state
            .db
            .format_query("UPDATE models SET type_id = NULL WHERE type_id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    // NULL out references in billing_rules table
    sqlx::query(
        &state
            .db
            .format_query("UPDATE billing_rules SET type_id = NULL WHERE type_id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM model_types WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Debug, serde::Deserialize)]
pub struct StatsQuery {
    pub provider_id: Option<i64>,
    pub api_provider_id: Option<i64>,
    pub type_id: Option<i64>,
    /// `library` | `system` = 模型库；其余为上架模型
    pub source: Option<String>,
}

fn push_class_cond(conds: &mut Vec<String>, column: &str, id: Option<i64>) {
    if let Some(frag) = class_filter_sql(column, id) {
        conds.push(frag);
    }
}

async fn count_unclassified(
    state: &AppState,
    null_column: &str,
    cross: [(&str, Option<i64>); 2],
    source: Option<&str>,
    volc_hide_pred: Option<&str>,
) -> AppResult<i64> {
    let mut conds = vec![format!("{null_column} IS NULL")];
    for (col, id) in cross {
        push_class_cond(&mut conds, col, id);
    }
    if let Some(pred) = crate::db::preset_models::listed_join_predicate(source) {
        conds.push(pred.to_string());
    }
    if let Some(pred) = crate::db::preset_models::library_catalog_free_predicate(source) {
        conds.push(pred.to_string());
    }
    if let Some(pred) = volc_hide_pred {
        conds.push(pred.to_string());
    }
    let table = crate::db::preset_models::models_join_relation(source);
    let formatted = state.db.format_query(&format!(
        "SELECT COUNT(*) FROM {table} m WHERE {}",
        conds.join(" AND ")
    ));
    let mut q = sqlx::query_scalar::<_, i64>(&formatted);
    for (_, id) in cross {
        if let Some(id) = class_filter_bind(id) {
            q = q.bind(id);
        }
    }
    Ok(q.fetch_one(&state.db.pool).await?)
}

/// 分类统计接口 — 每个维度的 count 基于"排除自身、保留其他维度"的交叉筛选
/// 例如选中 api_provider_id=1 后：
///   - 官方服务商 count = 满足 api_provider_id=1 + type_id(如有) 的模型数
///   - API服务商 count = 满足 provider_id(如有) + type_id(如有) 的模型数（排除自身）
///   - 类型 count = 满足 api_provider_id=1 + provider_id(如有) 的模型数
/// `0` 表示未分类（IS NULL），与列表接口一致。
pub async fn get_classifications_stats(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<StatsQuery>,
) -> AppResult<Json<ClassificationsResponse>> {
    let volc_active = is_volcengine_enhance_active(&state).await;
    let track_copies = !crate::db::preset_models::is_library_source(query.source.as_deref());
    let volc_hide = crate::api::models::volc_preset_hide_sql(&state, "m.mid", track_copies).await;
    let volc_hide_pred = {
        let pred = volc_hide.trim_start_matches(" AND ");
        if pred.is_empty() {
            None
        } else {
            Some(pred)
        }
    };
    let source = query.source.as_deref();
    let table = crate::db::preset_models::models_join_relation(source);

    // ── 官方服务商 count（交叉: api_provider_id + type_id） ──
    let mut p_sql = format!(
        r#"SELECT p.id, p.name, p.name_en, p.logo, p.is_system, COUNT(m.id) as count 
           FROM model_providers p 
           LEFT JOIN {table} m ON p.id = m.provider_id"#
    );

    let mut p_conds: Vec<String> = vec![];
    push_class_cond(&mut p_conds, "m.api_provider_id", query.api_provider_id);
    push_class_cond(&mut p_conds, "m.type_id", query.type_id);
    if let Some(pred) = crate::db::preset_models::listed_join_predicate(source) {
        p_conds.push(pred.to_string());
    }
    if let Some(pred) = crate::db::preset_models::library_catalog_free_predicate(source) {
        p_conds.push(pred.to_string());
    }
    if let Some(pred) = volc_hide_pred {
        p_conds.push(pred.to_string());
    }
    if !p_conds.is_empty() {
        p_sql.push_str(" AND ");
        p_sql.push_str(&p_conds.join(" AND "));
    }
    p_sql.push_str(" WHERE p.is_active = 1 GROUP BY p.id, p.name, p.name_en, p.logo, p.is_system ORDER BY p.sort_order DESC, p.id ASC");

    let formatted_p_sql = state.db.format_query(&p_sql);
    let mut pq = sqlx::query_as::<_, crate::models::ClassificationCount>(&formatted_p_sql);
    if let Some(apid) = class_filter_bind(query.api_provider_id) {
        pq = pq.bind(apid);
    }
    if let Some(tid) = class_filter_bind(query.type_id) {
        pq = pq.bind(tid);
    }
    let providers = pq.fetch_all(&state.db.pool).await?;

    // ── API 服务商 count（交叉: provider_id + type_id） ──
    let mut ap_sql = format!(
        r#"SELECT p.id, p.name, p.name_en, p.logo, p.is_system, COUNT(m.id) as count 
           FROM model_api_providers p 
           LEFT JOIN {table} m ON p.id = m.api_provider_id"#
    );

    let mut ap_conds: Vec<String> = vec![];
    push_class_cond(&mut ap_conds, "m.provider_id", query.provider_id);
    push_class_cond(&mut ap_conds, "m.type_id", query.type_id);
    if let Some(pred) = crate::db::preset_models::listed_join_predicate(source) {
        ap_conds.push(pred.to_string());
    }
    if let Some(pred) = crate::db::preset_models::library_catalog_free_predicate(source) {
        ap_conds.push(pred.to_string());
    }
    if let Some(pred) = volc_hide_pred {
        ap_conds.push(pred.to_string());
    }
    if !ap_conds.is_empty() {
        ap_sql.push_str(" AND ");
        ap_sql.push_str(&ap_conds.join(" AND "));
    }
    ap_sql.push_str(" WHERE p.is_active = 1 GROUP BY p.id, p.name, p.name_en, p.logo, p.is_system ORDER BY p.sort_order DESC, p.id ASC");

    let formatted_ap_sql = state.db.format_query(&ap_sql);
    let mut apq = sqlx::query_as::<_, crate::models::ClassificationCount>(&formatted_ap_sql);
    if let Some(pid) = class_filter_bind(query.provider_id) {
        apq = apq.bind(pid);
    }
    if let Some(tid) = class_filter_bind(query.type_id) {
        apq = apq.bind(tid);
    }
    let api_providers = apq.fetch_all(&state.db.pool).await?;

    // ── 类型 count（交叉: provider_id + api_provider_id） ──
    let mut t_sql = format!(
        r#"SELECT t.id, t.name, t.name_en, t.logo, t.is_system, COUNT(m.id) as count 
           FROM model_types t 
           LEFT JOIN {table} m ON t.id = m.type_id"#
    );

    let mut t_conds: Vec<String> = vec![];
    push_class_cond(&mut t_conds, "m.provider_id", query.provider_id);
    push_class_cond(&mut t_conds, "m.api_provider_id", query.api_provider_id);
    if let Some(pred) = crate::db::preset_models::listed_join_predicate(source) {
        t_conds.push(pred.to_string());
    }
    if let Some(pred) = crate::db::preset_models::library_catalog_free_predicate(source) {
        t_conds.push(pred.to_string());
    }
    if let Some(pred) = volc_hide_pred {
        t_conds.push(pred.to_string());
    }
    if !t_conds.is_empty() {
        t_sql.push_str(" AND ");
        t_sql.push_str(&t_conds.join(" AND "));
    }
    let type_where = if volc_active {
        " WHERE t.is_active = 1 GROUP BY t.id, t.name, t.name_en, t.logo, t.is_system ORDER BY t.sort_order DESC, t.id ASC"
    } else {
        " WHERE t.is_active = 1 AND t.name <> '封装' GROUP BY t.id, t.name, t.name_en, t.logo, t.is_system ORDER BY t.sort_order DESC, t.id ASC"
    };
    t_sql.push_str(type_where);

    let formatted_t_sql = state.db.format_query(&t_sql);
    let mut tq = sqlx::query_as::<_, crate::models::ClassificationCount>(&formatted_t_sql);
    if let Some(pid) = class_filter_bind(query.provider_id) {
        tq = tq.bind(pid);
    }
    if let Some(apid) = class_filter_bind(query.api_provider_id) {
        tq = tq.bind(apid);
    }
    let types = tq.fetch_all(&state.db.pool).await?;

    let unclassified_providers = count_unclassified(
        &state,
        "m.provider_id",
        [
            ("m.api_provider_id", query.api_provider_id),
            ("m.type_id", query.type_id),
        ],
        source,
        volc_hide_pred,
    )
    .await?;
    let unclassified_api_providers = count_unclassified(
        &state,
        "m.api_provider_id",
        [
            ("m.provider_id", query.provider_id),
            ("m.type_id", query.type_id),
        ],
        source,
        volc_hide_pred,
    )
    .await?;
    let unclassified_types = count_unclassified(
        &state,
        "m.type_id",
        [
            ("m.provider_id", query.provider_id),
            ("m.api_provider_id", query.api_provider_id),
        ],
        source,
        volc_hide_pred,
    )
    .await?;

    let custom = crate::api::models::count_source_models(
        &state,
        false,
        query.provider_id,
        query.api_provider_id,
        query.type_id,
    )
    .await?;
    let library = crate::api::models::count_source_models(
        &state,
        true,
        query.provider_id,
        query.api_provider_id,
        query.type_id,
    )
    .await?;

    Ok(Json(ClassificationsResponse {
        providers,
        api_providers,
        types,
        unclassified_providers,
        unclassified_api_providers,
        unclassified_types,
        source_counts: SourceModelCounts { custom, library },
    }))
}
