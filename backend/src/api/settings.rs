/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::admin_permission::{require_edit, AdminContext};
use crate::error::{AppError, AppResult};
use crate::models::{
    build_public_payment_channels, merge_payment_channels_ui, public_payment_status_from_channels,
    AgreementSettings, AllSettings, CurrencySettings, DatabaseSettings, GoogleOAuthSettings,
    InvoiceSettings, LogCleanupSettings, LoginSettings, MarketingSettings, PaymentAlipaySettings,
    PaymentAllinpaySettings, PaymentChannelsUiSettings, PaymentGatewayEnableFlags,
    PaymentHyperbcSettings, PaymentStripeSettings, PaymentWechatSettings, PublicMarketingSettings,
    PublicNotificationSettings, PublicRegistrationSettings, PublicSettings, RegistrationSettings,
    RelaySettings, SMTPSettings, SiteSettings, SmsSettings, StorageSettings, UpdateSettingsRequest,
    WechatOAuthSettings,
};
use crate::AppState;
use axum::{
    extract::{Extension, State},
    Json,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

// ════════════════════════════════════════════════════════════════════════════
// 【安全原则】公开接口绝不暴露隐私数据（密钥、密码、Secret、数据库信息等）。
// get_public_settings 仅返回前端 UI 渲染所需的最小安全数据集。
// get_settings 返回完整设置，仅限管理员访问（通过 admin_middleware 保护）。
// 此原则必须被所有开发者（包括 AI）严格遵守。
// ════════════════════════════════════════════════════════════════════════════

/// 公开接口 — 返回前端 UI 渲染所需的安全配置，不含任何密钥/密码
pub async fn get_public_settings(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<PublicSettings>> {
    let mut site = get_setting(&state, "site_settings", default_site_settings()).await?;
    // 注册 IP 名单只给管理端，公开接口不带子公司内网段
    site.ip_blacklist.clear();
    site.ip_whitelist.clear();
    let mut currency =
        get_setting(&state, "currency_settings", default_currency_settings()).await?;
    let fx_snapshot = crate::services::fx_rates::load_snapshot(&state).await;
    let now = chrono::Utc::now().timestamp();
    crate::services::fx_rates::derive_auxiliary(&mut currency, fx_snapshot.as_ref(), now);
    let login = get_setting(&state, "login_settings", default_login_settings()).await?;
    let registration: RegistrationSettings = get_setting(
        &state,
        "registration_settings",
        default_registration_settings(),
    )
    .await?;
    let marketing: MarketingSettings =
        get_setting(&state, "marketing_settings", default_marketing_settings()).await?;
    let agreement = get_setting(&state, "agreement_settings", default_agreement_settings()).await?;
    let mut menu_config = get_setting(
        &state,
        "menu_config_settings",
        default_menu_config_settings(),
    )
    .await?;
    menu_config
        .items
        .retain(|item| item.key != "/assets" && item.key != "/assets-intl");
    let notification: crate::models::NotificationSettings =
        get_setting(&state, "notification_settings", Default::default()).await?;

    // OAuth 仅提取 client_id / app_id，不暴露 secret
    let wechat_oauth_setting =
        get_setting::<Option<crate::models::WechatOAuthSettings>>(&state, "wechat_oauth", None)
            .await?;
    let wechat_oauth_app_id = wechat_oauth_setting
        .as_ref()
        .filter(|w| !w.app_id.is_empty())
        .map(|w| w.app_id.clone());
    let wechat_oauth_callback_domain = wechat_oauth_setting
        .as_ref()
        .map(|w| w.callback_domain.trim())
        .filter(|s| !s.is_empty())
        .map(crate::api::auth::normalize_origin);
    let google_oauth_client_id =
        get_setting::<Option<crate::models::GoogleOAuthSettings>>(&state, "google_oauth", None)
            .await?
            .and_then(|g| {
                if g.client_id.is_empty() {
                    None
                } else {
                    Some(g.client_id)
                }
            });

    // 支付渠道仅提取 enabled 开关与展示元数据，不暴露任何密钥
    let wechat_cfg =
        get_setting::<Option<PaymentWechatSettings>>(&state, "payment_wechat", None).await?;
    let alipay_cfg =
        get_setting::<Option<PaymentAlipaySettings>>(&state, "payment_alipay", None).await?;
    let stripe_cfg =
        get_setting::<Option<PaymentStripeSettings>>(&state, "payment_stripe", None).await?;
    let hyperbc_cfg =
        get_setting::<Option<PaymentHyperbcSettings>>(&state, "payment_hyperbc", None).await?;
    let allinpay_cfg =
        get_setting::<Option<PaymentAllinpaySettings>>(&state, "payment_allinpay", None).await?;
    let gateway = PaymentGatewayEnableFlags {
        wechat: wechat_cfg.as_ref().map_or(false, |p| p.enabled),
        alipay: alipay_cfg.as_ref().map_or(false, |p| p.enabled),
        stripe: stripe_cfg.as_ref().map_or(false, |p| p.enabled),
        hyperbc: hyperbc_cfg.as_ref().map_or(false, |p| p.enabled),
        allinpay: allinpay_cfg.as_ref().map_or(false, |p| p.enabled),
    };
    let channels_ui = merge_payment_channels_ui(
        get_setting::<Option<PaymentChannelsUiSettings>>(&state, "payment_channels_ui", None)
            .await?,
        &gateway,
    );
    let payment_channels = build_public_payment_channels(&channels_ui, &gateway);
    #[cfg(all(plugin_pay, plugin_payment))]
    let payment_channels = crate::api::plugins::pay::annotate_public_channels(
        payment_channels,
        &channels_ui,
        &currency,
        fx_snapshot.as_ref(),
        now,
    );
    let payment = public_payment_status_from_channels(&payment_channels);
    // 加价比例、手动汇率等仅管理端可见
    currency.fx = Default::default();

    let invoices = get_setting(
        &state,
        "invoice_settings",
        crate::models::InvoiceSettings::default(),
    )
    .await
    .ok()
    .map(|mut inv| {
        inv.clear_placeholder_issuer();
        inv
    });

    Ok(Json(PublicSettings {
        is_open_source: cfg!(not(feature = "commercial_plugins")),
        site,
        currency,
        login,
        registration: PublicRegistrationSettings::from(&registration),
        marketing: PublicMarketingSettings::from(&marketing),
        payment,
        payment_channels,
        agreement,
        wechat_oauth_app_id,
        wechat_oauth_callback_domain,
        google_oauth_client_id,
        menu_config: Some(menu_config),
        notification: PublicNotificationSettings::from(&notification),
        invoices,
    }))
}

/// 管理员专属接口 — 返回完整设置（含所有密钥），需 admin_middleware 保护
/// 支付网关密钥（payment_*）仅对具备 `settings.payment` 查看权限的管理员返回
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<crate::auth::Claims>,
) -> AppResult<Json<AllSettings>> {
    let ctx = crate::admin_permission::load_admin_context(&state, &claims.sub).await?;
    let mut all = load_all_settings(&state).await?;
    redact_payment_secrets(&ctx, &mut all);
    // timesystem 固定 UTC，与站点运营/管理展示时区解耦
    all.server_timezone = Some(crate::time_system::TIMESYSTEM_TZ.to_string());
    all.server_time = Some(crate::time_system::utc_naive_string());
    Ok(Json(all))
}

/// 无「支付设置」查看权限的管理员不下发任何支付网关配置（私钥 / API Key / Webhook Secret）
fn redact_payment_secrets(ctx: &AdminContext, all: &mut AllSettings) {
    if crate::admin_permission::can_view(ctx, "settings.payment") {
        return;
    }
    all.payment_wechat = None;
    all.payment_alipay = None;
    all.payment_stripe = None;
    all.payment_hyperbc = None;
    all.payment_allinpay = None;
}

fn require_settings_fields(ctx: &AdminContext, req: &UpdateSettingsRequest) -> AppResult<()> {
    if req.site.is_some()
        || req.login.is_some()
        || req.registration.is_some()
        || req.agreement.is_some()
        || req.menu_config.is_some()
        || req.relay.is_some()
    {
        require_edit(ctx, "settings.basic")?;
    }
    if req.currency.is_some()
        || req.payment_wechat.is_some()
        || req.payment_alipay.is_some()
        || req.payment_stripe.is_some()
        || req.payment_hyperbc.is_some()
        || req.payment_allinpay.is_some()
        || req.payment_channels_ui.is_some()
        || req.invoices.is_some()
    {
        require_edit(ctx, "settings.payment")?;
    }
    if req.smtp.is_some() || req.sms.is_some() || req.notification.is_some() {
        require_edit(ctx, "settings.message_notification")?;
    }
    if req.google_oauth.is_some() || req.wechat_oauth.is_some() {
        require_edit(ctx, "settings.oauth")?;
    }
    if req.storage.is_some() || req.log_cleanup.is_some() {
        require_edit(ctx, "settings.database")?;
    }
    if let Some(v) = &req.marketing {
        let only_redemption = v
            .as_object()
            .is_some_and(|o| o.len() == 1 && o.contains_key("enable_redemption"));
        require_edit(
            ctx,
            if only_redemption {
                "marketing.redemptions"
            } else {
                "marketing.registration_gifts"
            },
        )?;
    }
    Ok(())
}

fn list_has_entry(items: &[String]) -> bool {
    items.iter().any(|s| !s.trim().is_empty())
}

fn validate_site_ip_lists(site: &SiteSettings) -> AppResult<()> {
    if site.ip_blacklist_enabled && !list_has_entry(&site.ip_blacklist) {
        return Err(AppError::BadRequest(
            "已开启注册 IP 黑名单，请至少填写一条 IP 或网段".to_string(),
        ));
    }
    if site.ip_whitelist_enabled && !list_has_entry(&site.ip_whitelist) {
        return Err(AppError::BadRequest(
            "已开启注册 IP 白名单，请至少填写一条 IP 或网段".to_string(),
        ));
    }
    Ok(())
}

fn validate_email_whitelist_entries(reg: &RegistrationSettings) -> AppResult<()> {
    if reg.email_whitelist_enabled && !list_has_entry(&reg.email_whitelist) {
        return Err(AppError::BadRequest(
            "已开启邮箱白名单，请至少填写一个允许的邮箱域名".to_string(),
        ));
    }
    Ok(())
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<AdminContext>,
    Json(request): Json<UpdateSettingsRequest>,
) -> AppResult<Json<AllSettings>> {
    require_settings_fields(&ctx, &request)?;
    let mut currency_or_site_changed = false;
    if let Some(v) = request.site.as_ref() {
        let current =
            get_setting::<SiteSettings>(&state, "site_settings", default_site_settings()).await?;
        let merged = merge_patch(&current, v)?;
        validate_site_ip_lists(&merged)?;
        let saved =
            merge_and_save_setting(&state, "site_settings", v, default_site_settings()).await?;
        crate::relay::relay_settings::put_cached_site_timezones(&saved);
        notify_schedule_changed();
        currency_or_site_changed = true;
        if let Ok(agreement) = get_setting::<AgreementSettings>(
            &state,
            "agreement_settings",
            default_agreement_settings(),
        )
        .await
        {
            let _ = crate::api::legal::generate_and_save_legal_static_files(
                &state.config.data_dir,
                &agreement,
                &saved,
            )
            .await;
        }
    }
    if let Some(mut v) = request.currency {
        // 符号 / 单位由货币目录决定，辅助货币由汇率推导，均不接受客户端写入
        if let Some(obj) = v.as_object_mut() {
            for key in ["currency_symbol", "currency_unit", "auxiliary_currencies"] {
                obj.remove(key);
            }
        }
        let fx_in_patch = v.get("fx").is_some();
        let mut prev =
            get_setting(&state, "currency_settings", default_currency_settings()).await?;
        crate::services::fx_rates::seed_legacy_auxiliary(&mut prev);
        let mut next = merge_patch(&prev, &v)?;
        crate::services::fx_rates::normalize_currency_settings(&prev, &mut next, fx_in_patch)
            .map_err(AppError::BadRequest)?;
        save_setting(&state, "currency_settings", &next).await?;
        currency_or_site_changed = true;
    }
    if let Some(v) = request.login {
        merge_and_save_setting(&state, "login_settings", &v, default_login_settings()).await?;
    }
    if let Some(v) = request.registration.as_ref() {
        let current = get_setting::<RegistrationSettings>(
            &state,
            "registration_settings",
            default_registration_settings(),
        )
        .await?;
        let merged = merge_patch(&current, v)?;
        validate_email_whitelist_entries(&merged)?;
        merge_and_save_setting(
            &state,
            "registration_settings",
            v,
            default_registration_settings(),
        )
        .await?;
    }
    if let Some(v) = request.smtp {
        merge_and_save_setting(&state, "smtp_settings", &v, default_smtp_settings()).await?;
    }
    // 短信余额提醒开关与余额模板 ID 跨配置校验（保存前合并预览，避免开了却发不了）
    if request.sms.is_some() || request.notification.is_some() {
        let current_sms =
            get_setting::<SmsSettings>(&state, "sms_settings", default_sms_settings()).await?;
        let current_notif = get_setting::<crate::models::NotificationSettings>(
            &state,
            "notification_settings",
            crate::models::NotificationSettings::default(),
        )
        .await?;
        let effective_sms = match &request.sms {
            Some(v) => merge_patch(&current_sms, v)?,
            None => current_sms,
        };
        let effective_notif = match &request.notification {
            Some(v) => merge_patch(&current_notif, v)?,
            None => current_notif,
        };
        crate::services::sms::ensure_balance_sms_config(
            effective_notif.sms_balance_notification,
            &effective_sms,
        )?;
    }
    if let Some(v) = request.sms {
        merge_and_save_setting(&state, "sms_settings", &v, default_sms_settings()).await?;
    }
    if let Some(v) = request.marketing {
        merge_and_save_setting(
            &state,
            "marketing_settings",
            &v,
            default_marketing_settings(),
        )
        .await?;
    }
    if request.database.is_some() {
        return Err(AppError::BadRequest(
            "数据库连接仅供查看，不能在后台修改。请设置环境变量 DATABASE_URL，或写入数据目录 .database_url 后重启。".to_string(),
        ));
    }
    if let Some(v) = request.payment_wechat {
        merge_and_save_setting::<PaymentWechatSettings>(
            &state,
            "payment_wechat",
            &v,
            Default::default(),
        )
        .await?;
    }
    if let Some(v) = request.payment_alipay {
        merge_and_save_setting::<PaymentAlipaySettings>(
            &state,
            "payment_alipay",
            &v,
            Default::default(),
        )
        .await?;
    }
    if let Some(v) = request.payment_stripe {
        merge_and_save_setting::<PaymentStripeSettings>(
            &state,
            "payment_stripe",
            &v,
            Default::default(),
        )
        .await?;
    }
    if let Some(v) = request.payment_hyperbc {
        merge_and_save_setting::<PaymentHyperbcSettings>(
            &state,
            "payment_hyperbc",
            &v,
            Default::default(),
        )
        .await?;
    }
    if let Some(v) = request.payment_allinpay {
        merge_and_save_setting::<PaymentAllinpaySettings>(
            &state,
            "payment_allinpay",
            &v,
            Default::default(),
        )
        .await?;
    }
    if let Some(v) = request.payment_channels_ui {
        reject_invalid_channel_recharge_limits(&v)?;
        merge_and_save_setting::<PaymentChannelsUiSettings>(
            &state,
            "payment_channels_ui",
            &v,
            Default::default(),
        )
        .await?;
    }
    if let Some(v) = request.google_oauth {
        merge_and_save_setting::<GoogleOAuthSettings>(
            &state,
            "google_oauth",
            &v,
            Default::default(),
        )
        .await?;
    }
    if let Some(v) = request.wechat_oauth {
        merge_and_save_setting::<WechatOAuthSettings>(
            &state,
            "wechat_oauth",
            &v,
            Default::default(),
        )
        .await?;
    }
    if let Some(v) = request.agreement {
        let saved_agreement = merge_and_save_setting(
            &state,
            "agreement_settings",
            &v,
            default_agreement_settings(),
        )
        .await?;
        let site =
            get_setting::<SiteSettings>(&state, "site_settings", default_site_settings()).await?;
        let _ = crate::api::legal::generate_and_save_legal_static_files(
            &state.config.data_dir,
            &saved_agreement,
            &site,
        )
        .await;
    }
    if let Some(v) = request.storage {
        merge_and_save_setting::<StorageSettings>(
            &state,
            "storage_settings",
            &v,
            Default::default(),
        )
        .await?;
    }
    if let Some(v) = request.log_cleanup {
        merge_and_save_setting::<LogCleanupSettings>(
            &state,
            "log_cleanup_settings",
            &v,
            Default::default(),
        )
        .await?;
        notify_schedule_changed();
    }
    if let Some(v) = request.menu_config {
        merge_and_save_setting(
            &state,
            "menu_config_settings",
            &v,
            default_menu_config_settings(),
        )
        .await?;
    }
    if let Some(v) = request.notification {
        merge_and_save_setting(
            &state,
            "notification_settings",
            &v,
            crate::models::NotificationSettings::default(),
        )
        .await?;
    }
    if let Some(v) = request.relay {
        let saved = save_relay_settings(&state, &v).await?;
        crate::relay::relay_settings::put_cached_relay_settings(saved);
    }
    if let Some(v) = request.invoices {
        let mut saved = merge_and_save_setting::<InvoiceSettings>(
            &state,
            "invoice_settings",
            &v,
            Default::default(),
        )
        .await?;
        saved.clear_placeholder_issuer();
        save_setting(&state, "invoice_settings", &saved).await?;
    }

    if currency_or_site_changed {
        crate::api::plugins::notify_marketplace_data_changed(&state).await;
    }

    let mut all = load_all_settings(&state).await?;
    redact_payment_secrets(&ctx, &mut all);
    // timesystem 固定 UTC，与站点运营/管理展示时区解耦
    all.server_timezone = Some(crate::time_system::TIMESYSTEM_TZ.to_string());
    all.server_time = Some(crate::time_system::utc_naive_string());
    Ok(Json(all))
}

/// 发送测试邮件
pub async fn test_email(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let to = body["to"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("缺少收件邮箱 to".to_string()))?;
    let smtp =
        get_setting::<SMTPSettings>(&state, "smtp_settings", default_smtp_settings()).await?;
    let svc = crate::services::email::EmailService::new(&smtp)?;
    svc.send_test_email(to).await?;
    Ok(Json(
        serde_json::json!({"success": true, "message": "测试邮件发送成功"}),
    ))
}

/// 发送测试短信
pub async fn test_sms(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let mobile = body["mobile"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("缺少手机号 mobile".to_string()))?;
    let sms = get_setting::<SmsSettings>(&state, "sms_settings", default_sms_settings()).await?;
    if !sms.credentials_configured() {
        return Err(AppError::BadRequest("请先完善短信通知配置".to_string()));
    }
    let svc = crate::services::sms::SmsService::new(&sms);
    let sent = svc.send_verification_code(mobile, "666666").await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "message": sent.accepted_message(mobile, "测试短信"),
        "serial_no": sent.serial_no,
        "request_id": sent.request_id,
        "phone": sent.phone,
    })))
}

/// 测试发送余额不足提醒（邮件 / 短信）
/// body: { channel: "email"|"sms", to?: email, mobile?: phone, balance?: "88.0000", threshold?: "100.0000",
///         subject?: "...", html?: "..." }
/// — balance/threshold/subject/html 仅邮件用；短信为无变量模板，只传 mobile
pub async fn test_low_balance_notification(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let channel = body["channel"]
        .as_str()
        .unwrap_or("email")
        .to_ascii_lowercase();
    let balance = body["balance"].as_str().unwrap_or("88.0000").to_string();
    let threshold = body["threshold"].as_str().unwrap_or("100.0000").to_string();

    let notif = get_setting::<crate::models::NotificationSettings>(
        &state,
        "notification_settings",
        crate::models::NotificationSettings::default(),
    )
    .await?;

    match channel.as_str() {
        "email" => {
            let to = body["to"]
                .as_str()
                .ok_or_else(|| AppError::BadRequest("缺少收件邮箱 to".to_string()))?;
            let smtp =
                get_setting::<SMTPSettings>(&state, "smtp_settings", default_smtp_settings())
                    .await?;
            if smtp.host.trim().is_empty() || smtp.from_address.trim().is_empty() {
                return Err(AppError::BadRequest(
                    "请先在「邮件通知」中完善 SMTP 配置".to_string(),
                ));
            }
            let subject_tpl = body["subject"]
                .as_str()
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| {
                    if notif.low_balance_email_subject.trim().is_empty() {
                        crate::models::default_low_balance_email_subject()
                    } else {
                        notif.low_balance_email_subject.clone()
                    }
                });
            let html_tpl = body["html"]
                .as_str()
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| {
                    if notif.low_balance_email_html.trim().is_empty() {
                        crate::models::default_low_balance_email_html()
                    } else {
                        notif.low_balance_email_html.clone()
                    }
                });
            let svc = crate::services::email::EmailService::new(&smtp)?;
            svc.send_low_balance_alert(to, &balance, &threshold, &subject_tpl, &html_tpl)
                .await?;
            Ok(Json(serde_json::json!({
                "success": true,
                "message": "余额提醒测试邮件已发送"
            })))
        }
        "sms" => {
            let mobile = body["mobile"]
                .as_str()
                .ok_or_else(|| AppError::BadRequest("缺少手机号 mobile".to_string()))?;
            let sms =
                get_setting::<SmsSettings>(&state, "sms_settings", default_sms_settings()).await?;
            if !sms.credentials_configured() {
                return Err(AppError::BadRequest(
                    "请先在「短信通知」中完善短信配置".to_string(),
                ));
            }
            crate::services::sms::ensure_balance_sms_config(true, &sms)?;
            let svc = crate::services::sms::SmsService::new(&sms);
            let sent = svc
                .send_balance_alert(mobile, sms.balance_template_id_effective())
                .await?;
            Ok(Json(serde_json::json!({
                "success": true,
                "message": sent.accepted_message(mobile, "余额提醒测试短信"),
                "serial_no": sent.serial_no,
                "request_id": sent.request_id,
                "phone": sent.phone,
            })))
        }
        _ => Err(AppError::BadRequest(
            "channel 仅支持 email 或 sms".to_string(),
        )),
    }
}

pub async fn verify_database(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<serde_json::Value>> {
    let url = match parse_postgres_url(&state.config.database_url) {
        Some(settings) => build_postgres_url(&settings),
        None => state.config.database_url.clone(),
    };
    match PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&url)
        .await
    {
        Ok(_) => Ok(Json(
            serde_json::json!({"success": true, "message": "连接成功"}),
        )),
        Err(e) => Ok(Json(
            serde_json::json!({"success": false, "message": format!("连接失败: {}", e)}),
        )),
    }
}

fn format_uptime_zh(secs: i64) -> String {
    let secs = secs.max(0);
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    if days > 0 {
        format!("{days} 天 {hours} 小时 {minutes} 分钟")
    } else if hours > 0 {
        format!("{hours} 小时 {minutes} 分钟")
    } else {
        format!("{minutes} 分钟")
    }
}

fn cache_hit_pct(hit: i64, read: i64) -> Option<f64> {
    let total = hit.saturating_add(read);
    if total <= 0 {
        None
    } else {
        Some((hit as f64 / total as f64) * 100.0)
    }
}

/// 当前库状态：目录 + pg_stat_database 共享内存快照（按需一次查询，不扫业务表）
#[derive(sqlx::FromRow)]
struct DatabaseInfoRow {
    database_name: String,
    server_version: String,
    started_at_utc: String,
    uptime_secs: i64,
    size_bytes: i64,
    size_pretty: String,
    table_count: i64,
    encoding: String,
    backends: i64,
    max_connections: i64,
    xact_commit: i64,
    xact_rollback: i64,
    blks_hit: i64,
    blks_read: i64,
    deadlocks: i64,
    temp_bytes: i64,
    stats_reset_utc: Option<String>,
}

pub async fn database_info(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<serde_json::Value>> {
    let row: DatabaseInfoRow = sqlx::query_as(
        "SELECT \
            current_database() AS database_name, \
            current_setting('server_version') AS server_version, \
            to_char(pg_postmaster_start_time() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS started_at_utc, \
            EXTRACT(EPOCH FROM (now() - pg_postmaster_start_time()))::bigint AS uptime_secs, \
            s.size_bytes, \
            pg_size_pretty(s.size_bytes) AS size_pretty, \
            (SELECT count(*)::bigint \
               FROM pg_catalog.pg_class c \
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
              WHERE n.nspname = 'public' AND c.relkind = 'r') AS table_count, \
            pg_encoding_to_char(d.encoding) AS encoding, \
            st.numbackends::bigint AS backends, \
            current_setting('max_connections')::bigint AS max_connections, \
            st.xact_commit::bigint AS xact_commit, \
            st.xact_rollback::bigint AS xact_rollback, \
            st.blks_hit::bigint AS blks_hit, \
            st.blks_read::bigint AS blks_read, \
            st.deadlocks::bigint AS deadlocks, \
            st.temp_bytes::bigint AS temp_bytes, \
            to_char(st.stats_reset AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS stats_reset_utc \
         FROM pg_catalog.pg_database d \
         CROSS JOIN LATERAL (SELECT pg_database_size(current_database()) AS size_bytes) s \
         JOIN pg_catalog.pg_stat_database st ON st.datname = d.datname \
         WHERE d.datname = current_database()",
    )
    .fetch_one(&state.db.pool)
    .await
    .map_err(|e| AppError::Internal(format!("读取数据库状态失败: {e}")))?;

    let cache_hit = cache_hit_pct(row.blks_hit, row.blks_read);
    let process_uptime_secs = crate::services::runtime_info::process_uptime_secs();

    Ok(Json(serde_json::json!({
        "database_name": row.database_name,
        "server_version": row.server_version,
        "started_at_utc": row.started_at_utc,
        "uptime_secs": row.uptime_secs,
        "uptime": format_uptime_zh(row.uptime_secs),
        "size_bytes": row.size_bytes,
        "size_pretty": row.size_pretty,
        "table_count": row.table_count,
        "encoding": row.encoding,
        "backends": row.backends,
        "max_connections": row.max_connections,
        "xact_commit": row.xact_commit,
        "xact_rollback": row.xact_rollback,
        "cache_hit_pct": cache_hit.map(|v| (v * 10.0).round() / 10.0),
        "deadlocks": row.deadlocks,
        "temp_bytes": row.temp_bytes,
        "temp_pretty": bytes_pretty(row.temp_bytes),
        "stats_reset_utc": row.stats_reset_utc,
        "process_started_at_utc": crate::services::runtime_info::process_started_at_utc(),
        "process_uptime_secs": process_uptime_secs,
        "process_uptime": format_uptime_zh(process_uptime_secs),
        "pool_size": state.db.pool.size(),
        "pool_idle": state.db.pool.num_idle(),
    })))
}

fn bytes_pretty(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut n = bytes.max(0) as f64;
    let mut i = 0;
    while n >= 1024.0 && i < UNITS.len() - 1 {
        n /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes.max(0), UNITS[0])
    } else {
        format!("{n:.1} {}", UNITS[i])
    }
}

const DB_RESET_CONFIRM: &str = "确认清空当前数据";

fn is_db_reset_confirm(phrase: &str) -> bool {
    phrase.trim() == DB_RESET_CONFIRM
}

pub async fn initialize_database(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let phrase = body.get("confirm").and_then(|v| v.as_str()).unwrap_or("");
    if !is_db_reset_confirm(phrase) {
        return Err(AppError::BadRequest(
            "请输入确认文案：确认清空当前数据".to_string(),
        ));
    }

    let rebuild_err = {
        let mut conn = state
            .db
            .pool
            .acquire()
            .await
            .map_err(|e| AppError::Internal(format!("无法获取数据库连接: {e}")))?;
        let _ = sqlx::query(
            r#"SELECT pg_terminate_backend(pid)
               FROM pg_stat_activity
               WHERE datname = current_database()
                 AND pid <> pg_backend_pid()
                 AND backend_type = 'client backend'"#,
        )
        .execute(&mut *conn)
        .await;
        sqlx::query("DROP SCHEMA IF EXISTS public CASCADE")
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Internal(format!("清空数据库失败: {e}")))?;

        let mut rebuild_err = sqlx::query("CREATE SCHEMA public")
            .execute(&mut *conn)
            .await
            .err()
            .map(|e| format!("重建 public schema 失败: {e}"));
        if rebuild_err.is_none() {
            if let Err(e) = sqlx::query("GRANT ALL ON SCHEMA public TO CURRENT_USER")
                .execute(&mut *conn)
                .await
            {
                rebuild_err = Some(format!("授权 schema 失败: {e}"));
            } else {
                let _ = sqlx::query("GRANT ALL ON SCHEMA public TO public")
                    .execute(&mut *conn)
                    .await;
            }
        }
        rebuild_err
    };

    let migrate_err = if rebuild_err.is_none() {
        crate::db::migrations::run_pg(&state.db.pool)
            .await
            .err()
            .map(|e| e.to_string())
    } else {
        rebuild_err
    };

    if migrate_err.is_none() {
        let _ = crate::sync_registration_settings(&state.db, state.config.register_enabled).await;
    }
    state.reset_runtime_after_db_wipe().await;

    if let Some(e) = migrate_err {
        return Err(AppError::Internal(format!("重新初始化表结构失败: {e}")));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "当前数据库已清空并重建表结构。请重新设置超级管理员，流程与全新安装相同。"
    })))
}

pub async fn backup_database(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<serde_json::Value>> {
    let parsed = match parse_postgres_url(&state.config.database_url) {
        Some(s) => s,
        None => {
            return Ok(Json(serde_json::json!({
                "success": false,
                "message": "当前数据库连接地址无法解析，无法备份"
            })));
        }
    };

    if parsed.host.starts_with('-')
        || parsed.database.starts_with('-')
        || parsed.username.starts_with('-')
    {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": "不合法的连接参数，拒绝执行备份"
        })));
    }

    let backup_dir = format!("{}/backups", state.config.data_dir);
    if let Err(e) = tokio::fs::create_dir_all(&backup_dir).await {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": format!("无法创建备份目录: {}", e)
        })));
    }

    let stamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let output_path = format!("{backup_dir}/postgres_{stamp}.sql");
    let host = parsed.host.clone();
    let port = parsed.port.to_string();
    let username = parsed.username.clone();
    let database = parsed.database.clone();
    let password = parsed.password.clone();
    let sslmode = if parsed.ssl_mode {
        "require"
    } else {
        "disable"
    };
    let dump_path = output_path.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("pg_dump");
        cmd.arg("-h")
            .arg(&host)
            .arg("-p")
            .arg(&port)
            .arg("-U")
            .arg(&username)
            .arg("-d")
            .arg(&database)
            .arg("-f")
            .arg(&dump_path)
            .env("PGSSLMODE", sslmode)
            .env("PGCONNECT_TIMEOUT", "15");
        if !password.is_empty() {
            cmd.env("PGPASSWORD", password);
        }
        cmd.output()
    })
    .await;

    match result {
        Ok(Ok(out)) if out.status.success() => Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("数据库备份成功，保存在 {}", output_path)
        }))),
        Ok(Ok(out)) => {
            let err_str = String::from_utf8_lossy(&out.stderr);
            Ok(Json(serde_json::json!({
                "success": false,
                "message": format!("pg_dump 执行失败: {}", err_str)
            })))
        }
        Ok(Err(e)) => Ok(Json(serde_json::json!({
            "success": false,
            "message": format!("无法启动 pg_dump（需安装 postgresql-client）: {}", e)
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "success": false,
            "message": format!("备份任务异常: {}", e)
        }))),
    }
}

#[derive(serde::Deserialize)]
pub struct TestStorageConnectionRequest {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(flatten)]
    pub storage: StorageSettings,
}

pub async fn test_storage_connection(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<TestStorageConnectionRequest>,
) -> AppResult<Json<serde_json::Value>> {
    use crate::services::object_store::{ObjectStore, StoreKind};
    let kind = StoreKind::parse(
        req.provider
            .as_deref()
            .unwrap_or(&req.storage.default_provider),
    );
    let Some(store) = ObjectStore::from_settings_for(&req.storage, kind) else {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": "请先填写该存储的完整凭证（密钥、地域、Endpoint、Bucket）"
        })));
    };
    match store.test_connection().await {
        Ok(msg) => Ok(Json(serde_json::json!({"success": true, "message": msg}))),
        Err(e) => Ok(Json(
            serde_json::json!({"success": false, "message": format!("测试失败: {}", e)}),
        )),
    }
}

pub type ListStorageBucketsRequest = crate::services::object_store::ListBucketsQuery;

/// 用 AK/SK 调用 TOS 或 COS ListBuckets，返回桶名、地域和内外网地址。
pub async fn list_storage_buckets(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ListStorageBucketsRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let saved = crate::relay::tos_persist::load_storage_settings(&state).await;
    let saved_tos = saved
        .as_ref()
        .map(|s| (s.tos_access_key.as_str(), s.tos_secret_key.as_str()));
    let saved_cos = saved
        .as_ref()
        .map(|s| (s.cos_secret_id.as_str(), s.cos_secret_key.as_str()));
    match crate::services::object_store::list_buckets_auto(&req, saved_tos, saved_cos).await {
        Ok(buckets) => Ok(Json(serde_json::json!({ "success": true, "buckets": buckets }))),
        Err(e) => Ok(Json(serde_json::json!({ "success": false, "message": e }))),
    }
}

/// 基础设置图片（站点 Logo 等）上传到站点存储 `settings/`；`slot` 为图片用途，用作文件名前缀
pub async fn upload_settings_image(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<AdminContext>,
    mut multipart: axum::extract::Multipart,
) -> AppResult<Json<serde_json::Value>> {
    require_edit(&ctx, "settings.basic")?;
    let store = crate::relay::tos_persist::load_system_object_store(&state)
        .await
        .ok_or_else(|| {
            AppError::BadRequest(
                "上传图片需要先在「数据库/存储 → 存储设置」中配置对象存储".to_string(),
            )
        })?;

    let mut slot = String::new();
    let mut file: Option<(Vec<u8>, String, String)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("读取上传内容失败".to_string()))?
    {
        match field.name().unwrap_or("") {
            "slot" => slot = field.text().await.unwrap_or_default(),
            "file" => {
                let mime = field.content_type().unwrap_or("").to_string();
                let name = field.file_name().unwrap_or("").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|_| AppError::BadRequest("读取文件失败".to_string()))?;
                file = Some((data.to_vec(), mime, name));
            }
            _ => {}
        }
    }

    let (data, mime, name) =
        file.ok_or_else(|| AppError::BadRequest("请选择要上传的图片".to_string()))?;
    if !mime.starts_with("image/") {
        return Err(AppError::BadRequest("只支持上传图片格式的文件".to_string()));
    }
    if data.len() > 5 * 1024 * 1024 {
        return Err(AppError::BadRequest("图片大小不能超过 5MB".to_string()));
    }
    let ext = crate::services::object_store::upload_ext(&name, "png");
    let slot: String = slot
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(32)
        .collect();
    let object_key = store.site_key(
        crate::services::object_store::SiteFolder::Settings,
        &format!(
            "{}_{}.{}",
            if slot.is_empty() { "image" } else { &slot },
            chrono::Utc::now().timestamp_millis(),
            ext
        ),
    );

    let file_url = store
        .upload_file(&object_key, data, &mime, None)
        .await
        .map_err(|e| {
            tracing::warn!("基础设置图片上传失败 {}: {}", object_key, e);
            AppError::Internal("图片上传失败，请稍后重试".to_string())
        })?;

    Ok(Json(serde_json::json!({
        "file_url": file_url,
        "object_key": object_key,
    })))
}

// ======================== 内部工具函数 ========================

fn postgres_host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn build_postgres_url(settings: &DatabaseSettings) -> String {
    let ssl_mode = if settings.ssl_mode {
        "require"
    } else {
        "disable"
    };
    let port = if settings.port == 0 {
        5432
    } else {
        settings.port
    };
    let mut url = format!("postgres://{}", urlencoding::encode(&settings.username));
    if !settings.password.is_empty() {
        url.push(':');
        url.push_str(&urlencoding::encode(&settings.password));
    }
    url.push('@');
    url.push_str(&postgres_host_for_url(&settings.host));
    url.push(':');
    url.push_str(&port.to_string());
    url.push('/');
    url.push_str(&urlencoding::encode(&settings.database));
    url.push_str("?sslmode=");
    url.push_str(ssl_mode);
    url
}

fn parse_postgres_url(url: &str) -> Option<DatabaseSettings> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = reqwest::Url::parse(trimmed).ok()?;
    match parsed.scheme() {
        "postgres" | "postgresql" => {}
        _ => return None,
    }
    let host = parsed.host_str()?.to_string();
    let port = parsed.port().unwrap_or(5432);
    let raw_db = parsed.path().trim_start_matches('/');
    if raw_db.is_empty() {
        return None;
    }
    let database = urlencoding::decode(raw_db).ok()?.into_owned();
    let username = urlencoding::decode(parsed.username()).ok()?.into_owned();
    let password = match parsed.password() {
        Some(p) => urlencoding::decode(p).ok()?.into_owned(),
        None => String::new(),
    };
    let ssl_mode = parsed.query_pairs().any(|(k, v)| {
        k == "sslmode" && matches!(v.as_ref(), "require" | "verify-ca" | "verify-full")
    });
    Some(DatabaseSettings {
        db_type: "postgres".to_string(),
        host,
        port,
        database,
        username,
        password,
        ssl_mode,
    })
}

/// 加载全部设置（统一入口）
pub async fn load_all_settings(state: &Arc<AppState>) -> AppResult<AllSettings> {
    let payment_wechat: Option<PaymentWechatSettings> =
        get_setting(state, "payment_wechat", None).await?;
    let payment_alipay: Option<PaymentAlipaySettings> =
        get_setting(state, "payment_alipay", None).await?;
    let payment_stripe: Option<PaymentStripeSettings> =
        get_setting(state, "payment_stripe", None).await?;
    let payment_hyperbc: Option<PaymentHyperbcSettings> =
        get_setting(state, "payment_hyperbc", None).await?;
    let payment_allinpay: Option<PaymentAllinpaySettings> =
        get_setting(state, "payment_allinpay", None).await?;
    let gateway = PaymentGatewayEnableFlags {
        wechat: payment_wechat.as_ref().map_or(false, |p| p.enabled),
        alipay: payment_alipay.as_ref().map_or(false, |p| p.enabled),
        stripe: payment_stripe.as_ref().map_or(false, |p| p.enabled),
        hyperbc: payment_hyperbc.as_ref().map_or(false, |p| p.enabled),
        allinpay: payment_allinpay.as_ref().map_or(false, |p| p.enabled),
    };

    let site = get_setting(state, "site_settings", default_site_settings()).await?;

    let mut currency = get_setting(state, "currency_settings", default_currency_settings()).await?;
    let fx_snapshot = crate::services::fx_rates::load_snapshot(state).await;
    crate::services::fx_rates::derive_auxiliary(
        &mut currency,
        fx_snapshot.as_ref(),
        chrono::Utc::now().timestamp(),
    );

    Ok(AllSettings {
        site,
        currency,
        login: get_setting(state, "login_settings", default_login_settings()).await?,
        registration: get_setting(
            state,
            "registration_settings",
            default_registration_settings(),
        )
        .await?,
        smtp: get_setting(state, "smtp_settings", default_smtp_settings()).await?,
        sms: get_setting(state, "sms_settings", None).await?,
        marketing: get_setting(state, "marketing_settings", default_marketing_settings()).await?,
        database: {
            let stored =
                get_setting(state, "database_settings", default_database_settings()).await?;
            // 页面必须展示正在使用的连接（DATABASE_URL / data/.database_url），而不是从未保存过的默认值
            parse_postgres_url(&state.config.database_url).unwrap_or(stored)
        },
        payment_wechat,
        payment_alipay,
        payment_stripe,
        payment_hyperbc,
        payment_allinpay,
        payment_channels_ui: Some(merge_payment_channels_ui(
            get_setting(state, "payment_channels_ui", None).await?,
            &gateway,
        )),
        google_oauth: get_setting(state, "google_oauth", None).await?,
        wechat_oauth: get_setting(state, "wechat_oauth", None).await?,
        agreement: get_setting(state, "agreement_settings", default_agreement_settings()).await?,
        storage: get_setting(state, "storage_settings", None).await?,
        log_cleanup: Some(load_log_cleanup_settings(state).await?),
        menu_config: Some({
            let mut mc = get_setting(
                state,
                "menu_config_settings",
                default_menu_config_settings(),
            )
            .await?;
            mc.items
                .retain(|item| item.key != "/assets" && item.key != "/assets-intl");
            mc
        }),
        notification: get_setting(state, "notification_settings", Default::default()).await?,
        relay: get_setting(state, "relay_settings", default_relay_settings())
            .await?
            .prepared(),
        invoices: Some({
            let mut inv =
                get_setting(state, "invoice_settings", InvoiceSettings::default()).await?;
            inv.clear_placeholder_issuer();
            inv
        }),
        server_timezone: None,
        server_time: None,
    })
}

pub async fn load_log_cleanup_settings(state: &AppState) -> AppResult<LogCleanupSettings> {
    let val: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = ?"),
    )
    .bind("log_cleanup_settings")
    .fetch_optional(&state.db.pool)
    .await?;
    if let Some(v) = val {
        if let Ok(parsed) = serde_json::from_str::<LogCleanupSettings>(&v) {
            return Ok(parsed);
        }
    }
    let legacy: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = ?"),
    )
    .bind("storage_settings")
    .fetch_optional(&state.db.pool)
    .await?;
    Ok(legacy
        .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok())
        .map(|v| LogCleanupSettings::from_legacy_storage_json(&v))
        .unwrap_or_default())
}

static SCHEDULE_NOTIFIER: std::sync::OnceLock<tokio::sync::watch::Sender<u64>> =
    std::sync::OnceLock::new();

/// 通知定时运维任务配置已变更，唤醒当前休眠对齐新时间
pub fn notify_schedule_changed() {
    if let Some(tx) = SCHEDULE_NOTIFIER.get() {
        let _ = tx.send_modify(|v| *v = v.wrapping_add(1));
    }
}

/// 订阅定时运维调度变更通知通道
pub fn subscribe_schedule_changed() -> tokio::sync::watch::Receiver<u64> {
    SCHEDULE_NOTIFIER
        .get_or_init(|| {
            let (tx, _) = tokio::sync::watch::channel(0);
            tx
        })
        .subscribe()
}

pub(crate) async fn get_setting<T: serde::de::DeserializeOwned + Clone>(
    state: &Arc<AppState>,
    key: &str,
    default: T,
) -> AppResult<T> {
    let val: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = ?"),
    )
    .bind(key)
    .fetch_optional(&state.db.pool)
    .await?;
    if let Some(v) = val {
        Ok(serde_json::from_str(&v).unwrap_or(default))
    } else {
        Ok(default)
    }
}

async fn save_setting<T: serde::Serialize>(
    state: &Arc<AppState>,
    key: &str,
    value: &T,
) -> AppResult<()> {
    let val = serde_json::to_string(value).unwrap_or_default();
    sqlx::query(&state.db.format_query(
        "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = EXCLUDED.value"
    ))
    .bind(key)
    .bind(val)
    .execute(&state.db.pool)
    .await?;
    Ok(())
}

fn reject_invalid_channel_recharge_limits(v: &serde_json::Value) -> AppResult<()> {
    let Some(channels) = v.get("channels").and_then(|c| c.as_array()) else {
        return Ok(());
    };
    let mut parsed = Vec::with_capacity(channels.len());
    for channel in channels {
        let item: crate::models::PaymentChannelUiItem = serde_json::from_value(channel.clone())
            .map_err(|e| AppError::BadRequest(format!("支付渠道配置格式错误: {}", e)))?;
        parsed.push(item);
    }
    crate::models::validate_channel_recharge_limits(&parsed).map_err(AppError::BadRequest)
}

fn merge_json(old: &mut serde_json::Value, new: &serde_json::Value) {
    if let (Some(old_obj), Some(new_obj)) = (old.as_object_mut(), new.as_object()) {
        for (k, v) in new_obj {
            if v.is_object() {
                if !old_obj.contains_key(k) {
                    old_obj.insert(k.clone(), serde_json::json!({}));
                }
                merge_json(old_obj.get_mut(k).unwrap(), v);
            } else {
                old_obj.insert(k.clone(), v.clone());
            }
        }
    } else {
        *old = new.clone();
    }
}

/// 内存合并配置补丁（不落库），用于跨配置校验
fn merge_patch<T: serde::de::DeserializeOwned + serde::Serialize>(
    current: &T,
    patch: &serde_json::Value,
) -> AppResult<T> {
    let mut cur = serde_json::to_value(current)
        .map_err(|e| AppError::BadRequest(format!("配置序列化失败: {}", e)))?;
    merge_json(&mut cur, patch);
    serde_json::from_value(cur)
        .map_err(|e| AppError::BadRequest(format!("配置合并后数据格式错误: {}", e)))
}

async fn merge_and_save_setting<T: serde::de::DeserializeOwned + serde::Serialize + Clone>(
    state: &Arc<AppState>,
    key: &str,
    new_val_json: &serde_json::Value,
    default_val: T,
) -> AppResult<T> {
    let mut current_json = if let Ok(Some(v)) = sqlx::query_scalar::<_, String>(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = ?"),
    )
    .bind(key)
    .fetch_optional(&state.db.pool)
    .await
    {
        serde_json::from_str(&v).unwrap_or_else(|_| serde_json::to_value(&default_val).unwrap())
    } else {
        serde_json::to_value(&default_val).unwrap()
    };

    merge_json(&mut current_json, new_val_json);

    let final_struct: T = serde_json::from_value(current_json)
        .map_err(|e| AppError::BadRequest(format!("配置合并后数据格式错误: {}", e)))?;
    save_setting(state, key, &final_struct).await?;
    Ok(final_struct)
}

// ======================== 默认值函数 ========================

/// 首次安装初始化管理员时，只写入站点运营时区。
/// 不改 timesystem：进程 TZ、数据库 TIME ZONE、`server_timezone` 仍固定 UTC。
pub async fn apply_initial_site_timezone(
    state: &Arc<AppState>,
    timezone: Option<&str>,
) -> AppResult<()> {
    let Some(name) = timezone.and_then(crate::time_system::try_iana_timezone_name) else {
        return Ok(());
    };
    let patch = serde_json::json!({ "default_timezone": name });
    let saved =
        merge_and_save_setting(state, "site_settings", &patch, default_site_settings()).await?;
    crate::relay::relay_settings::put_cached_site_timezones(&saved);
    tracing::info!("Initial site operation timezone set to {name}");
    Ok(())
}

pub fn default_site_settings() -> SiteSettings {
    SiteSettings {
        name: "Tkeapi".to_string(),
        title: "Tkeapi - LLM API Gateway".to_string(),
        intro: String::new(),
        keywords: "LLM, API, Gateway, Rust".to_string(),
        description: "Next-gen LLM API Distribution & Management Platform".to_string(),
        favicon: String::new(),
        logo: String::new(),
        logo_title_url: String::new(),
        login_title: String::new(),
        login_title_url: String::new(),
        login_subtitle: String::new(),
        enable_multilingual: true,
        supported_languages: vec!["zh".to_string(), "en".to_string()],
        default_language: "zh".to_string(),
        default_timezone: crate::time_system::DEFAULT_TIMEDISPLAY.to_string(),
        admin_display_timezone: String::new(),
        show_timezone: true,
        enable_theme_toggle: true,
        default_theme: "dark".to_string(),
        copyright: "© 2026 TkeAPI. All rights reserved.".to_string(),
        admin_path: "admin1688".to_string(),
        login_style: "split".to_string(),
        login_quote: String::new(),
        ip_blacklist_enabled: false,
        ip_blacklist: Vec::new(),
        ip_whitelist_enabled: true,
        ip_whitelist: crate::models::settings::default_registration_ip_whitelist(),
    }
}

pub fn default_relay_settings() -> RelaySettings {
    RelaySettings::default()
}

/// 保存 relay：写入新 `inflight_limits` 时剥掉旧字段，避免 merge 把已关类别再次打开
async fn save_relay_settings(
    state: &Arc<AppState>,
    new_val: &serde_json::Value,
) -> AppResult<RelaySettings> {
    let mut current = sqlx::query_scalar::<_, String>(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = ?"),
    )
    .bind("relay_settings")
    .fetch_optional(&state.db.pool)
    .await?
    .and_then(|v| serde_json::from_str(&v).ok())
    .unwrap_or_else(|| serde_json::to_value(default_relay_settings()).unwrap_or_default());

    if new_val.get("inflight_limits").is_some() {
        if let Some(obj) = current.as_object_mut() {
            obj.remove("video_inflight_enabled");
            obj.remove("video_inflight_tiers");
        }
    }
    merge_json(&mut current, new_val);
    let saved = serde_json::from_value::<RelaySettings>(current)
        .map_err(|e| AppError::BadRequest(format!("配置合并后数据格式错误: {}", e)))?
        .prepared();
    save_setting(state, "relay_settings", &saved).await?;
    Ok(saved)
}

pub fn default_currency_settings() -> CurrencySettings {
    CurrencySettings {
        default_currency: "CNY".to_string(),
        currency_symbol: "¥".to_string(),
        currency_unit: "元".to_string(),
        token_ratio: 1.0,
        auxiliary_currencies: vec![],
        fx: Default::default(),
        quick_amounts: vec![20.0, 50.0, 100.0, 500.0, 1000.0, 5000.0],
        min_recharge_amount: 5.0,
        max_recharge_amount: 10000.0,
    }
}

pub async fn get_currency_settings(state: &crate::AppState) -> CurrencySettings {
    sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = 'currency_settings'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    .ok()
    .flatten()
    .and_then(|v: String| serde_json::from_str::<CurrencySettings>(&v).ok())
    .unwrap_or_else(default_currency_settings)
}

async fn build_currency_fx_overview(
    state: &Arc<AppState>,
) -> AppResult<crate::services::fx_rates::FxOverview> {
    let currency = get_setting(state, "currency_settings", default_currency_settings()).await?;
    let channels_ui = merge_payment_channels_ui(
        get_setting(state, "payment_channels_ui", None).await?,
        &PaymentGatewayEnableFlags::default(),
    );
    let snapshot = crate::services::fx_rates::load_snapshot(state).await;
    Ok(crate::services::fx_rates::build_overview(
        &currency,
        snapshot.as_ref(),
        &channels_ui,
        chrono::Utc::now().timestamp(),
    ))
}

/// 汇率概览：可选货币目录、实时汇率与各支付渠道收款币种
pub async fn get_currency_fx(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<crate::services::fx_rates::FxOverview>> {
    Ok(Json(build_currency_fx_overview(&state).await?))
}

/// 立即从免费汇率源拉取一次
pub async fn refresh_currency_fx(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<crate::services::fx_rates::FxOverview>> {
    crate::services::fx_rates::refresh_manually(&state)
        .await
        .map_err(AppError::BadRequest)?;
    Ok(Json(build_currency_fx_overview(&state).await?))
}

pub fn default_login_settings() -> LoginSettings {
    LoginSettings {
        enable_username_login: true,
        enable_mobile_login: false,
        enable_email_login: false,
        enable_wechat_login: false,
        enable_google_login: false,
    }
}

pub fn default_registration_settings() -> RegistrationSettings {
    RegistrationSettings {
        enable_username_registration: true,
        username_ip_daily_limit: Some(3),
        enable_email_registration: false,
        enable_mobile_registration: false,
        enable_password_recovery: false,
        ip_rate_limit_enabled: false,
        ip_daily_limit: 6,
        email_validation_strict: false,
        email_whitelist_enabled: false,
        email_whitelist: vec![
            "qq.com".to_string(),
            "163.com".to_string(),
            "outlook.com".to_string(),
            "aliyun.com".to_string(),
            "foxmail.com".to_string(),
        ],
        require_bind_mobile: false,
        require_bind_email: false,
        bind_enforcement: "all".to_string(),
        enable_user_kyc: false,
        max_accounts_per_email: 5,
        max_accounts_per_mobile: 5,
        max_accounts_per_wechat: 5,
        allow_international_mobile: false,
    }
}

pub fn default_smtp_settings() -> SMTPSettings {
    SMTPSettings {
        host: "smtp.example.com".to_string(),
        port: 465,
        username: "".to_string(),
        password: "".to_string(),
        from_address: "noreply@example.com".to_string(),
        from_name: "Tkeapi".to_string(),
    }
}

pub fn default_sms_settings() -> SmsSettings {
    SmsSettings::default()
}

pub fn default_marketing_settings() -> MarketingSettings {
    MarketingSettings {
        enable_registration_gift: false,
        enable_redemption: false,
        gift_mode: "fixed".to_string(),
        fixed_amount: 0.0,
        min_amount: 0.0,
        max_amount: 0.0,
    }
}

pub fn default_database_settings() -> DatabaseSettings {
    DatabaseSettings {
        db_type: "postgres".to_string(),
        host: "localhost".to_string(),
        port: 5432,
        database: "postgres".to_string(),
        username: "postgres".to_string(),
        password: "postgres".to_string(),
        ssl_mode: false,
    }
}

pub fn default_agreement_settings() -> AgreementSettings {
    AgreementSettings::default()
}

async fn collect_runtime(version: &str) -> serde_json::Value {
    let version = version.to_string();
    let version_fb = version.clone();
    tokio::task::spawn_blocking(move || crate::services::runtime_info::collect(&version))
        .await
        .unwrap_or_else(|_| crate::services::runtime_info::fallback(&version_fb))
}

fn format_commit_date(raw_date: &str) -> String {
    let trimmed = raw_date.trim();
    if trimmed.is_empty() {
        return "-".to_string();
    }
    if trimmed.contains("(UTC") || trimmed.ends_with("UTC") {
        return trimmed.to_string();
    }
    if let Some((dt, tz)) = trimmed.rsplit_once(' ') {
        if (tz.starts_with('+') || tz.starts_with('-')) && (tz.len() == 5 || tz.len() == 6) {
            let sign = &tz[0..1];
            let rest = &tz[1..];
            let parts: Vec<&str> = if rest.contains(':') {
                rest.split(':').collect()
            } else if rest.len() == 4 {
                vec![&rest[0..2], &rest[2..4]]
            } else {
                vec![]
            };
            if parts.len() == 2 {
                let hours: u32 = parts[0].parse().unwrap_or(0);
                let minutes: u32 = parts[1].parse().unwrap_or(0);
                let tz_suffix = if hours == 0 && minutes == 0 {
                    "(UTC)".to_string()
                } else if minutes == 0 {
                    format!("(UTC{}{})", sign, hours)
                } else {
                    format!("(UTC{}{}:{:02})", sign, hours, minutes)
                };
                return format!("{} {}", dt, tz_suffix);
            }
        }
    }
    trimmed.to_string()
}

pub async fn system_about() -> AppResult<Json<serde_json::Value>> {
    // 优先动态调用 git log 获取最新提交记录（无论 debug/release 模式）
    let output = std::process::Command::new("git")
        .args([
            "log",
            "-10",
            "--format=%H\x1F%h\x1F%an\x1F%cd\x1F%s",
            "--date=format:%Y-%m-%d %H:%M:%S %z",
        ])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let raw = String::from_utf8_lossy(&out.stdout).to_string();
            let mut commits = vec![];
            for (i, line) in raw.lines().filter(|l| !l.trim().is_empty()).enumerate() {
                let parts: Vec<&str> = line.splitn(5, '\x1F').collect();
                let version = format!("v1.0.{}", 10usize.saturating_sub(i));
                let hash = parts.first().unwrap_or(&"").to_string();
                let short_hash = parts.get(1).unwrap_or(&"").to_string();
                let raw_author = parts.get(2).unwrap_or(&"").to_string();
                let author = if raw_author.chars().count() > 2 {
                    let chars: Vec<char> = raw_author.chars().collect();
                    format!(
                        "{}***{}",
                        chars.first().unwrap_or(&'a'),
                        chars.last().unwrap_or(&'z')
                    )
                } else if raw_author.chars().count() == 2 {
                    let chars: Vec<char> = raw_author.chars().collect();
                    format!("{}*", chars.first().unwrap_or(&'a'))
                } else {
                    raw_author
                };
                let date = format_commit_date(parts.get(3).unwrap_or(&""));
                let message = parts.get(4).unwrap_or(&"").replace("\n", " ");

                commits.push(serde_json::json!({
                    "index": i,
                    "is_current": i == 0,
                    "version": version,
                    "hash": hash,
                    "short_hash": short_hash,
                    "author": author,
                    "date": date,
                    "message": message
                }));
            }

            if !commits.is_empty() {
                let current = commits.first().cloned().unwrap_or(serde_json::json!({}));
                let version = current
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                let runtime = collect_runtime(version).await;
                return Ok(Json(serde_json::json!({
                    "success": true,
                    "current": current,
                    "commits": commits,
                    "runtime": runtime,
                    "is_open_source": cfg!(not(feature = "commercial_plugins")),
                })));
            }
        }
    }

    // git 不可用时回退到编译期预生成的静态数据
    let static_commits_json = include_str!(concat!(env!("OUT_DIR"), "/git_commits.json"));

    let commits: Vec<serde_json::Value> =
        serde_json::from_str(static_commits_json).unwrap_or_else(|_| {
            vec![serde_json::json!({
                "index": 0,
                "is_current": true,
                "version": "unknown",
                "hash": "",
                "short_hash": "------",
                "author": "N/A",
                "date": "N/A",
                "message": "版本信息不可用",
            })]
        });

    let current = commits.first().cloned().unwrap_or(serde_json::json!({}));
    let version = current
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    let runtime = collect_runtime(version).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "current": current,
        "commits": commits,
        "runtime": runtime,
        "is_open_source": cfg!(not(feature = "commercial_plugins")),
    })))
}

pub fn default_menu_config_settings() -> crate::models::MenuConfigSettings {
    crate::models::MenuConfigSettings {
        items: vec![
            crate::models::MenuItemConfig {
                key: "/dashboard".to_string(),
                label_zh: "系统概览".to_string(),
                label_en: "Dashboard".to_string(),
                icon: "DashboardOutlined".to_string(),
                enabled: true,
                sort_order: 1,
                allowed_levels: "all".to_string(),
            },
            crate::models::MenuItemConfig {
                key: "/playground".to_string(),
                label_zh: "创作中心".to_string(),
                label_en: "Playground".to_string(),
                icon: "ExperimentOutlined".to_string(),
                enabled: true,
                sort_order: 2,
                allowed_levels: "all".to_string(),
            },
            crate::models::MenuItemConfig {
                key: "/playground-2026".to_string(),
                label_zh: "创作中心2026".to_string(),
                label_en: "Playground 2026".to_string(),
                icon: "ExperimentOutlined".to_string(),
                enabled: true,
                sort_order: 3,
                allowed_levels: "all".to_string(),
            },
            crate::models::MenuItemConfig {
                key: "/docs".to_string(),
                label_zh: "中继接口".to_string(),
                label_en: "Relay API".to_string(),
                icon: "RocketOutlined".to_string(),
                enabled: true,
                sort_order: 4,
                allowed_levels: "all".to_string(),
            },
            crate::models::MenuItemConfig {
                key: "/tokens".to_string(),
                label_zh: "令牌管理".to_string(),
                label_en: "Tokens".to_string(),
                icon: "KeyOutlined".to_string(),
                enabled: true,
                sort_order: 5,
                allowed_levels: "all".to_string(),
            },
            crate::models::MenuItemConfig {
                key: "/logs".to_string(),
                label_zh: "调用日志".to_string(),
                label_en: "Logs".to_string(),
                icon: "HistoryOutlined".to_string(),
                enabled: true,
                sort_order: 6,
                allowed_levels: "all".to_string(),
            },
            crate::models::MenuItemConfig {
                key: "/task-logs".to_string(),
                label_zh: "任务日志".to_string(),
                label_en: "Task Logs".to_string(),
                icon: "ScheduleOutlined".to_string(),
                enabled: true,
                sort_order: 7,
                allowed_levels: "all".to_string(),
            },
            crate::models::MenuItemConfig {
                key: "/advanced-marketing".to_string(),
                label_zh: "高级推广".to_string(),
                label_en: "Advanced Marketing".to_string(),
                icon: "TeamOutlined".to_string(),
                enabled: true,
                sort_order: 10,
                allowed_levels: "all".to_string(),
            },
            crate::models::MenuItemConfig {
                key: "/wallet".to_string(),
                label_zh: "我的钱包".to_string(),
                label_en: "Wallet".to_string(),
                icon: "WalletOutlined".to_string(),
                enabled: true,
                sort_order: 11,
                allowed_levels: "all".to_string(),
            },
            crate::models::MenuItemConfig {
                key: "/profile".to_string(),
                label_zh: "个人中心".to_string(),
                label_en: "Profile".to_string(),
                icon: "UserOutlined".to_string(),
                enabled: true,
                sort_order: 12,
                allowed_levels: "all".to_string(),
            },
        ],
    }
}

/// 手动触发一次全量系统日常维护（用量统计/大字段清理/错误清理/冷行归档/临时存储清理）
pub async fn trigger_maintenance(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<AdminContext>,
) -> AppResult<Json<serde_json::Value>> {
    require_edit(&ctx, "settings.database")?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        crate::run_daily_maintenance_job(&state_clone).await;
    });

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "全量系统日常维护任务已在后台触发执行"
    })))
}
