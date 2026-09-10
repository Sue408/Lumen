use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, Months, Timelike, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::db::with_db;
use crate::db::Db;
use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Day,
    Week,
    Month,
}

impl Period {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "day" => Ok(Period::Day),
            "week" => Ok(Period::Week),
            "month" => Ok(Period::Month),
            other => Err(AppError::message(format!("未知统计周期：{other}"))),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Period::Day => "day",
            Period::Week => "week",
            Period::Month => "month",
        }
    }

    fn bucket_count(self, start: DateTime<Local>) -> usize {
        match self {
            Period::Day => 24,
            Period::Week => 7,
            Period::Month => days_in_month(start) as usize,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Totals {
    calls: i64,
    input: i64,
    output: i64,
    cost: f64,
}

type LogRow = (DateTime<FixedOffset>, i64, i64, f64);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricDto {
    pub label: String,
    pub value: String,
    pub comparison: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostDto {
    pub name: String,
    pub cost: f64,
    pub tone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesDto {
    pub current: String,
    pub previous: String,
    pub current_values: Vec<f64>,
    pub previous_values: Vec<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageOverview {
    pub period_key: String,
    pub heading: String,
    pub summary_lead: String,
    pub summary_tail: String,
    pub metrics: Vec<MetricDto>,
    pub total_cost: f64,
    pub axis_labels: Vec<String>,
    pub y_axis_max: f64,
    pub series: SeriesDto,
    pub model_costs: Vec<ModelCostDto>,
}

const TONES: [&str; 4] = ["ochre", "indigo", "moss", "yellow"];
const MAX_MODEL_SLICES: usize = 4;

fn days_in_month(date: DateTime<Local>) -> u32 {
    let (year, month) = (date.year(), date.month());
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .expect("月份合法")
        .and_hms_opt(0, 0, 0)
        .expect("零点合法");
    let next = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("下月合法")
    .and_hms_opt(0, 0, 0)
    .expect("零点合法");
    (next - first).num_days() as u32
}

fn start_of_day(date: DateTime<Local>) -> DateTime<Local> {
    let naive = date.date_naive().and_hms_opt(0, 0, 0).expect("零点合法");
    naive
        .and_local_timezone(Local)
        .earliest()
        .unwrap_or(date)
}

pub fn period_start(period: Period, anchor: DateTime<Local>) -> DateTime<Local> {
    match period {
        Period::Day => start_of_day(anchor),
        Period::Week => {
            let offset = anchor.weekday().num_days_from_monday() as i64;
            start_of_day(anchor) - Duration::days(offset)
        }
        Period::Month => chrono::NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1)
            .expect("月初合法")
            .and_hms_opt(0, 0, 0)
            .expect("零点合法")
            .and_local_timezone(Local)
            .earliest()
            .unwrap_or(anchor),
    }
}

fn period_end(period: Period, start: DateTime<Local>) -> DateTime<Local> {
    match period {
        Period::Day => start + Duration::days(1),
        Period::Week => start + Duration::days(7),
        Period::Month => start
            .checked_add_months(Months::new(1))
            .unwrap_or(start + Duration::days(31)),
    }
}

fn previous_start(period: Period, start: DateTime<Local>) -> DateTime<Local> {
    match period {
        Period::Day => start - Duration::days(1),
        Period::Week => start - Duration::days(7),
        Period::Month => start
            .checked_sub_months(Months::new(1))
            .unwrap_or(start - Duration::days(30)),
    }
}

fn fetch_rows(
    conn: &Connection,
    start: DateTime<Local>,
    end: DateTime<Local>,
) -> Result<Vec<LogRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT occurred_at, input_tokens, output_tokens, cost
         FROM request_logs
         WHERE status = 'success' AND occurred_at >= ?1 AND occurred_at < ?2",
    )?;
    let rows = stmt.query_map(
        [
            start.with_timezone(&Utc).to_rfc3339(),
            end.with_timezone(&Utc).to_rfc3339(),
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
            ))
        },
    )?;
    let mut parsed = Vec::new();
    for row in rows {
        let (raw, input, output, cost) = row?;
        if let Ok(occurred) = DateTime::parse_from_rfc3339(&raw) {
            parsed.push((occurred, input, output, cost));
        }
    }
    Ok(parsed)
}

fn totals(rows: &[LogRow]) -> Totals {
    rows.iter().fold(Totals::default(), |acc, (_, input, output, cost)| Totals {
        calls: acc.calls + 1,
        input: acc.input + input,
        output: acc.output + output,
        cost: acc.cost + cost,
    })
}

fn bucket_index(period: Period, start: DateTime<Local>, moment: DateTime<Local>) -> Option<usize> {
    match period {
        Period::Day => Some(moment.hour() as usize),
        Period::Week => {
            let days = (moment.date_naive() - start.date_naive()).num_days();
            (days >= 0).then_some(days as usize)
        }
        Period::Month => Some((moment.day() as usize).saturating_sub(1)),
    }
}

fn cumulative_series(
    period: Period,
    start: DateTime<Local>,
    rows: &[LogRow],
) -> Vec<f64> {
    let count = period.bucket_count(start);
    let mut buckets = vec![0f64; count];
    for (occurred, input, output, _) in rows {
        let moment = occurred.with_timezone(&Local);
        if let Some(index) = bucket_index(period, start, moment) {
            if index < count {
                buckets[index] += (input + output) as f64 / 10_000.0;
            }
        }
    }
    let mut running = 0.0;
    buckets
        .into_iter()
        .map(|value| {
            running += value;
            round2(running)
        })
        .collect()
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

pub fn nice_axis_max(value: f64) -> f64 {
    if value <= 0.0 {
        return 10.0;
    }
    let exponent = value.log10().floor();
    let base = 10f64.powf(exponent);
    let normalized = value / base;
    let nice = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * base
}

fn format_thousands(value: i64) -> String {
    let digits = value.to_string();
    let mut output = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(ch);
    }
    output
}

fn format_tokens_wan(value: i64) -> String {
    format!("{:.1} 万", value as f64 / 10_000.0)
}

fn comparison(unit: &str, current: f64, previous: f64) -> String {
    if previous <= 0.0 {
        return "暂无同期数据".to_string();
    }
    let pct = (current - previous) / previous * 100.0;
    let sign = if pct >= 0.0 { "+" } else { "−" };
    format!("较{unit}同期 {sign}{:.0}%", pct.abs())
}

fn series_labels(period: Period, start: DateTime<Local>) -> (String, String) {
    match period {
        Period::Day => ("今天".to_string(), "昨天同期".to_string()),
        Period::Week => ("本周".to_string(), "上周同期".to_string()),
        Period::Month => (
            format!("{}月", start.month()),
            format!("{}月同期", previous_start(period, start).month()),
        ),
    }
}

fn axis_labels(period: Period) -> Vec<String> {
    match period {
        Period::Day => vec!["00:00", "06:00", "12:00", "18:00", "现在"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        Period::Week => vec!["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        Period::Month => vec!["1 日", "8 日", "15 日", "22 日", "今天"]
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

fn heading(period: Period, start: DateTime<Local>) -> String {
    match period {
        Period::Day => "今日总账".to_string(),
        Period::Week => "本周总账".to_string(),
        Period::Month => format!("{}月总账", start.month()),
    }
}

fn unit_word(period: Period) -> &'static str {
    match period {
        Period::Day => "昨天",
        Period::Week => "上周",
        Period::Month => "上月",
    }
}

fn model_costs(
    conn: &Connection,
    start: DateTime<Local>,
    end: DateTime<Local>,
) -> Result<Vec<ModelCostDto>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(upstream_model_name, route_alias, '未知') AS name, SUM(cost) AS cost
         FROM request_logs
         WHERE status = 'success' AND occurred_at >= ?1 AND occurred_at < ?2
         GROUP BY name
         ORDER BY cost DESC",
    )?;
    let rows = stmt.query_map(
        [
            start.with_timezone(&Utc).to_rfc3339(),
            end.with_timezone(&Utc).to_rfc3339(),
        ],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
    )?;
    let mut collected: Vec<(String, f64)> = Vec::new();
    for row in rows {
        let (name, cost) = row?;
        if cost > 0.0 {
            collected.push((name, cost));
        }
    }

    let mut costs: Vec<ModelCostDto> = Vec::new();
    if collected.len() <= MAX_MODEL_SLICES {
        for (index, (name, cost)) in collected.into_iter().enumerate() {
            costs.push(ModelCostDto {
                name,
                cost: round2(cost),
                tone: TONES[index % TONES.len()].to_string(),
            });
        }
        return Ok(costs);
    }

    let keep = MAX_MODEL_SLICES - 1;
    let mut rest = 0.0;
    for (index, (name, cost)) in collected.into_iter().enumerate() {
        if index < keep {
            costs.push(ModelCostDto {
                name,
                cost: round2(cost),
                tone: TONES[index % TONES.len()].to_string(),
            });
        } else {
            rest += cost;
        }
    }
    costs.push(ModelCostDto {
        name: "其他".to_string(),
        cost: round2(rest),
        tone: TONES[keep % TONES.len()].to_string(),
    });
    Ok(costs)
}

pub fn build_overview(
    conn: &Connection,
    period: Period,
    anchor: DateTime<Local>,
) -> Result<UsageOverview, AppError> {
    let start = period_start(period, anchor);
    let end = period_end(period, start);
    let prev_start = previous_start(period, start);

    let current_rows = fetch_rows(conn, start, end)?;
    let previous_rows = fetch_rows(conn, prev_start, start)?;
    let current = totals(&current_rows);
    let previous = totals(&previous_rows);

    let unit = unit_word(period);
    let metrics = vec![
        MetricDto {
            label: "调用次数".to_string(),
            value: format!("{} 次", format_thousands(current.calls)),
            comparison: comparison(unit, current.calls as f64, previous.calls as f64),
        },
        MetricDto {
            label: "输入 Tokens".to_string(),
            value: format_tokens_wan(current.input),
            comparison: comparison(unit, current.input as f64, previous.input as f64),
        },
        MetricDto {
            label: "输出 Tokens".to_string(),
            value: format_tokens_wan(current.output),
            comparison: comparison(unit, current.output as f64, previous.output as f64),
        },
        MetricDto {
            label: "总花费".to_string(),
            value: format!("¥ {:.2}", current.cost),
            comparison: comparison(unit, current.cost, previous.cost),
        },
    ];

    let current_series = cumulative_series(period, start, &current_rows);
    let previous_series = cumulative_series(period, prev_start, &previous_rows);
    let max = current_series
        .iter()
        .chain(previous_series.iter())
        .copied()
        .fold(0.0f64, f64::max);

    let cost_pct = if previous.cost > 0.0 {
        Some((current.cost - previous.cost) / previous.cost * 100.0)
    } else {
        None
    };
    let lead = if current.calls >= previous.calls {
        "本期调用保持活跃"
    } else {
        "本期调用有所回落"
    };
    let tail = match cost_pct {
        Some(pct) if pct >= 0.0 => format!("，花费较上期增加 {:.0}%。", pct),
        Some(pct) => format!("，花费较上期下降 {:.0}%。", pct.abs()),
        None => "，暂无上期可比数据。".to_string(),
    };

    let (current_label, previous_label) = series_labels(period, start);
    let model_costs = model_costs(conn, start, end)?;

    Ok(UsageOverview {
        period_key: period.key().to_string(),
        heading: heading(period, start),
        summary_lead: lead.to_string(),
        summary_tail: tail,
        metrics,
        total_cost: round2(current.cost),
        axis_labels: axis_labels(period),
        y_axis_max: nice_axis_max(max),
        series: SeriesDto {
            current: current_label,
            previous: previous_label,
            current_values: current_series,
            previous_values: previous_series,
        },
        model_costs,
    })
}

pub async fn query_overview(
    db: &Db,
    period: Period,
    anchor: DateTime<Local>,
) -> Result<UsageOverview, AppError> {
    with_db(db, move |conn| build_overview(conn, period, anchor)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::logs::insert_log;
    use crate::db::models::RequestLog;
    use crate::db::open_in_memory;

    fn sample_log(occurred: DateTime<Local>, input: i64, output: i64, cost: f64, model: &str) -> RequestLog {
        RequestLog {
            id: uuid::Uuid::new_v4().to_string(),
            occurred_at: occurred.with_timezone(&Utc).to_rfc3339(),
            endpoint: "/v1/chat/completions".to_string(),
            method: "POST".to_string(),
            route_alias: Some(format!("lumen/{model}")),
            route_id: None,
            upstream_model_id: None,
            upstream_model_name: Some(model.to_string()),
            model_real: None,
            provider_id: None,
            virtual_key_id: None,
            kind: "chat".to_string(),
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            reasoning_tokens: 0,
            cost,
            usage_source: "provider".to_string(),
            status: "success".to_string(),
            http_status: Some(200),
            latency_ms: Some(10),
            error_message: None,
            request_id: None,
            is_stream: false,
        }
    }

    #[test]
    fn axis_max_rounds_up_to_nice_value() {
        assert_eq!(nice_axis_max(0.0), 10.0);
        assert_eq!(nice_axis_max(3.0), 5.0);
        assert_eq!(nice_axis_max(47.0), 50.0);
        assert_eq!(nice_axis_max(404.0), 500.0);
    }

    #[test]
    fn overview_totals_and_model_costs_from_logs() {
        let db = open_in_memory().unwrap();
        let now = Local::now();
        let today = period_start(Period::Day, now);
        {
            let conn = db.lock().unwrap();
            insert_log(&conn, &sample_log(today + Duration::hours(1), 1000, 500, 0.5, "alpha")).unwrap();
            insert_log(&conn, &sample_log(today + Duration::hours(2), 3000, 1000, 1.5, "beta")).unwrap();
            insert_log(&conn, &sample_log(today + Duration::hours(3), 1000, 250, 0.25, "alpha")).unwrap();
            // 昨天的数据不应计入今日
            insert_log(&conn, &sample_log(today - Duration::hours(2), 9999, 9999, 9.0, "alpha")).unwrap();
        }
        let conn = db.lock().unwrap();
        let overview = build_overview(&conn, Period::Day, now).unwrap();

        assert_eq!(overview.total_cost, 2.25);
        assert_eq!(overview.metrics[0].value, "3 次");
        assert_eq!(overview.metrics[3].value, "¥ 2.25");
        assert_eq!(overview.model_costs.len(), 2);
        assert_eq!(overview.model_costs[0].name, "beta");
        assert_eq!(overview.model_costs[1].name, "alpha");
        assert_eq!(overview.model_costs[0].cost, 1.5);
        // 输入 tokens: 5000 → 0.5 万
        assert_eq!(overview.metrics[1].value, "0.5 万");
    }
}
