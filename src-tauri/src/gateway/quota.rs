use chrono::{DateTime, Datelike, Duration, Local};

use crate::db::models::{
    QUOTA_PERIOD_DAILY, QUOTA_PERIOD_MONTHLY, QUOTA_PERIOD_TOTAL, QUOTA_PERIOD_WEEKLY,
};
use crate::util::{start_of_date, start_of_day};

/// 额度计量周期。未知取值回退到自然月，保证配置损坏时行为可预测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaPeriod {
    Daily,
    Weekly,
    Monthly,
    Total,
}

impl QuotaPeriod {
    pub fn parse(value: &str) -> Self {
        match value {
            QUOTA_PERIOD_DAILY => QuotaPeriod::Daily,
            QUOTA_PERIOD_WEEKLY => QuotaPeriod::Weekly,
            QUOTA_PERIOD_MONTHLY => QuotaPeriod::Monthly,
            QUOTA_PERIOD_TOTAL => QuotaPeriod::Total,
            _ => QuotaPeriod::Monthly,
        }
    }
}

/// 当前额度周期的起点（本地时区）。周一起始，与 `db/stats.rs` 的周口径一致。
pub fn period_start(period: QuotaPeriod, now: DateTime<Local>) -> DateTime<Local> {
    match period {
        QuotaPeriod::Daily => start_of_day(now),
        QuotaPeriod::Weekly => {
            let offset = now.weekday().num_days_from_monday() as i64;
            start_of_day(now) - Duration::days(offset)
        }
        QuotaPeriod::Monthly => start_of_date(now.year(), now.month(), 1, now),
        QuotaPeriod::Total => start_of_date(1970, 1, 1, now),
    }
}

/// 是否已达 / 超过额度。无上限（`None`）永远放行；恰好等于上限视为超限。
pub fn is_over_quota(spent: f64, limit: Option<f64>) -> bool {
    match limit {
        Some(limit) => spent >= limit,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
    }

    fn assert_same_day(left: DateTime<Local>, right: DateTime<Local>) {
        assert_eq!(left.date_naive(), right.date_naive());
        assert_eq!((left.hour(), left.minute()), (0, 0));
    }

    #[test]
    fn daily_starts_at_midnight() {
        let start = period_start(QuotaPeriod::Daily, at(2026, 9, 11, 15, 30));
        assert_same_day(start, at(2026, 9, 11, 0, 0));
    }

    #[test]
    fn weekly_starts_on_monday_across_month_boundary() {
        // 2026-09-11 是周五，本周起点应回到 2026-09-07（周一）。
        let start = period_start(QuotaPeriod::Weekly, at(2026, 9, 11, 9, 0));
        assert_eq!(start.date_naive(), at(2026, 9, 7, 0, 0).date_naive());
    }

    #[test]
    fn weekly_stays_put_on_monday() {
        let start = period_start(QuotaPeriod::Weekly, at(2026, 9, 7, 6, 0));
        assert_eq!(start.date_naive(), at(2026, 9, 7, 0, 0).date_naive());
    }

    #[test]
    fn monthly_starts_on_first_day() {
        let start = period_start(QuotaPeriod::Monthly, at(2026, 9, 30, 23, 59));
        assert_eq!(start.date_naive(), at(2026, 9, 1, 0, 0).date_naive());
    }

    #[test]
    fn monthly_crosses_year_boundary() {
        let start = period_start(QuotaPeriod::Monthly, at(2027, 1, 3, 8, 0));
        assert_eq!(start.date_naive(), at(2027, 1, 1, 0, 0).date_naive());
    }

    #[test]
    fn total_starts_at_epoch() {
        let start = period_start(QuotaPeriod::Total, at(2026, 9, 11, 12, 0));
        assert_eq!(start.date_naive(), at(1970, 1, 1, 0, 0).date_naive());
    }

    #[test]
    fn parse_falls_back_to_monthly() {
        assert_eq!(QuotaPeriod::parse("daily"), QuotaPeriod::Daily);
        assert_eq!(QuotaPeriod::parse("weekly"), QuotaPeriod::Weekly);
        assert_eq!(QuotaPeriod::parse("total"), QuotaPeriod::Total);
        assert_eq!(QuotaPeriod::parse("monthly"), QuotaPeriod::Monthly);
        assert_eq!(QuotaPeriod::parse("garbage"), QuotaPeriod::Monthly);
    }

    #[test]
    fn unlimited_never_blocks() {
        assert!(!is_over_quota(1_000_000.0, None));
    }

    #[test]
    fn reaching_the_limit_blocks() {
        assert!(!is_over_quota(4.99, Some(5.0)));
        assert!(is_over_quota(5.0, Some(5.0)));
        assert!(is_over_quota(5.01, Some(5.0)));
    }

    #[test]
    fn zero_limit_blocks_everything() {
        assert!(is_over_quota(0.0, Some(0.0)));
    }
}
