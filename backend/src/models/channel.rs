/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#![allow(dead_code)]
use crate::time_system::DbTs;
use serde::{Deserialize, Serialize};

fn default_rate() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Channel {
    pub id: i64,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    #[serde(skip_serializing)]
    pub api_key: String,
    pub models: String,              // JSON array string
    pub model_mapping: String,       // JSON object string
    pub user_groups: String,         // JSON array of user level ids/keys
    pub exclude_user_groups: String, // JSON array of excluded user level ids/keys (blacklist)
    #[sqlx(default)]
    pub group_aid: Option<String>,
    #[sqlx(default)]
    pub preset_id: Option<i64>,
    #[sqlx(default)]
    pub category_id: Option<i64>,
    #[sqlx(default)]
    pub yid: Option<String>,
    pub sort_order: i64,
    pub priority: i64,
    pub weight: i64,
    pub status: i64, // 1=active, 0=disabled, 2=testing
    pub balance: Option<f64>,
    pub max_rps: Option<i64>,
    pub quota_limit: f64, // 渠道使用最大额度（上限），-1 即代表无限额
    pub quota_used: f64,  // 该渠道累计真实消耗金额
    #[sqlx(default)]
    pub daily_quota_limit: f64,
    #[sqlx(default)]
    pub daily_quota_used: f64,
    #[sqlx(default)]
    pub weekly_quota_limit: f64,
    #[sqlx(default)]
    pub weekly_quota_used: f64,
    #[sqlx(default)]
    pub monthly_quota_limit: f64,
    #[sqlx(default)]
    pub monthly_quota_used: f64,
    #[sqlx(default)]
    pub last_reset_day: String,
    #[sqlx(default)]
    pub last_reset_week: String,
    #[sqlx(default)]
    pub last_reset_month: String,
    pub config: String, // JSON extras
    pub rate: f64,
    pub created_at: DbTs,
    pub updated_at: DbTs,
}

impl Channel {
    pub fn get_models(&self) -> Vec<String> {
        serde_json::from_str(&self.models).unwrap_or_default()
    }

    pub fn get_user_groups(&self) -> Vec<String> {
        serde_json::from_str(&self.user_groups).unwrap_or_default()
    }

    pub fn get_exclude_user_groups(&self) -> Vec<String> {
        serde_json::from_str(&self.exclude_user_groups).unwrap_or_default()
    }

    #[inline]
    pub fn has_available_quota(&self, now_day: &str, now_week: &str, now_month: &str) -> bool {
        super::channel_quota::has_available_quota(
            self.quota_limit,
            self.quota_used,
            self.daily_quota_limit,
            self.daily_quota_used,
            &self.last_reset_day,
            now_day,
            self.weekly_quota_limit,
            self.weekly_quota_used,
            &self.last_reset_week,
            now_week,
            self.monthly_quota_limit,
            self.monthly_quota_used,
            &self.last_reset_month,
            now_month,
        )
    }

    #[inline]
    pub fn matches_model(&self, pattern_model: &str, pattern_mids: &[String]) -> bool {
        let m = self.models.trim();
        m.contains(pattern_model) || pattern_mids.iter().any(|p| m.contains(p))
    }

    #[inline]
    pub fn matches_user_group(&self, ug_str: &str, level_str: &str) -> bool {
        let ug = self.user_groups.trim();
        ug.is_empty() || ug == "[]" || ug.contains(ug_str) || ug.contains(level_str)
    }

    pub fn get_model_mapping(&self) -> std::collections::HashMap<String, String> {
        serde_json::from_str(&self.model_mapping).unwrap_or_default()
    }

    pub fn resolve_model(&self, model: &str) -> String {
        let mapping = self.get_model_mapping();

        // 1. Exact match
        if let Some(target) = mapping.get(model) {
            return target.clone();
        }

        // 2. Regex match (keys starting with '/')
        for (pattern, target) in &mapping {
            if pattern.starts_with('/') {
                let p = pattern.trim_matches('/');
                if let Ok(re) = regex::Regex::new(p) {
                    if re.is_match(model) {
                        return target.clone();
                    }
                }
            }
        }

        model.to_string()
    }

    /// 获取渠道的 TOS 存储配置：返回 Some(过期天数) 表示开启，None 表示未开启
    pub fn tos_storage(&self) -> Option<i64> {
        let cfg: serde_json::Value = serde_json::from_str(&self.config).ok()?;
        if !cfg.get("tos_storage_enabled")?.as_bool()? {
            return None;
        }
        Some(
            cfg.get("tos_storage_days")
                .and_then(|d| d.as_i64())
                .unwrap_or(1),
        )
    }

    /// 渠道指定的对象存储厂商（`tos`/`cos`）；空则跟随站点 `default_provider`
    pub fn tos_storage_provider(&self) -> Option<String> {
        let cfg: serde_json::Value = serde_json::from_str(&self.config).ok()?;
        cfg.get("tos_storage_provider")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
    }

    /// 对象存储转存上下文：`(channel_id, 过期天数, 可选厂商)`；未开启则 None
    pub fn storage_persist(&self) -> Option<(i64, i64, Option<String>)> {
        let days = self.tos_storage()?;
        Some((self.id, days, self.tos_storage_provider()))
    }

    /// 渠道是否转存 base64 数据（未显式关闭时默认为开 true）
    pub fn tos_storage_b64(&self) -> bool {
        let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&self.config) else {
            return true;
        };
        cfg.get("tos_storage_b64_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    }

    pub fn check_quota_limits(
        &self,
        now_day: &str,
        now_week: &str,
        now_month: &str,
    ) -> Result<(), String> {
        crate::models::channel_quota::check_quota_limits(
            self.quota_limit,
            self.quota_used,
            self.daily_quota_limit,
            self.daily_quota_used,
            &self.last_reset_day,
            now_day,
            self.weekly_quota_limit,
            self.weekly_quota_used,
            &self.last_reset_week,
            now_week,
            self.monthly_quota_limit,
            self.monthly_quota_used,
            &self.last_reset_month,
            now_month,
        )
    }

    /// 获取当前渠道分组针对指定上游配置 (channel_configs.id) 的独立调度覆盖参数（仅优先级与权重）
    pub fn get_upstream_override(
        &self,
        config_id: i64,
    ) -> Option<crate::models::channel_config::UpstreamScheduleOverride> {
        let cfg_val: serde_json::Value = serde_json::from_str(&self.config).ok()?;
        extract_upstream_override(&cfg_val, config_id)
    }

    /// 将当前渠道分组的独立覆盖参数（优先级、权重）叠加到上游默认配置，未单独设置的字段回退使用上游默认值；渠道倍率固定使用上游配置值
    pub fn resolve_upstream_config(
        &self,
        cfg: &crate::models::ChannelConfig,
    ) -> crate::models::ChannelConfig {
        let mut resolved = cfg.clone();
        if let Some(ov) = self.get_upstream_override(cfg.id) {
            if let Some(priority) = ov.priority {
                resolved.priority = priority;
            }
            if let Some(weight) = ov.weight {
                resolved.weight = weight;
            }
        }
        resolved
    }
}

/// 从渠道分组 `config` JSON 中提取指定上游配置 (`config_id`) 的调度覆盖参数（仅优先级与权重）
pub fn extract_upstream_override(
    cfg_val: &serde_json::Value,
    config_id: i64,
) -> Option<crate::models::channel_config::UpstreamScheduleOverride> {
    let map = cfg_val.get("upstream_overrides")?.as_object()?;
    let entry = map.get(&config_id.to_string())?;
    let mut ov: crate::models::channel_config::UpstreamScheduleOverride =
        serde_json::from_value(entry.clone()).ok()?;
    if let Some(w) = ov.weight {
        if w < 1 {
            ov.weight = Some(1);
        }
    }
    if ov.priority.is_none() && ov.weight.is_none() {
        None
    } else {
        Some(ov)
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub provider_type: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub models: Vec<String>,
    pub model_mapping: Option<std::collections::HashMap<String, String>>,
    pub user_groups: Option<Vec<String>>,
    pub exclude_user_groups: Option<Vec<String>>,
    pub group_aid: Option<String>,
    pub preset_id: Option<i64>,
    pub category_id: Option<i64>,
    pub sort_order: Option<i64>,
    pub priority: Option<i64>,
    pub weight: Option<i64>,
    pub max_rps: Option<i64>,
    pub quota_limit: Option<f64>,
    pub quota_used: Option<f64>,
    pub daily_quota_limit: Option<f64>,
    pub weekly_quota_limit: Option<f64>,
    pub monthly_quota_limit: Option<f64>,
    pub config: Option<serde_json::Value>,
    #[serde(default = "default_rate")]
    pub rate: f64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub provider_type: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub models: Option<Vec<String>>,
    pub model_mapping: Option<std::collections::HashMap<String, String>>,
    pub user_groups: Option<Vec<String>>,
    pub exclude_user_groups: Option<Vec<String>>,
    pub group_aid: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::models::user::deserialize_some_option"
    )]
    pub preset_id: Option<Option<i64>>,
    /// None = 未传不改；Some(None) = 清空；Some(Some(id)) = 设置
    #[serde(
        default,
        deserialize_with = "crate::models::user::deserialize_some_option"
    )]
    pub category_id: Option<Option<i64>>,
    pub sort_order: Option<i64>,
    pub priority: Option<i64>,
    pub weight: Option<i64>,
    pub status: Option<i64>,
    pub max_rps: Option<i64>,
    pub quota_limit: Option<f64>,
    pub quota_used: Option<f64>,
    pub daily_quota_limit: Option<f64>,
    pub weekly_quota_limit: Option<f64>,
    pub monthly_quota_limit: Option<f64>,
    pub config: Option<serde_json::Value>,
    pub rate: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ChannelListResponse {
    pub data: Vec<ChannelSafe>,
    pub total: i64,
}

/// Channel without API key for safe serialization
#[derive(Debug, Serialize)]
pub struct ChannelSafe {
    pub id: i64,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    pub model_mapping: std::collections::HashMap<String, String>,
    pub user_groups: Vec<String>,
    pub exclude_user_groups: Vec<String>,
    pub group_aid: Option<String>,
    pub preset_id: Option<i64>,
    pub category_id: Option<i64>,
    pub sort_order: i64,
    pub priority: i64,
    pub weight: i64,
    pub status: i64,
    pub balance: Option<f64>,
    pub max_rps: Option<i64>,
    pub quota_limit: f64, // 渠道额度上限
    pub quota_used: f64,  // 当前消耗总计
    pub daily_quota_limit: f64,
    pub daily_quota_used: f64,
    pub weekly_quota_limit: f64,
    pub weekly_quota_used: f64,
    pub monthly_quota_limit: f64,
    pub monthly_quota_used: f64,
    pub last_reset_day: String,
    pub last_reset_week: String,
    pub last_reset_month: String,
    pub rate: f64,
    pub created_at: String,
    pub updated_at: String,
    pub config: serde_json::Value,
}

impl ChannelSafe {
    /// 根据用户角色构建安全的响应数据
    /// - 管理员：返回原文密钥，前端通过 Input.Password 眼睛图标控制显隐
    /// - 非管理员：返回脱敏密钥，保证数据安全
    pub fn from_with_role(ch: Channel, is_admin: bool) -> Self {
        let models = ch.get_models();
        let model_mapping = ch.get_model_mapping();
        let user_groups = ch.get_user_groups();
        let exclude_user_groups = ch.get_exclude_user_groups();
        let key = if is_admin {
            ch.api_key.clone()
        } else {
            mask_secret(&ch.api_key)
        };
        Self {
            id: ch.id,
            name: ch.name,
            provider_type: ch.provider_type,
            base_url: ch.base_url,
            api_key: key,
            models,
            model_mapping,
            user_groups,
            exclude_user_groups,
            group_aid: ch.group_aid,
            preset_id: ch.preset_id,
            category_id: ch.category_id,
            sort_order: ch.sort_order,
            priority: ch.priority,
            weight: ch.weight,
            status: ch.status,
            balance: ch.balance,
            max_rps: ch.max_rps,
            quota_limit: ch.quota_limit,
            quota_used: ch.quota_used,
            daily_quota_limit: ch.daily_quota_limit,
            daily_quota_used: ch.daily_quota_used,
            weekly_quota_limit: ch.weekly_quota_limit,
            weekly_quota_used: ch.weekly_quota_used,
            monthly_quota_limit: ch.monthly_quota_limit,
            monthly_quota_used: ch.monthly_quota_used,
            last_reset_day: ch.last_reset_day,
            last_reset_week: ch.last_reset_week,
            last_reset_month: ch.last_reset_month,
            rate: ch.rate,
            created_at: ch.created_at.into_string(),
            updated_at: ch.updated_at.into_string(),
            config: serde_json::from_str(&ch.config).unwrap_or(serde_json::json!({})),
        }
    }
}

/// 渠道分组分类（图片 / 视频 / 聊天等，可自定义）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChannelCategory {
    pub id: i64,
    pub name: String,
    #[sqlx(default)]
    pub name_en: String,
    pub sort_order: i64,
    pub is_active: i64,
    #[serde(default)]
    pub is_system: i64,
    pub created_at: DbTs,
    pub updated_at: DbTs,
}

#[derive(Debug, Deserialize)]
pub struct ChannelCategoryRequest {
    pub name: String,
    pub name_en: Option<String>,
    pub sort_order: i64,
    pub is_active: i64,
}

/// 脱敏工具：保留前4后4字符，中间用 ****** 替代；短于8字符则全部隐藏
pub fn mask_secret(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > 8 {
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[chars.len() - 4..].iter().collect();
        format!("{}******{}", prefix, suffix)
    } else if !s.is_empty() {
        "******".to_string()
    } else {
        String::new()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestChannelRequest {
    pub model: Option<String>,
    pub forward_rule_id: Option<i64>,
    pub sub_channel_id: Option<i64>,
    pub entry_path: Option<String>,
    pub scenario: Option<String>,
    pub custom_body: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ChannelConfig;
    use crate::time_system::DbTs;

    fn sample_config() -> ChannelConfig {
        ChannelConfig {
            id: 42,
            name: "Default Preset".to_string(),
            provider_type: "openai".to_string(),
            base_url: "https://api.example.com".to_string(),
            api_key: "sk-test".to_string(),
            remark: None,
            created_at: DbTs::default(),
            updated_at: DbTs::default(),
            yid: "30042".to_string(),
            sort_order: 0,
            rate: 1.0,
            priority: 5,
            weight: 2,
            quota_limit: -1.0,
            quota_used: 0.0,
            daily_quota_limit: -1.0,
            daily_quota_used: 0.0,
            weekly_quota_limit: -1.0,
            weekly_quota_used: 0.0,
            monthly_quota_limit: -1.0,
            monthly_quota_used: 0.0,
            last_reset_day: String::new(),
            last_reset_week: String::new(),
            last_reset_month: String::new(),
            daily_reset_hour: 0,
            daily_reset_minute: 0,
            daily_reset_cooldown_minutes: 0,
            status: 1,
            category_id: None,
            category_ids: "[]".to_string(),
            upstream_system: String::new(),
            upstream_group: String::new(),
            upstream_sync_interval_minutes: 0,
            upstream_sync_rate_add: 0.0,
            upstream_currency: String::new(),
            site_currency: String::new(),
            upstream_currency_rate: 1.0,
            upstream_synced_at: None,
        }
    }

    fn sample_channel(config_json: &str) -> Channel {
        Channel {
            id: 1,
            name: "Group A".to_string(),
            provider_type: "high_availability_group".to_string(),
            base_url: String::new(),
            api_key: String::new(),
            models: "[]".to_string(),
            model_mapping: "{}".to_string(),
            user_groups: "[]".to_string(),
            exclude_user_groups: "[]".to_string(),
            group_aid: Some("1001".to_string()),
            preset_id: None,
            category_id: None,
            yid: None,
            sort_order: 0,
            priority: 0,
            weight: 1,
            status: 1,
            balance: None,
            max_rps: None,
            quota_limit: -1.0,
            quota_used: 0.0,
            daily_quota_limit: -1.0,
            daily_quota_used: 0.0,
            weekly_quota_limit: -1.0,
            weekly_quota_used: 0.0,
            monthly_quota_limit: -1.0,
            monthly_quota_used: 0.0,
            last_reset_day: String::new(),
            last_reset_week: String::new(),
            last_reset_month: String::new(),
            config: config_json.to_string(),
            rate: 1.0,
            created_at: DbTs::default(),
            updated_at: DbTs::default(),
        }
    }

    #[test]
    fn resolve_upstream_config_falls_back_to_default_when_not_overridden() {
        let cfg = sample_config();
        let ch = sample_channel(r#"{"sub_channels":[42]}"#);
        assert!(ch.get_upstream_override(42).is_none());
        let resolved = ch.resolve_upstream_config(&cfg);
        assert!((resolved.rate - 1.0).abs() < 1e-6);
        assert_eq!(resolved.priority, 5);
        assert_eq!(resolved.weight, 2);
    }

    #[test]
    fn resolve_upstream_config_applies_partial_and_full_overrides() {
        let cfg = sample_config();
        let ch_partial = sample_channel(
            r#"{"sub_channels":[42],"upstream_overrides":{"42":{"priority":20}}}"#,
        );
        let resolved_partial = ch_partial.resolve_upstream_config(&cfg);
        assert!((resolved_partial.rate - 1.0).abs() < 1e-6);
        assert_eq!(resolved_partial.priority, 20);
        assert_eq!(resolved_partial.weight, 2); // falls back to cfg.weight

        let ch_full = sample_channel(
            r#"{"sub_channels":[42],"upstream_overrides":{"42":{"rate":0.75,"priority":10,"weight":8}}}"#,
        );
        let resolved_full = ch_full.resolve_upstream_config(&cfg);
        // 渠道倍率不可按分组覆盖，始终保持上游预设默认值 1.0
        assert!((resolved_full.rate - 1.0).abs() < 1e-6);
        assert_eq!(resolved_full.priority, 10);
        assert_eq!(resolved_full.weight, 8);
    }
}
