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
    // ── 创作中心2026：独立插件注册 + 独立表（与 playground 无共享） ──
    once_migration!(pool, done, "init_playground_2026_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category)
           VALUES ('playground_2026', '创作中心2026', '提供直接的视频、图片、声音、聊天模型体验服务（2026独立版）', 0, 'user')
           ON CONFLICT (name) DO NOTHING"#,
        r#"CREATE TABLE IF NOT EXISTS playground_2026_projects (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            uid TEXT NOT NULL,
            name TEXT NOT NULL DEFAULT '未命名项目',
            description TEXT DEFAULT '',
            cover_url TEXT DEFAULT '',
            canvas_data TEXT DEFAULT '{}',
            is_deleted INTEGER NOT NULL DEFAULT 0,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg2026_projects_user ON playground_2026_projects(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_projects_uid ON playground_2026_projects(uid)",
        "COMMENT ON TABLE playground_2026_projects IS '创作中心2026项目表（独立于 playground_projects）'",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_assets (
            id BIGSERIAL PRIMARY KEY,
            project_id BIGINT NOT NULL REFERENCES playground_2026_projects(id) ON DELETE CASCADE,
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
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg2026_assets_project ON playground_2026_assets(project_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_assets_user ON playground_2026_assets(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_assets_type ON playground_2026_assets(asset_type)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_assets_file_hash ON playground_2026_assets(file_hash)",
        "COMMENT ON TABLE playground_2026_assets IS '创作中心2026素材表（独立于 playground_assets）'",
        r#"CREATE TABLE IF NOT EXISTS user_model_configs_2026 (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            model_mid TEXT NOT NULL,
            param_values TEXT NOT NULL DEFAULT '{}',
            is_locked INTEGER NOT NULL DEFAULT 1,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(user_id, model_mid)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_user_model_configs_2026_user ON user_model_configs_2026(user_id)",
        "COMMENT ON TABLE user_model_configs_2026 IS '用户在创作中心2026锁定的模型自定义参数配置'"
    );

    // ── 创作中心2026：独立令牌限制字段 ──
    once_migration!(pool, done, "init_playground_2026_token_flag_v1",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS only_playground_2026 BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN api_tokens.only_playground_2026 IS '是否仅限创作中心2026使用，1=是，0=否'"
    );

    // ── 站点门户增强版（商业插件，与 site_portal 配置/静态目录隔离）──
    once_migration!(
        pool,
        done,
        "init_site_portal_pro_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category)
           VALUES ('site_portal_pro', '站点门户增强版', '提供站点内容的基本介绍，支持生成静态HTML页面用于SEO/GEO优化（商业增强版，与站点门户独立）', 0, 'user')
           ON CONFLICT (name) DO NOTHING"#
    );

    // ── 站点门户：默认改用经典科技风格接管首页（旧默认 apply_to_homepage=false 走托管页）──
    once_migration!(
        pool,
        done,
        "portal_default_classic_style_homepage_v1",
        r#"UPDATE plugin_configs
           SET config_value = jsonb_set(config_value::jsonb, '{apply_to_homepage}', 'true', true)::text,
               updated_at = CURRENT_TIMESTAMP
           WHERE plugin_name IN ('site_portal', 'site_portal_pro')
             AND config_key = 'style_config'
             AND COALESCE(config_value::jsonb->>'apply_to_homepage', 'false') = 'false'"#,
        r#"UPDATE plugin_configs
           SET config_value = jsonb_set(
                 COALESCE(NULLIF(config_value, '')::jsonb, '{"enabled":false,"html":""}'::jsonb),
                 '{enabled}', 'false', true
               )::text,
               updated_at = CURRENT_TIMESTAMP
           WHERE plugin_name IN ('site_portal', 'site_portal_pro')
             AND config_key = 'custom_homepage'
             AND COALESCE(config_value::jsonb->>'enabled', 'false') = 'true'
             AND COALESCE(TRIM(config_value::jsonb->>'html'), '') = ''"#
    );

    // ── 站点门户增强版独立 DOCS 文档表初始化 ──
    once_migration!(
        pool,
        done,
        "site_portal_pro_docs_init_v1",
        r#"CREATE TABLE IF NOT EXISTS site_portal_pro_docs (
            id SERIAL PRIMARY KEY,
            parent_id INTEGER NULL REFERENCES site_portal_pro_docs(id) ON DELETE CASCADE,
            title VARCHAR(255) NOT NULL,
            content TEXT DEFAULT '',
            is_dir INTEGER NOT NULL DEFAULT 0,
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            slug VARCHAR(255) DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS site_portal_pro_docs_intl (
            id SERIAL PRIMARY KEY,
            doc_id INTEGER NOT NULL REFERENCES site_portal_pro_docs(id) ON DELETE CASCADE,
            lang VARCHAR(10) NOT NULL,
            title VARCHAR(255) NOT NULL,
            content TEXT DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text),
            UNIQUE(doc_id, lang)
        )"#
    );

    // ── 站点门户增强版 DOCS 二级分类 ──
    once_migration!(pool, done, "site_portal_pro_doc_categories_v1",
        r#"CREATE TABLE IF NOT EXISTS site_portal_pro_doc_categories (
            id SERIAL PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "ALTER TABLE site_portal_pro_docs ADD COLUMN IF NOT EXISTS category_id INTEGER NULL REFERENCES site_portal_pro_doc_categories(id) ON DELETE SET NULL",
        "ALTER TABLE plugin_docs ADD COLUMN IF NOT EXISTS category_id INTEGER NULL",
        r#"INSERT INTO site_portal_pro_doc_categories (name, sort_order)
           SELECT v.name, v.sort_order
           FROM (VALUES
             ('API 参考', 10),
             ('部署安装', 20),
             ('商务支持', 30)
           ) AS v(name, sort_order)
           WHERE NOT EXISTS (SELECT 1 FROM site_portal_pro_doc_categories LIMIT 1)"#,
        r#"UPDATE site_portal_pro_docs d
           SET category_id = c.id
           FROM site_portal_pro_doc_categories c
           WHERE d.parent_id IS NULL
             AND d.category_id IS NULL
             AND c.name = 'API 参考'"#
    );

    // 将仍未归属的根文档挂到「API 参考」（兼容迁移后才导入的旧数据）
    once_migration!(
        pool,
        done,
        "site_portal_pro_docs_backfill_api_category_v1",
        r#"INSERT INTO site_portal_pro_doc_categories (name, sort_order)
           SELECT 'API 参考', 10
           WHERE NOT EXISTS (
             SELECT 1 FROM site_portal_pro_doc_categories WHERE name = 'API 参考'
           )"#,
        r#"UPDATE site_portal_pro_docs d
           SET category_id = c.id
           FROM site_portal_pro_doc_categories c
           WHERE d.parent_id IS NULL
             AND d.category_id IS NULL
             AND c.name = 'API 参考'"#
    );

    // 合并同名分类（忽略大小写和所有空格），并加唯一约束防止再插入
    once_migration!(
        pool,
        done,
        "site_portal_pro_doc_categories_dedupe_v1",
        r#"UPDATE site_portal_pro_docs d
           SET category_id = keep.id
           FROM site_portal_pro_doc_categories dup
           JOIN (
             SELECT REPLACE(LOWER(name), ' ', '') AS norm_name, MIN(id) AS id
             FROM site_portal_pro_doc_categories
             GROUP BY REPLACE(LOWER(name), ' ', '')
           ) keep ON REPLACE(LOWER(dup.name), ' ', '') = keep.norm_name
           WHERE d.category_id = dup.id
             AND dup.id <> keep.id"#,
        r#"DELETE FROM site_portal_pro_doc_categories a
           USING site_portal_pro_doc_categories b
           WHERE REPLACE(LOWER(a.name), ' ', '') = REPLACE(LOWER(b.name), ' ', '') AND a.id > b.id"#,
        r#"CREATE UNIQUE INDEX IF NOT EXISTS idx_site_portal_pro_doc_categories_name
           ON site_portal_pro_doc_categories (name)"#
    );

    // 确保 site_portal_pro 「使用指南」分类存在并置为首位 (sort_order = 1)
    once_migration!(
        pool,
        done,
        "site_portal_pro_doc_categories_user_guide_v2",
        r#"INSERT INTO site_portal_pro_doc_categories (name, sort_order)
           SELECT '使用指南', 1
           WHERE NOT EXISTS (
             SELECT 1 FROM site_portal_pro_doc_categories WHERE name = '使用指南'
           )"#,
        r#"UPDATE site_portal_pro_doc_categories SET sort_order = 1 WHERE name = '使用指南'"#
    );

    // 确保 site_portal_pro 分类包含 is_default 列，默认将「使用指南」置为默认分类 (is_default = 1)
    once_migration!(pool, done, "site_portal_pro_doc_categories_is_default_v1",
        "ALTER TABLE site_portal_pro_doc_categories ADD COLUMN IF NOT EXISTS is_default INTEGER NOT NULL DEFAULT 0",
        r#"UPDATE site_portal_pro_doc_categories SET is_default = 1 WHERE name = '使用指南' AND NOT EXISTS (SELECT 1 FROM site_portal_pro_doc_categories WHERE is_default = 1)"#
    );

    // 新增「商务合作」分类，并将原挂在 API 参考下的商务合作文档迁入
    once_migration!(
        pool,
        done,
        "site_portal_pro_doc_categories_business_coop_v1",
        r#"UPDATE site_portal_pro_doc_categories
           SET name = '商务合作', sort_order = 40, updated_at = CURRENT_TIMESTAMP
           WHERE name = '商务支持'
             AND NOT EXISTS (
               SELECT 1 FROM site_portal_pro_doc_categories WHERE name = '商务合作'
             )"#,
        r#"INSERT INTO site_portal_pro_doc_categories (name, sort_order)
           SELECT '商务合作', 40
           WHERE NOT EXISTS (
             SELECT 1 FROM site_portal_pro_doc_categories WHERE name = '商务合作'
           )"#,
        r#"UPDATE site_portal_pro_doc_categories SET sort_order = 40 WHERE name = '商务合作'"#,
        r#"UPDATE site_portal_pro_docs d
           SET category_id = c.id, updated_at = CURRENT_TIMESTAMP
           FROM site_portal_pro_doc_categories c
           WHERE c.name = '商务合作'
             AND d.parent_id IS NULL
             AND (
               d.slug = 'business-cooperation'
               OR REPLACE(LOWER(d.title), ' ', '') IN ('商务合作', 'businesscooperation')
             )"#
    );

    // ── 火山方舟：钱包入账锚点 + 停用原因；删除已废弃的独立 limit_quota ──
    once_migration!(pool, done, "ark_bindings_wallet_fuse_v1",
        "ALTER TABLE ark_endpoint_bindings ADD COLUMN IF NOT EXISTS wallet_charged_quota DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE ark_endpoint_bindings ADD COLUMN IF NOT EXISTS fuse_reason TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE ark_endpoint_bindings DROP COLUMN IF EXISTS limit_quota",
        "COMMENT ON COLUMN ark_endpoint_bindings.wallet_charged_quota IS '已成功扣入用户钱包的累计消费(元)，与 used_quota 差值即为待扣/待退'",
        "COMMENT ON COLUMN ark_endpoint_bindings.fuse_reason IS '停用原因: wallet=余额熔断(可自动恢复) manual=管理员停用(cron不拉起) 空=正常'",
        // 存量 status=0 视为余额熔断，保证上线后仍可被 cron 自动恢复
        "UPDATE ark_endpoint_bindings SET fuse_reason = 'wallet' WHERE status = 0 AND fuse_reason = ''"
    );

    // ── 级联支持 480p 目标：补默认 res_mul（已有 key 不覆盖）──
    once_migration!(
        pool,
        done,
        "cascade_480p_target_v1",
        r#"UPDATE forward_rules
           SET config_json = jsonb_set(
             COALESCE(config_json::jsonb, '{}'::jsonb),
             '{res_mul}',
             '{"480p":1.5}'::jsonb || COALESCE(config_json::jsonb -> 'res_mul', '{}'::jsonb),
             true
           )::text
           WHERE COALESCE(config_json::jsonb->>'is_cascade', 'false') IN ('true', '1')
             AND (config_json::jsonb -> 'res_mul' -> '480p') IS NULL"#
    );

    // ── 团队营销：主题推广落地页 ──
    once_migration!(
        pool,
        done,
        "theme_promotions_v1",
        r#"CREATE TABLE IF NOT EXISTS theme_promotions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            html_content TEXT NOT NULL DEFAULT '',
            status INTEGER NOT NULL DEFAULT 1,
            is_permanent INTEGER NOT NULL DEFAULT 1,
            start_at TIMESTAMPTZ,
            end_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_theme_promotions_status ON theme_promotions (status)",
        "COMMENT ON TABLE theme_promotions IS '高级营销主题推广落地页（HTML 单页活动）'",
        "COMMENT ON COLUMN theme_promotions.slug IS '公开路径段，访问 /promo/{slug}'",
        "COMMENT ON COLUMN theme_promotions.status IS '1=上线 0=下线'",
        "COMMENT ON COLUMN theme_promotions.is_permanent IS '1=长期有效 0=按 start_at/end_at 判断'"
    );

    // ── 主题推广：INTEGER → BIGINT，与全局 i64 约定对齐 ──
    once_migration!(
        pool,
        done,
        "theme_promotions_bigint_v1",
        "ALTER TABLE theme_promotions ALTER COLUMN status TYPE BIGINT",
        "ALTER TABLE theme_promotions ALTER COLUMN is_permanent TYPE BIGINT"
    );

    // ── 主题推广：新增 promo_type 字段（system=系统推广 custom=自定义推广） ──
    once_migration!(pool, done, "theme_promotions_promo_type_v1",
        "ALTER TABLE theme_promotions ADD COLUMN IF NOT EXISTS promo_type TEXT NOT NULL DEFAULT 'custom'",
        "COMMENT ON COLUMN theme_promotions.promo_type IS 'system=系统推广(直接跳转首页) custom=自定义推广(单页HTML)'"
    );

    // ── 主题推广：新增 target_path 字段并预置默认系统推广（首页推广 & 模型广场推广） ──
    once_migration!(pool, done, "theme_promotions_system_defaults_v1",
        "ALTER TABLE theme_promotions ADD COLUMN IF NOT EXISTS target_path TEXT NOT NULL DEFAULT '/'",
        "COMMENT ON COLUMN theme_promotions.target_path IS '系统推广点击后跳转的目标路径'",
        r#"INSERT INTO theme_promotions (id, title, slug, html_content, promo_type, target_path, status, is_permanent)
           VALUES
             ('preset_system_portal', '首页推广', 'portal', '', 'system', '/', 1, 1),
             ('preset_system_models', '模型广场推广', 'models', '', 'system', '/home/models', 1, 1)
           ON CONFLICT (slug) DO UPDATE SET target_path = EXCLUDED.target_path, promo_type = EXCLUDED.promo_type"#
    );

    // 日志 HA 标志请求时快照（读路径勿 JOIN 当前 channels.provider_type）
    once_migration!(pool, done, "logs_is_ha_snapshot_v1",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS is_ha INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE logs_archive ADD COLUMN IF NOT EXISTS is_ha INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN logs.is_ha IS '请求当时是否走高可用组(1=是)；写路径快照，不随渠道配置变更'"
    );

    // ── 营销链接点击统计：同 IP 同日同链接只计 1 次（粗略 UV） ──
    once_migration!(pool, done, "marketing_link_clicks_v1",
        r#"CREATE TABLE IF NOT EXISTS marketing_link_click_dedup (
            link_type TEXT NOT NULL,
            link_key TEXT NOT NULL,
            promoter_uid TEXT NOT NULL,
            client_ip TEXT NOT NULL,
            click_date DATE NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (link_type, link_key, promoter_uid, client_ip, click_date)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS marketing_link_click_stats (
            link_type TEXT NOT NULL,
            link_key TEXT NOT NULL,
            promoter_uid TEXT NOT NULL,
            click_count BIGINT NOT NULL DEFAULT 0,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (link_type, link_key, promoter_uid)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_mlc_stats_promoter ON marketing_link_click_stats (promoter_uid)",
        "COMMENT ON TABLE marketing_link_click_dedup IS '营销链接点击去重：link_type+link_key+promoter+IP+自然日唯一'",
        "COMMENT ON TABLE marketing_link_click_stats IS '营销链接点击累计（按推广员与链接维度）'",
        "COMMENT ON COLUMN marketing_link_click_dedup.link_type IS 'invite | team_invite | theme_promo'",
        "COMMENT ON COLUMN marketing_link_click_dedup.link_key IS 'invite=_ ; team_invite=邀请码 ; theme_promo=slug'",
        "COMMENT ON COLUMN marketing_link_click_dedup.click_date IS '站点时区下的自然日'"
    );

    // ── 补全预置 ATP Token 视频转发规则与用户等级折扣模式 ──
    once_migration!(pool, done, "user_level_discount_type_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT 'ATP Token 视频生成', 'atp', '将标准视频生成请求（/v1/video/generations）或阿里百炼格式参数转换为 ATP Token 媒体视频 API 格式（omni tasks），支持 Seedance / Kling / Wan / HappyHorse 系列模型，自动轮询及参数兼容', '{"target_type":"atp_video","path_rewrite":{"old":"/v1/video/generations","new":"/omni/media/v1/contents/generations/tasks"},"auth_type":"bearer","poll_path":"/omni/media/v1/contents/generations/tasks/${task_id}"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = 'ATP Token 视频生成')"#,
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS discount_type INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN user_levels.discount_type IS '折扣模式: 0=不选择(跟随老逻辑全站+等级), 1=使用全站折扣, 2=使用等级折扣'"
    );

    // ── 补全预置 MiniMax 视频生成转发规则（并回填缺失 eid）──
    once_migration!(pool, done, "minimax_video_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT 'MiniMax 视频生成', 'minimax', 'MiniMax V2 视频生成原生通道，自动将多模态请求转换为 prompt 数组，并支持异步任务轮询', '{"target_type":"minimax_video","path_rewrite":{"old":"/v1/video/generations","new":"/v2/video_generation"},"auth_type":"bearer","poll_path":"/v2/query/video_generation/${task_id}"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = 'MiniMax 视频生成')"#,
        "UPDATE forward_rules SET eid = '1' || lpad((floor(random() * 10000)::int)::text, 4, '0') WHERE eid IS NULL OR eid = ''"
    );

    Ok(())
}
