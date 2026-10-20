/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026 聊天业务一次性数据变更。
//! 由迁移 `playground_2026_chat_business_init_v1` 调用一次：
//! 方案（注入/去掉通用聊天方案）→ 启用聊天模型 → 绑定方案 → 特性 + 输入协议 + 思考预设。

use crate::api::plugins::manager::doubao_seed_evolving_seed::{
    seed_doubao_seed_evolving_scheme, DOUBAO_SEED_EVOLVING_SCHEME_ID,
};
use crate::api::plugins::manager::glm_deepseek_chat_seed::{
    seed_glm_deepseek_chat_schemes, DEEPSEEK_V4_PRO_SCHEME_ID, GLM_5_2_SCHEME_ID,
    GLM_5_3_SCHEME_ID, QWEN_3_7_MAX_SCHEME_ID,
};
use crate::api::plugins::manager::gpt_6_astra_chat_seed::seed_gpt_6_astra_chat_scheme;
use serde_json::{json, Value};

const DROPPED_CHAT_SCHEME_IDS: [&str; 3] = ["chat_standard", "chat_creative", "chat_precise"];

fn compact_id(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn is_chat_llm_type(type_name: &str) -> bool {
    type_name.contains("聊天") || type_name.contains("对话")
}

fn scheme_for(name: &str, model_id: &str, type_name: &str) -> Option<&'static str> {
    if !is_chat_llm_type(type_name) {
        return None;
    }
    let blob = compact_id(&format!("{name}{model_id}"));
    if blob.contains("glm53") {
        return Some(GLM_5_3_SCHEME_ID);
    }
    if blob.contains("glm") {
        return Some(GLM_5_2_SCHEME_ID);
    }
    if blob.contains("deepseek") {
        return Some(DEEPSEEK_V4_PRO_SCHEME_ID);
    }
    if blob.contains("qwen") {
        return Some(QWEN_3_7_MAX_SCHEME_ID);
    }
    if blob.contains("doubao") {
        return Some(DOUBAO_SEED_EVOLVING_SCHEME_ID);
    }
    None
}

fn chat_feature_attrs(name: &str, model_id: &str, type_name: &str) -> Option<Vec<&'static str>> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::chat_protocol::suggest_feature_labels(
            name, model_id, type_name,
        )
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = (name, model_id, type_name);
        None
    }
}

fn chat_input_protocol(name: &str, model_id: &str, type_name: &str) -> Option<&'static str> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::chat_protocol::suggest_input_protocol(
            name, model_id, type_name,
        )
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = (name, model_id, type_name);
        None
    }
}

fn chat_thinking_profile(name: &str, model_id: &str, type_name: &str) -> Option<&'static str> {
    #[cfg(feature = "commercial_plugins")]
    {
        crate::api::plugins::playground_2026::chat_protocol::suggest_thinking_profile(
            name, model_id, type_name,
        )
    }
    #[cfg(not(feature = "commercial_plugins"))]
    {
        let _ = (name, model_id, type_name);
        None
    }
}

fn attrs_to_feature_keys(attrs: &[&str]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|a| match *a {
            "理解图片" => Some("understand-image".to_string()),
            "理解音频" => Some("understand-audio".to_string()),
            "理解视频" => Some("understand-video".to_string()),
            "理解文件" => Some("understand-file".to_string()),
            _ => None,
        })
        .collect()
}

async fn persist_pg_schemes(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<u64> {
    let schemes_raw: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
    )
    .fetch_optional(pool)
    .await?;
    let Some(raw) = schemes_raw else {
        return Ok(0);
    };
    let Ok(mut schemes) = serde_json::from_str::<Vec<Value>>(&raw) else {
        return Ok(0);
    };
    if schemes.is_empty() {
        seed_doubao_seed_evolving_scheme(&mut schemes);
        seed_glm_deepseek_chat_schemes(&mut schemes);
        seed_gpt_6_astra_chat_scheme(&mut schemes);
    } else {
        schemes.retain(|s| {
            let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("");
            !DROPPED_CHAT_SCHEME_IDS.contains(&id)
        });
        seed_doubao_seed_evolving_scheme(&mut schemes);
        seed_glm_deepseek_chat_schemes(&mut schemes);
        seed_gpt_6_astra_chat_scheme(&mut schemes);
    }
    let next = serde_json::to_string(&schemes)?;
    if next == raw {
        return Ok(0);
    }
    sqlx::query(
        "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
         WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
    )
    .bind(&next)
    .execute(pool)
    .await?;
    Ok(1)
}

async fn upsert_model_config(
    pool: &sqlx::Pool<sqlx::Postgres>,
    row_id: i64,
    scheme_id: &str,
    feature_keys: Option<&[String]>,
    input_protocol: Option<&str>,
    thinking_profile: Option<&str>,
) -> anyhow::Result<bool> {
    let key = format!("pg_model_id_{row_id}");
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = $1",
    )
    .bind(&key)
    .fetch_optional(pool)
    .await?;
    let mut conf = existing
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| json!({"enabled": false, "scheme_id": null, "sort_order": 0}));
    let keys_match = match feature_keys {
        Some(keys) => {
            conf.get("feature_keys")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    let got: Vec<String> = arr
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect();
                    got == keys
                })
                == Some(true)
        }
        None => true,
    };
    let protocol_match = match input_protocol {
        Some(p) => conf.get("input_protocol").and_then(|v| v.as_str()) == Some(p),
        None => true,
    };
    let thinking_match = match thinking_profile {
        Some(p) => conf.get("thinking_profile").and_then(|v| v.as_str()) == Some(p),
        None => true,
    };
    let already_on = conf.get("enabled").and_then(|v| v.as_bool()) == Some(true)
        && conf.get("scheme_id").and_then(|v| v.as_str()) == Some(scheme_id)
        && keys_match
        && protocol_match
        && thinking_match;
    if already_on {
        return Ok(false);
    }
    if let Some(obj) = conf.as_object_mut() {
        obj.insert("enabled".into(), json!(true));
        obj.insert("scheme_id".into(), json!(scheme_id));
        if let Some(keys) = feature_keys {
            obj.insert("feature_keys".into(), json!(keys));
        }
        if let Some(p) = input_protocol {
            obj.insert("input_protocol".into(), json!(p));
        }
        if let Some(p) = thinking_profile {
            obj.insert("thinking_profile".into(), json!(p));
        }
    }
    let next = serde_json::to_string(&conf)?;
    sqlx::query(
        "INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at) \
         VALUES ('playground_2026', $1, $2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
         ON CONFLICT (plugin_name, config_key) DO UPDATE \
         SET config_value = EXCLUDED.config_value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(&key)
    .bind(&next)
    .execute(pool)
    .await?;
    Ok(true)
}

async fn write_model_feature_attrs(
    pool: &sqlx::Pool<sqlx::Postgres>,
    row_id: i64,
    attrs: &[&str],
) -> anyhow::Result<bool> {
    let next = serde_json::to_string(&attrs)?;
    let res = sqlx::query(
        "UPDATE models SET feature_attributes = $1, updated_at = CURRENT_TIMESTAMP \
         WHERE id = $2 AND COALESCE(feature_attributes, '[]') IS DISTINCT FROM $1",
    )
    .bind(&next)
    .bind(row_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

async fn clear_dropped_scheme_bindings(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<u64> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT config_key, config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' \
           AND config_key LIKE 'pg_model_id_%' \
           AND config_value ~ '^\\s*\\{'",
    )
    .fetch_all(pool)
    .await?;
    let mut updated = 0u64;
    for (key, val) in rows {
        let Ok(mut conf) = serde_json::from_str::<Value>(&val) else {
            continue;
        };
        let scheme_id = conf.get("scheme_id").and_then(|v| v.as_str()).unwrap_or("");
        if !DROPPED_CHAT_SCHEME_IDS.contains(&scheme_id) {
            continue;
        }
        if let Some(obj) = conf.as_object_mut() {
            obj.insert("scheme_id".into(), Value::Null);
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

async fn patch_feature_keys_only(
    pool: &sqlx::Pool<sqlx::Postgres>,
    row_id: i64,
    feature_keys: &[String],
    input_protocol: Option<&str>,
    thinking_profile: Option<&str>,
) -> anyhow::Result<bool> {
    let key = format!("pg_model_id_{row_id}");
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = $1",
    )
    .bind(&key)
    .fetch_optional(pool)
    .await?;
    let mut conf = existing
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| json!({"enabled": false, "scheme_id": null, "sort_order": 0}));
    let already = conf
        .get("feature_keys")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let got: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
            got == feature_keys
        })
        == Some(true)
        && match input_protocol {
            Some(p) => conf.get("input_protocol").and_then(|v| v.as_str()) == Some(p),
            None => true,
        }
        && match thinking_profile {
            Some(p) => conf.get("thinking_profile").and_then(|v| v.as_str()) == Some(p),
            None => true,
        };
    if already {
        return Ok(false);
    }
    if let Some(obj) = conf.as_object_mut() {
        obj.insert("feature_keys".into(), json!(feature_keys));
        if let Some(p) = input_protocol {
            obj.insert("input_protocol".into(), json!(p));
        }
        if let Some(p) = thinking_profile {
            obj.insert("thinking_profile".into(), json!(p));
        }
    }
    let next = serde_json::to_string(&conf)?;
    sqlx::query(
        "INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at) \
         VALUES ('playground_2026', $1, $2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
         ON CONFLICT (plugin_name, config_key) DO UPDATE \
         SET config_value = EXCLUDED.config_value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(&key)
    .bind(&next)
    .execute(pool)
    .await?;
    Ok(true)
}

async fn bind_chat_models_and_attrs(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<u64> {
    let mut updated = 0u64;
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT m.id, m.name, m.model_id, COALESCE(mt.name, '') \
         FROM models m \
         LEFT JOIN model_types mt ON mt.id = m.type_id",
    )
    .fetch_all(pool)
    .await?;
    for (id, name, model_id, type_name) in rows {
        let keys = chat_feature_attrs(&name, &model_id, &type_name)
            .map(|attrs| attrs_to_feature_keys(&attrs));
        let input_protocol = chat_input_protocol(&name, &model_id, &type_name);
        let thinking_profile = chat_thinking_profile(&name, &model_id, &type_name);
        if let Some(scheme_id) = scheme_for(&name, &model_id, &type_name) {
            if upsert_model_config(
                pool,
                id,
                scheme_id,
                keys.as_deref(),
                input_protocol,
                thinking_profile,
            )
            .await?
            {
                updated += 1;
            }
        } else if let Some(ref keys) = keys {
            if patch_feature_keys_only(pool, id, keys, input_protocol, thinking_profile).await? {
                updated += 1;
            }
        }
        if let Some(attrs) = chat_feature_attrs(&name, &model_id, &type_name) {
            if write_model_feature_attrs(pool, id, &attrs).await? {
                updated += 1;
            }
        }
    }
    Ok(updated)
}

pub(crate) async fn drop_playground_2026_generic_chat_schemes(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let mut updated = persist_pg_schemes(pool).await?;
    updated += clear_dropped_scheme_bindings(pool).await?;
    updated += rebind_known_chat_schemes(pool).await?;
    updated += rebind_glm53_off_glm52(pool).await?;
    Ok(updated)
}

async fn rebind_known_chat_schemes(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<u64> {
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT m.id, m.name, m.model_id, COALESCE(mt.name, '') \
         FROM models m \
         LEFT JOIN model_types mt ON mt.id = m.type_id",
    )
    .fetch_all(pool)
    .await?;
    let mut updated = 0u64;
    for (id, name, model_id, type_name) in rows {
        let Some(scheme_id) = scheme_for(&name, &model_id, &type_name) else {
            continue;
        };
        let key = format!("pg_model_id_{id}");
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT config_value FROM plugin_configs \
             WHERE plugin_name = 'playground_2026' AND config_key = $1",
        )
        .bind(&key)
        .fetch_optional(pool)
        .await?;
        let Some(raw) = existing else {
            continue;
        };
        let mut conf = match serde_json::from_str::<Value>(&raw) {
            Ok(v) if v.is_object() => v,
            _ => continue,
        };
        let current = conf.get("scheme_id").and_then(|v| v.as_str()).unwrap_or("");
        if !current.is_empty() {
            continue;
        }
        if let Some(obj) = conf.as_object_mut() {
            obj.insert("scheme_id".into(), json!(scheme_id));
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

async fn rebind_glm53_off_glm52(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<u64> {
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT m.id, m.name, m.model_id, COALESCE(mt.name, '') \
         FROM models m \
         LEFT JOIN model_types mt ON mt.id = m.type_id",
    )
    .fetch_all(pool)
    .await?;
    let mut updated = 0u64;
    for (id, name, model_id, type_name) in rows {
        if scheme_for(&name, &model_id, &type_name) != Some(GLM_5_3_SCHEME_ID) {
            continue;
        }
        let key = format!("pg_model_id_{id}");
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT config_value FROM plugin_configs \
             WHERE plugin_name = 'playground_2026' AND config_key = $1",
        )
        .bind(&key)
        .fetch_optional(pool)
        .await?;
        let mut conf = existing
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .filter(|v| v.is_object())
            .unwrap_or_else(|| json!({"enabled": true, "scheme_id": null, "sort_order": 0}));
        let scheme_id = conf.get("scheme_id").and_then(|v| v.as_str()).unwrap_or("");
        let thinking = conf
            .get("thinking_profile")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if scheme_id == GLM_5_3_SCHEME_ID && thinking == "glm53" {
            continue;
        }
        if let Some(obj) = conf.as_object_mut() {
            obj.insert("scheme_id".into(), json!(GLM_5_3_SCHEME_ID));
            obj.insert("thinking_profile".into(), json!("glm53"));
        }
        let next = serde_json::to_string(&conf)?;
        sqlx::query(
            "INSERT INTO plugin_configs (plugin_name, config_key, config_value, created_at, updated_at) \
             VALUES ('playground_2026', $1, $2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
             ON CONFLICT (plugin_name, config_key) DO UPDATE \
             SET config_value = EXCLUDED.config_value, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(&key)
        .bind(&next)
        .execute(pool)
        .await?;
        updated += 1;
    }
    Ok(updated)
}

pub(crate) async fn seed_playground_2026_glm_deepseek_chat(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let mut updated = persist_pg_schemes(pool).await?;
    updated += clear_dropped_scheme_bindings(pool).await?;
    updated += bind_chat_models_and_attrs(pool).await?;
    Ok(updated)
}

#[allow(dead_code)]
pub(crate) async fn seed_playground_2026_qwen_chat_and_attrs(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    seed_playground_2026_glm_deepseek_chat(pool).await
}
