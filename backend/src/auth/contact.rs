/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use std::sync::Arc;

use serde_json::json;
use sqlx::{Postgres, Transaction};

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::LoginCandidate;
use crate::AppState;

pub const DEFAULT_ACCOUNTS_PER_CONTACT: i64 = 5;
const MAX_ACCOUNTS_PER_CONTACT: i64 = 99;

#[derive(Debug, Clone, Copy)]
pub struct Occupancy {
    pub bound_count: i64,
    pub limit: i64,
}

impl Occupancy {
    pub fn remaining(&self) -> i64 {
        (self.limit - self.bound_count).max(0)
    }

    pub fn at_limit(&self) -> bool {
        would_exceed_limit(self.bound_count, self.limit, false)
    }

    pub fn extra_json(&self) -> serde_json::Value {
        json!({
            "bound_count": self.bound_count,
            "limit": self.limit,
            "remaining": self.remaining(),
        })
    }

    pub fn in_use_error(&self, contact_label: &str) -> AppError {
        AppError::ConflictDetails(
            format!(
                "该{}已绑定 {} 个账号（上限 {}），可继续注册或直接登录",
                contact_label, self.bound_count, self.limit
            ),
            "CONTACT_IN_USE",
            self.extra_json(),
        )
    }

    pub fn at_limit_error(&self, contact_label: &str) -> AppError {
        AppError::ConflictDetails(
            format!(
                "该{}已绑定 {} 个账号，已达上限 {}",
                contact_label, self.bound_count, self.limit
            ),
            "CONTACT_AT_LIMIT",
            self.extra_json(),
        )
    }
}

pub fn normalize_limit(raw: i64) -> i64 {
    if (1..=MAX_ACCOUNTS_PER_CONTACT).contains(&raw) {
        raw
    } else {
        DEFAULT_ACCOUNTS_PER_CONTACT
    }
}

pub fn would_exceed_limit(bound_count: i64, limit: i64, already_holds: bool) -> bool {
    if already_holds {
        false
    } else {
        bound_count >= limit
    }
}

pub fn is_real_email(email: &str) -> bool {
    let email = email.trim();
    !email.is_empty() && !email.to_ascii_lowercase().ends_with("@tokensbyte.local")
}

pub fn is_real_mobile(mobile: &str) -> bool {
    !mobile.trim().is_empty()
}

pub fn is_real_wechat(wechat_id: &str) -> bool {
    !wechat_id.trim().is_empty()
}

fn strip_mobile_separators(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '(' | ')'))
        .collect()
}

fn mainland_mobile(digits: &str) -> Option<String> {
    let rest = digits
        .strip_prefix("+86")
        .or_else(|| digits.strip_prefix("0086"))
        .or_else(|| {
            if digits.len() == 13 {
                digits.strip_prefix("86")
            } else {
                None
            }
        })
        .unwrap_or(digits);
    let b = rest.as_bytes();
    let ok = b.len() == 11
        && b[0] == b'1'
        && (b'3'..=b'9').contains(&b[1])
        && b.iter().all(u8::is_ascii_digit);
    ok.then(|| rest.to_string())
}

/// 手机号规范化：大陆号统一为 11 位；国际号仅在允许时接受 E.164（+国家码，共 8–15 位数字）
pub fn normalize_mobile(raw: &str, allow_international: bool) -> AppResult<String> {
    let m = strip_mobile_separators(raw);
    if let Some(cn) = mainland_mobile(&m) {
        return Ok(cn);
    }
    if allow_international && !m.starts_with("+86") {
        if let Some(digits) = m.strip_prefix('+') {
            let valid = (8..=15).contains(&digits.len())
                && !digits.starts_with('0')
                && digits.bytes().all(|c| c.is_ascii_digit());
            if valid {
                return Ok(m);
            }
        }
    }
    Err(AppError::BadRequest(if allow_international {
        "手机号格式不正确（大陆号码 11 位，国际号码请带 + 国家码）".to_string()
    } else {
        "手机号格式不正确，仅支持中国大陆 11 位手机号".to_string()
    }))
}

/// 登录查号用：能规范化就用规范值，否则原样（兼容历史数据）
pub fn canonical_mobile_lenient(raw: &str) -> String {
    let m = strip_mobile_separators(raw);
    mainland_mobile(&m).unwrap_or(m)
}

/// 邮箱规范化：去除首尾空白并转小写，防止大小写绕过同邮箱账号数上限或匹配失败
pub fn canonical_email(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

pub fn canonical_contact_value(kind: &str, value: &str) -> String {
    match kind {
        "email" => canonical_email(value),
        "mobile" => canonical_mobile_lenient(value),
        _ => value.trim().to_string(),
    }
}

fn exclude_id(exclude_user_id: Option<&str>) -> &str {
    exclude_user_id.unwrap_or("")
}

pub fn parse_contact_kind(kind: &str) -> AppResult<&'static str> {
    match kind.trim() {
        "email" => Ok("email"),
        "mobile" => Ok("mobile"),
        "wechat" => Ok("wechat"),
        _ => Err(AppError::BadRequest(
            "kind 必须是 email、mobile 或 wechat".to_string(),
        )),
    }
}

pub fn effective_limit(override_limit: Option<i64>, global_default: i64) -> i64 {
    normalize_limit(override_limit.unwrap_or(global_default))
}

pub async fn fetch_override_limit(
    state: &Arc<AppState>,
    kind: &str,
    value: &str,
) -> AppResult<Option<i64>> {
    let norm = canonical_contact_value(kind, value);
    let sql = if kind == "email" {
        "SELECT max_accounts FROM contact_bind_limits WHERE kind = ? AND LOWER(value) = LOWER(?)"
    } else {
        "SELECT max_accounts FROM contact_bind_limits WHERE kind = ? AND value = ?"
    };
    let row: Option<i64> = sqlx::query_scalar(&state.db.format_query(sql))
        .bind(kind)
        .bind(&norm)
        .fetch_optional(&state.db.pool)
        .await?;
    Ok(row)
}

async fn fetch_override_limit_tx(
    tx: &mut Transaction<'_, Postgres>,
    db: &Database,
    kind: &str,
    value: &str,
) -> AppResult<Option<i64>> {
    let norm = canonical_contact_value(kind, value);
    let sql = if kind == "email" {
        "SELECT max_accounts FROM contact_bind_limits WHERE kind = ? AND LOWER(value) = LOWER(?)"
    } else {
        "SELECT max_accounts FROM contact_bind_limits WHERE kind = ? AND value = ?"
    };
    let row: Option<i64> = sqlx::query_scalar(&db.format_query(sql))
        .bind(kind)
        .bind(&norm)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(row)
}

pub async fn upsert_override_limit(
    state: &Arc<AppState>,
    kind: &str,
    value: &str,
    max_accounts: i64,
) -> AppResult<()> {
    if !(1..=MAX_ACCOUNTS_PER_CONTACT).contains(&max_accounts) {
        return Err(AppError::BadRequest("绑定上限必须是 1–99".to_string()));
    }
    let norm = canonical_contact_value(kind, value);
    if kind == "email" {
        sqlx::query(&state.db.format_query(
            "DELETE FROM contact_bind_limits WHERE kind = 'email' AND LOWER(value) = LOWER(?) AND value != ?",
        ))
        .bind(&norm)
        .bind(&norm)
        .execute(&state.db.pool)
        .await?;
    }
    sqlx::query(&state.db.format_query(
        r#"INSERT INTO contact_bind_limits (kind, value, max_accounts)
           VALUES (?, ?, ?)
           ON CONFLICT (kind, value) DO UPDATE SET
             max_accounts = EXCLUDED.max_accounts,
             updated_at = CURRENT_TIMESTAMP"#,
    ))
    .bind(kind)
    .bind(&norm)
    .bind(max_accounts)
    .execute(&state.db.pool)
    .await?;
    Ok(())
}

pub async fn delete_override_limit(
    state: &Arc<AppState>,
    kind: &str,
    value: &str,
) -> AppResult<()> {
    let norm = canonical_contact_value(kind, value);
    let sql = if kind == "email" {
        "DELETE FROM contact_bind_limits WHERE kind = ? AND LOWER(value) = LOWER(?)"
    } else {
        "DELETE FROM contact_bind_limits WHERE kind = ? AND value = ?"
    };
    sqlx::query(&state.db.format_query(sql))
        .bind(kind)
        .bind(&norm)
        .execute(&state.db.pool)
        .await?;
    Ok(())
}

async fn resolved_email_limit(
    state: &Arc<AppState>,
    email: &str,
    global_default: i64,
) -> AppResult<i64> {
    let ov = fetch_override_limit(state, "email", email).await?;
    Ok(effective_limit(ov, global_default))
}

async fn resolved_mobile_limit(
    state: &Arc<AppState>,
    mobile: &str,
    global_default: i64,
) -> AppResult<i64> {
    let ov = fetch_override_limit(state, "mobile", mobile).await?;
    Ok(effective_limit(ov, global_default))
}

async fn resolved_wechat_limit(
    state: &Arc<AppState>,
    wechat_id: &str,
    global_default: i64,
) -> AppResult<i64> {
    let ov = fetch_override_limit(state, "wechat", wechat_id).await?;
    Ok(effective_limit(ov, global_default))
}

pub async fn email_occupancy(
    state: &Arc<AppState>,
    email: &str,
    exclude_user_id: Option<&str>,
    global_default: i64,
) -> AppResult<Occupancy> {
    let email = canonical_email(email);
    let limit = resolved_email_limit(state, &email, global_default).await?;
    if !is_real_email(&email) {
        return Ok(Occupancy {
            bound_count: 0,
            limit,
        });
    }
    let bound_count: i64 = sqlx::query_scalar(&state.db.format_query(
        "SELECT COUNT(*) FROM users WHERE LOWER(email) = LOWER(?) AND LOWER(email) NOT LIKE ? AND id != ?",
    ))
    .bind(&email)
    .bind("%@tokensbyte.local")
    .bind(exclude_id(exclude_user_id))
    .fetch_one(&state.db.pool)
    .await?;
    Ok(Occupancy { bound_count, limit })
}

pub async fn mobile_occupancy(
    state: &Arc<AppState>,
    mobile: &str,
    exclude_user_id: Option<&str>,
    global_default: i64,
) -> AppResult<Occupancy> {
    let mobile = canonical_mobile_lenient(mobile);
    let limit = resolved_mobile_limit(state, &mobile, global_default).await?;
    if !is_real_mobile(&mobile) {
        return Ok(Occupancy {
            bound_count: 0,
            limit,
        });
    }
    let bound_count: i64 = sqlx::query_scalar(&state.db.format_query(
        "SELECT COUNT(*) FROM users WHERE mobile = ? AND COALESCE(mobile, '') != '' AND id != ?",
    ))
    .bind(&mobile)
    .bind(exclude_id(exclude_user_id))
    .fetch_one(&state.db.pool)
    .await?;
    Ok(Occupancy { bound_count, limit })
}

pub async fn wechat_occupancy(
    state: &Arc<AppState>,
    wechat_id: &str,
    exclude_user_id: Option<&str>,
    global_default: i64,
) -> AppResult<Occupancy> {
    let wechat_id = wechat_id.trim();
    let limit = resolved_wechat_limit(state, wechat_id, global_default).await?;
    if !is_real_wechat(wechat_id) {
        return Ok(Occupancy {
            bound_count: 0,
            limit,
        });
    }
    let bound_count: i64 = sqlx::query_scalar(&state.db.format_query(
        "SELECT COUNT(*) FROM users WHERE wechat_id = ? AND COALESCE(wechat_id, '') != '' AND id != ?",
    ))
    .bind(wechat_id)
    .bind(exclude_id(exclude_user_id))
    .fetch_one(&state.db.pool)
    .await?;
    Ok(Occupancy { bound_count, limit })
}

pub fn gate_shared_contact(
    occ: Occupancy,
    confirm_new_account: bool,
    label: &str,
) -> AppResult<()> {
    if occ.bound_count <= 0 {
        return Ok(());
    }
    if occ.at_limit() {
        return Err(occ.at_limit_error(label));
    }
    if !confirm_new_account {
        return Err(occ.in_use_error(label));
    }
    Ok(())
}

async fn lock_contact(
    tx: &mut Transaction<'_, Postgres>,
    db: &Database,
    kind: &str,
    value: &str,
) -> AppResult<()> {
    let norm = canonical_contact_value(kind, value);
    sqlx::query(&db.format_query("SELECT pg_advisory_xact_lock(hashtext(?))"))
        .bind(format!("{kind}:{norm}"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn email_occupancy_tx(
    tx: &mut Transaction<'_, Postgres>,
    db: &Database,
    email: &str,
    exclude_user_id: Option<&str>,
    global_default: i64,
) -> AppResult<Occupancy> {
    let email = canonical_email(email);
    let ov = fetch_override_limit_tx(tx, db, "email", &email).await?;
    let limit = effective_limit(ov, global_default);
    if !is_real_email(&email) {
        return Ok(Occupancy {
            bound_count: 0,
            limit,
        });
    }
    let bound_count: i64 = sqlx::query_scalar(&db.format_query(
        "SELECT COUNT(*) FROM users WHERE LOWER(email) = LOWER(?) AND LOWER(email) NOT LIKE ? AND id != ?",
    ))
    .bind(&email)
    .bind("%@tokensbyte.local")
    .bind(exclude_id(exclude_user_id))
    .fetch_one(&mut **tx)
    .await?;
    Ok(Occupancy { bound_count, limit })
}

async fn mobile_occupancy_tx(
    tx: &mut Transaction<'_, Postgres>,
    db: &Database,
    mobile: &str,
    exclude_user_id: Option<&str>,
    global_default: i64,
) -> AppResult<Occupancy> {
    let mobile = canonical_mobile_lenient(mobile);
    let ov = fetch_override_limit_tx(tx, db, "mobile", &mobile).await?;
    let limit = effective_limit(ov, global_default);
    if !is_real_mobile(&mobile) {
        return Ok(Occupancy {
            bound_count: 0,
            limit,
        });
    }
    let bound_count: i64 = sqlx::query_scalar(&db.format_query(
        "SELECT COUNT(*) FROM users WHERE mobile = ? AND COALESCE(mobile, '') != '' AND id != ?",
    ))
    .bind(&mobile)
    .bind(exclude_id(exclude_user_id))
    .fetch_one(&mut **tx)
    .await?;
    Ok(Occupancy { bound_count, limit })
}

async fn wechat_occupancy_tx(
    tx: &mut Transaction<'_, Postgres>,
    db: &Database,
    wechat_id: &str,
    exclude_user_id: Option<&str>,
    global_default: i64,
) -> AppResult<Occupancy> {
    let wechat_id = wechat_id.trim();
    let ov = fetch_override_limit_tx(tx, db, "wechat", wechat_id).await?;
    let limit = effective_limit(ov, global_default);
    if !is_real_wechat(wechat_id) {
        return Ok(Occupancy {
            bound_count: 0,
            limit,
        });
    }
    let bound_count: i64 = sqlx::query_scalar(&db.format_query(
        "SELECT COUNT(*) FROM users WHERE wechat_id = ? AND COALESCE(wechat_id, '') != '' AND id != ?",
    ))
    .bind(wechat_id)
    .bind(exclude_id(exclude_user_id))
    .fetch_one(&mut **tx)
    .await?;
    Ok(Occupancy { bound_count, limit })
}

pub async fn assert_email_slot_tx(
    tx: &mut Transaction<'_, Postgres>,
    db: &Database,
    email: &str,
    exclude_user_id: Option<&str>,
    limit: i64,
) -> AppResult<()> {
    if !is_real_email(email) {
        return Ok(());
    }
    lock_contact(tx, db, "email", email).await?;
    let occ = email_occupancy_tx(tx, db, email, exclude_user_id, limit).await?;
    if occ.at_limit() {
        return Err(occ.at_limit_error("邮箱"));
    }
    Ok(())
}

pub async fn assert_mobile_slot_tx(
    tx: &mut Transaction<'_, Postgres>,
    db: &Database,
    mobile: &str,
    exclude_user_id: Option<&str>,
    limit: i64,
) -> AppResult<()> {
    if !is_real_mobile(mobile) {
        return Ok(());
    }
    lock_contact(tx, db, "mobile", mobile).await?;
    let occ = mobile_occupancy_tx(tx, db, mobile, exclude_user_id, limit).await?;
    if occ.at_limit() {
        return Err(occ.at_limit_error("手机号"));
    }
    Ok(())
}

pub async fn assert_wechat_slot_tx(
    tx: &mut Transaction<'_, Postgres>,
    db: &Database,
    wechat_id: &str,
    exclude_user_id: Option<&str>,
    limit: i64,
) -> AppResult<()> {
    if !is_real_wechat(wechat_id) {
        return Ok(());
    }
    lock_contact(tx, db, "wechat", wechat_id).await?;
    let occ = wechat_occupancy_tx(tx, db, wechat_id, exclude_user_id, limit).await?;
    if occ.at_limit() {
        return Err(occ.at_limit_error("微信"));
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn assert_email_slot(
    state: &Arc<AppState>,
    email: &str,
    exclude_user_id: Option<&str>,
    limit: i64,
) -> AppResult<()> {
    let occ = email_occupancy(state, email, exclude_user_id, limit).await?;
    if occ.at_limit() {
        return Err(occ.at_limit_error("邮箱"));
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn assert_mobile_slot(
    state: &Arc<AppState>,
    mobile: &str,
    exclude_user_id: Option<&str>,
    limit: i64,
) -> AppResult<()> {
    let occ = mobile_occupancy(state, mobile, exclude_user_id, limit).await?;
    if occ.at_limit() {
        return Err(occ.at_limit_error("手机号"));
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn assert_wechat_slot(
    state: &Arc<AppState>,
    wechat_id: &str,
    exclude_user_id: Option<&str>,
    limit: i64,
) -> AppResult<()> {
    let occ = wechat_occupancy(state, wechat_id, exclude_user_id, limit).await?;
    if occ.at_limit() {
        return Err(occ.at_limit_error("微信"));
    }
    Ok(())
}

pub async fn list_email_candidates(
    state: &Arc<AppState>,
    email: &str,
) -> AppResult<Vec<LoginCandidate>> {
    let email = canonical_email(email);
    if !is_real_email(&email) {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(&state.db.format_query(
        "SELECT uid, username, nickname FROM users WHERE LOWER(email) = LOWER(?) AND LOWER(email) NOT LIKE ? ORDER BY created_at ASC, uid ASC",
    ))
    .bind(&email)
    .bind("%@tokensbyte.local")
    .fetch_all(&state.db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(uid, username, nickname)| LoginCandidate {
            uid,
            username,
            nickname,
        })
        .collect())
}

pub async fn list_mobile_candidates(
    state: &Arc<AppState>,
    mobile: &str,
) -> AppResult<Vec<LoginCandidate>> {
    let mobile = canonical_mobile_lenient(mobile);
    if !is_real_mobile(&mobile) {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(&state.db.format_query(
        "SELECT uid, username, nickname FROM users WHERE mobile = ? AND COALESCE(mobile, '') != '' ORDER BY created_at ASC, uid ASC",
    ))
    .bind(&mobile)
    .fetch_all(&state.db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(uid, username, nickname)| LoginCandidate {
            uid,
            username,
            nickname,
        })
        .collect())
}

pub fn account_select_error(candidates: Vec<LoginCandidate>, label: &str) -> AppError {
    AppError::ConflictDetails(
        format!("该{}绑定了多个账号，请选择要操作的 UID", label),
        "ACCOUNT_SELECT_REQUIRED",
        json!({ "candidates": candidates }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PasswordMatchKind {
        None,
        Single,
        Multiple,
    }

    fn classify_password_matches(
        match_count: usize,
        selected_uid: Option<&str>,
    ) -> PasswordMatchKind {
        if selected_uid.is_some() {
            if match_count == 1 {
                PasswordMatchKind::Single
            } else {
                PasswordMatchKind::None
            }
        } else {
            match match_count {
                0 => PasswordMatchKind::None,
                1 => PasswordMatchKind::Single,
                _ => PasswordMatchKind::Multiple,
            }
        }
    }

    #[test]
    fn default_limit_when_invalid() {
        assert_eq!(normalize_limit(0), DEFAULT_ACCOUNTS_PER_CONTACT);
        assert_eq!(normalize_limit(-1), DEFAULT_ACCOUNTS_PER_CONTACT);
        assert_eq!(normalize_limit(100), DEFAULT_ACCOUNTS_PER_CONTACT);
        assert_eq!(normalize_limit(5), 5);
        assert_eq!(normalize_limit(1), 1);
        assert_eq!(normalize_limit(99), 99);
    }

    #[test]
    fn occupancy_allows_existing_holder() {
        assert!(!would_exceed_limit(5, 5, true));
        assert!(would_exceed_limit(5, 5, false));
        assert!(!would_exceed_limit(4, 5, false));
    }

    #[test]
    fn login_selects_when_multiple_and_no_uid() {
        assert_eq!(
            classify_password_matches(2, None),
            PasswordMatchKind::Multiple
        );
        assert_eq!(
            classify_password_matches(1, None),
            PasswordMatchKind::Single
        );
        assert_eq!(classify_password_matches(0, None), PasswordMatchKind::None);
        assert_eq!(
            classify_password_matches(1, Some("10001")),
            PasswordMatchKind::Single
        );
        assert_eq!(
            classify_password_matches(0, Some("10001")),
            PasswordMatchKind::None
        );
    }

    #[test]
    fn placeholder_email_is_not_real() {
        assert!(!is_real_email("u_123@tokensbyte.local"));
        assert!(!is_real_email(""));
        assert!(is_real_email("user@example.com"));
    }

    #[test]
    fn override_limit_wins_until_invalid() {
        assert_eq!(effective_limit(Some(8), 5), 8);
        assert_eq!(effective_limit(None, 5), 5);
        assert_eq!(effective_limit(Some(0), 5), DEFAULT_ACCOUNTS_PER_CONTACT);
        assert_eq!(effective_limit(Some(100), 5), DEFAULT_ACCOUNTS_PER_CONTACT);
    }

    #[test]
    fn parse_contact_kind_rejects_unknown() {
        assert_eq!(parse_contact_kind("email").unwrap(), "email");
        assert_eq!(parse_contact_kind("mobile").unwrap(), "mobile");
        assert_eq!(parse_contact_kind("wechat").unwrap(), "wechat");
        assert!(parse_contact_kind("google").is_err());
    }

    #[test]
    fn mainland_mobile_variants_collapse_to_eleven_digits() {
        for raw in [
            "13800138000",
            " 138 0013 8000 ",
            "138-0013-8000",
            "+8613800138000",
            "+86 138 0013 8000",
            "008613800138000",
            "8613800138000",
        ] {
            assert_eq!(
                normalize_mobile(raw, false).unwrap(),
                "13800138000",
                "{raw}"
            );
        }
    }

    #[test]
    fn invalid_or_international_mobile_rejected_unless_allowed() {
        assert!(normalize_mobile("12800138000", false).is_err());
        assert!(normalize_mobile("1380013800", false).is_err());
        assert!(normalize_mobile("abc", false).is_err());
        assert!(normalize_mobile("+14155552671", false).is_err());
        assert_eq!(
            normalize_mobile("+1 415-555-2671", true).unwrap(),
            "+14155552671"
        );
        assert!(normalize_mobile("+0123456789", true).is_err());
        assert!(normalize_mobile("+86123", true).is_err());
        assert!(normalize_mobile("4155552671", true).is_err());
    }

    #[test]
    fn lenient_mobile_keeps_unknown_formats() {
        assert_eq!(canonical_mobile_lenient("+86 13800138000"), "13800138000");
        assert_eq!(canonical_mobile_lenient("+14155552671"), "+14155552671");
    }

    #[test]
    fn canonical_contact_value_normalizes_by_kind() {
        assert_eq!(
            canonical_contact_value("email", "  User@Example.COM "),
            "user@example.com"
        );
        assert_eq!(
            canonical_contact_value("mobile", "+86 138-0013-8000"),
            "13800138000"
        );
        assert_eq!(
            canonical_contact_value("wechat", "  oWx_AbCd123 "),
            "oWx_AbCd123"
        );
    }
}
