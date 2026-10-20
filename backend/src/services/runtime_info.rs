/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 进程/主机运行时指标，供管理端「系统关于」页展示。

use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use md5::{Digest, Md5};
use serde_json::{json, Value};
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};

static PROCESS_STARTED_AT: OnceLock<DateTime<Utc>> = OnceLock::new();

/// 在 `main` 尽早调用，锁定进程启动时间。
pub fn mark_process_start() {
    let _ = PROCESS_STARTED_AT.set(Utc::now());
}

fn started_at() -> DateTime<Utc> {
    *PROCESS_STARTED_AT.get_or_init(Utc::now)
}

pub fn process_started_at_utc() -> String {
    started_at().format("%Y-%m-%d %H:%M:%S (UTC)").to_string()
}

pub fn process_uptime_secs() -> i64 {
    (Utc::now() - started_at()).num_seconds().max(0)
}

fn instance_name() -> String {
    std::env::var("INSTANCE_NAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("HOSTNAME")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| System::host_name().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "tokensbyte".to_string())
}

fn instance_id(name: &str) -> String {
    // Docker 默认 hostname 多为 12 位 hex；否则取名称哈希短码
    let trimmed = name.trim();
    if trimmed.len() == 12 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return trimmed.to_lowercase();
    }
    let digest = Md5::digest(trimmed.as_bytes());
    format!("{:x}", digest)[..12].to_string()
}

/// 覆盖当前工作目录的磁盘：占用百分比与总容量（字节）。
fn disk_snapshot() -> (f64, u64) {
    let disks = Disks::new_with_refreshed_list();
    let cwd = std::env::current_dir().ok();
    // (mount_len, available, total) — 优先覆盖 cwd 的最长挂载点
    let mut best: Option<(usize, u64, u64)> = None;

    for disk in disks.list() {
        let total = disk.total_space();
        if total == 0 {
            continue;
        }
        let mount = disk.mount_point();
        let covers_cwd = cwd
            .as_ref()
            .is_some_and(|cwd_path| cwd_path.starts_with(mount));
        if !covers_cwd {
            continue;
        }
        let mount_len = mount.as_os_str().len();
        let avail = disk.available_space();
        if best.is_none_or(|(len, _, _)| mount_len >= len) {
            best = Some((mount_len, avail, total));
        }
    }

    let (avail, total) = match best {
        Some((_, avail, total)) => (avail, total),
        None => match disks
            .list()
            .iter()
            .filter(|d| d.total_space() > 0)
            .max_by_key(|d| d.total_space())
        {
            Some(d) => (d.available_space(), d.total_space()),
            None => return (0.0, 0),
        },
    };

    if total == 0 {
        return (0.0, 0);
    }
    let used = total.saturating_sub(avail) as f64;
    ((used / total as f64) * 100.0, total)
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// 同步采集（应在 `spawn_blocking` 中调用；CPU 需短间隔二次刷新）。
pub fn collect(version: &str) -> Value {
    let name = instance_name();
    let id = instance_id(&name);

    let mut sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
            .with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    // 首次 refresh 建立基线
    sys.refresh_cpu_usage();
    std::thread::sleep(Duration::from_millis(200));
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpu = round1(f64::from(sys.global_cpu_usage()));
    let cpu_cores = sys.cpus().len().max(sys.physical_core_count().unwrap_or(0));
    let mem_total = sys.total_memory();
    let mem_used = sys.used_memory();
    let memory = if mem_total > 0 {
        round1((mem_used as f64 / mem_total as f64) * 100.0)
    } else {
        0.0
    };
    let (disk, disk_total) = disk_snapshot();
    let disk = round1(disk);

    let platform = format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH);
    let role = std::env::var("NODE_ROLE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "master".to_string());

    json!({
        "instance_name": name,
        "instance_id": id,
        "status": "online",
        "role": role,
        "cpu_percent": cpu,
        "cpu_cores": cpu_cores,
        "memory_percent": memory,
        "memory_total_bytes": mem_total,
        "disk_percent": disk,
        "disk_total_bytes": disk_total,
        "version": version,
        "platform": platform,
        "started_at": started_at().format("%Y-%m-%d %H:%M:%S (UTC)").to_string(),
    })
}

pub fn fallback(version: &str) -> Value {
    let name = instance_name();
    let id = instance_id(&name);
    json!({
        "instance_name": name,
        "instance_id": id,
        "status": "online",
        "role": std::env::var("NODE_ROLE").unwrap_or_else(|_| "master".to_string()),
        "cpu_percent": 0.0,
        "cpu_cores": 0,
        "memory_percent": 0.0,
        "memory_total_bytes": 0,
        "disk_percent": 0.0,
        "disk_total_bytes": 0,
        "version": version,
        "platform": format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        "started_at": started_at().format("%Y-%m-%d %H:%M:%S (UTC)").to_string(),
    })
}
