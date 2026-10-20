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
    // ── 数据同步插件：跨站拉取模型目录与计费规则 ──
    once_migration!(
        pool,
        done,
        "init_data_sync_plugin_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category, allowed_levels, created_at, updated_at)
           VALUES (
             'data_sync',
             '数据同步',
             '通过站点请求密钥跨站拉取模型列表与计费规则；本站优先跳过冲突，不同步折扣与渠道密钥',
             0,
             'system_builtin',
             'all',
             CURRENT_TIMESTAMP,
             CURRENT_TIMESTAMP
           )
           ON CONFLICT (name) DO UPDATE SET
             title = EXCLUDED.title,
             description = EXCLUDED.description,
             category = EXCLUDED.category"#,
        r#"CREATE TABLE IF NOT EXISTS data_sync_logs (
            id BIGSERIAL PRIMARY KEY,
            action TEXT NOT NULL,
            peer_url TEXT,
            operator_id TEXT,
            summary TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'success',
            error_message TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_data_sync_logs_created ON data_sync_logs (created_at DESC)",
        "COMMENT ON TABLE data_sync_logs IS '数据同步插件操作审计日志'"
    );

    // ── 数据同步：多站点请求密钥（命名/备注/有效期/IP 白名单）──
    once_migration!(pool, done, "data_sync_multi_keys_v1",
        r#"CREATE TABLE IF NOT EXISTS data_sync_keys (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            remark TEXT NOT NULL DEFAULT '',
            secret TEXT NOT NULL,
            expires_at TIMESTAMPTZ,
            ip_whitelist TEXT NOT NULL DEFAULT '[]',
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_data_sync_keys_active ON data_sync_keys (is_active)",
        "COMMENT ON TABLE data_sync_keys IS '数据同步站点请求密钥：支持多密钥、备注、有效期、IP白名单(空=不限制)'",
        r#"INSERT INTO data_sync_keys (name, remark, secret, expires_at, ip_whitelist, is_active)
           SELECT '默认密钥', '由旧版单密钥自动迁移', config_value, NULL, '[]', 1
           FROM plugin_configs
           WHERE plugin_name = 'data_sync'
             AND config_key = 'site_request_secret'
             AND COALESCE(config_value, '') <> ''
             AND NOT EXISTS (SELECT 1 FROM data_sync_keys LIMIT 1)"#,
        r#"DELETE FROM plugin_configs
           WHERE plugin_name = 'data_sync' AND config_key = 'site_request_secret'"#
    );

    // ── 上游渠道配置启用/禁用状态 ──
    once_migration!(
        pool,
        done,
        "channel_configs_status_v1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS status INTEGER NOT NULL DEFAULT 1",
        "COMMENT ON COLUMN channel_configs.status IS '1=启用, 0=禁用'"
    );

    // ── 补全预置 MiniMax 图片生成转发规则 ──
    once_migration!(
        pool,
        done,
        "minimax_image_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT 'MiniMax 图片生成', 'minimax', 'MiniMax 文生图/图生图原生通道（/v1/image_generation），兼容 OpenAI 参数并透传官方字段（aspect_ratio/subject_reference/prompt_optimizer 等）', '{"target_type":"minimax_image","path_rewrite":{"old":"/v1/images/generations","new":"/v1/image_generation"},"auth_type":"bearer"}', '图片', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = 'MiniMax 图片生成')"#
    );

    // ── MiniMax 规则 rule_type 统一为 minimax（target_type 仍为 minimax_image / minimax_video）──
    once_migration!(
        pool,
        done,
        "minimax_forward_rule_unify_v1",
        r#"UPDATE forward_rules
        SET rule_type = 'minimax'
        WHERE rule_type IN ('minimax_image', 'minimax_video')"#
    );

    // ── 上游渠道配置分类（复用 channel_categories）──
    once_migration!(pool, done, "channel_configs_category_v1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS category_id BIGINT REFERENCES channel_categories(id)",
        "COMMENT ON COLUMN channel_configs.category_id IS '上游分类，关联 channel_categories.id'"
    );

    // ── 兑换码：单用户活动参与次数上限（与单码多次兑换解耦）──
    once_migration!(pool, done, "redemptions_per_user_activity_limit_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS per_user_activity_limit INTEGER NOT NULL DEFAULT -1",
        "COMMENT ON COLUMN redemptions.per_user_activity_limit IS '同一活动(同 name)下单用户可兑换次数，-1=不限制'",
        "CREATE INDEX IF NOT EXISTS idx_redemptions_name ON redemptions (name)"
    );

    // ── 兑换日志按 user_id 索引：活动参与次数统计 / 防刷查询 ──
    once_migration!(
        pool,
        done,
        "redemption_logs_user_id_idx_v1",
        "CREATE INDEX IF NOT EXISTS idx_redemption_logs_user_id ON redemption_logs (user_id)"
    );

    // ── 创作中心2026：独立作品表（图片/视频 outputs，与 projects 平级）──
    once_migration!(pool, done, "init_playground_2026_outputs_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_outputs (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            uid TEXT NOT NULL DEFAULT '',
            media_type TEXT NOT NULL DEFAULT 'image',
            status TEXT NOT NULL DEFAULT 'pending',
            prompt TEXT NOT NULL DEFAULT '',
            model_name TEXT NOT NULL DEFAULT '',
            model_mid TEXT NOT NULL DEFAULT '',
            param_values TEXT NOT NULL DEFAULT '{}',
            preview_url TEXT NOT NULL DEFAULT '',
            aspect_ratio TEXT NOT NULL DEFAULT '',
            resolution TEXT NOT NULL DEFAULT '',
            error_message TEXT NOT NULL DEFAULT '',
            upstream_task_id TEXT NOT NULL DEFAULT '',
            sys_log_id TEXT NOT NULL DEFAULT '',
            batch_id TEXT NOT NULL DEFAULT '',
            is_deleted INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg2026_outputs_user_media_created ON playground_2026_outputs(user_id, media_type, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_outputs_upstream_task ON playground_2026_outputs(upstream_task_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_outputs_batch ON playground_2026_outputs(batch_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_outputs_user_deleted ON playground_2026_outputs(user_id, is_deleted)",
        "COMMENT ON TABLE playground_2026_outputs IS '创作中心2026作品表（图片/视频，独立于 assets/projects）'",
        "COMMENT ON COLUMN playground_2026_outputs.media_type IS 'image|video'",
        "COMMENT ON COLUMN playground_2026_outputs.status IS 'pending|done|error'",
        "COMMENT ON COLUMN playground_2026_outputs.upstream_task_id IS '上游异步任务 id，对应 /v1/tasks/{id}'",
        "COMMENT ON COLUMN playground_2026_outputs.batch_id IS '同一次生成多张结果的批次 id'",
        "COMMENT ON COLUMN playground_2026_outputs.param_values IS '灵活 JSON：方案参数 key→value + _fields[{key,label}] 显示名 + 可选 reference_urls；勿拆成死字段'"
    );

    // ── 创作中心2026：作品分类（收藏夹系统分类 + 用户自定义）──
    once_migration!(pool, done, "init_playground_2026_albums_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_albums (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            name TEXT NOT NULL DEFAULT '',
            kind TEXT NOT NULL DEFAULT 'custom',
            is_system INTEGER NOT NULL DEFAULT 0,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        r#"CREATE TABLE IF NOT EXISTS playground_2026_album_items (
            album_id BIGINT NOT NULL REFERENCES playground_2026_albums(id) ON DELETE CASCADE,
            output_id BIGINT NOT NULL REFERENCES playground_2026_outputs(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (album_id, output_id)
        )"#,
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_pg2026_albums_user_favorites ON playground_2026_albums(user_id) WHERE kind = 'favorites'",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_albums_user_sort ON playground_2026_albums(user_id, sort_order ASC, id ASC)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_album_items_output ON playground_2026_album_items(output_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_album_items_user ON playground_2026_album_items(user_id, album_id)",
        "COMMENT ON TABLE playground_2026_albums IS '创作中心2026作品分类：favorites 系统收藏夹 + custom 用户分类'",
        "COMMENT ON COLUMN playground_2026_albums.kind IS 'favorites|custom'",
        "COMMENT ON TABLE playground_2026_album_items IS '作品与分类的多对多归属'"
    );

    once_migration!(pool, done, "pg2026_outputs_param_values_comment_v1",
        "COMMENT ON COLUMN playground_2026_outputs.param_values IS '灵活 JSON：方案参数 key→value + _fields[{key,label}] 显示名 + 可选 reference_urls；勿拆成死字段'"
    );

    // ── 可灵 3.0 推荐转发规则（kling_video，与旧 kling 解耦；文/图一条 + Omni 一条）──
    once_migration!(
        pool,
        done,
        "kling_video_v3_forward_rules_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT * FROM (VALUES
            ('可灵视频 3.0（文/图·推荐）', 'kling_video', '可灵官方 3.0 文生/图生推荐通道：URL 为 /text-to-video/${model}，body 含 contents 时自动改写为 /image-to-video/${model}；统一轮询 /tasks；渠道密钥填官方 API Key（Authorization: Bearer，无需 JWT）', '{"target_type":"kling_video","path_rewrite":{"old":"/v1/video/generations","new":"/text-to-video/${model}"},"auth_type":"bearer","poll_path":"/tasks?task_ids=${task_id}"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')),
            ('可灵 Omni 视频 3.0（推荐）', 'kling_video', '可灵官方 Omni 3.0 推荐通道：URL 为 /omni-video/${model}；多模态 contents；统一轮询 /tasks；渠道密钥填官方 API Key（Authorization: Bearer，无需 JWT）', '{"target_type":"kling_video","path_rewrite":{"old":"/v1/video/generations","new":"/omni-video/${model}"},"auth_type":"bearer","poll_path":"/tasks?task_ids=${task_id}"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0'))
        ) AS t(name, rule_type, description, config_json, category, is_system, eid)
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = t.name)"#
    );

    // ── 上游渠道配置：日额度自定义刷新时刻 + 冷却分钟 ──
    once_migration!(pool, done, "channel_configs_daily_reset_cutover_v1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS daily_reset_hour INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS daily_reset_minute INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS daily_reset_cooldown_minutes INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN channel_configs.daily_reset_hour IS '日额度刷新时(0-23)，站点时区'",
        "COMMENT ON COLUMN channel_configs.daily_reset_minute IS '日额度刷新分(0-59)，站点时区'",
        "COMMENT ON COLUMN channel_configs.daily_reset_cooldown_minutes IS '到达刷新时刻后再冷却多少分钟才真正刷新日已用(0=立即)'"
    );

    // ── 火山 MediaKit：插件日志关联表（log_id = logs.id，列表不再全表扫 model）──
    once_migration!(
        pool,
        done,
        "volcengine_enhance_logs_link_v1",
        r#"CREATE TABLE IF NOT EXISTS volcengine_enhance_logs (
            log_id BIGINT PRIMARY KEY
        )"#,
        "COMMENT ON TABLE volcengine_enhance_logs IS '火山 MediaKit 使用日志关联：log_id=logs.id'",
        "COMMENT ON COLUMN volcengine_enhance_logs.log_id IS '关联主日志表 logs.id'"
    );

    // ── 创作中心2026：工作流（镜像 projects，独立于画布项目）──
    once_migration!(pool, done, "init_playground_2026_workflows_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_workflows (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            uid TEXT NOT NULL,
            name TEXT NOT NULL DEFAULT '未命名工作流',
            description TEXT DEFAULT '',
            cover_url TEXT DEFAULT '',
            canvas_data TEXT DEFAULT '{}',
            is_deleted INTEGER NOT NULL DEFAULT 0,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg2026_workflows_user ON playground_2026_workflows(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_workflows_uid ON playground_2026_workflows(uid)",
        "COMMENT ON TABLE playground_2026_workflows IS '创作中心2026工作流表（节点编排，独立于 playground_2026_projects）'"
    );

    // ── 高可用插件：使用日志（log_id=logs.id，attempts 含全量子渠过程）──
    once_migration!(pool, done, "ha_usage_logs_v1",
        r#"CREATE TABLE IF NOT EXISTS ha_usage_logs (
            log_id BIGINT PRIMARY KEY,
            group_aid TEXT,
            attempt_count SMALLINT NOT NULL DEFAULT 0,
            final_ok SMALLINT NOT NULL DEFAULT 0,
            final_status_code INT NOT NULL DEFAULT 0,
            attempts JSONB NOT NULL DEFAULT '[]',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_ha_usage_logs_created ON ha_usage_logs (created_at DESC)",
        "COMMENT ON TABLE ha_usage_logs IS '高可用插件使用日志：log_id=logs.id，attempts=子渠过程JSON'"
    );

    // ── 方舟监控：流水统计起点 + 热点查询索引 ──
    once_migration!(pool, done, "ark_monitor_ledger_after_and_indexes_v1",
        "ALTER TABLE ark_endpoint_bindings ADD COLUMN IF NOT EXISTS wallet_ledger_after TIMESTAMPTZ",
        "COMMENT ON COLUMN ark_endpoint_bindings.wallet_ledger_after IS '方舟钱包流水统计起点：换绑用户/接入点时置为当前时间；NULL=统计全部历史流水'",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ark_bindings_account ON ark_endpoint_bindings (account_id)",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ark_video_tasks_ep_estimated ON ark_video_tasks (endpoint_id) WHERE is_estimated = TRUE AND status IN ('succeeded', 'success')",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ark_video_tasks_ep_confirmed ON ark_video_tasks (endpoint_id) WHERE is_estimated = FALSE AND status IN ('succeeded', 'success')",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_recharge_ark_monitor_user_created ON recharge_records (user_id, created_at) WHERE operator = 'ark_monitor' AND recharge_type IN ('ark_video_consume', 'ark_video_refund')"
    );

    // ── 用户实名认证 KYC ──
    once_migration!(
        pool,
        done,
        "user_kyc_v1",
        r#"CREATE TABLE IF NOT EXISTS user_kyc (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
            kyc_type TEXT NOT NULL DEFAULT 'personal',
            status TEXT NOT NULL DEFAULT 'none',
            real_name TEXT,
            id_doc_type TEXT,
            id_doc_front_url TEXT,
            id_doc_back_url TEXT,
            company_name TEXT,
            business_license_url TEXT,
            tax_registration_url TEXT,
            legal_notarization_url TEXT,
            validity_type TEXT NOT NULL DEFAULT 'long_term',
            expire_at TIMESTAMPTZ,
            reject_reason TEXT,
            admin_remark TEXT,
            reviewed_by TEXT,
            reviewed_at TIMESTAMPTZ,
            submitted_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_user_kyc_status ON user_kyc(status)",
        "COMMENT ON TABLE user_kyc IS '用户实名认证：个人/企业证件与有效期'",
        "COMMENT ON COLUMN user_kyc.kyc_type IS 'personal|enterprise'",
        "COMMENT ON COLUMN user_kyc.status IS 'none|pending|approved|rejected|expired'",
        "COMMENT ON COLUMN user_kyc.id_doc_type IS 'id_card|passport|driver_license'",
        "COMMENT ON COLUMN user_kyc.validity_type IS 'long_term|expire_date'"
    );

    // ── 新增模型：折扣限价默认开启，倍率默认 1.0 ──
    once_migration!(pool, done, "models_site_discount_default_on_v1",
        "ALTER TABLE models ALTER COLUMN site_discount SET DEFAULT 1.0",
        "ALTER TABLE models ALTER COLUMN site_discount_enabled SET DEFAULT 1",
        "COMMENT ON COLUMN models.site_discount IS '折扣限价倍率（开启时折扣不低于此值，默认 1.0=原价）'",
        "COMMENT ON COLUMN models.site_discount_enabled IS '折扣限价开关（0=关，1=开，新增默认开启）'"
    );

    // ── 更新 Seedance 2.0 官方计费规则（PID 74112）系统默认配置（包含 4K 分辨率计费） ──
    once_migration!(pool, done, "update_seedance2_0_default_rule_4k_v1",
        r#"UPDATE billing_rules SET extended_config = '{"resolution_rates":{"1080p":{"with_video":31,"without_video":51},"480p":{"with_video":28,"without_video":46},"4k":{"with_video":16,"without_video":26},"720p":{"with_video":28,"without_video":46}}}' WHERE (name = 'Seedance2.0官方计费' OR pid = '74112') AND is_system = 1"#,
        "UPDATE billing_rules SET pid = '74112' WHERE name = 'Seedance2.0官方计费' AND is_system = 1 AND (pid = '' OR pid IS NULL)"
    );

    // ── 将 Seedance 2.5 官方计费规则调整为系统计费规则 ──
    once_migration!(pool, done, "make_seedance2_5_system_rule_v1",
        "UPDATE billing_rules SET is_system = 1, pid = CASE WHEN pid LIKE '6%' OR pid = '' OR pid IS NULL THEN '73119' ELSE pid END WHERE name = 'Seedance2.5官方计费'",
        r#"INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, extended_config, is_system, pid, pricing_type)
        SELECT 'Seedance2.5官方计费', 'tokens', 0.0, 0.0, 0.0, 0.0, 'seedance2.0', '{"enable_time_multipliers":false,"resolution_rates":{"480p":{"with_video":42,"without_video":70},"720p":{"with_video":42,"without_video":70}},"time_multipliers":[]}', 1, '73119', 'official'
        WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = 'Seedance2.5官方计费')
        "#
    );

    // ── 创作中心2026：统一资源存储路径（uploads/works）+ 工作流资源关联 ──
    once_migration!(pool, done, "pg2026_unified_asset_storage_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_project_assets (
            id BIGSERIAL PRIMARY KEY,
            project_id BIGINT NOT NULL,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            uid TEXT NOT NULL,
            asset_type TEXT NOT NULL,
            file_name TEXT DEFAULT '',
            file_size BIGINT DEFAULT 0,
            file_url TEXT NOT NULL,
            tos_object_key TEXT DEFAULT '',
            thumbnail_url TEXT DEFAULT '',
            prompt TEXT DEFAULT '',
            model_id TEXT DEFAULT '',
            model_name TEXT DEFAULT '',
            generation_params TEXT DEFAULT '{}',
            canvas_node_data TEXT DEFAULT '{}',
            duration_seconds DOUBLE PRECISION DEFAULT 0,
            width BIGINT DEFAULT 0,
            height BIGINT DEFAULT 0,
            is_deleted INTEGER NOT NULL DEFAULT 0,
            file_hash TEXT DEFAULT '',
            library_asset_id BIGINT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg2026_project_assets_project ON playground_2026_project_assets(project_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_project_assets_user ON playground_2026_project_assets(user_id)",
        "ALTER TABLE playground_2026_assets ADD COLUMN IF NOT EXISTS tos_object_key TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE playground_2026_assets ADD COLUMN IF NOT EXISTS file_size BIGINT NOT NULL DEFAULT 0",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_assets_user_tos_key ON playground_2026_assets(user_id, tos_object_key)",
        "COMMENT ON COLUMN playground_2026_assets.tos_object_key IS '对象存储 Object Key（统一 assets/uploads|works）'",
        "COMMENT ON COLUMN playground_2026_assets.file_size IS '文件字节大小'",
        "ALTER TABLE playground_2026_project_assets ADD COLUMN IF NOT EXISTS library_asset_id BIGINT",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_project_assets_library ON playground_2026_project_assets(library_asset_id)",
        "COMMENT ON COLUMN playground_2026_project_assets.library_asset_id IS '关联资源库 playground_2026_assets.id'",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_workflow_assets (
            workflow_id BIGINT NOT NULL REFERENCES playground_2026_workflows(id) ON DELETE CASCADE,
            asset_id BIGINT NOT NULL REFERENCES playground_2026_assets(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (workflow_id, asset_id)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg2026_workflow_assets_asset ON playground_2026_workflow_assets(asset_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_workflow_assets_user ON playground_2026_workflow_assets(user_id)",
        "COMMENT ON TABLE playground_2026_workflow_assets IS '工作流与资源库资产多对多关联'"
    );

    // ── 创作中心2026：资源库表对齐代码（旧 canvas assets → project_assets；outputs → assets）──
    // 背景：统一存储后 API 读写 playground_2026_assets(media_type/source_type/…)，
    // 但存量库仍可能是「项目素材」形态的 assets + 独立 outputs，导致 list /assets 报 media_type 不存在。
    once_migration!(
        pool,
        done,
        "pg2026_assets_library_from_outputs_v1",
        r#"DO $mig$
        DECLARE
          assets_has_media BOOLEAN;
          assets_has_project BOOLEAN;
          outputs_exists BOOLEAN;
          album_has_output_id BOOLEAN;
          album_has_asset_id BOOLEAN;
        BEGIN
          SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'playground_2026_assets'
              AND column_name = 'media_type'
          ) INTO assets_has_media;

          SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'playground_2026_assets'
              AND column_name = 'project_id'
          ) INTO assets_has_project;

          SELECT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = 'playground_2026_outputs'
          ) INTO outputs_exists;

          -- 1) 旧 canvas assets → project_assets，再删旧 assets
          IF assets_has_project AND NOT assets_has_media THEN
            CREATE TABLE IF NOT EXISTS playground_2026_project_assets (
              id BIGSERIAL PRIMARY KEY,
              project_id BIGINT NOT NULL,
              user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
              uid TEXT NOT NULL,
              asset_type TEXT NOT NULL,
              file_name TEXT DEFAULT '',
              file_size BIGINT DEFAULT 0,
              file_url TEXT NOT NULL,
              tos_object_key TEXT DEFAULT '',
              thumbnail_url TEXT DEFAULT '',
              prompt TEXT DEFAULT '',
              model_id TEXT DEFAULT '',
              model_name TEXT DEFAULT '',
              generation_params TEXT DEFAULT '{}',
              canvas_node_data TEXT DEFAULT '{}',
              duration_seconds DOUBLE PRECISION DEFAULT 0,
              width BIGINT DEFAULT 0,
              height BIGINT DEFAULT 0,
              is_deleted INTEGER NOT NULL DEFAULT 0,
              file_hash TEXT DEFAULT '',
              library_asset_id BIGINT,
              created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            INSERT INTO playground_2026_project_assets (
              id, project_id, user_id, uid, asset_type, file_name, file_size, file_url, tos_object_key,
              thumbnail_url, prompt, model_id, model_name, generation_params, canvas_node_data,
              duration_seconds, width, height, is_deleted, file_hash, created_at
            )
            SELECT
              id, project_id, user_id, uid, asset_type,
              COALESCE(file_name, ''), COALESCE(file_size, 0), file_url,
              COALESCE(tos_object_key, ''), COALESCE(thumbnail_url, ''), COALESCE(prompt, ''),
              COALESCE(model_id, ''), COALESCE(model_name, ''), COALESCE(generation_params, '{}'),
              COALESCE(canvas_node_data, '{}'), COALESCE(duration_seconds, 0),
              COALESCE(width, 0), COALESCE(height, 0), COALESCE(is_deleted, 0),
              COALESCE(file_hash, ''), created_at
            FROM playground_2026_assets a
            WHERE NOT EXISTS (
              SELECT 1 FROM playground_2026_project_assets p WHERE p.id = a.id
            );

            PERFORM setval(
              pg_get_serial_sequence('playground_2026_project_assets', 'id'),
              GREATEST(
                COALESCE((SELECT MAX(id) FROM playground_2026_project_assets), 1),
                1
              )
            );

            DROP TABLE playground_2026_assets CASCADE;
            assets_has_media := FALSE;
            assets_has_project := FALSE;
          END IF;

          -- 2) outputs → assets（资源库）
          IF outputs_exists AND NOT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = 'playground_2026_assets'
          ) THEN
            ALTER TABLE playground_2026_outputs RENAME TO playground_2026_assets;
            BEGIN
              ALTER INDEX IF EXISTS idx_pg2026_outputs_user_media_created
                RENAME TO idx_pg2026_assets_lib_user_media_created;
            EXCEPTION WHEN OTHERS THEN NULL;
            END;
            BEGIN
              ALTER INDEX IF EXISTS idx_pg2026_outputs_upstream_task
                RENAME TO idx_pg2026_assets_lib_upstream_task;
            EXCEPTION WHEN OTHERS THEN NULL;
            END;
            BEGIN
              ALTER INDEX IF EXISTS idx_pg2026_outputs_batch
                RENAME TO idx_pg2026_assets_lib_batch;
            EXCEPTION WHEN OTHERS THEN NULL;
            END;
            BEGIN
              ALTER INDEX IF EXISTS idx_pg2026_outputs_user_deleted
                RENAME TO idx_pg2026_assets_lib_user_deleted;
            EXCEPTION WHEN OTHERS THEN NULL;
            END;
            BEGIN
              ALTER SEQUENCE playground_2026_outputs_id_seq
                RENAME TO playground_2026_assets_id_seq;
            EXCEPTION WHEN OTHERS THEN NULL;
            END;
            COMMENT ON TABLE playground_2026_assets IS
              '创作中心2026资源库（原 outputs：上传/作品，含 media_type/source_type）';
          END IF;

          -- 3) 补齐资源库列
          IF EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = 'playground_2026_assets'
          ) THEN
            ALTER TABLE playground_2026_assets
              ADD COLUMN IF NOT EXISTS source_type TEXT NOT NULL DEFAULT 'work';
            ALTER TABLE playground_2026_assets
              ADD COLUMN IF NOT EXISTS tos_object_key TEXT NOT NULL DEFAULT '';
            ALTER TABLE playground_2026_assets
              ADD COLUMN IF NOT EXISTS file_size BIGINT NOT NULL DEFAULT 0;
            COMMENT ON COLUMN playground_2026_assets.source_type IS 'upload|work';
            COMMENT ON COLUMN playground_2026_assets.media_type IS 'image|video';
          END IF;

          -- 4) album_items.output_id → asset_id
          SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'playground_2026_album_items'
              AND column_name = 'output_id'
          ) INTO album_has_output_id;
          SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'playground_2026_album_items'
              AND column_name = 'asset_id'
          ) INTO album_has_asset_id;

          IF album_has_output_id AND NOT album_has_asset_id THEN
            ALTER TABLE playground_2026_album_items RENAME COLUMN output_id TO asset_id;
            COMMENT ON TABLE playground_2026_album_items IS '资源与分类的多对多归属';
          END IF;

          -- 5) 重建 workflow_assets → assets FK（DROP CASCADE 可能卸掉约束）
          IF EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = 'playground_2026_workflow_assets'
          ) AND EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = 'playground_2026_assets'
          ) AND NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints
            WHERE table_schema = 'public'
              AND table_name = 'playground_2026_workflow_assets'
              AND constraint_type = 'FOREIGN KEY'
              AND constraint_name = 'playground_2026_workflow_assets_asset_id_fkey'
          ) THEN
            BEGIN
              ALTER TABLE playground_2026_workflow_assets
                ADD CONSTRAINT playground_2026_workflow_assets_asset_id_fkey
                FOREIGN KEY (asset_id) REFERENCES playground_2026_assets(id) ON DELETE CASCADE;
            EXCEPTION WHEN duplicate_object THEN NULL;
            END;
          END IF;
        END
        $mig$"#
    );

    // ── 创作中心2026：用户偏好（收藏模型 mid、画布编辑器等 JSON）──
    once_migration!(pool, done, "pg2026_user_prefs_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_user_prefs (
            user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            prefs JSONB NOT NULL DEFAULT '{}'::jsonb,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "COMMENT ON TABLE playground_2026_user_prefs IS '创作中心2026用户偏好（收藏模型、画布编辑器等）'",
        "COMMENT ON COLUMN playground_2026_user_prefs.prefs IS 'JSON：favorite_mids、flow_editor 等'"
    );

    Ok(())
}
