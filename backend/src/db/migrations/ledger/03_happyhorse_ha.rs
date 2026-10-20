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
    // ── 初始化快乐小马智能路由系统配置及种子数据（受一次性迁移保护） ──
    once_migration!(pool, done, "init_happyhorse_router_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category)
           VALUES ('happyhorse_router', '快乐小马智能路由', '自动合并阿里云 DashScope happyhorse 的文生/图生/参考生/编辑视频 4 个模型，自动分发请求', 0, 'system')
           ON CONFLICT (name) DO NOTHING"#,
        r#"CREATE TABLE IF NOT EXISTS happyhorse_configs (
            id SERIAL PRIMARY KEY,
            custom_model_name TEXT NOT NULL,
            custom_model_id TEXT NOT NULL,
            t2v_model TEXT NOT NULL,
            i2v_model TEXT NOT NULL,
            r2v_model TEXT NOT NULL,
            edit_model TEXT NOT NULL,
            routing_node TEXT NOT NULL UNIQUE,
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_happyhorse_configs_custom_id ON happyhorse_configs (custom_model_id)",
        "COMMENT ON TABLE happyhorse_configs IS '快乐小马智能路由配置表'",
        "COMMENT ON COLUMN happyhorse_configs.custom_model_name IS '自定义模型名称'",
        "COMMENT ON COLUMN happyhorse_configs.custom_model_id IS '自定义模型ID(用户在API中请求的模型)'",
        "COMMENT ON COLUMN happyhorse_configs.t2v_model IS '绑定的文生视频模型ID'",
        "COMMENT ON COLUMN happyhorse_configs.i2v_model IS '绑定的图生视频模型ID'",
        "COMMENT ON COLUMN happyhorse_configs.r2v_model IS '绑定的参考生视频模型ID'",
        "COMMENT ON COLUMN happyhorse_configs.edit_model IS '绑定的视频编辑模型ID'",
        "COMMENT ON COLUMN happyhorse_configs.routing_node IS '生成的智能推理路由节点ID'",
        "COMMENT ON COLUMN happyhorse_configs.is_active IS '是否启用，1=启用，0=禁用'",
        r#"INSERT INTO happyhorse_configs (custom_model_name, custom_model_id, t2v_model, i2v_model, r2v_model, edit_model, routing_node, is_active)
           VALUES ('快乐小马智能路由', 'happyhorse-smart', 'happyhorse-1.0-t2v', 'happyhorse-1.0-i2v', 'happyhorse-1.0-r2v', 'happyhorse-1.0-video-edit', 'ephh-happyhorse', 1)
           ON CONFLICT (routing_node) DO NOTHING"#
    );

    // ── 初始化文件去重指纹与快乐小马微调及日志唯一标识字段（受一次性迁移保护） ──
    once_migration!(pool, done, "init_happyhorse_updates_v1",
        "ALTER TABLE playground_assets ADD COLUMN IF NOT EXISTS file_hash TEXT DEFAULT ''",
        "COMMENT ON COLUMN playground_assets.file_hash IS '文件内容SHA256哈希，用于幂等去重'",
        "CREATE INDEX IF NOT EXISTS idx_pg_assets_file_hash ON playground_assets(file_hash)",
        "ALTER TABLE plugin_assets ADD COLUMN IF NOT EXISTS meta_fingerprint VARCHAR(128)",
        "COMMENT ON COLUMN plugin_assets.meta_fingerprint IS 'HTTP HEAD元数据指纹(URL域名路径+Content-Length+ETag/Last-Modified的SHA-256)，用于大文件快速去重，避免下载完整文件'",
        "CREATE INDEX IF NOT EXISTS idx_plugin_assets_meta_fp ON plugin_assets (meta_fingerprint)",
        r#"CREATE TABLE IF NOT EXISTS user_level_logs (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            old_level TEXT NOT NULL DEFAULT '',
            old_level_name TEXT NOT NULL DEFAULT '',
            new_level TEXT NOT NULL DEFAULT '',
            new_level_name TEXT NOT NULL DEFAULT '',
            operator TEXT NOT NULL DEFAULT '',
            operator_id TEXT NOT NULL DEFAULT '',
            source TEXT NOT NULL DEFAULT 'admin',
            remark TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "COMMENT ON TABLE user_level_logs IS '用户等级变更历史日志'",
        "COMMENT ON COLUMN user_level_logs.source IS '变更来源: admin=管理员手动, marketing=推广负责人, system=系统自动'",
        "CREATE INDEX IF NOT EXISTS idx_user_level_logs_user_id ON user_level_logs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_user_level_logs_created_at ON user_level_logs(created_at DESC)",
        "ALTER TABLE happyhorse_logs ALTER COLUMN id TYPE BIGINT",
        "ALTER TABLE happyhorse_logs DROP COLUMN IF EXISTS status",
        "ALTER TABLE happyhorse_logs DROP COLUMN IF EXISTS latency_ms",
        "ALTER TABLE happyhorse_logs DROP COLUMN IF EXISTS error_message",
        "ALTER TABLE happyhorse_logs DROP COLUMN IF EXISTS task_id",
        "COMMENT ON TABLE happyhorse_logs IS '快乐小马智能路由转换日志表'",
        "UPDATE happyhorse_configs SET t2v_model = m.mid FROM models m WHERE happyhorse_configs.t2v_model = m.model_id AND happyhorse_configs.t2v_model != m.mid",
        "UPDATE happyhorse_configs SET i2v_model = m.mid FROM models m WHERE happyhorse_configs.i2v_model = m.model_id AND happyhorse_configs.i2v_model != m.mid",
        "UPDATE happyhorse_configs SET r2v_model = m.mid FROM models m WHERE happyhorse_configs.r2v_model = m.model_id AND happyhorse_configs.r2v_model != m.mid",
        "UPDATE happyhorse_configs SET edit_model = m.mid FROM models m WHERE happyhorse_configs.edit_model = m.model_id AND happyhorse_configs.edit_model != m.mid",
        "COMMENT ON COLUMN happyhorse_configs.t2v_model IS '绑定的文生视频模型MID(不可变标识)'",
        "COMMENT ON COLUMN happyhorse_configs.i2v_model IS '绑定的图生视频模型MID(不可变标识)'",
        "COMMENT ON COLUMN happyhorse_configs.r2v_model IS '绑定的参考生视频模型MID(不可变标识)'",
        "COMMENT ON COLUMN happyhorse_configs.edit_model IS '绑定的视频编辑模型MID(不可变标识)'",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS log_id TEXT",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_logs_log_id ON logs (log_id)"
    );

    // 回填存量数据的 log_id（使用 前缀 + 时间戳hex + id hex 拼接，保证唯一且有序，仅执行一次避免大表全扫描卡顿）
    once_migration!(pool, done, "backfill_logs_log_id_v1",
        "UPDATE logs SET log_id = CASE \
            WHEN task_id IS NOT NULL AND task_id != '' \
                 AND action_type IS NOT NULL AND action_type NOT IN ('', '聊天') \
            THEN 'tsk_' || lpad(to_hex((EXTRACT(EPOCH FROM created_at::timestamp) * 1000)::bigint), 12, '0') || lpad(to_hex(id), 14, '0') \
            ELSE 'log_' || lpad(to_hex((EXTRACT(EPOCH FROM created_at::timestamp) * 1000)::bigint), 12, '0') || lpad(to_hex(id), 14, '0') \
        END \
        WHERE log_id IS NULL"
    );

    // 自动清洗历史失败日志的脏扣费数据（受一次性迁移保护）
    once_migration!(pool, done, "clean_dirty_logs_cost_20260609",
        "UPDATE logs SET cost = 0.0, pre_deduct_gift = 0.0 WHERE status_code < 200 OR status_code >= 400"
    );

    // 自动修复历史遗留的 users.used_quota 统计不准确问题（受一次性迁移保护）
    once_migration!(pool, done, "fix_used_quota_v2_20260609",
        "UPDATE users u SET \
         used_quota = COALESCE((SELECT SUM(cost) FROM logs l WHERE l.user_id = u.id), 0.0), \
         gift_used_quota = COALESCE((SELECT SUM(LEAST(cost, pre_deduct_gift)) FROM logs l WHERE l.user_id = u.id), 0.0) \
         WHERE u.used_quota > 0"
    );

    // 自动修复历史遗留的 users.balance 错误并进行真实余额校准（受一次性迁移保护）
    once_migration!(pool, done, "fix_users_balance_20260609",
        "UPDATE users u SET \
         balance = COALESCE((SELECT SUM(amount) FROM recharge_records r WHERE r.user_id = u.id AND r.wallet_type = 'system'), 0.0) - COALESCE((SELECT SUM(cost - pre_deduct_gift) FROM logs l WHERE l.user_id = u.id), 0.0), \
         gift_balance = GREATEST(COALESCE((SELECT SUM(amount) FROM recharge_records r WHERE r.user_id = u.id AND r.wallet_type = 'gift'), 0.0) - COALESCE((SELECT SUM(pre_deduct_gift) FROM logs l WHERE l.user_id = u.id), 0.0), 0.0) \
         WHERE EXISTS (SELECT 1 FROM logs WHERE user_id = u.id) OR EXISTS (SELECT 1 FROM recharge_records WHERE user_id = u.id)"
    );

    // 修复之前对于部分退款（cost < pre_deduct_gift）导致系统余额倒贴的漏洞并校准余额（受一次性迁移保护）
    once_migration!(pool, done, "fix_users_balance_v2_20260609",
        "UPDATE users u SET \
         balance = COALESCE((SELECT SUM(amount) FROM recharge_records r WHERE r.user_id = u.id AND r.wallet_type = 'system'), 0.0) - COALESCE((SELECT SUM(GREATEST(cost - pre_deduct_gift, 0.0)) FROM logs l WHERE l.user_id = u.id), 0.0), \
         gift_balance = GREATEST(COALESCE((SELECT SUM(amount) FROM recharge_records r WHERE r.user_id = u.id AND r.wallet_type = 'gift'), 0.0) - COALESCE((SELECT SUM(LEAST(cost, pre_deduct_gift)) FROM logs l WHERE l.user_id = u.id), 0.0), 0.0) \
         WHERE EXISTS (SELECT 1 FROM logs WHERE user_id = u.id) OR EXISTS (SELECT 1 FROM recharge_records WHERE user_id = u.id)"
    );

    // ── 营销、计费规则、渠道倍率与高可用令牌等扩展列定义，受一次性迁移保护 ──
    once_migration!(pool, done, "marketing_billing_channel_extensions_v1",
        "ALTER TABLE marketing_teams ADD COLUMN IF NOT EXISTS members_can_set_pay BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN marketing_teams.members_can_set_pay IS '团队成员是否可以设置推广用户的支付权限(0=否,1=是)'",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN billing_rules.sort_order IS '排序，数字越大越靠前'",
        "ALTER TABLE forward_rules ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN forward_rules.sort_order IS '排序序号，数字越大越靠前'",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS rate DOUBLE PRECISION NOT NULL DEFAULT 1.0",
        "COMMENT ON COLUMN channels.rate IS '倍率'",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS rate DOUBLE PRECISION NOT NULL DEFAULT 1.0",
        "COMMENT ON COLUMN channel_configs.rate IS '倍率'",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS high_availability INTEGER NOT NULL DEFAULT 1",
        "COMMENT ON COLUMN api_tokens.high_availability IS '是否开启高可用密钥功能(0=禁用,1=启用)'"
    );

    // ── 初始化高可用密钥渠道、指纹与令牌维度限额结构及内置配置（受一次性迁移保护） ──
    once_migration!(pool, done, "init_high_availability_updates_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category, allowed_levels, created_at, updated_at)
           VALUES ('high_availability_channel', '高可用上游渠道系统插件', '启用后，支持管理后台配置高可用渠道组，支持多上游自动防灾切换与按子渠道倍率计费模式。', 1, 'system_builtin', 'all', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (name) DO NOTHING"#,
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES
           ('high_availability_channel', 'ha_max_retries', '3', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
           ('high_availability_channel', 'ha_cooldown_429', '60', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
           ('high_availability_channel', 'ha_cooldown_network', '300', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
           ('high_availability_channel', 'ha_cooldown_auth', '1800', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#,
        "UPDATE users SET username = SUBSTRING(username FROM 1 FOR 48) WHERE char_length(username) > 48",
        "UPDATE users SET nickname = SUBSTRING(nickname FROM 1 FOR 24) WHERE char_length(nickname) > 24",
        "ALTER TABLE users ALTER COLUMN username TYPE VARCHAR(48)",
        "ALTER TABLE users ALTER COLUMN nickname TYPE VARCHAR(24)",
        "UPDATE api_tokens SET name = CASE WHEN SUBSTRING(REGEXP_REPLACE(name, '[^\\w ]|_', '', 'g') FROM 1 FOR 36) = '' THEN 'default' ELSE SUBSTRING(REGEXP_REPLACE(name, '[^\\w ]|_', '', 'g') FROM 1 FOR 36) END WHERE name !~ '^([^\\W_]| )+$' OR CHAR_LENGTH(name) > 36",
        "ALTER TABLE api_tokens DROP CONSTRAINT IF EXISTS chk_api_tokens_name",
        "ALTER TABLE api_tokens ADD CONSTRAINT chk_api_tokens_name CHECK (char_length(name) <= 36 AND name ~ '^([^\\W_]| )+$')",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS daily_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1.0",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS daily_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS weekly_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1.0",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS weekly_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS monthly_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1.0",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS monthly_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS last_reset_day TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS last_reset_week TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS last_reset_month TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS priority INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN channel_configs.priority IS '请求优先级'",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS weight INTEGER NOT NULL DEFAULT 1",
        "COMMENT ON COLUMN channel_configs.weight IS '请求权重'",
        "UPDATE plugins SET category = 'system_builtin', is_enabled = 1 WHERE name IN ('high_availability_channel', 'site_icons')"
    );

    // ── 火山引擎画质增强与字幕擦除插件条件编译迁移 ──
    #[cfg(feature = "plugin_volcengine_enhance")]
    {
        if !done.contains("volcengine_enhance_init_v1") {
            tracing::info!("开始执行火山引擎画质增强与字幕擦除插件迁移与初始化...");
            // 1. 注册插件 (指定 category = 'system', 标识为系统增强插件，此处加上数据库字段意义的备注说明方便维护)
            let _ = sqlx::query(
                "INSERT INTO plugins (name, title, description, is_enabled, allowed_levels, category, created_at, updated_at) \
                 VALUES ('volcengine_enhance', '火山引擎 AI MediaKit 插件', \
                 '集成火山引擎 AI MediaKit，提供视频画质增强（标准版、专业版、极速版、大模型版）与字幕擦除（标准版、精细版）能力，支持按规格阶梯计费。', \
                 0, 'all', 'system', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
                 ON CONFLICT (name) DO UPDATE SET title = EXCLUDED.title"
            ).execute(pool).await;

            // 兼容处理：对于已经插入过的旧记录，更新插件的显示标题名称
            let _ = sqlx::query(
                "UPDATE plugins SET title = '火山引擎 AI MediaKit 插件' WHERE name = 'volcengine_enhance'"
            ).execute(pool).await;

            // 2. 批量拉取映射 ID，规避复杂嵌套子查询，确保服务商、API 提供商和模型类型都获取到
            let volc_provider_id: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM model_providers WHERE name = '火山引擎' LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

            let volc_api_provider_id: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM model_api_providers WHERE name ILIKE '%火山%' OR name ILIKE '%volcengine%' LIMIT 1"
            ).fetch_optional(pool).await.unwrap_or(None);

            // 获取"视频增强"类型 ID（用于 6 个预置模型）
            let enhance_type_id: Option<i64> =
                sqlx::query_scalar("SELECT id FROM model_types WHERE name = '视频增强' LIMIT 1")
                    .fetch_optional(pool)
                    .await
                    .unwrap_or(None);

            // 注册 4 个细分版本的视频画质计费规则 (按秒换算)
            let _ = sqlx::query(
                "INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, pricing_tiers, extended_config, is_system, provider_id, type_id) \
                 SELECT '火山 MediaKit 视频画质增强 (标准版)', 'duration', 0.0, 0.0, 0.0, 0.0125, 'video_quality', \
                 '[{\"resolution\":\"720p\",\"fps_range\":\"<=30\",\"rate\":0.0125,\"enabled\":true},{\"resolution\":\"720p\",\"fps_range\":\">30\",\"rate\":0.025,\"enabled\":true},{\"resolution\":\"1080p\",\"fps_range\":\"<=30\",\"rate\":0.025,\"enabled\":true},{\"resolution\":\"1080p\",\"fps_range\":\">30\",\"rate\":0.05,\"enabled\":true},{\"resolution\":\"2k\",\"fps_range\":\"<=30\",\"rate\":0.05,\"enabled\":true},{\"resolution\":\"2k\",\"fps_range\":\">30\",\"rate\":0.10,\"enabled\":true},{\"resolution\":\"4k\",\"fps_range\":\"<=30\",\"rate\":0.10,\"enabled\":true},{\"resolution\":\"4k\",\"fps_range\":\">30\",\"rate\":0.20,\"enabled\":true}]', \
                 '{}', 1, $1, $2 \
                 WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = '火山 MediaKit 视频画质增强 (标准版)')"
            )
            .bind(volc_provider_id)
            .bind(enhance_type_id)
            .execute(pool).await;

            let _ = sqlx::query(
                "INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, pricing_tiers, extended_config, is_system, provider_id, type_id) \
                 SELECT '火山 MediaKit 视频画质增强 (专业版)', 'duration', 0.0, 0.0, 0.0, 0.125, 'video_quality', \
                 '[{\"resolution\":\"720p\",\"fps_range\":\"<=30\",\"rate\":0.125,\"enabled\":true},{\"resolution\":\"720p\",\"fps_range\":\">30\",\"rate\":0.25,\"enabled\":true},{\"resolution\":\"1080p\",\"fps_range\":\"<=30\",\"rate\":0.25,\"enabled\":true},{\"resolution\":\"1080p\",\"fps_range\":\">30\",\"rate\":0.50,\"enabled\":true},{\"resolution\":\"2k\",\"fps_range\":\"<=30\",\"rate\":0.50,\"enabled\":true},{\"resolution\":\"2k\",\"fps_range\":\">30\",\"rate\":1.00,\"enabled\":true},{\"resolution\":\"4k\",\"fps_range\":\"<=30\",\"rate\":1.00,\"enabled\":true},{\"resolution\":\"4k\",\"fps_range\":\">30\",\"rate\":2.00,\"enabled\":true}]', \
                 '{}', 1, $1, $2 \
                 WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = '火山 MediaKit 视频画质增强 (专业版)')"
            )
            .bind(volc_provider_id)
            .bind(enhance_type_id)
            .execute(pool).await;

            let _ = sqlx::query(
                "INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, pricing_tiers, extended_config, is_system, provider_id, type_id) \
                 SELECT '火山 MediaKit 视频画质增强 (极速版)', 'duration', 0.0, 0.0, 0.0, 0.00333333, 'video_quality', \
                 '[{\"resolution\":\"720p\",\"fps_range\":\"<=30\",\"rate\":0.00333333,\"enabled\":true},{\"resolution\":\"720p\",\"fps_range\":\">30\",\"rate\":0.00666667,\"enabled\":true},{\"resolution\":\"1080p\",\"fps_range\":\"<=30\",\"rate\":0.00666667,\"enabled\":true},{\"resolution\":\"1080p\",\"fps_range\":\">30\",\"rate\":0.01333333,\"enabled\":true},{\"resolution\":\"2k\",\"fps_range\":\"<=30\",\"rate\":0.01333333,\"enabled\":true},{\"resolution\":\"2k\",\"fps_range\":\">30\",\"rate\":0.02666667,\"enabled\":true},{\"resolution\":\"4k\",\"fps_range\":\"<=30\",\"rate\":0.02666667,\"enabled\":true},{\"resolution\":\"4k\",\"fps_range\":\">30\",\"rate\":0.05333333,\"enabled\":true}]', \
                 '{}', 1, $1, $2 \
                 WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = '火山 MediaKit 视频画质增强 (极速版)')"
            )
            .bind(volc_provider_id)
            .bind(enhance_type_id)
            .execute(pool).await;

            let _ = sqlx::query(
                "INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, pricing_tiers, extended_config, is_system, provider_id, type_id) \
                 SELECT '火山 MediaKit 视频画质增强 (大模型版)', 'duration', 0.0, 0.0, 0.0, 0.04166667, 'video_quality', \
                 '[{\"resolution\":\"720p\",\"fps_range\":\"<=30\",\"rate\":0.04166667,\"enabled\":true},{\"resolution\":\"720p\",\"fps_range\":\">30\",\"rate\":0.08333333,\"enabled\":true},{\"resolution\":\"1080p\",\"fps_range\":\"<=30\",\"rate\":0.08333333,\"enabled\":true},{\"resolution\":\"1080p\",\"fps_range\":\">30\",\"rate\":0.16666667,\"enabled\":true}]', \
                 '{}', 1, $1, $2 \
                 WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = '火山 MediaKit 视频画质增强 (大模型版)')"
            )
            .bind(volc_provider_id)
            .bind(enhance_type_id)
            .execute(pool).await;

            let rule_id_standard: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM billing_rules WHERE name = '火山 MediaKit 视频画质增强 (标准版)'",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            let rule_id_professional: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM billing_rules WHERE name = '火山 MediaKit 视频画质增强 (专业版)'",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            let rule_id_fast: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM billing_rules WHERE name = '火山 MediaKit 视频画质增强 (极速版)'",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            let rule_id_generative: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM billing_rules WHERE name = '火山 MediaKit 视频画质增强 (大模型版)'",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

            // 注册 2 个细分版本的字幕擦除计费规则 (按秒换算)
            let _ = sqlx::query(
                "INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, pricing_tiers, extended_config, is_system, provider_id, type_id) \
                 SELECT '火山 MediaKit 视频字幕擦除 (标准版)', 'duration', 0.0, 0.0, 0.0, 0.00666667, 'standard', \
                 '[]', \
                 '{}', 1, $1, $2 \
                 WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = '火山 MediaKit 视频字幕擦除 (标准版)')"
            )
            .bind(volc_provider_id)
            .bind(enhance_type_id)
            .execute(pool).await;

            let _ = sqlx::query(
                "INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, pricing_tiers, extended_config, is_system, provider_id, type_id) \
                 SELECT '火山 MediaKit 视频字幕擦除 (精细版)', 'duration', 0.0, 0.0, 0.0, 0.01666667, 'standard', \
                 '[]', \
                 '{}', 1, $1, $2 \
                 WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = '火山 MediaKit 视频字幕擦除 (精细版)')"
            )
            .bind(volc_provider_id)
            .bind(enhance_type_id)
            .execute(pool).await;

            let rule_id_erase_standard: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM billing_rules WHERE name = '火山 MediaKit 视频字幕擦除 (标准版)'",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            let rule_id_erase_pro: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM billing_rules WHERE name = '火山 MediaKit 视频字幕擦除 (精细版)'",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

            // 3. 注册 4 个火山 MediaKit 内置转发规则，使用安全的 WHERE NOT EXISTS 语法防重，避开 ON CONFLICT 报错
            let preset_rules = vec![
                (
                    "火山 MediaKit 视频画质增强 (标准/专业版)",
                    "volcengine",
                    "火山画质增强标准版与专业版通用转发规则，自动进行路径和请求体参数转换，支持异步任务轮询。",
                    r#"{"target_type":"volcengine_media_enhance","path_rewrite":{"old":"/v1/video/generations","new":"/api/v1/tools/enhance-video"},"poll_path":"/api/v1/tasks/${task_id}","auth_type":"bearer"}"#
                ),
                (
                    "火山 MediaKit 视频画质增强 (极速版)",
                    "volcengine",
                    "火山画质增强极速版专用转发规则，自动转发至 enhance-video-fast，支持异步任务轮询。",
                    r#"{"target_type":"volcengine_media_enhance","path_rewrite":{"old":"/v1/video/generations","new":"/api/v1/tools/enhance-video-fast"},"poll_path":"/api/v1/tasks/${task_id}","auth_type":"bearer"}"#
                ),
                (
                    "火山 MediaKit 视频画质增强 (大模型版)",
                    "volcengine",
                    "火山画质增强大模型版专用转发规则，自动转发至 enhance-video-generative，支持异步任务轮询。",
                    r#"{"target_type":"volcengine_media_enhance","path_rewrite":{"old":"/v1/video/generations","new":"/api/v1/tools/enhance-video-generative"},"poll_path":"/api/v1/tasks/${task_id}","auth_type":"bearer"}"#
                ),
                (
                    "火山 MediaKit 视频字幕擦除",
                    "volcengine",
                    "火山视频字幕擦除（标准/精细版）通用转发规则，自动转发至 erase-video-subtitle，支持异步任务轮询。",
                    r#"{"target_type":"volcengine_media_enhance","path_rewrite":{"old":"/v1/video/generations","new":"/api/v1/tools/erase-video-subtitle"},"poll_path":"/api/v1/tasks/${task_id}","auth_type":"bearer"}"#
                )
            ];

            for (name, rtype, desc, config) in &preset_rules {
                let _ = sqlx::query(
                    "INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid) \
                     SELECT $1, $2, $3, $4, '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0') \
                     WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = $1)"
                )
                .bind(name).bind(rtype).bind(desc).bind(config)
                .execute(pool).await;
            }

            // 4. 获取刚注册好的内置规则 ID 映射
            let rule_id_sd_pf: Option<i64> = sqlx::query_scalar("SELECT id FROM forward_rules WHERE name = '火山 MediaKit 视频画质增强 (标准/专业版)'").fetch_optional(pool).await.unwrap_or(None);
            let rule_id_ft: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM forward_rules WHERE name = '火山 MediaKit 视频画质增强 (极速版)'",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            let rule_id_gt: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM forward_rules WHERE name = '火山 MediaKit 视频画质增强 (大模型版)'",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            let rule_id_erase: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM forward_rules WHERE name = '火山 MediaKit 视频字幕擦除'",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

            // 5. 初始化 6 个画质增强预置模型，显式绑定 provider_id (火山引擎) 、默认转发规则 forward_rule_ids 以及默认计费规则 billing_rule_id
            let preset_models = vec![
                (
                    "vve-sd",
                    "火山画质增强-标准版",
                    "volc_video_enhance_standard",
                    rule_id_sd_pf,
                    rule_id_standard,
                ),
                (
                    "vve-pf",
                    "火山画质增强-专业版",
                    "volc_video_enhance_professional",
                    rule_id_sd_pf,
                    rule_id_professional,
                ),
                (
                    "vve-ft",
                    "火山画质增强-极速版",
                    "volc_video_enhance_fast",
                    rule_id_ft,
                    rule_id_fast,
                ),
                (
                    "vve-gt",
                    "火山画质增强-大模型版",
                    "volc_video_enhance_generative",
                    rule_id_gt,
                    rule_id_generative,
                ),
                (
                    "vvs-er",
                    "火山字幕擦除-标准版",
                    "volc_video_subtitle_erase",
                    rule_id_erase,
                    rule_id_erase_standard,
                ),
                (
                    "vvs-ep",
                    "火山字幕擦除-精细版",
                    "volc_video_subtitle_erase_pro",
                    rule_id_erase,
                    rule_id_erase_pro,
                ),
            ];

            for (mid, name, model_id, rule_id, billing_rule_id) in &preset_models {
                let rule_ids_json = rule_id.map(|id| format!("[{}]", id));
                let _ = sqlx::query(
                    "INSERT INTO models (mid, name, model_id, provider_id, api_provider_id, type_id, forward_rule_ids, billing_rule_id, is_active, \
                     remark, created_at, updated_at) \
                     SELECT $1, $2, $3, $4, $5, $6, $7, $8, 0, '火山引擎画质增强/字幕擦除插件预置模型，请勿删除', \
                     CURRENT_TIMESTAMP, CURRENT_TIMESTAMP \
                     WHERE NOT EXISTS (SELECT 1 FROM models WHERE mid = $1)"
                )
                .bind(mid).bind(name).bind(model_id)
                .bind(volc_provider_id)
                .bind(volc_api_provider_id)
                .bind(enhance_type_id)
                .bind(rule_ids_json)
                .bind(billing_rule_id)
                .execute(pool).await;
            }

            // 5.5 初始化两个豆包级联画质增强模型种子数据，强制绑定到级联计费规则与转发规则
            let video_type_id: Option<i64> =
                sqlx::query_scalar("SELECT id FROM model_types WHERE name = '视频' LIMIT 1")
                    .fetch_optional(pool)
                    .await
                    .unwrap_or(None);
            let rule_id_cascade_billing: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM billing_rules WHERE name = '火山级联画质增强默认计费' LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

            let cascade_models = vec![
                (
                    "dbs-sr",
                    "豆包 Seedance 2.0 (画质增强级联)",
                    "Doubao-seedance-2-0-sr",
                    "doubao-seedance-2-0-260128",
                    30.0,
                ),
                (
                    "dbs-fs",
                    "豆包 Seedance 2.0 极速版 (画质增强级联)",
                    "Doubao-seedance-2-0-fast-sr",
                    "doubao-seedance-2-0-fast-260128",
                    30.0,
                ),
            ];

            for (mid, name, model_id, alias, pre_deduct) in &cascade_models {
                let _ = sqlx::query(
                    "INSERT INTO models (mid, name, model_id, model_id_alias, provider_id, api_provider_id, type_id, group_ratios, billing_rule_id, pre_deduction, is_active, remark, created_at, updated_at) \
                     SELECT $1, $2, $3, $4, $5, $6, $7, '{\"default\":1.0}', $8, $9, 0, '火山方舟级联画质增强模型，请勿删除', \
                     CURRENT_TIMESTAMP, CURRENT_TIMESTAMP \
                     WHERE NOT EXISTS (SELECT 1 FROM models WHERE mid = $1)"
                )
                .bind(mid).bind(name).bind(model_id).bind(alias)
                .bind(volc_provider_id)
                .bind(volc_api_provider_id)
                .bind(video_type_id)
                .bind(rule_id_cascade_billing)
                .bind(pre_deduct)
                .execute(pool).await;
            }

            let _ = sqlx::query(
                "INSERT INTO sys_migration_history (id) VALUES ('volcengine_enhance_init_v1')",
            )
            .execute(pool)
            .await;
            done.insert("volcengine_enhance_init_v1".into());
            tracing::info!("火山引擎画质增强插件初始化完成");
        }
    }

    // 7. PostgreSQL 18.4 专用性能与稳定性优化：引入覆盖索引（Covering Indexes）加速大表查询与统计（受一次性迁移保护）
    once_migration!(pool, done, "pg18_performance_optimizations_v1",
        "CREATE INDEX IF NOT EXISTS idx_logs_user_dashboard_covering ON logs (user_id, created_at DESC) INCLUDE (cost, prompt_tokens, completion_tokens, cached_tokens)",
        "CREATE INDEX IF NOT EXISTS idx_logs_admin_dashboard_covering ON logs (created_at DESC) INCLUDE (cost, prompt_tokens, completion_tokens, cached_tokens)"
    );

    // 8. 将 'playground' 插件的 title 从 '模型体验中心' 修改为 '模型创作中心'（受一次性迁移保护）
    once_migration!(
        pool,
        done,
        "rename_playground_title_to_creation_center_20260621",
        "UPDATE plugins SET title = '模型创作中心' WHERE name = 'playground'"
    );

    // 9. 将系统默认菜单配置中 '/playground' 的 label_zh 从 '体验中心' 或 '操场' 修改为 '创作中心'（受一次性迁移保护）
    once_migration!(
        pool,
        done,
        "update_menu_playground_label_to_creation_center_20260621",
        "UPDATE settings SET value = replace(\
            replace(\
                replace(\
                    replace(value, '\"label_zh\":\"体验中心\"', '\"label_zh\":\"创作中心\"'), \
                    '\"label_zh\": \"体验中心\"', '\"label_zh\": \"创作中心\"'\
                ), \
                '\"label_zh\":\"操场\"', '\"label_zh\":\"创作中心\"'\
            ), \
            '\"label_zh\": \"操场\"', '\"label_zh\": \"创作中心\"'\
        ) WHERE key = 'menu_config_settings'"
    );

    Ok(())
}
