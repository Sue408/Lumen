use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, Local, Months, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::db::attribution::{build_attribution, Attribution};
use crate::db::with_db;
use crate::db::Db;
use crate::error::AppError;
use crate::util::{days_in_month, round2, round4, start_of_date, start_of_day};

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
    cache_read: i64,
    cache_denom: i64,
}

/// 日志过滤范围：全部 / 未归属（历史 NULL）/ 指定密钥。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyScope {
    All,
    Unassigned,
    Key(String),
}

/// 前端请求「未归属」桶时使用的哨兵值。
pub const UNASSIGNED_SENTINEL: &str = "__unassigned__";

impl KeyScope {
    pub fn from_filter(value: Option<String>) -> Self {
        match value {
            None => KeyScope::All,
            Some(value) if value == UNASSIGNED_SENTINEL => KeyScope::Unassigned,
            Some(value) if value.is_empty() => KeyScope::All,
            Some(value) => KeyScope::Key(value),
        }
    }

    fn clause(&self) -> &'static str {
        match self {
            KeyScope::All => "",
            KeyScope::Unassigned => " AND virtual_key_id IS NULL",
            KeyScope::Key(_) => " AND virtual_key_id = ?3",
        }
    }

    fn param(&self) -> Option<&str> {
        match self {
            KeyScope::Key(id) => Some(id),
            _ => None,
        }
    }
}

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
}

/// 堆叠图的一层：某密钥或某模型在本期的累积花费（美元）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLayer {
    pub name: String,
    /// `ochre` / `indigo` / `moss` / `yellow`；「未归属」与「其他」为 `ink`。
    pub tone: String,
    pub values: Vec<f64>,
    pub amount: f64,
}

/// 用量质量：命中率 / 失败率 / 推理占比。统计覆盖全部记录（含失败）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    pub cache_hit_rate: f64,
    pub error_rate: f64,
    pub reasoning_share: f64,
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
    pub layers: Vec<UsageLayer>,
    pub attribution: Attribution,
    pub quality: Quality,
    pub model_costs: Vec<ModelCostDto>,
}

const TONES: [&str; 4] = ["ochre", "indigo", "moss", "yellow"];
const MAX_MODEL_SLICES: usize = 4;

/// 缓存命中率的分母（输入侧总量）：已含命中时只补缓存写入，否则命中与写入都计入。
/// 口径与 `db/models.rs::contains_cache_read` 一致。
const CACHE_DENOM_SQL: &str = "CASE WHEN cache_read_in_input = 1
             THEN input_tokens + cache_creation_tokens
             ELSE input_tokens + cache_read_tokens + cache_creation_tokens END";

pub fn period_start(period: Period, anchor: DateTime<Local>) -> DateTime<Local> {
    match period {
        Period::Day => start_of_day(anchor),
        Period::Week => {
            let offset = anchor.weekday().num_days_from_monday() as i64;
            start_of_day(anchor) - Duration::days(offset)
        }
        Period::Month => start_of_date(anchor.year(), anchor.month(), 1, anchor),
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

/// SQL 侧分桶：把 UTC 的 `occurred_at` 折算到本地时区后取桶号，
/// 聚合在 SQLite 里完成，整表行不再回 Rust 逐行解析。
fn bucket_expr(period: Period) -> &'static str {
    match period {
        Period::Day => "CAST(strftime('%H', occurred_at, 'localtime') AS INTEGER)",
        Period::Week => "(CAST(strftime('%w', occurred_at, 'localtime') AS INTEGER) + 6) % 7",
        Period::Month => "CAST(strftime('%d', occurred_at, 'localtime') AS INTEGER) - 1",
    }
}

/// 时间范围 + 可选 key 的绑定参数，供各条统计查询共用。
fn bind_range(
    start: DateTime<Local>,
    end: DateTime<Local>,
    scope: &KeyScope,
) -> Vec<Box<dyn rusqlite::ToSql>> {
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(start.with_timezone(&Utc).to_rfc3339()),
        Box::new(end.with_timezone(&Utc).to_rfc3339()),
    ];
    if let Some(key) = scope.param() {
        values.push(Box::new(key.to_string()));
    }
    values
}

fn layer_expr(scope: &KeyScope) -> &'static str {
    match scope {
        KeyScope::All => "COALESCE(k.name, '未归属')",
        _ => "COALESCE(upstream_model_name, route_alias, '未知')",
    }
}

fn accumulate(buckets: Vec<f64>) -> Vec<f64> {
    let mut running = 0.0;
    buckets
        .into_iter()
        .map(|value| {
            running += value;
            round2(running)
        })
        .collect()
}

/// 单个周期的全部「成功请求」口径：合计、逐桶 token 累积序列、分层、
/// 分层合计（不截断，供归因）、按模型花费。一次 (桶, 层, 模型) 的
/// GROUP BY 扫描即可全部导出，避免同一周期被反复整表读。
struct PeriodStats {
    totals: Totals,
    series: Vec<f64>,
    layers: Vec<UsageLayer>,
    layer_totals: Vec<(String, f64)>,
    model_costs: Vec<ModelCostDto>,
}

fn period_stats(
    conn: &Connection,
    period: Period,
    start: DateTime<Local>,
    scope: &KeyScope,
) -> Result<PeriodStats, AppError> {
    let count = period.bucket_count(start);
    let sql = format!(
        "SELECT {},
                {} AS layer,
                COALESCE(upstream_model_name, route_alias, '未知') AS model,
                SUM(cost),
                SUM(input_tokens),
                SUM(output_tokens),
                COUNT(*),
                SUM(cache_read_tokens),
                SUM({CACHE_DENOM_SQL})
         FROM request_logs
         LEFT JOIN virtual_keys k ON k.id = request_logs.virtual_key_id
         WHERE request_logs.status = 'success'
           AND request_logs.occurred_at >= ?1 AND request_logs.occurred_at < ?2{}
         GROUP BY 1, 2, 3",
        bucket_expr(period),
        layer_expr(scope),
        scope.clause()
    );
    let values = bind_range(start, period_end(period, start), scope);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(values.iter().map(|value| value.as_ref())),
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        },
    )?;

    let mut totals = Totals::default();
    let mut tokens = vec![0f64; count];
    let mut layer_totals: BTreeMap<String, f64> = BTreeMap::new();
    let mut layer_buckets: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut model_totals: BTreeMap<String, f64> = BTreeMap::new();

    for row in rows {
        let (bucket, layer, model, cost, input, output, calls, cache_read, cache_denom) = row?;
        totals.calls += calls;
        totals.input += input;
        totals.output += output;
        totals.cost += cost;
        totals.cache_read += cache_read;
        totals.cache_denom += cache_denom;
        if bucket >= 0 && (bucket as usize) < count {
            tokens[bucket as usize] += (input + output) as f64 / 10_000.0;
            layer_buckets
                .entry(layer.clone())
                .or_insert_with(|| vec![0f64; count])[bucket as usize] += cost;
        }
        *layer_totals.entry(layer).or_insert(0.0) += cost;
        *model_totals.entry(model).or_insert(0.0) += cost;
    }

    let mut ranked: Vec<(String, f64)> = layer_totals.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(PeriodStats {
        totals,
        series: accumulate(tokens),
        layers: assemble_layers(&ranked, &mut layer_buckets, count),
        layer_totals: ranked,
        model_costs: assemble_model_costs(model_totals),
    })
}

/// 取花费前 4 层 + 「其他」；「未归属」与「其他」走墨阶，不占矿物颜料。
fn assemble_layers(
    ranked: &[(String, f64)],
    buckets: &mut BTreeMap<String, Vec<f64>>,
    count: usize,
) -> Vec<UsageLayer> {
    let keep = if ranked.len() <= MAX_MODEL_SLICES {
        ranked.len()
    } else {
        MAX_MODEL_SLICES - 1
    };
    let rest_total: f64 = ranked.iter().skip(keep).map(|(_, cost)| cost).sum();

    let mut layers: Vec<UsageLayer> = Vec::new();
    let mut tone_index = 0usize;
    for (name, amount) in ranked.iter().take(keep) {
        let tone = if name == "未归属" {
            "ink".to_string()
        } else {
            let tone = TONES[tone_index % TONES.len()].to_string();
            tone_index += 1;
            tone
        };
        layers.push(UsageLayer {
            name: name.clone(),
            tone,
            values: accumulate(buckets.remove(name).unwrap_or_else(|| vec![0f64; count])),
            amount: round2(*amount),
        });
    }
    if ranked.len() > keep {
        let mut rest = vec![0f64; count];
        for values in buckets.values() {
            for (index, value) in values.iter().enumerate() {
                rest[index] += value;
            }
        }
        layers.push(UsageLayer {
            name: "其他".to_string(),
            tone: "ink".to_string(),
            values: accumulate(rest),
            amount: round2(rest_total),
        });
    }
    layers
}

/// 按模型花费排序，取前 4 + 「其他」。
fn assemble_model_costs(totals: BTreeMap<String, f64>) -> Vec<ModelCostDto> {
    let mut collected: Vec<(String, f64)> = totals
        .into_iter()
        .filter(|(_, cost)| *cost > 0.0)
        .collect();
    collected.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if collected.len() <= MAX_MODEL_SLICES {
        return collected
            .into_iter()
            .enumerate()
            .map(|(index, (name, cost))| ModelCostDto {
                name,
                cost: round2(cost),
                tone: TONES[index % TONES.len()].to_string(),
            })
            .collect();
    }

    let keep = MAX_MODEL_SLICES - 1;
    let mut costs: Vec<ModelCostDto> = Vec::new();
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
    costs
}

fn quality_totals(
    conn: &Connection,
    start: DateTime<Local>,
    end: DateTime<Local>,
    scope: &KeyScope,
) -> Result<Quality, AppError> {
    let sql = format!(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM({CACHE_DENOM_SQL}), 0),
                COALESCE(SUM(reasoning_tokens), 0),
                COALESCE(SUM(output_tokens), 0)
         FROM request_logs
         WHERE occurred_at >= ?1 AND occurred_at < ?2{}",
        scope.clause()
    );
    let start = start.with_timezone(&Utc).to_rfc3339();
    let end = end.with_timezone(&Utc).to_rfc3339();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(start), Box::new(end)];
    if let Some(key) = scope.param() {
        values.push(Box::new(key.to_string()));
    }
    let mut stmt = conn.prepare(&sql)?;
    let (count, errors, cache_read, cache_denom, reasoning, output) = stmt.query_row(
        rusqlite::params_from_iter(values.iter().map(|value| value.as_ref())),
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        },
    )?;
    Ok(Quality {
        cache_hit_rate: if cache_denom > 0 {
            round4(cache_read as f64 / cache_denom as f64)
        } else {
            0.0
        },
        error_rate: if count > 0 {
            round4(errors as f64 / count as f64)
        } else {
            0.0
        },
        reasoning_share: if output > 0 {
            round4(reasoning as f64 / output as f64)
        } else {
            0.0
        },
    })
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
    if value == 0 {
        return "0".to_string();
    }
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

struct LayerTotals {
    totals: Totals,
    layer_totals: Vec<(String, f64)>,
}

/// 上期只需要合计与分层合计（供指标对比与归因），不需要分桶、不需要按模型，
/// 因此单独走一条只 `GROUP BY 层` 的轻查询——连 `strftime` 都不必逐行算。
fn period_layer_totals(
    conn: &Connection,
    start: DateTime<Local>,
    end: DateTime<Local>,
    scope: &KeyScope,
) -> Result<LayerTotals, AppError> {
    let sql = format!(
        "SELECT {} AS layer,
                SUM(cost),
                SUM(input_tokens),
                SUM(output_tokens),
                COUNT(*),
                SUM(cache_read_tokens),
                SUM({CACHE_DENOM_SQL})
         FROM request_logs
         LEFT JOIN virtual_keys k ON k.id = request_logs.virtual_key_id
         WHERE request_logs.status = 'success'
           AND request_logs.occurred_at >= ?1 AND request_logs.occurred_at < ?2{}
         GROUP BY 1",
        layer_expr(scope),
        scope.clause()
    );
    let values = bind_range(start, end, scope);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(values.iter().map(|value| value.as_ref())),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        },
    )?;

    let mut totals = Totals::default();
    let mut layer_totals: BTreeMap<String, f64> = BTreeMap::new();
    for row in rows {
        let (layer, cost, input, output, calls, cache_read, cache_denom) = row?;
        totals.calls += calls;
        totals.input += input;
        totals.output += output;
        totals.cost += cost;
        totals.cache_read += cache_read;
        totals.cache_denom += cache_denom;
        *layer_totals.entry(layer).or_insert(0.0) += cost;
    }

    let mut ranked: Vec<(String, f64)> = layer_totals.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(LayerTotals {
        totals,
        layer_totals: ranked,
    })
}

pub fn build_overview(
    conn: &Connection,
    period: Period,
    anchor: DateTime<Local>,
    scope: &KeyScope,
) -> Result<UsageOverview, AppError> {
    let start = period_start(period, anchor);
    let end = period_end(period, start);
    let prev_start = previous_start(period, start);

    let PeriodStats {
        totals: current,
        series: current_series,
        layers,
        layer_totals: current_layers,
        model_costs,
    } = period_stats(conn, period, start, scope)?;
    let LayerTotals {
        totals: previous,
        layer_totals: previous_layers,
    } = period_layer_totals(conn, prev_start, start, scope)?;

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
            value: format!("$ {:.2}", current.cost),
            comparison: comparison(unit, current.cost, previous.cost),
        },
    ];

    let max = current_series.iter().copied().fold(0.0f64, f64::max);

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
    let attribution = build_attribution(
        &previous_layers,
        &current_layers,
        (previous.cache_read, previous.cache_denom),
        (current.cache_read, current.cache_denom),
    );
    let quality = quality_totals(conn, start, end, scope)?;

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
        },
        layers,
        attribution,
        quality,
        model_costs,
    })
}

pub async fn query_overview(
    db: &Db,
    period: Period,
    anchor: DateTime<Local>,
    scope: KeyScope,
) -> Result<UsageOverview, AppError> {
    with_db(db, move |conn| build_overview(conn, period, anchor, &scope)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::keys::save_virtual_key;
    use crate::db::logs::insert_log;
    use crate::db::models::{RequestLog, VirtualKeyInput};
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
            cache_read_in_input: false,
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
    fn zero_tokens_render_without_a_spurious_unit() {
        assert_eq!(format_tokens_wan(0), "0");
        assert_eq!(format_tokens_wan(5_000), "0.5 万");
        assert_eq!(format_tokens_wan(482_000), "48.2 万");
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
        let overview = build_overview(&conn, Period::Day, now, &KeyScope::All).unwrap();

        assert_eq!(overview.total_cost, 2.25);
        assert_eq!(overview.metrics[0].value, "3 次");
        assert_eq!(overview.metrics[3].value, "$ 2.25");
        assert_eq!(overview.model_costs.len(), 2);
        assert_eq!(overview.model_costs[0].name, "beta");
        assert_eq!(overview.model_costs[1].name, "alpha");
        assert_eq!(overview.model_costs[0].cost, 1.5);
        // 输入 tokens: 5000 → 0.5 万
        assert_eq!(overview.metrics[1].value, "0.5 万");
    }

    #[test]
    fn overview_respects_key_scope() {
        let db = open_in_memory().unwrap();
        let now = Local::now();
        let today = period_start(Period::Day, now);
        {
            let conn = db.lock().unwrap();
            let mut first = sample_log(today + Duration::hours(1), 1000, 500, 0.5, "alpha");
            first.virtual_key_id = Some("k1".to_string());
            insert_log(&conn, &first).unwrap();
            let mut second = sample_log(today + Duration::hours(2), 2000, 1000, 1.5, "beta");
            second.virtual_key_id = Some("k2".to_string());
            insert_log(&conn, &second).unwrap();
            // 无归属：virtual_key_id 保持 NULL
            insert_log(
                &conn,
                &sample_log(today + Duration::hours(3), 3000, 1500, 2.0, "gamma"),
            )
            .unwrap();
        }
        let conn = db.lock().unwrap();

        let all = build_overview(&conn, Period::Day, now, &KeyScope::All).unwrap();
        assert_eq!(all.total_cost, 4.0);

        let unassigned = build_overview(&conn, Period::Day, now, &KeyScope::Unassigned).unwrap();
        assert_eq!(unassigned.total_cost, 2.0);
        assert_eq!(unassigned.metrics[0].value, "1 次");

        let keyed = build_overview(&conn, Period::Day, now, &KeyScope::Key("k1".to_string())).unwrap();
        assert_eq!(keyed.total_cost, 0.5);
        assert_eq!(keyed.metrics[0].value, "1 次");
        assert_eq!(keyed.model_costs.len(), 1);
        assert_eq!(keyed.model_costs[0].name, "alpha");
    }

    #[test]
    fn layers_group_by_key_for_all_and_by_model_for_single_key() {
        let db = open_in_memory().unwrap();
        let now = Local::now();
        let today = period_start(Period::Day, now);
        let first_id;
        {
            let conn = db.lock().unwrap();
            let first = save_virtual_key(
                &conn,
                &VirtualKeyInput {
                    id: None,
                    key: Some("sk-lumen-k1".to_string()),
                    name: "甲".to_string(),
                    enabled: true,
                    quota_limit: None,
                    quota_period: "monthly".to_string(),
                },
            )
            .unwrap();
            first_id = first.id.clone();
            let second = save_virtual_key(
                &conn,
                &VirtualKeyInput {
                    id: None,
                    key: Some("sk-lumen-k2".to_string()),
                    name: "乙".to_string(),
                    enabled: true,
                    quota_limit: None,
                    quota_period: "monthly".to_string(),
                },
            )
            .unwrap();

            let mut a = sample_log(today + Duration::hours(1), 100, 50, 1.0, "alpha");
            a.virtual_key_id = Some(first.id.clone());
            insert_log(&conn, &a).unwrap();
            let mut b = sample_log(today + Duration::hours(2), 100, 50, 2.0, "alpha");
            b.virtual_key_id = Some(first.id.clone());
            insert_log(&conn, &b).unwrap();
            let mut c = sample_log(today + Duration::hours(3), 100, 50, 5.0, "beta");
            c.virtual_key_id = Some(second.id.clone());
            insert_log(&conn, &c).unwrap();
            insert_log(
                &conn,
                &sample_log(today + Duration::hours(4), 100, 50, 4.0, "gamma"),
            )
            .unwrap();
        }
        let conn = db.lock().unwrap();

        let all = build_overview(&conn, Period::Day, now, &KeyScope::All).unwrap();
        let names: Vec<&str> = all.layers.iter().map(|layer| layer.name.as_str()).collect();
        assert_eq!(names, vec!["乙", "未归属", "甲"]);
        assert_eq!(all.layers[0].tone, "ochre");
        assert_eq!(all.layers[1].tone, "ink");
        assert_eq!(all.layers[2].tone, "indigo");
        assert!((all.layers[1].amount - 4.0).abs() < 1e-9);
        assert!((all.layers[1].values.last().copied().unwrap() - 4.0).abs() < 1e-9);

        let keyed = build_overview(&conn, Period::Day, now, &KeyScope::Key(first_id)).unwrap();
        let keyed_names: Vec<&str> = keyed.layers.iter().map(|layer| layer.name.as_str()).collect();
        assert_eq!(keyed_names, vec!["alpha"]);
        assert!((keyed.layers[0].amount - 3.0).abs() < 1e-9);
    }

    #[test]
    fn quality_reports_cache_error_and_reasoning_rates() {
        let db = open_in_memory().unwrap();
        let now = Local::now();
        let today = period_start(Period::Day, now);
        {
            let conn = db.lock().unwrap();
            let mut ok = sample_log(today + Duration::hours(1), 40, 100, 0.5, "alpha");
            ok.cache_read_tokens = 60;
            ok.reasoning_tokens = 20;
            insert_log(&conn, &ok).unwrap();

            let mut failed = sample_log(today + Duration::hours(2), 0, 0, 0.0, "alpha");
            failed.status = "error".to_string();
            insert_log(&conn, &failed).unwrap();
        }
        let conn = db.lock().unwrap();
        let overview = build_overview(&conn, Period::Day, now, &KeyScope::All).unwrap();

        assert!((overview.quality.cache_hit_rate - 0.6).abs() < 1e-9);
        assert!((overview.quality.error_rate - 0.5).abs() < 1e-9);
        assert!((overview.quality.reasoning_share - 0.2).abs() < 1e-9);
    }

    #[test]
    fn quality_cache_hit_rate_respects_boundary() {
        let db = open_in_memory().unwrap();
        let now = Local::now();
        let today = period_start(Period::Day, now);
        {
            let conn = db.lock().unwrap();
            // OpenAI 语义：input 已含命中。input=100、命中=60 → 分母 100，命中率 0.6
            // （旧的固定相加分母会得 60/160 = 0.375）。
            let mut contained = sample_log(today + Duration::hours(1), 100, 10, 0.5, "gpt");
            contained.cache_read_tokens = 60;
            contained.cache_read_in_input = true;
            insert_log(&conn, &contained).unwrap();

            // Anthropic 语义：input 不含命中。input=40、命中=60 → 分母 100，命中率 0.6。
            let mut separate = sample_log(today + Duration::hours(2), 40, 10, 0.5, "claude");
            separate.cache_read_tokens = 60;
            separate.cache_read_in_input = false;
            insert_log(&conn, &separate).unwrap();
        }
        let conn = db.lock().unwrap();
        let overview = build_overview(&conn, Period::Day, now, &KeyScope::All).unwrap();
        // 合并：命中 120，分母 (100) + (40 + 60) = 200 → 0.6。
        assert!(
            (overview.quality.cache_hit_rate - 0.6).abs() < 1e-9,
            "got {}",
            overview.quality.cache_hit_rate
        );
    }

    /// 手动跑的粗略基准：`cargo test bench_overview -- --nocapture --ignored`。
    #[test]
    #[ignore]
    fn bench_overview() {
        use crate::db::demo::{inject_demo, DemoScenario};
        let path = std::env::temp_dir().join("lumen-bench-overview.db");
        let _ = std::fs::remove_file(&path);
        let db = crate::db::open(&path).unwrap();
        {
            let conn = db.lock().unwrap();
            inject_demo(&conn, DemoScenario::Rich).unwrap();
        }
        let conn = db.lock().unwrap();
        let now = Local::now();
        let plan_sql = "SELECT COUNT(*) FROM request_logs
             WHERE status = 'success' AND occurred_at >= ?1 AND occurred_at < ?2";
        {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {plan_sql}")).unwrap();
            let rows = stmt
                .query_map([now.to_rfc3339(), (now + Duration::days(1)).to_rfc3339()], |row| {
                    row.get::<_, String>(3)
                })
                .unwrap();
            for row in rows {
                println!("PLAN: {}", row.unwrap());
            }
            let cnt: i64 = conn
                .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
                .unwrap();
            println!("TOTAL ROWS: {cnt}");
        }
        for _ in 0..2 {
            for period in [Period::Day, Period::Week, Period::Month] {
                let t = std::time::Instant::now();
                let overview = build_overview(&conn, period, now, &KeyScope::All).unwrap();
                println!("{:?}: {:?} (layers={})", period, t.elapsed(), overview.layers.len());
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn week_buckets_follow_local_weekdays() {
        let db = open_in_memory().unwrap();
        let now = Local::now();
        let week_start = period_start(Period::Week, now);
        {
            let conn = db.lock().unwrap();
            // 周一 10:00 → 桶 0，周三 10:00 → 桶 2
            insert_log(
                &conn,
                &sample_log(week_start + Duration::hours(10), 100, 50, 1.0, "alpha"),
            )
            .unwrap();
            insert_log(
                &conn,
                &sample_log(week_start + Duration::days(2) + Duration::hours(10), 100, 50, 3.0, "alpha"),
            )
            .unwrap();
        }
        let conn = db.lock().unwrap();
        let overview = build_overview(&conn, Period::Week, now, &KeyScope::All).unwrap();
        let values = &overview.layers[0].values;
        assert!((values[0] - 1.0).abs() < 1e-9, "周一应落在桶 0，实际 {:?}", values);
        assert!((values[1] - 1.0).abs() < 1e-9);
        assert!((values[2] - 4.0).abs() < 1e-9, "周三应落在桶 2，实际 {:?}", values);
        assert!((values[6] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn month_buckets_follow_local_days() {
        let db = open_in_memory().unwrap();
        let now = Local::now();
        let month_start = period_start(Period::Month, now);
        {
            let conn = db.lock().unwrap();
            // 1 日 → 桶 0，3 日 → 桶 2
            insert_log(
                &conn,
                &sample_log(month_start + Duration::hours(10), 100, 50, 1.0, "alpha"),
            )
            .unwrap();
            insert_log(
                &conn,
                &sample_log(month_start + Duration::days(2) + Duration::hours(10), 100, 50, 3.0, "alpha"),
            )
            .unwrap();
        }
        let conn = db.lock().unwrap();
        let overview = build_overview(&conn, Period::Month, now, &KeyScope::All).unwrap();
        let values = &overview.layers[0].values;
        assert!((values[0] - 1.0).abs() < 1e-9, "1 日应落在桶 0，实际 {:?}", values);
        assert!((values[1] - 1.0).abs() < 1e-9);
        assert!((values[2] - 4.0).abs() < 1e-9, "3 日应落在桶 2，实际 {:?}", values);
    }
}
