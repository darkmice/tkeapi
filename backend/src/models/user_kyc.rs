/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::time_system::DbTs;
use serde::{Deserialize, Serialize};

/// 用户实名认证记录（每用户一条）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserKyc {
    pub id: i64,
    pub user_id: String,
    /// personal | enterprise
    pub kyc_type: String,
    /// none | pending | approved | rejected | expired
    pub status: String,
    pub real_name: Option<String>,
    /// id_card | passport | driver_license | other
    pub id_doc_type: Option<String>,
    pub id_doc_number: Option<String>,
    pub personal_email: Option<String>,
    pub personal_phone: Option<String>,
    pub id_doc_front_url: Option<String>,
    pub id_doc_back_url: Option<String>,
    pub company_name: Option<String>,
    /// unified_social_credit_code | business_license | organization_code | other
    pub company_doc_type: Option<String>,
    pub company_doc_number: Option<String>,
    pub company_email: Option<String>,
    pub company_phone: Option<String>,
    pub business_license_url: Option<String>,
    pub tax_registration_url: Option<String>,
    pub legal_notarization_url: Option<String>,
    /// long_term | expire_date
    pub validity_type: String,
    pub expire_at: Option<DbTs>,
    pub reject_reason: Option<String>,
    pub admin_remark: Option<String>,
    pub is_default: bool,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<DbTs>,
    pub submitted_at: Option<DbTs>,
    pub created_at: DbTs,
    pub updated_at: DbTs,
}

impl UserKyc {
    pub fn empty_for(user_id: &str) -> Self {
        Self {
            id: 0,
            user_id: user_id.to_string(),
            kyc_type: "personal".to_string(),
            status: "none".to_string(),
            real_name: None,
            id_doc_type: None,
            id_doc_number: None,
            personal_email: None,
            personal_phone: None,
            id_doc_front_url: None,
            id_doc_back_url: None,
            company_name: None,
            company_doc_type: None,
            company_doc_number: None,
            company_email: None,
            company_phone: None,
            business_license_url: None,
            tax_registration_url: None,
            legal_notarization_url: None,
            validity_type: "long_term".to_string(),
            expire_at: None,
            reject_reason: None,
            admin_remark: None,
            is_default: false,
            reviewed_by: None,
            reviewed_at: None,
            submitted_at: None,
            created_at: DbTs::default(),
            updated_at: DbTs::default(),
        }
    }
}

/// 用户提交 / 管理员保存共用的写入体
#[derive(Debug, Deserialize)]
pub struct UpsertUserKycRequest {
    pub id: Option<i64>,
    pub kyc_type: Option<String>,
    /// 仅管理员可写
    pub status: Option<String>,
    pub real_name: Option<String>,
    pub id_doc_type: Option<String>,
    pub id_doc_number: Option<String>,
    pub personal_email: Option<String>,
    pub personal_phone: Option<String>,
    pub id_doc_front_url: Option<String>,
    pub id_doc_back_url: Option<String>,
    pub company_name: Option<String>,
    pub company_doc_type: Option<String>,
    pub company_doc_number: Option<String>,
    pub company_email: Option<String>,
    pub company_phone: Option<String>,
    pub contact_email: Option<String>,
    pub contact_phone: Option<String>,
    pub business_license_url: Option<String>,
    pub tax_registration_url: Option<String>,
    pub legal_notarization_url: Option<String>,
    pub validity_type: Option<String>,
    pub expire_at: Option<String>,
    pub reject_reason: Option<String>,
    pub admin_remark: Option<String>,
    pub is_default: Option<bool>,
}

impl UpsertUserKycRequest {
    pub fn resolved_personal_email(&self) -> Option<String> {
        self.personal_email
            .clone()
            .or_else(|| self.contact_email.clone())
    }
    pub fn resolved_personal_phone(&self) -> Option<String> {
        self.personal_phone
            .clone()
            .or_else(|| self.contact_phone.clone())
    }
    pub fn resolved_company_email(&self) -> Option<String> {
        self.company_email
            .clone()
            .or_else(|| self.contact_email.clone())
    }
    pub fn resolved_company_phone(&self) -> Option<String> {
        self.company_phone
            .clone()
            .or_else(|| self.contact_phone.clone())
    }
}

pub fn normalize_kyc_type(v: &str) -> Option<&'static str> {
    match v.trim() {
        "personal" => Some("personal"),
        "enterprise" => Some("enterprise"),
        _ => None,
    }
}

pub fn normalize_kyc_status(v: &str) -> Option<&'static str> {
    match v.trim() {
        "none" => Some("none"),
        "pending" => Some("pending"),
        "approved" => Some("approved"),
        "rejected" => Some("rejected"),
        "expired" => Some("expired"),
        _ => None,
    }
}

pub fn normalize_id_doc_type(v: &str) -> Option<&'static str> {
    match v.trim() {
        "id_card" => Some("id_card"),
        "passport" => Some("passport"),
        "driver_license" => Some("driver_license"),
        "other" => Some("other"),
        "" => None,
        _ => None,
    }
}

pub fn normalize_company_doc_type(v: &str) -> Option<&'static str> {
    match v.trim() {
        "unified_social_credit_code" => Some("unified_social_credit_code"),
        "business_license" => Some("business_license"),
        "organization_code" => Some("organization_code"),
        "other" => Some("other"),
        "" => None,
        _ => None,
    }
}

pub fn normalize_validity_type(v: &str) -> Option<&'static str> {
    match v.trim() {
        "long_term" => Some("long_term"),
        "expire_date" => Some("expire_date"),
        _ => None,
    }
}
