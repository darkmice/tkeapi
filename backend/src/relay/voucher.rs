/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 模型卡券兑换码的结算占用与退回。
//! 预扣不进这里：请求能不能开始只看账户余额。最终结算才按兑换先后占用卡券，
//! 并让钱包把已被卡券承担的预扣退回去。
//! Note: 见 .agents/notes/implemented/feature/2026-10-09-model-voucher-redemption.md

use std::collections::BTreeMap;

use sqlx::{Postgres, Transaction};

use crate::money::round_money;

#[derive(Debug, Clone)]
pub struct ActiveModelVoucher {
    pub remaining: f64,
    pub discount: f64,
}

pub fn resolve_voucher_discount(
    default_discount: f64,
    model_discounts_json: &str,
    candidate_names: &[String],
) -> f64 {
    let base = if default_discount.is_finite() && default_discount > 0.0 && default_discount <= 1.0
    {
        (default_discount * 10000.0).round() / 10000.0
    } else {
        1.0
    };
    let map: BTreeMap<String, f64> = serde_json::from_str(model_discounts_json).unwrap_or_default();
    if !map.is_empty() {
        for name in candidate_names {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(&d) = map.get(trimmed) {
                if d.is_finite() && d > 0.0 && d <= 1.0 {
                    return (d * 10000.0).round() / 10000.0;
                }
            }
        }
    }
    base
}

pub fn build_candidate_model_names(
    db_model: Option<&crate::models::Model>,
    requested_model: &str,
    resolved_model: &str,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut push = |raw: &str| {
        let s = raw.trim();
        if !s.is_empty() && !names.iter().any(|existing| existing == s) {
            names.push(s.to_string());
        }
    };
    if let Some(m) = db_model {
        push(&m.mid);
        push(&m.model_id);
        push(&m.model_id_alias);
    }
    push(requested_model);
    push(resolved_model);
    names
}

pub async fn active_vouchers_for_model(
    db: &crate::db::Database,
    user_id: &str,
    db_model: Option<&crate::models::Model>,
    requested_model: &str,
    resolved_model: &str,
) -> Vec<ActiveModelVoucher> {
    if user_id.trim().is_empty() {
        return Vec::new();
    }
    let names = build_candidate_model_names(db_model, requested_model, resolved_model);
    if names.is_empty() {
        return Vec::new();
    }
    let rows: Vec<(f64, String, f64, String)> = sqlx::query_as(&db.format_query(
        "SELECT remaining, allowed_models, \
                COALESCE(credit_discount, 1.0), COALESCE(model_discounts, '{}') \
         FROM user_model_vouchers \
         WHERE user_id = ? AND status = 1 AND remaining > 0 \
           AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP) \
         ORDER BY redeemed_at ASC, id ASC",
    ))
    .bind(user_id)
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .filter(|(_, allowed, _, _)| model_allowed(allowed, &names))
        .filter_map(
            |(remaining, _, credit_discount, model_discounts)| {
                let rem = round_money(remaining);
                if rem <= 0.0 {
                    return None;
                }
                let discount = resolve_voucher_discount(credit_discount, &model_discounts, &names);
                Some(ActiveModelVoucher {
                    remaining: rem,
                    discount,
                })
            },
        )
        .collect()
}

pub fn model_allowed(allowed_models_json: &str, names: &[String]) -> bool {
    let ids: Vec<String> = serde_json::from_str(allowed_models_json).unwrap_or_default();
    if ids.is_empty() || names.is_empty() {
        return false;
    }
    ids.iter().any(|id| {
        let id = id.trim();
        !id.is_empty() && names.iter().any(|name| name == id)
    })
}

/// 按当前请求日志精准解析对应的唯一模型记录（优先匹配 mid、渠道绑定 mid、转发规则 eid、计费规则 pid），
/// 避免同 model_id 的不同 mid 模型互相串用卡券。
pub(crate) async fn equivalent_names(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    log_id: i64,
    model: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let model = model.trim();
    if model.is_empty() {
        return Ok(Vec::new());
    }
    let row: Option<(String, String, String)> = sqlx::query_as(&db.format_query(
        "SELECT m.model_id, COALESCE(m.mid, ''), COALESCE(m.model_id_alias, '') \
         FROM models m \
         LEFT JOIN logs l ON l.id = ? \
         LEFT JOIN channels c ON c.id = l.channel_id \
         WHERE m.mid = ? OR m.model_id = ? OR m.model_id_alias = ? \
         ORDER BY \
           CASE \
             WHEN m.mid = ? THEN 0 \
             WHEN c.models IS NOT NULL AND position('\"' || m.mid || '\"' in c.models) > 0 THEN 1 \
             WHEN l.forward_eid IS NOT NULL AND l.forward_eid <> '' \
                  AND EXISTS (SELECT 1 FROM forward_rules fr WHERE fr.eid = l.forward_eid AND COALESCE(m.forward_rule_ids, '') ~ ('\\m' || fr.id || '\\M')) THEN 2 \
             WHEN l.billing_pid IS NOT NULL AND l.billing_pid <> '' \
                  AND EXISTS (SELECT 1 FROM billing_rules br WHERE br.id = m.billing_rule_id AND br.pid = l.billing_pid) THEN 3 \
             ELSE 4 \
           END, \
           m.id DESC \
         LIMIT 1",
    ))
    .bind(log_id)
    .bind(model)
    .bind(model)
    .bind(model)
    .bind(model)
    .fetch_optional(&mut **tx)
    .await?;
    let mut names = vec![model.to_string()];
    if let Some((model_id, mid, alias)) = row {
        for name in [mid, model_id, alias] {
            let name = name.trim().to_string();
            if !name.is_empty() && !names.iter().any(|existing| existing == &name) {
                names.push(name);
            }
        }
    }
    Ok(names)
}

/// 按传入顺序占用。返回 `(实占, [(voucher_id, take)])`。
pub fn take_fifo(buckets: &[(i64, f64)], amount: f64) -> (f64, Vec<(i64, f64)>) {
    let amount = round_money(amount);
    let mut left = amount;
    let mut holds = Vec::new();
    if left <= 0.0 {
        return (0.0, holds);
    }
    for (id, remaining) in buckets {
        if left <= 0.0 {
            break;
        }
        let remaining = round_money(*remaining);
        if remaining <= 0.0 {
            continue;
        }
        let take = round_money(left.min(remaining));
        if take <= 0.0 {
            continue;
        }
        holds.push((*id, take));
        left = round_money(left - take);
    }
    (round_money(amount - left), holds)
}

/// 结算目标：能用卡券覆盖的部分，不超过已占用加仍可用余额。
pub fn voucher_target(settled: f64, held: f64, extra: f64) -> f64 {
    let cap = round_money(round_money(held).max(0.0) + round_money(extra).max(0.0));
    round_money(round_money(settled).max(0.0).min(cap))
}

/// 从后往前退占用，让先兑换的卡券继续承担。
/// holds 为 `(hold_id, voucher_id, amount)`，按占用先后排列。
pub fn plan_release(holds: &[(i64, i64, f64)], release: f64) -> Vec<(i64, i64, f64)> {
    let mut left = round_money(release);
    let mut plan = Vec::new();
    if left <= 0.0 {
        return plan;
    }
    for (hold_id, voucher_id, amount) in holds.iter().rev() {
        if left <= 0.0 {
            break;
        }
        let amount = round_money(*amount);
        if amount <= 0.0 {
            continue;
        }
        let give = round_money(left.min(amount));
        if give <= 0.0 {
            continue;
        }
        plan.push((*hold_id, *voucher_id, give));
        left = round_money(left - give);
    }
    plan
}

async fn lock_user(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(&db.format_query("SELECT id FROM users WHERE id = ? FOR UPDATE"))
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn has_spendable_card(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<bool, sqlx::Error> {
    let id: Option<i64> = sqlx::query_scalar(&db.format_query(
        "SELECT id FROM user_model_vouchers \
         WHERE user_id = ? AND status = 1 AND remaining > 0 \
           AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP) \
         LIMIT 1",
    ))
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(id.is_some())
}

/// 把这笔日志占用的卡券全部退回。返回退回金额。
pub async fn release_holds(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    log_id: i64,
) -> Result<f64, sqlx::Error> {
    if log_id <= 0 {
        return Ok(0.0);
    }
    let pending: f64 = sqlx::query_scalar(&db.format_query(
        "SELECT COALESCE(SUM(amount), 0) FROM user_model_voucher_holds WHERE log_id = ?",
    ))
    .bind(log_id)
    .fetch_one(&mut **tx)
    .await?;
    if round_money(pending) <= 0.0 {
        return Ok(0.0);
    }
    lock_user(db, tx, user_id).await?;
    let holds: Vec<(i64, i64, f64)> = sqlx::query_as(&db.format_query(
        "SELECT id, voucher_id, amount FROM user_model_voucher_holds \
         WHERE log_id = ? ORDER BY id ASC FOR UPDATE",
    ))
    .bind(log_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut total = 0.0;
    for (_hold_id, voucher_id, amount) in &holds {
        let amount = round_money(*amount);
        if amount <= 0.0 {
            continue;
        }
        sqlx::query(&db.format_query(
            "UPDATE user_model_vouchers \
             SET remaining = ROUND((remaining + ?)::numeric, 6)::double precision, updated_at = CURRENT_TIMESTAMP \
             WHERE id = ? AND status = 1",
        ))
        .bind(amount)
        .bind(voucher_id)
        .execute(&mut **tx)
        .await?;
        total = round_money(total + amount);
    }
    if !holds.is_empty() {
        sqlx::query(&db.format_query(
            "DELETE FROM user_model_voucher_holds WHERE log_id = ?",
        ))
        .bind(log_id)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query(&db.format_query(
        "UPDATE logs SET pre_deduct_voucher = 0 WHERE id = ?",
    ))
    .bind(log_id)
    .execute(&mut **tx)
    .await?;
    Ok(total)
}

/// 把占用调到最终应付。先兑换的卡券优先留下。返回 `(最终卡券, 调整前占用)`。
pub async fn retarget_for_log(
    db: &crate::db::Database,
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    log_id: i64,
    model: &str,
    settled: f64,
) -> Result<(f64, f64), sqlx::Error> {
    if log_id <= 0 {
        return Ok((0.0, 0.0));
    }
    let pending: f64 = sqlx::query_scalar(&db.format_query(
        "SELECT COALESCE(SUM(amount), 0) FROM user_model_voucher_holds WHERE log_id = ?",
    ))
    .bind(log_id)
    .fetch_one(&mut **tx)
    .await?;
    let pending = round_money(pending);
    let settled = round_money(settled.max(0.0));
    if pending <= 0.0 && (settled <= 0.0 || !has_spendable_card(db, tx, user_id).await?) {
        return Ok((0.0, 0.0));
    }
    let names = equivalent_names(db, tx, log_id, model).await?;
    lock_user(db, tx, user_id).await?;
    let holds: Vec<(i64, i64, f64)> = sqlx::query_as(&db.format_query(
        "SELECT id, voucher_id, amount FROM user_model_voucher_holds \
         WHERE log_id = ? ORDER BY id ASC FOR UPDATE",
    ))
    .bind(log_id)
    .fetch_all(&mut **tx)
    .await?;
    let held = round_money(holds.iter().map(|(_, _, amount)| *amount).sum());

    let rows: Vec<(i64, f64, String)> = sqlx::query_as(&db.format_query(
        "SELECT id, remaining, allowed_models FROM user_model_vouchers \
         WHERE user_id = ? AND status = 1 AND remaining > 0 \
           AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP) \
         ORDER BY redeemed_at ASC, id ASC \
         FOR UPDATE",
    ))
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;
    let buckets: Vec<(i64, f64)> = rows
        .iter()
        .filter(|(_, _, allowed)| model_allowed(allowed, &names))
        .map(|(id, remaining, _)| (*id, *remaining))
        .collect();
    let extra = round_money(buckets.iter().map(|(_, remaining)| *remaining).sum());
    let target = voucher_target(settled, held, extra);

    if target < held {
        for (hold_id, voucher_id, give) in plan_release(&holds, round_money(held - target)) {
            sqlx::query(&db.format_query(
                "UPDATE user_model_vouchers \
                 SET remaining = ROUND((remaining + ?)::numeric, 6)::double precision, updated_at = CURRENT_TIMESTAMP \
                 WHERE id = ? AND status = 1",
            ))
            .bind(give)
            .bind(voucher_id)
            .execute(&mut **tx)
            .await?;
            sqlx::query(&db.format_query(
                "UPDATE user_model_voucher_holds \
                 SET amount = ROUND((amount - ?)::numeric, 6)::double precision WHERE id = ?",
            ))
            .bind(give)
            .bind(hold_id)
            .execute(&mut **tx)
            .await?;
        }
        sqlx::query(&db.format_query(
            "DELETE FROM user_model_voucher_holds WHERE log_id = ? AND amount <= 0",
        ))
        .bind(log_id)
        .execute(&mut **tx)
        .await?;
    } else if target > held {
        let (_taken, more) = take_fifo(&buckets, round_money(target - held));
        for (voucher_id, take) in more {
            let touched = sqlx::query(&db.format_query(
                "UPDATE user_model_vouchers \
                 SET remaining = CASE WHEN remaining - ? <= 0.0000005 THEN 0 ELSE ROUND((remaining - ?)::numeric, 6)::double precision END, \
                     updated_at = CURRENT_TIMESTAMP \
                 WHERE id = ? AND status = 1 AND remaining + 0.000001 >= ?",
            ))
            .bind(take)
            .bind(take)
            .bind(voucher_id)
            .bind(take)
            .execute(&mut **tx)
            .await?
            .rows_affected();
            if touched == 0 {
                return Err(sqlx::Error::Protocol(
                    "model voucher remaining changed during settlement".into(),
                ));
            }
            sqlx::query(&db.format_query(
                "INSERT INTO user_model_voucher_holds (voucher_id, log_id, amount) VALUES (?, ?, ?)",
            ))
            .bind(voucher_id)
            .bind(log_id)
            .bind(take)
            .execute(&mut **tx)
            .await?;
        }
    }

    sqlx::query(&db.format_query(
        "UPDATE logs SET pre_deduct_voucher = ? WHERE id = ?",
    ))
    .bind(target)
    .bind(log_id)
    .execute(&mut **tx)
    .await?;
    Ok((target, held))
}

#[cfg(test)]
mod tests {
    use super::{
        model_allowed, plan_release, resolve_voucher_discount, take_fifo, voucher_target,
    };

    #[test]
    fn fifo_stops_at_the_first_cards() {
        let (taken, holds) = take_fifo(&[(1, 4.0), (2, 3.0), (3, 9.0)], 5.0);
        assert_eq!(taken, 5.0);
        assert_eq!(holds, vec![(1, 4.0), (2, 1.0)]);
    }

    #[test]
    fn empty_model_list_matches_nothing() {
        assert!(!model_allowed("[]", &["gpt-4o".to_string()]));
        assert!(model_allowed("[\"gpt-4o\"]", &["gpt-4o".to_string()]));
        assert!(model_allowed(
            "[\"gpt-4o\"]",
            &["alias".to_string(), "gpt-4o".to_string()]
        ));
        assert!(!model_allowed("[\"gpt-4o\"]", &["claude".to_string()]));
    }

    #[test]
    fn settlement_keeps_the_earliest_hold() {
        let plan = plan_release(&[(10, 1, 4.0), (11, 2, 3.0)], 5.0);
        assert_eq!(plan, vec![(11, 2, 3.0), (10, 1, 2.0)]);
        assert_eq!(voucher_target(1.2, 0.4, 1.0), 1.2);
        assert_eq!(voucher_target(0.1, 0.4, 5.0), 0.1);
        assert_eq!(voucher_target(2.0, 0.4, 0.1), 0.5);
    }

    #[test]
    fn voucher_discount_prefers_per_model_override_and_falls_back_to_default() {
        let names = vec!["m-gpt4o".to_string(), "gpt-4o".to_string()];
        assert_eq!(
            resolve_voucher_discount(0.85, r#"{"m-gpt4o": 0.65}"#, &names),
            0.65
        );
        assert_eq!(
            resolve_voucher_discount(0.85, r#"{"other": 0.65}"#, &names),
            0.85
        );
        assert_eq!(resolve_voucher_discount(0.0, "{}", &names), 1.0);
    }
}
