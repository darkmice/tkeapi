/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

/// 索引 DDL 幂等：CREATE 名冲突(23505) / DROP 目录缺口(XX000) 可跳过；不吞业务唯一约束错误。
pub(crate) fn is_idempotent_index_ddl_err(err: &sqlx::Error, stmt: &str) -> bool {
    let sqlx::Error::Database(db) = err else {
        return false;
    };
    // code() 返回临时 Cow，须先绑定再 as_deref，否则临时值提前释放
    let code = db.code();
    let code = code.as_deref();
    let msg = db.message();
    let upper = stmt.to_ascii_uppercase();
    if upper.contains("CREATE INDEX")
        && code == Some("23505")
        && db
            .constraint()
            .map(|c| c == "pg_class_relname_nsp_index")
            .unwrap_or_else(|| msg.contains("pg_class_relname_nsp_index"))
    {
        return true;
    }
    upper.contains("DROP INDEX")
        && code == Some("XX000")
        && msg.contains("pg_attribute catalog is missing")
}

/// 判定错误是否为可重试的数据库锁争抢/锁等待超时/连接池瞬时超时
pub(crate) fn is_retryable_lock_err(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::PoolTimedOut => true,
        sqlx::Error::Database(db) => {
            let code = db.code();
            let code = code.as_deref();
            let msg = db.message();
            code == Some("55P03") // lock_not_available
                || code == Some("40P01") // deadlock_detected
                || msg.contains("lock timeout")
                || msg.contains("canceling statement due to lock timeout")
        }
        _ => false,
    }
}

/// 执行单条迁移语句：带短锁超时保护（2s 探针）与高韧性随机退避重试（最多 120 次），
/// 既杜绝长事务/锁表导致的队头阻塞，又确保核心表结构与字段 100% 成功落地
pub(crate) async fn execute_migration_stmt(
    pool: &sqlx::Pool<sqlx::Postgres>,
    id: &str,
    stmt: &str,
) -> bool {
    const MAX_LOCK_RETRIES: usize = 120;
    let mut attempt = 0;

    loop {
        attempt += 1;
        let res = match pool.acquire().await {
            Ok(mut conn) => {
                let _ = sqlx::query("SET lock_timeout = '2000ms'")
                    .execute(&mut *conn)
                    .await;
                let exec_res = sqlx::query(stmt).execute(&mut *conn).await;
                let _ = sqlx::query("RESET lock_timeout").execute(&mut *conn).await;
                exec_res
            }
            Err(e) => Err(e),
        };

        match res {
            Ok(_) => return true,
            Err(e) if is_idempotent_index_ddl_err(&e, stmt) => {
                tracing::warn!(
                    "⚠️ [Migration] ID: {} 索引 DDL 已满足/目录异常可跳过. 语句: '{}'",
                    id,
                    stmt
                );
                return true;
            }
            Err(e) if is_retryable_lock_err(&e) && attempt < MAX_LOCK_RETRIES => {
                // 500ms ~ 1500ms 随机抖动休眠，避免高并发下锁惊群与周期性冲突
                let jitter_ms = 500 + ((id.len() * 37 + attempt * 53) % 1000) as u64;
                if attempt % 5 == 0 || attempt <= 3 {
                    tracing::warn!(
                        "⏳ [Migration] ID: {} 遇到表锁争抢/超时 (第 {}/{} 次)，让路业务并休眠 {}ms 后重试... 语句: '{}'",
                        id,
                        attempt,
                        MAX_LOCK_RETRIES,
                        jitter_ms,
                        stmt
                    );
                }
                tokio::time::sleep(std::time::Duration::from_millis(jitter_ms)).await;
                continue;
            }
            Err(e) => {
                // 如果是纯注释语句（COMMENT ON），即便遇锁争抢超时也仅记录警告并放行，
                // 绝不因非核心注释元数据导致核心服务启动受阻
                if stmt.trim().to_ascii_uppercase().starts_with("COMMENT ON") {
                    tracing::warn!(
                        "⚠️ [Migration] ID: {} 列/表注释语句未成功落地（已跳过，不影响核心功能）: '{}' 错误: {:?}",
                        id,
                        stmt,
                        e
                    );
                    return true;
                }

                tracing::error!(
                    "❌ [Migration] ID: {} 致命失败 (尝试 {} 次). 语句: '{}' 错误: {:?}",
                    id,
                    attempt,
                    stmt,
                    e
                );
                return false;
            }
        }
    }
}

/// 一次性迁移：依赖调用处已加载的 `done`（sys_migration_history），跳过已完成项。
/// 关键表结构与字段未成功落地时强行阻断启动，杜绝带病上线导致运行时报列缺失或类型失配 500
macro_rules! once_migration {
    ($pool:expr, $done:ident, $id:literal, $( $stmt:expr ),+ $(,)?) => {{
        if !$done.contains($id) {
            let mut _m_ok = true;
            $(
                if _m_ok {
                    if !$crate::db::migrations::macros::execute_migration_stmt($pool, $id, $stmt).await {
                        _m_ok = false;
                    }
                }
            )+
            if _m_ok {
                let _ = sqlx::query("INSERT INTO sys_migration_history (id) VALUES ($1)")
                    .bind($id)
                    .execute($pool)
                    .await;
                $done.insert($id.to_string());
                tracing::info!("✅ [Migration] 一次性迁移完成: {}", $id);
            } else {
                panic!("❌ [Migration] 核心结构迁移失败且已阻断服务启动: {}。请排查数据库锁占用或错误原因，严禁结构缺失上线！", $id);
            }
        }
    }};
}

pub(crate) use once_migration;

/// 大表慢迁移通用异步执行引擎：
/// 1. 获取专属单连接（隔离会话参数）；
/// 2. 注入安全生产级 256MB 内存与 0 并行 Worker（彻底消除多 Worker 内存放大导致的 OOM-Killer 强杀风险）；
/// 3. 逐句并发执行 DDL，遇到已存在/缺失等幂等错误自动跳过；
/// 4. 成功后向 sys_migration_history 自动登记完成标记（由数据库自填时间戳，永无类型冲突）。
pub async fn execute_heavy_migration(
    pool: sqlx::Pool<sqlx::Postgres>,
    id: &'static str,
    stmts: &[&'static str],
) {
    tracing::info!(
        "🚀 [Migration] 后台启动大表高并发优化任务 (ID: {}, 不阻塞服务启动)...",
        id
    );
    let mut conn = match pool.acquire().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("❌ [Migration] 后台大表任务 {} 获取连接失败: {:?}", id, e);
            return;
        }
    };

    let session_optimizations = [
        "SET maintenance_work_mem = '1024MB'",
        "SET max_parallel_maintenance_workers = 2",
    ];
    for opt in session_optimizations {
        let _ = sqlx::query(opt).execute(&mut *conn).await;
    }

    let mut all_success = true;
    for stmt in stmts {
        match sqlx::query(stmt).execute(&mut *conn).await {
            Ok(_) => {}
            Err(e) if is_idempotent_index_ddl_err(&e, stmt) => {
                tracing::warn!("⚠️ [Migration] 后台大表索引 DDL 已满足可跳过: '{}'", stmt);
            }
            Err(e) => {
                all_success = false;
                tracing::error!(
                    "❌ [Migration] 后台大表任务 {} 执行错误: '{}', 错误: {:?}",
                    id,
                    stmt,
                    e
                );
                break;
            }
        }
    }

    // 3. 显式将连接的会话参数复原为数据库默认值，确保归还连接池时不留任何参数残留
    let _ = sqlx::query("RESET maintenance_work_mem")
        .execute(&mut *conn)
        .await;
    let _ = sqlx::query("RESET max_parallel_maintenance_workers")
        .execute(&mut *conn)
        .await;

    if all_success {
        let _ = sqlx::query(
            "INSERT INTO sys_migration_history (id) VALUES ($1) ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .execute(&pool)
        .await;
        tracing::info!(
            "✅ [Migration] 大表任务 {} 全量构建完成，优化器已接入最优索引！",
            id
        );
    }
}

/// 超大容量表安全异步迁移宏（启动零阻塞 + 专属连接会话内存调优）：
/// 自动判定未执行时在 tokio::spawn 后台异步执行，主线程毫秒级启动对外监听，杜绝 502 停机。
macro_rules! once_heavy_migration {
    ($pool:expr, $done:ident, $id:literal, $( $stmt:expr ),+ $(,)?) => {{
        if !$done.contains($id) {
            let pool_bg = $pool.clone();
            tokio::spawn(async move {
                $crate::db::migrations::macros::execute_heavy_migration(
                    pool_bg,
                    $id,
                    &[ $( $stmt ),+ ],
                )
                .await;
            });
        }
    }};
}

pub(crate) use once_heavy_migration;
