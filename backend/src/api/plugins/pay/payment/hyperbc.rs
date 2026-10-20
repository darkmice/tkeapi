/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::models::PaymentHyperbcSettings;
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use md5::{Digest, Md5};
use reqwest::Client;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub struct HyperbcClient {
    settings: PaymentHyperbcSettings,
    http: Client,
}

/// CipherBC API 统一响应格式
/// 文档: {status: 200, msg: "success", data: {...}, sign: "..."}
#[derive(Debug, Deserialize)]
pub struct HyperbcResponse<T> {
    pub status: i32,
    pub msg: Option<String>,
    pub data: Option<T>,
    #[serde(default)]
    #[allow(dead_code)]
    pub sign: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct H5AddressInfo {
    pub coin: String,
    pub address: String,
    pub amount: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateH5OrderData {
    pub checkout_url: String,
    pub order_no: String,
    pub addresses: Option<Vec<H5AddressInfo>>,
    pub amount: Option<String>,
    pub currency: Option<String>,
}

fn deserialize_flexible_i32<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        Some(serde_json::Value::Number(n)) => Ok(n.as_i64().unwrap_or(0) as i32),
        Some(serde_json::Value::String(s)) => Ok(s.parse::<i32>().unwrap_or(0)),
        _ => Ok(0),
    }
}

fn deserialize_flexible_i32_opt<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        Some(serde_json::Value::Number(n)) => Ok(n.as_i64().map(|x| x as i32)),
        Some(serde_json::Value::String(s)) => Ok(s.parse::<i32>().ok()),
        _ => Ok(None),
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HyperbcPaymentItem {
    pub coin: Option<String>,
    pub address: Option<String>,
    pub txid: Option<String>,
    pub amount: Option<String>,
    pub confirm_time: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_flexible_i32_opt")]
    pub status: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_flexible_i32_opt")]
    pub check_status: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_flexible_i32_opt")]
    pub check_code: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct QueryOrderData {
    #[serde(default)]
    pub order_no: String,
    #[serde(default)]
    pub merchant_order_id: String,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_i32")]
    pub status: i32,
    #[serde(default, deserialize_with = "deserialize_flexible_i32_opt")]
    pub check_status: Option<i32>,
    pub pay_amount: Option<String>,
    pub pay_currency: Option<String>,
    #[serde(default)]
    pub payments: Option<Vec<HyperbcPaymentItem>>,
}

impl QueryOrderData {
    /// 统计 payments 中有效到账的稳定币总金额（仅统计链上已确认状态且币种为 USDT/USDC 的正数金额）
    pub fn get_actual_crypto_amount(&self) -> f64 {
        if let Some(ref list) = self.payments {
            list.iter()
                .filter(|p| {
                    p.status.is_none_or(|s| s == 1 || s == 2)
                        && p.coin
                            .as_deref()
                            .map(str::trim)
                            .filter(|c| !c.is_empty())
                            .is_none_or(|c| {
                                let upper = c.to_ascii_uppercase();
                                upper.starts_with("USDT") || upper.starts_with("USDC")
                            })
                })
                .filter_map(|p| p.amount.as_deref().and_then(|a| a.parse::<f64>().ok()))
                .filter(|&a| a.is_finite() && a > 0.0)
                .sum()
        } else {
            0.0
        }
    }

    /// 提取异常状态码（优先从 check_status，若无则从 payments[i].check_status 或 check_code 提取）
    pub fn get_check_code(&self) -> i32 {
        if let Some(c) = self.check_status {
            if c != 0 {
                return c;
            }
        }
        if let Some(ref list) = self.payments {
            for p in list {
                if let Some(c) = p.check_status.or(p.check_code) {
                    if c != 0 {
                        return c;
                    }
                }
            }
        }
        0
    }
}

fn normalize_pem(pem: &str) -> String {
    let replaced = pem.replace(['\u{2014}', '\u{2013}'], "-");
    let lines: Vec<&str> = replaced
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

impl HyperbcClient {
    pub fn new(settings: PaymentHyperbcSettings) -> Self {
        Self {
            settings,
            http: Client::new(),
        }
    }

    /// 从回调原始 JSON 字符串中自适应提取待签名内容：
    /// 基于全局公共 OrderedJson 机制，若包含 data 节点且为 Object 则提取 data 内部字段，否则提取根节点
    pub fn get_sign_content_from_raw(raw_json: &str) -> Result<String> {
        let root: crate::utils::OrderedJson =
            serde_json::from_str(raw_json).map_err(|e| anyhow!("解析原始 JSON 失败: {}", e))?;
        Ok(root.data_or_self().to_sign_query(true, true, &["sign"]))
    }

    pub fn get_sign_content(val: &Value) -> String {
        // 递归展平子层级（嵌套的 Object 或 Array），提取并拼接所有叶子节点的值，丢弃 key 且不含拼接符
        fn value_as_string(v: &Value) -> String {
            match v {
                Value::Null => String::new(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                Value::Array(arr) => {
                    let mut s = String::new();
                    for item in arr {
                        s.push_str(&value_as_string(item));
                    }
                    s
                }
                Value::Object(map) => {
                    let mut s = String::new();
                    for (_, val) in map {
                        s.push_str(&value_as_string(val));
                    }
                    s
                }
            }
        }

        if let Value::Object(map) = val {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort(); // 第一层 key 依然按照字典序排序

            let mut parts = Vec::new();
            for k in keys {
                if k == "sign" {
                    continue;
                }
                let v = map.get(k).unwrap();
                if v.is_null() {
                    continue;
                }
                let v_str = value_as_string(v);
                parts.push(format!("{}={}", k, v_str));
            }
            parts.join("&")
        } else {
            value_as_string(val)
        }
    }

    /// 使用商户 RSA 私钥对参数进行 RSA-MD5 签名
    pub fn rsa_sign(&self, val: &Value) -> Result<String> {
        let sign_str = Self::get_sign_content(val);

        // 1. 计算 MD5
        let mut hasher = Md5::new();
        hasher.update(sign_str.as_bytes());
        let hash_result = hasher.finalize();

        // 2. 构造 PKCS#1 v1.5 MD5 DigestInfo 前缀
        let prefix: [u8; 18] = [
            0x30, 0x20, 0x30, 0x0c, 0x06, 0x08, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x02, 0x05,
            0x05, 0x00, 0x04, 0x10,
        ];

        let mut digest_info = Vec::with_capacity(34);
        digest_info.extend_from_slice(&prefix);
        digest_info.extend_from_slice(&hash_result);

        // 3. 使用商户私钥做 Raw RSA 签名 (PKCS#1 v1.5，清洗后兼容 PKCS#8 与 PKCS#1)
        let clean_pem = normalize_pem(&self.settings.merchant_private_key);
        let private_key = RsaPrivateKey::from_pkcs8_pem(clean_pem.trim())
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(clean_pem.trim()))
            .map_err(|e| anyhow!("解析商户私钥失败: {}", e))?;

        let signature_bytes = private_key
            .sign(Pkcs1v15Sign::new_unprefixed(), &digest_info)
            .map_err(|e| anyhow!("RSA 签名计算失败: {}", e))?;

        Ok(BASE64.encode(signature_bytes))
    }

    /// 核心验签方法：传入待签名串与签名 Base64 字符串
    pub fn verify_signature_str(&self, sign_str: &str, sign_base64: &str) -> Result<bool> {
        // 1. 计算 MD5
        let mut hasher = Md5::new();
        hasher.update(sign_str.as_bytes());
        let hash_result = hasher.finalize();

        // 2. 构造 DigestInfo
        let prefix: [u8; 18] = [
            0x30, 0x20, 0x30, 0x0c, 0x06, 0x08, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x02, 0x05,
            0x05, 0x00, 0x04, 0x10,
        ];

        let mut digest_info = Vec::with_capacity(34);
        digest_info.extend_from_slice(&prefix);
        digest_info.extend_from_slice(&hash_result);

        // 3. 公钥验签（使用清洗后的公钥，容错中文字符与换行）
        let clean_pem = normalize_pem(&self.settings.hyperbc_public_key);
        let public_key = RsaPublicKey::from_public_key_pem(clean_pem.trim())
            .map_err(|e| anyhow!("解析 HyperBC 平台公钥失败: {}", e))?;

        let signature_bytes = BASE64
            .decode(sign_base64)
            .map_err(|e| anyhow!("Base64 解码签名失败: {}", e))?;

        match public_key.verify(
            Pkcs1v15Sign::new_unprefixed(),
            &digest_info,
            &signature_bytes,
        ) {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!("[HyperBC验签] RSA 校验不通过详情: {:?}", e);
                Ok(false)
            }
        }
    }

    /// 构造 HyperBC 回调成功标准响应 JSON（官方规范：包含 status 200、商户私钥 RSA-MD5 签名与 success_data）
    pub fn success_response(&self) -> String {
        let sign = self
            .rsa_sign(&serde_json::json!({ "success_data": "success" }))
            .unwrap_or_default();
        format!(
            r#"{{"status":200,"sign":"{}","data":{{"success_data":"success"}}}}"#,
            sign
        )
    }

    /// 创建 H5 Hosted Cashier 订单
    /// merchant_order_id: 商户订单号
    /// amount: 系统法币金额，内部会根据汇率折算为加密货币金额
    /// currency: 法币币种 (如 "usd", "cny")
    /// return_url: 前端返回跳转地址（优先级低于 success_url/fail_url）
    /// lang: 语言 ("zh" 或 "en")
    pub async fn create_h5_order(
        &self,
        merchant_order_id: &str,
        amount: f64,
        currency: &str,
        return_url: &str,
        lang: &str,
    ) -> Result<CreateH5OrderData> {
        let now = chrono::Utc::now().timestamp();
        // 计算换算后的加密货币金额
        let crypto_amount = format!("{:.2}", amount / self.settings.crypto_exchange_rate);

        let mut params = BTreeMap::new();
        // 公共参数
        params.insert(
            "app_id".to_string(),
            Value::String(self.settings.app_id.clone()),
        );
        params.insert("version".to_string(), Value::String("1.0".to_string()));
        params.insert("time".to_string(), Value::String(now.to_string()));
        // 业务参数（回调地址在 CipherBC 商户后台配置，不通过 API 传递）
        params.insert(
            "merchant_order_id".to_string(),
            Value::String(merchant_order_id.to_string()),
        );
        params.insert("amount".to_string(), Value::String(crypto_amount));
        params.insert("currency".to_string(), Value::String(currency.to_string()));
        params.insert(
            "return_url".to_string(),
            Value::String(return_url.to_string()),
        );
        params.insert("lang".to_string(), Value::String(lang.to_string()));

        let params_val =
            Value::Object(params.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        let sign = self.rsa_sign(&params_val)?;

        // 请求 JSON
        let mut request_body = params;
        request_body.insert("sign".to_string(), Value::String(sign));

        let api_url = format!(
            "{}/h5_order/create",
            self.settings.api_url.trim_end_matches('/')
        );

        tracing::info!(
            "[HyperBC] create_h5_order 请求: url={}, app_id={}, merchant_order_id={}, amount={}",
            api_url,
            self.settings.app_id,
            merchant_order_id,
            amount
        );

        let resp = self
            .http
            .post(&api_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow!("HyperBC 请求失败: {}", e))?;

        let status = resp.status();
        let resp_body = resp.text().await.unwrap_or_default();

        tracing::info!("[HyperBC] create_h5_order 响应: HTTP {}", status);

        if !status.is_success() {
            return Err(anyhow!("HyperBC HTTP {}: {}", status, resp_body));
        }

        let result: HyperbcResponse<CreateH5OrderData> = serde_json::from_str(&resp_body)
            .map_err(|e| anyhow!("HyperBC JSON 解析失败: {} body={}", e, resp_body))?;

        if result.status != 200 {
            return Err(anyhow!(
                "HyperBC create_h5_order 失败: status={}, msg={:?}",
                result.status,
                result.msg
            ));
        }

        let data = result
            .data
            .ok_or_else(|| anyhow!("HyperBC 返回的 data 为空"))?;

        Ok(data)
    }

    /// 查询订单详情
    #[allow(dead_code)]
    pub async fn query_order(&self, order_no: &str) -> Result<QueryOrderData> {
        let now = chrono::Utc::now().timestamp();
        let mut params = BTreeMap::new();
        params.insert(
            "app_id".to_string(),
            Value::String(self.settings.app_id.clone()),
        );
        params.insert("version".to_string(), Value::String("1.0".to_string()));
        params.insert(
            "time".to_string(),
            Value::Number(serde_json::Number::from(now)),
        );
        params.insert("order_no".to_string(), Value::String(order_no.to_string()));

        let params_val =
            Value::Object(params.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        let sign = self.rsa_sign(&params_val)?;

        let mut request_body = params;
        request_body.insert("sign".to_string(), Value::String(sign));

        let api_url = format!(
            "{}/h5_order/detail",
            self.settings.api_url.trim_end_matches('/')
        );

        let resp = self
            .http
            .post(&api_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow!("HyperBC 查询订单请求失败: {}", e))?;

        let status = resp.status();
        let resp_body = resp.text().await.unwrap_or_default();
        tracing::warn!(
            "[HyperBC查单] 订单 {} 响应报文: HTTP {}, body: {}",
            order_no,
            status,
            resp_body
        );

        if !status.is_success() {
            return Err(anyhow!("HyperBC HTTP {}: {}", status, resp_body));
        }

        let result: HyperbcResponse<QueryOrderData> = serde_json::from_str(&resp_body)
            .map_err(|e| anyhow!("HyperBC JSON 解析失败: {} body={}", e, resp_body))?;

        if result.status != 200 {
            return Err(anyhow!(
                "HyperBC query_order 失败: status={}, msg={:?}",
                result.status,
                result.msg
            ));
        }

        let sign = result
            .sign
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("HyperBC query_order 响应缺少 sign 签名"))?;
        let sign_content = Self::get_sign_content_from_raw(&resp_body)?;
        if !self.verify_signature_str(&sign_content, sign)? {
            return Err(anyhow!("HyperBC query_order 响应 RSA 签名校验失败"));
        }

        let data = result
            .data
            .ok_or_else(|| anyhow!("HyperBC 返回的订单 data 为空"))?;

        Ok(data)
    }
}
