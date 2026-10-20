/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 纯内存限额拦截器（DashMap<token_id, QuotaSlot>）
//!
//! 设计要点：
//! - Key = `token_id (i64)`，单令牌唯一槽位，彻底杜绝历史 Key 堆积（旧方案 `token_id:day` 会随天数线性膨胀）。
//! - 跨本地日/周/月直接在槽位内懒刷新（原子 CAS），零锁争用。
//! - 限额预存微单位整数 `u64`，热路径 0 浮点运算、0 除法。
//! - 鉴权中间件直接传入已查出的 `ApiToken`，消除了旧方案的二次数据库查询。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;

use crate::models::ApiToken;
use crate::time_system::{local_period_keys, PeriodKeys};

/// f64 额度 ↔ 微单位整数（精度与 money::MONEY_SCALE 保持一致：1e-6）
#[inline(always)]
fn to_micros(v: f64) -> u64 {
    if v <= 0.0 {
        0
    } else {
        (v * crate::money::MONEY_SCALE).round() as u64
    }
}

/// 限额字段：< 0 表示无限制，转为 None；≥ 0 预转为微单位 u64
#[inline(always)]
fn limit_micros_opt(limit: f64) -> Option<u64> {
    if limit < 0.0 {
        None
    } else {
        Some(to_micros(limit))
    }
}

/// 跨日/周/月周期的已用计数懒刷新辅助：
/// 用 AtomicU64 存储 `day_key` 的哈希（FNV-1a 变体，32bit 足够区分自然日键），
/// 匹配则保留，不匹配则清零——无锁、无堆分配、无 clone。
///
/// 安全性说明：哈希碰撞概率极低（日期格式为 `YYYY-WW`/`YYYY-MM`），
/// 即便碰撞也只是多重置一次计数器（保守不超额），不影响计费安全。
fn key_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[derive(Debug)]
pub struct QuotaSlot {
    day_hash: AtomicU64,
    week_hash: AtomicU64,
    month_hash: AtomicU64,

    daily_used: AtomicU64,
    weekly_used: AtomicU64,
    monthly_used: AtomicU64,
    total_used: AtomicU64,

    daily_limit: Option<u64>,
    weekly_limit: Option<u64>,
    monthly_limit: Option<u64>,
    total_limit: Option<u64>,
}

impl QuotaSlot {
    pub fn from_token(token: &ApiToken, keys: &PeriodKeys) -> Self {
        let daily = if token.last_reset_day.as_deref() == Some(keys.day.as_str()) {
            token.daily_quota_used.max(0.0)
        } else {
            0.0
        };
        let weekly = if token.last_reset_week.as_deref() == Some(keys.week.as_str()) {
            token.weekly_quota_used.max(0.0)
        } else {
            0.0
        };
        let monthly = if token.last_reset_month.as_deref() == Some(keys.month.as_str()) {
            token.monthly_quota_used.max(0.0)
        } else {
            0.0
        };

        Self {
            day_hash: AtomicU64::new(key_hash(&keys.day)),
            week_hash: AtomicU64::new(key_hash(&keys.week)),
            month_hash: AtomicU64::new(key_hash(&keys.month)),
            daily_used: AtomicU64::new(to_micros(daily)),
            weekly_used: AtomicU64::new(to_micros(weekly)),
            monthly_used: AtomicU64::new(to_micros(monthly)),
            total_used: AtomicU64::new(to_micros(token.quota_used.max(0.0))),
            daily_limit: limit_micros_opt(token.daily_quota_limit),
            weekly_limit: limit_micros_opt(token.weekly_quota_limit),
            monthly_limit: limit_micros_opt(token.monthly_quota_limit),
            total_limit: limit_micros_opt(token.quota_limit),
        }
    }

    /// 跨周期懒刷新：无锁 CAS，仅在自然日/周/月切换时清零对应计数器
    pub fn refresh_period(&self, keys: &PeriodKeys) {
        let dh = key_hash(&keys.day);
        if self.day_hash.load(Ordering::Relaxed) != dh
            && self.day_hash.swap(dh, Ordering::SeqCst) != dh
        {
            self.daily_used.store(0, Ordering::SeqCst);
        }
        let wh = key_hash(&keys.week);
        if self.week_hash.load(Ordering::Relaxed) != wh
            && self.week_hash.swap(wh, Ordering::SeqCst) != wh
        {
            self.weekly_used.store(0, Ordering::SeqCst);
        }
        let mh = key_hash(&keys.month);
        if self.month_hash.load(Ordering::Relaxed) != mh
            && self.month_hash.swap(mh, Ordering::SeqCst) != mh
        {
            self.monthly_used.store(0, Ordering::SeqCst);
        }
    }

    /// 纯整数限额检查（无浮点运算）
    #[inline(always)]
    pub fn assert_under_limit(&self) -> Result<(), QuotaMemoryError> {
        if let Some(lim) = self.total_limit {
            if self.total_used.load(Ordering::Relaxed) >= lim {
                return Err(QuotaMemoryError::TotalExhausted);
            }
        }
        if let Some(lim) = self.daily_limit {
            if self.daily_used.load(Ordering::Relaxed) >= lim {
                return Err(QuotaMemoryError::DailyExhausted);
            }
        }
        if let Some(lim) = self.weekly_limit {
            if self.weekly_used.load(Ordering::Relaxed) >= lim {
                return Err(QuotaMemoryError::WeeklyExhausted);
            }
        }
        if let Some(lim) = self.monthly_limit {
            if self.monthly_used.load(Ordering::Relaxed) >= lim {
                return Err(QuotaMemoryError::MonthlyExhausted);
            }
        }
        Ok(())
    }

    /// 原子 CAS 累加并校验限额（允许最后一笔整额累加，可略超，与旧行为一致）
    pub fn check_and_add(&self, add: u64) -> Result<(), QuotaMemoryError> {
        loop {
            let d = self.daily_used.load(Ordering::Relaxed);
            let w = self.weekly_used.load(Ordering::Relaxed);
            let m = self.monthly_used.load(Ordering::Relaxed);
            let t = self.total_used.load(Ordering::Relaxed);

            if let Some(lim) = self.total_limit {
                if t >= lim {
                    return Err(QuotaMemoryError::TotalExhausted);
                }
            }
            if let Some(lim) = self.daily_limit {
                if d >= lim {
                    return Err(QuotaMemoryError::DailyExhausted);
                }
            }
            if let Some(lim) = self.weekly_limit {
                if w >= lim {
                    return Err(QuotaMemoryError::WeeklyExhausted);
                }
            }
            if let Some(lim) = self.monthly_limit {
                if m >= lim {
                    return Err(QuotaMemoryError::MonthlyExhausted);
                }
            }

            // 以 daily_used 为乐观锁锚点，CAS 失败则重试
            if self
                .daily_used
                .compare_exchange_weak(
                    d,
                    d.saturating_add(add),
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                continue;
            }
            self.weekly_used.fetch_add(add, Ordering::SeqCst);
            self.monthly_used.fetch_add(add, Ordering::SeqCst);
            self.total_used.fetch_add(add, Ordering::SeqCst);
            return Ok(());
        }
    }

    /// 强制累加（不校验限额，用于在途结算）
    #[inline(always)]
    pub fn force_add(&self, add: u64) {
        self.daily_used.fetch_add(add, Ordering::SeqCst);
        self.weekly_used.fetch_add(add, Ordering::SeqCst);
        self.monthly_used.fetch_add(add, Ordering::SeqCst);
        self.total_used.fetch_add(add, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone)]
pub struct IncrResult {
    pub amount: f64,
    pub day: String,
    pub week: String,
    pub month: String,
}

#[derive(Debug, thiserror::Error)]
pub enum QuotaMemoryError {
    #[error("今日额度已耗尽")]
    DailyExhausted,
    #[error("本周额度已耗尽")]
    WeeklyExhausted,
    #[error("本月额度已耗尽")]
    MonthlyExhausted,
    #[error("总额度已耗尽")]
    TotalExhausted,
}

#[derive(Clone, Default)]
pub struct MemoryQuotaGuard {
    slots: Arc<DashMap<i64, QuotaSlot>>,
}

impl MemoryQuotaGuard {
    pub fn new() -> Self {
        Self {
            slots: Arc::new(DashMap::new()),
        }
    }

    pub fn clear_all(&self) {
        self.slots.clear();
    }

    /// 只读校验（鉴权中间件）：直接基于已查出的 ApiToken，零额外 DB 查询
    pub fn check_quota(&self, token: &ApiToken, timedisplay: &str) -> Result<(), QuotaMemoryError> {
        let keys = local_period_keys(timedisplay);
        let slot = self
            .slots
            .entry(token.id)
            .or_insert_with(|| QuotaSlot::from_token(token, &keys));
        slot.refresh_period(&keys);
        slot.assert_under_limit()
    }

    /// 热路径：校验并累加（日/周/月/总）
    pub fn check_and_incr_quota(
        &self,
        token: &ApiToken,
        amount: f64,
        timedisplay: &str,
    ) -> Result<IncrResult, QuotaMemoryError> {
        if amount <= 0.0 || token.id <= 0 {
            return Ok(IncrResult {
                amount: 0.0,
                day: String::new(),
                week: String::new(),
                month: String::new(),
            });
        }

        let keys = local_period_keys(timedisplay);
        let slot = self
            .slots
            .entry(token.id)
            .or_insert_with(|| QuotaSlot::from_token(token, &keys));
        slot.refresh_period(&keys);
        let add = to_micros(amount);
        slot.check_and_add(add)?;

        Ok(IncrResult {
            amount,
            day: keys.day,
            week: keys.week,
            month: keys.month,
        })
    }

    /// 结算强制记入：跳过限额校验（中间件已放行的在途请求）
    pub fn force_incr(&self, token: &ApiToken, amount: f64, timedisplay: &str) -> IncrResult {
        if amount <= 0.0 || token.id <= 0 {
            return IncrResult {
                amount: 0.0,
                day: String::new(),
                week: String::new(),
                month: String::new(),
            };
        }

        let keys = local_period_keys(timedisplay);
        let slot = self
            .slots
            .entry(token.id)
            .or_insert_with(|| QuotaSlot::from_token(token, &keys));
        slot.refresh_period(&keys);
        slot.force_add(to_micros(amount));

        IncrResult {
            amount,
            day: keys.day,
            week: keys.week,
            month: keys.month,
        }
    }

    /// 旁路累加（同步落库成功后调用，slot Miss 则静默跳过）
    pub fn apply_incr(&self, token_id: i64, amount: f64) {
        if amount <= 0.0 || token_id <= 0 {
            return;
        }
        if let Some(slot) = self.slots.get(&token_id) {
            slot.force_add(to_micros(amount));
        }
    }

    /// 扣减退款（slot Miss 则静默跳过，下次请求从 DB hydrate 时自然对齐）
    pub fn apply_refund(&self, token_id: i64, amount: f64) {
        if amount <= 0.0 || token_id <= 0 {
            return;
        }
        if let Some(slot) = self.slots.get(&token_id) {
            let sub = to_micros(amount);
            Self::saturating_sub(&slot.daily_used, sub);
            Self::saturating_sub(&slot.weekly_used, sub);
            Self::saturating_sub(&slot.monthly_used, sub);
            Self::saturating_sub(&slot.total_used, sub);
        }
    }

    /// 令牌配置/额度修改、清零、删除时精确 O(1) 移除内存槽位
    pub fn invalidate_token(&self, token_id: i64) {
        if token_id > 0 {
            self.slots.remove(&token_id);
        }
    }

    fn saturating_sub(atom: &AtomicU64, sub: u64) {
        loop {
            let cur = atom.load(Ordering::Relaxed);
            let next = cur.saturating_sub(sub);
            if atom
                .compare_exchange_weak(cur, next, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }
}
