/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use axum::{
    extract::{Extension, Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;
use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    auth,
    error::AppError,
    models::{CreateSiteIconReq, SiteIcon, SiteIconQuery, SiteIconSyncLog, UpdateSiteIconReq},
    time_system::DbTs,
    AppState,
};

const PLUGIN_NAME: &str = "site_icons";
const STORAGE_REQUIRED: &str = "请配置站点存储才可以添加图标";

async fn icon_store(state: &AppState) -> Option<crate::services::object_store::ObjectStore> {
    crate::api::plugins::site_store_for_plugin(state, PLUGIN_NAME).await
}

/// 图标名仅允许字母数字下划线连字符，防止路径穿越。
fn sanitize_icon_name(name: &str) -> Result<String, AppError> {
    let normalized = name.trim().to_lowercase().replace(' ', "_");
    if normalized.is_empty() {
        return Err(AppError::BadRequest("图标名称不能为空".into()));
    }
    if normalized.len() > 64 {
        return Err(AppError::BadRequest("图标名称过长".into()));
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::BadRequest(
            "图标名称仅允许字母、数字、下划线和连字符".into(),
        ));
    }
    Ok(normalized)
}

/// 将相对路径限制在 assets_dir 下，拒绝 `..` 穿越。
fn resolve_asset_path(assets_dir: &str, relative: &str) -> Result<PathBuf, AppError> {
    let base = FsPath::new(assets_dir);
    let mut full = base.to_path_buf();
    for comp in FsPath::new(relative).components() {
        match comp {
            Component::Normal(s) => full.push(s),
            Component::CurDir => {}
            _ => {
                return Err(AppError::BadRequest("非法文件路径".into()));
            }
        }
    }
    Ok(full)
}

/// 拒绝可执行脚本的 SVG（管理端配合写入侧兜底）。
fn validate_svg_content(svg: &str) -> Result<(), AppError> {
    if svg.len() > 512 * 1024 {
        return Err(AppError::BadRequest("SVG 内容过大 (最大 512KB)".into()));
    }
    let lower = svg.to_lowercase();
    if lower.contains("<script")
        || lower.contains("javascript:")
        || lower.contains("data:text/html")
        || lower.contains("vbscript:")
    {
        return Err(AppError::BadRequest("SVG 含有不安全脚本内容".into()));
    }
    // 危险事件处理器检测 (仅在属性赋值形如 ` onload=` 时触发，避免误伤常规文本或 CSS 属性)
    if has_dangerous_svg_event(&lower) {
        return Err(AppError::BadRequest(
            "SVG 含有不安全的事件处理器属性".into(),
        ));
    }
    Ok(())
}

fn has_dangerous_svg_event(lower_svg: &str) -> bool {
    let dangerous_events = [
        "onload",
        "onerror",
        "onclick",
        "onmouseover",
        "onmouseout",
        "onmouseenter",
        "onmouseleave",
        "onmousedown",
        "onmouseup",
        "onmousemove",
        "onfocus",
        "onblur",
        "onchange",
        "onsubmit",
        "onreset",
        "onselect",
        "onkeydown",
        "onkeypress",
        "onkeyup",
        "onanimationstart",
        "onanimationend",
        "onbegin",
        "onend",
        "onrepeat",
    ];
    for evt in &dangerous_events {
        if let Some(pos) = lower_svg.find(evt) {
            let after = &lower_svg[pos + evt.len()..];
            let trimmed = after.trim_start();
            if trimmed.starts_with('=') {
                return true;
            }
        }
    }
    false
}

async fn require_admin(state: &AppState, claims: &auth::Claims) -> Result<(), AppError> {
    let role: String =
        sqlx::query_scalar(&state.db.format_query("SELECT role FROM users WHERE id = ?"))
            .bind(&claims.sub)
            .fetch_one(&state.db.pool)
            .await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Admin access required".into()));
    }
    Ok(())
}

/// 同步进度追踪器
#[derive(Debug, Clone)]
pub struct SyncProgress {
    inner: Arc<Mutex<SyncProgressInner>>,
}

#[derive(Debug)]
struct SyncProgressInner {
    running: bool,
    total: i64,
    current: i64,
    logs: Vec<String>,
    finished: bool,
    error: Option<String>,
}

impl SyncProgress {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SyncProgressInner {
                running: false,
                total: 0,
                current: 0,
                logs: Vec::new(),
                finished: false,
                error: None,
            })),
        }
    }

    async fn start(&self, total: i64) {
        let mut inner = self.inner.lock().await;
        inner.running = true;
        inner.total = total;
        inner.current = 0;
        inner.logs = vec![format!("🚀 开始同步，共发现 {} 个图标目录", total)];
        inner.finished = false;
        inner.error = None;
    }

    async fn log(&self, msg: String) {
        let mut inner = self.inner.lock().await;
        inner.logs.push(msg);
        if inner.logs.len() > 500 {
            let start = inner.logs.len() - 500;
            inner.logs = inner.logs.split_off(start);
        }
    }

    async fn progress(&self, current: i64) {
        let mut inner = self.inner.lock().await;
        inner.current = current;
    }

    async fn finish(&self, msg: String) {
        let mut inner = self.inner.lock().await;
        inner.logs.push(msg);
        inner.finished = true;
        inner.running = false;
    }

    async fn fail(&self, err: String) {
        let mut inner = self.inner.lock().await;
        inner.logs.push(format!("❌ {}", err));
        inner.error = Some(err);
        inner.finished = true;
        inner.running = false;
    }

    async fn get_status(&self, since: usize) -> serde_json::Value {
        let inner = self.inner.lock().await;
        let new_logs: Vec<&String> = inner.logs.iter().skip(since).collect();
        json!({
            "running": inner.running,
            "total": inner.total,
            "current": inner.current,
            "finished": inner.finished,
            "error": inner.error,
            "logs": new_logs,
            "log_offset": inner.logs.len(),
        })
    }

    async fn is_running(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.running
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_icons).post(create_icon))
        .route("/public", get(list_icons_public))
        .route("/storage-status", get(get_storage_status))
        .route("/sync", post(sync_from_github))
        .route("/sync-progress", get(get_sync_progress))
        .route("/sync-logs", get(list_sync_logs))
        .route("/reset", post(reset_icon_library))
        .route("/{id}/content", get(get_icon_content))
        .route("/{id}", put(update_icon).delete(delete_icon))
}

/// 查询当前站点绑定的存储配置状态（管理员）
pub async fn get_storage_status(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    if let Some(store) = icon_store(&state).await {
        Ok(Json(json!({
            "success": true,
            "provider": store.provider(),
            "is_cloud": true,
            "bucket": store.bucket(),
            "endpoint": store.endpoint(),
            "region": store.region(),
            "path_prefix": store.path_prefix(),
        })))
    } else {
        Ok(Json(json!({
            "success": true,
            "provider": "local",
            "is_cloud": false,
            "bucket": "",
            "endpoint": "",
            "region": "",
            "path_prefix": "",
        })))
    }
}

/// 列出/搜索图标 (管理员)
pub async fn list_icons(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Query(params): Query<SiteIconQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    let page = params.page.unwrap_or(1).max(1);
    let size = params.size.unwrap_or(60).min(200);
    let offset = (page - 1) * size;

    let mut conditions = vec!["1=1".to_string()];
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref q) = params.q {
        if !q.is_empty() {
            conditions.push(format!(
                "(name ILIKE ${} OR title ILIKE ${})",
                binds.len() + 1,
                binds.len() + 2
            ));
            binds.push(format!("%{}%", q));
            binds.push(format!("%{}%", q));
        }
    }
    if let Some(ref cat) = params.category {
        if !cat.is_empty() {
            conditions.push(format!("category = ${}", binds.len() + 1));
            binds.push(cat.clone());
        }
    }
    if let Some(ref src) = params.source {
        if !src.is_empty() {
            conditions.push(format!("source = ${}", binds.len() + 1));
            binds.push(src.clone());
        }
    }
    if let Some(letter) = params
        .initial
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let letter = letter.to_ascii_lowercase();
        if letter == "#" {
            conditions.push("substring(lower(name) from 1 for 1) !~ '^[a-z]$'".to_string());
        } else if letter.len() == 1 && letter.chars().all(|c| c.is_ascii_alphabetic()) {
            conditions.push(format!("lower(name) LIKE ${}", binds.len() + 1));
            binds.push(format!("{letter}%"));
        }
    }

    let where_clause = conditions.join(" AND ");

    let count_sql = format!("SELECT COUNT(*) FROM site_icons WHERE {}", where_clause);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    for b in &binds {
        count_query = count_query.bind(b);
    }
    let total: i64 = count_query.fetch_one(&state.db.pool).await?;

    let data_sql = format!(
        "SELECT * FROM site_icons WHERE {} ORDER BY lower(name) ASC, name ASC LIMIT ${} OFFSET ${}",
        where_clause,
        binds.len() + 1,
        binds.len() + 2
    );
    let mut data_query = sqlx::query_as::<_, SiteIcon>(&data_sql);
    for b in &binds {
        data_query = data_query.bind(b);
    }
    data_query = data_query.bind(size).bind(offset);
    let icons: Vec<SiteIcon> = data_query.fetch_all(&state.db.pool).await?;

    Ok(Json(json!({
        "success": true,
        "data": icons,
        "total": total,
        "page": page,
        "size": size
    })))
}

/// 公开搜索图标 (已登录用户)
pub async fn list_icons_public(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SiteIconQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let size = params.size.unwrap_or(60).min(200);
    let offset = (page - 1) * size;

    let mut where_parts = vec!["is_active = 1".to_string()];
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref q) = params.q {
        if !q.is_empty() {
            where_parts.push(format!(
                "(name ILIKE ${} OR title ILIKE ${})",
                binds.len() + 1,
                binds.len() + 2
            ));
            binds.push(format!("%{}%", q));
            binds.push(format!("%{}%", q));
        }
    }
    if let Some(ref cat) = params.category {
        if !cat.is_empty() {
            where_parts.push(format!("category = ${}", binds.len() + 1));
            binds.push(cat.clone());
        }
    }

    let where_clause = where_parts.join(" AND ");

    let count_sql = format!("SELECT COUNT(*) FROM site_icons WHERE {}", where_clause);
    let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
    for b in &binds {
        cq = cq.bind(b);
    }
    let total: i64 = cq.fetch_one(&state.db.pool).await?;

    let data_sql = format!(
        "SELECT * FROM site_icons WHERE {} ORDER BY source ASC, name ASC LIMIT ${} OFFSET ${}",
        where_clause,
        binds.len() + 1,
        binds.len() + 2
    );
    let mut dq = sqlx::query_as::<_, SiteIcon>(&data_sql);
    for b in &binds {
        dq = dq.bind(b);
    }
    dq = dq.bind(size).bind(offset);
    let icons: Vec<SiteIcon> = dq.fetch_all(&state.db.pool).await?;

    Ok(Json(json!({
        "success": true,
        "data": icons,
        "total": total,
        "page": page,
        "size": size
    })))
}

/// 手动添加自定义 SVG 图标
pub async fn create_icon(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Json(payload): Json<CreateSiteIconReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    if payload.name.trim().is_empty() || payload.svg_content.trim().is_empty() {
        return Err(AppError::BadRequest("图标名称和 SVG 内容不能为空".into()));
    }
    validate_svg_content(&payload.svg_content)?;

    let safe_name = sanitize_icon_name(&payload.name)?;
    let file_name = format!("{}.svg", safe_name);
    let assets_dir = &state.config.assets_dir;

    let Some(store) = icon_store(&state).await else {
        return Err(AppError::BadRequest(STORAGE_REQUIRED.into()));
    };
    let object_key = store.full_key(&format!("icons/custom/{}", file_name));
    store
        .upload_file(
            &object_key,
            payload.svg_content.as_bytes().to_vec(),
            "image/svg+xml",
            None,
        )
        .await
        .map_err(|e| AppError::BadRequest(format!("上传到站点存储失败: {}", e)))?;

    // 展示、编辑和模型广场都读本地 icons/lobe/{name}.svg，云端只承接手动添加的副本。
    let lobe_rel = format!("icons/lobe/{}", file_name);
    let lobe_dir = format!("{}/icons/lobe", assets_dir);
    tokio::fs::create_dir_all(&lobe_dir)
        .await
        .map_err(|e| AppError::Internal(format!("创建本地图标目录失败: {}", e)))?;
    let lobe_full = resolve_asset_path(assets_dir, &lobe_rel)?;
    tokio::fs::write(&lobe_full, &payload.svg_content)
        .await
        .map_err(|e| AppError::Internal(format!("写入本地 SVG 文件失败: {}", e)))?;

    let custom_rel = format!("icons/custom/{}", file_name);
    let custom_dir = format!("{}/icons/custom", assets_dir);
    tokio::fs::create_dir_all(&custom_dir).await.ok();
    if let Ok(custom_full) = resolve_asset_path(assets_dir, &custom_rel) {
        tokio::fs::write(&custom_full, &payload.svg_content).await.ok();
    }
    let stored_file_path = lobe_rel;

    let title = payload.title.unwrap_or_else(|| safe_name.clone());
    let category = payload.category.unwrap_or_else(|| "自定义".to_string());
    let tags_json = serde_json::to_string(&payload.tags.unwrap_or_default()).unwrap_or("[]".into());
    let now = DbTs::now();

    let icon: SiteIcon = sqlx::query_as(
        "INSERT INTO site_icons (name, title, file_path, source, category, tags, is_active, created_at, updated_at) \
         VALUES ($1, $2, $3, 'custom', $4, $5, 1, $6, $7) \
         ON CONFLICT (name, source) DO UPDATE SET title = $2, file_path = $3, category = $4, tags = $5, updated_at = $7 \
         RETURNING *"
    )
    .bind(&safe_name)
    .bind(&title)
    .bind(&stored_file_path)
    .bind(&category)
    .bind(&tags_json)
    .bind(&now)
    .bind(&now)
    .fetch_one(&state.db.pool)
    .await?;

    Ok(Json(json!({
        "success": true,
        "message": "图标添加成功",
        "data": icon
    })))
}

/// 读取图标 SVG 源码，供编辑页加载现有图形。
pub async fn get_icon_content(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    let icon: SiteIcon = sqlx::query_as("SELECT * FROM site_icons WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("图标不存在".into()))?;
    let svg = load_icon_svg(&state, &icon).await?;
    if !svg.to_lowercase().contains("<svg") {
        return Err(AppError::BadRequest("图标文件不是有效 SVG".into()));
    }
    Ok(Json(json!({
        "success": true,
        "data": svg
    })))
}

async fn load_icon_svg(state: &AppState, icon: &SiteIcon) -> Result<String, AppError> {
    let mut candidates = vec![
        format!("icons/lobe/{}.svg", icon.name),
        format!("icons/custom/{}.svg", icon.name),
    ];
    if !icon.file_path.starts_with("http://") && !icon.file_path.starts_with("https://") {
        candidates.push(icon.file_path.clone());
    }
    for rel in candidates {
        let Ok(full) = resolve_asset_path(&state.config.assets_dir, &rel) else {
            continue;
        };
        if let Ok(text) = tokio::fs::read_to_string(&full).await {
            if text.to_lowercase().contains("<svg") {
                return Ok(text);
            }
        }
    }
    Err(AppError::NotFound("图标文件不存在".into()))
}

/// 更新图标
pub async fn update_icon(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateSiteIconReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    let current: SiteIcon = sqlx::query_as("SELECT * FROM site_icons WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("图标不存在".into()))?;

    let name = if let Some(ref n) = payload.name {
        sanitize_icon_name(n)?
    } else {
        sanitize_icon_name(&current.name).unwrap_or_else(|_| current.name.clone())
    };
    let title = payload.title.unwrap_or(current.title);
    let category = payload.category.unwrap_or(current.category);
    let tags = payload
        .tags
        .map(|t| serde_json::to_string(&t).unwrap_or("[]".into()))
        .unwrap_or(current.tags);
    let is_active = payload.is_active.unwrap_or(current.is_active);
    let now = DbTs::now();

    let file_path = if let Some(ref svg) = payload.svg_content {
        validate_svg_content(svg)?;
        let file_name = format!("{}.svg", name);
        let assets_dir = &state.config.assets_dir;
        // 模型广场写死 /assets/icons/lobe/{name}.svg。展示路径必须是这份本地文件，
        // 不能改成云 URL：同名对象会被 CDN 缓存，列表和再次编辑读到的仍是旧图。
        let lobe_rel = format!("icons/lobe/{}", file_name);
        let lobe_dir = format!("{}/icons/lobe", assets_dir);
        tokio::fs::create_dir_all(&lobe_dir)
            .await
            .map_err(|e| AppError::Internal(format!("创建本地图标目录失败: {}", e)))?;
        let lobe_full = resolve_asset_path(assets_dir, &lobe_rel)?;
        tokio::fs::write(&lobe_full, svg)
            .await
            .map_err(|e| AppError::Internal(format!("写入本地 SVG 文件失败: {}", e)))?;

        if current.source == "custom" {
            let custom_rel = format!("icons/custom/{}", file_name);
            let custom_dir = format!("{}/icons/custom", assets_dir);
            tokio::fs::create_dir_all(&custom_dir).await.ok();
            if let Ok(custom_full) = resolve_asset_path(assets_dir, &custom_rel) {
                tokio::fs::write(&custom_full, svg).await.ok();
            }
        }
        lobe_rel
    } else {
        current.file_path
    };

    let updated: SiteIcon = sqlx::query_as(
        "UPDATE site_icons SET name = $1, title = $2, file_path = $3, category = $4, tags = $5, is_active = $6, updated_at = $7 WHERE id = $8 RETURNING *"
    )
    .bind(&name)
    .bind(&title)
    .bind(&file_path)
    .bind(&category)
    .bind(&tags)
    .bind(is_active)
    .bind(&now)
    .bind(id)
    .fetch_one(&state.db.pool)
    .await?;

    if payload.svg_content.is_some() {
        crate::api::plugins::get_marketplace_cache()
            .write()
            .await
            .invalidate();
    }

    Ok(Json(json!({
        "success": true,
        "message": "图标更新成功",
        "data": updated
    })))
}

/// 删除图标
pub async fn delete_icon(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    let icon: Option<SiteIcon> = sqlx::query_as("SELECT * FROM site_icons WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await?;

    if let Some(ref icon) = icon {
        // 1. 本地文件清理
        if let Ok(full) = resolve_asset_path(&state.config.assets_dir, &icon.file_path) {
            tokio::fs::remove_file(&full).await.ok();
        }
        // 如果是 custom 图标，也把 lobe 与 custom 目录下的双写备份清理掉
        if icon.source == "custom" {
            let lobe_path = format!("{}/icons/lobe/{}.svg", state.config.assets_dir, icon.name);
            tokio::fs::remove_file(&lobe_path).await.ok();
            let custom_path = format!("{}/icons/custom/{}.svg", state.config.assets_dir, icon.name);
            tokio::fs::remove_file(&custom_path).await.ok();
        }
        // 2. 手动添加的图标才有云端副本
        if icon.source == "custom" {
            if let Some(store) = icon_store(&state).await {
                let custom_key = store.full_key(&format!("icons/custom/{}.svg", icon.name));
                store.delete_file(&custom_key).await.ok();
                if icon.file_path.starts_with("http://") || icon.file_path.starts_with("https://")
                {
                    if let Some(key) = store.extract_object_key(&icon.file_path) {
                        store.delete_file(&key).await.ok();
                    }
                }
            }
        }
    }

    sqlx::query("DELETE FROM site_icons WHERE id = $1")
        .bind(id)
        .execute(&state.db.pool)
        .await?;

    Ok(Json(json!({
        "success": true,
        "message": "图标删除成功"
    })))
}

/// 清空图标库：数据库记录、本地 SVG、云端对象全部删除，不保留任何图标数据。
pub async fn reset_icon_library(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    if state.icon_sync_progress.is_running().await {
        return Err(AppError::BadRequest(
            "同步任务正在进行中，请待同步结束后再清空".into(),
        ));
    }

    let icons: Vec<SiteIcon> = sqlx::query_as("SELECT * FROM site_icons")
        .fetch_all(&state.db.pool)
        .await?;

    let local_failed = remove_local_icon_files(&state.config.assets_dir, &icons).await;
    let cloud_failed = purge_cloud_icons(&state, &icons).await;

    let deleted = sqlx::query("DELETE FROM site_icons")
        .execute(&state.db.pool)
        .await?
        .rows_affected();

    tracing::info!(
        "站点图标库已清空: db={} local_failed={} cloud_failed={}",
        deleted,
        local_failed,
        cloud_failed
    );

    let file_failed = local_failed + cloud_failed;
    let message = if file_failed == 0 {
        format!("已清空 {} 个图标", deleted)
    } else {
        format!("已清空 {} 个图标，部分文件删除失败", deleted)
    };

    Ok(Json(json!({
        "success": true,
        "message": message,
        "deleted": deleted,
        "local_failed": local_failed,
        "cloud_failed": cloud_failed
    })))
}

async fn remove_local_icon_files(assets_dir: &str, icons: &[SiteIcon]) -> usize {
    for icon in icons {
        let remote =
            icon.file_path.starts_with("http://") || icon.file_path.starts_with("https://");
        if !remote {
            if let Ok(full) = resolve_asset_path(assets_dir, &icon.file_path) {
                tokio::fs::remove_file(&full).await.ok();
            }
        }
        for rel in [
            format!("icons/lobe/{}.svg", icon.name),
            format!("icons/custom/{}.svg", icon.name),
        ] {
            if let Ok(full) = resolve_asset_path(assets_dir, &rel) {
                tokio::fs::remove_file(&full).await.ok();
            }
        }
    }
    let mut failed = 0usize;
    for sub in ["icons/lobe", "icons/custom"] {
        let Ok(dir) = resolve_asset_path(assets_dir, sub) else {
            failed += 1;
            continue;
        };
        if let Err(e) = tokio::fs::remove_dir_all(&dir).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                failed += 1;
                tracing::warn!("删除本地图标目录 {} 失败: {}", dir.display(), e);
            }
        }
    }
    failed
}

fn push_object_key(keys: &mut std::collections::HashSet<String>, key: &str) {
    let key = key.trim().trim_start_matches('/');
    if !key.is_empty() {
        keys.insert(key.to_string());
    }
}

async fn list_icon_prefix(
    store: &crate::services::object_store::ObjectStore,
    prefix: &str,
    keys: &mut std::collections::HashSet<String>,
) -> bool {
    match store.list_folder(prefix).await {
        Ok((objects, _)) => {
            for obj in objects {
                push_object_key(keys, &obj.key);
            }
            true
        }
        Err(e) => {
            tracing::warn!("列出图标对象存储 {} 失败: {}", prefix, e);
            false
        }
    }
}

async fn purge_cloud_icons(state: &AppState, icons: &[SiteIcon]) -> usize {
    let Some(legacy) = crate::relay::tos_persist::load_system_object_store(state).await else {
        return 0;
    };
    let store = legacy.clone().scoped_to_plugin(PLUGIN_NAME);
    let mut keys = std::collections::HashSet::<String>::new();
    let mut lobe_listed = true;
    let mut custom_listed = true;
    for s in [&store, &legacy] {
        lobe_listed &= list_icon_prefix(s, "icons/lobe/", &mut keys).await;
        custom_listed &= list_icon_prefix(s, "icons/custom/", &mut keys).await;
    }
    let mut failed = usize::from(!lobe_listed) + usize::from(!custom_listed);

    for icon in icons {
        if icon.file_path.starts_with("http://") || icon.file_path.starts_with("https://") {
            if let Some(key) = store.extract_object_key(&icon.file_path) {
                push_object_key(&mut keys, &key);
            }
        } else {
            let rel = icon.file_path.trim().trim_start_matches('/');
            let covered = (lobe_listed && rel.starts_with("icons/lobe/"))
                || (custom_listed && rel.starts_with("icons/custom/"));
            if !rel.is_empty() && !covered {
                push_object_key(&mut keys, &store.full_key(rel));
            }
        }
        if !lobe_listed {
            push_object_key(
                &mut keys,
                &store.full_key(&format!("icons/lobe/{}.svg", icon.name)),
            );
        }
        if !custom_listed {
            push_object_key(
                &mut keys,
                &store.full_key(&format!("icons/custom/{}.svg", icon.name)),
            );
        }
    }

    let key_list: Vec<String> = keys.into_iter().collect();
    for chunk in key_list.chunks(32) {
        let results =
            futures::future::join_all(chunk.iter().map(|key| store.delete_file(key))).await;
        for (key, res) in chunk.iter().zip(results) {
            if let Err(e) = res {
                failed += 1;
                tracing::warn!("删除图标对象 {} 失败: {}", key, e);
            }
        }
    }
    failed
}

/// 查看同步日志
pub async fn list_sync_logs(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    let logs: Vec<SiteIconSyncLog> =
        sqlx::query_as("SELECT * FROM site_icon_sync_logs ORDER BY id DESC LIMIT 50")
            .fetch_all(&state.db.pool)
            .await?;

    Ok(Json(json!({
        "success": true,
        "data": logs
    })))
}

/// 获取同步进度（前端轮询）
#[derive(serde::Deserialize)]
pub struct ProgressQuery {
    pub since: Option<usize>,
}

pub async fn get_sync_progress(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
    Query(params): Query<ProgressQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    let since = params.since.unwrap_or(0);
    let status = state.icon_sync_progress.get_status(since).await;
    Ok(Json(json!({
        "success": true,
        "data": status
    })))
}

/// 启动异步同步任务（立即返回）
pub async fn sync_from_github(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<auth::Claims>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &claims).await?;
    // 检查是否已有任务在运行
    if state.icon_sync_progress.is_running().await {
        return Err(AppError::BadRequest("同步任务正在进行中，请稍候".into()));
    }

    // spawn 后台任务
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = do_sync(state_clone).await {
            tracing::warn!("图标同步任务失败: {}", e);
        }
    });

    Ok(Json(json!({
        "success": true,
        "message": "同步任务已启动，请查看进度面板"
    })))
}

/// 实际同步逻辑（在后台线程执行）
async fn do_sync(state: Arc<AppState>) -> anyhow::Result<()> {
    let progress = &state.icon_sync_progress;
    let now = DbTs::now();

    // 1. 从 GitHub API 获取 src 目录列表
    progress
        .log("📡 正在请求 GitHub API 获取图标目录列表...".into())
        .await;

    let github_url = "https://api.github.com/repos/lobehub/lobe-icons/contents/src";
    let resp = match crate::services::http_client::with_download_timeout(
        state
            .http_client
            .get(github_url)
            .header("User-Agent", "TokensByte-IconSync/1.0")
            .header("Accept", "application/vnd.github.v3+json"),
    )
    .send()
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("请求 GitHub API 失败: {}", e);
            progress.fail(msg.clone()).await;
            write_sync_log(&state, 0, 0, 0, "failed", Some(&msg), &now).await;
            return Ok(());
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let msg = format!(
            "GitHub API 返回 {}: {}",
            status,
            &body[..body.len().min(200)]
        );
        progress.fail(msg.clone()).await;
        write_sync_log(&state, 0, 0, 0, "failed", Some(&msg), &now).await;
        return Ok(());
    }

    let dirs: Vec<serde_json::Value> = match resp.json().await {
        Ok(d) => d,
        Err(e) => {
            let msg = format!("解析 GitHub API 响应失败: {}", e);
            progress.fail(msg.clone()).await;
            write_sync_log(&state, 0, 0, 0, "failed", Some(&msg), &now).await;
            return Ok(());
        }
    };

    let icon_names: Vec<(String, String)> = dirs
        .iter()
        .filter(|d| d["type"].as_str() == Some("dir"))
        .filter_map(|d| {
            let name = d["name"].as_str()?.to_string();
            Some((name.clone(), name))
        })
        .collect();

    let total_count = icon_names.len() as i64;
    progress.start(total_count).await;
    progress
        .log(format!(
            "✅ GitHub API 返回成功，发现 {} 个图标目录",
            total_count
        ))
        .await;

    // 2. 创建 icons/lobe 目录
    let assets_dir = &state.config.assets_dir;
    let dir = format!("{}/icons/lobe", assets_dir);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        let msg = format!("创建图标目录失败: {}", e);
        progress.fail(msg.clone()).await;
        write_sync_log(&state, 0, 0, 0, "failed", Some(&msg), &now).await;
        return Ok(());
    }

    let mut total_synced = 0i64;
    let mut _total_new = 0i64;
    let mut total_skipped = 0i64;
    let mut errors: Vec<String> = Vec::new();

    // 3. 逐个下载 SVG
    for (idx, (original_name, _)) in icon_names.iter().enumerate() {
        let slug = original_name.to_lowercase();
        let cdn_color_url = format!(
            "https://unpkg.com/@lobehub/icons-static-svg@latest/icons/{}-color.svg",
            slug
        );
        let cdn_default_url = format!(
            "https://unpkg.com/@lobehub/icons-static-svg@latest/icons/{}.svg",
            slug
        );

        progress.progress((idx + 1) as i64).await;
        progress
            .log(format!(
                "⬇️  [{}/{}] 正在下载: {} ...",
                idx + 1,
                total_count,
                original_name
            ))
            .await;

        let mut svg_content = String::new();
        let mut download_success = false;

        // Try color version first
        if let Ok(resp) = crate::services::http_client::with_download_timeout(
            state
                .http_client
                .get(&cdn_color_url)
                .header("User-Agent", "TokensByte-IconSync/1.0"),
        )
        .send()
        .await
        {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if text.contains("<svg") {
                        svg_content = text;
                        download_success = true;
                    }
                }
            }
        }

        // Fallback to default version if color version failed
        if !download_success {
            if let Ok(resp) = crate::services::http_client::with_download_timeout(
                state
                    .http_client
                    .get(&cdn_default_url)
                    .header("User-Agent", "TokensByte-IconSync/1.0"),
            )
            .send()
            .await
            {
                if resp.status().is_success() {
                    if let Ok(text) = resp.text().await {
                        if text.contains("<svg") {
                            svg_content = text;
                            download_success = true;
                        }
                    }
                } else {
                    total_skipped += 1;
                    progress
                        .log(format!(
                            "⏭️  [{}/{}] {} CDN 返回 {}，已跳过",
                            idx + 1,
                            total_count,
                            original_name,
                            resp.status()
                        ))
                        .await;
                    continue;
                }
            } else {
                let err_msg = format!("{}: 下载请求失败", original_name);
                progress.log(format!("⚠️  {}", err_msg)).await;
                errors.push(err_msg);
                continue;
            }
        }

        if !download_success {
            total_skipped += 1;
            progress
                .log(format!(
                    "⏭️  [{}/{}] {} 无效的 SVG，已跳过",
                    idx + 1,
                    total_count,
                    original_name
                ))
                .await;
            continue;
        }

        let file_name = format!("{}.svg", slug);
        let file_path = format!("icons/lobe/{}", file_name);
        let full_path = format!("{}/{}", assets_dir, file_path);

        if let Err(e) = tokio::fs::write(&full_path, &svg_content).await {
            let err_msg = format!("{}: 写入文件失败 {}", original_name, e);
            progress.log(format!("⚠️  {}", err_msg)).await;
            errors.push(err_msg);
            continue;
        }

        let result = sqlx::query(
            "INSERT INTO site_icons (name, title, file_path, source, category, tags, is_active, created_at, updated_at) \
             VALUES ($1, $2, $3, 'lobe-icons', 'AI品牌', '[]', 1, $4, $5) \
             ON CONFLICT (name, source) DO UPDATE SET title = $2, file_path = $3, updated_at = $5"
        )
        .bind(&slug)
        .bind(original_name)
        .bind(&file_path)
        .bind(&now)
        .bind(&now)
        .execute(&state.db.pool)
        .await;

        match result {
            Ok(_) => {
                total_synced += 1;
                _total_new += 1;
                progress
                    .log(format!(
                        "✅ [{}/{}] {} 同步成功",
                        idx + 1,
                        total_count,
                        original_name
                    ))
                    .await;
            }
            Err(e) => {
                let err_msg = format!("{}: 数据库写入失败 {}", original_name, e);
                progress.log(format!("⚠️  {}", err_msg)).await;
                errors.push(err_msg);
            }
        }
    }

    // 统计
    let existing_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM site_icons WHERE source = 'lobe-icons'")
            .fetch_one(&state.db.pool)
            .await
            .unwrap_or(0);

    let status_str = if errors.is_empty() {
        "success"
    } else {
        "partial"
    };
    let error_msg = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    let summary = format!(
        "🎉 同步完成！成功 {} 个，跳过 {} 个，失败 {} 个，库中共 {} 个图标",
        total_synced,
        total_skipped,
        errors.len(),
        existing_count
    );
    progress.finish(summary.clone()).await;

    write_sync_log(
        &state,
        total_synced,
        existing_count,
        total_synced,
        status_str,
        error_msg.as_deref(),
        &now,
    )
    .await;

    tracing::info!("{}", summary);
    Ok(())
}

/// 启动时自动检查图标文件完整性，缺失则自动恢复同步
pub async fn auto_recover_on_startup(state: Arc<AppState>) {
    // 检查插件是否已启用
    let enabled: Option<i64> =
        sqlx::query_scalar("SELECT is_enabled FROM plugins WHERE name = 'site_icons'")
            .fetch_optional(&state.db.pool)
            .await
            .ok()
            .flatten();

    if enabled != Some(1) {
        return; // 插件未启用，跳过
    }

    // 查询数据库中 lobe-icons 来源的图标数量
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM site_icons WHERE source = 'lobe-icons'")
            .fetch_one(&state.db.pool)
            .await
            .unwrap_or(0);

    if total == 0 {
        return; // 数据库中没有图标记录，无需检查
    }

    // 抽样检查前 20 个图标的文件是否存在
    let sample_paths: Vec<String> = sqlx::query_scalar(
        "SELECT file_path FROM site_icons WHERE source = 'lobe-icons' ORDER BY name ASC LIMIT 20",
    )
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    let mut missing = 0;
    let assets_dir = &state.config.assets_dir;
    for fp in &sample_paths {
        let full = format!("{}/{}", assets_dir, fp);
        if tokio::fs::metadata(&full).await.is_err() {
            missing += 1;
        }
    }

    // 超过半数文件缺失，触发自动恢复同步
    if missing > sample_paths.len() / 2 {
        tracing::warn!(
            "🔄 站点图标文件缺失 ({}/{} 抽样缺失, 数据库共 {} 条)，自动触发恢复同步...",
            missing,
            sample_paths.len(),
            total
        );
        let state_clone = state.clone();
        tokio::spawn(async move {
            // 等待 5 秒让服务完全就绪
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if let Err(e) = do_sync(state_clone).await {
                tracing::warn!("图标自动恢复同步失败: {}", e);
            }
        });
    } else {
        tracing::info!(
            "✅ 站点图标文件完整性检查通过 ({} 条记录, {}/{} 抽样文件存在)",
            total,
            sample_paths.len() - missing,
            sample_paths.len()
        );
    }
}

/// 写入同步日志到数据库
async fn write_sync_log(
    state: &Arc<AppState>,
    total_synced: i64,
    total_new: i64,
    total_updated: i64,
    status: &str,
    error_message: Option<&str>,
    now: &DbTs,
) {
    sqlx::query(
        "INSERT INTO site_icon_sync_logs (total_synced, total_new, total_updated, status, error_message, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(total_synced)
    .bind(total_new)
    .bind(total_updated)
    .bind(status)
    .bind(error_message)
    .bind(now)
    .execute(&state.db.pool)
    .await
    .ok();
}
