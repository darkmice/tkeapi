/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#![allow(dead_code)]
use anyhow::{anyhow, Result};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;

type HmacSha256 = Hmac<Sha256>;

/// DeleteAsset 全局串行间隔（批量删 / 清空 / 定时清理 / 上游中继共用）。
pub const DELETE_ASSET_GAP_MS: u64 = 1000;
/// 遇限流后的退避（不持锁），再重试当前条。
pub const DELETE_ASSET_RATE_LIMIT_BACKOFF_MS: u64 = 60_000;
/// 单条 DeleteAsset 限流最大重试次数，耗尽则跳过，避免清空任务永久卡住。
pub const DELETE_ASSET_RATE_LIMIT_MAX_RETRIES: u32 = 5;

static DELETE_ASSET_PACE: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// 全局锁内执行一次 DeleteAsset。
/// - 成功/非限流：持锁 sleep 间隔，保证约 1s/条
/// - 限流：先放锁再退避，避免一条 429 卡住全站删除
/// 返回 `true` 表示限流（调用方应重试）。
pub async fn run_delete_asset_paced<F, Fut>(op: F) -> bool
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), bool>>,
{
    let result = {
        let _pace = DELETE_ASSET_PACE.lock().await;
        let r = op().await;
        if matches!(&r, Ok(()) | Err(false)) {
            tokio::time::sleep(std::time::Duration::from_millis(DELETE_ASSET_GAP_MS)).await;
        }
        r
    };
    match result {
        Ok(()) | Err(false) => false,
        Err(true) => {
            tokio::time::sleep(std::time::Duration::from_millis(
                DELETE_ASSET_RATE_LIMIT_BACKOFF_MS,
            ))
            .await;
            true
        }
    }
}

/// 对单条 DeleteAsset 做 paced + 限流重试；耗尽则跳过（不永久卡住）。
pub async fn delete_asset_paced_with_retries<F, Fut>(mut op: F, log_tag: &str, asset_id: &str)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), bool>>,
{
    let mut hits = 0u32;
    loop {
        if !run_delete_asset_paced(|| op()).await {
            return;
        }
        hits += 1;
        if hits >= DELETE_ASSET_RATE_LIMIT_MAX_RETRIES {
            tracing::warn!(
                "[{}] DeleteAsset 限流重试耗尽({}次)，跳过: {}",
                log_tag,
                DELETE_ASSET_RATE_LIMIT_MAX_RETRIES,
                asset_id
            );
            return;
        }
    }
}

/// HMAC-SHA256 签名（公共工具函数，供火山引擎所有服务复用）
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// SHA256 十六进制哈希（公共工具函数）
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// 火山引擎 HMAC-SHA256 签名（Signature V4）— 统一公共函数
/// 供 VolcClient、volcengine_billing、即梦 CV 等所有火山引擎服务复用
/// 返回 (authorization_header, x_date, payload_sha256_hex)
pub fn volcengine_sign(
    access_key: &str,
    secret_key: &str,
    method: &str,
    host: &str,
    path: &str,
    query: &str,
    service: &str,
    region: &str,
    payload: &[u8],
) -> (String, String, String) {
    let now = chrono::Utc::now();
    let x_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_short = now.format("%Y%m%d").to_string();

    let hashed_payload = sha256_hex(payload);

    // 1. Canonical Request
    let canonical_headers = format!(
        "content-type:application/json\nhost:{}\nx-content-sha256:{}\nx-date:{}\n",
        host, hashed_payload, x_date
    );
    let signed_headers = "content-type;host;x-content-sha256;x-date";

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, path, query, canonical_headers, signed_headers, hashed_payload
    );
    let hashed_canonical_request = sha256_hex(canonical_request.as_bytes());

    // 2. String to Sign
    let credential_scope = format!("{}/{}/{}/request", date_short, region, service);
    let string_to_sign = format!(
        "HMAC-SHA256\n{}\n{}\n{}",
        x_date, credential_scope, hashed_canonical_request
    );

    // 3. Signing Key — 使用原始 SecretKey（不加前缀），与火山引擎官方 SDK 一致
    let k_date = hmac_sha256(secret_key.as_bytes(), date_short.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"request");

    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    // 4. Authorization Header
    let authorization = format!(
        "HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        access_key, credential_scope, signed_headers, signature
    );

    (authorization, x_date, hashed_payload)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolcConfig {
    pub access_key: String,
    pub secret_key: String,
    pub app_id: String,
    pub project_name: String,
    pub group_id: Option<String>,
    pub region: String, // cn-beijing (国内版) 或 ap-southeast-1 (国际版)
}

impl VolcConfig {
    pub fn from_map(map: &HashMap<String, String>) -> Option<Self> {
        let ak = map.get("volc_access_key")?.trim();
        let sk = map.get("volc_secret_key")?.trim();
        let app_id = map
            .get("volc_app_id")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let project_name = map
            .get("volc_project_name")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "default".to_string());
        let project_name = if project_name.is_empty() {
            "default".to_string()
        } else {
            project_name
        };

        let group_id = map
            .get("volc_group_id")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let region = map
            .get("volc_region")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "cn-beijing".to_string());
        let region = if region.is_empty() {
            "cn-beijing".to_string()
        } else {
            region
        };

        if ak.is_empty() || sk.is_empty() {
            return None;
        }

        Some(Self {
            access_key: ak.to_string(),
            secret_key: sk.to_string(),
            app_id,
            project_name,
            group_id,
            region,
        })
    }
}

#[derive(Clone)]
pub struct VolcClient {
    config: VolcConfig,
    client: Client,
    logger: Option<(crate::db::Database, String, String, String)>, // (db, user_id, source, plugin_name)
}

impl VolcClient {
    pub fn new(config: VolcConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            logger: None,
        }
    }

    pub fn with_logger(mut self, db: crate::db::Database, user_id: String) -> Self {
        self.logger = Some((db, user_id, "page".to_string(), "asset_manager".to_string()));
        self
    }

    /// 设置日志来源标识：api_proxy / page / relay_convert
    pub fn with_source(mut self, source: &str) -> Self {
        if let Some(ref mut l) = self.logger {
            l.2 = source.to_string();
        }
        self
    }

    /// 设置插件命名空间：决定 API 日志写入哪个插件（asset_manager / asset_manager_intl）
    pub fn with_plugin_name(mut self, plugin_name: &str) -> Self {
        if let Some(ref mut l) = self.logger {
            l.3 = plugin_name.to_string();
        }
        self
    }

    pub async fn call_api<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        service: &str,
        region: &str,
        action: &str,
        version: &str,
        body: T,
    ) -> Result<R> {
        // 根据 region 区分国内版与国际版（BytePlus）的 API 域名
        // 国内版: open.volcengineapi.com / {service}.volcengineapi.com
        // 国际版: open.byteplusapi.com / {service}.byteplusapi.com
        let is_international = region.starts_with("ap-");
        let base_domain = if is_international {
            "byteplusapi.com"
        } else {
            "volcengineapi.com"
        };

        let host = if service == "ark" {
            // ark 素材资产管理 API 统一使用 open.{domain}
            format!("open.{}", base_domain)
        } else {
            format!("{}.{}", service, base_domain)
        };
        let url = format!("https://{}/?Action={}&Version={}", host, action, version);
        let path = "/";
        let query = format!("Action={}&Version={}", action, version);
        let payload = serde_json::to_vec(&body)?;

        let (auth, x_date, payload_hash) = volcengine_sign(
            &self.config.access_key,
            &self.config.secret_key,
            "POST",
            &host,
            path,
            &query,
            service,
            region,
            &payload,
        );

        let res = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Date", &x_date)
            .header("X-Content-Sha256", &payload_hash)
            .header("Authorization", auth)
            .body(payload.clone())
            .send()
            .await?;

        let status = res.status();
        let text = res.text().await?;
        let status_code = status.as_u16();

        // 异步记录日志：不阻塞主流程，直接丢进数据库
        if let Some((db, user_id, source, plugin_name)) = &self.logger {
            let req_payload = String::from_utf8_lossy(&payload).into_owned();
            let res_payload = text.clone();
            let db_clone = db.clone();
            let uid_clone = user_id.clone();
            let action_clone = action.to_string();
            let source_clone = source.clone();
            let plugin_name_clone = plugin_name.clone();

            // 若 HTTP 成功但业务内容表明失败（如素材状态 Failed/审核未通过），日志状态码修正为 422
            let log_status = if status.is_success()
                && (res_payload.contains("\"Failed\"") || res_payload.contains("\"failed\""))
            {
                422
            } else {
                status_code as i32
            };

            tokio::spawn(async move {
                let _ = sqlx::query(&db_clone.format_query(
                    "INSERT INTO plugin_api_logs (user_id, plugin_name, api_endpoint, request_payload, response_payload, status_code, source) 
                     VALUES (?, ?, ?, ?, ?, ?, ?)"
                ))
                .bind(&uid_clone)
                .bind(&plugin_name_clone)
                .bind(&action_clone)
                .bind(&req_payload)
                .bind(&res_payload)
                .bind(log_status)
                .bind(&source_clone)
                .execute(&db_clone.pool)
                .await;
            });
        }

        if !status.is_success() {
            return Err(anyhow!("Volcengine API error: {} - {}", status, text));
        }

        // 火山引擎 API 响应有两种结构：
        // 1. visual 等服务：Response struct 自己定义了 ResponseMetadata + Result → 直接反序列化
        // 2. ark 素材 API：Response struct 只定义了 Result 内部字段 → 需要提取 Result 再反序列化
        let raw: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
            anyhow!(
                "Failed to parse Volcengine response JSON: {} - Raw: {}",
                e,
                text
            )
        })?;

        // 若返回体中带有 Result 字段，优先尝试从 Result 内部提取并反序列化（防止目标 struct 包含 default 字段误把最外层信封反序列化为空默认值）
        if let Some(result_val) = raw.get("Result") {
            if let Ok(parsed) = serde_json::from_value::<R>(result_val.clone()) {
                return Ok(parsed);
            }
        }

        // 兜底尝试直接反序列化整个响应（兼容自带 ResponseMetadata 包装的视觉服务结构体）
        if let Ok(parsed) = serde_json::from_value::<R>(raw.clone()) {
            return Ok(parsed);
        }

        // 若上述均未命中且有 Result，抛出 Result 解析错误以便详细排查
        if let Some(result_val) = raw.get("Result") {
            return serde_json::from_value(result_val.clone())
                .map_err(|e| anyhow!("Failed to parse Volcengine Result: {} - Raw: {}", e, text));
        }

        Err(anyhow!(
            "Volcengine response missing 'Result' field - Raw: {}",
            text
        ))
    }
}

// --- Specific API Request/Response Structs ---

#[derive(Serialize)]
pub struct CreateVisualValidateSessionRequest {
    #[serde(rename = "AppId")]
    pub app_id: String,
    #[serde(rename = "CallbackUrl")]
    pub callback_url: String,
    #[serde(rename = "TokenValidTime")]
    pub token_valid_time: i32,
}

#[derive(Deserialize, Debug)]
pub struct CreateVisualValidateSessionResponse {
    #[serde(rename = "ResponseMetadata")]
    pub metadata: ResponseMetadata,
    #[serde(rename = "Result")]
    pub result: Option<SessionResult>,
}

#[derive(Deserialize, Debug)]
pub struct ResponseMetadata {
    #[serde(rename = "RequestId")]
    pub request_id: String,
    #[serde(rename = "Action")]
    pub action: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Service")]
    pub service: String,
    #[serde(rename = "Region")]
    pub region: String,
    #[serde(rename = "Error")]
    pub error: Option<ResponseError>,
}

#[derive(Deserialize, Debug)]
pub struct ResponseError {
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct SessionResult {
    #[serde(rename = "BytedToken")]
    pub byted_token: String,
    #[serde(rename = "H5Link")]
    pub h5_link: String,
}

#[derive(Serialize)]
pub struct GetVisualValidateResultRequest {
    #[serde(rename = "AppId")]
    pub app_id: String,
    #[serde(rename = "BytedToken")]
    pub byted_token: String,
}

#[derive(Deserialize, Debug)]
pub struct GetVisualValidateResultResponse {
    #[serde(rename = "ResponseMetadata")]
    pub metadata: ResponseMetadata,
    #[serde(rename = "Result")]
    pub result: Option<ValidationResult>,
}

#[derive(Deserialize, Debug)]
pub struct ValidationResult {
    #[serde(rename = "Status")]
    pub status: i32, // 0-Success, 1-Failed
    #[serde(rename = "AssetGroupId")]
    pub asset_group_id: Option<String>,
}

// --- 方舟 Ark 素材资产 API (ServiceName=ark, Version=2024-01-01) ---

/// CreateAssetGroup 创建素材资产组合
#[derive(Serialize)]
pub struct CreateAssetGroupRequest {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Description")]
    pub description: String,
    #[serde(rename = "GroupType", skip_serializing_if = "Option::is_none")]
    pub group_type: Option<String>, // 默认 AIGC
    #[serde(rename = "ProjectName", skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>, // 默认 default
}

#[derive(Deserialize, Debug)]
pub struct CreateAssetGroupResponse {
    #[serde(rename = "Id")]
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AssetModerationConfig {
    #[serde(rename = "Strategy")]
    pub strategy: String,
}

/// 方舟 Create/Update Asset(Group) 的 Name 上限（文档：64 个字符）
const ARK_NAME_MAX_CHARS: usize = 64;

/// 组合名等：按字符截断到上限。
pub fn clamp_ark_name(name: &str) -> String {
    clamp_name(name, false)
}

/// 素材名：截断到上限，尽量保留扩展名（便于 ListAssets 模糊搜索）。
pub fn clamp_create_asset_name(name: &str) -> String {
    clamp_name(name, true)
}

fn clamp_name(name: &str, keep_ext: bool) -> String {
    if name.chars().count() <= ARK_NAME_MAX_CHARS {
        return name.to_string();
    }
    if keep_ext {
        if let Some(i) = name.rfind('.') {
            let ext = &name[i..];
            let ext_len = ext.chars().count();
            if i > 0 && ext_len <= 16 && !name[i + 1..].contains('/') {
                let mut out: String = name[..i]
                    .chars()
                    .take(ARK_NAME_MAX_CHARS - ext_len)
                    .collect();
                out.push_str(ext);
                return out;
            }
        }
    }
    name.chars().take(ARK_NAME_MAX_CHARS).collect()
}

/// CreateAsset 创建素材资产（上传到方舟素材库）
#[derive(Serialize)]
pub struct CreateAssetRequest {
    #[serde(rename = "GroupId")]
    pub group_id: String,
    #[serde(rename = "URL")]
    pub url: String, // 公网可访问的图片 URL
    #[serde(rename = "AssetType")]
    pub asset_type: String, // "Image" / "Video" / "Audio"
    #[serde(rename = "Name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "ProjectName", skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(rename = "Moderation", skip_serializing_if = "Option::is_none")]
    pub moderation: Option<AssetModerationConfig>,
}

#[derive(Deserialize, Debug)]
pub struct CreateAssetResponse {
    #[serde(rename = "Id")]
    pub id: String, // asset-20260318071009-*****
}

/// GetAsset 查询素材资产信息
#[derive(Serialize)]
pub struct GetAssetRequest {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "ProjectName", skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetAssetResponse {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Status")]
    pub status: String, // "Active" / "Processing" / "Failed"
    #[serde(rename = "FailCode", default)]
    pub fail_code: Option<String>,
    #[serde(rename = "FailReason", default)]
    pub fail_reason: Option<String>,
    #[serde(rename = "Error", default)]
    pub error: Option<GetAssetResponseError>,
    #[serde(rename = "GroupId")]
    pub group_id: Option<String>,
    #[serde(rename = "AssetType")]
    pub asset_type: Option<String>,
    #[serde(rename = "URL")]
    pub url: Option<String>,
    #[serde(rename = "CreateTime")]
    pub create_time: Option<String>,
    #[serde(rename = "ProjectName")]
    pub project_name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetAssetResponseError {
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
}

/// DeleteAsset 删除素材资产
#[derive(Serialize)]
pub struct DeleteAssetRequest {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "ProjectName", skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct DeleteAssetResponse {}

/// 规范化方舟素材 ID：先 trim，再去掉 `asset://` 前缀；空串表示无效。
pub fn normalize_ark_asset_id(id: &str) -> &str {
    let id = id.trim();
    id.strip_prefix("asset://").unwrap_or(id).trim()
}

impl VolcClient {
    /// 判断方舟/开放平台错误是否为限流（明确 HTTP/业务码，不用裸 `429` 子串）。
    pub fn is_api_rate_limited(err: &str) -> bool {
        let m = err.to_ascii_lowercase();
        // 本仓库包装：`Volcengine API error: 429 …` / `上游素材接口错误: 429`
        m.contains("api error: 429")
            || m.contains("接口错误: 429")
            || m.contains("accountflowlimitexceeded")
            || m.contains("too many requests")
            || m.contains("flow control")
            || m.contains("flowcontrol")
            || m.contains("rate limit")
            || m.contains("ratelimit")
            || m.contains("request limit")
    }

    /// 调用方舟 DeleteAsset：`Ok` 成功，`Err(true)` 限流可重试，`Err(false)` 其它失败。
    pub async fn delete_asset_logged(&self, asset_id: &str, log_tag: &str) -> Result<(), bool> {
        let id = normalize_ark_asset_id(asset_id);
        if id.is_empty() {
            return Err(false);
        }
        let req = DeleteAssetRequest {
            id: id.to_string(),
            project_name: Some(self.config.project_name.clone()),
        };
        match self
            .call_api::<_, DeleteAssetResponse>(
                "ark",
                &self.config.region,
                "DeleteAsset",
                "2024-01-01",
                req,
            )
            .await
        {
            Ok(_) => {
                tracing::info!("[{}] DeleteAsset 成功: {}", log_tag, id);
                Ok(())
            }
            Err(e) => {
                let msg = e.to_string();
                if Self::is_api_rate_limited(&msg) {
                    tracing::info!("[{}] DeleteAsset 限流: {} - {}", log_tag, id, msg);
                    Err(true)
                } else {
                    tracing::info!(
                        "[{}] DeleteAsset 失败(不影响本地删除): {} - {}",
                        log_tag,
                        id,
                        msg
                    );
                    Err(false)
                }
            }
        }
    }

    /// 分页查询方舟云端素材资产列表
    pub async fn list_assets(&self, req: ListAssetsRequest) -> Result<ListAssetsResponse, String> {
        self.call_api::<_, ListAssetsResponse>(
            "ark",
            &self.config.region,
            "ListAssets",
            "2024-01-01",
            req,
        )
        .await
        .map_err(|e| e.to_string())
    }

    /// 分页查询方舟素材资产组列表
    pub async fn list_asset_groups(
        &self,
        req: ListAssetGroupsRequest,
    ) -> Result<ListAssetGroupsResponse, String> {
        self.call_api::<_, ListAssetGroupsResponse>(
            "ark",
            &self.config.region,
            "ListAssetGroups",
            "2024-01-01",
            req,
        )
        .await
        .map_err(|e| e.to_string())
    }
}

/// ListAssets 筛选过滤
#[derive(Serialize, Debug, Clone)]
pub struct ListAssetsFilter {
    /// 资产组类别（火山 OpenAPI 必填，默认 "AIGC"）
    #[serde(rename = "GroupType")]
    pub group_type: String,
    #[serde(rename = "Statuses", skip_serializing_if = "Option::is_none")]
    pub statuses: Option<Vec<String>>,
    #[serde(rename = "AssetTypes", skip_serializing_if = "Option::is_none")]
    pub asset_types: Option<Vec<String>>,
    #[serde(rename = "GroupIds", skip_serializing_if = "Option::is_none")]
    pub group_ids: Option<Vec<String>>,
    #[serde(rename = "Name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Default for ListAssetsFilter {
    fn default() -> Self {
        Self {
            group_type: "AIGC".to_string(),
            statuses: None,
            asset_types: None,
            group_ids: None,
            name: None,
        }
    }
}

/// ListAssets 分页查询素材列表
#[derive(Serialize, Debug, Clone)]
pub struct ListAssetsRequest {
    #[serde(rename = "PageNumber")]
    pub page_number: i32,
    #[serde(rename = "PageSize")]
    pub page_size: i32,
    #[serde(rename = "ProjectName", skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(rename = "Filter")]
    pub filter: ListAssetsFilter,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct ArkAssetItem {
    #[serde(alias = "Id", rename = "id")]
    pub id: String,
    #[serde(alias = "Name", rename = "name", default)]
    pub name: Option<String>,
    #[serde(alias = "AssetType", rename = "asset_type", default)]
    pub asset_type: Option<String>,
    #[serde(alias = "Status", rename = "status", default)]
    pub status: Option<String>,
    #[serde(alias = "URL", rename = "url", default)]
    pub url: Option<String>,
    #[serde(alias = "GroupId", rename = "group_id", default)]
    pub group_id: Option<String>,
    #[serde(alias = "CreateTime", rename = "create_time", default)]
    pub create_time: Option<String>,
    #[serde(alias = "UpdateTime", rename = "update_time", default)]
    pub update_time: Option<String>,
    #[serde(alias = "FailReason", rename = "fail_reason", default)]
    pub fail_reason: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ListAssetsResponse {
    #[serde(alias = "Items")]
    pub items: Vec<ArkAssetItem>,
    #[serde(alias = "TotalCount")]
    pub total_count: i64,
    #[serde(alias = "PageNumber", default)]
    pub page_number: Option<i32>,
    #[serde(alias = "PageSize", default)]
    pub page_size: Option<i32>,
}

/// ListAssetGroups 过滤
#[derive(Serialize, Debug, Clone, Default)]
pub struct ListAssetGroupsFilter {
    #[serde(rename = "GroupType", skip_serializing_if = "Option::is_none")]
    pub group_type: Option<String>,
    #[serde(rename = "Name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// ListAssetGroups 分页查询素材组列表
#[derive(Serialize, Debug, Clone)]
pub struct ListAssetGroupsRequest {
    #[serde(rename = "PageNumber")]
    pub page_number: i32,
    #[serde(rename = "PageSize")]
    pub page_size: i32,
    #[serde(rename = "ProjectName", skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
    pub filter: Option<ListAssetGroupsFilter>,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct ArkAssetGroupItem {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name", default)]
    pub name: Option<String>,
    #[serde(rename = "Description", default)]
    pub description: Option<String>,
    #[serde(rename = "GroupType", default)]
    pub group_type: Option<String>,
    #[serde(rename = "CreateTime", default)]
    pub create_time: Option<String>,
    #[serde(rename = "UpdateTime", default)]
    pub update_time: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ListAssetGroupsResponse {
    #[serde(rename = "Items", default)]
    pub items: Vec<ArkAssetGroupItem>,
    #[serde(rename = "TotalCount", default)]
    pub total_count: i64,
    #[serde(rename = "PageNumber", default)]
    pub page_number: Option<i32>,
    #[serde(rename = "PageSize", default)]
    pub page_size: Option<i32>,
}

/// UpdateAssetGroup 更新素材资产组合
#[derive(Serialize)]
pub struct UpdateAssetGroupRequest {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "Description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "ProjectName", skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct UpdateAssetGroupResponse {
    #[serde(rename = "Id")]
    pub id: String,
}

/// DeleteAssetGroup 删除素材资产组合
#[derive(Serialize)]
pub struct DeleteAssetGroupRequest {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "ProjectName", skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct DeleteAssetGroupResponse {}

/// GetModerationResult 审核拦截原因查询（ServiceName=ark, Version=2024-01-01）
#[derive(Serialize)]
pub struct GetModerationResultRequest {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Type")]
    pub id_type: String, // asset_id / task_id / request_id
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ModerationBlockReason {
    // 文档示例为 snake_case；Ark 实际响应常见 PascalCase，双兼容
    #[serde(default, alias = "Label")]
    pub label: Option<String>,
    #[serde(default, alias = "SubLabel")]
    pub sub_label: Option<String>,
    #[serde(default, alias = "Detail")]
    pub detail: Option<String>,
}

/// 注意：block_reasons 不可加 `default`。
/// `VolcClient::call_api` 会先尝试整包反序列化；若字段可缺省，会误把
/// `{ResponseMetadata, Result:{...}}` 解析成空列表，永远读不到 Result。
#[derive(Deserialize, Debug)]
pub struct GetModerationResultResponse {
    #[serde(alias = "BlockReasons", rename = "block_reasons")]
    pub block_reasons: Vec<ModerationBlockReason>,
}
