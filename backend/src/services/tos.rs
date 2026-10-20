/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use base64::Engine;
use hmac::{Hmac, Mac};
use md5::Md5;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

fn tos_http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

type HmacSha256 = Hmac<Sha256>;

/// TOS4 空 body SHA256；Authorization 签名恒用此值（对齐 SDK sign_header，非预签名）
const EMPTY_PAYLOAD_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn hmac_sign(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key error");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// TOS4 派生签名（Authorization / 预签名共用）
fn tos4_signature(
    secret_key: &str,
    date_short: &str,
    region: &str,
    string_to_sign: &str,
) -> String {
    let k_date = hmac_sign(secret_key.as_bytes(), date_short.as_bytes());
    let k_region = hmac_sign(&k_date, region.as_bytes());
    let k_service = hmac_sign(&k_region, b"tos");
    let k_signing = hmac_sign(&k_service, b"request");
    hex::encode(hmac_sign(&k_signing, string_to_sign.as_bytes()))
}

/// 去掉 http(s):// 与尾部 `/`，得到 host 或裸域名
fn endpoint_host(endpoint: &str) -> &str {
    let ep = endpoint.trim().trim_end_matches('/');
    ep.strip_prefix("https://")
        .or_else(|| ep.strip_prefix("http://"))
        .unwrap_or(ep)
}

/// TOS 存储配置
#[derive(Debug, Clone)]
pub struct TosConfig {
    pub access_key: String,
    pub secret_key: String,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub path_prefix: String,
    pub custom_domain: String,
}

impl TosConfig {
    pub fn from_map(map: &HashMap<String, String>) -> Option<Self> {
        let ak = map.get("tos_access_key")?.trim().to_string();
        let sk = map.get("tos_secret_key")?.trim().to_string();
        let endpoint = map.get("tos_endpoint")?.trim().to_string();
        let region = map.get("tos_region")?.trim().to_string();
        let bucket = map.get("tos_bucket")?.trim().to_string();

        if ak.is_empty()
            || sk.is_empty()
            || endpoint.is_empty()
            || region.is_empty()
            || bucket.is_empty()
        {
            return None;
        }

        Some(Self {
            access_key: ak,
            secret_key: sk,
            endpoint,
            region,
            bucket,
            path_prefix: map
                .get("tos_path_prefix")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
            custom_domain: map
                .get("tos_custom_domain")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
        })
    }

    /// 生成文件的公开访问 URL（可走自定义域名 / CDN）
    pub fn file_url(&self, object_key: &str) -> String {
        let key = object_key.trim_start_matches('/');
        if !self.custom_domain.is_empty() {
            let raw = self.custom_domain.trim().trim_end_matches('/');
            let (scheme, host) = if let Some(h) = raw.strip_prefix("https://") {
                ("https", h.trim_end_matches('/'))
            } else if let Some(h) = raw.strip_prefix("http://") {
                ("http", h.trim_end_matches('/'))
            } else {
                ("https", raw)
            };
            return format!("{}://{}/{}", scheme, host, key);
        }
        self.official_request_target(key).2
    }

    /// Virtual-Hosted / Path-Style（桶名含 `.` 时走 Path-Style，避免证书不匹配）
    /// `object_key` 为空时为桶级路径（ListBuckets 对象列表等）
    fn request_host_path(&self, object_key: &str) -> (String, String) {
        let ep = endpoint_host(&self.endpoint);
        if self.bucket.contains('.') {
            let path = if object_key.is_empty() {
                format!("/{}", self.bucket)
            } else {
                format!("/{}/{}", self.bucket, object_key)
            };
            (ep.to_string(), path)
        } else if object_key.is_empty() {
            (format!("{}.{}", self.bucket, ep), "/".to_string())
        } else {
            (
                format!("{}.{}", self.bucket, ep),
                format!("/{}", object_key),
            )
        }
    }

    /// 直传专用：官方 endpoint 的 `(host, path, url)`，忽略自定义域名
    fn official_request_target(&self, key: &str) -> (String, String, String) {
        let (host, path) = self.request_host_path(key);
        let url = format!("https://{}{}", host, path);
        (host, path, url)
    }

    /// ListObjects 等桶级路径：`/` 或 `/{bucket}`
    fn bucket_request_target(&self) -> (String, String) {
        self.request_host_path("")
    }

    /// 生成完整的 object key（含路径前缀）
    pub fn full_key(&self, filename: &str) -> String {
        if self.path_prefix.is_empty() {
            filename.to_string()
        } else {
            let prefix = self.path_prefix.trim_end_matches('/');
            format!("{}/{}", prefix, filename)
        }
    }

    /// 从 file_url 反推 object key
    pub fn extract_object_key(&self, file_url: &str) -> Option<String> {
        let try_base = |base: &str| -> Option<String> {
            let prefix = format!("{}/", base.trim_end_matches('/'));
            file_url.strip_prefix(&prefix).map(|s| s.to_string())
        };

        if let Some(key) = try_base(&self.file_url("")) {
            return Some(key);
        }
        if !self.custom_domain.is_empty() {
            return try_base(&self.official_request_target("").2);
        }
        None
    }
}

/// 额外重试次数（总尝试 = 1 + N）；对齐旧 SDK 常用 max_retry_count(2)
const TOS_MAX_RETRY: u32 = 2;

#[inline]
fn tos_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 429) || status.is_server_error()
}

/// 构造 TOS URL：`query_pairs` 须已按 key 字母序；编码与签名共用同一 `Url`，避免 `/`↔`%2F` 不一致。
fn tos_request_url(
    host: &str,
    path: &str,
    query_pairs: &[(&str, &str)],
) -> Result<reqwest::Url, String> {
    let path = if path.is_empty() { "/" } else { path };
    let mut url = reqwest::Url::parse(&format!("https://{}{}", host, path))
        .map_err(|e| format!("TOS URL 无效: {}", e))?;
    if !query_pairs.is_empty() {
        let mut q = url.query_pairs_mut();
        for &(k, v) in query_pairs {
            q.append_pair(k, v);
        }
    }
    Ok(url)
}

/// TOS4 签名请求（list/put/delete/tagging 共用）。
/// SignedHeaders：host / content-type / x-tos-*；payload 恒 empty hash。
/// 网络错与 408/429/5xx 短退避重试，预签名不走此路径。
async fn signed_request(
    config: &TosConfig,
    method: reqwest::Method,
    host: &str,
    path: &str,
    query_pairs: &[(&str, &str)],
    extra_headers: &[(&str, &str)],
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    let url = tos_request_url(host, path, query_pairs)?;
    let mut attempt = 0u32;
    loop {
        if attempt > 0 {
            let ms = (100u64 << (attempt - 1)).min(1000);
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }
        match signed_request_once(
            config,
            method.clone(),
            url.clone(),
            extra_headers,
            body,
            timeout,
        )
        .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() || !tos_retryable_status(status) || attempt >= TOS_MAX_RETRY
                {
                    return Ok(resp);
                }
                let _ = resp.bytes().await;
            }
            Err(e) if attempt >= TOS_MAX_RETRY => return Err(e),
            Err(_) => {}
        }
        attempt += 1;
    }
}

async fn signed_request_once(
    config: &TosConfig,
    method: reqwest::Method,
    url: reqwest::Url,
    extra_headers: &[(&str, &str)],
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "TOS URL 缺少 host".to_string())?
        .to_string();
    let path = url.path().to_string();
    let query = url.query().unwrap_or("").to_string();

    let now = chrono::Utc::now();
    let date_str = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_short = now.format("%Y%m%d").to_string();

    let mut sign_hdrs: Vec<(String, String)> = vec![
        ("host".into(), host.clone()),
        ("x-tos-date".into(), date_str.clone()),
    ];
    for (k, v) in extra_headers {
        let lk = k.to_ascii_lowercase();
        if lk == "content-type" || lk.starts_with("x-tos-") {
            sign_hdrs.push((lk, (*v).to_string()));
        }
    }
    sign_hdrs.sort_by(|a, b| a.0.cmp(&b.0));

    let signed_headers = sign_hdrs
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let mut canonical_headers = String::new();
    for (k, v) in &sign_hdrs {
        canonical_headers.push_str(k);
        canonical_headers.push(':');
        canonical_headers.push_str(v.trim());
        canonical_headers.push('\n');
    }

    let canonical_request = format!(
        "{method}\n{path}\n{query}\n{canonical_headers}\n{signed_headers}\n{payload}",
        method = method.as_str(),
        payload = EMPTY_PAYLOAD_HASH,
    );
    let credential_scope = format!("{}/{}/tos/request", date_short, config.region);
    let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
    let string_to_sign = format!(
        "TOS4-HMAC-SHA256\n{}\n{}\n{}",
        date_str, credential_scope, canonical_hash
    );
    let signature = tos4_signature(
        &config.secret_key,
        &date_short,
        &config.region,
        &string_to_sign,
    );

    let auth = format!(
        "TOS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
        config.access_key, credential_scope, signed_headers, signature
    );

    let mut builder = tos_http()
        .request(method, url)
        .header("Host", &host)
        .header("x-tos-date", &date_str)
        .header("Authorization", &auth)
        .timeout(timeout);
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    if let Some(b) = body {
        builder = builder.body(b.to_vec());
    }

    builder
        .send()
        .await
        .map_err(|e| format!("TOS 请求失败: {}", e))
}

/// 测试当前配置：写入一个小文件再删除，确认存储可正常使用
pub async fn test_connection(config: &TosConfig) -> Result<String, String> {
    let key = config.full_key(&format!(
        "_connection_test/{}.txt",
        chrono::Utc::now().timestamp_millis()
    ));
    upload_file(
        config,
        &key,
        b"tokensbyte storage connection test".to_vec(),
        "text/plain",
        None,
    )
    .await
    .map_err(|e| format!("写入测试文件失败: {e}"))?;
    if let Err(e) = delete_file(config, &key).await {
        return Err(format!("测试文件已写入 {key}，但删除失败: {e}"));
    }
    Ok(format!("读写正常，已写入并删除测试文件 {key}"))
}

/// ListBuckets 返回的桶。地域与内外网域名用来自动填存储配置。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TosListedBucket {
    pub name: String,
    pub location: String,
    pub extranet_endpoint: String,
    pub intranet_endpoint: String,
}

/// 用 AK/SK 列举账号下的桶。未指定接入点时先走火山引擎公网，失败再试 BytePlus。
pub async fn list_buckets(
    access_key: &str,
    secret_key: &str,
    prefer_endpoint: &str,
) -> Result<Vec<TosListedBucket>, String> {
    let targets: [(&str, &str); 2] = if prefer_endpoint.contains("bytepluses.com") {
        [
            ("ap-southeast-1", "tos-ap-southeast-1.bytepluses.com"),
            ("cn-beijing", "tos-cn-beijing.volces.com"),
        ]
    } else {
        [
            ("cn-beijing", "tos-cn-beijing.volces.com"),
            ("ap-southeast-1", "tos-ap-southeast-1.bytepluses.com"),
        ]
    };
    let mut last_err = String::new();
    for (region, host) in targets {
        let config = TosConfig {
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
            endpoint: host.to_string(),
            region: region.to_string(),
            bucket: "list".to_string(),
            path_prefix: String::new(),
            custom_domain: String::new(),
        };
        match list_buckets_at(&config, host).await {
            Ok(buckets) => return Ok(buckets),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

async fn list_buckets_at(config: &TosConfig, host: &str) -> Result<Vec<TosListedBucket>, String> {
    let resp = signed_request(
        config,
        reqwest::Method::GET,
        host,
        "/",
        &[],
        &[],
        None,
        Duration::from_secs(15),
    )
    .await?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取存储桶列表失败: {}", e))?;
    if !status.is_success() {
        let snippet: String = body.chars().take(300).collect();
        return Err(format!("列举存储桶失败 ({}): {}", status, snippet));
    }
    Ok(parse_buckets(&body))
}

/// ListBuckets：文档示例为 JSON；REST 也可能是 XML。
fn parse_buckets(body: &str) -> Vec<TosListedBucket> {
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(arr) = v.get("Buckets").and_then(|b| b.as_array()) {
                return arr.iter().filter_map(bucket_from_json).collect();
            }
        }
    }
    let mut buckets = Vec::new();
    for block in body.split("<Bucket>").skip(1) {
        let Some(name) = extract_xml_value(block, "Name").filter(|s| !s.is_empty()) else {
            continue;
        };
        buckets.push(TosListedBucket {
            name,
            location: extract_xml_value(block, "Location")
                .or_else(|| extract_xml_value(block, "LocationConstraint"))
                .unwrap_or_default(),
            extranet_endpoint: extract_xml_value(block, "ExtranetEndpoint").unwrap_or_default(),
            intranet_endpoint: extract_xml_value(block, "IntranetEndpoint").unwrap_or_default(),
        });
    }
    buckets
}

fn bucket_from_json(v: &serde_json::Value) -> Option<TosListedBucket> {
    let name = v.get("Name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(TosListedBucket {
        name: name.to_string(),
        location: v
            .get("Location")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        extranet_endpoint: v
            .get("ExtranetEndpoint")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        intranet_endpoint: v
            .get("IntranetEndpoint")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// 上传文件到 TOS（显式设置 x-tos-acl: default 继承桶 ACL）
pub async fn upload_file(
    config: &TosConfig,
    object_key: &str,
    data: Vec<u8>,
    content_type: &str,
    tags: Option<&str>,
) -> Result<String, String> {
    let key = object_key.trim_start_matches('/');
    let (host, path, _) = config.official_request_target(key);

    let mut extras: Vec<(String, String)> = vec![("x-tos-acl".into(), "default".into())];
    if !content_type.is_empty() {
        extras.push(("Content-Type".into(), content_type.to_string()));
    }
    // 对齐 SDK set_tagging → header x-tos-tagging
    if let Some(t) = tags.filter(|s| !s.is_empty()) {
        extras.push(("x-tos-tagging".into(), t.to_string()));
    }
    let extra_refs: Vec<(&str, &str)> = extras
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let resp = signed_request(
        config,
        reqwest::Method::PUT,
        &host,
        &path,
        &[],
        &extra_refs,
        Some(&data),
        Duration::from_secs(300),
    )
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("上传失败 ({}): {}", status, body));
    }
    Ok(config.file_url(object_key))
}

/// 删除 TOS 文件（对象不存在视为成功，便于幂等清理）
pub async fn delete_file(config: &TosConfig, object_key: &str) -> Result<(), String> {
    delete_file_inner(config, object_key).await.map(|_| ())
}

/// Ok(true)=已删除；Ok(false)=本来就不存在(404)
async fn delete_file_inner(config: &TosConfig, object_key: &str) -> Result<bool, String> {
    let key = object_key.trim_start_matches('/');
    let (host, path, _) = config.official_request_target(key);

    let resp = signed_request(
        config,
        reqwest::Method::DELETE,
        &host,
        &path,
        &[],
        &[],
        None,
        Duration::from_secs(10),
    )
    .await?;

    let status = resp.status();
    if status.is_success() {
        return Ok(true);
    }
    if status.as_u16() == 404 {
        return Ok(false);
    }
    let body = resp.text().await.unwrap_or_default();
    Err(format!("删除失败 ({}): {}", status, body))
}

/// `purge_prefix` 结果：`success` = list 成功且无删除失败（list=0 仍可 success）。
#[derive(Debug)]
struct PurgeReport {
    prefix: String,
    listed: Option<usize>,
    deleted_ok: usize,
    missing: usize,
    fail: usize,
    errs: Vec<String>,
}

impl PurgeReport {
    fn success(&self) -> bool {
        self.fail == 0 && self.listed.is_some()
    }
}

impl std::fmt::Display for PurgeReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "prefix={}", self.prefix)?;
        match self.listed {
            Some(n) => write!(f, " list={n}")?,
            None => write!(f, " list=ERR")?,
        }
        write!(
            f,
            " deleted_ok={} missing={} fail={}",
            self.deleted_ok, self.missing, self.fail
        )?;
        if !self.errs.is_empty() {
            write!(f, " errs={}", self.errs.join(" | "))?;
        }
        Ok(())
    }
}

fn purge_push_key(keys: &mut std::collections::HashSet<String>, key: &str) {
    let key = key.trim().trim_start_matches('/');
    if !key.is_empty() {
        keys.insert(key.to_string());
    }
}

/// 清理相对前缀：合并 `extra_keys` + list 残留 + `.keep`，再并行删除。
async fn purge_prefix(
    config: &TosConfig,
    relative_prefix: &str,
    extra_keys: Vec<String>,
) -> PurgeReport {
    let prefix = relative_prefix.trim().trim_matches('/').to_string();
    let mut report = PurgeReport {
        prefix: config.full_key(&format!("{prefix}/")),
        listed: None,
        deleted_ok: 0,
        missing: 0,
        fail: 0,
        errs: Vec::new(),
    };
    let mut keys = std::collections::HashSet::<String>::new();

    for key in &extra_keys {
        purge_push_key(&mut keys, key);
    }

    match list_folder(config, &format!("{prefix}/")).await {
        Ok((objects, _)) => {
            report.listed = Some(objects.len());
            for obj in &objects {
                purge_push_key(&mut keys, &obj.key);
            }
        }
        Err(e) => {
            report.fail += 1;
            report.errs.push(format!("list: {e}"));
        }
    }

    purge_push_key(&mut keys, &config.full_key(&format!("{prefix}/.keep")));

    let key_list: Vec<String> = keys.into_iter().collect();
    let results =
        futures::future::join_all(key_list.iter().map(|k| delete_file_inner(config, k))).await;
    for (key, res) in key_list.iter().zip(results) {
        match res {
            Ok(true) => report.deleted_ok += 1,
            Ok(false) => report.missing += 1,
            Err(e) => {
                report.fail += 1;
                if report.errs.len() < 5 {
                    report.errs.push(format!("{key}: {e}"));
                }
            }
        }
    }
    report
}

/// 后台清理并打真实成功/失败日志（创作中心项目/工作流共用）。
pub fn spawn_purge(
    config: TosConfig,
    relative_prefix: String,
    extra_keys: Vec<String>,
    label: &'static str,
    id: i64,
) {
    tokio::spawn(async move {
        let report = purge_prefix(&config, &relative_prefix, extra_keys).await;
        if report.success() {
            tracing::info!("{} {} TOS 清理成功 {}", label, id, report);
        } else {
            tracing::warn!("{} {} TOS 清理异常 {}", label, id, report);
        }
    });
}

/// 生成预签名 PUT URL（前端直传；仅签 host，走官方 endpoint）
pub fn generate_presigned_put_url(
    config: &TosConfig,
    object_key: &str,
    expires_secs: u64,
) -> String {
    let (host, path, base_url) = config.official_request_target(object_key.trim_start_matches('/'));

    let now = chrono::Utc::now();
    let date_str = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_short = now.format("%Y%m%d").to_string();

    let credential_scope = format!("{}/{}/tos/request", date_short, config.region);
    let credential_val = format!("{}/{}", config.access_key, credential_scope);
    let credential_encoded = urlencoding::encode(&credential_val).to_string();

    let signed_headers = "host";
    let query = format!(
        "X-Tos-Algorithm=TOS4-HMAC-SHA256&X-Tos-Credential={cred}&X-Tos-Date={date}&X-Tos-Expires={exp}&X-Tos-SignedHeaders={sh}",
        cred = credential_encoded,
        date = date_str,
        exp = expires_secs,
        sh = signed_headers,
    );

    let canonical_request = format!(
        "PUT\n{}\n{}\nhost:{}\n\n{}\nUNSIGNED-PAYLOAD",
        path, query, host, signed_headers
    );
    let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
    let string_to_sign = format!(
        "TOS4-HMAC-SHA256\n{}\n{}\n{}",
        date_str, credential_scope, canonical_hash
    );
    let signature = tos4_signature(
        &config.secret_key,
        &date_short,
        &config.region,
        &string_to_sign,
    );

    format!("{}?{}&X-Tos-Signature={}", base_url, query, signature)
}

/// 更新 TOS 文件标签（对齐 SDK：PUT ?tagging + JSON TagSet + Content-MD5）
pub async fn update_object_tags(
    config: &TosConfig,
    object_key: &str,
    tags: HashMap<String, String>,
) -> Result<(), String> {
    let key = object_key.trim_start_matches('/');
    let (host, path, _) = config.official_request_target(key);

    let tag_list: Vec<serde_json::Value> = tags
        .into_iter()
        .map(|(k, v)| serde_json::json!({"Key": k, "Value": v}))
        .collect();
    let body = serde_json::json!({"TagSet": {"Tags": tag_list}}).to_string();
    let body_bytes = body.into_bytes();

    let content_md5 = base64::engine::general_purpose::STANDARD.encode(Md5::digest(&body_bytes));

    let resp = signed_request(
        config,
        reqwest::Method::PUT,
        &host,
        &path,
        &[("tagging", "")],
        &[
            ("Content-Type", "application/json"),
            ("Content-MD5", &content_md5),
        ],
        Some(&body_bytes),
        Duration::from_secs(10),
    )
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("设置标签失败 ({}): {}", status, text));
    }
    Ok(())
}

/// TOS 文件夹中的对象信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct TosObject {
    pub key: String,
    pub size: i64,
    pub last_modified: String,
}

/// ListObjectsV2 单页上限；超过则用 continuation-token 续页
const TOS_LIST_PAGE_SIZE: &str = "1000";
/// 防异常死循环（1000×100 = 10 万对象）
const TOS_LIST_MAX_PAGES: usize = 100;

/// 列出 TOS 文件夹下的所有文件（ListObjectsV2，自动续页）
pub async fn list_folder(
    config: &TosConfig,
    folder_prefix: &str,
) -> Result<(Vec<TosObject>, i64), String> {
    let full_prefix = config.full_key(folder_prefix);
    let prefix = if full_prefix.ends_with('/') {
        full_prefix
    } else {
        format!("{}/", full_prefix)
    };

    let (host, path) = config.bucket_request_target();
    let mut objects = Vec::new();
    let mut continuation: Option<String> = None;

    for _ in 0..TOS_LIST_MAX_PAGES {
        // 按字母序组装（continuation-token < list-type < max-keys < prefix）
        let mut query: Vec<(&str, &str)> = Vec::with_capacity(4);
        if let Some(token) = continuation.as_deref() {
            query.push(("continuation-token", token));
        }
        query.push(("list-type", "2"));
        query.push(("max-keys", TOS_LIST_PAGE_SIZE));
        query.push(("prefix", prefix.as_str()));

        let resp = signed_request(
            config,
            reqwest::Method::GET,
            &host,
            &path,
            &query,
            &[],
            None,
            Duration::from_secs(10),
        )
        .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("TOS ListObjects 失败 ({}): {}", status, body));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| format!("读取响应失败: {}", e))?;

        objects.extend(parse_list_objects(&body));

        let truncated = extract_xml_value(&body, "IsTruncated")
            .map(|s| s.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !truncated {
            break;
        }
        match extract_xml_value(&body, "NextContinuationToken") {
            Some(token) if !token.is_empty() => continuation = Some(token),
            _ => break,
        }
    }

    let mut total_size: i64 = 0;
    objects.retain(|o| {
        // 跳过零字节目录占位（非真实对象）
        if o.size == 0 && o.key.ends_with('/') {
            false
        } else {
            total_size += o.size;
            true
        }
    });

    Ok((objects, total_size))
}

fn parse_list_objects(body: &str) -> Vec<TosObject> {
    let mut objects = Vec::new();
    for content_block in body.split("<Contents").skip(1) {
        let Some((_, body_part)) = content_block.split_once('>') else {
            continue;
        };
        let Some(end) = body_part.find("</Contents>") else {
            continue;
        };
        let block = &body_part[..end];
        let Some(key) = extract_xml_value(block, "Key").filter(|k| !k.is_empty()) else {
            continue;
        };
        let size = extract_xml_value(block, "Size")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let last_modified = extract_xml_value(block, "LastModified").unwrap_or_default();
        objects.push(TosObject {
            key,
            size,
            last_modified,
        });
    }
    objects
}

fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml.find(&close)?;
    Some(xml[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_buckets;

    #[test]
    fn list_buckets_json_keeps_location_and_endpoints() {
        let body = r#"{
            "Buckets": [{
                "Name": "buckettest001",
                "Location": "cn-beijing",
                "ExtranetEndpoint": "tos-cn-beijing.volces.com",
                "IntranetEndpoint": "tos-cn-beijing.ivolces.com"
            }]
        }"#;
        let buckets = parse_buckets(body);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name, "buckettest001");
        assert_eq!(buckets[0].location, "cn-beijing");
        assert_eq!(buckets[0].extranet_endpoint, "tos-cn-beijing.volces.com");
        assert_eq!(buckets[0].intranet_endpoint, "tos-cn-beijing.ivolces.com");
    }

    #[test]
    fn list_buckets_xml_keeps_location_and_endpoints() {
        let body = r#"<ListAllMyBucketsResult><Buckets><Bucket>
            <Name>demo</Name><Location>ap-southeast-1</Location>
            <ExtranetEndpoint>tos-ap-southeast-1.bytepluses.com</ExtranetEndpoint>
            <IntranetEndpoint>tos-ap-southeast-1.ibytepluses.com</IntranetEndpoint>
        </Bucket></Buckets></ListAllMyBucketsResult>"#;
        let buckets = parse_buckets(body);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name, "demo");
        assert_eq!(buckets[0].location, "ap-southeast-1");
        assert_eq!(
            buckets[0].extranet_endpoint,
            "tos-ap-southeast-1.bytepluses.com"
        );
    }
}
