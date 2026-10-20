/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! NewAPI 等上游分组倍率拉取与应用到渠道预设 `rate`。

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const SYSTEM_NEWAPI: &str = "newapi";

pub const UPSTREAM_SYSTEMS: &[&str] = &[
    "兼容",
    "官方",
    "newapi",
    "Tkeapi",
    "akeapi",
    "火山引擎",
    "阿里云",
];

#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamGroupRatio {
    pub name: String,
    pub ratio: f64,
    pub label: String,
}

#[derive(Debug, Deserialize)]
struct NewapiPricingBody {
    #[serde(default)]
    success: Option<bool>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    group_ratio: Option<BTreeMap<String, f64>>,
    #[serde(default)]
    usable_group: Option<BTreeMap<String, String>>,
    #[serde(default)]
    data: Option<Value>,
}

pub fn is_known_upstream_system(value: &str) -> bool {
    value.is_empty()
        || UPSTREAM_SYSTEMS
            .iter()
            .any(|&s| s.eq_ignore_ascii_case(value))
}

pub fn normalize_newapi_origin(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("请填写端点基础地址".into());
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err("端点基础地址必须以 http:// 或 https:// 开头".into());
    }
    let mut origin = trimmed.to_string();
    loop {
        let lower = origin.to_ascii_lowercase();
        let stripped = ["/v1", "/api"].iter().find_map(|suffix| {
            if origin.len() > suffix.len() && lower.ends_with(suffix) {
                Some(
                    origin[..origin.len() - suffix.len()]
                        .trim_end_matches('/')
                        .to_string(),
                )
            } else {
                None
            }
        });
        match stripped {
            Some(next) => origin = next,
            None => break,
        }
    }
    Ok(origin)
}

pub fn newapi_pricing_url(base_url: &str) -> Result<String, String> {
    Ok(format!(
        "{}/api/pricing",
        normalize_newapi_origin(base_url)?
    ))
}

/// 上游货币与本站计价货币换算：
/// - 上游为 CNY / 人民币，本站为 USD / 美元：本站美元折扣倍率 = 上游倍率 / 汇率 (例: 3.0 RMB / 7.2 ≈ 0.4167)
/// - 上游为 USD / 美元，本站为 CNY / 人民币：本站人民币倍率 = 上游倍率 * 汇率 (例: 0.5 USD * 7.2 = 3.6)
/// - 币种相同或未设置：保持原倍率 1:1
/// - 其他币种情况：若指定了非 1.0 的有效汇率，默认按上游倍率 / 汇率换算
pub fn convert_currency_ratio(
    group_ratio: f64,
    upstream_currency: &str,
    site_currency: &str,
    currency_rate: f64,
) -> f64 {
    let u = upstream_currency.trim();
    let s = site_currency.trim();
    if u.is_empty() || s.is_empty() || u.eq_ignore_ascii_case(s) {
        return group_ratio;
    }
    let rate = if currency_rate.is_finite() && currency_rate > 0.0 {
        currency_rate
    } else {
        1.0
    };
    let is_cny = |c: &str| {
        c.eq_ignore_ascii_case("CNY") || c.eq_ignore_ascii_case("RMB") || c == "人民币" || c == "元"
    };
    let is_usd = |c: &str| c.eq_ignore_ascii_case("USD") || c == "美元" || c == "$";

    if is_cny(u) && is_usd(s) {
        group_ratio / rate
    } else if is_usd(u) && is_cny(s) {
        group_ratio * rate
    } else {
        group_ratio / rate
    }
}

pub fn applied_channel_rate(
    group_ratio: f64,
    rate_add: f64,
    upstream_currency: &str,
    site_currency: &str,
    currency_rate: f64,
) -> f64 {
    let add = if rate_add.is_finite() && rate_add > 0.0 {
        rate_add
    } else {
        0.0
    };
    let ratio = if group_ratio.is_finite() {
        group_ratio
    } else {
        0.0
    };
    let converted = convert_currency_ratio(ratio, upstream_currency, site_currency, currency_rate);
    let total = (converted + add).max(0.0);
    (total * 10000.0).round() / 10000.0
}

pub fn is_sync_due(synced_at: Option<&str>, interval_minutes: i64, now: DateTime<Utc>) -> bool {
    if interval_minutes <= 0 {
        return false;
    }
    let Some(raw) = synced_at.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    let Ok(last) = DateTime::parse_from_rfc3339(raw) else {
        return true;
    };
    now.signed_duration_since(last.with_timezone(&Utc))
        >= Duration::minutes(interval_minutes)
}

pub fn parse_newapi_groups(body: &str) -> Result<Vec<UpstreamGroupRatio>, String> {
    let parsed: NewapiPricingBody =
        serde_json::from_str(body).map_err(|e| format!("上游定价响应不是 JSON: {e}"))?;
    if parsed.success == Some(false) {
        let msg = parsed
            .message
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "上游返回失败".into());
        return Err(msg);
    }

    let mut ratios = parsed.group_ratio.unwrap_or_default();
    let mut labels = parsed.usable_group.unwrap_or_default();
    if let Some(Value::Object(data)) = parsed.data {
        if ratios.is_empty() {
            if let Some(Value::Object(map)) = data.get("group_ratio") {
                for (k, v) in map {
                    if let Some(n) = v.as_f64() {
                        ratios.insert(k.clone(), n);
                    }
                }
            }
        }
        if labels.is_empty() {
            if let Some(Value::Object(map)) = data.get("usable_group") {
                for (k, v) in map {
                    if let Some(s) = v.as_str() {
                        labels.insert(k.clone(), s.to_string());
                    }
                }
            }
        }
    }

    if ratios.is_empty() {
        return Err("上游未返回分组倍率".into());
    }

    let mut groups: Vec<UpstreamGroupRatio> = ratios
        .into_iter()
        .map(|(name, ratio)| {
            let label = labels.get(&name).cloned().unwrap_or_default();
            UpstreamGroupRatio { name, ratio, label }
        })
        .collect();
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(groups)
}
