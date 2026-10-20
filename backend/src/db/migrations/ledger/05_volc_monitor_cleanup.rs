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
    // ── 火山方舟视频监控插件：主账号、Endpoint绑定、视频任务、分账账单 ──
    once_migration!(pool, done, "add_volc_ark_monitor_v1",
        // 主账号凭证表（支持多火山账号）
        r#"CREATE TABLE IF NOT EXISTS ark_accounts (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            volc_account_id TEXT NOT NULL DEFAULT '',
            access_key TEXT NOT NULL,
            secret_key TEXT NOT NULL,
            region TEXT NOT NULL DEFAULT 'cn-beijing',
            remark TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE ark_accounts IS '火山方舟主账号凭证表（AK/SK）'",
        "COMMENT ON COLUMN ark_accounts.id IS '自增主键'",
        "COMMENT ON COLUMN ark_accounts.name IS '账号别名，全局唯一'",
        "COMMENT ON COLUMN ark_accounts.volc_account_id IS '火山官方账号ID (AccountId)'",
        "COMMENT ON COLUMN ark_accounts.access_key IS '火山引擎 AccessKey'",
        "COMMENT ON COLUMN ark_accounts.secret_key IS '火山引擎 SecretKey'",
        "COMMENT ON COLUMN ark_accounts.region IS 'API调用区域，默认cn-beijing'",
        "COMMENT ON COLUMN ark_accounts.remark IS '管理员备注'",
        // Endpoint与内部用户的绑定关系表
        r#"CREATE TABLE IF NOT EXISTS ark_endpoint_bindings (
            id SERIAL PRIMARY KEY,
            account_id INTEGER NOT NULL REFERENCES ark_accounts(id) ON DELETE CASCADE,
            endpoint_id TEXT NOT NULL,
            user_uid TEXT NOT NULL,
            api_key TEXT NOT NULL DEFAULT '',
            limit_quota DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            used_quota DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            status INTEGER NOT NULL DEFAULT 1,
            remark TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(account_id, endpoint_id)
        )"#,
        "COMMENT ON TABLE ark_endpoint_bindings IS '火山方舟Endpoint与内部用户的绑定关系'",
        "COMMENT ON COLUMN ark_endpoint_bindings.id IS '自增主键'",
        "COMMENT ON COLUMN ark_endpoint_bindings.account_id IS '关联的主账号ID'",
        "COMMENT ON COLUMN ark_endpoint_bindings.endpoint_id IS '火山方舟接入点ID (ep-xxxx)'",
        "COMMENT ON COLUMN ark_endpoint_bindings.user_uid IS '关联的内部用户UID'",
        "COMMENT ON COLUMN ark_endpoint_bindings.api_key IS '绑定的火山方舟静态API Key'",
        "COMMENT ON COLUMN ark_endpoint_bindings.limit_quota IS '消费额度上限(元)，0=不限制'",
        "COMMENT ON COLUMN ark_endpoint_bindings.used_quota IS '已消费金额(元)，由分账账单同步更新'",
        "COMMENT ON COLUMN ark_endpoint_bindings.status IS '状态: 1=正常 0=已熔断停用'",
        "COMMENT ON COLUMN ark_endpoint_bindings.remark IS '管理员备注'",
        "CREATE INDEX IF NOT EXISTS idx_ark_bindings_user ON ark_endpoint_bindings(user_uid)",
        "CREATE INDEX IF NOT EXISTS idx_ark_bindings_endpoint ON ark_endpoint_bindings(endpoint_id)",
        // 视频任务缓存表（拉取自ListVideos）
        r#"CREATE TABLE IF NOT EXISTS ark_video_tasks (
            id BIGSERIAL PRIMARY KEY,
            account_id INTEGER NOT NULL,
            endpoint_id TEXT NOT NULL DEFAULT '',
            task_id TEXT NOT NULL UNIQUE,
            model TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT '',
            duration DOUBLE PRECISION,
            resolution TEXT NOT NULL DEFAULT '',
            created_time TEXT NOT NULL DEFAULT '',
            split_amount DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            is_estimated BOOLEAN NOT NULL DEFAULT TRUE,
            total_tokens BIGINT NOT NULL DEFAULT 0,
            synced_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
            raw_response JSONB NOT NULL DEFAULT '{}'
        )"#,
        "COMMENT ON TABLE ark_video_tasks IS '火山方舟视频任务缓存(来自ListVideos API)'",
        "COMMENT ON COLUMN ark_video_tasks.id IS '自增主键'",
        "COMMENT ON COLUMN ark_video_tasks.account_id IS '所属主账号ID'",
        "COMMENT ON COLUMN ark_video_tasks.endpoint_id IS '归属接入点ID'",
        "COMMENT ON COLUMN ark_video_tasks.task_id IS '火山视频任务唯一ID'",
        "COMMENT ON COLUMN ark_video_tasks.model IS '使用的底座模型名称'",
        "COMMENT ON COLUMN ark_video_tasks.status IS '任务状态(succeed/failed/running等)'",
        "COMMENT ON COLUMN ark_video_tasks.duration IS '视频时长(秒)'",
        "COMMENT ON COLUMN ark_video_tasks.resolution IS '视频分辨率'",
        "COMMENT ON COLUMN ark_video_tasks.created_time IS '火山侧创建时间'",
        "COMMENT ON COLUMN ark_video_tasks.split_amount IS '对应的分账账单消费金额(元)'",
        "COMMENT ON COLUMN ark_video_tasks.is_estimated IS '消费金额是否为估算值(true=估算, false=账单确认)'",
        "COMMENT ON COLUMN ark_video_tasks.total_tokens IS '视频生成消耗的总 token 数'",
        "COMMENT ON COLUMN ark_video_tasks.raw_response IS '火山方舟视频返回的所有原始响应JSON(大字段)'",
        "CREATE INDEX IF NOT EXISTS idx_ark_video_tasks_endpoint ON ark_video_tasks(endpoint_id)",
        "CREATE INDEX IF NOT EXISTS idx_ark_video_tasks_account ON ark_video_tasks(account_id)",
        // 废弃并清理原账单表
        "DROP TABLE IF EXISTS ark_split_bills CASCADE",
        // 注册插件记录
        r#"INSERT INTO plugins (name, title, description, is_enabled, category, created_at, updated_at)
           VALUES ('volcengine_ark_monitor', '火山方舟视频监控', '基于火山方舟接入点(Endpoint)的视频任务与分账账单精密监控及超额熔断控制', 0, 'user', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (name) DO UPDATE SET title = EXCLUDED.title, description = EXCLUDED.description, category = EXCLUDED.category"#
    );

    // 新增用户信用额度限制和支付启用字段，修复最新代码与老版本数据库表结构不一致的问题
    once_migration!(pool, done, "add_user_credit_limit_and_pay_fields_v1",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS credit_limit DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS pay_enabled INTEGER NOT NULL DEFAULT 1"
    );

    once_migration!(
        pool,
        done,
        "add_channel_config_id_to_logs_v1",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS channel_config_id INTEGER"
    );

    once_migration!(
        pool,
        done,
        "add_yid_to_logs_v1",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS yid TEXT DEFAULT ''",
        "COMMENT ON COLUMN logs.yid IS '上游渠道对应的内部标识(由服务商或底层平台侧生成)'"
    );

    // 子配快照统一用 channel_config_id；展示 YID 由 JOIN channel_configs 得到
    once_migration!(
        pool,
        done,
        "drop_logs_yid_v1",
        "ALTER TABLE logs DROP COLUMN IF EXISTS yid"
    );

    once_migration!(
        pool,
        done,
        "add_ha_meltdown_whitelist_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('high_availability_channel', 'ha_meltdown_whitelist', '[]', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    once_migration!(pool, done, "marketing_teams_view_logs_v1",
        "ALTER TABLE marketing_teams ADD COLUMN IF NOT EXISTS members_can_view_logs BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN marketing_teams.members_can_view_logs IS '团队成员是否可以查询关联用户的日志记录(0=否,1=是)'"
    );

    // ── 火山方舟视频任务表新增消费金额是否为估算值字段 ──
    once_migration!(pool, done, "add_ark_video_tasks_is_estimated_v1",
        "ALTER TABLE ark_video_tasks ADD COLUMN IF NOT EXISTS is_estimated BOOLEAN NOT NULL DEFAULT TRUE",
        "COMMENT ON COLUMN ark_video_tasks.is_estimated IS '消费金额是否为估算值(true=估算, false=账单确认)'"
    );

    // ── 为历史遗留的缺失注释的数据库字段补齐备注 ──
    once_migration!(
        pool,
        done,
        "comment_missing_db_fields_v1",
        "COMMENT ON COLUMN users.credit_limit IS '用户信用额度限制(元)'",
        "COMMENT ON COLUMN users.pay_enabled IS '是否启用支付扣费与额度限制(0=禁用, 1=启用)'",
        "COMMENT ON COLUMN logs.channel_config_id IS '关联渠道配置表的ID'"
    );

    // ── 火山方舟视频监控插件增加调试日志启用默认配置 ──
    once_migration!(
        pool,
        done,
        "add_volc_ark_monitor_debug_log_config_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('volcengine_ark_monitor', 'enable_debug_log', 'false', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    // ── 删除已废弃的智能路由插件 ──
    once_migration!(
        pool,
        done,
        "remove_router_flow_plugin_v1",
        "DELETE FROM plugins WHERE name = 'router_flow'"
    );

    // ── 删除已废弃的火山卡池和GPT卡池插件 ──
    once_migration!(
        pool,
        done,
        "remove_pools_plugins_v3",
        "DELETE FROM plugins WHERE name IN ('volcengine_pool', 'gptimage_pool')"
    );

    // ── 清理已移除插件残留表/字段（代码侧已无引用；须在全量节点升级到无卡池版本后执行）──
    // 覆盖：router_flow / volcengine_pool / gptimage_pool 的表、channels 孤儿列、plugin 配置与用户菜单死链
    once_migration!(pool, done, "drop_removed_plugin_schema_v1",
        // 子表/日志先于主表
        "DROP TABLE IF EXISTS volcengine_pool_logs CASCADE",
        "DROP TABLE IF EXISTS volcengine_pool_account_mapping CASCADE",
        "DROP TABLE IF EXISTS volcengine_pool_accounts CASCADE",
        "DROP TABLE IF EXISTS volcengine_pools CASCADE",
        "DROP TABLE IF EXISTS gptimage_pool_logs CASCADE",
        "DROP TABLE IF EXISTS gptimage_pool_account_mapping CASCADE",
        "DROP TABLE IF EXISTS gptimage_pool_accounts CASCADE",
        "DROP TABLE IF EXISTS gptimage_pools CASCADE",
        "DROP TABLE IF EXISTS router_flow_groups CASCADE",
        // channels 孤儿外联列（Channel 模型与 API 已不再读写）
        "ALTER TABLE channels DROP COLUMN IF EXISTS pool_id",
        "ALTER TABLE channels DROP COLUMN IF EXISTS gptimage_pool_id",
        // 插件元数据与配置残留（幂等）
        "DELETE FROM plugin_configs WHERE plugin_name IN ('router_flow', 'volcengine_pool', 'gptimage_pool')",
        "DELETE FROM plugins WHERE name IN ('router_flow', 'volcengine_pool', 'gptimage_pool')",
        // 用户菜单默认项中的已删页面 /smart-router（value 为 JSON 文本）
        r#"UPDATE settings SET value = (
              SELECT COALESCE(
                jsonb_set(
                  value::jsonb,
                  '{items}',
                  COALESCE((
                    SELECT jsonb_agg(elem)
                    FROM jsonb_array_elements(COALESCE(value::jsonb->'items', '[]'::jsonb)) elem
                    WHERE elem->>'key' IS DISTINCT FROM '/smart-router'
                  ), '[]'::jsonb)
                )::text,
                value
              )
            )
            WHERE key = 'menu_config_settings'
              AND value IS NOT NULL
              AND value <> ''
              AND value::jsonb->'items' @> '[{"key":"/smart-router"}]'::jsonb"#
    );

    // 令牌名称：允许字母/数字/空格/下划线/连字符（对齐前后端校验）
    once_migration!(pool, done, "fix_api_tokens_name_allow_underscore_v1",
        "ALTER TABLE api_tokens DROP CONSTRAINT IF EXISTS chk_api_tokens_name",
        "ALTER TABLE api_tokens DROP CONSTRAINT IF EXISTS api_tokens_name_check",
        "ALTER TABLE api_tokens ADD CONSTRAINT chk_api_tokens_name CHECK (char_length(name) <= 36 AND name ~ '^[[:alnum:]_[:space:]-]+$')"
    );

    Ok(())
}
