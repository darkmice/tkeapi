/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteDecision {
    Allow,
    Require { perm_key: String, super_only: bool },
}

fn strip_api_prefix(path: &str) -> &str {
    path.strip_prefix("/api/v1").unwrap_or(path)
}

fn is_unrestricted_write_path(path: &str) -> bool {
    path.contains("/user/")
        || path.starts_with("/assets/upload")
        || path.starts_with("/team-marketing/referral/")
        || path.starts_with("/team-marketing/member/")
        || path == "/team-marketing/join"
}

fn is_read_post(path: &str) -> bool {
    matches!(
        path,
        "/users/consumption/stats_batch"
            | "/finance/recharges/stats_batch"
            | "/channel-configs/upstream-groups"
            | "/settings/database/verify"
            | "/settings/storage/test"
            | "/settings/storage/buckets"
    ) || path.ends_with("/test-connection")
        || path.ends_with("/list-buckets")
        || path.ends_with("/test-volcengine-connection")
        || path.ends_with("/preview")
        || path.ends_with("/ping")
}

fn starts_with_seg(path: &str, prefix: &str) -> bool {
    path == prefix || (path.starts_with(prefix) && path.as_bytes().get(prefix.len()) == Some(&b'/'))
}

const CORE_PREFIXES: &[(&str, &str, bool)] = &[
    ("/admin_groups", "", true),
    ("/users", "users.list|users.admins", false), // | = 任一即可进中间件，细粒度在 handler
    ("/user_levels", "users.levels", false),
    ("/channel-configs", "channels.configs", false),
    ("/channel-categories", "channels.groups", false),
    ("/channels", "channels.groups", false),
    ("/upstreams", "channels.configs", false),
    ("/model-api-providers", "models.list", false),
    ("/model-providers", "models.list", false),
    ("/model-types", "models.list", false),
    ("/models", "models.list", false),
    ("/forward-rules", "models.forward_rules", false),
    ("/billing-rules", "models.billing_rules", false),
    ("/settings/database", "settings.database", false),
    ("/settings/storage", "settings.database", false),
    ("/settings/email", "settings.message_notification", false),
    ("/settings/sms", "settings.message_notification", false),
    ("/settings/notification", "settings.message_notification", false),
    ("/settings/usage-stats", "settings.database", false),
    ("/settings/currency", "settings.payment", false),
    ("/settings/upload", "settings.basic", false),
    (
        "/settings",
        "settings.basic|settings.payment|settings.message_notification|settings.oauth|settings.database|marketing.redemptions|marketing.registration_gifts",
        false,
    ),
    ("/announcements", "marketing.announcements", false),
    ("/redemptions", "marketing.redemptions", false),
    ("/finance/invoices", "finance.invoices", false),
    ("/finance", "finance.recharges", false),
    ("/team-marketing/teams", "plugin:team_marketing", false),
    ("/team-marketing/theme-promos", "plugin:team_marketing", false),
    ("/assets/admin", "plugin:asset_manager", false),
];

fn plugin_perm_key(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/plugins/")?;
    let slug = rest.split('/').next().filter(|s| !s.is_empty())?;
    let name = match slug {
        "site-portal" => "site_portal",
        "site-portal-pro" => "site_portal_pro",
        "docs-api" => "docs_api",
        "site-icons" => "site_icons",
        _ => slug,
    };
    Some(format!("plugin:{name}"))
}

pub fn classify_admin_write(full_path: &str) -> WriteDecision {
    let path = strip_api_prefix(full_path);
    if is_unrestricted_write_path(path) || is_read_post(path) {
        return WriteDecision::Allow;
    }
    for (prefix, key, super_only) in CORE_PREFIXES {
        if starts_with_seg(path, prefix) {
            return WriteDecision::Require {
                perm_key: (*key).to_string(),
                super_only: *super_only,
            };
        }
    }
    if let Some(key) = plugin_perm_key(path) {
        return WriteDecision::Require {
            perm_key: key,
            super_only: false,
        };
    }
    WriteDecision::Require {
        perm_key: String::new(),
        super_only: false,
    }
}
