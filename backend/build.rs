/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn format_commit_date(raw_date: &str) -> String {
    let trimmed = raw_date.trim();
    if trimmed.is_empty() {
        return "-".to_string();
    }
    if trimmed.contains("(UTC") || trimmed.ends_with("UTC") {
        return trimmed.to_string();
    }
    if let Some((dt, tz)) = trimmed.rsplit_once(' ') {
        if (tz.starts_with('+') || tz.starts_with('-')) && (tz.len() == 5 || tz.len() == 6) {
            let sign = &tz[0..1];
            let rest = &tz[1..];
            let parts: Vec<&str> = if rest.contains(':') {
                rest.split(':').collect()
            } else if rest.len() == 4 {
                vec![&rest[0..2], &rest[2..4]]
            } else {
                vec![]
            };
            if parts.len() == 2 {
                let hours: u32 = parts[0].parse().unwrap_or(0);
                let minutes: u32 = parts[1].parse().unwrap_or(0);
                let tz_suffix = if hours == 0 && minutes == 0 {
                    "(UTC)".to_string()
                } else if minutes == 0 {
                    format!("(UTC{}{})", sign, hours)
                } else {
                    format!("(UTC{}{}:{:02})", sign, hours, minutes)
                };
                return format!("{} {}", dt, tz_suffix);
            }
        }
    }
    trimmed.to_string()
}

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("git_commits.json");

    // 仅当当前项目根目录（../.git 或 .git）存在独立 Git 仓库时才运行 git log，避免子目录向上穿透读取父级商业版仓库
    let mut raw_log = None;
    if Path::new("../.git").exists() || Path::new(".git").exists() {
        if let Ok(out) = Command::new("git")
            .args([
                "log",
                "-10",
                "--format=%H\x1F%h\x1F%an\x1F%cd\x1F%s",
                "--date=format:%Y-%m-%d %H:%M:%S %z",
            ])
            .output()
        {
            if out.status.success() {
                let content = String::from_utf8_lossy(&out.stdout).to_string();
                if !content.trim().is_empty() {
                    raw_log = Some(content.clone());
                    // 自动更新本地的缓存文件，以便 Docker 编译时能直接拷贝进去使用
                    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
                        let git_log_path = Path::new(&manifest_dir).join("git_log.txt");
                        let _ = fs::write(git_log_path, content);
                    }
                }
            }
        }
    }

    // 若 git 命令不可用或未获取到内容，则尝试读取缓存的 git_log.txt 静态文件
    if raw_log.is_none() && Path::new("git_log.txt").exists() {
        if let Ok(content) = fs::read_to_string("git_log.txt") {
            raw_log = Some(content);
        }
    }

    let json_str = match raw_log {
        Some(raw) => {
            let mut commits = vec![];
            for (i, line) in raw.lines().filter(|l| !l.trim().is_empty()).enumerate() {
                let parts: Vec<&str> = line.splitn(5, '\x1F').collect();
                let version = format!("v1.0.{}", 10usize.saturating_sub(i));
                let hash = parts.first().unwrap_or(&"").to_string();
                let short_hash = parts.get(1).unwrap_or(&"").to_string();
                let raw_author = parts.get(2).unwrap_or(&"").to_string();
                let author = if raw_author.chars().count() > 2 {
                    let chars: Vec<char> = raw_author.chars().collect();
                    format!(
                        "{}***{}",
                        chars.first().unwrap_or(&'a'),
                        chars.last().unwrap_or(&'z')
                    )
                } else if raw_author.chars().count() == 2 {
                    let chars: Vec<char> = raw_author.chars().collect();
                    format!("{}*", chars.first().unwrap_or(&'a'))
                } else {
                    raw_author
                };
                let date = format_commit_date(parts.get(3).unwrap_or(&""));
                let message = parts.get(4).unwrap_or(&"").replace("\n", " ");

                commits.push(serde_json::json!({
                    "index": i,
                    "is_current": i == 0,
                    "version": version,
                    "hash": hash,
                    "short_hash": short_hash,
                    "author": author,
                    "date": date,
                    "message": message
                }));
            }
            if commits.is_empty() {
                serde_json::to_string(&vec![serde_json::json!({
                    "index": 0, "is_current": true, "version": "v1.0.0", "hash": "-", "short_hash": "Release", "author": "TokensByte System", "date": "2026", "message": "生产环境已构建 (无可用的提交记录)"
                })]).unwrap()
            } else {
                serde_json::to_string(&commits).unwrap()
            }
        }
        None => {
            // 后备数据（Fallback）：如果在构建环境没有任何 git 以及 .git 实体文件时的安全托底方案
            serde_json::to_string(&vec![serde_json::json!({
                "index": 0, "is_current": true, "version": "v1.0.0", "hash": "-", "short_hash": "Release", "author": "TokensByte System", "date": "2026", "message": "生产环境已构建 (Git 未集成)"
            })]).unwrap()
        }
    };

    fs::write(&dest_path, json_str).unwrap();

    // 监测对应更改以触发更新，保障热重载
    // 始终监听 git_log.txt：Docker 容器内无 .git 时靠它触发 build.rs 重跑
    println!("cargo:rerun-if-changed=git_log.txt");
    if Path::new("../.git/HEAD").exists() {
        println!("cargo:rerun-if-changed=../.git/HEAD");
    } else if Path::new(".git/HEAD").exists() {
        println!("cargo:rerun-if-changed=.git/HEAD");
    } else {
        // 无 git 时不强制重构（git_log.txt 已覆盖）
    }

    // 自动检测插件文件/目录的存在性，实现代码级别的解耦和优雅降级
    // Paths may be a single `.rs` file or a plugin directory (`mod.rs` inside).
    let optional_plugins = vec![
        ("src/api/plugins/redemptions", "plugin_redemptions"),
        ("src/models/redemption.rs", "plugin_redemptions_model"),
        ("src/api/plugins/finance", "plugin_finance"),
        ("src/api/plugins/team_marketing", "plugin_team_marketing"),
        ("src/api/plugins/marketing_games", "plugin_marketing_games"),
        ("src/api/plugins/pay/payment", "plugin_payment"),
        ("src/api/plugins/pay", "plugin_pay"),
    ];

    for (path_str, cfg_name) in optional_plugins {
        let path = Path::new(path_str);
        // 注册自定义的条件编译配置名，消除新版 rustc 编译时的未识别警告
        println!("cargo:rustc-check-cfg=cfg({})", cfg_name);
        if path.exists() {
            println!("cargo:rustc-cfg={}", cfg_name);
        }
        // 让 cargo 在文件出现或删除时重新运行 build.rs
        println!("cargo:rerun-if-changed={}", path_str);
    }

    // 注册商业宏 feature 值，保障开源版剥离 Cargo.toml feature 后仍可通过 rustc 1.80+ 审计
    println!("cargo:rustc-check-cfg=cfg(feature, values(\"commercial_plugins\", \"plugin_comfyui\", \"plugin_volcengine_enhance\", \"plugin_tencent_enhance\", \"plugin_data_sync\", \"plugin_content_security\"))");
}
