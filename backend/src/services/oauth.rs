/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

/// 微信用户信息
#[derive(Debug, Serialize, Deserialize)]
pub struct WechatUserInfo {
    pub openid: String,
    pub unionid: Option<String>,
    pub nickname: Option<String>,
    pub headimgurl: Option<String>,
}

/// 谷歌用户信息。`id` 为空表示这次同意只带回了邮箱。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleUserInfo {
    pub id: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
    pub picture: Option<String>,
}

impl GoogleUserInfo {
    /// 可以写入 `users.email` 的地址。显式未验证的邮箱不返回。
    pub fn verified_email(&self) -> Option<String> {
        if !self.email_verified {
            return None;
        }
        let email = self.email.as_deref()?.trim();
        if !plausible_google_email(email) {
            return None;
        }
        Some(email.to_string())
    }

    /// 登录用的稳定键。有 `id`/`sub` 时用它；只有已验证邮箱时用 `email:{小写地址}`。
    pub fn account_key(&self) -> Option<String> {
        let id = self.id.trim();
        if !id.is_empty() {
            return Some(id.to_string());
        }
        self.verified_email()
            .map(|email| format!("email:{}", email.to_ascii_lowercase()))
    }
}

/// 已保存的谷歌身份是否就是这次授权。真实 id 与「仅邮箱」键都认。
pub fn google_identity_matches(stored: Option<&str>, info: &GoogleUserInfo) -> bool {
    let Some(stored) = stored.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    if !info.id.trim().is_empty() && stored == info.id.trim() {
        return true;
    }
    match info.verified_email() {
        Some(email) => stored == format!("email:{}", email.to_ascii_lowercase()),
        None => false,
    }
}

/// OAuth 服务（无状态，纯函数式，低耦合易扩展）
pub struct OAuthService;

impl OAuthService {
    // ======================== 微信 OAuth ========================

    /// 生成微信授权 URL
    pub fn wechat_auth_url(app_id: &str, redirect_uri: &str, state: &str) -> String {
        format!(
            "https://open.weixin.qq.com/connect/qrconnect?appid={}&redirect_uri={}&response_type=code&scope=snsapi_login&state={}#wechat_redirect",
            app_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(state),
        )
    }

    /// 微信 code 换取用户信息
    pub async fn wechat_exchange(
        app_id: &str,
        app_secret: &str,
        code: &str,
    ) -> AppResult<WechatUserInfo> {
        let client = reqwest::Client::new();

        // 1. code 换 access_token + openid
        let token_url = format!(
            "https://api.weixin.qq.com/sns/oauth2/access_token?appid={}&secret={}&code={}&grant_type=authorization_code",
            app_id, app_secret, code
        );
        let token_resp: serde_json::Value = client
            .get(&token_url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("微信 token 请求失败: {}", e)))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("微信 token 解析失败: {}", e)))?;

        if let Some(errcode) = token_resp.get("errcode") {
            let code = errcode.as_i64().unwrap_or(0);
            if code != 0 {
                let msg = token_resp["errmsg"].as_str().unwrap_or("unknown");
                return Err(AppError::Internal(format!("微信授权失败: {}", msg)));
            }
        }

        let access_token = token_resp["access_token"]
            .as_str()
            .ok_or_else(|| AppError::Internal("微信返回缺少 access_token".to_string()))?;
        let openid = token_resp["openid"]
            .as_str()
            .ok_or_else(|| AppError::Internal("微信返回缺少 openid".to_string()))?;
        let unionid = token_resp["unionid"].as_str().map(|s| s.to_string());

        // 2. 获取用户信息
        let info_url = format!(
            "https://api.weixin.qq.com/sns/userinfo?access_token={}&openid={}",
            access_token, openid
        );
        let info_resp: serde_json::Value = client
            .get(&info_url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("微信用户信息请求失败: {}", e)))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("微信用户信息解析失败: {}", e)))?;

        Ok(WechatUserInfo {
            openid: openid.to_string(),
            unionid: unionid.or_else(|| info_resp["unionid"].as_str().map(|s| s.to_string())),
            nickname: info_resp["nickname"].as_str().map(|s| s.to_string()),
            headimgurl: info_resp["headimgurl"].as_str().map(|s| s.to_string()),
        })
    }

    // ======================== 谷歌 OAuth ========================

    /// 生成谷歌授权 URL
    pub fn google_auth_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}&access_type=offline",
            client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(state),
        )
    }

    /// 谷歌 code 换取用户信息
    pub async fn google_exchange(
        client_id: &str,
        client_secret: &str,
        code: &str,
        redirect_uri: &str,
    ) -> AppResult<GoogleUserInfo> {
        let client = reqwest::Client::new();

        // 1. code 换 access_token
        let token_resp: serde_json::Value = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code),
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("redirect_uri", redirect_uri),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("谷歌 token 请求失败: {}", e)))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("谷歌 token 解析失败: {}", e)))?;

        let access_token = token_resp["access_token"].as_str().ok_or_else(|| {
            let err = token_resp["error_description"]
                .as_str()
                .unwrap_or("unknown error");
            AppError::Internal(format!("谷歌授权失败: {}", err))
        })?;

        // id_token 与 userinfo 同源自谷歌 token 端点，只取声明，用来补上「只同意邮箱」时 userinfo 没有 id 的情况。
        let claims = token_resp
            .get("id_token")
            .and_then(|v| v.as_str())
            .and_then(decode_google_id_token_payload);

        // 2. 获取用户信息
        let info_resp: serde_json::Value = client
            .get("https://www.googleapis.com/oauth2/v2/userinfo")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("谷歌用户信息请求失败: {}", e)))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("谷歌用户信息解析失败: {}", e)))?;

        parse_google_profile(claims.as_ref(), &info_resp)
    }
}

fn decode_google_id_token_payload(id_token: &str) -> Option<serde_json::Value> {
    use base64::Engine as _;
    let payload = id_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn json_text(v: &serde_json::Value, key: &str) -> Option<String> {
    match v.get(key)? {
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        serde_json::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                Some(u.to_string())
            } else {
                n.as_i64().map(|i| i.to_string())
            }
        }
        _ => None,
    }
}

fn email_verified_flag(v: &serde_json::Value) -> Option<bool> {
    for key in ["verified_email", "email_verified"] {
        if let Some(flag) = v.get(key).and_then(|item| item.as_bool()) {
            return Some(flag);
        }
    }
    None
}

fn plausible_google_email(email: &str) -> bool {
    let email = email.trim();
    if email.is_empty() || email.to_ascii_lowercase().ends_with("@tokensbyte.local") {
        return false;
    }
    let mut parts = email.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        _ => false,
    }
}

/// 合并 userinfo 与 id_token。任一侧显式 `verified=false` 则不把邮箱当已验证。
pub(crate) fn parse_google_profile(
    claims: Option<&serde_json::Value>,
    info: &serde_json::Value,
) -> AppResult<GoogleUserInfo> {
    let empty = serde_json::Value::Null;
    let claims = claims.unwrap_or(&empty);
    let id = json_text(info, "id")
        .or_else(|| json_text(info, "sub"))
        .or_else(|| json_text(claims, "sub"))
        .unwrap_or_default();
    let email = json_text(info, "email").or_else(|| json_text(claims, "email"));
    let name = json_text(info, "name").or_else(|| json_text(claims, "name"));
    let picture = json_text(info, "picture").or_else(|| json_text(claims, "picture"));
    let flags = [email_verified_flag(info), email_verified_flag(claims)];
    let email_verified = match (flags[0], flags[1]) {
        (Some(a), Some(b)) => a && b,
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => email.is_some(),
    };
    if id.trim().is_empty() && email.as_deref().map(str::trim).unwrap_or("").is_empty() {
        let msg = info
            .get("error_description")
            .or_else(|| info.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("谷歌未返回已验证邮箱或账号标识");
        return Err(AppError::Internal(msg.to_string()));
    }
    Ok(GoogleUserInfo {
        id,
        email,
        email_verified,
        name,
        picture,
    })
}

#[cfg(test)]
mod google_profile_tests {
    use super::{google_identity_matches, parse_google_profile};
    use base64::Engine as _;

    fn jwt(payload: &str) -> serde_json::Value {
        let body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("e30.{body}.sig");
        serde_json::json!({ "id_token": token })
    }

    #[test]
    fn email_only_userinfo_binds_verified_email() {
        let info = serde_json::json!({
            "email": "User@gmail.com",
            "verified_email": true
        });
        let profile = parse_google_profile(None, &info).unwrap();
        assert!(profile.id.is_empty());
        assert_eq!(profile.verified_email().as_deref(), Some("User@gmail.com"));
        assert_eq!(
            profile.account_key().as_deref(),
            Some("email:user@gmail.com")
        );
        assert!(google_identity_matches(
            Some("email:user@gmail.com"),
            &profile
        ));
    }

    #[test]
    fn numeric_id_and_id_token_sub_fill_missing_userinfo_id() {
        let info = serde_json::json!({
            "id": 1234567890123456789u64,
            "email": "a@gmail.com",
            "verified_email": true
        });
        let profile = parse_google_profile(None, &info).unwrap();
        assert_eq!(profile.id, "1234567890123456789");
        assert_eq!(profile.account_key().as_deref(), Some("1234567890123456789"));

        let claims_token = jwt(r#"{"sub":"sub-9","email":"b@gmail.com","email_verified":true}"#);
        let claims = super::decode_google_id_token_payload(
            claims_token["id_token"].as_str().unwrap(),
        );
        let email_only = serde_json::json!({ "email": "b@gmail.com", "verified_email": true });
        let merged = parse_google_profile(claims.as_ref(), &email_only).unwrap();
        assert_eq!(merged.id, "sub-9");
        assert_eq!(merged.verified_email().as_deref(), Some("b@gmail.com"));
    }

    #[test]
    fn explicit_unverified_email_is_not_an_account_key() {
        let info = serde_json::json!({
            "email": "a@gmail.com",
            "verified_email": false
        });
        let profile = parse_google_profile(None, &info).unwrap();
        assert!(profile.verified_email().is_none());
        assert!(profile.account_key().is_none());
        assert!(!google_identity_matches(Some("email:a@gmail.com"), &profile));
    }

    #[test]
    fn email_without_verified_flag_still_binds() {
        let info = serde_json::json!({ "email": "only@gmail.com" });
        let profile = parse_google_profile(None, &info).unwrap();
        assert_eq!(profile.verified_email().as_deref(), Some("only@gmail.com"));
    }
}
