/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 令牌额度累加 / 退款（事务内调用，FOR UPDATE 防并发超用）
//! 热路径优先走内存拦截 + BillingPipeline 异步刷库；管道满时回退本模块同步 consume。

use crate::db::Database;
use crate::models::ApiToken;
use crate::AppState;
use sqlx::{Postgres, Transaction};

/// 在持锁前提下：当前未耗尽则允许整笔累加（最后一笔可略超）；已耗尽则返回 0。
pub fn allowed_consume_amount(
    token: &ApiToken,
    amount: f64,
    now_day: &str,
    now_week: &str,
    now_month: &str,
) -> f64 {
    if amount <= 0.0 {
        return 0.0;
    }
    if token
        .check_quota_limits(now_day, now_week, now_month)
        .is_err()
    {
        return 0.0;
    }
    amount
}

fn consume_sql() -> &'static str {
    "UPDATE api_tokens SET \
     quota_used = quota_used + ?, \
     daily_quota_used = CASE WHEN COALESCE(last_reset_day, '') <> ? THEN ? ELSE daily_quota_used + ? END, \
     weekly_quota_used = CASE WHEN COALESCE(last_reset_week, '') <> ? THEN ? ELSE weekly_quota_used + ? END, \
     monthly_quota_used = CASE WHEN COALESCE(last_reset_month, '') <> ? THEN ? ELSE monthly_quota_used + ? END, \
     last_reset_day = ?, \
     last_reset_week = ?, \
     last_reset_month = ?, \
     updated_at = CURRENT_TIMESTAMP \
     WHERE id = ?"
}

fn refund_sql() -> &'static str {
    "UPDATE api_tokens SET \
     quota_used = GREATEST(0, quota_used - ?), \
     daily_quota_used = CASE WHEN COALESCE(last_reset_day, '') = ? THEN GREATEST(0, daily_quota_used - ?) ELSE daily_quota_used END, \
     weekly_quota_used = CASE WHEN COALESCE(last_reset_week, '') = ? THEN GREATEST(0, weekly_quota_used - ?) ELSE weekly_quota_used END, \
     monthly_quota_used = CASE WHEN COALESCE(last_reset_month, '') = ? THEN GREATEST(0, monthly_quota_used - ?) ELSE monthly_quota_used END, \
     updated_at = CURRENT_TIMESTAMP \
     WHERE id = ?"
}

/// 异步优先切流：内存占用额度 → 投递 MPSC；失败则事务内同步落库（内存已占用，不再 apply_incr）。
pub async fn consume_async_or_sync(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    token: &ApiToken,
    amount: f64,
    timedisplay: &str,
) -> Result<f64, sqlx::Error> {
    if amount <= 0.0 || token.id <= 0 {
        return Ok(0.0);
    }

    let incr = match state
        .quota_memory
        .check_and_incr_quota(token, amount, timedisplay)
    {
        Ok(v) => v,
        Err(e) => {
            // 在途结算：中间件已放行，强制记入以免钱包已扣而令牌未计
            tracing::warn!(
                "[TokenQuota] 限额已满仍强制落账 令牌ID={} 金额={:.6}: {}",
                token.id,
                amount,
                e
            );
            state.quota_memory.force_incr(token, amount, timedisplay)
        }
    };
    if incr.amount <= 0.0 {
        return Ok(0.0);
    }

    let event = super::billing_pipeline::ConsumeEvent {
        token_id: token.id,
        amount: incr.amount,
        day: incr.day.clone(),
        week: incr.week.clone(),
        month: incr.month.clone(),
    };
    if state.billing_ingress.try_enqueue(event) {
        return Ok(incr.amount);
    }

    tracing::warn!(
        "[TokenQuota] BillingPipeline 已满，回退同步落库 令牌ID={} 金额={:.6}",
        token.id,
        incr.amount
    );
    // 内存已累加：同步只写 DB，使用消费时刻锁定的 keys
    consume_db_with_keys(
        &state.db,
        tx,
        token.id,
        incr.amount,
        &incr.day,
        &incr.week,
        &incr.month,
    )
    .await?;
    Ok(incr.amount)
}

/// 仅写 DB（不改内存）；使用调用方锁定的 period keys。
async fn consume_db_with_keys(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    token_id: i64,
    amount: f64,
    day: &str,
    week: &str,
    month: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(&db.format_query(consume_sql()))
        .bind(amount)
        .bind(day)
        .bind(amount)
        .bind(amount)
        .bind(week)
        .bind(amount)
        .bind(amount)
        .bind(month)
        .bind(amount)
        .bind(amount)
        .bind(day)
        .bind(week)
        .bind(month)
        .bind(token_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// 仅写 DB（不改内存）；按当前时刻 tz 算 keys（同步路径兼容）。
async fn consume_db_only(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    token_id: i64,
    amount: f64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    let (now_day, now_week, now_month) = crate::models::quota_period_keys(tz_name);
    consume_db_with_keys(db, tx, token_id, amount, &now_day, &now_week, &now_month).await
}

/// 消费令牌额度：行锁后校验，未耗尽则累加 `amount`，已耗尽则跳过（返回实际累加值）。
pub async fn consume(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    token_id: i64,
    amount: f64,
    tz_name: &str,
) -> Result<f64, sqlx::Error> {
    if amount <= 0.0 || token_id <= 0 {
        return Ok(0.0);
    }

    let token: ApiToken =
        sqlx::query_as(&db.format_query("SELECT * FROM api_tokens WHERE id = ? FOR UPDATE"))
            .bind(token_id)
            .fetch_one(&mut **tx)
            .await?;

    let (now_day, now_week, now_month) = crate::models::quota_period_keys(tz_name);
    let add = allowed_consume_amount(&token, amount, &now_day, &now_week, &now_month);
    if add <= 0.0 {
        tracing::warn!(
            "[TokenQuota] 跳过累加：令牌额度已耗尽 令牌ID={}, 金额={:.6}",
            token_id,
            amount
        );
        return Ok(0.0);
    }

    consume_db_only(db, tx, token_id, add, tz_name).await?;
    Ok(add)
}

/// 退款：扣减总额与当前周期已用量（不低于 0）；调用方应同步 `quota_memory.apply_refund`。
pub async fn refund(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    token_id: i64,
    amount: f64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    if amount <= 0.0 || token_id <= 0 {
        return Ok(());
    }
    let (now_day, now_week, now_month) = crate::models::quota_period_keys(tz_name);
    sqlx::query(&db.format_query(refund_sql()))
        .bind(amount)
        .bind(&now_day)
        .bind(amount)
        .bind(&now_week)
        .bind(amount)
        .bind(&now_month)
        .bind(amount)
        .bind(token_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// 带内存镜像的结算差额（异步任务结算等低频路径）
pub async fn apply_delta_with_memory(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    token_id: i64,
    delta: f64,
    tz_name: &str,
) -> Result<f64, sqlx::Error> {
    if delta > 0.0 {
        let added = consume(&state.db, tx, token_id, delta, tz_name).await?;
        if added > 0.0 {
            state.quota_memory.apply_incr(token_id, added);
            return Ok(added);
        }
        // 限额已满仍强制写库；丢弃内存 slot，下次请求从 DB hydrate，避免漏加或双加
        tracing::warn!(
            "[TokenQuota] 结算追加限额已满仍强制落账 令牌ID={} 金额={:.6}",
            token_id,
            delta
        );
        consume_db_only(&state.db, tx, token_id, delta, tz_name).await?;
        state.quota_memory.invalidate_token(token_id);
        Ok(delta)
    } else if delta < 0.0 {
        let amount = -delta;
        refund(&state.db, tx, token_id, amount, tz_name).await?;
        state.quota_memory.apply_refund(token_id, amount);
        Ok(delta)
    } else {
        Ok(0.0)
    }
}

/// 令牌活跃时间更新防抖节流间隔（30 秒）。
/// 在此窗口内的并发调用直接在内存中去重，避免高频竞争锁定 api_tokens 行级锁。
const LAST_USED_THROTTLE_SECS: u64 = 30;

/// 异步节流更新令牌活跃时间戳（非阻塞，彻底移出主结算事务）
pub fn touch_token_last_used(state: &AppState, token_id: i64) {
    if token_id <= 0 {
        return;
    }
    let now = std::time::Instant::now();
    let should_update = match state.token_last_used_throttle.get_mut(&token_id) {
        Some(mut last_time) => {
            if now.duration_since(*last_time)
                >= std::time::Duration::from_secs(LAST_USED_THROTTLE_SECS)
            {
                *last_time = now;
                true
            } else {
                false
            }
        }
        None => {
            state.token_last_used_throttle.insert(token_id, now);
            true
        }
    };

    if should_update {
        let db = state.db.clone();
        tokio::spawn(async move {
            let res = sqlx::query(&db.format_query(
                "UPDATE api_tokens SET last_used_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?"
            ))
            .bind(token_id)
            .execute(&db.pool)
            .await;
            if let Err(e) = res {
                tracing::warn!(
                    "[TokenQuota] 异步更新令牌活跃时间失败 (token_id={}): {:?}",
                    token_id,
                    e
                );
            }
        });
    }
}
