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
    // ComfyUI 接入：服务/工作流/任务表 + 系统增强插件种子（转发规则由插件运行时生成，不预置）
    once_migration!(pool, done, "comfyui_bridge_v1",
        r#"CREATE TABLE IF NOT EXISTS comfyui_servers (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            base_url TEXT NOT NULL,
            auth_header TEXT NOT NULL DEFAULT '',
            timeout_secs INTEGER NOT NULL DEFAULT 120,
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS comfyui_workflows (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            server_id BIGINT NOT NULL,
            workflow_json TEXT NOT NULL DEFAULT '{}',
            prompt_template TEXT NOT NULL DEFAULT '',
            param_map TEXT NOT NULL DEFAULT '{}',
            output_node_id TEXT NOT NULL DEFAULT '',
            forward_rule_id BIGINT,
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_comfyui_workflows_server ON comfyui_workflows(server_id)",
        "CREATE INDEX IF NOT EXISTS idx_comfyui_workflows_rule ON comfyui_workflows(forward_rule_id)",
        r#"CREATE TABLE IF NOT EXISTS comfyui_jobs (
            log_id BIGINT PRIMARY KEY,
            prompt_id TEXT NOT NULL,
            workflow_id BIGINT NOT NULL,
            server_id BIGINT NOT NULL,
            output_url TEXT
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_comfyui_jobs_prompt ON comfyui_jobs(prompt_id)",
        "COMMENT ON TABLE comfyui_jobs IS 'ComfyUI 任务：log_id=logs.id'",
        "COMMENT ON COLUMN comfyui_jobs.log_id IS '关联主日志表 logs.id'",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category, allowed_levels, created_at, updated_at)
           VALUES (
             'comfyui_bridge',
             'ComfyUI 接入',
             '管理 ComfyUI 服务地址与工作流，经 OpenAI 视频路由提交并轮询生成结果',
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

    // 工作流 ↔ 服务节点多对多；旧 server_id 回填后改为可空
    once_migration!(pool, done, "comfyui_workflow_nodes_v1",
        r#"CREATE TABLE IF NOT EXISTS comfyui_workflow_nodes (
            workflow_id BIGINT NOT NULL,
            server_id BIGINT NOT NULL,
            PRIMARY KEY (workflow_id, server_id)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_comfyui_wf_nodes_server ON comfyui_workflow_nodes(server_id)",
        r#"INSERT INTO comfyui_workflow_nodes (workflow_id, server_id)
           SELECT id, server_id FROM comfyui_workflows
           WHERE server_id IS NOT NULL
           ON CONFLICT DO NOTHING"#,
        "ALTER TABLE comfyui_workflows ALTER COLUMN server_id DROP NOT NULL"
    );

    once_migration!(pool, done, "comfyui_dispatch_v1",
        "ALTER TABLE comfyui_servers ADD COLUMN IF NOT EXISTS priority INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE comfyui_servers ADD COLUMN IF NOT EXISTS weight INTEGER NOT NULL DEFAULT 1",
        "ALTER TABLE comfyui_servers ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        r#"CREATE TABLE IF NOT EXISTS comfyui_dispatch_rules (
            id BIGSERIAL PRIMARY KEY,
            code TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            remark TEXT,
            is_active INTEGER NOT NULL DEFAULT 1,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"INSERT INTO comfyui_dispatch_rules (code, name, remark, is_active, sort_order)
           VALUES
             ('priority_weight', '权重优先', '先取优先级最高的节点，同分再按权重随机', 1, 1),
             ('random', '随机调用', '在已选且启用的节点中均匀随机', 1, 2),
             ('sequential', '顺序调用', '按节点排序依次轮流', 1, 3),
             ('least_busy', '空闲优先', '未完成任务最少的节点优先，同分再按权重优先', 1, 4)
           ON CONFLICT (code) DO NOTHING"#
    );

    // ── 模型来源：系统预设 / 自定义 ──
    once_migration!(pool, done, "models_is_system_v1",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS is_system INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN models.is_system IS '1=系统预设，0=自定义'",
        "UPDATE models SET is_system = 1 WHERE mid IN ('vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep', 'dbs-sr', 'dbs-fs')"
    );

    // ── 安装时写入系统预设模型（官方计费 + 转发规则）──
    if !done.contains("seed_system_preset_models_v1") {
        match crate::db::preset_models::seed_system_preset_models(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('seed_system_preset_models_v1')",
                )
                .execute(pool)
                .await;
                done.insert("seed_system_preset_models_v1".into());
                tracing::info!("系统预设模型种子写入完成，新增 {} 条", n);
            }
            Err(e) => {
                tracing::warn!("seed_system_preset_models_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    once_migration!(pool, done, "channel_configs_upstream_rate_sync_v1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS upstream_system TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS upstream_group TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS upstream_sync_interval_minutes INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS upstream_sync_rate_add DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS upstream_synced_at TIMESTAMPTZ",
        "COMMENT ON COLUMN channel_configs.upstream_system IS '上游系统: 兼容/官方/newapi/akeapi/火山引擎/阿里云，空=未选'",
        "COMMENT ON COLUMN channel_configs.upstream_group IS 'NewAPI 等已选同步分组名'",
        "COMMENT ON COLUMN channel_configs.upstream_sync_interval_minutes IS '分组倍率自动同步间隔分钟，0=关闭'",
        "COMMENT ON COLUMN channel_configs.upstream_sync_rate_add IS '同步时叠加到分组倍率上的增量，0=不叠加'",
        "COMMENT ON COLUMN channel_configs.upstream_synced_at IS '上次成功同步分组倍率的时间'"
    );

    // task_id 按 id 倒序取最新一行；pending 轮询；旧单列索引由复合索引覆盖
    once_migration!(pool, done, "logs_task_id_id_pending_poll_idx_v1",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_task_id_id ON logs (task_id, id DESC)",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_pending_poll ON logs (id ASC) WHERE is_completed = 0 AND status_code = 200",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_task_id"
    );

    once_migration!(
        pool,
        done,
        "comfyui_jobs_debug_error_v1",
        "ALTER TABLE comfyui_jobs ADD COLUMN IF NOT EXISTS debug_error TEXT",
        "COMMENT ON COLUMN comfyui_jobs.debug_error IS '节点失败调试信息，仅插件任务日志'"
    );

    once_migration!(
        pool,
        done,
        "comfyui_scenes_v1",
        "ALTER TABLE comfyui_workflows ADD COLUMN IF NOT EXISTS scenes TEXT NOT NULL DEFAULT '[]'",
        "DROP INDEX IF EXISTS idx_comfyui_workflows_rule",
        "ALTER TABLE comfyui_workflows DROP COLUMN IF EXISTS forward_rule_id"
    );

    once_migration!(pool, done, "comfyui_least_busy_live_queue_remark_v1",
        "UPDATE comfyui_dispatch_rules SET remark = '先排除连不上的节点；优先无运行任务，再选排队最少的' WHERE code = 'least_busy'"
    );

    // 菜单配置：将 /wallet 的 label_zh 从「资产中心」更新为「我的钱包」
    once_migration!(
        pool,
        done,
        "update_menu_wallet_label_to_my_wallet_20260815",
        "UPDATE settings SET value = replace(
            replace(value, '\"label_zh\":\"资产中心\"', '\"label_zh\":\"我的钱包\"'),
            '\"label_zh\": \"资产中心\"', '\"label_zh\": \"我的钱包\"'
        ) WHERE key = 'menu_config_settings'"
    );

    // 站点设置：版权信息默认值对齐为「© 2026 TkeAPI. All rights reserved.」
    once_migration!(pool, done, "update_site_copyright_default_20260815",
        "UPDATE settings SET value = replace(
            replace(
                replace(
                    replace(value, '\"copyright\":\"© 2026 Tkeapi. All rights reserved.\"', '\"copyright\":\"© 2026 TkeAPI. All rights reserved.\"'),
                    '\"copyright\": \"© 2026 Tkeapi. All rights reserved.\"', '\"copyright\": \"© 2026 TkeAPI. All rights reserved.\"'
                ),
                '\"copyright\":\"© 2026 MyCompany. All rights reserved.\"', '\"copyright\":\"© 2026 TkeAPI. All rights reserved.\"'
            ),
            '\"copyright\": \"© 2026 MyCompany. All rights reserved.\"', '\"copyright\": \"© 2026 TkeAPI. All rights reserved.\"'
        ) WHERE key = 'site_settings'"
    );

    // ── 站点门户增强版 DOCS 区分系统预设与自建文档 ──
    once_migration!(pool, done, "site_portal_pro_docs_is_preset_v1",
        "ALTER TABLE site_portal_pro_docs ADD COLUMN IF NOT EXISTS is_preset INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE site_portal_pro_doc_categories ADD COLUMN IF NOT EXISTS is_preset INTEGER NOT NULL DEFAULT 0",
        r#"UPDATE site_portal_pro_doc_categories
           SET is_preset = 1
           WHERE name IN ('使用指南', 'API 参考', '部署安装', '商务合作')"#,
        r#"UPDATE site_portal_pro_docs
           SET is_preset = 1
           WHERE slug IN (
             'deploy-install', 'api-reference', 'models-channels', 'business-cooperation',
             'deploy-quickstart', 'docker-compose', 'reverse-proxy', 'config-env',
             'api-quickstart', 'protocol-openai', 'protocol-claude', 'protocol-gemini',
             'protocol-midjourney', 'protocol-sunox', 'audio-video', 'models-billing',
             'sdk-integration', 'errors-faq', 'changelog'
           ) OR REPLACE(LOWER(title), ' ', '') IN (
             '使用指南', 'api参考', '部署安装', '商务合作',
             '模型与渠道配置指南', '商务与合作咨询', '快速开始与docker部署', 'docker-compose部署',
             'nginx反向代理配置', '环境变量与配置文件', '快速调用接入', 'openai官方协议接入',
             'anthropicclaude协议接入', 'googlegemini协议接入', 'midjourney绘画接入',
             'suno/udio音乐协议接入', '音视频处理协议接入', '模型定价与扣费说明',
             '多语言sdk接入', '常见错误码与faq', '更新日志'
           )"#
    );

    // 上游系统：将 akeapi 重命名为 Tkeapi
    once_migration!(pool, done, "channel_configs_upstream_system_tkeapi_rename_v1",
        "UPDATE channel_configs SET upstream_system = 'Tkeapi' WHERE upstream_system ILIKE 'akeapi'"
    );

    // 火山视频转素材ID：绑定协议 ark_action（方舟 CreateAsset）/ portrait_rest（Portrait Service）
    once_migration!(pool, done, "upstream_asset_relay_protocol_v1",
        "ALTER TABLE upstream_asset_bindings ADD COLUMN IF NOT EXISTS asset_protocol TEXT NOT NULL DEFAULT 'ark_action'",
        r#"UPDATE plugins SET
             description = '为火山视频任务将请求媒体 URL 经上游转为素材 ID（asset://）：支持方舟 CreateAsset 与 Portrait Service 人像资产，并生成可用转发规则',
             updated_at = CURRENT_TIMESTAMP
           WHERE name = 'upstream_asset_relay'"#
    );

    // 提示通知：增加弹窗通知开关 is_popup
    once_migration!(
        pool,
        done,
        "announcements_add_is_popup_v1",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS is_popup INTEGER NOT NULL DEFAULT 0"
    );

    // 插件管理后台左侧二级菜单：显示开关 / 排序权重 / 自定义名称
    once_migration!(pool, done, "plugins_admin_sidebar_menu_v1",
        "ALTER TABLE plugins ADD COLUMN IF NOT EXISTS show_in_admin_menu BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE plugins ADD COLUMN IF NOT EXISTS admin_menu_sort BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE plugins ADD COLUMN IF NOT EXISTS admin_menu_title TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN plugins.show_in_admin_menu IS '管理后台站点插件二级菜单开关（0=关，1=开）'",
        "COMMENT ON COLUMN plugins.admin_menu_sort IS '管理后台二级菜单排序权重，数字越大越靠前'",
        "COMMENT ON COLUMN plugins.admin_menu_title IS '管理后台二级菜单自定义名称，空则回落插件标题'"
    );

    // 插件左侧二级菜单：点击后默认打开的配置页 Tab
    once_migration!(pool, done, "plugins_admin_menu_default_tab_v1",
        "ALTER TABLE plugins ADD COLUMN IF NOT EXISTS admin_menu_default_tab TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN plugins.admin_menu_default_tab IS '点击管理后台二级菜单后打开的配置页 Tab key，空则第一个 Tab'"
    );

    // ComfyUI 本站请求队列：按节点放行名额 + 超时，记录每一条站点→节点请求
    once_migration!(pool, done, "comfyui_request_queue_v1",
        r#"CREATE TABLE IF NOT EXISTS comfyui_request_queue (
            id BIGSERIAL PRIMARY KEY,
            server_id BIGINT NOT NULL,
            workflow_id BIGINT NOT NULL,
            log_id BIGINT,
            prompt_id TEXT,
            status TEXT NOT NULL DEFAULT 'queued',
            error_message TEXT,
            prompt_preview TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            admitted_at TIMESTAMPTZ,
            finished_at TIMESTAMPTZ
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_comfyui_rq_status_server ON comfyui_request_queue(status, server_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_comfyui_rq_prompt ON comfyui_request_queue(prompt_id)",
        "CREATE INDEX IF NOT EXISTS idx_comfyui_rq_created ON comfyui_request_queue(id DESC)",
        "COMMENT ON TABLE comfyui_request_queue IS 'ComfyUI 本站请求队列：queued=等待放行，running=已放行到节点'",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES
             ('comfyui_bridge', 'queue_admit_limit', '10', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
             ('comfyui_bridge', 'queue_timeout_secs', '600', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    if !done.contains("playground_2026_quick_bar_seed_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_2026_quick_bar(pool).await
        {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_quick_bar_seed_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_quick_bar_seed_v1".into());
                tracing::info!("创作中心2026快捷栏回填完成，更新 {} 条配置", n);
            }
            Err(e) => {
                tracing::warn!("playground_2026_quick_bar_seed_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    once_migration!(
        pool,
        done,
        "comfyui_request_queue_hold_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES
             ('comfyui_bridge', 'queue_hold_all', '0', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
             ('comfyui_bridge', 'queue_hold_resume_at', '0', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    once_migration!(pool, done, "comfyui_dispatch_sui_xin_v1",
        "UPDATE comfyui_dispatch_rules SET name = '随心调用', remark = '按所选节点排序轮询，一台台调用；连不上或离线的跳过' WHERE code = 'sequential'"
    );

    once_migration!(pool, done, "comfyui_dispatch_shun_xu_v1",
        "UPDATE comfyui_dispatch_rules SET name = '顺序调用', remark = '按所选节点排序轮询，一台台调用；连不上或离线的跳过' WHERE code = 'sequential'"
    );

    once_migration!(pool, done, "comfyui_servers_max_pending_tasks_v1",
        "ALTER TABLE comfyui_servers ADD COLUMN IF NOT EXISTS max_pending_tasks INTEGER NOT NULL DEFAULT 1",
        "COMMENT ON COLUMN comfyui_servers.max_pending_tasks IS '排队中限制阈值，下游节点排队任务数达到该值时暂不下发新请求，默认 1'"
    );

    once_migration!(pool, done, "content_security_plugin_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, allowed_levels, category, created_at, updated_at)
           VALUES ('content_security', '内容安全审核',
           '接入腾讯云文本/图片/视频内容安全（TMS/IMS/VM），供其它插件与基础能力调用；含限流重试与任务日志。',
           0, 'all', 'system', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (name) DO UPDATE SET title = EXCLUDED.title, description = EXCLUDED.description, updated_at = CURRENT_TIMESTAMP"#,
        r#"CREATE TABLE IF NOT EXISTS content_security_logs (
            id BIGSERIAL PRIMARY KEY,
            media_type TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT '',
            user_id TEXT NOT NULL DEFAULT '',
            suggestion TEXT NOT NULL DEFAULT '',
            label TEXT NOT NULL DEFAULT '',
            sub_label TEXT NOT NULL DEFAULT '',
            score INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'done',
            vendor_task_id TEXT NOT NULL DEFAULT '',
            vendor_request_id TEXT NOT NULL DEFAULT '',
            retry_count INTEGER NOT NULL DEFAULT 0,
            error_message TEXT NOT NULL DEFAULT '',
            request_payload TEXT,
            response_payload TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_cs_logs_created ON content_security_logs(id DESC)",
        "CREATE INDEX IF NOT EXISTS idx_cs_logs_type_status ON content_security_logs(media_type, status, id DESC)",
        "CREATE INDEX IF NOT EXISTS idx_cs_logs_vendor_task_id ON content_security_logs(vendor_task_id) WHERE vendor_task_id <> ''",
        "COMMENT ON TABLE content_security_logs IS '内容安全审核任务日志'",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES
             ('content_security', 'region', 'ap-guangzhou', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
             ('content_security', 'text_enabled', '1', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
             ('content_security', 'image_enabled', '1', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
             ('content_security', 'video_enabled', '1', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
             ('content_security', 'retry_max', '3', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
             ('content_security', 'retry_interval_ms', '800', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#,
        "UPDATE logs SET is_completed = 1 \
         WHERE is_completed = 0 AND status_code NOT IN (0, 200)"
    );

    once_migration!(
        pool,
        done,
        "comfyui_queue_node_timeout_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           SELECT 'comfyui_bridge', 'queue_node_timeout_secs', config_value, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
             FROM plugin_configs
            WHERE plugin_name = 'comfyui_bridge' AND config_key = 'queue_timeout_secs'
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#,
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('comfyui_bridge', 'queue_node_timeout_secs', '600', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    once_migration!(pool, done, "comfyui_jobs_exec_times_v1",
        "ALTER TABLE comfyui_jobs ADD COLUMN IF NOT EXISTS exec_started_at TIMESTAMPTZ",
        "ALTER TABLE comfyui_jobs ADD COLUMN IF NOT EXISTS exec_finished_at TIMESTAMPTZ",
        "COMMENT ON COLUMN comfyui_jobs.exec_started_at IS 'ComfyUI history execution_start'",
        "COMMENT ON COLUMN comfyui_jobs.exec_finished_at IS 'ComfyUI history execution_success/error'"
    );

    once_migration!(pool, done, "comfyui_queue_fifo_log_v1",
        r#"DELETE FROM comfyui_request_queue a USING comfyui_request_queue b
           WHERE a.log_id IS NOT NULL AND a.log_id = b.log_id AND a.id < b.id"#,
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_comfyui_rq_log ON comfyui_request_queue (log_id) WHERE log_id IS NOT NULL",
        "CREATE INDEX IF NOT EXISTS idx_comfyui_rq_fifo ON comfyui_request_queue (server_id, id) WHERE status = 'queued'"
    );

    once_migration!(pool, done, "comfyui_queue_late_bind_v1",
        "ALTER TABLE comfyui_request_queue ADD COLUMN IF NOT EXISTS channel_id BIGINT",
        "ALTER TABLE comfyui_request_queue ADD COLUMN IF NOT EXISTS channel_group_aid TEXT",
        "ALTER TABLE comfyui_request_queue ADD COLUMN IF NOT EXISTS dispatch_rule TEXT NOT NULL DEFAULT 'priority_weight'",
        "ALTER TABLE comfyui_request_queue ADD COLUMN IF NOT EXISTS eligible_server_ids TEXT NOT NULL DEFAULT '[]'",
        "ALTER TABLE comfyui_request_queue ALTER COLUMN server_id DROP NOT NULL",
        r#"UPDATE comfyui_request_queue
           SET eligible_server_ids = '[' || server_id::text || ']'
           WHERE server_id IS NOT NULL
             AND (eligible_server_ids IS NULL OR eligible_server_ids = '' OR eligible_server_ids = '[]')"#,
        "CREATE INDEX IF NOT EXISTS idx_comfyui_rq_queued_id ON comfyui_request_queue (id) WHERE status = 'queued'",
        "COMMENT ON COLUMN comfyui_request_queue.eligible_server_ids IS '渠道分组可选节点，放行时再绑定 server_id'",
        "COMMENT ON COLUMN comfyui_request_queue.dispatch_rule IS '该请求所属渠道分组的 ComfyUI 调用规则'"
    );

    once_migration!(pool, done, "object_store_provider_v1",
        "ALTER TABLE tos_temp_files ADD COLUMN IF NOT EXISTS storage_provider TEXT NOT NULL DEFAULT 'tos'",
        "ALTER TABLE plugin_assets ADD COLUMN IF NOT EXISTS storage_provider TEXT NOT NULL DEFAULT 'tos'",
        "ALTER TABLE playground_assets ADD COLUMN IF NOT EXISTS storage_provider TEXT NOT NULL DEFAULT 'tos'",
        "ALTER TABLE playground_2026_assets ADD COLUMN IF NOT EXISTS storage_provider TEXT NOT NULL DEFAULT 'tos'",
        "ALTER TABLE playground_2026_project_assets ADD COLUMN IF NOT EXISTS storage_provider TEXT NOT NULL DEFAULT 'tos'",
        "CREATE INDEX IF NOT EXISTS idx_comfyui_rq_running_admitted ON comfyui_request_queue (admitted_at) WHERE status = 'running'"
    );

    once_migration!(pool, done, "api_tokens_kid_index_v1",
        "CREATE INDEX IF NOT EXISTS idx_api_tokens_kid ON api_tokens(kid) WHERE kid IS NOT NULL AND kid <> ''"
    );

    // 创作中心2026：仅系统类型「图片」「视频」初始绑定；勿用 LIKE，避免误伤「视频增强」
    once_migration!(
        pool,
        done,
        "pg2026_seed_model_feature_keys_v1",
        r#"WITH src AS (
             SELECT
               m.id,
               CASE
                 WHEN t.name = '图片' THEN 'create-image'
                 WHEN t.name = '视频' THEN 'text-to-video'
               END AS feat,
               CASE
                 WHEN pc.config_value ~ '^\s*\{' THEN pc.config_value::jsonb
                 ELSE '{"enabled":false,"scheme_id":null}'::jsonb
               END AS cfg
             FROM models m
             JOIN model_types t ON t.id = m.type_id
             LEFT JOIN plugin_configs pc
               ON pc.plugin_name = 'playground_2026'
              AND pc.config_key = 'pg_model_id_' || m.id::text
             WHERE t.name IN ('图片', '视频')
           ),
           built AS (
             SELECT
               id,
               jsonb_set(
                 cfg,
                 '{feature_keys}',
                 CASE
                   WHEN jsonb_typeof(cfg->'feature_keys') = 'array'
                    AND (cfg->'feature_keys') ? feat
                   THEN cfg->'feature_keys'
                   ELSE COALESCE(
                          CASE WHEN jsonb_typeof(cfg->'feature_keys') = 'array'
                               THEN cfg->'feature_keys' END,
                          '[]'::jsonb
                        ) || jsonb_build_array(feat)
                 END
               ) AS new_cfg
             FROM src
             WHERE feat IS NOT NULL
           )
           INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           SELECT 'playground_2026', 'pg_model_id_' || id::text, new_cfg::text, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
             FROM built
           ON CONFLICT (plugin_name, config_key) DO UPDATE
           SET config_value = EXCLUDED.config_value,
               updated_at = CURRENT_TIMESTAMP"#
    );

    // 已跑过 v1 LIKE 回填的库：从「视频增强」模型去掉误加的文生视频
    once_migration!(
        pool,
        done,
        "pg2026_strip_enhance_text_to_video_v1",
        r#"UPDATE plugin_configs pc
           SET config_value = jsonb_set(
                 pc.config_value::jsonb,
                 '{feature_keys}',
                 COALESCE((
                   SELECT jsonb_agg(to_jsonb(elem))
                   FROM jsonb_array_elements_text(
                     COALESCE(pc.config_value::jsonb->'feature_keys', '[]'::jsonb)
                   ) AS elem
                   WHERE elem <> 'text-to-video'
                 ), '[]'::jsonb)
               )::text,
               updated_at = CURRENT_TIMESTAMP
           FROM models m
           JOIN model_types t ON t.id = m.type_id
           WHERE pc.plugin_name = 'playground_2026'
             AND pc.config_key = 'pg_model_id_' || m.id::text
             AND t.name = '视频增强'
             AND pc.config_value ~ '^\s*\{'
             AND COALESCE(pc.config_value::jsonb->'feature_keys', '[]'::jsonb) ? 'text-to-video'"#
    );

    // v1 已标记完成，但后续「保存全部配置」把 feature_keys 写成空数组；只回填空/缺失，不覆盖已勾选
    once_migration!(
        pool,
        done,
        "pg2026_seed_model_feature_keys_v2",
        r#"WITH src AS (
             SELECT
               m.id,
               CASE
                 WHEN t.name = '图片' THEN 'create-image'
                 WHEN t.name = '视频' THEN 'text-to-video'
               END AS feat,
               CASE
                 WHEN pc.config_value ~ '^\s*\{' THEN pc.config_value::jsonb
                 ELSE '{"enabled":false,"scheme_id":null}'::jsonb
               END AS cfg
             FROM models m
             JOIN model_types t ON t.id = m.type_id
             LEFT JOIN plugin_configs pc
               ON pc.plugin_name = 'playground_2026'
              AND pc.config_key = 'pg_model_id_' || m.id::text
             WHERE t.name IN ('图片', '视频')
           ),
           built AS (
             SELECT
               id,
               jsonb_set(
                 cfg,
                 '{feature_keys}',
                 COALESCE(
                   CASE WHEN jsonb_typeof(cfg->'feature_keys') = 'array'
                        THEN cfg->'feature_keys' END,
                   '[]'::jsonb
                 ) || jsonb_build_array(feat)
               ) AS new_cfg
             FROM src
             WHERE feat IS NOT NULL
               AND (
                 jsonb_typeof(cfg->'feature_keys') IS DISTINCT FROM 'array'
                 OR jsonb_array_length(COALESCE(cfg->'feature_keys', '[]'::jsonb)) = 0
               )
           )
           INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           SELECT 'playground_2026', 'pg_model_id_' || id::text, new_cfg::text, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
             FROM built
           ON CONFLICT (plugin_name, config_key) DO UPDATE
           SET config_value = EXCLUDED.config_value,
               updated_at = CURRENT_TIMESTAMP"#
    );

    once_migration!(pool, done, "comfyui_content_audit_v1",
        "ALTER TABLE comfyui_workflows ADD COLUMN IF NOT EXISTS content_audit SMALLINT NOT NULL DEFAULT 0",
        "ALTER TABLE comfyui_jobs ADD COLUMN IF NOT EXISTS content_audit_task_id TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN comfyui_workflows.content_audit IS '1=开启内容安全审核（入队前文本/图，成片视频）'",
        "COMMENT ON COLUMN comfyui_jobs.content_audit_task_id IS '成片视频审核 vendor_task_id，空表示未发起或无需'",
        "ALTER TABLE content_security_logs DROP COLUMN IF EXISTS content_preview"
    );

    // task_id + queue_id；仅本迁移新增/清理的索引（queued_id/prompt/log/running_admitted 已在前序迁移）
    once_migration!(pool, done, "comfyui_jobs_task_id_v1",
        "DO $$ BEGIN IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name='comfyui_jobs' AND column_name='prompt_id') THEN ALTER TABLE comfyui_jobs RENAME COLUMN prompt_id TO task_id; END IF; END $$",
        "DROP INDEX IF EXISTS idx_comfyui_jobs_prompt",
        "CREATE INDEX IF NOT EXISTS idx_comfyui_jobs_task ON comfyui_jobs(task_id)",
        "ALTER TABLE comfyui_jobs ADD COLUMN IF NOT EXISTS queue_id BIGINT",
        "COMMENT ON COLUMN comfyui_jobs.task_id IS '对外任务 ID；POST 即固定'",
        "COMMENT ON COLUMN comfyui_jobs.queue_id IS 'comfyui_request_queue.id，节点 prompt_id 在队列表'",
        "UPDATE comfyui_jobs j SET queue_id = sub.qid FROM ( \
           SELECT DISTINCT ON (log_id) log_id, id AS qid FROM comfyui_request_queue \
           WHERE log_id IS NOT NULL ORDER BY log_id, id DESC \
         ) sub WHERE j.log_id = sub.log_id AND j.queue_id IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_comfyui_jobs_queue ON comfyui_jobs (queue_id) WHERE queue_id IS NOT NULL",
        "DROP INDEX IF EXISTS idx_comfyui_rq_fifo, idx_comfyui_rq_created, idx_comfyui_rq_status_server",
        "CREATE INDEX IF NOT EXISTS idx_comfyui_rq_await_prompt ON comfyui_request_queue (id) \
         WHERE status IN ('queued', 'running') AND (prompt_id IS NULL OR prompt_id = '')",
        "CREATE INDEX IF NOT EXISTS idx_comfyui_rq_running_server ON comfyui_request_queue (server_id) WHERE status = 'running'"
    );

    // ── 高可用日志筛选 + 低余额在途门禁按类别计数 ──
    once_migration!(pool, done, "ha_logs_filter_inflight_cat_idx_v1",
        "CREATE INDEX IF NOT EXISTS idx_ha_usage_logs_group_aid ON ha_usage_logs (group_aid) WHERE group_aid IS NOT NULL AND group_aid <> ''",
        "CREATE INDEX IF NOT EXISTS idx_logs_inflight_user_action ON logs (user_id, action_type) WHERE is_completed = 0 AND status_code IN (0, 200)"
    );

    // ── 聊天三协议原生透传：chat / responses / messages 入口即上游 ──
    once_migration!(
        pool,
        done,
        "chat_path_passthrough_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '聊天三协议原生透传 (chat/responses/messages)', 'openai',
               '入口路径原样转发上游：/v1/chat/completions、/v1/responses、/v1/messages；body 仅替换 model；messages 默认 anthropic+x-api-key',
               '{"path_passthrough":["/v1/chat/completions","/v1/responses","/v1/messages"]}',
               '聊天', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (
            SELECT 1 FROM forward_rules WHERE name = '聊天三协议原生透传 (chat/responses/messages)'
        )"#
    );

    // 日志汇总覆盖索引：替代无 INCLUDE 的 idx_logs_action_type_created，避免聚合回表
    once_migration!(pool, done, "logs_action_created_stats_covering_v1",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_action_created_stats ON logs (action_type, created_at DESC) INCLUDE (cost, status_code, pre_deduct_gift)",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_action_type_created"
    );

    // 创作中心2026：视频类型预制二级功能属性与用户端 AI 视频生成下拉对齐
    once_migration!(
        pool,
        done,
        "pg2026_video_preset_feature_attrs_v1",
        r#"UPDATE model_types
           SET default_features = '["文生视频","全能参考生视频","首尾帧生视频","编辑视频","延长视频"]',
               updated_at = CURRENT_TIMESTAMP
           WHERE name = '视频'"#,
        r#"UPDATE models AS m
           SET feature_attributes = sub.new_attrs,
               updated_at = CURRENT_TIMESTAMP
           FROM (
             SELECT
               id,
               (
                 SELECT COALESCE(jsonb_agg(to_jsonb(mapped) ORDER BY min_ord), '[]'::jsonb)::text
                 FROM (
                   SELECT
                     CASE elem
                       WHEN '参考生视频' THEN '全能参考生视频'
                       WHEN '视频参考' THEN '全能参考生视频'
                       WHEN '图生视频' THEN '首尾帧生视频'
                       WHEN '首帧生视频' THEN '首尾帧生视频'
                       WHEN '视频编辑' THEN '编辑视频'
                       WHEN '视频生视频' THEN '编辑视频'
                       WHEN '视频延长' THEN '延长视频'
                       ELSE elem
                     END AS mapped,
                     MIN(ord) AS min_ord
                   FROM jsonb_array_elements_text(
                     CASE
                       WHEN feature_attributes ~ '^\s*\[' THEN feature_attributes::jsonb
                       ELSE '[]'::jsonb
                     END
                   ) WITH ORDINALITY AS t(elem, ord)
                   GROUP BY 1
                 ) d
               ) AS new_attrs
             FROM models
             WHERE type_id IN (SELECT id FROM model_types WHERE name = '视频')
               AND feature_attributes IS NOT NULL
               AND btrim(feature_attributes) <> ''
               AND btrim(feature_attributes) <> '[]'
               AND feature_attributes ~ '^\s*\['
           ) sub
           WHERE m.id = sub.id
             AND m.feature_attributes IS DISTINCT FROM sub.new_attrs"#,
        r#"UPDATE plugin_configs pc
           SET config_value = jsonb_set(
                 pc.config_value::jsonb,
                 '{feature_keys}',
                 COALESCE((
                   SELECT jsonb_agg(DISTINCT to_jsonb(
                     CASE elem
                       WHEN 'reference-video' THEN 'reference-i2v'
                       ELSE elem
                     END
                   ))
                   FROM jsonb_array_elements_text(
                     COALESCE(pc.config_value::jsonb->'feature_keys', '[]'::jsonb)
                   ) AS elem
                 ), '[]'::jsonb)
               )::text,
               updated_at = CURRENT_TIMESTAMP
           WHERE pc.plugin_name = 'playground_2026'
             AND pc.config_key LIKE 'pg_model_id_%'
             AND pc.config_value ~ '^\s*\{'
             AND COALESCE(pc.config_value::jsonb->'feature_keys', '[]'::jsonb) ? 'reference-video'"#
    );

    // 视频二级功能：图生视频（首帧）与首尾帧拆开，对齐文生 / 首帧 / 首尾帧 / 全能参考 / 编辑 / 延长
    once_migration!(
        pool,
        done,
        "pg2026_video_preset_feature_attrs_v2",
        r#"UPDATE model_types
           SET default_features = '["文生视频","图生视频（首帧）","首尾帧生视频","全能参考生视频","编辑视频","延长视频"]',
               updated_at = CURRENT_TIMESTAMP
           WHERE name = '视频'"#,
        r#"UPDATE models AS m
           SET feature_attributes = sub.new_attrs,
               updated_at = CURRENT_TIMESTAMP
           FROM (
             SELECT
               id,
               (
                 SELECT COALESCE(jsonb_agg(to_jsonb(mapped) ORDER BY min_ord), '[]'::jsonb)::text
                 FROM (
                   SELECT
                     CASE elem
                       WHEN '图生视频' THEN '图生视频（首帧）'
                       WHEN '图生视频(首帧)' THEN '图生视频（首帧）'
                       WHEN '首帧生视频' THEN '图生视频（首帧）'
                       ELSE elem
                     END AS mapped,
                     MIN(ord) AS min_ord
                   FROM jsonb_array_elements_text(
                     CASE
                       WHEN feature_attributes ~ '^\s*\[' THEN feature_attributes::jsonb
                       ELSE '[]'::jsonb
                     END
                   ) WITH ORDINALITY AS t(elem, ord)
                   GROUP BY 1
                 ) d
               ) AS new_attrs
             FROM models
             WHERE type_id IN (SELECT id FROM model_types WHERE name = '视频')
               AND feature_attributes IS NOT NULL
               AND btrim(feature_attributes) <> ''
               AND btrim(feature_attributes) <> '[]'
               AND feature_attributes ~ '^\s*\['
           ) sub
           WHERE m.id = sub.id
             AND m.feature_attributes IS DISTINCT FROM sub.new_attrs"#
    );

    if !done.contains("playground_2026_layer_decomposition_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_2026_layer_decomposition(
            pool,
        )
        .await
        {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_layer_decomposition_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_layer_decomposition_v1".into());
                tracing::info!(
                    "创作中心2026 Seedream 5.0 图层拆分回填完成，更新 {} 条配置",
                    n
                );
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_layer_decomposition_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // 提示通知：增加排序属性 sort_order
    once_migration!(
        pool,
        done,
        "announcements_add_sort_order_v1",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN announcements.sort_order IS '排序权重，数字越大越靠前'"
    );

    if !done.contains("playground_seedance2_default_resolution_480p_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_seedance2_slider(pool)
            .await
        {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_seedance2_default_resolution_480p_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_seedance2_default_resolution_480p_v1".into());
                tracing::info!(
                    "创作中心 Seedance 2.0 resolution 改为 480p 回填完成，更新 {} 条配置",
                    n
                );
            }
            Err(e) => {
                tracing::warn!(
                    "playground_seedance2_default_resolution_480p_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    once_migration!(
        pool,
        done,
        "drop_happyhorse_router_v1",
        "DELETE FROM plugins WHERE name = 'happyhorse_router'",
        "DELETE FROM plugin_configs WHERE plugin_name = 'happyhorse_router'",
        "DROP TABLE IF EXISTS happyhorse_logs",
        "DROP TABLE IF EXISTS happyhorse_configs"
    );

    once_migration!(
        pool,
        done,
        "tencent_mps_tryon_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '腾讯云 MPS AI 换装 (同步轮询)', 'tencent',
               '将标准图片生成请求转换为腾讯云媒体处理 ProcessImage AI 换装。有 InputInfo 则官方透传，否则 OpenAI（model/prompt/resolution/image 或 image_urls）转换。密钥格式：SecretId:SecretKey，渠道 URL：https://mps.tencentcloudapi.com',
               '{"target_type":"tencent_mps_image","path_rewrite":{"old":"/v1/images/generations","new":"/"},"auth_type":"tencent_mps"}',
               '图片', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '腾讯云 MPS AI 换装 (同步轮询)')"#,
        r#"UPDATE forward_rules SET rule_type = 'tencent'
           WHERE rule_type IN ('tencent_vod', 'tencent_mps')"#
    );

    once_migration!(
        pool,
        done,
        "tencent_mps_create_image_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '腾讯云 MPS 场景生图 (同步轮询)', 'tencent',
               '将标准图片生成请求转换为腾讯云媒体处理 ProcessImage 场景生图（CreateImageConfig）。支持文生图与图生图（scene-image-gem-flash、scene-image-og-flash等8款模型档位），支持 resolution、aspect_ratio 及 additional_parameters。密钥格式：SecretId:SecretKey，渠道 URL：https://mps.tencentcloudapi.com',
               '{"target_type":"tencent_mps_create_image","path_rewrite":{"old":"/v1/images/generations","new":"/"},"auth_type":"tencent_mps"}',
               '图片', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '腾讯云 MPS 场景生图 (同步轮询)')"#
    );

    // ── MiniMax Anthropic Messages：入口 /anthropic/v1/messages + OpenAI→Anthropic 改写 ──
    once_migration!(
        pool,
        done,
        "minimax_anthropic_chat_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT 'MiniMax 聊天 (Anthropic兼容)', 'minimax',
               'OpenAI 聊天转 Anthropic Messages 并转发 MiniMax /anthropic/v1/messages（x-api-key）；原生入口 path_passthrough 仅换 model',
               '{"target_type":"anthropic","auth_type":"x-api-key","path_passthrough":["/anthropic/v1/messages"],"path_rewrite":{"old":"/v1/chat/completions","new":"/anthropic/v1/messages"}}',
               '聊天', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = 'MiniMax 聊天 (Anthropic兼容)')"#,
        r#"UPDATE forward_rules
           SET config_json = '{"path_passthrough":["/v1/chat/completions","/v1/responses","/v1/messages","/anthropic/v1/messages"]}',
               description = '入口路径原样转发上游：/v1/chat/completions、/v1/responses、/v1/messages、/anthropic/v1/messages；body 仅替换 model；Messages 入口默认 anthropic+x-api-key'
           WHERE name = '聊天三协议原生透传 (chat/responses/messages)' AND is_system = 1"#
    );

    // ── 火山视频转素材 Portrait：官方入口 path_accept（上游仍 rewrite 到 seedance/infer）──
    once_migration!(
        pool,
        done,
        "upstream_asset_relay_volc_official_entry_v1",
        r#"UPDATE forward_rules
           SET config_json = (config_json::jsonb || '{"path_accept":["/api/v3/contents/generations/tasks"]}'::jsonb)::text,
               updated_at = CURRENT_TIMESTAMP
           WHERE config_json::jsonb ? 'upstream_asset_convert'
             AND (config_json::jsonb ->> 'upstream_asset_convert') = 'true'
             AND COALESCE(config_json::jsonb #>> '{path_rewrite,new}', '')
                 LIKE '%/tenant/seedance/infer%'"#
    );

    // channel_configs.yid：空值与重复改为 3u{id}（保留同组最小 id），再唯一索引
    once_migration!(
        pool,
        done,
        "channel_configs_yid_unique_v1",
        r#"UPDATE channel_configs
           SET yid = '3u' || id::text
           WHERE COALESCE(btrim(yid), '') = ''"#,
        r#"UPDATE channel_configs c
           SET yid = '3u' || c.id::text
           FROM (
             SELECT id, ROW_NUMBER() OVER (PARTITION BY yid ORDER BY id) AS rn
             FROM channel_configs
           ) d
           WHERE c.id = d.id AND d.rn > 1"#,
        r#"CREATE UNIQUE INDEX IF NOT EXISTS idx_channel_configs_yid_unique
           ON channel_configs (yid)"#
    );

    // ── 数据同步密钥：可导出的数据类型（模型 / 计费）──
    once_migration!(pool, done, "data_sync_key_scopes_v1",
        r#"ALTER TABLE data_sync_keys ADD COLUMN IF NOT EXISTS allowed_scopes TEXT NOT NULL DEFAULT '["models","billing"]'"#,
        "COMMENT ON COLUMN data_sync_keys.allowed_scopes IS '密钥允许导出的数据类型 JSON 数组：models、billing'"
    );

    // ── 数据同步：商业版镜像运营（渠道密钥/分组）──
    once_migration!(
        pool,
        done,
        "data_sync_mirror_ops_v1",
        "ALTER TABLE data_sync_keys ADD COLUMN IF NOT EXISTS mirror_ops INTEGER NOT NULL DEFAULT 0"
    );
    once_migration!(pool, done, "data_sync_mirror_ops_comment_v1",
        "COMMENT ON COLUMN data_sync_keys.mirror_ops IS '1=允许下游镜像运营同步（上游渠道+模型渠道分组，含密钥）；仅商业版生效'"
    );

    if !done.contains("playground_2026_remove_seedream_5_0_layer_decomposition_v1") {
        match crate::db::migrations::helpers::playground::remove_playground_2026_seedream_5_0_layer_decomposition(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_remove_seedream_5_0_layer_decomposition_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_remove_seedream_5_0_layer_decomposition_v1".into());
                tracing::info!("创作中心2026 Seedream 5.0 移除图层拆分完成，更新 {} 条配置", n);
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_remove_seedream_5_0_layer_decomposition_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // ── 清理预设模型默认写入的冗余备注 ──
    once_migration!(
        pool,
        done,
        "clean_preset_models_default_remark_v1",
        "UPDATE models SET remark = '' WHERE remark = '系统预设模型，已绑定官方计费与转发规则'"
    );

    // ── 火山级联画质增强默认计费规则固定 PID 为 78242 并设置为系统规则 ──
    once_migration!(pool, done, "volcengine_enhance_cascade_billing_pid_78242_v1",
        "UPDATE billing_rules SET pid = '78242', is_system = 1 WHERE name = '火山级联画质增强默认计费'",
        r#"INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, extended_config, is_system, pid, pricing_type)
        SELECT '火山级联画质增强默认计费', 'duration', 0.0, 0.0, 0.0, 0.0, 'volc_enhance_cascade', '{"price_table": {"fast|720p|no": 0.80, "fast|1080p|no": 1.80, "fast|2k|no": 3.20, "fast|4k|no": 7.20, "standard|720p|no": 1.00, "standard|1080p|no": 2.24, "standard|2k|no": 4.00, "standard|4k|no": 8.94, "pro|720p|no": 1.20, "pro|1080p|no": 2.70, "pro|2k|no": 4.80, "pro|4k|no": 10.70, "ai|720p|no": 1.40, "ai|1080p|no": 3.16, "fast|720p|yes": 0.84, "fast|1080p|yes": 1.88, "fast|2k|yes": 3.40, "fast|4k|yes": 7.40, "standard|720p|yes": 1.06, "standard|1080p|yes": 2.36, "standard|2k|yes": 4.30, "standard|4k|yes": 9.24, "pro|720p|yes": 1.28, "pro|1080p|yes": 2.86, "pro|2k|yes": 5.20, "pro|4k|yes": 11.10, "ai|720p|yes": 1.60, "ai|1080p|yes": 3.51}}', 1, '78242', 'official'
        WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = '火山级联画质增强默认计费' OR pid = '78242')
        "#
    );

    // ── 移除系统预设可灵 (Kling) 计费规则 ──
    once_migration!(pool, done, "remove_kling_system_preset_billing_rules_v1",
        "UPDATE models SET billing_rule_id = NULL WHERE billing_rule_id IN (SELECT id FROM billing_rules WHERE is_system = 1 AND name LIKE '%可灵%')",
        "DELETE FROM billing_rules WHERE is_system = 1 AND name LIKE '%可灵%'"
    );

    // ── Seedance 2.0 Mini 官方计费规则与全系 Seedance 系统预设模型数据更新 ──
    once_migration!(pool, done, "seedance_models_and_mini_billing_v1",
        "UPDATE billing_rules SET is_system = 1, pid = CASE WHEN pid LIKE '6%' OR pid = '' OR pid IS NULL THEN '74114' ELSE pid END WHERE name = 'Seedance2.0mini官方计费'",
        r#"INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, extended_config, is_system, pid, pricing_type)
        SELECT 'Seedance2.0mini官方计费', 'tokens', 0.0, 0.0, 0.0, 0.0, 'seedance2.0', '{"resolution_rates":{"480p":{"with_video":14,"without_video":23},"720p":{"with_video":14,"without_video":23}}}', 1, '74114', 'official'
        WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = 'Seedance2.0mini官方计费')
        "#,
        "UPDATE models SET model_id_alias = 'doubao-seedance-2-5-260128', feature_attributes = '[\"文生视频\",\"图生视频\",\"全能参考生视频\"]', is_system = 1 WHERE model_id = 'doubao-seedance-2-5'",
        "UPDATE models SET model_id_alias = 'doubao-seedance-2-0-260128', feature_attributes = '[\"文生视频\",\"图生视频\",\"全能参考生视频\"]', is_system = 1 WHERE model_id = 'doubao-seedance-2-0'",
        "UPDATE models SET model_id_alias = 'doubao-seedance-2-0-fast-260128', is_system = 1 WHERE model_id = 'doubao-seedance-2-0-fast'",
        "UPDATE models SET model_id_alias = 'doubao-seedance-2-0-mini-260128', is_system = 1 WHERE model_id = 'doubao-seedance-2-0-mini'"
    );

    // 一次性补齐系统预设模型目录（自动补齐 310004 Doubao Seedance 2.0 Mini 等未入库预设；记录迁移历史避免每次重启反复插入已删除模型）
    if !done.contains("seed_preset_models_catalog_v2") {
        match crate::db::preset_models::seed_system_preset_models(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('seed_preset_models_catalog_v2')",
                )
                .execute(pool)
                .await;
                done.insert("seed_preset_models_catalog_v2".into());
                if n > 0 {
                    tracing::info!("系统预设模型更新完成，新增 {} 条模型", n);
                }
            }
            Err(e) => {
                tracing::warn!("系统预设模型更新写入警告: {e}");
            }
        }
    }

    // 团队营销：新增「授权帮用户申请开票」权限列
    once_migration!(pool, done, "marketing_teams_invoice_perm",
        "ALTER TABLE marketing_teams ADD COLUMN IF NOT EXISTS members_can_apply_invoice BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN marketing_teams.members_can_apply_invoice IS '团队成员是否可以代替推荐用户申请开具发票(0=否,1=是)'"
    );

    // ── 文档插件：火山方舟更名为火山引擎(方舟)，MediaKit 媒体处理默认隐藏但不删除 ──
    once_migration!(pool, done, "docs_volcengine_ark_naming_and_mediakit_hidden_v1",
        "UPDATE plugin_docs SET title = '4.火山引擎(方舟)原生协议' WHERE slug = 'volcengine-ark' OR title = '4.火山方舟原生协议'",
        "UPDATE plugin_docs SET title = '火山引擎(方舟)原生API接入' WHERE slug = 'volcengine-api' OR title = '火山方舟原生API接入'",
        "UPDATE plugin_docs SET is_active = 0 WHERE slug = 'volcengine-mediakit'"
    );

    Ok(())
}
