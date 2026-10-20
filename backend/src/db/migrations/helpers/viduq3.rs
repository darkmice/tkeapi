/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：写入 Vidu Q3 四个视频方案，绑定对应模型并开启。

use crate::api::plugins::manager::viduq3_seed::{
    seed_viduq3_schemes, VIDUQ3_DRAMA_SCHEME_ID, VIDUQ3_PRO_SCHEME_ID,
    VIDUQ3_REFERENCE_SCHEME_ID, VIDUQ3_TURBO_SCHEME_ID,
};
use serde_json::{json, Value};

const BINDINGS: &[(&str, &[&str])] = &[
    (VIDUQ3_PRO_SCHEME_ID, &["viduq3-pro"]),
    (VIDUQ3_TURBO_SCHEME_ID, &["viduq3-turbo"]),
    (VIDUQ3_REFERENCE_SCHEME_ID, &["viduq3", "viduq3-mix"]),
    (VIDUQ3_DRAMA_SCHEME_ID, &["viduq3-drama"]),
];

pub(crate) async fn seed_playground_2026_viduq3_schemes(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let schemes_raw: Option<String> = sqlx::query_scalar(
        "SELECT config_value FROM plugin_configs \
         WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
    )
    .fetch_optional(pool)
    .await?;
    if let Some(raw) = schemes_raw {
        let mut schemes = serde_json::from_str::<Vec<Value>>(&raw)
            .map_err(|e| anyhow::anyhow!("pg_schemes 不是方案数组: {e}"))?;
        if seed_viduq3_schemes(&mut schemes) {
            let next = serde_json::to_string(&schemes)?;
            sqlx::query(
                "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
                 WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
            )
            .bind(&next)
            .execute(pool)
            .await?;
        }
    }

    let mut bound = 0u64;
    for (scheme_id, names) in BINDINGS {
        let names: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM models \
             WHERE lower(btrim(model_id)) = ANY($1) \
                OR lower(btrim(name)) = ANY($1) \
                OR lower(btrim(COALESCE(model_id_alias, ''))) = ANY($1)",
        )
        .bind(&names)
        .fetch_all(pool)
        .await?;
        if !ids.is_empty() {
            sqlx::query(
                "UPDATE models SET is_active = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ANY($1)",
            )
            .bind(&ids)
            .execute(pool)
            .await?;
        }
        for id in ids {
            let key = format!("pg_model_id_{id}");
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT config_value FROM plugin_configs \
                 WHERE plugin_name = 'playground_2026' AND config_key = $1",
            )
            .bind(&key)
            .fetch_optional(pool)
            .await?;
            let mut cfg = existing
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .filter(|v| v.is_object())
                .unwrap_or_else(|| {
                    json!({
                        "enabled": true,
                        "feature_keys": [],
                        "sort_order": 0
                    })
                });
            let same_scheme = cfg.get("scheme_id").and_then(|v| v.as_str()) == Some(*scheme_id);
            let enabled = cfg.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            if same_scheme && enabled {
                continue;
            }
            cfg["scheme_id"] = json!(*scheme_id);
            cfg["enabled"] = json!(true);
            let value = serde_json::to_string(&cfg)?;
            sqlx::query(
                "INSERT INTO plugin_configs (plugin_name, config_key, config_value, updated_at) \
                 VALUES ('playground_2026', $1, $2, CURRENT_TIMESTAMP) \
                 ON CONFLICT (plugin_name, config_key) DO UPDATE \
                 SET config_value = EXCLUDED.config_value, updated_at = CURRENT_TIMESTAMP",
            )
            .bind(&key)
            .bind(&value)
            .execute(pool)
            .await?;
            bound += 1;
        }
    }
    Ok(bound)
}
