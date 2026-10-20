/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::api::settings::default_registration_settings;
use crate::auth;
use crate::error::{AppError, AppResult};
use crate::models::{
    normalize_company_doc_type, normalize_id_doc_type, normalize_kyc_status, normalize_kyc_type,
    normalize_validity_type, RegistrationSettings, UpsertUserKycRequest, UserKyc,
};
use crate::time_system::DbTs;
use crate::AppState;
use axum::{
    extract::{Extension, Multipart, Path, State},
    Json,
};
use std::sync::Arc;

async fn load_registration(state: &AppState) -> AppResult<RegistrationSettings> {
    let val: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = ?"),
    )
    .bind("registration_settings")
    .fetch_optional(&state.db.pool)
    .await?;
    Ok(val
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_else(default_registration_settings))
}

async fn fetch_user_kyc_list(state: &AppState, user_id: &str) -> AppResult<Vec<UserKyc>> {
    let rows: Vec<UserKyc> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM user_kyc WHERE user_id = ? ORDER BY id ASC"),
    )
    .bind(user_id)
    .fetch_all(&state.db.pool)
    .await?;
    Ok(rows)
}

async fn fetch_kyc_by_id(
    state: &AppState,
    user_id: &str,
    kyc_id: i64,
) -> AppResult<Option<UserKyc>> {
    let row: Option<UserKyc> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM user_kyc WHERE id = ? AND user_id = ?"),
    )
    .bind(kyc_id)
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;
    Ok(row)
}

async fn fetch_kyc(state: &AppState, user_id: &str) -> AppResult<UserKyc> {
    let row: Option<UserKyc> = sqlx::query_as(&state.db.format_query(
        "SELECT * FROM user_kyc WHERE user_id = ? ORDER BY is_default DESC, id DESC LIMIT 1",
    ))
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;
    Ok(row.unwrap_or_else(|| UserKyc::empty_for(user_id)))
}

fn empty_to_none(s: String) -> Option<String> {
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn parse_expire_at(raw: Option<&String>, validity_type: &str) -> AppResult<Option<DbTs>> {
    if validity_type != "expire_date" {
        return Ok(None);
    }
    let Some(s) = raw.map(|v| v.trim()).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    // 允许日期或完整时间戳
    let normalized = if s.len() == 10 {
        format!("{s}T23:59:59.000Z")
    } else {
        s.to_string()
    };
    let ts = DbTs::new(normalized);
    if ts.to_utc().is_none() {
        return Err(AppError::BadRequest("无效的证件到期日期".to_string()));
    }
    Ok(Some(ts))
}

fn validate_payload(
    kyc_type: &str,
    id_doc_type: Option<&str>,
    id_doc_number: Option<&str>,
    real_name: Option<&str>,
    personal_email: Option<&str>,
    _personal_phone: Option<&str>,
    company_name: Option<&str>,
    company_doc_type: Option<&str>,
    company_doc_number: Option<&str>,
    company_email: Option<&str>,
    _company_phone: Option<&str>,
    for_submit: bool,
) -> AppResult<()> {
    if !for_submit {
        match kyc_type {
            "personal" => {
                if let Some(email) = personal_email.map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    if !email.contains('@') {
                        return Err(AppError::BadRequest("联系邮箱格式不正确".to_string()));
                    }
                }
            }
            "enterprise" => {
                if let Some(email) = company_email.map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    if !email.contains('@') {
                        return Err(AppError::BadRequest("企业联系邮箱格式不正确".to_string()));
                    }
                }
            }
            _ => return Err(AppError::BadRequest("无效的实名类型".to_string())),
        }
        return Ok(());
    }
    match kyc_type {
        "personal" => {
            if real_name.map(|s| s.trim().is_empty()).unwrap_or(true) {
                return Err(AppError::BadRequest("请填写真实姓名".to_string()));
            }
            if id_doc_type.map(|s| s.trim().is_empty()).unwrap_or(true) {
                return Err(AppError::BadRequest("请选择证件类型".to_string()));
            }
            if id_doc_number.map(|s| s.trim().is_empty()).unwrap_or(true) {
                return Err(AppError::BadRequest("请填写证件号码".to_string()));
            }
            // 个人用户：联系邮箱必填，并校验格式
            let email = personal_email.map(|s| s.trim()).unwrap_or("");
            if email.is_empty() {
                return Err(AppError::BadRequest("请填写联系邮箱".to_string()));
            }
            if !email.contains('@') {
                return Err(AppError::BadRequest("联系邮箱格式不正确".to_string()));
            }
            // 个人用户：联系电话选填
        }
        "enterprise" => {
            if company_name.map(|s| s.trim().is_empty()).unwrap_or(true) {
                return Err(AppError::BadRequest("请填写企业名称".to_string()));
            }
            if company_doc_type
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(AppError::BadRequest("请选择企业证件类型".to_string()));
            }
            if company_doc_number
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(AppError::BadRequest("请填写企业证件号码".to_string()));
            }
            // 企业用户：企业联系邮箱必填，并校验格式
            let email = company_email.map(|s| s.trim()).unwrap_or("");
            if email.is_empty() {
                return Err(AppError::BadRequest("请填写企业联系邮箱".to_string()));
            }
            if !email.contains('@') {
                return Err(AppError::BadRequest("企业联系邮箱格式不正确".to_string()));
            }
            // 企业用户：企业联系电话选填
        }
        _ => return Err(AppError::BadRequest("无效的实名类型".to_string())),
    }
    Ok(())
}

async fn resolve_user_id(state: &AppState, user_id_or_uid: &str) -> AppResult<String> {
    let id: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM users WHERE id = ? OR uid = ? LIMIT 1"),
    )
    .bind(user_id_or_uid)
    .bind(user_id_or_uid)
    .fetch_optional(&state.db.pool)
    .await?;
    id.ok_or_else(|| AppError::NotFound("用户不存在".to_string()))
}

async fn insert_kyc_record(
    state: &AppState,
    user_id: &str,
    req: UpsertUserKycRequest,
    is_admin: bool,
    operator: Option<&str>,
    kyc_enabled: bool,
) -> AppResult<UserKyc> {
    let kyc_type = req
        .kyc_type
        .as_deref()
        .and_then(normalize_kyc_type)
        .unwrap_or("personal")
        .to_string();

    let validity_type = req
        .validity_type
        .as_deref()
        .and_then(normalize_validity_type)
        .unwrap_or("long_term")
        .to_string();

    let resolved_p_email = req.resolved_personal_email();
    let resolved_p_phone = req.resolved_personal_phone();
    let resolved_c_email = req.resolved_company_email();
    let resolved_c_phone = req.resolved_company_phone();

    let (
        real_name,
        id_doc_type,
        id_doc_number,
        personal_email,
        personal_phone,
        id_doc_front_url,
        id_doc_back_url,
        company_name,
        company_doc_type,
        company_doc_number,
        company_email,
        company_phone,
        business_license_url,
        tax_registration_url,
        legal_notarization_url,
    ) = (
        req.real_name.and_then(empty_to_none),
        req.id_doc_type
            .as_deref()
            .and_then(normalize_id_doc_type)
            .map(|s| s.to_string()),
        req.id_doc_number.and_then(empty_to_none),
        resolved_p_email.and_then(empty_to_none),
        resolved_p_phone.and_then(empty_to_none),
        req.id_doc_front_url.and_then(empty_to_none),
        req.id_doc_back_url.and_then(empty_to_none),
        req.company_name.and_then(empty_to_none),
        req.company_doc_type
            .as_deref()
            .and_then(normalize_company_doc_type)
            .map(|s| s.to_string()),
        req.company_doc_number.and_then(empty_to_none),
        resolved_c_email.and_then(empty_to_none),
        resolved_c_phone.and_then(empty_to_none),
        req.business_license_url.and_then(empty_to_none),
        req.tax_registration_url.and_then(empty_to_none),
        req.legal_notarization_url.and_then(empty_to_none),
    );

    let expire_at = parse_expire_at(req.expire_at.as_ref(), &validity_type)?;

    let (status, reject_reason, admin_remark, reviewed_by, reviewed_at, submitted_at) = if is_admin
    {
        let status = match req.status.as_deref() {
            Some(v) => normalize_kyc_status(v)
                .ok_or_else(|| AppError::BadRequest("实名状态无效".to_string()))?
                .to_string(),
            None => "approved".to_string(),
        };
        let reject_reason = req.reject_reason.and_then(empty_to_none);
        let admin_remark = req.admin_remark.and_then(empty_to_none);
        let (reviewed_by, reviewed_at) = if matches!(status.as_str(), "approved" | "rejected") {
            (operator.map(|s| s.to_string()), Some(DbTs::now()))
        } else {
            (None, None)
        };
        let submitted_at = if status != "none" {
            Some(DbTs::now())
        } else {
            None
        };
        (
            status,
            reject_reason,
            admin_remark,
            reviewed_by,
            reviewed_at,
            submitted_at,
        )
    } else {
        validate_payload(
            &kyc_type,
            id_doc_type.as_deref(),
            id_doc_number.as_deref(),
            real_name.as_deref(),
            personal_email.as_deref(),
            personal_phone.as_deref(),
            company_name.as_deref(),
            company_doc_type.as_deref(),
            company_doc_number.as_deref(),
            company_email.as_deref(),
            company_phone.as_deref(),
            kyc_enabled,
        )?;
        let (status, submitted_at) = if kyc_enabled {
            ("pending".to_string(), Some(DbTs::now()))
        } else {
            ("none".to_string(), None)
        };
        (status, None, None, None, None, submitted_at)
    };

    if is_admin && matches!(status.as_str(), "approved" | "pending") {
        validate_payload(
            &kyc_type,
            id_doc_type.as_deref(),
            id_doc_number.as_deref(),
            real_name.as_deref(),
            personal_email.as_deref(),
            personal_phone.as_deref(),
            company_name.as_deref(),
            company_doc_type.as_deref(),
            company_doc_number.as_deref(),
            company_email.as_deref(),
            company_phone.as_deref(),
            true,
        )?;
    }

    let has_default: bool = sqlx::query_scalar(&state.db.format_query(
        "SELECT EXISTS(SELECT 1 FROM user_kyc WHERE user_id = ? AND is_default = TRUE)",
    ))
    .bind(user_id)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(false);

    let is_default = req.is_default.unwrap_or(!has_default);
    if is_default {
        let _ = sqlx::query(
            &state
                .db
                .format_query("UPDATE user_kyc SET is_default = FALSE WHERE user_id = ?"),
        )
        .bind(user_id)
        .execute(&state.db.pool)
        .await;
    }

    let inserted_id: i64 = sqlx::query_scalar(
        &state.db.format_query(
            r#"INSERT INTO user_kyc (
                user_id, kyc_type, status, real_name, id_doc_type, id_doc_number, personal_email, personal_phone,
                id_doc_front_url, id_doc_back_url,
                company_name, company_doc_type, company_doc_number, company_email, company_phone,
                business_license_url, tax_registration_url, legal_notarization_url,
                validity_type, expire_at, reject_reason, admin_remark, is_default, reviewed_by, reviewed_at,
                submitted_at, created_at, updated_at
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?,
                ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?,
                ?, ?, ?, ?, ?, ?, ?,
                ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
            ) RETURNING id"#,
        ),
    )
    .bind(user_id)
    .bind(&kyc_type)
    .bind(&status)
    .bind(&real_name)
    .bind(&id_doc_type)
    .bind(&id_doc_number)
    .bind(&personal_email)
    .bind(&personal_phone)
    .bind(&id_doc_front_url)
    .bind(&id_doc_back_url)
    .bind(&company_name)
    .bind(&company_doc_type)
    .bind(&company_doc_number)
    .bind(&company_email)
    .bind(&company_phone)
    .bind(&business_license_url)
    .bind(&tax_registration_url)
    .bind(&legal_notarization_url)
    .bind(&validity_type)
    .bind(&expire_at)
    .bind(&reject_reason)
    .bind(&admin_remark)
    .bind(is_default)
    .bind(&reviewed_by)
    .bind(&reviewed_at)
    .bind(&submitted_at)
    .fetch_one(&state.db.pool)
    .await?;

    fetch_kyc_by_id(state, user_id, inserted_id)
        .await?
        .ok_or_else(|| AppError::Internal("创建实名记录失败".to_string()))
}

async fn update_kyc_record(
    state: &AppState,
    user_id: &str,
    kyc_id: i64,
    req: UpsertUserKycRequest,
    is_admin: bool,
    operator: Option<&str>,
    kyc_enabled: bool,
) -> AppResult<UserKyc> {
    let existing = fetch_kyc_by_id(state, user_id, kyc_id)
        .await?
        .ok_or_else(|| AppError::NotFound("实名记录不存在".to_string()))?;

    let kyc_type = match req.kyc_type.as_deref() {
        Some(v) => normalize_kyc_type(v)
            .ok_or_else(|| AppError::BadRequest("实名类型仅支持个人或企业".to_string()))?
            .to_string(),
        None => existing.kyc_type.clone(),
    };

    let validity_type = match req.validity_type.as_deref() {
        Some(v) => normalize_validity_type(v)
            .ok_or_else(|| AppError::BadRequest("有效期类型无效".to_string()))?
            .to_string(),
        None => existing.validity_type.clone(),
    };

    let resolved_p_email = req.resolved_personal_email();
    let resolved_p_phone = req.resolved_personal_phone();
    let resolved_c_email = req.resolved_company_email();
    let resolved_c_phone = req.resolved_company_phone();

    let (
        real_name,
        id_doc_type,
        id_doc_number,
        personal_email,
        personal_phone,
        id_doc_front_url,
        id_doc_back_url,
        company_name,
        company_doc_type,
        company_doc_number,
        company_email,
        company_phone,
        business_license_url,
        tax_registration_url,
        legal_notarization_url,
    ) = (
        req.real_name.and_then(empty_to_none).or(existing.real_name),
        req.id_doc_type
            .as_deref()
            .and_then(|v| {
                if v.is_empty() {
                    None
                } else {
                    normalize_id_doc_type(v).map(|s| s.to_string())
                }
            })
            .or(existing.id_doc_type),
        req.id_doc_number
            .and_then(empty_to_none)
            .or(existing.id_doc_number),
        resolved_p_email
            .and_then(empty_to_none)
            .or(existing.personal_email),
        resolved_p_phone
            .and_then(empty_to_none)
            .or(existing.personal_phone),
        req.id_doc_front_url
            .and_then(empty_to_none)
            .or(existing.id_doc_front_url),
        req.id_doc_back_url
            .and_then(empty_to_none)
            .or(existing.id_doc_back_url),
        req.company_name
            .and_then(empty_to_none)
            .or(existing.company_name),
        req.company_doc_type
            .as_deref()
            .and_then(|v| {
                if v.is_empty() {
                    None
                } else {
                    normalize_company_doc_type(v).map(|s| s.to_string())
                }
            })
            .or(existing.company_doc_type),
        req.company_doc_number
            .and_then(empty_to_none)
            .or(existing.company_doc_number),
        resolved_c_email
            .and_then(empty_to_none)
            .or(existing.company_email),
        resolved_c_phone
            .and_then(empty_to_none)
            .or(existing.company_phone),
        req.business_license_url
            .and_then(empty_to_none)
            .or(existing.business_license_url),
        req.tax_registration_url
            .and_then(empty_to_none)
            .or(existing.tax_registration_url),
        req.legal_notarization_url
            .and_then(empty_to_none)
            .or(existing.legal_notarization_url),
    );

    let expire_at = if req.validity_type.is_some() || req.expire_at.is_some() {
        parse_expire_at(req.expire_at.as_ref(), &validity_type)?
    } else if validity_type == "long_term" {
        None
    } else {
        existing.expire_at.clone()
    };

    let (status, reject_reason, admin_remark, reviewed_by, reviewed_at, submitted_at) = if is_admin
    {
        let status = match req.status.as_deref() {
            Some(v) => normalize_kyc_status(v)
                .ok_or_else(|| AppError::BadRequest("实名状态无效".to_string()))?
                .to_string(),
            None => {
                if existing.status == "none" {
                    "approved".to_string()
                } else {
                    existing.status.clone()
                }
            }
        };
        let reject_reason = match req.reject_reason {
            Some(s) => empty_to_none(s),
            None => existing.reject_reason.clone(),
        };
        let admin_remark = match req.admin_remark {
            Some(s) => empty_to_none(s),
            None => existing.admin_remark.clone(),
        };
        let (reviewed_by, reviewed_at) = if matches!(status.as_str(), "approved" | "rejected") {
            (
                operator
                    .map(|s| s.to_string())
                    .or(existing.reviewed_by.clone()),
                Some(DbTs::now()),
            )
        } else {
            (existing.reviewed_by.clone(), existing.reviewed_at.clone())
        };
        let submitted_at = if status != "none" {
            existing.submitted_at.clone().or_else(|| Some(DbTs::now()))
        } else {
            existing.submitted_at.clone()
        };
        (
            status,
            reject_reason,
            admin_remark,
            reviewed_by,
            reviewed_at,
            submitted_at,
        )
    } else {
        validate_payload(
            &kyc_type,
            id_doc_type.as_deref(),
            id_doc_number.as_deref(),
            real_name.as_deref(),
            personal_email.as_deref(),
            personal_phone.as_deref(),
            company_name.as_deref(),
            company_doc_type.as_deref(),
            company_doc_number.as_deref(),
            company_email.as_deref(),
            company_phone.as_deref(),
            kyc_enabled,
        )?;
        let (status, submitted_at) = if kyc_enabled {
            ("pending".to_string(), Some(DbTs::now()))
        } else {
            ("none".to_string(), None)
        };
        (
            status,
            None,
            existing.admin_remark.clone(),
            None,
            None,
            submitted_at,
        )
    };

    if is_admin && matches!(status.as_str(), "approved" | "pending") {
        validate_payload(
            &kyc_type,
            id_doc_type.as_deref(),
            id_doc_number.as_deref(),
            real_name.as_deref(),
            personal_email.as_deref(),
            personal_phone.as_deref(),
            company_name.as_deref(),
            company_doc_type.as_deref(),
            company_doc_number.as_deref(),
            company_email.as_deref(),
            company_phone.as_deref(),
            true,
        )?;
    }

    let is_default = match req.is_default {
        Some(true) => {
            let _ = sqlx::query(
                &state
                    .db
                    .format_query("UPDATE user_kyc SET is_default = FALSE WHERE user_id = ?"),
            )
            .bind(user_id)
            .execute(&state.db.pool)
            .await;
            true
        }
        Some(false) => false,
        None => existing.is_default,
    };

    sqlx::query(&state.db.format_query(
        r#"UPDATE user_kyc SET
                kyc_type = ?,
                status = ?,
                real_name = ?,
                id_doc_type = ?,
                id_doc_number = ?,
                personal_email = ?,
                personal_phone = ?,
                id_doc_front_url = ?,
                id_doc_back_url = ?,
                company_name = ?,
                company_doc_type = ?,
                company_doc_number = ?,
                company_email = ?,
                company_phone = ?,
                business_license_url = ?,
                tax_registration_url = ?,
                legal_notarization_url = ?,
                validity_type = ?,
                expire_at = ?,
                reject_reason = ?,
                admin_remark = ?,
                is_default = ?,
                reviewed_by = ?,
                reviewed_at = ?,
                submitted_at = ?,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ? AND user_id = ?"#,
    ))
    .bind(&kyc_type)
    .bind(&status)
    .bind(&real_name)
    .bind(&id_doc_type)
    .bind(&id_doc_number)
    .bind(&personal_email)
    .bind(&personal_phone)
    .bind(&id_doc_front_url)
    .bind(&id_doc_back_url)
    .bind(&company_name)
    .bind(&company_doc_type)
    .bind(&company_doc_number)
    .bind(&company_email)
    .bind(&company_phone)
    .bind(&business_license_url)
    .bind(&tax_registration_url)
    .bind(&legal_notarization_url)
    .bind(&validity_type)
    .bind(&expire_at)
    .bind(&reject_reason)
    .bind(&admin_remark)
    .bind(is_default)
    .bind(&reviewed_by)
    .bind(&reviewed_at)
    .bind(&submitted_at)
    .bind(kyc_id)
    .bind(user_id)
    .execute(&state.db.pool)
    .await?;

    fetch_kyc_by_id(state, user_id, kyc_id)
        .await?
        .ok_or_else(|| AppError::Internal("获取更新后实名记录失败".to_string()))
}

async fn delete_kyc_record(state: &AppState, user_id: &str, kyc_id: i64) -> AppResult<()> {
    let existing = fetch_kyc_by_id(state, user_id, kyc_id).await?;
    let was_default = existing.map(|e| e.is_default).unwrap_or(false);

    let result = sqlx::query(
        &state
            .db
            .format_query("DELETE FROM user_kyc WHERE id = ? AND user_id = ?"),
    )
    .bind(kyc_id)
    .bind(user_id)
    .execute(&state.db.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("实名记录不存在或已被删除".to_string()));
    }

    if was_default {
        let _ = sqlx::query(
            &state.db.format_query(
                "UPDATE user_kyc SET is_default = TRUE WHERE id = (SELECT id FROM user_kyc WHERE user_id = ? ORDER BY id DESC LIMIT 1)"
            ),
        )
        .bind(user_id)
        .execute(&state.db.pool)
        .await;
    }

    Ok(())
}

pub async fn set_kyc_default_record(
    state: &AppState,
    user_id: &str,
    kyc_id: i64,
) -> AppResult<UserKyc> {
    let _existing = fetch_kyc_by_id(state, user_id, kyc_id)
        .await?
        .ok_or_else(|| AppError::NotFound("实名记录不存在".to_string()))?;

    let _ = sqlx::query(
        &state
            .db
            .format_query("UPDATE user_kyc SET is_default = FALSE WHERE user_id = ?"),
    )
    .bind(user_id)
    .execute(&state.db.pool)
    .await;

    sqlx::query(
        &state
            .db
            .format_query("UPDATE user_kyc SET is_default = TRUE WHERE id = ? AND user_id = ?"),
    )
    .bind(kyc_id)
    .bind(user_id)
    .execute(&state.db.pool)
    .await?;

    fetch_kyc_by_id(state, user_id, kyc_id)
        .await?
        .ok_or_else(|| AppError::Internal("获取实名记录失败".to_string()))
}

#[derive(serde::Deserialize)]
pub struct GetMyKycQuery {
    pub kyc_type: Option<String>,
}

/// 用户获取自己的实名/详细信息（可根据 kyc_type 获取指定主体，未传则返回默认/最近主体）
pub async fn get_my_kyc(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    axum::extract::Query(query): axum::extract::Query<GetMyKycQuery>,
) -> AppResult<Json<UserKyc>> {
    if let Some(kt) = query.kyc_type.as_deref().and_then(normalize_kyc_type) {
        let row: Option<UserKyc> = sqlx::query_as(
            &state.db.format_query(
                "SELECT * FROM user_kyc WHERE user_id = ? AND kyc_type = ? ORDER BY is_default DESC, id DESC LIMIT 1",
            ),
        )
        .bind(&claims.sub)
        .bind(kt)
        .fetch_optional(&state.db.pool)
        .await?;
        Ok(Json(row.unwrap_or_else(|| {
            let mut empty = UserKyc::empty_for(&claims.sub);
            empty.kyc_type = kt.to_string();
            empty
        })))
    } else {
        Ok(Json(fetch_kyc(&state, &claims.sub).await?))
    }
}

/// 用户获取自己的所有实名记录列表
pub async fn get_my_kyc_list(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<Vec<UserKyc>>> {
    Ok(Json(fetch_user_kyc_list(&state, &claims.sub).await?))
}

/// 用户将自己的某条实名记录设为默认
pub async fn user_set_default_kyc(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(kyc_id): Path<i64>,
) -> AppResult<Json<UserKyc>> {
    Ok(Json(
        set_kyc_default_record(&state, &claims.sub, kyc_id).await?,
    ))
}

/// 用户提交实名/详细信息（个人实名与企业实名独立保存，互不覆盖，对应管理后台 2 条记录）
pub async fn submit_my_kyc(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Json(mut req): Json<UpsertUserKycRequest>,
) -> AppResult<Json<UserKyc>> {
    let reg = load_registration(&state).await?;
    let kyc_enabled = reg.enable_user_kyc;

    let target_kyc_type = req
        .kyc_type
        .as_deref()
        .and_then(normalize_kyc_type)
        .unwrap_or("personal")
        .to_string();

    req.kyc_type = Some(target_kyc_type.clone());

    // 精确查询该用户同类型的现有实名记录（个人对应个人，企业对应企业，互不覆盖）
    let existing_row: Option<UserKyc> = sqlx::query_as(
        &state.db.format_query(
            "SELECT * FROM user_kyc WHERE user_id = ? AND kyc_type = ? ORDER BY is_default DESC, id DESC LIMIT 1",
        ),
    )
    .bind(&claims.sub)
    .bind(&target_kyc_type)
    .fetch_optional(&state.db.pool)
    .await?;

    // 已通过实名的用户允许重新提交变更资料，提交后 update_kyc_record 会将其状态自动重置为 pending 待重新审核

    // 检查用户是否已有其他默认主体
    let has_other_default: bool = sqlx::query_scalar(
        &state.db.format_query(
            "SELECT EXISTS(SELECT 1 FROM user_kyc WHERE user_id = ? AND is_default = TRUE AND kyc_type != ?)",
        ),
    )
    .bind(&claims.sub)
    .bind(&target_kyc_type)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(false);

    // 如果用户显式传递了 is_default 则优先遵循；
    // 如果没有其他默认主体，则强制将本条设为默认；
    // 否则保持本记录现有的 is_default 或默认为 true
    let should_be_default = match req.is_default {
        Some(explicit) => explicit || !has_other_default,
        None => existing_row.as_ref().map(|e| e.is_default).unwrap_or(true),
    };
    req.is_default = Some(should_be_default);

    let result = if let Some(existing) = existing_row {
        update_kyc_record(
            &state,
            &claims.sub,
            existing.id,
            req,
            false,
            None,
            kyc_enabled,
        )
        .await?
    } else {
        insert_kyc_record(&state, &claims.sub, req, false, None, kyc_enabled).await?
    };

    if should_be_default {
        let _ =
            sqlx::query(&state.db.format_query(
                "UPDATE user_kyc SET is_default = FALSE WHERE user_id = ? AND id != ?",
            ))
            .bind(&claims.sub)
            .bind(result.id)
            .execute(&state.db.pool)
            .await;

        let _ =
            sqlx::query(&state.db.format_query(
                "UPDATE user_kyc SET is_default = TRUE WHERE user_id = ? AND id = ?",
            ))
            .bind(&claims.sub)
            .bind(result.id)
            .execute(&state.db.pool)
            .await;
    }

    fetch_kyc_by_id(&state, &claims.sub, result.id)
        .await?
        .ok_or_else(|| AppError::Internal("获取实名记录失败".to_string()))
        .map(Json)
}

/// 管理员获取指定用户的所有实名信息列表（不受站点开关限制，便于后台录入与管理）
pub async fn admin_get_user_kyc(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> AppResult<Json<Vec<UserKyc>>> {
    let user_id = resolve_user_id(&state, &user_id).await?;
    Ok(Json(fetch_user_kyc_list(&state, &user_id).await?))
}

/// 管理员为指定用户新增实名记录
pub async fn admin_create_user_kyc(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Extension(ctx): Extension<crate::admin_permission::AdminContext>,
    Path(user_id): Path<String>,
    Json(req): Json<UpsertUserKycRequest>,
) -> AppResult<Json<UserKyc>> {
    crate::admin_permission::require_edit(&ctx, "users.list")?;
    let user_id = resolve_user_id(&state, &user_id).await?;
    Ok(Json(
        insert_kyc_record(&state, &user_id, req, true, Some(&claims.username), true).await?,
    ))
}

/// 管理员修改/审核指定的单条实名记录
pub async fn admin_update_user_kyc_item(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Extension(ctx): Extension<crate::admin_permission::AdminContext>,
    Path((user_id, kyc_id)): Path<(String, i64)>,
    Json(req): Json<UpsertUserKycRequest>,
) -> AppResult<Json<UserKyc>> {
    crate::admin_permission::require_edit(&ctx, "users.list")?;
    let user_id = resolve_user_id(&state, &user_id).await?;
    Ok(Json(
        update_kyc_record(
            &state,
            &user_id,
            kyc_id,
            req,
            true,
            Some(&claims.username),
            true,
        )
        .await?,
    ))
}

/// 管理员删除指定的单条实名记录
pub async fn admin_delete_user_kyc_item(
    State(state): State<Arc<AppState>>,
    Extension(_claims): Extension<auth::Claims>,
    Extension(ctx): Extension<crate::admin_permission::AdminContext>,
    Path((user_id, kyc_id)): Path<(String, i64)>,
) -> AppResult<Json<serde_json::Value>> {
    crate::admin_permission::require_edit(&ctx, "users.list")?;
    let user_id = resolve_user_id(&state, &user_id).await?;
    delete_kyc_record(&state, &user_id, kyc_id).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "message": "实名记录已删除"
    })))
}

/// 管理员将用户的某条实名记录设为默认
pub async fn admin_set_default_user_kyc(
    State(state): State<Arc<AppState>>,
    Extension(_claims): Extension<auth::Claims>,
    Extension(ctx): Extension<crate::admin_permission::AdminContext>,
    Path((user_id, kyc_id)): Path<(String, i64)>,
) -> AppResult<Json<UserKyc>> {
    crate::admin_permission::require_edit(&ctx, "users.list")?;
    let user_id = resolve_user_id(&state, &user_id).await?;
    Ok(Json(
        set_kyc_default_record(&state, &user_id, kyc_id).await?,
    ))
}

/// 管理员保存/审核用户实名（兼容旧接口：传 id 则更新，未传按类型更新或新增）
pub async fn admin_upsert_user_kyc(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Extension(ctx): Extension<crate::admin_permission::AdminContext>,
    Path(user_id): Path<String>,
    Json(req): Json<UpsertUserKycRequest>,
) -> AppResult<Json<UserKyc>> {
    crate::admin_permission::require_edit(&ctx, "users.list")?;
    let user_id = resolve_user_id(&state, &user_id).await?;

    if let Some(kyc_id) = req.id {
        if kyc_id > 0 {
            return Ok(Json(
                update_kyc_record(
                    &state,
                    &user_id,
                    kyc_id,
                    req,
                    true,
                    Some(&claims.username),
                    true,
                )
                .await?,
            ));
        }
    }

    // 未指定特定 kyc_id 时，优先查找并更新该用户同类型的现有主体记录（个人或企业），互不覆盖
    let target_kyc_type = req
        .kyc_type
        .as_deref()
        .and_then(normalize_kyc_type)
        .unwrap_or("personal")
        .to_string();

    let existing_row: Option<UserKyc> = sqlx::query_as(
        &state.db.format_query(
            "SELECT * FROM user_kyc WHERE user_id = ? AND kyc_type = ? ORDER BY is_default DESC, id DESC LIMIT 1",
        ),
    )
    .bind(&user_id)
    .bind(&target_kyc_type)
    .fetch_optional(&state.db.pool)
    .await?;

    if let Some(existing) = existing_row {
        Ok(Json(
            update_kyc_record(
                &state,
                &user_id,
                existing.id,
                req,
                true,
                Some(&claims.username),
                true,
            )
            .await?,
        ))
    } else {
        Ok(Json(
            insert_kyc_record(&state, &user_id, req, true, Some(&claims.username), true).await?,
        ))
    }
}

/// 上传 KYC 证件图片（用户与管理员共用；管理员可传 target_user_id）
pub async fn upload_kyc_document(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    mut multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let is_admin = claims.role == "admin";

    let tos_config = crate::relay::tos_persist::load_system_object_store(&state)
        .await
        .ok_or_else(|| {
            AppError::BadRequest(
                "证件上传需要先配置对象存储，请管理员在「站点设置 → 数据库/存储」中完成 TOS 配置"
                    .to_string(),
            )
        })?;

    let mut file_data: Option<axum::body::Bytes> = None;
    let mut original_name = String::from("document");
    let mut mime_type = String::from("application/octet-stream");
    let mut target_user_id = claims.sub.clone();
    let mut doc_field = String::from("doc");

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            original_name = field
                .file_name()
                .unwrap_or("document")
                .chars()
                .take(120)
                .collect();
            mime_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            if !(mime_type.starts_with("image/") || mime_type == "application/pdf") {
                return Err(AppError::BadRequest(
                    "仅支持上传图片或 PDF 证件文件".to_string(),
                ));
            }
            file_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|_| AppError::BadRequest("读取文件失败".to_string()))?,
            );
        } else if name == "target_user_id" {
            let tid = field.text().await.unwrap_or_default();
            if is_admin && !tid.trim().is_empty() {
                target_user_id = tid.trim().to_string();
            }
        } else if name == "doc_field" {
            doc_field = field.text().await.unwrap_or_else(|_| "doc".to_string());
        }
    }

    let data = file_data.ok_or_else(|| AppError::BadRequest("请选择要上传的文件".to_string()))?;
    if data.len() > 12 * 1024 * 1024 {
        return Err(AppError::BadRequest("证件文件不能超过 12MB".to_string()));
    }

    let ext = original_name
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 8)
        .unwrap_or(if mime_type == "application/pdf" {
            "pdf"
        } else {
            "jpg"
        });
    let safe_field = doc_field
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(40)
        .collect::<String>();
    let target_uid: String = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT uid FROM users WHERE id = ? OR uid = ?"),
    )
    .bind(&target_user_id)
    .bind(&target_user_id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::BadRequest("用户不存在".to_string()))?;
    let object_key = tos_config.user_key(
        &target_uid,
        crate::services::object_store::UserFolder::Kyc,
        &format!(
            "{}_{}.{}",
            if safe_field.is_empty() {
                "doc"
            } else {
                &safe_field
            },
            chrono::Utc::now().timestamp_millis(),
            ext
        ),
    );

    let file_url = tos_config
        .upload_file(&object_key, data.to_vec(), &mime_type, None)
        .await
        .map_err(|e| {
            tracing::warn!("KYC TOS upload failed: {}", e);
            AppError::Internal("证件上传失败，请稍后重试".to_string())
        })?;

    Ok(Json(serde_json::json!({
        "file_url": file_url,
        "object_key": object_key,
    })))
}
