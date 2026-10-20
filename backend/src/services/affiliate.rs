/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use anyhow::Result;
use sqlx::{Postgres, Transaction};

pub const BASIS_CONSUMPTION: &str = "consumption";
pub const BASIS_RECHARGE: &str = "recharge";

pub fn normalize_commission_basis(raw: &str) -> &'static str {
    if raw == BASIS_RECHARGE {
        BASIS_RECHARGE
    } else {
        BASIS_CONSUMPTION
    }
}

#[derive(sqlx::FromRow)]
struct InviterTerms {
    id: String,
    commission_ratio: f64,
    commission_basis: String,
    marketing_enabled: i64,
}

/// 新返佣只在专属推广开启且比例大于 0 时发放。回冲不走这里。
fn new_awards_open(marketing_enabled: i64, ratio: f64) -> bool {
    marketing_enabled == 1 && ratio > 0.0
}

/// 同一邀请人、同一被邀请人、站点日一条。换上级会另起一条，避免记到原上级流水上。
fn daily_consumption_key(inviter_id: &str, from_user_id: &str, day: &str) -> String {
    format!("daycons:{inviter_id}:{from_user_id}:{day}")
}

/// 终态结算里应计佣的系统钱包消费。赠送钱包不计入。冻结未完成返回 0。
pub struct ConsumptionAward {
    pub inviter_id: String,
    pub ratio: f64,
    pub base_amount: f64,
}

async fn load_inviter_terms(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<Option<InviterTerms>> {
    let row: Option<InviterTerms> = sqlx::query_as(&db.format_query(
        "SELECT inv.id, ul.commission_ratio, COALESCE(NULLIF(ul.commission_basis, ''), 'consumption') AS commission_basis, \
         COALESCE(ul.marketing_enabled, 0)::bigint AS marketing_enabled \
         FROM users u \
         JOIN users inv ON (inv.id = u.referred_by OR inv.uid = u.referred_by) \
         JOIN user_levels ul ON ul.group_key = inv.user_group \
         WHERE u.id = ? AND u.referred_by IS NOT NULL AND u.referred_by <> '' \
         ORDER BY CASE WHEN inv.id = u.referred_by THEN 0 ELSE 1 END \
         LIMIT 1",
    ))
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row)
}

fn commission_amount(base: f64, ratio: f64) -> f64 {
    if base <= 0.0 || ratio <= 0.0 {
        return 0.0;
    }
    crate::money::round_money(base * ratio)
}

async fn insert_commission(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    inviter_id: &str,
    from_user_id: &str,
    recharge_id: Option<i64>,
    amount: f64,
    ratio: f64,
    source_key: &str,
) -> Result<bool> {
    let inserted = sqlx::query(&db.format_query(
        "INSERT INTO commissions (user_id, from_user_id, recharge_id, amount, ratio, source_key) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT (source_key) WHERE source_key IS NOT NULL DO NOTHING",
    ))
    .bind(inviter_id)
    .bind(from_user_id)
    .bind(recharge_id)
    .bind(amount)
    .bind(ratio)
    .bind(source_key)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(inserted > 0)
}

/// 回冲不超过佣金钱包余额。余额不足时只扣剩余部分。
pub fn capped_clawback(requested: f64, balance: f64) -> f64 {
    let requested = crate::money::round_money(requested);
    let balance = crate::money::round_money(balance);
    if requested <= 0.0 || balance <= 0.0 {
        return 0.0;
    }
    crate::money::round_money(requested.min(balance))
}

/// 管理端系统钱包加减时，是否提示、默认勾不勾、实际金额。
pub struct AdjustQuote {
    pub eligible: bool,
    pub direction: &'static str,
    pub suggested: f64,
    /// 实际联动金额。扣回时等于佣金钱包与系统钱包两笔之和。
    pub actual: f64,
    /// 扣回时先从佣金钱包扣的部分。发放时等于入账佣金。
    pub from_commission: f64,
    /// 扣回时佣金钱包不够、改从上级系统钱包扣的部分。
    pub from_system: f64,
    pub default_apply: bool,
}

/// 管理端扣回拆分：先吃满佣金钱包，差额记到系统钱包，系统钱包侧不封顶。
pub fn split_admin_clawback(suggested: f64, commission_balance: f64) -> (f64, f64) {
    let suggested = crate::money::round_money(suggested);
    let from_commission = capped_clawback(suggested, commission_balance);
    let from_system = crate::money::round_money((suggested - from_commission).max(0.0));
    (from_commission, from_system)
}

/// 系统钱包消费里，扣之前余额真正盖住的现金。穿过 0 的信控垫付不发佣。
pub fn cash_commission_base(system_spend: f64, balance_before: f64) -> f64 {
    let spend = crate::money::round_money(system_spend).max(0.0);
    let cash = crate::money::round_money(balance_before).max(0.0);
    crate::money::round_money(spend.min(cash))
}

/// 已发过充值佣的本金先被消费吃掉。返回 `(应计佣基数, 吃掉的本金)`。
pub fn split_principal(spend: f64, principal_left: f64) -> (f64, f64) {
    let spend = crate::money::round_money(spend).max(0.0);
    let left = crate::money::round_money(principal_left).max(0.0);
    let exempt = crate::money::round_money(spend.min(left));
    (crate::money::round_money(spend - exempt), exempt)
}

/// 方舟退款：先退还还挂在系统钱包上的现金，其余退回赠送钱包。
/// `claw_base` 只含当初从系统钱包扣掉的现金，赠送和多退出来的尾差不回冲佣金。
pub struct ArkRefundSplit {
    pub to_system: f64,
    pub to_gift: f64,
    pub claw_base: f64,
}

pub fn split_ark_refund(refund: f64, net_cash: f64, net_gift: f64) -> ArkRefundSplit {
    let refund = crate::money::round_money(refund).max(0.0);
    let net_cash = crate::money::round_money(net_cash).max(0.0);
    let net_gift = crate::money::round_money(net_gift).max(0.0);
    let cash = crate::money::round_money(refund.min(net_cash));
    let gift = crate::money::round_money((refund - cash).min(net_gift));
    let rest = crate::money::round_money((refund - cash - gift).max(0.0));
    ArkRefundSplit {
        to_system: crate::money::round_money(cash + rest),
        to_gift: gift,
        claw_base: cash,
    }
}

/// 一笔已经入账、还没退完的佣金。`lots` 按 id 从新到旧。
#[derive(Clone, Debug)]
pub struct CommissionLotView {
    pub id: i64,
    pub inviter_id: String,
    pub base_amount: f64,
    pub awarded: f64,
    pub clawed: f64,
}

#[derive(Clone, Debug)]
pub struct LotClawSlice {
    pub lot_id: i64,
    pub inviter_id: String,
    #[allow(dead_code)]
    pub base_used: f64,
    pub award_cut: f64,
}

/// 一笔方舟系统流水里，这次退款先退出信控垫付，再退还有佣金的现金，最后退已返本金。
pub struct ArkRowRefund {
    pub consumed: f64,
    pub take_taxable: f64,
    /// 这次退回的、已经发过充值佣的本金。
    pub take_exempt: f64,
    /// 这次退回的余额兑换码本金。
    pub take_redemption: f64,
}

pub fn plan_ark_row_refund(
    left: f64,
    cash: f64,
    real_cash: f64,
    system_refunded: f64,
    principal_exempt: f64,
    lot_remaining_base: f64,
    redemption_exempt: f64,
) -> ArkRowRefund {
    let left = crate::money::round_money(left).max(0.0);
    let cash = crate::money::round_money(cash).max(0.0);
    let real_cash = crate::money::round_money(real_cash).clamp(0.0, cash);
    let refunded = crate::money::round_money(system_refunded).clamp(0.0, cash);
    let open = crate::money::round_money((cash - refunded).max(0.0));
    let none = ArkRowRefund {
        consumed: 0.0,
        take_taxable: 0.0,
        take_exempt: 0.0,
        take_redemption: 0.0,
    };
    if left <= 0.0 || open <= 0.0 {
        return none;
    }
    let credit = crate::money::round_money((cash - real_cash).max(0.0));
    let credit_open = if refunded < credit {
        crate::money::round_money(credit - refunded)
    } else {
        0.0
    };
    let take = crate::money::round_money(left.min(open));
    let take_credit = crate::money::round_money(take.min(credit_open));
    let take_real = crate::money::round_money(take - take_credit);
    let take_taxable = crate::money::round_money(take_real.min(lot_remaining_base.max(0.0)));
    let after_taxable = crate::money::round_money((take_real - take_taxable).max(0.0));
    let take_exempt = crate::money::round_money(after_taxable.min(principal_exempt.max(0.0)));
    let take_redemption = crate::money::round_money(
        (after_taxable - take_exempt)
            .min(redemption_exempt.max(0.0))
            .max(0.0),
    );
    ArkRowRefund {
        consumed: take,
        take_taxable,
        take_exempt,
        take_redemption,
    }
}

/// 按退款占原计佣基数的比例回冲，封顶为该笔还没退的佣金。
pub fn allocate_lot_clawback(lots_newest_first: &[CommissionLotView], refund_base: f64) -> Vec<LotClawSlice> {
    let mut left = crate::money::round_money(refund_base);
    if left <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for lot in lots_newest_first {
        if left <= 0.0 {
            break;
        }
        let awarded = crate::money::round_money(lot.awarded);
        let remaining_award = crate::money::round_money(awarded - crate::money::round_money(lot.clawed));
        let base = crate::money::round_money(lot.base_amount).max(0.0);
        if awarded <= 0.0 || remaining_award <= 0.0 || base <= 0.0 {
            continue;
        }
        let remaining_base = crate::money::round_money(base * remaining_award / awarded);
        if remaining_base <= 0.0 {
            continue;
        }
        let take_base = crate::money::round_money(left.min(remaining_base));
        if take_base <= 0.0 {
            continue;
        }
        let award_cut = if take_base + 1e-9 >= remaining_base {
            remaining_award
        } else {
            crate::money::round_money(remaining_award * take_base / remaining_base).min(remaining_award)
        };
        if award_cut <= 0.0 {
            continue;
        }
        out.push(LotClawSlice {
            lot_id: lot.id,
            inviter_id: lot.inviter_id.clone(),
            base_used: take_base,
            award_cut,
        });
        left = crate::money::round_money(left - take_base);
    }
    out
}

pub fn quote_adjust(amount: f64, ratio: f64, basis: &str, commission_balance: f64) -> AdjustQuote {
    let amount = crate::money::round_money(amount);
    let basis = normalize_commission_basis(basis);
    let none = AdjustQuote {
        eligible: false,
        direction: "none",
        suggested: 0.0,
        actual: 0.0,
        from_commission: 0.0,
        from_system: 0.0,
        default_apply: false,
    };
    if ratio <= 0.0 || amount == 0.0 {
        return none;
    }
    if amount > 0.0 {
        if basis != BASIS_RECHARGE {
            return none;
        }
        let suggested = commission_amount(amount, ratio);
        if suggested <= 0.0 {
            return none;
        }
        return AdjustQuote {
            eligible: true,
            direction: "award",
            suggested,
            actual: suggested,
            from_commission: suggested,
            from_system: 0.0,
            default_apply: true,
        };
    }
    let suggested = commission_amount(-amount, ratio);
    if suggested <= 0.0 {
        return none;
    }
    let (from_commission, from_system) = split_admin_clawback(suggested, commission_balance);
    AdjustQuote {
        eligible: true,
        direction: "clawback",
        suggested,
        actual: crate::money::round_money(from_commission + from_system),
        from_commission,
        from_system,
        default_apply: basis == BASIS_RECHARGE,
    }
}

async fn lock_commission_balance(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    inviter_id: &str,
) -> Result<f64> {
    let balance: f64 = sqlx::query_scalar(
        &db.format_query("SELECT commission_balance FROM users WHERE id = ? FOR UPDATE"),
    )
    .bind(inviter_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(balance)
}

/// 佣金只进出 `commission_balance`。扣回不超过当前余额；流水金额与实际变动一致。
async fn settle_commission(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    inviter_id: &str,
    from_user_id: &str,
    recharge_id: Option<i64>,
    signed: f64,
    ratio: f64,
    source_key: &str,
) -> Result<f64> {
    let signed = crate::money::round_money(signed);
    if signed == 0.0 {
        return Ok(0.0);
    }
    let actual = if signed > 0.0 {
        signed
    } else {
        let balance = lock_commission_balance(db, tx, inviter_id).await?;
        -capped_clawback(-signed, balance)
    };
    if actual == 0.0 {
        return Ok(0.0);
    }
    if !insert_commission(
        db,
        tx,
        inviter_id,
        from_user_id,
        recharge_id,
        actual,
        ratio,
        source_key,
    )
    .await?
    {
        return Ok(0.0);
    }
    sqlx::query(&db.format_query(
        "UPDATE users SET commission_balance = GREATEST(commission_balance + ?, 0), updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    ))
    .bind(actual)
    .bind(inviter_id)
    .execute(&mut **tx)
    .await?;
    Ok(actual)
}

async fn lock_recharge_principal(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<f64> {
    let left: f64 = sqlx::query_scalar(&db.format_query(
        "SELECT COALESCE(commissioned_recharge_left, 0) FROM users WHERE id = ? FOR UPDATE",
    ))
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(left)
}

async fn set_recharge_principal(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    left: f64,
) -> Result<()> {
    sqlx::query(&db.format_query(
        "UPDATE users SET commissioned_recharge_left = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    ))
    .bind(crate::money::round_money(left).max(0.0))
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 系统钱包支出里不参与消费返佣的部分。先吃余额兑换码，再吃已发过充值佣的本金。
struct ExemptTake {
    taxable: f64,
    recharge: f64,
    redemption: f64,
}

async fn consume_recharge_principal(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    spend: f64,
) -> Result<ExemptTake> {
    let spend = crate::money::round_money(spend);
    if spend <= 0.0 {
        return Ok(ExemptTake {
            taxable: 0.0,
            recharge: 0.0,
            redemption: 0.0,
        });
    }
    let (recharge_left, redemption_left): (f64, f64) = sqlx::query_as(&db.format_query(
        "SELECT COALESCE(commissioned_recharge_left, 0), COALESCE(redemption_commission_exempt, 0) \
         FROM users WHERE id = ? FOR UPDATE",
    ))
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    let (rest, redemption) = split_principal(spend, redemption_left);
    let (taxable, recharge) = split_principal(rest, recharge_left);
    if redemption > 0.0 || recharge > 0.0 {
        sqlx::query(&db.format_query(
            "UPDATE users SET commissioned_recharge_left = ?, redemption_commission_exempt = ?, \
             updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        ))
        .bind(crate::money::round_money(recharge_left - recharge).max(0.0))
        .bind(crate::money::round_money(redemption_left - redemption).max(0.0))
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ExemptTake {
        taxable,
        recharge,
        redemption,
    })
}

async fn add_recharge_principal(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    base: f64,
) -> Result<()> {
    let base = crate::money::round_money(base);
    if base <= 0.0 {
        return Ok(());
    }
    let left = lock_recharge_principal(db, tx, user_id).await?;
    set_recharge_principal(db, tx, user_id, left + base).await
}

/// 一次系统钱包支出吃掉的、以后退回时要原路加回的两部分本金。
pub struct SpentExempt {
    pub recharge: f64,
    pub redemption: f64,
}

/// 系统钱包支出吃掉不参与消费返佣的本金。退回时只能加回这里返回的数。
pub async fn note_system_spend(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    amount: f64,
) -> Result<SpentExempt> {
    let taken = consume_recharge_principal(db, tx, user_id, amount).await?;
    Ok(SpentExempt {
        recharge: taken.recharge,
        redemption: taken.redemption,
    })
}

/// 余额兑换码入账。这笔系统余额之后的支出不参与消费返佣，也不发充值返佣。
pub async fn note_redemption_credit(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    amount: f64,
) -> Result<()> {
    let amount = crate::money::round_money(amount);
    if amount <= 0.0 {
        return Ok(());
    }
    let left: f64 = sqlx::query_scalar(&db.format_query(
        "SELECT COALESCE(redemption_commission_exempt, 0) FROM users WHERE id = ? FOR UPDATE",
    ))
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query(&db.format_query(
        "UPDATE users SET redemption_commission_exempt = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    ))
    .bind(crate::money::round_money(left + amount))
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 退换兑换余额时，只减还没花掉的兑换码本金，不动已经发过充值佣的本金。
pub async fn note_redemption_debit(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    amount: f64,
) -> Result<()> {
    let amount = crate::money::round_money(amount);
    if amount <= 0.0 {
        return Ok(());
    }
    let left: f64 = sqlx::query_scalar(&db.format_query(
        "SELECT COALESCE(redemption_commission_exempt, 0) FROM users WHERE id = ? FOR UPDATE",
    ))
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    let (_, take) = split_principal(amount, left);
    if take <= 0.0 {
        return Ok(());
    }
    sqlx::query(&db.format_query(
        "UPDATE users SET redemption_commission_exempt = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    ))
    .bind(crate::money::round_money(left - take).max(0.0))
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 系统钱包退回时，把这次支出吃掉的本金加回去。
pub async fn note_system_refund(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    amount: f64,
) -> Result<()> {
    add_recharge_principal(db, tx, user_id, amount).await
}

#[derive(sqlx::FromRow)]
struct LotRow {
    id: i64,
    inviter_id: String,
    base_amount: f64,
    awarded: f64,
    clawed: f64,
}

fn lot_view(row: &LotRow) -> CommissionLotView {
    CommissionLotView {
        id: row.id,
        inviter_id: row.inviter_id.clone(),
        base_amount: row.base_amount,
        awarded: row.awarded,
        clawed: row.clawed,
    }
}

async fn lock_open_lots(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    from_user_id: &str,
    source_prefix: Option<&str>,
) -> Result<Vec<LotRow>> {
    let rows = if let Some(prefix) = source_prefix {
        sqlx::query_as::<_, LotRow>(&db.format_query(
            "SELECT id, inviter_id, base_amount, awarded, clawed FROM commission_lots \
             WHERE from_user_id = ? AND awarded > clawed AND source_key LIKE ? \
             ORDER BY id DESC FOR UPDATE",
        ))
        .bind(from_user_id)
        .bind(format!("{prefix}%"))
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query_as::<_, LotRow>(&db.format_query(
            "SELECT id, inviter_id, base_amount, awarded, clawed FROM commission_lots \
             WHERE from_user_id = ? AND awarded > clawed \
             ORDER BY id DESC FOR UPDATE",
        ))
        .bind(from_user_id)
        .fetch_all(&mut **tx)
        .await?
    };
    Ok(rows)
}

async fn insert_commission_lot(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    source_key: &str,
    inviter_id: &str,
    from_user_id: &str,
    base_amount: f64,
    awarded: f64,
) -> Result<()> {
    let base_amount = crate::money::round_money(base_amount);
    let awarded = crate::money::round_money(awarded);
    if base_amount <= 0.0 || awarded <= 0.0 {
        return Ok(());
    }
    sqlx::query(&db.format_query(
        "INSERT INTO commission_lots (source_key, inviter_id, from_user_id, base_amount, awarded, clawed) \
         VALUES (?, ?, ?, ?, ?, 0) \
         ON CONFLICT (source_key) DO NOTHING",
    ))
    .bind(source_key)
    .bind(inviter_id)
    .bind(from_user_id)
    .bind(base_amount)
    .bind(awarded)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn mark_lots_clawed(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    slices: &[LotClawSlice],
) -> Result<()> {
    for slice in slices {
        sqlx::query(&db.format_query(
            "UPDATE commission_lots SET clawed = LEAST(awarded, clawed + ?) WHERE id = ?",
        ))
        .bind(slice.award_cut)
        .bind(slice.lot_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// 按邀请人汇总后扣回。佣金钱包不够的差额扣该邀请人的系统钱包。
async fn apply_grouped_claw(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    from_user_id: &str,
    slices: &[LotClawSlice],
    idem_key: &str,
    child_label: &str,
    operator: &str,
) -> Result<(f64, f64)> {
    let mut order: Vec<String> = Vec::new();
    let mut cuts: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for slice in slices {
        let entry = cuts.entry(slice.inviter_id.clone()).or_insert(0.0);
        if *entry == 0.0 {
            order.push(slice.inviter_id.clone());
        }
        *entry = crate::money::round_money(*entry + slice.award_cut);
    }
    let mut from_commission = 0.0;
    let mut from_system = 0.0;
    for inviter_id in order {
        let cut = cuts.get(&inviter_id).copied().unwrap_or(0.0);
        if cut <= 0.0 {
            continue;
        }
        let balance = lock_commission_balance(db, tx, &inviter_id).await?;
        let (want_commission, want_system) = split_admin_clawback(cut, balance);
        if want_commission > 0.0 {
            let source_key = format!("claw:{idem_key}:{inviter_id}");
            let applied = settle_commission(
                db,
                tx,
                &inviter_id,
                from_user_id,
                None,
                -want_commission,
                0.0,
                &source_key,
            )
            .await?;
            from_commission = crate::money::round_money(from_commission + (-applied));
        }
        if want_system > 0.0 {
            let action = if idem_key.starts_with("adjust:") {
                "余额调减"
            } else {
                "退款回冲"
            };
            let remark = format!("下级 {child_label} {action}，返佣差额从系统钱包扣回");
            let order_no = crate::api::plugins::finance::generate_system_order_no();
            sqlx::query(&db.format_query(
                "UPDATE users SET balance = balance - ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            ))
            .bind(want_system)
            .bind(&inviter_id)
            .execute(&mut **tx)
            .await?;
            sqlx::query(&db.format_query(
                "INSERT INTO recharge_records (user_id, amount, recharge_type, remark, operator, wallet_type, order_no) \
                 VALUES (?, ?, 'manual', ?, ?, 'system', ?)",
            ))
            .bind(&inviter_id)
            .bind(-want_system)
            .bind(&remark)
            .bind(operator)
            .bind(&order_no)
            .execute(&mut **tx)
            .await?;
            from_system = crate::money::round_money(from_system + want_system);
        }
    }
    mark_lots_clawed(db, tx, slices).await?;
    Ok((
        crate::money::round_money(from_commission),
        crate::money::round_money(from_system),
    ))
}

/// 按充值发放。仅当邀请人等级基准为 `recharge` 且比例大于 0 时入账。返回实际入账金额。
pub async fn award_recharge_commission(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    recharge_id: i64,
    amount: f64,
) -> Result<f64> {
    let amount = crate::money::round_money(amount);
    if amount <= 0.0 {
        return Ok(0.0);
    }
    let Some(terms) = load_inviter_terms(db, tx, user_id).await? else {
        return Ok(0.0);
    };
    if normalize_commission_basis(&terms.commission_basis) != BASIS_RECHARGE
        || !new_awards_open(terms.marketing_enabled, terms.commission_ratio)
    {
        return Ok(0.0);
    }
    let commission = commission_amount(amount, terms.commission_ratio);
    if commission <= 0.0 {
        return Ok(0.0);
    }
    let source_key = format!("recharge:{recharge_id}");
    let credited = settle_commission(
        db,
        tx,
        &terms.id,
        user_id,
        Some(recharge_id),
        commission,
        terms.commission_ratio,
        &source_key,
    )
    .await?;
    if credited > 0.0 {
        insert_commission_lot(
            db,
            tx,
            &source_key,
            &terms.id,
            user_id,
            amount,
            credited,
        )
        .await?;
        add_recharge_principal(db, tx, user_id, amount).await?;
    }
    Ok(credited)
}

/// 结算前测算本次可计佣的系统钱包消费。锁定用户行，与随后的钱包扣减读到同一笔余额。
/// 已返充值本金和信控垫付都会先从基数里拿掉；专属推广关闭时仍吃掉本金，只是不发新佣。
pub async fn prepare_consumption_commission(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    settled_cost: f64,
    pre_deducted: f64,
    pre_deduct_gift: f64,
    is_freeze: bool,
) -> Result<Option<ConsumptionAward>> {
    if is_freeze || crate::money::round_money(settled_cost) <= 0.0 {
        return Ok(None);
    }
    let (_, apply) = crate::money::settlement_delta(settled_cost, pre_deducted);
    let (balance_now, gift_now): (f64, f64) = sqlx::query_as(&db.format_query(
        "SELECT balance, gift_balance FROM users WHERE id = ? FOR UPDATE",
    ))
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    let gift_before_extra = if apply > 0.0 { gift_now } else { 0.0 };
    let base = crate::money::paid_consumption_for_commission(
        settled_cost,
        pre_deducted,
        pre_deduct_gift,
        gift_before_extra,
        false,
    );
    let pre = crate::money::round_money(pre_deducted).max(0.0);
    let pre_gift = crate::money::round_money(pre_deduct_gift).clamp(0.0, pre);
    let held_cash = crate::money::round_money((pre - pre_gift).max(0.0));
    let balance_before = crate::money::round_money(balance_now + held_cash);
    let real = cash_commission_base(base, balance_before);
    let taxable = consume_recharge_principal(db, tx, user_id, real)
        .await?
        .taxable;
    let Some(terms) = load_inviter_terms(db, tx, user_id).await? else {
        return Ok(None);
    };
    if normalize_commission_basis(&terms.commission_basis) != BASIS_CONSUMPTION
        || !new_awards_open(terms.marketing_enabled, terms.commission_ratio)
    {
        return Ok(None);
    }
    if commission_amount(taxable, terms.commission_ratio) <= 0.0 {
        return Ok(None);
    }
    Ok(Some(ConsumptionAward {
        inviter_id: terms.id,
        ratio: terms.commission_ratio,
        base_amount: taxable,
    }))
}

pub async fn credit_consumption_commission(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    from_user_id: &str,
    source_key: &str,
    award: &ConsumptionAward,
) -> Result<()> {
    let commission = commission_amount(award.base_amount, award.ratio);
    if commission <= 0.0 {
        return Ok(());
    }
    let posted = accrue_daily_consumption(
        db,
        tx,
        &award.inviter_id,
        from_user_id,
        commission,
        award.ratio,
        source_key,
    )
    .await?;
    if posted > 0.0 {
        insert_commission_lot(
            db,
            tx,
            source_key,
            &award.inviter_id,
            from_user_id,
            award.base_amount,
            posted,
        )
        .await?;
    }
    Ok(())
}

/// 已经拆好的系统钱包实扣（方舟视频等增量扣费）。
/// `balance_before` 是这笔扣款之前的系统余额。信控垫付不发佣，已返充值本金先被吃掉。
pub async fn award_paid_consumption(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    source_key: &str,
    paid_amount: f64,
    balance_before: f64,
) -> Result<()> {
    let paid_amount = crate::money::round_money(paid_amount);
    if paid_amount <= 0.0 {
        return Ok(());
    }
    let real = cash_commission_base(paid_amount, balance_before);
    let taken = consume_recharge_principal(db, tx, user_id, real).await?;
    let taxable = taken.taxable;
    if let Some(id) = source_key.strip_prefix("ark:") {
        if let Ok(id) = id.parse::<i64>() {
            sqlx::query(&db.format_query(
                "UPDATE recharge_records SET real_cash = ?, principal_exempt = ?, redemption_exempt = ? WHERE id = ?",
            ))
            .bind(real)
            .bind(taken.recharge)
            .bind(taken.redemption)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        }
    }
    let Some(terms) = load_inviter_terms(db, tx, user_id).await? else {
        return Ok(());
    };
    if normalize_commission_basis(&terms.commission_basis) != BASIS_CONSUMPTION
        || !new_awards_open(terms.marketing_enabled, terms.commission_ratio)
        || taxable <= 0.0
    {
        return Ok(());
    }
    let award = ConsumptionAward {
        inviter_id: terms.id,
        ratio: terms.commission_ratio,
        base_amount: taxable,
    };
    credit_consumption_commission(db, tx, user_id, source_key, &award).await
}

/// 方舟系统钱包退款：先退回吃掉的充值本金，再按原入账佣金回冲。赠送部分不进这里。
pub async fn clawback_ark_cash(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    binding_pattern: &str,
    claw_base: f64,
    idem_key: &str,
) -> Result<()> {
    let mut left = crate::money::round_money(claw_base);
    if left <= 0.0 {
        return Ok(());
    }
    #[derive(sqlx::FromRow)]
    struct ArkCashRow {
        id: i64,
        cash: f64,
        real_cash: f64,
        principal_exempt: f64,
        redemption_exempt: f64,
        system_refunded: f64,
    }
    let rows: Vec<ArkCashRow> = sqlx::query_as(&db.format_query(
        "SELECT id, (-amount)::float8 AS cash, COALESCE(real_cash, 0)::float8 AS real_cash, \
                COALESCE(principal_exempt, 0)::float8 AS principal_exempt, \
                COALESCE(redemption_exempt, 0)::float8 AS redemption_exempt, \
                COALESCE(system_refunded, 0)::float8 AS system_refunded \
         FROM recharge_records \
         WHERE user_id = ? AND operator = 'ark_monitor' \
           AND recharge_type = 'ark_video_consume' AND wallet_type = 'system' \
           AND remark LIKE ? AND amount < 0 \
         ORDER BY id DESC FOR UPDATE",
    ))
    .bind(user_id)
    .bind(binding_pattern)
    .fetch_all(&mut **tx)
    .await?;
    let mut restore = 0.0;
    let mut slices: Vec<LotClawSlice> = Vec::new();
    for row in rows {
        if left <= 0.0 {
            break;
        }
        let lot: Option<LotRow> = sqlx::query_as(&db.format_query(
            "SELECT id, inviter_id, base_amount, awarded, clawed FROM commission_lots \
             WHERE source_key = ? FOR UPDATE",
        ))
        .bind(format!("ark:{}", row.id))
        .fetch_optional(&mut **tx)
        .await?;
        let lot_base = lot
            .as_ref()
            .map(|item| {
                let view = lot_view(item);
                let awarded = crate::money::round_money(view.awarded);
                let remaining = crate::money::round_money(awarded - view.clawed);
                if awarded <= 0.0 || remaining <= 0.0 || view.base_amount <= 0.0 {
                    0.0
                } else {
                    crate::money::round_money(view.base_amount * remaining / awarded)
                }
            })
            .unwrap_or(0.0);
        let plan = plan_ark_row_refund(
            left,
            row.cash,
            row.real_cash,
            row.system_refunded,
            row.principal_exempt,
            lot_base,
            row.redemption_exempt,
        );
        if plan.consumed <= 0.0 {
            continue;
        }
        sqlx::query(&db.format_query(
            "UPDATE recharge_records SET system_refunded = system_refunded + ?, \
             principal_exempt = GREATEST(principal_exempt - ?, 0), \
             redemption_exempt = GREATEST(redemption_exempt - ?, 0) WHERE id = ?",
        ))
        .bind(plan.consumed)
        .bind(plan.take_exempt)
        .bind(plan.take_redemption)
        .bind(row.id)
        .execute(&mut **tx)
        .await?;
        if plan.take_exempt > 0.0 {
            restore = crate::money::round_money(restore + plan.take_exempt);
        }
        if plan.take_redemption > 0.0 {
            note_redemption_credit(db, tx, user_id, plan.take_redemption).await?;
        }
        if plan.take_taxable > 0.0 {
            if let Some(lot) = lot {
                let one = allocate_lot_clawback(&[lot_view(&lot)], plan.take_taxable);
                if let Some(slice) = one.into_iter().next() {
                    if slice.award_cut > 0.0 {
                        slices.push(slice);
                    }
                }
            }
        }
        left = crate::money::round_money(left - plan.consumed);
    }
    if restore > 0.0 {
        add_recharge_principal(db, tx, user_id, restore).await?;
    }
    if !slices.is_empty() {
        apply_grouped_claw(
            db,
            tx,
            user_id,
            &slices,
            idem_key,
            user_id,
            "ark_monitor",
        )
        .await?;
    }
    Ok(())
}

/// 同一被邀请人、同一站点日只保留一条消费返佣，金额随调用累加。`source_key` 仍用来防止同一笔结算重复入账。
async fn accrue_daily_consumption(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    inviter_id: &str,
    from_user_id: &str,
    signed: f64,
    ratio: f64,
    source_key: &str,
) -> Result<f64> {
    let signed = crate::money::round_money(signed);
    if signed == 0.0 {
        return Ok(0.0);
    }
    let actual = if signed > 0.0 {
        signed
    } else {
        let balance = lock_commission_balance(db, tx, inviter_id).await?;
        -capped_clawback(-signed, balance)
    };
    if actual == 0.0 || !claim_commission_event(db, tx, source_key).await? {
        return Ok(0.0);
    }
    let site_tz = crate::relay::relay_settings::get_cached_site_timezone(db).await;
    let tz: chrono_tz::Tz = site_tz.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let day = chrono::Utc::now()
        .with_timezone(&tz)
        .format("%Y-%m-%d")
        .to_string();
    let day_key = daily_consumption_key(inviter_id, from_user_id, &day);
    sqlx::query(&db.format_query(
        "INSERT INTO commissions (user_id, from_user_id, amount, ratio, source_key) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (source_key) WHERE source_key IS NOT NULL \
         DO UPDATE SET amount = commissions.amount + EXCLUDED.amount, ratio = EXCLUDED.ratio",
    ))
    .bind(inviter_id)
    .bind(from_user_id)
    .bind(actual)
    .bind(ratio)
    .bind(&day_key)
    .execute(&mut **tx)
    .await?;
    sqlx::query(&db.format_query(
        "UPDATE users SET commission_balance = GREATEST(commission_balance + ?, 0), updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    ))
    .bind(actual)
    .bind(inviter_id)
    .execute(&mut **tx)
    .await?;
    Ok(actual)
}

async fn claim_commission_event(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    source_key: &str,
) -> Result<bool> {
    let updated = if let Some(id) = source_key.strip_prefix("log:") {
        let Ok(id) = id.parse::<i64>() else {
            return Ok(false);
        };
        sqlx::query(&db.format_query(
            "UPDATE logs SET commission_posted = 1 WHERE id = ? AND commission_posted = 0",
        ))
        .bind(id)
        .execute(&mut **tx)
        .await?
    } else if let Some(id) = source_key
        .strip_prefix("arkrefund:")
        .or_else(|| source_key.strip_prefix("ark:"))
    {
        let Ok(id) = id.parse::<i64>() else {
            return Ok(false);
        };
        sqlx::query(&db.format_query(
            "UPDATE recharge_records SET commission_posted = 1 WHERE id = ? AND commission_posted = 0",
        ))
        .bind(id)
        .execute(&mut **tx)
        .await?
    } else {
        return Ok(false);
    };
    Ok(updated.rows_affected() > 0)
}

/// 校验用户填写的划转金额。`min`/`max` 为 0 表示不限制。
pub fn transfer_amount(
    requested: f64,
    balance: f64,
    min_amt: f64,
    max_amt: f64,
) -> Result<f64, &'static str> {
    let requested = crate::money::round_money(requested);
    let balance = crate::money::round_money(balance);
    let min_amt = crate::money::round_money(min_amt).max(0.0);
    let max_amt = crate::money::round_money(max_amt).max(0.0);
    if requested <= 0.0 || balance <= 0.0 {
        return Err("empty");
    }
    if max_amt > 0.0 && min_amt > max_amt {
        return Err("conflict");
    }
    if min_amt > 0.0 && (balance < min_amt || requested < min_amt) {
        return Err("below_min");
    }
    if requested > balance {
        return Err("over_balance");
    }
    if max_amt > 0.0 && requested > max_amt {
        return Err("above_max");
    }
    Ok(requested)
}

pub struct AdminClawback {
    pub from_commission: f64,
    pub from_system: f64,
}

pub struct RecordedClawQuote {
    pub eligible: bool,
    pub suggested: f64,
    pub from_commission: f64,
    pub from_system: f64,
    pub actual: f64,
    pub inviter_names: String,
    pub commission_balance: f64,
    pub system_balance: f64,
    pub default_apply: bool,
}

/// 管理端扣回预览：按还没退完的入账佣金估算，不看当前比例。
pub async fn quote_recorded_clawback(
    db: &crate::db::Database,
    from_user_id: &str,
    refund_base: f64,
    basis: &str,
) -> Result<RecordedClawQuote> {
    let none = RecordedClawQuote {
        eligible: false,
        suggested: 0.0,
        from_commission: 0.0,
        from_system: 0.0,
        actual: 0.0,
        inviter_names: String::new(),
        commission_balance: 0.0,
        system_balance: 0.0,
        default_apply: false,
    };
    let refund_base = crate::money::round_money(refund_base);
    if refund_base <= 0.0 {
        return Ok(none);
    }
    let rows: Vec<LotRow> = sqlx::query_as(&db.format_query(
        "SELECT id, inviter_id, base_amount, awarded, clawed FROM commission_lots \
         WHERE from_user_id = ? AND awarded > clawed ORDER BY id DESC",
    ))
    .bind(from_user_id)
    .fetch_all(&db.pool)
    .await?;
    let views: Vec<CommissionLotView> = rows.iter().map(lot_view).collect();
    let slices = allocate_lot_clawback(&views, refund_base);
    if slices.is_empty() {
        return Ok(none);
    }
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeMap::<String, f64>::new();
    let mut order: Vec<String> = Vec::new();
    for slice in &slices {
        let entry = seen.entry(slice.inviter_id.clone()).or_insert(0.0);
        if *entry == 0.0 {
            order.push(slice.inviter_id.clone());
        }
        *entry = crate::money::round_money(*entry + slice.award_cut);
    }
    let mut from_commission = 0.0;
    let mut from_system = 0.0;
    let mut commission_balance = 0.0;
    let mut system_balance = 0.0;
    for inviter_id in &order {
        let cut = seen.get(inviter_id).copied().unwrap_or(0.0);
        let row: Option<(String, f64, f64)> = sqlx::query_as(&db.format_query(
            "SELECT username, commission_balance, balance FROM users WHERE id = ?",
        ))
        .bind(inviter_id)
        .fetch_optional(&db.pool)
        .await?;
        let Some((username, comm, system)) = row else {
            continue;
        };
        names.push(username);
        let (part_comm, part_sys) = split_admin_clawback(cut, comm);
        from_commission = crate::money::round_money(from_commission + part_comm);
        from_system = crate::money::round_money(from_system + part_sys);
        commission_balance = crate::money::round_money(commission_balance + comm);
        system_balance = crate::money::round_money(system_balance + system);
    }
    let suggested = crate::money::round_money(from_commission + from_system);
    if suggested <= 0.0 {
        return Ok(none);
    }
    Ok(RecordedClawQuote {
        eligible: true,
        suggested,
        from_commission,
        from_system,
        actual: suggested,
        inviter_names: names.join("、"),
        commission_balance,
        system_balance,
        default_apply: normalize_commission_basis(basis) == BASIS_RECHARGE,
    })
}

/// 管理端明确选择扣回时使用。按已入账佣金回冲，先扣佣金钱包，差额再扣当时那位上级的系统钱包。
pub async fn clawback_admin_commission(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    recharge_id: i64,
    base_amount: f64,
    operator: &str,
    child_username: &str,
    _system_order_no: &str,
) -> Result<AdminClawback> {
    let base_amount = crate::money::round_money(base_amount);
    if base_amount <= 0.0 {
        return Ok(AdminClawback {
            from_commission: 0.0,
            from_system: 0.0,
        });
    }
    let rows = lock_open_lots(db, tx, user_id, None).await?;
    let views: Vec<CommissionLotView> = rows.iter().map(lot_view).collect();
    let slices = allocate_lot_clawback(&views, base_amount);
    if slices.is_empty() {
        return Ok(AdminClawback {
            from_commission: 0.0,
            from_system: 0.0,
        });
    }
    let (from_commission, from_system) = apply_grouped_claw(
        db,
        tx,
        user_id,
        &slices,
        &format!("adjust:{recharge_id}"),
        child_username,
        operator,
    )
    .await?;
    Ok(AdminClawback {
        from_commission,
        from_system,
    })
}

#[cfg(test)]
mod commission_wallet_tests {
    use super::{
        allocate_lot_clawback, capped_clawback, cash_commission_base, daily_consumption_key,
        new_awards_open, plan_ark_row_refund, quote_adjust, split_admin_clawback, split_ark_refund,
        split_principal, transfer_amount, CommissionLotView,
    };

    #[test]
    fn clawback_stops_at_commission_wallet() {
        assert_eq!(capped_clawback(3.0, 10.0), 3.0);
        assert_eq!(capped_clawback(3.0, 1.25), 1.25);
        assert_eq!(capped_clawback(3.0, 0.0), 0.0);
        assert_eq!(capped_clawback(0.0, 5.0), 0.0);
    }

    #[test]
    fn admin_clawback_takes_commission_then_system() {
        assert_eq!(split_admin_clawback(5.0, 0.0), (0.0, 5.0));
        assert_eq!(split_admin_clawback(5.0, 2.0), (2.0, 3.0));
        assert_eq!(split_admin_clawback(5.0, 9.0), (5.0, 0.0));
        assert_eq!(split_admin_clawback(0.0, 4.0), (0.0, 0.0));
    }

    #[test]
    fn recharge_topup_defaults_to_award() {
        let quote = quote_adjust(100.0, 0.1, "recharge", 0.0);
        assert!(quote.eligible);
        assert_eq!(quote.direction, "award");
        assert_eq!(quote.actual, 10.0);
        assert!(quote.default_apply);
    }

    #[test]
    fn consumption_topup_does_not_award() {
        let quote = quote_adjust(100.0, 0.1, "consumption", 50.0);
        assert!(!quote.eligible);
    }

    #[test]
    fn recharge_decrease_defaults_to_clawback_and_spills() {
        let quote = quote_adjust(-100.0, 0.1, "recharge", 4.0);
        assert!(quote.eligible);
        assert_eq!(quote.direction, "clawback");
        assert_eq!(quote.suggested, 10.0);
        assert_eq!(quote.from_commission, 4.0);
        assert_eq!(quote.from_system, 6.0);
        assert_eq!(quote.actual, 10.0);
        assert!(quote.default_apply);
    }

    #[test]
    fn consumption_decrease_asks_but_defaults_off() {
        let quote = quote_adjust(-80.0, 0.1, "consumption", 100.0);
        assert!(quote.eligible);
        assert_eq!(quote.from_commission, 8.0);
        assert_eq!(quote.from_system, 0.0);
        assert_eq!(quote.actual, 8.0);
        assert!(!quote.default_apply);
    }

    #[test]
    fn cash_above_zero_is_the_only_commission_base() {
        assert_eq!(cash_commission_base(10.0, 4.0), 4.0);
        assert_eq!(cash_commission_base(10.0, 0.0), 0.0);
        assert_eq!(cash_commission_base(10.0, -3.0), 0.0);
        assert_eq!(cash_commission_base(6.0, 20.0), 6.0);
    }

    #[test]
    fn principal_is_eaten_before_new_consumption_commission() {
        assert_eq!(split_principal(40.0, 100.0), (0.0, 40.0));
        assert_eq!(split_principal(100.0, 100.0), (0.0, 100.0));
        assert_eq!(split_principal(130.0, 100.0), (30.0, 100.0));
        assert_eq!(split_principal(20.0, 0.0), (20.0, 0.0));
    }

    #[test]
    fn ark_refund_returns_cash_before_gift() {
        let full = split_ark_refund(10.0, 4.0, 6.0);
        assert_eq!(full.to_system, 4.0);
        assert_eq!(full.to_gift, 6.0);
        assert_eq!(full.claw_base, 4.0);
        let partial = split_ark_refund(3.0, 4.0, 6.0);
        assert_eq!(partial.to_system, 3.0);
        assert_eq!(partial.to_gift, 0.0);
        assert_eq!(partial.claw_base, 3.0);
        let gift_only = split_ark_refund(5.0, 0.0, 6.0);
        assert_eq!(gift_only.to_system, 0.0);
        assert_eq!(gift_only.to_gift, 5.0);
        assert_eq!(gift_only.claw_base, 0.0);
    }

    #[test]
    fn ark_credit_refund_does_not_eat_older_cash_commission() {
        let credit_only = plan_ark_row_refund(10.0, 10.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(credit_only.consumed, 10.0);
        assert_eq!(credit_only.take_taxable, 0.0);
        assert_eq!(credit_only.take_exempt, 0.0);
        let mixed = plan_ark_row_refund(10.0, 10.0, 4.0, 0.0, 0.0, 4.0, 0.0);
        assert_eq!(mixed.consumed, 10.0);
        assert_eq!(mixed.take_taxable, 4.0);
        assert_eq!(mixed.take_exempt, 0.0);
        let principal = plan_ark_row_refund(3.0, 10.0, 10.0, 0.0, 10.0, 0.0, 0.0);
        assert_eq!(principal.consumed, 3.0);
        assert_eq!(principal.take_taxable, 0.0);
        assert_eq!(principal.take_exempt, 3.0);
        let redeemed = plan_ark_row_refund(10.0, 10.0, 10.0, 0.0, 4.0, 0.0, 6.0);
        assert_eq!(redeemed.take_exempt, 4.0);
        assert_eq!(redeemed.take_redemption, 6.0);
    }

    #[test]
    fn lot_clawback_uses_recorded_award_not_a_new_ratio() {
        let lots = vec![
            CommissionLotView {
                id: 2,
                inviter_id: "new".into(),
                base_amount: 40.0,
                awarded: 4.0,
                clawed: 0.0,
            },
            CommissionLotView {
                id: 1,
                inviter_id: "old".into(),
                base_amount: 100.0,
                awarded: 10.0,
                clawed: 0.0,
            },
        ];
        let slices = allocate_lot_clawback(&lots, 50.0);
        assert_eq!(slices.len(), 2);
        assert_eq!(slices[0].inviter_id, "new");
        assert_eq!(slices[0].base_used, 40.0);
        assert_eq!(slices[0].award_cut, 4.0);
        assert_eq!(slices[1].inviter_id, "old");
        assert_eq!(slices[1].base_used, 10.0);
        assert_eq!(slices[1].award_cut, 1.0);
        let capped = allocate_lot_clawback(&lots, 500.0);
        let total: f64 = capped.iter().map(|s| s.award_cut).sum();
        assert_eq!(total, 14.0);
    }

    #[test]
    fn marketing_off_blocks_new_awards_only() {
        assert!(!new_awards_open(0, 0.1));
        assert!(!new_awards_open(1, 0.0));
        assert!(new_awards_open(1, 0.1));
    }

    #[test]
    fn daily_key_changes_with_inviter() {
        assert_eq!(
            daily_consumption_key("inv-a", "child", "2026-10-05"),
            "daycons:inv-a:child:2026-10-05"
        );
        assert_ne!(
            daily_consumption_key("inv-a", "child", "2026-10-05"),
            daily_consumption_key("inv-b", "child", "2026-10-05")
        );
    }

    #[test]
    fn transfer_respects_min_and_max() {
        assert_eq!(transfer_amount(40.0, 100.0, 0.0, 0.0), Ok(40.0));
        assert_eq!(transfer_amount(20.0, 100.0, 10.0, 30.0), Ok(20.0));
        assert_eq!(transfer_amount(40.0, 100.0, 10.0, 30.0), Err("above_max"));
        assert_eq!(transfer_amount(5.0, 100.0, 10.0, 0.0), Err("below_min"));
        assert_eq!(transfer_amount(8.0, 8.0, 10.0, 0.0), Err("below_min"));
        assert_eq!(transfer_amount(50.0, 40.0, 0.0, 0.0), Err("over_balance"));
        assert_eq!(transfer_amount(0.0, 100.0, 0.0, 0.0), Err("empty"));
        assert_eq!(transfer_amount(20.0, 100.0, 50.0, 10.0), Err("conflict"));
    }
}
