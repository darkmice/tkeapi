/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 将插件 JSON 里的独立排序回填到 `models.sort_order`（仅当模型列表页面排序仍为 0）。

use std::collections::HashMap;

use sqlx::PgPool;

pub(crate) fn plugin_config_model_id(
    config_key: &str,
    mid_to_id: &HashMap<String, i64>,
) -> Option<i64> {
    if let Some(rest) = config_key.strip_prefix("pg_model_id_") {
        return rest.parse().ok();
    }
    if let Some(rest) = config_key.strip_prefix("mp_model_id_") {
        return rest.parse().ok();
    }
    if let Some(rest) = config_key.strip_prefix("pg_model_") {
        return mid_to_id.get(rest).copied();
    }
    None
}

pub(crate) fn json_sort_order(config_value: &str) -> i64 {
    serde_json::from_str::<serde_json::Value>(config_value)
        .ok()
        .and_then(|v| v.get("sort_order").cloned())
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_f64().map(|f| f as i64))
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0)
}

/// 模型列表已有页面排序时保持不变；为 0 时取各插件配置中的最大权重，避免已有展示顺序被抹平。
pub(crate) fn backfill_sort(model_sort: i64, plugin_sorts: &[i64]) -> i64 {
    if model_sort != 0 {
        return model_sort;
    }
    plugin_sorts.iter().copied().max().unwrap_or(0)
}

pub(crate) async fn backfill_models_sort_order_from_plugin_configs(
    pool: &PgPool,
) -> anyhow::Result<u64> {
    let models: Vec<(i64, String, i64)> =
        sqlx::query_as("SELECT id, COALESCE(mid, ''), sort_order FROM models")
            .fetch_all(pool)
            .await?;
    let mid_to_id: HashMap<String, i64> = models
        .iter()
        .filter(|(_, mid, _)| !mid.is_empty())
        .map(|(id, mid, _)| (mid.clone(), *id))
        .collect();
    let current: HashMap<i64, i64> = models.iter().map(|(id, _, sort)| (*id, *sort)).collect();

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT config_key, config_value FROM plugin_configs \
         WHERE plugin_name IN ('playground', 'playground_2026', 'model_marketplace')",
    )
    .fetch_all(pool)
    .await?;

    let mut plugin_max: HashMap<i64, i64> = HashMap::new();
    for (key, val) in rows {
        let Some(id) = plugin_config_model_id(&key, &mid_to_id) else {
            continue;
        };
        let sort = json_sort_order(&val);
        let entry = plugin_max.entry(id).or_insert(0);
        if sort > *entry {
            *entry = sort;
        }
    }

    let mut updated = 0u64;
    for (id, model_sort) in current {
        let plugin = plugin_max.get(&id).copied().unwrap_or(0);
        let next = backfill_sort(model_sort, &[plugin]);
        if next == model_sort {
            continue;
        }
        let next_i32 = next.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        sqlx::query(
            "UPDATE models SET sort_order = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
        )
        .bind(next_i32)
        .bind(id)
        .execute(pool)
        .await?;
        updated += 1;
    }
    Ok(updated)
}
