/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

pub mod contact;

use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // user id
    pub username: String,
    pub role: String,
    pub exp: usize,
    /// 签发时间（秒）。旧 token 无此字段时按 0 处理，改过密码即失效
    #[serde(default)]
    pub iat: usize,
}

pub const PASSWORD_MIN_CHARS: usize = 6;
pub const PASSWORD_MAX_CHARS: usize = 128;

/// 用户端设置密码（注册、找回、个人改密）统一长度规则
pub fn validate_password(password: &str) -> AppResult<()> {
    let n = password.chars().count();
    if n < PASSWORD_MIN_CHARS {
        return Err(AppError::BadRequest(format!(
            "密码长度至少为 {} 位",
            PASSWORD_MIN_CHARS
        )));
    }
    if n > PASSWORD_MAX_CHARS {
        return Err(AppError::BadRequest(format!(
            "密码长度不能超过 {} 位",
            PASSWORD_MAX_CHARS
        )));
    }
    Ok(())
}

/// Hash a password using Argon2
pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {}", e)))?;
    Ok(hash.to_string())
}

/// Verify a password against a hash
/// 支持两种哈希算法：
/// - bcrypt ($2y$, $2b$前缀) - 迁移用户使用
/// - Argon2 - 新用户使用
pub fn verify_password(password: &str, hash: &str) -> AppResult<bool> {
    // 判断是否为bcrypt哈希
    if hash.starts_with("$2y$") || hash.starts_with("$2b$") {
        Ok(bcrypt::verify(password, hash)
            .map_err(|e| AppError::Internal(format!("Bcrypt verify failed: {}", e)))?)
    } else {
        // 否则使用Argon2验证
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| AppError::Internal(format!("Invalid hash: {}", e)))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }
}

/// Create a JWT token
pub fn create_token(user_id: &str, username: &str, role: &str, secret: &str) -> AppResult<String> {
    let now = chrono::Utc::now();
    let expiration = now
        .checked_add_signed(chrono::Duration::days(7))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        role: role.to_string(),
        exp: expiration,
        iat: now.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("JWT creation failed: {}", e)))
}

/// Validate a JWT token
pub fn validate_token(token: &str, secret: &str) -> AppResult<Claims> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::Unauthorized)
}

/// Generate a new API token key (sk-xxx format)
pub fn generate_api_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    format!(
        "sk-{}",
        hex::encode(&bytes).chars().take(48).collect::<String>()
    )
}
