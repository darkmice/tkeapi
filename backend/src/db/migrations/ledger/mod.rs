/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use sqlx::PgPool;
use std::collections::HashSet;

#[path = "01_bootstrap.rs"]
mod m01_bootstrap;
#[path = "02_schema_upgrade.rs"]
mod m02_schema_upgrade;
#[path = "03_happyhorse_ha.rs"]
mod m03_happyhorse_ha;
#[path = "04_docs_playground_v1.rs"]
mod m04_docs_playground_v1;
#[path = "05_volc_monitor_cleanup.rs"]
mod m05_volc_monitor_cleanup;
#[path = "06_redemptions_channels_time.rs"]
mod m06_redemptions_channels_time;
#[path = "07_logs_indexes.rs"]
mod m07_logs_indexes;
#[path = "08_pg2026_portal_marketing.rs"]
mod m08_pg2026_portal_marketing;
#[path = "09_data_sync_forward_rules.rs"]
mod m09_data_sync_forward_rules;
#[path = "10_comfyui_tail.rs"]
mod m10_comfyui_tail;
#[path = "11_models_kyc_invoices.rs"]
mod m11_models_kyc_invoices;
#[path = "12_playground_skills_voice.rs"]
mod m12_playground_skills_voice;
#[path = "13_model_library_enhance_logs.rs"]
mod m13_model_library_enhance_logs;
#[path = "14_marketing_affiliate_portal_articles.rs"]
mod m14_marketing_affiliate_portal_articles;
#[path = "15_system_modules_bigint_int8.rs"]
mod m15_system_modules_bigint_int8;

pub async fn run_all(pool: &PgPool, done: &mut HashSet<String>) -> anyhow::Result<()> {
    m01_bootstrap::run(pool, done).await?;
    m02_schema_upgrade::run(pool, done).await?;
    m03_happyhorse_ha::run(pool, done).await?;
    m04_docs_playground_v1::run(pool, done).await?;
    m05_volc_monitor_cleanup::run(pool, done).await?;
    m06_redemptions_channels_time::run(pool, done).await?;
    m07_logs_indexes::run(pool, done).await?;
    m08_pg2026_portal_marketing::run(pool, done).await?;
    m09_data_sync_forward_rules::run(pool, done).await?;
    m10_comfyui_tail::run(pool, done).await?;
    m11_models_kyc_invoices::run(pool, done).await?;
    m12_playground_skills_voice::run(pool, done).await?;
    m13_model_library_enhance_logs::run(pool, done).await?;
    m14_marketing_affiliate_portal_articles::run(pool, done).await?;
    m15_system_modules_bigint_int8::run(pool, done).await?;
    Ok(())
}
