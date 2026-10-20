/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::error::AppResult;
use crate::time_system::DbTs;
use crate::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize)]
pub struct FinanceQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub user_id: Option<String>,
    pub recharge_type: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub status: Option<String>,
    pub payment_method: Option<String>,
    pub exclude_type: Option<String>,
    pub wallet_type: Option<String>,
    pub referrer: Option<String>,
    pub order_no: Option<String>,
}

/// 财务列表筛选：按管理后台展示时区解释墙钟。落库仍是 UTC。
async fn finance_filter_tz(state: &AppState) -> chrono_tz::Tz {
    let tz_name = crate::relay::relay_settings::get_cached_admin_display_timezone(&state.db).await;
    crate::time_system::parse_timedisplay(&tz_name)
}

// ========== 充值记录（recharge_records）==========

/// 生成专属赠送金订单号（G 开头，与系统充值的 T 开头、BonusPay 的 BP 开头形成清晰区分）
pub fn generate_gift_order_no() -> String {
    format!(
        "G{}R{}",
        chrono::Local::now().format("%Y%m%d%H%M%S"),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    )
}

/// 生成标准系统充值/系统资金订单号（T 开头）
pub fn generate_system_order_no() -> String {
    format!(
        "T{}R{}",
        chrono::Local::now().format("%Y%m%d%H%M%S"),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    )
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FinanceRechargeRecord {
    pub id: i64,
    pub user_id: String,
    pub username: String,
    pub uid: String,
    #[sqlx(default)]
    pub referrer_uid: Option<String>,
    #[sqlx(default)]
    pub referrer_username: Option<String>,
    pub amount: f64,
    pub recharge_type: String,
    #[sqlx(default)]
    pub order_no: Option<String>,
    pub remark: Option<String>,
    #[sqlx(default)]
    pub operator: Option<String>,
    #[sqlx(default)]
    pub wallet_type: Option<String>,
    pub created_at: DbTs,
}

#[derive(Debug, Serialize)]
pub struct FinanceRechargeResponse {
    pub data: Vec<FinanceRechargeRecord>,
    pub total: i64,
    pub total_amount: f64,
    /// 同一筛选条件下，模型卡券兑换面值。兑换当时一次核销，不进入用户余额。
    pub voucher_writeoff_amount: f64,
}

pub async fn list_recharges(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FinanceQuery>,
) -> AppResult<Json<FinanceRechargeResponse>> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20);
    let offset = (page - 1) * per_page;

    let base_from = "FROM recharge_records rr JOIN users u ON rr.user_id = u.id LEFT JOIN users inviter ON u.referred_by = inviter.id";
    let mut sql = format!("SELECT rr.*, u.username, u.uid, inviter.uid as referrer_uid, inviter.username as referrer_username {}", base_from);
    let mut where_clause = " WHERE 1=1".to_string();
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref search) = query.user_id {
        where_clause.push_str(" AND (rr.user_id = ? OR u.uid = ? OR u.username LIKE ? OR rr.order_no LIKE ? OR rr.remark LIKE ?)");
        binds.push(search.clone());
        binds.push(search.clone());
        binds.push(format!("%{}%", search));
        binds.push(format!("%{}%", search));
        binds.push(format!("%{}%", search));
    }

    if let Some(ref ono) = query.order_no {
        where_clause.push_str(" AND (rr.order_no = ? OR rr.order_no LIKE ? OR rr.remark LIKE ?)");
        binds.push(ono.clone());
        binds.push(format!("%{}%", ono));
        binds.push(format!("%{}%", ono));
    }

    if let Some(ref r_type) = query.recharge_type {
        where_clause.push_str(" AND rr.recharge_type = ?");
        binds.push(r_type.clone());
    }

    if let Some(ref e_type) = query.exclude_type {
        where_clause.push_str(" AND rr.recharge_type != ?");
        binds.push(e_type.clone());
    }

    if let Some(ref w_type) = query.wallet_type {
        where_clause.push_str(" AND rr.wallet_type = ?");
        binds.push(w_type.clone());
    }

    let tz = finance_filter_tz(&state).await;

    if let Some(ref start) = query.start_time {
        crate::api::date_helper::push_timestamptz_bound(
            &mut where_clause,
            &mut binds,
            "rr.created_at",
            start,
            false,
            tz,
        );
    }

    if let Some(ref end) = query.end_time {
        crate::api::date_helper::push_timestamptz_bound(
            &mut where_clause,
            &mut binds,
            "rr.created_at",
            end,
            true,
            tz,
        );
    }

    if let Some(ref ref_search) = query.referrer {
        where_clause.push_str(" AND (inviter.uid = ? OR inviter.username LIKE ?)");
        binds.push(ref_search.clone());
        binds.push(format!("%{}%", ref_search));
    }

    sql.push_str(&where_clause);

    let count_sql = format!("SELECT COUNT(*) {}{}", base_from, where_clause);
    let count_query_str = state.db.format_query(&count_sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query_str);
    for val in &binds {
        count_q = count_q.bind(val);
    }
    let total = count_q.fetch_one(&state.db.pool).await.map_err(|e| {
        tracing::warn!("Finance recharges count error: {:?}", e);
        e
    })?;

    sql.push_str(&format!(
        " ORDER BY rr.created_at DESC LIMIT {} OFFSET {}",
        per_page, offset
    ));
    let formatted_data_sql = state.db.format_query(&sql);
    let mut data_q = sqlx::query_as::<_, FinanceRechargeRecord>(&formatted_data_sql);
    for val in &binds {
        data_q = data_q.bind(val);
    }
    let data = data_q.fetch_all(&state.db.pool).await.map_err(|e| {
        tracing::warn!("Finance recharges data error: {:?}", e);
        e
    })?;

    let total_amount_sql = format!(
        "SELECT COALESCE(SUM(rr.amount), 0.0) {}{} AND COALESCE(rr.recharge_type, '') != 'model_voucher'",
        base_from, where_clause
    );
    let total_amount_query_str = state.db.format_query(&total_amount_sql);
    let mut amount_q = sqlx::query_scalar::<_, f64>(&total_amount_query_str);
    for val in &binds {
        amount_q = amount_q.bind(val);
    }
    let total_amount = amount_q.fetch_one(&state.db.pool).await.unwrap_or(0.0);

    let writeoff_sql = format!(
        "SELECT COALESCE(SUM(rr.amount), 0.0) {}{} AND rr.recharge_type = 'model_voucher'",
        base_from, where_clause
    );
    let writeoff_query_str = state.db.format_query(&writeoff_sql);
    let mut writeoff_q = sqlx::query_scalar::<_, f64>(&writeoff_query_str);
    for val in &binds {
        writeoff_q = writeoff_q.bind(val);
    }
    let voucher_writeoff_amount = writeoff_q.fetch_one(&state.db.pool).await.unwrap_or(0.0);

    Ok(Json(FinanceRechargeResponse {
        data,
        total,
        total_amount,
        voucher_writeoff_amount,
    }))
}

// ========== 支付订单（orders 表）==========

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FinanceOrderRecord {
    pub id: i64,
    pub out_trade_no: String,
    pub user_id: String,
    pub username: String,
    pub uid: String,
    pub payment_method: String,
    pub amount: f64,
    pub status: String,
    pub trade_no: Option<String>,
    pub created_at: DbTs,
    pub paid_at: Option<DbTs>,
    pub pay_currency: Option<String>,
    pub pay_amount: Option<f64>,
    pub fx_rate: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct FinanceOrderResponse {
    pub data: Vec<FinanceOrderRecord>,
    pub total: i64,
    pub total_amount: f64,
    pub payment_methods: Vec<String>,
}

pub async fn list_orders(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FinanceQuery>,
) -> AppResult<Json<FinanceOrderResponse>> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20);
    let offset = (page - 1) * per_page;

    let mut sql =
        "SELECT o.*, u.username, u.uid FROM orders o JOIN users u ON o.user_id = u.id".to_string();
    let mut where_clause = " WHERE 1=1".to_string();
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref search) = query.user_id {
        where_clause.push_str(" AND (o.user_id = ? OR u.uid = ? OR u.username LIKE ?)");
        binds.push(search.clone());
        binds.push(search.clone());
        binds.push(format!("%{}%", search));
    }

    if let Some(ref status) = query.status {
        where_clause.push_str(" AND o.status = ?");
        binds.push(status.clone());
    }

    if let Some(ref method) = query.payment_method {
        if method == "allinpay_wechat" || method == "allinpay wechat" {
            where_clause
                .push_str(" AND o.payment_method IN ('allinpay_wechat', 'allinpay wechat')");
        } else if method == "allinpay_alipay" || method == "allinpay alipay" {
            where_clause
                .push_str(" AND o.payment_method IN ('allinpay_alipay', 'allinpay alipay')");
        } else {
            where_clause.push_str(" AND o.payment_method = ?");
            binds.push(method.clone());
        }
    }

    let tz = finance_filter_tz(&state).await;

    if let Some(ref start) = query.start_time {
        crate::api::date_helper::push_timestamptz_bound(
            &mut where_clause,
            &mut binds,
            "o.created_at",
            start,
            false,
            tz,
        );
    }

    if let Some(ref end) = query.end_time {
        crate::api::date_helper::push_timestamptz_bound(
            &mut where_clause,
            &mut binds,
            "o.created_at",
            end,
            true,
            tz,
        );
    }

    sql.push_str(&where_clause);

    let count_sql = format!(
        "SELECT COUNT(*) FROM orders o JOIN users u ON o.user_id = u.id{}",
        where_clause
    );
    let count_query_str = state.db.format_query(&count_sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query_str);
    for val in &binds {
        count_q = count_q.bind(val);
    }
    let total = count_q.fetch_one(&state.db.pool).await.map_err(|e| {
        tracing::warn!("Finance orders count error: {:?}", e);
        e
    })?;

    sql.push_str(&format!(
        " ORDER BY o.created_at DESC LIMIT {} OFFSET {}",
        per_page, offset
    ));
    let formatted_data_sql = state.db.format_query(&sql);
    let mut data_q = sqlx::query_as::<_, FinanceOrderRecord>(&formatted_data_sql);
    for val in &binds {
        data_q = data_q.bind(val);
    }
    let data = data_q.fetch_all(&state.db.pool).await.map_err(|e| {
        tracing::warn!("Finance orders data error: {:?}", e);
        e
    })?;

    let total_amount_sql = format!("SELECT COALESCE(SUM(o.amount), 0.0) FROM orders o JOIN users u ON o.user_id = u.id{} AND o.status = 'paid'", where_clause);
    let total_amount_query_str = state.db.format_query(&total_amount_sql);
    let mut amount_q = sqlx::query_scalar::<_, f64>(&total_amount_query_str);
    for val in &binds {
        amount_q = amount_q.bind(val);
    }
    let total_amount = amount_q.fetch_one(&state.db.pool).await.unwrap_or(0.0);

    let payment_methods_sql = "SELECT DISTINCT o.payment_method FROM orders o JOIN users u ON o.user_id = u.id WHERE o.payment_method IS NOT NULL AND o.payment_method != ''";
    let raw_methods: Vec<String> =
        sqlx::query_scalar(&state.db.format_query(payment_methods_sql))
            .fetch_all(&state.db.pool)
            .await
            .unwrap_or_default();
    let mut payment_methods: Vec<String> = Vec::new();
    for m in raw_methods {
        let normalized = match m.as_str() {
            "allinpay wechat" => "allinpay_wechat".to_string(),
            "allinpay alipay" => "allinpay_alipay".to_string(),
            _ => m,
        };
        if !payment_methods.contains(&normalized) {
            payment_methods.push(normalized);
        }
    }

    Ok(Json(FinanceOrderResponse {
        data,
        total,
        total_amount,
        payment_methods,
    }))
}

pub async fn list_recharge_types(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<String>>> {
    let types: Vec<String> = sqlx::query_scalar(&state.db.format_query(
        "SELECT DISTINCT recharge_type FROM recharge_records \
         WHERE recharge_type IS NOT NULL \
           AND COALESCE(wallet_type, 'system') = 'system' \
           AND recharge_type NOT IN ('gift', 'registration', 'commission')",
    ))
    .fetch_all(&state.db.pool)
    .await?;

    Ok(Json(types))
}

// ========== 数据分析 ==========

#[derive(Debug, Deserialize, Clone)]
pub struct FinanceDataAnalysisQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct DailyStat {
    pub date: String,
    #[sqlx(default)]
    pub total_requests: i64,
    #[sqlx(default)]
    pub total_tokens: i64,
    #[sqlx(default)]
    pub total_cost: f64,
    #[sqlx(default)]
    pub active_tokens: i64,
    #[sqlx(default)]
    pub active_users: i64,
    #[sqlx(default)]
    pub new_users: i64,
    #[sqlx(default)]
    pub total_users: i64,
    #[sqlx(default)]
    pub system_recharge_total: f64,
    #[sqlx(default)]
    pub system_cost_total: f64,
    #[sqlx(default)]
    pub system_retained_total: f64,
    #[sqlx(default)]
    pub online_recharge: f64,
    #[sqlx(default)]
    pub system_cost: f64,
    #[sqlx(default)]
    pub daily_system_balance: f64,
    #[sqlx(default)]
    pub daily_gift_recharge: f64,
    #[sqlx(default)]
    pub daily_system_recharge: f64,
}

#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct FinanceModelStat {
    pub model: String,
    pub count: i64,
    #[sqlx(default)]
    pub total_cost: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinanceModelDailyStatInfo {
    pub date: String,
    pub count: i64,
    pub total_cost: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinanceModelStatWithHistory {
    pub model: String,
    pub count: i64,
    pub total_cost: f64,
    pub last_three_days: Vec<FinanceModelDailyStatInfo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinanceAnalysisResponse {
    pub daily_stats: Vec<DailyStat>,
    pub model_stats: Vec<FinanceModelStatWithHistory>,
}

static DAILY_STATS_CACHE: OnceLock<DashMap<String, (Instant, FinanceAnalysisResponse)>> =
    OnceLock::new();

fn get_daily_stats_cache() -> &'static DashMap<String, (Instant, FinanceAnalysisResponse)> {
    DAILY_STATS_CACHE.get_or_init(|| DashMap::new())
}

pub async fn get_daily_stats(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FinanceDataAnalysisQuery>,
) -> AppResult<Json<FinanceAnalysisResponse>> {
    // 财务日统计与 usage_daily_stats 归档一致：锁定站点运营时区，不受管理展示时区影响
    let tz = crate::api::date_helper::resolve_site_operation_timezone(&state.db).await;
    let cache_key = format!(
        "{}_{}_{}",
        tz.name(),
        query.start_date.as_deref().unwrap_or(""),
        query.end_date.as_deref().unwrap_or("")
    );

    // Stale-While-Revalidate (SWR) 缓存验证机制：保障财务图表永远秒开
    if let Some(entry) = get_daily_stats_cache().get(&cache_key) {
        let (timestamp, cached_res) = entry.value();
        if timestamp.elapsed() < Duration::from_secs(300) {
            return Ok(Json(cached_res.clone()));
        } else {
            // 缓存已过期，释放读锁，尝试通过写锁抢占“重算令牌”以防止并发击穿
            drop(entry);

            if let Some(mut write_entry) = get_daily_stats_cache().get_mut(&cache_key) {
                if write_entry.value().0.elapsed() >= Duration::from_secs(300) {
                    // 更新时间戳，延长生命周期防止并发请求重复触发后台计算
                    write_entry.value_mut().0 = Instant::now();

                    let state_clone = state.clone();
                    let query_clone = query.clone();
                    let cache_key_clone = cache_key.clone();
                    let tz_clone = tz.clone();

                    tokio::spawn(async move {
                        match calculate_finance_stats(state_clone, query_clone, tz_clone).await {
                            Ok(new_res) => {
                                get_daily_stats_cache()
                                    .insert(cache_key_clone, (Instant::now(), new_res));
                                tracing::info!("✅ [SWR] 后台异步更新财务统计缓存成功");
                            }
                            Err(e) => {
                                tracing::warn!("❌ [SWR] 后台异步更新财务统计缓存失败: {:?}", e);
                            }
                        }
                    });
                }
            }

            // 立即秒回缓存中的旧数据，完全消灭被卡死的等待体验
            if let Some(entry) = get_daily_stats_cache().get(&cache_key) {
                return Ok(Json(entry.value().1.clone()));
            }
        }
    }

    // 缓存首次加载时同步计算
    let stats = calculate_finance_stats(state.clone(), query, tz).await?;
    get_daily_stats_cache().insert(cache_key, (Instant::now(), stats.clone()));
    Ok(Json(stats))
}

/// 采用 Lambda 增量聚合架构（历史汇总表 + 今日日志表分段合并）的高性能财务分析核心函数
async fn calculate_finance_stats(
    state: Arc<AppState>,
    query: FinanceDataAnalysisQuery,
    tz: chrono_tz::Tz,
) -> AppResult<FinanceAnalysisResponse> {
    use chrono::{Duration, NaiveDate};
    let bounds = crate::api::date_helper::get_timezone_time_bounds(tz);
    let today_date = bounds.today;

    let start_naive = crate::api::date_helper::parse_to_naive_date(
        query.start_date.as_deref(),
        NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
        tz,
    );

    let slices = crate::api::date_helper::calculate_query_slices(
        query.start_date.as_deref(),
        query.end_date.as_deref(),
        tz,
    );

    let mut daily_stats_raw: Vec<DailyStat> = Vec::new();

    // 1. 加载历史每日趋势统计 (从 usage_daily_stats 秒级获取)
    if slices.has_history_days {
        let sql = format!(
            "SELECT 
                stat_date::text as date, 
                CAST(SUM(total_requests) AS BIGINT) as total_requests,
                CAST(SUM(total_tokens) AS BIGINT) as total_tokens,
                SUM(total_cost) as total_cost,
                COUNT(DISTINCT token_id) FILTER (WHERE token_id != -1) as active_tokens,
                COUNT(DISTINCT user_id) as active_users,
                SUM(GREATEST(total_cost - total_pre_deduct_gift - COALESCE(total_pre_deduct_voucher, 0.0), 0.0)) as system_cost
             FROM usage_daily_stats
             WHERE {}
             GROUP BY stat_date
             ORDER BY stat_date ASC",
            slices.history_cond("stat_date")
        );
        let hist_rows: Vec<DailyStat> = sqlx::query_as(&state.db.format_query(&sql))
            .bind(slices.hist_start_date)
            .bind(slices.hist_end_date)
            .fetch_all(&state.db.pool)
            .await?;

        daily_stats_raw.extend(hist_rows);
    }

    // 2. 加载实时段及碎片趋势
    for r_slice in slices.realtime_slices() {
        let sql = format!(
            "
            SELECT 
                COUNT(*),
                (COALESCE(SUM(prompt_tokens), 0) + COALESCE(SUM(completion_tokens), 0))::bigint,
                COALESCE(SUM(cost), 0.0),
                COUNT(DISTINCT token_id),
                COUNT(DISTINCT user_id),
                COALESCE(SUM(GREATEST(cost - pre_deduct_gift - COALESCE(pre_deduct_voucher, 0.0), 0.0)), 0.0)
            FROM logs
            WHERE {}
        ",
            r_slice.sql_cond("created_at")
        );
        let formatted_sql = state.db.format_query(&sql);
        let (reqs, toks, cost, active_t, active_u, sys_c) =
            sqlx::query_as::<_, (i64, i64, f64, i64, i64, f64)>(&formatted_sql)
                .bind(&r_slice.start)
                .bind(&r_slice.end)
                .fetch_one(&state.db.pool)
                .await?;

        let date_str = r_slice.start[..10].to_string();
        daily_stats_raw.push(DailyStat {
            date: date_str,
            total_requests: reqs,
            total_tokens: toks,
            total_cost: cost,
            active_tokens: active_t,
            active_users: active_u,
            system_cost: sys_c,
            new_users: 0,
            total_users: 0,
            system_recharge_total: 0.0,
            system_cost_total: 0.0,
            system_retained_total: 0.0,
            online_recharge: 0.0,
            daily_system_balance: 0.0,
            daily_gift_recharge: 0.0,
            daily_system_recharge: 0.0,
        });
    }

    // end_ts 为半开上界，配合 `<` 使用（勿再用 `<=`）
    let start_ts = query
        .start_date
        .as_deref()
        .map(|s| crate::api::date_helper::parse_timestamptz_bind(s, false, tz));
    let end_ts = query
        .end_date
        .as_deref()
        .map(|e| crate::api::date_helper::parse_timestamptz_bind(e, true, tz));

    // 第三步：获取起始时间前的平台总用户数（Base Users）
    let mut base_users_sql = "SELECT COUNT(*) FROM users".to_string();
    let mut base_binds = Vec::new();
    if let Some(ref start) = start_ts {
        base_users_sql.push_str(" WHERE created_at < ?::timestamptz");
        base_binds.push(start.clone());
    }
    let formatted_base = state.db.format_query(&base_users_sql);
    let mut base_q = sqlx::query_scalar::<_, i64>(&formatted_base);
    for val in &base_binds {
        base_q = base_q.bind(val);
    }
    let mut cumulative_users = base_q.fetch_one(&state.db.pool).await.unwrap_or(0);

    // 第四步：获取每天的新增用户数（按查询 timedisplay 自然日分桶）
    let date_bucket = crate::api::date_helper::sql_date_bucket("created_at", tz);
    let mut new_users_sql =
        format!("SELECT {date_bucket} as date, COUNT(*) as new_users FROM users WHERE 1=1");
    let mut new_users_binds = Vec::new();
    if let Some(ref start) = start_ts {
        new_users_sql.push_str(" AND created_at >= ?::timestamptz");
        new_users_binds.push(start.clone());
    }
    if let Some(ref end) = end_ts {
        new_users_sql.push_str(" AND created_at < ?::timestamptz");
        new_users_binds.push(end.clone());
    }
    new_users_sql.push_str(&format!(" GROUP BY {date_bucket}"));

    #[derive(sqlx::FromRow)]
    struct NewUserStat {
        date: String,
        new_users: i64,
    }
    let formatted_nu = state.db.format_query(&new_users_sql);
    let mut nu_q = sqlx::query_as::<_, NewUserStat>(&formatted_nu);
    for val in &new_users_binds {
        nu_q = nu_q.bind(val);
    }
    let new_users_list = nu_q.fetch_all(&state.db.pool).await.unwrap_or_default();

    // 第五步：获取充值数据（基础充值 + 每日充值）
    let mut base_recharge_sql =
        "SELECT COALESCE(SUM(amount), 0.0) FROM recharge_records WHERE COALESCE(recharge_type, '') NOT IN ('model_voucher', 'coupon_purchase', 'coupon_pack', 'coupon_subscription', 'ark_video_consume', 'ark_video_refund')"
            .to_string();
    let mut base_recharge_binds = Vec::new();
    if let Some(ref start) = start_ts {
        base_recharge_sql.push_str(" AND created_at < ?::timestamptz");
        base_recharge_binds.push(start.clone());
    }
    let formatted_base_recharge = state.db.format_query(&base_recharge_sql);
    let mut br_q = sqlx::query_scalar::<_, f64>(&formatted_base_recharge);
    for val in &base_recharge_binds {
        br_q = br_q.bind(val);
    }
    let mut cumulative_recharge = br_q.fetch_one(&state.db.pool).await.unwrap_or(0.0);

    // 历史累计消费 base_cost 100% 属于历史段，直接从小落地表统计，消灭在 logs 上的超慢全表扫描
    let mut base_cost_sql =
        "SELECT COALESCE(SUM(total_cost), 0.0) FROM usage_daily_stats".to_string();
    let mut base_cost_binds = Vec::new();
    if query.start_date.is_some() {
        base_cost_sql.push_str(" WHERE stat_date < ?");
        base_cost_binds.push(start_naive);
    }
    let formatted_base_cost = state.db.format_query(&base_cost_sql);
    let mut bc_q = sqlx::query_scalar::<_, f64>(&formatted_base_cost);
    for val in &base_cost_binds {
        bc_q = bc_q.bind(val);
    }
    let mut cumulative_cost = bc_q.fetch_one(&state.db.pool).await.unwrap_or(0.0);

    let mut daily_wallet_recharge_sql = format!(
        "
        SELECT 
            {date_bucket} as date, 
            CASE WHEN wallet_type = 'gift' THEN 'gift' ELSE 'system' END as wallet_group,
            COALESCE(SUM(amount), 0.0) as amount
        FROM recharge_records 
        WHERE (
            (wallet_type = 'gift')
            OR (wallet_type = 'system' AND recharge_type = 'manual')
            OR (recharge_type IN ('wechat', 'alipay', 'stripe', 'bonuspay'))
        )
        AND COALESCE(recharge_type, '') NOT IN ('ark_video_consume', 'ark_video_refund')
    "
    );
    let mut daily_wallet_recharge_binds = Vec::new();
    if let Some(ref start) = start_ts {
        daily_wallet_recharge_sql.push_str(" AND created_at >= ?::timestamptz");
        daily_wallet_recharge_binds.push(start.clone());
    }
    if let Some(ref end) = end_ts {
        daily_wallet_recharge_sql.push_str(" AND created_at < ?::timestamptz");
        daily_wallet_recharge_binds.push(end.clone());
    }
    daily_wallet_recharge_sql.push_str(&format!(
        " GROUP BY {date_bucket}, CASE WHEN wallet_type = 'gift' THEN 'gift' ELSE 'system' END"
    ));

    #[derive(sqlx::FromRow)]
    struct DailyWalletRechargeStat {
        date: String,
        wallet_group: String,
        amount: f64,
    }
    let formatted_dwr = state.db.format_query(&daily_wallet_recharge_sql);
    let mut dwr_q = sqlx::query_as::<_, DailyWalletRechargeStat>(&formatted_dwr);
    for val in &daily_wallet_recharge_binds {
        dwr_q = dwr_q.bind(val);
    }
    let daily_wallet_recharges = dwr_q.fetch_all(&state.db.pool).await.unwrap_or_default();

    let mut daily_sys_recharge_map = std::collections::HashMap::new();
    let mut daily_gift_recharge_map = std::collections::HashMap::new();
    for dwr in daily_wallet_recharges {
        if dwr.wallet_group == "gift" {
            daily_gift_recharge_map.insert(dwr.date.clone(), dwr.amount);
        } else {
            *daily_sys_recharge_map
                .entry(dwr.date.clone())
                .or_insert(0.0) += dwr.amount;
        }
    }

    let mut daily_recharge_sql = format!(
        "
        SELECT 
            {date_bucket} as date, 
            COALESCE(SUM(amount), 0.0) as amount,
            COALESCE(SUM(CASE WHEN recharge_type IN ('wechat', 'alipay', 'stripe', 'bonuspay') THEN amount ELSE 0.0 END), 0.0) as online_amount
        FROM recharge_records WHERE COALESCE(recharge_type, '') NOT IN ('model_voucher', 'coupon_purchase', 'coupon_pack', 'coupon_subscription', 'ark_video_consume', 'ark_video_refund')
    "
    );
    let mut daily_recharge_binds = Vec::new();
    if let Some(ref start) = start_ts {
        daily_recharge_sql.push_str(" AND created_at >= ?::timestamptz");
        daily_recharge_binds.push(start.clone());
    }
    if let Some(ref end) = end_ts {
        daily_recharge_sql.push_str(" AND created_at < ?::timestamptz");
        daily_recharge_binds.push(end.clone());
    }
    daily_recharge_sql.push_str(&format!(" GROUP BY {date_bucket}"));

    #[derive(sqlx::FromRow)]
    struct DailyRechargeStat {
        date: String,
        amount: f64,
        online_amount: f64,
    }
    let formatted_dr = state.db.format_query(&daily_recharge_sql);
    let mut dr_q = sqlx::query_as::<_, DailyRechargeStat>(&formatted_dr);
    for val in &daily_recharge_binds {
        dr_q = dr_q.bind(val);
    }
    let daily_recharges = dr_q.fetch_all(&state.db.pool).await.unwrap_or_default();

    // 合并数据
    let mut stats_map: std::collections::BTreeMap<String, DailyStat> =
        std::collections::BTreeMap::new();
    for stat in daily_stats_raw {
        stats_map.insert(stat.date.clone(), stat);
    }

    for nu_stat in new_users_list {
        let stat = stats_map
            .entry(nu_stat.date.clone())
            .or_insert_with(|| DailyStat {
                date: nu_stat.date,
                total_requests: 0,
                total_tokens: 0,
                total_cost: 0.0,
                active_tokens: 0,
                active_users: 0,
                new_users: 0,
                total_users: 0,
                system_recharge_total: 0.0,
                system_cost_total: 0.0,
                system_retained_total: 0.0,
                online_recharge: 0.0,
                system_cost: 0.0,
                daily_system_balance: 0.0,
                daily_gift_recharge: 0.0,
                daily_system_recharge: 0.0,
            });
        stat.new_users = nu_stat.new_users;
    }

    let mut daily_recharge_map = std::collections::HashMap::new();
    for dr in daily_recharges {
        daily_recharge_map.insert(dr.date.clone(), (dr.amount, dr.online_amount));
        stats_map
            .entry(dr.date.clone())
            .or_insert_with(|| DailyStat {
                date: dr.date,
                total_requests: 0,
                total_tokens: 0,
                total_cost: 0.0,
                active_tokens: 0,
                active_users: 0,
                new_users: 0,
                total_users: 0,
                system_recharge_total: 0.0,
                system_cost_total: 0.0,
                system_retained_total: 0.0,
                online_recharge: 0.0,
                system_cost: 0.0,
                daily_system_balance: 0.0,
                daily_gift_recharge: 0.0,
                daily_system_recharge: 0.0,
            });
    }

    let mut daily_stats: Vec<DailyStat> = stats_map.into_values().collect();
    for stat in daily_stats.iter_mut() {
        cumulative_users += stat.new_users;
        stat.total_users = cumulative_users;

        let (dr, online_dr) = daily_recharge_map
            .get(&stat.date)
            .copied()
            .unwrap_or((0.0, 0.0));
        cumulative_recharge += dr;
        cumulative_cost += stat.total_cost;

        stat.system_recharge_total = cumulative_recharge;
        stat.system_cost_total = cumulative_cost;
        stat.system_retained_total = cumulative_recharge - cumulative_cost;
        stat.online_recharge = online_dr;

        stat.daily_system_recharge = daily_sys_recharge_map
            .get(&stat.date)
            .copied()
            .unwrap_or(0.0);
        stat.daily_gift_recharge = daily_gift_recharge_map
            .get(&stat.date)
            .copied()
            .unwrap_or(0.0);
    }

    // 第二个查询：按模型汇总统计（调用公共高可用聚合 API）
    let slice_model_map =
        crate::relay::usage_stats::query_model_stats_by_slices(&state.db, None, &slices).await?;

    let mut model_stats: Vec<FinanceModelStat> = slice_model_map
        .into_iter()
        .map(|(model, (count, total_cost, _))| FinanceModelStat {
            model,
            count,
            total_cost,
        })
        .collect();
    model_stats.sort_by(|a, b| {
        b.total_cost
            .partial_cmp(&a.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let model_stats_top10: Vec<FinanceModelStat> = model_stats.into_iter().take(10).collect();

    // 计算最近 3 天的每日数据 (前天, 昨天, 今天)
    let day_0 = today_date;
    let day_1 = day_0 - Duration::days(1);
    let day_2 = day_0 - Duration::days(2);
    let date_str_0 = day_0.format("%Y-%m-%d").to_string();
    let date_str_1 = day_1.format("%Y-%m-%d").to_string();
    let date_str_2 = day_2.format("%Y-%m-%d").to_string();

    // 分别计算这三天的时区切片，高精度提取每天的财务模型消耗统计
    let slices_0 =
        crate::api::date_helper::calculate_query_slices(Some(&date_str_0), Some(&date_str_0), tz);
    let stats_0 =
        crate::relay::usage_stats::query_model_stats_by_slices(&state.db, None, &slices_0).await?;

    let slices_1 =
        crate::api::date_helper::calculate_query_slices(Some(&date_str_1), Some(&date_str_1), tz);
    let stats_1 =
        crate::relay::usage_stats::query_model_stats_by_slices(&state.db, None, &slices_1).await?;

    let slices_2 =
        crate::api::date_helper::calculate_query_slices(Some(&date_str_2), Some(&date_str_2), tz);
    let stats_2 =
        crate::relay::usage_stats::query_model_stats_by_slices(&state.db, None, &slices_2).await?;

    let mut model_stats_with_history = Vec::new();
    for m in model_stats_top10 {
        let mut last_three_days = Vec::new();
        for (target_date, day_stats) in [
            (date_str_2.clone(), &stats_2),
            (date_str_1.clone(), &stats_1),
            (date_str_0.clone(), &stats_0),
        ] {
            let found = day_stats.get(&m.model);
            last_three_days.push(FinanceModelDailyStatInfo {
                date: target_date,
                count: found.map(|f| f.0).unwrap_or(0),
                total_cost: found.map(|f| f.1).unwrap_or(0.0),
            });
        }

        model_stats_with_history.push(FinanceModelStatWithHistory {
            model: m.model,
            count: m.count,
            total_cost: m.total_cost,
            last_three_days,
        });
    }

    let response = FinanceAnalysisResponse {
        daily_stats,
        model_stats: model_stats_with_history,
    };

    Ok(response)
}

#[derive(Debug, Deserialize)]

pub struct WalletStatsBatchRequest {
    pub user_ids: Vec<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct WalletStatsResponseItem {
    pub recharge_amount: f64,
    pub gift_amount: f64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct WalletStatsBatchRawRow {
    pub user_id: String,
    pub wallet_type: Option<String>,
    pub amount: f64,
}

pub async fn get_wallet_stats_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WalletStatsBatchRequest>,
) -> AppResult<Json<std::collections::HashMap<String, WalletStatsResponseItem>>> {
    if req.user_ids.is_empty() {
        return Ok(Json(std::collections::HashMap::new()));
    }

    let placeholders = req
        .user_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    // 排除方舟监控扣费/退款流水，避免把 ark_video_* 算进「充值」
    let mut sql = format!(
        "SELECT user_id, wallet_type, COALESCE(SUM(amount), 0.0) as amount \
         FROM recharge_records \
         WHERE user_id IN ({}) \
         AND COALESCE(recharge_type, '') NOT IN ('ark_video_consume', 'ark_video_refund', 'coupon_purchase', 'coupon_pack', 'coupon_subscription')",
        placeholders
    );
    let mut binds: Vec<String> = req.user_ids.clone();

    if let Some(ref start) = req.start_date {
        crate::api::date_helper::push_timestamptz_bound_default(
            &mut sql,
            &mut binds,
            "created_at",
            start,
            false,
        );
    }
    if let Some(ref end) = req.end_date {
        crate::api::date_helper::push_timestamptz_bound_default(
            &mut sql,
            &mut binds,
            "created_at",
            end,
            true,
        );
    }
    sql.push_str(" GROUP BY user_id, wallet_type");

    let formatted_sql = state.db.format_query(&sql);
    let mut query = sqlx::query_as::<_, WalletStatsBatchRawRow>(&formatted_sql);
    for bind in binds {
        query = query.bind(bind);
    }

    let rows = query.fetch_all(&state.db.pool).await?;

    let mut result: std::collections::HashMap<String, WalletStatsResponseItem> =
        std::collections::HashMap::new();
    for row in rows {
        let entry = result
            .entry(row.user_id.clone())
            .or_insert_with(WalletStatsResponseItem::default);
        if let Some(wt) = row.wallet_type {
            if wt == "system" {
                entry.recharge_amount += row.amount;
            } else if wt == "gift" {
                entry.gift_amount += row.amount;
            }
        }
    }

    Ok(Json(result))
}

// ========== 佣金明细（佣金钱包流水 + 系统佣金赠送）==========

#[derive(Debug, Deserialize)]
pub struct CommissionListQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub user_id: Option<String>,
    pub source: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FinanceCommissionRecord {
    pub id: String,
    pub amount: f64,
    pub ratio: Option<f64>,
    pub source: String,
    pub wallet: String,
    pub created_at: DbTs,
    pub username: String,
    pub uid: String,
    pub from_username: Option<String>,
    pub from_uid: Option<String>,
    pub operator: Option<String>,
    pub remark: Option<String>,
    pub order_no: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FinanceCommissionResponse {
    pub data: Vec<FinanceCommissionRecord>,
    pub total: i64,
    pub total_amount: f64,
}

fn commission_source_allowed(source: &str) -> bool {
    matches!(
        source,
        "consumption"
            | "recharge"
            | "system_award"
            | "invite_gift"
            | "transfer"
            | "video"
            | "refund"
            | "clawback"
            | "system_clawback"
            | "other"
    )
}

/// 佣金钱包流水，加上系统侧不进 `commissions` 的邀请赠送、结转、系统钱包扣回。
fn commission_ledger_from() -> &'static str {
    r#"FROM (
        SELECT
            ('c' || c.id::text) AS id,
            c.amount::float8 AS amount,
            c.ratio::float8 AS ratio,
            CASE
                WHEN COALESCE(c.source_key, '') LIKE 'adjustclaw:%'
                    OR COALESCE(c.source_key, '') LIKE 'claw:%' THEN 'clawback'
                WHEN COALESCE(c.source_key, '') LIKE 'daycons:%'
                    OR COALESCE(c.source_key, '') LIKE 'log:%' THEN 'consumption'
                WHEN COALESCE(c.source_key, '') LIKE 'arkrefund:%'
                    OR (c.amount < 0 AND COALESCE(c.source_key, '') LIKE 'ark:%') THEN 'refund'
                WHEN COALESCE(c.source_key, '') LIKE 'ark:%' THEN 'video'
                WHEN COALESCE(c.source_key, '') LIKE 'recharge:%' AND rr.recharge_type = 'manual' THEN 'system_award'
                WHEN COALESCE(c.source_key, '') LIKE 'recharge:%' THEN 'recharge'
                WHEN c.amount < 0 THEN 'refund'
                ELSE 'other'
            END AS source,
            'commission'::text AS wallet,
            c.created_at::timestamptz AS created_at,
            beneficiary.username AS username,
            beneficiary.uid AS uid,
            src.username AS from_username,
            src.uid AS from_uid,
            rr.operator AS operator,
            rr.remark AS remark,
            rr.order_no AS order_no
        FROM commissions c
        JOIN users beneficiary ON beneficiary.id = c.user_id
        LEFT JOIN users src ON src.id = c.from_user_id
        LEFT JOIN recharge_records rr ON rr.id = c.recharge_id
        UNION ALL
        SELECT
            ('r' || rr.id::text) AS id,
            rr.amount::float8 AS amount,
            NULL::float8 AS ratio,
            CASE
                WHEN rr.recharge_type = 'commission' THEN 'invite_gift'
                WHEN rr.recharge_type = 'transfer' THEN 'transfer'
                ELSE 'system_clawback'
            END AS source,
            CASE
                WHEN rr.recharge_type = 'commission' THEN 'gift'
                ELSE 'system'
            END AS wallet,
            rr.created_at::timestamptz AS created_at,
            u.username AS username,
            u.uid AS uid,
            NULL::text AS from_username,
            NULL::text AS from_uid,
            rr.operator AS operator,
            rr.remark AS remark,
            rr.order_no AS order_no
        FROM recharge_records rr
        JOIN users u ON u.id = rr.user_id
        WHERE rr.recharge_type IN ('commission', 'transfer')
           OR (
                rr.recharge_type = 'manual'
                AND rr.remark LIKE '%返佣差额从系统钱包扣回'
           )
    ) q"#
}

pub async fn list_commissions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CommissionListQuery>,
) -> AppResult<Json<FinanceCommissionResponse>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 200);
    let offset = (page - 1) * per_page;

    let from_sql = commission_ledger_from();
    let mut where_clause = " WHERE 1=1".to_string();
    let mut binds: Vec<String> = Vec::new();

    if let Some(search) = query
        .user_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        where_clause.push_str(
            " AND (q.username LIKE ? OR q.uid = ? OR q.uid LIKE ? OR COALESCE(q.from_username, '') LIKE ? OR COALESCE(q.from_uid, '') = ? OR COALESCE(q.order_no, '') LIKE ? OR COALESCE(q.remark, '') LIKE ?)",
        );
        let like = format!("%{search}%");
        binds.push(like.clone());
        binds.push(search.to_string());
        binds.push(like.clone());
        binds.push(like.clone());
        binds.push(search.to_string());
        binds.push(like.clone());
        binds.push(like);
    }

    if let Some(source) = query
        .source
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| commission_source_allowed(s))
    {
        where_clause.push_str(" AND q.source = ?");
        binds.push(source.to_string());
    }

    let tz = finance_filter_tz(&state).await;
    if let Some(ref start) = query.start_time {
        crate::api::date_helper::push_timestamptz_bound(
            &mut where_clause,
            &mut binds,
            "q.created_at",
            start,
            false,
            tz,
        );
    }
    if let Some(ref end) = query.end_time {
        crate::api::date_helper::push_timestamptz_bound(
            &mut where_clause,
            &mut binds,
            "q.created_at",
            end,
            true,
            tz,
        );
    }

    let count_sql = format!("SELECT COUNT(*) {from_sql}{where_clause}");
    let count_sql = state.db.format_query(&count_sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    for val in &binds {
        count_q = count_q.bind(val);
    }
    let total = count_q.fetch_one(&state.db.pool).await.map_err(|e| {
        tracing::warn!("Finance commissions count error: {:?}", e);
        e
    })?;

    let total_amount_sql =
        format!("SELECT COALESCE(SUM(q.amount), 0.0)::float8 {from_sql}{where_clause}");
    let total_amount_sql = state.db.format_query(&total_amount_sql);
    let mut amount_q = sqlx::query_scalar::<_, f64>(&total_amount_sql);
    for val in &binds {
        amount_q = amount_q.bind(val);
    }
    let total_amount = amount_q.fetch_one(&state.db.pool).await.unwrap_or(0.0);

    let data_sql = format!(
        "SELECT q.* {from_sql}{where_clause} ORDER BY q.created_at DESC, q.id DESC LIMIT {per_page} OFFSET {offset}"
    );
    let data_sql = state.db.format_query(&data_sql);
    let mut data_q = sqlx::query_as::<_, FinanceCommissionRecord>(&data_sql);
    for val in &binds {
        data_q = data_q.bind(val);
    }
    let data = data_q.fetch_all(&state.db.pool).await.map_err(|e| {
        tracing::warn!("Finance commissions data error: {:?}", e);
        e
    })?;

    Ok(Json(FinanceCommissionResponse {
        data,
        total,
        total_amount,
    }))
}
