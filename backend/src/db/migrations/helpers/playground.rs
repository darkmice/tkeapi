/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

pub(crate) async fn seed_playground_2026_quick_bar(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    use crate::api::plugins::manager::quick_bar_seed::{
        seed_imagine_quick_bar_overrides, seed_imagine_quick_bar_schemes,
    };

    let mut updated = 0u64;
    let schemes_raw: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
    )
    .fetch_optional(pool)
    .await?;

    let mut scheme_params_by_id: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    if let Some(raw) = schemes_raw {
        if let Ok(mut schemes) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) {
            let changed = seed_imagine_quick_bar_schemes(&mut schemes);
            for s in &schemes {
                if let (Some(id), Some(params)) = (
                    s.get("id").and_then(|v| v.as_str()),
                    s.get("params").and_then(|v| v.as_array()),
                ) {
                    scheme_params_by_id.insert(id.to_string(), params.clone());
                }
            }
            if changed {
                let next = serde_json::to_string(&schemes)?;
                sqlx::query(
                    "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
                     WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
                )
                .bind(&next)
                .execute(pool)
                .await?;
                updated += 1;
            }
        }
    }

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT config_key, config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' \
           AND (config_key LIKE 'pg_model_id_%' OR (config_key LIKE 'pg_model_%' AND config_key NOT LIKE 'pg_model_id_%'))",
    )
    .fetch_all(pool)
    .await?;

    for (key, val) in rows {
        let Ok(mut conf) = serde_json::from_str::<serde_json::Value>(&val) else {
            continue;
        };
        if !conf.get("param_overrides").is_some_and(|v| v.is_object()) {
            continue;
        }
        let scheme_id = conf.get("scheme_id").and_then(|v| v.as_str()).unwrap_or("");
        let scheme_params = scheme_params_by_id
            .get(scheme_id)
            .cloned()
            .unwrap_or_default();
        let changed = conf
            .get_mut("param_overrides")
            .is_some_and(|overrides| seed_imagine_quick_bar_overrides(overrides, &scheme_params));
        if !changed {
            continue;
        }
        let next = serde_json::to_string(&conf)?;
        sqlx::query(
            "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
             WHERE plugin_name = 'playground_2026' AND config_key = $2",
        )
        .bind(&next)
        .bind(&key)
        .execute(pool)
        .await?;
        updated += 1;
    }

    Ok(updated)
}

/// 创作中心2026：历史迁移占位（Seedream 5.0 不支持图层拆分）
pub(crate) async fn seed_playground_2026_layer_decomposition(
    _pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    Ok(0)
}

/// 创作中心2026：移除 Seedream 5.0 方案中的 layer_decomposition（图层拆分仅 Pro 支持）
pub(crate) async fn remove_playground_2026_seedream_5_0_layer_decomposition(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let mut updated = 0u64;
    let schemes_raw: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
    )
    .fetch_optional(pool)
    .await?;

    if let Some(raw) = schemes_raw {
        if let Ok(mut schemes) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) {
            let mut changed = false;
            for s in schemes.iter_mut() {
                if s.get("id").and_then(|v| v.as_str()) == Some("seedream_5_0") {
                    if let Some(params) = s.get_mut("params").and_then(|v| v.as_array_mut()) {
                        let len_before = params.len();
                        params.retain(|p| {
                            p.get("key").and_then(|v| v.as_str()) != Some("layer_decomposition")
                        });
                        if params.len() != len_before {
                            changed = true;
                        }
                    }
                }
            }
            if changed {
                let next = serde_json::to_string(&schemes)?;
                sqlx::query(
                    "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
                     WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
                )
                .bind(&next)
                .execute(pool)
                .await?;
                updated += 1;
            }
        }
    }

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT config_key, config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' \
           AND (config_key LIKE 'pg_model_id_%' OR (config_key LIKE 'pg_model_%' AND config_key NOT LIKE 'pg_model_id_%'))",
    )
    .fetch_all(pool)
    .await?;

    for (key, val) in rows {
        let Ok(mut conf) = serde_json::from_str::<serde_json::Value>(&val) else {
            continue;
        };
        let scheme_id = conf.get("scheme_id").and_then(|v| v.as_str()).unwrap_or("");
        if scheme_id != "seedream_5_0" {
            continue;
        }
        let Some(overrides) = conf
            .get_mut("param_overrides")
            .and_then(|v| v.as_object_mut())
        else {
            continue;
        };
        let mut model_changed = false;
        if let Some(modify) = overrides.get_mut("modify").and_then(|v| v.as_object_mut()) {
            if modify.remove("layer_decomposition").is_some() {
                model_changed = true;
            }
        }
        if let Some(remove) = overrides.get_mut("remove").and_then(|v| v.as_array_mut()) {
            let before_len = remove.len();
            remove.retain(|v| v.as_str() != Some("layer_decomposition"));
            if remove.len() != before_len {
                model_changed = true;
            }
        }
        if let Some(add) = overrides.get_mut("add").and_then(|v| v.as_array_mut()) {
            let before_len = add.len();
            add.retain(|p| p.get("key").and_then(|v| v.as_str()) != Some("layer_decomposition"));
            if add.len() != before_len {
                model_changed = true;
            }
        }
        if model_changed {
            let next = serde_json::to_string(&conf)?;
            sqlx::query(
                "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
                 WHERE plugin_name = 'playground_2026' AND config_key = $2",
            )
            .bind(&next)
            .bind(&key)
            .execute(pool)
            .await?;
            updated += 1;
        }
    }

    Ok(updated)
}

/// 创作中心：Seedance 2.0 方案时长默认改为 slider 滑块（min: 4, max: 15, step: 1, default: 5, unit: "秒"）
pub(crate) async fn seed_playground_seedance2_slider(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let mut updated = 0u64;
    for plugin_name in &["playground_2026", "playground"] {
        let schemes_raw: Option<String> = sqlx::query_scalar(
            "SELECT config_value FROM plugin_configs \
             WHERE plugin_name = $1 AND config_key = 'pg_schemes'",
        )
        .bind(plugin_name)
        .fetch_optional(pool)
        .await?;

        if let Some(raw) = schemes_raw {
            if let Ok(mut schemes) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) {
                let mut changed = false;
                for s in &mut schemes {
                    if s.get("id").and_then(|v| v.as_str()) == Some("seedance2.0") {
                        if let Some(params) = s.get_mut("params").and_then(|p| p.as_array_mut()) {
                            for p in params {
                                if p.get("key").and_then(|v| v.as_str()) == Some("duration") {
                                    if let Some(obj) = p.as_object_mut() {
                                        obj.insert("type".into(), serde_json::json!("slider"));
                                        obj.insert(
                                            "data_type".into(),
                                            serde_json::json!("integer"),
                                        );
                                        obj.insert("min".into(), serde_json::json!(4));
                                        obj.insert("max".into(), serde_json::json!(15));
                                        obj.insert("step".into(), serde_json::json!(1));
                                        obj.insert("default".into(), serde_json::json!(5));
                                        obj.insert("unit".into(), serde_json::json!("秒"));
                                        obj.remove("options");
                                        changed = true;
                                    }
                                }
                                if p.get("key").and_then(|v| v.as_str()) == Some("resolution") {
                                    if let Some(obj) = p.as_object_mut() {
                                        obj.insert("default".into(), serde_json::json!("480p"));
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                }
                if changed {
                    let next = serde_json::to_string(&schemes)?;
                    sqlx::query(
                        "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
                         WHERE plugin_name = $2 AND config_key = 'pg_schemes'",
                    )
                    .bind(&next)
                    .bind(plugin_name)
                    .execute(pool)
                    .await?;
                    updated += 1;
                }
            }
        }
    }
    Ok(updated)
}

/// 创作中心：Seedance 2.0 方案删除反向提示词（不显示且不支持）
pub(crate) async fn strip_playground_seedance2_negative_prompt(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let mut updated = 0u64;
    for plugin_name in &["playground_2026", "playground"] {
        let schemes_raw: Option<String> = sqlx::query_scalar(
            "SELECT config_value FROM plugin_configs \
             WHERE plugin_name = $1 AND config_key = 'pg_schemes'",
        )
        .bind(plugin_name)
        .fetch_optional(pool)
        .await?;

        if let Some(raw) = schemes_raw {
            if let Ok(mut schemes) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) {
                let mut changed = false;
                for s in &mut schemes {
                    if s.get("id").and_then(|v| v.as_str()) == Some("seedance2.0") {
                        if let Some(inputs) = s.get_mut("inputs").and_then(|p| p.as_array_mut()) {
                            let len_before = inputs.len();
                            inputs.retain(|p| {
                                let key = p.get("key").and_then(|v| v.as_str());
                                let bind_key = p.get("bind_key").and_then(|v| v.as_str());
                                key != Some("negative_prompt")
                                    && bind_key != Some("negative_prompt")
                            });
                            if inputs.len() != len_before {
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
                    let next = serde_json::to_string(&schemes)?;
                    sqlx::query(
                        "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
                         WHERE plugin_name = $2 AND config_key = 'pg_schemes'",
                    )
                    .bind(&next)
                    .bind(plugin_name)
                    .execute(pool)
                    .await?;
                    updated += 1;
                }
            }
        }
    }
    Ok(updated)
}

/// 创作中心2026：四个 Seedream 图片方案写入火山文档「图像输出尺寸」
pub(crate) async fn seed_playground_2026_seedream_pro_image_special(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    use crate::api::plugins::manager::seedream_5_0_pro_seed::seed_seedream_official_image_special;

    let schemes_raw: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
    )
    .fetch_optional(pool)
    .await?;

    let Some(raw) = schemes_raw else {
        return Ok(0);
    };
    let mut schemes: Vec<serde_json::Value> = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(0),
    };
    if !seed_seedream_official_image_special(&mut schemes) {
        return Ok(0);
    }
    let next = serde_json::to_string(&schemes)?;
    sqlx::query(
        "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
         WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
    )
    .bind(&next)
    .execute(pool)
    .await?;
    Ok(1)
}

/// 创作中心2026：gpt-image-2 模型属性中文对照与描述菜单本地化同步
pub(crate) async fn seed_playground_2026_gpt_image_2_localization(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    use crate::api::plugins::manager::gpt_image_2_seed::seed_gpt_image_2_scheme;

    let schemes_raw: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
    )
    .fetch_optional(pool)
    .await?;

    let Some(raw) = schemes_raw else {
        return Ok(0);
    };
    let mut schemes: Vec<serde_json::Value> = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(0),
    };
    if !seed_gpt_image_2_scheme(&mut schemes) {
        return Ok(0);
    }
    let next = serde_json::to_string(&schemes)?;
    sqlx::query(
        "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
         WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
    )
    .bind(&next)
    .execute(pool)
    .await?;
    Ok(1)
}

#[cfg(feature = "commercial_plugins")]
fn push_feature_attr(raw: Option<&str>, plugin_keys: &[String], label: &str) -> (String, bool) {
    use crate::api::plugins::playground_2026::model_features::{
        attributes_from_keys, parse_attr_list,
    };
    let mut attrs = parse_attr_list(raw);
    if attrs.is_empty() {
        attrs = attributes_from_keys(plugin_keys);
    }
    if attrs.iter().any(|item| item == label) {
        return (
            serde_json::to_string(&attrs).unwrap_or_else(|_| "[]".to_string()),
            false,
        );
    }
    attrs.push(label.to_string());
    (
        serde_json::to_string(&attrs).unwrap_or_else(|_| "[]".to_string()),
        true,
    )
}

fn plugin_feature_keys(cfg: &serde_json::Value) -> Vec<String> {
    cfg.get("feature_keys")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 文档写明 edits 可选 mask 的模型号。渠道别名（-pro / -azure / -official / -xq）不算。
fn is_documented_mask_model(model_id: &str, alias: &str) -> bool {
    const IDS: &[&str] = &["gpt-image-2", "gpt-image-2-2026-04-21"];
    let id = model_id.trim().to_ascii_lowercase();
    let al = alias.trim().to_ascii_lowercase();
    IDS.iter()
        .any(|item| *item == id || (!al.is_empty() && *item == al))
}

/// 创作中心2026：图片类型增加「蒙版」「坐标框」。模型号恰好是 gpt-image-2 的勾蒙版并开启；Seedream 5.0 Pro 勾坐标框。
pub(crate) async fn seed_playground_2026_mark_capabilities(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    #[cfg(feature = "commercial_plugins")]
    {
        return seed_playground_2026_mark_capabilities_inner(pool).await;
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = pool;
        Ok(0)
    }
}

#[cfg(feature = "commercial_plugins")]
async fn seed_playground_2026_mark_capabilities_inner(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    use crate::api::plugins::manager::seedream_5_0_pro_seed::is_seedream_5_0_pro_model;
    use crate::api::plugins::playground_2026::model_features::parse_attr_list;

    let mut updated = 0u64;
    let type_rows: Vec<(i64, Option<String>)> =
        sqlx::query_as("SELECT id, default_features FROM model_types WHERE name = '图片'")
            .fetch_all(pool)
            .await?;
    for (id, raw) in type_rows {
        let mut attrs = parse_attr_list(raw.as_deref());
        let mut changed = false;
        for label in ["蒙版", "坐标框"] {
            if !attrs.iter().any(|item| item == label) {
                attrs.push(label.to_string());
                changed = true;
            }
        }
        if !changed {
            continue;
        }
        let next = serde_json::to_string(&attrs)?;
        sqlx::query("UPDATE model_types SET default_features = $1 WHERE id = $2")
            .bind(&next)
            .bind(id)
            .execute(pool)
            .await?;
        updated += 1;
    }

    let models: Vec<(i64, String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT m.id, m.model_id, COALESCE(m.model_id_alias, ''), m.name, \
                m.feature_attributes, pc.config_value \
         FROM models m \
         JOIN model_types t ON t.id = m.type_id \
         LEFT JOIN plugin_configs pc \
           ON pc.plugin_name = 'playground_2026' \
          AND pc.config_key = 'pg_model_id_' || m.id::text \
         WHERE t.name = '图片'",
        )
        .fetch_all(pool)
        .await?;

    for (id, model_id, alias, name, attrs_raw, cfg_raw) in models {
        let cfg = cfg_raw
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .filter(|v| v.is_object())
            .unwrap_or_else(|| serde_json::json!({}));
        let scheme_id = cfg
            .get("scheme_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let keys = plugin_feature_keys(&cfg);
        let gpt = is_documented_mask_model(&model_id, &alias);
        let seedream = scheme_id == "seedream_5_0_pro"
            || is_seedream_5_0_pro_model(&model_id, &name)
            || is_seedream_5_0_pro_model(&alias, &name);
        if !gpt && !seedream {
            continue;
        }
        let mut next_cfg = cfg;
        let mut cfg_changed = false;
        if gpt {
            if next_cfg.get("enabled").and_then(|v| v.as_bool()) != Some(true) {
                next_cfg["enabled"] = serde_json::json!(true);
                cfg_changed = true;
            }
            if scheme_id.is_empty() {
                next_cfg["scheme_id"] = serde_json::json!("gpt-image-2");
                cfg_changed = true;
            }
        }
        let mut raw = attrs_raw.clone();
        let mut attr_changed = false;
        if seedream {
            let (next, changed) = push_feature_attr(raw.as_deref(), &keys, "坐标框");
            if changed {
                raw = Some(next);
                attr_changed = true;
            }
        }
        if gpt {
            let (next, changed) = push_feature_attr(raw.as_deref(), &keys, "蒙版");
            if changed {
                raw = Some(next);
                attr_changed = true;
            }
        }
        if attr_changed {
            if let Some(next_attrs) = raw {
                sqlx::query("UPDATE models SET feature_attributes = $1 WHERE id = $2")
                    .bind(&next_attrs)
                    .bind(id)
                    .execute(pool)
                    .await?;
                updated += 1;
            }
        }
        if gpt && (cfg_changed || cfg_raw.is_none()) {
            let payload = next_cfg.to_string();
            let key = format!("pg_model_id_{id}");
            sqlx::query(
                "INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at) \
                 VALUES ('playground_2026', $1, $2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
                 ON CONFLICT (plugin_name, config_key) DO UPDATE \
                 SET config_value = EXCLUDED.config_value, updated_at = CURRENT_TIMESTAMP",
            )
            .bind(&key)
            .bind(&payload)
            .execute(pool)
            .await?;
            updated += 1;
        }
    }
    Ok(updated)
}

/// 去掉渠道别名上误勾的蒙版。只保留文档模型号 gpt-image-2。
pub(crate) async fn restrict_playground_2026_mask_models(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    #[cfg(feature = "commercial_plugins")]
    {
        return restrict_playground_2026_mask_models_inner(pool).await;
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = pool;
        Ok(0)
    }
}

#[cfg(feature = "commercial_plugins")]
async fn restrict_playground_2026_mask_models_inner(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    use crate::api::plugins::playground_2026::model_features::parse_attr_list;

    let models: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT m.id, m.model_id, COALESCE(m.model_id_alias, ''), m.feature_attributes \
         FROM models m \
         JOIN model_types t ON t.id = m.type_id \
         WHERE t.name = '图片'",
    )
    .fetch_all(pool)
    .await?;
    let mut updated = 0u64;
    for (id, model_id, alias, raw) in models {
        if is_documented_mask_model(&model_id, &alias) {
            continue;
        }
        let mut attrs = parse_attr_list(raw.as_deref());
        if !attrs.iter().any(|item| item == "蒙版") {
            continue;
        }
        attrs.retain(|item| item != "蒙版");
        let next = serde_json::to_string(&attrs)?;
        sqlx::query("UPDATE models SET feature_attributes = $1 WHERE id = $2")
            .bind(&next)
            .bind(id)
            .execute(pool)
            .await?;
        updated += 1;
    }
    Ok(updated)
}
