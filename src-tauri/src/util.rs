//! 跨模块共享的纯工具：本地时区的周期起点与金额取整。
//! 额度（`gateway/quota.rs`）与统计（`db/stats.rs`）必须共用同一套周期口径，
//! 因此这些计算集中在此处，避免两处实现各自漂移。

use chrono::{DateTime, Datelike, Local};

/// 当天零点（本地时区）。时区解析失败时回退到原时刻。
pub fn start_of_day(date: DateTime<Local>) -> DateTime<Local> {
    start_of_date(date.year(), date.month(), date.day(), date)
}

/// 指定日期的零点（本地时区）。时区解析失败时回退到 `fallback`。
pub fn start_of_date(
    year: i32,
    month: u32,
    day: u32,
    fallback: DateTime<Local>,
) -> DateTime<Local> {
    chrono::NaiveDate::from_ymd_opt(year, month, day)
        .expect("日期合法")
        .and_hms_opt(0, 0, 0)
        .expect("零点合法")
        .and_local_timezone(Local)
        .earliest()
        .unwrap_or(fallback)
}

/// 当月天数。
pub fn days_in_month(date: DateTime<Local>) -> u32 {
    let first = chrono::NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("月份合法");
    let next = if date.month() == 12 {
        chrono::NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
    }
    .expect("下月合法");
    (next - first).num_days() as u32
}

/// 保留 2 位小数（金额展示口径）。
pub fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// 保留 4 位小数（比率口径）。
pub fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}
