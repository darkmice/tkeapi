/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 渠道 / 上游预设额度累加与退款（事务内调用）

use crate::db::Database;
use crate::models::channel_quota::{consume_quota_sql, refund_quota_sql};
use crate::models::quota_period_keys;
use sqlx::{Postgres, Transaction};

async fn execute_consume<'e, E>(
    executor: E,
    sql: &str,
    amount: f64,
    now_day: &str,
    now_week: &str,
    now_month: &str,
    id: i64,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query(sql)
        .bind(amount)
        .bind(now_day)
        .bind(amount)
        .bind(amount)
        .bind(now_week)
        .bind(amount)
        .bind(amount)
        .bind(now_month)
        .bind(amount)
        .bind(amount)
        .bind(now_day)
        .bind(now_week)
        .bind(now_month)
        .bind(id)
        .execute(executor)
        .await?;
    Ok(())
}

async fn consume_with_keys(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id: i64,
    amount: f64,
    now_day: &str,
    now_week: &str,
    now_month: &str,
) -> Result<(), sqlx::Error> {
    let sql = db.format_query(&consume_quota_sql(table));
    execute_consume(&mut **tx, &sql, amount, now_day, now_week, now_month, id).await
}

async fn refund_with_keys(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id: i64,
    amount: f64,
    now_day: &str,
    now_week: &str,
    now_month: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(&db.format_query(&refund_quota_sql(table)))
        .bind(amount)
        .bind(now_day)
        .bind(amount)
        .bind(now_week)
        .bind(amount)
        .bind(now_month)
        .bind(amount)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// 消费：累加总/日/周/月已用量。
/// **共享资源**：`tz_name` 必须传入站点默认时区，禁止使用请求用户 timedisplay。
pub async fn consume(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id: i64,
    amount: f64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    if amount <= 0.0 || id <= 0 {
        return Ok(());
    }
    let (now_day, now_week, now_month) = quota_period_keys(tz_name);
    consume_with_keys(db, tx, table, id, amount, &now_day, &now_week, &now_month).await
}

/// 退款：扣减总/日/周/月已用量
pub async fn refund(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id: i64,
    amount: f64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    if amount <= 0.0 || id <= 0 {
        return Ok(());
    }
    let (now_day, now_week, now_month) = quota_period_keys(tz_name);
    refund_with_keys(db, tx, table, id, amount, &now_day, &now_week, &now_month).await
}

pub async fn consume_channel(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    channel_id: i64,
    amount: f64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    consume(db, tx, "channels", channel_id, amount, tz_name).await
}

pub async fn refund_channel(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    channel_id: i64,
    amount: f64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    refund(db, tx, "channels", channel_id, amount, tz_name).await
}

#[derive(sqlx::FromRow)]
struct ConfigDailyReset {
    daily_reset_hour: i64,
    daily_reset_minute: i64,
    daily_reset_cooldown_minutes: i64,
}

async fn config_period_keys(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    config_id: i64,
    tz_name: &str,
) -> Result<(String, String, String), sqlx::Error> {
    let (_, now_week, now_month) = quota_period_keys(tz_name);
    let row: Option<ConfigDailyReset> = sqlx::query_as(&db.format_query(
        "SELECT daily_reset_hour, daily_reset_minute, daily_reset_cooldown_minutes FROM channel_configs WHERE id = ?",
    ))
    .bind(config_id)
    .fetch_optional(&mut **tx)
    .await?;
    let (h, m, c) = row
        .map(|r| {
            (
                r.daily_reset_hour,
                r.daily_reset_minute,
                r.daily_reset_cooldown_minutes,
            )
        })
        .unwrap_or((0, 0, 0));
    let now_day = crate::time_system::quota_day_key_with_cutover(tz_name, h, m, c);
    Ok((now_day, now_week, now_month))
}

pub async fn consume_config(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    config_id: i64,
    amount: f64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    if amount <= 0.0 || config_id <= 0 {
        return Ok(());
    }
    let (now_day, now_week, now_month) = config_period_keys(db, tx, config_id, tz_name).await?;
    consume_with_keys(
        db,
        tx,
        "channel_configs",
        config_id,
        amount,
        &now_day,
        &now_week,
        &now_month,
    )
    .await
}

pub async fn refund_config(
    db: &Database,
    tx: &mut Transaction<'_, Postgres>,
    config_id: i64,
    amount: f64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    if amount <= 0.0 || config_id <= 0 {
        return Ok(());
    }
    let (now_day, now_week, now_month) = config_period_keys(db, tx, config_id, tz_name).await?;
    refund_with_keys(
        db,
        tx,
        "channel_configs",
        config_id,
        amount,
        &now_day,
        &now_week,
        &now_month,
    )
    .await
}

// ═══════════════════════════════════════════════════════════════
// 渠道与配置额度异步聚合流水线（0 行锁等待，每 2 秒批量刷库）
// ═══════════════════════════════════════════════════════════════

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum QuotaTarget {
    Channel(i64),
    Config(i64),
}

/// 渠道与预设额度内存无锁累加器（微单位整型：amount * 100_000）
#[derive(Default)]
pub struct ChannelQuotaAggregator {
    buffers: DashMap<QuotaTarget, AtomicU64>,
}

impl ChannelQuotaAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn record_channel(&self, channel_id: i64, amount: f64) {
        if channel_id > 0 && amount > 0.0 {
            self.record_inner(QuotaTarget::Channel(channel_id), amount);
        }
    }

    #[inline]
    pub fn record_config(&self, config_id: i64, amount: f64) {
        if config_id > 0 && amount > 0.0 {
            self.record_inner(QuotaTarget::Config(config_id), amount);
        }
    }

    fn record_inner(&self, target: QuotaTarget, amount: f64) {
        let micros = (amount * 100_000.0).round() as u64;
        if micros > 0 {
            self.buffers
                .entry(target)
                .or_default()
                .fetch_add(micros, Ordering::Relaxed);
        }
    }

    /// 提取所有增量并重置为 0
    pub fn drain_deltas(&self) -> Vec<(QuotaTarget, f64)> {
        let mut deltas = Vec::new();
        for entry in self.buffers.iter() {
            let val = entry.swap(0, Ordering::AcqRel);
            if val > 0 {
                let amount = (val as f64) / 100_000.0;
                deltas.push((*entry.key(), amount));
            }
        }
        deltas
    }

    #[inline]
    pub fn clear_target(&self, target: QuotaTarget) {
        self.buffers.remove(&target);
    }
}

static AGGREGATOR: LazyLock<ChannelQuotaAggregator> = LazyLock::new(ChannelQuotaAggregator::new);

/// 内存无锁累加渠道已用额度（主请求网关零阻塞调用）
#[inline]
pub fn record_channel_quota(channel_id: i64, amount: f64) {
    AGGREGATOR.record_channel(channel_id, amount);
}

/// 内存无锁累加预设配置已用额度（主请求网关零阻塞调用）
#[inline]
pub fn record_config_quota(config_id: i64, amount: f64) {
    AGGREGATOR.record_config(config_id, amount);
}

/// 清理指定渠道的内存待刷配额增量（手动清零或删除渠道时调用，防止旧增量回写）
#[inline]
pub fn clear_channel_quota_buffer(channel_id: i64) {
    if channel_id > 0 {
        AGGREGATOR.clear_target(QuotaTarget::Channel(channel_id));
    }
}

/// 清理指定预设配置的内存待刷配额增量（手动清零或删除预设时调用，防止旧增量回写）
#[inline]
pub fn clear_config_quota_buffer(config_id: i64) {
    if config_id > 0 {
        AGGREGATOR.clear_target(QuotaTarget::Config(config_id));
    }
}

/// 批量刷盘：将内存累加值写回 channels 与 channel_configs 表
pub async fn flush_channel_quotas(db: &Database) {
    let deltas = AGGREGATOR.drain_deltas();
    if deltas.is_empty() {
        return;
    }
    let tz_name = crate::relay::relay_settings::get_cached_site_timezone(db).await;
    let (now_day, now_week, now_month) = quota_period_keys(&tz_name);

    for (target, amount) in deltas {
        let (table, id) = match target {
            QuotaTarget::Channel(id) => ("channels", id),
            QuotaTarget::Config(id) => ("channel_configs", id),
        };
        let sql = db.format_query(&crate::models::channel_quota::consume_quota_sql(table));
        if let Err(e) =
            execute_consume(&db.pool, &sql, amount, &now_day, &now_week, &now_month, id).await
        {
            tracing::warn!(
                "[ChannelQuotaPipeline] 批量刷盘失败 (table={}, id={}): {:?}",
                table,
                id,
                e
            );
        }
    }
}

/// 启动后台定期批量刷盘循环（每 2 秒一次，退出时排空）
pub fn start(
    db: Database,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    flush_channel_quotas(&db).await;
                }
                _ = shutdown_rx.changed() => {
                    flush_channel_quotas(&db).await;
                    tracing::info!("[ChannelQuotaPipeline] 渠道配额刷盘协程已排空并退出");
                    return;
                }
            }
        }
    })
}
