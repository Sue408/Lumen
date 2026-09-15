use chrono::{DateTime, Duration, Local, Months, Timelike, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::db::stats::Period;
use crate::db::with_db;
use crate::db::Db;
use crate::error::AppError;
use crate::util::{days_in_month, round4};

/// 生成速度与连通性的默认回看窗口：近 24 小时。
pub const DEFAULT_WINDOW_HOURS: i64 = 24;
/// 允许的最大回看窗口，避免前端传超大值拖垮查询。
pub const MAX_WINDOW_HOURS: i64 = 24 * 30;

pub fn clamp_window(hours: Option<i64>) -> i64 {
    hours
        .unwrap_or(DEFAULT_WINDOW_HOURS)
        .clamp(1, MAX_WINDOW_HOURS)
}

/// 一个桶的吞吐：日视图为整点桶，周 / 月视图为整天桶。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThroughputBucket {
    /// 桶起点的本地时间，RFC3339。
    pub at: String,
    pub tokens: i64,
    pub requests: i64,
    /// 该桶的 tokens/s（小时桶除以 3600，天桶除以 86400）。
    pub tokens_per_sec: f64,
}

/// 某个周期（日 / 周 / 月）的吞吐与周期均值。桶粒度随周期变化：
/// 日 = 24 个整点、周 = 7 天、月 = 当月天数。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Throughput {
    /// "day" | "week" | "month"
    pub period: String,
    pub total_requests: i64,
    pub total_tokens: i64,
    /// 周期平均吞吐（周期总 token / 周期总秒数）。
    pub tokens_per_sec: f64,
    pub peak_tokens_per_sec: f64,
    pub buckets: Vec<ThroughputBucket>,
}

/// 单个上游模型的流式生成速度。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationSpeed {
    pub model: String,
    pub tokens_per_sec: f64,
    /// 参与统计的流式样本数。
    pub samples: i64,
}

/// 单个提供商的近 N 小时连通性（被动口径：只在有流量时更新）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnectivity {
    pub provider_id: String,
    pub provider_name: Option<String>,
    pub total: i64,
    pub success: i64,
    pub error: i64,
    pub success_rate: f64,
    pub avg_latency_ms: Option<f64>,
    pub last_success_at: Option<String>,
    pub last_error_at: Option<String>,
}

fn period_key(period: Period) -> &'static str {
    match period {
        Period::Day => "day",
        Period::Week => "week",
        Period::Month => "month",
    }
}

/// SQL 侧分桶（本地时区）：与 `db/stats.rs` 的用量总览保持同一套桶语义，
/// 保证吞吐曲线与花费曲线的横轴对齐。
fn bucket_expr(period: Period) -> &'static str {
    match period {
        Period::Day => "CAST(strftime('%H', occurred_at, 'localtime') AS INTEGER)",
        Period::Week => "(CAST(strftime('%w', occurred_at, 'localtime') AS INTEGER) + 6) % 7",
        Period::Month => "CAST(strftime('%d', occurred_at, 'localtime') AS INTEGER) - 1",
    }
}

fn bucket_count(period: Period, start: DateTime<Local>) -> usize {
    match period {
        Period::Day => 24,
        Period::Week => 7,
        Period::Month => days_in_month(start) as usize,
    }
}

fn period_end(period: Period, start: DateTime<Local>) -> DateTime<Local> {
    match period {
        Period::Day => start + Duration::days(1),
        Period::Week => start + Duration::days(7),
        Period::Month => start
            .checked_add_months(Months::new(1))
            .unwrap_or_else(|| start + Duration::days(31)),
    }
}

fn utc_string(value: DateTime<Local>) -> String {
    value.with_timezone(&Utc).to_rfc3339()
}

/// 按周期分桶的吞吐。桶粒度：日 = 1 小时，周 / 月 = 1 天。
pub fn period_throughput(
    conn: &Connection,
    period: Period,
    anchor: DateTime<Local>,
    now: DateTime<Local>,
) -> Result<Throughput, AppError> {
    let start = crate::db::stats::period_start(period, anchor);
    let end = period_end(period, start);
    // 周期平均的分母是「已过时长」：当前周期只算到今天，历史周期用完整周期。
    let effective_end = if now < end { now } else { end };
    let elapsed_seconds = (effective_end - start).num_seconds().max(1) as f64;
    let count = bucket_count(period, start);
    let is_daily = period == Period::Day;
    let bucket_seconds = if is_daily { 3600.0 } else { 86_400.0 };
    let bucket_hours = if is_daily { 1 } else { 24 };

    let sql = format!(
        "SELECT {}, COALESCE(SUM(total_tokens), 0), COUNT(*)
           FROM request_logs
          WHERE occurred_at >= ?1 AND occurred_at < ?2
          GROUP BY 1",
        bucket_expr(period)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([utc_string(start), utc_string(end)], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let mut tokens_by_bucket = vec![0i64; count];
    let mut requests_by_bucket = vec![0i64; count];
    for row in rows {
        let (bucket, tokens, requests) = row?;
        if bucket >= 0 && (bucket as usize) < count {
            tokens_by_bucket[bucket as usize] = tokens;
            requests_by_bucket[bucket as usize] = requests;
        }
    }

    let mut buckets = Vec::with_capacity(count);
    let mut total_tokens = 0i64;
    let mut total_requests = 0i64;
    let mut peak = 0.0f64;
    for index in 0..count {
        let at = start + Duration::hours(index as i64 * bucket_hours);
        let tokens = tokens_by_bucket[index];
        let requests = requests_by_bucket[index];
        let tokens_per_sec = tokens as f64 / bucket_seconds;
        total_tokens += tokens;
        total_requests += requests;
        if tokens_per_sec > peak {
            peak = tokens_per_sec;
        }
        buckets.push(ThroughputBucket {
            at: at.to_rfc3339(),
            tokens,
            requests,
            tokens_per_sec: round4(tokens_per_sec),
        });
    }

    Ok(Throughput {
        period: period_key(period).to_string(),
        total_requests,
        total_tokens,
        tokens_per_sec: round4(total_tokens as f64 / elapsed_seconds),
        peak_tokens_per_sec: round4(peak),
        buckets,
    })
}

/// 对齐到当前整点，返回 `(窗口起点, 窗口终点)` 的本地时间。
fn aligned_window(
    now: DateTime<Local>,
    hours: i64,
) -> Result<(DateTime<Local>, DateTime<Local>), AppError> {
    let end = now
        .with_minute(0)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .ok_or_else(|| AppError::message("无法对齐整点"))?;
    let start = end - Duration::hours(hours - 1);
    Ok((start, end + Duration::hours(1)))
}

/// 流式生成速度：仅取「有首字耗时且生成时长 > 0」的成功记录，旧数据自动排除。
pub fn generation_speed(
    conn: &Connection,
    hours: i64,
    now: DateTime<Local>,
) -> Result<Vec<GenerationSpeed>, AppError> {
    let hours = clamp_window(Some(hours));
    let (start, end) = aligned_window(now, hours)?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(upstream_model_name, route_alias, '未知') AS model,
                SUM(output_tokens),
                SUM(latency_ms - ttfb_ms),
                COUNT(*)
           FROM request_logs
          WHERE status = 'success'
            AND is_stream = 1
            AND ttfb_ms IS NOT NULL
            AND latency_ms IS NOT NULL
            AND latency_ms > ttfb_ms
            AND occurred_at >= ?1 AND occurred_at < ?2
          GROUP BY model",
    )?;
    let rows = stmt.query_map([utc_string(start), utc_string(end)], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    let mut speeds = Vec::new();
    for row in rows {
        let (model, output_tokens, generation_ms, samples) = row?;
        if generation_ms <= 0 {
            continue;
        }
        speeds.push(GenerationSpeed {
            model,
            tokens_per_sec: round4(output_tokens as f64 / (generation_ms as f64 / 1000.0)),
            samples,
        });
    }
    speeds.sort_by(|a, b| {
        b.tokens_per_sec
            .partial_cmp(&a.tokens_per_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(speeds)
}

/// 被动连通性：按提供商聚合窗口内的成功 / 失败、成功率与成功请求的平均延迟。
pub fn connectivity(
    conn: &Connection,
    hours: i64,
    now: DateTime<Local>,
) -> Result<Vec<ProviderConnectivity>, AppError> {
    let hours = clamp_window(Some(hours));
    let (start, end) = aligned_window(now, hours)?;
    let mut stmt = conn.prepare(
        "SELECT l.provider_id,
                p.name,
                COUNT(*),
                COALESCE(SUM(CASE WHEN l.status = 'success' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN l.status = 'error' THEN 1 ELSE 0 END), 0),
                AVG(CASE WHEN l.status = 'success' THEN l.latency_ms END),
                MAX(CASE WHEN l.status = 'success' THEN l.occurred_at END),
                MAX(CASE WHEN l.status = 'error' THEN l.occurred_at END)
           FROM request_logs l
           LEFT JOIN providers p ON p.id = l.provider_id
          WHERE l.provider_id IS NOT NULL
            AND l.occurred_at >= ?1 AND l.occurred_at < ?2
          GROUP BY l.provider_id",
    )?;
    let rows = stmt.query_map([utc_string(start), utc_string(end)], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<f64>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (provider_id, provider_name, total, success, error, avg_latency, last_success, last_error) =
            row?;
        result.push(ProviderConnectivity {
            provider_id,
            provider_name,
            total,
            success,
            error,
            success_rate: if total > 0 {
                round4(success as f64 / total as f64)
            } else {
                0.0
            },
            avg_latency_ms: avg_latency.map(round4),
            last_success_at: last_success,
            last_error_at: last_error,
        });
    }
    Ok(result)
}

/// 一次取回三条遥测，避免前端多次 IPC。吞吐随 `period` / `anchor` 走，
/// 生成速度与连通性固定为近 24 小时。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySnapshot {
    pub throughput: Throughput,
    pub generation: Vec<GenerationSpeed>,
    pub connectivity: Vec<ProviderConnectivity>,
}

pub async fn query_snapshot(
    db: &Db,
    period: Period,
    anchor: DateTime<Local>,
) -> Result<TelemetrySnapshot, AppError> {
    with_db(db, move |conn| {
        let now = Local::now();
        Ok(TelemetrySnapshot {
            throughput: period_throughput(conn, period, anchor, now)?,
            generation: generation_speed(conn, DEFAULT_WINDOW_HOURS, now)?,
            connectivity: connectivity(conn, DEFAULT_WINDOW_HOURS, now)?,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::logs::insert_log;
    use crate::db::models::RequestLog;
    use crate::db::open_in_memory;
    use chrono::TimeZone;

    fn at(hour: u32, minute: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 9, 13, hour, minute, 0)
            .single()
            .unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn sample(
        occurred: DateTime<Local>,
        provider: Option<&str>,
        model: Option<&str>,
        status: &str,
        is_stream: bool,
        latency_ms: i64,
        ttfb_ms: Option<i64>,
        output_tokens: i64,
        total_tokens: i64,
    ) -> RequestLog {
        RequestLog {
            id: uuid::Uuid::new_v4().to_string(),
            occurred_at: occurred.with_timezone(&Utc).to_rfc3339(),
            endpoint: "/v1/chat/completions".to_string(),
            method: "POST".to_string(),
            route_alias: model.map(|name| format!("lumen/{name}")),
            route_id: None,
            upstream_model_id: None,
            upstream_model_name: model.map(str::to_string),
            model_real: None,
            provider_id: provider.map(str::to_string),
            virtual_key_id: None,
            kind: "chat".to_string(),
            input_tokens: total_tokens - output_tokens,
            output_tokens,
            total_tokens,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_in_input: false,
            reasoning_tokens: 0,
            cost: 0.0,
            usage_source: "provider".to_string(),
            status: status.to_string(),
            http_status: Some(if status == "error" { 500 } else { 200 }),
            latency_ms: Some(latency_ms),
            ttfb_ms,
            error_message: None,
            error_domain: None,
            error_kind: None,
            request_id: None,
            is_stream,
            attempt_index: 0,
            session_id: None,
            trace_id: None,
        }
    }

    #[test]
    fn period_throughput_uses_hourly_buckets_for_day() {
        let db = open_in_memory().unwrap();
        let now = at(2, 30);
        {
            let conn = db.lock().unwrap();
            insert_log(
                &conn,
                &sample(at(1, 10), Some("p1"), Some("m"), "success", false, 10, None, 0, 1200),
            )
            .unwrap();
            insert_log(
                &conn,
                &sample(at(2, 5), Some("p1"), Some("m"), "success", false, 10, None, 0, 600),
            )
            .unwrap();
        }
        let conn = db.lock().unwrap();
        let result = period_throughput(&conn, Period::Day, now, now).unwrap();
        assert_eq!(result.period, "day");
        assert_eq!(result.buckets.len(), 24);
        assert_eq!(result.total_requests, 2);
        assert_eq!(result.total_tokens, 1800);
        assert_eq!(result.buckets[1].tokens, 1200);
        assert_eq!(result.buckets[2].tokens, 600);
        // 1200 / 3600 = 0.3333（round4）
        assert!((result.buckets[1].tokens_per_sec - 0.3333).abs() < 1e-9);
    }

    #[test]
    fn period_throughput_uses_daily_buckets_for_week_and_month() {
        let db = open_in_memory().unwrap();
        let now = at(12, 0);
        let conn = db.lock().unwrap();
        let week = period_throughput(&conn, Period::Week, now, now).unwrap();
        assert_eq!(week.period, "week");
        assert_eq!(week.buckets.len(), 7);
        let month = period_throughput(&conn, Period::Month, now, now).unwrap();
        assert_eq!(month.period, "month");
        assert_eq!(month.buckets.len(), 30, "2026-09 有 30 天");
    }

    #[test]
    fn period_throughput_averages_over_elapsed_time() {
        let db = open_in_memory().unwrap();
        let now = at(12, 0);
        {
            let conn = db.lock().unwrap();
            insert_log(
                &conn,
                &sample(at(10, 0), Some("p1"), Some("m"), "success", false, 10, None, 0, 86_400),
            )
            .unwrap();
        }
        let conn = db.lock().unwrap();
        let result = period_throughput(&conn, Period::Week, now, now).unwrap();
        // 当前周未走完：分母是「周起点到现在」的已过秒数，而非整周。
        let start = crate::db::stats::period_start(Period::Week, now);
        let elapsed = (now - start).num_seconds() as f64;
        assert!(
            (result.tokens_per_sec - 86_400.0 / elapsed).abs() < 1e-3,
            "got {}",
            result.tokens_per_sec
        );
        // 峰值是单天桶：86400 / 86400 = 1 tok/s。
        assert!((result.peak_tokens_per_sec - 1.0).abs() < 1e-9);
    }

    #[test]
    fn period_throughput_uses_full_period_for_history() {
        let db = open_in_memory().unwrap();
        let now = at(12, 0);
        // 一个月前的周已经走完，分母应回到完整周期（7 × 86400 秒）。
        let anchor = now - Duration::days(30);
        let start = crate::db::stats::period_start(Period::Week, anchor);
        let end = period_end(Period::Week, start);
        let conn = db.lock().unwrap();
        let result = period_throughput(&conn, Period::Week, anchor, now).unwrap();
        assert_eq!(result.total_tokens, 0);
        // 分母语义可通过「同周期内插一条记录」间接验证：单天 86400 → 86400/604800 ≈ 0.1429。
        drop(conn);
        let occurred = start + Duration::hours(10);
        {
            let conn = db.lock().unwrap();
            insert_log(
                &conn,
                &sample(occurred, Some("p1"), Some("m"), "success", false, 10, None, 0, 86_400),
            )
            .unwrap();
        }
        let conn = db.lock().unwrap();
        let result = period_throughput(&conn, Period::Week, anchor, now).unwrap();
        let full = (end - start).num_seconds() as f64;
        assert!(
            (result.tokens_per_sec - 86_400.0 / full).abs() < 1e-3,
            "got {}",
            result.tokens_per_sec
        );
    }

    #[test]
    fn generation_speed_excludes_non_stream_and_missing_ttfb() {
        let db = open_in_memory().unwrap();
        let now = at(2, 30);
        {
            let conn = db.lock().unwrap();
            // 有效流式样本：output=1000，生成时长 = 2000-500 = 1500ms → 666.67 tok/s
            insert_log(
                &conn,
                &sample(at(1, 0), Some("p1"), Some("fast"), "success", true, 2000, Some(500), 1000, 2000),
            )
            .unwrap();
            insert_log(
                &conn,
                &sample(at(1, 10), Some("p1"), Some("fast"), "success", false, 2000, None, 1000, 2000),
            )
            .unwrap();
            insert_log(
                &conn,
                &sample(at(1, 20), Some("p1"), Some("fast"), "success", true, 2000, None, 1000, 2000),
            )
            .unwrap();
            insert_log(
                &conn,
                &sample(at(1, 30), Some("p1"), Some("fast"), "error", true, 2000, Some(500), 1000, 2000),
            )
            .unwrap();
        }
        let conn = db.lock().unwrap();
        let speeds = generation_speed(&conn, 24, now).unwrap();
        assert_eq!(speeds.len(), 1);
        assert_eq!(speeds[0].model, "fast");
        assert_eq!(speeds[0].samples, 1);
        assert!(
            (speeds[0].tokens_per_sec - 666.6667).abs() < 0.01,
            "got {}",
            speeds[0].tokens_per_sec
        );
    }

    #[test]
    fn connectivity_reports_success_rate_and_latency() {
        let db = open_in_memory().unwrap();
        let now = at(2, 30);
        {
            let conn = db.lock().unwrap();
            insert_log(&conn, &sample(at(1, 0), Some("p1"), Some("m"), "success", false, 100, None, 0, 10)).unwrap();
            insert_log(&conn, &sample(at(1, 5), Some("p1"), Some("m"), "success", false, 300, None, 0, 10)).unwrap();
            insert_log(&conn, &sample(at(1, 10), Some("p1"), Some("m"), "error", false, 50, None, 0, 0)).unwrap();
            // 无归属记录不计入连通性
            insert_log(&conn, &sample(at(1, 15), None, Some("m"), "error", false, 50, None, 0, 0)).unwrap();
        }
        let conn = db.lock().unwrap();
        let rows = connectivity(&conn, 24, now).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider_id, "p1");
        assert_eq!(rows[0].total, 3);
        assert_eq!(rows[0].success, 2);
        assert_eq!(rows[0].error, 1);
        assert!((rows[0].success_rate - 0.6667).abs() < 1e-9);
        assert_eq!(rows[0].avg_latency_ms, Some(200.0));
    }
}
