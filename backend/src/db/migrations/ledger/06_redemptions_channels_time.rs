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
    // ── 增加用户通知订阅偏好设置字段 ──
    once_migration!(
        pool,
        done,
        "add_user_notification_preferences_v1",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS notification_preferences TEXT",
        "COMMENT ON COLUMN users.notification_preferences IS '用户的通知订阅偏好(JSON格式)'"
    );

    // ── 渠道分组分类：可自定义分类，默认图片/视频/聊天 ──
    once_migration!(pool, done, "init_channel_categories_v1",
        r#"CREATE TABLE IF NOT EXISTS channel_categories (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            name_en TEXT NOT NULL DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            is_system INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS category_id BIGINT REFERENCES channel_categories(id)",
        "INSERT INTO channel_categories (name, name_en, sort_order, is_active, is_system) VALUES ('图片', 'Image', 30, 1, 1) ON CONFLICT (name) DO UPDATE SET is_system = 1",
        "INSERT INTO channel_categories (name, name_en, sort_order, is_active, is_system) VALUES ('视频', 'Video', 20, 1, 1) ON CONFLICT (name) DO UPDATE SET is_system = 1",
        "INSERT INTO channel_categories (name, name_en, sort_order, is_active, is_system) VALUES ('聊天', 'Chat', 10, 1, 1) ON CONFLICT (name) DO UPDATE SET is_system = 1"
    );

    // 若先前误用 TIMESTAMPTZ，统一改为 TEXT 以匹配 sqlx String 映射
    once_migration!(
        pool,
        done,
        "fix_channel_categories_timestamps_v1",
        r#"DO $$
        BEGIN
          IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'channel_categories' AND column_name = 'created_at'
              AND data_type = 'timestamp with time zone'
          ) THEN
            ALTER TABLE channel_categories
              ALTER COLUMN created_at TYPE TEXT USING created_at::text,
              ALTER COLUMN updated_at TYPE TEXT USING updated_at::text;
          END IF;
        END $$"#
    );

    // ── 兑换码：有效期 / 总次数 / 每用户次数 + 兑换记录表 ──
    once_migration!(pool, done, "redemptions_limits_expiry_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS expires_at TEXT",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS max_uses INTEGER NOT NULL DEFAULT 1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS used_count INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS per_user_limit INTEGER NOT NULL DEFAULT 1",
        "UPDATE redemptions SET used_count = 1 WHERE is_used = 1 AND used_count = 0",
        r#"CREATE TABLE IF NOT EXISTS redemption_logs (
            id BIGSERIAL PRIMARY KEY,
            redemption_id BIGINT NOT NULL REFERENCES redemptions(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL,
            amount DOUBLE PRECISION NOT NULL,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_redemption_logs_code_user ON redemption_logs (redemption_id, user_id)"
    );

    once_migration!(
        pool,
        done,
        "redemptions_status_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS status INTEGER NOT NULL DEFAULT 1"
    );

    // ── 渠道分组 + 上游预设：日/月/总额度 ──
    once_migration!(pool, done, "channel_period_quota_v1",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS daily_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS daily_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS monthly_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS monthly_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS last_reset_day TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS last_reset_month TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN channels.daily_quota_limit IS '日额度上限(-1=无限)'",
        "COMMENT ON COLUMN channels.monthly_quota_limit IS '月额度上限(-1=无限)'",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS quota_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS daily_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS daily_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS monthly_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS monthly_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS last_reset_day TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS last_reset_month TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN channel_configs.quota_limit IS '总额度上限(-1=无限)'",
        "COMMENT ON COLUMN channel_configs.daily_quota_limit IS '日额度上限(-1=无限)'",
        "COMMENT ON COLUMN channel_configs.monthly_quota_limit IS '月额度上限(-1=无限)'"
    );

    // ── 渠道分组 + 上游预设：周额度（对齐令牌日/周/月）──
    once_migration!(pool, done, "channel_weekly_quota_v1",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS weekly_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS weekly_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS last_reset_week TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN channels.weekly_quota_limit IS '周额度上限(-1=无限)'",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS weekly_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS weekly_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS last_reset_week TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN channel_configs.weekly_quota_limit IS '周额度上限(-1=无限)'"
    );

    // ── 全库时间列 TEXT → TIMESTAMPTZ（timesystem UTC，提升 logs 等范围查询可走索引）──
    // 部署注意：logs 大表 ALTER TYPE 会持有 ACCESS EXCLUSIVE 并重写表，请安排维护窗口。
    // 周期键 last_reset_* / last_daily_reset 等仍为 TEXT（日历键，非时间戳）。
    once_migration!(
        pool,
        done,
        "timestamptz_unify_v1",
        r#"CREATE OR REPLACE FUNCTION _tb_text_to_tstz(t TEXT) RETURNS TIMESTAMPTZ AS $fn$
        BEGIN
          IF t IS NULL OR btrim(t) = '' THEN
            RETURN NULL;
          END IF;
          BEGIN
            IF substring(t from 11) LIKE '%+%'
               OR substring(t from 11) LIKE '%-%'
               OR substring(t from 11) LIKE '%Z%'
               OR position('T' in t) > 0 THEN
              RETURN t::timestamptz;
            END IF;
            RETURN (t || '+00:00')::timestamptz;
          EXCEPTION WHEN OTHERS THEN
            BEGIN
              RETURN (t || '+00:00')::timestamptz;
            EXCEPTION WHEN OTHERS THEN
              RETURN NULL;
            END;
          END;
        END;
        $fn$ LANGUAGE plpgsql IMMUTABLE"#,
        r#"DO $mig$
        DECLARE
          r RECORD;
          ddl TEXT;
          tbl_exists BOOLEAN;
          is_text BOOLEAN;
        BEGIN
          FOR r IN
            SELECT * FROM (VALUES
              ('logs', 'created_at', true),
              ('users', 'created_at', true),
              ('users', 'updated_at', true),
              ('api_tokens', 'created_at', true),
              ('api_tokens', 'updated_at', true),
              ('api_tokens', 'expires_at', false),
              ('api_tokens', 'last_used_at', false),
              ('channels', 'created_at', true),
              ('channels', 'updated_at', true),
              ('channel_configs', 'created_at', true),
              ('channel_configs', 'updated_at', true),
              ('channel_categories', 'created_at', true),
              ('channel_categories', 'updated_at', true),
              ('orders', 'created_at', true),
              ('orders', 'paid_at', false),
              ('redemptions', 'created_at', true),
              ('redemptions', 'updated_at', true),
              ('redemptions', 'used_at', false),
              ('redemptions', 'expires_at', false),
              ('redemption_logs', 'created_at', true),
              ('verification_codes', 'created_at', true),
              ('verification_codes', 'expires_at', true),
              ('user_levels', 'created_at', true),
              ('user_levels', 'updated_at', true),
              ('admin_groups', 'created_at', true),
              ('admin_groups', 'updated_at', true),
              ('announcements', 'created_at', true),
              ('announcements', 'updated_at', true),
              ('model_providers', 'created_at', true),
              ('model_providers', 'updated_at', true),
              ('model_types', 'created_at', true),
              ('model_types', 'updated_at', true),
              ('models', 'created_at', true),
              ('models', 'updated_at', true),
              ('model_api_providers', 'created_at', true),
              ('model_api_providers', 'updated_at', true),
              ('forward_rules', 'created_at', true),
              ('forward_rules', 'updated_at', true),
              ('billing_rules', 'created_at', true),
              ('billing_rules', 'updated_at', true),
              ('upstreams', 'created_at', true),
              ('upstreams', 'updated_at', true),
              ('plugins', 'created_at', true),
              ('plugins', 'updated_at', true),
              ('plugin_configs', 'created_at', true),
              ('plugin_configs', 'updated_at', true),
              ('plugin_asset_groups', 'created_at', true),
              ('plugin_asset_groups', 'updated_at', true),
              ('plugin_assets', 'created_at', true),
              ('plugin_assets', 'updated_at', true),
              ('plugin_docs', 'created_at', true),
              ('plugin_docs', 'updated_at', true),
              ('plugin_docs_intl', 'created_at', true),
              ('plugin_docs_intl', 'updated_at', true),
              ('plugin_api_logs', 'created_at', true),
              ('site_icons', 'created_at', true),
              ('site_icons', 'updated_at', true),
              ('site_icon_sync_logs', 'created_at', true),
              ('recharge_records', 'created_at', true),
              ('commissions', 'created_at', true),
              ('playground_projects', 'created_at', true),
              ('playground_projects', 'updated_at', true),
              ('playground_assets', 'created_at', true),
              ('user_model_configs', 'created_at', true),
              ('user_model_configs', 'updated_at', true),
              ('marketing_teams', 'created_at', true),
              ('marketing_teams', 'updated_at', true),
              ('marketing_team_leaders', 'created_at', true),
              ('marketing_team_members', 'created_at', true),
              ('router_flow_groups', 'created_at', true),
              ('router_flow_groups', 'updated_at', true),
              ('tos_temp_files', 'created_at', true),
              ('tos_temp_files', 'expire_at', true),
              ('volcengine_pools', 'created_at', true),
              ('volcengine_pools', 'updated_at', true),
              ('volcengine_pool_accounts', 'created_at', true),
              ('volcengine_pool_accounts', 'updated_at', true),
              ('volcengine_pool_accounts', 'last_error_at', false),
              ('volcengine_pool_logs', 'created_at', true),
              ('gptimage_pools', 'created_at', true),
              ('gptimage_pools', 'updated_at', true),
              ('gptimage_pool_accounts', 'created_at', true),
              ('gptimage_pool_accounts', 'updated_at', true),
              ('gptimage_pool_accounts', 'last_error_at', false),
              ('gptimage_pool_logs', 'created_at', true),
              ('happyhorse_configs', 'created_at', true),
              ('happyhorse_configs', 'updated_at', true),
              ('happyhorse_logs', 'created_at', true),
              ('sys_migration_history', 'executed_at', true)
            ) AS t(tbl, col, nn)
          LOOP
            SELECT EXISTS (
              SELECT 1 FROM information_schema.tables
              WHERE table_schema = 'public' AND table_name = r.tbl
            ) INTO tbl_exists;
            IF NOT tbl_exists THEN
              CONTINUE;
            END IF;

            SELECT (c.data_type IN ('text', 'character varying'))
            INTO is_text
            FROM information_schema.columns c
            WHERE c.table_schema = 'public' AND c.table_name = r.tbl AND c.column_name = r.col;

            IF NOT COALESCE(is_text, false) THEN
              CONTINUE;
            END IF;

            IF r.tbl = 'logs' AND r.col = 'created_at' THEN
              EXECUTE 'DROP INDEX IF EXISTS idx_logs_date_created_at';
            END IF;

            IF r.nn THEN
              ddl := format(
                'ALTER TABLE %I ALTER COLUMN %I DROP DEFAULT, ALTER COLUMN %I TYPE TIMESTAMPTZ USING COALESCE(_tb_text_to_tstz(%I), NOW()), ALTER COLUMN %I SET DEFAULT NOW(), ALTER COLUMN %I SET NOT NULL',
                r.tbl, r.col, r.col, r.col, r.col, r.col
              );
            ELSE
              ddl := format(
                'ALTER TABLE %I ALTER COLUMN %I DROP DEFAULT, ALTER COLUMN %I TYPE TIMESTAMPTZ USING _tb_text_to_tstz(%I)',
                r.tbl, r.col, r.col, r.col
              );
            END IF;
            EXECUTE ddl;
          END LOOP;
        END;
        $mig$"#,
        // 仅清理 TEXT 时代表达式索引；不再同步 CREATE date 索引（会锁大表，且查询多用站点时区桶）。
        "DROP INDEX IF EXISTS idx_logs_date_created_at",
        "DROP FUNCTION IF EXISTS _tb_text_to_tstz(TEXT)"
    );

    Ok(())
}
