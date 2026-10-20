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
    // Users table
    // ── 初始化核心基础表（受一次性迁移保护） ──
    once_migration!(
        pool,
        done,
        "init_core_tables_v1",
        r#"CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL UNIQUE,
            username VARCHAR(48) NOT NULL UNIQUE,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            nickname VARCHAR(24),
            mobile TEXT,
            wechat_id TEXT,
            role TEXT NOT NULL DEFAULT 'user' CHECK(role IN ('admin', 'user')),
            balance DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            user_group TEXT NOT NULL DEFAULT 'default',
            used_quota DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            referred_by TEXT, commission_balance DOUBLE PRECISION NOT NULL DEFAULT 0.0, admin_group_id INTEGER,
            register_ip TEXT DEFAULT '',
            admin_remark TEXT DEFAULT '',
            referral_history TEXT DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS recharge_records (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id),
            amount DOUBLE PRECISION NOT NULL,
            recharge_type TEXT NOT NULL DEFAULT 'other',
            remark TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS channels (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            provider_type TEXT NOT NULL,
            base_url TEXT NOT NULL,
            api_key TEXT NOT NULL,
            models TEXT NOT NULL DEFAULT '[]',
            model_mapping TEXT NOT NULL DEFAULT '{}',
            priority INTEGER NOT NULL DEFAULT 0,
            weight INTEGER NOT NULL DEFAULT 1,
            status INTEGER NOT NULL DEFAULT 1,
            balance DOUBLE PRECISION,
            max_rps INTEGER DEFAULT 0,
            quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1,
            quota_used DOUBLE PRECISION NOT NULL DEFAULT 0,
            config TEXT NOT NULL DEFAULT '{}',
            user_groups TEXT NOT NULL DEFAULT '[]',
            group_aid TEXT DEFAULT '',
            preset_id INTEGER,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS api_tokens (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id),
            token_key TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL DEFAULT 'default' CHECK (char_length(name) <= 36 AND name ~ '^([^\W_]| )+$'),
            quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1,
            quota_used DOUBLE PRECISION NOT NULL DEFAULT 0,
            allowed_models TEXT NOT NULL DEFAULT '[]',
            allowed_ips TEXT NOT NULL DEFAULT '',
            ip_whitelist TEXT,
            rps_limit INTEGER DEFAULT 0,
            rpm_limit INTEGER DEFAULT 0,
            expires_at TEXT,
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            daily_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1.0,
            daily_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            weekly_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1.0,
            weekly_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            monthly_quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1.0,
            monthly_quota_used DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            last_reset_day TEXT NOT NULL DEFAULT '',
            last_reset_week TEXT NOT NULL DEFAULT '',
            last_reset_month TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS logs (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            channel_id INTEGER,
            token_id INTEGER,
            model TEXT NOT NULL DEFAULT '',
            prompt_tokens INTEGER NOT NULL DEFAULT 0,
            completion_tokens INTEGER NOT NULL DEFAULT 0,
            cached_tokens INTEGER NOT NULL DEFAULT 0,
            cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            latency_ms INTEGER NOT NULL DEFAULT 0,
            status_code INTEGER NOT NULL DEFAULT 200,
            endpoint TEXT NOT NULL DEFAULT '',
            error_message TEXT,
            upstream_url TEXT DEFAULT '',
            request_content TEXT,
            response_content TEXT,
            upstream_req_content TEXT,
            is_stream INTEGER NOT NULL DEFAULT 0,
            billing_detail TEXT DEFAULT '',
            billing_pid TEXT DEFAULT '',
            forward_eid TEXT DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS redemptions (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            code TEXT NOT NULL UNIQUE,
            quota DOUBLE PRECISION NOT NULL,
            is_used INTEGER DEFAULT 0,
            used_at TEXT,
            used_by TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL DEFAULT ''
        )"#,
        r#"CREATE TABLE IF NOT EXISTS orders (
            id SERIAL PRIMARY KEY,
            out_trade_no TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL REFERENCES users(id),
            payment_method TEXT NOT NULL,
            amount DOUBLE PRECISION NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            trade_no TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            paid_at TEXT
        )"#,
        "DROP TABLE IF EXISTS task_logs",
        r#"CREATE TABLE IF NOT EXISTS plugin_api_logs (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            plugin_name TEXT NOT NULL,
            api_endpoint TEXT NOT NULL,
            request_payload TEXT,
            response_payload TEXT,
            status_code INTEGER,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS user_levels (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            group_key TEXT NOT NULL UNIQUE,
            discount DOUBLE PRECISION NOT NULL DEFAULT 1.0,
            commission_ratio DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            invite_reward_inviter DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            invite_reward_invitee DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            daily_invite_limit INTEGER NOT NULL DEFAULT 10,
            marketing_enabled INTEGER NOT NULL DEFAULT 0,
            max_token_count INTEGER NOT NULL DEFAULT 10,
            sort_order INTEGER NOT NULL DEFAULT 0,
            description TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS verification_codes (
            id SERIAL PRIMARY KEY,
            email TEXT NOT NULL,
            code TEXT NOT NULL,
            purpose TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#
    );

    // Model Providers table
    // ── 初始化服务商与模型管理表（受一次性迁移保护） ──
    once_migration!(
        pool,
        done,
        "init_provider_tables_v1",
        r#"CREATE TABLE IF NOT EXISTS model_providers (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            upstream_type TEXT NOT NULL DEFAULT 'other',
            config TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS model_types (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            upstream_type TEXT NOT NULL DEFAULT 'other',
            config TEXT,
            default_features TEXT DEFAULT '[]',
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS models (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            model_id TEXT NOT NULL,
            original_id TEXT NOT NULL DEFAULT '',
            provider_id INTEGER REFERENCES model_providers(id),
            type_id INTEGER REFERENCES model_types(id),
            group_ratios TEXT NOT NULL DEFAULT '{}',
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            description TEXT,
            feature_attributes TEXT DEFAULT '[]',
            upstream_type TEXT NOT NULL DEFAULT 'other',
            config TEXT,
            enable_log_content INTEGER NOT NULL DEFAULT 0,
            forward_rule_ids TEXT,
            billing_rule_id INTEGER,
            pre_deduction DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#
    );

    // Seed default user level
    once_migration!(
        pool,
        done,
        "seed_default_user_level_v1",
        r#"INSERT INTO user_levels (name, group_key, discount, description)
           VALUES ('默认用户', 'default', 1.0, '普通用户，无折扣')
           ON CONFLICT (group_key) DO NOTHING"#
    );

    // Admin Groups table
    // ── 初始化管理组、佣金表及核心字段扩展（受一次性迁移保护） ──
    once_migration!(pool, done, "backfill_channels_user_groups_v1",
        r#"CREATE TABLE IF NOT EXISTS admin_groups (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            permissions TEXT,
            description TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS admin_group_id INTEGER",
        "ALTER TABLE admin_groups ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS commission_ratio DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS invite_reward_inviter DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS invite_reward_invitee DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS daily_invite_limit INTEGER NOT NULL DEFAULT 10",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS marketing_enabled INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS is_default INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS max_token_count INTEGER NOT NULL DEFAULT 10",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        r#"CREATE TABLE IF NOT EXISTS commissions (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id),
            from_user_id TEXT NOT NULL REFERENCES users(id),
            recharge_id INTEGER REFERENCES recharge_records(id),
            amount DOUBLE PRECISION NOT NULL,
            ratio DOUBLE PRECISION NOT NULL,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS remark TEXT",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS description TEXT",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS feature_attributes TEXT DEFAULT '[]'",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS enable_log_content INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS is_stream INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS request_content TEXT",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS response_content TEXT",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS upstream_url TEXT DEFAULT ''",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS upstream_req_content TEXT DEFAULT ''",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS billing_detail TEXT DEFAULT ''",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS billing_pid TEXT DEFAULT ''",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS forward_eid TEXT DEFAULT ''",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS user_groups TEXT NOT NULL DEFAULT '[]'",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS quota_limit DOUBLE PRECISION NOT NULL DEFAULT -1",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS quota_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS group_aid TEXT DEFAULT ''",
        "CREATE INDEX IF NOT EXISTS idx_logs_created_at ON logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_logs_user_id ON logs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_logs_user_created ON logs(user_id, created_at)"
    );

    // ── 初始化转发规则与计费规则系统结构（受一次性迁移保护） ──
    once_migration!(pool, done, "init_routing_billing_tables_v1",
        r#"CREATE TABLE IF NOT EXISTS forward_rules (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            rule_type TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT '聊天',
            config_json TEXT NOT NULL DEFAULT '{}',
            description TEXT,
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            upstream_type TEXT NOT NULL DEFAULT 'other',
            config TEXT,
            eid TEXT DEFAULT '',
            is_system INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "ALTER TABLE model_types ADD COLUMN IF NOT EXISTS default_features TEXT DEFAULT '[]'",
        "INSERT INTO model_types (name, sort_order, is_active, remark) SELECT '向量', 50, 1, '文本向量（Embedding）模型' WHERE NOT EXISTS (SELECT 1 FROM model_types WHERE name = '向量')",
        "INSERT INTO model_types (name, sort_order, is_active, remark) SELECT '排序', 60, 1, '文本排序（Rerank）模型' WHERE NOT EXISTS (SELECT 1 FROM model_types WHERE name = '排序')",
        "INSERT INTO model_types (name, sort_order, is_active, remark) SELECT '视频增强', 35, 1, '视频画质增强与字幕擦除处理模型' WHERE NOT EXISTS (SELECT 1 FROM model_types WHERE name = '视频增强')",
        "ALTER TABLE forward_rules ADD COLUMN IF NOT EXISTS category TEXT NOT NULL DEFAULT '聊天'",
        "ALTER TABLE forward_rules ADD COLUMN IF NOT EXISTS is_system INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE forward_rules ADD COLUMN IF NOT EXISTS eid TEXT DEFAULT ''",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS forward_rule_ids TEXT",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system)
        SELECT t.name, t.rule_type, t.description, t.config_json, t.category, t.is_system
        FROM (VALUES
            ('OpenAI 兼容原生通道 (聊天)', 'openai', '标准的按路径聊天透传规则，支持/v1/chat/completions、/v1/responses与/v1/messages', '{"path_rewrites":[{"old":"/v1/chat/completions","new":"/v1/chat/completions"},{"old":"/v1/responses","new":"/v1/responses"},{"old":"/v1/messages","new":"/v1/messages"}]}', '聊天', 1),
            ('OpenAI 兼容原生通道 (图片)', 'openai', '供图片生成调用的原生通道', '{"path_rewrite":{"old":"/v1/images/generations","new":"/v1/images/generations"}}', '图片', 1),
            ('OpenAI 兼容原生通道 (视频)', 'openai', '供视频生成调用的原生通道', '{"path_rewrite":{"old":"/v1/video/generations","new":"/v1/video/generations"}}', '视频', 1),
            ('Anthropic 原生转化', 'anthropic', '将 OpenAI 格式请求转换为 Anthropic Messages API 格式，接口 /v1/messages', '{"target_type":"anthropic","path_rewrite":{"old":"/v1/chat/completions","new":"/v1/messages"},"auth_type":"x-api-key"}', '聊天', 1),
            ('Google Gemini 原生生图', 'gemini', '将标准的生图请求适配到 Gemini contents 接口', '{"target_type":"gemini_image","path_rewrite":{"old":"/v1/images/generations","new":"/v1beta/models/${model}:generateContent"},"auth_type":"query_key"}', '图片', 1),
            ('Google Gemini 格式转换 (聊天)', 'gemini', '将标准请求转换并适配到 Gemini contents', '{"target_type":"gemini","path_rewrite":{"old":"/v1/chat/completions","new":"/v1beta/models/${model}:generateContent"},"auth_type":"query_key"}', '聊天', 1),
            ('火山方舟 视频生成', 'volcengine', '将标准的视频生成请求适配到火山方舟 tasks 接口', '{"target_type":"volcengine","path_rewrite":{"old":"/v1/video/generations","new":"/api/v3/contents/generations/tasks"},"auth_type":"bearer"}', '视频', 1),
            ('火山方舟 聊天', 'volcengine', '将标准聊天、Responses及Messages请求转发到火山方舟官方接口，支持/api/v3/chat/completions、/api/v3/responses与/api/compatible/v1/messages', '{"target_type":"volcengine_chat","path_rewrites":[{"old":"/v1/chat/completions","new":"/api/v3/chat/completions"},{"old":"/v1/responses","new":"/api/v3/responses"},{"old":"/v1/messages","new":"/api/compatible/v1/messages"}],"auth_type":"bearer"}', '聊天', 1),
            ('火山方舟 图片生成', 'volcengine', '将标准的图片生成请求转发到火山方舟官方 images 接口，body 保持 OpenAI 兼容格式', '{"target_type":"volcengine_image","path_rewrite":{"old":"/v1/images/generations","new":"/api/v3/images/generations"},"auth_type":"bearer"}', '图片', 1),
            ('火山方舟 视频素材转换', 'volcengine', '在火山方舟视频生成基础上，自动将 content 中的网络 URL 通过 CreateAsset API 转换为素材 ID（asset://前缀），需配置素材资产管理插件的审核凭证', '{"target_type":"volcengine","asset_convert":true,"path_rewrite":{"old":"/v1/video/generations","new":"/api/v3/contents/generations/tasks"},"auth_type":"bearer"}', '视频', 1),
            ('火山方舟 视频素材转换(国际版)', 'volcengine', '在火山方舟视频生成基础上，自动将 content 中的网络 URL 通过 CreateAsset API 转换为素材 ID（asset://前缀），需配置国际版素材资产管理插件的审核凭证', '{"target_type":"volcengine","asset_convert":true,"asset_convert_ns":"asset_manager_intl","path_rewrite":{"old":"/v1/video/generations","new":"/api/v3/contents/generations/tasks"},"auth_type":"bearer"}', '视频', 1),
            ('火山方舟 视频素材免审核转换(国际版)', 'volcengine', '在火山方舟视频生成基础上，自动将 content 中的网络 URL 通过 CreateAsset API 转换为素材 ID（asset://前缀），且向火山方舟申请免审核，需配置国际版素材资产管理插件的审核凭证', '{"target_type":"volcengine","asset_convert":true,"asset_convert_ns":"asset_manager_intl","moderation":true,"path_rewrite":{"old":"/v1/video/generations","new":"/api/v3/contents/generations/tasks"},"auth_type":"bearer"}', '视频', 1),
            ('阿里百炼 DashScope 视频生成', 'aliyun', '将标准视频生成请求（/v1/video/generations）转换为阿里百炼 DashScope 格式，支持文生视频/图生视频/参考生视频/视频编辑，异步任务自动注入 X-DashScope-Async Header', '{"target_type":"dashscope","path_rewrite":{"old":"/v1/video/generations","new":"/api/v1/services/aigc/video-generation/video-synthesis"},"auth_type":"bearer","poll_path":"/api/v1/tasks/${task_id}"}', '视频', 1),
            ('阿里百炼 DashScope 图片生成', 'aliyun', '将标准图片生成请求（/v1/images/generations）转换为阿里百炼 DashScope 格式', '{"target_type":"dashscope_image","path_rewrite":{"old":"/v1/images/generations","new":"/api/v1/services/aigc/multimodal-generation/generation"},"auth_type":"bearer"}', '图片', 1),
            ('阿里百炼 DashScope 聊天', 'aliyun', '阿里百炼官方聊天转发规则，支持 /v1/chat/completions、/v1/responses 与 /v1/messages（/apps/anthropic/v1/messages）协议格式，上游统一 Bearer 鉴权', '{"path_rewrites":[{"old":"/v1/chat/completions","new":"/compatible-mode/v1/chat/completions"},{"old":"/v1/responses","new":"/compatible-mode/v1/responses"},{"old":"/v1/messages","new":"/apps/anthropic/v1/messages"}],"auth_type":"bearer"}', '聊天', 1),
            ('可灵 视频生成 (文/图/多图)', 'kling', '将标准视频生成请求转发到可灵官方 API，系统根据请求体自动分发到 text2video/image2video/multi-image2video', '{"target_type":"kling","path_rewrite":{"old":"/v1/video/generations","new":"/v1/videos/text2video"},"auth_type":"bearer"}', '视频', 1),
            ('可灵 Omni 视频 (kling-v3-omni/video-o1)', 'kling', '将视频生成请求转发到可灵 Omni 视频端点', '{"target_type":"kling","path_rewrite":{"old":"/v1/video/generations","new":"/v1/videos/omni-video"},"auth_type":"bearer"}', '视频', 1),
            ('可灵 图片生成', 'kling', '将标准图片生成请求转发到可灵官方 API，含多图参考自动分发', '{"target_type":"kling","path_rewrite":{"old":"/v1/images/generations","new":"/v1/images/generations"},"auth_type":"bearer"}', '图片', 1),
            ('可灵 Omni 图片 (kling-v3-omni/image-o1)', 'kling', '将图片生成请求转发到可灵 Omni 图片端点', '{"target_type":"kling","path_rewrite":{"old":"/v1/images/generations","new":"/v1/images/omni-image"},"auth_type":"bearer"}', '图片', 1),
            ('腾讯云 VOD AIGC 生图 (同步轮询)', 'tencent_vod', '同步版：无 poll_path，OpenAI 兼容请求将自动同步轮询至终态后返回结果。密钥格式：SecretId:SecretKey:SubAppId，模型格式：ModelName@ModelVersion', '{"target_type":"tencent_vod_image","path_rewrite":{"old":"/v1/images/generations","new":"/"},"auth_type":"tencent_vod"}', '图片', 1),
            ('腾讯云 VOD AIGC 生视频', 'tencent_vod', '将标准视频生成请求转换为腾讯云点播 AIGC CreateAigcVideoTask 接口。密钥格式：SecretId:SecretKey:SubAppId，模型格式：ModelName@ModelVersion', '{"target_type":"tencent_vod_video","path_rewrite":{"old":"/v1/video/generations","new":"/"},"auth_type":"tencent_vod"}', '视频', 1),
            ('即梦AI 图片生成', 'jimeng', '将标准图片生成请求转换为即梦AI（火山引擎 CV 视觉服务）格式。密钥格式：AccessKeyID:SecretAccessKey，模型映射为 req_key（如 high_aes_general_v30l_tta）', '{"target_type":"jimeng_image","path_rewrite":{"old":"/v1/images/generations","new":"/"},"auth_type":"jimeng"}', '图片', 1),
            ('即梦AI 视频生成', 'jimeng', '将标准视频生成请求转换为即梦AI（火山引擎 CV 视觉服务）格式。密钥格式：AccessKeyID:SecretAccessKey，模型映射为 req_key（如 dreamina_ic_generate_video_v2）', '{"target_type":"jimeng_video","path_rewrite":{"old":"/v1/video/generations","new":"/"},"auth_type":"jimeng"}', '视频', 1),
            ('GPT 官方图片生成', 'gpt', '将图片生成请求转发到 GPT 官方 API，自动根据请求体内容分发到 generations（文生图）或 edits（图生图/多图生图）端点', '{"target_type":"gpt","path_rewrite":{"old":"/v1/images/generations","new":"/v1/images/generations"},"auth_type":"bearer"}', '图片', 1),
            ('火山方舟 语音合成 (TTS V3)', 'volcengine', '将 OpenAI 格式语音合成请求（/v1/audio/speech）转换为火山方舟 TTS V3 SSE 格式。渠道地址: openspeech.bytedance.com，密钥为 X-Api-Key，模型ID通过 X-Api-Resource-Id 传递', '{"target_type":"volcengine_tts","path_rewrite":{"old":"/v1/audio/speech","new":"/api/v3/tts/unidirectional/sse"},"auth_type":"volcengine_tts"}', '音频', 1),
            ('火山方舟 语音合成 (TTS V3 Chunked)', 'volcengine', '将 OpenAI 格式语音合成请求（/v1/audio/speech）转换为火山方舟 TTS V3 HTTP Chunked 格式与 SSE 版本请求体和鉴权相同，仅传输协议不同（更轻量）', '{"target_type":"volcengine_tts","path_rewrite":{"old":"/v1/audio/speech","new":"/api/v3/tts/unidirectional"},"auth_type":"volcengine_tts"}', '音频', 1),
            ('OpenAI 兼容原生通道 (语音)', 'openai', '标准的语音合成透传规则，直接转发到 /v1/audio/speech', '{"path_rewrite":{"old":"/v1/audio/speech","new":"/v1/audio/speech"}}', '音频', 1),
            ('阿里百炼 DashScope 文本向量 (OpenAI兼容)', 'aliyun', '将文本向量请求转发到阿里百炼兼容接口', '{"target_type":"openai","path_rewrite":{"old":"/v1/embeddings","new":"/compatible-mode/v1/embeddings"},"auth_type":"bearer"}', '向量', 1),
            ('阿里百炼 DashScope 排序 (兼容模式)', 'aliyun', '将排序请求转发到阿里百炼兼容接口，适用于 qwen3-rerank 等模型', '{"target_type":"openai","path_rewrite":{"old":"/v1/rerank","new":"/compatible-api/v1/reranks"},"auth_type":"bearer"}', '排序', 1),
            ('阿里百炼 DashScope 排序 (原生)', 'aliyun', '将排序请求转发到阿里百炼原生 DashScope 接口，适用于 gte-rerank-v2 等模型', '{"target_type":"openai","path_rewrite":{"old":"/v1/rerank","new":"/api/v1/services/rerank/text-rerank/text-rerank"},"auth_type":"bearer"}', '排序', 1),
            ('Bytefor 视频生成', 'bytefor', '将标准的视频生成请求适配到 Bytefor 视频生成 API', '{"target_type":"bytefor_video","path_rewrite":{"old":"/v1/video/generations","new":"/api/v1/generate"},"poll_path":"/api/v1/task/${task_id}","auth_type":"bearer"}', '视频', 1),
            ('火山方舟 级联视频生成', 'volcengine', '供视频生成级联画质增强调用的火山方舟专属转发规则', '{"target_type":"volcengine","is_cascade":true,"res_mul":{"480p":1.5,"720p":2.15,"1080p":2.25,"2k":2.5,"4k":4.0},"path_rewrite":{"old":"/v1/video/generations","new":"/api/v3/contents/generations/tasks"},"auth_type":"bearer"}', '视频', 1),
            ('ATP Token 视频生成', 'atp', '将标准视频生成请求（/v1/video/generations）或阿里百炼格式参数转换为 ATP Token 媒体视频 API 格式（omni tasks），支持 Seedance / Kling / Wan / HappyHorse 系列模型，自动轮询及参数兼容', '{"target_type":"atp_video","path_rewrite":{"old":"/v1/video/generations","new":"/omni/media/v1/contents/generations/tasks"},"auth_type":"bearer","poll_path":"/omni/media/v1/contents/generations/tasks/${task_id}"}', '视频', 1)
        ) AS t(name, rule_type, description, config_json, category, is_system)
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = t.name)
        "#,
        r#"CREATE TABLE IF NOT EXISTS billing_rules (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            billing_type TEXT NOT NULL,
            prompt_rate DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            completion_rate DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            cached_rate DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            fixed_rate DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            duration_rate DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            billing_rule TEXT NOT NULL DEFAULT 'standard',
            pricing_tiers TEXT NOT NULL DEFAULT '[]',
            extended_config TEXT NOT NULL DEFAULT '{}',
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            upstream_type TEXT NOT NULL DEFAULT 'other',
            config TEXT,
            pid TEXT DEFAULT '',
            provider_id BIGINT REFERENCES model_providers(id),
            type_id BIGINT REFERENCES model_types(id),
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS billing_rule_id INTEGER",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS pre_deduction DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE models DROP COLUMN IF EXISTS billing_type",
        "ALTER TABLE models DROP COLUMN IF EXISTS prompt_rate",
        "ALTER TABLE models DROP COLUMN IF EXISTS completion_rate",
        "ALTER TABLE models DROP COLUMN IF EXISTS fixed_rate",
        "ALTER TABLE models DROP COLUMN IF EXISTS duration_rate",
        "ALTER TABLE models DROP COLUMN IF EXISTS billing_rule",
        "ALTER TABLE models DROP COLUMN IF EXISTS billing_unit",
        "ALTER TABLE models DROP COLUMN IF EXISTS pricing_tiers",
        "ALTER TABLE models DROP COLUMN IF EXISTS config",
        "ALTER TABLE models DROP COLUMN IF EXISTS upstream_type",
        "ALTER TABLE forward_rules DROP COLUMN IF EXISTS remark",
        "ALTER TABLE forward_rules DROP COLUMN IF EXISTS upstream_type",
        "ALTER TABLE forward_rules DROP COLUMN IF EXISTS config",
        "ALTER TABLE model_providers DROP COLUMN IF EXISTS upstream_type",
        "ALTER TABLE model_providers DROP COLUMN IF EXISTS config",
        "ALTER TABLE model_types DROP COLUMN IF EXISTS upstream_type",
        "ALTER TABLE model_types DROP COLUMN IF EXISTS config",
        "ALTER TABLE billing_rules DROP COLUMN IF EXISTS upstream_type",
        "ALTER TABLE billing_rules DROP COLUMN IF EXISTS config",
        "ALTER TABLE billing_rules DROP COLUMN IF EXISTS remark",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS extended_config TEXT NOT NULL DEFAULT '{}'",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS pid TEXT DEFAULT ''",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS provider_id BIGINT REFERENCES model_providers(id)",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS type_id BIGINT REFERENCES model_types(id)",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS is_system INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS cached_rate DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS claude_cache_creation_rate DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS claude_cache_read_rate DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE billing_rules ADD COLUMN IF NOT EXISTS pricing_type TEXT NOT NULL DEFAULT 'custom'",
        "UPDATE billing_rules SET name = '文本向量标准计费' WHERE name = '文本向量标准计费 (0.7/1M)'",
        "UPDATE billing_rules SET name = '排序模型多模态计费', billing_rule = 'multimodal', extended_config = '{\"image_prompt_rate\": 0.35}' WHERE name IN ('排序模型标准计费 (0.35/1M)', '排序模型标准计费')",
        r#"INSERT INTO billing_rules (name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, extended_config, is_system)
        SELECT t.name, t.billing_type, t.prompt_rate, t.completion_rate, t.fixed_rate, t.duration_rate, t.billing_rule, t.extended_config, t.is_system
        FROM (VALUES 
            ('标准1M万字计费 (1)', 'tokens', 1.0, 2.0, 0.0, 0.0, 'standard', '{}', 1),
            ('单次请求扣费 (0.1)', 'requests', 0.0, 0.0, 0.1, 0.0, 'standard', '{}', 1),
            ('Seedance2.0官方计费', 'tokens', 0.0, 0.0, 0.0, 0.0, 'seedance2.0', '{"resolution_rates":{"1080p":{"with_video":31,"without_video":51},"480p":{"with_video":28,"without_video":46},"4k":{"with_video":16,"without_video":26},"720p":{"with_video":28,"without_video":46}}}', 1),
            ('Seedance2.0Fast官方计费', 'tokens', 0.0, 0.0, 0.0, 0.0, 'seedance2.0', '{"resolution_rates":{"480p":{"with_video":22,"without_video":37},"720p":{"with_video":22,"without_video":37}}}', 1),
            ('Seedance2.0mini官方计费', 'tokens', 0.0, 0.0, 0.0, 0.0, 'seedance2.0', '{"resolution_rates":{"480p":{"with_video":14,"without_video":23},"720p":{"with_video":14,"without_video":23}}}', 1),
            ('Seedance2.5官方计费', 'tokens', 0.0, 0.0, 0.0, 0.0, 'seedance2.0', '{"enable_time_multipliers":false,"resolution_rates":{"480p":{"with_video":42,"without_video":70},"720p":{"with_video":42,"without_video":70}},"time_multipliers":[]}', 1),
            ('语音合成按字符计费 (2.8元/万字符)', 'requests', 0.0, 0.0, 2.8, 0.0, 'characters', '{}', 1),
            ('文本向量标准计费', 'tokens', 0.7, 0.0, 0.0, 0.0, 'standard', '{}', 1),
            ('排序模型多模态计费', 'tokens', 0.35, 0.0, 0.0, 0.0, 'multimodal', '{"image_prompt_rate": 0.35}', 1),
            ('火山级联画质增强默认计费', 'duration', 0.0, 0.0, 0.0, 0.0, 'volc_enhance_cascade', '{"price_table": {"fast|720p|no": 0.80, "fast|1080p|no": 1.80, "fast|2k|no": 3.20, "fast|4k|no": 7.20, "standard|720p|no": 1.00, "standard|1080p|no": 2.24, "standard|2k|no": 4.00, "standard|4k|no": 8.94, "pro|720p|no": 1.20, "pro|1080p|no": 2.70, "pro|2k|no": 4.80, "pro|4k|no": 10.70, "ai|720p|no": 1.40, "ai|1080p|no": 3.16, "fast|720p|yes": 0.84, "fast|1080p|yes": 1.88, "fast|2k|yes": 3.40, "fast|4k|yes": 7.40, "standard|720p|yes": 1.06, "standard|1080p|yes": 2.36, "standard|2k|yes": 4.30, "standard|4k|yes": 9.24, "pro|720p|yes": 1.28, "pro|1080p|yes": 2.86, "pro|2k|yes": 5.20, "pro|4k|yes": 11.10, "ai|720p|yes": 1.60, "ai|1080p|yes": 3.51}}', 1)
        ) AS t(name, billing_type, prompt_rate, completion_rate, fixed_rate, duration_rate, billing_rule, extended_config, is_system)
        WHERE NOT EXISTS (SELECT 1 FROM billing_rules WHERE name = t.name)
        "#,
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS register_ip TEXT DEFAULT ''",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS admin_remark TEXT DEFAULT ''",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS timezone TEXT DEFAULT 'Asia/Shanghai'",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS task_id TEXT DEFAULT ''",
        "COMMENT ON COLUMN logs.task_id IS '异步任务ID，非空时表示异步任务，用于轮询状态跟踪'",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS post_response TEXT DEFAULT ''",
        "COMMENT ON COLUMN logs.post_response IS '异步任务POST阶段提交响应结果'",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS action_type TEXT DEFAULT ''",
        "COMMENT ON COLUMN logs.action_type IS '任务类型：聊天、图片、视频等，用于精准筛选和显示'",
        "CREATE INDEX IF NOT EXISTS idx_logs_action_type_created ON logs (action_type, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_logs_task_id ON logs (task_id)",
        "CREATE INDEX IF NOT EXISTS idx_logs_token_created ON logs (token_id, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_logs_channel_created ON logs (channel_id, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_logs_model_created ON logs (model, created_at DESC)",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS allow_view_log_details INTEGER NOT NULL DEFAULT 1",
        "COMMENT ON COLUMN user_levels.allow_view_log_details IS '是否允许查看日志详情，1-允许，0-不允许'",
        r#"CREATE TABLE IF NOT EXISTS channel_configs (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            provider_type TEXT NOT NULL,
            base_url TEXT NOT NULL,
            api_key TEXT NOT NULL,
            remark TEXT,
            yid TEXT DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS preset_id INTEGER",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS remark TEXT",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS yid TEXT DEFAULT ''",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE model_providers ADD COLUMN IF NOT EXISTS remark TEXT",
        "ALTER TABLE model_types ADD COLUMN IF NOT EXISTS remark TEXT",
        r#"CREATE TABLE IF NOT EXISTS upstreams (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            upstream_type TEXT NOT NULL DEFAULT 'other',
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            config TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "ALTER TABLE upstreams ADD COLUMN IF NOT EXISTS upstream_type TEXT NOT NULL DEFAULT 'other'",
        "ALTER TABLE upstreams ADD COLUMN IF NOT EXISTS config TEXT"
    );

    // Plugins table
    // ── 初始化插件管理系统表及种子数据（受一次性迁移保护） ──
    once_migration!(pool, done, "init_plugin_tables_v1",
        r#"CREATE TABLE IF NOT EXISTS plugins (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            description TEXT,
            is_enabled INTEGER NOT NULL DEFAULT 0,
            allowed_levels TEXT NOT NULL DEFAULT 'all',
            created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            updated_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
        )"#,
        "ALTER TABLE plugins ADD COLUMN IF NOT EXISTS allowed_levels TEXT NOT NULL DEFAULT 'all'",
        r#"CREATE TABLE IF NOT EXISTS plugin_asset_groups (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id),
            group_id TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            plugin_ns TEXT NOT NULL DEFAULT 'asset_manager',
            description TEXT,
            created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            updated_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS plugin_assets (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id),
            asset_type TEXT NOT NULL,
            source TEXT NOT NULL,
            status TEXT NOT NULL,
            file_name TEXT NOT NULL,
            file_url TEXT NOT NULL,
            mime_type TEXT,
            size INTEGER,
            reject_reason TEXT,
            category TEXT DEFAULT '未分类',
            asset_id TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            remark TEXT,
            group_id TEXT,
            plugin_ns TEXT NOT NULL DEFAULT 'asset_manager',
            created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            updated_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
        )"#,
        "ALTER TABLE plugin_assets ADD COLUMN IF NOT EXISTS category TEXT DEFAULT '未分类'",
        "COMMENT ON COLUMN plugin_assets.category IS '素材分类'",
        "ALTER TABLE plugin_assets ADD COLUMN IF NOT EXISTS asset_id TEXT",
        "COMMENT ON COLUMN plugin_assets.asset_id IS '火山方舟素材ID（如 asset://...）'",
        "ALTER TABLE plugin_assets ADD COLUMN IF NOT EXISTS sort_order INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN plugin_assets.sort_order IS '排序权重，数字越大越靠前'",
        "ALTER TABLE plugin_assets ADD COLUMN IF NOT EXISTS remark TEXT",
        "COMMENT ON COLUMN plugin_assets.remark IS '管理员内部备注'",
        "ALTER TABLE plugin_assets ADD COLUMN IF NOT EXISTS group_id TEXT",
        "COMMENT ON COLUMN plugin_assets.group_id IS '素材绑定的组合ID'",
        "ALTER TABLE plugin_assets ADD COLUMN IF NOT EXISTS content_hash TEXT",
        "COMMENT ON COLUMN plugin_assets.content_hash IS '资源内容 SHA-256 哈希值，用于精确去重'",
        "CREATE INDEX IF NOT EXISTS idx_plugin_assets_content_hash ON plugin_assets(content_hash)",
        "ALTER TABLE plugin_api_logs ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'page'",
        "COMMENT ON COLUMN plugin_api_logs.source IS '日志来源: api_proxy=对外接口 / page=页面操作 / relay_convert=转发规则替换素材'",
        "ALTER TABLE plugin_assets ADD COLUMN IF NOT EXISTS plugin_ns TEXT NOT NULL DEFAULT 'asset_manager'",
        "ALTER TABLE plugin_asset_groups ADD COLUMN IF NOT EXISTS plugin_ns TEXT NOT NULL DEFAULT 'asset_manager'",
        "CREATE INDEX IF NOT EXISTS idx_plugin_assets_asset_id_ns ON plugin_assets(asset_id, plugin_ns)",
        "CREATE INDEX IF NOT EXISTS idx_plugin_assets_source_ns ON plugin_assets(source, plugin_ns)",
        r#"INSERT INTO plugins (name, title, description, is_enabled)
           VALUES ('asset_manager', '素材资产管理', '提供全站图片、视频大模型使用的素材上传与审核功能', 0)
           ON CONFLICT (name) DO NOTHING"#,
        r#"INSERT INTO plugins (name, title, description, is_enabled)
           VALUES ('asset_manager_intl', '素材资产管理国际版', '提供全站图片、视频大模型使用的素材上传与审核功能（国际版）', 0)
           ON CONFLICT (name) DO NOTHING"#,
        r#"INSERT INTO plugins (name, title, description, is_enabled)
           VALUES ('team_marketing', '团队营销管理', '提供营销团队的用户管理，支持推广团队创建与成员管理', 0)
           ON CONFLICT (name) DO NOTHING"#,
        r#"INSERT INTO plugins (name, title, description, is_enabled)
           VALUES ('playground', '模型创作中心', '提供直接的视频、图片、声音、聊天模型体验服务', 0)
           ON CONFLICT (name) DO NOTHING"#
    );

    // ── 初始化内置服务商与模型创作中心表（受一次性迁移保护） ──
    once_migration!(pool, done, "init_playground_tables_v1",
        "ALTER TABLE model_providers ADD COLUMN IF NOT EXISTS is_system INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE model_types ADD COLUMN IF NOT EXISTS is_system INTEGER NOT NULL DEFAULT 0",
        "INSERT INTO model_providers (name, sort_order, is_system) VALUES ('火山引擎', 1, 1), ('谷歌', 2, 1), ('阿里云', 3, 1), ('腾讯云', 4, 1) ON CONFLICT(name) DO UPDATE SET is_system = 1",
        "INSERT INTO model_types (name, sort_order, is_system) VALUES ('视频', 1, 1), ('图片', 2, 1), ('音频', 3, 1), ('聊天', 4, 1), ('向量', 50, 1), ('排序', 60, 1), ('视频增强', 70, 1) ON CONFLICT(name) DO UPDATE SET is_system = 1",
        "UPDATE model_types SET default_features = '[\"输入-文字输入\",\"输入-语音输入\",\"输入-视频输入\",\"输出-文字输出\"]' WHERE name = '聊天' AND (default_features = '[]' OR default_features IS NULL)",
        "UPDATE model_types SET default_features = '[\"文生图\",\"图文生图\",\"图生图\"]' WHERE name = '图片' AND (default_features = '[]' OR default_features IS NULL)",
        "UPDATE model_types SET default_features = '[\"文生视频\",\"图生视频\",\"首尾帧生视频\",\"参考生视频\",\"视频生视频\"]' WHERE name = '视频' AND (default_features = '[]' OR default_features IS NULL)",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS mid TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS original_id TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS site_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS site_discount_enabled INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS global_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0",
        "COMMENT ON COLUMN models.global_discount IS '全站折扣倍率，开启后与等级折扣/用户折扣取最小值'",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS global_discount_enabled INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN models.global_discount_enabled IS '全站折扣开关（0=关，1=开）'",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS model_id_alias TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN models.model_id_alias IS '模型ID别名映射值，非空时上游请求使用此ID替代model_id（渠道映射优先级更高）'",
        r#"CREATE TABLE IF NOT EXISTS playground_projects (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            uid TEXT NOT NULL,
            name TEXT NOT NULL DEFAULT '未命名项目',
            description TEXT DEFAULT '',
            cover_url TEXT DEFAULT '',
            canvas_data TEXT DEFAULT '{}',
            is_deleted INTEGER NOT NULL DEFAULT 0,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg_projects_user ON playground_projects(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg_projects_uid ON playground_projects(uid)",
        r#"CREATE TABLE IF NOT EXISTS announcements (
            id SERIAL PRIMARY KEY,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            is_popup INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS playground_assets (
            id SERIAL PRIMARY KEY,
            project_id INTEGER NOT NULL REFERENCES playground_projects(id) ON DELETE CASCADE,
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
            width INTEGER DEFAULT 0,
            height INTEGER DEFAULT 0,
            is_deleted INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg_assets_project ON playground_assets(project_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg_assets_user ON playground_assets(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg_assets_type ON playground_assets(asset_type)",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS kid TEXT DEFAULT ''"
    );

    // Marketing Teams table
    // ── 初始化推广团队主表（受一次性迁移保护） ──
    once_migration!(pool, done, "init_marketing_teams_v1",
        r#"CREATE TABLE IF NOT EXISTS marketing_teams (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            invite_code TEXT UNIQUE,
            max_members INTEGER NOT NULL DEFAULT 10,
            created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            updated_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
        )"#,
        "ALTER TABLE marketing_teams ADD COLUMN IF NOT EXISTS invite_code TEXT UNIQUE",
        "ALTER TABLE marketing_teams ADD COLUMN IF NOT EXISTS max_members INTEGER NOT NULL DEFAULT 10",
        "ALTER TABLE marketing_teams ADD COLUMN IF NOT EXISTS members_can_set_level INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE marketing_teams ADD COLUMN IF NOT EXISTS leader_can_remove_members INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN marketing_teams.leader_can_remove_members IS '团队负责人是否可以移除自己的推广成员(0=否,1=是)'"
    );

    // Backfill: generate invite_code for existing teams that don't have one (受一次性迁移保护)
    #[cfg(feature = "commercial_plugins")]
    {
        if !done.contains("backfill_team_invite_codes_v1") {
            let teams_without_code: Vec<i64> = sqlx::query_scalar(
                "SELECT id FROM marketing_teams WHERE invite_code IS NULL OR invite_code = ''",
            )
            .fetch_all(&*pool)
            .await
            .unwrap_or_default();
            for tid in teams_without_code {
                let code: String = (0..8)
                    .map(|_| {
                        let idx = rand::random::<u8>() % 36;
                        if idx < 10 {
                            (b'0' + idx) as char
                        } else {
                            (b'a' + idx - 10) as char
                        }
                    })
                    .collect();
                sqlx::query("UPDATE marketing_teams SET invite_code = $1 WHERE id = $2")
                    .bind(&code)
                    .bind(tid)
                    .execute(&*pool)
                    .await
                    .ok();
            }
            let _ = sqlx::query(
                "INSERT INTO sys_migration_history (id) VALUES ('backfill_team_invite_codes_v1')",
            )
            .execute(pool)
            .await;
            done.insert("backfill_team_invite_codes_v1".into());
        }
    }

    // ── 初始化火山引擎卡池系统表及字段扩展（受一次性迁移保护） ──
    once_migration!(pool, done, "init_volcengine_pool_tables_v1",
        "ALTER TABLE plugins ADD COLUMN IF NOT EXISTS category TEXT NOT NULL DEFAULT 'user'",
        "COMMENT ON COLUMN plugins.category IS '插件分类: user=用户增强, system=系统增强'",
        "UPDATE plugins SET category = 'user' WHERE name IN ('asset_manager', 'asset_manager_intl', 'team_marketing', 'playground') AND category = ''",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category)
           VALUES ('volcengine_pool', '火山引擎卡池系统', '管理多个火山引擎账号，实现智能调度、配额限制与故障自动隔离', 0, 'system')
           ON CONFLICT (name) DO NOTHING"#,
        r#"CREATE TABLE IF NOT EXISTS volcengine_pools (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            pool_type TEXT NOT NULL DEFAULT 'chat',
            strategy TEXT NOT NULL DEFAULT 'random',
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            model_id TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "COMMENT ON TABLE volcengine_pools IS '火山引擎卡池分组表'",
        "COMMENT ON COLUMN volcengine_pools.pool_type IS '卡池类型: chat=聊天, image=图片, video=视频, custom=自定义'",
        "COMMENT ON COLUMN volcengine_pools.strategy IS '调度策略: random=随机分布, sequential=顺序轮转'",
        "ALTER TABLE volcengine_pools ADD COLUMN IF NOT EXISTS model_id TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pools DROP COLUMN IF EXISTS quota_unit",
        "ALTER TABLE volcengine_pools DROP COLUMN IF EXISTS daily_reset_hour",
        "ALTER TABLE volcengine_pools DROP COLUMN IF EXISTS daily_reset_minute",
        "ALTER TABLE volcengine_pools DROP COLUMN IF EXISTS period_start",
        "ALTER TABLE volcengine_pools DROP COLUMN IF EXISTS period_end",
        "ALTER TABLE volcengine_pools DROP COLUMN IF EXISTS default_daily_quota",
        "ALTER TABLE volcengine_pools DROP COLUMN IF EXISTS default_hourly_quota",
        "ALTER TABLE volcengine_pools DROP COLUMN IF EXISTS default_period_quota",
        r#"CREATE TABLE IF NOT EXISTS volcengine_pool_accounts (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT 'volcengine',
            base_url TEXT NOT NULL DEFAULT 'https://ark.cn-beijing.volces.com/api/v3',
            api_key TEXT NOT NULL,
            models TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL DEFAULT 'active',
            daily_quota DOUBLE PRECISION NOT NULL DEFAULT 0,
            hourly_quota DOUBLE PRECISION NOT NULL DEFAULT 0,
            period_quota DOUBLE PRECISION NOT NULL DEFAULT 0,
            daily_used DOUBLE PRECISION NOT NULL DEFAULT 0,
            hourly_used DOUBLE PRECISION NOT NULL DEFAULT 0,
            period_used DOUBLE PRECISION NOT NULL DEFAULT 0,
            last_daily_reset TEXT NOT NULL DEFAULT '',
            last_hourly_reset TEXT NOT NULL DEFAULT '',
            last_period_reset TEXT NOT NULL DEFAULT '',
            last_error TEXT,
            last_error_at TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS provider TEXT NOT NULL DEFAULT 'volcengine'",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS base_url TEXT NOT NULL DEFAULT 'https://ark.cn-beijing.volces.com/api/v3'",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS daily_quota DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS hourly_quota DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS period_quota DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS daily_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS hourly_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS period_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS last_daily_reset TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS last_hourly_reset TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS last_period_reset TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS last_error TEXT",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS last_error_at TEXT",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS daily_reset_hour INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS daily_reset_minute INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS period_start TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pool_accounts ADD COLUMN IF NOT EXISTS period_end TEXT NOT NULL DEFAULT ''",
        "COMMENT ON TABLE volcengine_pool_accounts IS '火山引擎独立账号表'",
        "COMMENT ON COLUMN volcengine_pool_accounts.base_url IS '请求地址'",
        "COMMENT ON COLUMN volcengine_pool_accounts.models IS '支持的模型列表，逗号分隔'",
        "COMMENT ON COLUMN volcengine_pool_accounts.status IS '账号状态: active=可用, disabled=故障禁用, exhausted=配额耗尽'",
        "COMMENT ON COLUMN volcengine_pool_accounts.daily_quota IS '每日配额上限(0=不限)'",
        "COMMENT ON COLUMN volcengine_pool_accounts.hourly_quota IS '每小时配额上限(0=不限)'",
        "COMMENT ON COLUMN volcengine_pool_accounts.period_quota IS '时段配额上限(0=不限)'",
        r#"CREATE TABLE IF NOT EXISTS volcengine_pool_account_mapping (
            pool_id INTEGER NOT NULL REFERENCES volcengine_pools(id) ON DELETE CASCADE,
            account_id INTEGER NOT NULL REFERENCES volcengine_pool_accounts(id) ON DELETE CASCADE,
            PRIMARY KEY (pool_id, account_id)
        )"#,
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active'",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS quota_unit TEXT NOT NULL DEFAULT 'tokens'",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS daily_reset_hour INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS daily_reset_minute INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS period_start TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS period_end TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS daily_quota DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS hourly_quota DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS period_quota DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS daily_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS hourly_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS period_used DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS last_daily_reset TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS last_hourly_reset TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS last_period_reset TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE volcengine_pool_account_mapping ADD COLUMN IF NOT EXISTS priority INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON TABLE volcengine_pool_account_mapping IS '卡池与账号的多对多映射表'",
        r#"CREATE TABLE IF NOT EXISTS volcengine_pool_logs (
            id SERIAL PRIMARY KEY,
            pool_id INTEGER NOT NULL,
            account_id INTEGER NOT NULL,
            account_name TEXT NOT NULL DEFAULT '',
            model_id TEXT NOT NULL DEFAULT '',
            channel_id INTEGER NOT NULL DEFAULT 0,
            usage_amount DOUBLE PRECISION NOT NULL DEFAULT 0,
            quota_unit TEXT NOT NULL DEFAULT 'tokens',
            status TEXT NOT NULL DEFAULT 'success',
            error_message TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "COMMENT ON TABLE volcengine_pool_logs IS '卡池调度使用日志'",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS pool_id INTEGER",
        "COMMENT ON COLUMN channels.pool_id IS '关联的火山引擎卡池ID，为空表示不使用卡池'",
        "ALTER TABLE marketing_teams ADD COLUMN IF NOT EXISTS allowed_level_ids TEXT NOT NULL DEFAULT '[]'",
        "COMMENT ON COLUMN marketing_teams.allowed_level_ids IS '团队负责人被授权可分配的用户等级ID列表(JSON数组)'",
        "ALTER TABLE marketing_teams ADD COLUMN IF NOT EXISTS allowed_member_level_ids TEXT NOT NULL DEFAULT '[]'",
        "COMMENT ON COLUMN marketing_teams.allowed_member_level_ids IS '团队负责人被授权可分配给团队成员的用户等级ID列表(JSON数组)'"
    );

    // Marketing Team Leaders table (many-to-many)
    // ── 初始化推广团队关联表与公共配置表（受一次性迁移保护） ──
    once_migration!(
        pool,
        done,
        "init_marketing_team_relations_v1",
        r#"CREATE TABLE IF NOT EXISTS marketing_team_leaders (
            id SERIAL PRIMARY KEY,
            team_id INTEGER NOT NULL,
            user_id TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            UNIQUE(team_id, user_id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS marketing_team_members (
            id SERIAL PRIMARY KEY,
            team_id INTEGER NOT NULL,
            user_id TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            UNIQUE(team_id, user_id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS plugin_configs (
            id SERIAL PRIMARY KEY,
            plugin_name TEXT NOT NULL,
            config_key TEXT NOT NULL,
            config_value TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            updated_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            UNIQUE(plugin_name, config_key)
        )"#
    );

    // -- 多方式登录注册扩展 --
    // users 表新增 google_id（谷歌 OAuth 唯一标识）与微信登录字段，受一次性迁移保护
    once_migration!(
        pool,
        done,
        "user_oauth_fields_v1",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS google_id TEXT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS wechat_name TEXT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS google_name TEXT",
        "ALTER TABLE verification_codes ADD COLUMN IF NOT EXISTS phone TEXT DEFAULT ''"
    );

    // 回填已有令牌的 kid（只处理 kid 为空的记录，受一次性迁移保护）
    if !done.contains("backfill_token_kids_v1") {
        #[derive(sqlx::FromRow)]
        struct TokenUid {
            token_id: i64,
            uid: String,
        }
        let rows: Vec<TokenUid> = sqlx::query_as(
            "SELECT t.id as token_id, u.uid FROM api_tokens t JOIN users u ON t.user_id = u.id WHERE t.kid IS NULL OR t.kid = ''"
        ).fetch_all(&*pool).await.unwrap_or_default();
        for row in rows {
            let uid_suffix: String = row
                .uid
                .chars()
                .rev()
                .take(3)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            let random_part: String = (0..3)
                .map(|_| (b'0' + rand::random::<u8>() % 10) as char)
                .collect();
            let kid = format!("{}{}", uid_suffix, random_part);
            sqlx::query("UPDATE api_tokens SET kid = $1 WHERE id = $2")
                .bind(&kid)
                .bind(row.token_id)
                .execute(&*pool)
                .await
                .ok();
        }
        let _ =
            sqlx::query("INSERT INTO sys_migration_history (id) VALUES ('backfill_token_kids_v1')")
                .execute(pool)
                .await;
        done.insert("backfill_token_kids_v1".into());
    }

    // ── 初始化 GPT-Image 卡池系统表及字段配置（受一次性迁移保护） ──
    once_migration!(pool, done, "init_gptimage_pool_tables_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category)
           VALUES ('gptimage_pool', 'GPT-Image卡池系统', '管理多个GPT-Image来源账号，实现智能调度、配额限制与故障自动隔离', 0, 'system')
           ON CONFLICT (name) DO NOTHING"#,
        r#"CREATE TABLE IF NOT EXISTS gptimage_pools (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            pool_type TEXT NOT NULL DEFAULT 'image',
            strategy TEXT NOT NULL DEFAULT 'random',
            is_active INTEGER NOT NULL DEFAULT 1,
            remark TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "COMMENT ON TABLE gptimage_pools IS 'GPT-Image卡池分组表'",
        "COMMENT ON COLUMN gptimage_pools.pool_type IS '卡池类型: image=图片, custom=自定义'",
        "COMMENT ON COLUMN gptimage_pools.strategy IS '调度策略: random=随机分布, sequential=顺序轮转'",
        r#"CREATE TABLE IF NOT EXISTS gptimage_pool_accounts (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            base_url TEXT NOT NULL DEFAULT '',
            api_key TEXT NOT NULL,
            models TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'active',
            quota_unit TEXT NOT NULL DEFAULT 'images',
            daily_reset_hour INTEGER NOT NULL DEFAULT 0,
            daily_reset_minute INTEGER NOT NULL DEFAULT 0,
            period_start TEXT NOT NULL DEFAULT '',
            period_end TEXT NOT NULL DEFAULT '',
            daily_quota DOUBLE PRECISION NOT NULL DEFAULT 0,
            hourly_quota DOUBLE PRECISION NOT NULL DEFAULT 0,
            period_quota DOUBLE PRECISION NOT NULL DEFAULT 0,
            daily_used DOUBLE PRECISION NOT NULL DEFAULT 0,
            hourly_used DOUBLE PRECISION NOT NULL DEFAULT 0,
            period_used DOUBLE PRECISION NOT NULL DEFAULT 0,
            last_daily_reset TEXT NOT NULL DEFAULT '',
            last_hourly_reset TEXT NOT NULL DEFAULT '',
            last_period_reset TEXT NOT NULL DEFAULT '',
            last_error TEXT,
            last_error_at TEXT,
            priority INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "COMMENT ON TABLE gptimage_pool_accounts IS 'GPT-Image来源账号表'",
        "COMMENT ON COLUMN gptimage_pool_accounts.base_url IS '请求地址，如 https://api.openai.com'",
        "COMMENT ON COLUMN gptimage_pool_accounts.models IS '支持的模型列表，逗号分隔'",
        "COMMENT ON COLUMN gptimage_pool_accounts.quota_unit IS '配额计量单位: tokens=Token数, requests=请求次数, images=图片张数'",
        "COMMENT ON COLUMN gptimage_pool_accounts.status IS '账号状态: active=可用, disabled=故障禁用, exhausted=配额耗尽'",
        "COMMENT ON COLUMN gptimage_pool_accounts.daily_quota IS '每日配额上限(0=不限)'",
        "COMMENT ON COLUMN gptimage_pool_accounts.hourly_quota IS '每小时配额上限(0=不限)'",
        "COMMENT ON COLUMN gptimage_pool_accounts.period_quota IS '时段配额上限(0=不限)'",
        r#"CREATE TABLE IF NOT EXISTS gptimage_pool_account_mapping (
            pool_id INTEGER NOT NULL REFERENCES gptimage_pools(id) ON DELETE CASCADE,
            account_id INTEGER NOT NULL REFERENCES gptimage_pool_accounts(id) ON DELETE CASCADE,
            PRIMARY KEY (pool_id, account_id)
        )"#,
        "COMMENT ON TABLE gptimage_pool_account_mapping IS 'GPT-Image卡池与账号的多对多映射表'",
        r#"CREATE TABLE IF NOT EXISTS gptimage_pool_logs (
            id SERIAL PRIMARY KEY,
            pool_id INTEGER NOT NULL,
            account_id INTEGER NOT NULL,
            account_name TEXT NOT NULL DEFAULT '',
            model_id TEXT NOT NULL DEFAULT '',
            channel_id INTEGER NOT NULL DEFAULT 0,
            usage_amount DOUBLE PRECISION NOT NULL DEFAULT 0,
            quota_unit TEXT NOT NULL DEFAULT 'images',
            status TEXT NOT NULL DEFAULT 'success',
            error_message TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "COMMENT ON TABLE gptimage_pool_logs IS 'GPT-Image卡池调度使用日志'",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS gptimage_pool_id INTEGER",
        "COMMENT ON COLUMN channels.gptimage_pool_id IS '关联的GPT-Image卡池ID，为空表示不使用卡池'",
        "ALTER TABLE models DROP CONSTRAINT IF EXISTS models_name_key",
        "ALTER TABLE models DROP CONSTRAINT IF EXISTS models_model_id_key"
    );

    // ══════════════════════════════════════════════════════════════
    //  模型广场管理插件
    // ══════════════════════════════════════════════════════════════
    // ── 初始化站点图标库及流转计费日志扩展（受一次性迁移保护） ──
    once_migration!(pool, done, "init_site_icons_tables_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category)
           VALUES ('model_marketplace', '模型广场管理', '管理模型广场的模型展示，控制哪些模型对用户可见并配置展示信息', 0, 'user')
           ON CONFLICT (name) DO NOTHING"#,
        r#"INSERT INTO plugins (name, title, description, is_enabled, category)
           VALUES ('site_icons', '站点icon图标库', '提供 AI/LLM 品牌 SVG 图标库，支持搜索选择和自定义上传，数据来源 lobehub/lobe-icons', 1, 'system_builtin')
           ON CONFLICT (name) DO NOTHING"#,
        r#"CREATE TABLE IF NOT EXISTS site_icons (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            file_path TEXT NOT NULL DEFAULT '',
            source TEXT NOT NULL DEFAULT 'lobe-icons',
            category TEXT NOT NULL DEFAULT 'AI品牌',
            tags TEXT NOT NULL DEFAULT '[]',
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text),
            UNIQUE(name, source)
        )"#,
        "COMMENT ON TABLE site_icons IS '站点图标库，存储 SVG 图标元数据'",
        "COMMENT ON COLUMN site_icons.name IS '图标标识名（如 openai, claude）'",
        "COMMENT ON COLUMN site_icons.title IS '显示名称（如 OpenAI, Claude）'",
        "COMMENT ON COLUMN site_icons.file_path IS 'SVG 文件路径（相对于 data/assets/）'",
        "COMMENT ON COLUMN site_icons.source IS '图标来源: lobe-icons=从 GitHub 同步 / custom=手动上传'",
        "COMMENT ON COLUMN site_icons.category IS '分类: AI品牌 / 自定义'",
        "COMMENT ON COLUMN site_icons.tags IS '标签(JSON数组)'",
        r#"CREATE TABLE IF NOT EXISTS site_icon_sync_logs (
            id SERIAL PRIMARY KEY,
            total_synced INTEGER NOT NULL DEFAULT 0,
            total_new INTEGER NOT NULL DEFAULT 0,
            total_updated INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'success',
            error_message TEXT,
            created_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "COMMENT ON TABLE site_icon_sync_logs IS '站点图标同步日志'",
        "ALTER TABLE models ADD COLUMN IF NOT EXISTS logo TEXT",
        "ALTER TABLE model_providers ADD COLUMN IF NOT EXISTS logo TEXT",
        "ALTER TABLE model_types ADD COLUMN IF NOT EXISTS logo TEXT",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS cached_tokens INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN logs.cached_tokens IS '缓存命中的Token数量(属于输入的子集)'",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS billing_features TEXT",
        "COMMENT ON COLUMN logs.billing_features IS 'POST阶段提取的计费特征快照(JSON)，独立于enable_log开关，确保异步任务结算时始终有完整计费参数'",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS pre_deduct_gift DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN logs.pre_deduct_gift IS '预扣费中从赠送余额扣除的金额，用于退款时精准归还到对应钱包'",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS remark TEXT",
        "COMMENT ON COLUMN users.remark IS '推广用户备注'",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS referral_history TEXT DEFAULT ''",
        "COMMENT ON COLUMN users.referral_history IS '关联流转记录'",
        "INSERT INTO model_providers (name, sort_order, is_system) VALUES ('可灵 AI', 4, 1) ON CONFLICT(name) DO UPDATE SET is_system = 1"
    );

    // ══════════════════════════════════════════════════════════════
    //  智能路由 (Router Flow) 插件
    // ══════════════════════════════════════════════════════════════
    // ── 初始化智能路由表及通道等级字段扩充（受一次性迁移保护） ──
    once_migration!(pool, done, "init_router_flow_tables_v1",
        r#"INSERT INTO plugins (name, title, description, is_enabled, category)
           VALUES ('router_flow', '智能路由', '配置多个相同模型组成高可用路由组，支持价格优先、速度优先、稳定优先三种智能调度策略', 0, 'user')
           ON CONFLICT (name) DO NOTHING"#,
        r#"CREATE TABLE IF NOT EXISTS router_flow_groups (
            id SERIAL PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT DEFAULT '',
            route_rule TEXT NOT NULL DEFAULT 'price',
            model_ids TEXT NOT NULL DEFAULT '[]',
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        "COMMENT ON TABLE router_flow_groups IS '智能路由模型组表，用户创建的模型路由组'",
        "COMMENT ON COLUMN router_flow_groups.route_rule IS '路由策略: price=价格优先, speed=速度优先, stability=稳定优先'",
        "COMMENT ON COLUMN router_flow_groups.model_ids IS '绑定的模型 mid 列表(JSON数组)'",
        "CREATE INDEX IF NOT EXISTS idx_rf_groups_user ON router_flow_groups(user_id)",
        "ALTER TABLE router_flow_groups ADD COLUMN IF NOT EXISTS endpoint_id TEXT NOT NULL DEFAULT ''",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_rf_groups_endpoint ON router_flow_groups(endpoint_id) WHERE endpoint_id != ''",
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS exclude_user_groups TEXT NOT NULL DEFAULT '[]'",
        "COMMENT ON COLUMN channels.exclude_user_groups IS '不支持的用户等级列表(JSON数组)，黑名单模式'",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS gift_balance DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS gift_used_quota DOUBLE PRECISION NOT NULL DEFAULT 0.0",
        "COMMENT ON COLUMN users.gift_balance IS '赠送钱包余额，注册赠送/活动赠送等免费额度，消费时优先扣赠送余额'",
        "ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS last_used_at VARCHAR(30) DEFAULT NULL",
        "COMMENT ON COLUMN api_tokens.last_used_at IS '令牌最后使用时间'"
    );

    Ok(())
}
