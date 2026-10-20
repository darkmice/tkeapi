/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 对象存储门面：TOS / COS 共用上传、删除、预签名、列举，调用方不绑厂商签名。

use std::collections::HashMap;

use crate::models::StorageSettings;
use crate::services::cos::{self, CosConfig};
use crate::services::tos::{self, TosConfig, TosObject};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreKind {
    Tos,
    Cos,
}

/// 插件 `default_provider=global`：上传跟随站点设置，不是一个独立厂商。
pub fn is_global_storage_default(s: &str) -> bool {
    s.eq_ignore_ascii_case("global")
}

/// 上传文件名的扩展名：小写、仅字母数字且不超过 8 位，否则用 `fallback`
pub fn upload_ext(file_name: &str, fallback: &str) -> String {
    file_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| fallback.to_string())
}

/// 站点存储里插件文件的统一根：`{path_prefix}/plugins/{插件名}/`
const PLUGINS_DIR: &str = "plugins";

/// 站点存储里用户数据的统一根：`{path_prefix}/users/{uid}/{UserFolder}/`
const USERS_DIR: &str = "users";

/// 站点自身文件在 `{path_prefix}/` 下的固定目录
#[derive(Debug, Clone, Copy)]
pub enum SiteFolder {
    /// 后台基础设置里的图片（站点 Logo 等）
    Settings,
    ChannelCache,
}

impl SiteFolder {
    fn as_str(self) -> &'static str {
        match self {
            SiteFolder::Settings => "settings",
            SiteFolder::ChannelCache => "channel_cache",
        }
    }
}

/// 单个用户名下的数据分类（`users/{uid}/` 下一级）
#[derive(Debug, Clone, Copy)]
pub enum UserFolder {
    Kyc,
    Avatar,
}

impl UserFolder {
    fn as_str(self) -> &'static str {
        match self {
            UserFolder::Kyc => "kyc",
            UserFolder::Avatar => "avatar",
        }
    }
}

impl StoreKind {
    pub const ALL: [Self; 2] = [Self::Tos, Self::Cos];

    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("cos") {
            StoreKind::Cos
        } else {
            StoreKind::Tos
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            StoreKind::Tos => "tos",
            StoreKind::Cos => "cos",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ObjectStore {
    Tos(TosConfig),
    Cos(CosConfig),
}

impl ObjectStore {
    pub fn provider(&self) -> &'static str {
        match self {
            ObjectStore::Tos(_) => StoreKind::Tos.as_str(),
            ObjectStore::Cos(_) => StoreKind::Cos.as_str(),
        }
    }

    pub fn bucket(&self) -> &str {
        match self {
            ObjectStore::Tos(c) => &c.bucket,
            ObjectStore::Cos(c) => &c.bucket,
        }
    }

    pub fn endpoint(&self) -> &str {
        match self {
            ObjectStore::Tos(c) => &c.endpoint,
            ObjectStore::Cos(c) => &c.endpoint,
        }
    }

    pub fn path_prefix(&self) -> &str {
        match self {
            ObjectStore::Tos(c) => &c.path_prefix,
            ObjectStore::Cos(c) => &c.path_prefix,
        }
    }

    pub fn region(&self) -> &str {
        match self {
            ObjectStore::Tos(c) => &c.region,
            ObjectStore::Cos(c) => &c.region,
        }
    }

    pub fn from_settings(s: &StorageSettings) -> Option<Self> {
        let p = s.default_provider.trim();
        if p.is_empty() || p.eq_ignore_ascii_case("none") {
            return None;
        }
        Self::from_settings_for(s, StoreKind::parse(&s.default_provider))
    }

    pub fn from_settings_for(s: &StorageSettings, kind: StoreKind) -> Option<Self> {
        match kind {
            StoreKind::Tos => tos_from_settings(s).map(ObjectStore::Tos),
            StoreKind::Cos => CosConfig::from_fields(
                &s.cos_secret_id,
                &s.cos_secret_key,
                &s.cos_endpoint,
                &s.cos_region,
                &s.cos_bucket,
                &s.cos_path_prefix,
                &s.cos_custom_domain,
            )
            .map(ObjectStore::Cos),
        }
    }

    pub fn from_plugin_map_for(map: &HashMap<String, String>, kind: StoreKind) -> Option<Self> {
        match kind {
            StoreKind::Tos => TosConfig::from_map(map).map(ObjectStore::Tos),
            StoreKind::Cos => CosConfig::from_map(map).map(ObjectStore::Cos),
        }
    }

    pub fn full_key(&self, filename: &str) -> String {
        match self {
            ObjectStore::Tos(c) => c.full_key(filename),
            ObjectStore::Cos(c) => c.full_key(filename),
        }
    }

    pub fn site_key(&self, folder: SiteFolder, relative: &str) -> String {
        self.full_key(&format!(
            "{}/{}",
            folder.as_str(),
            relative.trim_start_matches('/')
        ))
    }

    pub fn user_key(&self, uid: &str, folder: UserFolder, relative: &str) -> String {
        self.full_key(&format!(
            "{USERS_DIR}/{}/{}/{}",
            uid.trim(),
            folder.as_str(),
            relative.trim_start_matches('/')
        ))
    }

    /// 站点存储交给插件用时，前缀下沉到 `plugins/{插件名}`；插件独立配置的存储不走这里。
    pub fn scoped_to_plugin(mut self, plugin_name: &str) -> Self {
        let name: String = plugin_name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        let prefix = match &mut self {
            ObjectStore::Tos(c) => &mut c.path_prefix,
            ObjectStore::Cos(c) => &mut c.path_prefix,
        };
        let base = prefix.trim().trim_matches('/');
        *prefix = if base.is_empty() {
            format!("{PLUGINS_DIR}/{name}")
        } else {
            format!("{base}/{PLUGINS_DIR}/{name}")
        };
        self
    }

    pub fn file_url(&self, object_key: &str) -> String {
        match self {
            ObjectStore::Tos(c) => c.file_url(object_key),
            ObjectStore::Cos(c) => c.file_url(object_key),
        }
    }

    pub fn extract_object_key(&self, file_url: &str) -> Option<String> {
        match self {
            ObjectStore::Tos(c) => c.extract_object_key(file_url),
            ObjectStore::Cos(c) => c.extract_object_key(file_url),
        }
    }

    pub async fn test_connection(&self) -> Result<String, String> {
        match self {
            ObjectStore::Tos(c) => tos::test_connection(c).await,
            ObjectStore::Cos(c) => cos::test_connection(c).await,
        }
    }

    pub async fn upload_file(
        &self,
        object_key: &str,
        data: Vec<u8>,
        content_type: &str,
        tags: Option<&str>,
    ) -> Result<String, String> {
        match self {
            ObjectStore::Tos(c) => tos::upload_file(c, object_key, data, content_type, tags).await,
            ObjectStore::Cos(c) => cos::upload_file(c, object_key, data, content_type, tags).await,
        }
    }

    pub async fn delete_file(&self, object_key: &str) -> Result<(), String> {
        match self {
            ObjectStore::Tos(c) => tos::delete_file(c, object_key).await,
            ObjectStore::Cos(c) => cos::delete_file(c, object_key).await,
        }
    }

    pub fn generate_presigned_put_url(&self, object_key: &str, expires_secs: u64) -> String {
        match self {
            ObjectStore::Tos(c) => tos::generate_presigned_put_url(c, object_key, expires_secs),
            ObjectStore::Cos(c) => cos::generate_presigned_put_url(c, object_key, expires_secs),
        }
    }

    pub async fn update_object_tags(
        &self,
        object_key: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), String> {
        match self {
            ObjectStore::Tos(c) => tos::update_object_tags(c, object_key, tags).await,
            ObjectStore::Cos(c) => cos::update_object_tags(c, object_key, tags).await,
        }
    }

    pub async fn list_folder(&self, folder_prefix: &str) -> Result<(Vec<TosObject>, i64), String> {
        match self {
            ObjectStore::Tos(c) => tos::list_folder(c, folder_prefix).await,
            ObjectStore::Cos(c) => cos::list_folder(c, folder_prefix).await,
        }
    }

    /// 删除 `users/{uid}/{folder}/` 下除 `keep` 以外的全部对象；单个对象删除失败只记日志
    pub async fn prune_user_folder(
        &self,
        uid: &str,
        folder: UserFolder,
        keep: Option<&str>,
    ) -> Result<(), String> {
        let prefix = format!("{USERS_DIR}/{}/{}", uid.trim(), folder.as_str());
        let (objects, _) = self.list_folder(&prefix).await?;
        let keep = keep.map(|k| k.trim().trim_start_matches('/'));
        for obj in objects {
            let key = obj.key.trim().trim_start_matches('/');
            if key.is_empty() || Some(key) == keep {
                continue;
            }
            if let Err(e) = self.delete_file(key).await {
                tracing::warn!("用户目录对象删除失败 {}: {}", key, e);
            }
        }
        Ok(())
    }

    pub fn collect_object_keys(
        &self,
        keys_and_urls: impl IntoIterator<Item = (String, String)>,
    ) -> Vec<String> {
        collect_keys_with(self, keys_and_urls)
    }

    pub fn spawn_purge(
        self,
        relative_prefix: String,
        extra_keys: Vec<String>,
        label: &'static str,
        id: i64,
    ) {
        match self {
            ObjectStore::Tos(c) => tos::spawn_purge(c, relative_prefix, extra_keys, label, id),
            store @ ObjectStore::Cos(_) => {
                tokio::spawn(async move {
                    let report = store.purge_prefix(&relative_prefix, extra_keys).await;
                    if report.ok {
                        tracing::info!("{} {} 对象存储清理成功 {}", label, id, report.detail);
                    } else {
                        tracing::warn!("{} {} 对象存储清理异常 {}", label, id, report.detail);
                    }
                });
            }
        }
    }

    async fn purge_prefix(&self, relative_prefix: &str, extra_keys: Vec<String>) -> PurgeReport {
        let prefix = relative_prefix.trim().trim_matches('/').to_string();
        let mut keys = std::collections::HashSet::<String>::new();
        for key in &extra_keys {
            let key = key.trim().trim_start_matches('/');
            if !key.is_empty() {
                keys.insert(key.to_string());
            }
        }
        match self.list_folder(&format!("{prefix}/")).await {
            Ok((objects, _)) => {
                for obj in objects {
                    let key = obj.key.trim().trim_start_matches('/');
                    if !key.is_empty() {
                        keys.insert(key.to_string());
                    }
                }
            }
            Err(e) => {
                return PurgeReport {
                    ok: false,
                    detail: format!(
                        "prefix={} list=ERR {}",
                        self.full_key(&format!("{prefix}/")),
                        e
                    ),
                };
            }
        }
        keys.insert(self.full_key(&format!("{prefix}/.keep")));
        let mut fail = 0usize;
        let mut errs = Vec::new();
        for key in &keys {
            if let Err(e) = self.delete_file(key).await {
                fail += 1;
                if errs.len() < 5 {
                    errs.push(format!("{key}: {e}"));
                }
            }
        }
        let detail = format!(
            "prefix={} list=ok keys={} fail={}{}",
            self.full_key(&format!("{prefix}/")),
            keys.len(),
            fail,
            if errs.is_empty() {
                String::new()
            } else {
                format!(" errs={}", errs.join(" | "))
            }
        );
        PurgeReport {
            ok: fail == 0,
            detail,
        }
    }
}

struct PurgeReport {
    ok: bool,
    detail: String,
}

fn collect_keys_with(
    store: &ObjectStore,
    keys_and_urls: impl IntoIterator<Item = (String, String)>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (key, url) in keys_and_urls {
        for candidate in [key, url] {
            let k = candidate.trim().trim_start_matches('/');
            if k.is_empty() {
                continue;
            }
            let resolved = if k.contains("://") {
                let Some(r) = store.extract_object_key(k).filter(|s| !s.is_empty()) else {
                    continue;
                };
                r
            } else {
                k.to_string()
            };
            if seen.insert(resolved.clone()) {
                out.push(resolved);
            }
        }
    }
    out
}

fn tos_from_settings(s: &StorageSettings) -> Option<TosConfig> {
    if s.tos_access_key.trim().is_empty()
        || s.tos_secret_key.trim().is_empty()
        || s.tos_endpoint.trim().is_empty()
        || s.tos_region.trim().is_empty()
        || s.tos_bucket.trim().is_empty()
    {
        return None;
    }
    Some(TosConfig {
        access_key: s.tos_access_key.trim().to_string(),
        secret_key: s.tos_secret_key.trim().to_string(),
        endpoint: s.tos_endpoint.trim().to_string(),
        region: s.tos_region.trim().to_string(),
        bucket: s.tos_bucket.trim().to_string(),
        path_prefix: s.tos_path_prefix.trim().to_string(),
        custom_domain: s.tos_custom_domain.trim().to_string(),
    })
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ListBucketsQuery {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub tos_access_key: Option<String>,
    #[serde(default)]
    pub tos_secret_key: Option<String>,
    #[serde(default)]
    pub tos_endpoint: Option<String>,
    #[serde(default)]
    pub cos_secret_id: Option<String>,
    #[serde(default)]
    pub cos_secret_key: Option<String>,
    #[serde(default)]
    pub cos_endpoint: Option<String>,
}

fn resolve_cred(input: Option<&str>, saved: Option<&str>, is_secret: bool) -> String {
    let mut val = input.unwrap_or_default().trim();
    if val.is_empty() || (is_secret && val.contains("****")) {
        if let Some(s) = saved {
            val = s.trim();
        }
    }
    val.to_string()
}

/// 统一调用 TOS 或 COS 的 ListBuckets 接口，自动补齐历史已保存的脱敏凭据并做入参校验
pub async fn list_buckets_auto(
    req: &ListBucketsQuery,
    saved_tos: Option<(&str, &str)>,
    saved_cos: Option<(&str, &str)>,
) -> Result<Vec<crate::services::tos::TosListedBucket>, String> {
    let provider = req
        .provider
        .as_deref()
        .map(str::trim)
        .unwrap_or_else(|| {
            if req.cos_secret_id.is_some() && req.tos_access_key.is_none() {
                "cos"
            } else {
                "tos"
            }
        });

    if provider.eq_ignore_ascii_case("cos") {
        let saved_id = saved_cos.map(|(id, _)| id);
        let saved_key = saved_cos.map(|(_, key)| key);
        let id = resolve_cred(req.cos_secret_id.as_deref(), saved_id, false);
        let key = resolve_cred(req.cos_secret_key.as_deref(), saved_key, true);
        if id.is_empty() || key.is_empty() {
            return Err("请先填写 SecretId 和 SecretKey".to_string());
        }
        let prefer = req.cos_endpoint.as_deref().unwrap_or_default();
        crate::services::cos::list_buckets(&id, &key, prefer).await
    } else {
        let saved_ak = saved_tos.map(|(ak, _)| ak);
        let saved_sk = saved_tos.map(|(_, sk)| sk);
        let ak = resolve_cred(req.tos_access_key.as_deref(), saved_ak, false);
        let sk = resolve_cred(req.tos_secret_key.as_deref(), saved_sk, true);
        if ak.is_empty() || sk.is_empty() {
            return Err("请先填写 Access Key 和 Secret Key".to_string());
        }
        let prefer = req.tos_endpoint.as_deref().unwrap_or_default();
        crate::services::tos::list_buckets(&ak, &sk, prefer).await
    }
}
