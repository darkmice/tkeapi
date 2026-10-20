/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::time_system::DbTs;
use serde::{Deserialize, Serialize};

fn default_rate() -> f64 {
    1.0
}

fn default_weight() -> i64 {
    1
}

fn default_status() -> i64 {
    1
}

fn default_currency_rate() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChannelConfig {
    pub id: i64,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    #[serde(skip_serializing)]
    pub api_key: String,
    pub remark: Option<String>,
    pub created_at: DbTs,
    pub updated_at: DbTs,
    #[sqlx(default)]
    pub yid: String,
    #[sqlx(default)]
    pub sort_order: i64,
    pub rate: f64,
    #[sqlx(default)]
    pub priority: i64,
    #[sqlx(default)]
    pub weight: i64,
    #[sqlx(default)]
    pub quota_limit: f64,
    #[sqlx(default)]
    pub quota_used: f64,
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
    /// 日额度刷新时（0-23），站点时区；默认 0 即自然日 00:00
    #[sqlx(default)]
    pub daily_reset_hour: i64,
    /// 日额度刷新分（0-59）
    #[sqlx(default)]
    pub daily_reset_minute: i64,
    /// 到达刷新时刻后再冷却多少分钟才真正刷新日已用（0=立即）
    #[sqlx(default)]
    pub daily_reset_cooldown_minutes: i64,
    /// 1=启用, 0=禁用（迁移后列恒有值；缺列时 sqlx 回退为 0）
    #[sqlx(default)]
    pub status: i64,
    /// 上游分类（复用 channel_categories）
    #[sqlx(default)]
    pub category_id: Option<i64>,
    /// 上游分类列表（JSON 数组，如 "[1,2,3]"）
    #[sqlx(default)]
    pub category_ids: String,
    /// 上游系统：兼容 / 官方 / newapi / Tkeapi / 火山引擎 / 阿里云，空=未选
    #[sqlx(default)]
    pub upstream_system: String,
    /// 已选同步分组名
    #[sqlx(default)]
    pub upstream_group: String,
    /// 自动同步间隔分钟，0=关闭
    #[sqlx(default)]
    pub upstream_sync_interval_minutes: i64,
    /// 同步时叠加到分组倍率的增量，0=不叠加
    #[sqlx(default)]
    pub upstream_sync_rate_add: f64,
    /// 上游计价货币单位，如 CNY/USD，空=同本站
    #[sqlx(default)]
    pub upstream_currency: String,
    /// 本站计价货币单位，如 USD/CNY，空=同系统
    #[sqlx(default)]
    pub site_currency: String,
    /// 货币换算汇率，如 7.2
    #[sqlx(default)]
    pub upstream_currency_rate: f64,
    /// 上次成功同步时间
    #[sqlx(default)]
    pub upstream_synced_at: Option<DbTs>,
}

impl ChannelConfig {
    pub fn parsed_category_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = if !self.category_ids.trim().is_empty() {
            serde_json::from_str::<Vec<i64>>(&self.category_ids).unwrap_or_default()
        } else {
            Vec::new()
        };
        if ids.is_empty() {
            if let Some(cid) = self.category_id {
                ids.push(cid);
            }
        }
        ids
    }

    /// 按本配置的刷新时刻 + 冷却，计算当前额度日键
    pub fn quota_day_key(&self, tz_name: &str) -> String {
        crate::time_system::quota_day_key_with_cutover(
            tz_name,
            self.daily_reset_hour,
            self.daily_reset_minute,
            self.daily_reset_cooldown_minutes,
        )
    }

    pub fn has_available_quota(&self, tz_name: &str, now_week: &str, now_month: &str) -> bool {
        let now_day = self.quota_day_key(tz_name);
        crate::models::channel_quota::has_available_quota(
            self.quota_limit,
            self.quota_used,
            self.daily_quota_limit,
            self.daily_quota_used,
            &self.last_reset_day,
            &now_day,
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
}

#[derive(Debug, Deserialize)]
pub struct CreateChannelConfigRequest {
    pub name: String,
    #[serde(default)]
    pub provider_type: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    pub remark: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default = "default_rate")]
    pub rate: f64,
    #[serde(default)]
    pub priority: i64,
    #[serde(default = "default_weight")]
    pub weight: i64,
    pub quota_limit: Option<f64>,
    pub daily_quota_limit: Option<f64>,
    pub weekly_quota_limit: Option<f64>,
    pub monthly_quota_limit: Option<f64>,
    /// 日额度刷新时（0-23）
    pub daily_reset_hour: Option<i64>,
    /// 日额度刷新分（0-59）
    pub daily_reset_minute: Option<i64>,
    /// 刷新冷却分钟（0=立即）
    pub daily_reset_cooldown_minutes: Option<i64>,
    /// 1=启用, 0=禁用；缺省为启用
    #[serde(default = "default_status")]
    pub status: i64,
    pub category_id: Option<i64>,
    #[serde(default)]
    pub category_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub upstream_system: String,
    #[serde(default)]
    pub upstream_group: String,
    #[serde(default)]
    pub upstream_sync_interval_minutes: i64,
    #[serde(default)]
    pub upstream_sync_rate_add: f64,
    #[serde(default)]
    pub upstream_currency: String,
    #[serde(default)]
    pub site_currency: String,
    #[serde(default = "default_currency_rate")]
    pub upstream_currency_rate: f64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChannelConfigRequest {
    pub name: Option<String>,
    pub provider_type: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub remark: Option<String>,
    pub sort_order: Option<i64>,
    pub rate: Option<f64>,
    pub priority: Option<i64>,
    pub weight: Option<i64>,
    pub quota_limit: Option<f64>,
    pub daily_quota_limit: Option<f64>,
    pub weekly_quota_limit: Option<f64>,
    pub monthly_quota_limit: Option<f64>,
    /// 日额度刷新时（0-23）
    pub daily_reset_hour: Option<i64>,
    /// 日额度刷新分（0-59）
    pub daily_reset_minute: Option<i64>,
    /// 刷新冷却分钟（0=立即）
    pub daily_reset_cooldown_minutes: Option<i64>,
    /// 1=启用, 0=禁用
    pub status: Option<i64>,
    /// None = 未传不改；Some(None) = 清空；Some(Some(id)) = 设置
    #[serde(
        default,
        deserialize_with = "crate::models::user::deserialize_some_option"
    )]
    pub category_id: Option<Option<i64>>,
    /// None = 未传不改；Some(None) = 清空；Some(Some(ids)) = 设置
    #[serde(
        default,
        deserialize_with = "crate::models::user::deserialize_some_option"
    )]
    pub category_ids: Option<Option<Vec<i64>>>,
    pub upstream_system: Option<String>,
    pub upstream_group: Option<String>,
    pub upstream_sync_interval_minutes: Option<i64>,
    pub upstream_sync_rate_add: Option<f64>,
    pub upstream_currency: Option<String>,
    pub site_currency: Option<String>,
    pub upstream_currency_rate: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ChannelConfigListResponse {
    pub data: Vec<ChannelConfigSafe>,
    pub total: i64,
}

/// 渠道分组内针对特定上游配置 (channel_configs.id) 的独立调度覆盖参数（仅支持优先级与权重）。
/// 未设置（None）的字段自动回退使用上游渠道配置的默认值；渠道倍率固定跟随上游配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpstreamScheduleOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BoundChannelSummary {
    pub id: i64,
    pub name: String,
    pub group_aid: Option<String>,
    pub is_ha: bool,
    pub status: i64,
    pub effective_priority: i64,
    pub effective_weight: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_priority: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_weight: Option<i64>,
    pub has_custom_schedule: bool,
}

#[derive(Debug, Serialize)]
pub struct ChannelConfigSafe {
    pub id: i64,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key: String,
    pub has_api_key: bool,
    pub remark: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub yid: String,
    pub sort_order: i64,
    pub rate: f64,
    pub priority: i64,
    pub weight: i64,
    pub quota_limit: f64,
    pub quota_used: f64,
    pub daily_quota_limit: f64,
    pub daily_quota_used: f64,
    pub weekly_quota_limit: f64,
    pub weekly_quota_used: f64,
    pub monthly_quota_limit: f64,
    pub monthly_quota_used: f64,
    pub last_reset_day: String,
    pub last_reset_week: String,
    pub last_reset_month: String,
    pub daily_reset_hour: i64,
    pub daily_reset_minute: i64,
    pub daily_reset_cooldown_minutes: i64,
    pub status: i64,
    pub category_id: Option<i64>,
    #[serde(default)]
    pub category_ids: Vec<i64>,
    pub upstream_system: String,
    pub upstream_group: String,
    pub upstream_sync_interval_minutes: i64,
    pub upstream_sync_rate_add: f64,
    pub upstream_currency: String,
    pub site_currency: String,
    pub upstream_currency_rate: f64,
    pub upstream_synced_at: Option<String>,
    #[serde(default)]
    pub bound_channel_count: i64,
    #[serde(default)]
    pub bound_channels: Vec<BoundChannelSummary>,
}

impl ChannelConfigSafe {
    /// 根据用户角色构建安全的响应数据
    /// - 管理员：返回原文密钥，前端通过 Input.Password 眼睛图标控制显隐
    /// - 非管理员：返回脱敏密钥，保证数据安全
    pub fn from_with_role(c: ChannelConfig, is_admin: bool) -> Self {
        let key = if is_admin {
            c.api_key.clone()
        } else {
            crate::models::channel::mask_secret(&c.api_key)
        };
        let cat_ids = c.parsed_category_ids();
        let cat_id = c.category_id.or_else(|| cat_ids.first().copied());
        ChannelConfigSafe {
            id: c.id,
            name: c.name,
            provider_type: c.provider_type,
            base_url: c.base_url,
            has_api_key: !c.api_key.is_empty(),
            api_key: key,
            remark: c.remark,
            created_at: c.created_at.into_string(),
            updated_at: c.updated_at.into_string(),
            yid: c.yid,
            sort_order: c.sort_order,
            rate: c.rate,
            priority: c.priority,
            weight: c.weight,
            quota_limit: c.quota_limit,
            quota_used: c.quota_used,
            daily_quota_limit: c.daily_quota_limit,
            daily_quota_used: c.daily_quota_used,
            weekly_quota_limit: c.weekly_quota_limit,
            weekly_quota_used: c.weekly_quota_used,
            monthly_quota_limit: c.monthly_quota_limit,
            monthly_quota_used: c.monthly_quota_used,
            last_reset_day: c.last_reset_day,
            last_reset_week: c.last_reset_week,
            last_reset_month: c.last_reset_month,
            daily_reset_hour: c.daily_reset_hour,
            daily_reset_minute: c.daily_reset_minute,
            daily_reset_cooldown_minutes: c.daily_reset_cooldown_minutes,
            status: c.status,
            category_id: cat_id,
            category_ids: cat_ids,
            upstream_system: c.upstream_system,
            upstream_group: c.upstream_group,
            upstream_sync_interval_minutes: c.upstream_sync_interval_minutes,
            upstream_sync_rate_add: c.upstream_sync_rate_add,
            upstream_currency: c.upstream_currency,
            site_currency: c.site_currency,
            upstream_currency_rate: c.upstream_currency_rate,
            upstream_synced_at: c.upstream_synced_at.map(|t| t.into_string()),
            bound_channel_count: 0,
            bound_channels: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChannelConfigSyncLog {
    pub id: i64,
    pub config_id: i64,
    pub upstream_system: String,
    pub upstream_group: String,
    pub upstream_ratio: f64,
    pub rate_add: f64,
    pub upstream_currency: String,
    pub site_currency: String,
    pub currency_rate: f64,
    pub previous_rate: f64,
    pub applied_rate: f64,
    pub sync_interval_minutes: i64,
    pub trigger_type: String,
    pub status: i64,
    pub error_message: String,
    pub created_at: DbTs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfigSyncLogDto {
    pub id: i64,
    pub config_id: i64,
    pub upstream_system: String,
    pub upstream_group: String,
    pub upstream_ratio: f64,
    pub rate_add: f64,
    pub upstream_currency: String,
    pub site_currency: String,
    pub currency_rate: f64,
    pub previous_rate: f64,
    pub applied_rate: f64,
    pub sync_interval_minutes: i64,
    pub trigger_type: String,
    pub status: i64,
    pub error_message: String,
    pub created_at: String,
}

impl From<ChannelConfigSyncLog> for ChannelConfigSyncLogDto {
    fn from(log: ChannelConfigSyncLog) -> Self {
        Self {
            id: log.id,
            config_id: log.config_id,
            upstream_system: log.upstream_system,
            upstream_group: log.upstream_group,
            upstream_ratio: log.upstream_ratio,
            rate_add: log.rate_add,
            upstream_currency: log.upstream_currency,
            site_currency: log.site_currency,
            currency_rate: log.currency_rate,
            previous_rate: log.previous_rate,
            applied_rate: log.applied_rate,
            sync_interval_minutes: log.sync_interval_minutes,
            trigger_type: log.trigger_type,
            status: log.status,
            error_message: log.error_message,
            created_at: log.created_at.into_string(),
        }
    }
}

