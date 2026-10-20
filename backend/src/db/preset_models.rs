/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 系统预设模型：写入「模型库」目录；上架后才进入 `models`。
//! 目录更新只改 `model_library`，不覆盖已上架模型。

use sqlx::PgPool;

#[derive(Debug, Clone, Copy)]
pub struct PresetModelDef {
    pub mid: &'static str,
    pub name: &'static str,
    pub model_id: &'static str,
    pub original_id: &'static str,
    pub model_id_alias: &'static str,
    pub provider_name: &'static str,
    pub type_name: &'static str,
    pub billing_rule_name: &'static str,
    pub forward_rule_name: &'static str,
    pub logo: &'static str,
    pub pre_deduction: f64,
    pub feature_attributes: &'static str,
    pub site_discount: f64,
    pub site_discount_enabled: i64,
    pub global_discount: f64,
    pub global_discount_enabled: i64,
    pub is_active: i64,
    pub enable_log_content: i64,
    pub remark: &'static str,
    pub description: &'static str,
}

/// 与系统内置官方计费 / 转发规则名称一一对应；新增条目须新开迁移，勿改已执行过的种子。
pub fn preset_model_catalog() -> &'static [PresetModelDef] {
    &[
        PresetModelDef {
            mid: "310001",
            name: "Doubao Seedance 2.0",
            model_id: "doubao-seedance-2-0-260128",
            original_id: "Doubao Seedance 预设",
            model_id_alias: "",
            provider_name: "火山引擎",
            type_name: "视频",
            billing_rule_name: "Seedance2.0官方计费",
            forward_rule_name: "火山方舟 视频生成",
            logo: "doubao",
            pre_deduction: 5.0,
            feature_attributes: "[]",
            site_discount: 1.0,
            site_discount_enabled: 1,
            global_discount: 1.0,
            global_discount_enabled: 1,
            is_active: 1,
            enable_log_content: 0,
            remark: "",
            description: "Doubao Seedance 2.0",
        },
        PresetModelDef {
            mid: "310002",
            name: "Doubao Seedance 2.0 Fast",
            model_id: "doubao-seedance-2-0-fast",
            original_id: "Doubao Seedance 预设",
            model_id_alias: "",
            provider_name: "火山引擎",
            type_name: "视频",
            billing_rule_name: "Seedance2.0Fast官方计费",
            forward_rule_name: "火山方舟 视频生成",
            logo: "doubao",
            pre_deduction: 5.0,
            feature_attributes: "[]",
            site_discount: 1.0,
            site_discount_enabled: 1,
            global_discount: 1.0,
            global_discount_enabled: 1,
            is_active: 1,
            enable_log_content: 1,
            remark: "",
            description: "Doubao Seedance 2.0 Fast",
        },
        PresetModelDef {
            mid: "310003",
            name: "Doubao Seedance 2.5",
            model_id: "doubao-seedance-2-5-260628",
            original_id: "Doubao Seedance 预设",
            model_id_alias: "",
            provider_name: "火山引擎",
            type_name: "视频",
            billing_rule_name: "Seedance2.5官方计费",
            forward_rule_name: "火山方舟 视频生成",
            logo: "doubao",
            pre_deduction: 5.0,
            feature_attributes: "[]",
            site_discount: 1.0,
            site_discount_enabled: 1,
            global_discount: 1.0,
            global_discount_enabled: 1,
            is_active: 1,
            enable_log_content: 1,
            remark: "",
            description: "Doubao Seedance 2.5",
        },
        PresetModelDef {
            mid: "310004",
            name: "Doubao Seedance 2.0 Mini",
            model_id: "doubao-seedance-2-0-mini",
            original_id: "Doubao Seedance 预设",
            model_id_alias: "",
            provider_name: "火山引擎",
            type_name: "视频",
            billing_rule_name: "Seedance2.0mini官方计费",
            forward_rule_name: "火山方舟 视频生成",
            logo: "doubao",
            pre_deduction: 5.0,
            feature_attributes: "[]",
            site_discount: 1.0,
            site_discount_enabled: 1,
            global_discount: 1.0,
            global_discount_enabled: 1,
            is_active: 1,
            enable_log_content: 1,
            remark: "",
            description: "Doubao Seedance 2.0 Mini",
        },
    ]
}

async fn lookup_id(pool: &PgPool, sql: &str, name: &str) -> Option<i64> {
    sqlx::query_scalar::<_, i64>(sql)
        .bind(name)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

async fn model_library_ready(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'model_library')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

async fn resolve_catalog_ids(
    pool: &PgPool,
    def: &PresetModelDef,
) -> Option<(Option<i64>, Option<i64>, i64, i64)> {
    let provider_id = lookup_id(
        pool,
        "SELECT id FROM model_providers WHERE name = $1 LIMIT 1",
        def.provider_name,
    )
    .await;
    let type_id = lookup_id(
        pool,
        "SELECT id FROM model_types WHERE name = $1 LIMIT 1",
        def.type_name,
    )
    .await;
    let billing_rule_id = lookup_id(
        pool,
        "SELECT id FROM billing_rules WHERE name = $1 LIMIT 1",
        def.billing_rule_name,
    )
    .await;
    let forward_rule_id = lookup_id(
        pool,
        "SELECT id FROM forward_rules WHERE name = $1 LIMIT 1",
        def.forward_rule_name,
    )
    .await;

    let Some(billing_rule_id) = billing_rule_id else {
        tracing::warn!(
            "系统预设模型 {} 跳过：找不到计费规则 {}",
            def.mid,
            def.billing_rule_name
        );
        return None;
    };
    let Some(forward_rule_id) = forward_rule_id else {
        tracing::warn!(
            "系统预设模型 {} 跳过：找不到转发规则 {}",
            def.mid,
            def.forward_rule_name
        );
        return None;
    };
    Some((provider_id, type_id, billing_rule_id, forward_rule_id))
}

/// 写入系统预置目录。`model_library` 就绪后只更新模型库，绝不改 `models`。
pub async fn seed_system_preset_models(pool: &PgPool) -> anyhow::Result<u32> {
    if model_library_ready(pool).await {
        return upsert_model_library(pool).await;
    }
    seed_legacy_models_table(pool).await
}

async fn seed_legacy_models_table(pool: &PgPool) -> anyhow::Result<u32> {
    let mut inserted = 0u32;
    for def in preset_model_catalog() {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM models WHERE mid = $1 LIMIT 1")
                .bind(def.mid)
                .fetch_optional(pool)
                .await?;
        if exists.is_some() {
            continue;
        }
        let Some((provider_id, type_id, billing_rule_id, forward_rule_id)) =
            resolve_catalog_ids(pool, def).await
        else {
            continue;
        };
        let forward_ids = format!("[{forward_rule_id}]");
        let result = sqlx::query(
            r#"INSERT INTO models (
                    mid, name, model_id, original_id, model_id_alias,
                    provider_id, type_id, group_ratios, forward_rule_ids, billing_rule_id,
                    pre_deduction, site_discount, site_discount_enabled,
                    global_discount, global_discount_enabled,
                    is_active, enable_log_content, is_system, logo, remark, description,
                    feature_attributes, created_at, updated_at
               )
               SELECT $1, $2, $3, $4, $5, $6, $7, '{"default":1.0}', $8, $9,
                      $10, $11, $12, $13, $14,
                      $15, $16, 1, $17,
                      $18,
                      $19, $20, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
               WHERE NOT EXISTS (SELECT 1 FROM models WHERE mid = $1)"#,
        )
        .bind(def.mid)
        .bind(def.name)
        .bind(def.model_id)
        .bind(def.original_id)
        .bind(def.model_id_alias)
        .bind(provider_id)
        .bind(type_id)
        .bind(&forward_ids)
        .bind(billing_rule_id)
        .bind(def.pre_deduction)
        .bind(def.site_discount)
        .bind(def.site_discount_enabled)
        .bind(def.global_discount)
        .bind(def.global_discount_enabled)
        .bind(def.is_active)
        .bind(def.enable_log_content)
        .bind(def.logo)
        .bind(def.remark)
        .bind(def.description)
        .bind(def.feature_attributes)
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            inserted += 1;
        }
    }
    Ok(inserted)
}

async fn upsert_model_library(pool: &PgPool) -> anyhow::Result<u32> {
    let mut written = 0u32;
    for def in preset_model_catalog() {
        let Some((provider_id, type_id, billing_rule_id, forward_rule_id)) =
            resolve_catalog_ids(pool, def).await
        else {
            continue;
        };
        let forward_ids = format!("[{forward_rule_id}]");
        let result = sqlx::query(
            r#"INSERT INTO model_library (
                    mid, name, model_id, original_id, model_id_alias,
                    provider_id, type_id, group_ratios, forward_rule_ids, billing_rule_id,
                    pre_deduction, site_discount, site_discount_enabled,
                    global_discount, global_discount_enabled,
                    is_active, enable_log_content, is_system, logo, remark, description,
                    feature_attributes, created_at, updated_at
               )
               VALUES (
                    $1, $2, $3, $4, $5, $6, $7, '{"default":1.0}', $8, $9,
                    $10, $11, $12, $13, $14,
                    $15, $16, 1, $17, $18, $19, $20, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
               )
               ON CONFLICT (mid) DO UPDATE SET
                    name = EXCLUDED.name,
                    model_id = EXCLUDED.model_id,
                    original_id = EXCLUDED.original_id,
                    model_id_alias = EXCLUDED.model_id_alias,
                    provider_id = EXCLUDED.provider_id,
                    type_id = EXCLUDED.type_id,
                    group_ratios = EXCLUDED.group_ratios,
                    forward_rule_ids = EXCLUDED.forward_rule_ids,
                    billing_rule_id = EXCLUDED.billing_rule_id,
                    pre_deduction = EXCLUDED.pre_deduction,
                    site_discount = EXCLUDED.site_discount,
                    site_discount_enabled = EXCLUDED.site_discount_enabled,
                    global_discount = EXCLUDED.global_discount,
                    global_discount_enabled = EXCLUDED.global_discount_enabled,
                    enable_log_content = EXCLUDED.enable_log_content,
                    logo = EXCLUDED.logo,
                    remark = EXCLUDED.remark,
                    description = EXCLUDED.description,
                    feature_attributes = EXCLUDED.feature_attributes,
                    updated_at = CURRENT_TIMESTAMP"#,
        )
        .bind(def.mid)
        .bind(def.name)
        .bind(def.model_id)
        .bind(def.original_id)
        .bind(def.model_id_alias)
        .bind(provider_id)
        .bind(type_id)
        .bind(&forward_ids)
        .bind(billing_rule_id)
        .bind(def.pre_deduction)
        .bind(def.site_discount)
        .bind(def.site_discount_enabled)
        .bind(def.global_discount)
        .bind(def.global_discount_enabled)
        .bind(def.is_active)
        .bind(def.enable_log_content)
        .bind(def.logo)
        .bind(def.remark)
        .bind(def.description)
        .bind(def.feature_attributes)
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            written += 1;
        }
    }
    Ok(written)
}

/// 把系统预设复制进模型库，并从运营列表拿掉。默认安装后运营列表为空。
pub async fn split_listed_and_library(pool: &PgPool) -> anyhow::Result<u32> {
    let copied = sqlx::query(
        r#"INSERT INTO model_library (
                mid, name, model_id, original_id, model_id_alias,
                provider_id, api_provider_id, type_id, group_ratios, billing_rule_id,
                pre_deduction, is_active, forward_rule_ids, enable_log_content,
                site_discount, site_discount_enabled, global_discount, global_discount_enabled,
                logo, remark, description, feature_attributes, is_system, sort_order,
                created_at, updated_at
           )
           SELECT
                mid, name, model_id, original_id, model_id_alias,
                provider_id, api_provider_id, type_id, group_ratios, billing_rule_id,
                pre_deduction, is_active, forward_rule_ids, enable_log_content,
                site_discount, site_discount_enabled, global_discount, global_discount_enabled,
                logo, remark, description, feature_attributes, is_system, sort_order,
                created_at, updated_at
           FROM models
           WHERE is_system = 1 AND mid IS NOT NULL AND mid <> ''
           ON CONFLICT (mid) DO NOTHING"#,
    )
    .execute(pool)
    .await?
    .rows_affected() as u32;

    sqlx::query("DELETE FROM models WHERE is_system = 1 AND mid IS NOT NULL AND mid <> ''")
        .execute(pool)
        .await?;
    sqlx::query("UPDATE models SET is_system = 0 WHERE is_system = 1")
        .execute(pool)
        .await?;

    let seeded = upsert_model_library(pool).await?;
    tracing::info!(
        "模型库拆分完成：从已上架复制 {} 条，目录写入/更新 {} 条",
        copied,
        seeded
    );
    Ok(copied)
}

/// `source=library|system` 读模型库，其余读上架模型。表名仅允许白名单。
pub fn is_library_source(source: Option<&str>) -> bool {
    matches!(source, Some("system") | Some("library"))
}

#[allow(dead_code)]
pub fn models_table(source: Option<&str>) -> &'static str {
    if is_library_source(source) {
        "model_library"
    } else {
        "models"
    }
}

/// 分类统计 JOIN 源：模型库只读预置目录。
pub fn models_join_relation(source: Option<&str>) -> &'static str {
    if is_library_source(source) {
        "model_library"
    } else {
        "models"
    }
}

/// 仓库目录和运营/下架副本只留一处。副本用 mid、library_mid 或 model_id 对上目录行时，目录行让位。
pub const LIBRARY_CATALOG_FREE: &str = "NOT EXISTS (\
    SELECT 1 FROM models shelf \
    WHERE (shelf.library_mid <> '' AND shelf.library_mid = m.mid) \
       OR shelf.mid = m.mid \
       OR (m.model_id <> '' AND shelf.model_id = m.model_id) \
)";

pub fn library_catalog_free_predicate(source: Option<&str>) -> Option<&'static str> {
    if is_library_source(source) {
        Some(LIBRARY_CATALOG_FREE)
    } else {
        None
    }
}

/// 上架列表 JOIN 额外条件；模型库视图不加。
pub fn listed_join_predicate(source: Option<&str>) -> Option<&'static str> {
    if is_library_source(source) {
        None
    } else {
        Some("m.is_listed = 1")
    }
}

pub const LISTED_ONLY: &str = " AND is_listed = 1";
#[allow(dead_code)]
pub const LISTED_ONLY_M: &str = " AND m.is_listed = 1";

/// `model_library` 没有 `library_mid`/`is_listed`，显式列出以免 SELECT * 映射失败。
pub const LIBRARY_MODEL_COLUMNS: &str = "id, mid, name, model_id, original_id, model_id_alias, \
    provider_id, api_provider_id, type_id, group_ratios, billing_rule_id, \
    pre_deduction, is_active, forward_rule_ids, enable_log_content, \
    site_discount, site_discount_enabled, global_discount, global_discount_enabled, \
    discount_schedule, logo, remark, description, feature_attributes, is_system, \
    NULL::text AS library_mid, 0::bigint AS is_listed, sort_order, created_at, updated_at, \
    NULL::text AS type_name";
