/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use sqlx::PgPool;
use std::collections::HashSet;

#[allow(unused_imports)]
use crate::db::migrations::{once_heavy_migration, once_migration};

pub async fn run(pool: &PgPool, done: &mut HashSet<String>) -> anyhow::Result<()> {
    // ── 站点门户增强版 全面升级整型字段为 BIGINT / INT8 (i64) 对齐系统规范 ──
    once_migration!(
        pool,
        done,
        "site_portal_pro_tables_to_bigint_v1",
        r#"DO $$
BEGIN
    -- 临时解除相关外键约束，以支持安全修改主键与外键列数据类型为 BIGINT
    ALTER TABLE site_portal_pro_articles DROP CONSTRAINT IF EXISTS site_portal_pro_articles_category_id_fkey;
    ALTER TABLE site_portal_pro_article_categories DROP CONSTRAINT IF EXISTS site_portal_pro_article_categories_parent_id_fkey;
    ALTER TABLE site_portal_pro_docs DROP CONSTRAINT IF EXISTS site_portal_pro_docs_parent_id_fkey;
    ALTER TABLE site_portal_pro_docs DROP CONSTRAINT IF EXISTS site_portal_pro_docs_category_id_fkey;
    ALTER TABLE site_portal_pro_docs_intl DROP CONSTRAINT IF EXISTS site_portal_pro_docs_intl_doc_id_fkey;

    -- 1. site_portal_pro_doc_categories
    ALTER TABLE site_portal_pro_doc_categories 
        ALTER COLUMN id TYPE BIGINT,
        ALTER COLUMN sort_order TYPE BIGINT,
        ALTER COLUMN is_default TYPE BIGINT,
        ALTER COLUMN is_preset TYPE BIGINT;

    -- 2. site_portal_pro_docs
    ALTER TABLE site_portal_pro_docs 
        ALTER COLUMN id TYPE BIGINT,
        ALTER COLUMN parent_id TYPE BIGINT,
        ALTER COLUMN category_id TYPE BIGINT,
        ALTER COLUMN is_dir TYPE BIGINT,
        ALTER COLUMN sort_order TYPE BIGINT,
        ALTER COLUMN is_active TYPE BIGINT,
        ALTER COLUMN is_preset TYPE BIGINT;

    -- 3. site_portal_pro_docs_intl
    ALTER TABLE site_portal_pro_docs_intl 
        ALTER COLUMN id TYPE BIGINT,
        ALTER COLUMN doc_id TYPE BIGINT;

    -- 4. site_portal_pro_article_categories
    ALTER TABLE site_portal_pro_article_categories 
        ALTER COLUMN id TYPE BIGINT,
        ALTER COLUMN parent_id TYPE BIGINT,
        ALTER COLUMN sort_order TYPE BIGINT;

    -- 5. site_portal_pro_articles
    ALTER TABLE site_portal_pro_articles 
        ALTER COLUMN id TYPE BIGINT,
        ALTER COLUMN category_id TYPE BIGINT,
        ALTER COLUMN sort_order TYPE BIGINT,
        ALTER COLUMN is_active TYPE BIGINT,
        ALTER COLUMN views TYPE BIGINT;

    -- 恢复外键约束
    ALTER TABLE site_portal_pro_articles ADD CONSTRAINT site_portal_pro_articles_category_id_fkey
        FOREIGN KEY (category_id) REFERENCES site_portal_pro_article_categories(id) ON DELETE SET NULL;
    ALTER TABLE site_portal_pro_article_categories ADD CONSTRAINT site_portal_pro_article_categories_parent_id_fkey
        FOREIGN KEY (parent_id) REFERENCES site_portal_pro_article_categories(id) ON DELETE CASCADE;
    ALTER TABLE site_portal_pro_docs ADD CONSTRAINT site_portal_pro_docs_parent_id_fkey
        FOREIGN KEY (parent_id) REFERENCES site_portal_pro_docs(id) ON DELETE CASCADE;
    ALTER TABLE site_portal_pro_docs ADD CONSTRAINT site_portal_pro_docs_category_id_fkey
        FOREIGN KEY (category_id) REFERENCES site_portal_pro_doc_categories(id) ON DELETE SET NULL;
    ALTER TABLE site_portal_pro_docs_intl ADD CONSTRAINT site_portal_pro_docs_intl_doc_id_fkey
        FOREIGN KEY (doc_id) REFERENCES site_portal_pro_docs(id) ON DELETE CASCADE;
END $$;"#
    );

    // ── 创作中心2026 全面升级整型字段为 BIGINT / INT8 (i64) 对齐系统规范 ──
    once_migration!(
        pool,
        done,
        "pg2026_tables_to_bigint_v1",
        r#"DO $$
BEGIN
    -- 1. playground_2026_projects
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'playground_2026_projects') THEN
        ALTER TABLE playground_2026_projects ALTER COLUMN is_deleted TYPE BIGINT, ALTER COLUMN is_pinned TYPE BIGINT;
    END IF;

    -- 2. playground_2026_assets
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'playground_2026_assets') THEN
        ALTER TABLE playground_2026_assets ALTER COLUMN is_deleted TYPE BIGINT;
    END IF;

    -- 3. user_model_configs_2026
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'user_model_configs_2026') THEN
        ALTER TABLE user_model_configs_2026 ALTER COLUMN is_locked TYPE BIGINT;
    END IF;

    -- 4. playground_2026_albums
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'playground_2026_albums') THEN
        ALTER TABLE playground_2026_albums ALTER COLUMN is_system TYPE BIGINT, ALTER COLUMN sort_order TYPE BIGINT;
    END IF;

    -- 5. playground_2026_workflows
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'playground_2026_workflows') THEN
        ALTER TABLE playground_2026_workflows ALTER COLUMN is_deleted TYPE BIGINT, ALTER COLUMN is_pinned TYPE BIGINT;
    END IF;

    -- 6. playground_2026_project_assets
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'playground_2026_project_assets') THEN
        ALTER TABLE playground_2026_project_assets ALTER COLUMN is_deleted TYPE BIGINT;
    END IF;

    -- 7. playground_2026_skill_categories
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'playground_2026_skill_categories') THEN
        ALTER TABLE playground_2026_skill_categories ALTER COLUMN sort_order TYPE BIGINT;
    END IF;

    -- 8. playground_2026_outputs (若存量环境未被 drop 或 rename)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'playground_2026_outputs') THEN
        ALTER TABLE playground_2026_outputs ALTER COLUMN is_deleted TYPE BIGINT;
    END IF;
END $$;"#
    );

    // ── API 文档 DocsApi 全面升级整型字段为 BIGINT / INT8 (i64) 对齐系统统一规范 ──
    once_migration!(
        pool,
        done,
        "docs_api_tables_to_bigint_v1",
        r#"DO $$
DECLARE
    r RECORD;
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'plugin_docs') THEN
        -- 临时解除相关外键约束，以支持安全修改主键与外键列数据类型为 BIGINT
        FOR r IN (
            SELECT tc.table_name, tc.constraint_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
            WHERE tc.constraint_type = 'FOREIGN KEY'
              AND tc.table_name IN ('plugin_docs', 'plugin_docs_intl')
        ) LOOP
            EXECUTE format('ALTER TABLE %I DROP CONSTRAINT IF EXISTS %I', r.table_name, r.constraint_name);
        END LOOP;

        -- 1. plugin_docs
        ALTER TABLE plugin_docs 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN parent_id TYPE BIGINT,
            ALTER COLUMN is_dir TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'plugin_docs' AND column_name = 'category_id') THEN
            ALTER TABLE plugin_docs ALTER COLUMN category_id TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'plugin_docs' AND column_name = 'is_preset') THEN
            ALTER TABLE plugin_docs ALTER COLUMN is_preset TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'plugin_docs' AND column_name = 'default_expanded') THEN
            ALTER TABLE plugin_docs ALTER COLUMN default_expanded TYPE BIGINT;
        END IF;

        -- 2. plugin_docs_intl
        IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'plugin_docs_intl') THEN
            ALTER TABLE plugin_docs_intl ALTER COLUMN id TYPE BIGINT, ALTER COLUMN doc_id TYPE BIGINT;
        END IF;

        -- 恢复外键约束
        ALTER TABLE plugin_docs ADD CONSTRAINT plugin_docs_parent_id_fkey
            FOREIGN KEY (parent_id) REFERENCES plugin_docs(id) ON DELETE CASCADE;
        IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'plugin_docs_intl') THEN
            ALTER TABLE plugin_docs_intl ADD CONSTRAINT plugin_docs_intl_doc_id_fkey
                FOREIGN KEY (doc_id) REFERENCES plugin_docs(id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;"#
    );

    // ── 站点系统与用户管理各模块 全面升级整型字段为 BIGINT / INT8 (i64) 对齐系统统一规范 ──
    once_migration!(
        pool,
        done,
        "site_and_users_modules_to_bigint_v1",
        r#"DO $$
BEGIN
    -- 1. users
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'users') THEN
        ALTER TABLE users ALTER COLUMN pay_enabled TYPE BIGINT;
    END IF;

    -- 2. user_levels
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'user_levels') THEN
        ALTER TABLE user_levels 
            ALTER COLUMN sort_order TYPE BIGINT,
            ALTER COLUMN allow_view_log_details TYPE BIGINT,
            ALTER COLUMN discount_type TYPE BIGINT,
            ALTER COLUMN invoice_enabled TYPE BIGINT;
    END IF;

    -- 3. admin_groups
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'admin_groups') THEN
        ALTER TABLE admin_groups ALTER COLUMN sort_order TYPE BIGINT;
    END IF;

    -- 4. announcements
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'announcements') THEN
        ALTER TABLE announcements 
            ALTER COLUMN is_pinned TYPE BIGINT,
            ALTER COLUMN is_popup TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT;
    END IF;

    -- 5. contact_bind_limits
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'contact_bind_limits') THEN
        ALTER TABLE contact_bind_limits ALTER COLUMN max_accounts TYPE BIGINT;
    END IF;

    -- 6. verification_codes
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'verification_codes') THEN
        ALTER TABLE verification_codes ALTER COLUMN attempts TYPE BIGINT;
    END IF;

    -- 7. redemptions
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'redemptions') THEN
        ALTER TABLE redemptions 
            ALTER COLUMN is_used TYPE BIGINT,
            ALTER COLUMN max_uses TYPE BIGINT,
            ALTER COLUMN used_count TYPE BIGINT,
            ALTER COLUMN per_user_limit TYPE BIGINT,
            ALTER COLUMN per_user_activity_limit TYPE BIGINT,
            ALTER COLUMN status TYPE BIGINT;
    END IF;
END $$;"#
    );

    // ── 令牌管理模块 全面升级整型字段为 BIGINT / INT8 (i64) 对齐系统统一规范 ──
    once_migration!(
        pool,
        done,
        "api_tokens_to_bigint_v1",
        r#"DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'api_tokens') THEN
        ALTER TABLE api_tokens 
            ALTER COLUMN rps_limit TYPE BIGINT,
            ALTER COLUMN rpm_limit TYPE BIGINT,
            ALTER COLUMN high_availability TYPE BIGINT;
    END IF;
END $$;"#
    );

    // ── 营销管理各模块（兑换管理、提示通知、推广团队、主题推广、返佣明细等）全面升级整型与序列为 BIGINT / INT8 (i64) 对齐系统统一规范 ──
    once_migration!(
        pool,
        done,
        "marketing_module_to_bigint_v1",
        r#"DO $$
BEGIN
    -- 1. commissions (佣金明细主键与充值外键对齐 BIGINT)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'commissions') THEN
        ALTER TABLE commissions ALTER COLUMN id TYPE BIGINT, ALTER COLUMN recharge_id TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS commissions_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 2. marketing_teams (团队营销主表对齐 BIGINT)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'marketing_teams') THEN
        ALTER TABLE marketing_teams 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN max_members TYPE BIGINT,
            ALTER COLUMN members_can_set_level TYPE BIGINT,
            ALTER COLUMN members_can_set_pay TYPE BIGINT,
            ALTER COLUMN members_can_view_logs TYPE BIGINT,
            ALTER COLUMN leader_can_remove_members TYPE BIGINT,
            ALTER COLUMN members_can_apply_invoice TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS marketing_teams_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 3. marketing_team_leaders (团队负责人关联表对齐 BIGINT)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'marketing_team_leaders') THEN
        ALTER TABLE marketing_team_leaders ALTER COLUMN id TYPE BIGINT, ALTER COLUMN team_id TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS marketing_team_leaders_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 4. marketing_team_members (团队成员关联表对齐 BIGINT)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'marketing_team_members') THEN
        ALTER TABLE marketing_team_members ALTER COLUMN id TYPE BIGINT, ALTER COLUMN team_id TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS marketing_team_members_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 5. theme_promotions (主题推广落地页对齐 BIGINT)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'theme_promotions') THEN
        ALTER TABLE theme_promotions ALTER COLUMN status TYPE BIGINT, ALTER COLUMN is_permanent TYPE BIGINT;
    END IF;

    -- 6. marketing_link_click_stats (营销链接点击统计对齐 BIGINT)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'marketing_link_click_stats') THEN
        ALTER TABLE marketing_link_click_stats ALTER COLUMN click_count TYPE BIGINT;
    END IF;

    -- 7. redemptions (兑换码主表对齐 BIGINT)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'redemptions') THEN
        ALTER TABLE redemptions 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN is_used TYPE BIGINT,
            ALTER COLUMN max_uses TYPE BIGINT,
            ALTER COLUMN used_count TYPE BIGINT,
            ALTER COLUMN per_user_limit TYPE BIGINT,
            ALTER COLUMN per_user_activity_limit TYPE BIGINT,
            ALTER COLUMN status TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS redemptions_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 8. redemption_logs (兑换日志表对齐 BIGINT)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'redemption_logs') THEN
        ALTER TABLE redemption_logs ALTER COLUMN id TYPE BIGINT, ALTER COLUMN redemption_id TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS redemption_logs_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 9. announcements (公告通知主表对齐 BIGINT)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'announcements') THEN
        ALTER TABLE announcements 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN is_pinned TYPE BIGINT,
            ALTER COLUMN is_popup TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS announcements_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;
END $$;"#
    );

    // ── 模型管理各模块（模型列表、预设模型库、计费规则、转发规则、官方/API服务商、模型类型、用户模型锁定等）全面升级整型与序列为 BIGINT / INT8 (i64) 对齐系统统一规范 ──
    once_migration!(
        pool,
        done,
        "models_management_modules_to_bigint_v1",
        r#"DO $$
BEGIN
    -- 1. models (模型列表主表)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'models') THEN
        ALTER TABLE models 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN provider_id TYPE BIGINT,
            ALTER COLUMN api_provider_id TYPE BIGINT,
            ALTER COLUMN type_id TYPE BIGINT,
            ALTER COLUMN billing_rule_id TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT,
            ALTER COLUMN enable_log_content TYPE BIGINT,
            ALTER COLUMN site_discount_enabled TYPE BIGINT,
            ALTER COLUMN global_discount_enabled TYPE BIGINT,
            ALTER COLUMN is_system TYPE BIGINT,
            ALTER COLUMN is_listed TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS models_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 2. model_library (系统模型库目录)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'model_library') THEN
        ALTER TABLE model_library 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN provider_id TYPE BIGINT,
            ALTER COLUMN api_provider_id TYPE BIGINT,
            ALTER COLUMN type_id TYPE BIGINT,
            ALTER COLUMN billing_rule_id TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT,
            ALTER COLUMN enable_log_content TYPE BIGINT,
            ALTER COLUMN site_discount_enabled TYPE BIGINT,
            ALTER COLUMN global_discount_enabled TYPE BIGINT,
            ALTER COLUMN is_system TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS model_library_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 3. billing_rules (计费规则)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'billing_rules') THEN
        ALTER TABLE billing_rules 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN provider_id TYPE BIGINT,
            ALTER COLUMN type_id TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT,
            ALTER COLUMN is_system TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS billing_rules_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 4. forward_rules (转发重定向规则)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'forward_rules') THEN
        ALTER TABLE forward_rules 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT,
            ALTER COLUMN is_system TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS forward_rules_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 5. model_providers (模型官方服务商)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'model_providers') THEN
        ALTER TABLE model_providers 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT,
            ALTER COLUMN is_system TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS model_providers_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 6. model_types (模型类型/分类)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'model_types') THEN
        ALTER TABLE model_types 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT,
            ALTER COLUMN is_system TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS model_types_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 7. model_api_providers (模型API服务商)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'model_api_providers') THEN
        ALTER TABLE model_api_providers 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN sort_order TYPE BIGINT,
            ALTER COLUMN is_active TYPE BIGINT,
            ALTER COLUMN is_system TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS model_api_providers_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 8. user_model_configs (用户模型锁定配置)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'user_model_configs') THEN
        ALTER TABLE user_model_configs ALTER COLUMN id TYPE BIGINT, ALTER COLUMN is_locked TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS user_model_configs_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;
END $$;"#
    );

    // ── 财务管理各模块（系统资金明细、在线充值明细、赠送金明细、佣金明细、发票申请审核、财务数据分析等）全面升级整型与序列为 BIGINT / INT8 (i64) 对齐系统统一规范 ──
    once_migration!(
        pool,
        done,
        "finance_management_modules_to_bigint_v1",
        r#"DO $$
BEGIN
    -- 1. recharge_records (系统资金明细/赠送金明细/充值记录表及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'recharge_records') THEN
        ALTER TABLE recharge_records ALTER COLUMN id TYPE BIGINT;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'recharge_records' AND column_name = 'commission_posted') THEN
            ALTER TABLE recharge_records ALTER COLUMN commission_posted TYPE BIGINT;
        END IF;
        ALTER SEQUENCE IF EXISTS recharge_records_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 2. orders (在线充值订单明细表及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'orders') THEN
        ALTER TABLE orders ALTER COLUMN id TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS orders_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 3. commissions (佣金明细表、充值外键及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'commissions') THEN
        ALTER TABLE commissions ALTER COLUMN id TYPE BIGINT, ALTER COLUMN recharge_id TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS commissions_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 4. user_invoice_requests (发票申请审核表及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'user_invoice_requests') THEN
        ALTER TABLE user_invoice_requests ALTER COLUMN id TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS user_invoice_requests_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 5. usage_daily_stats (财务数据分析离线落地表及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'usage_daily_stats') THEN
        ALTER TABLE usage_daily_stats 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN token_id TYPE BIGINT,
            ALTER COLUMN channel_id TYPE BIGINT,
            ALTER COLUMN total_requests TYPE BIGINT,
            ALTER COLUMN total_tokens TYPE BIGINT,
            ALTER COLUMN success_count TYPE BIGINT,
            ALTER COLUMN fail_count TYPE BIGINT;
        ALTER TABLE usage_daily_stats ADD COLUMN IF NOT EXISTS prompt_tokens BIGINT NOT NULL DEFAULT 0;
        ALTER TABLE usage_daily_stats ADD COLUMN IF NOT EXISTS completion_tokens BIGINT NOT NULL DEFAULT 0;
        ALTER TABLE usage_daily_stats ADD COLUMN IF NOT EXISTS with_video_tokens BIGINT NOT NULL DEFAULT 0;
        ALTER TABLE usage_daily_stats 
            ALTER COLUMN prompt_tokens TYPE BIGINT,
            ALTER COLUMN completion_tokens TYPE BIGINT,
            ALTER COLUMN with_video_tokens TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS usage_daily_stats_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;
END $$;"#
    );

    // ── 任务列表与日志生态辅助各模块（site_icon_sync_logs、plugin_api_logs、ha_usage_logs、content_security_logs、增强日志与火山方舟多账号任务体系等）全面升级整型与序列为 BIGINT / INT8 (i64) 对齐系统统一规范 ──
    once_migration!(
        pool,
        done,
        "site_admin_tasks_and_logs_auxiliary_modules_to_bigint_v1",
        r#"DO $$
DECLARE
    fk_name text;
BEGIN
    -- 1. site_icon_sync_logs: 序列与主键对齐 BIGINT
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'site_icon_sync_logs') THEN
        ALTER TABLE site_icon_sync_logs ALTER COLUMN id TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS site_icon_sync_logs_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 2. plugin_api_logs: 主键、状态码与自增序列对齐 BIGINT
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'plugin_api_logs') THEN
        ALTER TABLE plugin_api_logs ALTER COLUMN id TYPE BIGINT, ALTER COLUMN status_code TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS plugin_api_logs_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 3. ha_usage_logs: 尝试次数、最终状态对齐 BIGINT
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'ha_usage_logs') THEN
        ALTER TABLE ha_usage_logs 
            ALTER COLUMN attempt_count TYPE BIGINT,
            ALTER COLUMN final_ok TYPE BIGINT,
            ALTER COLUMN final_status_code TYPE BIGINT;
    END IF;

    -- 4. content_security_logs: 评分与重试计数对齐 BIGINT
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'content_security_logs') THEN
        ALTER TABLE content_security_logs ALTER COLUMN score TYPE BIGINT;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'content_security_logs' AND column_name = 'retry_count') THEN
            ALTER TABLE content_security_logs ALTER COLUMN retry_count TYPE BIGINT;
        END IF;
    END IF;

    -- 5. plugin_tencent_enhance_logs: 耗时对齐 BIGINT
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'plugin_tencent_enhance_logs') THEN
        ALTER TABLE plugin_tencent_enhance_logs ALTER COLUMN latency_ms TYPE BIGINT;
    END IF;

    -- 6. plugin_volcengine_enhance_logs: 耗时对齐 BIGINT
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'plugin_volcengine_enhance_logs') THEN
        ALTER TABLE plugin_volcengine_enhance_logs ALTER COLUMN latency_ms TYPE BIGINT;
    END IF;

    -- 7. ark_accounts & ark_endpoint_bindings & ark_video_tasks (火山方舟多账号监控体系对齐 BIGINT)
    SELECT constraint_name INTO fk_name
    FROM information_schema.table_constraints
    WHERE table_schema = 'public' 
      AND table_name = 'ark_endpoint_bindings' 
      AND constraint_type = 'FOREIGN KEY';
    IF fk_name IS NOT NULL THEN
        EXECUTE 'ALTER TABLE ark_endpoint_bindings DROP CONSTRAINT ' || quote_ident(fk_name);
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'ark_accounts') THEN
        ALTER TABLE ark_accounts ALTER COLUMN id TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS ark_accounts_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'ark_endpoint_bindings') THEN
        ALTER TABLE ark_endpoint_bindings 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN account_id TYPE BIGINT,
            ALTER COLUMN status TYPE BIGINT;
        ALTER SEQUENCE IF EXISTS ark_endpoint_bindings_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'ark_endpoint_bindings')
       AND EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'ark_accounts') THEN
        ALTER TABLE ark_endpoint_bindings ADD CONSTRAINT ark_endpoint_bindings_account_id_fkey FOREIGN KEY (account_id) REFERENCES ark_accounts(id) ON DELETE CASCADE;
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'ark_video_tasks') THEN
        ALTER TABLE ark_video_tasks ALTER COLUMN account_id TYPE BIGINT;
    END IF;
END $$;"#
    );

    // ── 请求日志大表 logs 历史冗余与废弃索引瘦身清理（独立前置迁移：无论站点是否已执行 logs_table_to_bigint_v1 均强制执行，毫秒级卸载，彻底消除写放大）──
    once_migration!(
        pool,
        done,
        "logs_table_prune_redundant_indexes_v1",
        r#"DROP INDEX IF EXISTS 
            idx_logs_admin_dashboard_covering,
            idx_logs_admin_dashboard_covering_v2,
            idx_logs_created_at_stats_opt,
            idx_logs_created_at_agg,
            idx_logs_created_ts_agg,
            idx_logs_user_dashboard_covering,
            idx_logs_stats_opt,
            idx_logs_user_created,
            idx_logs_user_created_at,
            idx_logs_user_created_at_new,
            idx_logs_user_created_timestamptz,
            idx_logs_user_created_ts_agg,
            idx_logs_vision_created_at_new,
            idx_logs_user_vision_created,
            idx_logs_task_id_active,
            idx_logs_created_at_new,
            idx_logs_action_created_stats_new;"#
    );

    // ── 请求日志大表 logs 全面升级整型字段为 BIGINT / INT8 (i64)，动态过滤 + 256MB 内存防 OOM + 临时卸载整型覆盖索引 + 极速纯堆表重写 ──
    once_migration!(
        pool,
        done,
        "logs_table_to_bigint_v1",
        r#"DO $$
DECLARE
    alter_clauses text;
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'logs') THEN
        -- 0. 兜底确保 has_video 列存在（防历史版本锁超时跳过导致缺列），一步到位 BIGINT
        ALTER TABLE logs ADD COLUMN IF NOT EXISTS has_video BIGINT NOT NULL DEFAULT 0;

        -- 1. 动态筛选仍为 32 位及以下整型（integer / smallint）的列，若已全部升级则完全跳过重写
        SELECT string_agg('ALTER COLUMN ' || quote_ident(column_name) || ' TYPE BIGINT', ', ')
        INTO alter_clauses
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'logs'
          AND column_name IN (
              'prompt_tokens', 'completion_tokens', 'cached_tokens', 'latency_ms',
              'status_code', 'is_stream', 'is_ha', 'is_completed', 'has_video',
              'channel_config_id', 'commission_posted'
          )
          AND data_type IN ('integer', 'smallint');

        IF alter_clauses IS NOT NULL AND alter_clauses <> '' THEN
            -- 2. 注入 4 核最优 1024MB 内存与 2 并行 Worker（共 3 核全速重写，预留 1 核给操作系统/SSH 防假死）
            PERFORM set_config('maintenance_work_mem', '1024MB', true);
            PERFORM set_config('max_parallel_maintenance_workers', '2', true);

            -- 3. 临时卸载所有依赖整型列的核心索引（避免 ALTER TABLE 时同步重建巨型索引导致长达几十分钟的 ACCESS EXCLUSIVE 排他锁表与卡死）
            DROP INDEX IF EXISTS 
                idx_logs_is_completed_pending,
                idx_logs_pending_poll,
                idx_logs_inflight_user_action,
                idx_logs_orphan_scan,
                idx_logs_stat_covering,
                idx_logs_user_dashboard_covering_v2,
                idx_logs_action_created_stats;

            -- 4. 单语句批量重写纯堆表（无任何覆盖索引或整型依赖索引负担，极速完成，服务启动秒级放行，对齐 i64 类型契约）
            EXECUTE 'ALTER TABLE logs ' || alter_clauses;
        END IF;
    END IF;
END $$;"#
    );

    // ── 站点系统与渠道管理各模块（channels、channel_configs、channel_categories、forward_rules、tos_temp_files 与 logs 渠道配置关联等）全面升级整型与序列为 BIGINT / INT8 (i64) 对齐系统统一规范 ──
    once_migration!(
        pool,
        done,
        "channels_and_configs_modules_to_bigint_v1",
        r#"DO $$
BEGIN
    -- 1. channels (渠道/分组管理表及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'channels') THEN
        ALTER TABLE channels 
            ALTER COLUMN id TYPE BIGINT,
            ALTER COLUMN priority TYPE BIGINT,
            ALTER COLUMN weight TYPE BIGINT,
            ALTER COLUMN status TYPE BIGINT;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channels' AND column_name = 'max_rps') THEN
            ALTER TABLE channels ALTER COLUMN max_rps TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channels' AND column_name = 'sort_order') THEN
            ALTER TABLE channels ALTER COLUMN sort_order TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channels' AND column_name = 'preset_id') THEN
            ALTER TABLE channels ALTER COLUMN preset_id TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channels' AND column_name = 'category_id') THEN
            ALTER TABLE channels ALTER COLUMN category_id TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channels' AND column_name = 'auto_ban') THEN
            ALTER TABLE channels ALTER COLUMN auto_ban TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channels' AND column_name = 'auto_interval_minutes') THEN
            ALTER TABLE channels ALTER COLUMN auto_interval_minutes TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channels' AND column_name = 'balance_auto_interval_minutes') THEN
            ALTER TABLE channels ALTER COLUMN balance_auto_interval_minutes TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channels' AND column_name = 'balance_source') THEN
            ALTER TABLE channels ALTER COLUMN balance_source TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channels' AND column_name = 'is_stream') THEN
            ALTER TABLE channels ALTER COLUMN is_stream TYPE BIGINT;
        END IF;
        ALTER SEQUENCE IF EXISTS channels_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 2. channel_configs (上游预设配置表及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'channel_configs') THEN
        ALTER TABLE channel_configs ALTER COLUMN id TYPE BIGINT;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_configs' AND column_name = 'category_id') THEN
            ALTER TABLE channel_configs ALTER COLUMN category_id TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_configs' AND column_name = 'sort_order') THEN
            ALTER TABLE channel_configs ALTER COLUMN sort_order TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_configs' AND column_name = 'priority') THEN
            ALTER TABLE channel_configs ALTER COLUMN priority TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_configs' AND column_name = 'weight') THEN
            ALTER TABLE channel_configs ALTER COLUMN weight TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_configs' AND column_name = 'daily_reset_hour') THEN
            ALTER TABLE channel_configs ALTER COLUMN daily_reset_hour TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_configs' AND column_name = 'daily_reset_minute') THEN
            ALTER TABLE channel_configs ALTER COLUMN daily_reset_minute TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_configs' AND column_name = 'daily_reset_cooldown_minutes') THEN
            ALTER TABLE channel_configs ALTER COLUMN daily_reset_cooldown_minutes TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_configs' AND column_name = 'status') THEN
            ALTER TABLE channel_configs ALTER COLUMN status TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_configs' AND column_name = 'upstream_sync_interval_minutes') THEN
            ALTER TABLE channel_configs ALTER COLUMN upstream_sync_interval_minutes TYPE BIGINT;
        END IF;
        ALTER SEQUENCE IF EXISTS channel_configs_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 3. channel_categories (渠道分类表及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'channel_categories') THEN
        ALTER TABLE channel_categories ALTER COLUMN id TYPE BIGINT;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_categories' AND column_name = 'sort_order') THEN
            ALTER TABLE channel_categories ALTER COLUMN sort_order TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_categories' AND column_name = 'is_active') THEN
            ALTER TABLE channel_categories ALTER COLUMN is_active TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'channel_categories' AND column_name = 'is_system') THEN
            ALTER TABLE channel_categories ALTER COLUMN is_system TYPE BIGINT;
        END IF;
        ALTER SEQUENCE IF EXISTS channel_categories_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 4. forward_rules (转发规则表及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'forward_rules') THEN
        ALTER TABLE forward_rules ALTER COLUMN id TYPE BIGINT;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'forward_rules' AND column_name = 'is_active') THEN
            ALTER TABLE forward_rules ALTER COLUMN is_active TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'forward_rules' AND column_name = 'is_system') THEN
            ALTER TABLE forward_rules ALTER COLUMN is_system TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'forward_rules' AND column_name = 'sort_order') THEN
            ALTER TABLE forward_rules ALTER COLUMN sort_order TYPE BIGINT;
        END IF;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'forward_rules' AND column_name = 'channel_id') THEN
            ALTER TABLE forward_rules ALTER COLUMN channel_id TYPE BIGINT;
        END IF;
        ALTER SEQUENCE IF EXISTS forward_rules_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;

    -- 5. tos_temp_files (临时存储过期追踪表及序列对齐 BIGINT / INT8)
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'tos_temp_files') THEN
        ALTER TABLE tos_temp_files ALTER COLUMN id TYPE BIGINT;
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'tos_temp_files' AND column_name = 'channel_id') THEN
            ALTER TABLE tos_temp_files ALTER COLUMN channel_id TYPE BIGINT;
        END IF;
        ALTER SEQUENCE IF EXISTS tos_temp_files_id_seq AS BIGINT MAXVALUE 9223372036854775807;
    END IF;
END $$;"#
    );

    Ok(())
}
