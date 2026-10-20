/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::error::{AppError, AppResult};
use crate::models::order::Order;
use crate::models::{CurrencySettings, PaymentChannelsUiSettings, PublicPaymentChannel};
use crate::services::fx_rates::{self, FxSnapshot};
use crate::time_system::DbTs;
use crate::AppState;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

mod payment;

use payment::alipay::AlipayClient;
use payment::allinpay::AllinpayClient;
use payment::hyperbc::HyperbcClient;
use payment::stripe::StripeClient;
use payment::wechat::WechatClient;

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateOrderReq {
    pub amount: f64,
    pub payment_method: String,
    /// 是否为移动端支付请求 (用于部分聚合支付通道多端跳转或扫码的自适应)
    #[serde(default)]
    pub is_mobile: Option<bool>,
    /// 卡券订单号。有值时这笔渠道收款只用于支付该未付款订单，不入钱包余额。
    #[serde(default)]
    pub coupon_order_no: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateOrderResp {
    pub out_trade_no: String,
    pub payment_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyperbc_data: Option<serde_json::Value>,
    /// 渠道收款币种与应收金额；跨币种时与入账金额（默认货币）不同
    pub pay_currency: String,
    pub pay_amount: f64,
}

/// 渠道收款币种最小单位的小数位：Stripe 零小数币种为 0，其余为 2
fn pay_minor_decimals(payment_method: &str, currency: &str) -> u32 {
    if payment_method == "stripe" {
        payment::stripe::currency_minor_decimals(currency)
    } else {
        2
    }
}

/// 将下单金额规整到渠道实际收款的最小货币单位（Stripe 零小数币种为整数，其余为分）。
/// 渠道侧按最小单位四舍五入收款，订单必须以同一值入账，杜绝「实收 10.00 / 入账 10.004」。
fn normalize_order_amount(amount: f64, payment_method: &str, currency: &str) -> f64 {
    if !amount.is_finite() {
        return 0.0;
    }
    let scale = 10f64.powi(pay_minor_decimals(payment_method, currency) as i32);
    (amount * scale).round() / scale
}

/// 下单报价：入账金额（默认货币）对应的渠道应收金额（收款币种）
#[derive(Debug, Clone, PartialEq)]
struct PayQuote {
    currency: String,
    amount: f64,
    /// 跨币种时 1 默认货币 = fx_rate 收款币种（含加价）
    fx_rate: Option<f64>,
}

fn channel_ui_id(payment_method: &str) -> &str {
    if payment_method.starts_with("allinpay_") {
        "allinpay"
    } else {
        payment_method
    }
}

/// 收款币种取渠道支持的默认货币，否则按渠道首选币种换算；换算向上取整到最小单位，平台不少收
fn quote_order(
    payment_method: &str,
    amount: f64,
    currency: &CurrencySettings,
    channels_ui: &PaymentChannelsUiSettings,
    snap: Option<&FxSnapshot>,
    now: i64,
) -> Result<PayQuote, String> {
    let ui_id = channel_ui_id(payment_method);
    let configured = channels_ui
        .channels
        .iter()
        .find(|c| c.id == ui_id)
        .and_then(|c| c.currencies.as_deref());
    let default = currency.default_currency.trim().to_ascii_uppercase();
    let effective = fx_rates::channel_effective_currencies(payment_method, configured, &default);
    let pay_currency =
        fx_rates::resolve_pay_currency(&default, &effective).ok_or("不支持的支付方式")?;
    if pay_currency == default {
        return Ok(PayQuote {
            currency: pay_currency,
            amount,
            fx_rate: None,
        });
    }
    let rate = fx_rates::payment_rate(&currency.fx, snap, &default, &pay_currency, now)
        .ok_or_else(|| {
            format!("暂无可用的 {default} → {pay_currency} 汇率，请稍后再试或联系管理员")
        })?;
    let decimals = pay_minor_decimals(payment_method, &pay_currency);
    let pay_amount = fx_rates::convert_ceil(amount, rate, decimals)
        .ok_or("充值金额过小，换算后不足渠道最小收款单位")?;
    Ok(PayQuote {
        currency: pay_currency,
        amount: pay_amount,
        fx_rate: Some(rate),
    })
}

/// 用户端渠道列表补充收款币种与换算汇率；跨币种但暂无可用汇率的渠道置为不可用
pub fn annotate_public_channels(
    mut channels: Vec<PublicPaymentChannel>,
    ui: &PaymentChannelsUiSettings,
    currency: &CurrencySettings,
    snap: Option<&FxSnapshot>,
    now: i64,
) -> Vec<PublicPaymentChannel> {
    let default = currency.default_currency.trim().to_ascii_uppercase();
    for ch in &mut channels {
        let configured = ui
            .channels
            .iter()
            .find(|c| c.id == ch.id)
            .and_then(|c| c.currencies.as_deref());
        let currencies = fx_rates::channel_effective_currencies(&ch.id, configured, &default);
        if let Some(pay_currency) = fx_rates::resolve_pay_currency(&default, &currencies) {
            let rate = fx_rates::payment_rate(&currency.fx, snap, &default, &pay_currency, now);
            if rate.is_none() {
                ch.enabled = false;
                ch.allinpay_methods.clear();
            }
            ch.pay_currency_symbol =
                fx_rates::currency_meta(&pay_currency).map(|m| m.symbol.to_string());
            ch.pay_decimals = Some(pay_minor_decimals(&ch.id, &pay_currency));
            ch.pay_rate = rate;
            ch.pay_currency = Some(pay_currency);
        }
        ch.currencies = currencies;
    }
    channels
}

pub async fn create_order(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(claims): axum::extract::Extension<crate::auth::Claims>,
    headers: axum::http::HeaderMap,
    Json(mut payload): Json<CreateOrderReq>,
) -> AppResult<Json<CreateOrderResp>> {
    let s = crate::api::settings::get_currency_settings(&state).await;

    let coupon_order_no = payload
        .coupon_order_no
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let mut pay_subject = "钱包充值".to_string();
    let mut coupon_price: Option<f64> = None;
    let mut return_path = "/wallet".to_string();
    if let Some(order_no) = &coupon_order_no {
        return_path = format!("/orders/{order_no}");
        let row: Option<(String, f64, String, Option<chrono::DateTime<chrono::Utc>>, f64)> = sqlx::query_as(
            &state.db.format_query(
                "SELECT pay_status, price_paid, plan_name, pay_expires_at, balance_hold FROM mkt_member_orders WHERE order_no = ? AND user_id = ?",
            ),
        )
        .bind(order_no)
        .bind(&claims.sub)
        .fetch_optional(&state.db.pool)
        .await?;
        let Some((pay_status, price, plan_name, pay_expires_at, hold)) = row else {
            return Err(AppError::NotFound("卡券订单不存在".into()));
        };
        if pay_status != "unpaid" {
            return Err(AppError::BadRequest("订单不是待支付".into()));
        }
        if pay_expires_at.is_some_and(|at| at <= chrono::Utc::now()) {
            #[cfg(feature = "commercial_plugins")]
            let _ = crate::api::plugins::marketing_membership::close_expired_unpaid(&state).await;
            return Err(AppError::BadRequest("订单已超过 24 小时未支付，已取消".into()));
        }
        let due = crate::money::round_money((price - hold).max(0.0));
        if due < 0.01 {
            return Err(AppError::BadRequest("该订单无需在线支付".into()));
        }
        payload.amount = due;
        coupon_price = Some(due);
        pay_subject = format!("卡券 {plan_name}");
    }

    // 后续校验、渠道下单、订单入账统一使用规整后的金额
    payload.amount =
        normalize_order_amount(payload.amount, &payload.payment_method, &s.default_currency);
    if let Some(price) = coupon_price {
        if crate::money::round_money(payload.amount) != crate::money::round_money(price) {
            return Err(AppError::BadRequest(
                "该金额无法通过此渠道按原价支付，请使用余额".into(),
            ));
        }
    }
    // 钱包充值限额：渠道单独开启时用渠道最小/最大，否则用全局通用充值设置。卡券订单不走这条。
    let raw_channels_ui: Option<crate::models::PaymentChannelsUiSettings> =
        sqlx::query_scalar::<_, String>(
            &state
                .db
                .format_query("SELECT value FROM settings WHERE key = 'payment_channels_ui'"),
        )
        .fetch_optional(&state.db.pool)
        .await?
        .and_then(|v| serde_json::from_str(&v).ok());
    let channels_ui = crate::models::merge_payment_channels_ui(
        raw_channels_ui,
        &crate::models::PaymentGatewayEnableFlags {
            wechat: true,
            alipay: true,
            stripe: true,
            hyperbc: true,
            allinpay: true,
        },
    );
    if coupon_order_no.is_none() {
        let (min_amount, max_amount) = crate::models::effective_recharge_amount_limits(
            &channels_ui,
            &payload.payment_method,
            s.min_recharge_amount,
            s.max_recharge_amount,
        );
        if min_amount > 0.0 && payload.amount < min_amount {
            return Err(AppError::BadRequest(format!(
                "充值金额不能小于 {}",
                min_amount
            )));
        }
        if max_amount > 0.0 && payload.amount > max_amount {
            return Err(AppError::BadRequest(format!(
                "充值金额不能大于 {}",
                max_amount
            )));
        }
    }
    if payload.amount < 0.01 {
        return Err(AppError::BadRequest("金额必须大于或等于 0.01".to_string()));
    }

    // 检查用户是否被禁止在线支付（pay_enabled 为 0 表示禁止支付，1 表示允许支付）
    let pay_enabled: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT pay_enabled FROM users WHERE id = ?"),
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db.pool)
    .await?;
    if pay_enabled.unwrap_or(1) == 0 {
        return Err(AppError::Forbidden(
            "您的在线支付功能已被管理员关闭".to_string(),
        ));
    }

    // 后台「支付渠道」开关对所有渠道生效（与用户端公开列表同口径：渠道开 ∧ 网关开）。
    // 网关 enabled 由下方各分支校验；channels_ui 已在金额限额处合并，未存 UI 配置的渠道默认放行给网关判定。
    if !crate::models::is_payment_channel_ui_enabled(&channels_ui, &payload.payment_method) {
        let msg = if payload.payment_method.starts_with("allinpay_") {
            "该通联支付渠道暂未开启"
        } else {
            "该支付渠道暂未开启"
        };
        return Err(AppError::BadRequest(msg.to_string()));
    }

    // 入账始终为默认货币 payload.amount；渠道不收默认货币时按汇率换算应收金额（汇率快照随订单落库）
    let fx_snapshot = fx_rates::load_snapshot(&state).await;
    let quote = quote_order(
        &payload.payment_method,
        payload.amount,
        &s,
        &channels_ui,
        fx_snapshot.as_ref(),
        chrono::Utc::now().timestamp(),
    )
    .map_err(AppError::BadRequest)?;

    let out_trade_no = format!(
        "T{}R{}",
        Local::now().format("%Y%m%d%H%M%S"),
        &Uuid::new_v4().simple().to_string()[..8]
    );

    // 回调基地址推断：优先级 PUBLIC_API_URL env > Origin header > Host header
    let base_notify_url = crate::relay::vendor_callback::infer_base_url(&headers);

    // 前端跳转基地址：优先 PUBLIC_FRONTEND_URL env，自动感知 Origin 头，兜底使用 base_notify_url
    let frontend_base_url = std::env::var("PUBLIC_FRONTEND_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            headers
                .get("origin")
                .and_then(|v| v.to_str().ok())
                .filter(|s| !s.is_empty() && *s != "null")
                .map(|s| s.trim_end_matches('/').to_string())
        })
        .unwrap_or_else(|| base_notify_url.clone());

    tracing::info!(
        "[支付] 用户 {} 发起充值 {:.2} {}, 应收 {} {}, 方式: {}, 订单号: {}, 回调基地址: {}, 前端基地址: {}",
        claims.sub,
        payload.amount,
        s.default_currency,
        quote.amount,
        quote.currency,
        payload.payment_method,
        out_trade_no,
        base_notify_url,
        frontend_base_url
    );

    let payment_url: String;
    let mut trade_no: Option<String> = None;
    let mut hyperbc_data: Option<serde_json::Value> = None;

    if payload.payment_method == "wechat" {
        let wechat_setting: Option<String> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT value FROM settings WHERE key = 'payment_wechat'"),
        )
        .fetch_optional(&state.db.pool)
        .await?;
        let wechat_config: crate::models::PaymentWechatSettings =
            serde_json::from_str(&wechat_setting.unwrap_or_default())
                .map_err(|_| AppError::BadRequest("微信支付未配置".to_string()))?;

        if !wechat_config.enabled {
            return Err(AppError::BadRequest("微信支付暂未开启".to_string()));
        }

        let wechat_client = WechatClient::new(wechat_config);
        let notify_url = format!("{}/api/v1/finance/pay/notify/wechat", base_notify_url);
        tracing::info!("[支付] 微信回调地址: {}", notify_url);
        payment_url = wechat_client
            .create_native_order(&out_trade_no, quote.amount, &pay_subject, &notify_url)
            .await?;
    } else if payload.payment_method == "alipay" {
        let alipay_setting: Option<String> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT value FROM settings WHERE key = 'payment_alipay'"),
        )
        .fetch_optional(&state.db.pool)
        .await?;
        let alipay_config: crate::models::PaymentAlipaySettings =
            serde_json::from_str(&alipay_setting.unwrap_or_default())
                .map_err(|_| AppError::BadRequest("支付宝未配置".to_string()))?;

        if !alipay_config.enabled {
            return Err(AppError::BadRequest("支付宝暂未开启".to_string()));
        }

        let alipay_client = AlipayClient::new(alipay_config);
        let notify_url = format!("{}/api/v1/finance/pay/notify/alipay", base_notify_url);
        let return_url = format!("{frontend_base_url}{return_path}");
        tracing::info!("[支付] 支付宝回调地址: {}", notify_url);
        payment_url = alipay_client.generate_page_pay_url(
            &out_trade_no,
            quote.amount,
            &pay_subject,
            &notify_url,
            &return_url,
        )?;
    } else if payload.payment_method == "stripe" {
        let stripe_setting: Option<String> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT value FROM settings WHERE key = 'payment_stripe'"),
        )
        .fetch_optional(&state.db.pool)
        .await?;
        let stripe_config: crate::models::PaymentStripeSettings =
            serde_json::from_str(&stripe_setting.unwrap_or_default())
                .map_err(|_| AppError::BadRequest("Stripe 未配置".to_string()))?;

        if !stripe_config.enabled {
            return Err(AppError::BadRequest("Stripe 支付暂未开启".to_string()));
        }

        let success_url = format!("{frontend_base_url}{return_path}?payment=success");
        let cancel_url = format!("{frontend_base_url}{return_path}?payment=cancelled");

        let currency = quote.currency.to_lowercase();

        let stripe_client = StripeClient::new(stripe_config);
        tracing::info!("[支付] Stripe Checkout Session 创建中, 货币: {}", currency);
        let (session_url, session_id) = stripe_client
            .create_checkout_session(
                &out_trade_no,
                quote.amount,
                &currency,
                &pay_subject,
                &success_url,
                &cancel_url,
            )
            .await
            .map_err(|e| AppError::UpstreamError(e.to_string()))?;
        tracing::info!("[支付] Stripe session_id: {}", session_id);
        payment_url = session_url;
        trade_no = Some(session_id);
    } else if payload.payment_method == "hyperbc" {
        let hyperbc_setting: Option<String> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT value FROM settings WHERE key = 'payment_hyperbc'"),
        )
        .fetch_optional(&state.db.pool)
        .await?;
        let hyperbc_config: crate::models::PaymentHyperbcSettings =
            serde_json::from_str(&hyperbc_setting.unwrap_or_default())
                .map_err(|_| AppError::BadRequest("HyperBC 支付未配置".to_string()))?;

        if !hyperbc_config.enabled {
            return Err(AppError::BadRequest("HyperBC 支付暂未开启".to_string()));
        }

        let hyperbc_client = HyperbcClient::new(hyperbc_config);
        let return_url = format!("{frontend_base_url}{return_path}");

        let lang = headers
            .get("accept-language")
            .and_then(|v| v.to_str().ok())
            .map(|s| if s.starts_with("en") { "en" } else { "zh" })
            .unwrap_or("zh");

        // HyperBC 只按默认货币下单（代币按后台加密货币汇率折算），见 fx_rates::channel_capable_currencies
        let currency = quote.currency.to_lowercase();

        tracing::info!(
            "[支付] HyperBC H5 Hosted Cashier 创建中: return_url={}, currency={}, lang={}",
            return_url,
            currency,
            lang
        );

        let h5_order = hyperbc_client
            .create_h5_order(&out_trade_no, quote.amount, &currency, &return_url, lang)
            .await
            .map_err(|e| AppError::UpstreamError(e.to_string()))?;

        payment_url = h5_order.checkout_url.clone();
        trade_no = Some(h5_order.order_no.clone());
        hyperbc_data = Some(serde_json::to_value(&h5_order).unwrap_or_default());
    } else if payload.payment_method == "allinpay_wechat"
        || payload.payment_method == "allinpay_alipay"
    {
        let allinpay_setting: Option<String> = sqlx::query_scalar(
            &state
                .db
                .format_query("SELECT value FROM settings WHERE key = 'payment_allinpay'"),
        )
        .fetch_optional(&state.db.pool)
        .await?;
        let allinpay_config: crate::models::PaymentAllinpaySettings =
            serde_json::from_str(&allinpay_setting.unwrap_or_default())
                .map_err(|_| AppError::BadRequest("通联支付未配置".to_string()))?;

        if !allinpay_config.enabled {
            return Err(AppError::BadRequest("通联支付暂未开启".to_string()));
        }

        let allinpay_client = AllinpayClient::new(allinpay_config);
        let notify_url = format!("{}/api/v1/finance/pay/notify/allinpay", base_notify_url);

        let client_is_mobile = payload.is_mobile.unwrap_or_else(|| {
            if let Some(ua) = headers.get("user-agent").and_then(|v| v.to_str().ok()) {
                let ua_lower = ua.to_lowercase();
                ua_lower.contains("mobile")
                    || ua_lower.contains("android")
                    || ua_lower.contains("iphone")
                    || ua_lower.contains("ipad")
            } else {
                false
            }
        });

        if client_is_mobile {
            // 手机H5收银台模式
            let return_url = format!("{frontend_base_url}{return_path}");
            tracing::info!("[支付] 通联 H5 收银台支付创建中, return_url={}", return_url);
            payment_url = allinpay_client
                .generate_h5_pay_url(&out_trade_no, quote.amount, &notify_url, &return_url)
                .map_err(|e| AppError::UpstreamError(e.to_string()))?;
        } else {
            // PC端扫码支付模式 (微信/支付宝二维码)
            let paytype = if payload.payment_method == "allinpay_wechat" {
                "W01"
            } else {
                "A01"
            };
            tracing::info!(
                "[支付] 通联 PC 扫码支付创建中, 方式: {}, paytype: {}",
                payload.payment_method,
                paytype
            );
            payment_url = allinpay_client
                .create_scan_pay(
                    &out_trade_no,
                    quote.amount,
                    &pay_subject,
                    &notify_url,
                    paytype,
                )
                .await
                .map_err(|e| AppError::UpstreamError(e.to_string()))?;
        }
    } else {
        return Err(AppError::BadRequest("不支持的支付方式".to_string()));
    }

    // 保存待支付订单记录 (仅用于微信/支付宝/Stripe/HyperBC 等传统支付方式)
    // 数据库写入操作字段说明：
    // - out_trade_no: 系统内唯一交易流水号，格式 T年月日时分秒R随机字符串
    // - user_id: 关联充值发起者的用户主键ID
    // - payment_method: 支付通道类型 (可选值: wechat, alipay, stripe, hyperbc, allinpay_wechat, allinpay_alipay)
    // - amount: 充值金额，对应本系统的法币余额数量
    // - status: 订单处理进度状态，初始化设定为 'pending'
    // - created_at: 订单的创建时间（TIMESTAMPTZ，由数据库 CURRENT_TIMESTAMP 写入）
    // - pay_currency / pay_amount / fx_rate: 渠道收款币种、应收金额与下单汇率，回调按此核验实收
    sqlx::query(&state.db.format_query("INSERT INTO orders (out_trade_no, user_id, payment_method, amount, status, created_at, trade_no, pay_currency, pay_amount, fx_rate) VALUES (?, ?, ?, ?, 'pending', CURRENT_TIMESTAMP, ?, ?, ?, ?)"))
        .bind(&out_trade_no)
        .bind(&claims.sub)
        .bind(&payload.payment_method)
        .bind(payload.amount)
        .bind(trade_no)
        .bind(&quote.currency)
        .bind(quote.amount)
        .bind(quote.fx_rate)
        .execute(&state.db.pool)
        .await?;

    if let Some(order_no) = &coupon_order_no {
        let linked = sqlx::query(&state.db.format_query(
            "UPDATE mkt_member_orders SET payment_no = ?, pay_method = ? \
             WHERE order_no = ? AND user_id = ? AND pay_status = 'unpaid'",
        ))
        .bind(&out_trade_no)
        .bind(&payload.payment_method)
        .bind(order_no)
        .bind(&claims.sub)
        .execute(&state.db.pool)
        .await?
        .rows_affected();
        if linked == 0 {
            let _ = sqlx::query(&state.db.format_query(
                "UPDATE orders SET status = 'cancelled' WHERE out_trade_no = ? AND status = 'pending'",
            ))
            .bind(&out_trade_no)
            .execute(&state.db.pool)
            .await;
            return Err(AppError::BadRequest("订单不是待支付".into()));
        }
    }

    tracing::info!("[支付] 订单创建成功: {}, payment_url 已生成", out_trade_no);

    Ok(Json(CreateOrderResp {
        out_trade_no,
        payment_url,
        hyperbc_data,
        pay_currency: quote.currency,
        pay_amount: quote.amount,
    }))
}

/// 订单金额转最小货币单位（分），与下单时各渠道 `round(amount * 100)` 口径一致
fn amount_to_fen(amount: f64) -> i64 {
    (amount * 100.0).round() as i64
}

/// 解析渠道回传的「元」字符串金额（如支付宝 total_amount = "10.00"）为分
fn parse_yuan_to_fen(raw: &str) -> Option<i64> {
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(amount_to_fen)
}

/// 回调中的商户标识是否等于本站配置；配置为空一律视为不匹配
fn merchant_id_matches(expected: &str, actual: Option<&str>) -> bool {
    let expected = expected.trim();
    !expected.is_empty() && actual.map(str::trim) == Some(expected)
}

/// 入账前业务核验。验签只证明报文来自支付平台，不证明这笔钱付给本商户、金额相符：
/// 必须同时满足 订单渠道一致 ∧ 商户标识一致 ∧ 渠道实收（分）== 订单金额（分），否则拒绝入账。
/// 防御场景：攻击者用自有商户号对本站待支付订单号发起小额真实支付，平台回调签名合法。
fn settlement_verified(
    tag: &str,
    out_trade_no: &str,
    order_amount: f64,
    channel_matches: bool,
    merchant_matches: bool,
    paid_fen: Option<i64>,
) -> bool {
    let expected_fen = amount_to_fen(order_amount);
    let ok = channel_matches && merchant_matches && paid_fen == Some(expected_fen);
    if !ok {
        tracing::warn!(
            "[{}] 入账核验未通过，拒绝入账: out_trade_no={}, 渠道一致={}, 商户一致={}, 实收(分)={:?}, 应收(分)={}",
            tag,
            out_trade_no,
            channel_matches,
            merchant_matches,
            paid_fen,
            expected_fen
        );
    }
    ok
}

/// 在线支付入账流水，并在邀请人等级为「按充值发放」时记佣。与订单 pending→paid 同事务。
async fn insert_paid_recharge_record(
    db: &crate::db::Database,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    amount: f64,
    recharge_type: &str,
    order_no: &str,
) -> Result<(), sqlx::Error> {
    let recharge_id: i64 = sqlx::query_scalar(&db.format_query(
        "INSERT INTO recharge_records (user_id, amount, recharge_type, remark, order_no) VALUES (?, ?, ?, '', ?) RETURNING id",
    ))
    .bind(user_id)
    .bind(amount)
    .bind(recharge_type)
    .bind(order_no)
    .fetch_one(&mut **tx)
    .await?;
    crate::services::affiliate::award_recharge_commission(db, tx, user_id, recharge_id, amount)
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    Ok(())
}

/// 渠道款入账。卡券订单金额对得上就发卡，不进余额；否则按充值加余额。
async fn credit_paid_order(
    state: &Arc<AppState>,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    amount: f64,
    payment_method: &str,
    out_trade_no: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "commercial_plugins")]
    {
        let coupon = crate::api::plugins::marketing_membership::try_fulfill_coupon_payment(
            state.as_ref(),
            tx,
            user_id,
            amount,
            payment_method,
            out_trade_no,
        )
        .await?;
        if coupon {
            tracing::info!("[支付] 订单 {} 已用于卡券支付，不入钱包余额", out_trade_no);
            return Ok(());
        }
    }
    let now = DbTs::now();
    sqlx::query(
        &state
            .db
            .format_query("UPDATE users SET balance = balance + ?, updated_at = ? WHERE id = ?"),
    )
    .bind(amount)
    .bind(&now)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    insert_paid_recharge_record(&state.db, tx, user_id, amount, payment_method, out_trade_no).await?;
    Ok(())
}

/// 通用的充值入账核心逻辑，集成更新订单状态、加用户余额、记录明细三合一事务。
/// 利用数据库行锁与 `status = 'pending'` 限制条件提供强幂等防护，杜绝高并发重复充值。
async fn complete_recharge_payment_common(
    state: &Arc<AppState>,
    out_trade_no: &str,
    amount: f64,
    user_id: &str,
    payment_method: &str,
    channel_name_zh: &str,
) -> AppResult<bool> {
    // 开启数据库事务进行余额入账与记录插入
    let mut tx = state
        .db
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(format!("启动事务失败: {:?}", e)))?;
    let now = DbTs::now();

    // 1. 更新订单表 orders，状态由 pending 变更为 paid
    // 限制条件：仅在原状态为 'pending' 时才能成功更新，防止并发或重复充值
    let result = sqlx::query(&state.db.format_query(
        "UPDATE orders SET status = 'paid', paid_at = ? WHERE out_trade_no = ? AND status = 'pending'"
    ))
    .bind(&now)
    .bind(out_trade_no)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(format!("更新订单状态失败: {:?}", e)))?;

    if result.rows_affected() == 0 {
        tracing::info!(
            "[{}] 订单已是支付状态或已被并发处理，跳过: {}",
            channel_name_zh,
            out_trade_no
        );
        let _ = tx.rollback().await;
        return Ok(false); // 已处理，返回 false
    }

    // 2. 卡券订单则发卡；否则增加余额并写充值流水
    credit_paid_order(
        state,
        &mut tx,
        user_id,
        amount,
        payment_method,
        out_trade_no,
    )
    .await
    .map_err(|e| AppError::Internal(format!("入账失败: {:?}", e)))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(format!("提交事务失败: {:?}", e)))?;

    tracing::info!(
        "[{}] ✅ 订单 {} 充值成功，入账 {:.2} 元",
        channel_name_zh,
        out_trade_no,
        amount
    );
    // 余额回升时清除低余额提醒周期标记
    crate::services::notification::spawn_low_balance_check(Arc::clone(state), user_id.to_string());
    Ok(true) // 处理成功，返回 true
}

/// HyperBC 结算金额：只按链上实收代币 × 汇率折算，且不超过订单金额（零亏损红线）。
/// 拿不到实收（payments 为空或金额无法解析）时返回 None：不自动入账，留待人工复核，
/// 杜绝「状态成功但无实收明细」的报文按订单全额入账。
fn hyperbc_settle_amount(actual_crypto: f64, exchange_rate: f64, order_amount: f64) -> Option<f64> {
    if !actual_crypto.is_finite() || actual_crypto <= 0.0 {
        return None;
    }
    let rate = if exchange_rate > 0.0 {
        exchange_rate
    } else {
        1.0
    };
    Some((actual_crypto * rate).min(order_amount))
}

async fn complete_hyperbc_payment(
    state: &Arc<AppState>,
    out_trade_no: &str,
    amount: f64,
    user_id: &str,
) -> AppResult<bool> {
    complete_recharge_payment_common(state, out_trade_no, amount, user_id, "hyperbc", "HyperBC")
        .await
}

/// 封装通联支付成功逻辑：更新订单、加用户余额并记录明细（利用数据库行锁与 status='pending' 确保严格的全局幂等性）
async fn complete_allinpay_payment(
    state: &Arc<AppState>,
    out_trade_no: &str,
    amount: f64,
    user_id: &str,
    payment_method: &str,
) -> AppResult<bool> {
    let channel_name = if payment_method == "allinpay_wechat" {
        "通联微信"
    } else {
        "通联支付宝"
    };
    complete_recharge_payment_common(
        state,
        out_trade_no,
        amount,
        user_id,
        payment_method,
        channel_name,
    )
    .await
}

#[derive(Debug, Serialize)]
pub struct OrderStatusResp {
    pub status: String,
}

pub async fn check_status(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(claims): axum::extract::Extension<crate::auth::Claims>,
    Path(out_trade_no): Path<String>,
) -> AppResult<Json<OrderStatusResp>> {
    let order: Option<Order> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM orders WHERE out_trade_no = ? AND user_id = ?"),
    )
    .bind(&out_trade_no)
    .bind(&claims.sub)
    .fetch_optional(&state.db.pool)
    .await?;

    if let Some(mut order) = order {
        // 如果订单是待支付，且支付方式是 HyperBC，我们尝试主动查询最新的支付状态以防回调延迟或在本地开发环境中丢失
        if order.status == "pending" && order.payment_method == "hyperbc" {
            if let Some(ref hyperbc_order_no) = order.trade_no {
                // 读取 HyperBC 配置
                let hyperbc_setting: Option<String> = sqlx::query_scalar(
                    &state
                        .db
                        .format_query("SELECT value FROM settings WHERE key = 'payment_hyperbc'"),
                )
                .fetch_optional(&state.db.pool)
                .await?;
                if let Some(setting_str) = hyperbc_setting {
                    if let Ok(config) =
                        serde_json::from_str::<crate::models::PaymentHyperbcSettings>(&setting_str)
                    {
                        if config.enabled {
                            let hyperbc_client = HyperbcClient::new(config.clone());
                            // 事务外调用第三方 API，确保不长期锁定数据库连接与行锁
                            match hyperbc_client.query_order(hyperbc_order_no).await {
                                Ok(query_data) => {
                                    let check_code = query_data.get_check_code();
                                    let actual_crypto = query_data.get_actual_crypto_amount();
                                    tracing::info!(
                                        "[支付状态查询] HyperBC 订单 {} 查询状态为: status={}, check_code={}, 实际到账代币={}",
                                        out_trade_no,
                                        query_data.status,
                                        check_code,
                                        actual_crypto
                                    );

                                    let order_matched = merchant_id_matches(
                                        &out_trade_no,
                                        Some(query_data.merchant_order_id.as_str()),
                                    ) && merchant_id_matches(
                                        hyperbc_order_no,
                                        Some(query_data.order_no.as_str()),
                                    );
                                    let should_settle = order_matched
                                        && ((query_data.status == 1 && check_code == 0)
                                            || (query_data.status == 2
                                                && check_code == 1001
                                                && actual_crypto > 0.0));

                                    if should_settle {
                                        // 结算金额严格按照实收代币与汇率折算，且不得超出预设金额（平台零亏损红线）
                                        match hyperbc_settle_amount(
                                            actual_crypto,
                                            config.crypto_exchange_rate,
                                            order.amount,
                                        ) {
                                            Some(settle_amount) => {
                                                match complete_hyperbc_payment(
                                                    &state,
                                                    &out_trade_no,
                                                    settle_amount,
                                                    &order.user_id,
                                                )
                                                .await
                                                {
                                                    Ok(_) => {
                                                        order.status = "paid".to_string();
                                                    }
                                                    Err(e) => {
                                                        tracing::warn!(
                                                            "[支付状态查询] 自动入账失败: {:?}",
                                                            e
                                                        );
                                                    }
                                                }
                                            }
                                            None => {
                                                tracing::warn!(
                                                    "[支付状态查询] HyperBC 订单 {} 无链上实收金额，不自动入账，需人工复核",
                                                    out_trade_no
                                                );
                                            }
                                        }
                                    } else if query_data.status == 10 {
                                        // 订单已取消
                                        let _ = sqlx::query(&state.db.format_query("UPDATE orders SET status = 'cancelled' WHERE out_trade_no = ? AND status = 'pending'"))
                                            .bind(&out_trade_no)
                                            .execute(&state.db.pool)
                                            .await;
                                        order.status = "cancelled".to_string();
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "[支付状态查询] 主动查询 HyperBC 订单 {} 失败: {:?}",
                                        out_trade_no,
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
            }
        } else if order.status == "pending"
            && (order.payment_method == "allinpay_wechat"
                || order.payment_method == "allinpay_alipay")
        {
            // 读取通联配置并进行主动查询，防止前台关闭或回调延迟导致状态不同步
            let allinpay_setting: Option<String> = sqlx::query_scalar(
                &state
                    .db
                    .format_query("SELECT value FROM settings WHERE key = 'payment_allinpay'"),
            )
            .fetch_optional(&state.db.pool)
            .await?;
            if let Some(setting_str) = allinpay_setting {
                if let Ok(config) =
                    serde_json::from_str::<crate::models::PaymentAllinpaySettings>(&setting_str)
                {
                    if config.enabled {
                        let (cfg_cusid, cfg_appid) = (config.cusid.clone(), config.appid.clone());
                        let allinpay_client = AllinpayClient::new(config);
                        match allinpay_client.query_order(&order.out_trade_no).await {
                            Ok(query_data) => {
                                tracing::info!("[支付状态查询] 通联订单 {} 查询状态为: retcode={}, trxstatus={:?}", out_trade_no, query_data.retcode, query_data.trxstatus);
                                // trxstatus 0000 交易成功，此时利用 complete_allinpay_payment 悲观锁事务安全入账
                                if query_data.retcode == "SUCCESS"
                                    && query_data.trxstatus.as_deref() == Some("0000")
                                    && settlement_verified(
                                        "支付状态查询-通联",
                                        &out_trade_no,
                                        order.expected_pay_amount(),
                                        true,
                                        merchant_id_matches(
                                            &cfg_cusid,
                                            query_data.cusid.as_deref(),
                                        ) && merchant_id_matches(
                                            &cfg_appid,
                                            query_data.appid.as_deref(),
                                        ),
                                        query_data
                                            .trxamt
                                            .as_deref()
                                            .and_then(|v| v.trim().parse::<i64>().ok()),
                                    )
                                {
                                    match complete_allinpay_payment(
                                        &state,
                                        &out_trade_no,
                                        order.amount,
                                        &order.user_id,
                                        &order.payment_method,
                                    )
                                    .await
                                    {
                                        Ok(_) => {
                                            order.status = "paid".to_string();
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "[支付状态查询] 通联自动入账失败: {:?}",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "[支付状态查询] 主动查询通联订单 {} 失败: {:?}",
                                    out_trade_no,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }
        Ok(Json(OrderStatusResp {
            status: order.status,
        }))
    } else {
        Err(AppError::NotFound("订单不存在或无权访问".into()))
    }
}

// ================= Notifications =================

pub async fn wechat_notify(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> impl IntoResponse {
    use crate::models::PaymentWechatSettings;
    use axum::http::StatusCode;

    tracing::info!("[微信回调] 收到回调通知, body长度: {}", body.len());
    tracing::debug!("[微信回调] 原始数据: {}", body);

    let resp_success = serde_json::json!({ "code": "SUCCESS", "message": "成功" });
    let resp_fail = serde_json::json!({ "code": "FAIL", "message": "失败" });

    // 1. 读取配置
    let wechat_setting: Option<String> = match sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = 'payment_wechat'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[微信回调] 读取配置失败: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
        }
    };

    let config =
        match serde_json::from_str::<PaymentWechatSettings>(&wechat_setting.unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[微信回调] 解析配置失败: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
            }
        };

    let (cfg_mchid, cfg_appid) = (config.mchid.clone(), config.appid.clone());
    let client = WechatClient::new(config);

    // 验证微信签名
    let signature = headers
        .get("wechatpay-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let timestamp = headers
        .get("wechatpay-timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let nonce_hdr = headers
        .get("wechatpay-nonce")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let serial = headers
        .get("wechatpay-serial")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if signature.is_empty() || timestamp.is_empty() || nonce_hdr.is_empty() || serial.is_empty() {
        tracing::warn!("[微信回调] 缺少必要的签名 Header");
        return (StatusCode::BAD_REQUEST, Json(resp_fail));
    }

    match client.fetch_platform_certs().await {
        Ok(certs) => {
            if let Some(pub_key) = certs.get(serial) {
                match client.verify_signature(pub_key, timestamp, nonce_hdr, &body, signature) {
                    Ok(true) => {
                        tracing::info!("[微信回调] 签名验证成功");
                    }
                    Ok(false) => {
                        tracing::warn!("[微信回调] 签名验证失败");
                        return (StatusCode::BAD_REQUEST, Json(resp_fail));
                    }
                    Err(e) => {
                        tracing::warn!("[微信回调] 签名验证异常: {:?}", e);
                        return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
                    }
                }
            } else {
                tracing::warn!("[微信回调] 找不到对应的平台证书 serial: {}", serial);
                return (StatusCode::BAD_REQUEST, Json(resp_fail));
            }
        }
        Err(e) => {
            tracing::warn!("[微信回调] 获取平台证书失败: {:?}", e);
            // 严格验证：如果获取不到证书，直接返回失败，防止伪造
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
        }
    }

    // 2. 解析回调报文
    let payload: serde_json::Value = match serde_json::from_str(&body) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("[微信回调] JSON解析失败: {:?}", e);
            return (StatusCode::BAD_REQUEST, Json(resp_fail));
        }
    };

    let event_type = payload["event_type"].as_str().unwrap_or("");
    tracing::info!("[微信回调] event_type: {}", event_type);

    if event_type != "TRANSACTION.SUCCESS" {
        tracing::info!("[微信回调] 非支付成功事件，忽略: {}", event_type);
        return (StatusCode::OK, Json(resp_success));
    }

    // 3. 解密资源
    let resource = &payload["resource"];
    let nonce = resource["nonce"].as_str().unwrap_or("");
    let associated_data = resource["associated_data"].as_str().unwrap_or("");
    let ciphertext = resource["ciphertext"].as_str().unwrap_or("");

    let decrypted = match client.decrypt_callback_resource(nonce, associated_data, ciphertext) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("[微信回调] AES解密失败: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
        }
    };

    tracing::info!("[微信回调] 解密成功: {}", decrypted);

    let data: serde_json::Value = match serde_json::from_str(&decrypted) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("[微信回调] 解密数据JSON解析失败: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
        }
    };

    let trade_state = data["trade_state"].as_str().unwrap_or("");
    let out_trade_no = data["out_trade_no"].as_str().unwrap_or("");
    let trade_no = data["transaction_id"].as_str().unwrap_or("");

    tracing::info!(
        "[微信回调] trade_state: {}, out_trade_no: {}, transaction_id: {}",
        trade_state,
        out_trade_no,
        trade_no
    );

    if trade_state != "SUCCESS" {
        tracing::info!("[微信回调] 交易状态非SUCCESS: {}", trade_state);
        return (StatusCode::OK, Json(resp_success));
    }

    // 4. 前置检查：事务外查询，避免不必要的事务开销
    let order: Option<Order> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM orders WHERE out_trade_no = ?"),
    )
    .bind(out_trade_no)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    if order.is_none() {
        tracing::warn!("[微信回调] 订单不存在: {}", out_trade_no);
        return (StatusCode::OK, Json(resp_success));
    }

    let order = order.unwrap();
    if order.status != "pending" {
        tracing::info!("[微信回调] 订单已处理过, 当前状态: {}, 跳过", order.status);
        return (StatusCode::OK, Json(resp_success));
    }

    // 4.1 业务核验：渠道、商户号/appid、订单总额（分）必须与本站订单一致
    if !settlement_verified(
        "微信回调",
        out_trade_no,
        order.expected_pay_amount(),
        order.payment_method == "wechat",
        merchant_id_matches(&cfg_mchid, data["mchid"].as_str())
            && merchant_id_matches(&cfg_appid, data["appid"].as_str()),
        data["amount"]["total"].as_i64(),
    ) {
        return (StatusCode::BAD_REQUEST, Json(resp_fail));
    }

    // 5. 事务处理：更新订单 + 充值余额 + 写充值记录
    let mut tx = match state.db.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("[微信回调] 开启事务失败: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
        }
    };

    let amount = order.amount;
    // 数据库更新操作字段说明：
    // - status: 订单状态更新为 'paid' 表示支付成功
    // - trade_no: 保存微信支付交易流水号
    // - paid_at: 记录实际支付完成的时间（TIMESTAMPTZ）
    let now = DbTs::now();

    let result = sqlx::query(&state.db.format_query("UPDATE orders SET status = 'paid', trade_no = ?, paid_at = ? WHERE out_trade_no = ? AND status = 'pending'"))
        .bind(trade_no).bind(&now).bind(out_trade_no)
        .execute(&mut *tx).await;
    match result {
        Ok(r) if r.rows_affected() == 0 => {
            tracing::info!("[微信回调] 订单已被并发处理，跳过: {}", out_trade_no);
            let _ = tx.rollback().await;
            return (StatusCode::OK, Json(resp_success));
        }
        Err(e) => {
            tracing::warn!("[微信回调] 更新订单状态失败: {:?}", e);
            let _ = tx.rollback().await;
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
        }
        _ => {}
    }

    if let Err(e) = credit_paid_order(
        &state,
        &mut tx,
        &order.user_id,
        amount,
        "wechat",
        out_trade_no,
    )
    .await
    {
        tracing::warn!("[微信回调] 入账失败: {:?}", e);
        let _ = tx.rollback().await;
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("[微信回调] 事务提交失败: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(resp_fail));
    }

    tracing::info!(
        "[微信回调] ✅ 订单 {} 处理完成, 用户 {} 充值 {:.2} 元",
        out_trade_no,
        order.user_id,
        amount
    );
    crate::services::notification::spawn_low_balance_check(
        Arc::clone(&state),
        order.user_id.clone(),
    );

    (StatusCode::OK, Json(resp_success))
}

pub async fn alipay_notify(State(state): State<Arc<AppState>>, body: String) -> impl IntoResponse {
    tracing::info!("[支付宝回调] 收到回调通知, body长度: {}", body.len());
    tracing::debug!("[支付宝回调] 原始数据: {}", body);

    // 使用标准 form_urlencoded 解析，正确处理 %2B → + 等编码
    let params: BTreeMap<String, String> = form_urlencoded::parse(body.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    let sign = params.get("sign").cloned().unwrap_or_default();
    let out_trade_no = params.get("out_trade_no").cloned().unwrap_or_default();
    let trade_no = params.get("trade_no").cloned().unwrap_or_default();
    let trade_status = params.get("trade_status").cloned().unwrap_or_default();

    tracing::info!(
        "[支付宝回调] trade_status: {}, out_trade_no: {}, trade_no: {}",
        trade_status,
        out_trade_no,
        trade_no
    );

    if trade_status != "TRADE_SUCCESS" && trade_status != "TRADE_FINISHED" {
        tracing::info!("[支付宝回调] 非成功状态，忽略: {}", trade_status);
        return "success".to_string();
    }

    let alipay_setting: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = 'payment_alipay'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or_default();

    let config = match serde_json::from_str::<crate::models::PaymentAlipaySettings>(
        &alipay_setting.unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[支付宝回调] 解析配置失败: {:?}", e);
            return "fail".to_string();
        }
    };

    let cfg_app_id = config.app_id.clone();
    let client = AlipayClient::new(config);
    match client.verify_signature(&params, &sign) {
        Ok(true) => tracing::info!("[支付宝回调] 签名验证通过"),
        Ok(false) => {
            tracing::warn!("[支付宝回调] 签名验证失败");
            return "fail".to_string();
        }
        Err(e) => {
            tracing::warn!("[支付宝回调] 签名验证异常: {:?}", e);
            return "fail".to_string();
        }
    }

    // 前置检查：事务外查询
    let order: Option<Order> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM orders WHERE out_trade_no = ?"),
    )
    .bind(&out_trade_no)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    if order.is_none() {
        tracing::warn!("[支付宝回调] 订单不存在: {}", out_trade_no);
        return "success".to_string();
    }

    let order = order.unwrap();
    if order.status != "pending" {
        tracing::info!("[支付宝回调] 订单已处理过, 当前状态: {}", order.status);
        return "success".to_string();
    }

    // 业务核验（支付宝官方要求验签后必做）：渠道、app_id、total_amount（分）必须与本站订单一致
    if !settlement_verified(
        "支付宝回调",
        &out_trade_no,
        order.expected_pay_amount(),
        order.payment_method == "alipay",
        merchant_id_matches(&cfg_app_id, params.get("app_id").map(String::as_str)),
        params
            .get("total_amount")
            .map(String::as_str)
            .and_then(parse_yuan_to_fen),
    ) {
        return "fail".to_string();
    }

    // 事务处理
    let mut tx = match state.db.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("[支付宝回调] 开启事务失败: {:?}", e);
            return "fail".to_string();
        }
    };

    let amount = order.amount;
    // 数据库更新操作字段说明：
    // - status: 订单状态更新为 'paid' 表示支付成功
    // - trade_no: 保存支付宝交易流水号
    // - paid_at: 记录实际支付完成的时间（TIMESTAMPTZ）
    let now = DbTs::now();

    let result = sqlx::query(&state.db.format_query("UPDATE orders SET status = 'paid', trade_no = ?, paid_at = ? WHERE out_trade_no = ? AND status = 'pending'"))
        .bind(&trade_no).bind(&now).bind(&out_trade_no)
        .execute(&mut *tx).await;
    match result {
        Ok(r) if r.rows_affected() == 0 => {
            tracing::info!("[支付宝回调] 订单已被并发处理，跳过: {}", out_trade_no);
            let _ = tx.rollback().await;
            return "success".to_string();
        }
        Err(e) => {
            tracing::warn!("[支付宝回调] 更新订单失败: {:?}", e);
            let _ = tx.rollback().await;
            return "fail".to_string();
        }
        _ => {}
    }

    if let Err(e) = credit_paid_order(
        &state,
        &mut tx,
        &order.user_id,
        amount,
        "alipay",
        &out_trade_no,
    )
    .await
    {
        tracing::warn!("[支付宝回调] 入账失败: {:?}", e);
        let _ = tx.rollback().await;
        return "fail".to_string();
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("[支付宝回调] 事务提交失败: {:?}", e);
        return "fail".to_string();
    }

    tracing::info!(
        "[支付宝回调] ✅ 订单 {} 处理完成, 用户 {} 充值 {:.2} 元",
        out_trade_no,
        order.user_id,
        amount
    );
    crate::services::notification::spawn_low_balance_check(
        Arc::clone(&state),
        order.user_id.clone(),
    );

    "success".to_string()
}

pub async fn stripe_notify(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> impl IntoResponse {
    use axum::http::StatusCode;

    tracing::info!("[Stripe回调] 收到 Webhook 通知, body长度: {}", body.len());

    let resp_ok = (StatusCode::OK, "ok");
    let resp_fail = (StatusCode::BAD_REQUEST, "fail");

    // 1. 读取 Stripe 配置
    let stripe_setting: Option<String> = match sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = 'payment_stripe'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[Stripe回调] 读取配置失败: {:?}", e);
            return resp_fail;
        }
    };

    let config = match serde_json::from_str::<crate::models::PaymentStripeSettings>(
        &stripe_setting.unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[Stripe回调] 解析配置失败: {:?}", e);
            return resp_fail;
        }
    };

    // 2. 验证 Webhook 签名（必须携带签名头且校验通过，杜绝伪造）
    let sig_header = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let is_live_key = config.secret_key.starts_with("sk_live_");
    let client = StripeClient::new(config);

    if sig_header.is_empty()
        || !client
            .verify_webhook_signature(&body, sig_header)
            .unwrap_or(false)
    {
        tracing::warn!("[Stripe回调] 缺少签名或签名校验失败");
        return resp_fail;
    }

    // 3. 解析事件
    let event: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[Stripe回调] JSON 解析失败: {:?}", e);
            return resp_fail;
        }
    };

    let event_type = event["type"].as_str().unwrap_or("");
    tracing::info!("[Stripe回调] event type: {}", event_type);

    if event_type != "checkout.session.completed" {
        tracing::info!("[Stripe回调] 非 checkout.session.completed 事件，忽略");
        return resp_ok;
    }

    let session = &event["data"]["object"];
    let payment_status = session["payment_status"].as_str().unwrap_or("");
    let out_trade_no = session["client_reference_id"].as_str().unwrap_or("");
    let stripe_session_id = session["id"].as_str().unwrap_or("");

    tracing::info!(
        "[Stripe回调] payment_status: {}, out_trade_no: {}, session_id: {}",
        payment_status,
        out_trade_no,
        stripe_session_id
    );

    if payment_status != "paid" {
        tracing::info!("[Stripe回调] payment_status 非 paid: {}", payment_status);
        return resp_ok;
    }

    // 4. 前置检查
    let order: Option<Order> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM orders WHERE out_trade_no = ?"),
    )
    .bind(out_trade_no)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    if order.is_none() {
        tracing::warn!("[Stripe回调] 订单不存在: {}", out_trade_no);
        return resp_ok;
    }

    let order = order.unwrap();
    if order.status != "pending" {
        tracing::info!("[Stripe回调] 订单已处理过, 当前状态: {}", order.status);
        return resp_ok;
    }
    if order.payment_method != "stripe" {
        tracing::warn!(
            "[Stripe回调] 订单渠道不符，拒绝入账: out_trade_no={}, 订单渠道={}",
            out_trade_no,
            order.payment_method
        );
        return resp_fail;
    }

    // 5. 向 Stripe 官方 API 主动二次验单（双重安全防御：官方必须确认 paid 且订单号一致）
    if stripe_session_id.is_empty() {
        tracing::warn!("[Stripe回调] 回调中缺少 session_id");
        return resp_fail;
    }
    if let Some(ref bound_trade_no) = order.trade_no {
        if bound_trade_no != stripe_session_id {
            tracing::warn!(
                "[Stripe回调] 订单绑定的 session_id 不匹配: 原绑定={}, 当前回调={}",
                bound_trade_no,
                stripe_session_id
            );
            return resp_fail;
        }
    }
    let official = match client.get_session(stripe_session_id).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                "[Stripe回调] 官方 API 验单请求失败: id={}, err={:?}",
                stripe_session_id,
                e
            );
            return resp_fail;
        }
    };
    let official_status = official["payment_status"].as_str().unwrap_or("");
    let official_ref = official["client_reference_id"].as_str().unwrap_or("");
    let official_currency = official["currency"].as_str().unwrap_or("").to_lowercase();
    let official_amount = official["amount_total"].as_i64().unwrap_or(0);
    let session_livemode = official["livemode"].as_bool().unwrap_or(false);

    // 新订单按下单时落库的收款币种核验；旧订单（pay_currency 为空）沿用当前默认货币
    let expected_currency = match order.pay_currency.as_deref().map(str::trim) {
        Some(c) if !c.is_empty() => c.to_lowercase(),
        _ => crate::api::settings::get_currency_settings(&state)
            .await
            .default_currency
            .to_lowercase(),
    };
    let expected_minor = client.to_minor_units(order.expected_pay_amount(), &expected_currency);

    if official_status != "paid"
        || official_ref != out_trade_no
        || (is_live_key && !session_livemode)
        || official_currency != expected_currency
        || official_amount != expected_minor
    {
        tracing::warn!(
            "[Stripe回调] 官方验单未通过: id={}, 状态={}, 单号={}, 币种={}(需{}), 金额={}(需{}), livemode={}",
            stripe_session_id,
            official_status,
            official_ref,
            official_currency,
            expected_currency,
            official_amount,
            expected_minor,
            session_livemode
        );
        return resp_fail;
    }

    // 6. 事务处理
    let mut tx = match state.db.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("[Stripe回调] 开启事务失败: {:?}", e);
            return resp_fail;
        }
    };

    let amount = order.amount;
    // 数据库更新操作字段说明：
    // - status: 订单状态更新为 'paid' 表示支付成功
    // - trade_no: 保存 Stripe 的 session id，用以对账
    // - paid_at: 记录实际支付完成的时间（TIMESTAMPTZ）
    let now = DbTs::now();

    let result = sqlx::query(&state.db.format_query("UPDATE orders SET status = 'paid', trade_no = ?, paid_at = ? WHERE out_trade_no = ? AND status = 'pending'"))
        .bind(stripe_session_id).bind(&now).bind(out_trade_no)
        .execute(&mut *tx).await;
    match result {
        Ok(r) if r.rows_affected() == 0 => {
            tracing::info!("[Stripe回调] 订单已被并发处理，跳过: {}", out_trade_no);
            let _ = tx.rollback().await;
            return resp_ok;
        }
        Err(e) => {
            tracing::warn!("[Stripe回调] 更新订单状态失败: {:?}", e);
            let _ = tx.rollback().await;
            return resp_fail;
        }
        _ => {}
    }

    if let Err(e) = credit_paid_order(
        &state,
        &mut tx,
        &order.user_id,
        amount,
        "stripe",
        out_trade_no,
    )
    .await
    {
        tracing::warn!("[Stripe回调] 入账失败: {:?}", e);
        let _ = tx.rollback().await;
        return resp_fail;
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("[Stripe回调] 事务提交失败: {:?}", e);
        return resp_fail;
    }

    tracing::info!(
        "[Stripe回调] ✅ 订单 {} 处理完成, 用户 {} 充值 {:.2}",
        out_trade_no,
        order.user_id,
        amount
    );
    crate::services::notification::spawn_low_balance_check(
        Arc::clone(&state),
        order.user_id.clone(),
    );

    resp_ok
}

/// HyperBC 支付异步回调通知处理函数
/// 收到来自 HyperBC 的 POST 请求，对 body 签名进行平台公钥验签，然后对对应订单状态进行更新，并入账用户余额
pub async fn hyperbc_notify(State(state): State<Arc<AppState>>, body: String) -> impl IntoResponse {
    tracing::warn!("[HyperBC回调] 原始数据: {}", body);

    // 1. 解析回调 JSON 报文
    let body_val: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[HyperBC回调] JSON 解析失败: {:?}", e);
            return "fail".into_response();
        }
    };

    // 2. 提取并移出 sign 签名参数，供后续验签
    let sign = match body_val.get("sign").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            tracing::warn!("[HyperBC回调] 验签失败 [缺少签名字段]: 回调报文中缺少 sign 参数");
            return "fail".into_response();
        }
    };

    // 构造平铺的待签名数据：若包含 data 节点且为 Object，则直接将 data 节点的属性拉平到最外层进行验签
    let flat_body_val = if let Some(data_obj) = body_val.get("data").and_then(|d| d.as_object()) {
        serde_json::Value::Object(data_obj.clone())
    } else {
        body_val.clone()
    };

    // 3. 加载数据库配置进行验签
    let hyperbc_setting: Option<String> = match sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = 'payment_hyperbc'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("[HyperBC回调] 查询数据库配置失败: {:?}", e);
            return "fail".into_response();
        }
    };

    let config: crate::models::PaymentHyperbcSettings =
        match serde_json::from_str(&hyperbc_setting.unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[HyperBC回调] 解析 HyperBC 配置失败: {:?}", e);
                return "fail".into_response();
            }
        };

    if config.hyperbc_public_key.trim().is_empty() {
        tracing::warn!(
            "[HyperBC回调] 验签失败 [未配置平台公钥]: 系统未配置 HyperBC 平台公钥，拒绝处理"
        );
        return "fail".into_response();
    }

    let client = HyperbcClient::new(config.clone());
    let sign_content = HyperbcClient::get_sign_content_from_raw(&body)
        .unwrap_or_else(|_| HyperbcClient::get_sign_content(&flat_body_val));

    match client.verify_signature_str(&sign_content, &sign) {
        Ok(true) => {
            tracing::info!("[HyperBC回调] RSA 签名验证通过");
        }
        Ok(false) => {
            tracing::warn!(
                "[HyperBC回调] 验签失败 [签名不匹配]: RSA 签名校验未通过 (sign={}, sign_content={})",
                sign,
                sign_content
            );
            return "fail".into_response();
        }
        Err(e) => {
            tracing::warn!(
                "[HyperBC回调] 验签失败 [执行异常]: RSA 验签执行异常: {:?}",
                e
            );
            return "fail".into_response();
        }
    }

    // 4. 解析业务 data 节点为强类型 QueryOrderData
    let data_val = match body_val.get("data") {
        Some(d) => d,
        None => {
            tracing::warn!("[HyperBC回调] 报文错误 [缺少业务数据]: 回调报文中缺少 data 节点");
            return "fail".into_response();
        }
    };

    let notify_data: crate::api::plugins::pay::payment::hyperbc::QueryOrderData =
        match serde_json::from_value(data_val.clone()) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(
                    "[HyperBC回调] 报文错误 [结构解析失败]: 解析 data 节点失败: {:?}",
                    e
                );
                return "fail".into_response();
            }
        };

    let merchant_order_id = &notify_data.merchant_order_id;
    if merchant_order_id.is_empty() {
        tracing::warn!(
            "[HyperBC回调] 报文错误 [缺少订单号]: data 节点中缺少有效 merchant_order_id"
        );
        return "fail".into_response();
    }

    let status = notify_data.status;
    let check_code = notify_data.get_check_code();
    let actual_crypto = notify_data.get_actual_crypto_amount();

    tracing::info!(
        "[HyperBC回调] 订单号: {}, status: {}, check_code: {}, 实际到账代币: {}",
        merchant_order_id,
        status,
        check_code,
        actual_crypto
    );

    let ok_resp = || {
        (
            [(
                axum::http::header::CONTENT_TYPE,
                "application/json; charset=utf-8",
            )],
            client.success_response(),
        )
            .into_response()
    };

    // status = 10 表示已取消
    if status == 10 {
        let _ = sqlx::query(&state.db.format_query(
            "UPDATE orders SET status = 'cancelled' WHERE out_trade_no = ? AND status = 'pending'",
        ))
        .bind(merchant_order_id)
        .execute(&state.db.pool)
        .await;
        return ok_resp();
    }

    // 判断结算资格：
    // 1: 正常支付 (status=1 且 check_code=0)
    // 2: 异常支付差额入账 (status=2 且 check_code=1001 且链上确有代币到账 actual_crypto > 0)
    let should_settle = (status == 1 && check_code == 0)
        || (status == 2 && check_code == 1001 && actual_crypto > 0.0);

    if !should_settle {
        tracing::info!(
            "[HyperBC回调] 订单 {} status={} check_code={} 不满足自动入账条件，忽略或需人工复核",
            merchant_order_id,
            status,
            check_code
        );
        return ok_resp();
    }

    // 查询订单信息以提取期望金额和用户ID
    let order: Option<Order> = match sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM orders WHERE out_trade_no = ?"),
    )
    .bind(merchant_order_id)
    .fetch_optional(&state.db.pool)
    .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("[HyperBC回调] 查询订单失败: {:?}", e);
            return "fail".into_response();
        }
    };

    let order = match order {
        Some(o) => o,
        None => {
            tracing::warn!("[HyperBC回调] 订单不存在: {}", merchant_order_id);
            return "fail".into_response();
        }
    };

    // 幂等防护：如果订单状态已经是 paid (已支付)，直接返回 success 避免重复入账
    if order.status == "paid" {
        tracing::info!(
            "[HyperBC回调] 订单已是支付状态，幂等跳过, out_trade_no: {}",
            merchant_order_id
        );
        return ok_resp();
    }

    if order.payment_method != "hyperbc" {
        tracing::warn!(
            "[HyperBC回调] 订单渠道不符，拒绝入账: out_trade_no={}, 订单渠道={}",
            merchant_order_id,
            order.payment_method
        );
        return "fail".into_response();
    }

    // 核对平台流水号 trade_no（下单时由本商户 app_id 创建并落库的 order_no），防止跨商户同订单号小额回调伪造
    if !merchant_id_matches(
        order.trade_no.as_deref().unwrap_or(""),
        Some(notify_data.order_no.as_str()),
    ) {
        tracing::warn!(
            "[HyperBC回调] 平台订单号不匹配，疑似跨商户伪造回调，拒绝入账: out_trade_no={}, expected_trade_no={:?}, notify_order_no={}",
            merchant_order_id,
            order.trade_no,
            notify_data.order_no
        );
        return "fail".into_response();
    }

    // 计算实际结算金额（零亏损红线：实到金额乘汇率，且不得超过预设金额；无实收不自动入账）
    let Some(settle_amount) =
        hyperbc_settle_amount(actual_crypto, config.crypto_exchange_rate, order.amount)
    else {
        tracing::warn!(
            "[HyperBC回调] 订单 {} 无链上实收金额，不自动入账，需人工复核",
            merchant_order_id
        );
        return "fail".into_response();
    };

    // 调用公用入账函数进行状态更新与余额充值，保证业务逻辑的高内聚和 100% 严格防并发防重入
    match complete_hyperbc_payment(&state, merchant_order_id, settle_amount, &order.user_id).await {
        Ok(_) => ok_resp(),
        Err(e) => {
            tracing::warn!("[HyperBC回调] 处理订单支付更新失败: {:?}", e);
            "fail".into_response()
        }
    }
}

/// 通联支付异步回调通知接口
pub async fn allinpay_notify(
    State(state): State<Arc<AppState>>,
    body: String,
) -> impl IntoResponse {
    tracing::info!("[通联回调] 收到回调通知, body长度: {}", body.len());
    tracing::debug!("[通联回调] 原始数据: {}", body);

    // 通联发送回调是标准的 form-urlencoded 格式
    let params: BTreeMap<String, String> = form_urlencoded::parse(body.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    let sign = params.get("sign").cloned().unwrap_or_default();
    // 统一下单回传：优先 reqsn，兼容 cusorderid（文档定义为对应 reqsn）
    let out_trade_no = params
        .get("reqsn")
        .filter(|s| !s.is_empty())
        .or_else(|| params.get("cusorderid"))
        .cloned()
        .unwrap_or_default();
    let trxstatus = params.get("trxstatus").cloned().unwrap_or_default();

    tracing::info!(
        "[通联回调] trxstatus: {}, reqsn: {}",
        trxstatus,
        out_trade_no
    );

    // 通联定义 "0000" 代表支付成功
    if trxstatus != "0000" {
        tracing::info!("[通联回调] 交易状态未成功，忽略");
        return "success".into_response();
    }

    // 获取配置
    let allinpay_setting: Option<String> = match sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = 'payment_allinpay'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[通联回调] 读取配置失败: {:?}", e);
            return "fail".into_response();
        }
    };

    let config = match serde_json::from_str::<crate::models::PaymentAllinpaySettings>(
        &allinpay_setting.unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[通联回调] 解析配置失败: {:?}", e);
            return "fail".into_response();
        }
    };

    let (cfg_cusid, cfg_appid) = (config.cusid.clone(), config.appid.clone());
    let client = AllinpayClient::new(config);
    // 利用平台公钥校验回调签名，防止伪造攻击
    match client.verify_signature(&params, &sign) {
        Ok(true) => tracing::info!("[通联回调] 签名验证通过"),
        Ok(false) => {
            tracing::warn!("[通联回调] 签名验证失败");
            return "fail".into_response();
        }
        Err(e) => {
            tracing::warn!("[通联回调] 签名验证过程中出现异常: {:?}", e);
            return "fail".into_response();
        }
    }

    // 获取原订单以确认支付金额和充值人
    let order: Option<Order> = match sqlx::query_as(
        &state
            .db
            .format_query("SELECT * FROM orders WHERE out_trade_no = ?"),
    )
    .bind(&out_trade_no)
    .fetch_optional(&state.db.pool)
    .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("[通联回调] 查询订单失败: {:?}", e);
            return "fail".into_response();
        }
    };

    let order = match order {
        Some(o) => o,
        None => {
            tracing::warn!("[通联回调] 订单不存在: {}", out_trade_no);
            return "fail".into_response();
        }
    };

    // 幂等防护：订单已支付直接返回成功，防重复充值
    if order.status == "paid" {
        tracing::info!(
            "[通联回调] 订单已是支付状态，幂等跳过, out_trade_no: {}",
            out_trade_no
        );
        return "success".into_response();
    }

    // 业务核验：渠道、cusid/appid、trxamt（分）必须与本站订单一致。
    // 验签只证明报文来自通联，跨商户号的真实小额交易同样签名合法，必须靠此处拦截。
    if !settlement_verified(
        "通联回调",
        &out_trade_no,
        order.expected_pay_amount(),
        order.payment_method.starts_with("allinpay_"),
        merchant_id_matches(&cfg_cusid, params.get("cusid").map(String::as_str))
            && merchant_id_matches(&cfg_appid, params.get("appid").map(String::as_str)),
        params
            .get("trxamt")
            .and_then(|v| v.trim().parse::<i64>().ok()),
    ) {
        return "fail".into_response();
    }

    // 事务处理入账
    match complete_allinpay_payment(
        &state,
        &out_trade_no,
        order.amount,
        &order.user_id,
        &order.payment_method,
    )
    .await
    {
        Ok(_) => "success".into_response(),
        Err(e) => {
            tracing::warn!("[通联回调] 处理加额更新失败: {:?}", e);
            "fail".into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        effective_recharge_amount_limits, is_payment_channel_ui_enabled, merge_payment_channels_ui,
        PaymentChannelUiItem, PaymentChannelsUiSettings, PaymentGatewayEnableFlags,
    };

    #[test]
    fn normalize_amount_matches_channel_minor_unit() {
        // 分币种：多余小数四舍五入到分，渠道实收与入账一致
        assert_eq!(normalize_order_amount(10.004, "alipay", "usd"), 10.0);
        assert_eq!(normalize_order_amount(10.006, "wechat", "cny"), 10.01);
        assert_eq!(normalize_order_amount(0.004, "allinpay_wechat", "cny"), 0.0);
        assert_eq!(normalize_order_amount(10.004, "stripe", "usd"), 10.0);
        // Stripe 零小数币种：规整到整数
        assert_eq!(normalize_order_amount(100.4, "stripe", "jpy"), 100.0);
        assert_eq!(normalize_order_amount(0.4, "stripe", "JPY"), 0.0);
        // 非 Stripe 渠道不受零小数币种影响
        assert_eq!(normalize_order_amount(100.45, "hyperbc", "jpy"), 100.45);
        assert_eq!(normalize_order_amount(f64::INFINITY, "alipay", "cny"), 0.0);
    }

    fn all_gateways_on() -> PaymentGatewayEnableFlags {
        PaymentGatewayEnableFlags {
            wechat: true,
            alipay: true,
            stripe: true,
            hyperbc: true,
            allinpay: true,
        }
    }

    #[test]
    fn channel_ui_switch_applies_to_every_channel() {
        let saved = PaymentChannelsUiSettings {
            channels: vec![PaymentChannelUiItem {
                id: "alipay".to_string(),
                enabled: false,
                ..Default::default()
            }],
        };
        let ui = merge_payment_channels_ui(Some(saved), &all_gateways_on());
        assert!(!is_payment_channel_ui_enabled(&ui, "alipay"));
        // 未存 UI 配置的渠道交给网关 enabled 判定
        assert!(is_payment_channel_ui_enabled(&ui, "wechat"));
        assert!(is_payment_channel_ui_enabled(&ui, "stripe"));
    }

    #[test]
    fn recharge_limit_uses_channel_when_enabled_else_global() {
        let saved = PaymentChannelsUiSettings {
            channels: vec![
                PaymentChannelUiItem {
                    id: "alipay".to_string(),
                    recharge_limit_enabled: true,
                    min_recharge_amount: 20.0,
                    max_recharge_amount: 200.0,
                    ..Default::default()
                },
                PaymentChannelUiItem {
                    id: "wechat".to_string(),
                    recharge_limit_enabled: false,
                    min_recharge_amount: 1.0,
                    max_recharge_amount: 2.0,
                    ..Default::default()
                },
                PaymentChannelUiItem {
                    id: "allinpay".to_string(),
                    recharge_limit_enabled: true,
                    min_recharge_amount: 0.0,
                    max_recharge_amount: 50.0,
                    ..Default::default()
                },
            ],
        };
        let ui = merge_payment_channels_ui(Some(saved), &all_gateways_on());
        assert_eq!(
            effective_recharge_amount_limits(&ui, "alipay", 5.0, 10000.0),
            (20.0, 200.0)
        );
        // 开关关闭时忽略渠道上填写的数字，回落到全局
        assert_eq!(
            effective_recharge_amount_limits(&ui, "wechat", 5.0, 10000.0),
            (5.0, 10000.0)
        );
        assert_eq!(
            effective_recharge_amount_limits(&ui, "stripe", 5.0, 10000.0),
            (5.0, 10000.0)
        );
        assert_eq!(
            effective_recharge_amount_limits(&ui, "allinpay_wechat", 5.0, 10000.0),
            (0.0, 50.0)
        );
        assert_eq!(
            effective_recharge_amount_limits(&ui, "allinpay_alipay", 5.0, 10000.0),
            (0.0, 50.0)
        );
    }

    #[test]
    fn settlement_rejects_cross_merchant_amount_and_channel_mismatch() {
        // 正常：渠道一致、商户一致、实收分 == 订单分
        assert!(settlement_verified(
            "t",
            "o",
            10000.0,
            true,
            true,
            Some(1_000_000)
        ));
        // 跨商户号的真实小额交易（签名合法）：商户不符 + 金额不符
        assert!(!settlement_verified(
            "t",
            "o",
            10000.0,
            true,
            false,
            Some(1)
        ));
        // 本商户但金额不符
        assert!(!settlement_verified("t", "o", 10000.0, true, true, Some(1)));
        // 渠道不符
        assert!(!settlement_verified(
            "t",
            "o",
            10.0,
            false,
            true,
            Some(1000)
        ));
        // 回调缺失金额字段
        assert!(!settlement_verified("t", "o", 10.0, true, true, None));
        // 历史订单金额 > 2 位小数：与下单时 round(amount*100) 口径一致
        assert!(settlement_verified(
            "t",
            "o",
            10.004,
            true,
            true,
            Some(1000)
        ));
    }

    #[test]
    fn merchant_id_and_yuan_parsing() {
        assert!(merchant_id_matches("5501234", Some(" 5501234 ")));
        assert!(!merchant_id_matches("5501234", Some("5509999")));
        assert!(!merchant_id_matches("5501234", None));
        // 本站未配置商户号时一律不匹配
        assert!(!merchant_id_matches("", Some("")));
        assert_eq!(parse_yuan_to_fen("10.00"), Some(1000));
        assert_eq!(parse_yuan_to_fen("0.01"), Some(1));
        assert_eq!(parse_yuan_to_fen("abc"), None);
    }

    #[test]
    fn hyperbc_settle_requires_actual_crypto_and_caps_order_amount() {
        // 无实收明细：不再按订单全额入账
        assert_eq!(hyperbc_settle_amount(0.0, 7.0, 700.0), None);
        assert_eq!(hyperbc_settle_amount(f64::NAN, 7.0, 700.0), None);
        // 按实收 × 汇率，封顶订单金额
        assert_eq!(hyperbc_settle_amount(10.0, 7.0, 700.0), Some(70.0));
        assert_eq!(hyperbc_settle_amount(200.0, 7.0, 700.0), Some(700.0));
        // 汇率未配置沿用 1.0
        assert_eq!(hyperbc_settle_amount(5.0, 0.0, 700.0), Some(5.0));
    }

    fn currency_with_manual(default: &str, rates: &[(&str, f64)]) -> CurrencySettings {
        let mut cur = CurrencySettings {
            default_currency: default.to_string(),
            ..Default::default()
        };
        cur.fx.live_enabled = true;
        cur.fx.currencies = rates
            .iter()
            .map(|(code, rate)| crate::models::FxCurrencyConfig {
                code: code.to_string(),
                manual: true,
                manual_rate: *rate,
                adjust: String::new(),
                adjust_value: 0.0,
                display: true,
            })
            .collect();
        cur
    }

    fn ui_with_currencies(id: &str, currencies: &[&str]) -> PaymentChannelsUiSettings {
        let saved = PaymentChannelsUiSettings {
            channels: vec![PaymentChannelUiItem {
                id: id.to_string(),
                enabled: true,
                currencies: Some(currencies.iter().map(|c| c.to_string()).collect()),
                ..Default::default()
            }],
        };
        merge_payment_channels_ui(Some(saved), &all_gateways_on())
    }

    #[test]
    fn quote_same_currency_keeps_amount() {
        let cur = currency_with_manual("CNY", &[]);
        let ui = merge_payment_channels_ui(None, &all_gateways_on());
        let q = quote_order("wechat", 100.0, &cur, &ui, None, 0).unwrap();
        assert_eq!(q.currency, "CNY");
        assert_eq!(q.amount, 100.0);
        assert_eq!(q.fx_rate, None);
    }

    #[test]
    fn quote_converts_cny_only_channels_and_rounds_up() {
        let cur = currency_with_manual("USD", &[("CNY", 7.123)]);
        let ui = merge_payment_channels_ui(None, &all_gateways_on());
        let q = quote_order("allinpay_wechat", 10.0, &cur, &ui, None, 0).unwrap();
        assert_eq!(q.currency, "CNY");
        assert_eq!(q.amount, 71.23);
        assert_eq!(q.fx_rate, Some(7.123));
        let q = quote_order("alipay", 0.333, &cur, &ui, None, 0).unwrap();
        assert_eq!(q.amount, 2.38);
    }

    #[test]
    fn quote_rejects_cross_currency_without_rate() {
        let cur = currency_with_manual("USD", &[]);
        let ui = merge_payment_channels_ui(None, &all_gateways_on());
        assert!(quote_order("wechat", 10.0, &cur, &ui, None, 0).is_err());
        assert!(quote_order("unknown", 10.0, &cur, &ui, None, 0).is_err());
    }

    #[test]
    fn quote_stripe_uses_configured_currency_minor_unit() {
        let cur = currency_with_manual("CNY", &[("JPY", 20.01)]);
        let ui = ui_with_currencies("stripe", &["JPY"]);
        let q = quote_order("stripe", 10.0, &cur, &ui, None, 0).unwrap();
        assert_eq!(q.currency, "JPY");
        assert_eq!(q.amount, 201.0);
    }

    #[test]
    fn quote_hyperbc_always_uses_default_currency() {
        let cur = currency_with_manual("JPY", &[("USD", 0.007)]);
        let ui = ui_with_currencies("hyperbc", &["USD"]);
        let q = quote_order("hyperbc", 1000.0, &cur, &ui, None, 0).unwrap();
        assert_eq!(q.currency, "JPY");
        assert_eq!(q.amount, 1000.0);
    }

    #[test]
    fn public_channels_disable_cross_currency_without_rate() {
        let cur = currency_with_manual("USD", &[]);
        let ui = merge_payment_channels_ui(None, &all_gateways_on());
        let channels = crate::models::build_public_payment_channels(&ui, &all_gateways_on());
        let channels = annotate_public_channels(channels, &ui, &cur, None, 0);
        let wechat = channels.iter().find(|c| c.id == "wechat").unwrap();
        assert!(!wechat.enabled);
        assert_eq!(wechat.pay_currency.as_deref(), Some("CNY"));
        let stripe = channels.iter().find(|c| c.id == "stripe").unwrap();
        assert!(stripe.enabled);
        assert_eq!(stripe.pay_rate, Some(1.0));
        assert_eq!(stripe.pay_currency.as_deref(), Some("USD"));
    }
}
