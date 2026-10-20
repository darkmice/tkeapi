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
    // ── logs 冷归档表：热表瘦身；默认不自动归档（log_row_retention_days=0）──
    once_migration!(pool, done, "logs_archive_v1",
        r#"CREATE TABLE IF NOT EXISTS logs_archive (LIKE logs INCLUDING DEFAULTS)"#,
        r#"DO $$ BEGIN
             ALTER TABLE logs_archive ADD CONSTRAINT logs_archive_pkey PRIMARY KEY (id);
           EXCEPTION WHEN duplicate_object THEN NULL;
           END $$"#,
        "ALTER TABLE logs_archive ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW()",
        "CREATE INDEX IF NOT EXISTS idx_logs_archive_created_at ON logs_archive (created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_logs_archive_user_created ON logs_archive (user_id, created_at DESC)",
        "COMMENT ON TABLE logs_archive IS '使用日志冷归档：超期行从 logs 迁入；仪表盘统计走 usage_daily_stats'"
    );

    // 验证码防爆破：增加 attempts 计数列
    once_migration!(pool, done, "verification_codes_attempts_v1",
        "ALTER TABLE verification_codes ADD COLUMN IF NOT EXISTS attempts INTEGER NOT NULL DEFAULT 0"
    );

    // ── 查询索引兼容加固（仅 expand：并发补缺 + 清理 INVALID/临时列孤儿索引；不删业务 covering）──
    // 已收口 TIMESTAMPTZ / 已有同名索引的环境可安全重跑；失败不写 history，下次启动重试。
    once_migration!(pool, done, "query_indexes_compat_v1",
        r#"DO $inv$
        DECLARE r RECORD;
        BEGIN
          FOR r IN
            SELECT c.relname AS idxname
            FROM pg_index i
            JOIN pg_class c ON c.oid = i.indexrelid
            JOIN pg_class t ON t.oid = i.indrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE n.nspname = 'public'
              AND NOT i.indisvalid
              AND t.relname IN ('logs', 'recharge_records', 'orders', 'users', 'logs_archive')
          LOOP
            EXECUTE format('DROP INDEX IF EXISTS %I', r.idxname);
          END LOOP;
        END
        $inv$"#,
        // expand-contract 临时列索引：列已不存在时清理，避免 planner/维护噪音
        r#"DO $orphan$
        BEGIN
          IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'logs' AND column_name = 'created_at_new'
          ) THEN
            DROP INDEX IF EXISTS idx_logs_user_created_at_new;
            DROP INDEX IF EXISTS idx_logs_created_at_new;
          END IF;
          IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'recharge_records' AND column_name = 'created_at_new'
          ) THEN
            DROP INDEX IF EXISTS idx_recharge_records_user_created_at_new;
            DROP INDEX IF EXISTS idx_recharge_records_created_at_new;
          END IF;
          IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'orders' AND column_name = 'created_at_new'
          ) THEN
            DROP INDEX IF EXISTS idx_orders_user_created_at_new;
            DROP INDEX IF EXISTS idx_orders_created_at_new;
          END IF;
          IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'created_at_new'
          ) THEN
            DROP INDEX IF EXISTS idx_users_created_at_new;
          END IF;
        END
        $orphan$"#,
        // 表达式索引与业务时区桶不一致，且无查询依赖；并发删除避免锁表
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_created_at_date",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_recharge_records_user_created ON recharge_records (user_id, created_at DESC)",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_orders_user_created ON orders (user_id, created_at DESC)",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_users_referred_by ON users (referred_by) WHERE referred_by IS NOT NULL AND referred_by <> ''"
    );

    // 旧 ID 保留（已执行环境跳过）；逻辑已收口到 logs_indexes_reconcile_v1
    once_migration!(pool, done, "logs_slow_query_indexes_v1", "SELECT 1");
    once_migration!(pool, done, "logs_created_at_agg_prune_v1", "SELECT 1");

    // ── logs 索引终态（唯一维护点）：确保 agg/vision；尽力删冗余/损坏旧索引 ──
    once_migration!(pool, done, "logs_indexes_reconcile_v1",
        // 半截并发构建留下的 INVALID：同名 IF NOT EXISTS 会跳过重建，先清掉
        r#"DO $inv$
        DECLARE r RECORD;
        BEGIN
          FOR r IN
            SELECT c.relname AS idxname
            FROM pg_index i
            JOIN pg_class c ON c.oid = i.indexrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND NOT i.indisvalid
              AND c.relname IN (
                'idx_logs_created_at_agg',
                'idx_logs_vision_created_at_new'
              )
          LOOP
            EXECUTE format('DROP INDEX IF EXISTS %I', r.idxname);
          END LOOP;
        END
        $inv$"#,
        // 日统计半开区间聚合
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_created_at_agg ON logs (created_at ASC)",
        // 视觉深翻页；谓词与 SQL_VISION_ACTION_FILTER 对齐
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_vision_created_at_new ON logs (created_at DESC) WHERE (action_type = ANY (ARRAY['图片'::text, '视频'::text, '视频增强'::text, '视觉模型'::text, '视觉'::text]))",
        // 冗余/expand 残留；lock_timeout 避免与 StartupBackfill 互相堵死（曾导致 DbGate 卡 Checking model）
        r#"DO $prune$
        DECLARE idx text;
        BEGIN
          PERFORM set_config('lock_timeout', '3s', true);
          FOREACH idx IN ARRAY ARRAY[
            'idx_logs_action_created_stats_new',
            'idx_logs_created_at_timestamptz',
            'idx_logs_created_at',
            'idx_logs_user_created_at_new',
            'idx_logs_created_at_new'
          ]
          LOOP
            BEGIN
              EXECUTE format('DROP INDEX IF EXISTS %I', idx);
            EXCEPTION WHEN OTHERS THEN
              RAISE WARNING 'logs_indexes_reconcile_v1 skip drop %: %', idx, SQLERRM;
            END;
          END LOOP;
        END
        $prune$"#,
        "ANALYZE logs"
    );

    // 火山视频转素材ID：绑定「上游渠道配置」(channel_configs) + 系统增强插件种子
    once_migration!(pool, done, "upstream_asset_relay_v1",
        r#"CREATE TABLE IF NOT EXISTS upstream_asset_bindings (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            channel_config_id BIGINT NOT NULL,
            asset_base_path TEXT NOT NULL DEFAULT '',
            forward_rule_id BIGINT,
            group_id TEXT,
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_upstream_asset_bindings_config ON upstream_asset_bindings(channel_config_id)",
        "CREATE INDEX IF NOT EXISTS idx_upstream_asset_bindings_rule ON upstream_asset_bindings(forward_rule_id)",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category, allowed_levels, created_at, updated_at)
           VALUES (
             'upstream_asset_relay',
             '火山视频转素材ID',
             '为火山视频任务自动将请求中的媒体 URL 经上游渠道 CreateAsset 转为素材 ID（asset://），并生成可用转发规则',
             0,
             'system',
             'all',
             CURRENT_TIMESTAMP,
             CURRENT_TIMESTAMP
           )
           ON CONFLICT (name) DO UPDATE SET
             title = EXCLUDED.title,
             description = EXCLUDED.description,
             category = EXCLUDED.category"#
    );

    // ── 模型广场：补齐系统供应商与模型类型英文名称 ──
    once_migration!(pool, done, "model_marketplace_system_names_en_v1",
        "UPDATE model_providers SET name_en = CASE name WHEN '火山引擎' THEN 'Volcengine' WHEN '谷歌' THEN 'Google' WHEN '阿里云' THEN 'Alibaba Cloud' WHEN '腾讯云' THEN 'Tencent Cloud' WHEN '可灵 AI' THEN 'Kling AI' ELSE name_en END WHERE name_en = ''",
        "UPDATE model_types SET name_en = CASE name WHEN '视频' THEN 'Video' WHEN '图片' THEN 'Image' WHEN '音频' THEN 'Audio' WHEN '聊天' THEN 'Chat' WHEN '向量' THEN 'Embedding' WHEN '排序' THEN 'Rerank' WHEN '视频增强' THEN 'Video Enhancement' ELSE name_en END WHERE name_en = ''"
    );

    Ok(())
}
