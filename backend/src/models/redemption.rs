/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::time_system::DbTs;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_unlimited() -> i64 {
    -1
}

fn default_one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Redemption {
    pub id: i64,
    pub name: String,
    pub code: String,
    pub quota: f64,
    pub is_used: i64,
    pub used_at: Option<DbTs>,
    pub used_by: Option<String>,
    pub created_at: DbTs,
    pub updated_at: DbTs,
    /// 过期时间（NULL/空 = 长期有效）
    #[sqlx(default)]
    pub expires_at: Option<DbTs>,
    /// 开始生效时间（NULL = 生成后立即生效）
    #[sqlx(default)]
    pub starts_at: Option<DbTs>,
    /// 单兑换码可兑换次数，-1 = 不限（兼容历史 0 = 不限）
    #[sqlx(default)]
    pub max_uses: i64,
    /// 已兑换次数（按单个兑换码累计）
    #[sqlx(default)]
    pub used_count: i64,
    /// 单兑换码单用户可兑换次数，-1 = 不限（兼容历史 0 = 不限）
    #[sqlx(default)]
    pub per_user_limit: i64,
    /// 同一活动（同 name）下单用户可兑换次数，-1 = 不限（兼容历史缺省）
    #[sqlx(default)]
    pub per_user_activity_limit: i64,
    /// 状态: 1=正常, 0=禁用, -1=作废, -2=已退换兑换余额
    #[sqlx(default)]
    pub status: i64,
    /// balance=余额兑换码, model_voucher=模型卡券兑换码, playground_space=创作空间兑换码
    #[sqlx(default)]
    #[serde(default)]
    pub code_type: String,
    /// 模型卡券允许消耗的模型 ID，JSON 数组
    #[sqlx(default)]
    #[serde(default)]
    pub allowed_models: String,
    /// 兑换后可消耗天数，0=长期有效
    #[sqlx(default)]
    #[serde(default)]
    pub valid_days: i64,
    /// 活动 ID，4 位数字。同一活动的兑换码共用
    #[sqlx(default)]
    #[serde(default)]
    pub activity_id: String,
    /// 0=已启用的条件全部满足，1=满足其中一项
    #[sqlx(default)]
    #[serde(default)]
    pub condition_match: i64,
    /// 系统余额门槛。0=不要求
    #[sqlx(default)]
    #[serde(default)]
    pub cond_min_balance: f64,
    /// 当月已结算消费门槛。0=不要求
    #[sqlx(default)]
    #[serde(default)]
    pub cond_month_spend: f64,
    /// 累计已结算消费门槛。0=不要求
    #[sqlx(default)]
    #[serde(default)]
    pub cond_total_spend: f64,
    /// 当月充值门槛。0=不要求
    #[sqlx(default)]
    #[serde(default)]
    pub cond_month_recharge: f64,
    /// 累计充值门槛。0=不要求
    #[sqlx(default)]
    #[serde(default)]
    pub cond_total_recharge: f64,
    /// 活动简介/广告语（发放与用户兑换时展示说明）
    #[sqlx(default)]
    #[serde(default)]
    pub description: String,
    /// 管理员备注说明（仅后台管理者可见）
    #[sqlx(default)]
    #[serde(default)]
    pub remark: String,
    /// 赠送金余额面值（余额兑换码计入 users.gift_balance）
    #[sqlx(default)]
    #[serde(default)]
    pub gift_quota: f64,
    /// 模型卡券面额价值（展示价值，0=不单独展示，实际可消费金额仍为 quota）
    #[sqlx(default)]
    #[serde(default)]
    pub worth_quota: f64,
    /// 模型卡券默认模型折扣倍率（1.0=原价）
    #[sqlx(default)]
    #[serde(default = "default_one")]
    pub voucher_discount: f64,
    /// 模型卡券单模型专属折扣 JSON（{"mid": 0.7}）
    #[sqlx(default)]
    #[serde(default)]
    pub model_discounts: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRedemptionRequest {
    pub name: String,
    pub count: i64,
    #[serde(default)]
    pub quota: f64,
    /// 赠送金余额面值（余额兑换码计入 users.gift_balance）
    #[serde(default)]
    pub gift_quota: f64,
    /// 模型卡券面额价值（展示价值，选填）
    #[serde(default)]
    pub worth_quota: f64,
    /// 模型卡券默认模型折扣倍率（0.0001~1.0，缺省为 1.0）
    #[serde(default)]
    pub voucher_discount: Option<f64>,
    /// 模型卡券单模型专属折扣 JSON 对象（{"mid": 0.7}）
    #[serde(default)]
    pub model_discounts: Option<serde_json::Value>,
    /// 是否长期有效（true 时忽略 expires_at）
    #[serde(default = "default_true")]
    pub permanent: bool,
    /// 过期时间 ISO 字符串（permanent=false 时必填）
    #[serde(default)]
    pub expires_at: Option<String>,
    /// 是否生成后立即生效（true 时忽略 starts_at）
    #[serde(default = "default_true")]
    pub effective_immediately: bool,
    /// 开始生效时间（effective_immediately=false 时必填）
    #[serde(default)]
    pub starts_at: Option<String>,
    /// 是否允许多次兑换（false 时强制 max_uses=1；与活动参与次数限制相互独立）
    #[serde(default)]
    pub allow_multiple: bool,
    /// 单兑换码兑换次数上限，-1 = 不限（仅 allow_multiple=true 时生效；每个码独立）
    #[serde(default = "default_unlimited")]
    pub max_uses: i64,
    /// 单兑换码单用户兑换次数上限，-1 = 不限（仅 allow_multiple=true 时生效）
    #[serde(default = "default_unlimited")]
    pub per_user_limit: i64,
    /// 同一活动下单用户可兑换次数上限，-1 = 不限（与 allow_multiple 相互独立）
    #[serde(default = "default_unlimited")]
    pub per_user_activity_limit: i64,
    /// balance | model_voucher | playground_space，缺省为余额兑换码
    #[serde(default)]
    pub code_type: Option<String>,
    /// 模型卡券可消耗的模型 ID
    #[serde(default)]
    pub allowed_models: Option<Vec<String>>,
    /// 兑换后有效天数，0=长期有效
    #[serde(default)]
    pub valid_days: i64,
    /// 0=全部满足，1=满足其中一项
    #[serde(default)]
    pub condition_match: i64,
    #[serde(default)]
    pub cond_min_balance: f64,
    #[serde(default)]
    pub cond_month_spend: f64,
    #[serde(default)]
    pub cond_total_spend: f64,
    #[serde(default)]
    pub cond_month_recharge: f64,
    #[serde(default)]
    pub cond_total_recharge: f64,
    /// 活动简介/广告语（发放与用户兑换时展示说明）
    #[serde(default)]
    pub description: String,
    /// 管理员备注说明（仅后台管理者可见）
    #[serde(default)]
    pub remark: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRedemptionGroupRemarkRequest {
    pub name: String,
    #[serde(default)]
    pub remark: String,
}

#[derive(Debug, Deserialize)]
pub struct RedeemRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRedemptionStatusRequest {
    pub status: i64,
}

#[derive(Debug, Deserialize)]
pub struct RedemptionQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub name: Option<String>,
    pub code_type: Option<String>,
    /// 兑换码明细里按序列号筛选，大小写不敏感，支持片段。
    pub code: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RedemptionGroup {
    pub name: String,
    pub total_count: i64,
    pub total_quota: f64,
    #[sqlx(default)]
    #[serde(default)]
    pub total_gift_quota: f64,
    #[sqlx(default)]
    #[serde(default)]
    pub total_worth_quota: f64,
    pub created_at: DbTs,
    pub expires_at: Option<DbTs>,
    /// 开始生效时间（NULL = 生成后立即生效）
    #[sqlx(default)]
    pub starts_at: Option<DbTs>,
    pub total_used_count: i64,
    pub max_uses: i64,
    pub per_user_limit: i64,
    #[sqlx(default)]
    pub per_user_activity_limit: i64,
    #[sqlx(default)]
    pub code_type: String,
    #[sqlx(default)]
    pub allowed_models: String,
    #[sqlx(default)]
    #[serde(default = "default_one")]
    pub voucher_discount: f64,
    #[sqlx(default)]
    #[serde(default)]
    pub model_discounts: String,
    #[sqlx(default)]
    pub valid_days: i64,
    #[sqlx(default)]
    pub activity_id: String,
    #[sqlx(default)]
    pub condition_match: i64,
    #[sqlx(default)]
    pub cond_min_balance: f64,
    #[sqlx(default)]
    pub cond_month_spend: f64,
    #[sqlx(default)]
    pub cond_total_spend: f64,
    #[sqlx(default)]
    pub cond_month_recharge: f64,
    #[sqlx(default)]
    pub cond_total_recharge: f64,
    #[sqlx(default)]
    #[serde(default)]
    pub description: String,
    #[sqlx(default)]
    #[serde(default)]
    pub remark: String,
}

#[derive(Debug, Serialize)]
pub struct RedemptionGroupResponse {
    pub data: Vec<RedemptionGroup>,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct RedemptionUse {
    /// redemption_logs.id。0 表示没有逐次日志的历史单次兑换。
    pub id: i64,
    pub amount: f64,
    pub gift_amount: f64,
    pub created_at: DbTs,
    /// 0=未退换，1=已退换这一次。
    pub refunded: i64,
}

#[derive(Debug, Serialize)]
pub struct RedemptionRedeemer {
    /// users.id，管理端按这个人退换。
    pub user_id: String,
    pub username: String,
    pub uid: String,
    pub remark: String,
    pub referrer_username: String,
    pub referrer_uid: String,
    /// 该用户兑换此码的次数，含已退换。
    pub times: i64,
    /// 该用户尚未退换的系统余额入账合计。
    pub amount: f64,
    /// 该用户尚未退换的赠送金余额入账合计。
    pub gift_amount: f64,
    pub uses: Vec<RedemptionUse>,
}

#[derive(Debug, Serialize)]
pub struct RedemptionListItem {
    #[serde(flatten)]
    pub code: Redemption,
    pub redeemers: Vec<RedemptionRedeemer>,
}

#[derive(Debug, Serialize)]
pub struct RedemptionListResponse {
    pub data: Vec<RedemptionListItem>,
    pub total: i64,
}
