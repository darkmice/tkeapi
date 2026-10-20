/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：初始化生成失败异常映射。v1 不覆盖已有配置，v2 重置为拆开的或条件并写入响应示例。
//! v3 只按代号补缺，已有同代号不再追加。v4 只改已有代号的标题和说明，不新增。

#[cfg(feature = "commercial_plugins")]
use crate::api::plugins::playground_2026::error_mappings::{default_mappings, CONFIG_KEY};

#[cfg(feature = "commercial_plugins")]
pub(crate) async fn seed_playground_2026_error_mappings(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = $1",
    )
    .bind(CONFIG_KEY)
    .fetch_optional(pool)
    .await?;
    if existing.is_some() {
        return Ok(0);
    }
    let raw = serde_json::to_string(&default_mappings())?;
    sqlx::query(
        "INSERT INTO plugin_configs (plugin_name, config_key, config_value, updated_at) \
         VALUES ('playground_2026', $1, $2, CURRENT_TIMESTAMP)",
    )
    .bind(CONFIG_KEY)
    .bind(&raw)
    .execute(pool)
    .await?;
    Ok(1)
}

#[cfg(not(feature = "commercial_plugins"))]
pub(crate) async fn seed_playground_2026_error_mappings(
    _pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    Ok(0)
}

#[cfg(feature = "commercial_plugins")]
pub(crate) async fn reset_playground_2026_error_mappings(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let raw = serde_json::to_string(&default_mappings())?;
    let result = sqlx::query(
        "INSERT INTO plugin_configs (plugin_name, config_key, config_value, updated_at) \
         VALUES ('playground_2026', $1, $2, CURRENT_TIMESTAMP) \
         ON CONFLICT (plugin_name, config_key) DO UPDATE \
         SET config_value = EXCLUDED.config_value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(CONFIG_KEY)
    .bind(&raw)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(not(feature = "commercial_plugins"))]
pub(crate) async fn reset_playground_2026_error_mappings(
    _pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    Ok(0)
}

/// 把默认表里指定代号补进已有异常映射。代号已存在则跳过。
#[cfg(feature = "commercial_plugins")]
pub(crate) async fn ensure_playground_2026_error_mapping_codes(
    pool: &sqlx::Pool<sqlx::Postgres>,
    codes: &[&str],
) -> anyhow::Result<u64> {
    let defaults = default_mappings();
    let defaults = defaults
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("默认异常映射不是列表"))?;
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = $1",
    )
    .bind(CONFIG_KEY)
    .fetch_optional(pool)
    .await?;
    let Some(raw) = existing else {
        let encoded = serde_json::to_string(&default_mappings())?;
        sqlx::query(
            "INSERT INTO plugin_configs (plugin_name, config_key, config_value, updated_at) \
             VALUES ('playground_2026', $1, $2, CURRENT_TIMESTAMP)",
        )
        .bind(CONFIG_KEY)
        .bind(&encoded)
        .execute(pool)
        .await?;
        return Ok(1);
    };
    let mut rows: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
    let mut present = std::collections::HashSet::new();
    for row in &rows {
        if let Some(code) = row.get("code").and_then(|v| v.as_str()) {
            present.insert(code.to_string());
        }
    }
    let mut added = 0u64;
    for code in codes {
        if present.contains(*code) {
            continue;
        }
        let Some(rule) = defaults.iter().find(|row| {
            row.get("code").and_then(|v| v.as_str()) == Some(*code)
        }) else {
            continue;
        };
        rows.push(rule.clone());
        present.insert((*code).to_string());
        added += 1;
    }
    if added == 0 {
        return Ok(0);
    }
    let encoded = serde_json::to_string(&rows)?;
    sqlx::query(
        "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
         WHERE plugin_name = 'playground_2026' AND config_key = $2",
    )
    .bind(&encoded)
    .bind(CONFIG_KEY)
    .execute(pool)
    .await?;
    Ok(added)
}

#[cfg(not(feature = "commercial_plugins"))]
pub(crate) async fn ensure_playground_2026_error_mapping_codes(
    _pool: &sqlx::Pool<sqlx::Postgres>,
    _codes: &[&str],
) -> anyhow::Result<u64> {
    Ok(0)
}

/// 用默认文案覆盖已有代号的标题和说明。文案已相同则不写库。
#[cfg(feature = "commercial_plugins")]
pub(crate) async fn sync_playground_2026_error_mapping_copy(
    pool: &sqlx::Pool<sqlx::Postgres>,
    codes: &[&str],
) -> anyhow::Result<u64> {
    let defaults = default_mappings();
    let defaults = defaults
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("默认异常映射不是列表"))?;
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = $1",
    )
    .bind(CONFIG_KEY)
    .fetch_optional(pool)
    .await?;
    let Some(raw) = existing else {
        return Ok(0);
    };
    let mut rows: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
    let mut changed = 0u64;
    for code in codes {
        let Some(rule) = defaults.iter().find(|row| {
            row.get("code").and_then(|v| v.as_str()) == Some(*code)
        }) else {
            continue;
        };
        let Some(stored) = rows.iter_mut().find(|row| {
            row.get("code").and_then(|v| v.as_str()) == Some(*code)
        }) else {
            continue;
        };
        let mut row_changed = false;
        for key in ["title", "hint"] {
            let next = rule.get(key).cloned().unwrap_or(serde_json::Value::Null);
            if stored.get(key) != Some(&next) {
                stored[key] = next;
                row_changed = true;
            }
        }
        if row_changed {
            changed += 1;
        }
    }
    if changed == 0 {
        return Ok(0);
    }
    let encoded = serde_json::to_string(&rows)?;
    sqlx::query(
        "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
         WHERE plugin_name = 'playground_2026' AND config_key = $2",
    )
    .bind(&encoded)
    .bind(CONFIG_KEY)
    .execute(pool)
    .await?;
    Ok(changed)
}

#[cfg(not(feature = "commercial_plugins"))]
pub(crate) async fn sync_playground_2026_error_mapping_copy(
    _pool: &sqlx::Pool<sqlx::Postgres>,
    _codes: &[&str],
) -> anyhow::Result<u64> {
    Ok(0)
}
