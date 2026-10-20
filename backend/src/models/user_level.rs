/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::time_system::DbTs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserLevel {
    pub id: i64,
    pub name: String,
    pub group_key: String,
    pub discount: f64,
    #[sqlx(default)]
    pub discount_type: i64,
    #[sqlx(default)]
    pub commission_ratio: f64,
    /// consumption=按系统钱包实际消费，recharge=按充值入账
    #[sqlx(default)]
    #[serde(default = "default_commission_basis")]
    pub commission_basis: String,
    /// 佣金钱包单次划转下限，0 为不限制
    #[sqlx(default)]
    pub commission_transfer_min: f64,
    /// 佣金钱包单次划转上限，0 为不限制
    #[sqlx(default)]
    pub commission_transfer_max: f64,
    #[sqlx(default)]
    pub invite_reward_inviter: f64,
    #[sqlx(default)]
    pub invite_reward_invitee: f64,
    #[sqlx(default)]
    pub daily_invite_limit: i64,
    #[sqlx(default)]
    pub marketing_enabled: i64,
    #[sqlx(default)]
    pub is_default: i64,
    /// 1=会员卡券方案可以把用户升到这个等级
    #[sqlx(default)]
    pub purchasable: i64,
    #[sqlx(default)]
    pub max_token_count: i64,
    #[sqlx(default)]
    pub allow_view_log_details: Option<i64>,
    #[sqlx(default)]
    pub invoice_enabled: Option<i64>,
    #[sqlx(default)]
    pub invoice_mode: Option<String>,
    #[sqlx(default)]
    pub invoice_config: Option<String>,
    pub description: String,
    #[sqlx(default)]
    pub sort_order: i64,
    pub created_at: DbTs,
    pub updated_at: DbTs,
    #[sqlx(default)]
    pub user_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserLevelRequest {
    pub name: String,
    pub group_key: String,
    pub discount: f64,
    pub discount_type: Option<i64>,
    pub commission_ratio: Option<f64>,
    pub commission_basis: Option<String>,
    pub commission_transfer_min: Option<f64>,
    pub commission_transfer_max: Option<f64>,
    pub invite_reward_inviter: Option<f64>,
    pub invite_reward_invitee: Option<f64>,
    pub daily_invite_limit: Option<i64>,
    pub marketing_enabled: Option<i64>,
    pub is_default: Option<i64>,
    pub purchasable: Option<i64>,
    pub max_token_count: Option<i64>,
    pub allow_view_log_details: Option<i64>,
    pub invoice_enabled: Option<i64>,
    pub invoice_mode: Option<String>,
    pub invoice_config: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserLevelRequest {
    pub name: Option<String>,
    pub group_key: Option<String>,
    pub discount: Option<f64>,
    pub discount_type: Option<i64>,
    pub commission_ratio: Option<f64>,
    pub commission_basis: Option<String>,
    pub commission_transfer_min: Option<f64>,
    pub commission_transfer_max: Option<f64>,
    pub invite_reward_inviter: Option<f64>,
    pub invite_reward_invitee: Option<f64>,
    pub daily_invite_limit: Option<i64>,
    pub marketing_enabled: Option<i64>,
    pub is_default: Option<i64>,
    pub purchasable: Option<i64>,
    pub max_token_count: Option<i64>,
    pub allow_view_log_details: Option<i64>,
    pub invoice_enabled: Option<i64>,
    pub invoice_mode: Option<String>,
    pub invoice_config: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i64>,
}

fn default_commission_basis() -> String {
    "consumption".to_string()
}

#[derive(Debug, Serialize)]
pub struct UserLevelListResponse {
    pub data: Vec<UserLevel>,
    pub total: i64,
}
