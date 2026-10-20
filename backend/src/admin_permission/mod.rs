/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

mod policy;
mod route_map;

use policy::permission_key_allowed;
pub use policy::AdminGroupPermissionPolicy;
use route_map::{classify_admin_write, WriteDecision};

use std::sync::Arc;

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::models::User;
use crate::AppState;

#[derive(Clone)]
pub struct AdminContext {
    is_super_admin: bool,
    view: Vec<String>,
    edit: Vec<String>,
}

pub fn is_super_admin(role: &str, admin_group_id: Option<i64>) -> bool {
    role == "admin" && admin_group_id.is_none()
}

pub fn users_write_perm(role: &str) -> &'static str {
    if role == "admin" {
        "users.admins"
    } else {
        "users.list"
    }
}

pub(crate) async fn load_admin_context(
    state: &Arc<AppState>,
    user_id: &str,
) -> AppResult<AdminContext> {
    let row: Option<(String, Option<i64>, Option<String>)> =
        sqlx::query_as(&state.db.format_query(
            "SELECT u.role, u.admin_group_id, g.permissions \
             FROM users u LEFT JOIN admin_groups g ON g.id = u.admin_group_id \
             WHERE u.id = ?",
        ))
        .bind(user_id)
        .fetch_optional(&state.db.pool)
        .await?;

    let Some((role, admin_group_id, raw)) = row else {
        return Err(AppError::Unauthorized);
    };
    if role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }
    if admin_group_id.is_none() {
        return Ok(AdminContext {
            is_super_admin: true,
            view: vec![],
            edit: vec![],
        });
    }
    let policy = AdminGroupPermissionPolicy::parse(raw.as_deref());
    Ok(AdminContext {
        is_super_admin: false,
        view: policy.view,
        edit: policy.edit,
    })
}

pub async fn is_super_admin_user(state: &Arc<AppState>, user_id: &str) -> AppResult<bool> {
    let row: Option<(String, Option<i64>)> = sqlx::query_as(
        &state
            .db
            .format_query("SELECT role, admin_group_id FROM users WHERE id = ?"),
    )
    .bind(user_id)
    .fetch_optional(&state.db.pool)
    .await?;
    Ok(row.is_some_and(|(role, gid)| is_super_admin(&role, gid)))
}

/// 读权限判定（GET 接口字段级过滤用）；`|` 分隔任一满足即可
pub fn can_view(ctx: &AdminContext, perm_key: &str) -> bool {
    ctx.is_super_admin
        || perm_key
            .split('|')
            .any(|k| permission_key_allowed(&ctx.view, k))
}

pub fn require_edit(ctx: &AdminContext, perm_key: &str) -> AppResult<()> {
    if ctx.is_super_admin
        || perm_key
            .split('|')
            .any(|k| permission_key_allowed(&ctx.edit, k))
    {
        Ok(())
    } else {
        Err(AppError::Forbidden("无编辑权限".to_string()))
    }
}

pub async fn require_admin_edit(
    state: &Arc<AppState>,
    claims: &auth::Claims,
    perm_key: &str,
) -> AppResult<()> {
    require_edit(&load_admin_context(state, &claims.sub).await?, perm_key)
}

pub async fn hydrate_user_admin_permissions(
    state: &Arc<AppState>,
    user: &mut User,
) -> AppResult<()> {
    if user.role != "admin" {
        user.permissions = None;
        user.edit_permissions = None;
        return Ok(());
    }
    if user.admin_group_id.is_none() {
        user.permissions = Some(vec![]);
        user.edit_permissions = Some(vec![]);
        return Ok(());
    }
    let raw: Option<String> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT permissions FROM admin_groups WHERE id = ?"),
    )
    .bind(user.admin_group_id)
    .fetch_optional(&state.db.pool)
    .await?;
    let policy = AdminGroupPermissionPolicy::parse(raw.as_deref());
    user.permissions = Some(policy.view);
    user.edit_permissions = Some(policy.edit);
    Ok(())
}

pub async fn assert_can_assign_admin_group(
    state: &Arc<AppState>,
    admin_group_id: Option<i64>,
    target_role: &str,
) -> AppResult<()> {
    if target_role != "admin" {
        return Ok(());
    }
    let Some(gid) = admin_group_id else {
        return Err(AppError::BadRequest(
            "管理员必须指定管理员等级，不可设为超级管理员".to_string(),
        ));
    };
    let exists: bool = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT EXISTS(SELECT 1 FROM admin_groups WHERE id = ?)"),
    )
    .bind(gid)
    .fetch_one(&state.db.pool)
    .await?;
    if exists {
        Ok(())
    } else {
        Err(AppError::BadRequest("管理员等级不存在".to_string()))
    }
}

/// 创建后不可在 admin / user 之间切换；未传 role 视为不改。
pub fn assert_role_immutable(current_role: &str, new_role: Option<&str>) -> AppResult<()> {
    if new_role.is_some_and(|r| r != current_role) {
        return Err(AppError::BadRequest("用户角色不可修改".to_string()));
    }
    Ok(())
}

pub fn assert_super_admin_immutable(
    current_role: &str,
    current_group: Option<i64>,
    new_role: Option<&str>,
    new_admin_group_id: Option<Option<i64>>,
) -> AppResult<()> {
    if is_super_admin(current_role, current_group) {
        if new_role.is_some_and(|r| r != "admin") {
            return Err(AppError::BadRequest("超级管理员角色不可修改".to_string()));
        }
        if new_admin_group_id.is_some_and(|g| g.is_some()) {
            return Err(AppError::BadRequest("超级管理员等级不可修改".to_string()));
        }
        return Ok(());
    }
    if new_admin_group_id == Some(None) && current_role == "admin" {
        return Err(AppError::BadRequest(
            "不可将管理员设为超级管理员".to_string(),
        ));
    }
    Ok(())
}

pub async fn enforce_admin_write(
    state: &Arc<AppState>,
    claims: &auth::Claims,
    path: &str,
) -> AppResult<Option<AdminContext>> {
    match classify_admin_write(path) {
        WriteDecision::Allow => Ok(None),
        WriteDecision::Require {
            perm_key,
            super_only,
        } => {
            let ctx = load_admin_context(state, &claims.sub).await?;
            if ctx.is_super_admin {
                return Ok(Some(ctx));
            }
            if super_only {
                return Err(AppError::Forbidden("仅超级管理员可执行此操作".to_string()));
            }
            if perm_key.is_empty() {
                tracing::warn!("admin write path missing permission map: {}", path);
                return Err(AppError::Forbidden("无编辑权限".to_string()));
            }
            require_edit(&ctx, &perm_key)?;
            Ok(Some(ctx))
        }
    }
}
