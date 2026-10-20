/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 站点内部账本金额精度约定：一律保留小数点后 6 位（四舍五入）。
//!
//! 适用范围：余额、赠送金、日志 cost、预扣/结算扣费、充值调账、额度用量等。
//! 不适用：支付通道对外金额（微信/支付宝等仍按对方要求保留分，即 2 位）。

/// 金额小数位数
pub const MONEY_DECIMAL_PLACES: u32 = 6;

/// 缩放因子：10^6（与 MONEY_DECIMAL_PLACES 保持一致）
pub const MONEY_SCALE: f64 = 1_000_000.0;

/// 四舍五入到约定小数位
#[inline]
pub fn round_money(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    (v * MONEY_SCALE).round() / MONEY_SCALE
}

/// 格式化为固定小数位字符串（日志/通知用）
#[inline]
pub fn format_money(v: f64) -> String {
    format!("{:.*}", MONEY_DECIMAL_PLACES as usize, round_money(v))
}

/// 精简金额格式化（最多 6 位小数，去除末尾无效的 0 与小数点）
#[inline]
pub fn format_clean_money(v: f64) -> String {
    let s = format!("{:.*}", MONEY_DECIMAL_PLACES as usize, round_money(v));
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 预扣/扣费：赠送余额优先，返回 `(gift_deducted, balance_deducted)`（均已 round）。
#[inline]
pub fn split_gift_first(amount: f64, gift_balance: f64) -> (f64, f64) {
    let amount = round_money(amount);
    if amount <= 0.0 {
        return (0.0, 0.0);
    }
    let gift = round_money(amount.min(gift_balance.max(0.0)));
    (gift, round_money(amount - gift))
}

/// 结算差额 `(settled_cost, apply_balance)`：应付原样；apply 正补扣、负退多扣。
#[inline]
pub fn settlement_delta(cost: f64, pre_deducted: f64) -> (f64, f64) {
    let settled = round_money(cost);
    let apply = round_money(settled - round_money(pre_deducted));
    (settled, apply)
}

/// 卡券优先承担后，钱包只结算剩余部分。
/// 返回 `(wallet_settled, wallet_pre, wallet_apply)`。
#[inline]
pub fn wallet_leg(
    settled: f64,
    pre_deducted: f64,
    voucher_final: f64,
    voucher_held: f64,
) -> (f64, f64, f64) {
    let wallet_settled = round_money((round_money(settled) - round_money(voucher_final)).max(0.0));
    let wallet_pre = round_money((round_money(pre_deducted) - round_money(voucher_held)).max(0.0));
    let (_, apply) = settlement_delta(wallet_settled, wallet_pre);
    (wallet_settled, wallet_pre, apply)
}

/// 整笔预扣退回时，钱包应退的 `(balance, gift)`。卡券部分由持仓表归还，不能退进余额。
#[inline]
pub fn wallet_refund_parts(pre_deducted: f64, pre_deduct_gift: f64, voucher: f64) -> (f64, f64) {
    let wallet = round_money((round_money(pre_deducted) - round_money(voucher)).max(0.0));
    let gift_refund = round_money(round_money(pre_deduct_gift).min(wallet).max(0.0));
    let balance_refund = round_money(wallet - gift_refund);
    (balance_refund, gift_refund)
}

/// 一笔已完成结算里，邀请返佣可计入的系统钱包消费。
/// 赠送钱包消费返回 0 侧差额，冻结阶段返回 0。`gift_balance_before_extra` 只在补扣（apply > 0）时使用。
#[inline]
pub fn paid_consumption_for_commission(
    settled_cost: f64,
    pre_deducted: f64,
    pre_deduct_gift: f64,
    gift_balance_before_extra: f64,
    is_freeze: bool,
) -> f64 {
    if is_freeze {
        return 0.0;
    }
    let settled = round_money(settled_cost);
    if settled <= 0.0 {
        return 0.0;
    }
    let pre = round_money(pre_deducted).max(0.0);
    let pre_gift = round_money(pre_deduct_gift).clamp(0.0, pre);
    let apply = round_money(settled - pre);
    let gift_total = if apply > 0.0 {
        let extra_gift = round_money(apply.min(gift_balance_before_extra.max(0.0)));
        round_money(pre_gift + extra_gift)
    } else {
        round_money(settled.min(pre_gift))
    };
    round_money((settled - gift_total).max(0.0))
}

/// 格式化预扣结算文案留痕（统一收口异步/同步任务的预扣结算说明）
/// 支持传入已有货币单位（Some）；未传入（None）且 pre_deducted > 0 时内部自动从 state 查询，避免外部到处重复查库
pub async fn format_settlement_note(
    state: &crate::AppState,
    settled_cost: f64,
    pre_deducted: f64,
    unit_opt: Option<&str>,
    is_by_second: bool,
) -> String {
    if pre_deducted <= 0.0 {
        return String::new();
    }
    let fallback_unit;
    let unit = match unit_opt {
        Some(u) => u,
        None => {
            fallback_unit = crate::api::settings::get_currency_settings(state)
                .await
                .currency_unit;
            &fallback_unit
        }
    };
    let (settled, delta) = settlement_delta(settled_cost, pre_deducted);
    let tag = if is_by_second {
        "[按秒预扣结算]"
    } else {
        "[预扣结算]"
    };
    let action = if delta < 0.0 {
        format!("退还差额 {}{}", format_clean_money(-delta), unit)
    } else if delta > 0.0 {
        format!("补扣差额 {}{}", format_clean_money(delta), unit)
    } else {
        "无需退补".to_string()
    };
    format!(
        " | {} 已预扣 {}{}，实际消费 {}{}，{}",
        tag,
        format_clean_money(pre_deducted),
        unit,
        format_clean_money(settled),
        unit,
        action
    )
}

#[cfg(test)]
mod tests {
    use super::paid_consumption_for_commission;

    #[test]
    fn commission_skips_freeze_and_gift_only() {
        assert_eq!(
            paid_consumption_for_commission(8.0, 8.0, 0.0, 0.0, true),
            0.0
        );
        assert_eq!(
            paid_consumption_for_commission(5.0, 0.0, 0.0, 9.0, false),
            0.0
        );
    }

    #[test]
    fn commission_counts_only_system_wallet() {
        assert_eq!(
            paid_consumption_for_commission(10.0, 0.0, 0.0, 3.0, false),
            7.0
        );
        assert_eq!(
            paid_consumption_for_commission(8.0, 8.0, 2.0, 0.0, false),
            6.0
        );
        assert_eq!(
            paid_consumption_for_commission(4.0, 10.0, 6.0, 0.0, false),
            0.0
        );
        assert_eq!(
            paid_consumption_for_commission(12.0, 10.0, 4.0, 1.0, false),
            7.0
        );
    }

    #[test]
    fn wallet_leg_leaves_voucher_off_the_balance() {
        use super::{wallet_leg, wallet_refund_parts};
        assert_eq!(wallet_leg(0.5, 1.0, 0.4, 0.4), (0.1, 0.6, -0.5));
        assert_eq!(wallet_leg(0.1, 1.0, 0.1, 0.4), (0.0, 0.6, -0.6));
        assert_eq!(wallet_leg(1.2, 1.0, 0.4, 0.4), (0.8, 0.6, 0.2));
        assert_eq!(wallet_leg(1.2, 1.0, 0.0, 0.0), (1.2, 1.0, 0.2));
        assert_eq!(wallet_refund_parts(1.0, 0.2, 0.4), (0.4, 0.2));
        assert_eq!(wallet_refund_parts(1.0, 0.2, 0.0), (0.8, 0.2));
    }
}
