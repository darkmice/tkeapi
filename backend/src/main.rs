/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#![recursion_limit = "256"]

use std::sync::Arc;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod admin_permission;
mod api;
mod auth;
mod config;
mod db;
mod debug_log;
mod error;
mod middleware;
mod models;
mod money;
mod providers;
mod relay;
mod services;
mod time_system;
pub mod utils;

use config::AppConfig;
use db::Database;

pub struct DashboardCacheEntry {
    pub stats: models::DashboardStats,
    pub timestamp: std::time::Instant,
}

pub struct AppState {
    pub db: Database,
    pub config: AppConfig,
    pub http_client: reqwest::Client,
    pub rate_limiter: middleware::rate_limit::GlobalRateLimiter,
    /// OAuth / 代入登录一次性兑换码：code → (jwt, expires_at)
    pub login_codes: dashmap::DashMap<String, (String, std::time::Instant)>,
    #[cfg(feature = "plugin_site_icons")]
    pub icon_sync_progress: api::plugins::site_icons::SyncProgress,
    pub dashboard_cache: dashmap::DashMap<String, DashboardCacheEntry>,
    pub model_trend_cache: dashmap::DashMap<String, api::dashboard::ModelTrendCacheEntry>,
    /// 高可用运行时熔断表（配置在 relay_settings HA_RULES）
    pub failed_channels: dashmap::DashMap<String, std::time::Instant>,
    /// 级联阶段二进行中互斥（log_id → ()），防并发轮询重复裁剪/超分
    pub cascade_s2_inflight: dashmap::DashMap<i64, ()>,
    /// 日限额内存拦截器（DashMap + DB hydration）
    pub quota_memory: relay::quota_memory::MemoryQuotaGuard,
    /// 异步计费事件投递（Worker 批量刷库；停机时 drain）
    pub billing_ingress: relay::billing_pipeline::BillingIngress,
    /// 令牌最后使用时间节流表（token_id -> 上次写库 Instant，防高并发行锁争夺）
    pub token_last_used_throttle: dashmap::DashMap<i64, std::time::Instant>,
    /// 视频 POST 受理后仍在跑的上游提交（关闭时排空，避免素材转换被掐断）
    pub video_submit_drain: std::sync::Arc<relay::VideoSubmitDrain>,
}

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    // timesystem：进程级锁定 UTC+0（须在创建 Tokio 多线程运行时之前单线程执行，避免多线程 setenv 竞争）
    time_system::enforce_process_utc();
    // 关于页运行时「启动时间」锚点
    services::runtime_info::mark_process_start();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {

    // 1. 初始化控制台标准输出写入器
    let (non_blocking_stdout, _stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
    let stdout_layer = tracing_subscriber::fmt::layer().with_writer(non_blocking_stdout);

    // 2. 根据是否存在 LOG_DIR 环境变量，动态判断是否开启按天滚动文件写入
    let mut file_layer = None;
    let mut _file_guard = None;

    if let Ok(log_dir) = std::env::var("LOG_DIR") {
        let file_appender = tracing_appender::rolling::daily(&log_dir, "app.log");
        let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);
        _file_guard = Some(guard);

        file_layer = Some(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking_file)
                .with_ansi(false),
        );
    }

    // 3. 配置过滤器与组合 Layer
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    let config_data = AppConfig::from_env();
    let database = Database::new(&config_data.database_url).await?;
    database.run_migrations().await?;

    // 同步 REGISTER_ENABLED 环境变量到数据库设置
    sync_registration_settings(&database, config_data.register_enabled).await?;

    // 优雅关闭信号广播通道：所有后台任务监听此信号，收到后完成当前工作并退出
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // 计费 MPSC Worker：与 shutdown 联动，退出前排空管道再刷库
    let billing_pipeline = relay::billing_pipeline::BillingPipelineHandle::start(
        database.clone(),
        shutdown_rx.clone(),
    );
    let billing_ingress = billing_pipeline.ingress();

    let state = Arc::new(AppState {
        db: database,
        config: config_data.clone(),
        http_client: services::http_client::build_outbound_client(),
        rate_limiter: middleware::rate_limit::GlobalRateLimiter::new(),
        login_codes: dashmap::DashMap::new(),
        #[cfg(feature = "plugin_site_icons")]
        icon_sync_progress: api::plugins::site_icons::SyncProgress::new(),
        dashboard_cache: dashmap::DashMap::new(),
        model_trend_cache: dashmap::DashMap::new(),
        failed_channels: dashmap::DashMap::new(),
        cascade_s2_inflight: dashmap::DashMap::new(),
        quota_memory: relay::quota_memory::MemoryQuotaGuard::new(),
        billing_ingress,
        token_last_used_throttle: dashmap::DashMap::new(),
        video_submit_drain: relay::VideoSubmitDrain::new(),
    });

    let mut bg_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // 1b. 渠道与预设配额异步聚合批量刷盘任务（每 2 秒一次，消除 channels 单行排他锁争用；停机时 drain）
    bg_handles.push(relay::channel_quota::start(
        state.db.clone(),
        shutdown_rx.clone(),
    ));

    // 3. 实时指标冷用户清理（每 5 分钟，空闲 >1h 剔除）
    bg_handles.push(tokio::spawn(middleware::live_metrics::run_cleanup_loop(
        shutdown_rx.clone(),
    )));

    // 3b. 看板缓存 TTL 清理（每 5 分钟，条目 >30min 剔除）
    bg_handles.push(tokio::spawn(
        api::dashboard::run_dashboard_cache_cleanup_loop(state.clone(), shutdown_rx.clone()),
    ));

    // 单例后台任务（扫库 / 定时 / 启动恢复）：蓝绿热更新时只允许一个进程运行。
    // WORKERS_STANDBY=1：先提供 HTTP，后台竞选 leader 锁，旧进程退出后由新进程接管；
    // 未设置：与历史行为一致，启动即运行。
    if workers_standby_enabled() {
        bg_handles.push(tokio::spawn(run_singleton_workers_as_leader(
            state.clone(),
            shutdown_rx.clone(),
        )));
    } else {
        bg_handles.extend(start_singleton_workers(state.clone(), shutdown_rx.clone()));
    }

    let assets_dir = config_data.assets_dir.clone();
    let portal_dir = config_data.portal_dir.clone();
    let portal_pro_dir = config_data.portal_pro_dir.clone();
    let legal_dir = format!("{}/legal", config_data.data_dir);
    // 确保 portal 目录存在
    std::fs::create_dir_all(&portal_dir).ok();
    std::fs::create_dir_all(&portal_pro_dir).ok();
    std::fs::create_dir_all(&legal_dir).ok();
    // 确保 assets 目录存在
    std::fs::create_dir_all(&assets_dir).ok();
    api::plugins::site_portal::ensure_portal_assets(&portal_dir, &assets_dir).await;
    #[cfg(feature = "commercial_plugins")]
    api::plugins::site_portal_pro::ensure_portal_pro_assets(&portal_pro_dir, &assets_dir).await;
    let _ = api::legal::ensure_legal_static_files(&state).await;
    let app = api::build_router(state.clone())
        .nest_service("/portal", tower_http::services::ServeDir::new(&portal_dir))
        .nest_service(
            "/portal-pro",
            tower_http::services::ServeDir::new(&portal_pro_dir),
        )
        .nest_service("/assets", tower_http::services::ServeDir::new(&assets_dir));

    let addr = format!("{}:{}", config_data.host, config_data.port);
    eprintln!("⚙️ Binding server to {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("🚀 TokensByte server running at http://{}", addr);
    eprintln!(
        "✅ TokensByte server is now completely online at http://{} !",
        addr
    );

    // 优雅关闭：仅显式 APP_ENV=dev|development 时秒退；默认按生产优雅 drain
    // 禁止用 CARGO_MANIFEST_DIR 推断（cargo run 注入会误伤生产）
    let is_dev = matches!(
        std::env::var("APP_ENV")
            .or_else(|_| std::env::var("RUN_MODE"))
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "dev" | "development"
    );
    if is_dev {
        tracing::warn!("⚡ APP_ENV=dev：关闭信号将秒退，跳过计费管道 drain（勿用于生产）");
        drop(billing_pipeline);
        let shutdown_signal_dev = async move {
            wait_for_shutdown_signal().await;
            tracing::info!("⚡ 开发环境收到关闭信号，强制秒退释放端口...");
            std::process::exit(0);
        };

        tokio::select! {
            res = axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()) => {
                res?;
            }
            _ = shutdown_signal_dev => {}
        }
    } else {
        let shutdown_signal = async move {
            wait_for_shutdown_signal().await;
            tracing::info!("⏳ 收到关闭信号，停止接受新请求，等待进行中的任务完成...");
            let _ = shutdown_tx.send(true);
        };

        let serve_future = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal);

        let mut serve_shutdown_rx = shutdown_rx.clone();
        tokio::select! {
            res = serve_future => {
                res?;
            }
            _ = async {
                while !*serve_shutdown_rx.borrow() {
                    if serve_shutdown_rx.changed().await.is_err() {
                        break;
                    }
                }
                let drain_secs = shutdown_drain_secs();
                tokio::time::sleep(std::time::Duration::from_secs(drain_secs)).await;
                tracing::warn!("⚠️ HTTP 服务未在 {} 秒内优雅关闭，强制结束连接", drain_secs);
            } => {}
        }

        // 视频提交已脱离 HTTP 连接；时限覆盖素材转换与上游超时，不用下面 30 秒的后台上限
        let video_budget = crate::services::http_client::upstream_timeout_duration()
            + std::time::Duration::from_secs(180);
        tracing::info!("⏳ 等待进行中的视频提交落账...");
        match tokio::time::timeout(video_budget, state.video_submit_drain.wait_idle()).await {
            Ok(()) => tracing::info!("✅ 视频提交已落账"),
            Err(_) => tracing::warn!("⚠️ 视频提交未在上游时限内结束"),
        }

        // HTTP 服务已关闭，等待后台任务完成（包括正在轮询中的异步任务）
        tracing::info!("⏳ HTTP 连接已关闭，等待后台任务完成...");
        let wait_bg = async {
            for h in bg_handles {
                let _ = h.await;
            }
            // 计费管道：shutdown 已发出，Worker 排空 MPSC 剩余事件并刷库
            billing_pipeline.join().await;
        };
        match tokio::time::timeout(std::time::Duration::from_secs(30), wait_bg).await {
            Ok(_) => tracing::info!("✅ 所有后台任务已完成，服务安全退出"),
            Err(_) => tracing::warn!("⚠️ 部分后台任务未在 30 秒内完成，强制退出"),
        }
    }

    Ok(())
}

/// HTTP 优雅关闭排空窗口（秒）。默认 30；热更新时通过 SHUTDOWN_DRAIN_SECS 放宽以等待长流式请求，
/// 且 compose 的 stop_grace_period 必须不小于该值。
fn shutdown_drain_secs() -> u64 {
    std::env::var("SHUTDOWN_DRAIN_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(30)
}

/// 是否启用单例后台任务 leader 选举（蓝绿热更新并存期使用）。
fn workers_standby_enabled() -> bool {
    matches!(
        std::env::var("WORKERS_STANDBY")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}

/// 单例后台任务 leader 锁（Postgres session 级 advisory lock，随持有连接关闭或进程退出自动释放）。
const SINGLETON_WORKERS_LOCK_SQL: &str =
    "SELECT pg_try_advisory_lock(hashtext('tokensbyte_singleton_workers')::bigint)";

/// 蓝绿并存期的单例任务调度：先竞选 leader 锁，拿到后才启动单例任务。
/// 旧进程优雅退出后连接关闭、锁释放，新进程在下一个轮询周期接管。
async fn run_singleton_workers_as_leader(
    state: Arc<AppState>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    let mut lock_conn: Option<sqlx::PgConnection> = None;
    let mut announced_standby = false;
    loop {
        if *shutdown_rx.borrow() {
            return;
        }
        // 锁必须绑定在独占连接上：detach 后不再归还连接池，本任务结束时随连接关闭释放
        if lock_conn.is_none() {
            match state.db.pool.acquire().await {
                Ok(pooled) => lock_conn = Some(pooled.detach()),
                Err(e) => tracing::warn!("[WorkersLeader] 获取 leader 锁连接失败: {:?}", e),
            }
        }
        if let Some(conn) = lock_conn.as_mut() {
            let res = sqlx::query_scalar::<_, bool>(SINGLETON_WORKERS_LOCK_SQL)
                .fetch_one(&mut *conn)
                .await;
            match res {
                Ok(true) => break,
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!("[WorkersLeader] 竞选 leader 锁失败，将重建连接: {:?}", e);
                    lock_conn = None;
                }
            }
        }
        if !announced_standby {
            tracing::info!("⏸️ [WorkersLeader] 单例后台任务待命中，等待旧实例退出后接管");
            announced_standby = true;
        }
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}
            _ = shutdown_rx.changed() => return,
        }
    }

    tracing::info!("▶️ [WorkersLeader] 已获得 leader 锁，启动单例后台任务");
    let handles = start_singleton_workers(state, shutdown_rx);
    for h in handles {
        let _ = h.await;
    }
    // 单例任务全部退出后才释放 leader 锁
    drop(lock_conn);
}

/// 启动所有单例后台任务：扫库轮询、定时任务、启动恢复与一次性回填。
/// 蓝绿并存期间同一时刻只能有一个进程运行这些任务（尤其 `recover_interrupted_logs`
/// 会退还"处理中"日志的预扣费，旧实例仍在处理请求时运行会造成误退款）。
/// 进程内存聚合类任务（计费管道、配额刷盘、指标与缓存清理）不在此列，各进程独立运行。
fn start_singleton_workers(
    state: Arc<AppState>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut bg_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // 1. 启动时恢复：处理上次中断遗留的"处理中"日志，退还预扣费（并发异步恢复，不阻塞端口监听）
    tokio::spawn(relay::proxy::recover_interrupted_logs(state.clone()));

    // 2. 启动后台异步任务轮询器（周期见 RelaySettings.poll_tick_secs，缓存；检查未结算视频/图片任务）
    bg_handles.push(relay::task::start(state.clone(), shutdown_rx.clone()));

    // 2b. ComfyUI 提交 worker：accept 后 admit→审图→/prompt；notify + 5s 扫库；随进程 shutdown 退出
    #[cfg(feature = "plugin_comfyui")]
    bg_handles.push(api::plugins::comfyui_bridge::start_submit_worker(
        state.clone(),
        shutdown_rx.clone(),
    ));

    bg_handles.push(spawn_cron_task(
        state.clone(),
        shutdown_rx.clone(),
        60,
        "UpstreamRateSync",
        |s| async move {
            api::channel_configs::run_upstream_rate_sync_tick(s).await;
        },
    ));

    bg_handles.push(spawn_cron_task(
        state.clone(),
        shutdown_rx.clone(),
        60,
        "FxRateRefresh",
        |s| async move {
            services::fx_rates::run_refresh_tick(s).await;
        },
    ));

    #[cfg(feature = "plugin_data_sync")]
    bg_handles.push(spawn_cron_task(
        state.clone(),
        shutdown_rx.clone(),
        60,
        "DataSyncAuto",
        |s| async move {
            api::plugins::data_sync::run_auto_sync_tick(s).await;
        },
    ));

    // 4. 启动孤儿日志清理定时任务（每 10 分钟检查 status_code=0 超过 1 小时的日志）
    bg_handles.push(spawn_cron_task(
        state.clone(),
        shutdown_rx.clone(),
        600,
        "OrphanLogsCleanup",
        |s| async move {
            relay::proxy::cleanup_orphan_pending_logs(&s).await;
        },
    ));

    // 4a. 营销卡券：面值到期与临时会员等级回退（每 60 秒）
    #[cfg(feature = "commercial_plugins")]
    bg_handles.push(spawn_cron_task(
        state.clone(),
        shutdown_rx.clone(),
        60,
        "CouponMembershipExpiry",
        |s| async move {
            crate::api::plugins::marketing_membership::billing::run_expiry_tick(&s).await;
        },
    ));

    // 4b. 营销通知置顶/弹窗/上架到期自动关闭（每 60 秒）
    bg_handles.push(spawn_cron_task(
        state.clone(),
        shutdown_rx.clone(),
        60,
        "AnnouncementExpiry",
        |s| async move {
            if let Err(e) = api::announcements::apply_announcement_expiries(&s).await {
                tracing::warn!("❌ [CronAnnouncementExpiry] 通知有效期处理失败: {:?}", e);
            }
        },
    ));

    // 5a. 每日用量增量统计任务（错峰执行，默认 01:00）
    bg_handles.push(spawn_daily_scheduled_task(
        state.clone(),
        shutdown_rx.clone(),
        "DailyStatsSync",
        |s| s.safe_daily_stats_hm(),
        |s| async move {
            let _ = relay::usage_stats::sync_daily_stats(&s).await;
        },
    ));

    // 5b. 日志大字段清理定时任务（错峰执行，默认 02:30）
    bg_handles.push(spawn_daily_scheduled_task(
        state.clone(),
        shutdown_rx.clone(),
        "LogContentCleanup",
        |s| s.safe_clean_hm(),
        |s| async move {
            let cleanup_settings = fetch_log_cleanup_settings(&s).await;
            cleanup_log_content(&s, cleanup_settings.log_retention_days).await;
        },
    ));

    // 5c. 日志行冷归档定时任务（错峰执行，默认 03:30）
    bg_handles.push(spawn_daily_scheduled_task(
        state.clone(),
        shutdown_rx.clone(),
        "LogRowArchive",
        |s| s.safe_archive_hm(),
        |s| async move {
            let cleanup_settings = fetch_log_cleanup_settings(&s).await;
            archive_old_logs(&s, cleanup_settings.log_row_retention_days).await;
        },
    ));

    // 5d. 超期错误日志清理定时任务（错峰执行，默认 04:00）
    bg_handles.push(spawn_daily_scheduled_task(
        state.clone(),
        shutdown_rx.clone(),
        "ErrorLogsCleanup",
        |s| s.safe_error_clean_hm(),
        |s| async move {
            let cleanup_settings = fetch_log_cleanup_settings(&s).await;
            cleanup_error_logs(&s, cleanup_settings.error_log_retention_days).await;
        },
    ));

    // 5e. 临时存储与插件日志清理定时任务（错峰执行，默认 04:30）
    bg_handles.push(spawn_daily_scheduled_task(
        state.clone(),
        shutdown_rx.clone(),
        "StorageAndPluginCleanup",
        |s| s.safe_storage_clean_hm(),
        |s| async move {
            relay::tos_persist::cleanup_expired_files(&s).await;
            api::plugins::manager::cleanup_expired_plugin_logs(&s).await;
            #[cfg(feature = "commercial_plugins")]
            api::plugins::assets::cleanup_expired_volc_assets(&s).await;
        },
    ));

    // 6. 创作中心画布中断节点自动恢复与 3 分钟过期失败记录清理（每 60 秒）
    #[cfg(feature = "commercial_plugins")]
    bg_handles.push(spawn_cron_task(
        state.clone(),
        shutdown_rx.clone(),
        60,
        "PlaygroundCleanup",
        |s| async move {
            api::plugins::playground::cleanup_stale_playground_nodes(&s).await;
            api::plugins::playground_2026::cleanup_stale_playground_2026_nodes(&s).await;
            api::plugins::playground_2026::cleanup_failed_playground_2026_assets(&s).await;
            api::plugins::playground_2026::cleanup_expired_playground_2026_scratch(&s).await;
        },
    ));

    // 6b. 创作中心2026示例图：启动时若存储里还没有，用项目内置图写入固定路径
    #[cfg(feature = "commercial_plugins")]
    bg_handles.push({
        let state_clone = state.clone();
        tokio::spawn(async move {
            api::plugins::playground_2026::ensure_demo_image_on_startup(state_clone).await;
        })
    });

    // 7. 启动时检查站点图标文件完整性，缺失则自动恢复（一次性任务）
    #[cfg(feature = "plugin_site_icons")]
    bg_handles.push({
        let state_clone = state.clone();
        tokio::spawn(async move {
            api::plugins::site_icons::auto_recover_on_startup(state_clone).await;
        })
    });

    // 8. 启动时在后台静默执行历史数据回填与自检（一次性任务）
    bg_handles.push({
        let state_clone = state.clone();
        tokio::spawn(async move {
            // 稍稍休眠几秒，给应用服务器充分启动的时间，再开始历史回填与自检
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if let Err(e) =
                relay::usage_stats::backfill_usage_daily_stats_on_startup(&state_clone).await
            {
                tracing::warn!(
                    "❌ [StartupBackfill] 启动初始化使用量每日统计表失败: {:?}",
                    e
                );
            }
            // 启动自检：清理已过期的对象存储临时文件与记录
            relay::tos_persist::cleanup_expired_files(&state_clone).await;
        })
    });

    // 10. 火山方舟视频监控：同步视频列表 + 分账账单 + 超额熔断（每 1 分钟）
    #[cfg(feature = "commercial_plugins")]
    bg_handles.push(spawn_cron_task(
        state.clone(),
        shutdown_rx.clone(),
        60,
        "VolcArkMonitorSync",
        |s| async move {
            if let Err(e) = api::plugins::volc_ark_monitor::service::run_sync(s).await {
                tracing::warn!("❌ [CronArkMonitor] 火山方舟视频监控同步失败: {:?}", e);
            }
        },
    ));

    bg_handles
}

static MAINTENANCE_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// 执行日常全量系统维护任务（用量增量统计、日志大字段清理、错误日志清理、日志归档、临时存储清理等）
/// 严格遵循安全执行顺序：先聚合统计用量，再进行冷行删除归档。
pub async fn run_daily_maintenance_job(state: &AppState) {
    if MAINTENANCE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        tracing::warn!("⚠️ 当前已有系统维护任务正在后台执行中，跳过本次触发");
        return;
    }

    // 1. 每日用量增量统计（必须最优先执行！在日志行归档从 logs 表移走物理删除前，先将近 3 天数据增量聚合入库）
    if let Err(e) = relay::usage_stats::sync_daily_stats_force(state).await {
        tracing::warn!("系统维护: 每日用量增量统计执行失败: {:?}", e);
    }

    let cleanup_settings = fetch_log_cleanup_settings(state).await;

    // 2. 清理超期日志的大字段内容（仅置 NULL，保留日志记录，统计数据不受影响）
    cleanup_log_content(state, cleanup_settings.log_retention_days).await;

    // 3. 彻底删除超期已结案错误记录
    cleanup_error_logs(state, cleanup_settings.error_log_retention_days).await;

    // 4. 将超期日志行迁入 logs_archive 并从热表删除
    archive_old_logs(state, cleanup_settings.log_row_retention_days).await;

    // 5. 清理已过期的对象存储临时文件与插件日志
    relay::tos_persist::cleanup_expired_files(state).await;
    api::plugins::manager::cleanup_expired_plugin_logs(state).await;
    #[cfg(feature = "commercial_plugins")]
    api::plugins::assets::cleanup_expired_volc_assets(state).await;

    MAINTENANCE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
}

async fn fetch_log_cleanup_settings(state: &AppState) -> models::LogCleanupSettings {
    crate::api::settings::load_log_cleanup_settings(state)
        .await
        .unwrap_or_default()
}

/// 清理超期日志的大字段内容（request_content / response_content / upstream_req_content）
/// 仅置 NULL，不删除日志记录，统计数据不受影响
async fn cleanup_log_content(state: &AppState, retention_days: i64) {
    if retention_days <= 0 {
        return;
    }

    // 分批清理，每批 10000 条，避免长事务锁表
    let cleanup_sql = state.db.format_query(
        "UPDATE logs SET request_content = NULL, response_content = NULL, upstream_req_content = NULL, post_response = NULL \
         WHERE id IN (\
            SELECT id FROM logs \
            WHERE created_at < CURRENT_TIMESTAMP - (? * INTERVAL '1 day') \
              AND (request_content IS NOT NULL OR response_content IS NOT NULL OR upstream_req_content IS NOT NULL OR post_response IS NOT NULL) \
            LIMIT 10000\
         )"
    );

    loop {
        let result = sqlx::query(&cleanup_sql)
            .bind(retention_days as f64)
            .execute(&state.db.pool)
            .await;

        match result {
            Ok(r) => {
                let affected = r.rows_affected();
                if affected > 0 {
                    tracing::info!(
                        "🧹 日志清理: 已清理 {} 条超过 {} 天的日志详情",
                        affected,
                        retention_days
                    );
                }
                if affected < 10000 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(e) => {
                tracing::warn!("日志清理失败: {}", e);
                break;
            }
        }
    }
}

/// 将超期日志行迁入 logs_archive 并从热表删除（分批；`log_row_retention_days<=0` 时跳过）。
/// jsonb 按列名填充，热表加列后无需再给 archive 做列序体操。
async fn archive_old_logs(state: &AppState, row_days: i64) {
    if row_days <= 0 {
        return;
    }

    // 关键防序倒置保护：执行日志行归档前，必须确保当天的每日用量增量统计已先落地，杜绝漏算
    if let Err(e) = relay::usage_stats::sync_daily_stats(state).await {
        tracing::warn!("归档前预先执行用量统计失败: {:?}", e);
    }

    // 配置天数 +2 缓冲，降低「今日统计未落档就被迁走」的风险
    let effective_days = row_days.saturating_add(2);
    let archive_sql = state.db.format_query(
        "WITH candidates AS (\
            SELECT id FROM logs \
            WHERE created_at < CURRENT_TIMESTAMP - (? * INTERVAL '1 day') \
            ORDER BY created_at ASC LIMIT 5000\
         ), inserted AS (\
            INSERT INTO logs_archive (\
                id, user_id, channel_id, token_id, model, prompt_tokens, completion_tokens, cached_tokens, \
                cost, latency_ms, status_code, endpoint, error_message, upstream_url, request_content, response_content, \
                upstream_req_content, is_stream, billing_detail, billing_pid, forward_eid, created_at, log_id, task_id, \
                is_completed, pre_deduct_gift, action_type, channel_config_id, post_response, plugin_tag, is_ha, \
                upstream_request_id, billing_features, has_video, commission_posted, coupon_deduct, pre_deduct_voucher, archived_at\
            ) \
            SELECT \
                l.id, l.user_id, l.channel_id, l.token_id, l.model, l.prompt_tokens, l.completion_tokens, l.cached_tokens, \
                l.cost, l.latency_ms, l.status_code, l.endpoint, l.error_message, l.upstream_url, l.request_content, l.response_content, \
                l.upstream_req_content, l.is_stream, l.billing_detail, l.billing_pid, l.forward_eid, l.created_at, l.log_id, l.task_id, \
                l.is_completed, l.pre_deduct_gift, l.action_type, l.channel_config_id, l.post_response, l.plugin_tag, l.is_ha, \
                l.upstream_request_id, l.billing_features, l.has_video, l.commission_posted, l.coupon_deduct, l.pre_deduct_voucher, NOW() \
            FROM logs l \
            JOIN candidates c ON c.id = l.id \
            ON CONFLICT (id) DO NOTHING \
            RETURNING id\
         ) \
         DELETE FROM logs WHERE id IN (\
            SELECT id FROM inserted \
            UNION \
            SELECT c.id FROM candidates c \
            WHERE EXISTS (SELECT 1 FROM logs_archive a WHERE a.id = c.id)\
         )",
    );

    let mut total: u64 = 0;
    loop {
        let result = sqlx::query(&archive_sql)
            .bind(effective_days as f64)
            .execute(&state.db.pool)
            .await;

        match result {
            Ok(r) => {
                let affected = r.rows_affected();
                total += affected;
                if affected < 5000 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(e) => {
                tracing::warn!("日志归档失败: {}", e);
                break;
            }
        }
    }

    if total > 0 {
        tracing::info!(
            "📦 日志归档: 已迁入 logs_archive {} 条（保留热表 {}+2 天）",
            total,
            row_days
        );
    }
}

/// 分批删除指定表中非 200 且非 0（排除 pending 状态）已结案的超期错误记录
async fn delete_expired_error_logs(db: &crate::db::Database, table: &str, days: i64) -> u64 {
    let sql = db.format_query(&format!(
        "DELETE FROM {table} WHERE id IN (\
            SELECT id FROM {table} \
            WHERE created_at < CURRENT_TIMESTAMP - (? * INTERVAL '1 day') \
              AND status_code NOT IN (200, 0) \
              AND is_completed = 1 \
            LIMIT 5000\
         )"
    ));
    let mut total: u64 = 0;
    loop {
        match sqlx::query(&sql).bind(days as f64).execute(&db.pool).await {
            Ok(r) => {
                let affected = r.rows_affected();
                total += affected;
                if affected < 5000 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(e) => {
                tracing::warn!("错误日志清理失败 ({table}): {}", e);
                break;
            }
        }
    }
    total
}

/// 定期删除日志表 logs 与 logs_archive 中的超期错误记录（`error_log_retention_days <= 0` 时跳过）
async fn cleanup_error_logs(state: &AppState, days: i64) {
    if days <= 0 {
        return;
    }
    let total_logs = delete_expired_error_logs(&state.db, "logs", days).await;
    let total_archive = delete_expired_error_logs(&state.db, "logs_archive", days).await;

    if total_logs > 0 || total_archive > 0 {
        tracing::info!(
            "🧹 错误日志清理: 已彻底删除超过 {} 天的已结案错误记录 (status_code NOT IN (200, 0))，logs: {} 条, logs_archive: {} 条",
            days,
            total_logs,
            total_archive
        );
    }
}

/// 将 REGISTER_ENABLED 环境变量同步到数据库的 registration_settings
pub(crate) async fn sync_registration_settings(
    db: &Database,
    register_enabled: bool,
) -> anyhow::Result<()> {
    use crate::api::settings::default_registration_settings;

    let existing: Option<String> = sqlx::query_scalar(
        &db.format_query("SELECT value FROM settings WHERE key = 'registration_settings'"),
    )
    .fetch_optional(&db.pool)
    .await?;

    if existing.is_some() {
        return Ok(());
    }

    // 不存在时用默认值
    let mut s = default_registration_settings();
    s.enable_username_registration = register_enabled;
    s.enable_email_registration = false;
    s.enable_password_recovery = true;

    let val = serde_json::to_string(&s).unwrap_or_default();
    sqlx::query(&db.format_query("INSERT INTO settings (key, value) VALUES ('registration_settings', ?) ON CONFLICT(key) DO NOTHING"))
        .bind(val)
        .execute(&db.pool)
        .await?;

    tracing::info!(
        "📝 Registration settings initialized: username={}, email={}, mobile={}, password_recovery={}",
        s.enable_username_registration,
        s.enable_email_registration,
        s.enable_mobile_registration,
        s.enable_password_recovery,
    );

    Ok(())
}

impl AppState {
    /// 清空数据库后：丢掉旧业务内存态，不杀进程，前端可立刻进入全新安装
    pub async fn reset_runtime_after_db_wipe(&self) {
        self.login_codes.clear();
        self.dashboard_cache.clear();
        self.model_trend_cache.clear();
        self.failed_channels.clear();
        self.cascade_s2_inflight.clear();
        self.quota_memory.clear_all();
        crate::relay::relay_settings::invalidate_all();
    }
}

/// 抽象的高可用定时任务派发器，统一接管定时休眠、异常捕捉以及优雅退出监听
fn spawn_cron_task<F, Fut>(
    state: Arc<AppState>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    interval_secs: u64,
    task_name: &'static str,
    mut job: F,
) -> tokio::task::JoinHandle<()>
where
    F: FnMut(Arc<AppState>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut rx = shutdown_rx;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(interval_secs)) => {
                    job(state.clone()).await;
                }
                _ = rx.changed() => {
                    tracing::info!("[CronTask] {} 定时任务已优雅关闭退出", task_name);
                    return;
                }
            }
        }
    })
}

static DAILY_SCHEDULE_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 抽象的高可用每日错峰定时任务派发器，支持基于站点时区精准唤醒、配置变更热对齐以及优雅退出
fn spawn_daily_scheduled_task<F, Fut>(
    state: Arc<AppState>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    task_name: &'static str,
    get_hm: impl Fn(&models::LogCleanupSettings) -> (u32, u32) + Send + Sync + 'static,
    job: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(Arc<AppState>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut rx = shutdown_rx;
        let mut schedule_rx = api::settings::subscribe_schedule_changed();
        loop {
            let tz_name = relay::relay_settings::get_cached_site_timezone(&state.db).await;
            let cleanup_settings = fetch_log_cleanup_settings(&state).await;
            let (hour, min) = get_hm(&cleanup_settings);
            let wait = time_system::duration_until_next_local_hms(&tz_name, hour, min, 0);
            tokio::select! {
                _ = tokio::time::sleep(wait) => {
                    tracing::info!(
                        "⏰ [DailySchedule] 开始执行错峰任务: {} (目标 {:02}:{:02}, tz={})",
                        task_name,
                        hour,
                        min,
                        tz_name
                    );
                    let _guard = DAILY_SCHEDULE_MUTEX.lock().await;
                    job(state.clone()).await;
                    drop(_guard);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                res = schedule_rx.changed() => {
                    if res.is_err() {
                        return;
                    }
                    // 运维配置或时区变更，重新对齐休眠时间
                    continue;
                }
                _ = rx.changed() => {
                    tracing::info!("[DailySchedule] {} 定时任务已优雅关闭退出", task_name);
                    return;
                }
            }
        }
    })
}

/// 跨平台等待 SIGINT (Ctrl+C) / SIGTERM 关闭信号的异步辅助函数
async fn wait_for_shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("无法注册 SIGTERM 信号处理器");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
}
