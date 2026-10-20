/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::api::date_helper;
use crate::auth;
use crate::error::{AppError, AppResult};
use crate::models::{LogDetailContent, LogListResponse, LogQuery, RequestLog};
use crate::relay::cascade::cascade_sanitize_for_user;
use crate::AppState;
use axum::extract::Path;
use axum::http::{header, HeaderMap, StatusCode};
use axum::{
    extract::{Extension, Query, State},
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

/// 去掉计费明细末尾的渠道/模型/分辨率映射段（` | 渠道映射: a ➞ b`、` | 分辨率映射@480p: a → b`），超管保留原文。
fn strip_model_mapping_from_billing_detail(detail: &mut String) {
    const MARKERS: &[&str] = &[" | 渠道映射", " | 模型映射", " | 分辨率映射"];
    if let Some(i) = MARKERS.iter().filter_map(|m| detail.find(m)).min() {
        let keep = detail[..i].trim_end().len();
        detail.truncate(keep);
    }
}

/// 普通用户列表：隐藏匹配规则相关标识（PID/EID/YID）。
pub(crate) fn redact_log_match_ids_for_user(
    billing_pid: &mut Option<String>,
    forward_eid: &mut Option<String>,
    yid: &mut Option<String>,
) {
    *billing_pid = None;
    *forward_eid = None;
    *yid = None;
}

/// 用户端详情：plugin_tag 白名单（仅 client_ct 展开展示）。
fn project_plugin_tag_for_user(plugin_tag: &mut Option<String>) {
    const KEEP: &[&str] = &["client_ct"];
    let Some(raw) = plugin_tag.as_deref() else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        *plugin_tag = None;
        return;
    };
    let Some(obj) = v.as_object() else {
        *plugin_tag = None;
        return;
    };
    let mut out = serde_json::Map::new();
    for key in KEEP {
        if let Some(s) = obj
            .get(*key)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
        {
            out.insert((*key).to_string(), serde_json::Value::String(s.to_string()));
        }
    }
    *plugin_tag = (!out.is_empty()).then(|| serde_json::Value::Object(out).to_string());
}

fn mask_upstream_url(upstream: &str) -> String {
    let Some(scheme_end) = upstream.find("://") else {
        return "***".to_string();
    };
    let scheme = &upstream[..scheme_end];
    let rest = &upstream[scheme_end + 3..];
    match rest.find('/') {
        Some(slash_idx) => format!("{}://***{}", scheme, &rest[slash_idx..]),
        None => format!("{}://***", scheme),
    }
}

/// 普通用户 RequestLog 响应脱敏（日志列表 / 仪表盘最近活动共用）。
pub(crate) fn redact_request_log_for_user(log: &mut RequestLog) {
    log.channel_id = None;
    log.channel_group_aid = None;
    log.channel_name = None;
    log.sub_channel_name = None;
    log.user_admin_remark = None;
    log.upstream_request_id = None;
    redact_log_match_ids_for_user(&mut log.billing_pid, &mut log.forward_eid, &mut log.yid);
    if let Some(ref err) = log.error_message {
        log.error_message = Some(crate::relay::proxy::sanitize_error_message(err));
    }
    if let Some(ref upstream) = log.upstream_url {
        log.upstream_url = Some(mask_upstream_url(upstream));
    }
}

/// 用户等级是否允许查看日志详情（缺省允许）。
pub(crate) async fn user_allow_view_log_details(
    db: &crate::db::Database,
    user_id: &str,
) -> AppResult<bool> {
    let perm: Option<i64> = sqlx::query_scalar(
        &db.format_query(
            "SELECT ul.allow_view_log_details FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?",
        ),
    )
    .bind(user_id)
    .fetch_optional(&db.pool)
    .await?
    .flatten();
    Ok(perm.unwrap_or(1) == 1)
}

/// 与部分索引 `idx_logs_vision_created_at_v2` 谓词对齐（精确对齐系统固定视觉类型，零冗余）。
pub(crate) const SQL_VISION_ACTION_FILTER: &str =
    " AND l.action_type = ANY(ARRAY['图片','视频','封装'])";

/// WHERE 只引用 `logs l`；跨表条件用 EXISTS，COUNT/stats 无需 JOIN。
pub(crate) fn build_log_where(
    claims: &auth::Claims,
    query: &LogQuery,
    allowed_target_user: bool,
    tz: chrono_tz::Tz,
) -> (String, Vec<String>) {
    let mut sql = " WHERE 1=1".to_string();
    let mut binds: Vec<String> = Vec::new();

    if claims.role != "admin" && !allowed_target_user {
        sql.push_str(" AND l.user_id = ?");
        binds.push(claims.sub.clone());
    } else if let Some(ref user_id) = query.user_id {
        if user_id == "unknown" {
            sql.push_str(" AND (l.user_id = 'unknown' OR l.user_id IS NULL OR l.user_id = '')");
        } else {
            sql.push_str(" AND l.user_id = ?");
            binds.push(user_id.clone());
        }
    }

    if let Some(ref uid) = query.uid {
        // 等价于原 LEFT JOIN users + u.uid = ?
        sql.push_str(" AND EXISTS (SELECT 1 FROM users xu WHERE xu.id = l.user_id AND xu.uid = ?)");
        binds.push(uid.clone());
    }

    if let Some(ref group) = query.user_group {
        if !group.is_empty() {
            sql.push_str(" AND EXISTS (SELECT 1 FROM users xu WHERE xu.id = l.user_id AND xu.user_group = ?)");
            binds.push(group.clone());
        }
    }

    if let Some(ref model) = query.model {
        sql.push_str(" AND l.model LIKE ?");
        let escaped = model.replace('%', "\\%").replace('_', "\\_");
        binds.push(format!("%{}%", escaped));
    }

    if let Some(channel_id) = query.channel_id {
        sql.push_str(" AND l.channel_id = CAST(? AS BIGINT)");
        binds.push(channel_id.to_string());
    }

    if let Some(ref aid) = query.channel_group_aid {
        // 等价于原 LEFT JOIN channels + c.group_aid = ?
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM channels xc WHERE xc.id = l.channel_id AND xc.group_aid = ?)",
        );
        binds.push(aid.clone());
    }

    if let Some(ref status) = query.status {
        push_log_status_filter(&mut sql, status);
    }

    if let Some(code) = query.status_code {
        sql.push_str(" AND l.status_code = CAST(? AS BIGINT)");
        binds.push(code.to_string());
    }

    if let Some(ref s) = query.start_date {
        push_created_at_bound(&mut sql, &mut binds, s, false, tz);
    }
    if let Some(ref e) = query.end_date {
        push_created_at_bound(&mut sql, &mut binds, e, true, tz);
    }

    if let Some(ref ep) = query.router_ep {
        sql.push_str(" AND (l.billing_detail LIKE ? OR l.model = ?)");
        binds.push(format!("%智能路由: {}%", ep));
        binds.push(ep.clone());
    }

    if let Some(ref action_type) = query.action_type {
        if !action_type.is_empty() {
            if action_type == "vision" || action_type == "视觉" {
                sql.push_str(SQL_VISION_ACTION_FILTER);
            } else {
                sql.push_str(" AND l.action_type = ?");
                binds.push(action_type.clone());
            }
        }
    }

    if let Some(ref log_id) = query.log_id {
        if !log_id.is_empty() {
            sql.push_str(" AND l.log_id = ?");
            binds.push(log_id.clone());
        }
    }

    if let Some(ref token_kid) = query.token_kid {
        if !token_kid.is_empty() {
            // 等价于原 LEFT JOIN api_tokens + t.kid = ?
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM api_tokens xt WHERE xt.id = l.token_id AND xt.kid = ?)",
            );
            binds.push(token_kid.clone());
        }
    }

    if let Some(ref task_id) = query.task_id {
        if !task_id.is_empty() {
            sql.push_str(" AND l.task_id = ?");
            binds.push(task_id.clone());
        }
    }

    if let Some(has_video) = query.has_video {
        sql.push_str(" AND l.has_video = CAST(? AS BIGINT)");
        binds.push(has_video.to_string());
    }

    if let Some(ref keyword) = query.search_keyword {
        push_log_search_keyword(&mut sql, &mut binds, keyword);
    }

    (sql, binds)
}

/// 日志记录 / 任务列表共用关键词：精确匹配 log_id、上游 task_id、渠道 AID、密钥 KID、上游 YID。
pub(crate) fn push_log_search_keyword(sql: &mut String, binds: &mut Vec<String>, keyword: &str) {
    let kw = keyword.trim();
    if kw.is_empty() {
        return;
    }
    // 固定前缀走索引，避免通用 OR + 多路 EXISTS
    // log_/tsk_ 为本系统 log_id；task_id 为上游任务 ID
    if kw.starts_with("log_") || kw.starts_with("tsk_") {
        sql.push_str(" AND l.log_id = ?");
        binds.push(kw.to_string());
    } else if kw.starts_with("tok_") {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM api_tokens xt WHERE xt.id = l.token_id AND xt.kid = ?)",
        );
        binds.push(kw.to_string());
    } else if kw.starts_with("cha_") {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM channels xc WHERE xc.id = l.channel_id AND xc.group_aid = ?)",
        );
        binds.push(kw.to_string());
    } else {
        sql.push_str(
            " AND (l.log_id = ? OR l.task_id = ? OR l.upstream_request_id = ? \
             OR EXISTS (SELECT 1 FROM channels xc WHERE xc.id = l.channel_id AND xc.group_aid = ?) \
             OR EXISTS (SELECT 1 FROM api_tokens xt WHERE xt.id = l.token_id AND xt.kid = ?) \
             OR EXISTS (SELECT 1 FROM channel_configs xcc WHERE xcc.id = l.channel_config_id AND xcc.yid = ?))",
        );
        binds.push(kw.to_string());
        binds.push(kw.to_string());
        binds.push(kw.to_string());
        binds.push(kw.to_string());
        binds.push(kw.to_string());
        binds.push(kw.to_string());
    }
}

/// 按 timestamptz 列做范围过滤：半开区间，无偏移墙钟按请求 timedisplay 解释。
pub(crate) fn push_created_at_bound(
    sql: &mut String,
    binds: &mut Vec<String>,
    raw: &str,
    is_end: bool,
    tz: chrono_tz::Tz,
) {
    date_helper::push_timestamptz_bound(sql, binds, "l.created_at", raw, is_end, tz);
}

/// 管理后台：请求头 / 管理后台展示时区。用户端：请求头 / 个人时区合法 IANA，否则 UTC。落库仍是 UTC。
pub(crate) async fn request_timedisplay(
    db: &crate::db::Database,
    claims: &auth::Claims,
    headers: &HeaderMap,
) -> AppResult<chrono_tz::Tz> {
    let header_tz = headers
        .get("x-timezone")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    Ok(date_helper::resolve_user_timezone(db, claims.role == "admin", &claims.sub, header_tz).await?)
}

pub(crate) const LOGS_LIST_JOINS: &str = " LEFT JOIN channels c ON l.channel_id = c.id \
      LEFT JOIN channel_configs cc ON l.channel_config_id = cc.id \
      LEFT JOIN users u ON l.user_id = u.id \
      LEFT JOIN user_levels ul ON u.user_group = ul.group_key \
      LEFT JOIN api_tokens t ON l.token_id = t.id";

/// 列表页附带模型 MID（logs.model 可能是 mid 或 model_id）；按渠道绑定与实际规则精准反查变体
pub(crate) const LOGS_MODEL_MID_JOIN: &str = " LEFT JOIN LATERAL (\
 SELECT m.mid FROM models m WHERE m.mid = l.model OR m.model_id = l.model \
 ORDER BY \
   CASE \
     WHEN m.mid = l.model THEN 0 \
     WHEN c.models IS NOT NULL AND position('\"' || m.mid || '\"' in c.models) > 0 THEN 1 \
     WHEN l.forward_eid IS NOT NULL AND l.forward_eid <> '' \
          AND EXISTS (SELECT 1 FROM forward_rules fr WHERE fr.eid = l.forward_eid AND COALESCE(m.forward_rule_ids, '') ~ ('\\m' || fr.id || '\\M')) THEN 2 \
     WHEN l.billing_pid IS NOT NULL AND l.billing_pid <> '' \
          AND EXISTS (SELECT 1 FROM billing_rules br WHERE br.id = m.billing_rule_id AND br.pid = l.billing_pid) THEN 3 \
     ELSE 4 \
   END, \
   m.id DESC \
 LIMIT 1) md ON TRUE";

/// 任务列表结算态：失败 / 冻结中 / 是否有计费明细（替代传 billing_detail 全文）。
pub(crate) const SQL_BILLING_SETTLE_FLAGS: &str = "\
COALESCE(l.billing_detail LIKE '%失败%', false) AS billing_failed, \
COALESCE(l.billing_detail LIKE '%冻结%', false) AS billing_frozen, \
(l.billing_detail IS NOT NULL AND btrim(l.billing_detail) <> '') AS billing_present";

/// 为 SQL 追加状态过滤条件：处理中 (0 或未完成)、成功 (200..399 且已完成)、失败 (错误码且剥离处理中)
pub(crate) fn push_log_status_filter(sql: &mut String, status: &str) {
    match status {
        "success" | "succeeded" => {
            sql.push_str(
                " AND l.status_code >= 200 AND l.status_code < 400 AND l.is_completed = 1",
            );
        }
        "processing" | "pending" => {
            sql.push_str(" AND (l.status_code = 0 OR l.is_completed = 0)");
        }
        "fail" | "failed" => {
            sql.push_str(
                " AND l.status_code != 0 AND (l.status_code >= 400 OR l.status_code < 200)",
            );
        }
        _ => {}
    }
}

/// 列表不选大 TEXT（依赖 RequestLog 上 `#[sqlx(default)]` → None）；展开走 get_log_detail。
/// 计费：布尔标记 + regexp 抽出用量数字，避免传输 billing_detail 全文。
pub(crate) const LOGS_LIST_SELECT: &str = "SELECT l.id, l.log_id, l.user_id, l.channel_id, l.token_id, l.model, \
         l.prompt_tokens, l.completion_tokens, l.cached_tokens, l.cost, l.latency_ms, \
         l.status_code, l.endpoint, l.error_message, l.upstream_url, l.upstream_request_id, \
         l.is_stream, \
         COALESCE(l.billing_detail LIKE '%退回%', false) AS billing_refunded, \
         COALESCE(l.billing_detail LIKE '%失败%', false) AS billing_failed, \
         COALESCE((regexp_match(COALESCE(l.billing_detail, ''), '(\\d+)创建@'))[1]::bigint, 0) AS billing_cache_creation, \
         COALESCE((regexp_match(COALESCE(l.billing_detail, ''), '(\\d+)读取@'))[1]::bigint, 0) AS billing_cache_read, \
         COALESCE((regexp_match(COALESCE(l.billing_detail, ''), '联网搜索:\\s*([\\d.]+)次'))[1]::float8, 0) AS billing_web_search, \
         l.billing_pid, l.forward_eid, l.pre_deduct_gift, \
         l.action_type, l.is_completed, l.channel_config_id, l.task_id, l.created_at, \
         l.is_ha, \
         c.group_aid AS channel_group_aid, c.name AS channel_name, \
         cc.name AS sub_channel_name, cc.yid AS yid, md.mid AS mid, \
         COALESCE(u.nickname, u.username) AS user_nickname, \
         NULLIF(btrim(COALESCE(u.admin_remark, '')), '') AS user_admin_remark, \
         u.user_group, ul.name AS user_level_name, u.uid AS user_uid, \
         t.name AS token_name, t.kid AS token_kid, \
         COALESCE(t.high_availability, 0)::bigint AS token_ha, \
         CASE WHEN COALESCE(t.only_playground, 0) = 1 OR COALESCE(t.only_playground_2026, 0) = 1 THEN 1::bigint ELSE 0::bigint END AS token_pg";

fn append_default_stats_window(
    where_clause: &str,
    binds: &[String],
    tz: chrono_tz::Tz,
) -> (String, Vec<String>) {
    let mut sql = where_clause.to_string();
    let mut sb = binds.to_vec();
    let thirty_days_ago = (chrono::Utc::now().with_timezone(&tz).date_naive()
        - chrono::Duration::days(30))
    .format("%Y-%m-%d")
    .to_string();
    push_created_at_bound(&mut sql, &mut sb, &thirty_days_ago, false, tz);
    (sql, sb)
}

pub(crate) async fn lookup_user_id(
    db: &crate::db::Database,
    key: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        &db.format_query("SELECT id FROM users WHERE uid = ? OR id = ? OR username = ?"),
    )
    .bind(key)
    .bind(key)
    .bind(key)
    .fetch_optional(&db.pool)
    .await
}

/// 解析用户标识；找不到时用占位，使后续 WHERE 安全返回空集
pub(crate) async fn resolve_user_filter(
    db: &crate::db::Database,
    key: &str,
) -> Result<String, sqlx::Error> {
    Ok(lookup_user_id(db, key)
        .await?
        .unwrap_or_else(|| "NOT_FOUND_USER".to_string()))
}

pub(crate) async fn fetch_logs_count(
    db: &crate::db::Database,
    where_clause: &str,
    binds: &[String],
) -> Result<i64, sqlx::Error> {
    fetch_logs_count_for_table(db, "logs", where_clause, binds).await
}

pub(crate) async fn fetch_logs_count_for_table(
    db: &crate::db::Database,
    table_name: &str,
    where_clause: &str,
    binds: &[String],
) -> Result<i64, sqlx::Error> {
    let sql = db.format_query(&format!("SELECT COUNT(*)::bigint FROM {table_name} l{where_clause}"));
    let mut q = sqlx::query_scalar::<_, i64>(&sql);
    for v in binds {
        q = q.bind(v);
    }
    q.fetch_one(&db.pool).await
}


/// COUNT + 汇总一次扫描（条件相同时替代并行两次全表聚合）
pub(crate) async fn fetch_logs_count_and_stats(
    db: &crate::db::Database,
    where_clause: &str,
    binds: &[String],
) -> Result<(i64, f64, i64, i64, f64, i64, i64, i64), sqlx::Error> {
    fetch_logs_count_and_stats_for_table(db, "logs", where_clause, binds).await
}

pub(crate) async fn fetch_logs_count_and_stats_for_table(
    db: &crate::db::Database,
    table_name: &str,
    where_clause: &str,
    binds: &[String],
) -> Result<(i64, f64, i64, i64, f64, i64, i64, i64), sqlx::Error> {
    let sql = db.format_query(&format!(
        "SELECT COUNT(*)::bigint, COALESCE(SUM(l.cost), 0.0), \
         COUNT(CASE WHEN l.status_code >= 200 AND l.status_code < 400 AND l.is_completed = 1 THEN 1 END), \
         COUNT(CASE WHEN l.status_code != 0 AND (l.status_code >= 400 OR l.status_code < 200) THEN 1 END), \
         COALESCE(SUM(GREATEST(LEAST(l.cost, l.pre_deduct_gift), 0.0)), 0.0), \
         COALESCE(SUM(l.prompt_tokens), 0)::bigint, \
         COALESCE(SUM(l.completion_tokens), 0)::bigint, \
         COALESCE(SUM(CASE WHEN l.has_video = 1 THEN l.completion_tokens ELSE 0 END), 0)::bigint \
         FROM {table_name} l{where_clause}"
    ));
    let mut q = sqlx::query_as::<_, (i64, f64, i64, i64, f64, i64, i64, i64)>(&sql);
    for v in binds {
        q = q.bind(v);
    }
    q.fetch_one(&db.pool).await
}

async fn fetch_logs_stats(
    db: &crate::db::Database,
    where_clause: &str,
    binds: &[String],
) -> (f64, i64, i64, f64, i64, i64, i64) {
    match fetch_logs_count_and_stats(db, where_clause, binds).await {
        Ok((_, cost, ok, fail, gift, prompt, completion, with_v)) => {
            (cost, ok, fail, gift, prompt, completion, with_v)
        }
        Err(_) => (0.0, 0, 0, 0.0, 0, 0, 0),
    }
}

pub(crate) async fn fetch_logs_list_rows(
    db: &crate::db::Database,
    data_sql: &str,
    binds: &[String],
) -> Result<Vec<RequestLog>, sqlx::Error> {
    let mut q = sqlx::query_as::<_, RequestLog>(data_sql);
    for v in binds {
        q = q.bind(v);
    }
    q.fetch_all(&db.pool).await
}

pub async fn list_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Extension(claims): Extension<auth::Claims>,
    Query(query): Query<LogQuery>,
) -> AppResult<Json<LogListResponse>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    // 频控：单个用户/客户端每分钟最多 30 次日志查询
    if !state.rate_limiter.check_heavy_query(&claims.sub, 30) {
        return Err(AppError::TooManyRequests("查询过于频繁，请稍后再试".into()));
    }

    let mut q = query.clone();
    let mut allowed_target_user = false;

    if claims.role == "admin" {
        allowed_target_user = true;
        if let Some(ref user_id) = q.user_id {
            if user_id != "unknown" {
                q.user_id = Some(resolve_user_filter(&state.db, user_id).await?);
            }
        }
    } else if let Some(ref target_user_id) = q.user_id {
        let my_uid: Option<String> =
            sqlx::query_scalar(&state.db.format_query("SELECT uid FROM users WHERE id = ?"))
                .bind(&claims.sub)
                .fetch_optional(&state.db.pool)
                .await?;
        let my_uid = my_uid.unwrap_or_default();

        if let Some(target_uuid) = lookup_user_id(&state.db, target_user_id).await? {
            let is_referral: bool = sqlx::query_scalar(
                &state.db.format_query(
                    "SELECT EXISTS(SELECT 1 FROM users WHERE id = ? AND (referred_by = ? OR referred_by = ?))",
                ),
            )
            .bind(&target_uuid)
            .bind(&claims.sub)
            .bind(&my_uid)
            .fetch_one(&state.db.pool)
            .await?;

            if is_referral {
                allowed_target_user = true;
                q.user_id = Some(target_uuid);
            } else {
                q.user_id = None;
            }
        } else {
            q.user_id = None;
        }
    }

    let tz = request_timedisplay(&state.db, &claims, &headers).await?;
    let (where_clause, binds) = build_log_where(&claims, &q, allowed_target_user, tz);

    // 无 start_date 时汇总默认近 30 个站点自然日，避免全历史扫描
    let stats_owned = if query.start_date.is_none() {
        Some(append_default_stats_window(&where_clause, &binds, tz))
    } else {
        None
    };
    let (stats_where, stats_binds) = match &stats_owned {
        Some((w, b)) => (w.as_str(), b.as_slice()),
        None => (where_clause.as_str(), binds.as_slice()),
    };

    let data_sql = state.db.format_query(&deferred_join_page_sql(
        LOGS_LIST_SELECT,
        &format!("{LOGS_LIST_JOINS}{LOGS_MODEL_MID_JOIN}"),
        &where_clause,
        per_page,
        offset,
    ));
    let binds_data = binds.clone();
    let db = state.db.clone();
    let stats_where_owned = stats_where.to_string();
    let stats_binds_owned = stats_binds.to_vec();

    // 翻页（page > 1）跳过全表聚合扫描，仅查当前页数据，耗时降至毫秒级；首屏 page=1 才跑 COUNT 与汇总
    let (
        total,
        mut logs,
        total_cost,
        success_count,
        fail_count,
        total_gift_cost,
        total_prompt_tokens,
        total_completion_tokens,
        total_with_video_tokens,
    ) = if page > 1 {
        (
            query.known_total.unwrap_or(-1),
            fetch_logs_list_rows(&db, &data_sql, &binds_data).await?,
            0.0,
            0,
            0,
            0.0,
            0,
            0,
            0,
        )
    } else if stats_owned.is_none() {
        let (agg_res, logs_res) = tokio::join!(
            fetch_logs_count_and_stats(&state.db, &where_clause, &binds),
            fetch_logs_list_rows(&db, &data_sql, &binds_data),
        );
        let (
            total,
            total_cost,
            success_count,
            fail_count,
            total_gift_cost,
            prompt,
            completion,
            with_v,
        ) = agg_res?;
        (
            total,
            logs_res?,
            total_cost,
            success_count,
            fail_count,
            total_gift_cost,
            prompt,
            completion,
            with_v,
        )
    } else {
        let (total_res, logs_res, stats) = tokio::join!(
            fetch_logs_count(&state.db, &where_clause, &binds),
            fetch_logs_list_rows(&db, &data_sql, &binds_data),
            fetch_logs_stats(&state.db, &stats_where_owned, &stats_binds_owned),
        );
        let (total_cost, success_count, fail_count, total_gift_cost, prompt, completion, with_v) =
            stats;
        (
            total_res?,
            logs_res?,
            total_cost,
            success_count,
            fail_count,
            total_gift_cost,
            prompt,
            completion,
            with_v,
        )
    };
    let total_tokens = total_prompt_tokens.saturating_add(total_completion_tokens);

    let mut allow_details = true;
    if claims.role != "admin" {
        allow_details = user_allow_view_log_details(&state.db, &claims.sub).await?;
        for log in &mut logs {
            redact_request_log_for_user(log);
        }
    }

    let total_system_cost = total_cost - total_gift_cost;

    Ok(Json(LogListResponse {
        data: logs,
        total,
        allow_details,
        total_cost,
        success_count,
        fail_count,
        total_tokens,
        total_prompt_tokens,
        total_completion_tokens,
        total_with_video_tokens: (total_with_video_tokens > 0).then_some(total_with_video_tokens),
        total_system_cost: Some(total_system_cost),
        total_gift_cost: Some(total_gift_cost),
    }))
}

/// 按需拉取单条日志的请求/响应大字段（列表接口已剥离）
pub async fn get_log_detail(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
) -> AppResult<Json<LogDetailContent>> {
    #[derive(sqlx::FromRow)]
    struct DetailRow {
        user_id: String,
        status_code: i64,
        is_completed: i64,
        task_id: Option<String>,
        model: String,
        endpoint: Option<String>,
        action_type: Option<String>,
        upstream_url: Option<String>,
        #[sqlx(default)]
        upstream_request_id: Option<String>,
        request_content: Option<String>,
        response_content: Option<String>,
        post_response: Option<String>,
        #[sqlx(default)]
        upstream_req_content: Option<String>,
        billing_detail: Option<String>,
        plugin_tag: Option<String>,
    }

    let is_admin = claims.role == "admin";
    // 普通用户不读上游出参大字段（接口也不返回）；超管保留完整列
    let detail_sql = if is_admin {
        "SELECT user_id, status_code, is_completed, task_id, model, endpoint, action_type, upstream_url, upstream_request_id, request_content, response_content, post_response, \
             upstream_req_content, billing_detail, plugin_tag \
             FROM logs WHERE id = ?"
    } else {
        "SELECT user_id, status_code, is_completed, task_id, model, endpoint, action_type, upstream_url, upstream_request_id, request_content, response_content, post_response, \
             billing_detail, plugin_tag \
             FROM logs WHERE id = ?"
    };
    let row: Option<DetailRow> = sqlx::query_as(&state.db.format_query(detail_sql))
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await?;
    let row = row.ok_or_else(|| AppError::BadRequest("日志记录不存在".to_string()))?;

    if !is_admin && row.user_id != claims.sub {
        let my_uid: Option<String> =
            sqlx::query_scalar(&state.db.format_query("SELECT uid FROM users WHERE id = ?"))
                .bind(&claims.sub)
                .fetch_optional(&state.db.pool)
                .await?;
        let my_uid = my_uid.unwrap_or_default();
        let is_referral: bool = sqlx::query_scalar(
            &state.db.format_query(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id = ? AND (referred_by = ? OR referred_by = ?))",
            ),
        )
        .bind(&row.user_id)
        .bind(&claims.sub)
        .bind(&my_uid)
        .fetch_one(&state.db.pool)
        .await?;
        if !is_referral {
            return Err(AppError::Forbidden("无权查看此日志详情".to_string()));
        }
    }

    if !is_admin && !user_allow_view_log_details(&state.db, &claims.sub).await? {
        return Ok(Json(LogDetailContent {
            id,
            request_content: None,
            response_content: None,
            post_response: None,
            upstream_req_content: None,
            billing_detail: None,
            upstream_request_id: None,
            plugin_tag: None,
        }));
    }

    let raw_plugin_tag = row.plugin_tag.clone();
    let mut plugin_tag = raw_plugin_tag.clone();
    let mut upstream_request_id = row.upstream_request_id;
    if !is_admin {
        project_plugin_tag_for_user(&mut plugin_tag);
        upstream_request_id = None;
    }

    let mut detail = LogDetailContent {
        id,
        request_content: row.request_content,
        response_content: row.response_content,
        post_response: row.post_response,
        upstream_req_content: row.upstream_req_content,
        billing_detail: row.billing_detail,
        upstream_request_id,
        plugin_tag,
    };

    // 火山方舟标准：上游为 /api/v3/contents/generations/tasks 时，校验 POST 提交响应仅保留 id
    if row
        .upstream_url
        .as_deref()
        .is_some_and(|u| u.contains("/api/v3/contents/generations/tasks"))
    {
        if let Some(ref mut post) = detail.post_response {
            if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(post) {
                if let Some(s1) = v.get_mut("stage1") {
                    let tid = crate::relay::response_formatter::extract_async_task_id(s1);
                    if !tid.is_empty() {
                        *s1 = serde_json::json!({ "id": tid });
                        *post = v.to_string();
                    }
                } else {
                    let tid = crate::relay::response_formatter::extract_async_task_id(&v);
                    if !tid.is_empty() {
                        *post = serde_json::json!({ "id": tid }).to_string();
                    }
                }
            }
        }
    }

    // 普通非级联任务：若 response_content 包含 model 根字段，将其覆写回请求的 row.model
    let has_cascade = raw_plugin_tag
        .as_deref()
        .map(|t| t.contains("\"cascade\""))
        .unwrap_or(false);
    if !has_cascade && !row.model.is_empty() {
        if let Some(ref mut resp) = detail.response_content {
            crate::relay::response_formatter::json_root_set(
                resp,
                "model",
                &serde_json::json!(row.model).to_string(),
            );
        }
    }

    // 用户端后台处理（管理员后端保持原样）
    if !is_admin {
        // 1. 级联脱敏须用原始 plugin_tag（含 cascade）；对外返回已白名单投影
        cascade_sanitize_for_user(
            &mut detail.response_content,
            &mut detail.post_response,
            raw_plugin_tag.as_deref(),
            row.is_completed == 1,
            row.task_id.as_deref().unwrap_or(""),
            &row.model,
            row.status_code,
        );
        if row.status_code != 200 {
            if let Some(ref resp) = detail.response_content {
                detail.response_content = Some(crate::relay::proxy::sanitize_error_message(resp));
            }
        }
        if let Some(ref mut bd) = detail.billing_detail {
            strip_model_mapping_from_billing_detail(bd);
        }

        // 2. 根据系统请求路径规范化响应格式（先 OpenAI 归一化，再包装官方信封，与 API 出口流水线完全一致）
        let ep = row.endpoint.as_deref().unwrap_or("");
        let clean_ep = ep.split(['|', '?']).next().unwrap_or("");
        let cat = row
            .action_type
            .as_deref()
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| {
                if clean_ep.contains("video") {
                    "视频"
                } else {
                    "图片"
                }
            });
        let req_str = detail.request_content.as_deref().unwrap_or("");
        let is_async =
            detail.post_response.is_some() || row.task_id.as_deref().is_some_and(|t| !t.is_empty());

        if let Some(ref mut post) = detail.post_response {
            let formatted =
                crate::relay::response_formatter::apply_format(clean_ep, cat, post, false, None);
            let wrapped = crate::relay::forward::wrap_official_client(
                clean_ep, &formatted, &row.model, req_str,
            );
            if !wrapped.is_empty() {
                *post = wrapped;
            }
        }
        if let Some(ref mut resp) = detail.response_content {
            let formatted = crate::relay::response_formatter::apply_format(
                clean_ep,
                cat,
                resp,
                is_async,
                row.task_id.as_deref(),
            );
            let query_path = if is_async {
                crate::relay::forward::to_official_query_path(
                    clean_ep,
                    row.task_id.as_deref().unwrap_or(""),
                )
            } else {
                clean_ep.to_string()
            };
            let wrapped = crate::relay::forward::wrap_official_client(
                &query_path,
                &formatted,
                &row.model,
                req_str,
            );
            if !wrapped.is_empty() {
                *resp = wrapped;
            }
        }
    }

    Ok(Json(detail))
}

const EXPORT_LIMIT: i64 = 100_000;

struct ExportCol {
    key: &'static str,
    header: &'static str,
}

/// 与管理端日志列表展示顺序对齐；勾选导出时保持该列序。
const LOG_EXPORT_COLUMNS: &[ExportCol] = &[
    ExportCol {
        key: "created_at",
        header: "时间",
    },
    ExportCol {
        key: "log_id",
        header: "日志ID",
    },
    ExportCol {
        key: "task_id",
        header: "任务ID",
    },
    ExportCol {
        key: "upstream_request_id",
        header: "上游请求ID",
    },
    ExportCol {
        key: "id",
        header: "ID",
    },
    ExportCol {
        key: "channel_group_aid",
        header: "渠道AID",
    },
    ExportCol {
        key: "is_ha",
        header: "渠道HA",
    },
    ExportCol {
        key: "yid",
        header: "上游YID",
    },
    ExportCol {
        key: "sub_channel_name",
        header: "实际上游",
    },
    ExportCol {
        key: "user_nickname",
        header: "用户昵称",
    },
    ExportCol {
        key: "user_uid",
        header: "UID",
    },
    ExportCol {
        key: "user_id",
        header: "用户ID",
    },
    ExportCol {
        key: "user_admin_remark",
        header: "管理员备注",
    },
    ExportCol {
        key: "token_name",
        header: "令牌",
    },
    ExportCol {
        key: "token_kid",
        header: "密钥KID",
    },
    ExportCol {
        key: "token_ha",
        header: "令牌HA",
    },
    ExportCol {
        key: "token_pg",
        header: "令牌PG",
    },
    ExportCol {
        key: "status_code",
        header: "状态码",
    },
    ExportCol {
        key: "model",
        header: "模型",
    },
    ExportCol {
        key: "billing_pid",
        header: "计费PID",
    },
    ExportCol {
        key: "forward_eid",
        header: "转发EID",
    },
    ExportCol {
        key: "latency_ms",
        header: "耗时(ms)",
    },
    ExportCol {
        key: "is_stream",
        header: "类型",
    },
    ExportCol {
        key: "prompt_tokens",
        header: "输入Tokens",
    },
    ExportCol {
        key: "completion_tokens",
        header: "输出Tokens",
    },
    ExportCol {
        key: "cached_tokens",
        header: "缓存Tokens",
    },
    ExportCol {
        key: "cost",
        header: "费用",
    },
    ExportCol {
        key: "billing_detail",
        header: "计费明细",
    },
    ExportCol {
        key: "error_message",
        header: "错误信息",
    },
    ExportCol {
        key: "action_type",
        header: "动作类型",
    },
    ExportCol {
        key: "endpoint",
        header: "请求路径",
    },
];

fn csv_escape(s: &str) -> String {
    if s.contains(['"', ',', '\n', '\r']) {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            if c == '"' {
                out.push('"');
            }
            out.push(c);
        }
        out.push('"');
        out
    } else {
        s.to_string()
    }
}

fn opt_dash(v: &Option<String>) -> &str {
    let s = v.as_deref().unwrap_or("");
    if s.is_empty() {
        "-"
    } else {
        s
    }
}

/// 空/缺省 → 全部列（列表顺序）；未知 key 忽略；有效列仍按列表顺序。
fn resolve_export_keys(raw: Option<&str>) -> Vec<&'static str> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return LOG_EXPORT_COLUMNS.iter().map(|c| c.key).collect();
    };
    let requested: std::collections::HashSet<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    LOG_EXPORT_COLUMNS
        .iter()
        .filter(|c| requested.contains(c.key))
        .map(|c| c.key)
        .collect()
}

fn export_cell(key: &str, row: &ExportLogRow) -> String {
    match key {
        "created_at" => csv_escape(&format_db_time(&row.created_at)),
        "log_id" => csv_escape(&row.log_id),
        "task_id" => csv_escape(&row.task_id),
        "upstream_request_id" => csv_escape(opt_dash(&row.upstream_request_id)),
        "id" => row.id.to_string(),
        "channel_group_aid" => csv_escape(opt_dash(&row.channel_group_aid)),
        "is_ha" => row.is_ha.to_string(),
        "yid" => csv_escape(opt_dash(&row.yid)),
        "sub_channel_name" => csv_escape(opt_dash(&row.sub_channel_name)),
        "user_nickname" => csv_escape(opt_dash(&row.user_nickname)),
        "user_uid" => csv_escape(if row.user_uid.is_empty() {
            "-"
        } else {
            row.user_uid.as_str()
        }),
        "user_id" => csv_escape(&row.user_id),
        "user_admin_remark" => csv_escape(opt_dash(&row.user_admin_remark)),
        "token_name" => csv_escape(opt_dash(&row.token_name)),
        "token_kid" => csv_escape(opt_dash(&row.token_kid)),
        "token_ha" => row.token_ha.to_string(),
        "token_pg" => row.token_pg.to_string(),
        "status_code" => row.status_code.to_string(),
        "model" => csv_escape(&row.model),
        "billing_pid" => csv_escape(opt_dash(&row.billing_pid)),
        "forward_eid" => csv_escape(opt_dash(&row.forward_eid)),
        "latency_ms" => row.latency_ms.to_string(),
        "is_stream" => csv_escape(if row.is_stream == Some(1) {
            "流"
        } else {
            "非流"
        }),
        "prompt_tokens" => row.prompt_tokens.to_string(),
        "completion_tokens" => row.completion_tokens.to_string(),
        "cached_tokens" => row.cached_tokens.to_string(),
        "cost" => crate::money::format_clean_money(row.cost),
        "billing_detail" => csv_escape(row.billing_detail.as_deref().unwrap_or("")),
        "error_message" => csv_escape(row.error_message.as_deref().unwrap_or("")),
        "action_type" => csv_escape(opt_dash(&row.action_type)),
        "endpoint" => csv_escape(&row.endpoint),
        _ => String::new(),
    }
}

fn build_export_csv(rows: &[ExportLogRow], keys: &[&str]) -> String {
    let mut csv = String::from('\u{FEFF}');
    let headers: Vec<&str> = keys
        .iter()
        .filter_map(|k| {
            LOG_EXPORT_COLUMNS
                .iter()
                .find(|c| c.key == *k)
                .map(|c| c.header)
        })
        .collect();
    csv.push_str(&headers.join(","));
    csv.push('\n');
    for row in rows {
        let line = keys
            .iter()
            .map(|k| export_cell(k, row))
            .collect::<Vec<_>>()
            .join(",");
        csv.push_str(&line);
        csv.push('\n');
    }
    csv
}

#[derive(sqlx::FromRow)]
struct ExportLogRow {
    id: i64,
    log_id: String,
    user_id: String,
    model: String,
    prompt_tokens: i64,
    completion_tokens: i64,
    cached_tokens: i64,
    cost: f64,
    latency_ms: i64,
    status_code: i64,
    endpoint: String,
    is_stream: Option<i64>,
    billing_detail: Option<String>,
    created_at: crate::time_system::DbTs,
    user_nickname: Option<String>,
    user_uid: String,
    user_admin_remark: Option<String>,
    task_id: String,
    channel_group_aid: Option<String>,
    is_ha: i64,
    yid: Option<String>,
    sub_channel_name: Option<String>,
    token_name: Option<String>,
    token_kid: Option<String>,
    token_ha: i64,
    token_pg: i64,
    billing_pid: Option<String>,
    forward_eid: Option<String>,
    error_message: Option<String>,
    action_type: Option<String>,
    #[sqlx(default)]
    upstream_request_id: Option<String>,
}

pub async fn export_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Extension(claims): Extension<auth::Claims>,
    Query(query): Query<LogQuery>,
) -> Result<Response, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("仅超级管理员可导出数据".to_string()));
    }

    let keys = resolve_export_keys(query.export_fields.as_deref());
    if keys.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "未选择有效的导出字段" })),
        )
            .into_response());
    }

    let mut q = query.clone();
    if let Some(ref user_id) = q.user_id {
        if user_id != "unknown" {
            q.user_id = Some(resolve_user_filter(&state.db, user_id).await?);
        }
    }

    let tz = request_timedisplay(&state.db, &claims, &headers).await?;
    let (where_clause, binds) = build_log_where(&claims, &q, true, tz);
    let total = fetch_logs_count(&state.db, &where_clause, &binds).await?;

    if total > EXPORT_LIMIT {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("当前筛选条件下共 {} 条数据，超出单次导出上限 {} 条，请缩小时间范围或增加筛选条件后重试", total, EXPORT_LIMIT)
            })),
        )
            .into_response());
    }

    let data_sql = state.db.format_query(&format!(
        "SELECT l.id, COALESCE(l.log_id, '') as log_id, l.user_id, l.model, l.prompt_tokens, l.completion_tokens, l.cached_tokens, \
         l.cost, l.latency_ms, l.status_code, l.endpoint, l.is_stream, \
         l.billing_detail, l.created_at, \
         COALESCE(u.nickname, u.username) as user_nickname, \
         COALESCE(u.uid, '') as user_uid, \
         NULLIF(btrim(COALESCE(u.admin_remark, '')), '') as user_admin_remark, \
         COALESCE(l.task_id, '') as task_id, \
         c.group_aid as channel_group_aid, l.is_ha, cc.yid as yid, cc.name as sub_channel_name, \
         t.name as token_name, t.kid as token_kid, COALESCE(t.high_availability, 0)::bigint as token_ha, \
         CASE WHEN COALESCE(t.only_playground, 0) = 1 OR COALESCE(t.only_playground_2026, 0) = 1 THEN 1::bigint ELSE 0::bigint END as token_pg, \
         l.billing_pid, l.forward_eid, l.error_message, l.action_type, l.upstream_request_id \
         FROM logs l{LOGS_LIST_JOINS} \
         {where_clause} ORDER BY l.created_at DESC LIMIT {EXPORT_LIMIT}"
    ));

    let raw_rows: Vec<ExportLogRow> = {
        let mut q = sqlx::query_as(&data_sql);
        for v in &binds {
            q = q.bind(v);
        }
        q.fetch_all(&state.db.pool).await?
    };

    let csv = build_export_csv(&raw_rows, &keys);

    let filename = format!(
        "usage_logs_{}.csv",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(axum::body::Body::from(csv))
        .unwrap())
}

/// 深翻页：先取 id 再回表 JOIN（排序仅 `created_at DESC`，以走时间索引有序扫描）。
pub(crate) fn deferred_join_page_sql(
    select_sql: &str,
    joins: &str,
    where_clause: &str,
    per_page: i64,
    offset: i64,
) -> String {
    deferred_join_page_sql_for_table("logs", select_sql, joins, where_clause, per_page, offset)
}

/// 支持指定目标表（例如 logs 或冷归档 logs_archive）的深翻页 SQL 生成器
pub(crate) fn deferred_join_page_sql_for_table(
    table_name: &str,
    select_sql: &str,
    joins: &str,
    where_clause: &str,
    per_page: i64,
    offset: i64,
) -> String {
    format!(
        "{select_sql} FROM (
            SELECT l.id FROM {table_name} l{where_clause}
            ORDER BY l.created_at DESC
            LIMIT {per_page} OFFSET {offset}
         ) page
         INNER JOIN {table_name} l ON l.id = page.id
         {joins}
         ORDER BY l.created_at DESC"
    )
}

/// 将数据库存储的时间字符串格式化为可读的北京时间
pub fn format_db_time(raw: &str) -> String {
    use chrono::{DateTime, FixedOffset, Utc};
    let raw = raw.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        let shanghai = FixedOffset::east_opt(8 * 3600).unwrap();
        return dt
            .with_timezone(&shanghai)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
    }
    if let Ok(dt) = DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f%#z") {
        let shanghai = FixedOffset::east_opt(8 * 3600).unwrap();
        return dt
            .with_timezone(&shanghai)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
    }
    if let Ok(dt) = raw.parse::<DateTime<Utc>>() {
        let shanghai = FixedOffset::east_opt(8 * 3600).unwrap();
        return dt
            .with_timezone(&shanghai)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
    }
    raw.split('.').next().unwrap_or(raw).to_string()
}
