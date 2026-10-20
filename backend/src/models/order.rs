/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::time_system::DbTs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Order {
    pub id: i64,
    pub out_trade_no: String,
    pub user_id: String,
    pub payment_method: String,
    pub amount: f64,
    pub status: String, // pending, paid, closed
    pub trade_no: Option<String>,
    pub created_at: DbTs,
    pub paid_at: Option<DbTs>,
    /// 渠道收款币种；NULL 为旧订单（与站点默认货币同币种）
    pub pay_currency: Option<String>,
    /// 渠道应收金额（收款币种）；NULL 时等于 amount
    pub pay_amount: Option<f64>,
    /// 下单时 1 默认货币 = fx_rate 收款币种（含加价）；同币种为 NULL
    pub fx_rate: Option<f64>,
}

impl Order {
    /// 回调核验用的渠道应收金额（收款币种）
    pub fn expected_pay_amount(&self) -> f64 {
        self.pay_amount.unwrap_or(self.amount)
    }
}
