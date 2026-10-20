/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 站点货币目录与汇率：默认货币候选、自动 / 手动汇率、跨币种收款报价。
//!
//! 汇率口径统一为「1 默认货币 = rate 目标货币」；自动汇率快照以 USD 为基准存于
//! `settings.currency_fx_snapshot`，每次使用直接读库，多实例一致。

use crate::models::{
    AuxiliaryCurrency, CurrencySettings, FxCurrencyConfig, FxSettings, PaymentChannelsUiSettings,
};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct CurrencyMeta {
    pub code: &'static str,
    pub symbol: &'static str,
    pub unit: &'static str,
    pub name_zh: &'static str,
    pub flag: &'static str,
    /// 对应站点「语言配置」中的语言代码
    pub langs: &'static [&'static str],
}

const fn meta(
    code: &'static str,
    symbol: &'static str,
    unit: &'static str,
    name_zh: &'static str,
    flag: &'static str,
    langs: &'static [&'static str],
) -> CurrencyMeta {
    CurrencyMeta {
        code,
        symbol,
        unit,
        name_zh,
        flag,
        langs,
    }
}

/// 可选货币：站点语言配置（前端 `ALL_LANGUAGES`）每种语言对应的法币，两边需同步
pub const CURRENCY_CATALOG: &[CurrencyMeta] = &[
    meta("CNY", "¥", "元", "人民币", "🇨🇳", &["zh"]),
    meta("HKD", "HK$", "HKD", "港币", "🇭🇰", &["zh-HK"]),
    meta("TWD", "NT$", "TWD", "新台币", "🇹🇼", &["zh-TW"]),
    meta("USD", "$", "USD", "美元", "🇺🇸", &["en"]),
    meta("JPY", "JP¥", "JPY", "日元", "🇯🇵", &["ja"]),
    meta("KRW", "₩", "KRW", "韩元", "🇰🇷", &["ko"]),
    meta("SGD", "S$", "SGD", "新加坡元", "🇸🇬", &["en", "zh"]),
    meta("MYR", "RM", "MYR", "马来西亚林吉特", "🇲🇾", &["ms", "en"]),
    meta("VND", "₫", "VND", "越南盾", "🇻🇳", &["vi"]),
    meta("EUR", "€", "EUR", "欧元", "🇪🇺", &["fr", "de", "es"]),
    meta("BRL", "R$", "BRL", "巴西雷亚尔", "🇧🇷", &["pt"]),
    meta("RUB", "₽", "RUB", "俄罗斯卢布", "🇷🇺", &["ru"]),
    meta("SAR", "﷼", "SAR", "沙特里亚尔", "🇸🇦", &["ar"]),
];

pub const MIN_REFRESH_MINUTES: u32 = 10;
pub const MAX_REFRESH_MINUTES: u32 = 1440;
pub const MAX_MARKUP_PERCENT: f64 = 50.0;
/// 币种百分比：100 = 在线汇率本身，110 = 在线汇率的 110%，50 = 一半
pub const MIN_RATE_PERCENT: f64 = 0.01;
pub const MAX_RATE_PERCENT: f64 = 1000.0;
pub const MAX_STALE_HOURS: u32 = 720;

const SNAPSHOT_KEY: &str = "currency_fx_snapshot";
const PRIMARY_URL: &str = "https://open.er-api.com/v6/latest/USD";
const FALLBACK_URLS: &[&str] = &[
    "https://cdn.jsdelivr.net/npm/@fawazahmed0/currency-api@latest/v1/currencies/usd.json",
    "https://latest.currency-api.pages.dev/v1/currencies/usd.json",
];
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// 自动刷新失败后的重试间隔
const FAILURE_RETRY_SECS: i64 = 300;
/// 后台「立即刷新」最小间隔，避免打爆免费源
const MANUAL_REFRESH_GAP_SECS: i64 = 15;

static LAST_ATTEMPT: AtomicI64 = AtomicI64::new(0);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

pub fn currency_meta(code: &str) -> Option<&'static CurrencyMeta> {
    let code = code.trim();
    CURRENCY_CATALOG
        .iter()
        .find(|c| c.code.eq_ignore_ascii_case(code))
}

fn norm_code(code: &str) -> String {
    code.trim().to_ascii_uppercase()
}

fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

fn positive(v: f64) -> Option<f64> {
    (v.is_finite() && v > 0.0).then_some(v)
}

// ───────────────────────── 渠道收款币种 ─────────────────────────

/// 渠道能收的币种：
/// - 微信 / 支付宝 / 通联：只收人民币
/// - Stripe：默认货币 + 全部目录币种（含不在目录内的旧默认货币）
/// - HyperBC：按后台「加密货币汇率」把默认货币金额折成代币，只能按默认货币下单
pub fn channel_capable_currencies(channel_id: &str, default_currency: &str) -> Vec<String> {
    let mut list = match channel_id {
        "wechat" | "alipay" | "allinpay" | "allinpay_wechat" | "allinpay_alipay" => {
            return vec!["CNY".to_string()];
        }
        "stripe" | "hyperbc" => vec![norm_code(default_currency)],
        _ => return Vec::new(),
    };
    list.retain(|c| !c.is_empty());
    if channel_id == "stripe" {
        for meta in CURRENCY_CATALOG {
            if !list.iter().any(|c| c == meta.code) {
                list.push(meta.code.to_string());
            }
        }
    }
    list
}

/// 渠道实际收款币种：后台勾选 ∩ 渠道能力（保持勾选顺序）；未勾选或交集为空时取全部能力
pub fn channel_effective_currencies(
    channel_id: &str,
    configured: Option<&[String]>,
    default_currency: &str,
) -> Vec<String> {
    let capable = channel_capable_currencies(channel_id, default_currency);
    let mut picked: Vec<String> = Vec::new();
    for code in configured.unwrap_or_default() {
        let code = norm_code(code);
        if capable.contains(&code) && !picked.contains(&code) {
            picked.push(code);
        }
    }
    if picked.is_empty() {
        capable
    } else {
        picked
    }
}

/// 下单收款币种：渠道支持默认货币则直收，否则取渠道首选币种换算
pub fn resolve_pay_currency(default_currency: &str, effective: &[String]) -> Option<String> {
    let default = norm_code(default_currency);
    if effective.contains(&default) {
        Some(default)
    } else {
        effective.first().cloned()
    }
}

// ───────────────────────── 汇率快照 ─────────────────────────

/// 自动汇率快照：1 USD = rates[code]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FxSnapshot {
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub rates: BTreeMap<String, f64>,
    #[serde(default)]
    pub source: String,
    /// 数据源发布时间（unix 秒，0 表示未知）
    #[serde(default)]
    pub provider_updated_at: i64,
    /// 本站拉取时间（unix 秒）
    #[serde(default)]
    pub fetched_at: i64,
}

impl FxSnapshot {
    fn usd_rate(&self, code: &str) -> Option<f64> {
        self.rates.get(code).copied().and_then(positive)
    }

    /// 1 from = ? to
    pub fn cross_rate(&self, from: &str, to: &str) -> Option<f64> {
        let to_rate = self.usd_rate(&norm_code(to))?;
        let from_rate = self.usd_rate(&norm_code(from))?;
        positive(to_rate / from_rate)
    }

    /// 数据年龄以源发布时间为准；超过 max_stale_hours 不再用于收款
    pub fn is_fresh(&self, max_stale_hours: u32, now: i64) -> bool {
        let published = if self.provider_updated_at > 0 {
            self.provider_updated_at
        } else {
            self.fetched_at
        };
        published > 0 && now - published <= i64::from(max_stale_hours.max(1)) * 3600
    }
}

pub async fn load_snapshot(state: &AppState) -> Option<FxSnapshot> {
    let raw: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = ?"),
    )
    .bind(SNAPSHOT_KEY)
    .fetch_optional(&state.db.pool)
    .await
    .ok()
    .flatten();
    raw.and_then(|v| serde_json::from_str(&v).ok())
}

async fn save_snapshot(state: &AppState, snap: &FxSnapshot) -> Result<(), String> {
    let val = serde_json::to_string(snap).map_err(|e| e.to_string())?;
    sqlx::query(&state.db.format_query(
        "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = EXCLUDED.value"
    ))
    .bind(SNAPSHOT_KEY)
    .bind(val)
    .execute(&state.db.pool)
    .await
    .map_err(|e| format!("保存汇率快照失败: {e}"))?;
    Ok(())
}

// ───────────────────────── 有效汇率 ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RateSource {
    Same,
    Manual,
    Auto,
}

#[derive(Debug, Clone, Copy)]
pub struct EffectiveRate {
    pub rate: f64,
    pub source: RateSource,
    /// 可用于收款：同币种、有效手动汇率、或未过期的自动汇率
    pub payable: bool,
}

/// 手动汇率只对保存时的默认货币有效；改了默认货币后需重新确认
fn manual_valid_for(fx: &FxSettings, default_currency: &str) -> bool {
    let base = norm_code(&fx.manual_base);
    base.is_empty() || base == norm_code(default_currency)
}

/// 1 默认货币 = ? target
pub fn effective_rate(
    fx: &FxSettings,
    snap: Option<&FxSnapshot>,
    default_currency: &str,
    target: &str,
    now: i64,
) -> Option<EffectiveRate> {
    let from = norm_code(default_currency);
    let to = norm_code(target);
    if from == to {
        return Some(EffectiveRate {
            rate: 1.0,
            source: RateSource::Same,
            payable: true,
        });
    }
    // 关闭实时汇率时不使用快照或手动汇率，避免国内单币种站点按残留汇率换算收款
    if !fx.live_enabled {
        return None;
    }
    let cfg = fx.currencies.iter().find(|c| norm_code(&c.code) == to);
    let adjust = cfg.map(|c| c.adjust.as_str()).unwrap_or("");
    // 全局固定，或该币种手填：不看实时汇率
    if fx.mode == "manual" || adjust == "manual" || (adjust.is_empty() && cfg.is_some_and(|c| c.manual))
    {
        if !manual_valid_for(fx, &from) {
            return None;
        }
        let rate = cfg.and_then(|c| positive(c.manual_rate))?;
        return Some(EffectiveRate {
            rate,
            source: RateSource::Manual,
            payable: true,
        });
    }
    let snap = snap?;
    let live = snap.cross_rate(&from, &to)?;
    let fresh = snap.is_fresh(fx.max_stale_hours, now);
    let rate = match adjust {
        "percent" => {
            // 100 = 在线汇率，110 = 在线汇率 × 1.1，50 = 在线汇率 × 0.5
            let pct = cfg
                .map(|c| c.adjust_value)
                .filter(|v| v.is_finite())
                .unwrap_or(100.0)
                .clamp(MIN_RATE_PERCENT, MAX_RATE_PERCENT);
            positive(live * pct / 100.0)?
        }
        "amount" => {
            let add = cfg
                .map(|c| c.adjust_value)
                .filter(|v| v.is_finite())
                .unwrap_or(0.0)
                .max(0.0);
            positive(live + add)?
        }
        _ => live,
    };
    Some(EffectiveRate {
        rate,
        source: RateSource::Auto,
        payable: fresh,
    })
}

/// 币种已写明自己的加价或手填汇率时，不再叠旧的整站加价
fn has_explicit_adjust(cfg: Option<&FxCurrencyConfig>) -> bool {
    matches!(
        cfg.map(|c| c.adjust.as_str()),
        Some("percent" | "amount" | "manual")
    )
}

fn markup_factor(fx: &FxSettings) -> f64 {
    let pct = if fx.markup_percent.is_finite() {
        fx.markup_percent.clamp(0.0, MAX_MARKUP_PERCENT)
    } else {
        0.0
    };
    1.0 + pct / 100.0
}

/// 收款汇率：同币种为 1。
/// 币种已配置百分比 / 数值 / 手填时，有效汇率就是收款汇率。
/// 未单独配置的旧数据仍乘整站 `markup_percent`。不可收款时为 None。
pub fn payment_rate(
    fx: &FxSettings,
    snap: Option<&FxSnapshot>,
    default_currency: &str,
    target: &str,
    now: i64,
) -> Option<f64> {
    let eff = effective_rate(fx, snap, default_currency, target, now)?;
    let cfg = fx
        .currencies
        .iter()
        .find(|c| norm_code(&c.code) == norm_code(target));
    match eff.source {
        RateSource::Same => Some(1.0),
        _ if !eff.payable => None,
        _ if has_explicit_adjust(cfg) => positive(eff.rate),
        _ => positive(eff.rate * markup_factor(fx)),
    }
}

/// 按收款币种最小单位向上取整，保证平台不少收；不足一个最小单位返回 None
pub fn convert_ceil(amount: f64, rate: f64, decimals: u32) -> Option<f64> {
    let amount = positive(amount)?;
    let rate = positive(rate)?;
    let scale = 10f64.powi(decimals.min(6) as i32);
    let minor = (amount * rate * scale - 1e-6).ceil();
    (minor.is_finite() && minor >= 1.0).then(|| minor / scale)
}

// ───────────────────────── 设置读写 ─────────────────────────

/// 旧版手填辅助货币作为汇率初始值（自动模式、仅展示兜底），仅在尚未配置 fx 时生效
pub fn seed_legacy_auxiliary(cur: &mut CurrencySettings) {
    if !cur.fx.currencies.is_empty() || cur.auxiliary_currencies.is_empty() {
        return;
    }
    let mut seeded: Vec<FxCurrencyConfig> = Vec::new();
    for aux in &cur.auxiliary_currencies {
        let code = norm_code(&aux.code);
        if currency_meta(&code).is_none() || seeded.iter().any(|c| c.code == code) {
            continue;
        }
        seeded.push(FxCurrencyConfig {
            code,
            manual: false,
            manual_rate: positive(aux.exchange_rate).unwrap_or(0.0),
            adjust: String::new(),
            adjust_value: 0.0,
            display: aux.enabled,
        });
    }
    cur.fx.currencies = seeded;
}

/// 对外展示的辅助货币由实时 / 手动汇率推导，取代旧版手填列表（结构不变，前端无需改动）
pub fn derive_auxiliary(cur: &mut CurrencySettings, snap: Option<&FxSnapshot>, now: i64) {
    seed_legacy_auxiliary(cur);
    if !cur.fx.live_enabled {
        cur.auxiliary_currencies.clear();
        return;
    }
    let default = norm_code(&cur.default_currency);
    let manual_ok = manual_valid_for(&cur.fx, &default);
    let mut list = Vec::new();
    for meta in CURRENCY_CATALOG {
        if meta.code == default {
            continue;
        }
        let cfg = cur.fx.currencies.iter().find(|c| c.code == meta.code);
        if cfg.is_some_and(|c| !c.display) {
            continue;
        }
        let rate = effective_rate(&cur.fx, snap, &default, meta.code, now)
            .map(|e| e.rate)
            .or_else(|| {
                cfg.filter(|_| manual_ok)
                    .and_then(|c| positive(c.manual_rate))
            });
        if let Some(rate) = rate {
            list.push(AuxiliaryCurrency {
                code: meta.code.to_string(),
                symbol: meta.symbol.to_string(),
                exchange_rate: rate,
                enabled: true,
            });
        }
    }
    cur.auxiliary_currencies = list;
}

/// 保存前规范化：默认货币只能从目录选择，符号 / 单位由目录决定；汇率参数夹紧到安全范围。
/// 切换默认货币时清空全部手动汇率（旧汇率以旧默认货币为基准，沿用会按错误汇率收款）。
/// `fx_in_patch` 表示本次提交包含汇率配置
pub fn normalize_currency_settings(
    prev: &CurrencySettings,
    next: &mut CurrencySettings,
    fx_in_patch: bool,
) -> Result<(), String> {
    let default = norm_code(&next.default_currency);
    let prev_default = norm_code(&prev.default_currency);
    let default_changed = !prev_default.is_empty() && default != prev_default;
    match currency_meta(&default) {
        Some(meta) => {
            next.currency_symbol = meta.symbol.to_string();
            next.currency_unit = meta.unit.to_string();
        }
        None if !default.is_empty() && default == prev_default => {}
        None => return Err(format!("不支持的默认货币：{default}")),
    }
    next.default_currency = default.clone();

    let fx = &mut next.fx;
    if fx.mode != "manual" {
        fx.mode = "auto".to_string();
    }
    fx.refresh_interval_minutes = fx
        .refresh_interval_minutes
        .clamp(MIN_REFRESH_MINUTES, MAX_REFRESH_MINUTES);
    fx.max_stale_hours = fx.max_stale_hours.clamp(1, MAX_STALE_HOURS);
    fx.markup_percent = if fx.markup_percent.is_finite() {
        fx.markup_percent.clamp(0.0, MAX_MARKUP_PERCENT)
    } else {
        0.0
    };

    let mut list: Vec<FxCurrencyConfig> = Vec::new();
    for mut c in std::mem::take(&mut fx.currencies) {
        c.code = norm_code(&c.code);
        if c.code == default
            || currency_meta(&c.code).is_none()
            || list.iter().any(|x| x.code == c.code)
        {
            continue;
        }
        c.manual_rate = positive(c.manual_rate).unwrap_or(0.0);
        c.adjust = match c.adjust.as_str() {
            "percent" | "amount" | "manual" => c.adjust,
            _ => String::new(),
        };
        c.adjust_value = if c.adjust_value.is_finite() {
            c.adjust_value
        } else {
            0.0
        };
        if c.adjust == "percent" {
            if !c.adjust_value.is_finite() || c.adjust_value <= 0.0 {
                return Err(format!(
                    "{} 的百分比要大于 0，100 表示按在线汇率折算",
                    c.code
                ));
            }
            c.adjust_value = c.adjust_value.clamp(MIN_RATE_PERCENT, MAX_RATE_PERCENT);
            c.manual = false;
        } else if c.adjust == "amount" {
            c.adjust_value = c.adjust_value.max(0.0);
            c.manual = false;
        } else if c.adjust == "manual" {
            c.manual = true;
            c.adjust_value = 0.0;
        }
        if default_changed {
            // 百分比跟实时汇率走，换默认货币后仍然成立；手填和加数值的口径变了，清空
            c.manual = false;
            c.manual_rate = 0.0;
            if c.adjust != "percent" {
                c.adjust.clear();
                c.adjust_value = 0.0;
            }
        } else if c.manual && c.manual_rate <= 0.0 {
            return Err(format!("{} 已设为手动汇率，请填写大于 0 的汇率", c.code));
        }
        list.push(c);
    }
    fx.currencies = list;

    if fx_in_patch || default_changed {
        fx.manual_base = default;
        next.auxiliary_currencies.clear();
    } else if fx.manual_base.is_empty() {
        fx.manual_base = prev_default;
    }
    Ok(())
}

// ───────────────────────── 后台概览 ─────────────────────────

#[derive(Serialize)]
pub struct FxCatalogItem {
    pub code: &'static str,
    pub symbol: &'static str,
    pub unit: &'static str,
    pub name_zh: &'static str,
    pub flag: &'static str,
    pub langs: &'static [&'static str],
}

#[derive(Serialize)]
pub struct FxSnapshotInfo {
    pub source: String,
    pub provider_updated_at: i64,
    pub fetched_at: i64,
    pub fresh: bool,
}

#[derive(Serialize)]
pub struct FxRateRow {
    pub code: String,
    /// 在线实时汇率：1 默认货币 = auto_rate 该币种
    pub auto_rate: Option<f64>,
    pub manual: bool,
    pub manual_rate: f64,
    /// 空 = 跟随实时；percent / amount / manual
    pub adjust: String,
    /// percent 为在线汇率的百分比；amount 为加数；manual 时为 0，汇率在 manual_rate
    pub adjust_value: f64,
    pub display: bool,
    pub effective_rate: Option<f64>,
    pub source: Option<RateSource>,
    /// 站点当前收款汇率；None 表示当前不可跨币种收款
    pub payment_rate: Option<f64>,
    /// 与 payment_rate 相同，供汇率表「站点使用汇率」列
    pub site_rate: Option<f64>,
}

#[derive(Serialize)]
pub struct FxChannelRow {
    pub id: String,
    pub capable: Vec<String>,
    pub currencies: Vec<String>,
    pub pay_currency: Option<String>,
    pub pay_rate: Option<f64>,
    pub needs_conversion: bool,
}

#[derive(Serialize)]
pub struct FxOverview {
    pub default_currency: String,
    pub catalog: Vec<FxCatalogItem>,
    pub snapshot: Option<FxSnapshotInfo>,
    pub last_error: Option<String>,
    pub rates: Vec<FxRateRow>,
    pub channels: Vec<FxChannelRow>,
}

pub fn build_overview(
    cur: &CurrencySettings,
    snap: Option<&FxSnapshot>,
    ui: &PaymentChannelsUiSettings,
    now: i64,
) -> FxOverview {
    let mut cur = cur.clone();
    seed_legacy_auxiliary(&mut cur);
    let fx = &cur.fx;
    let default = norm_code(&cur.default_currency);

    let catalog = CURRENCY_CATALOG
        .iter()
        .map(|m| FxCatalogItem {
            code: m.code,
            symbol: m.symbol,
            unit: m.unit,
            name_zh: m.name_zh,
            flag: m.flag,
            langs: m.langs,
        })
        .collect();

    let rates = CURRENCY_CATALOG
        .iter()
        .filter(|m| m.code != default)
        .map(|m| {
            let cfg = fx.currencies.iter().find(|c| c.code == m.code);
            let eff = effective_rate(fx, snap, &default, m.code, now);
            let pay = payment_rate(fx, snap, &default, m.code, now);
            FxRateRow {
                code: m.code.to_string(),
                auto_rate: snap.and_then(|s| s.cross_rate(&default, m.code)),
                manual: cfg.is_some_and(|c| c.manual),
                manual_rate: cfg.map(|c| c.manual_rate).unwrap_or(0.0),
                adjust: cfg.map(|c| c.adjust.clone()).unwrap_or_default(),
                adjust_value: cfg.map(|c| c.adjust_value).unwrap_or(0.0),
                display: cfg.is_none_or(|c| c.display),
                effective_rate: eff.map(|e| e.rate),
                source: eff.map(|e| e.source),
                payment_rate: pay,
                site_rate: pay,
            }
        })
        .collect();

    let channels = ui
        .channels
        .iter()
        .map(|c| {
            let currencies = channel_effective_currencies(&c.id, c.currencies.as_deref(), &default);
            let pay_currency = resolve_pay_currency(&default, &currencies);
            let pay_rate = pay_currency
                .as_deref()
                .and_then(|p| payment_rate(fx, snap, &default, p, now));
            FxChannelRow {
                id: c.id.clone(),
                capable: channel_capable_currencies(&c.id, &default),
                needs_conversion: pay_currency.as_deref().is_some_and(|p| p != default),
                currencies,
                pay_currency,
                pay_rate,
            }
        })
        .collect();

    FxOverview {
        default_currency: default,
        catalog,
        snapshot: snap.map(|s| FxSnapshotInfo {
            source: s.source.clone(),
            provider_updated_at: s.provider_updated_at,
            fetched_at: s.fetched_at,
            fresh: s.is_fresh(fx.max_stale_hours, now),
        }),
        last_error: LAST_ERROR.lock().ok().and_then(|e| e.clone()),
        rates,
        channels,
    }
}

// ───────────────────────── 拉取与定时刷新 ─────────────────────────

/// 定时任务（每分钟）：仅在开启实时汇率时，快照早于刷新间隔则拉取
pub async fn run_refresh_tick(state: Arc<AppState>) {
    let cur = crate::api::settings::get_currency_settings(&state).await;
    if !cur.fx.live_enabled {
        return;
    }
    let interval = i64::from(
        cur.fx
            .refresh_interval_minutes
            .clamp(MIN_REFRESH_MINUTES, MAX_REFRESH_MINUTES),
    ) * 60;
    let now = now_unix();
    if load_snapshot(&state)
        .await
        .is_some_and(|s| (0..interval).contains(&(now - s.fetched_at)))
    {
        return;
    }
    if now - LAST_ATTEMPT.load(Ordering::Relaxed) < FAILURE_RETRY_SECS {
        return;
    }
    if let Err(e) = refresh_now(&state).await {
        tracing::warn!("[FxRates] 自动刷新汇率失败: {}", e);
    }
}

/// 后台「立即刷新」
pub async fn refresh_manually(state: &AppState) -> Result<FxSnapshot, String> {
    let cur = crate::api::settings::get_currency_settings(state).await;
    if !cur.fx.live_enabled {
        return Err("实时汇率未开启".to_string());
    }
    if now_unix() - LAST_ATTEMPT.load(Ordering::Relaxed) < MANUAL_REFRESH_GAP_SECS {
        return Err("刷新过于频繁，请稍后再试".to_string());
    }
    refresh_now(state).await
}

async fn refresh_now(state: &AppState) -> Result<FxSnapshot, String> {
    LAST_ATTEMPT.store(now_unix(), Ordering::Relaxed);
    let result = match fetch_snapshot(&state.http_client).await {
        Ok(snap) => save_snapshot(state, &snap).await.map(|_| snap),
        Err(e) => Err(e),
    };
    if let Ok(mut last) = LAST_ERROR.lock() {
        *last = result.as_ref().err().cloned();
    }
    result
}

async fn fetch_snapshot(client: &reqwest::Client) -> Result<FxSnapshot, String> {
    let mut errors = Vec::new();
    match get_json(client, PRIMARY_URL).await {
        Ok(v) => match parse_open_er_api(&v, now_unix()) {
            Ok(snap) => return Ok(snap),
            Err(e) => errors.push(e),
        },
        Err(e) => errors.push(e),
    }
    for url in FALLBACK_URLS {
        match get_json(client, url).await {
            Ok(v) => match parse_currency_api(&v, now_unix()) {
                Ok(snap) => return Ok(snap),
                Err(e) => errors.push(e),
            },
            Err(e) => errors.push(e),
        }
    }
    Err(errors.join("; "))
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    let resp = client
        .get(url)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("{url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("{url}: HTTP {}", resp.status()));
    }
    resp.json().await.map_err(|e| format!("{url}: {e}"))
}

fn build_snapshot<'a>(
    pairs: impl Iterator<Item = (&'a String, &'a serde_json::Value)>,
    source: &str,
    provider_updated_at: i64,
    now: i64,
) -> Result<FxSnapshot, String> {
    let mut rates = BTreeMap::new();
    for (code, rate) in pairs {
        let code = norm_code(code);
        if currency_meta(&code).is_none() {
            continue;
        }
        if let Some(rate) = rate.as_f64().and_then(positive) {
            rates.insert(code, rate);
        }
    }
    if !matches!(rates.get("USD"), Some(v) if (v - 1.0).abs() < 1e-9) {
        return Err(format!("{source}: 基准不是 USD"));
    }
    if rates.len() * 2 < CURRENCY_CATALOG.len() {
        return Err(format!("{source}: 返回币种不足"));
    }
    Ok(FxSnapshot {
        base: "USD".to_string(),
        rates,
        source: source.to_string(),
        provider_updated_at,
        fetched_at: now,
    })
}

fn parse_open_er_api(v: &serde_json::Value, now: i64) -> Result<FxSnapshot, String> {
    const SOURCE: &str = "open.er-api.com";
    if v["result"].as_str() != Some("success") || v["base_code"].as_str() != Some("USD") {
        return Err(format!("{SOURCE}: 响应异常"));
    }
    let rates = v["rates"]
        .as_object()
        .ok_or_else(|| format!("{SOURCE}: 缺少 rates"))?;
    let updated = v["time_last_update_unix"].as_i64().unwrap_or(0);
    build_snapshot(rates.iter(), SOURCE, updated, now)
}

fn parse_currency_api(v: &serde_json::Value, now: i64) -> Result<FxSnapshot, String> {
    const SOURCE: &str = "fawazahmed0/currency-api";
    let rates = v["usd"]
        .as_object()
        .ok_or_else(|| format!("{SOURCE}: 缺少 usd"))?;
    let updated = v["date"]
        .as_str()
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc().timestamp())
        .unwrap_or(0);
    build_snapshot(rates.iter(), SOURCE, updated, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_790_000_000;

    fn snap(fetched_at: i64) -> FxSnapshot {
        let mut rates = BTreeMap::new();
        rates.insert("USD".to_string(), 1.0);
        rates.insert("CNY".to_string(), 7.2);
        rates.insert("JPY".to_string(), 144.0);
        FxSnapshot {
            base: "USD".to_string(),
            rates,
            source: "test".to_string(),
            provider_updated_at: fetched_at,
            fetched_at,
        }
    }

    fn fx_with(currencies: Vec<FxCurrencyConfig>) -> FxSettings {
        FxSettings {
            live_enabled: true,
            currencies,
            ..Default::default()
        }
    }

    fn manual_usd(rate: f64) -> FxCurrencyConfig {
        FxCurrencyConfig {
            code: "USD".to_string(),
            manual: true,
            manual_rate: rate,
            adjust: String::new(),
            adjust_value: 0.0,
            display: true,
        }
    }

    fn adjusted(code: &str, adjust: &str, value: f64) -> FxCurrencyConfig {
        FxCurrencyConfig {
            code: code.to_string(),
            manual: adjust == "manual",
            manual_rate: if adjust == "manual" { value } else { 0.0 },
            adjust: adjust.to_string(),
            adjust_value: if adjust == "manual" { 0.0 } else { value },
            display: true,
        }
    }

    #[test]
    fn cross_rate_uses_usd_base() {
        let s = snap(NOW);
        assert!((s.cross_rate("CNY", "USD").unwrap() - 1.0 / 7.2).abs() < 1e-12);
        assert!((s.cross_rate("cny", "jpy").unwrap() - 20.0).abs() < 1e-9);
        assert!(s.cross_rate("CNY", "EUR").is_none());
    }

    #[test]
    fn auto_rate_payable_only_when_fresh() {
        let fx = FxSettings {
            live_enabled: true,
            ..Default::default()
        };
        let fresh = snap(NOW - 3600);
        let e = effective_rate(&fx, Some(&fresh), "CNY", "USD", NOW).unwrap();
        assert_eq!(e.source, RateSource::Auto);
        assert!(e.payable);

        let stale = snap(NOW - 49 * 3600);
        let e = effective_rate(&fx, Some(&stale), "CNY", "USD", NOW).unwrap();
        assert!(!e.payable);
        assert!(payment_rate(&fx, Some(&stale), "CNY", "USD", NOW).is_none());
        assert!(payment_rate(&fx, None, "CNY", "USD", NOW).is_none());
    }

    #[test]
    fn same_currency_is_one_without_markup() {
        let fx = FxSettings {
            markup_percent: 5.0,
            ..Default::default()
        };
        assert_eq!(payment_rate(&fx, None, "cny", "CNY", NOW), Some(1.0));
    }

    #[test]
    fn live_fx_off_keeps_same_currency_and_blocks_conversion() {
        let mut fx = fx_with(vec![manual_usd(0.15)]);
        fx.live_enabled = false;
        assert_eq!(payment_rate(&fx, Some(&snap(NOW)), "CNY", "CNY", NOW), Some(1.0));
        assert!(effective_rate(&fx, Some(&snap(NOW)), "CNY", "USD", NOW).is_none());
        assert!(payment_rate(&fx, Some(&snap(NOW)), "CNY", "USD", NOW).is_none());
    }

    #[test]
    fn per_currency_manual_overrides_auto() {
        let fx = fx_with(vec![manual_usd(0.15)]);
        let e = effective_rate(&fx, Some(&snap(NOW)), "CNY", "USD", NOW).unwrap();
        assert_eq!(e.source, RateSource::Manual);
        assert_eq!(e.rate, 0.15);
        let jpy = effective_rate(&fx, Some(&snap(NOW)), "CNY", "JPY", NOW).unwrap();
        assert_eq!(jpy.source, RateSource::Auto);
    }

    #[test]
    fn global_manual_mode_never_uses_auto() {
        let fx = FxSettings {
            live_enabled: true,
            mode: "manual".to_string(),
            ..Default::default()
        };
        assert!(effective_rate(&fx, Some(&snap(NOW)), "CNY", "USD", NOW).is_none());
    }

    #[test]
    fn manual_rate_invalid_after_default_currency_change() {
        let mut fx = fx_with(vec![manual_usd(0.15)]);
        fx.manual_base = "CNY".to_string();
        assert!(effective_rate(&fx, None, "CNY", "USD", NOW).is_some());
        assert!(effective_rate(&fx, None, "EUR", "USD", NOW).is_none());
    }

    #[test]
    fn markup_applies_to_cross_currency() {
        let fx = FxSettings {
            live_enabled: true,
            markup_percent: 2.0,
            currencies: vec![manual_usd(0.14)],
            ..Default::default()
        };
        let r = payment_rate(&fx, None, "CNY", "USD", NOW).unwrap();
        assert!((r - 0.1428).abs() < 1e-12);
    }

    #[test]
    fn percent_adjust_follows_live_without_stacking_global_markup() {
        let fresh = snap(NOW);
        let fx = FxSettings {
            live_enabled: true,
            markup_percent: 9.0,
            currencies: vec![adjusted("JPY", "percent", 110.0)],
            ..Default::default()
        };
        let e = effective_rate(&fx, Some(&fresh), "CNY", "JPY", NOW).unwrap();
        assert_eq!(e.source, RateSource::Auto);
        assert!((e.rate - 22.0).abs() < 1e-9);
        let pay = payment_rate(&fx, Some(&fresh), "CNY", "JPY", NOW).unwrap();
        assert!((pay - 22.0).abs() < 1e-9);
        let stale = snap(NOW - 49 * 3600);
        assert!(payment_rate(&fx, Some(&stale), "CNY", "JPY", NOW).is_none());

        let half = fx_with(vec![adjusted("JPY", "percent", 50.0)]);
        let e = effective_rate(&half, Some(&fresh), "CNY", "JPY", NOW).unwrap();
        assert!((e.rate - 10.0).abs() < 1e-9);
    }

    #[test]
    fn amount_adjust_adds_to_live_rate() {
        let fx = fx_with(vec![adjusted("JPY", "amount", 1.5)]);
        let e = effective_rate(&fx, Some(&snap(NOW)), "CNY", "JPY", NOW).unwrap();
        assert!((e.rate - 21.5).abs() < 1e-9);
        assert_eq!(
            payment_rate(&fx, Some(&snap(NOW)), "CNY", "JPY", NOW),
            Some(e.rate)
        );
    }

    #[test]
    fn explicit_manual_does_not_apply_global_markup() {
        let fx = FxSettings {
            live_enabled: true,
            markup_percent: 2.0,
            currencies: vec![adjusted("USD", "manual", 0.14)],
            ..Default::default()
        };
        assert_eq!(payment_rate(&fx, None, "CNY", "USD", NOW), Some(0.14));
    }

    #[test]
    fn convert_ceil_never_undercharges() {
        assert_eq!(convert_ceil(100.0, 0.1428, 2), Some(14.28));
        assert_eq!(convert_ceil(100.0, 0.14281, 2), Some(14.29));
        assert_eq!(convert_ceil(10.0, 20.0, 0), Some(200.0));
        assert_eq!(convert_ceil(10.0, 20.01, 0), Some(201.0));
        assert_eq!(convert_ceil(0.1, 0.01, 2), Some(0.01));
        assert!(convert_ceil(0.0, 1.0, 2).is_none());
        assert!(convert_ceil(1.0, f64::NAN, 2).is_none());
    }

    #[test]
    fn channel_currencies_respect_capability() {
        let picked = vec!["usd".to_string(), "CNY".to_string(), "XXX".to_string()];
        assert_eq!(
            channel_effective_currencies("stripe", Some(picked.as_slice()), "CNY"),
            vec!["USD", "CNY"]
        );
        assert_eq!(
            channel_effective_currencies("wechat", Some(picked.as_slice()), "USD"),
            vec!["CNY"]
        );
        assert_eq!(
            channel_effective_currencies("hyperbc", Some(picked.as_slice()), "JPY"),
            vec!["JPY"]
        );
        let all = channel_effective_currencies("stripe", None, "XYZ");
        assert_eq!(all[0], "XYZ");
        assert_eq!(all.len(), CURRENCY_CATALOG.len() + 1);
        assert!(channel_effective_currencies("unknown", None, "CNY").is_empty());
    }

    #[test]
    fn pay_currency_prefers_default() {
        let list = vec!["USD".to_string(), "CNY".to_string()];
        assert_eq!(resolve_pay_currency("cny", &list).as_deref(), Some("CNY"));
        assert_eq!(resolve_pay_currency("JPY", &list).as_deref(), Some("USD"));
        assert!(resolve_pay_currency("CNY", &[]).is_none());
    }

    #[test]
    fn normalize_fills_symbol_and_rejects_unknown_default() {
        let prev = CurrencySettings {
            default_currency: "CNY".to_string(),
            ..Default::default()
        };
        let mut next = CurrencySettings {
            default_currency: "usd".to_string(),
            currency_symbol: "X".to_string(),
            ..Default::default()
        };
        next.fx.refresh_interval_minutes = 1;
        next.fx.markup_percent = 99.0;
        next.fx.currencies = vec![manual_usd(0.0), manual_usd(0.0)];
        next.fx.currencies[0].code = "eur".to_string();
        next.fx.currencies[0].manual_rate = 0.9;
        normalize_currency_settings(&prev, &mut next, true).unwrap();
        assert_eq!(next.default_currency, "USD");
        assert_eq!(next.currency_symbol, "$");
        assert_eq!(next.fx.refresh_interval_minutes, MIN_REFRESH_MINUTES);
        assert_eq!(next.fx.markup_percent, MAX_MARKUP_PERCENT);
        assert_eq!(next.fx.manual_base, "USD");
        assert_eq!(next.fx.currencies.len(), 1);
        assert_eq!(next.fx.currencies[0].code, "EUR");
        // 切换默认货币：旧手动汇率作废
        assert!(!next.fx.currencies[0].manual);
        assert_eq!(next.fx.currencies[0].manual_rate, 0.0);

        let mut same = CurrencySettings {
            default_currency: "CNY".to_string(),
            ..Default::default()
        };
        same.fx.currencies = vec![manual_usd(0.14)];
        normalize_currency_settings(&prev, &mut same, true).unwrap();
        assert!(same.fx.currencies[0].manual);
        assert_eq!(same.fx.currencies[0].manual_rate, 0.14);
        assert_eq!(same.fx.manual_base, "CNY");

        let mut shifted = CurrencySettings {
            default_currency: "USD".to_string(),
            ..Default::default()
        };
        shifted.fx.currencies = vec![
            adjusted("JPY", "percent", 110.0),
            adjusted("HKD", "percent", 5000.0),
            adjusted("EUR", "amount", 0.2),
        ];
        normalize_currency_settings(&prev, &mut shifted, true).unwrap();
        let jpy = shifted.fx.currencies.iter().find(|c| c.code == "JPY").unwrap();
        assert_eq!(jpy.adjust, "percent");
        assert_eq!(jpy.adjust_value, 110.0);
        let hkd = shifted.fx.currencies.iter().find(|c| c.code == "HKD").unwrap();
        assert_eq!(hkd.adjust_value, MAX_RATE_PERCENT);
        assert!(shifted.fx.currencies.iter().all(|c| c.code != "EUR" || c.adjust.is_empty()));

        let mut bad = CurrencySettings {
            default_currency: "XYZ".to_string(),
            ..Default::default()
        };
        assert!(normalize_currency_settings(&prev, &mut bad, true).is_err());

        let legacy = CurrencySettings {
            default_currency: "XYZ".to_string(),
            ..Default::default()
        };
        let mut keep = legacy.clone();
        assert!(normalize_currency_settings(&legacy, &mut keep, false).is_ok());
    }

    #[test]
    fn normalize_rejects_manual_without_rate() {
        let prev = CurrencySettings::default();
        let mut next = CurrencySettings {
            default_currency: "CNY".to_string(),
            ..Default::default()
        };
        next.fx.currencies = vec![manual_usd(0.0)];
        assert!(normalize_currency_settings(&prev, &mut next, true).is_err());

        let mut zero_pct = CurrencySettings {
            default_currency: "CNY".to_string(),
            ..Default::default()
        };
        zero_pct.fx.currencies = vec![adjusted("JPY", "percent", 0.0)];
        assert!(normalize_currency_settings(&prev, &mut zero_pct, true).is_err());
    }

    #[test]
    fn legacy_auxiliary_seeds_display_rates() {
        let mut cur = CurrencySettings {
            default_currency: "CNY".to_string(),
            auxiliary_currencies: vec![AuxiliaryCurrency {
                code: "usd".to_string(),
                symbol: "$".to_string(),
                exchange_rate: 0.14,
                enabled: true,
            }],
            ..Default::default()
        };
        derive_auxiliary(&mut cur, None, NOW);
        assert!(cur.auxiliary_currencies.is_empty());
        cur.fx.live_enabled = true;
        derive_auxiliary(&mut cur, None, NOW);
        assert_eq!(cur.auxiliary_currencies.len(), 1);
        assert_eq!(cur.auxiliary_currencies[0].code, "USD");
        assert_eq!(cur.auxiliary_currencies[0].exchange_rate, 0.14);
        let fx = &cur.fx;
        assert!(payment_rate(fx, None, "CNY", "USD", NOW).is_none());
    }

    #[test]
    fn parses_both_providers() {
        let er = serde_json::json!({
            "result": "success",
            "base_code": "USD",
            "time_last_update_unix": NOW,
            "rates": {"USD": 1, "CNY": 7.2, "JPY": 144.0, "EUR": 0.9, "KRW": 1300.0,
                      "HKD": 7.8, "SGD": 1.3, "MYR": 4.1, "XYZ": 2.0}
        });
        let s = parse_open_er_api(&er, NOW).unwrap();
        assert_eq!(s.provider_updated_at, NOW);
        assert!(!s.rates.contains_key("XYZ"));

        let ca = serde_json::json!({
            "date": "2026-10-03",
            "usd": {"usd": 1, "cny": 7.2, "jpy": 144.0, "eur": 0.9, "krw": 1300.0, "hkd": 7.8, "sgd": 1.3, "myr": 4.1}
        });
        let s = parse_currency_api(&ca, NOW).unwrap();
        assert_eq!(s.rates.get("CNY"), Some(&7.2));
        assert!(s.provider_updated_at > 0);

        let bad = serde_json::json!({"result": "error"});
        assert!(parse_open_er_api(&bad, NOW).is_err());
    }
}
