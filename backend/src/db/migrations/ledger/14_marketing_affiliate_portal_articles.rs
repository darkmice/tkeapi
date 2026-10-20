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

    once_migration!(pool, done, "announcements_target_module_v1",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS target_module VARCHAR(32) NOT NULL DEFAULT 'console'",
        "COMMENT ON COLUMN announcements.target_module IS '通知模块：console=控制台，playground_2026=创作中心2026，portal_pro=站点门户增强版'"
    );

    once_migration!(pool, done, "announcements_target_modules_color_v1",
        "ALTER TABLE announcements ALTER COLUMN target_module TYPE TEXT",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS banner_bg_color VARCHAR(16)",
        "COMMENT ON COLUMN announcements.target_module IS '通知模块，逗号分隔可多选：console,playground_2026,portal_pro'",
        "COMMENT ON COLUMN announcements.banner_bg_color IS '创作中心与门户顶栏背景色，#RRGGBB，空为默认背景'"
    );

    once_migration!(pool, done, "announcements_split_banner_bg_v1",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS playground_banner_bg VARCHAR(16)",
        "ALTER TABLE announcements ADD COLUMN IF NOT EXISTS portal_banner_bg VARCHAR(16)",
        "COMMENT ON COLUMN announcements.playground_banner_bg IS '创作中心顶栏背景色，#RRGGBB，空为默认'",
        "COMMENT ON COLUMN announcements.portal_banner_bg IS '门户顶栏背景色，#RRGGBB，空为默认'",
        r#"UPDATE announcements SET playground_banner_bg = banner_bg_color
           WHERE playground_banner_bg IS NULL AND banner_bg_color IS NOT NULL AND banner_bg_color <> ''
             AND position('playground_2026' in target_module) > 0"#,
        r#"UPDATE announcements SET portal_banner_bg = banner_bg_color
           WHERE portal_banner_bg IS NULL AND banner_bg_color IS NOT NULL AND banner_bg_color <> ''
             AND position('portal_pro' in target_module) > 0"#
    );

    once_migration!(
        pool,
        done,
        "playground_2026_site_default_enabled_default_off_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('playground_2026', 'site_default_enabled', 'false', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#
    );

    once_migration!(pool, done, "user_level_commission_basis_v1",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS commission_basis TEXT NOT NULL DEFAULT 'consumption'",
        "COMMENT ON COLUMN user_levels.commission_basis IS '返佣发放基准: consumption=按系统钱包实际消费, recharge=按充值入账'",
        "ALTER TABLE commissions ADD COLUMN IF NOT EXISTS source_key TEXT",
        "COMMENT ON COLUMN commissions.source_key IS '返佣幂等键: log:{logs.id} | recharge:{recharge_records.id} | ark:{id} | arkrefund:{id}'",
        "CREATE UNIQUE INDEX IF NOT EXISTS uidx_commissions_source_key ON commissions (source_key) WHERE source_key IS NOT NULL"
    );

    once_migration!(
        pool,
        done,
        "playground_2026_dedicated_token_unique_v1",
        r#"DELETE FROM api_tokens a
           WHERE a.only_playground_2026 = 1
             AND a.name = '创作中心2026'
             AND EXISTS (
               SELECT 1 FROM api_tokens b
               WHERE b.only_playground_2026 = 1
                 AND b.name = '创作中心2026'
                 AND b.user_id = a.user_id
                 AND b.id < a.id
             )"#,
        r#"CREATE UNIQUE INDEX IF NOT EXISTS uidx_api_tokens_pg2026_dedicated
           ON api_tokens (user_id)
           WHERE only_playground_2026 = 1 AND name = '创作中心2026'"#
    );

    once_migration!(
        pool,
        done,
        "volcengine_enhance_default_off_v1",
        r#"UPDATE plugins SET is_enabled = 0, updated_at = CURRENT_TIMESTAMP
           WHERE name = 'volcengine_enhance'
             AND NOT EXISTS (
                 SELECT 1 FROM plugin_configs
                 WHERE plugin_name = 'volcengine_enhance'
                   AND config_key = 'keys'
                   AND config_value IS NOT NULL
                   AND config_value <> ''
                   AND config_value <> '[]'
             )"#
    );

    // 新装默认关闭。系统更新时，管理后台已开启的创作中心2026保持开启。
    once_migration!(
        pool,
        done,
        "playground_2026_default_off_v1",
        r#"UPDATE plugins SET is_enabled = 0, updated_at = CURRENT_TIMESTAMP
           WHERE name = 'playground_2026' AND COALESCE(is_enabled, 0) <> 1"#
    );

    once_migration!(
        pool,
        done,
        "remove_seedance_cascade_enhance_models_v1",
        "DELETE FROM model_library WHERE mid IN ('dbs-sr', 'dbs-fs') OR model_id IN ('Doubao-seedance-2-0-sr', 'Doubao-seedance-2-0-fast-sr')",
        "DELETE FROM models WHERE mid IN ('dbs-sr', 'dbs-fs') OR model_id IN ('Doubao-seedance-2-0-sr', 'Doubao-seedance-2-0-fast-sr')"
    );

    // 八条 MediaKit 预置模型录入模型仓库目录，默认保持关闭（is_active = 0），由管理员在插件配置中按需激活。
    if !done.contains("volc_mediakit_preset_library_catalog_v1") {
        match crate::db::migrations::helpers::volc_mediakit_image::ensure_enhance_library_catalog(
            pool,
        )
        .await
        {
            Ok(()) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('volc_mediakit_preset_library_catalog_v1')",
                )
                .execute(pool)
                .await;
                done.insert("volc_mediakit_preset_library_catalog_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "volc_mediakit_preset_library_catalog_v1 写入目录失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    once_migration!(
        pool,
        done,
        "volc_mediakit_preset_models_default_off_v1",
        r#"UPDATE model_library
           SET is_active = 0, updated_at = CURRENT_TIMESTAMP
           WHERE mid IN ('vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep', 'vie-en', 'vir-bg')"#,
        r#"UPDATE models
           SET is_active = 0, updated_at = CURRENT_TIMESTAMP
           WHERE mid IN ('vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep', 'vie-en', 'vir-bg')
              OR library_mid IN ('vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep', 'vie-en', 'vir-bg')"#
    );

    // 画质增强模型默认全部留在模型仓库，不可预先塞入运营模型列表。未绑渠道/令牌/日志的预置行与重复上架副本全部清除。
    once_migration!(
        pool,
        done,
        "volc_mediakit_models_library_only_by_default_v1",
        r#"DELETE FROM models m
           WHERE (
               m.mid IN ('vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep', 'vie-en', 'vir-bg')
               OR (m.library_mid IN ('vve-sd', 'vve-pf', 'vve-ft', 'vve-gt', 'vvs-er', 'vvs-ep', 'vie-en', 'vir-bg') AND m.is_listed = 1)
           )
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

    // 用户端资产素材菜单默认关闭；已保存的菜单文案「资产充值」改为「资产素材」
    once_migration!(
        pool,
        done,
        "asset_manager_user_menu_default_off_v1",
        r#"INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at)
           VALUES ('asset_manager', 'user_asset_menu_enabled', 'false', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
           ON CONFLICT (plugin_name, config_key) DO NOTHING"#,
        r#"UPDATE settings SET value = replace(
            replace(value, '"label_zh":"资产充值"', '"label_zh":"资产素材"'),
            '"label_zh": "资产充值"', '"label_zh": "资产素材"'
        ) WHERE key = 'menu_config_settings'"#
    );

    once_migration!(pool, done, "commission_transfer_limits_daily_v1",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS commission_transfer_min DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS commission_transfer_max DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN user_levels.commission_transfer_min IS '佣金钱包单次划转下限，0 为不限制'",
        "COMMENT ON COLUMN user_levels.commission_transfer_max IS '佣金钱包单次划转上限，0 为不限制'",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS commission_posted INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE recharge_records ADD COLUMN IF NOT EXISTS commission_posted INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN logs.commission_posted IS '消费返佣是否已计入当日汇总，1 为已计入'",
        "COMMENT ON COLUMN recharge_records.commission_posted IS '该流水的返佣是否已计入当日汇总，1 为已计入'"
    );

    once_migration!(pool, done, "commission_wallet_unmirror_gift_v1",
        "COMMENT ON COLUMN users.commission_balance IS '佣金钱包。返佣只进此余额。调用扣费不使用它，用户可划转到系统钱包。'",
        r#"UPDATE users SET commission_balance = 0, updated_at = CURRENT_TIMESTAMP
           WHERE commission_balance <> 0"#
    );

    // ── 用户密码修改时间戳（用于吊销旧会话 JWT 凭据） ──
    once_migration!(pool, done, "user_password_changed_at_v1",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS password_changed_at TIMESTAMPTZ",
        "COMMENT ON COLUMN users.password_changed_at IS '密码最后修改时间，早于此时间的会话凭据判定为失效'"
    );

    // ── 用户姓名（名、姓氏）与出生年月 ──
    once_migration!(pool, done, "user_name_birth_ym_v1",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS first_name TEXT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS last_name TEXT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS birth_ym TEXT",
        "COMMENT ON COLUMN users.first_name IS '名'",
        "COMMENT ON COLUMN users.last_name IS '姓氏'",
        "COMMENT ON COLUMN users.birth_ym IS '出生年月，格式 YYYY-MM'"
    );

    // ── 充值订单跨币种收款快照：渠道收款币种、应收金额与下单汇率 ──
    once_migration!(pool, done, "orders_pay_currency_fx_snapshot_v1",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS pay_currency TEXT",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS pay_amount DOUBLE PRECISION",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS fx_rate DOUBLE PRECISION",
        "COMMENT ON COLUMN orders.amount IS '入账金额（站点默认货币）'",
        "COMMENT ON COLUMN orders.pay_currency IS '渠道收款币种；NULL 为旧订单，与站点默认货币同币种'",
        "COMMENT ON COLUMN orders.pay_amount IS '渠道应收金额（收款币种）；NULL 时等于 amount'",
        "COMMENT ON COLUMN orders.fx_rate IS '下单时 1 默认货币 = fx_rate 收款币种（含加价）；同币种为 NULL'"
    );

    // ── 用户头像（对象存储 users/{uid}/avatar/） ──
    once_migration!(
        pool,
        done,
        "user_avatar_v1",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar TEXT",
        "COMMENT ON COLUMN users.avatar IS '用户头像 URL，文件位于站点存储 users/{uid}/avatar/'"
    );

    // ── DocsApi：预设文档与手动文档分开。重置初始化只覆盖 is_preset = 1 ──
    once_migration!(
        pool,
        done,
        "plugin_docs_is_preset_v1",
        "ALTER TABLE plugin_docs ADD COLUMN IF NOT EXISTS is_preset INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN plugin_docs.is_preset IS '1=系统内置预设，重置初始化时覆盖；0=管理员手动添加，重置时保留'"
    );

    if !done.contains("plugin_docs_is_preset_backfill_v1") {
        let slugs = crate::api::plugins::docs_api::builtin_doc_slugs();
        match sqlx::query(
            r#"UPDATE plugin_docs AS d
               SET is_preset = 1
               WHERE d.id IN (
                   SELECT MIN(id) FROM plugin_docs
                   WHERE slug = ANY($1) AND btrim(slug) <> ''
                   GROUP BY slug
               )"#,
        )
        .bind(&slugs)
        .execute(pool)
        .await
        {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('plugin_docs_is_preset_backfill_v1')",
                )
                .execute(pool)
                .await;
                done.insert("plugin_docs_is_preset_backfill_v1".into());
                tracing::info!("已按内置 slug 回填 plugin_docs.is_preset");
            }
            Err(e) => {
                tracing::warn!("plugin_docs_is_preset_backfill_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    once_migration!(
        pool,
        done,
        "mobile_app_token_unique_v1",
        r#"CREATE UNIQUE INDEX IF NOT EXISTS uidx_api_tokens_mobile_app
           ON api_tokens (user_id)
           WHERE only_playground_2026 = 1 AND name = '移动端APP'"#
    );

    // 图片类型二级功能属性：图生图改为多图文生图，并保证文生图 / 图文生图 / 多图文生图都在选项里。
    if !done.contains("image_type_multi_i2i_attrs_v1") {
        match rewrite_image_generation_attrs(pool).await {
            Ok(()) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('image_type_multi_i2i_attrs_v1')",
                )
                .execute(pool)
                .await;
                done.insert("image_type_multi_i2i_attrs_v1".into());
                tracing::info!("图片类型二级功能属性已改为文生图 / 图文生图 / 多图文生图");
            }
            Err(e) => {
                tracing::warn!("image_type_multi_i2i_attrs_v1 失败，未标记完成以便重试: {e}");
            }
        }
    }

    // 图像生成接口示例请求里的模型 ID 统一为 gpt-image-2。
    once_migration!(
        pool,
        done,
        "docs_image_example_model_gpt_image_2_v1",
        r#"UPDATE plugin_docs
           SET content = replace(
                 replace(content, '"model": "dall-e-3"', '"model": "gpt-image-2"'),
                 '"model": "kling-v3-image"',
                 '"model": "gpt-image-2"'
               ),
               updated_at = CURRENT_TIMESTAMP
           WHERE slug = 'images'"#,
        r#"UPDATE plugin_docs_intl
           SET content = replace(
                 replace(content, '"model": "dall-e-3"', '"model": "gpt-image-2"'),
                 '"model": "kling-v3-image"',
                 '"model": "gpt-image-2"'
               ),
               updated_at = CURRENT_TIMESTAMP
           WHERE doc_id IN (SELECT id FROM plugin_docs WHERE slug = 'images')"#
    );

    // 视频生成接口示例请求里的模型 ID 统一为 doubao-seedance-2-5-260628。
    once_migration!(
        pool,
        done,
        "docs_video_example_model_seedance_25_v1",
        r#"UPDATE plugin_docs
           SET content = replace(
                 replace(content, '"model": "kling-v3-omni"', '"model": "doubao-seedance-2-5-260628"'),
                 '"model": "doubao-seedance-2-0"',
                 '"model": "doubao-seedance-2-5-260628"'
               ),
               updated_at = CURRENT_TIMESTAMP
           WHERE slug = 'video'"#,
        r#"UPDATE plugin_docs_intl
           SET content = replace(
                 replace(content, '"model": "kling-v3-omni"', '"model": "doubao-seedance-2-5-260628"'),
                 '"model": "doubao-seedance-2-0"',
                 '"model": "doubao-seedance-2-5-260628"'
               ),
               updated_at = CURRENT_TIMESTAMP
           WHERE doc_id IN (SELECT id FROM plugin_docs WHERE slug = 'video')"#
    );

    // 用户端 API 教程：目录默认收起，仅「快速开始」默认展开，可在文档树里改。
    once_migration!(
        pool,
        done,
        "plugin_docs_default_expanded_v1",
        "ALTER TABLE plugin_docs ADD COLUMN IF NOT EXISTS default_expanded INTEGER NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN plugin_docs.default_expanded IS '1=用户端 API 教程默认展开该目录；0=默认收起。仅目录有效'",
        "UPDATE plugin_docs SET default_expanded = 1 WHERE is_dir = 1 AND slug = 'quickstart'"
    );

    // ── 站点门户增强版 栏目文章/Blog 初始化 ──
    once_migration!(
        pool,
        done,
        "site_portal_pro_articles_init_v1",
        r#"CREATE TABLE IF NOT EXISTS site_portal_pro_article_categories (
            id SERIAL PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            slug VARCHAR(100) DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS site_portal_pro_articles (
            id SERIAL PRIMARY KEY,
            category_id INTEGER NULL REFERENCES site_portal_pro_article_categories(id) ON DELETE SET NULL,
            title VARCHAR(255) NOT NULL,
            slug VARCHAR(255) DEFAULT '',
            cover_image TEXT DEFAULT '',
            summary TEXT DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            views INTEGER NOT NULL DEFAULT 0,
            author VARCHAR(100) DEFAULT 'Admin',
            created_at TEXT NOT NULL DEFAULT (now()::text),
            updated_at TEXT NOT NULL DEFAULT (now()::text)
        )"#,
        r#"INSERT INTO site_portal_pro_article_categories (name, slug, sort_order)
           SELECT v.name, v.slug, v.sort_order
           FROM (VALUES
             ('官方动态', 'news', 30),
             ('产品教程', 'tutorials', 20),
             ('行业资讯', 'insights', 10)
           ) AS v(name, slug, sort_order)
           WHERE NOT EXISTS (SELECT 1 FROM site_portal_pro_article_categories LIMIT 1)"#
    );

    // ── 站点门户增强版 栏目文章/Blog 支持栏目嵌套 parent_id ──
    once_migration!(
        pool,
        done,
        "site_portal_pro_article_categories_parent_v1",
        "ALTER TABLE site_portal_pro_article_categories ADD COLUMN IF NOT EXISTS parent_id INTEGER NULL REFERENCES site_portal_pro_article_categories(id) ON DELETE CASCADE"
    );

    // ── 站点门户增强版 栏目文章/Blog 支持发布选项、浏览权限与独立发布时间 ──
    once_migration!(
        pool,
        done,
        "site_portal_pro_articles_publish_fields_v1",
        "ALTER TABLE site_portal_pro_articles ADD COLUMN IF NOT EXISTS publish_option VARCHAR(50) NOT NULL DEFAULT 'generate_html', \
         ADD COLUMN IF NOT EXISTS view_permission VARCHAR(50) NOT NULL DEFAULT 'public', \
         ADD COLUMN IF NOT EXISTS publish_at TEXT NOT NULL DEFAULT (now()::text)"
    );

    Ok(())
}

const IMAGE_GENERATION_MODES: [&str; 3] = ["文生图", "图文生图", "多图文生图"];

fn map_image_generation_attr(attr: &str) -> &str {
    match attr {
        "图生图" => "多图文生图",
        "创建图片" => "文生图",
        "编辑图片" => "图文生图",
        other => other,
    }
}

fn image_generation_attr_list(raw: &str, ensure_modes: bool) -> Option<Vec<String>> {
    let text = raw.trim();
    let parsed: Vec<serde_json::Value> = if text.is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(text).ok()?
    };
    let mut seen = Vec::new();
    for value in parsed {
        let Some(item) = value.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
            continue;
        };
        let mapped = map_image_generation_attr(item);
        if !seen.iter().any(|x: &String| x == mapped) {
            seen.push(mapped.to_string());
        }
    }
    if ensure_modes {
        let rest: Vec<String> = seen
            .into_iter()
            .filter(|item| !IMAGE_GENERATION_MODES.contains(&item.as_str()))
            .collect();
        seen = IMAGE_GENERATION_MODES
            .iter()
            .map(|item| (*item).to_string())
            .chain(rest)
            .collect();
    }
    Some(seen)
}

async fn rewrite_image_generation_attrs(pool: &PgPool) -> anyhow::Result<()> {
    let types: Vec<(i64, Option<String>)> = sqlx::query_as(
        "SELECT id, default_features FROM model_types \
         WHERE position('图片' in name) > 0 AND position('视频' in name) = 0",
    )
    .fetch_all(pool)
    .await?;
    for (id, raw) in types {
        let current = raw.unwrap_or_else(|| "[]".to_string());
        let Some(next) = image_generation_attr_list(&current, true) else {
            continue;
        };
        let encoded = serde_json::to_string(&next)?;
        let Some(previous) = image_generation_attr_list(&current, false) else {
            continue;
        };
        if previous == next && serde_json::from_str::<Vec<String>>(&current).ok().as_ref() == Some(&next) {
            continue;
        }
        sqlx::query("UPDATE model_types SET default_features = $1 WHERE id = $2")
            .bind(&encoded)
            .bind(id)
            .execute(pool)
            .await?;
    }

    let models: Vec<(i64, String)> = sqlx::query_as(
        "SELECT m.id, m.feature_attributes FROM models m \
         JOIN model_types t ON t.id = m.type_id \
         WHERE position('图片' in t.name) > 0 AND position('视频' in t.name) = 0 \
           AND m.feature_attributes IS NOT NULL AND btrim(m.feature_attributes) <> ''",
    )
    .fetch_all(pool)
    .await?;
    for (id, current) in models {
        let Some(next) = image_generation_attr_list(&current, false) else {
            continue;
        };
        let Some(previous) = serde_json::from_str::<Vec<String>>(&current).ok() else {
            continue;
        };
        if previous == next {
            continue;
        }
        let encoded = serde_json::to_string(&next)?;
        sqlx::query("UPDATE models SET feature_attributes = $1 WHERE id = $2")
            .bind(&encoded)
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}
