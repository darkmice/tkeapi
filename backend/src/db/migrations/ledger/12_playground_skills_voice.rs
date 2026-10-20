/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use sqlx::PgPool;
use std::collections::HashSet;

use crate::db::migrations::once_migration;

pub async fn run(pool: &PgPool, done: &mut HashSet<String>) -> anyhow::Result<()> {

    once_migration!(pool, done, "playground_2026_conversations_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_conversations (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            title TEXT NOT NULL DEFAULT '',
            model_mid TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        r#"CREATE TABLE IF NOT EXISTS playground_2026_conversation_messages (
            id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL REFERENCES playground_2026_conversations(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_pg2026_conversations_user_updated ON playground_2026_conversations (user_id, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_conversation_messages_conv_created ON playground_2026_conversation_messages (conversation_id, created_at ASC)"
    );

    once_migration!(
        pool,
        done,
        "playground_2026_conversation_message_parts_v1",
        "ALTER TABLE playground_2026_conversation_messages ADD COLUMN IF NOT EXISTS parts JSONB"
    );

    // ── 系统预设模型按业务场景归并原始模型 ID（聚合前端卡片展示） ──
    once_migration!(pool, done, "preset_models_original_id_grouping_v1",
        "UPDATE models SET original_id = '视频增强' WHERE mid IN ('vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep')",
        "UPDATE models SET original_id = '画质增强级联' WHERE mid IN ('dbs-sr', 'dbs-fs')",
        "UPDATE models SET original_id = 'Doubao Seedance 预设' WHERE mid IN ('310001', '310002', '310003', '310004')"
    );

    once_migration!(pool, done, "playground_2026_skills_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_skills (
            id TEXT PRIMARY KEY,
            user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
            slug TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            body_md TEXT NOT NULL DEFAULT '',
            applies_to JSONB NOT NULL DEFAULT '[]'::jsonb,
            trigger_words JSONB NOT NULL DEFAULT '[]'::jsonb,
            source TEXT NOT NULL DEFAULT 'user',
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_pg2026_skills_user_slug ON playground_2026_skills (user_id, slug) WHERE user_id IS NOT NULL",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_pg2026_skills_official_slug ON playground_2026_skills (slug) WHERE user_id IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_skills_user_updated ON playground_2026_skills (user_id, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_skills_source_updated ON playground_2026_skills (source, updated_at DESC)",
        "ALTER TABLE playground_2026_conversations ADD COLUMN IF NOT EXISTS skill_id TEXT"
    );

    once_migration!(
        pool,
        done,
        "playground_2026_skills_seed_prompt_rewriter_v1",
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-prompt-rewriter', NULL, 'prompt-rewriter', '提示词改写',
            '把口语需求整理成可粘贴到图片/视频页的提示词，不会替你出图。',
            E'你是提示词改写助手。用户用口语描述想生成的图片或视频时，你把它整理成可直接粘贴到生成页的提示词。\n\n规则：\n- 先确认主体、风格、构图、禁忌；信息不足就提问，不要臆造关键细节。\n- 给出一段完整、可复制的提示词（与用户语言一致）。\n- 不要声称你能出图、出视频或调用生成接口。用户需要自己去图片/视频页粘贴并生成。\n- 不要输出网关路径、厂商字段名或工具调用。',
            '["chat"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'prompt-rewriter' AND user_id IS NULL
        )"#
    );

    once_migration!(
        pool,
        done,
        "playground_2026_skill_categories_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_skill_categories (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            name_en TEXT NOT NULL DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_system BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
        r#"INSERT INTO playground_2026_skill_categories (id, name, name_en, sort_order, is_system)
        SELECT v.id, v.name, v.name_en, v.sort_order, TRUE
        FROM (VALUES
            ('chat', '聊天', 'Chat', 10),
            ('image', '图片', 'Image', 20),
            ('video', '视频', 'Video', 30),
            ('audio', '声音', 'Audio', 40)
        ) AS v(id, name, name_en, sort_order)
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skill_categories c WHERE c.id = v.id
        )"#
    );

    once_migration!(
        pool,
        done,
        "playground_2026_skills_seed_chat_writing_v1",
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-wenyan', NULL, 'wenyan', '文言文',
            '把白话改成典雅文言文，专名与术语保持原形。',
            $wenyan$你是文言文写作助手。用户给出白话、主题或现代文本时，改写成典雅可诵的文言文；若用户直接提问，也用文言作答。

规则：
- 用文言句式：判断用「…者…也」，因果用「盖」「故」，假设用「若」「倘」，完成用「矣」。删冗词，能省主语则省。
- 「之乎者也」点到即止，禁止堆砌虚词、网络口语和表情。
- 人名、地名、产品名、代码与术语保持原形，不强译。
- 先给正文；用户要对照时再附简短白话释义，不要把释义写进正文。
- 不要伪造典故出处；拿不准就不用典。$wenyan$,
            '["chat"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'wenyan' AND user_id IS NULL
        )"#,
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-gushi', NULL, 'gushi', '古诗',
            '按主题作近体诗，默认可作七绝；指定体裁则从其请。',
            $gushi$你是古诗创作助手。按用户主题、情绪或场景作诗。默认近体诗；用户指定诗经体、古风或词牌时再换体。

规则：
- 未点名体裁时作七言绝句。点名五绝、五律、七律、古风、词牌或诗经体则从其请。
- 近体诗：一句之内平仄交替，一联之内相对，邻联相粘；律诗中间两联对仗；押平声韵，一韵到底。
- 用赋比兴：可直陈，可比拟，宜先以草木岁时起兴再落到人事。重章叠句只用于诗经体。
- 先出诗题与正文；需要时再附格律说明（平仄、韵部）。不要把说明写进诗句。
- 今事、品牌、网络梗用古典意象转写，不要硬塞进诗。
- 不要伪称某句出自唐诗宋词。$gushi$,
            '["chat"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'gushi' AND user_id IS NULL
        )"#,
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-ad-copy', NULL, 'ad-copy', '广告词',
            '按产品与媒介写可投放的口号和短文案，不替你投放。',
            $adcopy$你是广告词写作助手。按用户的产品、对象和媒介写可投放的文案，不替用户投放。

规则：
- 先确认产品、受众、卖点、媒介（海报口号 / 短视频口播 / 详情页 / 搜索广告）。信息不足就提问，不要臆造关键卖点。
- 结构用公开文案框架：AIDA（注意-兴趣-欲望-行动）、PAS（问题-放大-解决）；标题对照 4U（有用、紧迫、独特、具体）。
- 每稿给：主口号（不超过十二字）、一句副句、以及两条备选。口号要能上口、有利益点，避免空洞形容词堆砌。
- 不写违法承诺、绝对化用语和未经验证的疗效或收益；不确定就改成可核验的表述。
- 不要输出投放后台字段名，也不要声称已经发布广告。$adcopy$,
            '["chat"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'ad-copy' AND user_id IS NULL
        )"#
    );

    once_migration!(
        pool,
        done,
        "playground_2026_skills_seed_image_video_v1",
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-cinematic-still', NULL, 'cinematic-still', '电影静帧',
            '电影剧照质感的静帧约束，用于图片生成。',
            $cinematic_still$电影剧照质感：35mm 镜头，浅景深，胶片颗粒，实用光，构图留白。真实皮肤与材质，不要插画、不要文字水印、不要畸变人脸。$cinematic_still$,
            '["image"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'cinematic-still' AND user_id IS NULL
        )"#,
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-product-hero', NULL, 'product-hero', '产品主图',
            '电商主图约束，用于图片生成。',
            $product_hero$电商主图：单件产品居中，干净棚拍，柔和立体光，背景简洁。突出材质与边缘，不要手持自拍、不要夸张场景、不要功效承诺文字。$product_hero$,
            '["image"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'product-hero' AND user_id IS NULL
        )"#,
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-cinematic-move', NULL, 'cinematic-move', '电影运镜',
            '电影短镜头运镜约束，用于视频生成。',
            $cinematic_move$电影短镜头：一条连续动作，缓慢推近或横移，稳定器质感，自然光。不要跳切、不要分镜列表、不要字幕和台标。$cinematic_move$,
            '["video"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'cinematic-move' AND user_id IS NULL
        )"#,
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-product-turn', NULL, 'product-turn', '产品展示',
            '产品展示片约束，用于视频生成。',
            $product_turn$产品展示片：产品静止或缓慢旋转，灯光匀、背景干净，突出外形与材质。不要剧情故事、不要多场景切换、不要口播字幕。$product_turn$,
            '["video"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'product-turn' AND user_id IS NULL
        )"#
    );

    once_migration!(
        pool,
        done,
        "playground_2026_skills_seed_image_video_audio_v1",
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-kawaii-still', NULL, 'kawaii-still', '可爱插画',
            '圆润Q版可爱风，可加星星、爱心、蝴蝶结等元素，用于图片生成。',
            $kawaii_still$圆润Q版可爱风：大眼睛、软萌脸颊，马卡龙或奶油色。可加星星、爱心、蝴蝶结、小动物等可爱元素。插画或轻厚涂，不要写实恐怖、不要血腥、不要文字水印。$kawaii_still$,
            '["image"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'kawaii-still' AND user_id IS NULL
        )"#,
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-kawaii-move', NULL, 'kawaii-move', '可爱短片',
            'Q版角色轻快短镜头，可加星星闪烁、爱心飘散，用于视频生成。',
            $kawaii_move$可爱短片：圆润Q版角色，动作轻快有弹性，可加星星闪烁、爱心飘散等可爱元素。一条连续动作，镜头平稳。不要跳切、不要恐怖、不要字幕和台标。$kawaii_move$,
            '["video"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'kawaii-move' AND user_id IS NULL
        )"#,
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-warm-narration', NULL, 'warm-narration', '温暖旁白',
            '柔和清晰的旁白声线约束，用于声音生成。',
            $warm_narration$温暖旁白：声线柔和清晰，语速适中，像对朋友说话。情绪贴合文本，不要播音腔、不要机械音、不要嘈杂背景。$warm_narration$,
            '["audio"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'warm-narration' AND user_id IS NULL
        )"#
    );

    once_migration!(
        pool,
        done,
        "playground_2026_audio_enabled_default_off_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('playground_2026', 'audio_enabled', 'false', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    once_migration!(pool, done, "playground_2026_audio_module_rename_v1",
        "UPDATE playground_2026_skill_categories SET name = '音频', updated_at = NOW() WHERE id = 'audio' AND name <> '音频'",
        "UPDATE playground_2026_skills SET description = '柔和清晰的旁白声线约束，用于音频生成。' WHERE id = 'pg2026-skill-warm-narration' AND description LIKE '%用于声音生成%'"
    );

    once_migration!(pool, done, "playground_2026_skills_drop_audio_v1",
        "DELETE FROM playground_2026_skills WHERE id = 'pg2026-skill-warm-narration' OR (slug = 'warm-narration' AND user_id IS NULL)",
        r#"DELETE FROM playground_2026_skills
           WHERE applies_to @> jsonb_build_array('audio')
             AND jsonb_array_length(applies_to) = 1"#,
        r#"UPDATE playground_2026_skills
           SET applies_to = COALESCE((
                SELECT jsonb_agg(elem)
                FROM jsonb_array_elements_text(applies_to) AS t(elem)
                WHERE elem <> 'audio'
            ), '[]'::jsonb),
               updated_at = NOW()
           WHERE applies_to @> jsonb_build_array('audio')"#,
        r#"UPDATE playground_2026_skills SET applies_to = '["chat"]'::jsonb
           WHERE applies_to = '[]'::jsonb"#,
        "DELETE FROM playground_2026_skill_categories WHERE id = 'audio'"
    );

    // ── 腾讯云 VOD MiniMax 视频生成专属转发规则（支持 OpenAI 与 MiniMax 官方路由） ──
    once_migration!(
        pool,
        done,
        "tencent_vod_minimax_video_forward_rule_v2",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '腾讯云 VOD MiniMax 视频生成', 'tencent', '将 OpenAI 与 MiniMax 官方视频请求转换为腾讯云点播 AIGC CreateAigcVideoTask 接口。密钥格式：SecretId:SecretKey:SubAppId，模型格式：ModelName@ModelVersion', '{"target_type":"tencent_vod_video","path_rewrite":{"old":"/v1/video/generations","new":"/"},"path_accept":["/v2/video_generation"],"auth_type":"tencent_vod"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '腾讯云 VOD MiniMax 视频生成')"#
    );

    // ── 站点模型展示排序统一到 models.sort_order（页面排序） ──
    if !done.contains("unify_model_page_sort_order_v1") {
        match crate::db::migrations::helpers::model_sort::backfill_models_sort_order_from_plugin_configs(pool).await {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('unify_model_page_sort_order_v1')",
                )
                .execute(pool)
                .await;
                done.insert("unify_model_page_sort_order_v1".into());
                tracing::info!("模型页面排序回填完成，更新 {} 条", n);
            }
            Err(e) => {
                tracing::warn!(
                    "unify_model_page_sort_order_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    once_migration!(
        pool,
        done,
        "playground_2026_prompt_optimize_category_v1",
        r#"INSERT INTO playground_2026_skill_categories (id, name, name_en, sort_order, is_system)
        SELECT 'prompt-optimize', '提示词优化', 'Prompt optimize', 90, TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skill_categories WHERE id = 'prompt-optimize'
        )"#
    );

    once_migration!(
        pool,
        done,
        "playground_2026_prompt_optimize_skills_v1",
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-prompt-optimize-image', NULL, 'prompt-optimize-image', '图片提示词优化',
            '把用户的画面描述扩写成可直接用于文生图的提示词。',
            $poi$你是图片提示词优化助手。用户给出想生成的画面描述时，把它扩写成更具体、可直接用于文生图的提示词。

规则：
- 只输出优化后的提示词正文，不要前言、标题、解释或 markdown 围栏。
- 保留用户的主体、风格与禁忌；信息不足时合理补全构图、光线、材质、镜头，不要改掉核心对象。
- 与用户语言一致。不要声称你能出图。$poi$,
            '["prompt-optimize","image"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'prompt-optimize-image' AND user_id IS NULL
        )"#,
        r#"INSERT INTO playground_2026_skills (id, user_id, slug, name, description, body_md, applies_to, trigger_words, source, enabled)
        SELECT 'pg2026-skill-prompt-optimize-video', NULL, 'prompt-optimize-video', '视频提示词优化',
            '把用户的镜头描述扩写成可直接用于文生视频的提示词。',
            $pov$你是视频提示词优化助手。用户给出想生成的镜头或动作描述时，把它扩写成更具体、可直接用于文生视频的提示词。

规则：
- 只输出优化后的提示词正文，不要前言、标题、解释或 markdown 围栏。
- 保留用户的主体、风格与禁忌；信息不足时合理补全镜头运动、时长感、光影与节奏，不要改掉核心对象。
- 与用户语言一致。不要声称你能出视频。$pov$,
            '["prompt-optimize","video"]'::jsonb, '[]'::jsonb, 'official', TRUE
        WHERE NOT EXISTS (
            SELECT 1 FROM playground_2026_skills WHERE slug = 'prompt-optimize-video' AND user_id IS NULL
        )"#
    );

    if !done.contains("playground_2026_prompt_optimize_llm_v1") {
        match crate::db::migrations::helpers::prompt_optimize::seed_prompt_optimize_llm_mid(pool)
            .await
        {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_prompt_optimize_llm_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_prompt_optimize_llm_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_prompt_optimize_llm_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    once_migration!(
        pool,
        done,
        "playground_2026_conversation_message_extra_v1",
        "ALTER TABLE playground_2026_conversation_messages ADD COLUMN IF NOT EXISTS extra JSONB"
    );

    // ── 火山方舟聊天转发规则升级：支持 Chat、Responses、Messages 三协议多路径重写 ──
    once_migration!(
        pool,
        done,
        "volcengine_chat_three_protocols_rewrites_v1",
        r#"UPDATE forward_rules
           SET config_json = '{"target_type":"volcengine_chat","path_rewrites":[{"old":"/v1/chat/completions","new":"/api/v3/chat/completions"},{"old":"/v1/responses","new":"/api/v3/responses"},{"old":"/v1/messages","new":"/api/compatible/v1/messages"}],"auth_type":"bearer"}',
               description = '将标准聊天、Responses及Messages请求转发到火山方舟官方接口，支持/api/v3/chat/completions、/api/v3/responses与/api/compatible/v1/messages',
               updated_at = CURRENT_TIMESTAMP
           WHERE name = '火山方舟 聊天' AND is_system = 1"#
    );

    once_migration!(
        pool,
        done,
        "announcements_flag_expires_at_v1",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS pin_expires_at TIMESTAMPTZ",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS popup_expires_at TIMESTAMPTZ",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS active_expires_at TIMESTAMPTZ",
        "COMMENT ON COLUMN announcements.pin_expires_at IS '置顶有效期，到期自动取消置顶'",
        "COMMENT ON COLUMN announcements.popup_expires_at IS '弹窗有效期，到期自动关闭弹窗'",
        "COMMENT ON COLUMN announcements.active_expires_at IS '上架有效期，到期自动下架'"
    );

    if !done.contains("playground_2026_chat_business_init_v1") {
        match crate::db::migrations::helpers::glm_deepseek_chat::seed_playground_2026_glm_deepseek_chat(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_chat_business_init_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_chat_business_init_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_chat_business_init_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_voice_library_v1") {
        match crate::db::migrations::helpers::voice_library::seed_playground_2026_voice_library(
            pool,
        )
        .await
        {
            Ok(n) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_voice_library_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_voice_library_v1".into());
                tracing::info!("创作中心2026 音频方案音色库回填完成，更新 {} 条配置", n);
            }
            Err(e) => {
                tracing::warn!("playground_2026_voice_library_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }
    Ok(())
}
