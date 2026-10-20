/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! SitePortal plugin — public portal pages + admin config.

use crate::{
    auth,
    error::{AppError, AppResult},
    AppState,
};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

/// 管理端路由（需认证）
pub fn admin_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/portal-config",
            get(get_portal_config).post(save_portal_config),
        )
        .route("/generate", post(generate_static))
}

/// 门户页面路由（公开，无需认证，直接渲染 HTML）
pub fn portal_pages_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(page_home))
        .route("/contact", get(page_contact))
        .route("/about", get(page_about))
        .fallback(page_fallback)
}

async fn page_fallback(
    State(state): State<Arc<AppState>>,
    uri: axum::http::Uri,
) -> Result<axum::response::Response, AppError> {
    let path = uri.path().trim_end_matches('/');
    if path.is_empty() {
        return Ok(page_home(State(state)).await?.into_response());
    }
    if path == "/about" {
        return Ok(page_about(State(state)).await?.into_response());
    }
    if path == "/contact" {
        return Ok(page_contact(State(state)).await?.into_response());
    }
    Ok((StatusCode::NOT_FOUND, axum::response::Html(String::new())).into_response())
}

// ─── 公开页面渲染 ───

const PORTAL_DISABLED_HTML: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>门户未启用 | Portal Not Enabled</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    background: #09090b;
    color: #fafafa;
    font-family: Inter, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    padding: 24px;
    text-align: center;
  }
  .container {
    max-width: 480px;
    padding: 40px 32px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 16px;
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(12px);
  }
  .icon {
    width: 56px;
    height: 56px;
    margin: 0 auto 20px;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.06);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #a1a1aa;
  }
  h1 {
    font-size: 24px;
    font-weight: 600;
    line-height: 1.3;
    margin-bottom: 6px;
    color: #f4f4f5;
  }
  .subtitle {
    font-size: 16px;
    font-weight: 500;
    color: #a1a1aa;
    margin-bottom: 16px;
  }
  p {
    font-size: 14px;
    color: #71717a;
    line-height: 1.6;
  }
</style>
</head>
<body>
  <div class="container">
    <div class="icon">
      <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"></circle>
        <line x1="12" y1="8" x2="12" y2="12"></line>
        <line x1="12" y1="16" x2="12.01" y2="16"></line>
      </svg>
    </div>
    <h1>门户未启用</h1>
    <div class="subtitle">Portal Not Enabled</div>
    <p>请前往后台管理系统的插件中心开启站点门户插件。</p>
    <p style="margin-top:4px;">Please enable the Site Portal plugin in the admin dashboard.</p>
  </div>
</body>
</html>"#;

async fn is_portal_enabled(state: &AppState) -> Result<bool, AppError> {
    let enabled: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT is_enabled FROM plugins WHERE name = 'site_portal'"),
    )
    .fetch_optional(&state.db.pool)
    .await?;
    Ok(enabled == Some(1))
}

async fn render_public_page(
    state: &AppState,
    page: &str,
    current_mid: Option<String>,
) -> Result<axum::response::Html<String>, AppError> {
    if !is_portal_enabled(state).await? {
        return Ok(axum::response::Html(PORTAL_DISABLED_HTML.to_string()));
    }

    let configs = load_configs(state).await?;
    let mut portal_data = build_portal_data(state, &configs).await?;
    portal_data.insert("current_page", &page);
    if let Some(mid) = current_mid {
        portal_data.insert("current_mid", &mid);
    }

    let mut tera = tera::Tera::default();
    register_templates(&mut tera)?;

    let html = render_page(&tera, page, &portal_data)?;
    Ok(axum::response::Html(html))
}

async fn page_home(
    State(state): State<Arc<AppState>>,
) -> Result<axum::response::Html<String>, AppError> {
    if !is_portal_enabled(&state).await? {
        return Ok(axum::response::Html(PORTAL_DISABLED_HTML.to_string()));
    }

    let configs = load_configs(&state).await?;
    Ok(axum::response::Html(
        render_homepage_html(&state, &configs).await?,
    ))
}

async fn page_contact(
    State(state): State<Arc<AppState>>,
) -> Result<axum::response::Html<String>, AppError> {
    render_public_page(&state, "contact", None).await
}
async fn page_about(
    State(state): State<Arc<AppState>>,
) -> Result<axum::response::Html<String>, AppError> {
    render_public_page(&state, "about", None).await
}

// ─── 辅助 ───

async fn require_admin(state: &AppState, claims: &auth::Claims) -> Result<(), AppError> {
    let role: String =
        sqlx::query_scalar(&state.db.format_query("SELECT role FROM users WHERE id = ?"))
            .bind(&claims.sub)
            .fetch_one(&state.db.pool)
            .await
            .map_err(|_| AppError::Unauthorized)?;
    if role != "admin" {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

async fn load_configs(
    state: &AppState,
) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
    crate::api::plugins::load_plugin_configs_pub(state, "site_portal").await
}

/// 经典科技风格实际页面：首页、系统模型广场、联系我们、关于我们。
const SYSTEM_MARKETPLACE_PATH: &str = "/home/models";

const FIXED_NAV_SPECS: [(&str, &str, &str, i64); 4] = [
    ("home", "站点首页|Home", "/home", 40),
    (
        "marketplace",
        "模型广场|Model Marketplace",
        SYSTEM_MARKETPLACE_PATH,
        30,
    ),
    ("contact", "联系我们|Contact Us", "/home/contact", 20),
    ("about", "关于我们|About Us", "/home/about", 10),
];

fn default_fixed_nav_item(key: &str, label: &str, path: &str, sort: i64) -> serde_json::Value {
    json!({
        "label": label,
        "path": path,
        "enabled": true,
        "target_blank": false,
        "key": key,
        "is_fixed": true,
        "sort": sort
    })
}

fn default_portal_nav_items() -> Vec<serde_json::Value> {
    FIXED_NAV_SPECS
        .iter()
        .map(|(key, label, path, sort)| default_fixed_nav_item(key, label, path, *sort))
        .collect()
}

fn default_portal_nav_config() -> serde_json::Value {
    json!({
        "logo_url": "",
        "logo_text": "Tkeapi",
        "logo_link": "/home",
        "items": default_portal_nav_items(),
        "cta_text": "登录|Login",
        "cta_link": "/login",
        "register_text": "立即注册|Get API Key",
        "register_link": "/register",
        "show_model_search": true
    })
}

fn marketplace_nav_path(nav: &serde_json::Value) -> String {
    nav.get("items")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .find(|item| item.get("key").and_then(|k| k.as_str()) == Some("marketplace"))
        .and_then(|item| item.get("path").and_then(|p| p.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(SYSTEM_MARKETPLACE_PATH)
        .to_string()
}

fn is_portal_models_path(path: &str) -> bool {
    matches!(
        path,
        "#models" | "models" | "/portal/models" | "/portal/models/" | "/home/models/"
    )
}

fn is_fixed_nav_key(key: &str) -> bool {
    FIXED_NAV_SPECS.iter().any(|(k, _, _, _)| *k == key)
}

fn rewrite_packaged_default_nav(nav: &mut serde_json::Value) {
    let Some(items) = nav.get("items").and_then(|v| v.as_array()) else {
        return;
    };
    let keys: Vec<&str> = items
        .iter()
        .filter_map(|item| item.get("key").and_then(|k| k.as_str()))
        .collect();
    if keys.len() != items.len() || keys.is_empty() {
        return;
    }
    let packaged = keys == ["contact", "about", "models", "integration", "marketplace"]
        || keys
            == [
                "features",
                "carousel",
                "models",
                "integration",
                "marketplace",
            ]
        || keys == ["marketplace", "integration", "contact", "about"]
        || keys == ["home", "marketplace", "integration", "contact", "about"];
    if packaged {
        nav["items"] = json!(default_portal_nav_items());
    }
}

/// 去掉经典科技页上不存在的锚点栏目，模型入口一律指向系统模型广场。
fn align_nav_with_classic_pages(nav: &mut serde_json::Value) {
    let Some(items) = nav.get_mut("items").and_then(|v| v.as_array_mut()) else {
        return;
    };
    items.retain(|item| {
        let key = item.get("key").and_then(|k| k.as_str()).unwrap_or("");
        let path = item.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let dead_anchor = path.is_empty() || path.starts_with('#');
        !(matches!(key, "integration" | "features" | "carousel") && dead_anchor)
            && !(key == "models" && path.starts_with('#'))
    });
    let has_marketplace = items
        .iter()
        .any(|item| item.get("key").and_then(|k| k.as_str()) == Some("marketplace"));
    if has_marketplace {
        items.retain(|item| item.get("key").and_then(|k| k.as_str()) != Some("models"));
    }
    for item in items.iter_mut() {
        let key = item.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
        let path = item.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
        if key == "marketplace" && (path.is_empty() || is_portal_models_path(&path)) {
            item["path"] = json!(SYSTEM_MARKETPLACE_PATH);
        }
        if key == "models" && (is_portal_models_path(&path) || path == SYSTEM_MARKETPLACE_PATH) {
            item["key"] = json!("marketplace");
            item["path"] = json!(SYSTEM_MARKETPLACE_PATH);
            item["is_fixed"] = json!(true);
        }
    }
}

fn ensure_fixed_nav_items(nav: &mut serde_json::Value) {
    let Some(items) = nav.get_mut("items").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for item in items.iter_mut() {
        if item
            .get("key")
            .and_then(|k| k.as_str())
            .is_some_and(is_fixed_nav_key)
        {
            item["is_fixed"] = json!(true);
        }
    }
    for (key, label, path, sort) in FIXED_NAV_SPECS {
        let exists = items
            .iter()
            .any(|item| item.get("key").and_then(|k| k.as_str()) == Some(key));
        if !exists {
            items.push(default_fixed_nav_item(key, label, path, sort));
        }
    }
}

fn json_sort_weight(item: &serde_json::Value) -> i64 {
    item.get("sort")
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().map(|n| n as i64))
                .or_else(|| v.as_f64().map(|n| n.round() as i64))
        })
        .unwrap_or(0)
}

fn sort_json_items_by_weight_desc(items: &mut Vec<serde_json::Value>) {
    let len = items.len();
    for (i, item) in items.iter_mut().enumerate() {
        if item.get("sort").is_none() {
            item["sort"] = json!(((len - i) * 10) as i64);
        }
    }
    items.sort_by(|a, b| json_sort_weight(b).cmp(&json_sort_weight(a)));
}

fn apply_contact_sorts(columns: &mut serde_json::Value) {
    let Some(content) = columns.pointer_mut("/contact/content") else {
        return;
    };
    if let Some(links) = content
        .get_mut("social_links")
        .and_then(|v| v.as_array_mut())
    {
        sort_json_items_by_weight_desc(links);
    }
    const CHANNEL_KEYS: [&str; 3] = ["email", "phone", "address"];
    let mut keys: Vec<String> = content
        .get("channel_order")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .filter(|k| CHANNEL_KEYS.contains(&k.as_str()))
                .collect()
        })
        .unwrap_or_default();
    for key in CHANNEL_KEYS {
        if !keys.iter().any(|k| k == key) {
            keys.push(key.to_string());
        }
    }
    let sort_map = content.get("channel_sort").and_then(|v| v.as_object());
    let mut ranked: Vec<(String, i64, usize)> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let weight = sort_map
                .and_then(|m| m.get(key))
                .and_then(|v| {
                    v.as_i64()
                        .or_else(|| v.as_u64().map(|n| n as i64))
                        .or_else(|| v.as_f64().map(|n| n.round() as i64))
                })
                .unwrap_or(((keys.len() - i) * 10) as i64);
            (key.clone(), weight, i)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    if let Some(obj) = content.as_object_mut() {
        obj.insert(
            "channel_order".to_string(),
            json!(ranked
                .into_iter()
                .map(|(k, _, _)| k)
                .collect::<Vec<String>>()),
        );
    }
}

fn default_portal_footer_config() -> serde_json::Value {
    json!({
        "brand_name": "Tkeapi",
        "description": "Tkeapi 是一个开源的 AI 大模型 API 中转平台系统，通过一个统一端点聚合全球前沿大模型通道，提供极速路由、高可用容灾与极低成本的模型中转与分发能力。|Tkeapi is an open-source AI model API relay platform system. It aggregates global frontier LLMs through one unified endpoint, providing high-availability routing and low-cost model distribution.",
        "links_title": "产品与服务|Products & Services",
        "links": [
            {"label": "站点首页|Home", "path": "/home", "enabled": true},
            {"label": "模型广场|Model Marketplace", "path": "/home/models", "enabled": true},
            {"label": "联系我们|Contact Us", "path": "/home/contact", "enabled": true},
            {"label": "关于我们|About Us", "path": "/home/about", "enabled": true}
        ],
        "news_title": "开发者资讯|Developer News",
        "news_description": "订阅每周通讯，获取最新全球 AI 模型折扣、高可用路由升级和开发洞察。|Subscribe to our weekly newsletter for the latest global AI model discounts, high-availability route upgrades, and development insights.",
        "copyright": "© 2026 TkeAPI. All rights reserved.",
        "company_name": "Sexy Velora LLC",
        "company_address": "30 N Gould St Ste R, Sheridan, WY 82801",
        "icp_number": "",
        "terms_enabled": true,
        "terms_text": "服务条款|Terms of Service",
        "terms_link": "/legal/terms",
        "privacy_enabled": true,
        "privacy_text": "隐私政策|Privacy Policy",
        "privacy_link": "/legal/privacy"
    })
}

fn default_portal_home_config() -> serde_json::Value {
    json!({"hero_title":"一个接口，调用全球数百个 AI 模型","hero_subtitle":"OpenAI兼容和原生格式，极速接入主流模型，零门槛开始","features":[
            {
                "icon": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M12 22v-5\"/><path d=\"M9 8V2\"/><path d=\"M15 8V2\"/><path d=\"M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z\"/></svg>",
                "title": "一键极速接入",
                "description": "OpenAI 兼容 API 格式，只需修改 Base URL 和 Key，即可无缝替换至数百个主流模型，零代码成本迁移。"
            },
            {
                "icon": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20\"/><path d=\"M2 12h20\"/></svg>",
                "title": "全球模型全面覆盖",
                "description": "聚合 OpenAI、Anthropic、Google Gemini、DeepSeek、字节跳动火山引擎等数十家顶级服务商的模型矩阵。"
            },
            {
                "icon": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14H4z\"/></svg>",
                "title": "智能分流与高可用",
                "description": "全球边缘节点路由，支持在主渠道高负载或故障时自动无感容灾重试，首字耗时降至毫秒级。"
            },
            {
                "icon": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z\"/><path d=\"m9 12 2 2 4-4\"/></svg>",
                "title": "极致安全数据脱敏",
                "description": "支持 IP 白名单防刷保护，端到端高强度加密传输，异步任务与计费明细日志支持 Base64 数据隐私脱敏。"
            }
        ],"api_base_url":"","cta_title":"准备好开始构建了吗？","cta_description":"只需 3 分钟即可获取您的 API 密钥并开始创新。基础 URL：https://api.artsapi.com/api，零成本平滑迁移。","cta_primary_btn_text":"开始对话","cta_primary_btn_link":"https://api.artsapi.com","cta_secondary_btn_text":"阅读文档","cta_secondary_btn_link":"https://docs.artsapi.com"})
}

fn default_portal_columns_config() -> serde_json::Value {
    json!({
        "models": {"title": "模型广场", "path": "/home/models", "enabled": true},
        "contact": {
            "title": "联系我们",
            "path": "contact",
            "enabled": true,
            "content": {
                "email": "",
                "phone": "",
                "address": "",
                "email_enabled": true,
                "phone_enabled": true,
                "address_enabled": true,
                "channel_sort": {"email": 30, "phone": 20, "address": 10},
                "custom_content": "",
                "social_links": [
                    {"platform": "Twitter / X", "value": "https://x.com/tkeapi", "enabled": true, "sort": 30},
                    {"platform": "GitHub", "value": "https://github.com/aiqachat/tkeapi", "enabled": true, "sort": 20},
                    {"platform": "Telegram", "value": "https://t.me/tokensbyte", "enabled": true, "sort": 10}
                ],
                "items": [
                    {"icon":"<svg viewBox='0 0 24 24' fill='currentColor' xmlns='http://www.w3.org/2000/svg'><path d='M3 3H21C21.5523 3 22 3.44772 22 4V20C22 20.5523 21.5523 21 21 21H3C2.44772 21 2 20.5523 2 20V4C2 3.44772 2.44772 3 3 3ZM12.0606 11.6829L5.64722 6.2377L4.35278 7.7623L12.0731 14.3171L19.6544 7.75616L18.3456 6.24384L12.0606 11.6829Z'/></svg>","title":"邮箱","value":""},
                    {"icon":"<svg viewBox='0 0 24 24' fill='currentColor' xmlns='http://www.w3.org/2000/svg'><path d='M21 16.42V19.9561C21 20.4811 20.5941 20.9167 20.0705 20.9537C19.6331 20.9846 19.2763 21 19 21C10.1634 21 3 13.8366 3 5C3 4.72371 3.01545 4.36687 3.04635 3.9295C3.08337 3.40588 3.51894 3 4.04386 3H7.5801C7.83678 3 8.05176 3.19442 8.07753 3.4498C8.10067 3.67907 8.12218 3.86314 8.14207 4.00202C8.34435 5.41472 8.75753 6.75936 9.3487 8.00303C9.44359 8.20265 9.38171 8.44159 9.20185 8.57006L7.04355 10.1118C8.35752 13.1811 10.8189 15.6425 13.8882 16.9565L15.4271 14.8019C15.5572 14.6199 15.799 14.5573 16.001 14.6532C17.2446 15.2439 18.5891 15.6566 20.0016 15.8584C20.1396 15.8782 20.3225 15.8995 20.5502 15.9225C20.8056 15.9483 21 16.1633 21 16.42Z'/></svg>","title":"电话","value":""},
                    {"icon":"<svg viewBox='0 0 24 24' fill='currentColor' xmlns='http://www.w3.org/2000/svg'><path d='M18.364 17.364L12 23.7279L5.63604 17.364C2.12132 13.8492 2.12132 8.15076 5.63604 4.63604C9.15076 1.12132 14.8492 1.12132 18.364 4.63604C21.8787 8.15076 21.8787 13.8492 18.364 17.364ZM12 15C14.2091 15 16 13.2091 16 11C16 8.79086 14.2091 7 12 7C9.79086 7 8 8.79086 8 11C8 13.2091 9.79086 15 12 15ZM12 13C10.8954 13 10 12.1046 10 11C10 9.89543 10.8954 9 12 9C13.1046 9 14 9.89543 14 11C14 12.1046 13.1046 13 12 13Z'/></svg>","title":"地址","value":""}
                ]
            }
        },
        "about": {
            "title": "关于我们",
            "path": "about",
            "enabled": true,
            "hero_subtitle": "作为业内领先的 AI 模型聚合服务平台，我们消除服务商锁定风险，通过更优的价格、高可用性和企业级的稳定性，为您打通全球最先进大语言模型的访问桥梁。",
            "stat_tokens": "10B+",
            "stat_developers": "10K+",
            "stat_providers": "40+",
            "stat_models": "200+",
            "press_title": "统一大模型接口体验，让接入 AI 零负担",
            "press_desc": "旨在成为开发者接入、对比、调用不同 AI 模型的首选聚合网关。只需一次接入，即可全量访问数百个前沿模型。",
            "press_link_text": "联系我们洽谈合作 →",
            "content": ""
        }
    })
}

fn merge_portal_defaults(value: &mut serde_json::Value, defaults: &serde_json::Value) {
    if let (Some(current), Some(default_map)) = (value.as_object_mut(), defaults.as_object()) {
        for (key, default_value) in default_map {
            current
                .entry(key.clone())
                .or_insert_with(|| default_value.clone());
        }
    }
}

fn merge_home_config_patch(home: &mut serde_json::Value, patch: &serde_json::Value) {
    let Some(base) = home.as_object_mut() else {
        return;
    };
    let Some(patch_obj) = patch.as_object() else {
        return;
    };
    for (k, v) in patch_obj {
        if k == "api_base_url" {
            let trimmed = v.as_str().map(str::trim).unwrap_or("");
            base.insert(k.clone(), json!(trimmed));
        } else {
            base.insert(k.clone(), v.clone());
        }
    }
}

fn stored_portal_home_config(
    configs: &std::collections::HashMap<String, String>,
) -> serde_json::Value {
    let defaults = default_portal_home_config();
    let mut home = configs
        .get("home_config")
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .unwrap_or_else(|| defaults.clone());
    merge_portal_defaults(&mut home, &defaults);
    rewrite_whatstoken_brand(&mut home);
    home
}

fn portal_columns_config(configs: &std::collections::HashMap<String, String>) -> serde_json::Value {
    let defaults = default_portal_columns_config();
    let mut columns = configs
        .get("columns_config")
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .unwrap_or_else(|| defaults.clone());
    merge_portal_defaults(&mut columns, &defaults);
    for key in ["models", "contact", "about"] {
        if let (Some(current), Some(default_value)) = (columns.get_mut(key), defaults.get(key)) {
            merge_portal_defaults(current, default_value);
        }
    }
    if let (Some(content), Some(default_content)) = (
        columns.pointer_mut("/contact/content"),
        defaults.pointer("/contact/content"),
    ) {
        merge_portal_defaults(content, default_content);
    }
    let social_empty = columns
        .pointer("/contact/content/social_links")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.is_empty()
                || arr.iter().all(|item| {
                    let platform = item
                        .get("platform")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    let value = item
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    platform.is_empty() && value.is_empty()
                })
        })
        .unwrap_or(true);
    if social_empty {
        if let Some(links) = columns.pointer_mut("/contact/content/social_links") {
            *links = defaults
                .pointer("/contact/content/social_links")
                .cloned()
                .unwrap_or_else(|| json!([]));
        }
    }
    if let Some(links) = columns
        .pointer_mut("/contact/content/social_links")
        .and_then(|v| v.as_array_mut())
    {
        for item in links {
            if let Some(obj) = item.as_object_mut() {
                obj.entry("enabled".to_string()).or_insert(json!(true));
                if obj.get("value").and_then(|v| v.as_str()) == Some("https://x.com/tokensbyte") {
                    obj.insert("value".to_string(), json!("https://x.com/tkeapi"));
                }
            }
        }
    }
    if columns.pointer("/models/title").and_then(|v| v.as_str()) == Some("模型数据") {
        columns["models"]["title"] = json!("模型广场");
    }
    if is_portal_models_path(
        columns
            .pointer("/models/path")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    ) {
        columns["models"]["path"] = json!(SYSTEM_MARKETPLACE_PATH);
    }
    apply_contact_sorts(&mut columns);
    rewrite_whatstoken_brand(&mut columns);
    columns
}

/// 将已保存配置中的旧品牌文案 WhatsToken AI 替换为 Tkeapi。
fn rewrite_whatstoken_brand(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) if s.contains("WhatsToken AI") => {
            *s = s.replace("WhatsToken AI", "Tkeapi");
        }
        serde_json::Value::Array(items) => {
            for item in items {
                rewrite_whatstoken_brand(item);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                rewrite_whatstoken_brand(v);
            }
        }
        _ => {}
    }
}

fn portal_nav_config(configs: &std::collections::HashMap<String, String>) -> serde_json::Value {
    let defaults = default_portal_nav_config();
    let mut nav = configs
        .get("nav_config")
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .unwrap_or_else(|| defaults.clone());

    let is_legacy_default = nav
        .get("items")
        .and_then(|items| items.as_array())
        .map(|items| {
            let keys: Vec<&str> = items
                .iter()
                .filter_map(|item| item.get("key").and_then(|key| key.as_str()))
                .collect();
            !keys.is_empty()
                && keys.len() == items.len()
                && keys
                    .iter()
                    .all(|key| matches!(*key, "home" | "models" | "contact" | "about"))
        })
        .unwrap_or(false);

    if is_legacy_default {
        if let (Some(nav_map), Some(default_map)) = (nav.as_object_mut(), defaults.as_object()) {
            nav_map.insert("items".to_string(), default_map["items"].clone());
            if nav_map.get("logo_text").and_then(|v| v.as_str()) == Some("WhatsToken AI") {
                nav_map.insert("logo_text".to_string(), json!("Tkeapi"));
            }
        }
    }
    merge_portal_defaults(&mut nav, &defaults);
    rewrite_whatstoken_brand(&mut nav);
    migrate_features_carousel_nav_to_contact_about(&mut nav);
    rewrite_packaged_default_nav(&mut nav);
    align_nav_with_classic_pages(&mut nav);
    ensure_fixed_nav_items(&mut nav);
    if let Some(items) = nav.get_mut("items").and_then(|v| v.as_array_mut()) {
        sort_json_items_by_weight_desc(items);
    }
    nav
}

fn migrate_features_carousel_nav_to_contact_about(nav: &mut serde_json::Value) {
    let Some(items) = nav.get_mut("items").and_then(|v| v.as_array_mut()) else {
        return;
    };
    let mut has_contact = items.iter().any(|item| {
        item.get("key").and_then(|k| k.as_str()) == Some("contact")
            || item.get("path").and_then(|p| p.as_str()) == Some("/home/contact")
    });
    let mut has_about = items.iter().any(|item| {
        item.get("key").and_then(|k| k.as_str()) == Some("about")
            || item.get("path").and_then(|p| p.as_str()) == Some("/home/about")
    });
    let is_features = |item: &serde_json::Value| {
        item.get("key").and_then(|k| k.as_str()) == Some("features")
            || item.get("path").and_then(|p| p.as_str()) == Some("#features")
    };
    let is_carousel = |item: &serde_json::Value| {
        item.get("key").and_then(|k| k.as_str()) == Some("carousel")
            || item.get("path").and_then(|p| p.as_str()) == Some("#carousel")
    };
    let contact_item =
        default_fixed_nav_item("contact", "联系我们|Contact Us", "/home/contact", 20);
    let about_item = default_fixed_nav_item("about", "关于我们|About Us", "/home/about", 10);
    let mut i = 0;
    while i < items.len() {
        if is_features(&items[i]) {
            if has_contact {
                items.remove(i);
                continue;
            }
            items[i] = contact_item.clone();
            has_contact = true;
        } else if is_carousel(&items[i]) {
            if has_about {
                items.remove(i);
                continue;
            }
            items[i] = about_item.clone();
            has_about = true;
        }
        i += 1;
    }
}

fn portal_footer_config(configs: &std::collections::HashMap<String, String>) -> serde_json::Value {
    let defaults = default_portal_footer_config();
    let mut footer = configs
        .get("footer_config")
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .unwrap_or_else(|| defaults.clone());
    if footer.get("brand_name").is_none() {
        let legacy = footer;
        footer = defaults.clone();
        for key in ["copyright", "description", "icp_number"] {
            if let Some(value) = legacy
                .get(key)
                .filter(|value| value.as_str().map(|text| !text.is_empty()).unwrap_or(true))
            {
                footer[key] = value.clone();
            }
        }
    }
    merge_portal_defaults(&mut footer, &defaults);
    sanitize_footer_links(&mut footer);
    rewrite_whatstoken_brand(&mut footer);
    footer
}

fn footer_link_path(link: &serde_json::Value) -> &str {
    link.get("path")
        .or_else(|| link.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

fn is_dead_footer_path(path: &str) -> bool {
    matches!(
        path,
        "" | "#" | "#features" | "#models" | "#carousel" | "#integration"
    ) || is_portal_models_path(path)
}

/// 旧底部默认是首页上不存在的锚点。整组死链换成与导航相同的四个页面；单条模型锚点改到系统模型广场。
fn sanitize_footer_links(footer: &mut serde_json::Value) {
    let defaults = default_portal_footer_config();
    let Some(links) = footer.get_mut("links").and_then(|v| v.as_array_mut()) else {
        return;
    };
    let all_dead = !links.is_empty()
        && links.iter().all(|link| is_dead_footer_path(footer_link_path(link)));
    if all_dead {
        if let Some(default_links) = defaults.get("links").and_then(|v| v.as_array()) {
            *links = default_links.clone();
        }
        return;
    }
    for link in links.iter_mut() {
        let path = footer_link_path(link).to_string();
        if path == "#models" || is_portal_models_path(&path) {
            if let Some(obj) = link.as_object_mut() {
                obj.insert("path".to_string(), json!(SYSTEM_MARKETPLACE_PATH));
                obj.remove("url");
            }
        }
    }
    links.retain(|link| !is_dead_footer_path(footer_link_path(link)));
    if links.is_empty() {
        if let Some(default_links) = defaults.get("links").and_then(|v| v.as_array()) {
            *links = default_links.clone();
        }
    }
}

const HERO_SLOGANS: [&str; 3] = [
    "一个接口，调用全球数百个 AI 模型",
    "聚合顶尖视频与视觉大模型，让艺术灵感即刻成片",
    "汇聚全球顶尖大语言模型，为智能应用注入澎湃算力",
];

fn portal_home_config(configs: &std::collections::HashMap<String, String>) -> serde_json::Value {
    let mut home = stored_portal_home_config(configs);

    let is_default_title = home
        .get("hero_title")
        .and_then(|v| v.as_str())
        .map(|s| s.is_empty() || HERO_SLOGANS.contains(&s))
        .unwrap_or(true);
    if is_default_title {
        let idx = rand::random::<usize>() % HERO_SLOGANS.len();
        home["hero_title"] = serde_json::json!(HERO_SLOGANS[idx]);
    }

    rewrite_cta_description_base_url(&mut home);
    home
}

/// 后台填写了 API Base URL 时，把 CTA 文案里的 http(s) 链接同步成该配置。
fn rewrite_cta_description_base_url(home: &mut serde_json::Value) {
    let Some(api) = home
        .get("api_base_url")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return;
    };
    let Some(desc) = home.get("cta_description").and_then(|v| v.as_str()) else {
        return;
    };
    let rewritten = replace_http_urls(desc, &api);
    if rewritten != desc {
        home["cta_description"] = json!(rewritten);
    }
}

fn replace_http_urls(text: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let http = rest.find("http://");
        let https = rest.find("https://");
        let start = match (http, https) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => {
                out.push_str(rest);
                break;
            }
        };
        out.push_str(&rest[..start]);
        let url = &rest[start..];
        let end = url
            .char_indices()
            .find(|(_, c)| {
                c.is_whitespace()
                    || matches!(
                        *c,
                        ',' | '，' | ';' | '；' | '!' | '！' | '(' | ')' | '（' | '）'
                    )
            })
            .map(|(i, _)| i)
            .unwrap_or(url.len());
        out.push_str(replacement);
        rest = &url[end..];
    }
    out
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn bilingual_text(value: &str) -> (&str, &str) {
    value
        .split_once('|')
        .map(|(zh, en)| (zh.trim(), en.trim()))
        .unwrap_or((value.trim(), value.trim()))
}

fn bilingual_span(value: &str) -> String {
    let (zh, en) = bilingual_text(value);
    format!(
        "<span data-portal-zh=\"{}\" data-portal-en=\"{}\">{}</span>",
        html_escape(zh),
        html_escape(en),
        html_escape(en)
    )
}

fn replace_managed_block(html: &mut String, name: &str, replacement: &str) {
    let start_marker = format!("<!-- PORTAL_{}_START -->", name);
    let end_marker = format!("<!-- PORTAL_{}_END -->", name);
    let Some(start) = html.find(&start_marker) else {
        return;
    };
    let content_start = start + start_marker.len();
    let Some(relative_end) = html[content_start..].find(&end_marker) else {
        return;
    };
    let end = content_start + relative_end;
    html.replace_range(content_start..end, &format!("\n{}\n  ", replacement));
}

fn replace_meta_content(html: &mut String, name: &str, value: &str) {
    let prefix = format!(r#"<meta name="{}" content="#, name);
    let Some(start) = html.find(&prefix) else {
        return;
    };
    let content_start = start + prefix.len();
    let Some(relative_end) = html[content_start..].find('"') else {
        return;
    };
    html.replace_range(
        content_start..content_start + relative_end,
        &html_escape(value),
    );
}

fn managed_homepage_html(configs: &std::collections::HashMap<String, String>) -> String {
    let nav = portal_nav_config(configs);
    let footer = portal_footer_config(configs);
    let scripts = configs
        .get("custom_scripts")
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .unwrap_or(json!({"customer_service": "", "analytics": ""}));
    let seo = configs
        .get("seo_config")
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .unwrap_or(json!({}));

    let mut html = include_str!(
        "../../../../../frontend/src/pages/Plugins/SitePortal/whats-token-homepage.html"
    )
    .to_string();
    html = html.replacen(
        "<html lang=\"en\">",
        "<html lang=\"en\" data-managed-seo=\"true\">",
        1,
    );

    let logo_text = nav["logo_text"].as_str().unwrap_or("Tkeapi");
    let logo_link = nav["logo_link"].as_str().unwrap_or("/home");
    let logo_url = nav["logo_url"].as_str().unwrap_or("");
    let logo_image = if logo_url.is_empty() {
        String::new()
    } else {
        format!(
            "<img src=\"{}\" alt=\"{}\" class=\"h-8 w-8 object-contain transition-transform duration-300 group-hover:scale-105\">",
            html_escape(logo_url),
            html_escape(logo_text)
        )
    };
    let logo = format!(
        "<a href=\"{}\" class=\"flex items-center gap-3 group\" aria-label=\"{} home\">{}<span class=\"font-heading text-xl sm:text-2xl font-bold tracking-tight bg-gradient-to-r from-foreground to-foreground/80 bg-clip-text text-transparent group-hover:opacity-90 duration-300\">{}</span></a>",
        html_escape(logo_link),
        html_escape(logo_text),
        logo_image,
        html_escape(logo_text)
    );
    replace_managed_block(&mut html, "LOGO", &logo);

    let nav_items = nav["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["enabled"].as_bool().unwrap_or(true))
        .map(|item| {
            let label = item["label"].as_str().unwrap_or("");
            let path = item["path"].as_str().unwrap_or("#");
            format!(
                "<a href=\"{}\" class=\"text-sm font-medium text-muted-foreground hover:text-foreground transition-colors\">{}</a>",
                html_escape(path),
                bilingual_span(label)
            )
        })
        .collect::<Vec<_>>()
        .join("\n        ");
    let nav_html = format!(
        "<nav class=\"hidden md:flex items-center gap-6\">{}</nav>",
        if nav_items.is_empty() {
            String::new()
        } else {
            format!("\n        {}\n      ", nav_items)
        }
    );
    replace_managed_block(&mut html, "NAV", &nav_html);

    let cta_text = nav["cta_text"].as_str().unwrap_or("登录|Login");
    let register_text = nav["register_text"]
        .as_str()
        .unwrap_or("立即注册|Get API Key");
    let actions = format!(
        "<a href=\"{}\" class=\"hidden sm:inline-flex text-sm font-medium text-muted-foreground hover:text-foreground h-9 px-4 items-center justify-center rounded-lg transition-colors\">{}</a>\n        <a href=\"{}\" class=\"inline-flex h-9 items-center justify-center rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground shadow hover:bg-primary/95 transition-all duration-200 hover:scale-[1.02]\">{}</a>",
        html_escape(nav["cta_link"].as_str().unwrap_or("/login")),
        bilingual_span(cta_text),
        html_escape(nav["register_link"].as_str().unwrap_or("/register")),
        bilingual_span(register_text)
    );
    replace_managed_block(&mut html, "ACTIONS", &actions);

    let footer_links = footer["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|link| link.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true))
        .map(|link| {
            format!(
                "<li><a href=\"{}\" class=\"text-muted-foreground hover:text-foreground transition-colors\">{}</a></li>",
                html_escape(link["path"].as_str().unwrap_or("#")),
                bilingual_span(link["label"].as_str().unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join("\n            ");
    let company_name = footer["company_name"].as_str().unwrap_or("");
    let company_address = footer["company_address"].as_str().unwrap_or("");
    let icp_number = footer["icp_number"].as_str().unwrap_or("");
    let company_rows = [
        (!company_name.is_empty())
            .then(|| format!("<p>Company Name: {}</p>", html_escape(company_name))),
        (!company_address.is_empty())
            .then(|| format!("<p>Company Address: {}</p>", html_escape(company_address))),
        (!icp_number.is_empty()).then(|| format!("<p>{}</p>", html_escape(icp_number))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n              ");

    let terms_enabled = footer
        .get("terms_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let privacy_enabled = footer
        .get("privacy_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let terms_html = if terms_enabled {
        format!(
            r#"<a href="{}" class="hover:text-foreground transition-colors">{}</a>"#,
            html_escape(footer["terms_link"].as_str().unwrap_or("/legal/terms")),
            bilingual_span(
                footer["terms_text"]
                    .as_str()
                    .unwrap_or("服务条款|Terms of Service")
            )
        )
    } else {
        String::new()
    };
    let privacy_html = if privacy_enabled {
        format!(
            r#"<a href="{}" class="hover:text-foreground transition-colors">{}</a>"#,
            html_escape(footer["privacy_link"].as_str().unwrap_or("/legal/privacy")),
            bilingual_span(
                footer["privacy_text"]
                    .as_str()
                    .unwrap_or("隐私政策|Privacy Policy")
            )
        )
    } else {
        String::new()
    };

    let footer_html = format!(
        r#"<footer class="mt-auto border-t border-border bg-muted/20 py-12 sm:py-16 transition-colors duration-300">
    <div class="container mx-auto max-w-[1400px] px-4 sm:px-6">
      <div class="grid grid-cols-1 md:grid-cols-12 gap-8 md:gap-12 pb-12 border-b border-border">
        <div class="md:col-span-5 flex flex-col space-y-4">
          <div class="flex items-center gap-2"><span class="font-heading text-lg font-bold tracking-tight">{brand}</span></div>
          <p class="text-sm text-muted-foreground leading-relaxed max-w-[420px]">{description}</p>
        </div>
        <div class="md:col-span-3 space-y-3.5">
          <h4 class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{links_title}</h4>
          <ul class="space-y-2 text-sm">{links}</ul>
        </div>
        <div class="md:col-span-4 space-y-4">
          <h4 class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{news_title}</h4>
          <p class="text-xs text-muted-foreground">{news_description}</p>
        </div>
      </div>
      <div class="pt-8 flex flex-col sm:flex-row items-start justify-between text-xs text-muted-foreground gap-4">
        <div class="space-y-2 leading-relaxed text-left">
          <span class="block">{copyright}</span>
          <div>{company_rows}</div>
        </div>
        <div class="flex items-center gap-4 sm:justify-end">
          {terms_html}
          {privacy_html}
        </div>
      </div>
    </div>
  </footer>"#,
        brand = bilingual_span(footer["brand_name"].as_str().unwrap_or("Tkeapi")),
        description = bilingual_span(footer["description"].as_str().unwrap_or("")),
        links_title = bilingual_span(footer["links_title"].as_str().unwrap_or("")),
        links = if footer_links.is_empty() {
            String::new()
        } else {
            format!("\n            {}\n          ", footer_links)
        },
        news_title = bilingual_span(footer["news_title"].as_str().unwrap_or("")),
        news_description = bilingual_span(footer["news_description"].as_str().unwrap_or("")),
        copyright = html_escape(footer["copyright"].as_str().unwrap_or("")),
        company_rows = company_rows,
        terms_html = terms_html,
        privacy_html = privacy_html,
    );
    replace_managed_block(&mut html, "FOOTER", &footer_html);

    if let Some(title) = seo["meta_title"].as_str().filter(|value| !value.is_empty()) {
        if let (Some(start), Some(end)) = (html.find("<title>"), html.find("</title>")) {
            html.replace_range(start + 7..end, &html_escape(title));
        }
    }
    if let Some(description) = seo["meta_description"]
        .as_str()
        .filter(|value| !value.is_empty())
    {
        replace_meta_content(&mut html, "description", description);
    }
    if let Some(keywords) = seo["meta_keywords"]
        .as_str()
        .filter(|value| !value.is_empty())
    {
        replace_meta_content(&mut html, "keywords", keywords);
    }
    if let Some(analytics) = scripts["analytics"]
        .as_str()
        .filter(|value| !value.is_empty())
    {
        html = html.replacen("</head>", &format!("{}\n</head>", analytics), 1);
    }
    if let Some(customer_service) = scripts["customer_service"]
        .as_str()
        .filter(|value| !value.is_empty())
    {
        html = html.replacen("</body>", &format!("{}\n</body>", customer_service), 1);
    }
    let api_base_url = portal_home_config(configs)
        .get("api_base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let api_base_js = serde_json::to_string(&api_base_url).unwrap_or_else(|_| "\"\"".to_string());
    html = html.replace("'__PORTAL_API_BASE_URL__'", &api_base_js);
    html
}

/// 自定义主页优先：开启且 HTML 非空时返回内容。
fn active_custom_homepage(configs: &std::collections::HashMap<String, String>) -> Option<String> {
    let custom_hp: serde_json::Value = configs
        .get("custom_homepage")
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or(json!({ "enabled": false, "html": "" }));
    let enabled = custom_hp["enabled"].as_bool().unwrap_or(false);
    let html = custom_hp["html"].as_str().unwrap_or("").trim();
    if enabled && !html.is_empty() {
        Some(html.to_string())
    } else {
        None
    }
}

/// 风格选择是否接管 /home（自定义主页开启时无效）。
/// 未配置时默认 true：开启插件后默认使用经典科技风格模板，而非托管/自定义主页。
fn style_applies_to_homepage(configs: &std::collections::HashMap<String, String>) -> bool {
    if let Some(cfg_str) = configs.get("style_config") {
        if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(cfg_str) {
            if let Some(apply) = cfg["apply_to_homepage"].as_bool() {
                return apply;
            }
        }
    }
    true
}

/// 统一首页渲染：自定义 HTML > 风格化 Tera 首页 > 托管 Tkeapi 首页。
async fn render_homepage_html(
    state: &AppState,
    configs: &std::collections::HashMap<String, String>,
) -> Result<String, AppError> {
    if let Some(html) = active_custom_homepage(configs) {
        return Ok(html);
    }
    if !style_applies_to_homepage(configs) {
        return Ok(managed_homepage_html(configs));
    }
    let mut portal_data = build_portal_data(state, configs).await?;
    let mut tera = tera::Tera::default();
    register_templates(&mut tera)?;
    render_homepage_with_context(configs, &tera, &mut portal_data)
}

fn render_homepage_with_context(
    configs: &std::collections::HashMap<String, String>,
    tera: &tera::Tera,
    portal_data: &mut tera::Context,
) -> Result<String, AppError> {
    if let Some(html) = active_custom_homepage(configs) {
        return Ok(html);
    }
    if style_applies_to_homepage(configs) {
        portal_data.insert("current_page", &"home");
        return render_page(tera, "home", portal_data);
    }
    Ok(managed_homepage_html(configs))
}

async fn upsert(state: &AppState, key: &str, value: &str) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        &state.db.format_query("UPDATE plugin_configs SET config_value = ?, updated_at = CURRENT_TIMESTAMP WHERE plugin_name = 'site_portal' AND config_key = ?")
    ).bind(value).bind(key).execute(&state.db.pool).await?;
    if result.rows_affected() == 0 {
        sqlx::query(
            &state.db.format_query("INSERT INTO plugin_configs (plugin_name, config_key, config_value) VALUES ('site_portal', ?, ?)")
        ).bind(key).bind(value).execute(&state.db.pool).await?;
    }
    crate::api::plugins::invalidate_plugin_configs_cache("site_portal");
    Ok(())
}

// ─── 获取门户配置 ───

async fn get_portal_config(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> AppResult<Json<serde_json::Value>> {
    require_admin(&state, &claims).await?;
    let configs = load_configs(&state).await?;

    // 仅返回管理端实际编辑的配置
    Ok(Json(json!({
        "nav_config": portal_nav_config(&configs),
        "footer_config": portal_footer_config(&configs),
        "custom_scripts": configs.get("custom_scripts").and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok()).unwrap_or(json!({
            "customer_service": "",
            "analytics": ""
        })),
        "seo_config": configs.get("seo_config").and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok()).unwrap_or(json!({
            "meta_title": "",
            "meta_description": "",
            "meta_keywords": ""
        })),
        "style_config": configs.get("style_config").and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok()).unwrap_or(json!({
            "current_style": "classic",
            "apply_to_homepage": true
        })),
        "static_gen_config": configs.get("static_gen_config").and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok()).unwrap_or(json!({
            "manual_mode": false
        })),
        "generate_log": configs.get("generate_log").and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok()).unwrap_or(json!([])),
        "home_config": stored_portal_home_config(&configs),
        "columns": portal_columns_config(&configs),
        "custom_homepage": configs.get("custom_homepage").and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok()).unwrap_or(json!({
            "enabled": false,
            "html": ""
        })),
    })))
}

// ─── 保存门户配置 ───

#[derive(Deserialize)]
struct SavePortalRequest {
    section: String,
    data: serde_json::Value,
}

async fn save_portal_config(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Json(payload): Json<SavePortalRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_admin(&state, &claims).await?;

    let (key, value) = match payload.section.as_str() {
        "nav" => (
            "nav_config",
            serde_json::to_string(&payload.data).unwrap_or_default(),
        ),
        "footer" => (
            "footer_config",
            serde_json::to_string(&payload.data).unwrap_or_default(),
        ),
        "scripts" => (
            "custom_scripts",
            serde_json::to_string(&payload.data).unwrap_or_default(),
        ),
        "seo" => (
            "seo_config",
            serde_json::to_string(&payload.data).unwrap_or_default(),
        ),
        "style" => {
            let existing_configs = load_configs(&state).await?;
            let mut merged = existing_configs
                .get("style_config")
                .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
                .unwrap_or(json!({ "current_style": "classic", "apply_to_homepage": true }));
            if let (Some(base), Some(patch)) = (merged.as_object_mut(), payload.data.as_object()) {
                for (k, v) in patch {
                    base.insert(k.clone(), v.clone());
                }
            }
            (
                "style_config",
                serde_json::to_string(&merged).unwrap_or_default(),
            )
        }
        "static_gen" => (
            "static_gen_config",
            serde_json::to_string(&payload.data).unwrap_or_default(),
        ),
        "home" => {
            let existing_configs = load_configs(&state).await?;
            let mut home = existing_configs
                .get("home_config")
                .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
                .unwrap_or_else(|| json!({}));
            merge_home_config_patch(&mut home, &payload.data);
            (
                "home_config",
                serde_json::to_string(&home).unwrap_or_default(),
            )
        }
        "custom_homepage" => (
            "custom_homepage",
            serde_json::to_string(&payload.data).unwrap_or_default(),
        ),
        "columns" => (
            "columns_config",
            serde_json::to_string(&payload.data).unwrap_or_default(),
        ),
        "about" => {
            let existing_configs = load_configs(&state).await?;
            let mut columns: serde_json::Value = existing_configs
                .get("columns_config")
                .and_then(|v| serde_json::from_str(v).ok())
                .unwrap_or_else(default_portal_columns_config);
            columns["about"] = payload.data.clone();
            (
                "columns_config",
                serde_json::to_string(&columns).unwrap_or_default(),
            )
        }
        "contact" => {
            let existing_configs = load_configs(&state).await?;
            let mut columns: serde_json::Value = existing_configs
                .get("columns_config")
                .and_then(|v| serde_json::from_str(v).ok())
                .unwrap_or_else(default_portal_columns_config);
            columns["contact"] = payload.data.clone();
            (
                "columns_config",
                serde_json::to_string(&columns).unwrap_or_default(),
            )
        }
        _ => {
            return Err(AppError::BadRequest(format!(
                "未知配置区域: {}",
                payload.section
            )))
        }
    };
    upsert(&state, key, &value).await?;

    // 自动在后台进行静态生成
    let configs = load_configs(&state).await?;
    let static_gen_cfg: serde_json::Value = configs
        .get("static_gen_config")
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or(json!({ "manual_mode": false }));
    let manual_mode = static_gen_cfg["manual_mode"].as_bool().unwrap_or(false);

    if !manual_mode {
        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) = run_all_static_generation(&state_clone).await {
                eprintln!("自动静态生成失败: {:?}", e);
            }
        });
    }

    Ok(Json(json!({ "message": "配置已保存" })))
}

// ─── 静态 HTML 生成 ───

#[derive(Deserialize)]
struct GenerateRequest {
    scope: String,
    columns: Option<Vec<String>>,
}

/// FAI 艺术未配置站点图标时，用页头那枚菱形标作为默认 favicon。
fn portal_favicon_href(style: &serde_json::Value, site_favicon: &str) -> String {
    if style.get("current_style").and_then(|v| v.as_str()) != Some("fai_art") {
        return String::new();
    }
    let custom = site_favicon.trim();
    if !custom.is_empty() {
        return custom.to_string();
    }
    let svg = include_str!("../../../templates/portal/styles/fai_art/assets/favicon.svg");
    format!("data:image/svg+xml,{}", urlencoding::encode(svg))
}

fn column_paths(configs: &std::collections::HashMap<String, String>) -> (String, String) {
    let columns_cfg: serde_json::Value = configs
        .get("columns_config")
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or(json!({}));
    (
        columns_cfg["contact"]["path"]
            .as_str()
            .unwrap_or("contact")
            .to_string(),
        columns_cfg["about"]["path"]
            .as_str()
            .unwrap_or("about")
            .to_string(),
    )
}

const FAI_CARD_CAT: &[u8] =
    include_bytes!("../../../templates/portal/styles/fai_art/assets/card_cat.jpg");
const FAI_CARD_PORTRAIT: &[u8] =
    include_bytes!("../../../templates/portal/styles/fai_art/assets/card_portrait.jpg");
const FAI_CARD_DANCERS: &[u8] =
    include_bytes!("../../../templates/portal/styles/fai_art/assets/card_dancers.jpg");
const FAI_CARD_KING: &[u8] =
    include_bytes!("../../../templates/portal/styles/fai_art/assets/card_king.jpg");

pub async fn ensure_portal_assets(portal_dir: &str, assets_dir: &str) {
    let images = [
        ("card_cat.jpg", FAI_CARD_CAT),
        ("card_portrait.jpg", FAI_CARD_PORTRAIT),
        ("card_dancers.jpg", FAI_CARD_DANCERS),
        ("card_king.jpg", FAI_CARD_KING),
    ];
    for (name, bytes) in images {
        let p_portal = format!("{}/assets/images/portal/fai/{}", portal_dir, name);
        if let Some(parent) = std::path::Path::new(&p_portal).parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        tokio::fs::write(&p_portal, bytes).await.ok();

        let p_portal_arts = format!("{}/assets/images/portal/arts/{}", portal_dir, name);
        if let Some(parent) = std::path::Path::new(&p_portal_arts).parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        tokio::fs::write(&p_portal_arts, bytes).await.ok();

        let p_assets = format!("{}/images/portal/fai/{}", assets_dir, name);
        if let Some(parent) = std::path::Path::new(&p_assets).parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        tokio::fs::write(&p_assets, bytes).await.ok();

        let p_assets_arts = format!("{}/images/portal/arts/{}", assets_dir, name);
        if let Some(parent) = std::path::Path::new(&p_assets_arts).parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        tokio::fs::write(&p_assets_arts, bytes).await.ok();
    }
}

async fn write_static_html(path: String, html: &str) -> Result<(), AppError> {
    if let Some(parent) = std::path::Path::new(&path).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::BadRequest(format!("创建目录失败: {e}")))?;
    }
    tokio::fs::write(&path, html)
        .await
        .map_err(|e| AppError::BadRequest(format!("写入静态页失败: {e}")))
}

async fn generate_static(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Json(payload): Json<GenerateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_admin(&state, &claims).await?;

    let configs = load_configs(&state).await?;
    let mut portal_data = build_portal_data(&state, &configs).await?;
    let portal_dir = state.config.portal_dir.clone();
    let (contact_path, about_path) = column_paths(&configs);
    ensure_portal_assets(&portal_dir, &state.config.assets_dir).await;

    let mut tera = tera::Tera::default();
    register_templates(&mut tera)?;

    let mut generated = Vec::new();
    let mut generated_paths: Vec<serde_json::Value> = Vec::new();

    let should_gen = |page: &str| -> bool {
        payload.scope == "all"
            || payload.scope == page
            || (payload.scope == "columns"
                && payload
                    .columns
                    .as_ref()
                    .map(|c| c.iter().any(|item| item == page))
                    .unwrap_or(false))
    };

    if should_gen("home") {
        let html = render_homepage_with_context(&configs, &tera, &mut portal_data)?;
        write_static_html(format!("{}/index.html", portal_dir), &html).await?;
        generated.push("首页");
        generated_paths.push(json!({ "label": "首页", "path": "/portal/" }));
    }

    if should_gen("contact") {
        portal_data.insert("current_page", &"contact");
        let html = render_page(&tera, "contact", &portal_data)?;
        write_static_html(format!("{}/{}/index.html", portal_dir, contact_path), &html).await?;
        generated.push("联系我们");
        generated_paths
            .push(json!({ "label": "联系我们", "path": format!("/portal/{}/", contact_path) }));
    }

    if should_gen("about") {
        portal_data.insert("current_page", &"about");
        let html = render_page(&tera, "about", &portal_data)?;
        write_static_html(format!("{}/{}/index.html", portal_dir, about_path), &html).await?;
        generated.push("关于我们");
        generated_paths
            .push(json!({ "label": "关于我们", "path": format!("/portal/{}/", about_path) }));
    }

    // 记录日志
    let log_entry = json!({
        "time": chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "scope": payload.scope,
        "pages": generated,
    });
    let mut logs: Vec<serde_json::Value> = configs
        .get("generate_log")
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or_default();
    logs.insert(0, log_entry);
    if logs.len() > 20 {
        logs.truncate(20);
    }
    upsert(
        &state,
        "generate_log",
        &serde_json::to_string(&logs).unwrap_or_default(),
    )
    .await
    .ok();

    Ok(Json(json!({
        "message": format!("已生成 {} 个页面", generated.len()),
        "generated": generated,
        "generated_paths": generated_paths
    })))
}

// ═══════════════════════════════════════════
//  模板渲染核心
// ═══════════════════════════════════════════

async fn build_portal_data(
    state: &AppState,
    configs: &std::collections::HashMap<String, String>,
) -> Result<tera::Context, AppError> {
    let mut ctx = tera::Context::new();

    let site_settings_val: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT value FROM settings WHERE key = 'site_settings'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or_default();
    let site_settings = site_settings_val
        .and_then(|v| serde_json::from_str::<crate::models::settings::SiteSettings>(&v).ok())
        .unwrap_or_else(|| crate::api::settings::default_site_settings());

    let nav_config_val = portal_nav_config(configs);

    let mut nav = nav_config_val;
    let marketplace_path = marketplace_nav_path(&nav);
    if let Some(nav_map) = nav.as_object_mut() {
        let logo_empty = nav_map
            .get("logo_url")
            .and_then(|v| v.as_str())
            .map(|s| s.is_empty())
            .unwrap_or(true);
        if logo_empty && !site_settings.logo.is_empty() {
            nav_map.insert("logo_url".to_string(), json!(site_settings.logo));
        }
        nav_map.insert("marketplace_path".to_string(), json!(marketplace_path));
    }
    let home = portal_home_config(configs);
    let columns = portal_columns_config(configs);
    let footer = portal_footer_config(configs);
    let scripts: serde_json::Value = configs
        .get("custom_scripts")
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or(json!({"customer_service":"","analytics":""}));
    let seo: serde_json::Value = configs
        .get("seo_config")
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or(json!({"meta_title":"","meta_description":"","meta_keywords":""}));
    let style: serde_json::Value = configs
        .get("style_config")
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or(json!({"current_style": "classic"}));
    let favicon_href = portal_favicon_href(&style, &site_settings.favicon);

    #[derive(sqlx::FromRow, serde::Serialize, Clone)]
    struct SimpleModel {
        id: i64,
        model_name: String,
        mid: String,
        model_id: String,
        #[sqlx(default)]
        original_id: String,
        #[sqlx(rename = "type_name")]
        model_type: Option<String>,
        #[sqlx(rename = "provider_name")]
        provider: Option<String>,
        logo: Option<String>,
        type_logo: Option<String>,
        provider_logo: Option<String>,
        description: Option<String>,
        billing: Option<sqlx::types::Json<serde_json::Value>>,
        #[sqlx(default)]
        sort_order: i64,
        #[sqlx(default)]
        global_discount: f64,
        #[sqlx(default)]
        global_discount_enabled: i64,
    }
    let is_mp_enabled: bool = sqlx::query_scalar::<_, i64>(
        &state
            .db
            .format_query("SELECT is_enabled FROM plugins WHERE name = 'model_marketplace'"),
    )
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None)
        == Some(1);

    let mp_configs = if is_mp_enabled {
        crate::api::plugins::load_plugin_configs_pub(state, "model_marketplace")
            .await
            .unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };
    let display_mode = mp_configs
        .get("mp_display_mode")
        .map(|s| s.as_str())
        .unwrap_or("blacklist");
    let is_blacklist = display_mode == "blacklist";

    // 优化：如果为白名单模式且没有任何配置开启的模型，直接跳过查询 models
    let has_enabled_models = is_blacklist
        || mp_configs.iter().any(|(k, v)| {
            k.starts_with("mp_model_id_")
                && serde_json::from_str::<serde_json::Value>(v)
                    .map(|json| json.get("enabled").and_then(|e| e.as_bool()) == Some(true))
                    .unwrap_or(false)
        });
    let grouped_models: Vec<serde_json::Value> = if !has_enabled_models {
        Vec::new()
    } else {
        let models: Vec<SimpleModel> = sqlx::query_as(
        &state.db.format_query(
            "SELECT m.id, m.name AS model_name, m.mid, m.model_id, m.original_id, m.sort_order, t.name AS type_name, p.name AS provider_name, \
             m.global_discount, m.global_discount_enabled, \
             CASE WHEN i.file_path IS NOT NULL THEN '/assets/' || i.file_path \
                  WHEN m.logo IS NOT NULL AND m.logo != '' THEN m.logo \
                  ELSE NULL END AS logo, \
             CASE WHEN ti.file_path IS NOT NULL THEN '/assets/' || ti.file_path \
                  WHEN t.logo IS NOT NULL AND t.logo != '' THEN t.logo \
                  ELSE NULL END AS type_logo, \
             CASE WHEN pi.file_path IS NOT NULL THEN '/assets/' || pi.file_path \
                  WHEN p.logo IS NOT NULL AND p.logo != '' THEN p.logo \
                  ELSE NULL END AS provider_logo, \
             m.description, \
             json_build_object('billing_type', br.billing_type, 'billing_rule', br.billing_rule, 'prompt_rate', br.prompt_rate, 'completion_rate', br.completion_rate, 'fixed_rate', br.fixed_rate, 'duration_rate', br.duration_rate, 'extended_config', br.extended_config, 'pricing_tiers', br.pricing_tiers, 'cached_rate', br.cached_rate, 'claude_cache_creation_rate', br.claude_cache_creation_rate, 'claude_cache_read_rate', br.claude_cache_read_rate, 'global_discount', m.global_discount, 'global_discount_enabled', m.global_discount_enabled) AS billing \
             FROM models m \
             LEFT JOIN model_types t ON m.type_id = t.id \
             LEFT JOIN model_providers p ON m.provider_id = p.id \
             LEFT JOIN site_icons i ON i.name = m.logo \
             LEFT JOIN site_icons ti ON ti.name = t.logo \
             LEFT JOIN site_icons pi ON pi.name = p.logo \
             LEFT JOIN billing_rules br ON m.billing_rule_id = br.id \
             WHERE m.is_active = 1 AND m.is_listed = 1 ORDER BY m.sort_order DESC, m.id DESC"
        )
    ).fetch_all(&state.db.pool).await.unwrap_or_default();

        let mut filtered_models = Vec::new();
        for mut m in models {
            let config_key = format!("mp_model_id_{}", m.id);
            let model_conf: serde_json::Value = mp_configs
                .get(&config_key)
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(json!({"sort_order": 0, "description": ""}));

            let default_enabled = is_blacklist;
            let is_enabled = model_conf
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(default_enabled);
            if !is_enabled {
                continue;
            }

            let customized_desc = model_conf
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if !customized_desc.is_empty() {
                m.description = Some(customized_desc.to_string());
            }
            filtered_models.push(m);
        }

        filtered_models.sort_by(|a, b| {
            b.sort_order
                .cmp(&a.sort_order)
                .then_with(|| b.id.cmp(&a.id))
        });

        let mut grouped_map: std::collections::HashMap<String, Vec<serde_json::Value>> =
            std::collections::HashMap::new();
        let mut grouped_order: Vec<String> = Vec::new();
        for m in &filtered_models {
            let val = serde_json::to_value(m).unwrap_or(json!({}));
            let original_id = m.original_id.clone();
            let model_id = m.model_id.clone();
            let base_key = if !original_id.is_empty() {
                original_id
            } else {
                model_id.clone()
            };
            let type_name = val.get("type_name").and_then(|v| v.as_str()).unwrap_or("");
            let group_key = format!("{}::{}", base_key, type_name);

            if !grouped_map.contains_key(&group_key) {
                grouped_order.push(group_key.clone());
            }
            grouped_map.entry(group_key).or_default().push(val);
        }

        grouped_order
            .into_iter()
            .filter_map(|group_key| {
                let mut variants = grouped_map.remove(&group_key)?;
                variants.sort_by(|a, b| {
                    let sa = a.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0);
                    let sb = b.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0);
                    let ida = a.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
                    let idb = b.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
                    sb.cmp(&sa).then_with(|| idb.cmp(&ida))
                });
                let primary = &variants[0];
                let mut group = primary.clone();
                let max_sort_order = variants
                    .iter()
                    .map(|v| v.get("sort_order").and_then(|x| x.as_i64()).unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                group["sort_order"] = json!(max_sort_order);
                group["variant_count"] = json!(variants.len());
                group["variants"] = json!(variants);
                let original_id = primary
                    .get("original_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let model_id = primary
                    .get("model_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let base_key = if !original_id.is_empty() {
                    original_id
                } else {
                    model_id
                };
                group["model_id"] = json!(base_key);
                Some(group)
            })
            .collect()
    };

    let currency = crate::api::settings::get_currency_settings(state).await;

    let default_lang = site_settings.default_language.as_str();

    ctx.insert("nav", &nav);
    ctx.insert("home", &home);
    ctx.insert("columns", &columns);
    ctx.insert("footer", &footer);
    ctx.insert("scripts", &scripts);
    ctx.insert("seo", &seo);
    ctx.insert("style", &style);
    ctx.insert("favicon_href", &favicon_href);
    ctx.insert("models", &grouped_models);
    ctx.insert("currency", &currency);
    ctx.insert("year", &chrono::Utc::now().format("%Y").to_string());
    ctx.insert("default_language", default_lang);

    ctx.insert("portal_locales_zh", PORTAL_LOCALE_ZH);
    ctx.insert("portal_locales_zh_tw", PORTAL_LOCALE_ZH_TW);
    ctx.insert("portal_locales_en", PORTAL_LOCALE_EN);
    ctx.insert("portal_locales_ja", PORTAL_LOCALE_JA);
    ctx.insert("portal_locales_ko", PORTAL_LOCALE_KO);
    ctx.insert("portal_locales_vi", PORTAL_LOCALE_VI);

    Ok(ctx)
}

const PORTAL_LOCALE_ZH: &str =
    include_str!("../../../../../frontend/src/pages/Plugins/SitePortal/locales/portal/zh.json");
const PORTAL_LOCALE_ZH_TW: &str =
    include_str!("../../../../../frontend/src/pages/Plugins/SitePortal/locales/portal/zh-TW.json");
const PORTAL_LOCALE_EN: &str =
    include_str!("../../../../../frontend/src/pages/Plugins/SitePortal/locales/portal/en.json");
const PORTAL_LOCALE_JA: &str =
    include_str!("../../../../../frontend/src/pages/Plugins/SitePortal/locales/portal/ja.json");
const PORTAL_LOCALE_KO: &str =
    include_str!("../../../../../frontend/src/pages/Plugins/SitePortal/locales/portal/ko.json");
const PORTAL_LOCALE_VI: &str =
    include_str!("../../../../../frontend/src/pages/Plugins/SitePortal/locales/portal/vi.json");

fn render_page(tera: &tera::Tera, page: &str, ctx: &tera::Context) -> Result<String, AppError> {
    let tpl = match page {
        "home" => "home.html",
        "contact" => "contact.html",
        "about" => "about.html",
        _ => return Err(AppError::BadRequest(format!("未知页面: {}", page))),
    };
    tera.render(tpl, ctx)
        .map_err(|e| AppError::Internal(format!("模板渲染失败: {}", e)))
}

/// Tera 子模板必须以 `{% extends %}` 为首个有效内容；剥离文件头版权注释，避免注册 panic。
fn tera_child_source(raw: &'static str) -> &'static str {
    raw.find("{% extends").map(|idx| &raw[idx..]).unwrap_or(raw)
}

fn register_templates(tera: &mut tera::Tera) -> Result<(), AppError> {
    tera.add_raw_templates([
        (
            "styles/fai_art/_chrome_styles.html",
            include_str!("../../../templates/portal/styles/fai_art/_chrome_styles.html"),
        ),
        (
            "styles/fai_art/_home.html",
            include_str!("../../../templates/portal/styles/fai_art/_home.html"),
        ),
        (
            "styles/fai_art/_contact.html",
            include_str!("../../../templates/portal/styles/fai_art/_contact.html"),
        ),
        (
            "base.html",
            include_str!("../../../templates/portal/base.html"),
        ),
        (
            "home.html",
            tera_child_source(include_str!("../../../templates/portal/home.html")),
        ),
        (
            "contact.html",
            tera_child_source(include_str!("../../../templates/portal/contact.html")),
        ),
        (
            "about.html",
            tera_child_source(include_str!("../../../templates/portal/about.html")),
        ),
    ])
    // 管理端生成依赖此错误信息；勿用 Internal（对外会被吞成通用 500）
    .map_err(|e| {
        let detail = std::error::Error::source(&e)
            .map(|source| format!("{e}: {source}"))
            .unwrap_or_else(|| e.to_string());
        AppError::BadRequest(format!("门户模板注册失败: {detail}"))
    })
}

async fn run_all_static_generation(state: &AppState) -> Result<(), AppError> {
    let configs = load_configs(state).await?;
    let mut portal_data = build_portal_data(state, &configs).await?;
    let portal_dir = state.config.portal_dir.clone();
    let (contact_path, about_path) = column_paths(&configs);
    ensure_portal_assets(&portal_dir, &state.config.assets_dir).await;

    let mut tera = tera::Tera::default();
    register_templates(&mut tera)?;

    let home_html = render_homepage_with_context(&configs, &tera, &mut portal_data)?;
    write_static_html(format!("{}/index.html", portal_dir), &home_html).await?;

    portal_data.insert("current_page", &"contact");
    let html = render_page(&tera, "contact", &portal_data)?;
    write_static_html(format!("{}/{}/index.html", portal_dir, contact_path), &html).await?;

    portal_data.insert("current_page", &"about");
    let html = render_page(&tera, "about", &portal_data)?;
    write_static_html(format!("{}/{}/index.html", portal_dir, about_path), &html).await?;

    Ok(())
}
