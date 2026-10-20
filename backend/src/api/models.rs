/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use axum::{
    extract::{Path, State},
    Json,
};
use rand::Rng;
use sqlx::Row;
use std::sync::Arc;

use crate::error::AppResult;
use crate::models::{CreateModelRequest, Model, ModelListResponse, UpdateModelRequest};
use crate::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct ModelQuery {
    pub provider_id: Option<i64>,
    pub api_provider_id: Option<i64>,
    pub type_id: Option<i64>,
    pub page_size: Option<i64>,
    /// `library` | `system` = 模型库；其余为上架模型
    pub source: Option<String>,
}

/// `None` = 全部，`Some(0)` = 未分类（IS NULL），`Some(n)` = 指定 id。列名仅硬编码。
pub(crate) fn class_filter_sql(column: &str, id: Option<i64>) -> Option<String> {
    match id {
        None => None,
        Some(0) => Some(format!("{column} IS NULL")),
        Some(_) => Some(format!("{column} = ?")),
    }
}

pub(crate) fn class_filter_bind(id: Option<i64>) -> Option<i64> {
    id.filter(|n| *n > 0)
}

fn model_list_where(query: &ModelQuery) -> String {
    let mut sql = String::from(" WHERE 1=1");
    for (col, id) in [
        ("provider_id", query.provider_id),
        ("api_provider_id", query.api_provider_id),
        ("type_id", query.type_id),
    ] {
        if let Some(frag) = class_filter_sql(col, id) {
            sql.push_str(" AND ");
            sql.push_str(&frag);
        }
    }
    sql
}

/// 与 [`model_list_where`] 占位符顺序一致（仅 `>0` 的 id 需要 bind）
fn model_filter_ids(query: &ModelQuery) -> impl Iterator<Item = i64> + '_ {
    [query.provider_id, query.api_provider_id, query.type_id]
        .into_iter()
        .filter_map(class_filter_bind)
}

pub async fn list_models(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<ModelQuery>,
) -> AppResult<Json<ModelListResponse>> {
    let library = crate::db::preset_models::is_library_source(query.source.as_deref());
    let class_sql = model_list_where(&query);

    let mut models = if library {
        fetch_library_models(&state, &query, &class_sql).await?
    } else {
        fetch_listed_models(&state, &query, &class_sql).await?
    };

    let limit = query.page_size.filter(|&p| p > 0).map(|p| p.min(10_000));
    let total = models.len() as i64;
    if let Some(ps) = limit {
        models.truncate(ps as usize);
    }

    Ok(Json(ModelListResponse {
        data: models,
        total,
    }))
}

/// 插件关闭时，运营模型与模型仓库都不展示 MediaKit 预置 mid 及其上架副本。
/// 插件开启时，这些行都展示，不因 `is_active` 被拿掉。
/// `track_copies` 为真时额外看 `library_mid`，只用于 `models` 表。
pub(crate) async fn volc_preset_hide_sql(
    state: &AppState,
    col: &str,
    track_copies: bool,
) -> String {
    #[cfg(feature = "plugin_volcengine_enhance")]
    {
        crate::api::plugins::volc_inactive_filter_sql(state, col, track_copies).await
    }
    #[cfg(not(feature = "plugin_volcengine_enhance"))]
    {
        let _ = state;
        let hidden = format!(
            "({col} IS NULL OR {col} NOT IN ('vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep', 'vie-en', 'vir-bg'))"
        );
        if !track_copies {
            return format!(" AND {hidden}");
        }
        let lib = match col.rsplit_once('.') {
            Some((alias, _)) if !alias.is_empty() => format!("{alias}.library_mid"),
            _ => "library_mid".to_string(),
        };
        format!(
            " AND {hidden} AND ({lib} IS NULL OR {lib} = '' OR {lib} NOT IN ('vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep', 'vie-en', 'vir-bg'))"
        )
    }
}

async fn fetch_listed_models(
    state: &AppState,
    query: &ModelQuery,
    class_sql: &str,
) -> AppResult<Vec<Model>> {
    let volc_filter = volc_preset_hide_sql(state, "mid", true).await;

    let where_sql = format!(
        "{class_sql}{volc_filter}{}",
        crate::db::preset_models::LISTED_ONLY
    );
    let list_sql = format!("SELECT m.*, t.name AS type_name FROM models m LEFT JOIN model_types t ON m.type_id = t.id{where_sql} ORDER BY m.sort_order DESC, m.id DESC");
    let formatted_sql = state.db.format_query(&list_sql);
    let mut q = sqlx::query_as::<_, Model>(&formatted_sql);
    for id in model_filter_ids(query) {
        q = q.bind(id);
    }
    Ok(q.fetch_all(&state.db.pool).await?)
}

async fn fetch_library_models(
    state: &AppState,
    query: &ModelQuery,
    class_sql: &str,
) -> AppResult<Vec<Model>> {
    let catalog_filter = volc_preset_hide_sql(state, "mid", false).await;
    let copy_filter = volc_preset_hide_sql(state, "mid", true).await;

    // 1. 系统预置目录（model_library 表）
    let catalog_sql = format!(
        "SELECT {} FROM model_library m{class_sql}{catalog_filter} AND {} ORDER BY sort_order DESC, id DESC",
        crate::db::preset_models::LIBRARY_MODEL_COLUMNS,
        crate::db::preset_models::LIBRARY_CATALOG_FREE
    );
    let formatted_catalog_sql = state.db.format_query(&catalog_sql);
    let mut cq = sqlx::query_as::<_, Model>(&formatted_catalog_sql);
    for id in model_filter_ids(query) {
        cq = cq.bind(id);
    }
    let mut catalog = cq.fetch_all(&state.db.pool).await?;
    for m in &mut catalog {
        m.library_kind = Some("catalog".into());
        m.is_listed = 0;
    }

    // 2. 不在运营列表的模型行。进仓库即表示未上架，不再单独标「已下架」。
    let unlisted_sql = format!(
        "SELECT m.*, t.name AS type_name FROM models m LEFT JOIN model_types t ON m.type_id = t.id{class_sql}{copy_filter} AND m.is_listed = 0 AND m.is_system = 0 ORDER BY m.sort_order DESC, m.id DESC"
    );
    let formatted_unlisted_sql = state.db.format_query(&unlisted_sql);
    let mut uq = sqlx::query_as::<_, Model>(&formatted_unlisted_sql);
    for id in model_filter_ids(query) {
        uq = uq.bind(id);
    }
    let mut unlisted = uq.fetch_all(&state.db.pool).await?;
    for m in &mut unlisted {
        m.library_kind = Some("unlisted".into());
    }

    // 3. 合并：catalog 在前，unlisted 在后
    catalog.extend(unlisted);
    Ok(catalog)
}

async fn scalar_count(state: &AppState, sql: &str, ids: &[i64]) -> AppResult<i64> {
    let formatted = state.db.format_query(sql);
    let mut q = sqlx::query_scalar::<_, i64>(&formatted);
    for id in ids {
        q = q.bind(*id);
    }
    Ok(q.fetch_one(&state.db.pool).await?)
}

/// 与 [`fetch_listed_models`] / [`fetch_library_models`] 同一 WHERE，只返回条数。
pub(crate) async fn count_source_models(
    state: &AppState,
    library: bool,
    provider_id: Option<i64>,
    api_provider_id: Option<i64>,
    type_id: Option<i64>,
) -> AppResult<i64> {
    let query = ModelQuery {
        provider_id,
        api_provider_id,
        type_id,
        page_size: None,
        source: None,
    };
    let class_sql = model_list_where(&query);
    let ids: Vec<i64> = model_filter_ids(&query).collect();
    let catalog_filter = volc_preset_hide_sql(state, "mid", false).await;
    let copy_filter = volc_preset_hide_sql(state, "mid", true).await;

    if !library {
        return scalar_count(
            state,
            &format!(
                "SELECT COUNT(*) FROM models{class_sql}{copy_filter}{}",
                crate::db::preset_models::LISTED_ONLY
            ),
            &ids,
        )
        .await;
    }

    let catalog = scalar_count(
        state,
        &format!(
            "SELECT COUNT(*) FROM model_library m{class_sql}{catalog_filter} AND {}",
            crate::db::preset_models::LIBRARY_CATALOG_FREE
        ),
        &ids,
    )
    .await?;
    let unlisted = scalar_count(
        state,
        &format!(
            "SELECT COUNT(*) FROM models{class_sql}{copy_filter} AND is_listed = 0 AND is_system = 0"
        ),
        &ids,
    )
    .await?;
    Ok(catalog + unlisted)
}

async fn allocate_listed_mid(state: &AppState) -> AppResult<String> {
    loop {
        let candidate: String = {
            let n: u32 = rand::thread_rng().gen_range(300000..=309999);
            n.to_string()
        };
        let taken: Option<i64> =
            sqlx::query_scalar(&state.db.format_query("SELECT id FROM models WHERE mid = ?"))
                .bind(&candidate)
                .fetch_optional(&state.db.pool)
                .await?;
        if taken.is_none() {
            return Ok(candidate);
        }
    }
}

pub async fn create_model(
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateModelRequest>,
) -> AppResult<Json<Model>> {
    req.name = req.name.trim().to_string();
    req.model_id = req.model_id.trim().to_string();

    if req.name.is_empty() || req.model_id.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "名称和模型 ID 不能为空".to_string(),
        ));
    }

    // mid 是唯一标识，允许 name 和 model_id 重复（不同 mid 可对应相同 model_id，用于差异化计费）
    let mid = allocate_listed_mid(&state).await?;

    let group_ratios = serde_json::to_string(&req.group_ratios.unwrap_or_default())
        .unwrap_or_else(|_| "{}".to_string());
    // 过滤掉不存在或已禁用的规则 ID，防止脏数据入库
    let forward_rule_ids: Option<String> = if let Some(ids) = req.forward_rule_ids {
        let mut valid: Vec<i64> = Vec::new();
        for rid in &ids {
            let exists: bool = sqlx::query_scalar(&state.db.format_query(
                "SELECT EXISTS(SELECT 1 FROM forward_rules WHERE id = ? AND is_active = 1)",
            ))
            .bind(rid)
            .fetch_one(&state.db.pool)
            .await
            .unwrap_or(false);
            if exists {
                valid.push(*rid);
            }
        }
        Some(serde_json::to_string(&valid).unwrap_or_else(|_| "[]".to_string()))
    } else {
        None
    };

    let pre_deduction = req.pre_deduction.unwrap_or(1.0);
    let site_discount = req.site_discount.unwrap_or(1.0);
    let site_discount_enabled = req.site_discount_enabled.unwrap_or(1);
    let global_discount = req.global_discount.unwrap_or(1.0);
    let global_discount_enabled = req.global_discount_enabled.unwrap_or(0);
    let discount_schedule = req.discount_schedule.clone().unwrap_or_default();
    validate_unified_model_discounts(
        site_discount,
        site_discount_enabled,
        global_discount,
        global_discount_enabled,
        &discount_schedule,
    )?;

    let original_id = req.original_id.unwrap_or_default();
    let model_id_alias = req.model_id_alias.unwrap_or_default();

    let is_active = req.is_active.unwrap_or(1);
    let enable_log_content = req.enable_log_content.unwrap_or(0);
    let sort_order = req.sort_order.unwrap_or(0);

    let new_id = sqlx::query(
        &state.db.format_query(r#"INSERT INTO models (mid, name, model_id, original_id, model_id_alias, provider_id, api_provider_id, type_id, group_ratios, forward_rule_ids, billing_rule_id, pre_deduction, site_discount, site_discount_enabled, global_discount, global_discount_enabled, discount_schedule, is_active, enable_log_content, is_system, library_mid, is_listed, logo, remark, description, feature_attributes, sort_order)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, NULL, 1, ?, ?, ?, ?, ?)
           RETURNING id"#)
    )
    .bind(&mid)
    .bind(&req.name)
    .bind(&req.model_id)
    .bind(&original_id)
    .bind(&model_id_alias)
    .bind(req.provider_id)
    .bind(req.api_provider_id)
    .bind(req.type_id)
    .bind(&group_ratios)
    .bind(forward_rule_ids)
    .bind(req.billing_rule_id)
    .bind(pre_deduction)
    .bind(site_discount)
    .bind(site_discount_enabled)
    .bind(global_discount)
    .bind(global_discount_enabled)
    .bind(&discount_schedule)
    .bind(is_active)
    .bind(enable_log_content)
    .bind(&req.logo)
    .bind(&req.remark)
    .bind(&req.description)
    .bind(&req.feature_attributes)
    .bind(sort_order)
    .fetch_one(&state.db.pool)
    .await?
    .get::<i64, _>("id");

    let model = sqlx::query_as(&state.db.format_query("SELECT * FROM models WHERE id = ?"))
        .bind(new_id)
        .fetch_one(&state.db.pool)
        .await?;

    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(model))
}

/// 从模型库复制一份到上架模型；新 mid，目录后续更新不影响这份。
pub async fn publish_library_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<Model>> {
    let src: Model = sqlx::query_as(&format!(
        "SELECT {} FROM model_library WHERE id = $1",
        crate::db::preset_models::LIBRARY_MODEL_COLUMNS
    ))
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| crate::error::AppError::NotFound("模型库中未找到该模型".to_string()))?;

    let existing: Option<(i64, i64)> = sqlx::query_as(
        &state.db.format_query(
            "SELECT id, is_listed FROM models WHERE (library_mid = ? AND library_mid <> '') OR mid = ? ORDER BY is_listed DESC, id DESC LIMIT 1"
        )
    )
    .bind(&src.mid)
    .bind(&src.mid)
    .fetch_optional(&state.db.pool)
    .await?;

    if let Some((_existing_id, is_listed)) = existing {
        if is_listed == 1 {
            return Err(crate::error::AppError::BadRequest(
                "该模型已在运营模型中上架，不能重复上架".to_string(),
            ));
        } else {
            return Err(crate::error::AppError::BadRequest(
                "该模型已存在于仓库（已下架状态），请在仓库中直接点击重新上架".to_string(),
            ));
        }
    }

    let mid = allocate_listed_mid(&state).await?;
    let group_ratios = if src.group_ratios.trim().is_empty() {
        r#"{"default":1.0}"#.to_string()
    } else {
        src.group_ratios.clone()
    };

    let new_id = sqlx::query(
        &state.db.format_query(
            r#"INSERT INTO models (
                    mid, name, model_id, original_id, model_id_alias,
                    provider_id, api_provider_id, type_id, group_ratios, forward_rule_ids,
                    billing_rule_id, pre_deduction, site_discount, site_discount_enabled,
                    global_discount, global_discount_enabled, discount_schedule, is_active, enable_log_content,
                    is_system, library_mid, is_listed, logo, remark, description, feature_attributes, sort_order
               )
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, 1, ?, ?, ?, ?, ?)
               RETURNING id"#,
        ),
    )
    .bind(&mid)
    .bind(&src.name)
    .bind(&src.model_id)
    .bind(&src.original_id)
    .bind(&src.model_id_alias)
    .bind(src.provider_id)
    .bind(src.api_provider_id)
    .bind(src.type_id)
    .bind(&group_ratios)
    .bind(&src.forward_rule_ids)
    .bind(src.billing_rule_id)
    .bind(src.pre_deduction)
    .bind(src.site_discount)
    .bind(src.site_discount_enabled)
    .bind(src.global_discount)
    .bind(src.global_discount_enabled)
    .bind(&src.discount_schedule)
    .bind(src.is_active)
    .bind(src.enable_log_content)
    .bind(&src.mid)
    .bind(&src.logo)
    .bind(&src.remark)
    .bind(&src.description)
    .bind(&src.feature_attributes)
    .bind(src.sort_order)
    .fetch_one(&state.db.pool)
    .await?
    .get::<i64, _>("id");

    let model = sqlx::query_as(&state.db.format_query("SELECT * FROM models WHERE id = ?"))
        .bind(new_id)
        .fetch_one(&state.db.pool)
        .await?;

    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(model))
}

pub async fn unlist_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<Model>> {
    let listed: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_listed FROM models WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;
    match listed {
        None => return Err(crate::error::AppError::NotFound("模型不存在".to_string())),
        Some(v) if v != 1 => {
            return Err(crate::error::AppError::BadRequest(
                "该模型已下架".to_string(),
            ))
        }
        Some(_) => {}
    }
    sqlx::query(&state.db.format_query(
        "UPDATE models SET is_listed = 0, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    ))
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    let model = sqlx::query_as(&state.db.format_query("SELECT * FROM models WHERE id = ?"))
        .bind(id)
        .fetch_one(&state.db.pool)
        .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(model))
}

pub async fn relist_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<Model>> {
    let listed: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_listed FROM models WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;
    match listed {
        None => return Err(crate::error::AppError::NotFound("模型不存在".to_string())),
        Some(v) if v == 1 => {
            return Err(crate::error::AppError::BadRequest(
                "该模型已上架".to_string(),
            ))
        }
        Some(_) => {}
    }
    sqlx::query(&state.db.format_query(
        "UPDATE models SET is_listed = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    ))
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    let model = sqlx::query_as(&state.db.format_query("SELECT * FROM models WHERE id = ?"))
        .bind(id)
        .fetch_one(&state.db.pool)
        .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(model))
}

#[derive(Debug, serde::Deserialize)]
pub struct CopyModelRequest {
    /// 新名称；空或缺省沿用原名
    pub name: Option<String>,
}

fn copy_name(req: &CopyModelRequest, src_name: &str) -> String {
    req.name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(src_name)
        .to_string()
}

/// 仓库目录副本 mid：避开 `model_library` 与 `models` 已用 mid（目录可见性按 mid 判断）。
async fn allocate_library_mid(state: &AppState) -> AppResult<String> {
    loop {
        let candidate: String = {
            let n: u32 = rand::thread_rng().gen_range(320000..=329999);
            n.to_string()
        };
        let taken: bool = sqlx::query_scalar(&state.db.format_query(
            "SELECT EXISTS(SELECT 1 FROM model_library WHERE mid = ?) OR EXISTS(SELECT 1 FROM models WHERE mid = ?)",
        ))
        .bind(&candidate)
        .bind(&candidate)
        .fetch_one(&state.db.pool)
        .await?;
        if !taken {
            return Ok(candidate);
        }
    }
}

/// 复制 `models` 行（运营模型或仓库里的下架行）：新 mid，可改名，其余列原样，留在原列表。
pub async fn copy_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<CopyModelRequest>,
) -> AppResult<Json<Model>> {
    let src_name: String = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT name FROM models WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| crate::error::AppError::NotFound("模型不存在".to_string()))?;
    let name = copy_name(&req, &src_name);
    let mid = allocate_listed_mid(&state).await?;

    let new_id: i64 = sqlx::query_scalar(&state.db.format_query(
        r#"INSERT INTO models (
                mid, name, model_id, original_id, model_id_alias,
                provider_id, api_provider_id, type_id, group_ratios, forward_rule_ids,
                billing_rule_id, pre_deduction, site_discount, site_discount_enabled,
                global_discount, global_discount_enabled, discount_schedule, is_active, enable_log_content,
                is_system, library_mid, is_listed, logo, remark, description, feature_attributes, sort_order,
                created_at, updated_at
           )
           SELECT
                ?, ?, model_id, original_id, model_id_alias,
                provider_id, api_provider_id, type_id, group_ratios, forward_rule_ids,
                billing_rule_id, pre_deduction, site_discount, site_discount_enabled,
                global_discount, global_discount_enabled, discount_schedule, is_active, enable_log_content,
                is_system, library_mid, is_listed, logo, remark, description, feature_attributes, sort_order,
                CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
           FROM models WHERE id = ?
           RETURNING id"#,
    ))
    .bind(&mid)
    .bind(&name)
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| crate::error::AppError::NotFound("模型不存在".to_string()))?;

    let model = sqlx::query_as(&state.db.format_query("SELECT * FROM models WHERE id = ?"))
        .bind(new_id)
        .fetch_one(&state.db.pool)
        .await?;
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(model))
}

/// 复制模型仓库目录条目（`model_library`）：新 mid，可改名，其余列原样，仍在仓库目录。
pub async fn copy_library_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<CopyModelRequest>,
) -> AppResult<Json<Model>> {
    let src_name: String = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT name FROM model_library WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| crate::error::AppError::NotFound("模型库中未找到该模型".to_string()))?;
    let name = copy_name(&req, &src_name);
    let mid = allocate_library_mid(&state).await?;

    let new_id: i64 = sqlx::query_scalar(&state.db.format_query(
        r#"INSERT INTO model_library (
                mid, name, model_id, original_id, model_id_alias,
                provider_id, api_provider_id, type_id, group_ratios, billing_rule_id,
                pre_deduction, is_active, forward_rule_ids, enable_log_content,
                site_discount, site_discount_enabled, global_discount, global_discount_enabled,
                discount_schedule, logo, remark, description, feature_attributes, is_system, sort_order,
                created_at, updated_at
           )
           SELECT
                ?, ?, model_id, original_id, model_id_alias,
                provider_id, api_provider_id, type_id, group_ratios, billing_rule_id,
                pre_deduction, is_active, forward_rule_ids, enable_log_content,
                site_discount, site_discount_enabled, global_discount, global_discount_enabled,
                discount_schedule, logo, remark, description, feature_attributes, is_system, sort_order,
                CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
           FROM model_library WHERE id = ?
           RETURNING id"#,
    ))
    .bind(&mid)
    .bind(&name)
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| crate::error::AppError::NotFound("模型库中未找到该模型".to_string()))?;

    let mut model: Model = sqlx::query_as(&format!(
        "SELECT {} FROM model_library WHERE id = $1",
        crate::db::preset_models::LIBRARY_MODEL_COLUMNS
    ))
    .bind(new_id)
    .fetch_one(&state.db.pool)
    .await?;
    model.library_kind = Some("catalog".into());
    crate::api::plugins::notify_marketplace_data_changed(&state).await;
    Ok(Json(model))
}

/// 把 JSON 对象的顶层 key 从旧模型请求 ID 改到新 ID。新 key 已存在时保留新值，只丢掉旧 key。
fn rename_json_object_key(value: &mut serde_json::Value, old_id: &str, new_id: &str) -> bool {
    if old_id.is_empty() || old_id == new_id {
        return false;
    }
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    if !obj.contains_key(old_id) {
        return false;
    }
    if obj.contains_key(new_id) {
        obj.remove(old_id);
        return true;
    }
    if let Some(existing) = obj.remove(old_id) {
        obj.insert(new_id.to_string(), existing);
        return true;
    }
    false
}

/// 渠道别名（含高可用、分辨率）按 model_id 索引。请求 ID 变更后，已绑定该 mid 的渠道要把旧 key 改到新 ID。
/// `peer_selected`：同渠道还选了别的、仍使用旧 ID 的模型，这条映射不能搬走。
/// `peers_still_use_old`：模型表里还有别人用着旧 ID，不能把 models 数组里的旧 ID 文本改成这个 mid。
fn retarget_channel_model_keys(
    models_raw: &str,
    mapping_raw: &str,
    config_raw: &str,
    mid: &str,
    old_id: &str,
    new_id: &str,
    peers_still_use_old: bool,
    peer_selected: bool,
) -> (String, String, String, bool) {
    if mid.is_empty() || old_id.is_empty() || old_id == new_id || peer_selected {
        return (
            models_raw.to_string(),
            mapping_raw.to_string(),
            config_raw.to_string(),
            false,
        );
    }
    let Ok(mut models) = serde_json::from_str::<Vec<String>>(models_raw) else {
        return (
            models_raw.to_string(),
            mapping_raw.to_string(),
            config_raw.to_string(),
            false,
        );
    };
    let bound_mid = models.iter().any(|item| item == mid);
    let bound_legacy = models.iter().any(|item| item == old_id);
    if !bound_mid && !bound_legacy {
        return (
            models_raw.to_string(),
            mapping_raw.to_string(),
            config_raw.to_string(),
            false,
        );
    }

    let mut models_changed = false;
    let mut rename_maps = bound_mid;
    if bound_legacy && !peers_still_use_old {
        rename_maps = true;
        if bound_mid {
            models.retain(|item| item != old_id);
        } else {
            for item in &mut models {
                if item == old_id {
                    *item = mid.to_string();
                }
            }
        }
        models_changed = true;
    }

    let mut mapping: serde_json::Value =
        serde_json::from_str(mapping_raw).unwrap_or_else(|_| serde_json::json!({}));
    let mut config: serde_json::Value =
        serde_json::from_str(config_raw).unwrap_or_else(|_| serde_json::json!({}));
    let mut maps_changed = false;
    if rename_maps {
        maps_changed |= rename_json_object_key(&mut mapping, old_id, new_id);
        if let Some(obj) = config.as_object_mut() {
            for key in ["ha_model_mapping", "res_model_mapping"] {
                if let Some(map) = obj.get_mut(key) {
                    maps_changed |= rename_json_object_key(map, old_id, new_id);
                }
            }
        }
    }
    if !models_changed && !maps_changed {
        return (
            models_raw.to_string(),
            mapping_raw.to_string(),
            config_raw.to_string(),
            false,
        );
    }
    let models_out = if models_changed {
        serde_json::to_string(&models).unwrap_or_else(|_| models_raw.to_string())
    } else {
        models_raw.to_string()
    };
    let mapping_out = if maps_changed {
        serde_json::to_string(&mapping).unwrap_or_else(|_| mapping_raw.to_string())
    } else {
        mapping_raw.to_string()
    };
    let config_out = if maps_changed {
        serde_json::to_string(&config).unwrap_or_else(|_| config_raw.to_string())
    } else {
        config_raw.to_string()
    };
    (models_out, mapping_out, config_out, true)
}

/// 模型请求 ID 变更后，把渠道上的别名映射 key 改到新 ID，避免编辑渠道时映射对不上而丢失。
async fn retarget_channels_after_model_id_change(
    state: &AppState,
    model_pk: i64,
    mid: &str,
    old_id: &str,
    new_id: &str,
) -> AppResult<()> {
    if mid.is_empty() || old_id.is_empty() || old_id == new_id {
        return Ok(());
    }
    let peer_mids: Vec<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT mid FROM models WHERE model_id = ? AND id != ?"),
    )
    .bind(old_id)
    .bind(model_pk)
    .fetch_all(&state.db.pool)
    .await?;

    let rows = sqlx::query(
        &state
            .db
            .format_query("SELECT id, models, model_mapping, config FROM channels"),
    )
    .fetch_all(&state.db.pool)
    .await?;

    for row in rows {
        let channel_id: i64 = row.try_get("id")?;
        let models_raw: String = row.try_get("models").unwrap_or_else(|_| "[]".to_string());
        let mapping_raw: String = row
            .try_get("model_mapping")
            .unwrap_or_else(|_| "{}".to_string());
        let config_raw: String = row.try_get("config").unwrap_or_else(|_| "{}".to_string());
        let selected: Vec<String> = serde_json::from_str(&models_raw).unwrap_or_default();
        let peer_selected = peer_mids
            .iter()
            .any(|peer| selected.iter().any(|item| item == peer));
        let (models_out, mapping_out, config_out, changed) = retarget_channel_model_keys(
            &models_raw,
            &mapping_raw,
            &config_raw,
            mid,
            old_id,
            new_id,
            !peer_mids.is_empty(),
            peer_selected,
        );
        if !changed {
            continue;
        }
        sqlx::query(&state.db.format_query(
            "UPDATE channels SET models = ?, model_mapping = ?, config = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ))
        .bind(&models_out)
        .bind(&mapping_out)
        .bind(&config_out)
        .bind(channel_id)
        .execute(&state.db.pool)
        .await?;
    }
    Ok(())
}

fn validate_unified_model_discounts(
    site_discount: f64,
    site_enabled: i64,
    global_discount: f64,
    global_enabled: i64,
    schedule: &str,
) -> AppResult<()> {
    if !site_discount.is_finite() || site_discount <= 0.0 {
        return Err(crate::error::AppError::BadRequest(
            "折扣限价倍率必须大于 0".to_string(),
        ));
    }
    if !global_discount.is_finite() || global_discount <= 0.0 {
        return Err(crate::error::AppError::BadRequest(
            "全站折扣倍率必须大于 0".to_string(),
        ));
    }
    if site_enabled == 1 && global_enabled == 1 && global_discount < site_discount {
        return Err(crate::error::AppError::BadRequest(format!(
            "全站折扣 {:.2} 不能低于折扣限价 {:.2}",
            global_discount, site_discount
        )));
    }
    if let Err(msg) =
        crate::models::validate_discount_schedule(schedule, site_enabled == 1, global_enabled == 1)
    {
        return Err(crate::error::AppError::BadRequest(msg));
    }
    Ok(())
}

async fn update_model_discounts(
    state: &crate::AppState,
    table: &str,
    id: i64,
    req: &UpdateModelRequest,
) -> AppResult<()> {
    if req.site_discount.is_none()
        && req.site_discount_enabled.is_none()
        && req.global_discount.is_none()
        && req.global_discount_enabled.is_none()
        && req.discount_schedule.is_none()
    {
        return Ok(());
    }
    let row: (f64, i64, f64, i64, String) = sqlx::query_as(&state.db.format_query(&format!(
        "SELECT site_discount, site_discount_enabled, global_discount, global_discount_enabled, discount_schedule FROM {table} WHERE id = ?"
    )))
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    let site_discount = req.site_discount.unwrap_or(row.0);
    let site_enabled = req.site_discount_enabled.unwrap_or(row.1);
    let global_discount = req.global_discount.unwrap_or(row.2);
    let global_enabled = req.global_discount_enabled.unwrap_or(row.3);
    let schedule = req.discount_schedule.as_deref().unwrap_or(row.4.as_str());

    validate_unified_model_discounts(
        site_discount,
        site_enabled,
        global_discount,
        global_enabled,
        schedule,
    )?;

    sqlx::query(&state.db.format_query(&format!(
        "UPDATE {table} SET site_discount = ?, site_discount_enabled = ?, global_discount = ?, global_discount_enabled = ?, discount_schedule = ? WHERE id = ?"
    )))
    .bind(site_discount)
    .bind(site_enabled)
    .bind(global_discount)
    .bind(global_enabled)
    .bind(schedule)
    .bind(id)
    .execute(&state.db.pool)
    .await?;
    Ok(())
}

pub async fn update_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(mut req): Json<UpdateModelRequest>,
) -> AppResult<Json<Model>> {
    // Basic trimming if fields are provided
    if let Some(name) = &mut req.name {
        *name = name.trim().to_string();
        if name.is_empty() {
            return Err(crate::error::AppError::BadRequest(
                "名称不能为空".to_string(),
            ));
        }
    }
    if let Some(model_id) = &mut req.model_id {
        *model_id = model_id.trim().to_string();
        if model_id.is_empty() {
            return Err(crate::error::AppError::BadRequest(
                "模型 ID 不能为空".to_string(),
            ));
        }
    }

    // mid 是唯一标识，允许 name 和 model_id 重复（不同 mid 可对应相同 model_id，用于差异化计费）

    let old_model_id: String = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT model_id FROM models WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?
    .unwrap_or_default();

    // 若当前模型 mid 为空，自动生成一个 30 开头的唯一 6 位 mid
    let current_mid: Option<String> =
        sqlx::query_scalar(&state.db.format_query("SELECT mid FROM models WHERE id = ?"))
            .bind(id)
            .fetch_optional(&state.db.pool)
            .await?;
    if current_mid.as_deref().unwrap_or("").is_empty() {
        let new_mid = loop {
            let candidate: String = {
                let n: u32 = rand::thread_rng().gen_range(300000..=309999);
                n.to_string()
            };
            let taken: Option<i64> = sqlx::query_scalar(
                &state
                    .db
                    .format_query("SELECT id FROM models WHERE mid = ? AND id != ?"),
            )
            .bind(&candidate)
            .bind(id)
            .fetch_optional(&state.db.pool)
            .await?;
            if taken.is_none() {
                break candidate;
            }
        };
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET mid = ? WHERE id = ?"),
        )
        .bind(&new_mid)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }

    if let Some(name) = &req.name {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET name = ? WHERE id = ?"),
        )
        .bind(name)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(model_id) = &req.model_id {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET model_id = ? WHERE id = ?"),
        )
        .bind(model_id)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
        if model_id != &old_model_id {
            let mid: String =
                sqlx::query_scalar(&state.db.format_query("SELECT mid FROM models WHERE id = ?"))
                    .bind(id)
                    .fetch_optional(&state.db.pool)
                    .await?
                    .unwrap_or_default();
            retarget_channels_after_model_id_change(&state, id, &mid, &old_model_id, model_id)
                .await?;
        }
    }
    if let Some(original_id) = &req.original_id {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET original_id = ? WHERE id = ?"),
        )
        .bind(original_id)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(model_id_alias) = &req.model_id_alias {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET model_id_alias = ? WHERE id = ?"),
        )
        .bind(model_id_alias)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(pid) = req.provider_id {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET provider_id = ? WHERE id = ?"),
        )
        .bind(pid)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(apid) = req.api_provider_id {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET api_provider_id = ? WHERE id = ?"),
        )
        .bind(apid)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(tid) = req.type_id {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET type_id = ? WHERE id = ?"),
        )
        .bind(tid)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(ref gr) = req.group_ratios {
        let gr_str = serde_json::to_string(gr).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET group_ratios = ? WHERE id = ?"),
        )
        .bind(&gr_str)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(rule_id) = req.billing_rule_id {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET billing_rule_id = ? WHERE id = ?"),
        )
        .bind(rule_id)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(pd) = req.pre_deduction {
        // Here we could enforce billing_type == "tokens" logic, but since billing_type is now in billing_rules,
        // we'll rely on the frontend to send pre_deduction = 0.0, or we could fetch the rule and check.
        // For strictness, if the frontend implements it, this is fine. If not, we can check here.
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET pre_deduction = ? WHERE id = ?"),
        )
        .bind(pd)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(rules) = &req.forward_rule_ids {
        // 过滤掉不存在或已禁用的规则 ID，防止脏数据入库
        let mut valid_ids: Vec<i64> = Vec::new();
        for rid in rules {
            let exists: bool = sqlx::query_scalar(&state.db.format_query(
                "SELECT EXISTS(SELECT 1 FROM forward_rules WHERE id = ? AND is_active = 1)",
            ))
            .bind(rid)
            .fetch_one(&state.db.pool)
            .await
            .unwrap_or(false);
            if exists {
                valid_ids.push(*rid);
            }
        }
        let rules_str = serde_json::to_string(&valid_ids).unwrap_or_else(|_| "[]".to_string());
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET forward_rule_ids = ? WHERE id = ?"),
        )
        .bind(&rules_str)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(active) = req.is_active {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET is_active = ? WHERE id = ?"),
        )
        .bind(active)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(elc) = req.enable_log_content {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET enable_log_content = ? WHERE id = ?"),
        )
        .bind(elc)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    update_model_discounts(&state, "models", id, &req).await?;
    if let Some(ref logo) = req.logo {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET logo = ? WHERE id = ?"),
        )
        .bind(logo)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(ref remark) = req.remark {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET remark = ? WHERE id = ?"),
        )
        .bind(remark)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(ref description) = req.description {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET description = ? WHERE id = ?"),
        )
        .bind(description)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(ref feature_attributes) = req.feature_attributes {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET feature_attributes = ? WHERE id = ?"),
        )
        .bind(feature_attributes)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(sort_order) = req.sort_order {
        sqlx::query(
            &state
                .db
                .format_query("UPDATE models SET sort_order = ? WHERE id = ?"),
        )
        .bind(sort_order)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
        crate::api::plugins::sync_marketplace_page_sort(&state, id, sort_order).await?;
        crate::api::plugins::sync_playground_2026_page_sort(&state, id, sort_order).await?;
    }

    sqlx::query(
        &state
            .db
            .format_query("UPDATE models SET updated_at = CURRENT_TIMESTAMP WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    let model = sqlx::query_as(&state.db.format_query("SELECT * FROM models WHERE id = ?"))
        .bind(id)
        .fetch_one(&state.db.pool)
        .await?;

    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(model))
}

pub async fn delete_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let origin: Option<(Option<String>, i64)> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT library_mid, is_listed FROM models WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;
    if let Some((library_mid, is_listed)) = origin {
        let from_library = library_mid
            .as_deref()
            .map(str::trim)
            .is_some_and(|mid| !mid.is_empty());
        if is_listed == 1 && from_library {
            return Err(crate::error::AppError::BadRequest(
                "来自模型仓库的运营模型不能删除，请下架".to_string(),
            ));
        }
    }

    sqlx::query(&state.db.format_query("DELETE FROM models WHERE id = ?"))
        .bind(id)
        .execute(&state.db.pool)
        .await?;

    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// 编辑模型库（`model_library`）中的预置目录条目。
pub async fn update_library_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(mut req): Json<UpdateModelRequest>,
) -> AppResult<Json<Model>> {
    // 校验
    if let Some(name) = &mut req.name {
        *name = name.trim().to_string();
        if name.is_empty() {
            return Err(crate::error::AppError::BadRequest(
                "名称不能为空".to_string(),
            ));
        }
    }
    if let Some(model_id) = &mut req.model_id {
        *model_id = model_id.trim().to_string();
        if model_id.is_empty() {
            return Err(crate::error::AppError::BadRequest(
                "模型 ID 不能为空".to_string(),
            ));
        }
    }

    let tbl = "model_library";

    // 逐字段更新
    macro_rules! set_field {
        ($field:ident, $col:expr) => {
            if let Some(ref val) = req.$field {
                sqlx::query(
                    &state
                        .db
                        .format_query(&format!("UPDATE {tbl} SET {} = ? WHERE id = ?", $col)),
                )
                .bind(val)
                .bind(id)
                .execute(&state.db.pool)
                .await?;
            }
        };
    }
    set_field!(name, "name");
    set_field!(model_id, "model_id");
    set_field!(original_id, "original_id");
    set_field!(model_id_alias, "model_id_alias");
    set_field!(logo, "logo");
    set_field!(remark, "remark");
    set_field!(description, "description");
    set_field!(feature_attributes, "feature_attributes");

    if let Some(pid) = req.provider_id {
        sqlx::query(
            &state
                .db
                .format_query(&format!("UPDATE {tbl} SET provider_id = ? WHERE id = ?")),
        )
        .bind(pid)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(apid) = req.api_provider_id {
        sqlx::query(&state.db.format_query(&format!(
            "UPDATE {tbl} SET api_provider_id = ? WHERE id = ?"
        )))
        .bind(apid)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(tid) = req.type_id {
        sqlx::query(
            &state
                .db
                .format_query(&format!("UPDATE {tbl} SET type_id = ? WHERE id = ?")),
        )
        .bind(tid)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(ref gr) = req.group_ratios {
        let gr_str = serde_json::to_string(gr).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            &state
                .db
                .format_query(&format!("UPDATE {tbl} SET group_ratios = ? WHERE id = ?")),
        )
        .bind(&gr_str)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(rule_id) = req.billing_rule_id {
        sqlx::query(&state.db.format_query(&format!(
            "UPDATE {tbl} SET billing_rule_id = ? WHERE id = ?"
        )))
        .bind(rule_id)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(pd) = req.pre_deduction {
        sqlx::query(
            &state
                .db
                .format_query(&format!("UPDATE {tbl} SET pre_deduction = ? WHERE id = ?")),
        )
        .bind(pd)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(active) = req.is_active {
        sqlx::query(
            &state
                .db
                .format_query(&format!("UPDATE {tbl} SET is_active = ? WHERE id = ?")),
        )
        .bind(active)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(rules) = &req.forward_rule_ids {
        let mut valid_ids: Vec<i64> = Vec::new();
        for rid in rules {
            let exists: bool = sqlx::query_scalar(&state.db.format_query(
                "SELECT EXISTS(SELECT 1 FROM forward_rules WHERE id = ? AND is_active = 1)",
            ))
            .bind(rid)
            .fetch_one(&state.db.pool)
            .await
            .unwrap_or(false);
            if exists {
                valid_ids.push(*rid);
            }
        }
        let rules_str = serde_json::to_string(&valid_ids).unwrap_or_else(|_| "[]".to_string());
        sqlx::query(&state.db.format_query(&format!(
            "UPDATE {tbl} SET forward_rule_ids = ? WHERE id = ?"
        )))
        .bind(&rules_str)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    if let Some(elc) = req.enable_log_content {
        sqlx::query(&state.db.format_query(&format!(
            "UPDATE {tbl} SET enable_log_content = ? WHERE id = ?"
        )))
        .bind(elc)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }
    update_model_discounts(&state, tbl, id, &req).await?;
    if let Some(sort_order) = req.sort_order {
        sqlx::query(
            &state
                .db
                .format_query(&format!("UPDATE {tbl} SET sort_order = ? WHERE id = ?")),
        )
        .bind(sort_order)
        .bind(id)
        .execute(&state.db.pool)
        .await?;
    }

    sqlx::query(&state.db.format_query(&format!(
        "UPDATE {tbl} SET updated_at = CURRENT_TIMESTAMP WHERE id = ?"
    )))
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    let model: Model = sqlx::query_as(&format!(
        "SELECT {} FROM model_library WHERE id = $1",
        crate::db::preset_models::LIBRARY_MODEL_COLUMNS
    ))
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(model))
}

pub async fn delete_library_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    sqlx::query(
        &state
            .db
            .format_query("DELETE FROM model_library WHERE id = ?"),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?;

    crate::api::plugins::notify_marketplace_data_changed(&state).await;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[cfg(test)]
mod model_id_retarget_tests {
    use super::retarget_channel_model_keys;

    #[test]
    fn moves_alias_keys_when_channel_binds_mid() {
        let (models, mapping, config, changed) = retarget_channel_model_keys(
            r#"["301001"]"#,
            r#"{"old-id":"upstream-a"}"#,
            r#"{"ha_model_mapping":{"old-id":{"9":"ha-up"}},"res_model_mapping":{"old-id":{"default":{"1080p":"r-up"}}},"tos_storage_enabled":true}"#,
            "301001",
            "old-id",
            "new-id",
            false,
            false,
        );
        assert!(changed);
        assert_eq!(models, r#"["301001"]"#);
        let mapping: serde_json::Value = serde_json::from_str(&mapping).unwrap();
        assert_eq!(mapping["new-id"], "upstream-a");
        assert!(mapping.get("old-id").is_none());
        let config: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(config["ha_model_mapping"]["new-id"]["9"], "ha-up");
        assert_eq!(
            config["res_model_mapping"]["new-id"]["default"]["1080p"],
            "r-up"
        );
        assert_eq!(config["tos_storage_enabled"], true);
    }

    #[test]
    fn leaves_mapping_when_another_selected_model_still_uses_old_id() {
        let (models, mapping, _, changed) = retarget_channel_model_keys(
            r#"["301001","309999"]"#,
            r#"{"old-id":"upstream-a"}"#,
            "{}",
            "301001",
            "old-id",
            "new-id",
            true,
            true,
        );
        assert!(!changed);
        assert_eq!(models, r#"["301001","309999"]"#);
        assert_eq!(mapping, r#"{"old-id":"upstream-a"}"#);
    }

    #[test]
    fn rewrites_legacy_model_id_token_only_when_nobody_else_owns_it() {
        let (models, mapping, _, changed) = retarget_channel_model_keys(
            r#"["old-id"]"#,
            r#"{"old-id":"upstream-a"}"#,
            "{}",
            "301001",
            "old-id",
            "new-id",
            false,
            false,
        );
        assert!(changed);
        assert_eq!(models, r#"["301001"]"#);
        let mapping: serde_json::Value = serde_json::from_str(&mapping).unwrap();
        assert_eq!(mapping["new-id"], "upstream-a");
    }
}

#[cfg(test)]
mod copy_name_tests {
    use super::{copy_name, CopyModelRequest};

    #[test]
    fn keeps_source_name_when_blank_or_missing() {
        let none = CopyModelRequest { name: None };
        let blank = CopyModelRequest {
            name: Some("   ".into()),
        };
        assert_eq!(copy_name(&none, "GPT-4o"), "GPT-4o");
        assert_eq!(copy_name(&blank, "GPT-4o"), "GPT-4o");
    }

    #[test]
    fn uses_trimmed_new_name() {
        let req = CopyModelRequest {
            name: Some("  GPT-4o 副本 ".into()),
        };
        assert_eq!(copy_name(&req, "GPT-4o"), "GPT-4o 副本");
    }
}
