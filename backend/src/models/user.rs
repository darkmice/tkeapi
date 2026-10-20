/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#![allow(dead_code)]
use crate::time_system::DbTs;
use chrono::Datelike;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub uid: String,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub nickname: Option<String>,
    /// 名
    #[sqlx(default)]
    pub first_name: Option<String>,
    /// 姓氏
    #[sqlx(default)]
    pub last_name: Option<String>,
    /// 出生年月，格式 YYYY-MM
    #[sqlx(default)]
    pub birth_ym: Option<String>,
    /// 头像 URL
    #[sqlx(default)]
    pub avatar: Option<String>,
    pub mobile: Option<String>,
    pub wechat_id: Option<String>,
    pub wechat_name: Option<String>,
    /// 谷歌 OAuth 唯一标识
    pub google_id: Option<String>,
    pub google_name: Option<String>,
    pub role: String,
    pub balance: f64,
    #[sqlx(default)]
    pub used_quota: f64,
    pub user_group: String,
    #[sqlx(default)]
    pub level_name: Option<String>,
    #[sqlx(default)]
    pub level_id: Option<i64>,
    #[sqlx(default)]
    pub allow_view_log_details: Option<i64>,
    #[sqlx(default)]
    pub invoice_enabled: Option<i64>,
    #[sqlx(default)]
    pub invoice_mode: Option<String>,
    #[sqlx(default)]
    pub invoice_config: Option<String>,
    /// 等级是否开启专属推广（注册邀请奖励）
    #[sqlx(default)]
    pub marketing_enabled: Option<i64>,
    /// 等级返利比例，0 到 1
    #[sqlx(default)]
    pub commission_ratio: Option<f64>,
    #[sqlx(default)]
    pub admin_group_id: Option<i64>,
    #[serde(skip_deserializing)]
    #[sqlx(skip)]
    pub permissions: Option<Vec<String>>,
    #[serde(skip_deserializing)]
    #[sqlx(skip)]
    pub edit_permissions: Option<Vec<String>>,
    pub is_active: i64,
    pub referred_by: Option<String>,
    #[sqlx(default)]
    pub commission_balance: f64,
    pub created_at: DbTs,
    pub updated_at: DbTs,
    #[sqlx(default)]
    pub register_ip: Option<String>,
    /// 最近一次登录或接口请求看到的客户端 IP
    #[sqlx(default)]
    pub last_active_ip: Option<String>,
    #[sqlx(default)]
    pub admin_remark: Option<String>,
    #[sqlx(default)]
    pub remark: Option<String>,
    #[sqlx(default)]
    pub referral_history: Option<String>,
    #[sqlx(default)]
    pub gift_balance: f64,
    #[sqlx(default)]
    pub gift_used_quota: f64,
    /// 用户模型单独折扣(JSON: {"mid": discount})，优先于等级折扣
    #[sqlx(default)]
    pub model_discounts: Option<String>,
    #[sqlx(default)]
    pub timezone: Option<String>,
    /// 信控额度：管理员设置的信用额度
    #[sqlx(default)]
    pub credit_limit: f64,
    /// 是否允许在线支付：1-允许，0-禁止
    #[sqlx(default)]
    pub pay_enabled: i64,
    #[sqlx(default)]
    pub notification_preferences: Option<String>,
    /// 密码最后修改时间，早于此时间的 JWT 凭据判定为失效
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_changed_at: Option<DbTs>,
    /// 列表接口填充：是否已通过个人实名
    #[sqlx(skip)]
    #[serde(default)]
    pub kyc_personal: bool,
    /// 列表接口填充：是否已通过企业实名
    #[sqlx(skip)]
    #[serde(default)]
    pub kyc_enterprise: bool,
    /// 列表接口填充：个人实名最新审核状态 none|pending|approved|rejected|expired
    #[sqlx(skip)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kyc_personal_status: Option<String>,
    /// 列表接口填充：企业实名最新审核状态 none|pending|approved|rejected|expired
    #[sqlx(skip)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kyc_enterprise_status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    #[serde(default)]
    pub email: String,
    pub password: String,
    pub group: Option<String>, // Keep for compatibility
    pub user_group: Option<String>,
    pub admin_group_id: Option<i64>,
    pub aff: Option<String>,
    pub referred_by: Option<String>,
    pub role: Option<String>,
    /// 团队邀请码，注册后自动加入对应团队
    pub team: Option<String>,
    /// 用户端注册时的浏览器 IANA 时区
    #[serde(default)]
    pub timezone: Option<String>,
    pub mobile: Option<String>,
    pub wechat_id: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    /// 出生年月，格式 YYYY-MM；空字符串表示清空
    pub birth_ym: Option<String>,
    pub balance: Option<f64>,
    pub gift_balance: Option<f64>,
    /// 是否允许在线支付
    pub pay_enabled: Option<i64>,
}

pub fn deserialize_some_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub nickname: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    /// 出生年月，格式 YYYY-MM；空字符串表示清空
    pub birth_ym: Option<String>,
    pub mobile: Option<String>,
    pub wechat_id: Option<String>,
    pub google_id: Option<String>,
    pub role: Option<String>,
    pub balance: Option<f64>,
    pub user_group: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some_option")]
    pub admin_group_id: Option<Option<i64>>,
    pub is_active: Option<i64>,
    pub commission_balance: Option<f64>,
    pub admin_remark: Option<String>,
    pub referral_history: Option<String>,
    pub referred_by: Option<String>,
    pub gift_balance: Option<f64>,
    pub gift_used_quota: Option<f64>,
    /// 用户模型单独折扣(JSON: {"mid": discount})
    pub model_discounts: Option<String>,
    pub timezone: Option<String>,
    /// 信控额度
    pub credit_limit: Option<f64>,
    /// 是否允许在线支付
    pub pay_enabled: Option<i64>,
    pub notification_preferences: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProfileUpdateRequest {
    pub nickname: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    /// 出生年月，格式 YYYY-MM；空字符串表示清空
    pub birth_ym: Option<String>,
    pub old_password: Option<String>,
    pub password: Option<String>,
    pub email: Option<String>,
    pub mobile: Option<String>,
    pub wechat_id: Option<String>,
    pub timezone: Option<String>,
    pub notification_preferences: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RechargeRecord {
    pub id: i64,
    pub user_id: String,
    pub amount: f64,
    pub recharge_type: String,
    pub remark: Option<String>,
    #[sqlx(default)]
    pub operator: Option<String>,
    #[sqlx(default)]
    pub wallet_type: Option<String>,
    #[sqlx(default)]
    pub order_no: Option<String>,
    pub created_at: DbTs,
}

#[derive(Debug, Deserialize)]
pub struct RechargeRequest {
    pub amount: f64,
    pub remark: Option<String>,
    /// wallet_type: 'system'=系统钱包, 'gift'=赠送钱包
    #[serde(default)]
    pub wallet_type: String,
    /// 系统钱包调整时是否联动上级返佣。加款缺省为是，扣款缺省为否。扣回先扣佣金钱包，差额再扣系统钱包。
    #[serde(default)]
    pub apply_commission: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct WalletStats {
    pub balance: f64,
    pub gift_balance: f64,
    pub credit_limit: f64,
    pub total_consumption: f64,
    pub total_calls: i64,
    pub success_calls: i64,
    pub commission_balance: f64,
    pub total_referred: i64,
    pub marketing_enabled: bool,
    pub commission_ratio: f64,
    pub invite_reward_inviter: f64,
    pub invite_reward_invitee: f64,
    /// consumption=按系统钱包实际消费，recharge=按充值入账
    #[serde(default = "default_wallet_commission_basis")]
    pub commission_basis: String,
    /// 单次划转下限，0 为不限制
    #[serde(default)]
    pub commission_transfer_min: f64,
    /// 单次划转上限，0 为不限制
    #[serde(default)]
    pub commission_transfer_max: f64,
    /// 是否允许在线支付
    pub pay_enabled: bool,
}

fn default_wallet_commission_basis() -> String {
    "consumption".to_string()
}

/// 用户名+密码登录（保持原有接口兼容）
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub login_type: Option<String>,
    pub username: String,
    pub password: String,
    /// 邮箱/手机命中多个账号时，指定要登录的 UID
    #[serde(default)]
    pub uid: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: User,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginCandidate {
    pub uid: String,
    pub username: String,
    pub nickname: Option<String>,
}

impl From<&User> for LoginCandidate {
    fn from(user: &User) -> Self {
        Self {
            uid: user.uid.clone(),
            username: user.username.clone(),
            nickname: user.nickname.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LoginSelectResponse {
    pub need_select: bool,
    pub candidates: Vec<LoginCandidate>,
}

#[derive(Debug, Serialize)]
pub struct UserListResponse {
    pub data: Vec<User>,
    pub total: i64,
}

/// 发送邮箱验证码请求
#[derive(Debug, Deserialize)]
pub struct SendCodeRequest {
    pub email: String,
    pub purpose: String, // register, reset_password, bind_email
}

/// 发送短信验证码请求
#[derive(Debug, Deserialize)]
pub struct SendSmsCodeRequest {
    pub mobile: String,
    pub purpose: String, // register, bind_mobile
}

/// 邮箱注册请求
#[derive(Debug, Deserialize)]
pub struct EmailRegisterRequest {
    pub email: String,
    pub code: String,
    pub password: String,
    pub aff: Option<String>,
    /// 团队邀请码，注册后自动加入对应团队
    pub team: Option<String>,
    /// 用户端注册时的浏览器 IANA 时区
    #[serde(default)]
    pub timezone: Option<String>,
    /// 该邮箱/手机已有账号时，确认继续注册新 UID
    #[serde(default)]
    pub confirm_new_account: bool,
}

/// 手机号注册请求
#[derive(Debug, Deserialize)]
pub struct MobileRegisterRequest {
    pub mobile: String,
    pub code: String,
    pub password: String,
    pub aff: Option<String>,
    /// 团队邀请码，注册后自动加入对应团队
    pub team: Option<String>,
    /// 用户端注册时的浏览器 IANA 时区
    #[serde(default)]
    pub timezone: Option<String>,
    /// 该邮箱/手机已有账号时，确认继续注册新 UID
    #[serde(default)]
    pub confirm_new_account: bool,
}

/// 重置密码请求
#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub email: Option<String>,
    pub mobile: Option<String>,
    pub code: String,
    pub new_password: String,
    /// 同一邮箱/手机绑定多个账号时指定 UID
    #[serde(default)]
    pub uid: Option<String>,
}

/// 绑定/换绑手机请求
#[derive(Debug, Deserialize)]
pub struct BindMobileRequest {
    /// 原手机验证码（换绑时必填）
    pub old_code: Option<String>,
    /// 新手机号
    pub mobile: String,
    /// 新手机验证码
    pub code: String,
}

/// 绑定/换绑邮箱请求
#[derive(Debug, Deserialize)]
pub struct BindEmailRequest {
    /// 原邮箱验证码（换绑时必填）
    pub old_code: Option<String>,
    /// 新邮箱
    pub email: String,
    /// 新邮箱验证码
    pub code: String,
}

/// 解绑第三方请求
#[derive(Debug, Deserialize)]
pub struct UnbindRequest {
    /// 当前登录密码（安全校验）
    pub password: String,
}

const PERSON_NAME_MAX: usize = 32;

/// 名 / 姓氏：空白视为清空，超长拒绝。
pub fn normalize_person_name(value: &str, label: &str) -> Result<Option<String>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > PERSON_NAME_MAX {
        return Err(format!(
            "{label}长度最多不能超过 {PERSON_NAME_MAX} 个字符"
        ));
    }
    Ok(Some(trimmed.to_string()))
}

/// 出生年月：空白视为清空，只接受 YYYY-MM，且不能晚于当前月。
pub fn normalize_birth_ym(value: &str) -> Result<Option<String>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut parts = trimmed.split('-');
    let year_s = parts.next().unwrap_or("");
    let month_s = parts.next().unwrap_or("");
    if parts.next().is_some() || year_s.len() != 4 || month_s.len() != 2 {
        return Err("出生年月格式应为 YYYY-MM".to_string());
    }
    let year: i32 = year_s
        .parse()
        .map_err(|_| "出生年月格式应为 YYYY-MM".to_string())?;
    let month: u32 = month_s
        .parse()
        .map_err(|_| "出生年月格式应为 YYYY-MM".to_string())?;
    if !(1900..=2100).contains(&year) || !(1..=12).contains(&month) {
        return Err("出生年月不合法".to_string());
    }
    let now = chrono::Utc::now();
    if year > now.year() || (year == now.year() && month > now.month()) {
        return Err("出生年月不能晚于当前月份".to_string());
    }
    Ok(Some(format!("{year:04}-{month:02}")))
}

/// 请求里出现的字段才覆盖；缺省字段保持原值。空字符串清空。
pub fn assign_profile_identity(
    first_name: &mut Option<String>,
    last_name: &mut Option<String>,
    birth_ym: &mut Option<String>,
    req_first: Option<String>,
    req_last: Option<String>,
    req_birth: Option<String>,
) -> Result<(), String> {
    if let Some(value) = req_first {
        *first_name = normalize_person_name(&value, "名")?;
    }
    if let Some(value) = req_last {
        *last_name = normalize_person_name(&value, "姓氏")?;
    }
    if let Some(value) = req_birth {
        *birth_ym = normalize_birth_ym(&value)?;
    }
    Ok(())
}

#[cfg(test)]
mod profile_identity_tests {
    use super::*;

    #[test]
    fn person_name_trims_and_rejects_overlong() {
        assert_eq!(normalize_person_name("  明  ", "名").unwrap().as_deref(), Some("明"));
        assert_eq!(normalize_person_name("   ", "名").unwrap(), None);
        let long = "名".repeat(33);
        assert!(normalize_person_name(&long, "名").is_err());
    }

    #[test]
    fn birth_ym_normalizes_month() {
        assert_eq!(normalize_birth_ym("1990-01").unwrap().as_deref(), Some("1990-01"));
        assert_eq!(normalize_birth_ym("  ").unwrap(), None);
        assert!(normalize_birth_ym("1990-13").is_err());
        assert!(normalize_birth_ym("1990-1").is_err());
        assert!(normalize_birth_ym("2999-01").is_err());
    }
}
