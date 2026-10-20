/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::time_system::DbTs;
use crate::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InvoiceConfigData {
    pub company_name: Option<String>,
    pub company_address: Option<String>,
    pub tax_id: Option<String>,
    pub contact_email: Option<String>,
    pub company_phone: Option<String>,
    pub company_website: Option<String>,
    pub currency_symbol: Option<String>,
    pub invoice_prefix: Option<String>,
    pub receipt_prefix: Option<String>,
    pub notes: Option<String>,
    pub show_invoices: Option<bool>,
    pub show_receipts: Option<bool>,
    // 境内模式配置
    pub china_min_amount: Option<f64>,
    pub china_invoice_content: Option<String>,
    pub china_tax_rate: Option<f64>,
    pub china_invoice_notice: Option<String>,
    // 销方开票主体列表
    pub sellers: Option<Vec<crate::models::SellerInvoiceSubject>>,
}

impl Default for InvoiceConfigData {
    fn default() -> Self {
        Self {
            company_name: None,
            company_address: None,
            tax_id: None,
            contact_email: None,
            company_phone: None,
            company_website: None,
            currency_symbol: Some("$".to_string()),
            invoice_prefix: Some("INV-".to_string()),
            receipt_prefix: Some("REC-".to_string()),
            notes: Some(
                "Thank you for your business! Payment has been processed in full.".to_string(),
            ),
            show_invoices: Some(true),
            show_receipts: Some(true),
            china_min_amount: Some(100.0),
            china_invoice_content: Some("*信息技术服务*软件技术服务费".to_string()),
            china_tax_rate: Some(6.0),
            china_invoice_notice: Some(
                "增值税普通发票将在审核通过后发送至填写的邮箱。".to_string(),
            ),
            sellers: None,
        }
    }
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct FundTypeStats {
    /// 实际充值支付的现金资金总额 (在线订单 + 手动现金充值，合法收款凭证)
    pub total_real_paid: f64,
    /// 管理后台或活动赠送体验金总额 (免税赠送，不可开具发票与付款收据)
    pub total_gift_recharged: f64,
    /// 实际现金消费金额
    pub total_real_consumed: f64,
    /// 赠送金抵扣消费金额
    pub total_gift_consumed: f64,
    /// 当前系统钱包现金余额
    pub current_balance: f64,
    /// 当前赠送钱包余额
    pub current_gift_balance: f64,
    /// 当前信控授信额度
    pub current_credit_limit: f64,
    /// 累计已申请或开具发票总额 (中国大陆模式)
    pub total_invoiced_amount: f64,
    /// 剩余可开具发票的实付金额 (total_real_paid - total_invoiced_amount)
    pub remaining_invoiceable_amount: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct UserInvoiceItem {
    pub id: String,
    pub order_id: Option<i64>,
    pub date: String,
    pub raw_date: String,
    pub payment: String,
    pub amount: f64,
    pub currency: String,
    pub payment_method: String,
    pub description: String,
    /// 资金类型: "real_payment" (实付资金) | "gift_bonus" (赠送金)
    pub fund_type: String,
    /// 资金类型标签: "实付充值" | "活动赠送"
    pub fund_type_label: String,
    /// 是否可作为实付发票出具
    pub is_invoiceable: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct UserReceiptItem {
    pub id: String,
    pub order_id: Option<i64>,
    pub date: String,
    pub raw_date: String,
    pub amount: f64,
    pub currency: String,
    pub payment_method: String,
    pub transaction_id: Option<String>,
    /// 资金类型: "real_payment" (收据仅为实付资金出具)
    pub fund_type: String,
    pub fund_type_label: String,
}

#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct UserInvoiceRequestRecord {
    pub id: i64,
    #[sqlx(default)]
    pub request_no: Option<String>,
    pub user_id: String,
    pub invoice_type: String,
    pub title_type: String,
    pub title: String,
    pub tax_number: Option<String>,
    pub company_address: Option<String>,
    pub company_phone: Option<String>,
    pub bank_name: Option<String>,
    pub bank_account: Option<String>,
    pub amount: f64,
    pub email: String,
    pub remark: Option<String>,
    pub status: String,
    pub reject_reason: Option<String>,
    pub invoice_file_url: Option<String>,
    #[sqlx(default)]
    pub invoice_code: Option<String>,
    #[sqlx(default)]
    pub invoice_number: Option<String>,
    #[sqlx(default)]
    pub admin_remark: Option<String>,
    #[sqlx(default)]
    pub issued_at: Option<DbTs>,
    #[sqlx(default)]
    pub operator_id: Option<String>,
    #[sqlx(default)]
    pub operator_name: Option<String>,
    pub created_at: DbTs,
    pub updated_at: DbTs,
}

#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct AdminInvoiceRequestItem {
    pub id: i64,
    #[sqlx(default)]
    pub request_no: Option<String>,
    pub user_id: String,
    #[sqlx(default)]
    pub username: Option<String>,
    #[sqlx(default)]
    pub user_email: Option<String>,
    #[sqlx(default)]
    pub user_nickname: Option<String>,
    #[sqlx(default)]
    pub user_uid: Option<String>,
    pub invoice_type: String,
    pub title_type: String,
    pub title: String,
    pub tax_number: Option<String>,
    pub company_address: Option<String>,
    pub company_phone: Option<String>,
    pub bank_name: Option<String>,
    pub bank_account: Option<String>,
    pub amount: f64,
    pub email: String,
    pub remark: Option<String>,
    pub status: String,
    pub reject_reason: Option<String>,
    pub invoice_file_url: Option<String>,
    #[sqlx(default)]
    pub invoice_code: Option<String>,
    #[sqlx(default)]
    pub invoice_number: Option<String>,
    #[sqlx(default)]
    pub admin_remark: Option<String>,
    #[sqlx(default)]
    pub issued_at: Option<DbTs>,
    #[sqlx(default)]
    pub operator_id: Option<String>,
    #[sqlx(default)]
    pub operator_name: Option<String>,
    pub created_at: DbTs,
    pub updated_at: DbTs,
}

#[derive(Debug, Deserialize)]
pub struct AdminInvoiceListParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub status: Option<String>,
    pub invoice_type: Option<String>,
    pub title_type: Option<String>,
    pub keyword: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct InvoiceAuditStats {
    pub pending_count: i64,
    pub approved_count: i64,
    pub rejected_count: i64,
    pub total_count: i64,
    pub total_approved_amount: f64,
    pub this_month_approved_amount: f64,
}

#[derive(Debug, Serialize)]
pub struct AdminInvoiceListResponse {
    pub items: Vec<AdminInvoiceRequestItem>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub stats: InvoiceAuditStats,
}

#[derive(Debug, Deserialize)]
pub struct AdminInvoiceAuditPayload {
    pub action: String, // "approve" | "reject" | "update"
    pub invoice_code: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_file_url: Option<String>,
    pub admin_remark: Option<String>,
    pub reject_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvoiceRequestPayload {
    pub invoice_type: String, // normal, special
    pub title_type: String,   // company, personal
    pub title: String,
    pub tax_number: Option<String>,
    pub company_address: Option<String>,
    pub company_phone: Option<String>,
    pub bank_name: Option<String>,
    pub bank_account: Option<String>,
    pub amount: f64,
    pub email: String,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct BillingSubject {
    #[serde(default)]
    pub id: Option<i64>,
    pub subject_type: String, // "enterprise" | "personal"
    pub display_name: String,
    pub doc_type: Option<String>,
    pub doc_number: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub bank_name: Option<String>,
    pub bank_account: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CustomerProfile {
    /// 显示名称：优先企业名称 -> 个人真实姓名 -> 用户昵称 -> 用户名
    pub display_name: String,
    /// 用户账号唯一标识 (优先数字 UID 如 1006968055，无则用户 UUID)
    pub account_id: String,
    /// 邮箱：优先企业/实名联系邮箱 -> 用户账号邮箱
    pub email: Option<String>,
    /// 电话：优先企业/实名联系电话 -> 用户绑定手机
    pub phone: Option<String>,
    /// 用户主体类别: "enterprise" | "personal" | "unverified"
    pub kyc_type: String,
    /// 企业名称
    pub company_name: Option<String>,
    /// 企业证件类型 (如 unified_social_credit_code)
    pub company_doc_type: Option<String>,
    /// 统一社会信用代码 / 税号 / 纳税人识别号
    pub company_doc_number: Option<String>,
    /// 个人真实姓名
    pub real_name: Option<String>,
    /// 个人证件类型 (如 id_card, passport)
    pub id_doc_type: Option<String>,
    /// 个人证件号码
    pub id_doc_number: Option<String>,
    /// 注册地址 / 联系地址
    pub address: Option<String>,
    /// 是否已完善用户信息
    pub is_completed: bool,
    /// 独立的个人开票主体（若有填写真实姓名或证件）
    pub personal_subject: Option<BillingSubject>,
    /// 独立的企业开票主体（若有填写企业名称或税号）
    pub enterprise_subject: Option<BillingSubject>,
    /// 所有有效的实名认证主体列表（支持多个实名信息）
    pub subjects: Vec<BillingSubject>,
    /// 是否同时存在多个有效开票主体，可供前端下拉切换
    pub has_multiple_subjects: bool,
}

#[derive(Debug, Serialize)]
pub struct UserInvoicesResponse {
    pub enabled: bool,
    pub mode: String,
    pub config: InvoiceConfigData,
    pub fund_stats: FundTypeStats,
    pub customer_profile: CustomerProfile,
    pub invoices: Vec<UserInvoiceItem>,
    pub receipts: Vec<UserReceiptItem>,
    pub china_requests: Vec<UserInvoiceRequestRecord>,
    pub total_paid_amount: f64,
}

fn format_day_suffix(day: u32) -> &'static str {
    match day {
        1 | 21 | 31 => "st",
        2 | 22 => "nd",
        3 | 23 => "rd",
        _ => "th",
    }
}

fn format_invoice_date(dt: DateTime<Utc>) -> String {
    let month_name = dt.format("%B").to_string();
    let day = dt
        .format("%e")
        .to_string()
        .trim()
        .parse::<u32>()
        .unwrap_or(1);
    let suffix = format_day_suffix(day);
    let year = dt.format("%Y").to_string();
    format!("{month_name} {day}{suffix}, {year}")
}

fn format_receipt_date(dt: DateTime<Utc>, user_tz_name: &str) -> String {
    let tz: chrono_tz::Tz = user_tz_name.parse().unwrap_or(chrono_tz::Asia::Shanghai);
    let local_dt = dt.with_timezone(&tz);
    let month_name = local_dt.format("%B").to_string();
    let day = local_dt
        .format("%e")
        .to_string()
        .trim()
        .parse::<u32>()
        .unwrap_or(1);
    let suffix = format_day_suffix(day);
    let year = local_dt.format("%Y").to_string();
    let time_part = local_dt.format("%l:%M:%S %p").to_string();
    let tz_offset = local_dt.format("%Z").to_string();
    let tz_display = if tz_offset == "CST"
        || tz_offset == "+08"
        || tz_offset == "+08:00"
        || tz_offset == "+0800"
    {
        "UTC+8".to_string()
    } else if tz_offset.starts_with("GMT") {
        tz_offset.replace("GMT", "UTC")
    } else if tz_offset.starts_with('+') || tz_offset.starts_with('-') {
        format!("UTC{}", tz_offset)
    } else {
        tz_offset
    };
    format!(
        "{month_name} {day}{suffix}, {year} at {} {tz_display}",
        time_part.trim()
    )
}

async fn load_user_invoices(
    state: &Arc<AppState>,
    user_id: &str,
    is_admin: bool,
) -> AppResult<UserInvoicesResponse> {
    // 1. 查询当前用户等级发票开关与时区
    let level_info: Option<(Option<i64>, Option<String>)> = sqlx::query_as(&state.db.format_query(
        "SELECT ul.invoice_enabled, u.timezone \
             FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key \
             WHERE u.id = ?",
    ))
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;

    let (invoice_enabled_val, user_tz) = match level_info {
        Some((e, tz)) => (e.unwrap_or(0), tz),
        None => (0, None),
    };

    let enabled = invoice_enabled_val == 1 || is_admin;

    // 2. 读取全局统一的发票设置 (settings 表中 key = 'invoice_settings')
    let global_inv_setting_str: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = ?"),
    )
    .bind("invoice_settings")
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    let mut invoice_settings: crate::models::InvoiceSettings =
        if let Some(ref s) = global_inv_setting_str {
            serde_json::from_str(s).unwrap_or_default()
        } else {
            Default::default()
        };
    invoice_settings.clear_placeholder_issuer();

    let mode = invoice_settings.invoice_mode.clone();

    let effective_sellers = invoice_settings.get_effective_sellers();

    let default_seller = effective_sellers
        .iter()
        .find(|s| s.is_default)
        .or_else(|| effective_sellers.first());

    let mut config = InvoiceConfigData {
        company_name: Some(invoice_settings.invoice_company_name),
        company_address: Some(invoice_settings.invoice_company_address),
        tax_id: Some(invoice_settings.invoice_tax_id),
        contact_email: Some(invoice_settings.invoice_contact_email),
        company_phone: default_seller
            .and_then(|s| s.company_phone.clone())
            .or_else(|| {
                if invoice_settings.invoice_company_phone.is_empty() {
                    None
                } else {
                    Some(invoice_settings.invoice_company_phone.clone())
                }
            }),
        company_website: default_seller
            .and_then(|s| s.company_website.clone())
            .or_else(|| {
                if invoice_settings.invoice_company_website.is_empty() {
                    None
                } else {
                    Some(invoice_settings.invoice_company_website.clone())
                }
            }),
        currency_symbol: Some(invoice_settings.invoice_currency_symbol),
        invoice_prefix: Some(invoice_settings.invoice_prefix),
        receipt_prefix: Some(invoice_settings.receipt_prefix),
        notes: Some(invoice_settings.invoice_notes),
        show_invoices: Some(invoice_settings.invoice_show_invoices),
        show_receipts: Some(invoice_settings.invoice_show_receipts),
        china_min_amount: Some(invoice_settings.china_min_amount),
        china_invoice_content: Some(invoice_settings.china_invoice_content),
        china_tax_rate: Some(invoice_settings.china_tax_rate),
        china_invoice_notice: Some(invoice_settings.china_invoice_notice),
        sellers: if !effective_sellers.is_empty() {
            Some(effective_sellers)
        } else {
            None
        },
    };

    if config.currency_symbol.as_deref().unwrap_or("").is_empty() {
        config.currency_symbol = Some("$".to_string());
    }

    let tz_name = user_tz.unwrap_or_else(|| "Asia/Shanghai".to_string());
    let inv_prefix = config.invoice_prefix.as_deref().unwrap_or("INV-");
    let rec_prefix = config.receipt_prefix.as_deref().unwrap_or("REC-");
    let curr = config.currency_symbol.as_deref().unwrap_or("$");

    // 2. 查询用户钱包与信用余额
    let user_wallet: Option<(f64, f64, f64)> = sqlx::query_as(&state.db.format_query(
        "SELECT COALESCE(balance, 0.0), COALESCE(gift_balance, 0.0), COALESCE(credit_limit, 0.0) \
             FROM users WHERE id = ?",
    ))
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;

    let (current_balance, current_gift_balance, current_credit_limit) =
        user_wallet.unwrap_or((0.0, 0.0, 0.0));

    // 3. 查询已完成支付在线订单 (orders WHERE user_id = ? AND status = 'paid')
    let orders: Vec<(i64, String, String, f64, Option<String>, DbTs, Option<DbTs>)> =
        sqlx::query_as(&state.db.format_query(
            "SELECT id, out_trade_no, payment_method, amount, trade_no, created_at, paid_at \
             FROM orders WHERE user_id = ? AND status = 'paid' \
             ORDER BY created_at DESC",
        ))
        .bind(user_id)
        .fetch_all(&state.db.pool)
        .await
        .unwrap_or_default();

    // 4. 查询充值记录 (包含 wallet_type 和 operator)
    let recharges: Vec<(
        i64,
        f64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        DbTs,
    )> = sqlx::query_as(&state.db.format_query(
        "SELECT id, amount, recharge_type, remark, operator, wallet_type, created_at \
             FROM recharge_records WHERE user_id = ? \
             ORDER BY created_at DESC",
    ))
    .bind(user_id)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    // 5. 查询消费统计 (严格双轨：历史天优先走 usage_daily_stats，今日走 logs，确保归档后财务数据 100% 精准无损)
    let tz = crate::time_system::parse_timedisplay(&tz_name);
    let (_total_consumed, total_gift_consumed, total_real_consumed) =
        crate::api::date_helper::query_user_consumption_summary(&state.db, user_id, tz).await;

    // 6. 查询已申请发票总额 (中国大陆模式)
    let invoiced_total: Option<f64> = sqlx::query_scalar(
        &state.db.format_query(
            "SELECT SUM(amount) FROM user_invoice_requests WHERE user_id = ? AND status IN ('approved', 'pending')"
        )
    )
    .bind(user_id)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(None);

    let total_invoiced_amount = invoiced_total.unwrap_or(0.0);

    let mut invoices: Vec<UserInvoiceItem> = Vec::new();
    let mut receipts: Vec<UserReceiptItem> = Vec::new();
    let mut total_real_paid: f64 = 0.0;
    let mut total_gift_recharged: f64 = 0.0;

    // A. 处理在线支付完成的订单 (均为实付资金，可作为发票与付款收据)
    for (order_id, out_trade_no, method, amount, trade_no, created_at, paid_at) in orders {
        if amount <= 0.0 {
            continue;
        }
        total_real_paid += amount;
        let dt: DateTime<Utc> = paid_at
            .as_ref()
            .and_then(|t| t.to_utc())
            .or_else(|| created_at.to_utc())
            .unwrap_or_else(Utc::now);
        let inv_date_str = format_invoice_date(dt);
        let rec_date_str = format_receipt_date(dt, &tz_name);

        let inv_id = if out_trade_no.starts_with("T") {
            format!("{}{}", inv_prefix, &out_trade_no)
        } else {
            format!("{}{}-{}", inv_prefix, dt.format("%Y%m%d"), order_id)
        };

        let rec_id = if let Some(ref tn) = trade_no {
            format!("{}{}", rec_prefix, tn)
        } else {
            format!("{}{}-{}", rec_prefix, dt.format("%Y%m%d"), order_id)
        };

        invoices.push(UserInvoiceItem {
            id: inv_id,
            order_id: Some(order_id),
            date: inv_date_str,
            raw_date: dt.to_rfc3339(),
            payment: "paid".to_string(),
            amount,
            currency: curr.to_string(),
            payment_method: method.clone(),
            description: "Purchase of Prepaid API Service Credits".to_string(),
            fund_type: "real_payment".to_string(),
            fund_type_label: "实付充值 (Real Paid)".to_string(),
            is_invoiceable: true,
        });

        receipts.push(UserReceiptItem {
            id: rec_id,
            order_id: Some(order_id),
            date: rec_date_str,
            raw_date: dt.to_rfc3339(),
            amount,
            currency: curr.to_string(),
            payment_method: method,
            transaction_id: trade_no,
            fund_type: "real_payment".to_string(),
            fund_type_label: "实付资金 (Real Paid)".to_string(),
        });
    }

    // B. 处理充值记录（严格区分系统现金钱包 vs 赠送钱包 vs 信控）
    for (rec_id, amount, r_type, remark, _operator, wallet_type_opt, created_at) in recharges {
        let w_type = wallet_type_opt.as_deref().unwrap_or("system");
        let dt: DateTime<Utc> = created_at.to_utc().unwrap_or_else(Utc::now);
        let inv_date_str = format_invoice_date(dt);
        let rec_date_str = format_receipt_date(dt, &tz_name);

        let is_online_order_callback = r_type == "wechat"
            || r_type == "alipay"
            || r_type == "stripe"
            || remark.as_deref().unwrap_or("").contains("订单号");

        if w_type == "gift"
            || r_type == "gift"
            || r_type == "registration"
            || r_type == "commission"
        {
            // === 赠送钱包 (活动体验金) ===
            // 绝不加入付款收据 (Receipt) 和发票账单 (Invoice)，因为客户未真实付款，不属于应税商业交易！
            if amount > 0.0 {
                total_gift_recharged += amount;
            }
        } else if w_type == "system" && amount > 0.0 && !is_online_order_callback {
            // === 系统现金钱包 (手工充值/对公转账到账，且非在线订单回调去重) ===
            total_real_paid += amount;
            let inv_no = format!("{}{}-{}", inv_prefix, dt.format("%Y%m%d"), rec_id);
            let rec_no = format!("{}{}-{}", rec_prefix, dt.format("%Y%m%d"), rec_id);
            let desc = remark.unwrap_or_else(|| "Manual Wire / Offline Cash Top-up".to_string());

            invoices.push(UserInvoiceItem {
                id: inv_no,
                order_id: Some(rec_id),
                date: inv_date_str,
                raw_date: dt.to_rfc3339(),
                payment: "paid".to_string(),
                amount,
                currency: curr.to_string(),
                payment_method: if r_type == "manual" {
                    "Wire Transfer / Offline".to_string()
                } else {
                    r_type.clone()
                },
                description: desc,
                fund_type: "real_payment".to_string(),
                fund_type_label: "实付充值 (Cash Top-up)".to_string(),
                is_invoiceable: true,
            });

            receipts.push(UserReceiptItem {
                id: rec_no,
                order_id: Some(rec_id),
                date: rec_date_str,
                raw_date: dt.to_rfc3339(),
                amount,
                currency: curr.to_string(),
                payment_method: if r_type == "manual" {
                    "Wire Transfer / Offline".to_string()
                } else {
                    r_type
                },
                transaction_id: None,
                fund_type: "real_payment".to_string(),
                fund_type_label: "实付现金 (Cash Paid)".to_string(),
            });
        }
    }

    let remaining_invoiceable_amount = (total_real_paid - total_invoiced_amount).max(0.0);

    let fund_stats = FundTypeStats {
        total_real_paid,
        total_gift_recharged,
        total_real_consumed,
        total_gift_consumed,
        current_balance,
        current_gift_balance,
        current_credit_limit,
        total_invoiced_amount,
        remaining_invoiceable_amount,
    };

    // 4. 查询境内发票申请记录
    let china_requests: Vec<UserInvoiceRequestRecord> = sqlx::query_as(&state.db.format_query(
        "SELECT * FROM user_invoice_requests WHERE user_id = ? ORDER BY created_at DESC",
    ))
    .bind(user_id)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    // 5. 查询用户账号基础信息 (users 表: username, email, nickname, mobile, uid)
    let user_info: Option<(String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(
            &state.db.format_query(
                "SELECT uid, username, email, nickname, mobile FROM users WHERE id = ?",
            ),
        )
        .bind(user_id)
        .fetch_optional(&state.db.pool)
        .await
        .unwrap_or(None);

    let (u_uid, u_username, u_email, u_nickname, u_mobile) = match user_info {
        Some((uid, username, email, nickname, mobile)) => {
            let account_id = if !uid.is_empty() {
                uid
            } else {
                user_id.to_string()
            };
            (account_id, username, email, nickname, mobile)
        }
        None => (
            user_id.to_string(),
            String::new(),
            String::new(),
            None,
            None,
        ),
    };

    // 6. 查询完善用户信息 (user_kyc 表多记录)
    let kyc_rows: Vec<crate::models::UserKyc> = sqlx::query_as(&state.db.format_query(
        "SELECT * FROM user_kyc WHERE user_id = ? ORDER BY is_default DESC, id DESC",
    ))
    .bind(user_id)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    let _def_kyc = kyc_rows.iter().find(|k| k.is_default);
    let _ent_kyc = kyc_rows
        .iter()
        .find(|k| k.kyc_type == "enterprise" && k.status == "approved")
        .or_else(|| kyc_rows.iter().find(|k| k.kyc_type == "enterprise"));
    let _per_kyc = kyc_rows
        .iter()
        .find(|k| k.kyc_type == "personal" && k.status == "approved")
        .or_else(|| kyc_rows.iter().find(|k| k.kyc_type == "personal"));
    let primary_kyc = _def_kyc.or_else(|| kyc_rows.first());

    // 7. 查询历史已提交/通过的发票抬头记录 (user_invoice_requests 表)
    let last_inv_req: Option<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(&state.db.format_query(
        "SELECT title, tax_number, company_address, company_phone, email, bank_name, bank_account \
             FROM user_invoice_requests WHERE user_id = ? ORDER BY id DESC LIMIT 1",
    ))
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    let last_ent_inv_req: Option<(String, Option<String>, Option<String>, Option<String>, String, Option<String>, Option<String>)> = sqlx::query_as(
        &state.db.format_query(
            "SELECT title, tax_number, company_address, company_phone, email, bank_name, bank_account \
             FROM user_invoice_requests WHERE user_id = ? AND title_type = 'company' ORDER BY id DESC LIMIT 1"
        )
    )
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    let mut customer_profile = CustomerProfile {
        display_name: u_nickname
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| u_username.clone()),
        account_id: u_uid,
        email: if !u_email.trim().is_empty() {
            Some(u_email.clone())
        } else {
            None
        },
        phone: u_mobile.clone().filter(|s| !s.trim().is_empty()),
        kyc_type: "unverified".to_string(),
        company_name: None,
        company_doc_type: None,
        company_doc_number: None,
        real_name: None,
        id_doc_type: None,
        id_doc_number: None,
        address: None,
        is_completed: false,
        personal_subject: None,
        enterprise_subject: None,
        subjects: Vec::new(),
        has_multiple_subjects: false,
    };

    if let Some(kyc) = primary_kyc {
        let is_enterprise = kyc.kyc_type == "enterprise";
        let has_cname = kyc
            .company_name
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let has_rname = kyc
            .real_name
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

        if is_enterprise && (has_cname || kyc.company_doc_number.is_some()) {
            customer_profile.is_completed = true;
            customer_profile.kyc_type = "enterprise".to_string();
            customer_profile.company_name = kyc.company_name.clone();
            customer_profile.company_doc_type = kyc.company_doc_type.clone();
            customer_profile.company_doc_number = kyc.company_doc_number.clone();
            if let Some(ref cname) = kyc.company_name {
                if !cname.trim().is_empty() {
                    customer_profile.display_name = cname.trim().to_string();
                }
            }
            if let Some(ref cmail) = kyc.company_email {
                if !cmail.trim().is_empty() {
                    customer_profile.email = Some(cmail.trim().to_string());
                }
            }
            if let Some(ref cphone) = kyc.company_phone {
                if !cphone.trim().is_empty() {
                    customer_profile.phone = Some(cphone.trim().to_string());
                }
            }
        } else if has_rname || kyc.id_doc_number.is_some() {
            customer_profile.is_completed = true;
            customer_profile.kyc_type = "personal".to_string();
            customer_profile.real_name = kyc.real_name.clone();
            customer_profile.id_doc_type = kyc.id_doc_type.clone();
            customer_profile.id_doc_number = kyc.id_doc_number.clone();
            if let Some(ref rname) = kyc.real_name {
                if !rname.trim().is_empty() {
                    customer_profile.display_name = rname.trim().to_string();
                }
            }
            if let Some(ref pmail) = kyc.personal_email {
                if !pmail.trim().is_empty() {
                    customer_profile.email = Some(pmail.trim().to_string());
                }
            }
            if let Some(ref pphone) = kyc.personal_phone {
                if !pphone.trim().is_empty() {
                    customer_profile.phone = Some(pphone.trim().to_string());
                }
            }
        }
    }

    // 若 KYC 没填地址/税号，而历史开票抬头中有，则自动补齐
    if let Some((ref req_title, ref req_tax, ref req_addr, ref req_phone, ref req_mail, _, _)) =
        last_inv_req
    {
        if !customer_profile.is_completed && !req_title.trim().is_empty() {
            customer_profile.display_name = req_title.trim().to_string();
            customer_profile.is_completed = true;
        }
        if customer_profile.company_doc_number.is_none() && customer_profile.id_doc_number.is_none()
        {
            if let Some(ref tax) = req_tax {
                if !tax.trim().is_empty() {
                    customer_profile.company_doc_number = Some(tax.trim().to_string());
                }
            }
        }
        if customer_profile.address.is_none() {
            if let Some(ref addr) = req_addr {
                if !addr.trim().is_empty() {
                    customer_profile.address = Some(addr.trim().to_string());
                }
            }
        }
        if customer_profile.email.is_none() && !req_mail.trim().is_empty() {
            customer_profile.email = Some(req_mail.trim().to_string());
        }
        if customer_profile.phone.is_none() {
            if let Some(ref phone) = req_phone {
                if !phone.trim().is_empty() {
                    customer_profile.phone = Some(phone.trim().to_string());
                }
            }
        }
    }

    // 8. 构建该用户所有的有效开票主体列表（支持多个实名信息/多企业/多主体）
    let ent_inv_ref = last_ent_inv_req.as_ref().or(last_inv_req.as_ref());
    let mut all_subjects: Vec<BillingSubject> = Vec::new();

    for kyc in &kyc_rows {
        if kyc.kyc_type == "enterprise" {
            let has_cname = kyc
                .company_name
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let has_cdoc = kyc
                .company_doc_number
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if has_cname || has_cdoc {
                let cname = {
                    let named = kyc.company_name.as_deref().unwrap_or("").trim().to_string();
                    if !named.is_empty() {
                        named
                    } else {
                        kyc.company_doc_number
                            .as_deref()
                            .unwrap_or("")
                            .trim()
                            .to_string()
                    }
                };
                let mut ent = BillingSubject {
                    id: Some(kyc.id),
                    subject_type: "enterprise".to_string(),
                    display_name: cname,
                    doc_type: kyc.company_doc_type.clone(),
                    doc_number: kyc.company_doc_number.clone(),
                    email: kyc
                        .company_email
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| {
                            if !u_email.trim().is_empty() {
                                Some(u_email.clone())
                            } else {
                                None
                            }
                        }),
                    phone: kyc
                        .company_phone
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| u_mobile.clone()),
                    address: customer_profile.address.clone(),
                    bank_name: None,
                    bank_account: None,
                    is_default: kyc.is_default,
                    status: Some(kyc.status.clone()),
                };
                if let Some((
                    _,
                    _,
                    ref req_addr,
                    ref req_phone,
                    _,
                    ref req_bank_name,
                    ref req_bank_account,
                )) = ent_inv_ref
                {
                    if ent.address.is_none() && req_addr.is_some() {
                        ent.address = req_addr.clone();
                    }
                    if ent.phone.is_none() && req_phone.is_some() {
                        ent.phone = req_phone.clone();
                    }
                    if ent.bank_name.is_none() && req_bank_name.is_some() {
                        ent.bank_name = req_bank_name.clone();
                    }
                    if ent.bank_account.is_none() && req_bank_account.is_some() {
                        ent.bank_account = req_bank_account.clone();
                    }
                }
                all_subjects.push(ent);
            }
        } else if kyc.kyc_type == "personal" {
            let has_rname = kyc
                .real_name
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let has_rdoc = kyc
                .id_doc_number
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if has_rname || has_rdoc {
                let rname = kyc
                    .real_name
                    .as_deref()
                    .unwrap_or_else(|| u_nickname.as_deref().unwrap_or(&u_username))
                    .trim()
                    .to_string();
                all_subjects.push(BillingSubject {
                    id: Some(kyc.id),
                    subject_type: "personal".to_string(),
                    display_name: rname,
                    doc_type: kyc.id_doc_type.clone(),
                    doc_number: kyc.id_doc_number.clone(),
                    email: kyc
                        .personal_email
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| {
                            if !u_email.trim().is_empty() {
                                Some(u_email.clone())
                            } else {
                                None
                            }
                        }),
                    phone: kyc
                        .personal_phone
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| u_mobile.clone()),
                    address: customer_profile.address.clone(),
                    bank_name: None,
                    bank_account: None,
                    is_default: kyc.is_default,
                    status: Some(kyc.status.clone()),
                });
            }
        }
    }

    let enterprise_sub = all_subjects
        .iter()
        .find(|s| s.subject_type == "enterprise" && s.is_default)
        .or_else(|| all_subjects.iter().find(|s| s.subject_type == "enterprise"))
        .cloned();
    let personal_sub = all_subjects
        .iter()
        .find(|s| s.subject_type == "personal" && s.is_default)
        .or_else(|| all_subjects.iter().find(|s| s.subject_type == "personal"))
        .cloned();

    let has_multiple_subjects = all_subjects.len() > 1;
    customer_profile.has_multiple_subjects = has_multiple_subjects;
    customer_profile.personal_subject = personal_sub;
    customer_profile.enterprise_subject = enterprise_sub;
    customer_profile.subjects = all_subjects;

    Ok(UserInvoicesResponse {
        enabled,
        mode,
        config,
        fund_stats,
        customer_profile,
        invoices,
        receipts,
        china_requests,
        total_paid_amount: total_real_paid,
    })
}

pub async fn get_user_invoices(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<UserInvoicesResponse>> {
    let is_admin = claims.role == "admin";
    let data = load_user_invoices(&state, &claims.sub, is_admin).await?;
    Ok(Json(data))
}

pub async fn admin_get_user_invoices(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<UserInvoicesResponse>> {
    let user_id: String = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT id FROM users WHERE id = ? OR uid = ?"),
    )
    .bind(&id)
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let data = load_user_invoices(&state, &user_id, true).await?;
    Ok(Json(data))
}

/// 生成发票申请单号
/// 规则：开头 fp + 中间年月日 (YYMMDD) + 尾数随机 4 位数（例如 fp2605152521）
/// 防重逻辑：如果在数据库中查到重复单号，进行重试；若单日该位数已拥挤/重复过多，尾数自动增加到 5 位数，依此类推。
pub async fn generate_invoice_request_no(state: &Arc<AppState>) -> Result<String, AppError> {
    use rand::Rng;
    let shanghai_time = chrono::Utc::now().with_timezone(&chrono_tz::Asia::Shanghai);
    let date_str = shanghai_time.format("%y%m%d").to_string(); // e.g. "260906" or "260515"
    let prefix = format!("fp{}", date_str);

    let mut digit_count: usize = 4;

    loop {
        // 当前位数下重试最多 15 次，若均发生冲突则位数自增（4位 -> 5位 -> 6位 ...）
        for _ in 0..15 {
            let min_val = if digit_count == 4 {
                1000
            } else {
                10_u64.pow((digit_count - 1) as u32)
            };
            let max_val = 10_u64.pow(digit_count as u32) - 1;
            let random_num = rand::thread_rng().gen_range(min_val..=max_val);
            let candidate = format!("{}{:0width$}", prefix, random_num, width = digit_count);

            let exists: Option<i64> =
                sqlx::query_scalar(&state.db.format_query(
                    "SELECT 1 FROM user_invoice_requests WHERE request_no = ? LIMIT 1",
                ))
                .bind(&candidate)
                .fetch_optional(&state.db.pool)
                .await?;

            if exists.is_none() {
                return Ok(candidate);
            }
        }

        digit_count += 1;
        if digit_count > 10 {
            let uuid_suffix = &uuid::Uuid::new_v4().simple().to_string()[..6];
            return Ok(format!("{}{}", prefix, uuid_suffix));
        }
    }
}

pub async fn create_user_invoice_request(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Json(payload): Json<CreateInvoiceRequestPayload>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = &claims.sub;

    let level_enabled: Option<i64> = sqlx::query_scalar(
        &state.db.format_query(
            "SELECT ul.invoice_enabled FROM users u LEFT JOIN user_levels ul ON u.user_group = ul.group_key WHERE u.id = ?"
        )
    )
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;

    if claims.role != "admin" && level_enabled.unwrap_or(0) != 1 {
        return Err(AppError::Forbidden(
            "当前用户等级未开启发票开具权限".to_string(),
        ));
    }

    if payload.amount <= 0.0 {
        return Err(AppError::BadRequest("开票金额必须大于 0".to_string()));
    }
    if payload.title.trim().is_empty() {
        return Err(AppError::BadRequest("发票抬头不能为空".to_string()));
    }
    if payload.email.trim().is_empty() {
        return Err(AppError::BadRequest("接收邮箱不能为空".to_string()));
    }

    if payload.invoice_type == "special" {
        let has_approved_enterprise: bool = sqlx::query_scalar(
            &state.db.format_query(
                "SELECT EXISTS(SELECT 1 FROM user_kyc WHERE user_id = ? AND kyc_type = 'enterprise' AND status = 'approved')"
            )
        )
        .bind(user_id)
        .fetch_one(&state.db.pool)
        .await
        .unwrap_or(false);

        if !has_approved_enterprise {
            return Err(AppError::BadRequest(
                "仅通过实名认证的企业主体才可申请开具增值税专用发票".to_string(),
            ));
        }
    }

    // 严格核验实付金额：仅允许对实际充值支付的现金开票，完全排除赠送金与信控额度
    let orders_total: f64 = sqlx::query_scalar(&state.db.format_query(
        "SELECT COALESCE(SUM(amount), 0.0) FROM orders WHERE user_id = ? AND status = 'paid'",
    ))
    .bind(user_id)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(0.0);

    let manual_cash_total: f64 = sqlx::query_scalar(
        &state.db.format_query(
            "SELECT COALESCE(SUM(amount), 0.0) FROM recharge_records \
             WHERE user_id = ? AND wallet_type = 'system' \
               AND ((amount > 0 \
                 AND recharge_type NOT IN ('wechat', 'alipay', 'stripe', 'gift', 'registration', 'commission', 'model_voucher', 'coupon_purchase', 'coupon_pack', 'coupon_subscription', 'redemption_refund') \
                 AND (remark IS NULL OR remark NOT LIKE '%订单号%')) \
                OR recharge_type = 'redemption_refund')"
        )
    )
    .bind(user_id)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(0.0);

    let total_real_paid = orders_total + manual_cash_total;

    let invoiced_total: f64 = sqlx::query_scalar(&state.db.format_query(
        "SELECT COALESCE(SUM(amount), 0.0) FROM user_invoice_requests \
             WHERE user_id = ? AND status IN ('approved', 'pending')",
    ))
    .bind(user_id)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(0.0);

    let remaining_invoiceable = (total_real_paid - invoiced_total).max(0.0);

    if payload.amount > remaining_invoiceable + 0.001 {
        return Err(AppError::BadRequest(format!(
            "申请开票金额 (¥{:.2}) 超过当前剩余可开票实付额度 (¥{:.2})。依据国家增值税法与财务规范，仅支持对真实充值支付的现金资金开具发票，活动赠送金、体验券及信控额度均不参与发票开具。",
            payload.amount, remaining_invoiceable
        )));
    }

    let request_no = generate_invoice_request_no(&state).await?;

    let id: i64 = sqlx::query_scalar(
        &state.db.format_query(
            "INSERT INTO user_invoice_requests \
             (request_no, user_id, invoice_type, title_type, title, tax_number, company_address, company_phone, bank_name, bank_account, amount, email, remark, status) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending') \
             RETURNING id"
        )
    )
    .bind(&request_no)
    .bind(user_id)
    .bind(&payload.invoice_type)
    .bind(&payload.title_type)
    .bind(payload.title.trim())
    .bind(payload.tax_number.as_deref().map(str::trim))
    .bind(payload.company_address.as_deref().map(str::trim))
    .bind(payload.company_phone.as_deref().map(str::trim))
    .bind(payload.bank_name.as_deref().map(str::trim))
    .bind(payload.bank_account.as_deref().map(str::trim))
    .bind(payload.amount)
    .bind(payload.email.trim())
    .bind(payload.remark.as_deref().map(str::trim))
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "id": id,
        "request_no": request_no,
        "message": "发票申请提交成功，请等待管理员审核处理"
    })))
}

pub async fn list_user_invoice_requests(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<Vec<UserInvoiceRequestRecord>>> {
    let records: Vec<UserInvoiceRequestRecord> = sqlx::query_as(&state.db.format_query(
        "SELECT * FROM user_invoice_requests WHERE user_id = ? ORDER BY created_at DESC",
    ))
    .bind(&claims.sub)
    .fetch_all(&state.db.pool)
    .await?;

    Ok(Json(records))
}

pub async fn cancel_user_invoice_request(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = &claims.sub;

    let existing: Option<(String, String)> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT status, user_id FROM user_invoice_requests WHERE id = ?"),
    )
    .bind(id)
    .fetch_optional(&state.db.pool)
    .await?;

    let Some((current_status, record_user_id)) = existing else {
        return Err(AppError::NotFound("发票申请记录不存在".to_string()));
    };

    if &record_user_id != user_id {
        return Err(AppError::Forbidden("无权操作此发票申请".to_string()));
    }

    if current_status != "pending" {
        return Err(AppError::BadRequest(
            "仅支持取消审核中的发票申请".to_string(),
        ));
    }

    sqlx::query(&state.db.format_query(
        "UPDATE user_invoice_requests SET \
             status = 'cancelled', \
             updated_at = now() \
             WHERE id = ? AND user_id = ? AND status = 'pending'",
    ))
    .bind(id)
    .bind(user_id)
    .execute(&state.db.pool)
    .await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "开票申请已成功取消，额度已释放"
    })))
}

pub async fn admin_list_invoice_requests(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AdminInvoiceListParams>,
) -> AppResult<Json<AdminInvoiceListResponse>> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let base_from = "FROM user_invoice_requests r LEFT JOIN users u ON r.user_id = u.id";
    let mut where_clause = " WHERE 1=1".to_string();
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref st) = params.status {
        let st_trim = st.trim();
        if !st_trim.is_empty() && st_trim != "all" {
            where_clause.push_str(" AND r.status = ?");
            binds.push(st_trim.to_string());
        }
    }

    if let Some(ref it) = params.invoice_type {
        let it_trim = it.trim();
        if !it_trim.is_empty() && it_trim != "all" {
            where_clause.push_str(" AND r.invoice_type = ?");
            binds.push(it_trim.to_string());
        }
    }

    if let Some(ref tt) = params.title_type {
        let tt_trim = tt.trim();
        if !tt_trim.is_empty() && tt_trim != "all" {
            where_clause.push_str(" AND r.title_type = ?");
            binds.push(tt_trim.to_string());
        }
    }

    if let Some(ref uid) = params.user_id {
        let u_trim = uid.trim();
        if !u_trim.is_empty() && u_trim != "all" {
            where_clause.push_str(" AND (r.user_id = ? OR u.uid = ?)");
            binds.push(u_trim.to_string());
            binds.push(u_trim.to_string());
        }
    }

    if let Some(ref start) = params.start_date {
        let s = start.trim();
        if !s.is_empty() {
            crate::api::date_helper::push_timestamptz_bound_default(
                &mut where_clause,
                &mut binds,
                "r.created_at",
                s,
                false,
            );
        }
    }

    if let Some(ref end) = params.end_date {
        let e = end.trim();
        if !e.is_empty() {
            crate::api::date_helper::push_timestamptz_bound_default(
                &mut where_clause,
                &mut binds,
                "r.created_at",
                e,
                true,
            );
        }
    }

    if let Some(ref kw) = params.keyword {
        let k = kw.trim();
        if !k.is_empty() {
            // 聚焦发票核心检索维度（抬头/姓名、税号、申请单号、发票号、用户名、UID），彻底保证模糊检索功能不回归
            where_clause.push_str(" AND (r.title ILIKE ? OR r.tax_number ILIKE ? OR r.request_no ILIKE ? OR r.invoice_number ILIKE ? OR u.username ILIKE ? OR u.uid ILIKE ?)");
            let pattern = format!("%{}%", k);
            for _ in 0..6 {
                binds.push(pattern.clone());
            }
        }
    }

    // 1. 查询统计指标（若指定 user_id 则仅统计该用户数据）
    let (stats_sql, stats_binds): (String, Vec<String>) = if let Some(ref uid) = params.user_id {
        let u_trim = uid.trim();
        if !u_trim.is_empty() && u_trim != "all" {
            (
                "SELECT \
                COUNT(*)::bigint AS total_count, \
                COUNT(*) FILTER (WHERE r.status = 'pending')::bigint AS pending_count, \
                COUNT(*) FILTER (WHERE r.status = 'approved')::bigint AS approved_count, \
                COUNT(*) FILTER (WHERE r.status = 'rejected')::bigint AS rejected_count, \
                COALESCE(SUM(r.amount) FILTER (WHERE r.status = 'approved'), 0.0) AS total_approved_amount, \
                COALESCE(SUM(r.amount) FILTER (WHERE r.status = 'approved' AND r.created_at >= date_trunc('month', now())), 0.0) AS this_month_approved_amount \
                FROM user_invoice_requests r LEFT JOIN users u ON r.user_id = u.id::text \
                WHERE (r.user_id = ? OR u.uid = ?)".to_string(),
                vec![u_trim.to_string(), u_trim.to_string()],
            )
        } else {
            (
                "SELECT \
                COUNT(*)::bigint AS total_count, \
                COUNT(*) FILTER (WHERE status = 'pending')::bigint AS pending_count, \
                COUNT(*) FILTER (WHERE status = 'approved')::bigint AS approved_count, \
                COUNT(*) FILTER (WHERE status = 'rejected')::bigint AS rejected_count, \
                COALESCE(SUM(amount) FILTER (WHERE status = 'approved'), 0.0) AS total_approved_amount, \
                COALESCE(SUM(amount) FILTER (WHERE status = 'approved' AND created_at >= date_trunc('month', now())), 0.0) AS this_month_approved_amount \
                FROM user_invoice_requests".to_string(),
                vec![],
            )
        }
    } else {
        (
            "SELECT \
            COUNT(*)::bigint AS total_count, \
            COUNT(*) FILTER (WHERE status = 'pending')::bigint AS pending_count, \
            COUNT(*) FILTER (WHERE status = 'approved')::bigint AS approved_count, \
            COUNT(*) FILTER (WHERE status = 'rejected')::bigint AS rejected_count, \
            COALESCE(SUM(amount) FILTER (WHERE status = 'approved'), 0.0) AS total_approved_amount, \
            COALESCE(SUM(amount) FILTER (WHERE status = 'approved' AND created_at >= date_trunc('month', now())), 0.0) AS this_month_approved_amount \
            FROM user_invoice_requests".to_string(),
            vec![],
        )
    };

    let formatted_stats_sql = state.db.format_query(&stats_sql);
    let mut stats_q = sqlx::query_as::<_, (i64, i64, i64, i64, f64, f64)>(&formatted_stats_sql);
    for b in &stats_binds {
        stats_q = stats_q.bind(b);
    }
    let stats_row: Option<(i64, i64, i64, i64, f64, f64)> =
        stats_q.fetch_optional(&state.db.pool).await.unwrap_or(None);

    let stats = match stats_row {
        Some((
            total_count,
            pending_count,
            approved_count,
            rejected_count,
            total_approved_amount,
            this_month_approved_amount,
        )) => InvoiceAuditStats {
            total_count,
            pending_count,
            approved_count,
            rejected_count,
            total_approved_amount,
            this_month_approved_amount,
        },
        None => InvoiceAuditStats::default(),
    };

    // 2. 查询当前条件总记录数
    let count_sql = format!("SELECT COUNT(*) {}{}", base_from, where_clause);
    let count_query = state.db.format_query(&count_sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
    for val in &binds {
        count_q = count_q.bind(val);
    }
    let total = count_q.fetch_one(&state.db.pool).await.unwrap_or(0);

    // 3. 查询当前页数据
    let select_fields = "r.id, r.request_no, r.user_id, u.username, u.email as user_email, u.nickname as user_nickname, u.uid as user_uid, \
        r.invoice_type, r.title_type, r.title, r.tax_number, r.company_address, r.company_phone, r.bank_name, r.bank_account, \
        r.amount, r.email, r.remark, r.status, r.reject_reason, r.invoice_file_url, r.invoice_code, r.invoice_number, \
        r.admin_remark, r.issued_at, r.operator_id, r.operator_name, r.created_at, r.updated_at";

    let list_sql = format!(
        "SELECT {} {}{} ORDER BY r.created_at DESC LIMIT {} OFFSET {}",
        select_fields, base_from, where_clause, per_page, offset
    );

    let list_query = state.db.format_query(&list_sql);
    let mut list_q = sqlx::query_as::<_, AdminInvoiceRequestItem>(&list_query);
    for val in &binds {
        list_q = list_q.bind(val);
    }
    let items = list_q.fetch_all(&state.db.pool).await.unwrap_or_default();

    Ok(Json(AdminInvoiceListResponse {
        items,
        total,
        page,
        per_page,
        stats,
    }))
}

pub async fn admin_update_invoice_request(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<AdminInvoiceAuditPayload>,
) -> AppResult<Json<serde_json::Value>> {
    let operator_id = claims.sub.clone();
    let operator_name = if !claims.username.is_empty() {
        claims.username.clone()
    } else {
        "Admin".to_string()
    };

    let existing: Option<(String, f64, String)> =
        sqlx::query_as(&state.db.format_query(
            "SELECT status, amount, user_id FROM user_invoice_requests WHERE id = ?",
        ))
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await?;

    let Some((_current_status, _amount, _user_id)) = existing else {
        return Err(AppError::NotFound("发票申请记录不存在".to_string()));
    };

    match payload.action.as_str() {
        "approve" => {
            let inv_number = payload
                .invoice_number
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if inv_number.is_none() {
                return Err(AppError::BadRequest("开具发票必须填写发票号码".to_string()));
            }

            sqlx::query(&state.db.format_query(
                "UPDATE user_invoice_requests SET \
                     status = 'approved', \
                     invoice_number = ?, \
                     invoice_code = ?, \
                     invoice_file_url = ?, \
                     admin_remark = ?, \
                     reject_reason = NULL, \
                     issued_at = now(), \
                     operator_id = ?, \
                     operator_name = ?, \
                     updated_at = now() \
                     WHERE id = ?",
            ))
            .bind(inv_number)
            .bind(
                payload
                    .invoice_code
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            )
            .bind(
                payload
                    .invoice_file_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            )
            .bind(
                payload
                    .admin_remark
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            )
            .bind(&operator_id)
            .bind(&operator_name)
            .bind(id)
            .execute(&state.db.pool)
            .await?;
        }
        "reject" => {
            let reason = payload
                .reject_reason
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if reason.is_none() {
                return Err(AppError::BadRequest("驳回申请必须填写驳回原因".to_string()));
            }

            sqlx::query(&state.db.format_query(
                "UPDATE user_invoice_requests SET \
                     status = 'rejected', \
                     reject_reason = ?, \
                     admin_remark = ?, \
                     operator_id = ?, \
                     operator_name = ?, \
                     updated_at = now() \
                     WHERE id = ?",
            ))
            .bind(reason)
            .bind(
                payload
                    .admin_remark
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            )
            .bind(&operator_id)
            .bind(&operator_name)
            .bind(id)
            .execute(&state.db.pool)
            .await?;
        }
        "update" => {
            sqlx::query(&state.db.format_query(
                "UPDATE user_invoice_requests SET \
                     invoice_number = COALESCE(?, invoice_number), \
                     invoice_code = ?, \
                     invoice_file_url = ?, \
                     admin_remark = ?, \
                     reject_reason = COALESCE(?, reject_reason), \
                     operator_id = ?, \
                     operator_name = ?, \
                     updated_at = now() \
                     WHERE id = ?",
            ))
            .bind(
                payload
                    .invoice_number
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            )
            .bind(
                payload
                    .invoice_code
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            )
            .bind(
                payload
                    .invoice_file_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            )
            .bind(
                payload
                    .admin_remark
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            )
            .bind(
                payload
                    .reject_reason
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            )
            .bind(&operator_id)
            .bind(&operator_name)
            .bind(id)
            .execute(&state.db.pool)
            .await?;
        }
        _ => {
            return Err(AppError::BadRequest("不支持的发票处理操作".to_string()));
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "发票处理成功"
    })))
}
