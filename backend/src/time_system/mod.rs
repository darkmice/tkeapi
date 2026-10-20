/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! timesystem / timedisplay 解耦层
//!
//! - **timesystem**：全局固定 UTC+0，所有落库时间与进程运行时基准。
//! - **站点运营时区**（`default_timezone`）：计费时段、额度刷新、统计/佣金自然日。
//! - **展示时区**：管理后台 `admin_display_timezone`，用户端个人 `users.timezone`。

pub mod core;
pub mod db_ts;
pub mod period;

pub use core::{
    enforce_process_utc, parse_timedisplay, resolve_admin_display_name, resolve_timedisplay,
    resolve_user_end_timedisplay, try_iana_timezone_name, utc_naive_string, DEFAULT_TIMEDISPLAY,
    TIMESYSTEM_TZ,
};
pub use db_ts::DbTs;
pub use period::{
    duration_until_next_local_hms, local_day_bounds_utc, local_period_keys,
    quota_day_key_with_cutover, PeriodKeys,
};
