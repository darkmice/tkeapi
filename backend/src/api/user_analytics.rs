/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::api::date_helper::{self, calculate_query_slices};
use crate::error::AppResult;
use crate::AppState;

const TOP_N: usize = 10;
/// 月注册图固定最近 12 个自然月（含本月）。排行与每日注册仍用 TOP_N。
const MONTHLY_REG_MONTHS: i32 = 12;

#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct DateCount {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct RankItem {
    pub name: String,
    pub amount: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct UserAnalyticsResponse {
    pub daily_registrations: Vec<DateCount>,
    pub monthly_registrations: Vec<DateCount>,
    pub daily_consumption_rank: Vec<RankItem>,
    pub monthly_consumption_rank: Vec<RankItem>,
    pub monthly_recharge_rank: Vec<RankItem>,
    pub total_recharge_rank: Vec<RankItem>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AmountRow {
    user_id: String,
    amount: f64,
}

#[derive(Debug, Clone)]
struct RankDraft {
    user_id: String,
    name: String,
    amount: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct NameRow {
    id: String,
    name: String,
}

fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let idx = year * 12 + (month as i32 - 1) + delta;
    let y = idx.div_euclid(12);
    let m = idx.rem_euclid(12) as u32 + 1;
    (y, m)
}

fn day_keys(end: NaiveDate, n: i64) -> Vec<String> {
    let start = end - Duration::days(n - 1);
    (0..n)
        .map(|i| (start + Duration::days(i)).format("%Y-%m-%d").to_string())
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct AnalyticsQuery {
    /// 月排行月份，格式 YYYY-MM。缺省或非法时用请求时区的本月。
    pub month: Option<String>,
}

/// 月消费、月充值排行的闭区间。未来月份收回到本月，本月终点截到今天。
fn resolve_rank_month(raw: Option<&str>, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let current = month_range(today.year(), today.month(), today);
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return current;
    };
    let mut parts = raw.split('-');
    let year = parts.next().and_then(|s| s.parse::<i32>().ok());
    let month = parts.next().and_then(|s| s.parse::<u32>().ok());
    if parts.next().is_some() {
        return current;
    }
    let (Some(year), Some(month)) = (year, month) else {
        return current;
    };
    if !(1..=12).contains(&month) || !(2000..=2100).contains(&year) {
        return current;
    }
    let Some(start) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return current;
    };
    if start > today {
        return current;
    }
    month_range(year, month, today)
}

fn month_range(year: i32, month: u32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(today);
    let (next_y, next_m) = shift_month(year, month, 1);
    let next = NaiveDate::from_ymd_opt(next_y, next_m, 1).unwrap_or(start);
    let end = (next - Duration::days(1)).min(today);
    (start, if end < start { start } else { end })
}

fn month_keys(today: NaiveDate, n: i32) -> Vec<String> {
    let (mut y, mut m) = shift_month(today.year(), today.month(), -(n - 1));
    let mut keys = Vec::with_capacity(n as usize);
    for _ in 0..n {
        keys.push(format!("{y:04}-{m:02}"));
        (y, m) = shift_month(y, m, 1);
    }
    keys
}

fn fill_counts(keys: &[String], rows: Vec<DateCount>) -> Vec<DateCount> {
    let map: HashMap<String, i64> = rows.into_iter().map(|r| (r.date, r.count)).collect();
    keys.iter()
        .map(|date| DateCount {
            date: date.clone(),
            count: map.get(date).copied().unwrap_or(0),
        })
        .collect()
}

/// 先按用户合并各段金额，再截取前 N。不能先截断再合并，否则跨日用户会被低估。
fn merge_amounts(parts: impl IntoIterator<Item = Vec<AmountRow>>) -> Vec<AmountRow> {
    let mut map: HashMap<String, f64> = HashMap::new();
    for part in parts {
        for row in part {
            if !row.amount.is_finite() || row.amount <= 0.0 {
                continue;
            }
            *map.entry(row.user_id).or_insert(0.0) += row.amount;
        }
    }
    let mut rows: Vec<AmountRow> = map
        .into_iter()
        .filter(|(_, amount)| *amount > 0.0)
        .map(|(user_id, amount)| AmountRow { user_id, amount })
        .collect();
    rows.sort_by(|a, b| {
        b.amount
            .partial_cmp(&a.amount)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.user_id.cmp(&b.user_id))
    });
    rows
}

fn top_amounts(rows: Vec<AmountRow>, limit: usize) -> Vec<AmountRow> {
    rows.into_iter().take(limit).collect()
}

fn label_ranks(rows: Vec<RankDraft>) -> Vec<RankItem> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for row in &rows {
        *counts.entry(row.name.clone()).or_insert(0) += 1;
    }
    rows.into_iter()
        .map(|row| {
            let dup = counts.get(&row.name).copied().unwrap_or(0) > 1;
            let name = if row.name.is_empty() {
                row.user_id
            } else if dup {
                let short: String = row.user_id.chars().take(8).collect();
                format!("{} ({})", row.name, short)
            } else {
                row.name
            };
            RankItem {
                name,
                amount: row.amount,
            }
        })
        .collect()
}

fn attach_names(rows: Vec<AmountRow>, names: &HashMap<String, String>) -> Vec<RankItem> {
    let drafts = rows
        .into_iter()
        .map(|row| {
            let name = names
                .get(&row.user_id)
                .cloned()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| row.user_id.clone());
            RankDraft {
                user_id: row.user_id,
                name,
                amount: row.amount,
            }
        })
        .collect();
    label_ranks(drafts)
}

/// 在线支付与系统钱包手动充值。赠送金、兑换、注册奖励不计入充值。
fn paid_recharge_where(alias: &str) -> String {
    let p = if alias.is_empty() {
        String::new()
    } else {
        format!("{alias}.")
    };
    format!(
        "{p}amount > 0 AND COALESCE({p}wallet_type, 'system') = 'system' AND {p}recharge_type NOT IN ('gift', 'registration', 'commission', 'redemption', 'redemption_refund', 'model_voucher', 'coupon_purchase', 'coupon_pack', 'coupon_subscription', 'ark_video_consume', 'ark_video_refund')"
    )
}

fn sql_month_bucket(ts_expr: &str, tz: chrono_tz::Tz) -> String {
    format!(
        "TO_CHAR({} AT TIME ZONE '{}', 'YYYY-MM')",
        ts_expr,
        date_helper::sql_safe_tz_name(tz)
    )
}

fn range_ts(
    start_day: NaiveDate,
    end_day_inclusive: NaiveDate,
    tz: chrono_tz::Tz,
) -> (String, String) {
    let start =
        date_helper::parse_timestamptz_bind(&start_day.format("%Y-%m-%d").to_string(), false, tz);
    let end = date_helper::parse_timestamptz_bind(
        &end_day_inclusive.format("%Y-%m-%d").to_string(),
        true,
        tz,
    );
    (start, end)
}

async fn query_counts(
    db: &crate::db::Database,
    bucket_sql: &str,
    start_ts: &str,
    end_ts: &str,
) -> AppResult<Vec<DateCount>> {
    let sql = format!(
        "SELECT {bucket_sql} AS date, CAST(COUNT(*) AS BIGINT) AS count \
         FROM users \
         WHERE created_at >= ?::timestamptz AND created_at < ?::timestamptz \
         GROUP BY 1"
    );
    let rows = sqlx::query_as::<_, DateCount>(&db.format_query(&sql))
        .bind(start_ts)
        .bind(end_ts)
        .fetch_all(&db.pool)
        .await?;
    Ok(rows)
}

async fn query_recharge_rank(
    db: &crate::db::Database,
    start_ts: Option<&str>,
    end_ts: Option<&str>,
) -> AppResult<Vec<AmountRow>> {
    let pred = paid_recharge_where("r");
    let mut sql = format!(
        "SELECT r.user_id, COALESCE(SUM(r.amount), 0.0) AS amount \
         FROM recharge_records r \
         WHERE {pred}"
    );
    if start_ts.is_some() {
        sql.push_str(" AND r.created_at >= ?::timestamptz");
    }
    if end_ts.is_some() {
        sql.push_str(" AND r.created_at < ?::timestamptz");
    }
    sql.push_str(&format!(
        " GROUP BY r.user_id HAVING COALESCE(SUM(r.amount), 0.0) > 0 ORDER BY amount DESC LIMIT {TOP_N}"
    ));
    let f_sql = db.format_query(&sql);
    let mut q = sqlx::query_as::<_, AmountRow>(&f_sql);
    if let Some(start) = start_ts {
        q = q.bind(start);
    }
    if let Some(end) = end_ts {
        q = q.bind(end);
    }
    Ok(q.fetch_all(&db.pool).await?)
}

/// 消费口径与财务一致：历史天走 usage_daily_stats.total_cost，今日走 logs.cost。
async fn query_consumption_rank(
    db: &crate::db::Database,
    start_day: NaiveDate,
    end_day: NaiveDate,
    tz: chrono_tz::Tz,
) -> AppResult<Vec<AmountRow>> {
    let slices = calculate_query_slices(
        Some(&start_day.format("%Y-%m-%d").to_string()),
        Some(&end_day.format("%Y-%m-%d").to_string()),
        tz,
    );
    let mut parts: Vec<Vec<AmountRow>> = Vec::new();
    if let (true, Some(start), Some(end)) = (
        slices.has_history_days,
        slices.hist_start_date,
        slices.hist_end_date,
    ) {
        let sql = format!(
            "SELECT user_id, COALESCE(SUM(total_cost), 0.0) AS amount \
             FROM usage_daily_stats WHERE {} GROUP BY user_id",
            slices.history_cond("stat_date")
        );
        let rows = sqlx::query_as::<_, AmountRow>(&db.format_query(&sql))
            .bind(start)
            .bind(end)
            .fetch_all(&db.pool)
            .await?;
        parts.push(rows);
    }
    for slice in slices.realtime_slices() {
        let sql = format!(
            "SELECT user_id, COALESCE(SUM(cost), 0.0) AS amount \
             FROM logs WHERE {} GROUP BY user_id",
            slice.sql_cond("created_at")
        );
        let rows = sqlx::query_as::<_, AmountRow>(&db.format_query(&sql))
            .bind(&slice.start)
            .bind(&slice.end)
            .fetch_all(&db.pool)
            .await?;
        parts.push(rows);
    }
    Ok(top_amounts(merge_amounts(parts), TOP_N))
}

async fn load_names(
    db: &crate::db::Database,
    ids: &[String],
) -> AppResult<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, NameRow>(&db.format_query(
        "SELECT id, COALESCE(NULLIF(BTRIM(nickname), ''), NULLIF(BTRIM(username), ''), id) AS name \
         FROM users WHERE id = ANY(?)",
    ))
    .bind(ids.to_vec())
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.into_iter().map(|row| (row.id, row.name)).collect())
}

pub async fn get_user_analytics(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AnalyticsQuery>,
) -> AppResult<Json<UserAnalyticsResponse>> {
    let tz = crate::api::date_helper::resolve_site_operation_timezone(&state.db).await;
    let today = date_helper::get_timezone_time_bounds(tz).today;
    let days = day_keys(today, TOP_N as i64);
    let months = month_keys(today, MONTHLY_REG_MONTHS);
    let day_start = NaiveDate::parse_from_str(&days[0], "%Y-%m-%d").unwrap_or(today);
    let month_start = NaiveDate::parse_from_str(&format!("{}-01", months[0]), "%Y-%m-%d")
        .unwrap_or(NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today));
    let (rank_start, rank_end) = resolve_rank_month(query.month.as_deref(), today);

    let (day_start_ts, day_end_ts) = range_ts(day_start, today, tz);
    let (month_start_ts, month_end_ts) = range_ts(month_start, today, tz);
    let (rank_start_ts, rank_end_ts) = range_ts(rank_start, rank_end, tz);

    let day_bucket = date_helper::sql_date_bucket("created_at", tz);
    let month_bucket = sql_month_bucket("created_at", tz);

    let db = &state.db;
    let daily_reg = query_counts(db, &day_bucket, &day_start_ts, &day_end_ts).await?;
    let monthly_reg = query_counts(db, &month_bucket, &month_start_ts, &month_end_ts).await?;
    let daily_spend = query_consumption_rank(db, today, today, tz).await?;
    let monthly_spend = query_consumption_rank(db, rank_start, rank_end, tz).await?;
    let monthly_pay = query_recharge_rank(db, Some(&rank_start_ts), Some(&rank_end_ts)).await?;
    let total_pay = query_recharge_rank(db, None, None).await?;

    let mut ids: Vec<String> = Vec::new();
    for row in daily_spend
        .iter()
        .chain(monthly_spend.iter())
        .chain(monthly_pay.iter())
        .chain(total_pay.iter())
    {
        ids.push(row.user_id.clone());
    }
    ids.sort();
    ids.dedup();
    let names = load_names(db, &ids).await?;

    Ok(Json(UserAnalyticsResponse {
        daily_registrations: fill_counts(&days, daily_reg),
        monthly_registrations: fill_counts(&months, monthly_reg),
        daily_consumption_rank: attach_names(daily_spend, &names),
        monthly_consumption_rank: attach_names(monthly_spend, &names),
        monthly_recharge_rank: attach_names(monthly_pay, &names),
        total_recharge_rank: attach_names(total_pay, &names),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_month_defaults_and_clamps() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        assert_eq!(
            resolve_rank_month(None, today),
            (
                NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
            )
        );
        assert_eq!(
            resolve_rank_month(Some("2026-09"), today),
            (
                NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
            )
        );
        assert_eq!(resolve_rank_month(Some("2027-01"), today).0, today);
        assert_eq!(resolve_rank_month(Some("bad"), today).0, today);
    }

    #[test]
    fn month_keys_cross_year() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let keys = month_keys(today, 10);
        assert_eq!(keys.len(), 10);
        assert_eq!(keys[0], "2025-04");
        assert_eq!(keys[9], "2026-01");
    }

    #[test]
    fn monthly_registration_window_is_last_twelve_months() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let keys = month_keys(today, MONTHLY_REG_MONTHS);
        assert_eq!(keys.len(), 12);
        assert_eq!(keys[0], "2025-11");
        assert_eq!(keys[11], "2026-10");
    }

    #[test]
    fn day_keys_end_on_today() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        let keys = day_keys(today, 10);
        assert_eq!(keys.len(), 10);
        assert_eq!(keys[0], "2026-09-21");
        assert_eq!(keys[9], "2026-09-30");
    }

    #[test]
    fn fill_counts_inserts_zero_days() {
        let keys = vec!["2026-09-21".into(), "2026-09-22".into()];
        let filled = fill_counts(
            &keys,
            vec![DateCount {
                date: "2026-09-22".into(),
                count: 3,
            }],
        );
        assert_eq!(filled[0].count, 0);
        assert_eq!(filled[1].count, 3);
    }

    #[test]
    fn merge_amounts_sums_before_top_cut() {
        let merged = top_amounts(
            merge_amounts([
                vec![
                    AmountRow {
                        user_id: "a".into(),
                        amount: 6.0,
                    },
                    AmountRow {
                        user_id: "b".into(),
                        amount: 9.0,
                    },
                ],
                vec![AmountRow {
                    user_id: "a".into(),
                    amount: 5.0,
                }],
            ]),
            1,
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].user_id, "a");
        assert!((merged[0].amount - 11.0).abs() < 1e-9);
    }

    #[test]
    fn label_ranks_disambiguates_duplicate_names() {
        let rows = label_ranks(vec![
            RankDraft {
                user_id: "aaaa1111-x".into(),
                name: "张三".into(),
                amount: 2.0,
            },
            RankDraft {
                user_id: "bbbb2222-y".into(),
                name: "张三".into(),
                amount: 1.0,
            },
        ]);
        assert_eq!(rows[0].name, "张三 (aaaa1111)");
        assert_eq!(rows[1].name, "张三 (bbbb2222)");
    }
}
