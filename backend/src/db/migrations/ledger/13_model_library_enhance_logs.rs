/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use sqlx::PgPool;
use std::collections::HashSet;

use crate::db::migrations::{once_heavy_migration, once_migration};

pub async fn run(pool: &PgPool, done: &mut HashSet<String>) -> anyhow::Result<()> {

    // ── 腾讯云 EdgeOne MiniMax 视频生成专属转发规则（支持 OpenAI 与 MiniMax 官方路由） ──
    once_migration!(
        pool,
        done,
        "tencent_edgeone_minimax_video_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '腾讯云 EdgeOne MiniMax 视频生成', 'tencent', '通过腾讯云 EdgeOne Makers Model Pro 调用 MiniMax H3 视频生成模型。支持 OpenAI 与 MiniMax 官方路由发起，自动转换并透传多模态 content 数组与 TaskId，轮询 /minimax/query/${task_id}', '{"target_type":"minimax_video","path_rewrites":[{"old":"/v1/video/generations","new":"/minimax/v2/video_generation/tpl-3"},{"old":"/v2/video_generation","new":"/minimax/v2/video_generation/tpl-3"}],"auth_type":"bearer","poll_path":"/minimax/query/${task_id}"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '腾讯云 EdgeOne MiniMax 视频生成')"#
    );

    // ── 腾讯云 VOD 可灵视频生成专属转发规则（支持 OpenAI 与可灵官方路由） ──
    once_migration!(
        pool,
        done,
        "tencent_vod_kling_video_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '腾讯云 VOD 可灵视频生成', 'tencent', '将 OpenAI 与可灵官方视频请求（文生/图生/Omni）转换为腾讯云点播 AIGC CreateAigcVideoTask 接口。密钥格式：SecretId:SecretKey:SubAppId，模型格式：ModelName@ModelVersion', '{"target_type":"tencent_vod_video","path_rewrite":{"old":"/v1/video/generations","new":"/"},"auth_type":"tencent_vod"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '腾讯云 VOD 可灵视频生成')"#
    );

    once_migration!(pool, done, "announcements_display_time_mode_v1",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS display_time_mode VARCHAR(16) NOT NULL DEFAULT 'created'",
        "COMMENT ON COLUMN announcements.display_time_mode IS '用户端显示时间：created=新建时间, updated=最后修改时间'"
    );

    // ── OpenAI 聊天转发规则升级：支持 Chat、Responses、Messages 三协议多路径重写 ──
    once_migration!(
        pool,
        done,
        "openai_chat_three_protocols_rewrites_v1",
        r#"UPDATE forward_rules
           SET config_json = '{"path_rewrites":[{"old":"/v1/chat/completions","new":"/v1/chat/completions"},{"old":"/v1/responses","new":"/v1/responses"},{"old":"/v1/messages","new":"/v1/messages"}]}',
               description = '标准的按路径聊天透传规则，支持/v1/chat/completions、/v1/responses与/v1/messages',
               updated_at = CURRENT_TIMESTAMP
           WHERE name = 'OpenAI 兼容原生通道 (聊天)'
              OR config_json = '{"path_rewrite":{"old":"/v1/chat/completions","new":"/v1/chat/completions"}}'"#
    );

    // ── 2026-09-15 转发规则与素材索引综合更新：MiniMax三协议、素材索引、阿里百炼聊天合并、腾讯云生图及百炼原生路径去重 ──
    once_migration!(
        pool,
        done,
        "rules_and_assets_consolidation_20260915_v1",
        // 1. MiniMax 聊天规则升级：支持 Chat、Responses、Anthropic Messages 多路径重写
        r#"UPDATE forward_rules
           SET name = 'MiniMax 聊天',
               description = 'MiniMax 官方聊天转发规则，支持 /v1/chat/completions、/v1/responses 与 /anthropic/v1/messages 三种协议格式，上游统一 Bearer 鉴权',
               config_json = '{"path_rewrites":[{"old":"/v1/chat/completions","new":"/v1/chat/completions"},{"old":"/v1/responses","new":"/v1/responses"},{"old":"/anthropic/v1/messages","new":"/anthropic/v1/messages"},{"old":"/v1/messages","new":"/anthropic/v1/messages"}],"auth_type":"bearer"}',
               updated_at = CURRENT_TIMESTAMP
           WHERE (name = 'MiniMax 聊天 (Anthropic兼容)' OR name = 'MiniMax 聊天') AND is_system = 1"#,
        // 2. 素材资产管理：为本站关联云端素材建立高能部分索引，确保千万级海量数据下查询恒定毫秒级
        r#"CREATE INDEX IF NOT EXISTS idx_plugin_assets_ns_has_asset_id
           ON plugin_assets (plugin_ns, id DESC)
           WHERE asset_id IS NOT NULL AND asset_id != ''"#,
        // 3. 阿里百炼统一聊天规则：创建新规则并确保配置为 3 端点重写
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '阿里百炼 DashScope 聊天', 'aliyun',
               '阿里百炼官方聊天转发规则，支持 /v1/chat/completions、/v1/responses 与 /v1/messages（/apps/anthropic/v1/messages）协议格式，上游统一 Bearer 鉴权',
               '{"path_rewrites":[{"old":"/v1/chat/completions","new":"/compatible-mode/v1/chat/completions"},{"old":"/v1/responses","new":"/compatible-mode/v1/responses"},{"old":"/v1/messages","new":"/apps/anthropic/v1/messages"}],"auth_type":"bearer"}',
               '聊天', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '阿里百炼 DashScope 聊天')"#,
        r#"UPDATE forward_rules
        SET config_json = '{"path_rewrites":[{"old":"/v1/chat/completions","new":"/compatible-mode/v1/chat/completions"},{"old":"/v1/responses","new":"/compatible-mode/v1/responses"},{"old":"/v1/messages","new":"/apps/anthropic/v1/messages"}],"auth_type":"bearer"}',
            description = '阿里百炼官方聊天转发规则，支持 /v1/chat/completions、/v1/responses 与 /v1/messages（/apps/anthropic/v1/messages）协议格式，上游统一 Bearer 鉴权',
            updated_at = CURRENT_TIMESTAMP
        WHERE name = '阿里百炼 DashScope 聊天'"#,
        // 4. 将旧阿里百炼聊天规则(ID 27, 28)的模型自动重定向至新规则，并清理旧规则
        r#"UPDATE models m
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint IN (27, 28) THEN n.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(m.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE name = '阿里百炼 DashScope 聊天' LIMIT 1) n
        WHERE m.forward_rule_ids::jsonb @> '27'::jsonb OR m.forward_rule_ids::jsonb @> '28'::jsonb"#,
        r#"DELETE FROM forward_rules WHERE name IN ('阿里百炼 DashScope 聊天 (OpenAI兼容)', '阿里百炼 DashScope 聊天 (Anthropic兼容)')"#,
        // 5. 腾讯云 VOD 生图去重：模型重定向至同步轮询版，清理带 poll_path 规则
        r#"UPDATE models m
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN sync.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(m.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (
            SELECT id FROM forward_rules 
            WHERE category = '图片' AND rule_type IN ('tencent', 'tencent_vod') 
              AND (config_json::jsonb->>'target_type') = 'tencent_vod_image' AND NOT (config_json::jsonb ? 'poll_path')
            LIMIT 1
        ) sync,
        (
            SELECT id FROM forward_rules 
            WHERE category = '图片' AND rule_type IN ('tencent', 'tencent_vod') 
              AND (config_json::jsonb->>'target_type') = 'tencent_vod_image' AND (config_json::jsonb ? 'poll_path')
        ) del
        WHERE m.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        r#"DELETE FROM forward_rules
        WHERE category = '图片' AND rule_type IN ('tencent', 'tencent_vod')
          AND (config_json::jsonb->>'target_type') = 'tencent_vod_image' AND (config_json::jsonb ? 'poll_path')"#,
        // 6. 阿里百炼官方路径(old==new)重复生图与视频规则清理：模型重定向至标准规则，清理冗余规则
        r#"UPDATE models m
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN std.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(m.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE category = '视频' AND rule_type = 'aliyun' AND (config_json::jsonb #>> '{path_rewrite,old}') = '/v1/video/generations' LIMIT 1) std,
             (SELECT id FROM forward_rules WHERE category = '视频' AND rule_type = 'aliyun' AND (config_json::jsonb #>> '{path_rewrite,old}') = (config_json::jsonb #>> '{path_rewrite,new}') AND (config_json::jsonb #>> '{path_rewrite,old}') LIKE '%/api/v1/services/aigc/%') del
        WHERE m.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        r#"UPDATE models m
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN std.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(m.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE category = '图片' AND rule_type = 'aliyun' AND (config_json::jsonb #>> '{path_rewrite,old}') = '/v1/images/generations' LIMIT 1) std,
             (SELECT id FROM forward_rules WHERE category = '图片' AND rule_type = 'aliyun' AND (config_json::jsonb #>> '{path_rewrite,old}') = (config_json::jsonb #>> '{path_rewrite,new}') AND (config_json::jsonb #>> '{path_rewrite,old}') LIKE '%/api/v1/services/aigc/%') del
        WHERE m.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        r#"DELETE FROM forward_rules
        WHERE rule_type = 'aliyun'
          AND (config_json::jsonb #>> '{path_rewrite,old}') = (config_json::jsonb #>> '{path_rewrite,new}')
          AND (config_json::jsonb #>> '{path_rewrite,old}') LIKE '%/api/v1/services/aigc/%'"#
    );

    once_migration!(pool, done, "tencent_enhance_plugin_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, allowed_levels, category)
           VALUES ('tencent_enhance', '腾讯云增强',
           '腾讯云增强，仅作为级联阶段二，不预置模型与渠道',
           0, 'all', 'system')
           ON CONFLICT (name) DO NOTHING"#,
        r#"CREATE TABLE IF NOT EXISTS plugin_tencent_enhance_logs (
            id BIGSERIAL PRIMARY KEY,
            task_id VARCHAR(128) NOT NULL DEFAULT '',
            relay_log_id BIGINT,
            source_type VARCHAR(32) NOT NULL DEFAULT 'cascade',
            video_url TEXT NOT NULL DEFAULT '',
            target_resolution VARCHAR(32) NOT NULL DEFAULT '',
            template_id BIGINT NOT NULL DEFAULT 0,
            status VARCHAR(32) NOT NULL DEFAULT 'processing',
            output_url TEXT,
            error_message TEXT,
            latency_ms INT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_tencent_enhance_task_id ON plugin_tencent_enhance_logs(task_id)",
        "CREATE INDEX IF NOT EXISTS idx_tencent_enhance_status ON plugin_tencent_enhance_logs(status)",
        "CREATE INDEX IF NOT EXISTS idx_tencent_enhance_created_at ON plugin_tencent_enhance_logs(created_at DESC)",
        "COMMENT ON TABLE plugin_tencent_enhance_logs IS '腾讯云视频增强自测与级联阶段二流水'"
    );
    // ── 模型库与上架模型拆分：预置目录独立，更新不污染已上架 ──
    once_migration!(pool, done, "model_library_table_v1",
        r#"CREATE TABLE IF NOT EXISTS model_library (
            id BIGSERIAL PRIMARY KEY,
            mid TEXT NOT NULL,
            name TEXT NOT NULL,
            model_id TEXT NOT NULL,
            original_id TEXT NOT NULL DEFAULT '',
            model_id_alias TEXT NOT NULL DEFAULT '',
            provider_id BIGINT,
            api_provider_id BIGINT,
            type_id BIGINT,
            group_ratios TEXT NOT NULL DEFAULT '{"default":1.0}',
            billing_rule_id BIGINT,
            pre_deduction DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            is_active INTEGER NOT NULL DEFAULT 1,
            forward_rule_ids TEXT,
            enable_log_content INTEGER NOT NULL DEFAULT 0,
            site_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0,
            site_discount_enabled INTEGER NOT NULL DEFAULT 1,
            global_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0,
            global_discount_enabled INTEGER NOT NULL DEFAULT 0,
            logo TEXT,
            remark TEXT,
            description TEXT,
            feature_attributes TEXT DEFAULT '[]',
            is_system INTEGER NOT NULL DEFAULT 1,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_model_library_mid ON model_library (mid)",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS library_mid TEXT",
        "CREATE INDEX IF NOT EXISTS idx_models_library_mid ON models (library_mid)",
        "COMMENT ON TABLE model_library IS '系统模型库（预置目录）。上架后复制到 models，目录更新不覆盖已上架模型。'",
        "COMMENT ON COLUMN models.library_mid IS '来源模型库 mid；空表示自建上架模型'"
    );

    once_migration!(pool, done, "models_is_listed_v1",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS is_listed INTEGER NOT NULL DEFAULT 1",
        "COMMENT ON COLUMN models.is_listed IS '1=上架（可调用/进渠道/进广场），0=下架（在模型库）'",
        "CREATE INDEX IF NOT EXISTS idx_models_is_listed ON models (is_listed)"
    );

    // v1 带 REFERENCES 时，若 providers/types 的 id 仍是 integer，建表会失败；无 FK 再保证能建表。
    once_migration!(
        pool,
        done,
        "model_library_table_nofk_v1",
        r#"CREATE TABLE IF NOT EXISTS model_library (
            id BIGSERIAL PRIMARY KEY,
            mid TEXT NOT NULL,
            name TEXT NOT NULL,
            model_id TEXT NOT NULL,
            original_id TEXT NOT NULL DEFAULT '',
            model_id_alias TEXT NOT NULL DEFAULT '',
            provider_id BIGINT,
            api_provider_id BIGINT,
            type_id BIGINT,
            group_ratios TEXT NOT NULL DEFAULT '{"default":1.0}',
            billing_rule_id BIGINT,
            pre_deduction DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            is_active INTEGER NOT NULL DEFAULT 1,
            forward_rule_ids TEXT,
            enable_log_content INTEGER NOT NULL DEFAULT 0,
            site_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0,
            site_discount_enabled INTEGER NOT NULL DEFAULT 1,
            global_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0,
            global_discount_enabled INTEGER NOT NULL DEFAULT 0,
            logo TEXT,
            remark TEXT,
            description TEXT,
            feature_attributes TEXT DEFAULT '[]',
            is_system INTEGER NOT NULL DEFAULT 1,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_model_library_mid ON model_library (mid)"
    );

    once_migration!(pool, done, "model_library_columns_align_v1",
        "ALTER TABLE model_library ADD COLUMN IF NOT EXISTS provider_id BIGINT",
        "ALTER TABLE model_library ADD COLUMN IF NOT EXISTS api_provider_id BIGINT",
        "ALTER TABLE model_library ADD COLUMN IF NOT EXISTS type_id BIGINT",
        "ALTER TABLE model_library ADD COLUMN IF NOT EXISTS billing_rule_id BIGINT",
        "ALTER TABLE model_library ADD COLUMN IF NOT EXISTS is_active INTEGER NOT NULL DEFAULT 1",
        "ALTER TABLE model_library ADD COLUMN IF NOT EXISTS forward_rule_ids TEXT",
        "ALTER TABLE model_library ADD COLUMN IF NOT EXISTS enable_log_content INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE model_library ADD COLUMN IF NOT EXISTS is_system INTEGER NOT NULL DEFAULT 1"
    );

    once_migration!(
        pool,
        done,
        "model_library_columns_nullable_v1",
        "ALTER TABLE model_library ALTER COLUMN logo DROP NOT NULL",
        "ALTER TABLE model_library ALTER COLUMN logo TYPE TEXT",
        "ALTER TABLE model_library ALTER COLUMN remark DROP NOT NULL",
        "ALTER TABLE model_library ALTER COLUMN description DROP NOT NULL"
    );

    if !done.contains("model_library_split_seed_v1") {
        match crate::db::preset_models::split_listed_and_library(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('model_library_split_seed_v1')",
                )
                .execute(pool)
                .await;
                done.insert("model_library_split_seed_v1".into());
            }
            Err(e) => {
                tracing::warn!("model_library_split_seed_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }
    if !done.contains("volc_mediakit_image_tools_v1") {
        match crate::db::migrations::helpers::volc_mediakit_image::seed(pool).await {
            Ok(()) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('volc_mediakit_image_tools_v1')",
                )
                .execute(pool)
                .await;
                done.insert("volc_mediakit_image_tools_v1".into());
            }
            Err(e) => {
                tracing::warn!("volc_mediakit_image_tools_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    if !done.contains("volc_mediakit_image_identity_v1") {
        match crate::db::migrations::helpers::volc_mediakit_image::seed(pool).await {
            Ok(()) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('volc_mediakit_image_identity_v1')",
                )
                .execute(pool)
                .await;
                done.insert("volc_mediakit_image_identity_v1".into());
            }
            Err(e) => {
                tracing::warn!("volc_mediakit_image_identity_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    once_migration!(
        pool,
        done,
        "playground_2026_image_edit_enabled_default_off_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('playground_2026', 'image_edit_enabled', 'false', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    if !done.contains("pg2026_volc_image_edit_ready_v1") {
        match crate::db::migrations::helpers::volc_mediakit_image::activate_for_pg2026_edit(pool)
            .await
        {
            Ok(()) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('pg2026_volc_image_edit_ready_v1')",
                )
                .execute(pool)
                .await;
                done.insert("pg2026_volc_image_edit_ready_v1".into());
            }
            Err(e) => {
                tracing::warn!("pg2026_volc_image_edit_ready_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }
    // ── 日志与归档表增加上游 request_id 字段（元数据加列秒级完成，保持冷热表对齐） ──
    once_migration!(pool, done, "logs_add_upstream_request_id_v1",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS upstream_request_id TEXT",
        "ALTER TABLE logs_archive ADD COLUMN IF NOT EXISTS upstream_request_id TEXT",
        "COMMENT ON COLUMN logs.upstream_request_id IS '上游厂商返回的 request_id（响应头中提取）'",
        "COMMENT ON COLUMN logs_archive.upstream_request_id IS '上游厂商返回的 request_id（冷归档）'"
    );

    // ── 大表索引异步后台构建（专属会话内存 + CONCURRENTLY 并发建索引，启动零阻塞，不锁业务读写） ──
    once_heavy_migration!(pool, done, "logs_upstream_request_id_idx_v1",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_upstream_request_id ON logs (upstream_request_id) WHERE upstream_request_id IS NOT NULL AND upstream_request_id <> ''",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_archive_upstream_request_id ON logs_archive (upstream_request_id) WHERE upstream_request_id IS NOT NULL AND upstream_request_id <> ''",
        "ANALYZE logs",
        "ANALYZE logs_archive"
    );

    once_migration!(pool, done, "volcengine_enhance_tool_logs_v1",
        r#"CREATE TABLE IF NOT EXISTS plugin_volcengine_enhance_logs (
            id BIGSERIAL PRIMARY KEY,
            task_id VARCHAR(128) NOT NULL DEFAULT '',
            relay_log_id BIGINT,
            source_type VARCHAR(32) NOT NULL DEFAULT 'cascade',
            video_url TEXT NOT NULL DEFAULT '',
            target_resolution VARCHAR(32) NOT NULL DEFAULT '',
            status VARCHAR(32) NOT NULL DEFAULT 'processing',
            output_url TEXT,
            error_message TEXT,
            latency_ms INT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_volc_enhance_tool_task_id ON plugin_volcengine_enhance_logs(task_id)",
        "CREATE INDEX IF NOT EXISTS idx_volc_enhance_tool_status ON plugin_volcengine_enhance_logs(status)",
        "CREATE INDEX IF NOT EXISTS idx_volc_enhance_tool_created_at ON plugin_volcengine_enhance_logs(created_at DESC)",
        "COMMENT ON TABLE plugin_volcengine_enhance_logs IS '火山增强自测与级联阶段二流水'"
    );

    // ── 控制台仪表盘统计 covering 索引加固（覆盖 token_id 与聚合列，彻底消除回表与 3.15K IOPS 读突刺） ──
    once_heavy_migration!(pool, done, "logs_dashboard_covering_v2",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_admin_dashboard_covering_v2 ON logs (created_at DESC) INCLUDE (prompt_tokens, completion_tokens, cost, token_id)",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_user_dashboard_covering_v2 ON logs (user_id, created_at DESC) INCLUDE (prompt_tokens, completion_tokens, cost, token_id)",
        "ANALYZE logs"
    );

    // ── 双轨用量统计 covering 索引、logs 复合主键升级与充值订单号自动触发器 ──
    once_heavy_migration!(pool, done, "logs_and_recharge_opt_v1",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_usage_daily_stats_user_date ON usage_daily_stats (user_id, stat_date) INCLUDE (total_cost, total_pre_deduct_gift)",
        "ANALYZE usage_daily_stats",
        "CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_pkey_composite ON logs (id, created_at)",
        r#"DO $$ BEGIN
             IF EXISTS (
               SELECT 1 FROM pg_index i
               JOIN pg_class t ON t.oid = i.indrelid
               WHERE t.relname = 'logs' AND i.indisprimary AND i.indnatts >= 2
             ) THEN
               DROP INDEX IF EXISTS idx_logs_pkey_composite;
               RETURN;
             END IF;

             ALTER TABLE logs DROP CONSTRAINT IF EXISTS logs_pkey;
             ALTER TABLE logs ADD CONSTRAINT logs_pkey PRIMARY KEY USING INDEX idx_logs_pkey_composite;
           EXCEPTION WHEN OTHERS THEN
             RAISE WARNING 'logs_composite_pkey skip: %', SQLERRM;
           END $$"#,
        "ANALYZE logs",
        r#"UPDATE recharge_records
           SET order_no = (CASE WHEN wallet_type = 'gift' THEN 'G' ELSE 'T' END)
               || to_char(COALESCE(created_at, NOW()) AT TIME ZONE 'Asia/Shanghai', 'YYYYMMDDHH24MISS')
               || 'R' || substring(md5(id::text || COALESCE(created_at::text, '')) from 1 for 8)
           WHERE order_no IS NULL OR TRIM(order_no) = ''"#,
        r#"CREATE OR REPLACE FUNCTION trg_recharge_auto_order_no()
           RETURNS TRIGGER AS $$
           BEGIN
               IF NEW.order_no IS NULL OR TRIM(NEW.order_no) = '' THEN
                   NEW.order_no := (CASE WHEN NEW.wallet_type = 'gift' THEN 'G' ELSE 'T' END)
                       || to_char(CLOCK_TIMESTAMP() AT TIME ZONE 'Asia/Shanghai', 'YYYYMMDDHH24MISS')
                       || 'R' || substring(md5(random()::text || clock_timestamp()::text) from 1 for 8);
               END IF;
               RETURN NEW;
           END;
           $$ LANGUAGE plpgsql;"#,
        "DROP TRIGGER IF EXISTS trg_recharge_auto_order_no ON recharge_records",
        "CREATE TRIGGER trg_recharge_auto_order_no BEFORE INSERT ON recharge_records FOR EACH ROW EXECUTE FUNCTION trg_recharge_auto_order_no()"
    );

    // ── 普通用户邮箱/手机改为限额共享，身份以 UID 唯一区分 ──
    once_migration!(pool, done, "users_shared_email_mobile_v1",
        r#"DO $$
        DECLARE r record;
        BEGIN
          FOR r IN
            SELECT c.conname
            FROM pg_constraint c
            JOIN pg_class t ON c.conrelid = t.oid
            WHERE t.relname = 'users' AND c.contype = 'u'
              AND pg_get_constraintdef(c.oid) ILIKE '%(email)%'
          LOOP
            EXECUTE format('ALTER TABLE users DROP CONSTRAINT IF EXISTS %I', r.conname);
          END LOOP;
          FOR r IN
            SELECT i.relname AS idx
            FROM pg_index x
            JOIN pg_class i ON i.oid = x.indexrelid
            JOIN pg_class t ON t.oid = x.indrelid
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY (x.indkey)
            WHERE t.relname = 'users'
              AND x.indisunique
              AND NOT x.indisprimary
              AND a.attname = 'email'
              AND x.indnkeyatts = 1
          LOOP
            EXECUTE format('DROP INDEX IF EXISTS %I', r.idx);
          END LOOP;
        END $$"#,
        "CREATE INDEX IF NOT EXISTS idx_users_email ON users (email)",
        "CREATE INDEX IF NOT EXISTS idx_users_mobile ON users (mobile) WHERE mobile IS NOT NULL AND btrim(mobile) <> ''",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_placeholder_email_unique ON users (email) WHERE email LIKE '%@tokensbyte.local'"
    );

    // ── 单条邮箱/手机绑定上限覆盖（站点默认仍走 registration 设置） ──
    once_migration!(
        pool,
        done,
        "contact_bind_limits_v1",
        r#"CREATE TABLE IF NOT EXISTS contact_bind_limits (
            kind TEXT NOT NULL CHECK (kind IN ('email', 'mobile')),
            value TEXT NOT NULL CHECK (btrim(value) <> ''),
            max_accounts INTEGER NOT NULL CHECK (max_accounts BETWEEN 1 AND 99),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            PRIMARY KEY (kind, value)
        )"#
    );

    // ── 视觉 Tab 部分索引扩展（精确对齐系统固定 4 个视觉类型，对齐 SQL_VISION_ACTION_FILTER） ──
    once_heavy_migration!(pool, done, "logs_vision_index_include_image_enhance_v1",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_vision_created_at_v2 ON logs (created_at DESC) WHERE (action_type = ANY (ARRAY['图片'::text, '视频'::text, '视频增强'::text, '图像增强'::text]))",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_user_vision_created_v2 ON logs (user_id, created_at DESC) WHERE (action_type = ANY (ARRAY['图片'::text, '视频'::text, '视频增强'::text, '图像增强'::text]))",
        "ANALYZE logs"
    );

    once_migration!(
        pool,
        done,
        "playground_2026_video_edit_workbench_enabled_default_off_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('playground_2026', 'video_edit_workbench_enabled', 'false', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    // ── 火山 Seedream 5.0 Pro 仅支持单图文档说明与多语言更新 ──
    if !done.contains("update_volc_image_doc_pro_single_v1") {
        let zh_content = include_str!("../../../api/plugins/docs_api/default_docs/zh/volc-image.md");
        let en_content = include_str!("../../../api/plugins/docs_api/default_docs/en/volc-image.md");
        let _ = sqlx::query("UPDATE plugin_docs SET content = $1, updated_at = CURRENT_TIMESTAMP WHERE slug = 'volc-image'")
            .bind(zh_content)
            .execute(pool)
            .await;
        let _ = sqlx::query("UPDATE plugin_docs_intl SET content = $1, updated_at = CURRENT_TIMESTAMP WHERE lang = 'en' AND doc_id IN (SELECT id FROM plugin_docs WHERE slug = 'volc-image')")
            .bind(en_content)
            .execute(pool)
            .await;
        let _ = sqlx::query(
            "INSERT INTO sys_migration_history (id) VALUES ('update_volc_image_doc_pro_single_v1')",
        )
        .execute(pool)
        .await;
        done.insert("update_volc_image_doc_pro_single_v1".into());
        tracing::info!("已同步火山 Seedream 5.0 Pro 文档说明至数据库");
    }

    if !done.contains("volc_enhance_type_unify_v1") {
        match crate::db::migrations::helpers::volc_mediakit_image::seed(pool).await {
            Ok(()) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('volc_enhance_type_unify_v1')",
                )
                .execute(pool)
                .await;
                done.insert("volc_enhance_type_unify_v1".into());
            }
            Err(e) => {
                tracing::warn!("volc_enhance_type_unify_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    once_migration!(pool, done, "model_discount_schedule_v1",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS discount_schedule TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN models.discount_schedule IS '折扣限价与全站折扣的按天时段规划 JSON。enabled=false 时使用统一折扣'",
        "ALTER TABLE model_library ADD COLUMN IF NOT EXISTS discount_schedule TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN model_library.discount_schedule IS '折扣限价与全站折扣的按天时段规划 JSON。enabled=false 时使用统一折扣'"
    );

    if !done.contains("volc_mediakit_image_billing_pid_v1") {
        match crate::db::migrations::helpers::volc_mediakit_image::seed(pool).await {
            Ok(()) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('volc_mediakit_image_billing_pid_v1')",
                )
                .execute(pool)
                .await;
                done.insert("volc_mediakit_image_billing_pid_v1".into());
            }
            Err(e) => {
                tracing::warn!("volc_mediakit_image_billing_pid_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    once_migration!(
        pool,
        done,
        "ha_plugin_preset_rules_seed_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('high_availability_channel', 'ha_rules', '{"def":"r1","rules":[{"id":"r1","name":"通用对话","retries":3,"budget":900,"ttfb":0,"err":"first","melt":{"401":1800,"402":1800,"404":3,"429":60,"5xx":300}},{"id":"r2","name":"图像生成","retries":2,"budget":1200,"ttfb":120,"err":"last","melt":{"401":1800,"402":1800,"404":3,"429":60,"5xx":300}},{"id":"r3","name":"深度思考","retries":3,"budget":1200,"ttfb":60,"err":"first","melt":{"401":1800,"402":1800,"404":3,"429":60,"5xx":300}},{"id":"r4","name":"音视频长任务","retries":2,"budget":1800,"ttfb":0,"err":"last","melt":{"401":1800,"402":1800,"404":3,"429":60,"5xx":300}},{"id":"r5","name":"向量与排序","retries":3,"budget":600,"ttfb":30,"err":"first","melt":{"401":1800,"402":1800,"404":3,"429":60,"5xx":300}},{"id":"r6","name":"极速容灾","retries":4,"budget":300,"ttfb":5,"err":"first","melt":{"401":1800,"402":1800,"404":3,"429":60,"5xx":300}}]}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    // ── 腾讯云 VOD 火山视频生成专属转发规则（支持 OpenAI 与火山方舟官方路由） ──
    once_migration!(
        pool,
        done,
        "tencent_vod_volcengine_video_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '腾讯云 VOD 火山视频生成', 'tencent', '将 OpenAI 与火山方舟官方视频请求转换为腾讯云点播 AIGC CreateAigcVideoTask 接口。密钥格式：SecretId:SecretKey:SubAppId，模型格式：ModelName@ModelVersion', '{"target_type":"tencent_vod_video","path_rewrite":{"old":"/v1/video/generations","new":"/"},"path_accept":["/api/v3/contents/generations/tasks"],"auth_type":"tencent_vod"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '腾讯云 VOD 火山视频生成')"#
    );

    // ── 视频模型 Token 细分与日统计预聚合字段（has_video 与细分指标） ──
    once_migration!(pool, done, "logs_has_video_and_stats_columns_v1",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS has_video SMALLINT NOT NULL DEFAULT 0",
        "ALTER TABLE logs_archive ADD COLUMN IF NOT EXISTS has_video SMALLINT NOT NULL DEFAULT 0",
        "ALTER TABLE usage_daily_stats ADD COLUMN IF NOT EXISTS prompt_tokens BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE usage_daily_stats ADD COLUMN IF NOT EXISTS completion_tokens BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE usage_daily_stats ADD COLUMN IF NOT EXISTS with_video_tokens BIGINT NOT NULL DEFAULT 0"
    );

    // ── 实时用量高性能通用覆盖索引（无模型硬编码，避免自定义模型漏索引） ──
    once_heavy_migration!(pool, done, "idx_logs_stat_covering_v1",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_stat_covering ON logs (created_at DESC) INCLUDE (model, prompt_tokens, completion_tokens, has_video, cost, status_code, is_completed)",
        "ANALYZE logs"
    );

    if !done.contains("playground_2026_mark_mask_bbox_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_2026_mark_capabilities(
            pool,
        )
        .await
        {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_mark_mask_bbox_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_mark_mask_bbox_v1".into());
                tracing::info!("创作中心2026 标记蒙版/坐标框回填完成，更新 {} 条配置", n);
            }
            Err(e) => {
                tracing::warn!("playground_2026_mark_mask_bbox_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    if !done.contains("playground_2026_mark_mask_exact_v1") {
        match crate::db::migrations::helpers::playground::restrict_playground_2026_mask_models(pool)
            .await
        {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_mark_mask_exact_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_mark_mask_exact_v1".into());
                tracing::info!("创作中心2026 蒙版仅保留 gpt-image-2，去掉 {} 条渠道别名", n);
            }
            Err(e) => {
                tracing::warn!("playground_2026_mark_mask_exact_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    once_migration!(
        pool,
        done,
        "playground_2026_scratch_objects_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_scratch_objects (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            object_key TEXT NOT NULL,
            file_url TEXT NOT NULL DEFAULT '',
            storage_provider TEXT NOT NULL DEFAULT 'tos',
            purpose TEXT NOT NULL DEFAULT 'mask',
            file_size BIGINT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expire_at TIMESTAMPTZ NOT NULL
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg2026_scratch_expire ON playground_2026_scratch_objects (expire_at)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_scratch_user_key ON playground_2026_scratch_objects (user_id, object_key)",
        "COMMENT ON TABLE playground_2026_scratch_objects IS '创作中心2026笔画蒙版等临时对象，到期或用完即删'"
    );

    // ── 用户最后活跃 IP，供管理端用户详细展示 ──
    once_migration!(
        pool,
        done,
        "users_last_active_ip_v1",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS last_active_ip TEXT NOT NULL DEFAULT ''"
    );

    #[cfg(feature = "commercial_plugins")]
    once_migration!(
        pool,
        done,
        "mobile_app_plugin_v1",
        r#"INSERT INTO plugins (
            name, title, description, is_enabled, allowed_levels, category,
            show_in_admin_menu, admin_menu_sort, admin_menu_title, admin_menu_default_tab
        ) VALUES (
            'mobile_app', '移动端APP管理',
            '手机端 App 的版本、维护开关、底部入口和接口目录。App 启动配置只从本插件读取。',
            0, 'all', 'user',
            1, 40, '', 'app_client'
        ) ON CONFLICT (name) DO NOTHING"#
    );

    #[cfg(not(feature = "commercial_plugins"))]
    once_migration!(
        pool,
        done,
        "mobile_app_plugin_strip_v1",
        "DELETE FROM plugin_configs WHERE plugin_name = 'mobile_app'",
        "DELETE FROM plugins WHERE name = 'mobile_app'"
    );

    // 默认安装写进运营列表的预置副本：mid 与目录 mid 相同。已绑渠道、令牌或产生过调用的留下。
    once_migration!(
        pool,
        done,
        "preset_models_stay_in_library_v1",
        r#"DELETE FROM models AS m
           WHERE m.is_listed = 1
             AND m.library_mid IS NOT NULL
             AND m.library_mid <> ''
             AND m.library_mid = m.mid
             AND NOT EXISTS (
               SELECT 1 FROM channels c
               WHERE position(m.mid in c.models) > 0
                  OR position(m.mid in c.model_mapping) > 0
                  OR position(m.model_id in c.models) > 0
             )
             AND NOT EXISTS (
               SELECT 1 FROM api_tokens t
               WHERE position(m.mid in t.allowed_models) > 0
                  OR position(m.model_id in t.allowed_models) > 0
             )
             AND NOT EXISTS (
               SELECT 1 FROM logs l
               WHERE l.model = m.model_id OR l.model = m.mid
             )
             AND NOT EXISTS (
               SELECT 1 FROM logs_archive l
               WHERE l.model = m.model_id OR l.model = m.mid
             )"#
    );

    // 目录里的 MediaKit 预置模型跟随运营行的激活状态。没有已激活运营行时保持关闭，避免插件一开就占满模型仓库。
    once_migration!(
        pool,
        done,
        "volc_mediakit_library_active_follows_models_v1",
        r#"UPDATE model_library AS l
           SET is_active = CASE
                 WHEN EXISTS (
                   SELECT 1 FROM models m WHERE m.mid = l.mid AND m.is_active = 1
                 ) THEN 1
                 ELSE 0
               END,
               updated_at = CURRENT_TIMESTAMP
           WHERE l.mid IN (
             'vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep', 'vie-en', 'vir-bg'
           )"#
    );

    if !done.contains("volc_mediakit_library_under_enhance_v1") {
        match crate::db::migrations::helpers::volc_mediakit_image::ensure_enhance_library_catalog(
            pool,
        )
        .await
        {
            Ok(()) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('volc_mediakit_library_under_enhance_v1')",
                )
                .execute(pool)
                .await;
                done.insert("volc_mediakit_library_under_enhance_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "volc_mediakit_library_under_enhance_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // Seedance 2.5 系统预设补 1080p。只补缺失档，已有自定义 1080p 与时段倍率保持不动。
    once_migration!(
        pool,
        done,
        "seedance25_official_1080p_rate_v1",
        r#"UPDATE billing_rules
        SET extended_config = jsonb_set(
              extended_config::jsonb,
              '{resolution_rates,1080p}',
              '{"with_video":46,"without_video":77}'::jsonb,
              true
            )::text,
            updated_at = CURRENT_TIMESTAMP
        WHERE is_system = 1
          AND (name = 'Seedance2.5官方计费' OR pid = '73119')
          AND jsonb_typeof(extended_config::jsonb -> 'resolution_rates') = 'object'
          AND NOT (extended_config::jsonb -> 'resolution_rates' ? '1080p')"#
    );
    Ok(())
}
