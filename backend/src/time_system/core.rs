/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! timesystem 核心：强制 UTC+0 运行基准与 timedisplay 解析

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

/// 底层系统时区：全局固定 UTC，不可被站点/用户配置覆盖。
pub const TIMESYSTEM_TZ: &str = "UTC";

const _: () = {
    let b = TIMESYSTEM_TZ.as_bytes();
    assert!(b.len() == 3 && b[0] == b'U' && b[1] == b'T' && b[2] == b'C');
};

/// 默认显示时区（仅作 timedisplay 回退，绝不作为 timesystem）。
pub const DEFAULT_TIMEDISPLAY: &str = "Asia/Shanghai";

/// 进程级锁定：强制 `TZ=UTC`，保证 `chrono::Local` / 日志时间与 timesystem 一致。
/// 应在 `main` 最早阶段调用一次。
pub fn enforce_process_utc() {
    // SAFETY: 仅在进程启动早期、尚未创建其它依赖 TZ 的线程前调用。
    unsafe {
        std::env::set_var("TZ", TIMESYSTEM_TZ);
    }
    #[cfg(unix)]
    {
        // 部分 libc 会缓存时区；重新解析当前 TZ。
        extern "C" {
            fn tzset();
        }
        unsafe {
            tzset();
        }
    }
    tracing::info!(
        "[TimeSystem] timesystem locked to {} (UTC+0)",
        TIMESYSTEM_TZ
    );
}

/// 当前 UTC 时刻（唯一允许作为“现在”写入业务逻辑的时钟源）。
#[inline]
pub fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

/// 格式化为 DB 友好的无偏移 UTC 朴素字符串：`YYYY-MM-DD HH:MM:SS`
#[inline]
pub fn format_utc_naive(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[inline]
pub fn utc_naive_string() -> String {
    format_utc_naive(now_utc())
}

/// 解析合法 IANA 时区（浏览器 `Intl` 返回值）；空/`local`/非法则 `None`，不回退。
fn try_parse_iana_timezone(name: &str) -> Option<Tz> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("local") {
        return None;
    }
    trimmed.parse().ok()
}

/// 合法 IANA 时区的规范名；空/`local`/非法则 `None`。
pub fn try_iana_timezone_name(name: &str) -> Option<String> {
    try_parse_iana_timezone(name).map(|tz| tz.name().to_string())
}

/// 解析 IANA timedisplay；非法时回退默认显示时区。
pub fn parse_timedisplay(name: &str) -> Tz {
    if let Some(tz) = try_parse_iana_timezone(name) {
        return tz;
    }
    let trimmed = name.trim();
    if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("local") {
        tracing::warn!(
            "[TimeSystem] invalid timedisplay '{}', fallback {}",
            trimmed,
            DEFAULT_TIMEDISPLAY
        );
    }
    DEFAULT_TIMEDISPLAY
        .parse()
        .unwrap_or(chrono_tz::Asia::Shanghai)
}

/// 用户端 timedisplay：合法请求头 / 个人时区，否则 timesystem UTC。
pub fn resolve_user_end_timedisplay(header_tz: Option<&str>, user_tz: Option<&str>) -> Tz {
    for raw in [header_tz, user_tz].into_iter().flatten() {
        if let Some(tz) = try_parse_iana_timezone(raw) {
            return tz;
        }
    }
    chrono_tz::UTC
}

/// timedisplay 优先级：请求头覆盖 > 用户个人时区 > 第三参回退 > 内置默认。
/// 管理端第三参是展示时区；业务统计自然日必须另走站点运营时区，不能用本函数。
pub fn resolve_timedisplay(
    header_tz: Option<&str>,
    user_tz: Option<&str>,
    site_default_tz: Option<&str>,
) -> Tz {
    if let Some(h) = header_tz.map(str::trim).filter(|s| !s.is_empty()) {
        return parse_timedisplay(h);
    }
    if let Some(u) = user_tz.map(str::trim).filter(|s| !s.is_empty()) {
        return parse_timedisplay(u);
    }
    if let Some(s) = site_default_tz.map(str::trim).filter(|s| !s.is_empty()) {
        return parse_timedisplay(s);
    }
    parse_timedisplay(DEFAULT_TIMEDISPLAY)
}

/// 管理后台展示时区：合法 IANA 优先，否则回退站点运营时区。
pub fn resolve_admin_display_name(admin_display: &str, site_operation: &str) -> String {
    if let Some(name) = try_iana_timezone_name(admin_display) {
        return name;
    }
    if let Some(name) = try_iana_timezone_name(site_operation) {
        return name;
    }
    DEFAULT_TIMEDISPLAY.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_display_falls_back_to_site_operation() {
        assert_eq!(
            resolve_admin_display_name("", "America/New_York"),
            "America/New_York"
        );
        assert_eq!(
            resolve_admin_display_name("Europe/London", "Asia/Shanghai"),
            "Europe/London"
        );
        assert_eq!(
            resolve_admin_display_name("not-a-zone", "Asia/Tokyo"),
            "Asia/Tokyo"
        );
        assert_eq!(resolve_admin_display_name("", ""), DEFAULT_TIMEDISPLAY);
    }
}
