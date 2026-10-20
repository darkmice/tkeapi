/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! Backend plugins — mirrors `frontend/src/pages/Plugins/`.
//! Each plugin lives in its own folder (code + i18n/docs resources).
//!
//! Layout (aligned with frontend):
//! - `manager/`              → PluginConfig / PluginsList / ModelMarketplace（扩展规范见 manager/mod.rs 模块注释）
//! - `docs_api/`             → DocsApi (+ `default_docs/` markdown)
//! - `playground/`           → Playground
//! - `playground_2026/`      → Playground_2026
//! - `assets/`               → AssetManager / UserAssets
//! - `site_portal/`          → SitePortal
//! - `site_portal_pro/`      → SitePortalPro (+ `docs`)
//! - `team_marketing/`       → TeamMarketing
//! - `upstream_asset_relay/` → UpstreamAssetRelay
//! - `volc_ark_monitor/`     → VolcengineArkMonitor
//! - `comfyui_bridge/`       → ComfyUiBridge
//! - `content_security/`     → ContentSecurity
//! - `volc_enhance/`         → 火山引擎 AI MediaKit 增强
//! - `tencent_enhance/`      → 腾讯云增强
//! - `site_icons/`           → SiteIcons
//! - `mobile_app/`           → 移动端APP管理
//! - `marketing_membership/` → 营销卡券会员
//! - `support_tickets/`     → 工单
//! - `data_sync/`            → DataSync
//! - `finance/` / `pay/`（含 `pay/payment/` 渠道客户端）/ `redemptions/` → 可选插件（目录存在即编译）

// Plugin manager (marketplace / config / TOS / 列表开关)
pub(crate) mod manager;
pub use manager::*;

#[cfg(feature = "plugin_volcengine_enhance")]
pub mod volc_enhance;
#[cfg(feature = "plugin_volcengine_enhance")]
pub use volc_enhance::{
    apply_volc_enhance_path, build_volcengine_media_enhance_body, crop_volc_cascade_video,
    extract_volc_last_frame, infer_volc_media_enhance, is_image_tool_mid, is_volc_enhance_version,
    is_volc_preset_mid, link_volcengine_enhance_log,
    lock_volc_cascade_s2_channel, poll_volc_cascade_stage2, resolve_volc_image_model,
    resolve_volc_video_model, submit_volc_cascade_stage2, transcode_volc_mov, volc_cascade_s1_tag,
    volc_inactive_filter_sql, volc_preset_blocked, volc_s2_from_tag,
};

#[cfg(feature = "plugin_tencent_enhance")]
pub mod tencent_enhance;
#[cfg(feature = "plugin_tencent_enhance")]
pub use tencent_enhance::{
    cleanup_tencent_enhance_logs, crop_tencent_cascade_video, extract_tencent_last_frame,
    poll_tencent_cascade_stage2, submit_tencent_cascade_stage2, tencent_cascade_s1_tag,
    transcode_tencent_mov,
};

// Always-on plugins
pub mod docs_api;

/// 插件域能力判定：当前模型是否为插件提供的图像封装工具（画质增强/背景移除等）
#[inline]
pub fn is_plugin_image_tool(model: &crate::models::Model) -> bool {
    #[cfg(feature = "plugin_volcengine_enhance")]
    {
        is_image_tool_mid(&model.mid)
            || is_image_tool_mid(&model.model_id)
            || model.library_mid.as_deref().is_some_and(is_image_tool_mid)
    }
    #[cfg(not(feature = "plugin_volcengine_enhance"))]
    {
        let _ = model;
        false
    }
}
pub mod playground;

// Feature-gated plugins
#[cfg(feature = "commercial_plugins")]
pub mod assets;
#[cfg(feature = "commercial_plugins")]
pub mod logs_archive;
#[cfg(feature = "commercial_plugins")]
pub mod marketing_membership;
#[cfg(feature = "commercial_plugins")]
pub mod mobile_app;
#[cfg(feature = "commercial_plugins")]
pub mod playground_2026;
#[cfg(feature = "commercial_plugins")]
pub mod site_portal_pro;
#[cfg(feature = "commercial_plugins")]
pub use site_portal_pro::docs as site_portal_pro_docs;
#[cfg(feature = "commercial_plugins")]
pub use site_portal_pro::articles as site_portal_pro_articles;
#[cfg(feature = "commercial_plugins")]
pub mod support_tickets;
#[cfg(all(feature = "commercial_plugins", plugin_team_marketing))]
pub mod team_marketing;
#[cfg(feature = "commercial_plugins")]
pub mod upstream_asset_relay;
#[cfg(feature = "commercial_plugins")]
pub mod volc_ark_monitor;

#[cfg(feature = "plugin_content_security")]
pub mod content_security;

#[cfg(feature = "plugin_comfyui")]
pub mod comfyui_bridge;
#[cfg(feature = "plugin_data_sync")]
pub mod data_sync;
#[cfg(feature = "plugin_site_icons")]
pub mod site_icons;
#[cfg(feature = "plugin_site_portal")]
pub mod site_portal;

// Optional plugins (presence detected by build.rs → rustc-cfg)
#[cfg(plugin_finance)]
pub mod finance;
#[cfg(all(feature = "commercial_plugins", plugin_marketing_games))]
pub mod marketing_games;
#[cfg(all(plugin_pay, plugin_payment))]
pub mod pay;
#[cfg(plugin_redemptions)]
pub mod redemptions;
