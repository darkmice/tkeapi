/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 迁移：`ledger/` 冻结历史 → [`incremental.rs`](incremental.rs) 追加新迁移。
//! 合并旧分支：`python3 scripts/port_legacy_migrations.py --legacy <旧migrations.rs>`
//! 详规：`.cursor/rules/db-migrations.mdc`

use std::collections::HashSet;

pub(crate) mod macros;
pub(crate) use macros::{once_heavy_migration, once_migration};

pub(crate) mod helpers;
mod incremental;
mod ledger;

pub async fn run_pg(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sys_migration_history (\
         id TEXT PRIMARY KEY, \
         executed_at TEXT NOT NULL DEFAULT (now()::text))",
    )
    .execute(pool)
    .await
    .ok();

    // 针对历史老库可能已存在 sys_migration_history 但缺少主键/唯一约束的情况，
    // 自动去重并补全 PRIMARY KEY (id)，确保 ON CONFLICT (id) 语法永不报错
    sqlx::query(
        r#"DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint c
                JOIN pg_class t ON c.conrelid = t.oid
                WHERE t.relname = 'sys_migration_history' AND c.contype IN ('p', 'u')
            ) THEN
                DELETE FROM sys_migration_history a USING sys_migration_history b
                WHERE a.ctid < b.ctid AND a.id = b.id;

                ALTER TABLE sys_migration_history ADD PRIMARY KEY (id);
            END IF;
        EXCEPTION WHEN OTHERS THEN
            NULL;
        END $$;"#,
    )
    .execute(pool)
    .await
    .ok();

    let mut done: HashSet<String> = sqlx::query_scalar("SELECT id FROM sys_migration_history")
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();

    // 新装默认关闭写在各自的 INSERT 里。这三条若再执行，会在系统更新时改掉管理员已经设好的开关。
    skip_switch_overwrite_migrations(pool, &mut done).await?;

    ledger::run_all(pool, &mut done).await?;
    incremental::run(pool, &mut done).await?;
    tracing::info!("PostgreSQL AnyPool migrations completed successfully");
    Ok(())
}

/// 站点插件 `is_enabled` 与 MediaKit 预置模型 `is_active` 只由管理员修改。
/// 提前登记后，ledger 里对应的覆盖语句不再执行。
async fn skip_switch_overwrite_migrations(
    pool: &sqlx::PgPool,
    done: &mut HashSet<String>,
) -> anyhow::Result<()> {
    const IDS: &[&str] = &[
        "volcengine_enhance_default_off_v1",
        "playground_2026_default_off_v1",
        "volc_mediakit_preset_models_default_off_v1",
    ];
    for id in IDS {
        if done.contains(*id) {
            continue;
        }
        sqlx::query(
            "INSERT INTO sys_migration_history (id) VALUES ($1) ON CONFLICT (id) DO NOTHING",
        )
        .bind(*id)
        .execute(pool)
        .await?;
        done.insert((*id).to_string());
        tracing::info!("跳过会改写已有开关的迁移: {id}");
    }
    Ok(())
}
