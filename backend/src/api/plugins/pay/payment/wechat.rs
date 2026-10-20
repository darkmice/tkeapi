/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::models::PaymentWechatSettings;
use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use anyhow::{anyhow, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::Local;
use rand::Rng;
use reqwest::Client;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::sha2::Sha256;
use rsa::signature::{SignatureEncoding, Signer};

pub struct WechatClient {
    settings: PaymentWechatSettings,
    http_client: Client,
}

impl WechatClient {
    pub fn new(settings: PaymentWechatSettings) -> Self {
        Self {
            settings,
            http_client: Client::new(),
        }
    }

    fn generate_nonce(&self) -> String {
        let mut rng = rand::thread_rng();
        (0..32)
            .map(|_| {
                let idx = rng.gen_range(0..62);
                let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                chars[idx] as char
            })
            .collect()
    }

    fn sign(
        &self,
        method: &str,
        url: &str,
        timestamp: &str,
        nonce: &str,
        body: &str,
    ) -> Result<String> {
        let message = format!("{}\n{}\n{}\n{}\n{}\n", method, url, timestamp, nonce, body);

        // Parse private key
        let mut pk_str = self.settings.private_key.trim().to_string();
        if !pk_str.starts_with("-----BEGIN PRIVATE KEY-----")
            && !pk_str.starts_with("-----BEGIN RSA PRIVATE KEY-----")
        {
            pk_str = format!(
                "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
                pk_str
                    .chars()
                    .collect::<Vec<char>>()
                    .chunks(64)
                    .map(|c| c.into_iter().collect::<String>())
                    .collect::<Vec<String>>()
                    .join("\n")
            );
        }

        let priv_key = rsa::RsaPrivateKey::from_pkcs8_pem(&pk_str)
            .map_err(|e| anyhow!("Failed to parse Wechat private key: {}", e))?;

        let signing_key = SigningKey::<Sha256>::new(priv_key);
        let signature = signing_key.sign(message.as_bytes());
        Ok(STANDARD.encode(signature.to_bytes()))
    }

    fn build_auth_header(&self, method: &str, url: &str, body: &str) -> Result<String> {
        let timestamp = Local::now().timestamp().to_string();
        let nonce = self.generate_nonce();
        let signature = self.sign(method, url, &timestamp, &nonce, body)?;

        Ok(format!(
            "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",signature=\"{}\",timestamp=\"{}\",serial_no=\"{}\"",
            self.settings.mchid,
            nonce,
            signature,
            timestamp,
            self.settings.cert_serial_no
        ))
    }

    pub async fn create_native_order(
        &self,
        out_trade_no: &str,
        amount: f64,
        description: &str,
        notify_url: &str,
    ) -> Result<String> {
        let url_path = "/v3/pay/transactions/native";
        let full_url = format!("https://api.mch.weixin.qq.com{}", url_path);

        // Convert amount to cents
        let total = (amount * 100.0).round() as i32;

        let body = serde_json::json!({
            "mchid": self.settings.mchid,
            "out_trade_no": out_trade_no,
            "appid": self.settings.appid,
            "description": description,
            "notify_url": notify_url,
            "amount": {
                "total": total,
                "currency": "CNY"
            }
        });

        let body_str = body.to_string();
        let auth_header = self.build_auth_header("POST", url_path, &body_str)?;

        let resp = self
            .http_client
            .post(&full_url)
            .header("Authorization", auth_header)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("User-Agent", "TokensByte/1.0")
            .body(body_str)
            .send()
            .await?;

        let status = resp.status();
        let resp_body = resp.text().await?;

        if status.is_success() {
            let json: serde_json::Value = serde_json::from_str(&resp_body)?;
            if let Some(code_url) = json.get("code_url").and_then(|v| v.as_str()) {
                Ok(code_url.to_string())
            } else {
                Err(anyhow!("No code_url in wechat response"))
            }
        } else {
            Err(anyhow!("Wechat pay error: [{}] {}", status, resp_body))
        }
    }

    pub fn decrypt_callback_resource(
        &self,
        nonce: &str,
        associated_data: &str,
        ciphertext: &str,
    ) -> Result<String> {
        let key_bytes = self.settings.api_v3_key.as_bytes();
        if key_bytes.len() != 32 {
            return Err(anyhow!("Wechat API v3 key must be exactly 32 bytes"));
        }

        let key = Key::<Aes256Gcm>::from_slice(key_bytes);
        let cipher = Aes256Gcm::new(key);

        let nonce_bytes = nonce.as_bytes();
        // 回调报文未鉴权：from_slice 长度不符会 panic，release 为 panic=abort 将整进程退出
        if nonce_bytes.len() != 12 {
            return Err(anyhow!("Wechat callback nonce must be exactly 12 bytes"));
        }
        let nonce_obj = Nonce::from_slice(nonce_bytes);

        let cipher_decoded = STANDARD.decode(ciphertext)?;

        let payload = Payload {
            msg: &cipher_decoded,
            aad: associated_data.as_bytes(),
        };

        match cipher.decrypt(nonce_obj, payload) {
            Ok(plaintext) => Ok(String::from_utf8(plaintext)?),
            Err(e) => Err(anyhow!("AES-GCM decryption failed: {:?}", e)),
        }
    }

    pub async fn fetch_platform_certs(
        &self,
    ) -> Result<std::collections::BTreeMap<String, rsa::RsaPublicKey>> {
        static WECHAT_CERT_CACHE: std::sync::LazyLock<
            dashmap::DashMap<
                String,
                (
                    std::collections::BTreeMap<String, rsa::RsaPublicKey>,
                    std::time::Instant,
                ),
            >,
        > = std::sync::LazyLock::new(dashmap::DashMap::new);
        const CERT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(600);

        let cache_key = format!(
            "{}:{}",
            self.settings.mchid.trim(),
            self.settings.cert_serial_no.trim()
        );
        if let Some(cached) = WECHAT_CERT_CACHE.get(&cache_key) {
            if cached.1.elapsed() < CERT_CACHE_TTL && !cached.0.is_empty() {
                return Ok(cached.0.clone());
            }
        }

        let url_path = "/v3/certificates";
        let full_url = format!("https://api.mch.weixin.qq.com{}", url_path);

        let auth_header = self.build_auth_header("GET", url_path, "")?;

        let resp = self
            .http_client
            .get(&full_url)
            .header("Authorization", auth_header)
            .header("Accept", "application/json")
            .header("User-Agent", "TokensByte/1.0")
            .send()
            .await?;

        let status = resp.status();
        let resp_body = resp.text().await?;

        if !status.is_success() {
            return Err(anyhow!(
                "Failed to fetch WeChat platform certs: [{}] {}",
                status,
                resp_body
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&resp_body)?;
        let data = json["data"]
            .as_array()
            .ok_or_else(|| anyhow!("Invalid cert format"))?;

        let mut certs = std::collections::BTreeMap::new();
        for item in data {
            if let Some(serial_no) = item["serial_no"].as_str() {
                let encrypt_cert = &item["encrypt_certificate"];
                if let (Some(nonce), Some(assoc_data), Some(ciphertext)) = (
                    encrypt_cert["nonce"].as_str(),
                    encrypt_cert["associated_data"].as_str(),
                    encrypt_cert["ciphertext"].as_str(),
                ) {
                    if let Ok(pem) = self.decrypt_callback_resource(nonce, assoc_data, ciphertext) {
                        if let Ok(pub_key) = extract_pub_key_from_x509_pem(&pem) {
                            certs.insert(serial_no.to_string(), pub_key);
                        }
                    }
                }
            }
        }
        if !certs.is_empty() {
            WECHAT_CERT_CACHE.insert(cache_key, (certs.clone(), std::time::Instant::now()));
        }
        Ok(certs)
    }

    pub fn verify_signature(
        &self,
        pub_key: &rsa::RsaPublicKey,
        timestamp: &str,
        nonce: &str,
        body: &str,
        signature_base64: &str,
    ) -> Result<bool> {
        let message = format!("{}\n{}\n{}\n", timestamp, nonce, body);
        let signature = STANDARD.decode(signature_base64)?;

        let sig = rsa::pkcs1v15::Signature::try_from(signature.as_slice())
            .map_err(|e| anyhow!("Invalid WeChat signature format: {}", e))?;

        let vk = rsa::pkcs1v15::VerifyingKey::<rsa::sha2::Sha256>::new(pub_key.clone());
        use rsa::signature::Verifier;
        if vk.verify(message.as_bytes(), &sig).is_ok() {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn extract_pub_key_from_x509_pem(pem: &str) -> Result<rsa::RsaPublicKey> {
    use rsa::pkcs8::DecodePublicKey;
    use x509_cert::der::{Decode, Encode};
    use x509_cert::Certificate;

    let der = x509_cert::der::pem::decode_vec(pem.as_bytes())
        .map_err(|e| anyhow!("Failed to decode PEM: {}", e))?
        .1;
    let cert = Certificate::from_der(&der).map_err(|e| anyhow!("Failed to decode X509: {}", e))?;
    let spki_der = cert
        .tbs_certificate()
        .subject_public_key_info()
        .to_der()
        .map_err(|e| anyhow!("Failed to encode SPKI: {}", e))?;
    let pub_key = rsa::RsaPublicKey::from_public_key_der(&spki_der)
        .map_err(|e| anyhow!("Failed to decode RsaPublicKey from SPKI: {}", e))?;
    Ok(pub_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decrypt_rejects_bad_nonce_without_panic() {
        let client = WechatClient::new(PaymentWechatSettings {
            api_v3_key: "0123456789abcdef0123456789abcdef".to_string(),
            ..Default::default()
        });
        for nonce in ["", "x", "0123456789abc"] {
            assert!(client.decrypt_callback_resource(nonce, "", "").is_err());
        }
    }
}
