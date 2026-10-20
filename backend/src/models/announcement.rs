/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::time_system::DbTs;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Announcement {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub is_pinned: i64,
    pub is_popup: i64,
    pub is_active: i64,
    pub sort_order: i64,
    pub pin_expires_at: Option<DbTs>,
    pub popup_expires_at: Option<DbTs>,
    pub active_expires_at: Option<DbTs>,
    pub display_time_mode: String,
    /// 逗号分隔，可多选：pay_modal,console,playground_2026,portal_pro
    pub target_module: String,
    pub playground_banner_bg: Option<String>,
    pub portal_banner_bg: Option<String>,
    pub created_at: DbTs,
    pub updated_at: DbTs,
}

#[derive(Debug, Deserialize)]
pub struct CreateAnnouncementReq {
    pub title: String,
    pub content: String,
    pub is_pinned: i64,
    pub is_popup: Option<i64>,
    pub is_active: i64,
    pub sort_order: Option<i64>,
    pub pin_expires_at: Option<String>,
    pub popup_expires_at: Option<String>,
    pub active_expires_at: Option<String>,
    pub display_time_mode: Option<String>,
    pub target_module: Option<String>,
    pub playground_banner_bg: Option<String>,
    pub portal_banner_bg: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAnnouncementReq {
    pub title: Option<String>,
    pub content: Option<String>,
    pub is_pinned: Option<i64>,
    pub is_popup: Option<i64>,
    pub is_active: Option<i64>,
    pub sort_order: Option<i64>,
    pub pin_expires_at: Option<String>,
    pub popup_expires_at: Option<String>,
    pub active_expires_at: Option<String>,
    pub display_time_mode: Option<String>,
    pub target_module: Option<String>,
    pub playground_banner_bg: Option<String>,
    pub portal_banner_bg: Option<String>,
}
