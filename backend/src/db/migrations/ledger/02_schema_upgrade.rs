/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use crate::db::migrations::once_migration;
use sqlx::PgPool;
use std::collections::HashSet;

pub async fn run(pool: &PgPool, done: &mut HashSet<String>) -> anyhow::Result<()> {
    // ─── 统一将所有 INTEGER 列升级为 BIGINT，与 Rust 模型层 i64 对齐 ───
    once_migration!(
        pool,
        done,
        "upgrade_columns_to_bigint_v1",
        // ── user_levels ──
        "ALTER TABLE user_levels ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE user_levels ALTER COLUMN daily_invite_limit TYPE BIGINT",
        "ALTER TABLE user_levels ALTER COLUMN marketing_enabled TYPE BIGINT",
        "ALTER TABLE user_levels ALTER COLUMN is_default TYPE BIGINT",
        "ALTER TABLE user_levels ALTER COLUMN max_token_count TYPE BIGINT",
        // ── users ──
        "ALTER TABLE users ALTER COLUMN is_active TYPE BIGINT",
        "ALTER TABLE users ALTER COLUMN admin_group_id TYPE BIGINT",
        // ── api_tokens ──
        "ALTER TABLE api_tokens ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE api_tokens ALTER COLUMN is_active TYPE BIGINT",
        // ── channels ──
        "ALTER TABLE channels ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE channels ALTER COLUMN preset_id TYPE BIGINT",
        "ALTER TABLE channels ALTER COLUMN pool_id TYPE BIGINT",
        "ALTER TABLE channels ALTER COLUMN gptimage_pool_id TYPE BIGINT",
        // ── logs ──
        "ALTER TABLE logs ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE logs ALTER COLUMN channel_id TYPE BIGINT",
        "ALTER TABLE logs ALTER COLUMN token_id TYPE BIGINT",
        // ── channel_configs ──
        "ALTER TABLE channel_configs ALTER COLUMN id TYPE BIGINT",
        // ── admin_groups ──
        "ALTER TABLE admin_groups ALTER COLUMN id TYPE BIGINT",
        // ── plugins ──
        "ALTER TABLE plugins ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE plugins ALTER COLUMN is_enabled TYPE BIGINT",
        // ── site_icons ──
        "ALTER TABLE site_icons ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE site_icons ALTER COLUMN is_active TYPE BIGINT",
        // ── site_icon_sync_logs ──
        "ALTER TABLE site_icon_sync_logs ALTER COLUMN total_synced TYPE BIGINT",
        "ALTER TABLE site_icon_sync_logs ALTER COLUMN total_new TYPE BIGINT",
        "ALTER TABLE site_icon_sync_logs ALTER COLUMN total_updated TYPE BIGINT",
        // ── redemptions（原误写为 redemption_codes，已修正） ──
        "ALTER TABLE redemptions ALTER COLUMN id TYPE BIGINT",
        // ── models ──
        "ALTER TABLE models ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE models ALTER COLUMN provider_id TYPE BIGINT",
        "ALTER TABLE models ALTER COLUMN type_id TYPE BIGINT",
        "ALTER TABLE models ALTER COLUMN billing_rule_id TYPE BIGINT",
        // ── model_providers ──
        "ALTER TABLE model_providers ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE model_providers ADD COLUMN IF NOT EXISTS name_en TEXT NOT NULL DEFAULT ''",
        // ── model_types ──
        "ALTER TABLE model_types ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE model_types ADD COLUMN IF NOT EXISTS name_en TEXT NOT NULL DEFAULT ''",
        // ── forward_rules ──
        "ALTER TABLE forward_rules ALTER COLUMN id TYPE BIGINT",
        // ── billing_rules ──
        "ALTER TABLE billing_rules ALTER COLUMN id TYPE BIGINT",
        // ── recharge_records ──
        "ALTER TABLE recharge_records ALTER COLUMN id TYPE BIGINT",
        // ── orders ──
        "ALTER TABLE orders ALTER COLUMN id TYPE BIGINT",
        // ── upstreams ──
        "ALTER TABLE upstreams ALTER COLUMN id TYPE BIGINT",
        // ── announcements ──
        "ALTER TABLE announcements ALTER COLUMN id TYPE BIGINT",
        // ── verification_codes ──
        "ALTER TABLE verification_codes ALTER COLUMN id TYPE BIGINT",
        // ── volcengine_pools（主表） ──
        "ALTER TABLE volcengine_pools ALTER COLUMN id TYPE BIGINT",
        // ── volcengine_pool_accounts ──
        "ALTER TABLE volcengine_pool_accounts ALTER COLUMN id TYPE BIGINT",
        // ── volcengine_pool_account_mapping ──
        "ALTER TABLE volcengine_pool_account_mapping ALTER COLUMN pool_id TYPE BIGINT",
        "ALTER TABLE volcengine_pool_account_mapping ALTER COLUMN account_id TYPE BIGINT",
        // ── volcengine_pool_logs ──
        "ALTER TABLE volcengine_pool_logs ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE volcengine_pool_logs ALTER COLUMN pool_id TYPE BIGINT",
        "ALTER TABLE volcengine_pool_logs ALTER COLUMN account_id TYPE BIGINT",
        "ALTER TABLE volcengine_pool_logs ALTER COLUMN channel_id TYPE BIGINT",
        // ── gptimage_pools（主表） ──
        "ALTER TABLE gptimage_pools ALTER COLUMN id TYPE BIGINT",
        // ── gptimage_pool_accounts ──
        "ALTER TABLE gptimage_pool_accounts ALTER COLUMN id TYPE BIGINT",
        // ── gptimage_pool_account_mapping ──
        "ALTER TABLE gptimage_pool_account_mapping ALTER COLUMN pool_id TYPE BIGINT",
        "ALTER TABLE gptimage_pool_account_mapping ALTER COLUMN account_id TYPE BIGINT",
        // ── gptimage_pool_logs ──
        "ALTER TABLE gptimage_pool_logs ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE gptimage_pool_logs ALTER COLUMN pool_id TYPE BIGINT",
        "ALTER TABLE gptimage_pool_logs ALTER COLUMN account_id TYPE BIGINT",
        "ALTER TABLE gptimage_pool_logs ALTER COLUMN channel_id TYPE BIGINT",
        // ── playground_projects（体验中心项目） ──
        "ALTER TABLE playground_projects ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE playground_projects ALTER COLUMN is_deleted TYPE BIGINT",
        // ── playground_assets（体验中心资源） ──
        "ALTER TABLE playground_assets ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE playground_assets ALTER COLUMN project_id TYPE BIGINT",
        "ALTER TABLE playground_assets ALTER COLUMN width TYPE BIGINT",
        "ALTER TABLE playground_assets ALTER COLUMN height TYPE BIGINT",
        "ALTER TABLE playground_assets ALTER COLUMN is_deleted TYPE BIGINT",
        // ── plugin_api_logs ──
        "ALTER TABLE plugin_api_logs ALTER COLUMN id TYPE BIGINT",
        // ── plugin_configs ──
        "ALTER TABLE plugin_configs ALTER COLUMN id TYPE BIGINT",
        // ── plugin_asset_groups ──
        "ALTER TABLE plugin_asset_groups ALTER COLUMN id TYPE BIGINT",
        // ── plugin_assets ──
        "ALTER TABLE plugin_assets ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE plugin_assets ALTER COLUMN size TYPE BIGINT",
        "ALTER TABLE plugin_assets ALTER COLUMN sort_order TYPE BIGINT",
        // ── marketing_teams ──
        "ALTER TABLE marketing_teams ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE marketing_teams ALTER COLUMN max_members TYPE BIGINT",
        "ALTER TABLE marketing_teams ALTER COLUMN members_can_set_level TYPE BIGINT",
        "ALTER TABLE marketing_teams ALTER COLUMN leader_can_remove_members TYPE BIGINT",
        // ── marketing_team_leaders ──
        "ALTER TABLE marketing_team_leaders ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE marketing_team_leaders ALTER COLUMN team_id TYPE BIGINT",
        // ── marketing_team_members ──
        "ALTER TABLE marketing_team_members ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE marketing_team_members ALTER COLUMN team_id TYPE BIGINT",
        // ── site_icon_sync_logs ──
        "ALTER TABLE site_icon_sync_logs ALTER COLUMN id TYPE BIGINT",
        // ── router_flow_groups ──
        "ALTER TABLE router_flow_groups ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE router_flow_groups ALTER COLUMN is_active TYPE BIGINT",
    );

    // recharge_records 新增 operator 字段（记录后台操作人员）和 wallet_type 字段（区分所属钱包），受一次性迁移保护
    once_migration!(pool, done, "recharge_records_wallet_fields_v1",
        "ALTER TABLE recharge_records ADD COLUMN IF NOT EXISTS operator TEXT DEFAULT ''",
        "COMMENT ON COLUMN recharge_records.operator IS '操作人员用户名，后台手动操作时记录'",
        "ALTER TABLE recharge_records ADD COLUMN IF NOT EXISTS wallet_type TEXT NOT NULL DEFAULT 'system'",
        "COMMENT ON COLUMN recharge_records.wallet_type IS '所属钱包类型: system=系统钱包, gift=赠送钱包'"
    );

    // 迁移历史数据（受一次性迁移保护，仅执行一次，避免大表全扫描引起卡顿）
    // 1. 原 recharge_type='gift' 的记录归入赠送钱包
    // 2. 补充修复：registration（注册赠送）和 commission（邀请奖励）类型实际写入赠送钱包，
    //    但早期未正确设置 wallet_type，导致赠送钱包明细为空但余额不为零的数据不一致
    once_migration!(pool, done, "backfill_recharge_wallet_type_v1",
        "UPDATE recharge_records SET wallet_type = 'gift' WHERE recharge_type = 'gift' AND wallet_type = 'system'",
        "UPDATE recharge_records SET wallet_type = 'gift' WHERE recharge_type IN ('registration', 'commission') AND wallet_type = 'system'"
    );

    // ══════════════════════════════════════════════════════════════
    //  API服务商 (API Providers) 支持
    // ══════════════════════════════════════════════════════════════
    // ── 初始化API服务商与临时文件折扣锁定参数等表结构（受一次性迁移保护） ──
    once_migration!(pool, done, "init_api_providers_tables_v1",
        r#"CREATE TABLE IF NOT EXISTS model_api_providers (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            name_en TEXT NOT NULL DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            is_system INTEGER NOT NULL DEFAULT 0,
            remark TEXT,
            logo TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "COMMENT ON TABLE model_api_providers IS 'API服务商表（提供接口的服务商，区别于官方服务商）'",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS api_provider_id BIGINT REFERENCES model_api_providers(id)",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category)
           VALUES ('site_portal', '站点门户', '提供站点内容的基本介绍，支持生成静态HTML页面用于SEO/GEO优化', 0, 'user')
           ON CONFLICT (name) DO NOTHING"#,
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS only_playground BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE api_tokens ALTER COLUMN only_playground TYPE BIGINT",
        "COMMENT ON COLUMN api_tokens.only_playground IS '是否仅限创作中心使用，1=是，0=否'",
        r#"CREATE TABLE IF NOT EXISTS tos_temp_files (
            id SERIAL PRIMARY KEY,
            object_key TEXT NOT NULL,
            channel_id INTEGER NOT NULL DEFAULT 0,
            source TEXT NOT NULL DEFAULT 'channel',
            expire_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_tos_temp_files_expire ON tos_temp_files (expire_at)",
        "ALTER TABLE tos_temp_files ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE tos_temp_files ALTER COLUMN channel_id TYPE BIGINT",
        "COMMENT ON TABLE tos_temp_files IS 'TOS临时文件过期追踪'",
        "COMMENT ON COLUMN tos_temp_files.object_key IS 'TOS对象键'",
        "COMMENT ON COLUMN tos_temp_files.channel_id IS '来源渠道ID'",
        "COMMENT ON COLUMN tos_temp_files.source IS '业务来源(channel=渠道存储)'",
        "COMMENT ON COLUMN tos_temp_files.expire_at IS '过期时间(ISO 8601)'",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS model_discounts TEXT",
        "COMMENT ON COLUMN users.model_discounts IS '用户模型单独折扣(JSON: {mid: discount}), 优先于等级折扣, 受模型折扣限价约束'",
        r#"CREATE TABLE IF NOT EXISTS user_model_configs (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            model_mid TEXT NOT NULL,
            param_values TEXT NOT NULL DEFAULT '{}',
            is_locked INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text),
            UNIQUE(user_id, model_mid)
        )"#,
        "ALTER TABLE user_model_configs ALTER COLUMN id TYPE bigint",
        "CREATE INDEX IF NOT EXISTS idx_user_model_configs_user ON user_model_configs(user_id)",
        "COMMENT ON TABLE user_model_configs IS '用户在模型创作中心锁定的模型自定义参数配置'",
        "COMMENT ON COLUMN user_model_configs.user_id IS '用户ID'",
        "COMMENT ON COLUMN user_model_configs.model_mid IS '模型MID标识'",
        "COMMENT ON COLUMN user_model_configs.param_values IS '锁定的配置参数序列化JSON串'",
        "COMMENT ON COLUMN user_model_configs.is_locked IS '是否已锁定，1=是，0=否'",
        r#"CREATE TABLE IF NOT EXISTS happyhorse_logs (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            original_model TEXT NOT NULL,
            media_type TEXT NOT NULL,
            matched_model TEXT NOT NULL,
            status INTEGER NOT NULL,
            latency_ms INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            task_id TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_happyhorse_logs_created ON happyhorse_logs (created_at DESC)",
        "COMMENT ON TABLE happyhorse_logs IS '快乐小马智能路由转换日志表'",
        "COMMENT ON COLUMN happyhorse_logs.original_model IS '原始请求模型ID'",
        "COMMENT ON COLUMN happyhorse_logs.media_type IS '媒体类型(文生视频/图生视频/参考生视频/视频编辑)'",
        "COMMENT ON COLUMN happyhorse_logs.matched_model IS '路由分发的实际模型ID'",
        "ALTER TABLE happyhorse_logs ADD COLUMN IF NOT EXISTS log_id BIGINT",
        "COMMENT ON COLUMN happyhorse_logs.log_id IS '关联主日志表logs.id，用于JOIN获取完整请求/响应/计费信息'",
        "CREATE INDEX IF NOT EXISTS idx_happyhorse_logs_log_id ON happyhorse_logs (log_id)",
        "ALTER TABLE happyhorse_logs DROP COLUMN IF EXISTS request_payload",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS plugin_tag TEXT DEFAULT ''",
        "COMMENT ON COLUMN logs.plugin_tag IS '插件标记JSON，用于匹配规则展示和插件解耦'"
    );
    // happyhorse_logs: user_id → user_uid（存储短标识，提高效率和可读性）
    // 注意：新功能表存储用户标识统一使用 uid（users.uid）而非 user_id（users.id）
    // 使用 sys_migration_history 一次性机制包裹，并优先检查列是否存在，避免重复运行报错
    if !done.contains("happyhorse_logs_rename_user_id_to_user_uid") {
        let col_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'happyhorse_logs' AND column_name = 'user_id'")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        if col_exists > 0 {
            let _ = sqlx::query("ALTER TABLE happyhorse_logs RENAME COLUMN user_id TO user_uid")
                .execute(pool)
                .await;
            let _ = sqlx::query("UPDATE happyhorse_logs SET user_uid = COALESCE((SELECT u.uid FROM users u WHERE u.id = happyhorse_logs.user_uid), user_uid)").execute(pool).await;
            let _ = sqlx::query(
                "COMMENT ON COLUMN happyhorse_logs.user_uid IS '用户短标识(users.uid)'",
            )
            .execute(pool)
            .await;
        }
        let _ = sqlx::query("INSERT INTO sys_migration_history (id) VALUES ('happyhorse_logs_rename_user_id_to_user_uid')").execute(pool).await;
        done.insert("happyhorse_logs_rename_user_id_to_user_uid".into());
    }

    Ok(())
}
