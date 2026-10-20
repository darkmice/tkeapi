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
    // ── 更新火山画质增强/字幕擦除预设模型备注文案 ──
    once_migration!(pool, done, "update_volcengine_enhance_preset_remark_v1",
        "UPDATE models SET remark = '系统预设' WHERE remark = '火山引擎画质增强/字幕擦除插件预置模型，请勿删除'"
    );

    // ── 移除系统预设可灵 (Kling) 模型 ──
    once_migration!(pool, done, "remove_kling_system_preset_models_v1",
        "DELETE FROM models WHERE is_system = 1 AND (mid IN ('310004', '310005', '310006', '310007') OR model_id LIKE 'kling%')"
    );

    // ── 模型分类外键级联置空保护（防止删除服务商/类型时外键拦截） ──
    once_migration!(pool, done, "fk_model_classifications_on_delete_set_null_v1",
        "UPDATE billing_rules SET provider_id = NULL WHERE provider_id IS NOT NULL AND provider_id NOT IN (SELECT id FROM model_providers)",
        "UPDATE billing_rules SET type_id = NULL WHERE type_id IS NOT NULL AND type_id NOT IN (SELECT id FROM model_types)",
        "UPDATE models SET provider_id = NULL WHERE provider_id IS NOT NULL AND provider_id NOT IN (SELECT id FROM model_providers)",
        "UPDATE models SET type_id = NULL WHERE type_id IS NOT NULL AND type_id NOT IN (SELECT id FROM model_types)",
        "UPDATE models SET api_provider_id = NULL WHERE api_provider_id IS NOT NULL AND api_provider_id NOT IN (SELECT id FROM model_api_providers)",
        "ALTER TABLE billing_rules DROP CONSTRAINT IF EXISTS billing_rules_provider_id_fkey",
        "ALTER TABLE billing_rules ADD CONSTRAINT billing_rules_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES model_providers(id) ON DELETE SET NULL",
        "ALTER TABLE billing_rules DROP CONSTRAINT IF EXISTS billing_rules_type_id_fkey",
        "ALTER TABLE billing_rules ADD CONSTRAINT billing_rules_type_id_fkey FOREIGN KEY (type_id) REFERENCES model_types(id) ON DELETE SET NULL",
        "ALTER TABLE models DROP CONSTRAINT IF EXISTS models_provider_id_fkey",
        "ALTER TABLE models ADD CONSTRAINT models_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES model_providers(id) ON DELETE SET NULL",
        "ALTER TABLE models DROP CONSTRAINT IF EXISTS models_type_id_fkey",
        "ALTER TABLE models ADD CONSTRAINT models_type_id_fkey FOREIGN KEY (type_id) REFERENCES model_types(id) ON DELETE SET NULL",
        "ALTER TABLE models DROP CONSTRAINT IF EXISTS models_api_provider_id_fkey",
        "ALTER TABLE models ADD CONSTRAINT models_api_provider_id_fkey FOREIGN KEY (api_provider_id) REFERENCES model_api_providers(id) ON DELETE SET NULL"
    );

    if !done.contains("playground_2026_seedream_pro_image_special_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_2026_seedream_pro_image_special(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_seedream_pro_image_special_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_seedream_pro_image_special_v1".into());
                tracing::info!("创作中心2026 Seedream Pro 图片专用参数回填完成，更新 {} 条配置", n);
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_seedream_pro_image_special_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_seedream_pro_smart_ratio_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_2026_seedream_pro_image_special(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_seedream_pro_smart_ratio_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_seedream_pro_smart_ratio_v1".into());
                tracing::info!("创作中心2026 Seedream Pro 智能比例回填完成，更新 {} 条配置", n);
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_seedream_pro_smart_ratio_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_gpt_image_2_zh_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_2026_gpt_image_2_localization(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_gpt_image_2_zh_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_gpt_image_2_zh_v1".into());
                tracing::info!("创作中心2026 gpt-image-2 中文与描述菜单本地化迁移完成，更新 {} 条配置", n);
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_gpt_image_2_zh_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_gpt_image_2_variations_default_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_2026_gpt_image_2_localization(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_gpt_image_2_variations_default_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_gpt_image_2_variations_default_v1".into());
                tracing::info!("创作中心2026 gpt-image-2 默认变体数量修正迁移完成，更新 {} 条配置", n);
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_gpt_image_2_variations_default_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    once_migration!(
        pool,
        done,
        "playground_2026_gpt_image_2_strip_redundant_params_v1",
        include_str!("../helpers/playground_2026_gpt_image_2_strip_redundant_params_v1.sql")
    );

    if !done.contains("playground_2026_gpt_image_2_official_size_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_2026_gpt_image_2_localization(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_gpt_image_2_official_size_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_gpt_image_2_official_size_v1".into());
                tracing::info!(
                    "创作中心2026 gpt-image-2 官方比例/4K 尺寸迁移完成，更新 {} 条配置",
                    n
                );
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_gpt_image_2_official_size_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_gpt_image_2_multi_ref_v1") {
        match crate::db::migrations::helpers::playground::seed_playground_2026_gpt_image_2_localization(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_gpt_image_2_multi_ref_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_gpt_image_2_multi_ref_v1".into());
                tracing::info!(
                    "创作中心2026 gpt-image-2 多参考图上限迁移完成，更新 {} 条配置",
                    n
                );
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_gpt_image_2_multi_ref_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    once_migration!(
        pool,
        done,
        "playground_2026_seedream_image_special_official_v1",
        include_str!("../helpers/playground_2026_seedream_image_special_official_v1.sql")
    );

    once_migration!(
        pool,
        done,
        "playground_2026_seedream_strip_size_param_v1",
        include_str!("../helpers/playground_2026_seedream_strip_size_param_v1.sql")
    );

    once_migration!(
        pool,
        done,
        "playground_2026_seedream_5_0_pro_default_resolution_1_5k_v1",
        include_str!("../helpers/playground_2026_seedream_5_0_pro_default_resolution_1_5k_v1.sql")
    );

    // ── fal.ai MiniMax H3 Max 参考生视频转发规则 ──
    once_migration!(
        pool,
        done,
        "fal_minimax_h3_max_r2v_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT 'fal.ai MiniMax H3 Max 参考生视频', 'fal',
               'fal.ai MiniMax 参考生视频：队列提交 + 状态轮询 + 结果拉取。渠道 base_url=https://queue.fal.run，密钥填 FAL_KEY；模型 ID 填变体名如 h3-max（提交 /minimax/${model}/reference-to-video，轮询 /minimax/${model}/requests/${task_id}/status）',
               '{"target_type":"fal_video","path_rewrite":{"old":"/v1/video/generations","new":"/minimax/${model}/reference-to-video"},"auth_type":"key","poll_path":"/minimax/${model}/requests/${task_id}/status"}',
               '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (
            SELECT 1 FROM forward_rules WHERE name = 'fal.ai MiniMax H3 Max 参考生视频'
        )"#
    );

    // 已落地规则：修正 poll_path（去掉误加的 /reference-to-video）
    once_migration!(
        pool,
        done,
        "fal_minimax_h3_max_r2v_poll_path_v2",
        r#"UPDATE forward_rules
        SET config_json = jsonb_set(
              config_json::jsonb,
              '{poll_path}',
              '"/minimax/${model}/requests/${task_id}/status"'
            )::text,
            updated_at = NOW()
        WHERE name = 'fal.ai MiniMax H3 Max 参考生视频'
          AND config_json::jsonb->>'poll_path'
            = '/minimax/${model}/reference-to-video/requests/${task_id}/status'"#
    );

    // ── fal.ai MiniMax H3 Max 参考生视频官方系统计费规则 ──
    once_migration!(
        pool,
        done,
        "fal_minimax_h3_max_r2v_billing_rule_v1",
        r#"INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, extended_config, is_system, pid, pricing_type)
        SELECT 'fal.ai MiniMax H3 Max 参考生视频', 'duration', 0.0, 0.0, 0.0, 0.08, 'fal_ref_video',
               '{"free_ref_tokens":4096,"ref_token_rate_per_1k":0.02,"image_tokens_default":1024,"video_ref_tokens_per_sec":{"480p":2886,"768p":7459},"audio_ref_tokens_per_sec":80}',
               1, '7' || lpad((floor(random() * 10000)::int)::text, 4, '0'), 'official'
        WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = 'fal.ai MiniMax H3 Max 参考生视频')
        "#
    );

    // ── 同步现有环境预设模型为最新用户配置（若未被删除） ──
    once_migration!(pool, done, "sync_preset_models_latest_config_v1",
        "UPDATE models SET model_id = 'doubao-seedance-2-0-260128', original_id = 'doubao-seedance-2-0', model_id_alias = '', pre_deduction = 5.0, site_discount = 1.0, site_discount_enabled = 1, global_discount = 1.0, global_discount_enabled = 1, enable_log_content = 0, feature_attributes = '[]' WHERE mid = '310001'",
        "UPDATE models SET model_id = 'doubao-seedance-2-0-fast', original_id = 'doubao-seedance-2-0-fast', model_id_alias = '', pre_deduction = 5.0, site_discount = 1.0, site_discount_enabled = 1, global_discount = 1.0, global_discount_enabled = 1, enable_log_content = 1, feature_attributes = '[]' WHERE mid = '310002'",
        "UPDATE models SET model_id = 'doubao-seedance-2-5-260628', original_id = 'doubao-seedance-2-5', model_id_alias = '', pre_deduction = 5.0, site_discount = 1.0, site_discount_enabled = 1, global_discount = 1.0, global_discount_enabled = 1, enable_log_content = 1, feature_attributes = '[]' WHERE mid = '310003'",
        "UPDATE models SET model_id = 'doubao-seedance-2-0-mini', original_id = 'doubao-seedance-2-0-mini', model_id_alias = '', pre_deduction = 5.0, site_discount = 1.0, site_discount_enabled = 1, global_discount = 1.0, global_discount_enabled = 1, enable_log_content = 1, feature_attributes = '[]' WHERE mid = '310004'"
    );

    // ── 火山方舟多模态向量官方转发规则 ──
    once_migration!(
        pool,
        done,
        "volcengine_embeddings_forward_rules_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '火山方舟 多模态向量', 'volcengine',
               '将多模态向量请求转发到火山方舟官方多模态向量接口（/api/v3/embeddings/multimodal）',
               '{"target_type":"openai","path_rewrite":{"old":"/v1/embeddings","new":"/api/v3/embeddings/multimodal"},"path_passthrough":["/api/v3/embeddings/multimodal"],"auth_type":"bearer"}',
               '向量', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '火山方舟 多模态向量')"#
    );

    // ── 模型列表页面排序权重字段 ──
    once_migration!(
        pool,
        done,
        "models_sort_order_column_v1",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN models.sort_order IS '页面排序，数值越大越在列表中显示越靠前'"
    );

    // ── 创作中心 Seedance 2.0 方案剔除反向提示词 ──
    if !done.contains("strip_seedance2_negative_prompt_v1") {
        match crate::db::migrations::helpers::playground::strip_playground_seedance2_negative_prompt(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('strip_seedance2_negative_prompt_v1')",
                )
                .execute(pool)
                .await;
                done.insert("strip_seedance2_negative_prompt_v1".into());
                tracing::info!("创作中心 Seedance 2.0 方案移除反向提示词完成，更新 {} 条配置", n);
            }
            Err(e) => {
                tracing::warn!(
                    "strip_seedance2_negative_prompt_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // ── 用户等级发票收据配置与申请表 ──
    once_migration!(pool, done, "user_levels_invoices_feature_v1",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS invoice_enabled INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS invoice_mode TEXT NOT NULL DEFAULT 'international'",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS invoice_config TEXT NOT NULL DEFAULT '{}'",
        r#"CREATE TABLE IF NOT EXISTS user_invoice_requests (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            invoice_type TEXT NOT NULL DEFAULT 'normal',
            title_type TEXT NOT NULL DEFAULT 'company',
            title TEXT NOT NULL,
            tax_number TEXT,
            company_address TEXT,
            company_phone TEXT,
            bank_name TEXT,
            bank_account TEXT,
            amount DOUBLE PRECISION NOT NULL,
            email TEXT NOT NULL,
            remark TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            reject_reason TEXT,
            invoice_file_url TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#
    );

    // ── 用户实名 KYC：新增个人/企业详细信息字段 ──
    once_migration!(pool, done, "user_kyc_detailed_info_fields_v1",
        "ALTER TABLE user_kyc ADD COLUMN IF NOT EXISTS id_doc_number TEXT",
        "ALTER TABLE user_kyc ADD COLUMN IF NOT EXISTS personal_email TEXT",
        "ALTER TABLE user_kyc ADD COLUMN IF NOT EXISTS personal_phone TEXT",
        "ALTER TABLE user_kyc ADD COLUMN IF NOT EXISTS company_doc_type TEXT",
        "ALTER TABLE user_kyc ADD COLUMN IF NOT EXISTS company_doc_number TEXT",
        "ALTER TABLE user_kyc ADD COLUMN IF NOT EXISTS company_email TEXT",
        "ALTER TABLE user_kyc ADD COLUMN IF NOT EXISTS company_phone TEXT",
        "COMMENT ON COLUMN user_kyc.id_doc_number IS '个人证件号码'",
        "COMMENT ON COLUMN user_kyc.personal_email IS '个人联系邮箱'",
        "COMMENT ON COLUMN user_kyc.personal_phone IS '个人联系电话'",
        "COMMENT ON COLUMN user_kyc.company_doc_type IS '企业证件类型：unified_social_credit_code|business_license|organization_code|other'",
        "COMMENT ON COLUMN user_kyc.company_doc_number IS '企业证件号码'",
        "COMMENT ON COLUMN user_kyc.company_email IS '企业联系邮箱'",
        "COMMENT ON COLUMN user_kyc.company_phone IS '企业联系电话'"
    );

    // ── 用户实名 KYC：解除单用户唯一限制，支持同时拥有多条个人与企业认证 ──
    once_migration!(
        pool,
        done,
        "user_kyc_multiple_records_v1",
        "ALTER TABLE user_kyc DROP CONSTRAINT IF EXISTS user_kyc_user_id_key",
        "DROP INDEX IF EXISTS user_kyc_user_id_key",
        "CREATE INDEX IF NOT EXISTS idx_user_kyc_user_id ON user_kyc(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_user_kyc_user_type ON user_kyc(user_id, kyc_type)"
    );

    // ── 用户实名 KYC：新增 is_default 默认实名主体配置与索引 ──
    once_migration!(pool, done, "user_kyc_add_is_default_v1",
        "ALTER TABLE user_kyc ADD COLUMN IF NOT EXISTS is_default BOOLEAN NOT NULL DEFAULT FALSE",
        "UPDATE user_kyc SET is_default = TRUE WHERE id IN (SELECT MAX(id) FROM user_kyc GROUP BY user_id)",
        "CREATE INDEX IF NOT EXISTS idx_user_kyc_is_default ON user_kyc(user_id, is_default)",
        "COMMENT ON COLUMN user_kyc.is_default IS '是否为用户默认实名主体'"
    );

    // ── 用户实名 KYC：严格确保每个用户只能有且仅有 1 个默认主体（部分唯一索引） ──
    once_migration!(pool, done, "user_kyc_strictly_one_default_v1",
        "UPDATE user_kyc SET is_default = FALSE WHERE is_default = TRUE AND id NOT IN (SELECT MAX(id) FROM user_kyc WHERE is_default = TRUE GROUP BY user_id)",
        "UPDATE user_kyc SET is_default = TRUE WHERE id IN (SELECT MAX(id) FROM user_kyc WHERE user_id NOT IN (SELECT user_id FROM user_kyc WHERE is_default = TRUE) GROUP BY user_id)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_user_kyc_one_default ON user_kyc(user_id) WHERE is_default = TRUE"
    );

    // ── 发票申请审核与开票信息扩展字段 ──
    once_migration!(
        pool,
        done,
        "user_invoice_requests_audit_fields_v1",
        "ALTER TABLE user_invoice_requests ADD COLUMN IF NOT EXISTS invoice_code TEXT",
        "ALTER TABLE user_invoice_requests ADD COLUMN IF NOT EXISTS invoice_number TEXT",
        "ALTER TABLE user_invoice_requests ADD COLUMN IF NOT EXISTS admin_remark TEXT",
        "ALTER TABLE user_invoice_requests ADD COLUMN IF NOT EXISTS issued_at TIMESTAMPTZ",
        "ALTER TABLE user_invoice_requests ADD COLUMN IF NOT EXISTS operator_id TEXT",
        "ALTER TABLE user_invoice_requests ADD COLUMN IF NOT EXISTS operator_name TEXT",
        "COMMENT ON COLUMN user_invoice_requests.invoice_code IS '发票代码'",
        "COMMENT ON COLUMN user_invoice_requests.invoice_number IS '发票号码'",
        "COMMENT ON COLUMN user_invoice_requests.admin_remark IS '财务审核或开具备注'",
        "COMMENT ON COLUMN user_invoice_requests.issued_at IS '实际开票时间'",
        "COMMENT ON COLUMN user_invoice_requests.operator_id IS '操作管理员ID'",
        "COMMENT ON COLUMN user_invoice_requests.operator_name IS '操作管理员用户名/姓名'"
    );

    // ── 发票申请单号字段 (fp + YYMMDD + 4位以上随机数) ──
    once_migration!(pool, done, "user_invoice_requests_request_no_v1",
        "ALTER TABLE user_invoice_requests ADD COLUMN IF NOT EXISTS request_no TEXT",
        "UPDATE user_invoice_requests SET request_no = 'fp' || to_char(created_at AT TIME ZONE 'Asia/Shanghai', 'YYMMDD') || LPAD(id::text, 4, '0') WHERE request_no IS NULL",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_user_invoice_requests_request_no ON user_invoice_requests(request_no)",
        "COMMENT ON COLUMN user_invoice_requests.request_no IS '发票申请单号 (fp+YYMMDD+随机数)'"
    );

    // ── recharge_records 新增 order_no 独立字段与索引，并回填历史单号 ──
    once_migration!(pool, done, "recharge_records_order_no_field_v1",
        "ALTER TABLE recharge_records ADD COLUMN IF NOT EXISTS order_no TEXT DEFAULT ''",
        "COMMENT ON COLUMN recharge_records.order_no IS '关联订单号/支付流水号，方便统计分析与对账'",
        "CREATE INDEX IF NOT EXISTS idx_recharge_records_order_no ON recharge_records (order_no) WHERE order_no != ''",
        r#"UPDATE recharge_records 
           SET order_no = (regexp_match(remark, '(?:订单号|Order\s*(?:No|Number|Id)|单号)[\s:：]+([a-zA-Z0-9_-]+)'))[1]
           WHERE (order_no IS NULL OR order_no = '') 
             AND remark ~ '(?:订单号|Order\s*(?:No|Number|Id)|单号)[\s:：]+[a-zA-Z0-9_-]+'"#,
        r#"UPDATE recharge_records 
           SET order_no = remark 
           WHERE (order_no IS NULL OR order_no = '') 
             AND remark ~ '^T\d{14}[A-Za-z0-9_-]+$'"#
    );

    // ── 上游渠道配置：NewAPI 等上游货币与本站货币单位折算汇率 ──
    once_migration!(pool, done, "channel_configs_upstream_currency_rate_v1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS upstream_currency TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS site_currency TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS upstream_currency_rate DOUBLE PRECISION NOT NULL DEFAULT 1.0",
        "COMMENT ON COLUMN channel_configs.upstream_currency IS '上游计价货币单位，如 CNY/USD，空=同本站'",
        "COMMENT ON COLUMN channel_configs.site_currency IS '本站计价货币单位，如 USD/CNY，空=同系统'",
        "COMMENT ON COLUMN channel_configs.upstream_currency_rate IS '货币换算汇率(例如 1 USD = 7.2 CNY 时填 7.2)'"
    );

    // ── 高并发与慢查询优化索引：超大容量表专用异步后台迁移（专属会话内存调优，杜绝启动停机与 I/O 跑满） ──
    once_heavy_migration!(pool, done, "perf_concurrency_opt_v1",
        // 1. 用户按类型查询、分页与 COUNT 聚合（全量覆盖，覆盖全历史）
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_user_action_created ON logs (user_id, action_type, created_at DESC)",
        // 2. 用户视觉模型日志分页与总数统计（带 user_id 部分索引）
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_user_vision_created ON logs (user_id, created_at DESC) WHERE (action_type = ANY (ARRAY['图片'::text, '视频'::text, '视频增强'::text, '视觉模型'::text, '视觉'::text]))",
        // 3. 孤儿任务扫描优化
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_orphan_scan ON logs (created_at) WHERE is_completed = 0 AND status_code = 0",
        // 4. 高可用插件日志关联索引
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ha_usage_logs_log_id ON ha_usage_logs (log_id)",
        // 5. 精简确凿冗余索引（已被复合索引最左前缀覆盖，减少写入放大）
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_user_id",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_user_created",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_is_completed_pending",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_created_at_new",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_user_created_at_new",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_action_created_stats_new",
        // 6. 刷新执行计划统计信息
        "ANALYZE logs",
        "ANALYZE api_tokens",
        "ANALYZE ha_usage_logs",
    );

    // ── 任务在途轮询快照优化索引：forward_rules.eid 专有部分索引（0.1ms 点查，保障热切换轮询零抖动） ──
    once_migration!(pool, done, "task_poll_forward_rules_eid_v1",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_forward_rules_eid ON forward_rules (eid) WHERE eid != ''",
        "ANALYZE forward_rules"
    );
    Ok(())
}
