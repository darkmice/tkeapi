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
    // ── DocsApi 站点 API 教程文档增强插件初始化 ──
    if !done.contains("docs_api_init_v5") {
        // 1. 创建 plugin_docs 表
        let _ = sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS plugin_docs (
                id SERIAL PRIMARY KEY,
                parent_id INTEGER NULL REFERENCES plugin_docs(id) ON DELETE CASCADE,
                title VARCHAR(255) NOT NULL,
                content TEXT DEFAULT '',
                is_dir INTEGER NOT NULL DEFAULT 0,
                sort_order INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                slug VARCHAR(255) DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (now()::text),
                updated_at TEXT NOT NULL DEFAULT (now()::text)
            )"#,
        )
        .execute(pool)
        .await;

        // 2. 提前创建 plugin_docs_intl 国际化表，保证种子数据 seed_default_docs_direct 可以顺利写入翻译数据
        let _ = sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS plugin_docs_intl (
                id SERIAL PRIMARY KEY,
                doc_id INTEGER NOT NULL REFERENCES plugin_docs(id) ON DELETE CASCADE,
                lang VARCHAR(10) NOT NULL,
                title VARCHAR(255) NOT NULL,
                content TEXT DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (now()::text),
                updated_at TEXT NOT NULL DEFAULT (now()::text),
                UNIQUE(doc_id, lang)
            )"#,
        )
        .execute(pool)
        .await;

        // 3. 注册 docs_api 插件（默认关闭）
        let _ = sqlx::query(
            "INSERT INTO plugins (name, title, description, is_enabled, allowed_levels, category, created_at, updated_at) \
             VALUES ('docs_api', 'DocsApi文档', '提供站点 API 教程的文档管理系统，支持多级目录大纲与 Markdown 内容手动编辑。', 0, 'all', 'user', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
             ON CONFLICT (name) DO UPDATE SET title = EXCLUDED.title, description = EXCLUDED.description, is_enabled = EXCLUDED.is_enabled"
        ).execute(pool).await;

        // 4. 写入初始数据
        if let Err(e) = crate::api::plugins::docs_api::seed_default_docs_direct(pool).await {
            tracing::warn!("Failed to seed default docs: {:?}", e);
        }

        let _ = sqlx::query("INSERT INTO sys_migration_history (id) VALUES ('docs_api_init_v5')")
            .execute(pool)
            .await;
        done.insert("docs_api_init_v5".into());
        tracing::info!("✅ DocsApi 文档插件初始化完成");
    }

    // ── DocsApi 插件新增 slug 字段（受一次性迁移保护） ──
    once_migration!(
        pool,
        done,
        "docs_api_add_slug_v1",
        "ALTER TABLE plugin_docs ADD COLUMN IF NOT EXISTS slug VARCHAR(255) DEFAULT ''"
    );

    // ── 级联转发规则补充默认 res_mul（分辨率倍率，缺省 1.0 不影响现网计价）──
    once_migration!(
        pool,
        done,
        "cascade_res_mul_v1",
        r#"UPDATE forward_rules
           SET config_json = (COALESCE(config_json::jsonb, '{}'::jsonb) || '{"res_mul":{"720p":2.15,"1080p":2.25,"2k":2.5,"4k":4.0}}'::jsonb)::text
           WHERE name = '火山方舟 级联视频生成'
             AND (config_json::jsonb -> 'res_mul') IS NULL"#
    );

    // ── playground_projects 新增 is_pinned 字段（受一次性迁移保护） ──
    once_migration!(pool, done, "pg_projects_add_is_pinned_v1",
        "ALTER TABLE playground_projects ADD COLUMN IF NOT EXISTS is_pinned INTEGER NOT NULL DEFAULT 0"
    );

    // ── logs 表新增 is_completed 字段：标识任务是否已终结 ──
    // ── 初始化日志终结标记及条件索引（受一次性迁移保护） ──
    once_migration!(pool, done, "logs_add_is_completed_v1",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS is_completed SMALLINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN logs.is_completed IS '任务是否已终结(1=已完成,0=进行中或待结算)'",
        "UPDATE logs SET is_completed = 1 WHERE is_completed = 0 AND (billing_detail IS NULL OR billing_detail NOT LIKE '%冻结%')",
        "CREATE INDEX IF NOT EXISTS idx_logs_is_completed_pending ON logs (id DESC) WHERE is_completed = 0",
        "UPDATE logs SET is_completed = 1 WHERE is_completed = 0 AND (billing_detail LIKE '[测试渠道，不扣费]%' OR endpoint LIKE 'test|%')"
    );

    // ── 统一合并的零散 DML 一次性回填 ──
    once_migration!(pool, done, "backfill_misc_data_v1",
        "UPDATE user_levels SET is_default = 1 WHERE group_key = 'default' AND NOT EXISTS (SELECT 1 FROM user_levels WHERE is_default = 1)",
        "UPDATE forward_rules SET category = '音频' WHERE category = '语音'",
        "UPDATE forward_rules SET rule_type = 'aliyun' WHERE name LIKE '%阿里百炼%' AND rule_type != 'aliyun'",
        "UPDATE forward_rules SET config_json = '{\"target_type\":\"anthropic\",\"path_rewrite\":{\"old\":\"/v1/chat/completions\",\"new\":\"/v1/messages\"},\"auth_type\":\"x-api-key\"}', description = '将 OpenAI 格式请求转换为 Anthropic Messages API 格式，接口 /v1/messages' WHERE name = 'Anthropic 原生转化' AND is_system = 1",
        "UPDATE forward_rules SET eid = '1' || floor(random() * 9000 + 1000)::text WHERE eid = '' OR eid IS NULL",
        "UPDATE billing_rules SET pid = '7' || floor(random() * 9000 + 1000)::text WHERE is_system = 1 AND (pid = '' OR pid IS NULL)",
        "UPDATE billing_rules SET pid = '6' || floor(random() * 9000 + 1000)::text WHERE is_system = 0 AND (pid = '' OR pid IS NULL)",
        "UPDATE channel_configs SET yid = '3' || floor(random() * 9000 + 1000)::text WHERE yid = '' OR yid IS NULL",
        "UPDATE model_types SET logo = 'sora' WHERE name = '视频' AND (logo IS NULL OR logo = '')",
        "UPDATE model_types SET logo = 'midjourney' WHERE name = '图片' AND (logo IS NULL OR logo = '')",
        "UPDATE model_types SET logo = 'suno' WHERE name = '音频' AND (logo IS NULL OR logo = '')",
        "UPDATE model_types SET logo = 'chatgpt' WHERE name = '聊天' AND (logo IS NULL OR logo = '')",
        // 仅回填空 logo/remark，禁止改写 sort_order（管理端自定义排序升级后须保留）
        "UPDATE model_types SET logo = CASE WHEN logo IS NULL OR logo = '' THEN 'volcengine' ELSE logo END, remark = CASE WHEN remark IS NULL OR remark = '' THEN '视频画质增强与字幕擦除处理模型' ELSE remark END WHERE name = '视频增强' AND (logo IS NULL OR logo = '' OR remark IS NULL OR remark = '')"
    );

    // ── usage_daily_stats 每日使用统计落地表及 logs 高性能查询索引（受一次性迁移保护） ──
    once_migration!(pool, done, "add_usage_daily_stats_v1",
        r#"CREATE TABLE IF NOT EXISTS usage_daily_stats (
            id BIGSERIAL PRIMARY KEY,
            stat_date DATE NOT NULL,
            user_id TEXT NOT NULL,
            model TEXT NOT NULL,
            token_id BIGINT NOT NULL DEFAULT -1,
            channel_id BIGINT NOT NULL DEFAULT -1,
            action_type TEXT NOT NULL DEFAULT '',
            total_requests BIGINT NOT NULL DEFAULT 0,
            total_tokens BIGINT NOT NULL DEFAULT 0,
            total_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            total_pre_deduct_gift DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            success_count BIGINT NOT NULL DEFAULT 0,
            fail_count BIGINT NOT NULL DEFAULT 0,
            ext_json JSONB,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE usage_daily_stats IS '使用量每日统计表 (Lambda 离线统计落地表)'",
        "COMMENT ON COLUMN usage_daily_stats.id IS '自增主键'",
        "COMMENT ON COLUMN usage_daily_stats.stat_date IS '统计日期 (YYYY-MM-DD)'",
        "COMMENT ON COLUMN usage_daily_stats.user_id IS '用户ID'",
        "COMMENT ON COLUMN usage_daily_stats.model IS '模型名称'",
        "COMMENT ON COLUMN usage_daily_stats.token_id IS '令牌ID (-1代表无令牌)'",
        "COMMENT ON COLUMN usage_daily_stats.channel_id IS '渠道ID (-1代表无渠道)'",
        "COMMENT ON COLUMN usage_daily_stats.action_type IS '动作类型(聊天,图片,视频等)'",
        "COMMENT ON COLUMN usage_daily_stats.total_requests IS '总请求数'",
        "COMMENT ON COLUMN usage_daily_stats.total_tokens IS '总消费 tokens 数量'",
        "COMMENT ON COLUMN usage_daily_stats.total_cost IS '总消费金额'",
        "COMMENT ON COLUMN usage_daily_stats.total_pre_deduct_gift IS '总消费赠送余额金额'",
        "COMMENT ON COLUMN usage_daily_stats.success_count IS '状态码 2xx 的成功请求数'",
        "COMMENT ON COLUMN usage_daily_stats.fail_count IS '状态码非 2xx 的失败请求数'",
        "COMMENT ON COLUMN usage_daily_stats.ext_json IS '扩展元数据 JSONB (供未来新指标无感扩展使用)'",
        "CREATE UNIQUE INDEX IF NOT EXISTS uidx_usage_daily_stats_dims ON usage_daily_stats (stat_date, user_id, model, token_id, channel_id, action_type)",
        "CREATE INDEX IF NOT EXISTS idx_usage_daily_stats_date_user ON usage_daily_stats (stat_date, user_id)",
        "CREATE INDEX IF NOT EXISTS idx_logs_created_at_timestamptz ON logs (created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_logs_user_created_timestamptz ON logs (user_id, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_logs_date_created_at ON logs ((SUBSTRING(created_at FROM 1 FOR 10)))",
        "CREATE INDEX IF NOT EXISTS idx_logs_stats_opt ON logs (user_id, created_at DESC) INCLUDE (cost, status_code, pre_deduct_gift)",
        "CREATE INDEX IF NOT EXISTS idx_logs_created_at_stats_opt ON logs (created_at DESC) INCLUDE (cost, status_code, pre_deduct_gift)"
    );

    once_migration!(
        pool,
        done,
        "add_ha_cooldown_404_v2",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('high_availability_channel', 'ha_cooldown_404', '3', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) 
           DO UPDATE SET config_value = '3', updated_at = CURRENT_TIMESTAMP 
           WHERE plugin_configs.config_value = '10'"#
    );

    Ok(())
}
