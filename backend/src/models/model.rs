/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#![allow(dead_code)]
use crate::time_system::DbTs;
use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Model {
    pub id: i64,
    pub mid: String, // 6位系统识别码，永久不变
    pub name: String,
    pub model_id: String,
    #[sqlx(default)]
    pub original_id: String,
    #[sqlx(default)]
    pub model_id_alias: String, // 模型ID别名映射值，非空时上游请求使用此ID替代model_id
    pub provider_id: Option<i64>,
    pub type_id: Option<i64>,
    pub api_provider_id: Option<i64>,
    pub group_ratios: String, // {"default": 1.0, "vip": 0.8}
    pub billing_rule_id: Option<i64>,
    pub pre_deduction: f64,
    pub is_active: i64,
    pub forward_rule_ids: Option<String>,
    pub enable_log_content: i64,
    #[sqlx(default)]
    pub site_discount: f64, // 折扣限价倍率（开启时折扣不低于此值，1.0=原价）
    #[sqlx(default)]
    pub site_discount_enabled: i64, // 折扣限价开关（0=关，1=开）
    #[sqlx(default)]
    pub global_discount: f64, // 全站折扣倍率
    #[sqlx(default)]
    pub global_discount_enabled: i64, // 全站折扣开关（0=关，1=开）
    /// 折扣限价 / 全站折扣的按天时段规划。`enabled=false` 或空串时使用上面的统一折扣
    #[sqlx(default)]
    #[serde(default)]
    pub discount_schedule: String,
    #[sqlx(default)]
    pub logo: Option<String>,
    #[sqlx(default)]
    pub remark: Option<String>,
    #[sqlx(default)]
    pub description: Option<String>,
    #[sqlx(default)]
    pub feature_attributes: Option<String>,
    /// 1=模型库目录，0=上架模型（`models` 表内应恒为 0）
    #[sqlx(default)]
    #[serde(default)]
    pub is_system: i64,
    /// 来源模型库 mid；自建上架为空
    #[sqlx(default)]
    #[serde(default)]
    pub library_mid: Option<String>,
    /// 1=上架（可调用/进渠道/进广场），0=下架（在模型库）
    #[sqlx(default)]
    #[serde(default)]
    pub is_listed: i64,
    /// 模型库列表：已被上架的份数（非表字段）
    #[sqlx(skip)]
    #[serde(default)]
    pub listed_count: i64,
    /// 模型库条目：catalog=预置目录，unlisted=从运营列表回到仓库的模型行。仓库界面不另标状态。
    #[sqlx(skip)]
    #[serde(default)]
    pub library_kind: Option<String>,
    #[sqlx(default)]
    #[serde(default)]
    pub sort_order: i64,
    pub created_at: DbTs,
    pub updated_at: DbTs,
    #[sqlx(default)]
    pub type_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BillingRule {
    pub id: i64,
    pub name: String,
    pub billing_type: String,
    pub prompt_rate: f64,
    pub completion_rate: f64,
    #[sqlx(default)]
    pub cached_rate: f64,
    #[sqlx(default)]
    pub claude_cache_creation_rate: f64,
    #[sqlx(default)]
    pub claude_cache_read_rate: f64,
    pub fixed_rate: f64,
    pub duration_rate: f64,
    pub billing_rule: String,
    pub pricing_tiers: String,
    pub extended_config: String,
    #[sqlx(default)]
    pub provider_id: Option<i64>,
    #[sqlx(default)]
    pub type_id: Option<i64>,
    pub is_active: i64,
    pub is_system: i64,
    pub pid: String,
    #[sqlx(default)]
    pub pricing_type: String,
    #[sqlx(default)]
    pub sort_order: i64,
    pub created_at: DbTs,
    pub updated_at: DbTs,
    #[sqlx(default)]
    #[serde(default = "default_applied_multiplier")]
    pub applied_multiplier: f64,
    #[sqlx(default)]
    #[serde(skip)]
    pub is_multiplier_applied: bool,
}

fn default_applied_multiplier() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeMultiplier {
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub end: String,
    pub multiplier: f64,
    /// ISO 星期 1=周一 … 7=周日。空或缺省 = 每天
    #[serde(default, deserialize_with = "deserialize_days")]
    pub days: Vec<u8>,
    /// 勾选日全天生效，不看 start/end；与同日时段并存时时段优先
    #[serde(default)]
    pub all_day: bool,
}

fn deserialize_days<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<u8>, D::Error> {
    Ok(Option::<Vec<u8>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Deserialize)]
struct TimeMultiplierConfig {
    #[serde(default)]
    enable_time_multipliers: bool,
    #[serde(default)]
    invert_time_multipliers: bool,
    /// 反向开启时，未命中时段且当天没有全天规则时用此倍率；缺省则兼容旧逻辑 `1/窗口倍率`
    #[serde(default)]
    invert_remainder_multiplier: Option<f64>,
    #[serde(default)]
    time_multipliers: Vec<TimeMultiplier>,
}

fn prev_weekday(weekday: chrono::Weekday) -> chrono::Weekday {
    use chrono::Weekday::*;
    match weekday {
        Mon => Sun,
        Tue => Mon,
        Wed => Tue,
        Thu => Wed,
        Fri => Thu,
        Sat => Fri,
        Sun => Sat,
    }
}

fn weekday_applies(days: &[u8], weekday: chrono::Weekday) -> bool {
    if days.is_empty() {
        return true;
    }
    let n = weekday.number_from_monday() as u8;
    days.contains(&n)
}

fn invert_remainder(multipliers: &[TimeMultiplier], weekday: chrono::Weekday) -> f64 {
    let Some(item) = multipliers
        .iter()
        .find(|item| !item.all_day && weekday_applies(&item.days, weekday))
    else {
        return 1.0;
    };
    if item.multiplier <= 0.0 {
        1.0
    } else {
        1.0 / item.multiplier
    }
}

fn invert_rest_rate(
    multipliers: &[TimeMultiplier],
    weekday: chrono::Weekday,
    remainder: Option<f64>,
) -> f64 {
    if let Some(r) = remainder {
        if finite_non_negative(r) {
            return r;
        }
    }
    if !multipliers
        .iter()
        .any(|item| !item.all_day && weekday_applies(&item.days, weekday))
    {
        return 1.0;
    }
    invert_remainder(multipliers, weekday)
}

fn day_has_all_day(multipliers: &[TimeMultiplier], weekday: chrono::Weekday) -> bool {
    multipliers
        .iter()
        .any(|item| item.all_day && weekday_applies(&item.days, weekday))
}

/// 半开区间 `[start, end)`；跨天窗口归属起始日，次日凌晨仍按起始日的 `days` 判定
fn interval_matches(
    item: &TimeMultiplier,
    current_time: chrono::NaiveTime,
    weekday: chrono::Weekday,
) -> bool {
    if item.all_day {
        return false;
    }
    let Ok(start_t) = chrono::NaiveTime::parse_from_str(&item.start, "%H:%M") else {
        return false;
    };
    let Ok(end_t) = chrono::NaiveTime::parse_from_str(&item.end, "%H:%M") else {
        return false;
    };
    if start_t > end_t {
        if current_time >= start_t {
            weekday_applies(&item.days, weekday)
        } else if current_time < end_t {
            weekday_applies(&item.days, prev_weekday(weekday))
        } else {
            false
        }
    } else {
        current_time >= start_t && current_time < end_t && weekday_applies(&item.days, weekday)
    }
}

fn resolve_time_multiplier(
    multipliers: &[TimeMultiplier],
    invert: bool,
    remainder: Option<f64>,
    current_time: chrono::NaiveTime,
    weekday: chrono::Weekday,
) -> f64 {
    if multipliers.is_empty() {
        return 1.0;
    }
    let has_valid_remainder = remainder.is_some_and(finite_non_negative);
    let matched = multipliers
        .iter()
        .find(|item| interval_matches(item, current_time, weekday));
    if let Some(item) = matched {
        // 未填剩余倍率时保持旧反向：窗口改成 1.00；填了则窗口用填写值（即模型当前价）
        if invert && !has_valid_remainder && !day_has_all_day(multipliers, weekday) {
            return 1.0;
        }
        return if finite_non_negative(item.multiplier) {
            item.multiplier
        } else {
            1.0
        };
    }
    if let Some(item) = multipliers
        .iter()
        .find(|item| item.all_day && weekday_applies(&item.days, weekday))
    {
        return if finite_non_negative(item.multiplier) {
            item.multiplier
        } else {
            1.0
        };
    }
    // 未规划倍率是一等配置：只要填了就生效，不必再开 invert。
    // 旧反向（invert 且未填剩余）仍走倒数。
    if has_valid_remainder || invert {
        invert_rest_rate(multipliers, weekday, remainder)
    } else {
        1.0
    }
}

impl BillingRule {
    fn time_multiplier_config(&self) -> Option<TimeMultiplierConfig> {
        if self.extended_config.is_empty() || self.extended_config == "{}" {
            return None;
        }
        serde_json::from_str(&self.extended_config).ok()
    }

    /// 是否启用时间段价格倍率（须显式 `enable_time_multipliers: true`）
    fn time_multipliers_enabled(&self) -> bool {
        self.time_multiplier_config()
            .map(|c| c.enable_time_multipliers)
            .unwrap_or(false)
    }

    /// 按站点默认时区取「当前时刻」的时段倍率（请求开始时锁定用）
    pub fn get_current_multiplier(&self, default_tz: &str) -> f64 {
        if !self.time_multipliers_enabled() {
            return 1.0;
        }
        let tz: chrono_tz::Tz = default_tz.parse().unwrap_or(chrono_tz::Asia::Shanghai);
        let local_now = chrono::Utc::now().with_timezone(&tz);
        self.multiplier_at_local_datetime(local_now.time(), local_now.weekday())
    }

    /// 按给定本地时钟与星期匹配时段倍率
    pub fn multiplier_at_local_datetime(
        &self,
        current_time: chrono::NaiveTime,
        weekday: chrono::Weekday,
    ) -> f64 {
        let Some(config) = self.time_multiplier_config() else {
            return 1.0;
        };
        if !config.enable_time_multipliers {
            return 1.0;
        }
        resolve_time_multiplier(
            &config.time_multipliers,
            config.invert_time_multipliers,
            config.invert_remainder_multiplier,
            current_time,
            weekday,
        )
    }

    /// 按给定本地时钟（时分秒）匹配时段倍率；半开区间 `[start, end)`，支持跨天
    pub fn multiplier_at_local_time(&self, current_time: chrono::NaiveTime) -> f64 {
        self.multiplier_at_local_datetime(current_time, chrono::Utc::now().weekday())
    }

    /// 将当前时刻倍率写入 `applied_multiplier`（请求开始锁定）
    pub fn lock_time_multiplier(&mut self, default_tz: &str) {
        self.applied_multiplier = self.get_current_multiplier(default_tz);
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DiscountScheduleFile {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    slots: Vec<DiscountSlot>,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscountSlot {
    #[serde(default)]
    start: String,
    #[serde(default)]
    end: String,
    /// 缺省回退统一折扣；显式 0 表示该时段免费
    #[serde(default)]
    site_discount: Option<f64>,
    #[serde(default)]
    global_discount: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_days")]
    days: Vec<u8>,
    #[serde(default)]
    all_day: bool,
}

fn parse_discount_schedule(raw: &str) -> Option<DiscountScheduleFile> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "{}" {
        return None;
    }
    serde_json::from_str(raw).ok()
}

fn discount_slot_window(slot: &DiscountSlot) -> TimeMultiplier {
    TimeMultiplier {
        start: slot.start.clone(),
        end: slot.end.clone(),
        multiplier: 1.0,
        days: slot.days.clone(),
        all_day: slot.all_day,
    }
}

fn matching_discount_slot<'a>(
    slots: &'a [DiscountSlot],
    current_time: chrono::NaiveTime,
    weekday: chrono::Weekday,
) -> Option<&'a DiscountSlot> {
    let windows: Vec<TimeMultiplier> = slots.iter().map(discount_slot_window).collect();
    if let Some((idx, _)) = windows
        .iter()
        .enumerate()
        .find(|(_, item)| interval_matches(item, current_time, weekday))
    {
        return slots.get(idx);
    }
    let idx = windows
        .iter()
        .position(|item| item.all_day && weekday_applies(&item.days, weekday))?;
    slots.get(idx)
}

pub fn discount_schedule_enabled(raw: &str) -> bool {
    parse_discount_schedule(raw)
        .map(|s| s.enabled && !s.slots.is_empty())
        .unwrap_or(false)
}

/// 统一折扣为底。时段规划开启且命中时，只替换已开启的那一项。
pub fn effective_model_discounts(
    site_discount: f64,
    global_discount: f64,
    site_enabled: i64,
    global_enabled: i64,
    schedule_json: &str,
    current_time: chrono::NaiveTime,
    weekday: chrono::Weekday,
) -> (f64, f64) {
    let Some(schedule) = parse_discount_schedule(schedule_json) else {
        return (site_discount, global_discount);
    };
    if !schedule.enabled || global_enabled != 1 {
        return (site_discount, global_discount);
    }
    let Some(slot) = matching_discount_slot(&schedule.slots, current_time, weekday) else {
        return (site_discount, global_discount);
    };
    let site = if site_enabled == 1 {
        scheduled_discount(slot.site_discount).unwrap_or(site_discount)
    } else {
        site_discount
    };
    let global = if global_enabled == 1 {
        scheduled_discount(slot.global_discount).unwrap_or(global_discount)
    } else {
        global_discount
    };
    (site, global)
}

fn finite_non_negative(v: f64) -> bool {
    v.is_finite() && v >= 0.0
}

fn scheduled_discount(raw: Option<f64>) -> Option<f64> {
    raw.filter(|v| finite_non_negative(*v))
}

fn discount_rate_label(v: f64) -> String {
    if v.abs() <= 1e-9 {
        "免费".to_string()
    } else {
        format!("{v:.2}")
    }
}

fn slot_days(days: &[u8]) -> Vec<u8> {
    if days.is_empty() {
        return vec![1, 2, 3, 4, 5, 6, 7];
    }
    let mut out: Vec<u8> = days
        .iter()
        .copied()
        .filter(|d| (1..=7).contains(d))
        .collect();
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        vec![1, 2, 3, 4, 5, 6, 7]
    } else {
        out
    }
}

fn next_iso_day(day: u8) -> u8 {
    if day == 7 {
        1
    } else {
        day + 1
    }
}

struct DiscountInterval {
    start: u16,
    end: u16,
    day: u8,
    index: usize,
}

fn clock_minutes(raw: &str) -> Option<u16> {
    let t = chrono::NaiveTime::parse_from_str(raw, "%H:%M").ok()?;
    Some((t.hour() as u16) * 60 + t.minute() as u16)
}

fn expand_time_intervals(
    all_day: bool,
    start_raw: &str,
    end_raw: &str,
    days: &[u8],
    index: usize,
) -> Result<Vec<DiscountInterval>, String> {
    if all_day {
        return Ok(Vec::new());
    }
    let start =
        clock_minutes(start_raw).ok_or_else(|| format!("第 {} 条时段的开始时间无效", index + 1))?;
    let end =
        clock_minutes(end_raw).ok_or_else(|| format!("第 {} 条时段的结束时间无效", index + 1))?;
    if start == end {
        return Err(format!("第 {} 条时段的起止时间不能相同", index + 1));
    }
    let mut out = Vec::new();
    for day in slot_days(days) {
        if start > end {
            out.push(DiscountInterval {
                start,
                end: 1440,
                day,
                index,
            });
            out.push(DiscountInterval {
                start: 0,
                end,
                day: next_iso_day(day),
                index,
            });
        } else {
            out.push(DiscountInterval {
                start,
                end,
                day,
                index,
            });
        }
    }
    Ok(out)
}

fn check_schedule_conflicts(
    all_day: &[(usize, Vec<u8>)],
    intervals: &[DiscountInterval],
) -> Result<(), String> {
    for i in 0..all_day.len() {
        for j in (i + 1)..all_day.len() {
            if let Some(day) = all_day[i].1.iter().find(|d| all_day[j].1.contains(d)) {
                return Err(format!(
                    "星期 {} 的全天规则重复（第 {} 条与第 {} 条）",
                    day,
                    all_day[i].0 + 1,
                    all_day[j].0 + 1
                ));
            }
        }
    }
    for a in 0..intervals.len() {
        for b in (a + 1)..intervals.len() {
            let left = &intervals[a];
            let right = &intervals[b];
            if left.index == right.index || left.day != right.day {
                continue;
            }
            if left.start.max(right.start) < left.end.min(right.end) {
                return Err(format!(
                    "时段在星期 {} 重叠（第 {} 条与第 {} 条）",
                    left.day,
                    left.index + 1,
                    right.index + 1
                ));
            }
        }
    }
    Ok(())
}

/// 计费规则时段倍率保存前校验。关闭时不检查条目。
pub fn validate_time_multiplier_config(raw: &str) -> Result<(), String> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "{}" || raw == "null" {
        return Ok(());
    }
    let config: TimeMultiplierConfig =
        serde_json::from_str(raw).map_err(|_| "扩展配置格式无效".to_string())?;
    if !config.enable_time_multipliers {
        return Ok(());
    }
    if config.time_multipliers.is_empty() {
        return Err("已开启时段倍率，请至少添加一条时段或全天规则".to_string());
    }
    if let Some(rem) = config.invert_remainder_multiplier {
        if !finite_non_negative(rem) {
            return Err("剩余时段倍率必须大于等于 0（0 为免费）".to_string());
        }
    }
    let mut intervals = Vec::new();
    let mut all_day: Vec<(usize, Vec<u8>)> = Vec::new();
    for (index, item) in config.time_multipliers.iter().enumerate() {
        if !finite_non_negative(item.multiplier) {
            return Err(format!(
                "第 {} 条规则的倍率必须大于等于 0（0 为免费）",
                index + 1
            ));
        }
        if item.all_day {
            all_day.push((index, slot_days(&item.days)));
            continue;
        }
        intervals.extend(expand_time_intervals(
            item.all_day,
            &item.start,
            &item.end,
            &item.days,
            index,
        )?);
    }
    check_schedule_conflicts(&all_day, &intervals)
}

/// 保存前校验。关闭规划时不检查条目。
pub fn validate_discount_schedule(
    raw: &str,
    site_enabled: bool,
    global_enabled: bool,
) -> Result<(), String> {
    let Some(schedule) = parse_discount_schedule(raw) else {
        if raw.trim().is_empty() || raw.trim() == "{}" {
            return Ok(());
        }
        return Err("时段折扣配置无法解析".to_string());
    };
    if !schedule.enabled {
        return Ok(());
    }
    if !global_enabled {
        return Err("开启全站折扣后才能规划时段折扣".to_string());
    }
    if schedule.slots.is_empty() {
        return Err("已开启时段折扣，请至少添加一条时段或全天规则".to_string());
    }
    let mut intervals = Vec::new();
    let mut all_day: Vec<(usize, Vec<u8>)> = Vec::new();
    for (index, slot) in schedule.slots.iter().enumerate() {
        let site_rate = if site_enabled {
            Some(scheduled_discount(slot.site_discount).ok_or_else(|| {
                format!("第 {} 条规则的折扣限价必须大于等于 0（0 为免费）", index + 1)
            })?)
        } else {
            None
        };
        let global_rate = if global_enabled {
            Some(scheduled_discount(slot.global_discount).ok_or_else(|| {
                format!("第 {} 条规则的全站折扣必须大于等于 0（0 为免费）", index + 1)
            })?)
        } else {
            None
        };
        if let (Some(site_rate), Some(global_rate)) = (site_rate, global_rate) {
            if global_rate < site_rate {
                return Err(format!(
                    "第 {} 条规则的全站折扣 {} 低于折扣限价 {}",
                    index + 1,
                    discount_rate_label(global_rate),
                    discount_rate_label(site_rate)
                ));
            }
        }
        if slot.all_day {
            all_day.push((index, slot_days(&slot.days)));
            continue;
        }
        intervals.extend(expand_time_intervals(
            slot.all_day,
            &slot.start,
            &slot.end,
            &slot.days,
            index,
        )?);
    }
    check_schedule_conflicts(&all_day, &intervals)
}

impl Model {
    pub fn apply_discount_schedule_at(
        &mut self,
        current_time: chrono::NaiveTime,
        weekday: chrono::Weekday,
    ) {
        let (site, global) = effective_model_discounts(
            self.site_discount,
            self.global_discount,
            self.site_discount_enabled,
            self.global_discount_enabled,
            &self.discount_schedule,
            current_time,
            weekday,
        );
        self.site_discount = site;
        self.global_discount = global;
    }

    pub fn apply_discount_schedule_now(&mut self, default_tz: &str) {
        let tz: chrono_tz::Tz = default_tz.parse().unwrap_or(chrono_tz::Asia::Shanghai);
        let local = chrono::Utc::now().with_timezone(&tz);
        self.apply_discount_schedule_at(local.time(), local.weekday());
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateBillingRuleRequest {
    pub name: String,
    pub billing_type: String,
    pub prompt_rate: f64,
    pub completion_rate: f64,
    #[serde(default)]
    pub cached_rate: f64,
    #[serde(default)]
    pub claude_cache_creation_rate: f64,
    #[serde(default)]
    pub claude_cache_read_rate: f64,
    pub fixed_rate: f64,
    pub duration_rate: f64,
    pub billing_rule: String,
    pub pricing_tiers: Option<serde_json::Value>,
    pub extended_config: Option<serde_json::Value>,
    pub provider_id: Option<i64>,
    pub type_id: Option<i64>,
    #[serde(default = "default_active")]
    pub is_active: i64,
    pub pid: Option<String>,
    #[serde(default = "default_pricing_type")]
    pub pricing_type: String,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBillingRuleRequest {
    pub name: Option<String>,
    pub billing_type: Option<String>,
    pub prompt_rate: Option<f64>,
    pub completion_rate: Option<f64>,
    pub cached_rate: Option<f64>,
    pub claude_cache_creation_rate: Option<f64>,
    pub claude_cache_read_rate: Option<f64>,
    pub fixed_rate: Option<f64>,
    pub duration_rate: Option<f64>,
    pub billing_rule: Option<String>,
    pub pricing_tiers: Option<serde_json::Value>,
    pub extended_config: Option<serde_json::Value>,
    pub provider_id: Option<i64>,
    pub type_id: Option<i64>,
    pub is_active: Option<i64>,
    pub pid: Option<String>,
    pub pricing_type: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTier {
    pub max_prompt_tokens: f64,
    pub max_completion_tokens: Option<f64>,
    // ---- 常规费率 ----
    pub prompt_rate: f64,
    pub completion_rate: f64,
    /// 缓存命中(非音频)费率(/1M)，#[serde(default)] 兼容旧数据
    #[serde(default)]
    pub cached_rate: f64,
    /// 缓存写入费率(/1M)，GPT-5.6+ 等；未填则写入量并入未缓存输入按 prompt_rate
    #[serde(default)]
    pub cache_write_rate: f64,
    /// 输入(音频)费率(/1M)，豆包聊天分离计价
    #[serde(default)]
    pub audio_prompt_rate: f64,
    /// 缓存命中(音频)费率(/1M)
    #[serde(default)]
    pub audio_cached_rate: f64,
    // ---- 低延迟费率 (service_tier=fast) ----
    /// 低延迟·输入(非音频)费率(/1M)
    #[serde(default)]
    pub fast_prompt_rate: f64,
    /// 低延迟·输出费率(/1M)
    #[serde(default)]
    pub fast_completion_rate: f64,
    /// 低延迟·缓存命中(非音频)费率(/1M)
    #[serde(default)]
    pub fast_cached_rate: f64,
    /// 低延迟·输入(音频)费率(/1M)
    #[serde(default)]
    pub fast_audio_prompt_rate: f64,
    /// 低延迟·缓存命中(音频)费率(/1M)
    #[serde(default)]
    pub fast_audio_cached_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ForwardRule {
    pub id: i64,
    pub name: String,
    pub rule_type: String,
    pub category: String,
    pub config_json: String,
    pub description: Option<String>,
    pub is_active: i64,
    pub is_system: i64, // 1 for built-in, 0 for custom
    pub eid: String,
    #[sqlx(default)]
    pub sort_order: i64,
    pub created_at: DbTs,
    pub updated_at: DbTs,
}

#[derive(Debug, Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub rule_type: String,
    pub category: Option<String>,
    pub config_json: Option<String>,
    pub description: Option<String>,
    #[serde(default = "default_active")]
    pub is_active: i64,
    pub eid: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRuleRequest {
    pub name: Option<String>,
    pub rule_type: Option<String>,
    pub category: Option<String>,
    pub config_json: Option<String>,
    pub description: Option<String>,
    pub is_active: Option<i64>,
    pub eid: Option<String>,
    pub sort_order: Option<i64>,
}

pub fn default_active() -> i64 {
    1
}

pub fn default_pricing_type() -> String {
    "custom".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModelProvider {
    pub id: i64,
    pub name: String,
    #[sqlx(default)]
    pub name_en: String,
    pub sort_order: i64,
    pub is_active: i64,
    #[serde(default)]
    pub is_system: i64,
    pub remark: Option<String>,
    #[sqlx(default)]
    pub logo: Option<String>,
    #[sqlx(default)]
    pub model_count: i64,
    pub created_at: DbTs,
    pub updated_at: DbTs,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModelType {
    pub id: i64,
    pub name: String,
    #[sqlx(default)]
    pub name_en: String,
    pub sort_order: i64,
    pub is_active: i64,
    #[serde(default)]
    pub is_system: i64,
    #[sqlx(default)]
    pub logo: Option<String>,
    #[sqlx(default)]
    pub default_features: Option<String>,
    #[sqlx(default)]
    pub model_count: i64,
    pub created_at: DbTs,
    pub updated_at: DbTs,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ClassificationCount {
    pub id: Option<i64>,
    pub name: String,
    #[sqlx(default)]
    pub name_en: String,
    #[serde(default)]
    pub is_system: i64,
    #[sqlx(default)]
    pub logo: Option<String>,
    pub count: i64,
}

/// 运营模型 / 模型仓库在当前分类筛选下的条数，与列表接口同一口径。
#[derive(Debug, Serialize)]
pub struct SourceModelCounts {
    pub custom: i64,
    pub library: i64,
}

#[derive(Debug, Serialize)]
pub struct ClassificationsResponse {
    pub providers: Vec<ClassificationCount>,
    pub api_providers: Vec<ClassificationCount>,
    pub types: Vec<ClassificationCount>,
    #[serde(default)]
    pub unclassified_providers: i64,
    #[serde(default)]
    pub unclassified_api_providers: i64,
    #[serde(default)]
    pub unclassified_types: i64,
    pub source_counts: SourceModelCounts,
}

#[derive(Debug, Deserialize)]
pub struct CreateModelRequest {
    pub name: String,
    pub model_id: String,
    pub original_id: Option<String>,
    pub model_id_alias: Option<String>,
    pub provider_id: Option<i64>,
    pub type_id: Option<i64>,
    pub api_provider_id: Option<i64>,
    pub group_ratios: Option<serde_json::Value>,
    pub billing_rule_id: Option<i64>,
    pub pre_deduction: Option<f64>,
    pub forward_rule_ids: Option<Vec<i64>>,
    pub is_active: Option<i64>,
    pub enable_log_content: Option<i64>,
    pub site_discount: Option<f64>,
    pub site_discount_enabled: Option<i64>,
    pub global_discount: Option<f64>,
    pub global_discount_enabled: Option<i64>,
    pub discount_schedule: Option<String>,
    pub logo: Option<String>,
    pub remark: Option<String>,
    pub description: Option<String>,
    pub feature_attributes: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateModelRequest {
    pub name: Option<String>,
    pub model_id: Option<String>,
    pub original_id: Option<String>,
    pub model_id_alias: Option<String>,
    pub provider_id: Option<i64>,
    pub type_id: Option<i64>,
    pub api_provider_id: Option<i64>,
    pub group_ratios: Option<serde_json::Value>,
    pub billing_rule_id: Option<i64>,
    pub pre_deduction: Option<f64>,
    pub is_active: Option<i64>,
    pub forward_rule_ids: Option<Vec<i64>>,
    pub enable_log_content: Option<i64>,
    pub site_discount: Option<f64>,
    pub site_discount_enabled: Option<i64>,
    pub global_discount: Option<f64>,
    pub global_discount_enabled: Option<i64>,
    pub discount_schedule: Option<String>,
    pub logo: Option<String>,
    pub remark: Option<String>,
    pub description: Option<String>,
    pub feature_attributes: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ModelListResponse {
    pub data: Vec<Model>,
    pub total: i64,
}

#[derive(Debug, Deserialize)]
pub struct ClassificationRequest {
    pub name: String,
    pub name_en: Option<String>,
    pub sort_order: i64,
    pub is_active: i64,
    pub remark: Option<String>,
    pub logo: Option<String>,
}

#[cfg(test)]
mod discount_schedule_tests {
    use super::{
        effective_model_discounts, resolve_time_multiplier, validate_discount_schedule,
        validate_time_multiplier_config, TimeMultiplier,
    };
    use chrono::{NaiveTime, Weekday};

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn disabled_schedule_keeps_unified_rates() {
        let (site, global) = effective_model_discounts(
            1.0,
            0.8,
            1,
            1,
            r#"{"enabled":false,"slots":[{"all_day":true,"days":[1],"site_discount":0.5,"global_discount":0.5}]}"#,
            at(10, 0),
            Weekday::Mon,
        );
        assert_eq!(site, 1.0);
        assert_eq!(global, 0.8);
    }

    #[test]
    fn slot_overrides_only_enabled_dimension() {
        let raw = r#"{"enabled":true,"slots":[{"start":"09:00","end":"12:00","days":[1],"site_discount":0.9,"global_discount":0.6}]}"#;
        let (site, global) =
            effective_model_discounts(1.0, 0.8, 1, 0, raw, at(10, 0), Weekday::Mon);
        assert_eq!(site, 1.0);
        assert_eq!(global, 0.8);
        let (site, global) =
            effective_model_discounts(1.0, 0.8, 1, 1, raw, at(15, 0), Weekday::Mon);
        assert_eq!(site, 1.0);
        assert_eq!(global, 0.8);
    }

    #[test]
    fn all_day_loses_to_a_narrower_slot() {
        let raw = r#"{"enabled":true,"slots":[
            {"start":"09:00","end":"12:00","days":[1],"site_discount":0.5,"global_discount":0.5},
            {"all_day":true,"days":[1],"site_discount":0.7,"global_discount":0.7}
        ]}"#;
        let (site, _) = effective_model_discounts(1.0, 1.0, 1, 1, raw, at(10, 0), Weekday::Mon);
        assert_eq!(site, 0.5);
        let (site, _) = effective_model_discounts(1.0, 1.0, 1, 1, raw, at(15, 0), Weekday::Mon);
        assert_eq!(site, 0.7);
        let (site, _) = effective_model_discounts(1.0, 1.0, 1, 1, raw, at(10, 0), Weekday::Tue);
        assert_eq!(site, 1.0);
    }

    #[test]
    fn overnight_window_belongs_to_the_start_day() {
        let raw = r#"{"enabled":true,"slots":[{"start":"22:00","end":"02:00","days":[5],"site_discount":0.4,"global_discount":0.4}]}"#;
        let (site, _) = effective_model_discounts(1.0, 1.0, 1, 1, raw, at(23, 0), Weekday::Fri);
        assert_eq!(site, 0.4);
        let (site, _) = effective_model_discounts(1.0, 1.0, 1, 1, raw, at(1, 0), Weekday::Sat);
        assert_eq!(site, 0.4);
        let (site, _) = effective_model_discounts(1.0, 1.0, 1, 1, raw, at(1, 0), Weekday::Fri);
        assert_eq!(site, 1.0);
    }

    #[test]
    fn rejects_overlap_and_floor_inversion() {
        let overlap = r#"{"enabled":true,"slots":[
            {"start":"09:00","end":"12:00","days":[1],"site_discount":1,"global_discount":1},
            {"start":"11:00","end":"13:00","days":[1],"site_discount":1,"global_discount":1}
        ]}"#;
        assert!(validate_discount_schedule(overlap, true, true).is_err());
        let inverted = r#"{"enabled":true,"slots":[{"all_day":true,"days":[1],"site_discount":0.9,"global_discount":0.5}]}"#;
        assert!(validate_discount_schedule(inverted, true, true).is_err());
        assert!(validate_discount_schedule(inverted, true, false).is_err());
    }

    #[test]
    fn zero_slot_is_free_and_omitted_rate_keeps_unified() {
        let free = r#"{"enabled":true,"slots":[{"all_day":true,"days":[1],"site_discount":0,"global_discount":0}]}"#;
        let (site, global) =
            effective_model_discounts(1.0, 0.8, 1, 1, free, at(10, 0), Weekday::Mon);
        assert_eq!(site, 0.0);
        assert_eq!(global, 0.0);
        let (site, global) =
            effective_model_discounts(1.0, 0.8, 1, 1, free, at(10, 0), Weekday::Tue);
        assert_eq!(site, 1.0);
        assert_eq!(global, 0.8);
        assert!(validate_discount_schedule(free, true, true).is_ok());

        let omitted = r#"{"enabled":true,"slots":[{"all_day":true,"days":[1]}]}"#;
        let (site, global) =
            effective_model_discounts(1.0, 0.8, 1, 1, omitted, at(10, 0), Weekday::Mon);
        assert_eq!(site, 1.0);
        assert_eq!(global, 0.8);
        assert!(validate_discount_schedule(omitted, true, true).is_err());

        let raised = r#"{"enabled":true,"slots":[{"all_day":true,"days":[1],"site_discount":0.5,"global_discount":0}]}"#;
        assert!(validate_discount_schedule(raised, true, true).is_err());
    }

    #[test]
    fn remainder_multiplier_applies_to_days_without_narrow_slots() {
        let multipliers = vec![TimeMultiplier {
            start: "00:00".to_string(),
            end: "00:00".to_string(),
            multiplier: 0.5,
            days: vec![6, 7],
            all_day: true,
        }];
        assert_eq!(
            resolve_time_multiplier(&multipliers, false, Some(1.2), at(10, 0), Weekday::Sat),
            0.5
        );
        assert_eq!(
            resolve_time_multiplier(&multipliers, false, Some(1.2), at(10, 0), Weekday::Mon),
            1.2
        );
        assert_eq!(
            resolve_time_multiplier(&multipliers, true, None, at(10, 0), Weekday::Mon),
            1.0
        );
    }

    #[test]
    fn validates_time_multiplier_config_conflicts_and_rates() {
        let valid = r#"{"enable_time_multipliers":true,"invert_remainder_multiplier":0,"time_multipliers":[{"start":"09:00","end":"12:00","multiplier":0.5,"days":[1],"all_day":false}]}"#;
        assert!(validate_time_multiplier_config(valid).is_ok());

        let overlap = r#"{"enable_time_multipliers":true,"time_multipliers":[
            {"start":"09:00","end":"12:00","multiplier":0.8,"days":[1],"all_day":false},
            {"start":"11:00","end":"13:00","multiplier":0.5,"days":[1],"all_day":false}
        ]}"#;
        assert!(validate_time_multiplier_config(overlap).is_err());

        let neg = r#"{"enable_time_multipliers":true,"time_multipliers":[{"all_day":true,"days":[1],"multiplier":-0.5}]}"#;
        assert!(validate_time_multiplier_config(neg).is_err());
    }
}
