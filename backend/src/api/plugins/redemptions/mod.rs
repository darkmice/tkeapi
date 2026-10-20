/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::auth;
use crate::models::{
    CreateRedemptionRequest, RedeemRequest, Redemption, RedemptionGroupResponse,
    RedemptionListResponse,
};
use crate::AppState;
use chrono::Datelike;
use axum::{
    extract::{ConnectInfo, Extension, Path, Query, State},
    http::HeaderMap,
    Json,
};
use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

use rand::{distributions::Alphanumeric, Rng};

/// Admin: List all redemption codes
pub async fn list_redemptions(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Query(query): Query<crate::models::RedemptionQuery>,
) -> AppResult<Json<RedemptionListResponse>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(10).max(1).min(100);
    let offset = (page - 1) * page_size;

    let code_filter = query
        .code
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{}%", s));

    let code_type_filter = query
        .code_type
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let mut qb = sqlx::QueryBuilder::new("SELECT * FROM redemptions WHERE 1=1 ");
    if let Some(name) = &query.name {
        qb.push(" AND name = ");
        qb.push_bind(name);
    }
    if let Some(code_type) = code_type_filter {
        if code_type == "balance" {
            qb.push(" AND (code_type = 'balance' OR code_type IS NULL OR code_type = '')");
        } else {
            qb.push(" AND code_type = ");
            qb.push_bind(code_type);
        }
    }
    if let Some(code) = &code_filter {
        qb.push(" AND code ILIKE ");
        qb.push_bind(code);
    }
    qb.push(" ORDER BY id DESC LIMIT ");
    qb.push_bind(page_size);
    qb.push(" OFFSET ");
    qb.push_bind(offset);

    let redemptions: Vec<Redemption> = qb.build_query_as().fetch_all(&state.db.pool).await?;

    let mut count_qb = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM redemptions WHERE 1=1 ");
    if let Some(name) = &query.name {
        count_qb.push(" AND name = ");
        count_qb.push_bind(name);
    }
    if let Some(code_type) = code_type_filter {
        if code_type == "balance" {
            count_qb.push(" AND (code_type = 'balance' OR code_type IS NULL OR code_type = '')");
        } else {
            count_qb.push(" AND code_type = ");
            count_qb.push_bind(code_type);
        }
    }
    if let Some(code) = &code_filter {
        count_qb.push(" AND code ILIKE ");
        count_qb.push_bind(code);
    }
    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(&state.db.pool)
        .await?;

    let ids: Vec<i64> = redemptions.iter().map(|row| row.id).collect();
    let mut redeemers_by_code: std::collections::HashMap<i64, Vec<crate::models::RedemptionRedeemer>> =
        std::collections::HashMap::new();
    if !ids.is_empty() {
        #[derive(sqlx::FromRow)]
        struct RedeemerRow {
            log_id: i64,
            redemption_id: i64,
            user_id: String,
            amount: f64,
            gift_amount: f64,
            created_at: crate::time_system::DbTs,
            refunded: i64,
            username: String,
            uid: String,
            remark: Option<String>,
            referrer_username: Option<String>,
            referrer_uid: Option<String>,
        }
        let mut redeemer_qb = sqlx::QueryBuilder::new(
            "SELECT l.id AS log_id, l.redemption_id::bigint AS redemption_id, l.user_id, l.amount, \
                    COALESCE(l.gift_amount, 0.0) AS gift_amount, l.created_at, l.refunded, \
                    COALESCE(u.username, '') AS username, \
                    COALESCE(u.uid, '') AS uid, \
                    COALESCE(NULLIF(u.admin_remark, ''), u.remark, '') AS remark, \
                    COALESCE(inv.username, '') AS referrer_username, \
                    COALESCE(inv.uid, '') AS referrer_uid \
             FROM redemption_logs l \
             LEFT JOIN users u ON u.id = l.user_id \
             LEFT JOIN users inv ON u.referred_by = inv.id \
             WHERE l.redemption_id IN (",
        );
        let mut separated = redeemer_qb.separated(", ");
        for id in &ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(") ORDER BY l.id ASC");
        let rows: Vec<RedeemerRow> = redeemer_qb.build_query_as().fetch_all(&state.db.pool).await?;
        let mut user_index: std::collections::HashMap<(i64, String), usize> =
            std::collections::HashMap::new();
        for row in rows {
            let key = (row.redemption_id, row.user_id.clone());
            let open_amount = if row.refunded == 0 {
                crate::money::round_money(row.amount)
            } else {
                0.0
            };
            let open_gift_amount = if row.refunded == 0 {
                crate::money::round_money(row.gift_amount)
            } else {
                0.0
            };
            let one_use = crate::models::RedemptionUse {
                id: row.log_id,
                amount: crate::money::round_money(row.amount),
                gift_amount: crate::money::round_money(row.gift_amount),
                created_at: row.created_at,
                refunded: row.refunded,
            };
            if let Some(pos) = user_index.get(&key).copied() {
                if let Some(list) = redeemers_by_code.get_mut(&row.redemption_id) {
                    list[pos].times += 1;
                    list[pos].amount = crate::money::round_money(list[pos].amount + open_amount);
                    list[pos].gift_amount =
                        crate::money::round_money(list[pos].gift_amount + open_gift_amount);
                    list[pos].uses.push(one_use);
                }
                continue;
            }
            let list = redeemers_by_code.entry(row.redemption_id).or_default();
            user_index.insert(key, list.len());
            list.push(crate::models::RedemptionRedeemer {
                user_id: row.user_id.clone(),
                username: if row.username.is_empty() {
                    row.user_id
                } else {
                    row.username
                },
                uid: row.uid,
                remark: row.remark.unwrap_or_default(),
                referrer_username: row.referrer_username.unwrap_or_default(),
                referrer_uid: row.referrer_uid.unwrap_or_default(),
                times: 1,
                amount: open_amount,
                gift_amount: open_gift_amount,
                uses: vec![one_use],
            });
        }

        let legacy_ids: Vec<String> = redemptions
            .iter()
            .filter(|row| {
                !redeemers_by_code.contains_key(&row.id)
                    && row.used_by.as_ref().is_some_and(|id| !id.is_empty())
            })
            .filter_map(|row| row.used_by.clone())
            .collect();
        if !legacy_ids.is_empty() {
            #[derive(sqlx::FromRow)]
            struct LegacyUser {
                id: String,
                username: String,
                uid: String,
                remark: Option<String>,
                referrer_username: Option<String>,
                referrer_uid: Option<String>,
            }
            let users: Vec<LegacyUser> = sqlx::query_as(&state.db.format_query(
                "SELECT u.id, COALESCE(u.username, '') AS username, COALESCE(u.uid, '') AS uid, \
                        u.admin_remark AS remark, inv.username AS referrer_username, inv.uid AS referrer_uid \
                 FROM users u \
                 LEFT JOIN users inv ON u.referred_by = inv.id \
                 WHERE u.id = ANY(?)",
            ))
            .bind(&legacy_ids)
            .fetch_all(&state.db.pool)
            .await?;
            let by_id: std::collections::HashMap<String, LegacyUser> =
                users.into_iter().map(|user| (user.id.clone(), user)).collect();
            for row in &redemptions {
                if redeemers_by_code.contains_key(&row.id) {
                    continue;
                }
                let Some(user_id) = row.used_by.as_ref() else { continue };
                let Some(user) = by_id.get(user_id) else { continue };
                let refunded = if row.status == BALANCE_RETURNED_STATUS { 1 } else { 0 };
                redeemers_by_code.insert(row.id, vec![crate::models::RedemptionRedeemer {
                    user_id: user.id.clone(),
                    username: if user.username.is_empty() { user.id.clone() } else { user.username.clone() },
                    uid: user.uid.clone(),
                    remark: user.remark.clone().unwrap_or_default(),
                    referrer_username: user.referrer_username.clone().unwrap_or_default(),
                    referrer_uid: user.referrer_uid.clone().unwrap_or_default(),
                    times: 1,
                    amount: if refunded == 0 { crate::money::round_money(row.quota) } else { 0.0 },
                    gift_amount: if refunded == 0 { crate::money::round_money(row.gift_quota) } else { 0.0 },
                    uses: vec![crate::models::RedemptionUse {
                        id: 0,
                        amount: crate::money::round_money(row.quota),
                        gift_amount: crate::money::round_money(row.gift_quota),
                        created_at: row.used_at.clone().unwrap_or_else(|| row.created_at.clone()),
                        refunded,
                    }],
                }]);
            }
        }
    }

    let data = redemptions
        .into_iter()
        .map(|row| {
            let redeemers = redeemers_by_code.remove(&row.id).unwrap_or_default();
            crate::models::RedemptionListItem { code: row, redeemers }
        })
        .collect();

    Ok(Json(RedemptionListResponse { data, total }))
}

/// 给还没有活动 ID 的历史活动补 4 位编号。同一活动名称共用一个 ID。
async fn assign_missing_activity_ids(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    let pending: i64 = sqlx::query_scalar(&state.db.format_query(
        "SELECT COUNT(*) FROM redemptions WHERE btrim(COALESCE(activity_id, '')) = ''",
    ))
    .fetch_one(&state.db.pool)
    .await?;
    if pending == 0 {
        return Ok(());
    }

    let mut tx = state.db.pool.begin().await?;
    sqlx::query(&state.db.format_query(
        "SELECT pg_advisory_xact_lock(hashtext('redemption_activity_id_alloc'), 0)",
    ))
    .execute(&mut *tx)
    .await?;

    let names: Vec<String> = sqlx::query_scalar(&state.db.format_query(
        "SELECT DISTINCT name FROM redemptions WHERE btrim(COALESCE(activity_id, '')) = ''",
    ))
    .fetch_all(&mut *tx)
    .await?;

    for name in names {
        let existing: Option<String> = sqlx::query_scalar(&state.db.format_query(
            "SELECT activity_id FROM redemptions WHERE name = ? AND btrim(COALESCE(activity_id, '')) <> '' LIMIT 1",
        ))
        .bind(&name)
        .fetch_optional(&mut *tx)
        .await?;
        let activity_id = if let Some(id) = existing {
            id
        } else {
            let mut generated = String::new();
            for _ in 0..64 {
                let candidate = format!("{:04}", rand::thread_rng().gen_range(0..10000u32));
                let taken: Option<i64> = sqlx::query_scalar(&state.db.format_query(
                    "SELECT id FROM redemptions WHERE activity_id = ? LIMIT 1",
                ))
                .bind(&candidate)
                .fetch_optional(&mut *tx)
                .await?;
                if taken.is_none() {
                    generated = candidate;
                    break;
                }
            }
            if generated.is_empty() {
                return Err(sqlx::Error::Protocol(
                    "redemption activity id exhausted".into(),
                ));
            }
            generated
        };
        sqlx::query(&state.db.format_query(
            "UPDATE redemptions SET activity_id = ? WHERE name = ? AND btrim(COALESCE(activity_id, '')) = ''",
        ))
        .bind(&activity_id)
        .bind(&name)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn list_redemption_groups(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Query(query): Query<crate::models::RedemptionQuery>,
) -> AppResult<Json<RedemptionGroupResponse>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }

    assign_missing_activity_ids(&state).await?;

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(10).max(1).min(100);
    let offset = (page - 1) * page_size;

    let name_filter = query
        .name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let code_type_filter = query
        .code_type
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT \
             name, \
             COUNT(id) as total_count, \
             COALESCE(SUM(quota), 0.0) as total_quota, \
             COALESCE(SUM(gift_quota), 0.0) as total_gift_quota, \
             COALESCE(SUM(worth_quota), 0.0) as total_worth_quota, \
             MAX(created_at) as created_at, \
             MAX(expires_at) as expires_at, \
             MIN(starts_at) as starts_at, \
             COALESCE(SUM(used_count), 0)::bigint as total_used_count, \
             MAX(max_uses) as max_uses, \
             MAX(per_user_limit) as per_user_limit, \
             MAX(per_user_activity_limit) as per_user_activity_limit, \
             MAX(code_type) as code_type, \
             COALESCE(( \
               SELECT jsonb_agg(model_id ORDER BY model_id)::text \
               FROM ( \
                 SELECT DISTINCT btrim(model_token) AS model_id \
                 FROM redemptions r_models \
                 CROSS JOIN LATERAL jsonb_array_elements_text( \
                   CASE \
                     WHEN left(btrim(r_models.allowed_models), 1) = '[' \
                      AND right(btrim(r_models.allowed_models), 1) = ']' \
                     THEN r_models.allowed_models::jsonb \
                     ELSE '[]'::jsonb \
                   END \
                 ) AS model_tokens(model_token) \
                 WHERE r_models.name = redemptions.name \
                   AND btrim(model_token) <> '' \
               ) listed \
             ), '[]') as allowed_models, \
             COALESCE(MAX(voucher_discount), 1.0) as voucher_discount, \
             COALESCE(MAX(model_discounts), '{}') as model_discounts, \
             MAX(valid_days) as valid_days, \
             MAX(activity_id) as activity_id, \
             MAX(condition_match) as condition_match, \
             MAX(cond_min_balance) as cond_min_balance, \
              MAX(cond_month_spend) as cond_month_spend, \
              MAX(cond_total_spend) as cond_total_spend, \
              MAX(cond_month_recharge) as cond_month_recharge, \
              MAX(cond_total_recharge) as cond_total_recharge, \
              COALESCE(MAX(description), '') as description, \
              COALESCE(MAX(remark), '') as remark \
              FROM redemptions WHERE 1=1 ",
    );
    if let Some(name) = name_filter {
        push_redemption_group_search(&mut qb, name);
    }
    if let Some(code_type) = code_type_filter {
        if code_type == "balance" {
            qb.push(" AND (code_type = 'balance' OR code_type IS NULL OR code_type = '')");
        } else {
            qb.push(" AND code_type = ");
            qb.push_bind(code_type);
        }
    }
    qb.push(" GROUP BY name ORDER BY MAX(created_at) DESC LIMIT ");
    qb.push_bind(page_size);
    qb.push(" OFFSET ");
    qb.push_bind(offset);

    let groups: Vec<crate::models::RedemptionGroup> =
        qb.build_query_as().fetch_all(&state.db.pool).await?;

    let mut count_qb =
        sqlx::QueryBuilder::new("SELECT COUNT(DISTINCT name) FROM redemptions WHERE 1=1 ");
    if let Some(name) = name_filter {
        push_redemption_group_search(&mut count_qb, name);
    }
    if let Some(code_type) = code_type_filter {
        if code_type == "balance" {
            count_qb.push(" AND (code_type = 'balance' OR code_type IS NULL OR code_type = '')");
        } else {
            count_qb.push(" AND code_type = ");
            count_qb.push_bind(code_type);
        }
    }
    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(&state.db.pool)
        .await?;

    Ok(Json(RedemptionGroupResponse {
        data: groups,
        total,
    }))
}

/// 活动列表搜索：活动名称、活动ID、备注、活动简介，或该活动下任意一张兑换码序列号。
/// 用 EXISTS 命中序列号，避免先滤掉同活动的其他码，分组统计仍是整场活动。
fn push_redemption_group_search(qb: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>, keyword: &str) {
    let like = format!("%{}%", keyword);
    qb.push(" AND (name ILIKE ");
    qb.push_bind(like.clone());
    qb.push(" OR COALESCE(activity_id, '') ILIKE ");
    qb.push_bind(like.clone());
    qb.push(" OR COALESCE(remark, '') ILIKE ");
    qb.push_bind(like.clone());
    qb.push(" OR COALESCE(description, '') ILIKE ");
    qb.push_bind(like.clone());
    qb.push(
        " OR EXISTS (SELECT 1 FROM redemptions code_hit WHERE code_hit.name = redemptions.name AND code_hit.code ILIKE ",
    );
    qb.push_bind(like);
    qb.push("))");
}

/// Admin: Update remark for an entire redemption activity group (admin-only note)
pub async fn update_redemption_group_remark(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Json(request): Json<crate::models::UpdateRedemptionGroupRemarkRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }

    let name = request.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("活动名称不能为空".to_string()));
    }
    let remark = request.remark.trim().to_string();
    if remark.chars().count() > 500 {
        return Err(AppError::BadRequest("备注说明不能超过 500 个字符".to_string()));
    }

    let updated = sqlx::query(
        &state.db.format_query(
            "UPDATE redemptions SET remark = ?, updated_at = CURRENT_TIMESTAMP WHERE name = ?",
        ),
    )
    .bind(&remark)
    .bind(name)
    .execute(&state.db.pool)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::NotFound("未找到对应的兑换活动".to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "remark": remark
    })))
}

/// Admin: Bulk generate redemption codes
pub async fn generate_redemptions(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Json(request): Json<CreateRedemptionRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }

    if request.count <= 0 || request.count > 1000 {
        return Err(AppError::BadRequest(
            "Count must be between 1 and 1000".to_string(),
        ));
    }

    let code_type = match request.code_type.as_deref().unwrap_or("balance") {
        "model_voucher" => "model_voucher",
        "playground_space" => "playground_space",
        "balance" | "" => "balance",
        _ => {
            return Err(AppError::BadRequest(
                "兑换码类型无效".to_string(),
            ))
        }
    };
    let (normalized_quota, normalized_gift_quota) =
        normalize_redemption_quotas(code_type, request.quota, request.gift_quota)
            .map_err(|message| AppError::BadRequest(message.to_string()))?;

    let tz_name = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let expires_at = if request.permanent {
        None
    } else {
        let Some(ref exp) = request.expires_at else {
            return Err(AppError::BadRequest(
                "请设置有效期，或选择长期有效".to_string(),
            ));
        };
        Some(normalize_expiry_date(exp, &tz_name)?)
    };
    let starts_at = if request.effective_immediately {
        None
    } else {
        let Some(ref raw) = request.starts_at else {
            return Err(AppError::BadRequest(
                "请设置开始生效时间，或选择生成后立即生效".to_string(),
            ));
        };
        Some(normalize_start_time(raw, &tz_name)?)
    };
    if let (Some(start), Some(end)) = (&starts_at, &expires_at) {
        if let (Some(start_at), Some(end_at)) = (start.to_utc(), end.to_utc()) {
            if start_at >= end_at {
                return Err(AppError::BadRequest(
                    "开始生效时间必须早于到期时间".to_string(),
                ));
            }
        }
    }

    // allow_multiple 只约束「单码可兑次数」；活动级单用户参与次数与之独立
    let max_uses = if request.allow_multiple {
        if request.max_uses < -1 {
            return Err(AppError::BadRequest(
                "兑换次数无效（-1 表示不限制）".to_string(),
            ));
        }
        if request.max_uses == 0 {
            -1
        } else {
            request.max_uses
        }
    } else {
        1
    };

    // 单码单用户：关闭多次兑换时固定为 1；开启时默认不限（由活动参与次数统一约束）
    let per_user_limit = if request.allow_multiple {
        if request.per_user_limit < -1 {
            return Err(AppError::BadRequest(
                "单用户兑换次数无效（-1 表示不限制）".to_string(),
            ));
        }
        if request.per_user_limit == 0 {
            -1
        } else {
            request.per_user_limit
        }
    } else {
        1
    };

    if request.per_user_activity_limit < -1 {
        return Err(AppError::BadRequest(
            "活动参与次数无效（-1 表示不限制）".to_string(),
        ));
    }
    // 约定：-1/0 = 不限；>0 为上限（与 allow_multiple / max_uses 相互独立）
    let per_user_activity_limit = if request.per_user_activity_limit <= 0 {
        -1
    } else {
        request.per_user_activity_limit
    };
    let (allowed_models, valid_days, normalized_worth_quota, voucher_discount, model_discounts_json) =
        if code_type == "model_voucher" {
            let mut models: Vec<String> = request
                .allowed_models
                .unwrap_or_default()
                .into_iter()
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .collect();
            models.sort();
            models.dedup();
            if models.is_empty() {
                return Err(AppError::BadRequest(
                    "请指定卡券可以消耗的模型".to_string(),
                ));
            }
            let known: Vec<(String, String, String, f64, i64)> = sqlx::query_as(&state.db.format_query(
                "SELECT model_id, COALESCE(mid, ''), COALESCE(model_id_alias, ''), \
                        COALESCE(site_discount, 1.0), COALESCE(site_discount_enabled, 0)::bigint \
                 FROM models \
                 WHERE model_id = ANY(?) OR mid = ANY(?) OR model_id_alias = ANY(?)",
            ))
            .bind(&models)
            .bind(&models)
            .bind(&models)
            .fetch_all(&state.db.pool)
            .await?;
            let mut accepted = std::collections::HashSet::new();
            let mut model_floors = std::collections::HashMap::new();
            for (model_id, mid, alias, site_discount, site_discount_enabled) in known {
                let floor = if site_discount_enabled != 0 && site_discount.is_finite() && site_discount > 0.0 {
                    Some(site_discount.min(1.0))
                } else {
                    None
                };
                for name in [model_id, mid, alias] {
                    let name = name.trim().to_string();
                    if !name.is_empty() {
                        accepted.insert(name.clone());
                        if let Some(f) = floor {
                            model_floors.insert(name, f);
                        }
                    }
                }
            }
            let unknown: Vec<&str> = models
                .iter()
                .filter(|name| !accepted.contains(name.as_str()))
                .map(|name| name.as_str())
                .collect();
            if !unknown.is_empty() {
                return Err(AppError::BadRequest(format!(
                    "这些模型不存在，不能生成卡券: {}",
                    unknown.join(", ")
                )));
            }
            if !request.worth_quota.is_finite() || request.worth_quota < 0.0 {
                return Err(AppError::BadRequest(
                    "额度面额价值不能小于 0".to_string(),
                ));
            }
            let worth = crate::money::round_money(request.worth_quota.max(0.0));
            let (v_disc, m_discs) = normalize_voucher_discounts(
                request.voucher_discount,
                request.model_discounts.as_ref(),
                &accepted,
                &model_floors,
            )?;
            (
                serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string()),
                validate_grant_days(request.valid_days)?,
                worth,
                v_disc,
                m_discs,
            )
        } else if code_type == "playground_space" {
            (
                "[]".to_string(),
                validate_grant_days(request.valid_days)?,
                0.0,
                1.0,
                "{}".to_string(),
            )
        } else {
            ("[]".to_string(), 0, 0.0, 1.0, "{}".to_string())
        };
    let cond_min_balance = normalize_gate_amount(request.cond_min_balance, "账户余额条件")?;
    let cond_month_spend = normalize_gate_amount(request.cond_month_spend, "当月消费条件")?;
    let cond_total_spend = normalize_gate_amount(request.cond_total_spend, "总消费条件")?;
    let cond_month_recharge = normalize_gate_amount(request.cond_month_recharge, "当月充值条件")?;
    let cond_total_recharge = normalize_gate_amount(request.cond_total_recharge, "总充值条件")?;
    let gates_on = [
        cond_min_balance,
        cond_month_spend,
        cond_total_spend,
        cond_month_recharge,
        cond_total_recharge,
    ]
    .iter()
    .filter(|amount| **amount > 0.0)
    .count();
    let condition_match = if gates_on == 0 {
        0
    } else if request.condition_match == 0 || request.condition_match == 1 {
        request.condition_match
    } else {
        return Err(AppError::BadRequest("兑换条件的成立方式无效".to_string()));
    };

    let description = request.description.trim().to_string();
    if description.chars().count() > 500 {
        return Err(AppError::BadRequest("活动简介不能超过 500 个字符".to_string()));
    }
    let remark = request.remark.trim().to_string();
    if remark.chars().count() > 500 {
        return Err(AppError::BadRequest("备注说明不能超过 500 个字符".to_string()));
    }

    let mut codes = Vec::new();
    let mut unique_codes = std::collections::HashSet::new();
    let mut tx = state.db.pool.begin().await?;
    let activity_name = request.name.trim().to_string();
    if activity_name.is_empty() {
        return Err(AppError::BadRequest("请填写活动名称".to_string()));
    }
    sqlx::query(&state.db.format_query(
        "SELECT pg_advisory_xact_lock(hashtext('redemption_activity_name'), hashtext(?))",
    ))
    .bind(&activity_name)
    .execute(&mut *tx)
    .await?;
    sqlx::query(&state.db.format_query(
        "SELECT pg_advisory_xact_lock(hashtext('redemption_activity_id_alloc'), 0)",
    ))
    .execute(&mut *tx)
    .await?;
    let name_taken: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM redemptions WHERE btrim(name) = ? LIMIT 1"),
    )
    .bind(&activity_name)
    .fetch_optional(&mut *tx)
    .await?;
    if name_taken.is_some() {
        return Err(AppError::BadRequest(
            "活动名称已存在，不能用相同名称再生成兑换码".to_string(),
        ));
    }
    let mut activity_id = String::new();
    for _ in 0..32 {
        let candidate = format!("{:04}", rand::thread_rng().gen_range(0..10000u32));
        let id_taken: Option<i64> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT id FROM redemptions WHERE activity_id = ? LIMIT 1"),
        )
        .bind(&candidate)
        .fetch_optional(&mut *tx)
        .await?;
        if id_taken.is_none() {
            activity_id = candidate;
            break;
        }
    }
    if activity_id.is_empty() {
        return Err(AppError::BadRequest("活动ID已用尽，请稍后再试".to_string()));
    }

    for _ in 0..request.count {
        // 8 位大写字母+数字，便于复制使用；冲突时重试
        for attempt in 0..16 {
            let code: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(8)
                .map(|c| (c as char).to_ascii_uppercase())
                .collect();

            if !unique_codes.contains(&code) {
                let exists: Option<i64> = sqlx::query_scalar(
                    &state
                        .db
                        .format_query("SELECT id FROM redemptions WHERE code = ? LIMIT 1"),
                )
                .bind(&code)
                .fetch_optional(&mut *tx)
                .await?;
                if exists.is_none() {
                    unique_codes.insert(code.clone());
                    codes.push(code);
                    break;
                }
            }

            if attempt == 15 {
                return Err(AppError::BadRequest("生成兑换码失败，请重试".to_string()));
            }
        }
    }

    if !codes.is_empty() {
        let mut query_builder = sqlx::QueryBuilder::new(
            "INSERT INTO redemptions (name, code, quota, gift_quota, worth_quota, voucher_discount, model_discounts, expires_at, starts_at, max_uses, used_count, per_user_limit, per_user_activity_limit, is_used, code_type, allowed_models, valid_days, activity_id, condition_match, cond_min_balance, cond_month_spend, cond_total_spend, cond_month_recharge, cond_total_recharge, description, remark) "
        );
        let q = normalized_quota;
        let gq = normalized_gift_quota;
        let wq = normalized_worth_quota;
        let vd = voucher_discount;
        let md = model_discounts_json;
        query_builder.push_values(codes.clone(), |mut b, code| {
            b.push_bind(activity_name.clone())
                .push_bind(code)
                .push_bind(q)
                .push_bind(gq)
                .push_bind(wq)
                .push_bind(vd)
                .push_bind(md.clone())
                .push_bind(expires_at.clone())
                .push_bind(starts_at.clone())
                .push_bind(max_uses)
                .push_bind(0)
                .push_bind(per_user_limit)
                .push_bind(per_user_activity_limit)
                .push_bind(0)
                .push_bind(code_type)
                .push_bind(allowed_models.clone())
                .push_bind(valid_days)
                .push_bind(activity_id.clone())
                .push_bind(condition_match)
                .push_bind(cond_min_balance)
                .push_bind(cond_month_spend)
                .push_bind(cond_total_spend)
                .push_bind(cond_month_recharge)
                .push_bind(cond_total_recharge)
                .push_bind(description.clone())
                .push_bind(remark.clone());
        });

        let query = query_builder.build();
        query.execute(&mut *tx).await?;
    }

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "count": request.count,
        "codes": codes,
        "activity_id": activity_id,
        "description": description,
        "remark": remark
    })))
}

/// Admin: Delete a redemption code
pub async fn delete_redemption(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }

    let current: Option<(String, i64, i64)> = sqlx::query_as(&state.db.format_query(
        "SELECT COALESCE(code_type, ''), used_count, is_used FROM redemptions WHERE id = ?",
    ))
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;
    let Some((code_type, used_count, is_used)) = current else {
        return Err(AppError::NotFound("Redemption code not found".to_string()));
    };
    if let Some(message) = redeemed_deletion_error(&code_type, used_count, is_used) {
        return Err(AppError::BadRequest(message.to_string()));
    }

    let deleted = sqlx::query(
        &state.db.format_query(
            "DELETE FROM redemptions WHERE id = ? AND used_count = 0 AND is_used = 0",
        ),
    )
    .bind(id)
    .execute(&state.db.pool)
    .await?
    .rows_affected();
    if deleted == 0 {
        return Err(AppError::BadRequest(
            redeemed_deletion_error(&code_type, 1, 1)
                .unwrap_or("已兑换的兑换码不能删除")
                .to_string(),
        ));
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

/// Admin: Update redemption status (void/disable)
pub async fn update_redemption_status(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
    Json(request): Json<crate::models::UpdateRedemptionStatusRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }

    if request.status != 1 && request.status != 0 && request.status != VOIDED_STATUS {
        return Err(AppError::BadRequest("Invalid status".to_string()));
    }

    let current: Option<(i64, String, i64, i64)> = sqlx::query_as(&state.db.format_query(
        "SELECT status, COALESCE(code_type, ''), used_count, is_used FROM redemptions WHERE id = ?",
    ))
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;
    let Some((status, code_type, used_count, is_used)) = current else {
        return Err(AppError::NotFound("Redemption code not found".to_string()));
    };
    if status == BALANCE_RETURNED_STATUS {
        return Err(AppError::BadRequest(
            "已退还兑换余额的兑换码不能再改状态".to_string(),
        ));
    }
    if status == VOIDED_STATUS && request.status != VOIDED_STATUS {
        return Err(AppError::BadRequest(
            "已作废的兑换码不能再改状态".to_string(),
        ));
    }
    if let Some(message) =
        balance_redeemed_status_error(&code_type, used_count, is_used, request.status)
    {
        return Err(AppError::BadRequest(message.to_string()));
    }

    let rows = sqlx::query(
        &state.db.format_query(
            "UPDATE redemptions SET status = ?, updated_at = CURRENT_TIMESTAMP \
             WHERE id = ? AND status <> ? AND (status <> ? OR ? = ?)",
        ),
    )
    .bind(request.status)
    .bind(id)
    .bind(BALANCE_RETURNED_STATUS)
    .bind(VOIDED_STATUS)
    .bind(request.status)
    .bind(VOIDED_STATUS)
    .execute(&state.db.pool)
    .await?;

    if rows.rows_affected() == 0 {
        return Err(AppError::BadRequest(
            "该兑换码已作废或已退还，不能更改状态".to_string(),
        ));
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

const VOIDED_STATUS: i64 = -1;
const BALANCE_RETURNED_STATUS: i64 = -2;

fn is_balance_code_type(code_type: &str) -> bool {
    let kind = code_type.trim();
    kind.is_empty() || kind == "balance"
}

fn is_model_voucher_code(code_type: &str) -> bool {
    code_type.trim() == "model_voucher"
}

fn is_playground_space_code(code_type: &str) -> bool {
    code_type.trim() == "playground_space"
}

#[allow(dead_code)]
fn redeemed_space_code(code_type: &str, used_count: i64, is_used: i64) -> bool {
    is_playground_space_code(code_type) && (used_count > 0 || is_used != 0)
}

#[allow(dead_code)]
fn redeemed_voucher_code(code_type: &str, used_count: i64, is_used: i64) -> bool {
    is_model_voucher_code(code_type) && (used_count > 0 || is_used != 0)
}

fn redeemed_deletion_error(code_type: &str, used_count: i64, is_used: i64) -> Option<&'static str> {
    if used_count <= 0 && is_used == 0 {
        return None;
    }
    if is_playground_space_code(code_type) {
        Some("已兑换的创作空间兑换码不能删除")
    } else if is_model_voucher_code(code_type) {
        Some("已兑换的模型卡券兑换码不能删除")
    } else {
        Some("已兑换入账的余额兑换码不能删除")
    }
}

const PLAYGROUND_SPACE_MB_MAX: f64 = 1_048_576.0;

/// 创作空间面额是整数 MB，最大 1 TiB。
fn playground_space_mb(quota: f64) -> Result<i64, &'static str> {
    if !quota.is_finite() || quota < 1.0 || quota > PLAYGROUND_SPACE_MB_MAX || quota.fract() != 0.0
    {
        return Err("存储空间必须是 1 到 1048576 的整数 MB");
    }
    Ok(quota as i64)
}

/// 校验并规整兑换码面值（返回 `(quota, gift_quota)`）。
/// 余额兑换码支持单填系统余额面值、单填赠送金余额面值或两项同时大于 0；非余额码只使用 `quota` 且要求 `> 0`。
fn normalize_redemption_quotas(
    code_type: &str,
    quota: f64,
    gift_quota: f64,
) -> Result<(f64, f64), &'static str> {
    if code_type == "playground_space" {
        let mb = playground_space_mb(quota)? as f64;
        return Ok((mb, 0.0));
    }
    if code_type == "model_voucher" {
        if !quota.is_finite() || quota <= 0.0 {
            return Err("额度必须大于 0");
        }
        let rounded = crate::money::round_money(quota);
        if rounded <= 0.0 {
            return Err("额度过小");
        }
        return Ok((rounded, 0.0));
    }
    if !quota.is_finite() || quota < 0.0 || !gift_quota.is_finite() || gift_quota < 0.0 {
        return Err("面值不能小于 0");
    }
    let sys_q = if quota == 0.0 {
        0.0
    } else {
        let rounded = crate::money::round_money(quota);
        if rounded <= 0.0 {
            return Err("系统余额面值过小");
        }
        rounded
    };
    let gift_q = if gift_quota == 0.0 {
        0.0
    } else {
        let rounded = crate::money::round_money(gift_quota);
        if rounded <= 0.0 {
            return Err("赠送金余额面值过小");
        }
        rounded
    };
    if sys_q <= 0.0 && gift_q <= 0.0 {
        return Err("系统余额面值与赠送金余额面值至少有一项大于 0");
    }
    Ok((sys_q, gift_q))
}

fn normalize_gate_amount(amount: f64, label: &str) -> AppResult<f64> {
    if !amount.is_finite() || amount < 0.0 {
        return Err(AppError::BadRequest(format!("{label}无效")));
    }
    if amount == 0.0 {
        return Ok(0.0);
    }
    let rounded = crate::money::round_money(amount);
    if rounded <= 0.0 {
        return Err(AppError::BadRequest(format!("{label}过小")));
    }
    Ok(rounded)
}

fn validate_grant_days(days: i64) -> AppResult<i64> {
    if days < 0 || days > 36500 {
        return Err(AppError::BadRequest("兑换后有效天数无效".to_string()));
    }
    Ok(days)
}

fn normalize_voucher_discounts(
    raw_discount: Option<f64>,
    raw_map: Option<&serde_json::Value>,
    accepted_models: &std::collections::HashSet<String>,
    model_floors: &std::collections::HashMap<String, f64>,
) -> AppResult<(f64, String)> {
    let default_discount = match raw_discount {
        None => 1.0,
        Some(val) => {
            if !val.is_finite() || val <= 0.0 || val > 1.0 {
                return Err(AppError::BadRequest(
                    "卡券模型折扣倍率须在 (0, 1] 之间".to_string(),
                ));
            }
            (val * 10000.0).round() / 10000.0
        }
    };
    let mut map = serde_json::Map::new();
    if let Some(value) = raw_map {
        let obj = match value {
            serde_json::Value::Object(o) => Some(o.clone()),
            serde_json::Value::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(
                        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(trimmed)
                            .map_err(|_| {
                                AppError::BadRequest("单模型独立折扣格式无效".to_string())
                            })?,
                    )
                }
            }
            serde_json::Value::Null => None,
            _ => {
                return Err(AppError::BadRequest(
                    "单模型独立折扣须为 JSON 对象".to_string(),
                ));
            }
        };
        if let Some(entries) = obj {
            for (k, v) in entries {
                let key = k.trim();
                if key.is_empty() || !accepted_models.contains(key) {
                    continue;
                }
                let Some(num) = v.as_f64() else {
                    return Err(AppError::BadRequest(format!(
                        "模型 {} 的独立折扣无效",
                        key
                    )));
                };
                if !num.is_finite() || num <= 0.0 || num > 1.0 {
                    return Err(AppError::BadRequest(format!(
                        "模型 {} 的独立折扣须在 (0, 1] 之间",
                        key
                    )));
                }
                let rounded = (num * 10000.0).round() / 10000.0;
                if let Some(&floor) = model_floors.get(key) {
                    let floor_rounded = (floor * 10000.0).round() / 10000.0;
                    if rounded + 1e-9 < floor_rounded {
                        return Err(AppError::BadRequest(format!(
                            "模型 {} 的专属折扣 ({}) 不能低于该模型的最低限价 ({})",
                            key, rounded, floor_rounded
                        )));
                    }
                }
                if let Some(n) = serde_json::Number::from_f64(rounded) {
                    map.insert(key.to_string(), serde_json::Value::Number(n));
                }
            }
        }
    }
    let json_str =
        serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_else(|_| "{}".to_string());
    Ok((default_discount, json_str))
}

pub(crate) fn parse_voucher_discount_map(raw: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw.trim())
        .map(serde_json::Value::Object)
        .unwrap_or_else(|_| serde_json::json!({}))
}

fn redeemed_balance_code(code_type: &str, used_count: i64, is_used: i64) -> bool {
    is_balance_code_type(code_type) && (used_count > 0 || is_used != 0)
}

/// 已入账的余额码仍可禁用、启用或作废（作废仅永久关闭后续兑换，不扣回已入账余额）。
fn balance_redeemed_status_error(
    code_type: &str,
    used_count: i64,
    is_used: i64,
    new_status: i64,
) -> Option<&'static str> {
    if !redeemed_balance_code(code_type, used_count, is_used) {
        return None;
    }
    match new_status {
        0 | 1 | -1 => None,
        _ => Some("已兑换入账的余额兑换码不能更改为此状态"),
    }
}

fn redemption_unusable_message(status: i64) -> String {
    match status {
        BALANCE_RETURNED_STATUS => "该兑换码已退还兑换余额".to_string(),
        -1 => "该兑换码已作废".to_string(),
        _ => "该兑换码已被禁用".to_string(),
    }
}

/// 同一用户多次兑换合并成一笔扣回。非正数和空用户跳过。按用户 ID 排序，避免并发扣款死锁。
#[cfg(test)]
fn aggregate_clawback_amounts(rows: Vec<(String, f64)>) -> Vec<(String, f64)> {
    let mut totals: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for (user_id, amount) in rows {
        if user_id.is_empty() || !amount.is_finite() || amount <= 0.0 {
            continue;
        }
        *totals.entry(user_id).or_insert(0.0) += amount;
    }
    totals
        .into_iter()
        .map(|(user_id, amount)| (user_id, crate::money::round_money(amount)))
        .collect()
}

/// 同一用户多次兑换合并成一笔扣回（分别累计系统余额与赠送金余额）。非正数和空用户跳过。按用户 ID 排序，避免并发扣款死锁。
fn aggregate_clawback_wallet_amounts(rows: Vec<(String, f64, f64)>) -> Vec<(String, f64, f64)> {
    let mut totals: std::collections::BTreeMap<String, (f64, f64)> =
        std::collections::BTreeMap::new();
    for (user_id, sys_amount, gift_amount) in rows {
        if user_id.is_empty() {
            continue;
        }
        let sys = if sys_amount.is_finite() && sys_amount > 0.0 {
            sys_amount
        } else {
            0.0
        };
        let gift = if gift_amount.is_finite() && gift_amount > 0.0 {
            gift_amount
        } else {
            0.0
        };
        if sys <= 0.0 && gift <= 0.0 {
            continue;
        }
        let entry = totals.entry(user_id).or_insert((0.0, 0.0));
        entry.0 += sys;
        entry.1 += gift;
    }
    totals
        .into_iter()
        .map(|(user_id, (sys, gift))| {
            (
                user_id,
                crate::money::round_money(sys),
                crate::money::round_money(gift),
            )
        })
        .filter(|(_, sys, gift)| *sys > 0.0 || *gift > 0.0)
        .collect()
}

#[derive(Clone, Debug, sqlx::FromRow)]
struct ClawLog {
    id: i64,
    user_id: String,
    amount: f64,
    #[sqlx(default)]
    gift_amount: f64,
    refunded: i64,
}

enum RefundTarget {
    All,
    User(String),
    Log(i64),
}

fn refund_scope_name(target: &RefundTarget) -> &'static str {
    match target {
        RefundTarget::All => "all",
        RefundTarget::User(_) => "user",
        RefundTarget::Log(_) => "log",
    }
}

/// 从已锁住的兑换日志里挑出本次要扣回的记录。已退换的跳过，避免重复扣。
fn pick_refund_logs(logs: &[ClawLog], target: &RefundTarget) -> Result<Vec<ClawLog>, &'static str> {
    let open = |row: &&ClawLog| {
        row.refunded == 0
            && !row.user_id.is_empty()
            && (row.amount > 0.0 || row.gift_amount > 0.0)
    };
    match target {
        RefundTarget::Log(id) => {
            let Some(row) = logs.iter().find(|row| row.id == *id) else {
                return Err("兑换记录不存在");
            };
            if row.refunded != 0 {
                return Err("这一次兑换已经退换");
            }
            if row.user_id.is_empty() || !(row.amount > 0.0 || row.gift_amount > 0.0) {
                return Err("这一次兑换没有可退换金额");
            }
            Ok(vec![row.clone()])
        }
        RefundTarget::User(user_id) => {
            let rows: Vec<ClawLog> = logs
                .iter()
                .filter(|row| row.user_id == *user_id && open(row))
                .cloned()
                .collect();
            if rows.is_empty() {
                if logs.iter().any(|row| row.user_id == *user_id) {
                    return Err("该用户的兑换已经退换");
                }
                return Err("该用户没有可退换的兑换记录");
            }
            Ok(rows)
        }
        RefundTarget::All => Ok(logs.iter().filter(open).cloned().collect()),
    }
}

fn space_revoke_message(message: &'static str) -> &'static str {
    match message {
        "这一次兑换已经退换" => "这一次兑换已经收回",
        "该用户的兑换已经退换" => "该用户的存储空间已经收回",
        "该用户没有可退换的兑换记录" => "该用户没有可收回的存储空间",
        "这一次兑换没有可退换金额" => "这一次兑换没有可收回的存储空间",
        _ => message,
    }
}

fn voucher_revoke_message(message: &'static str) -> &'static str {
    match message {
        "这一次兑换已经退换" => "这一次兑换的卡券已经作废",
        "该用户的兑换已经退换" => "该用户的模型卡券已经作废",
        "该用户没有可退换的兑换记录" => "该用户没有可作废的模型卡券",
        "这一次兑换没有可退换金额" => "这一次兑换没有可作废的模型卡券",
        _ => message,
    }
}

fn refund_remark(name: &str, code: &str, target: &RefundTarget) -> String {
    match target {
        RefundTarget::Log(id) => format!("退换兑换余额: {name} ({code})，单次 #{id}"),
        RefundTarget::User(_) => format!("退换兑换余额: {name} ({code})，指定用户"),
        RefundTarget::All => format!("退换兑换余额: {name} ({code})"),
    }
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct RefundBalanceBody {
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    log_id: i64,
}

/// 管理端退换兑换余额。不传范围时扣回所有未退换记录并关闭兑换码。
/// 传 user_id 只扣该用户尚未退换的次数。传 log_id 只扣其中一次。后两种不关闭兑换码。
pub async fn refund_redemption_balance(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
    body: Option<Json<RefundBalanceBody>>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }
    let scope = body.map(|Json(body)| body).unwrap_or_default();
    let target = if scope.log_id > 0 {
        RefundTarget::Log(scope.log_id)
    } else if !scope.user_id.trim().is_empty() {
        RefundTarget::User(scope.user_id.trim().to_string())
    } else {
        RefundTarget::All
    };

    let mut tx = state.db.pool.begin().await?;
    let redemption: Option<Redemption> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM redemptions WHERE id = ? FOR UPDATE"),
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(redemption) = redemption else {
        return Err(AppError::NotFound("Redemption code not found".to_string()));
    };
    if !is_balance_code_type(&redemption.code_type) {
        return Err(AppError::BadRequest(
            "只有余额兑换码可以退换兑换余额".to_string(),
        ));
    }
    if redemption.status == BALANCE_RETURNED_STATUS {
        return Err(AppError::BadRequest(
            "该兑换码已退换兑换余额".to_string(),
        ));
    }

    let logs: Vec<ClawLog> = sqlx::query_as(&state.db.format_query(
        "SELECT id, user_id, amount, COALESCE(gift_amount, 0.0) AS gift_amount, refunded FROM redemption_logs WHERE redemption_id = ? ORDER BY id ASC FOR UPDATE",
    ))
    .bind(redemption.id)
    .fetch_all(&mut *tx)
    .await?;

    let mut close_code = matches!(target, RefundTarget::All);
    let picked = if logs.is_empty() {
        match &target {
            RefundTarget::Log(_) => {
                return Err(AppError::BadRequest(
                    "缺少兑换记录，无法只退换其中一次".to_string(),
                ));
            }
            RefundTarget::User(user_id) => {
                if redemption.used_count > 1 {
                    return Err(AppError::BadRequest(
                        "缺少兑换记录，无法按用户退换兑换余额".to_string(),
                    ));
                }
                if redemption.used_by.as_deref() != Some(user_id.as_str()) {
                    return Err(AppError::BadRequest(
                        "该用户没有可退换的兑换记录".to_string(),
                    ));
                }
                close_code = true;
            }
            RefundTarget::All => {
                if redemption.used_count > 1 {
                    return Err(AppError::BadRequest(
                        "缺少兑换记录，无法按用户退换兑换余额".to_string(),
                    ));
                }
            }
        }
        Vec::new()
    } else {
        pick_refund_logs(&logs, &target).map_err(|message| AppError::BadRequest(message.to_string()))?
    };

    let mut lines = aggregate_clawback_wallet_amounts(
        picked
            .iter()
            .map(|row| (row.user_id.clone(), row.amount, row.gift_amount))
            .collect(),
    );
    if lines.is_empty() && logs.is_empty() {
        let redeemed = redemption.used_count > 0 || redemption.is_used != 0;
        let user_id = redemption.used_by.clone().unwrap_or_default();
        if !redeemed
            || user_id.is_empty()
            || !(redemption.quota > 0.0 || redemption.gift_quota > 0.0)
        {
            return Err(AppError::BadRequest(
                "该兑换码尚未兑换入账，无需退换兑换余额".to_string(),
            ));
        }
        lines.push((
            user_id,
            crate::money::round_money(redemption.quota.max(0.0)),
            crate::money::round_money(redemption.gift_quota.max(0.0)),
        ));
    }
    if lines.is_empty() && !close_code {
        return Err(AppError::BadRequest(
            "没有可退换的兑换记录".to_string(),
        ));
    }

    let remark = refund_remark(&redemption.name, &redemption.code, &target);
    let mut total_sys = 0.0;
    let mut total_gift = 0.0;
    for (user_id, sys_amount, gift_amount) in &lines {
        let updated = sqlx::query(&state.db.format_query(
            "UPDATE users SET balance = balance - ?, gift_balance = gift_balance - ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ))
        .bind(*sys_amount)
        .bind(*gift_amount)
        .bind(user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(AppError::BadRequest(
                "兑换用户不存在，无法退换兑换余额".to_string(),
            ));
        }
        if *sys_amount > 0.0 {
            sqlx::query(&state.db.format_query(
                "INSERT INTO recharge_records (user_id, amount, recharge_type, remark, operator, wallet_type) VALUES (?, ?, 'redemption_refund', ?, ?, 'system')",
            ))
            .bind(user_id)
            .bind(-*sys_amount)
            .bind(&remark)
            .bind(&claims.username)
            .execute(&mut *tx)
            .await?;
            crate::services::affiliate::note_redemption_debit(
                &state.db,
                &mut tx,
                user_id,
                *sys_amount,
            )
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        }
        if *gift_amount > 0.0 {
            let gift_order_no = crate::api::plugins::finance::generate_gift_order_no();
            sqlx::query(&state.db.format_query(
                "INSERT INTO recharge_records (user_id, amount, recharge_type, remark, operator, wallet_type, order_no) VALUES (?, ?, 'redemption_refund', ?, ?, 'gift', ?)",
            ))
            .bind(user_id)
            .bind(-*gift_amount)
            .bind(&remark)
            .bind(&claims.username)
            .bind(&gift_order_no)
            .execute(&mut *tx)
            .await?;
        }
        total_sys += *sys_amount;
        total_gift += *gift_amount;
    }

    if !picked.is_empty() {
        let ids: Vec<i64> = picked.iter().map(|row| row.id).collect();
        let marked = sqlx::query(&state.db.format_query(
            "UPDATE redemption_logs SET refunded = 1 WHERE redemption_id = ? AND refunded = 0 AND id = ANY(?)",
        ))
        .bind(redemption.id)
        .bind(&ids)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if marked != ids.len() as u64 {
            return Err(AppError::BadRequest("这一次兑换已经退换".to_string()));
        }
    }

    if close_code {
        let closed = sqlx::query(&state.db.format_query(
            "UPDATE redemptions SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status <> ?",
        ))
        .bind(BALANCE_RETURNED_STATUS)
        .bind(redemption.id)
        .bind(BALANCE_RETURNED_STATUS)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if closed == 0 {
            return Err(AppError::BadRequest(
                "该兑换码已退换兑换余额".to_string(),
            ));
        }
    }

    tx.commit().await?;

    for (user_id, _, _) in &lines {
        crate::services::notification::spawn_low_balance_check(Arc::clone(&state), user_id.clone());
    }

    let total = crate::money::round_money(total_sys + total_gift);
    Ok(Json(serde_json::json!({
        "success": true,
        "amount": total,
        "system_amount": crate::money::round_money(total_sys),
        "gift_amount": crate::money::round_money(total_gift),
        "users": lines.len(),
        "scope": refund_scope_name(&target),
        "closed": close_code
    })))
}

/// 管理端收回创作空间。不传范围时收回全部未收回记录并作废兑换码。
/// 传 user_id 只收回该用户尚未收回的次数。传 log_id 只收回其中一次。后两种不关闭兑换码。
/// 已用次数不退回。
pub async fn revoke_playground_space(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
    body: Option<Json<RefundBalanceBody>>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }
    let scope = body.map(|Json(body)| body).unwrap_or_default();
    let target = if scope.log_id > 0 {
        RefundTarget::Log(scope.log_id)
    } else if !scope.user_id.trim().is_empty() {
        RefundTarget::User(scope.user_id.trim().to_string())
    } else {
        RefundTarget::All
    };

    let mut tx = state.db.pool.begin().await?;
    let redemption: Option<Redemption> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM redemptions WHERE id = ? FOR UPDATE"),
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(redemption) = redemption else {
        return Err(AppError::NotFound("Redemption code not found".to_string()));
    };
    if !is_playground_space_code(&redemption.code_type) {
        return Err(AppError::BadRequest(
            "只有创作空间兑换码可以收回存储空间".to_string(),
        ));
    }
    let (space_mb, closed) =
        revoke_playground_space_in_tx(&state, &mut tx, &redemption, &target).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "space_mb": space_mb,
        "scope": refund_scope_name(&target),
        "closed": closed
    })))
}

async fn revoke_playground_space_in_tx(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    redemption: &Redemption,
    target: &RefundTarget,
) -> AppResult<(i64, bool)> {
    let close_code = matches!(target, RefundTarget::All);
    let logs: Vec<ClawLog> = sqlx::query_as(&state.db.format_query(
        "SELECT id, user_id, amount, refunded FROM redemption_logs WHERE redemption_id = ? ORDER BY id ASC FOR UPDATE",
    ))
    .bind(redemption.id)
    .fetch_all(&mut **tx)
    .await?;

    let picked = if logs.is_empty() {
        match target {
            RefundTarget::Log(_) => {
                return Err(AppError::BadRequest(
                    "缺少兑换记录，无法只收回其中一次".to_string(),
                ));
            }
            RefundTarget::User(_) => {
                return Err(AppError::BadRequest(
                    "该用户没有可收回的存储空间".to_string(),
                ));
            }
            RefundTarget::All => Vec::new(),
        }
    } else {
        pick_refund_logs(&logs, target)
            .map_err(|message| AppError::BadRequest(space_revoke_message(message).to_string()))?
    };
    if picked.is_empty() && !close_code {
        return Err(AppError::BadRequest(
            "没有可收回的存储空间".to_string(),
        ));
    }

    let mut space_mb: i64 = 0;
    if !picked.is_empty() {
        let ids: Vec<i64> = picked.iter().map(|row| row.id).collect();
        for row in &picked {
            space_mb = space_mb.saturating_add(playground_space_mb(row.amount).unwrap_or(0));
        }
        sqlx::query(&state.db.format_query(
            "UPDATE playground_2026_space_grants SET status = 0 \
             WHERE status = 1 AND redemption_log_id = ANY(?)",
        ))
        .bind(&ids)
        .execute(&mut **tx)
        .await?;
        let marked = sqlx::query(&state.db.format_query(
            "UPDATE redemption_logs SET refunded = 1 WHERE redemption_id = ? AND refunded = 0 AND id = ANY(?)",
        ))
        .bind(redemption.id)
        .bind(&ids)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        if marked != ids.len() as u64 {
            return Err(AppError::BadRequest("这一次兑换已经收回".to_string()));
        }
    }

    if close_code {
        sqlx::query(&state.db.format_query(
            "UPDATE redemptions SET status = -1, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status <> ?",
        ))
        .bind(redemption.id)
        .bind(BALANCE_RETURNED_STATUS)
        .execute(&mut **tx)
        .await?;
    }

    Ok((space_mb, close_code))
}

/// 管理端作废已兑出的模型卡券。不传范围时作废该兑换码下全部未作废卡券并作废兑换码本身。
/// 传 user_id 只作废该用户尚未作废的卡券。传 log_id 只作废单次核销对应的卡券。后两种不关闭兑换码。
pub async fn revoke_model_voucher(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
    body: Option<Json<RefundBalanceBody>>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }
    let scope = body.map(|Json(body)| body).unwrap_or_default();
    let target = if scope.log_id > 0 {
        RefundTarget::Log(scope.log_id)
    } else if !scope.user_id.trim().is_empty() {
        RefundTarget::User(scope.user_id.trim().to_string())
    } else {
        RefundTarget::All
    };

    let mut tx = state.db.pool.begin().await?;
    let redemption: Option<Redemption> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM redemptions WHERE id = ? FOR UPDATE"),
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(redemption) = redemption else {
        return Err(AppError::NotFound("Redemption code not found".to_string()));
    };
    if !is_model_voucher_code(&redemption.code_type) {
        return Err(AppError::BadRequest(
            "只有模型卡券兑换码可以作废已兑卡券".to_string(),
        ));
    }
    let (vouchers, closed) =
        revoke_model_voucher_in_tx(&state, &mut tx, &redemption, &target).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "vouchers": vouchers,
        "scope": refund_scope_name(&target),
        "closed": closed
    })))
}

async fn revoke_model_voucher_in_tx(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    redemption: &Redemption,
    target: &RefundTarget,
) -> AppResult<(usize, bool)> {
    let close_code = matches!(target, RefundTarget::All);
    let logs: Vec<ClawLog> = sqlx::query_as(&state.db.format_query(
        "SELECT id, user_id, amount, refunded FROM redemption_logs WHERE redemption_id = ? ORDER BY id ASC FOR UPDATE",
    ))
    .bind(redemption.id)
    .fetch_all(&mut **tx)
    .await?;

    let picked = if logs.is_empty() {
        match target {
            RefundTarget::Log(_) => {
                return Err(AppError::BadRequest(
                    "缺少兑换记录，无法只作废其中一次".to_string(),
                ));
            }
            RefundTarget::User(_) => {
                return Err(AppError::BadRequest(
                    "该用户没有可作废的模型卡券".to_string(),
                ));
            }
            RefundTarget::All => Vec::new(),
        }
    } else {
        pick_refund_logs(&logs, target)
            .map_err(|message| AppError::BadRequest(voucher_revoke_message(message).to_string()))?
    };
    if picked.is_empty() && !close_code {
        return Err(AppError::BadRequest(
            "没有可作废的模型卡券".to_string(),
        ));
    }

    if !picked.is_empty() {
        let ids: Vec<i64> = picked.iter().map(|row| row.id).collect();
        sqlx::query(&state.db.format_query(
            "UPDATE user_model_vouchers SET remaining = 0, status = 0, updated_at = CURRENT_TIMESTAMP \
             WHERE status = 1 AND redemption_log_id = ANY(?)",
        ))
        .bind(&ids)
        .execute(&mut **tx)
        .await?;

        match target {
            RefundTarget::All => {
                sqlx::query(&state.db.format_query(
                    "UPDATE user_model_vouchers SET remaining = 0, status = 0, updated_at = CURRENT_TIMESTAMP \
                     WHERE status = 1 AND redemption_id = ? AND redemption_log_id IS NULL",
                ))
                .bind(redemption.id)
                .execute(&mut **tx)
                .await?;
            }
            RefundTarget::User(user_id) => {
                sqlx::query(&state.db.format_query(
                    "UPDATE user_model_vouchers SET remaining = 0, status = 0, updated_at = CURRENT_TIMESTAMP \
                     WHERE status = 1 AND redemption_id = ? AND user_id = ? AND redemption_log_id IS NULL",
                ))
                .bind(redemption.id)
                .bind(user_id)
                .execute(&mut **tx)
                .await?;
            }
            RefundTarget::Log(_) => {}
        }

        let marked = sqlx::query(&state.db.format_query(
            "UPDATE redemption_logs SET refunded = 1 WHERE redemption_id = ? AND refunded = 0 AND id = ANY(?)",
        ))
        .bind(redemption.id)
        .bind(&ids)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        if marked != ids.len() as u64 {
            return Err(AppError::BadRequest("这一次兑换的卡券已经作废".to_string()));
        }
    }

    if close_code {
        sqlx::query(&state.db.format_query(
            "UPDATE redemptions SET status = -1, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status <> ?",
        ))
        .bind(redemption.id)
        .bind(BALANCE_RETURNED_STATUS)
        .execute(&mut **tx)
        .await?;
    }

    Ok((picked.len(), close_code))
}

#[derive(serde::Deserialize)]
pub struct DeleteGroupQuery {
    pub name: String,
}

pub async fn delete_redemption_group(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Query(query): Query<DeleteGroupQuery>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }

    if query.name.trim().is_empty() {
        return Err(AppError::BadRequest("Activity name required".to_string()));
    }

    let removed = sqlx::query(
        &state
            .db
            .format_query("DELETE FROM redemptions WHERE name = ? AND used_count = 0 AND is_used = 0"),
    )
    .bind(&query.name)
    .execute(&state.db.pool)
    .await?
    .rows_affected();

    if removed == 0 {
        let redeemed_row: Option<(String, i64, i64)> = sqlx::query_as(&state.db.format_query(
            "SELECT code_type, used_count, is_used FROM redemptions \
             WHERE name = ? AND (used_count > 0 OR is_used <> 0) LIMIT 1",
        ))
        .bind(&query.name)
        .fetch_optional(&state.db.pool)
        .await?;
        if let Some((code_type, used_count, is_used)) = redeemed_row {
            if let Some(err) = redeemed_deletion_error(&code_type, used_count, is_used) {
                return Err(AppError::BadRequest(err.to_string()));
            }
        }
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

/// 将有效期规范为落库的 DbTs（按站点时区计算过期瞬间的 UTC 时间）
fn normalize_expiry_date(raw: &str, tz_name: &str) -> AppResult<crate::time_system::DbTs> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(
            "请设置有效期，或选择长期有效".to_string(),
        ));
    }
    let tz: chrono_tz::Tz = tz_name.parse().unwrap_or(chrono_tz::Asia::Shanghai);

    let mut dt_opt = None;

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        dt_opt = Some(dt.with_timezone(&chrono::Utc));
    } else if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
        if let Some(local) = ndt.and_local_timezone(tz).latest() {
            dt_opt = Some(local.with_timezone(&chrono::Utc));
        }
    } else if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        if let Some(local) = ndt.and_local_timezone(tz).latest() {
            dt_opt = Some(local.with_timezone(&chrono::Utc));
        }
    } else if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
        if let Some(local) = ndt.and_local_timezone(tz).latest() {
            dt_opt = Some(local.with_timezone(&chrono::Utc));
        }
    } else if let Ok(d) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        if let Some(next) = d.succ_opt() {
            if let Some(end) = next
                .and_hms_opt(0, 0, 0)
                .and_then(|ndt| ndt.and_local_timezone(tz).latest())
            {
                dt_opt = Some(end.with_timezone(&chrono::Utc));
            }
        }
    }

    if let Some(dt) = dt_opt {
        Ok(crate::time_system::DbTs::from_utc(dt))
    } else {
        Err(AppError::BadRequest("有效期格式不正确".to_string()))
    }
}

/// 开始生效时间：无时区字符串按站点默认时区墙钟解释；纯日期从该日 00:00 起生效。
fn start_instant(raw: &str, tz: chrono_tz::Tz) -> Option<chrono::DateTime<chrono_tz::Tz>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&tz));
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
        return ndt.and_local_timezone(tz).latest();
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return ndt.and_local_timezone(tz).latest();
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
        return ndt.and_local_timezone(tz).latest();
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return d
            .and_hms_opt(0, 0, 0)
            .and_then(|ndt| ndt.and_local_timezone(tz).latest());
    }
    None
}

fn normalize_start_time(raw: &str, tz_name: &str) -> AppResult<crate::time_system::DbTs> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(
            "请设置开始生效时间，或选择生成后立即生效".to_string(),
        ));
    }
    let tz: chrono_tz::Tz = tz_name.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let Some(local) = start_instant(trimmed, tz) else {
        return Err(AppError::BadRequest("开始生效时间格式不正确".to_string()));
    };
    Ok(crate::time_system::DbTs::from_utc(local.with_timezone(&chrono::Utc)))
}

/// 未到开始生效时间。空值表示生成后立即生效。
fn is_not_started(starts_at: Option<&str>, tz_name: &str) -> bool {
    let Some(raw) = starts_at.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let tz: chrono_tz::Tz = tz_name.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let Some(start) = start_instant(raw, tz) else {
        return false;
    };
    chrono::Utc::now().with_timezone(&tz) < start
}

/// 过期判定：无时区字符串按站点默认时区墙钟解释；纯日期则该日结束（次日 00:00）前仍有效。
fn is_expired(expires_at: Option<&str>, tz_name: &str) -> bool {
    let Some(exp) = expires_at.map(str::trim).filter(|s| !s.is_empty()) else {
        return false; // 长期有效
    };
    let tz: chrono_tz::Tz = tz_name.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let now = chrono::Utc::now().with_timezone(&tz);

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(exp) {
        return dt.with_timezone(&tz) < now;
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(exp, "%Y-%m-%d %H:%M:%S") {
        if let Some(local) = ndt.and_local_timezone(tz).latest() {
            return local < now;
        }
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(exp, "%Y-%m-%dT%H:%M:%S") {
        if let Some(local) = ndt.and_local_timezone(tz).latest() {
            return local < now;
        }
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(exp, "%Y-%m-%d %H:%M") {
        if let Some(local) = ndt.and_local_timezone(tz).latest() {
            return local < now;
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(exp, "%Y-%m-%d") {
        // 日期当天结束仍有效：站点时区下「过期日 + 1 天 00:00」起算过期
        if let Some(next) = d.succ_opt() {
            if let Some(end) = next
                .and_hms_opt(0, 0, 0)
                .and_then(|ndt| ndt.and_local_timezone(tz).latest())
            {
                return now >= end;
            }
        }
    }
    false
}

/// User: Redeem a code to balance
struct EligibilityGate {
    label: &'static str,
    required: f64,
    current: f64,
}

fn gate_met(required: f64, current: f64) -> bool {
    crate::money::round_money(current) + 1e-9 >= crate::money::round_money(required)
}

/// 已启用的门槛。金额为 0 的项不参与。match_any 为真时命中任意一项即可。
fn eligibility_failure(match_any: bool, gates: &[EligibilityGate]) -> Option<String> {
    let active: Vec<&EligibilityGate> = gates.iter().filter(|gate| gate.required > 0.0).collect();
    if active.is_empty() {
        return None;
    }
    let passed = if match_any {
        active.iter().any(|gate| gate_met(gate.required, gate.current))
    } else {
        active.iter().all(|gate| gate_met(gate.required, gate.current))
    };
    if passed {
        return None;
    }
    let detail = active
        .iter()
        .map(|gate| {
            format!(
                "{}需达到 {}，当前 {}",
                gate.label,
                crate::money::format_clean_money(gate.required),
                crate::money::format_clean_money(gate.current)
            )
        })
        .collect::<Vec<_>>()
        .join("；");
    let lead = if match_any {
        "未满足兑换条件，下列至少要达到一项"
    } else {
        "未满足兑换条件，下列需要全部达到"
    };
    Some(format!("{lead}：{detail}"))
}

async fn settled_spend(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    start_day: &str,
    end_day: &str,
) -> AppResult<f64> {
    let tz = crate::api::date_helper::resolve_site_operation_timezone(&state.db).await;
    let slices = crate::api::date_helper::calculate_query_slices(Some(start_day), Some(end_day), tz);
    let mut total = 0.0;
    if slices.has_history_days {
        let sql = format!(
            "SELECT COALESCE(SUM(total_cost), 0.0) FROM usage_daily_stats WHERE user_id = ? AND {}",
            slices.history_cond("stat_date")
        );
        let amount: f64 = sqlx::query_scalar(&state.db.format_query(&sql))
            .bind(user_id)
            .bind(slices.hist_start_date)
            .bind(slices.hist_end_date)
            .fetch_one(&mut **tx)
            .await?;
        total += amount;
    }
    for slice in slices.realtime_slices() {
        let sql = format!(
            "SELECT COALESCE(SUM(cost), 0.0) FROM logs WHERE user_id = ? AND {}",
            slice.sql_cond("created_at")
        );
        let amount: f64 = sqlx::query_scalar(&state.db.format_query(&sql))
            .bind(user_id)
            .bind(&slice.start)
            .bind(&slice.end)
            .fetch_one(&mut **tx)
            .await?;
        total += amount;
    }
    Ok(total)
}

async fn cash_recharge(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    start_ts: Option<&str>,
    end_ts: Option<&str>,
) -> AppResult<f64> {
    let mut sql = String::from(
        "SELECT COALESCE(SUM(amount), 0.0) FROM recharge_records \
         WHERE user_id = ? AND amount > 0 \
           AND ( \
             (recharge_type = 'manual' AND COALESCE(NULLIF(wallet_type, ''), 'system') = 'system') \
             OR ( \
               btrim(COALESCE(order_no, '')) <> '' \
               AND recharge_type NOT IN ( \
                 'redemption', 'redemption_refund', 'model_voucher', \
                 'gift', 'registration', 'commission', 'transfer', \
                 'coupon_purchase', 'coupon_pack', 'coupon_subscription', \
                 'ark_video_consume', 'ark_video_refund' \
               ) \
             ) \
           )",
    );
    if start_ts.is_some() {
        sql.push_str(" AND created_at >= ?::timestamptz");
    }
    if end_ts.is_some() {
        sql.push_str(" AND created_at < ?::timestamptz");
    }
    let formatted = state.db.format_query(&sql);
    let mut query = sqlx::query_scalar::<_, f64>(&formatted).bind(user_id);
    if let Some(start_ts) = start_ts {
        query = query.bind(start_ts);
    }
    if let Some(end_ts) = end_ts {
        query = query.bind(end_ts);
    }
    Ok(query.fetch_one(&mut **tx).await?)
}

fn site_month_bounds(tz: chrono_tz::Tz) -> (String, String, String, String) {
    let now = chrono::Utc::now().with_timezone(&tz);
    let today = now.date_naive();
    let month_start = chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
    let next_month = if today.month() == 12 {
        chrono::NaiveDate::from_ymd_opt(today.year() + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1)
    }
    .unwrap_or(month_start);
    let offset = now.format("%z").to_string();
    (
        month_start.format("%Y-%m-%d").to_string(),
        today.format("%Y-%m-%d").to_string(),
        format!("{} 00:00:00{}", month_start.format("%Y-%m-%d"), offset),
        format!("{} 00:00:00{}", next_month.format("%Y-%m-%d"), offset),
    )
}

async fn ensure_redemption_eligibility(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    redemption: &Redemption,
) -> AppResult<()> {
    let required = [
        ("账户余额", redemption.cond_min_balance),
        ("当月消费", redemption.cond_month_spend),
        ("总消费", redemption.cond_total_spend),
        ("当月充值", redemption.cond_month_recharge),
        ("总充值", redemption.cond_total_recharge),
    ];
    if required.iter().all(|(_, amount)| *amount <= 0.0) {
        return Ok(());
    }
    let tz_name = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    let tz: chrono_tz::Tz = tz_name.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let (month_start_day, today, month_start_ts, next_month_ts) = site_month_bounds(tz);
    let balance: f64 = if redemption.cond_min_balance > 0.0 {
        sqlx::query_scalar(&state.db.format_query(
            "SELECT COALESCE(balance, 0.0) FROM users WHERE id = ?",
        ))
        .bind(user_id)
        .fetch_one(&mut **tx)
        .await?
    } else {
        0.0
    };
    let month_spend = if redemption.cond_month_spend > 0.0 {
        settled_spend(state, tx, user_id, &month_start_day, &today).await?
    } else {
        0.0
    };
    let total_spend = if redemption.cond_total_spend > 0.0 {
        settled_spend(state, tx, user_id, "2000-01-01", &today).await?
    } else {
        0.0
    };
    let month_recharge = if redemption.cond_month_recharge > 0.0 {
        cash_recharge(state, tx, user_id, Some(&month_start_ts), Some(&next_month_ts)).await?
    } else {
        0.0
    };
    let total_recharge = if redemption.cond_total_recharge > 0.0 {
        cash_recharge(state, tx, user_id, None, None).await?
    } else {
        0.0
    };
    let gates = [
        EligibilityGate { label: "账户余额", required: redemption.cond_min_balance, current: balance },
        EligibilityGate { label: "当月消费", required: redemption.cond_month_spend, current: month_spend },
        EligibilityGate { label: "总消费", required: redemption.cond_total_spend, current: total_spend },
        EligibilityGate { label: "当月充值", required: redemption.cond_month_recharge, current: month_recharge },
        EligibilityGate { label: "总充值", required: redemption.cond_total_recharge, current: total_recharge },
    ];
    if let Some(message) = eligibility_failure(redemption.condition_match == 1, &gates) {
        return Err(AppError::BadRequest(message));
    }
    Ok(())
}

pub async fn redeem_code(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<RedeemRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = claims.sub;
    let client_ip = crate::api::auth::extract_client_ip(&headers, &addr);

    // 0) IP 防刷：1 分钟超 20 次 → 封禁该 IP 24 小时（优先于用户限流与查库）
    if let Err(msg) = state.rate_limiter.check_redeem_ip(&client_ip) {
        return Err(AppError::Forbidden(msg));
    }

    // 1) 限流放最前：未过限流前不碰库（防刷）
    //    每用户 5 次/分钟；顺带清理过期条目，避免 DashMap 无限增长
    static REDEEM_RL: OnceLock<DashMap<String, (u32, Instant)>> = OnceLock::new();
    let rl = REDEEM_RL.get_or_init(|| DashMap::new());
    let now = Instant::now();
    if rl.len() > 10_000 {
        rl.retain(|_, (_, last_reset)| now.duration_since(*last_reset) <= Duration::from_secs(120));
    }
    let mut allowed = true;
    if let Some(mut entry) = rl.get_mut(&user_id) {
        let (count, last_reset) = *entry;
        if now.duration_since(last_reset) > Duration::from_secs(60) {
            *entry = (1, now);
        } else if count >= 5 {
            allowed = false;
        } else {
            *entry = (count + 1, last_reset);
        }
    } else {
        rl.insert(user_id.clone(), (1, now));
    }
    if !allowed {
        return Err(AppError::BadRequest("请求过于频繁，请稍后再试".to_string()));
    }

    redeem_for_user(state, user_id, request.code).await
}

/// 用户自兑和后台代兑共用。不含用户侧频率限制，兑换码仍只能成功核销一次。
pub async fn redeem_for_user(
    state: Arc<AppState>,
    user_id: String,
    raw_code: String,
) -> AppResult<Json<serde_json::Value>> {
    let code = raw_code.trim().to_ascii_uppercase();
    if code.is_empty() {
        return Err(AppError::BadRequest("请输入兑换码".to_string()));
    }
    if code.len() > 32 {
        return Err(AppError::BadRequest("兑换码无效，请检查后重试".to_string()));
    }

    #[cfg(all(feature = "commercial_plugins", plugin_marketing_games))]
    if code.starts_with("MG-") {
        return crate::api::plugins::marketing_games::claim_prize_by_code(&state, &user_id, &code)
            .await;
    }

    // 3) 只读 marketing 开关（避免 load_all_settings 每次 ~20 次查库）
    let marketing_raw: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = 'marketing_settings'"),
    )
    .fetch_optional(&state.db.pool)
    .await?;
    let enable_redemption = marketing_raw
        .and_then(|v| {
            serde_json::from_str::<crate::models::MarketingSettings>(&v)
                .ok()
                .map(|m| m.enable_redemption)
        })
        .unwrap_or_else(|| crate::api::settings::default_marketing_settings().enable_redemption);
    if !enable_redemption {
        return Err(AppError::Forbidden("兑换功能未开启".to_string()));
    }

    // 4) 事务外预检：无效/禁用/过期/用尽快速失败，不占用连接做写事务
    let preview: Option<Redemption> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM redemptions WHERE code = ? LIMIT 1"),
    )
    .bind(&code)
    .fetch_optional(&state.db.pool)
    .await?;

    let preview = match preview {
        None => {
            return Err(AppError::BadRequest("兑换码无效，请检查后重试".to_string()));
        }
        Some(r) => r,
    };

    if preview.status != 1 {
        return Err(AppError::BadRequest(redemption_unusable_message(
            preview.status,
        )));
    }

    let tz_name = crate::relay::relay_settings::get_cached_site_timezone(&state.db).await;
    if is_not_started(preview.starts_at.as_deref(), &tz_name) {
        return Err(AppError::BadRequest("兑换码尚未生效".to_string()));
    }
    if is_expired(preview.expires_at.as_deref(), &tz_name) {
        return Err(AppError::BadRequest("兑换码已过期".to_string()));
    }

    let max_uses_preview = preview.max_uses;
    if max_uses_preview > 0 && preview.used_count >= max_uses_preview {
        return Err(AppError::BadRequest("该兑换码兑换次数已用完".to_string()));
    }
    if max_uses_preview == 1 && preview.is_used != 0 && preview.used_count == 0 {
        return Err(AppError::BadRequest("该兑换码已被使用".to_string()));
    }

    // 5) 写事务：行锁 + 活动顾问锁 + 乐观更新，防超发 / 防并发突破活动上限
    let mut tx = state.db.pool.begin().await?;

    let redemption: Redemption = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM redemptions WHERE id = ? FOR UPDATE"),
    )
    .bind(preview.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::BadRequest("兑换码无效，请检查后重试".to_string()))?;

    if redemption.status != 1 {
        return Err(AppError::BadRequest(redemption_unusable_message(
            redemption.status,
        )));
    }
    if is_not_started(redemption.starts_at.as_deref(), &tz_name) {
        return Err(AppError::BadRequest("兑换码尚未生效".to_string()));
    }
    if is_expired(redemption.expires_at.as_deref(), &tz_name) {
        return Err(AppError::BadRequest("兑换码已过期".to_string()));
    }

    let max_uses = redemption.max_uses;
    if max_uses > 0 && redemption.used_count >= max_uses {
        return Err(AppError::BadRequest("该兑换码兑换次数已用完".to_string()));
    }

    // 单兑换码单用户限制：<=0（-1 约定 / 历史 0）表示不限
    let per_user_limit = redemption.per_user_limit;
    if per_user_limit > 0 {
        // 单用户并发互斥锁：防止同用户多线程并发突破限领次数双重入账
        sqlx::query(
            &state
                .db
                .format_query("SELECT pg_advisory_xact_lock(hashtext(?), hashtext(?))"),
        )
        .bind(format!("redemption_code:{}", redemption.id))
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;

        let user_used: i64 = sqlx::query_scalar(&state.db.format_query(
            "SELECT COUNT(*) FROM redemption_logs WHERE redemption_id = ? AND user_id = ?",
        ))
        .bind(redemption.id)
        .bind(&user_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap_or(0);

        let legacy_used = redemption.used_by.as_deref() == Some(user_id.as_str())
            && user_used == 0
            && redemption.is_used != 0
            && max_uses == 1;

        if legacy_used || user_used >= per_user_limit {
            return Err(AppError::BadRequest(
                "您已达到该兑换码的兑换次数上限".to_string(),
            ));
        }
    }

    // 活动级单用户参与次数
    let per_user_activity_limit = redemption.per_user_activity_limit;
    if per_user_activity_limit > 0 {
        sqlx::query(
            &state
                .db
                .format_query("SELECT pg_advisory_xact_lock(hashtext(?), hashtext(?))"),
        )
        .bind(format!("redemption_activity:{}", redemption.name))
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;

        // 先按 user_id 过滤，配合 idx_redemption_logs_user_id，避免大活动全表扫 logs
        let activity_used: i64 = sqlx::query_scalar(&state.db.format_query(
            "SELECT COUNT(*) FROM redemption_logs \
             WHERE user_id = ? \
               AND redemption_id IN (SELECT id FROM redemptions WHERE name = ?)",
        ))
        .bind(&user_id)
        .bind(&redemption.name)
        .fetch_one(&mut *tx)
        .await
        .unwrap_or(0);

        if activity_used >= per_user_activity_limit {
            return Err(AppError::BadRequest(
                "您已达到该活动的参与次数上限".to_string(),
            ));
        }
    }

    ensure_redemption_eligibility(&state, &mut tx, &user_id, &redemption).await?;

    // 乐观锁：要求 status=1，防止预检后被禁用仍兑入账
    let rows_affected = sqlx::query(&state.db.format_query(
        "UPDATE redemptions SET \
         used_count = used_count + 1, \
         is_used = CASE WHEN ? > 0 AND used_count + 1 >= ? THEN 1 ELSE is_used END, \
         used_at = CURRENT_TIMESTAMP, \
         used_by = CASE WHEN ? = 1 THEN ? ELSE used_by END, \
         updated_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND status = 1 AND (? <= 0 OR used_count < ?)",
    ))
    .bind(max_uses)
    .bind(max_uses)
    .bind(max_uses)
    .bind(&user_id)
    .bind(redemption.id)
    .bind(max_uses)
    .bind(max_uses)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(AppError::BadRequest(
            "该兑换码兑换次数已用完，或已被他人抢先兑换".to_string(),
        ));
    }

    let code_kind = match redemption.code_type.trim() {
        "model_voucher" => "model_voucher",
        "playground_space" => "playground_space",
        "balance" | "" => "balance",
        _ => {
            return Err(AppError::BadRequest("兑换码类型无效".to_string()));
        }
    };
    let space_mb = if code_kind == "playground_space" {
        Some(
            playground_space_mb(redemption.quota)
                .map_err(|message| AppError::BadRequest(message.to_string()))?,
        )
    } else {
        None
    };

    if code_kind == "balance" {
        let sys_quota = crate::money::round_money(redemption.quota.max(0.0));
        let gift_quota = crate::money::round_money(redemption.gift_quota.max(0.0));
        if sys_quota <= 0.0 && gift_quota <= 0.0 {
            return Err(AppError::BadRequest("兑换码面值无效".to_string()));
        }
        let updated_user = sqlx::query(&state.db.format_query(
            "UPDATE users SET balance = balance + ?, gift_balance = gift_balance + ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ))
        .bind(sys_quota)
        .bind(gift_quota)
        .bind(&user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated_user == 0 {
            return Err(AppError::BadRequest("兑换用户不存在".to_string()));
        }
    } else {
        let user_exists: Option<i64> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT 1::bigint FROM users WHERE id = ?"),
        )
        .bind(&user_id)
        .fetch_optional(&mut *tx)
        .await?;
        if user_exists.is_none() {
            return Err(AppError::BadRequest("兑换用户不存在".to_string()));
        }
    }

    let log_gift_amount = if code_kind == "balance" {
        crate::money::round_money(redemption.gift_quota.max(0.0))
    } else {
        0.0
    };
    let log_id: i64 = sqlx::query_scalar(&state.db.format_query(
        "INSERT INTO redemption_logs (redemption_id, user_id, amount, gift_amount) VALUES (?, ?, ?, ?) RETURNING id",
    ))
    .bind(redemption.id)
    .bind(&user_id)
    .bind(redemption.quota)
    .bind(log_gift_amount)
    .fetch_one(&mut *tx)
    .await?;

    match code_kind {
        "model_voucher" => {
            let worth_val = crate::money::round_money(redemption.worth_quota.max(0.0));
            let v_disc = if redemption.voucher_discount.is_finite()
                && redemption.voucher_discount > 0.0
                && redemption.voucher_discount <= 1.0
            {
                redemption.voucher_discount
            } else {
                1.0
            };
            let m_discs = if redemption.model_discounts.trim().is_empty() {
                "{}"
            } else {
                redemption.model_discounts.trim()
            };
            sqlx::query(&state.db.format_query(
                "INSERT INTO user_model_vouchers \
                 (user_id, redemption_id, redemption_log_id, name, face_value, remaining, worth_value, credit_discount, model_discounts, allowed_models, expires_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CASE WHEN ? > 0 THEN CURRENT_TIMESTAMP + (? * INTERVAL '1 day') ELSE NULL END)",
            ))
            .bind(&user_id)
            .bind(redemption.id)
            .bind(log_id)
            .bind(&redemption.name)
            .bind(crate::money::round_money(redemption.quota))
            .bind(crate::money::round_money(redemption.quota))
            .bind(worth_val)
            .bind(v_disc)
            .bind(m_discs)
            .bind(&redemption.allowed_models)
            .bind(redemption.valid_days)
            .bind(redemption.valid_days)
            .execute(&mut *tx)
            .await?;
            sqlx::query(&state.db.format_query(
                "INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type) \
                 VALUES (?, ?, 'model_voucher', ?, 'system')",
            ))
            .bind(&user_id)
            .bind(redemption.quota)
            .bind(format!("模型卡券: {}", redemption.name))
            .execute(&mut *tx)
            .await?;
        }
        "playground_space" => {
            let space_mb = space_mb.unwrap_or(0);
            sqlx::query(&state.db.format_query(
                "INSERT INTO playground_2026_space_grants \
                 (user_id, redemption_id, redemption_log_id, space_mb, expires_at, status) \
                 VALUES (?, ?, ?, ?, CASE WHEN ? > 0 THEN CURRENT_TIMESTAMP + (? * INTERVAL '1 day') ELSE NULL END, 1)",
            ))
            .bind(&user_id)
            .bind(redemption.id)
            .bind(log_id)
            .bind(space_mb)
            .bind(redemption.valid_days)
            .bind(redemption.valid_days)
            .execute(&mut *tx)
            .await?;
        }
        _ => {
            let sys_quota = crate::money::round_money(redemption.quota.max(0.0));
            let gift_quota = crate::money::round_money(redemption.gift_quota.max(0.0));
            if sys_quota > 0.0 {
                sqlx::query(
                    &state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type) VALUES (?, ?, 'redemption', ?, 'system')")
                )
                .bind(&user_id)
                .bind(sys_quota)
                .bind(format!("兑换码: {}", redemption.name))
                .execute(&mut *tx)
                .await?;
                crate::services::affiliate::note_redemption_credit(
                    &state.db,
                    &mut tx,
                    &user_id,
                    sys_quota,
                )
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            }
            if gift_quota > 0.0 {
                let gift_order_no = crate::api::plugins::finance::generate_gift_order_no();
                sqlx::query(
                    &state.db.format_query("INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no) VALUES (?, ?, 'redemption', ?, 'gift', ?)")
                )
                .bind(&user_id)
                .bind(gift_quota)
                .bind(format!("兑换码(赠送金): {}", redemption.name))
                .bind(&gift_order_no)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    tx.commit().await?;

    if code_kind == "balance" {
        crate::services::notification::spawn_low_balance_check(Arc::clone(&state), user_id.clone());
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "quota_added": redemption.quota,
        "worth_quota": crate::money::round_money(redemption.worth_quota.max(0.0)),
        "voucher_discount": redemption.voucher_discount,
        "model_discounts": parse_voucher_discount_map(&redemption.model_discounts),
        "gift_quota_added": log_gift_amount,
        "space_mb": space_mb.unwrap_or(0),
        "valid_days": redemption.valid_days,
        "code_type": code_kind,
        "name": redemption.name,
        "description": redemption.description
    })))
}

async fn load_voucher_model_name_map(
    state: &AppState,
    raw_keys: &[String],
) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
    let mut keys: Vec<String> = Vec::new();
    for raw in raw_keys {
        let trimmed = raw.trim();
        if !trimmed.is_empty() && !keys.iter().any(|k| k == trimmed) {
            keys.push(trimmed.to_string());
        }
    }
    if keys.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows: Vec<(String, String, String)> = sqlx::query_as(&state.db.format_query(
        "SELECT COALESCE(mid, ''), COALESCE(model_id, ''), COALESCE(NULLIF(btrim(name), ''), model_id) \
         FROM models WHERE mid = ANY(?) OR model_id = ANY(?) \
         ORDER BY is_system ASC, is_listed DESC, id DESC",
    ))
    .bind(&keys)
    .bind(&keys)
    .fetch_all(&state.db.pool)
    .await?;
    let mut map = std::collections::HashMap::with_capacity(rows.len() * 2);
    for (mid, model_id, display_name) in rows {
        let mid = mid.trim();
        let model_id = model_id.trim();
        let display_name = display_name.trim();
        if display_name.is_empty() {
            continue;
        }
        if !mid.is_empty() {
            map.entry(mid.to_string())
                .or_insert_with(|| display_name.to_string());
        }
        if !model_id.is_empty() {
            map.entry(model_id.to_string())
                .or_insert_with(|| display_name.to_string());
        }
    }
    Ok(map)
}

fn display_voucher_models(
    raw_ids: &[String],
    name_map: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::with_capacity(raw_ids.len());
    for item in raw_ids {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resolved = name_map
            .get(trimmed)
            .cloned()
            .unwrap_or_else(|| trimmed.to_string());
        if !out.contains(&resolved) {
            out.push(resolved);
        }
    }
    out
}

fn display_voucher_discount_map(
    raw_val: &serde_json::Value,
    name_map: &std::collections::HashMap<String, String>,
) -> serde_json::Value {
    let Some(obj) = raw_val.as_object() else {
        return serde_json::json!({});
    };
    let mut out = serde_json::Map::new();
    for (key, val) in obj {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resolved = name_map
            .get(trimmed)
            .cloned()
            .unwrap_or_else(|| trimmed.to_string());
        if let Some(num) = val.as_f64() {
            if let Some(existing) = out.get(&resolved).and_then(|v| v.as_f64()) {
                if num < existing {
                    out.insert(resolved, val.clone());
                }
            } else {
                out.insert(resolved, val.clone());
            }
        }
    }
    serde_json::Value::Object(out)
}

/// 当前用户兑换过的模型卡券，含已用完和已过期，供卡券记录展示。
pub async fn list_my_vouchers(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<Vec<serde_json::Value>>> {
    let rows: Vec<(
        i64,
        String,
        f64,
        f64,
        f64,
        f64,
        String,
        String,
        Option<crate::time_system::DbTs>,
        crate::time_system::DbTs,
        String,
        i64,
        crate::time_system::DbTs,
        String,
    )> = sqlx::query_as(&state.db.format_query(
        "SELECT v.id, v.name, v.face_value, v.remaining, \
                COALESCE(v.worth_value, 0.0), COALESCE(v.credit_discount, 1.0), COALESCE(v.model_discounts, '{}'), \
                v.allowed_models, v.expires_at, v.redeemed_at, \
                COALESCE(r.code, ''), v.status, v.updated_at, COALESCE(r.description, '') \
         FROM user_model_vouchers v \
         LEFT JOIN redemptions r ON r.id = v.redemption_id \
         WHERE v.user_id = ? \
         ORDER BY v.redeemed_at DESC, v.id DESC LIMIT 200",
    ))
    .bind(&claims.sub)
    .fetch_all(&state.db.pool)
    .await?;

    let mut voucher_model_keys: Vec<String> = Vec::new();
    for row in &rows {
        let models: Vec<String> = serde_json::from_str(&row.7).unwrap_or_default();
        voucher_model_keys.extend(models);
        if let Some(obj) = parse_voucher_discount_map(&row.6).as_object() {
            voucher_model_keys.extend(obj.keys().cloned());
        }
    }
    let name_map = load_voucher_model_name_map(&state, &voucher_model_keys).await?;

    let now = chrono::Utc::now();
    let mut data: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                id,
                name,
                face_value,
                remaining,
                worth_value,
                credit_discount,
                model_discounts,
                allowed_models,
                expires_at,
                redeemed_at,
                code,
                stored,
                updated_at,
                description,
            )| {
                let face_value = crate::money::round_money(face_value);
                let remaining = crate::money::round_money(remaining.max(0.0));
                let worth_value = crate::money::round_money(worth_value.max(0.0));
                let models: Vec<String> = serde_json::from_str(&allowed_models).unwrap_or_default();
                let expired = expires_at
                    .as_ref()
                    .and_then(|ts| ts.to_utc())
                    .is_some_and(|at| at <= now);
                let status = if stored != 1 {
                    "voided"
                } else if remaining <= 0.0 {
                    "exhausted"
                } else if expired {
                    "expired"
                } else {
                    "active"
                };
                serde_json::json!({
                    "source": "redeem",
                    "id": id,
                    "name": name,
                    "description": description,
                    "face_value": face_value,
                    "remaining": remaining,
                    "worth_value": worth_value,
                    "credit_discount": credit_discount,
                    "model_discounts": display_voucher_discount_map(&parse_voucher_discount_map(&model_discounts), &name_map),
                    "allowed_models": display_voucher_models(&models, &name_map),
                    "expires_at": expires_at,
                    "redeemed_at": redeemed_at,
                    "code": code,
                    "status": status,
                    "updated_at": updated_at,
                })
            },
        )
        .collect();

    // 只列余额兑换。创作空间和模型卡券各自有账，不能当成已入账余额。
    let balances: Vec<(i64, String, f64, f64, String, crate::time_system::DbTs, i64, i64, i64, String)> = sqlx::query_as(&state.db.format_query(
        "SELECT l.id, COALESCE(r.name, ''), l.amount, COALESCE(l.gift_amount, 0.0), COALESCE(r.code, ''), l.created_at, r.id, r.status, l.refunded, COALESCE(r.description, '') \
         FROM redemption_logs l \
         JOIN redemptions r ON r.id = l.redemption_id \
         WHERE l.user_id = ? \
           AND COALESCE(NULLIF(btrim(r.code_type), ''), 'balance') = 'balance' \
         ORDER BY l.created_at DESC, l.id DESC LIMIT 200",
    ))
    .bind(&claims.sub)
    .fetch_all(&state.db.pool)
    .await?;

    for row in balances {
        let returned = row.7 == -2 || row.8 != 0;
        let card_name = if row.1.trim().is_empty() {
            "余额卡券".to_string()
        } else {
            row.1
        };
        let sys_amount = crate::money::round_money(row.2.max(0.0));
        let gift_amount = crate::money::round_money(row.3.max(0.0));
        let total_face = crate::money::round_money(sys_amount + gift_amount);
        data.push(serde_json::json!({
            "source": "balance",
            "id": row.0,
            "name": card_name,
            "description": row.9,
            "face_value": total_face,
            "remaining": if returned { 0.0 } else { total_face },
            "system_amount": sys_amount,
            "gift_amount": gift_amount,
            "allowed_models": Vec::<String>::new(),
            "expires_at": None::<String>,
            "redeemed_at": row.5,
            "code": row.4,
            "status": if returned { "returned" } else { "credited" },
            "updated_at": row.5,
        }));
    }

    // 兼容历史直接使用且未记 log 的 legacy 余额码
    let legacy_balances: Vec<(i64, String, f64, f64, String, crate::time_system::DbTs, i64, i64, String)> = sqlx::query_as(&state.db.format_query(
        "SELECT r.id, r.name, r.quota, COALESCE(r.gift_quota, 0.0), r.code, COALESCE(r.used_at, r.created_at), r.id, r.status, COALESCE(r.description, '') \
         FROM redemptions r \
         WHERE r.used_by = ? \
           AND COALESCE(NULLIF(btrim(r.code_type), ''), 'balance') = 'balance' \
           AND NOT EXISTS ( \
             SELECT 1 FROM redemption_logs l WHERE l.redemption_id = r.id AND l.user_id = r.used_by \
           ) \
         ORDER BY COALESCE(r.used_at, r.created_at) DESC, r.id DESC LIMIT 200",
    ))
    .bind(&claims.sub)
    .fetch_all(&state.db.pool)
    .await?;

    for row in legacy_balances {
        let returned = row.7 == -2;
        let card_name = if row.1.trim().is_empty() {
            "余额卡券".to_string()
        } else {
            row.1
        };
        let sys_amount = crate::money::round_money(row.2.max(0.0));
        let gift_amount = crate::money::round_money(row.3.max(0.0));
        let total_face = crate::money::round_money(sys_amount + gift_amount);
        data.push(serde_json::json!({
            "source": "balance",
            "id": row.0,
            "name": card_name,
            "description": row.8,
            "face_value": total_face,
            "remaining": if returned { 0.0 } else { total_face },
            "system_amount": sys_amount,
            "gift_amount": gift_amount,
            "allowed_models": Vec::<String>::new(),
            "expires_at": None::<String>,
            "redeemed_at": row.5,
            "code": row.4,
            "status": if returned { "returned" } else { "credited" },
            "updated_at": row.5,
        }));
    }

    let spaces: Vec<(
        i64,
        String,
        i64,
        String,
        crate::time_system::DbTs,
        Option<crate::time_system::DbTs>,
        i64,
        String,
    )> = sqlx::query_as(&state.db.format_query(
        "SELECT g.id, COALESCE(NULLIF(r.name, ''), '创作空间兑换码'), g.space_mb, COALESCE(r.code, ''), \
                g.created_at, g.expires_at, g.status, COALESCE(r.description, '') \
         FROM playground_2026_space_grants g \
         LEFT JOIN redemptions r ON r.id = g.redemption_id \
         WHERE g.user_id = ? \
         ORDER BY g.created_at DESC, g.id DESC LIMIT 200",
    ))
    .bind(&claims.sub)
    .fetch_all(&state.db.pool)
    .await?;
    for row in spaces {
        let expired = row.5.as_ref().and_then(|ts| ts.to_utc()).is_some_and(|at| at <= now);
        let status = if row.6 != 1 {
            "voided"
        } else if expired {
            "expired"
        } else {
            "active"
        };
        data.push(serde_json::json!({
            "source": "playground_space",
            "id": row.0,
            "name": row.1,
            "description": row.7,
            "face_value": 0.0,
            "remaining": 0.0,
            "space_mb": row.2,
            "allowed_models": Vec::<String>::new(),
            "expires_at": row.5,
            "redeemed_at": row.4,
            "code": row.3,
            "status": status,
            "updated_at": row.4,
        }));
    }

    Ok(Json(data))
}

#[cfg(not(feature = "commercial_plugins"))]
pub async fn list_purchase_orders_os() -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "data": [],
        "total": 0,
        "total_amount": 0.0
    })))
}

#[cfg(not(feature = "commercial_plugins"))]
async fn os_empty_my_orders() -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "data": [],
        "total": 0,
        "total_amount": 0.0
    })))
}

#[cfg(not(feature = "commercial_plugins"))]
async fn os_empty_records() -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "credits": [],
        "space_grants": [],
        "subscriptions": []
    })))
}

#[cfg(not(feature = "commercial_plugins"))]
async fn os_empty_placements() -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "campaigns": []
    })))
}

#[cfg(not(feature = "commercial_plugins"))]
async fn os_empty_catalog() -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "level_name": "",
        "balance": 0.0
    })))
}

#[cfg(not(feature = "commercial_plugins"))]
async fn os_resolve_user_id(state: &AppState, raw: &str) -> AppResult<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(AppError::BadRequest("缺少用户 ID".to_string()));
    }
    let id: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM users WHERE id = ? OR uid = ? LIMIT 1"),
    )
    .bind(raw)
    .bind(raw)
    .fetch_optional(&state.db.pool)
    .await?;
    id.ok_or_else(|| AppError::NotFound("用户不存在".to_string()))
}

#[cfg(not(feature = "commercial_plugins"))]
#[allow(clippy::too_many_arguments)]
fn os_push_balance_coupon(
    coupons: &mut Vec<serde_json::Value>,
    id: i64,
    name: String,
    amount: f64,
    gift_amount: f64,
    code: String,
    created_at: crate::time_system::DbTs,
    redemption_id: i64,
    redemption_status: i64,
    refunded: i64,
    log_id: Option<i64>,
) {
    let returned = redemption_status == -2 || refunded != 0;
    let sys_amount = crate::money::round_money(amount.max(0.0));
    let gift_amt = crate::money::round_money(gift_amount.max(0.0));
    let total_face = crate::money::round_money(sys_amount + gift_amt);
    coupons.push(serde_json::json!({
        "source": "balance",
        "id": id,
        "order_id": null,
        "redemption_id": redemption_id,
        "log_id": log_id,
        "name": name,
        "face_value": total_face,
        "remaining": if returned { 0.0 } else { total_face },
        "system_amount": sys_amount,
        "gift_amount": gift_amt,
        "models": [],
        "expires_at": null,
        "status": if returned { "returned" } else { "credited" },
        "created_at": created_at,
        "pay_method": "",
        "code": code,
        "voidable": false,
    }));
}

#[cfg(not(feature = "commercial_plugins"))]
async fn os_admin_user_coupons(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(raw_user_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }
    let user_id = os_resolve_user_id(&state, &raw_user_id).await?;
    let mut coupons: Vec<serde_json::Value> = Vec::new();
    let vouchers: Vec<(
        i64,
        String,
        f64,
        f64,
        f64,
        f64,
        String,
        String,
        Option<crate::time_system::DbTs>,
        crate::time_system::DbTs,
        i64,
        String,
    )> = sqlx::query_as(&state.db.format_query(
        "SELECT v.id, v.name, v.face_value, v.remaining, \
                COALESCE(v.worth_value, 0.0), COALESCE(v.credit_discount, 1.0), COALESCE(v.model_discounts, '{}'), \
                v.allowed_models, v.expires_at, v.redeemed_at, \
                v.status, COALESCE(r.code, '') \
         FROM user_model_vouchers v \
         LEFT JOIN redemptions r ON r.id = v.redemption_id \
         WHERE v.user_id = ? \
         ORDER BY v.redeemed_at DESC, v.id DESC LIMIT 200",
    ))
    .bind(&user_id)
    .fetch_all(&state.db.pool)
    .await?;
    let mut voucher_model_keys: Vec<String> = Vec::new();
    for row in &vouchers {
        let models: Vec<String> = serde_json::from_str(&row.7).unwrap_or_default();
        voucher_model_keys.extend(models);
        if let Some(obj) = parse_voucher_discount_map(&row.6).as_object() {
            voucher_model_keys.extend(obj.keys().cloned());
        }
    }
    let name_map = load_voucher_model_name_map(&state, &voucher_model_keys).await?;
    let now = chrono::Utc::now();
    for row in vouchers {
        let models: Vec<String> = serde_json::from_str(&row.7).unwrap_or_default();
        let expired = row
            .8
            .as_ref()
            .and_then(|ts| ts.to_utc())
            .is_some_and(|at| at <= now);
        let face_value = crate::money::round_money(row.2);
        let remaining = crate::money::round_money(row.3).max(0.0);
        let worth_value = crate::money::round_money(row.4.max(0.0));
        let status = if row.10 != 1 {
            "voided"
        } else if remaining <= 0.0 {
            "exhausted"
        } else if expired {
            "expired"
        } else {
            "active"
        };
        coupons.push(serde_json::json!({
            "source": "redeem",
            "id": row.0,
            "order_id": null,
            "name": row.1,
            "face_value": face_value,
            "remaining": remaining,
            "worth_value": worth_value,
            "credit_discount": row.5,
            "model_discounts": display_voucher_discount_map(&parse_voucher_discount_map(&row.6), &name_map),
            "models": display_voucher_models(&models, &name_map),
            "expires_at": row.8,
            "status": status,
            "created_at": row.9,
            "pay_method": "",
            "code": row.11,
            "voidable": status == "active",
        }));
    }
    let balances: Vec<(
        i64,
        String,
        f64,
        f64,
        String,
        crate::time_system::DbTs,
        i64,
        i64,
        i64,
    )> = sqlx::query_as(&state.db.format_query(
        "SELECT l.id, COALESCE(r.name, ''), l.amount, COALESCE(l.gift_amount, 0.0), COALESCE(r.code, ''), l.created_at, r.id, r.status, l.refunded \
         FROM redemption_logs l \
         JOIN redemptions r ON r.id = l.redemption_id \
         WHERE l.user_id = ? \
           AND COALESCE(NULLIF(btrim(r.code_type), ''), 'balance') = 'balance' \
         ORDER BY l.created_at DESC, l.id DESC LIMIT 200",
    ))
    .bind(&user_id)
    .fetch_all(&state.db.pool)
    .await?;
    for row in balances {
        os_push_balance_coupon(
            &mut coupons,
            row.0,
            row.1,
            row.2,
            row.3,
            row.4,
            row.5,
            row.6,
            row.7,
            row.8,
            Some(row.0),
        );
    }
    let legacy_balances: Vec<(
        i64,
        String,
        f64,
        f64,
        String,
        crate::time_system::DbTs,
        i64,
        i64,
    )> = sqlx::query_as(&state.db.format_query(
        "SELECT r.id, r.name, r.quota, COALESCE(r.gift_quota, 0.0), r.code, COALESCE(r.used_at, r.created_at), r.id, r.status \
         FROM redemptions r \
         WHERE r.used_by = ? \
           AND COALESCE(NULLIF(btrim(r.code_type), ''), 'balance') = 'balance' \
           AND NOT EXISTS ( \
             SELECT 1 FROM redemption_logs l WHERE l.redemption_id = r.id AND l.user_id = r.used_by \
           ) \
         ORDER BY COALESCE(r.used_at, r.created_at) DESC, r.id DESC LIMIT 200",
    ))
    .bind(&user_id)
    .fetch_all(&state.db.pool)
    .await?;
    for row in legacy_balances {
        os_push_balance_coupon(
            &mut coupons,
            row.0,
            row.1,
            row.2,
            row.3,
            row.4,
            row.5,
            row.6,
            row.7,
            0,
            None,
        );
    }
    let spaces: Vec<(
        i64,
        String,
        i64,
        String,
        crate::time_system::DbTs,
        i64,
        i64,
        Option<crate::time_system::DbTs>,
        i64,
    )> = sqlx::query_as(&state.db.format_query(
        "SELECT g.id, COALESCE(NULLIF(r.name, ''), '创作空间兑换码'), g.space_mb, COALESCE(r.code, ''), \
                g.created_at, g.redemption_id, g.redemption_log_id, g.expires_at, g.status \
         FROM playground_2026_space_grants g \
         LEFT JOIN redemptions r ON r.id = g.redemption_id \
         WHERE g.user_id = ? \
         ORDER BY g.created_at DESC, g.id DESC LIMIT 200",
    ))
    .bind(&user_id)
    .fetch_all(&state.db.pool)
    .await?;
    for row in spaces {
        let expired = row
            .7
            .as_ref()
            .and_then(|ts| ts.to_utc())
            .is_some_and(|at| at <= now);
        let status = if row.8 != 1 {
            "voided"
        } else if expired {
            "expired"
        } else {
            "active"
        };
        coupons.push(serde_json::json!({
            "source": "playground_space",
            "id": row.0,
            "order_id": null,
            "redemption_id": row.5,
            "log_id": row.6,
            "name": row.1,
            "face_value": 0.0,
            "remaining": 0.0,
            "space_mb": row.2,
            "models": [],
            "expires_at": row.7,
            "status": status,
            "created_at": row.4,
            "pay_method": "",
            "code": row.3,
            "voidable": false,
        }));
    }
    Ok(Json(serde_json::json!({
        "coupons": coupons,
        "plans": []
    })))
}

#[cfg(not(feature = "commercial_plugins"))]
async fn os_admin_void_voucher(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(voucher_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }
    let mut tx = state.db.pool.begin().await?;
    let row: Option<(Option<i64>, i64, String)> = sqlx::query_as(&state.db.format_query(
        "UPDATE user_model_vouchers SET remaining = 0, status = 0, updated_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND status = 1 \
         RETURNING redemption_log_id, redemption_id, user_id",
    ))
    .bind(voucher_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((redemption_log_id, redemption_id, user_id)) = row else {
        return Err(AppError::BadRequest("这张兑换卡券已经终止".to_string()));
    };
    if let Some(log_id) = redemption_log_id.filter(|id| *id > 0) {
        sqlx::query(&state.db.format_query(
            "UPDATE redemption_logs SET refunded = 1 WHERE id = ? AND refunded = 0",
        ))
        .bind(log_id)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(&state.db.format_query(
            "UPDATE redemption_logs SET refunded = 1 \
             WHERE id = (SELECT id FROM redemption_logs WHERE redemption_id = ? AND user_id = ? AND refunded = 0 ORDER BY id ASC LIMIT 1)",
        ))
        .bind(redemption_id)
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

#[cfg(not(feature = "commercial_plugins"))]
#[derive(serde::Deserialize)]
struct OsAdminRedeemBody {
    code: String,
}

#[cfg(not(feature = "commercial_plugins"))]
async fn os_admin_redeem_for_user(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(raw_user_id): Path<String>,
    Json(body): Json<OsAdminRedeemBody>,
) -> AppResult<Json<serde_json::Value>> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admin only".to_string()));
    }
    let user_id = os_resolve_user_id(&state, &raw_user_id).await?;
    redeem_for_user(state, user_id, body.code).await
}

#[cfg(not(feature = "commercial_plugins"))]
pub fn opensource_membership_compat_router() -> axum::Router<Arc<AppState>> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/my-orders", get(os_empty_my_orders))
        .route("/records", get(os_empty_records))
        .route("/placements/{key}", get(os_empty_placements))
        .route("/catalog", get(os_empty_catalog))
        .route("/admin/users/{user_id}/coupons", get(os_admin_user_coupons))
        .route("/admin/users/{user_id}/redeem", post(os_admin_redeem_for_user))
        .route("/admin/vouchers/{voucher_id}/void", post(os_admin_void_voucher))
}

#[cfg(test)]
mod starts_at_tests {
    use super::{is_not_started, normalize_start_time};

    #[test]
    fn empty_start_is_already_effective() {
        assert!(!is_not_started(None, "Asia/Shanghai"));
        assert!(!is_not_started(Some(""), "Asia/Shanghai"));
        assert!(!is_not_started(Some("  "), "Asia/Shanghai"));
    }

    #[test]
    fn future_and_past_instants() {
        let future = chrono::Utc::now() + chrono::Duration::hours(2);
        let past = chrono::Utc::now() - chrono::Duration::hours(2);
        let future_raw = future.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let past_raw = past.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        assert!(is_not_started(Some(&future_raw), "Asia/Shanghai"));
        assert!(!is_not_started(Some(&past_raw), "Asia/Shanghai"));
    }

    #[test]
    fn date_only_uses_start_of_site_day() {
        let tz: chrono_tz::Tz = "Asia/Shanghai".parse().unwrap();
        let today = chrono::Utc::now().with_timezone(&tz).date_naive();
        let tomorrow = today + chrono::Duration::days(1);
        assert!(!is_not_started(
            Some(&today.format("%Y-%m-%d").to_string()),
            "Asia/Shanghai"
        ));
        assert!(is_not_started(
            Some(&tomorrow.format("%Y-%m-%d").to_string()),
            "Asia/Shanghai"
        ));
    }

    #[test]
    fn wall_clock_is_stored_as_site_timezone() {
        let stored = normalize_start_time("2026-10-15 10:00:00", "Asia/Shanghai").unwrap();
        let tz: chrono_tz::Tz = "Asia/Shanghai".parse().unwrap();
        let local = stored.to_utc().unwrap().with_timezone(&tz);
        assert_eq!(local.format("%H:%M:%S").to_string(), "10:00:00");
        assert_eq!(local.format("%Y-%m-%d").to_string(), "2026-10-15");
    }
}

#[cfg(test)]
mod balance_refund_tests {
    use super::{
        aggregate_clawback_amounts, aggregate_clawback_wallet_amounts,
        balance_redeemed_status_error, eligibility_failure, is_balance_code_type,
        is_model_voucher_code, is_playground_space_code, normalize_redemption_quotas,
        normalize_voucher_discounts, pick_refund_logs, playground_space_mb, redeemed_balance_code,
        redeemed_deletion_error, redeemed_space_code, redeemed_voucher_code,
        redemption_unusable_message, ClawLog, EligibilityGate, RefundTarget,
        BALANCE_RETURNED_STATUS,
    };

    #[test]
    fn voucher_discounts_validate_and_filter_to_accepted_models() {
        let mut accepted = std::collections::HashSet::new();
        accepted.insert("gpt-5".to_string());
        accepted.insert("claude-4-sonnet".to_string());
        let mut floors = std::collections::HashMap::new();
        floors.insert("gpt-5".to_string(), 0.6);

        let (disc, json_str) = normalize_voucher_discounts(
            Some(0.8),
            Some(&serde_json::json!({
                "gpt-5": 0.65,
                "unselected-model": 0.5
            })),
            &accepted,
            &floors,
        )
        .unwrap();
        assert!((disc - 0.8).abs() < 1e-9);
        assert_eq!(json_str, r#"{"gpt-5":0.65}"#);

        assert!(normalize_voucher_discounts(Some(0.0), None, &accepted, &floors).is_err());
        assert!(normalize_voucher_discounts(Some(1.2), None, &accepted, &floors).is_err());
        assert!(normalize_voucher_discounts(
            Some(1.0),
            Some(&serde_json::json!({"gpt-5": 0.0})),
            &accepted,
            &floors
        )
        .is_err());
        // Below model floor (0.55 < 0.6) must be rejected
        assert!(normalize_voucher_discounts(
            Some(0.8),
            Some(&serde_json::json!({"gpt-5": 0.55})),
            &accepted,
            &floors
        )
        .is_err());
    }

    #[test]
    fn same_user_redemptions_sum_and_sort_by_user() {
        let lines = aggregate_clawback_amounts(vec![
            ("b".to_string(), 3.0),
            ("a".to_string(), 1.5),
            ("b".to_string(), 2.0),
            ("".to_string(), 9.0),
            ("a".to_string(), 0.0),
        ]);
        assert_eq!(
            lines,
            vec![("a".to_string(), 1.5), ("b".to_string(), 5.0)]
        );
    }

    #[test]
    fn balance_quotas_support_system_or_gift_or_both() {
        assert_eq!(normalize_redemption_quotas("balance", 10.0, 0.0).unwrap(), (10.0, 0.0));
        assert_eq!(normalize_redemption_quotas("balance", 0.0, 25.5).unwrap(), (0.0, 25.5));
        assert_eq!(normalize_redemption_quotas("balance", 10.0, 5.0).unwrap(), (10.0, 5.0));
        assert!(normalize_redemption_quotas("balance", 0.0, 0.0).is_err());
        assert!(normalize_redemption_quotas("balance", -1.0, 5.0).is_err());
        assert!(normalize_redemption_quotas("model_voucher", 0.0, 10.0).is_err());
        assert_eq!(normalize_redemption_quotas("model_voucher", 12.0, 99.0).unwrap(), (12.0, 0.0));
    }

    #[test]
    fn clawback_wallet_amounts_aggregate_system_and_gift_separately() {
        let rows = aggregate_clawback_wallet_amounts(vec![
            ("u2".into(), 0.0, 5.0),
            ("u1".into(), 2.0, 3.0),
            ("u2".into(), 4.0, 1.5),
            ("u3".into(), 0.0, 0.0),
        ]);
        assert_eq!(
            rows,
            vec![
                ("u1".into(), 2.0, 3.0),
                ("u2".into(), 4.0, 6.5),
            ]
        );
    }

    #[test]
    fn only_redeemed_balance_codes_lose_void() {
        assert!(redeemed_balance_code("balance", 1, 0));
        assert!(redeemed_balance_code("", 0, 1));
        assert!(!redeemed_balance_code("balance", 0, 0));
        assert!(!redeemed_balance_code("model_voucher", 1, 1));
        assert!(!redeemed_balance_code("playground_space", 1, 1));
        assert!(!is_balance_code_type("playground_space"));
        assert!(is_playground_space_code("playground_space"));
        assert!(is_model_voucher_code("model_voucher"));
        assert!(redeemed_space_code("playground_space", 1, 0));
        assert!(!redeemed_space_code("balance", 1, 1));
        assert!(redeemed_voucher_code("model_voucher", 1, 0));
        assert!(!redeemed_voucher_code("model_voucher", 0, 0));
        assert_eq!(
            redeemed_deletion_error("model_voucher", 1, 0),
            Some("已兑换的模型卡券兑换码不能删除")
        );
        assert_eq!(
            redeemed_deletion_error("playground_space", 1, 0),
            Some("已兑换的创作空间兑换码不能删除")
        );
        assert_eq!(
            redeemed_deletion_error("balance", 1, 0),
            Some("已兑换入账的余额兑换码不能删除")
        );
        assert_eq!(redeemed_deletion_error("model_voucher", 0, 0), None);
        assert_eq!(playground_space_mb(1024.0).unwrap(), 1024);
        assert!(playground_space_mb(1.5).is_err());
        assert!(playground_space_mb(0.0).is_err());
        assert!(playground_space_mb(1_048_577.0).is_err());
        assert!(is_balance_code_type(" balance "));
        assert_eq!(
            redemption_unusable_message(BALANCE_RETURNED_STATUS),
            "该兑换码已退还兑换余额"
        );
    }

    #[test]
    fn eligibility_all_and_any() {
        let gates = [
            EligibilityGate { label: "账户余额", required: 10.0, current: 10.0 },
            EligibilityGate { label: "当月消费", required: 20.0, current: 1.0 },
        ];
        assert!(eligibility_failure(false, &gates).is_some());
        assert!(eligibility_failure(true, &gates).is_none());
        assert!(eligibility_failure(false, &[]).is_none());
        let off = [EligibilityGate { label: "总充值", required: 0.0, current: 0.0 }];
        assert!(eligibility_failure(false, &off).is_none());
    }

    #[test]
    fn redeemed_multi_use_balance_can_be_disabled() {
        assert!(balance_redeemed_status_error("balance", 3, 0, 0).is_none());
        assert!(balance_redeemed_status_error("balance", 3, 0, 1).is_none());
        assert!(balance_redeemed_status_error("balance", 3, 0, -1).is_none());
        assert!(balance_redeemed_status_error("balance", 0, 0, -1).is_none());
        assert!(balance_redeemed_status_error("model_voucher", 3, 1, -1).is_none());
        assert!(balance_redeemed_status_error("playground_space", 3, 1, -1).is_none());
    }

    #[test]
    fn refund_can_target_one_user_or_one_log() {
        let logs = vec![
            ClawLog { id: 1, user_id: "a".into(), amount: 3.0, gift_amount: 0.0, refunded: 0 },
            ClawLog { id: 2, user_id: "a".into(), amount: 0.0, gift_amount: 4.0, refunded: 0 },
            ClawLog { id: 3, user_id: "b".into(), amount: 5.0, gift_amount: 2.0, refunded: 0 },
        ];
        let once = pick_refund_logs(&logs, &RefundTarget::Log(2)).unwrap();
        assert_eq!(once.len(), 1);
        assert_eq!(once[0].id, 2);
        let user = pick_refund_logs(&logs, &RefundTarget::User("a".into())).unwrap();
        assert_eq!(user.iter().map(|row| row.id).collect::<Vec<_>>(), vec![1, 2]);
        let mut partial = logs.clone();
        partial[0].refunded = 1;
        let left = pick_refund_logs(&partial, &RefundTarget::User("a".into())).unwrap();
        assert_eq!(left.iter().map(|row| row.id).collect::<Vec<_>>(), vec![2]);
        assert_eq!(
            pick_refund_logs(&partial, &RefundTarget::Log(1)).unwrap_err(),
            "这一次兑换已经退换"
        );
        let all = pick_refund_logs(&partial, &RefundTarget::All).unwrap();
        assert_eq!(all.iter().map(|row| row.id).collect::<Vec<_>>(), vec![2, 3]);
    }
}
