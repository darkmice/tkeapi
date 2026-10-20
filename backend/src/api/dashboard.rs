/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::auth;
use crate::error::AppResult;
use crate::middleware::live_metrics::MetricsSnapshot;
use crate::models::{DashboardStats, ModelStat, RequestLog};
use crate::AppState;
use axum::{
    extract::{Extension, State},
    http::HeaderMap,
    Json,
};
use std::sync::Arc;
use std::time::Duration;

/// 看板缓存条目存活上限：超过后由后台任务剔除，防止 DashMap 无限增长
pub const DASHBOARD_CACHE_TTL: Duration = Duration::from_secs(30 * 60);
/// 清理扫描间隔
pub const DASHBOARD_CACHE_CLEANUP_INTERVAL_SECS: u64 = 300;
/// 看板 SWR 缓存有效时长。与前端自动刷新间隔一致，避免每次轮询都重扫 logs。
pub const DASHBOARD_SWR_TTL: Duration = Duration::from_secs(180);

/// 剔除超过 TTL 的看板缓存 key
pub fn cleanup_stale_dashboard_cache(
    cache: &dashmap::DashMap<String, crate::DashboardCacheEntry>,
    ttl: Duration,
) -> usize {
    let stale_keys: Vec<String> = cache
        .iter()
        .filter(|entry| entry.timestamp.elapsed() >= ttl)
        .map(|entry| entry.key().clone())
        .collect();
    let n = stale_keys.len();
    for key in stale_keys {
        cache.remove(&key);
    }
    n
}

fn cleanup_stale_model_trend_cache(
    cache: &dashmap::DashMap<String, ModelTrendCacheEntry>,
    ttl: Duration,
) -> usize {
    let stale_keys: Vec<String> = cache
        .iter()
        .filter(|entry| entry.timestamp.elapsed() >= ttl)
        .map(|entry| entry.key().clone())
        .collect();
    let n = stale_keys.len();
    for key in stale_keys {
        cache.remove(&key);
    }
    n
}

/// 后台定期清理过期看板缓存
pub async fn run_dashboard_cache_cleanup_loop(
    state: Arc<AppState>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut interval =
        tokio::time::interval(Duration::from_secs(DASHBOARD_CACHE_CLEANUP_INTERVAL_SECS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let removed = cleanup_stale_dashboard_cache(&state.dashboard_cache, DASHBOARD_CACHE_TTL);
                let removed_trend = cleanup_stale_model_trend_cache(
                    &state.model_trend_cache,
                    DASHBOARD_CACHE_TTL,
                );
                if removed > 0 || removed_trend > 0 {
                    tracing::info!(
                        "[DashboardCache] 已清理 {} 条概览缓存、{} 条用量趋势缓存",
                        removed,
                        removed_trend
                    );
                }
            }
            _ = shutdown.changed() => {
                tracing::info!("[DashboardCache] 过期缓存清理任务已退出");
                return;
            }
        }
    }
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct DashboardParams {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    /// 手动刷新：跳过缓存，同步重算后写回。
    pub refresh: Option<bool>,
}

/// 看板首屏：统计走 SWR 缓存，实时吞吐每次现算（不进缓存）
#[derive(Debug, serde::Serialize)]
pub struct DashboardPageResponse {
    #[serde(flatten)]
    pub stats: DashboardStats,
    pub live_metrics: MetricsSnapshot,
}

fn with_live_metrics(stats: DashboardStats, claims: &auth::Claims) -> DashboardPageResponse {
    DashboardPageResponse {
        stats,
        live_metrics: crate::api::metrics::live_metrics_snapshot(claims),
    }
}

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(params): axum::extract::Query<DashboardParams>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<DashboardPageResponse>> {
    let is_admin = claims.role == "admin";
    let user_id = &claims.sub;

    let header_tz = headers
        .get("x-timezone")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let tz = if is_admin {
        crate::api::date_helper::resolve_site_operation_timezone(&state.db).await
    } else {
        crate::api::date_helper::resolve_user_timezone(&state.db, false, user_id, header_tz)
            .await?
    };

    let cache_key = if is_admin {
        format!(
            "admin_global_{}_{:?}_{:?}",
            tz.name(),
            params.start_date,
            params.end_date
        )
    } else {
        format!(
            "user_{}_{}_{:?}_{:?}",
            user_id,
            tz.name(),
            params.start_date,
            params.end_date
        )
    };

    let force_refresh = params.refresh.unwrap_or(false);

    // Stale-While-Revalidate：有效期内直接返回。过期则先回旧数据，后台重算。
    // 手动刷新不走这条路径。
    if !force_refresh {
        if let Some(entry) = state.dashboard_cache.get(&cache_key) {
            let elapsed = entry.timestamp.elapsed();
            if elapsed < DASHBOARD_SWR_TTL {
                return Ok(Json(with_live_metrics(entry.stats.clone(), &claims)));
            } else {
                drop(entry);

                if let Some(mut write_entry) = state.dashboard_cache.get_mut(&cache_key) {
                    if write_entry.timestamp.elapsed() >= DASHBOARD_SWR_TTL {
                        let previous_timestamp = write_entry.timestamp;
                        let claim_timestamp = std::time::Instant::now();
                        write_entry.timestamp = claim_timestamp;
                        drop(write_entry);

                        let state_clone = state.clone();
                        let cache_key_clone = cache_key.clone();
                        let params_clone = params.clone();
                        let user_id_clone = user_id.to_string();
                        let is_admin_clone = is_admin;
                        let tz_clone = tz;

                        tokio::spawn(async move {
                            match calculate_dashboard_stats(
                                state_clone.clone(),
                                is_admin_clone,
                                &user_id_clone,
                                params_clone,
                                tz_clone,
                            )
                            .await
                            {
                                Ok(new_stats) => {
                                    let newer_landed = state_clone
                                        .dashboard_cache
                                        .get(&cache_key_clone)
                                        .is_some_and(|entry| entry.timestamp > claim_timestamp);
                                    if newer_landed {
                                        return;
                                    }
                                    state_clone.dashboard_cache.insert(
                                        cache_key_clone.clone(),
                                        crate::DashboardCacheEntry {
                                            stats: new_stats,
                                            timestamp: std::time::Instant::now(),
                                        },
                                    );
                                    tracing::info!(
                                        "✅ [SWR] 后台异步更新控制台仪表盘缓存成功: {}",
                                        cache_key_clone
                                    );
                                }
                                Err(e) => {
                                    if let Some(mut failed) =
                                        state_clone.dashboard_cache.get_mut(&cache_key_clone)
                                    {
                                        if failed.timestamp == claim_timestamp {
                                            failed.timestamp = previous_timestamp;
                                        }
                                    }
                                    tracing::warn!(
                                        "❌ [SWR] 后台异步更新控制台仪表盘缓存失败: {:?}, key: {}",
                                        e,
                                        cache_key_clone
                                    );
                                }
                            }
                        });
                    }
                }

                if let Some(entry) = state.dashboard_cache.get(&cache_key) {
                    return Ok(Json(with_live_metrics(entry.stats.clone(), &claims)));
                }
            }
        }
    }

    // 缓存首次加载时触发同步计算
    let stats = calculate_dashboard_stats(state.clone(), is_admin, user_id, params, tz).await?;
    state.dashboard_cache.insert(
        cache_key,
        crate::DashboardCacheEntry {
            stats: stats.clone(),
            timestamp: std::time::Instant::now(),
        },
    );

    Ok(Json(with_live_metrics(stats, &claims)))
}

/// 每日汇总是否已经写下这一天。有记录就走汇总表，避免重复扫 logs。
async fn usage_daily_stat_ready(state: &AppState, day: chrono::NaiveDate) -> bool {
    let sql = state
        .db
        .format_query("SELECT EXISTS(SELECT 1 FROM usage_daily_stats WHERE stat_date = ?)");
    sqlx::query_scalar::<_, bool>(&sql)
        .bind(day)
        .fetch_one(&state.db.pool)
        .await
        .unwrap_or(false)
}

async fn query_aggregated_data_helper(
    state: &Arc<AppState>,
    is_admin: bool,
    user_id: &str,
    slices: &crate::api::date_helper::QueryTimeSlice,
) -> AppResult<(i64, i64, f64, i64)> {
    let mut total_requests = 0i64;
    let mut total_tokens = 0i64;
    let mut total_cost = 0.0f64;
    let mut active_tokens = 0i64;

    // A. 历史归档天段统计
    if slices.has_history_days {
        let (reqs, tokens, cost): (Option<i64>, Option<i64>, Option<f64>) = if is_admin {
            let sql = format!("SELECT CAST(SUM(total_requests) AS BIGINT), CAST(SUM(total_tokens) AS BIGINT), SUM(total_cost) FROM usage_daily_stats WHERE {}", slices.history_cond("stat_date"));
            sqlx::query_as(&state.db.format_query(&sql))
                .bind(slices.hist_start_date)
                .bind(slices.hist_end_date)
                .fetch_one(&state.db.pool)
                .await?
        } else {
            let sql = format!("SELECT CAST(SUM(total_requests) AS BIGINT), CAST(SUM(total_tokens) AS BIGINT), SUM(total_cost) FROM usage_daily_stats WHERE user_id = ? AND {}", slices.history_cond("stat_date"));
            sqlx::query_as(&state.db.format_query(&sql))
                .bind(user_id)
                .bind(slices.hist_start_date)
                .bind(slices.hist_end_date)
                .fetch_one(&state.db.pool)
                .await?
        };
        total_requests += reqs.unwrap_or(0);
        total_tokens += tokens.unwrap_or(0);
        total_cost += cost.unwrap_or(0.0);

        let active_toks: Option<i64> = if is_admin {
            let sql = format!("SELECT COUNT(DISTINCT token_id) FROM usage_daily_stats WHERE {} AND token_id != -1", slices.history_cond("stat_date"));
            sqlx::query_scalar(&state.db.format_query(&sql))
                .bind(slices.hist_start_date)
                .bind(slices.hist_end_date)
                .fetch_one(&state.db.pool)
                .await?
        } else {
            let sql = format!("SELECT COUNT(DISTINCT token_id) FROM usage_daily_stats WHERE user_id = ? AND {} AND token_id != -1", slices.history_cond("stat_date"));
            sqlx::query_scalar(&state.db.format_query(&sql))
                .bind(user_id)
                .bind(slices.hist_start_date)
                .bind(slices.hist_end_date)
                .fetch_one(&state.db.pool)
                .await?
        };
        active_tokens = active_tokens.max(active_toks.unwrap_or(0));
    }

    // B. 实时及碎片段：COUNT/SUM/DISTINCT 一次扫过 covering 列，避免同区间二次全表聚合
    for r_slice in slices.realtime_slices() {
        let sql = if is_admin {
            format!(
                "SELECT COUNT(*)::bigint, \
                 (COALESCE(SUM(prompt_tokens), 0) + COALESCE(SUM(completion_tokens), 0))::bigint, \
                 COALESCE(SUM(cost), 0)::float8, \
                 COUNT(DISTINCT token_id)::bigint \
                 FROM logs WHERE {}",
                r_slice.sql_cond("created_at")
            )
        } else {
            format!(
                "SELECT COUNT(*)::bigint, \
                 (COALESCE(SUM(prompt_tokens), 0) + COALESCE(SUM(completion_tokens), 0))::bigint, \
                 COALESCE(SUM(cost), 0)::float8, \
                 COUNT(DISTINCT token_id)::bigint \
                 FROM logs WHERE user_id = ? AND {}",
                r_slice.sql_cond("created_at")
            )
        };
        let formatted = state.db.format_query(&sql);
        let (reqs, tokens, cost, distinct_toks): (i64, i64, f64, i64) = if is_admin {
            sqlx::query_as(&formatted)
                .bind(&r_slice.start)
                .bind(&r_slice.end)
                .fetch_one(&state.db.pool)
                .await?
        } else {
            sqlx::query_as(&formatted)
                .bind(user_id)
                .bind(&r_slice.start)
                .bind(&r_slice.end)
                .fetch_one(&state.db.pool)
                .await?
        };
        total_requests += reqs;
        total_tokens += tokens;
        total_cost += cost;
        active_tokens = active_tokens.max(distinct_toks);
    }

    Ok((total_requests, total_tokens, total_cost, active_tokens))
}

/// 同一段 logs 同时拿出按模型合计和全段合计（含去重令牌数），避免当天热表扫两遍。
async fn query_logs_rollup(
    state: &Arc<AppState>,
    is_admin: bool,
    user_id: &str,
    slices: &crate::api::date_helper::QueryTimeSlice,
) -> AppResult<(
    std::collections::HashMap<String, (i64, f64, i64)>,
    i64,
    i64,
    f64,
    i64,
)> {
    #[derive(sqlx::FromRow)]
    struct RollupRow {
        model: Option<String>,
        count: i64,
        tokens: i64,
        cost: f64,
        is_total: bool,
    }

    let mut model_map = std::collections::HashMap::new();
    let mut total_requests = 0i64;
    let mut total_tokens = 0i64;
    let mut total_cost = 0.0f64;
    let mut active_tokens = 0i64;

    for r_slice in slices.realtime_slices() {
        // 模型 Rollup 查询仅聚合 model/count/tokens/cost，100% 覆盖于 idx_logs_stat_covering_v3，实现 0 回表 Index-Only Scan
        let sql = if is_admin {
            format!(
                "SELECT model, \
                 COUNT(*)::bigint AS count, \
                 (COALESCE(SUM(prompt_tokens), 0) + COALESCE(SUM(completion_tokens), 0))::bigint AS tokens, \
                 COALESCE(SUM(cost), 0)::float8 AS cost, \
                 (GROUPING(model) = 1) AS is_total \
                 FROM logs WHERE {} \
                 GROUP BY GROUPING SETS ((model), ())",
                r_slice.sql_cond("created_at")
            )
        } else {
            format!(
                "SELECT model, \
                 COUNT(*)::bigint AS count, \
                 (COALESCE(SUM(prompt_tokens), 0) + COALESCE(SUM(completion_tokens), 0))::bigint AS tokens, \
                 COALESCE(SUM(cost), 0)::float8 AS cost, \
                 (GROUPING(model) = 1) AS is_total \
                 FROM logs WHERE user_id = ? AND {} \
                 GROUP BY GROUPING SETS ((model), ())",
                r_slice.sql_cond("created_at")
            )
        };
        let formatted = state.db.format_query(&sql);

        // 活跃令牌去重数独立极速统计（单流去重，无 GROUP BY model，走索引覆盖毫秒返回）
        let active_sql = if is_admin {
            format!(
                "SELECT COUNT(DISTINCT token_id)::bigint FROM logs WHERE {} AND token_id != -1",
                r_slice.sql_cond("created_at")
            )
        } else {
            format!(
                "SELECT COUNT(DISTINCT token_id)::bigint FROM logs WHERE user_id = ? AND {} AND token_id != -1",
                r_slice.sql_cond("created_at")
            )
        };
        let formatted_active = state.db.format_query(&active_sql);

        let (rows, distinct_toks): (Vec<RollupRow>, Option<i64>) = if is_admin {
            tokio::try_join!(
                sqlx::query_as(&formatted)
                    .bind(&r_slice.start)
                    .bind(&r_slice.end)
                    .fetch_all(&state.db.pool),
                sqlx::query_scalar(&formatted_active)
                    .bind(&r_slice.start)
                    .bind(&r_slice.end)
                    .fetch_one(&state.db.pool)
            )?
        } else {
            tokio::try_join!(
                sqlx::query_as(&formatted)
                    .bind(user_id)
                    .bind(&r_slice.start)
                    .bind(&r_slice.end)
                    .fetch_all(&state.db.pool),
                sqlx::query_scalar(&formatted_active)
                    .bind(user_id)
                    .bind(&r_slice.start)
                    .bind(&r_slice.end)
                    .fetch_one(&state.db.pool)
            )?
        };

        for row in rows {
            if row.is_total {
                total_requests += row.count;
                total_tokens += row.tokens;
                total_cost += row.cost;
                continue;
            }
            let Some(model) = row.model else {
                continue;
            };
            let entry = model_map.entry(model).or_insert((0i64, 0.0f64, 0i64));
            entry.0 += row.count;
            entry.1 += row.cost;
            entry.2 += row.tokens;
        }

        active_tokens = active_tokens.max(distinct_toks.unwrap_or(0));
    }

    Ok((
        model_map,
        total_requests,
        total_tokens,
        total_cost,
        active_tokens,
    ))
}

/// 采用 Lambda 增量聚合架构（历史汇总表 + 今日日志表分段合并）的高性能控制台统计函数
async fn calculate_dashboard_stats(
    state: Arc<AppState>,
    is_admin: bool,
    user_id: &str,
    params: DashboardParams,
    tz: chrono_tz::Tz,
) -> AppResult<DashboardStats> {
    use chrono::{Duration, NaiveDate};

    let bounds = crate::api::date_helper::get_timezone_time_bounds(tz);
    let today_date = bounds.today;
    let today_str = today_date.format("%Y-%m-%d").to_string();

    let start_naive = crate::api::date_helper::parse_to_naive_date(
        params.start_date.as_deref(),
        NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
        tz,
    );

    let end_naive = crate::api::date_helper::parse_to_naive_date(
        params.end_date.as_deref(),
        NaiveDate::from_ymd_opt(9999, 12, 31).unwrap(),
        tz,
    );

    let yesterday_date = bounds.yesterday;

    // 1. 基础指标：今日 logs 只扫一遍，筛选合计 = 归档/碎片 + 今日
    let slices = crate::api::date_helper::calculate_query_slices(
        params.start_date.as_deref().or(Some("1970-01-01")),
        params.end_date.as_deref(),
        tz,
    );
    let today_slices =
        crate::api::date_helper::calculate_query_slices(Some(&today_str), Some(&today_str), tz);
    let detail_days = crate::api::date_helper::model_detail_days(
        if params.end_date.is_some() {
            Some(end_naive)
        } else {
            None
        },
        today_date,
    );
    let need_today_models = slices.has_today || detail_days.iter().any(|d| *d == today_date);
    let (today_model_map, today_requests, today_tokens, today_cost, today_active_tokens) =
        if need_today_models {
            query_logs_rollup(&state, is_admin, user_id, &today_slices).await?
        } else {
            let (reqs, tokens, cost, active) =
                query_aggregated_data_helper(&state, is_admin, user_id, &today_slices).await?;
            (std::collections::HashMap::new(), reqs, tokens, cost, active)
        };
    let hist_only = slices.excluding_today();
    let (mut total_requests, mut total_tokens, mut total_cost, _) =
        query_aggregated_data_helper(&state, is_admin, user_id, &hist_only).await?;
    if slices.has_today {
        total_requests += today_requests;
        total_tokens += today_tokens;
        total_cost += today_cost;
    }

    // 基础关系表总数（如用户、渠道、API令牌等）
    let total_users: i64 = if is_admin {
        sqlx::query_scalar::<_, i64>(&state.db.format_query("SELECT COUNT(*) FROM users"))
            .fetch_one(&state.db.pool)
            .await?
    } else {
        1
    };

    let total_channels: i64 = if is_admin {
        sqlx::query_scalar::<_, i64>(&state.db.format_query("SELECT COUNT(*) FROM channels"))
            .fetch_one(&state.db.pool)
            .await?
    } else {
        0
    };

    let total_api_tokens: i64 = if is_admin {
        sqlx::query_scalar::<_, i64>(&state.db.format_query("SELECT COUNT(*) FROM api_tokens"))
            .fetch_one(&state.db.pool)
            .await?
    } else {
        sqlx::query_scalar::<_, i64>(
            &state
                .db
                .format_query("SELECT COUNT(*) FROM api_tokens WHERE user_id = ?"),
        )
        .bind(user_id)
        .fetch_one(&state.db.pool)
        .await?
    };

    // 2. 昨日：汇总表已有该日则走表；零点到每日汇总完成前才回查 logs
    let yesterday_str = yesterday_date.format("%Y-%m-%d").to_string();
    let yesterday_archived = usage_daily_stat_ready(&state, yesterday_date).await;
    let yesterday_slices = if yesterday_archived {
        crate::api::date_helper::calculate_query_slices(
            Some(&yesterday_str),
            Some(&yesterday_str),
            tz,
        )
    } else {
        crate::api::date_helper::logs_calendar_day_slice(yesterday_date, tz)
    };
    let need_yesterday_models = detail_days.iter().any(|d| *d == yesterday_date);
    let (
        yesterday_model_map,
        yesterday_requests,
        yesterday_tokens,
        yesterday_cost,
        yesterday_active_tokens,
    ) = if need_yesterday_models && !yesterday_archived {
        query_logs_rollup(&state, is_admin, user_id, &yesterday_slices).await?
    } else {
        let (reqs, tokens, cost, active) =
            query_aggregated_data_helper(&state, is_admin, user_id, &yesterday_slices).await?;
        let map = if need_yesterday_models {
            crate::relay::usage_stats::query_model_stats_by_slices(
                &state.db,
                if is_admin { None } else { Some(user_id) },
                &yesterday_slices,
            )
            .await?
        } else {
            std::collections::HashMap::new()
        };
        (map, reqs, tokens, cost, active)
    };

    let mut date_where = String::new();
    let mut recent_binds = Vec::new();

    if let Some(ref s) = params.start_date {
        crate::api::date_helper::push_timestamptz_bound(
            &mut date_where,
            &mut recent_binds,
            "l.created_at",
            s,
            false,
            tz,
        );
    }
    if let Some(ref e) = params.end_date {
        crate::api::date_helper::push_timestamptz_bound(
            &mut date_where,
            &mut recent_binds,
            "l.created_at",
            e,
            true,
            tz,
        );
    }

    // 最近活动：不选 TOAST 大字段，避免读 request/response
    const RECENT_LOG_COLS: &str = "l.id, l.user_id, l.channel_id, l.token_id, l.model, \
         l.prompt_tokens, l.completion_tokens, l.cached_tokens, l.cost, l.latency_ms, \
         l.status_code, l.endpoint, l.error_message, l.created_at, \
         COALESCE(u.nickname, u.username) AS user_nickname, u.uid AS user_uid";
    let recent_logs: Vec<RequestLog> = if is_admin {
        let sql = format!(
            "SELECT {RECENT_LOG_COLS} FROM logs l LEFT JOIN users u ON l.user_id = u.id \
             WHERE 1=1{} ORDER BY l.created_at DESC LIMIT 10",
            date_where
        );
        let formatted_sql = state.db.format_query(&sql);
        let mut q = sqlx::query_as::<_, RequestLog>(&formatted_sql);
        for bind_val in &recent_binds {
            q = q.bind(bind_val);
        }
        q.fetch_all(&state.db.pool).await?
    } else {
        let sql = format!(
            "SELECT {RECENT_LOG_COLS} FROM logs l LEFT JOIN users u ON l.user_id = u.id \
             WHERE l.user_id = ?{} ORDER BY l.created_at DESC LIMIT 10",
            date_where
        );
        let formatted_sql = state.db.format_query(&sql);
        let mut q = sqlx::query_as::<_, RequestLog>(&formatted_sql).bind(user_id);
        for bind_val in &recent_binds {
            q = q.bind(bind_val);
        }
        q.fetch_all(&state.db.pool).await?
    };

    let mut recent_logs = recent_logs;
    if !is_admin {
        for log in &mut recent_logs {
            crate::api::logs::redact_request_log_for_user(log);
        }
    }

    // 4. 各模型统计：今日合计已从同一次 logs 汇总取出
    let user_filter = if is_admin { None } else { Some(user_id) };
    let mut model_map =
        crate::relay::usage_stats::query_model_stats_by_slices(&state.db, user_filter, &hist_only)
            .await?;
    if slices.has_today {
        for (m, (cnt, c, t)) in &today_model_map {
            let entry = model_map.entry(m.clone()).or_insert((0i64, 0.0f64, 0i64));
            entry.0 += cnt;
            entry.1 += c;
            entry.2 += t;
        }
    }

    // 转换并对模型进行排序 (按总花费降序，总请求数降序)
    let mut top_models_all: Vec<(String, f64, i64, i64)> = model_map
        .into_iter()
        .map(|(m, (cnt, c, t))| (m, c, t, cnt))
        .collect();
    top_models_all.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.3.cmp(&a.3))
    });

    let top_10_models: Vec<(String, f64, i64, i64)> = top_models_all.into_iter().take(10).collect();

    // 4.3 模型明细近几日：前日及更早走归档，昨日与当日走 logs
    let mut stats_by_date: std::collections::HashMap<
        String,
        std::collections::HashMap<String, (i64, f64, i64)>,
    > = std::collections::HashMap::new();

    let hist_days: Vec<_> = detail_days
        .iter()
        .copied()
        .filter(|d| *d < yesterday_date)
        .collect();
    if let (Some(hist_start), Some(hist_end)) = (hist_days.first(), hist_days.last()) {
        let hist_map = crate::relay::usage_stats::query_model_daily_stats_history(
            &state.db,
            user_filter,
            *hist_start,
            *hist_end,
        )
        .await?;
        stats_by_date.extend(hist_map);
    }

    if need_yesterday_models {
        stats_by_date.insert(yesterday_str.clone(), yesterday_model_map);
    }

    if detail_days.iter().any(|d| *d == today_date) {
        stats_by_date.insert(today_str.clone(), today_model_map);
    }

    let day_stats_list: Vec<(String, std::collections::HashMap<String, (i64, f64, i64)>)> =
        detail_days
            .iter()
            .map(|day| {
                let date_str = day.format("%Y-%m-%d").to_string();
                let day_stats = stats_by_date.remove(&date_str).unwrap_or_default();
                (date_str, day_stats)
            })
            .collect();

    let mut model_stats = Vec::new();
    for (m_name, m_cost, m_tokens, m_count) in top_10_models {
        let mut last_three_days = Vec::new();
        for (target_date, day_stats) in &day_stats_list {
            let found = day_stats.get(m_name.as_str());
            last_three_days.push(crate::models::DashboardModelDailyStatInfo {
                date: target_date.clone(),
                count: found.map(|f| f.0).unwrap_or(0),
                total_cost: found.map(|f| f.1).unwrap_or(0.0),
            });
        }
        model_stats.push(ModelStat {
            model: m_name,
            count: m_count,
            total_tokens: Some(m_tokens),
            total_cost: Some(m_cost),
            last_three_days,
        });
    }

    // 5. 每日统计趋势 (Daily Trends)：默认最近 30 天，或根据用户传入的自定义日期范围进行裁剪
    let trend_start_date = if params.start_date.is_some() {
        start_naive
    } else {
        today_date - Duration::days(29)
    };

    let trend_end_date = if params.end_date.is_some() {
        end_naive.min(today_date)
    } else {
        today_date
    };

    #[derive(Debug, sqlx::FromRow)]
    struct TrendHistRaw {
        pub date: String,
        pub requests: i64,
        pub cost: f64,
    }

    let mut trends_map: std::collections::HashMap<String, (i64, f64)> =
        std::collections::HashMap::new();

    // 5.1 历史统计趋势
    let archive_trend_end = (yesterday_date - Duration::days(1)).min(trend_end_date);
    let hist_trends: Vec<TrendHistRaw> = if trend_start_date <= archive_trend_end {
        let actual_hist_end = archive_trend_end;
        if is_admin {
            sqlx::query_as(&state.db.format_query(
                "SELECT stat_date::text as date, CAST(SUM(total_requests) AS BIGINT) as requests, SUM(total_cost) as cost \
                 FROM usage_daily_stats WHERE stat_date >= ? AND stat_date <= ? GROUP BY stat_date ORDER BY stat_date ASC"
            ))
            .bind(trend_start_date)
            .bind(actual_hist_end)
            .fetch_all(&state.db.pool)
            .await
            .unwrap_or_default()
        } else {
            sqlx::query_as(&state.db.format_query(
                "SELECT stat_date::text as date, CAST(SUM(total_requests) AS BIGINT) as requests, SUM(total_cost) as cost \
                 FROM usage_daily_stats WHERE user_id = ? AND stat_date >= ? AND stat_date <= ? GROUP BY stat_date ORDER BY stat_date ASC"
            ))
            .bind(user_id)
            .bind(trend_start_date)
            .bind(actual_hist_end)
            .fetch_all(&state.db.pool)
            .await
            .unwrap_or_default()
        }
    } else {
        vec![]
    };

    for row in hist_trends {
        trends_map.insert(row.date, (row.requests, row.cost));
    }

    if trend_start_date <= yesterday_date && trend_end_date >= yesterday_date {
        trends_map.insert(yesterday_str.clone(), (yesterday_requests, yesterday_cost));
    }

    // 5.2 今日趋势（仅当结束日期范围包含今天时才合入今日实时数据）
    if trend_end_date >= today_date {
        let (today_trend_reqs, today_trend_cost) = (today_requests, today_cost);
        trends_map.insert(today_str.clone(), (today_trend_reqs, today_trend_cost));
    }

    // 生成顺序完整、无断档的趋势数组
    let mut daily_trends = Vec::new();
    let mut d_iter = trend_start_date;
    while d_iter <= trend_end_date {
        let d_str = d_iter.format("%Y-%m-%d").to_string();
        let val = trends_map.get(&d_str).cloned().unwrap_or((0, 0.0));
        daily_trends.push(crate::models::DashboardDailyTrend {
            date: d_str,
            requests: val.0,
            cost: val.1,
        });
        d_iter += Duration::days(1);
    }

    let stats = DashboardStats {
        total_requests,
        total_tokens,
        total_cost,
        total_users,
        total_channels,
        total_api_tokens,
        today_requests,
        today_tokens,
        today_cost,
        today_active_tokens,
        yesterday_requests,
        yesterday_tokens,
        yesterday_cost,
        yesterday_active_tokens,
        recent_logs,
        model_stats,
        daily_trends,
    };

    Ok(stats)
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]

pub struct ModelStat30d {
    pub model: String,
    pub count: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct ModelDailyStat {
    pub date: String,
    pub model: String,
    pub count: i64,
    pub total_cost: f64,
}

pub struct ModelTrendCacheEntry {
    pub response: ModelTrend30dResponse,
    pub timestamp: std::time::Instant,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelTrend30dResponse {
    pub top_models: Vec<ModelStat30d>,
    pub daily_data: Vec<ModelDailyStat>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ModelTrendParams {
    pub days: Option<i64>,
    pub limit: Option<i64>,
    /// 用量趋势：前 N 名之外的模型按日合并为 `__other__`。分布页不传。
    pub include_other: Option<bool>,
}

const USAGE_TREND_OTHER_MODEL: &str = "__other__";

/// 趋势窗口起点相对今天的回看天数。
/// 未传或非法值保持原接口：`today - 30`。
/// 7 / 30 / 90 为闭区间：最近 7 天、最近 30 天、最近 90 天。
fn trend_lookback_days(days: Option<i64>) -> i64 {
    match days {
        Some(7) => 6,
        Some(30) => 29,
        Some(90) => 89,
        _ => 30,
    }
}

/// 按成本返回的模型数。未传保持 30 天分布页的 12 个；用量趋势传 10。
fn trend_model_limit(limit: Option<i64>) -> usize {
    match limit {
        Some(10) => 10,
        Some(15) => 15,
        _ => 12,
    }
}

pub async fn get_model_stats_30d(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<ModelTrendParams>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<ModelTrend30dResponse>> {
    let is_admin = claims.role == "admin";
    let user_id = &claims.sub;

    let header_tz = headers
        .get("x-timezone")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let tz = if is_admin {
        crate::api::date_helper::resolve_site_operation_timezone(&state.db).await
    } else {
        crate::api::date_helper::resolve_user_timezone(&state.db, false, user_id, header_tz)
            .await?
    };

    let bounds = crate::api::date_helper::get_timezone_time_bounds(tz);
    let trend_cache_key = format!(
        "trend:{}:{}:{}:{}:{}:{}",
        if is_admin { "admin" } else { user_id },
        tz.name(),
        bounds.today,
        query.days.unwrap_or(0),
        query.limit.unwrap_or(0),
        query.include_other.unwrap_or(false)
    );
    if let Some(hit) = state.model_trend_cache.get(&trend_cache_key) {
        if hit.timestamp.elapsed() < DASHBOARD_SWR_TTL {
            return Ok(Json(hit.response.clone()));
        }
    }
    let today_date = bounds.today;
    let yesterday_date = bounds.yesterday;
    let trend_start_date = today_date - chrono::Duration::days(trend_lookback_days(query.days));
    let yesterday_archived = usage_daily_stat_ready(&state, yesterday_date).await;
    let hist_end_date = if yesterday_archived {
        yesterday_date
    } else {
        yesterday_date - chrono::Duration::days(1)
    };
    let today_start_ts = bounds.today_start_ts;

    #[derive(Debug, sqlx::FromRow)]
    struct ModelRaw30d {
        model: String,
        count: Option<i64>,
        total_tokens: Option<i64>,
        total_cost: Option<f64>,
    }

    // 1. 获取历史汇总数据
    let hist_stats: Vec<ModelRaw30d> = if is_admin {
        sqlx::query_as(&state.db.format_query(
            "
            SELECT 
                model,
                CAST(SUM(total_requests) AS BIGINT) as count,
                CAST(SUM(total_tokens) AS BIGINT) as total_tokens,
                SUM(total_cost) as total_cost
            FROM usage_daily_stats
            WHERE stat_date >= ? AND stat_date <= ?
            GROUP BY model
        ",
        ))
        .bind(trend_start_date)
        .bind(hist_end_date)
        .fetch_all(&state.db.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(&state.db.format_query(
            "
            SELECT 
                model,
                CAST(SUM(total_requests) AS BIGINT) as count,
                CAST(SUM(total_tokens) AS BIGINT) as total_tokens,
                SUM(total_cost) as total_cost
            FROM usage_daily_stats
            WHERE user_id = ? AND stat_date >= ? AND stat_date <= ?
            GROUP BY model
        ",
        ))
        .bind(user_id)
        .bind(trend_start_date)
        .bind(hist_end_date)
        .fetch_all(&state.db.pool)
        .await
        .unwrap_or_default()
    };

    // 2. 获取今日实时数据
    let today_stats: Vec<ModelRaw30d> = if is_admin {
        sqlx::query_as(&state.db.format_query(
            "
            SELECT 
                model,
                COUNT(*) as count,
                (COALESCE(SUM(prompt_tokens), 0) + COALESCE(SUM(completion_tokens), 0))::bigint as total_tokens,
                COALESCE(SUM(cost), 0.0) as total_cost
            FROM logs
            WHERE created_at >= ?::timestamptz
            GROUP BY model
        ",
        ))
        .bind(&today_start_ts)
        .fetch_all(&state.db.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(&state.db.format_query(
            "
            SELECT 
                model,
                COUNT(*) as count,
                (COALESCE(SUM(prompt_tokens), 0) + COALESCE(SUM(completion_tokens), 0))::bigint as total_tokens,
                COALESCE(SUM(cost), 0.0) as total_cost
            FROM logs
            WHERE user_id = ? AND created_at >= ?::timestamptz
            GROUP BY model
        ",
        ))
        .bind(user_id)
        .bind(&today_start_ts)
        .fetch_all(&state.db.pool)
        .await
        .unwrap_or_default()
    };

    let yesterday_str = yesterday_date.format("%Y-%m-%d").to_string();
    let yesterday_models = if yesterday_archived {
        std::collections::HashMap::new()
    } else {
        let yesterday_slices = crate::api::date_helper::logs_calendar_day_slice(yesterday_date, tz);
        crate::relay::usage_stats::query_model_stats_by_slices(
            &state.db,
            if is_admin {
                None
            } else {
                Some(user_id)
            },
            &yesterday_slices,
        )
        .await
        .unwrap_or_default()
    };

    // 3. 内存合并并排序得到 top 10
    let mut merge_map: std::collections::HashMap<String, ModelStat30d> =
        std::collections::HashMap::new();
    for row in hist_stats {
        let entry = merge_map
            .entry(row.model.clone())
            .or_insert_with(|| ModelStat30d {
                model: row.model.clone(),
                count: 0,
                total_tokens: 0,
                total_cost: 0.0,
            });
        entry.count += row.count.unwrap_or(0);
        entry.total_tokens += row.total_tokens.unwrap_or(0);
        entry.total_cost += row.total_cost.unwrap_or(0.0);
    }
    for row in &today_stats {
        let entry = merge_map
            .entry(row.model.clone())
            .or_insert_with(|| ModelStat30d {
                model: row.model.clone(),
                count: 0,
                total_tokens: 0,
                total_cost: 0.0,
            });
        entry.count += row.count.unwrap_or(0);
        entry.total_tokens += row.total_tokens.unwrap_or(0);
        entry.total_cost += row.total_cost.unwrap_or(0.0);
    }
    for (model, (count, cost, tokens)) in &yesterday_models {
        let entry = merge_map
            .entry(model.clone())
            .or_insert_with(|| ModelStat30d {
                model: model.clone(),
                count: 0,
                total_tokens: 0,
                total_cost: 0.0,
            });
        entry.count += count;
        entry.total_tokens += tokens;
        entry.total_cost += cost;
    }

    let mut all_models: Vec<ModelStat30d> = merge_map.into_values().collect();
    all_models.sort_by(|a, b| {
        b.total_cost
            .partial_cmp(&a.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.count.cmp(&a.count))
    });

    let top_models: Vec<ModelStat30d> = all_models
        .into_iter()
        .take(trend_model_limit(query.limit))
        .collect();
    let top_model_names: std::collections::HashSet<String> =
        top_models.iter().map(|s| s.model.clone()).collect();

    // 4. 提取按成本排序后的模型每日趋势
    #[derive(Debug, sqlx::FromRow)]
    struct ModelDailyRaw {
        date: String,
        model: String,
        count: Option<i64>,
        total_cost: Option<f64>,
    }

    let mut daily_data = Vec::new();

    if !top_model_names.is_empty() {
        let top_model_names_vec: Vec<String> = top_model_names.iter().cloned().collect();

        let hist_daily: Vec<ModelDailyRaw> = if is_admin {
            sqlx::query_as(&state.db.format_query(
                "
                SELECT 
                    stat_date::text as date,
                    model,
                    CAST(SUM(total_requests) AS BIGINT) as count,
                    SUM(total_cost) as total_cost
                FROM usage_daily_stats
                WHERE stat_date >= ? AND stat_date <= ? AND model = ANY(?)
                GROUP BY stat_date, model
            ",
            ))
            .bind(trend_start_date)
            .bind(hist_end_date)
            .bind(&top_model_names_vec)
            .fetch_all(&state.db.pool)
            .await
            .unwrap_or_default()
        } else {
            sqlx::query_as(&state.db.format_query(
                "
                SELECT 
                    stat_date::text as date,
                    model,
                    CAST(SUM(total_requests) AS BIGINT) as count,
                    SUM(total_cost) as total_cost
                FROM usage_daily_stats
                WHERE user_id = ? AND stat_date >= ? AND stat_date <= ? AND model = ANY(?)
                GROUP BY stat_date, model
            ",
            ))
            .bind(user_id)
            .bind(trend_start_date)
            .bind(hist_end_date)
            .bind(&top_model_names_vec)
            .fetch_all(&state.db.pool)
            .await
            .unwrap_or_default()
        };

        for row in hist_daily {
            daily_data.push(ModelDailyStat {
                date: row.date,
                model: row.model,
                count: row.count.unwrap_or(0),
                total_cost: row.total_cost.unwrap_or(0.0),
            });
        }

        for (model, (count, cost, _)) in &yesterday_models {
            if top_model_names.contains(model) {
                daily_data.push(ModelDailyStat {
                    date: yesterday_str.clone(),
                    model: model.clone(),
                    count: *count,
                    total_cost: *cost,
                });
            }
        }

        let today_date_str = today_date.format("%Y-%m-%d").to_string();
        for row in &today_stats {
            if top_model_names.contains(&row.model) {
                daily_data.push(ModelDailyStat {
                    date: today_date_str.clone(),
                    model: row.model.clone(),
                    count: row.count.unwrap_or(0),
                    total_cost: row.total_cost.unwrap_or(0.0),
                });
            }
        }

        if query.include_other.unwrap_or(false) {
            #[derive(Debug, sqlx::FromRow)]
            struct OtherDailyRaw {
                date: String,
                count: Option<i64>,
                total_cost: Option<f64>,
            }

            let other_daily: Vec<OtherDailyRaw> = if is_admin {
                sqlx::query_as(&state.db.format_query(
                    "
                    SELECT
                        stat_date::text as date,
                        CAST(SUM(total_requests) AS BIGINT) as count,
                        SUM(total_cost) as total_cost
                    FROM usage_daily_stats
                    WHERE stat_date >= ? AND stat_date <= ? AND NOT (model = ANY(?))
                    GROUP BY stat_date
                ",
                ))
                .bind(trend_start_date)
                .bind(hist_end_date)
                .bind(&top_model_names_vec)
                .fetch_all(&state.db.pool)
                .await
                .unwrap_or_default()
            } else {
                sqlx::query_as(&state.db.format_query(
                    "
                    SELECT
                        stat_date::text as date,
                        CAST(SUM(total_requests) AS BIGINT) as count,
                        SUM(total_cost) as total_cost
                    FROM usage_daily_stats
                    WHERE user_id = ? AND stat_date >= ? AND stat_date <= ? AND NOT (model = ANY(?))
                    GROUP BY stat_date
                ",
                ))
                .bind(user_id)
                .bind(trend_start_date)
                .bind(hist_end_date)
                .bind(&top_model_names_vec)
                .fetch_all(&state.db.pool)
                .await
                .unwrap_or_default()
            };

            for row in other_daily {
                let count = row.count.unwrap_or(0);
                let total_cost = row.total_cost.unwrap_or(0.0);
                if count == 0 && total_cost.abs() <= 0.0 {
                    continue;
                }
                daily_data.push(ModelDailyStat {
                    date: row.date,
                    model: USAGE_TREND_OTHER_MODEL.to_string(),
                    count,
                    total_cost,
                });
            }

            let mut yesterday_other_count = 0i64;
            let mut yesterday_other_cost = 0.0;
            for (model, (count, cost, _)) in &yesterday_models {
                if top_model_names.contains(model) {
                    continue;
                }
                yesterday_other_count += count;
                yesterday_other_cost += cost;
            }
            if yesterday_other_count != 0 || yesterday_other_cost.abs() > 0.0 {
                daily_data.push(ModelDailyStat {
                    date: yesterday_str.clone(),
                    model: USAGE_TREND_OTHER_MODEL.to_string(),
                    count: yesterday_other_count,
                    total_cost: yesterday_other_cost,
                });
            }

            let mut today_other_count = 0i64;
            let mut today_other_cost = 0.0;
            for row in &today_stats {
                if top_model_names.contains(&row.model) {
                    continue;
                }
                today_other_count += row.count.unwrap_or(0);
                today_other_cost += row.total_cost.unwrap_or(0.0);
            }
            if today_other_count != 0 || today_other_cost.abs() > 0.0 {
                daily_data.push(ModelDailyStat {
                    date: today_date_str,
                    model: USAGE_TREND_OTHER_MODEL.to_string(),
                    count: today_other_count,
                    total_cost: today_other_cost,
                });
            }
        }
    }

    let response = ModelTrend30dResponse {
        top_models,
        daily_data,
    };
    state.model_trend_cache.insert(
        trend_cache_key,
        ModelTrendCacheEntry {
            response: response.clone(),
            timestamp: std::time::Instant::now(),
        },
    );
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::{trend_lookback_days, trend_model_limit};

    #[test]
    fn trend_lookback_keeps_default_30_day_offset() {
        assert_eq!(trend_lookback_days(None), 30);
        assert_eq!(trend_lookback_days(Some(30)), 29);
        assert_eq!(trend_lookback_days(Some(15)), 30);
        assert_eq!(trend_lookback_days(Some(7)), 6);
        assert_eq!(trend_lookback_days(Some(90)), 89);
    }

    #[test]
    fn trend_model_limit_defaults_to_twelve() {
        assert_eq!(trend_model_limit(None), 12);
        assert_eq!(trend_model_limit(Some(12)), 12);
        assert_eq!(trend_model_limit(Some(11)), 12);
        assert_eq!(trend_model_limit(Some(10)), 10);
        assert_eq!(trend_model_limit(Some(15)), 15);
    }
}
