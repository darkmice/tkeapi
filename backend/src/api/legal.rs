/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::error::AppError;
use crate::models::{
    AgreementSettings, SiteSettings, DEFAULT_PRIVACY_EN, DEFAULT_PRIVACY_ZH, DEFAULT_TOS_EN,
    DEFAULT_TOS_ZH,
};
use crate::AppState;
use axum::{
    extract::{Query, State},
    http::header,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use std::collections::HashMap;
use std::sync::Arc;

/// 站点协议静态页面公开路由
pub fn legal_pages_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/terms", get(serve_terms))
        .route("/terms.html", get(serve_terms))
        .route("/terms_zh.html", get(serve_terms_zh))
        .route("/terms_en.html", get(serve_terms_en))
        .route("/tos", get(serve_terms))
        .route("/tos.html", get(serve_terms))
        .route("/tos_zh.html", get(serve_terms_zh))
        .route("/tos_en.html", get(serve_terms_en))
        .route("/privacy", get(serve_privacy))
        .route("/privacy.html", get(serve_privacy))
        .route("/privacy_zh.html", get(serve_privacy_zh))
        .route("/privacy_en.html", get(serve_privacy_en))
}

async fn serve_terms(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    serve_legal_page(&state, "terms", &params, &headers).await
}

async fn serve_terms_zh(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let mut params = HashMap::new();
    params.insert("lang".to_string(), "zh".to_string());
    serve_legal_page(&state, "terms", &params, &headers).await
}

async fn serve_terms_en(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let mut params = HashMap::new();
    params.insert("lang".to_string(), "en".to_string());
    serve_legal_page(&state, "terms", &params, &headers).await
}

async fn serve_privacy(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    serve_legal_page(&state, "privacy", &params, &headers).await
}

async fn serve_privacy_zh(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let mut params = HashMap::new();
    params.insert("lang".to_string(), "zh".to_string());
    serve_legal_page(&state, "privacy", &params, &headers).await
}

async fn serve_privacy_en(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let mut params = HashMap::new();
    params.insert("lang".to_string(), "en".to_string());
    serve_legal_page(&state, "privacy", &params, &headers).await
}

async fn serve_legal_page(
    state: &Arc<AppState>,
    page_type: &str,
    params: &HashMap<String, String>,
    headers: &axum::http::HeaderMap,
) -> Response {
    let lang_suffix = legal_lang_suffix(
        params.get("lang").map(String::as_str),
        headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok()),
    );

    // 确保静态页面文件已存在
    let legal_dir = format!("{}/legal", state.config.data_dir);
    let file_path = format!("{}/{}_{}.html", legal_dir, page_type, lang_suffix);

    if !std::path::Path::new(&file_path).exists() {
        if let Err(e) = ensure_legal_static_files(state).await {
            tracing::error!("生成站点协议静态文件失败: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to generate legal static page",
            )
                .into_response();
        }
    }

    // 直接读取静态 HTML 文件返回，不消耗数据库查询与动态渲染性能
    // 使用 no-cache, no-store 杜绝管理员在后台修改协议后因浏览器强缓存导致预览仍为旧内容的问题
    match tokio::fs::read_to_string(&file_path).await {
        Ok(html_content) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (
                    header::CACHE_CONTROL,
                    "no-cache, no-store, must-revalidate",
                ),
                (header::PRAGMA, "no-cache"),
                (header::EXPIRES, "0"),
            ],
            Html(html_content),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("读取协议静态文件失败 [{}]: {:?}", file_path, e);
            (
                axum::http::StatusCode::NOT_FOUND,
                "Legal page not found",
            )
                .into_response()
        }
    }
}

/// 确保站点协议静态文件存在（如不存在则读取配置并生成）
pub async fn ensure_legal_static_files(state: &Arc<AppState>) -> Result<(), AppError> {
    let agreement: AgreementSettings = crate::api::settings::get_setting(
        state,
        "agreement_settings",
        crate::api::settings::default_agreement_settings(),
    )
    .await?;

    let site: SiteSettings = crate::api::settings::get_setting(
        state,
        "site_settings",
        crate::api::settings::default_site_settings(),
    )
    .await?;

    generate_and_save_legal_static_files(&state.config.data_dir, &agreement, &site).await
}

/// 立即编译并落盘生成站点协议全套静态 HTML 页面
pub async fn generate_and_save_legal_static_files(
    data_dir: &str,
    agreement: &AgreementSettings,
    site: &SiteSettings,
) -> Result<(), AppError> {
    let legal_dir = format!("{}/legal", data_dir);
    tokio::fs::create_dir_all(&legal_dir)
        .await
        .map_err(|e| AppError::Internal(format!("创建 legal 静态目录失败: {}", e)))?;

    let mut target_dirs = vec![legal_dir.clone()];
    for candidate in &["../frontend/public/legal", "frontend/public/legal"] {
        if let Some(parent) = std::path::Path::new(candidate).parent() {
            if parent.exists() {
                target_dirs.push(candidate.to_string());
            }
        }
    }

    let write_file = |filename: &str, content: &str| {
        let targets = target_dirs.clone();
        let filename = filename.to_string();
        let content = content.to_string();
        async move {
            for (idx, dir) in targets.iter().enumerate() {
                let _ = tokio::fs::create_dir_all(dir).await;
                let file_path = format!("{}/{}", dir, filename);
                let res = tokio::fs::write(&file_path, &content).await;
                if idx == 0 {
                    res.map_err(|e| AppError::Internal(format!("写入 {} 失败: {}", file_path, e)))?;
                }
            }
            Ok::<(), AppError>(())
        }
    };

    let copyright = site.copyright.trim();
    let site_title = site.title.trim();

    // 1. 服务条款 (中文)
    let terms_zh_html = if agreement.tos_mode == "link" && !agreement.tos_link.trim().is_empty() {
        build_redirect_html("服务条款", &agreement.tos_link, site_title)
    } else {
        let content = if agreement.tos_content.trim().is_empty() {
            DEFAULT_TOS_ZH
        } else {
            &agreement.tos_content
        };
        build_static_html(
            "服务条款 (Terms of Service)",
            content,
            copyright,
            "zh",
            "terms",
            site_title,
        )
    };
    write_file("terms_zh.html", &terms_zh_html).await?;
    write_file("terms.html", &terms_zh_html).await?;
    write_file("tos_zh.html", &terms_zh_html).await?;
    write_file("tos.html", &terms_zh_html).await?;

    // 2. 服务条款 (英文)
    let terms_en_html = if agreement.tos_mode_en == "link" {
        let link = if !agreement.tos_link_en.trim().is_empty() {
            &agreement.tos_link_en
        } else {
            &agreement.tos_link
        };
        if !link.trim().is_empty() {
            build_redirect_html("Terms of Service", link, site_title)
        } else {
            let content = if agreement.tos_content_en.trim().is_empty() {
                DEFAULT_TOS_EN
            } else {
                &agreement.tos_content_en
            };
            build_static_html("Terms of Service", content, copyright, "en", "terms", site_title)
        }
    } else {
        let content = if agreement.tos_content_en.trim().is_empty() {
            DEFAULT_TOS_EN
        } else {
            &agreement.tos_content_en
        };
        build_static_html("Terms of Service", content, copyright, "en", "terms", site_title)
    };
    write_file("terms_en.html", &terms_en_html).await?;
    write_file("tos_en.html", &terms_en_html).await?;

    // 3. 隐私协议 (中文)
    let privacy_zh_html =
        if agreement.privacy_mode == "link" && !agreement.privacy_link.trim().is_empty() {
            build_redirect_html("隐私政策", &agreement.privacy_link, site_title)
        } else {
            let content = if agreement.privacy_content.trim().is_empty() {
                DEFAULT_PRIVACY_ZH
            } else {
                &agreement.privacy_content
            };
            build_static_html(
                "隐私政策 (Privacy Policy)",
                content,
                copyright,
                "zh",
                "privacy",
                site_title,
            )
        };
    write_file("privacy_zh.html", &privacy_zh_html).await?;
    write_file("privacy.html", &privacy_zh_html).await?;

    // 4. 隐私协议 (英文)
    let privacy_en_html = if agreement.privacy_mode_en == "link" {
        let link = if !agreement.privacy_link_en.trim().is_empty() {
            &agreement.privacy_link_en
        } else {
            &agreement.privacy_link
        };
        if !link.trim().is_empty() {
            build_redirect_html("Privacy Policy", link, site_title)
        } else {
            let content = if agreement.privacy_content_en.trim().is_empty() {
                DEFAULT_PRIVACY_EN
            } else {
                &agreement.privacy_content_en
            };
            build_static_html("Privacy Policy", content, copyright, "en", "privacy", site_title)
        }
    } else {
        let content = if agreement.privacy_content_en.trim().is_empty() {
            DEFAULT_PRIVACY_EN
        } else {
            &agreement.privacy_content_en
        };
        build_static_html("Privacy Policy", content, copyright, "en", "privacy", site_title)
    };
    write_file("privacy_en.html", &privacy_en_html).await?;

    tracing::info!(
        "✅ 站点协议静态页面已全部生成并同步至 {:?}",
        target_dirs
    );
    Ok(())
}

/// 清洗正文开头与页眉重复的大标题，防止页面顶部出现两行重叠的协议标题
fn sanitize_redundant_heading(content: &str, page_type: &str) -> String {
    let trimmed = content.trim();
    if !trimmed.starts_with("<h1") && !trimmed.starts_with("<h2") {
        return content.to_string();
    }
    let close_tag = if trimmed.starts_with("<h1") { "</h1" } else { "</h2" };
    if let Some(pos) = trimmed.find(close_tag) {
        if let Some(end_pos) = trimmed[pos..].find('>') {
            let heading_end = pos + end_pos + 1;
            let heading_text = trimmed[..heading_end].to_lowercase();
            let is_match = match page_type {
                "terms" | "tos" => {
                    heading_text.contains("服务条款")
                        || heading_text.contains("terms of service")
                        || heading_text.contains("terms")
                }
                "privacy" => {
                    heading_text.contains("隐私政策")
                        || heading_text.contains("隐私协议")
                        || heading_text.contains("privacy policy")
                        || heading_text.contains("privacy")
                }
                _ => false,
            };
            if is_match {
                return trimmed[heading_end..].trim_start().to_string();
            }
        }
    }
    content.to_string()
}

fn build_static_html(
    title: &str,
    content: &str,
    copyright: &str,
    lang: &str,
    page_type: &str,
    site_title: &str,
) -> String {
    let is_zh = lang == "zh";
    let html_lang = if is_zh { "zh-CN" } else { "en" };

    let footer_html = if !copyright.is_empty() {
        format!(r#"<footer class="footer">{}</footer>"#, html_escape(copyright))
    } else {
        String::new()
    };

    let cleaned_content = sanitize_redundant_heading(content, page_type);
    let normalized_content = cleaned_content.replace("&nbsp;", " ");

    let head_title = if site_title.trim().is_empty() {
        title.to_string()
    } else {
        format!("{}-{}", title, site_title.trim())
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="{html_lang}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Cache-Control" content="no-cache, no-store, must-revalidate">
  <meta http-equiv="Pragma" content="no-cache">
  <meta http-equiv="Expires" content="0">
  <title>{head_title_escaped}</title>
  <style>
    *, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
    html, body {{
      width: 100%;
      overflow-x: hidden;
    }}
    body {{
      min-height: 100vh;
      background-color: #09090b;
      color: #e4e4e7;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
      line-height: 1.8;
      display: flex;
      flex-direction: column;
      align-items: center;
      -webkit-font-smoothing: antialiased;
    }}
    .container {{
      max-width: 860px;
      width: 100%;
      margin: 0 auto;
      padding: 48px 24px 40px;
      flex: 1;
      box-sizing: border-box;
    }}
    .page-title {{
      color: #ffffff;
      font-size: 26px;
      font-weight: 600;
      text-align: center;
      margin-bottom: 36px;
      letter-spacing: -0.01em;
      word-break: break-word;
      overflow-wrap: break-word;
    }}
    .ql-editor {{
      font-size: 15px;
      line-height: 1.85;
      color: rgba(255, 255, 255, 0.85);
      word-break: break-word;
      overflow-wrap: anywhere;
      word-wrap: break-word;
      white-space: normal;
      width: 100%;
      max-width: 100%;
      box-sizing: border-box;
    }}
    .ql-editor h1, .ql-editor h2, .ql-editor h3, .ql-editor h4 {{
      color: #ffffff;
      margin-top: 30px;
      margin-bottom: 12px;
      font-weight: 600;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }}
    .ql-editor h1 {{ font-size: 24px; }}
    .ql-editor h2 {{ font-size: 20px; }}
    .ql-editor h3 {{ font-size: 17px; }}
    .ql-editor h4 {{ font-size: 15px; }}
    .ql-editor p {{
      margin-bottom: 14px;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }}
    .ql-editor ul, .ql-editor ol {{
      padding-left: 24px;
      margin-bottom: 16px;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }}
    .ql-editor li {{
      margin-bottom: 6px;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }}
    .ql-editor strong {{
      color: #ffffff;
      font-weight: 600;
    }}
    .ql-editor a {{
      color: #60a5fa;
      text-decoration: underline;
      word-break: break-all;
      overflow-wrap: anywhere;
    }}
    .ql-editor a:hover {{
      color: #93c5fd;
    }}
    .ql-editor blockquote {{
      border-left: 3px solid #3b82f6;
      padding: 10px 16px;
      margin: 18px 0;
      color: rgba(255, 255, 255, 0.75);
      background: rgba(255, 255, 255, 0.03);
      border-radius: 0 6px 6px 0;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }}
    .ql-editor table {{
      width: 100%;
      border-collapse: collapse;
      margin: 18px 0;
      font-size: 14px;
      word-break: break-word;
    }}
    .ql-editor th, .ql-editor td {{
      border: 1px solid rgba(255, 255, 255, 0.12);
      padding: 10px 14px;
      word-break: break-word;
      overflow-wrap: anywhere;
    }}
    .ql-editor th {{
      background: rgba(255, 255, 255, 0.05);
      font-weight: 600;
      color: #ffffff;
    }}

    /* Quill 对齐扩展支持 */
    .ql-editor .ql-align-center {{ text-align: center; }}
    .ql-editor .ql-align-right {{ text-align: right; }}
    .ql-editor .ql-align-justify {{ text-align: justify; }}

    /* Quill 缩进扩展支持 */
    .ql-editor .ql-indent-1 {{ padding-left: 2em; }}
    .ql-editor .ql-indent-2 {{ padding-left: 4em; }}
    .ql-editor .ql-indent-3 {{ padding-left: 6em; }}
    .ql-editor .ql-indent-4 {{ padding-left: 8em; }}
    .ql-editor .ql-indent-5 {{ padding-left: 10em; }}
    .ql-editor .ql-indent-6 {{ padding-left: 12em; }}
    .ql-editor .ql-indent-7 {{ padding-left: 14em; }}
    .ql-editor .ql-indent-8 {{ padding-left: 16em; }}

    /* Quill 字号扩展支持 */
    .ql-editor .ql-size-small {{ font-size: 13px; }}
    .ql-editor .ql-size-large {{ font-size: 18px; }}
    .ql-editor .ql-size-huge {{ font-size: 22px; }}

    /* Quill 代码块支持 */
    .ql-editor pre, .ql-editor pre.ql-syntax {{
      background: rgba(255, 255, 255, 0.05);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 6px;
      padding: 12px 16px;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 13px;
      line-height: 1.6;
      overflow-x: auto;
      margin: 14px 0;
      color: #93c5fd;
    }}
    .ql-editor code {{
      background: rgba(255, 255, 255, 0.08);
      padding: 2px 6px;
      border-radius: 4px;
      font-size: 13px;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    }}

    .footer {{
      width: 100%;
      text-align: center;
      font-size: 13px;
      line-height: 1.6;
      color: rgba(255, 255, 255, 0.45);
      padding: 24px 24px 36px;
      user-select: none;
      border-top: 1px solid rgba(255, 255, 255, 0.06);
      box-sizing: border-box;
    }}
    @media (max-width: 640px) {{
      .container {{ padding: 32px 16px 24px; }}
      .page-title {{ font-size: 21px; margin-bottom: 24px; }}
    }}
  </style>
</head>
<body>
  <div class="container">
    <h1 class="page-title">{title_escaped}</h1>
    <div class="ql-editor">
      {content}
    </div>
  </div>
  {footer_html}
</body>
</html>
"#,
        html_lang = html_lang,
        head_title_escaped = html_escape(&head_title),
        title_escaped = html_escape(title),
        content = normalized_content,
        footer_html = footer_html
    )
}

fn build_redirect_html(title: &str, target_url: &str, site_title: &str) -> String {
    let clean_url = target_url.trim();
    let url_with_proto = if !clean_url.starts_with("http://")
        && !clean_url.starts_with("https://")
        && !clean_url.starts_with('/')
    {
        format!("https://{}", clean_url)
    } else {
        clean_url.to_string()
    };

    let head_title = if site_title.trim().is_empty() {
        title.to_string()
    } else {
        format!("{}-{}", title, site_title.trim())
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Cache-Control" content="no-cache, no-store, must-revalidate">
  <meta http-equiv="Pragma" content="no-cache">
  <meta http-equiv="Expires" content="0">
  <meta http-equiv="refresh" content="0;url={url}">
  <title>{head_title}</title>
  <script>window.location.replace("{url}");</script>
  <style>
    body {{
      background: #09090b;
      color: rgba(255, 255, 255, 0.6);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      display: flex;
      align-items: center;
      justify-content: center;
      height: 100vh;
      margin: 0;
      padding: 20px;
      box-sizing: border-box;
      text-align: center;
    }}
    a {{ color: #3b82f6; text-decoration: underline; word-break: break-all; }}
  </style>
</head>
<body>
  <p>正在跳转至 <a href="{url}">{title}</a> ...</p>
</body>
</html>
"#,
        head_title = html_escape(&head_title),
        title = html_escape(title),
        url = html_escape(&url_with_proto)
    )
}

/// 中文界面用中文协议，其余语言用英文协议。未带 lang 时看浏览器首选语言。
fn legal_lang_suffix(explicit: Option<&str>, accept_language: Option<&str>) -> &'static str {
    if let Some(raw) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return if is_chinese_lang(raw) { "zh" } else { "en" };
    }
    match accept_language.map(str::trim).filter(|s| !s.is_empty()) {
        Some(accept) => {
            let primary = accept
                .split(',')
                .next()
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim();
            if primary.is_empty() || is_chinese_lang(primary) {
                "zh"
            } else {
                "en"
            }
        }
        None => "zh",
    }
}

fn is_chinese_lang(lang: &str) -> bool {
    let lang = lang.trim().to_ascii_lowercase();
    lang == "zh" || lang.starts_with("zh-") || lang.starts_with("zh_")
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_static_html() {
        let html = build_static_html(
            "测试协议",
            "<p>正文内容</p>",
            "© 2026 Test",
            "zh",
            "terms",
            "测试站点",
        );
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>测试协议-测试站点</title>"));
        assert!(html.contains("测试协议"));
        assert!(html.contains("<p>正文内容</p>"));
        assert!(html.contains("© 2026 Test"));
        assert!(!html.contains("lang-switch"));
    }

    #[test]
    fn legal_content_follows_ui_language() {
        assert_eq!(legal_lang_suffix(Some("zh"), None), "zh");
        assert_eq!(legal_lang_suffix(Some("zh-TW"), None), "zh");
        assert_eq!(legal_lang_suffix(Some("en"), None), "en");
        assert_eq!(legal_lang_suffix(Some("ja"), None), "en");
        assert_eq!(legal_lang_suffix(Some("ko"), None), "en");
        assert_eq!(legal_lang_suffix(None, Some("ja,en;q=0.8")), "en");
        assert_eq!(legal_lang_suffix(None, Some("zh-CN,en;q=0.8")), "zh");
    }

    #[test]
    fn test_sanitize_redundant_heading() {
        let raw = "<h2>服务条款 (Terms of Service)</h2>\n<p>正文内容</p>";
        let cleaned = sanitize_redundant_heading(raw, "terms");
        assert_eq!(cleaned, "<p>正文内容</p>");

        let raw_en = "<h2>Terms of Service</h2><p>Content</p>";
        let cleaned_en = sanitize_redundant_heading(raw_en, "terms");
        assert_eq!(cleaned_en, "<p>Content</p>");

        let raw_other = "<h2>一、服务内容与范围</h2><p>正文内容</p>";
        let cleaned_other = sanitize_redundant_heading(raw_other, "terms");
        assert_eq!(cleaned_other, raw_other);
    }

    #[test]
    fn test_build_redirect_html() {
        let html = build_redirect_html("外部条款", "example.com/terms", "测试站点");
        assert!(html.contains("<title>外部条款-测试站点</title>"));
        assert!(html.contains("https://example.com/terms"));
        assert!(html.contains("meta http-equiv=\"refresh\""));
    }
}

