/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：把 claude-fable-5 聊天方案写入 pg_schemes。
//! 只安装方案，不绑定、不启用模型。

use crate::api::plugins::manager::claude_fable_chat_seed::seed_claude_fable_chat_scheme;
use serde_json::Value;

pub(crate) async fn seed_playground_2026_claude_fable_chat_scheme(
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
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
    seed_claude_fable_chat_scheme(&mut schemes);
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
