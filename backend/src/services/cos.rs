/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 腾讯云 COS XML REST（q-sign-algorithm=sha1），无官方 SDK。

use base64::Engine;
use hmac::{Hmac, Mac};
use md5::Md5;
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use super::tos::TosObject;

fn cos_http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .http1_only()
            .build()
            .expect("COS HTTP/1 client")
    })
}

type HmacSha1 = Hmac<Sha1>;

fn hmac_sha1_hex(key: &[u8], data: &[u8]) -> String {
    let mut mac = HmacSha1::new_from_slice(key).expect("HMAC key error");
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

fn sha1_hex(data: &[u8]) -> String {
    hex::encode(Sha1::digest(data))
}

/// COS URI 编码：未保留字符用 %XX（大写）；`encode_slash=false` 时保留 `/`
fn cos_uri_encode(s: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b'/' if !encode_slash => out.push('/'),
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

fn endpoint_host(endpoint: &str) -> &str {
    let ep = endpoint.trim().trim_end_matches('/');
    ep.strip_prefix("https://")
        .or_else(|| ep.strip_prefix("http://"))
        .unwrap_or(ep)
}

#[derive(Debug, Clone)]
pub struct CosConfig {
    pub secret_id: String,
    pub secret_key: String,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub path_prefix: String,
    pub custom_domain: String,
}

impl CosConfig {
    pub(crate) fn from_fields(
        secret_id: &str,
        secret_key: &str,
        endpoint: &str,
        region: &str,
        bucket: &str,
        path_prefix: &str,
        custom_domain: &str,
    ) -> Option<Self> {
        let secret_id = secret_id.trim();
        let secret_key = secret_key.trim();
        let endpoint = endpoint.trim();
        let region = region.trim();
        let bucket = bucket.trim();
        if secret_id.is_empty()
            || secret_key.is_empty()
            || endpoint.is_empty()
            || region.is_empty()
            || bucket.is_empty()
        {
            return None;
        }
        Some(Self {
            secret_id: secret_id.to_string(),
            secret_key: secret_key.to_string(),
            endpoint: endpoint.to_string(),
            region: region.to_string(),
            bucket: bucket.to_string(),
            path_prefix: path_prefix.trim().to_string(),
            custom_domain: custom_domain.trim().to_string(),
        })
    }

    pub fn from_map(map: &HashMap<String, String>) -> Option<Self> {
        Self::from_fields(
            map.get("cos_secret_id").map(String::as_str).unwrap_or(""),
            map.get("cos_secret_key").map(String::as_str).unwrap_or(""),
            map.get("cos_endpoint").map(String::as_str).unwrap_or(""),
            map.get("cos_region").map(String::as_str).unwrap_or(""),
            map.get("cos_bucket").map(String::as_str).unwrap_or(""),
            map.get("cos_path_prefix").map(String::as_str).unwrap_or(""),
            map.get("cos_custom_domain")
                .map(String::as_str)
                .unwrap_or(""),
        )
    }

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
            return format!("{scheme}://{host}/{key}");
        }
        self.official_url(key)
    }

    fn request_host(&self) -> String {
        format!("{}.{}", self.bucket, endpoint_host(&self.endpoint))
    }

    fn object_path(object_key: &str) -> String {
        let key = object_key.trim_start_matches('/');
        if key.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", cos_uri_encode(key, false))
        }
    }

    fn official_url(&self, key: &str) -> String {
        let host = self.request_host();
        let key = key.trim_start_matches('/');
        if key.is_empty() {
            format!("https://{host}/")
        } else {
            format!("https://{host}/{key}")
        }
    }

    pub fn full_key(&self, filename: &str) -> String {
        if self.path_prefix.is_empty() {
            filename.to_string()
        } else {
            let prefix = self.path_prefix.trim_end_matches('/');
            format!("{prefix}/{filename}")
        }
    }

    pub fn extract_object_key(&self, file_url: &str) -> Option<String> {
        let try_base = |base: &str| -> Option<String> {
            let prefix = format!("{}/", base.trim_end_matches('/'));
            file_url.strip_prefix(&prefix).map(|s| s.to_string())
        };
        if let Some(key) = try_base(&self.file_url("")) {
            return Some(key);
        }
        if !self.custom_domain.is_empty() {
            return try_base(&self.official_url(""));
        }
        None
    }
}

const COS_MAX_RETRY: u32 = 2;

#[inline]
fn retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 429) || status.is_server_error()
}

/// HeaderList / UrlParamList + HttpHeaders / HttpParameters（官方：key 小写 URL 编码后 `k=v&…`）
fn cos_kv_sig(pairs: &[(String, String)]) -> (String, String) {
    let list = pairs
        .iter()
        .map(|(k, _)| cos_uri_encode(k, true))
        .collect::<Vec<_>>()
        .join(";");
    let joined = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", cos_uri_encode(k, true), cos_uri_encode(v, true)))
        .collect::<Vec<_>>()
        .join("&");
    (list, joined)
}

fn sign_request(
    config: &CosConfig,
    method: &str,
    host: &str,
    uri_path: &str,
    query_pairs: &[(&str, &str)],
    extra_headers: &[(&str, &str)],
    sign_time: &str,
) -> String {
    let mut hdrs: Vec<(String, String)> = Vec::with_capacity(1 + extra_headers.len());
    hdrs.push(("host".into(), host.to_string()));
    for (k, v) in extra_headers {
        hdrs.push((k.to_ascii_lowercase(), (*v).to_string()));
    }
    hdrs.sort_by(|a, b| a.0.cmp(&b.0));
    let (header_list, http_headers) = cos_kv_sig(&hdrs);

    let mut params: Vec<(String, String)> = query_pairs
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), (*v).to_string()))
        .collect();
    params.sort_by(|a, b| a.0.cmp(&b.0));
    let (param_list, http_params) = cos_kv_sig(&params);

    let http_string = format!(
        "{}\n{}\n{}\n{}\n",
        method.to_ascii_lowercase(),
        uri_path,
        http_params,
        http_headers
    );
    let string_to_sign = format!("sha1\n{sign_time}\n{}\n", sha1_hex(http_string.as_bytes()));
    let sign_key = hmac_sha1_hex(config.secret_key.as_bytes(), sign_time.as_bytes());
    let signature = hmac_sha1_hex(sign_key.as_bytes(), string_to_sign.as_bytes());

    format!(
        "q-sign-algorithm=sha1&q-ak={}&q-sign-time={sign_time}&q-key-time={sign_time}&q-header-list={header_list}&q-url-param-list={param_list}&q-signature={signature}",
        config.secret_id,
    )
}

fn request_url(
    host: &str,
    path: &str,
    query_pairs: &[(&str, &str)],
) -> Result<reqwest::Url, String> {
    let mut url = format!("https://{host}{path}");
    if !query_pairs.is_empty() {
        let qs = query_pairs
            .iter()
            .map(|(k, v)| format!("{}={}", cos_uri_encode(k, true), cos_uri_encode(v, true)))
            .collect::<Vec<_>>()
            .join("&");
        url.push('?');
        url.push_str(&qs);
    }
    reqwest::Url::parse(&url).map_err(|e| format!("COS URL 无效: {e}"))
}

async fn signed_request(
    config: &CosConfig,
    method: reqwest::Method,
    host: &str,
    path: &str,
    query_pairs: &[(&str, &str)],
    extra_headers: &[(&str, &str)],
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    let url = request_url(host, path, query_pairs)?;
    let mut attempt = 0u32;
    loop {
        if attempt > 0 {
            let ms = (100u64 << (attempt - 1)).min(1000);
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }
        match signed_request_once(
            config,
            method.clone(),
            host,
            path,
            query_pairs,
            extra_headers,
            url.clone(),
            body,
            timeout,
        )
        .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() || !retryable_status(status) || attempt >= COS_MAX_RETRY {
                    return Ok(resp);
                }
                let _ = resp.bytes().await;
            }
            Err(e) if attempt >= COS_MAX_RETRY => return Err(e),
            Err(_) => {}
        }
        attempt += 1;
    }
}

async fn signed_request_once(
    config: &CosConfig,
    method: reqwest::Method,
    host: &str,
    path: &str,
    query_pairs: &[(&str, &str)],
    extra_headers: &[(&str, &str)],
    url: reqwest::Url,
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    let now = chrono::Utc::now().timestamp();
    let sign_time = format!("{};{}", now, now + 3600);
    let mut signed_extras: Vec<(String, String)> = extra_headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), (*v).to_string()))
        .collect();
    if let Some(b) = body {
        signed_extras.push(("content-length".into(), b.len().to_string()));
    }
    let extra_refs: Vec<(&str, &str)> = signed_extras
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let authorization = sign_request(
        config,
        method.as_str(),
        host,
        path,
        query_pairs,
        &extra_refs,
        &sign_time,
    );
    let mut builder = cos_http()
        .request(method, url)
        .header("Host", host)
        .header("Authorization", &authorization)
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
        .map_err(|e| format!("COS 请求失败: {e}"))
}

/// 用 SecretId/SecretKey 列举腾讯云 COS 账号下的存储桶。
pub async fn list_buckets(
    secret_id: &str,
    secret_key: &str,
    prefer_endpoint: &str,
) -> Result<Vec<super::tos::TosListedBucket>, String> {
    let prefer_host = if !prefer_endpoint.trim().is_empty() {
        endpoint_host(prefer_endpoint).trim()
    } else {
        ""
    };
    let mut targets = vec!["service.cos.myqcloud.com"];
    if !prefer_host.is_empty() && !targets.contains(&prefer_host) {
        targets.push(prefer_host);
    }

    let mut last_err = String::new();
    for host in targets {
        match list_buckets_at(secret_id, secret_key, host).await {
            Ok(buckets) => return Ok(buckets),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

async fn list_buckets_at(
    secret_id: &str,
    secret_key: &str,
    host: &str,
) -> Result<Vec<super::tos::TosListedBucket>, String> {
    let temp_config = CosConfig {
        secret_id: secret_id.trim().to_string(),
        secret_key: secret_key.trim().to_string(),
        endpoint: host.to_string(),
        region: String::new(),
        bucket: String::new(),
        path_prefix: String::new(),
        custom_domain: String::new(),
    };
    let resp = signed_request(
        &temp_config,
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
        .map_err(|e| format!("读取存储桶列表失败: {e}"))?;

    if !status.is_success() {
        let msg = extract_xml_value(&body, "Message")
            .or_else(|| extract_xml_value(&body, "Code"))
            .unwrap_or_else(|| body.chars().take(300).collect());
        return Err(format!("列举存储桶失败 ({status}): {msg}"));
    }
    Ok(parse_cos_buckets(&body))
}

/// 测试当前配置：写入一个小文件再删除，确认存储可正常使用
pub async fn test_connection(config: &CosConfig) -> Result<String, String> {
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

pub async fn upload_file(
    config: &CosConfig,
    object_key: &str,
    data: Vec<u8>,
    content_type: &str,
    tags: Option<&str>,
) -> Result<String, String> {
    let key = object_key.trim_start_matches('/');
    let host = config.request_host();
    let path = CosConfig::object_path(key);

    let mut extras: Vec<(String, String)> = Vec::new();
    if !content_type.is_empty() {
        extras.push(("Content-Type".into(), content_type.to_string()));
    }
    if let Some(t) = tags.filter(|s| !s.is_empty()) {
        extras.push(("x-cos-tagging".into(), t.to_string()));
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
        return Err(format!("上传失败 ({status}): {body}"));
    }
    Ok(config.file_url(object_key))
}

pub async fn delete_file(config: &CosConfig, object_key: &str) -> Result<(), String> {
    delete_file_inner(config, object_key).await.map(|_| ())
}

async fn delete_file_inner(config: &CosConfig, object_key: &str) -> Result<bool, String> {
    let key = object_key.trim_start_matches('/');
    let host = config.request_host();
    let path = CosConfig::object_path(key);
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
    Err(format!("删除失败 ({status}): {body}"))
}

pub fn generate_presigned_put_url(
    config: &CosConfig,
    object_key: &str,
    expires_secs: u64,
) -> String {
    let key = object_key.trim_start_matches('/');
    let host = config.request_host();
    let path = CosConfig::object_path(key);
    let now = chrono::Utc::now().timestamp();
    let sign_time = format!("{};{}", now, now + expires_secs.max(1) as i64);
    let authorization = sign_request(config, "PUT", &host, &path, &[], &[], &sign_time);
    let qs = authorization
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => {
                format!("{}={}", cos_uri_encode(k, true), cos_uri_encode(v, true))
            }
            None => cos_uri_encode(pair, true),
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("https://{host}{path}?{qs}")
}

pub async fn update_object_tags(
    config: &CosConfig,
    object_key: &str,
    tags: HashMap<String, String>,
) -> Result<(), String> {
    let key = object_key.trim_start_matches('/');
    let host = config.request_host();
    let path = CosConfig::object_path(key);

    let mut xml = String::from("<Tagging><TagSet>");
    for (k, v) in &tags {
        xml.push_str("<Tag><Key>");
        xml.push_str(&xml_escape(k));
        xml.push_str("</Key><Value>");
        xml.push_str(&xml_escape(v));
        xml.push_str("</Value></Tag>");
    }
    xml.push_str("</TagSet></Tagging>");
    let body_bytes = xml.into_bytes();
    let content_md5 = base64::engine::general_purpose::STANDARD.encode(Md5::digest(&body_bytes));

    let resp = signed_request(
        config,
        reqwest::Method::PUT,
        &host,
        &path,
        &[("tagging", "")],
        &[
            ("Content-Type", "application/xml"),
            ("Content-MD5", &content_md5),
        ],
        Some(&body_bytes),
        Duration::from_secs(10),
    )
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("设置标签失败 ({status}): {text}"));
    }
    Ok(())
}

const LIST_PAGE_SIZE: &str = "1000";
const LIST_MAX_PAGES: usize = 100;

pub async fn list_folder(
    config: &CosConfig,
    folder_prefix: &str,
) -> Result<(Vec<TosObject>, i64), String> {
    let full_prefix = config.full_key(folder_prefix);
    let prefix = if full_prefix.ends_with('/') {
        full_prefix
    } else {
        format!("{full_prefix}/")
    };
    let host = config.request_host();
    let mut objects = Vec::new();
    let mut marker: Option<String> = None;

    for _ in 0..LIST_MAX_PAGES {
        let mut query: Vec<(&str, &str)> =
            vec![("max-keys", LIST_PAGE_SIZE), ("prefix", prefix.as_str())];
        if let Some(m) = marker.as_deref() {
            query.push(("marker", m));
        }
        query.sort_by(|a, b| a.0.cmp(b.0));

        let resp = signed_request(
            config,
            reqwest::Method::GET,
            &host,
            "/",
            &query,
            &[],
            None,
            Duration::from_secs(10),
        )
        .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("COS ListObjects 失败 ({status}): {body}"));
        }
        let body = resp
            .text()
            .await
            .map_err(|e| format!("读取响应失败: {e}"))?;
        objects.extend(parse_list_objects(&body));
        let truncated = extract_xml_value(&body, "IsTruncated")
            .map(|s| s.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !truncated {
            break;
        }
        match extract_xml_value(&body, "NextMarker")
            .or_else(|| objects.last().map(|o| o.key.clone()))
        {
            Some(m) if !m.is_empty() => marker = Some(m),
            _ => break,
        }
    }

    let mut total_size: i64 = 0;
    objects.retain(|o| {
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
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn parse_cos_buckets(body: &str) -> Vec<super::tos::TosListedBucket> {
    let mut buckets = Vec::new();
    for block in body.split("<Bucket>").skip(1) {
        let Some(name) = extract_xml_value(block, "Name").filter(|s| !s.is_empty()) else {
            continue;
        };
        let location = extract_xml_value(block, "Location")
            .or_else(|| extract_xml_value(block, "LocationConstraint"))
            .unwrap_or_default();
        let extranet_endpoint = if !location.is_empty() {
            format!("https://cos.{location}.myqcloud.com")
        } else {
            String::new()
        };
        let intranet_endpoint = if !location.is_empty() {
            format!("https://cos-internal.{location}.tencentcos.cn")
        } else {
            String::new()
        };
        buckets.push(super::tos::TosListedBucket {
            name,
            location,
            extranet_endpoint,
            intranet_endpoint,
        });
    }
    buckets
}

