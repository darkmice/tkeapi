/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：写入 gemini-nano-banana-2.1 图片方案，并绑定同名模型。

use crate::api::plugins::manager::gemini_nano_banana_21_seed::{
    seed_gemini_nano_banana_21_scheme, GEMINI_NANO_BANANA_21_SCHEME_ID,
};
use serde_json::{json, Value};

pub(crate) async fn seed_playground_2026_gemini_nano_banana_21_scheme(
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
        seed_gemini_nano_banana_21_scheme(&mut schemes);
        let next = serde_json::to_string(&schemes)?;
        if next != raw {
            sqlx::query(
                "UPDATE plugin_configs SET config_value = $1, updated_at = CURRENT_TIMESTAMP \
                 WHERE plugin_name = 'playground_2026' AND config_key = 'pg_schemes'",
            )
            .bind(&next)
            .execute(pool)
            .await?;
        }
    }

    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM models \
         WHERE lower(btrim(model_id)) = 'gemini-nano-banana-2.1' \
            OR lower(btrim(name)) = 'gemini-nano-banana-2.1' \
            OR lower(btrim(COALESCE(model_id_alias, ''))) = 'gemini-nano-banana-2.1'",
    )
    .fetch_all(pool)
    .await?;
    let mut bound = 0u64;
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
        if cfg.get("scheme_id").and_then(|v| v.as_str()) == Some(GEMINI_NANO_BANANA_21_SCHEME_ID) {
            continue;
        }
        cfg["scheme_id"] = json!(GEMINI_NANO_BANANA_21_SCHEME_ID);
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
    Ok(bound)
}
