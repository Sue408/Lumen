use rusqlite::{params, Connection};

use super::models::{VirtualKey, VirtualKeyInput};
use crate::error::AppError;

pub fn list_virtual_keys(conn: &Connection) -> Result<Vec<VirtualKey>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM virtual_keys ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], VirtualKey::from_row)?;
    let mut keys = Vec::new();
    for row in rows {
        keys.push(row?);
    }
    Ok(keys)
}

pub fn save_virtual_key(
    conn: &Connection,
    input: &VirtualKeyInput,
) -> Result<VirtualKey, AppError> {
    let id = input
        .id
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let key = input
        .key
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(generate_key);
    let created_at = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO virtual_keys (id, key, name, enabled, quota_limit, quota_period, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
            key = excluded.key,
            name = excluded.name,
            enabled = excluded.enabled,
            quota_limit = excluded.quota_limit,
            quota_period = excluded.quota_period",
        params![
            id,
            key,
            input.name,
            input.enabled as i64,
            input.quota_limit,
            input.quota_period,
            created_at
        ],
    )
    .map_err(|error| AppError::from_constraint(error, "该密钥已存在，请换一个"))?;
    let mut stmt = conn.prepare("SELECT * FROM virtual_keys WHERE id = ?1")?;
    let mut rows = stmt.query_map([&id], VirtualKey::from_row)?;
    rows.next()
        .transpose()?
        .ok_or_else(|| AppError::message("保存虚拟密钥失败"))
}

pub fn delete_virtual_key(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM virtual_keys WHERE id = ?1", [id])?;
    Ok(())
}

/// 按密钥原文查找启用中的虚拟密钥，供网关鉴权使用。
pub fn find_enabled_virtual_key(
    conn: &Connection,
    key: &str,
) -> Result<Option<VirtualKey>, AppError> {
    let mut stmt =
        conn.prepare("SELECT * FROM virtual_keys WHERE key = ?1 AND enabled = 1")?;
    let mut rows = stmt.query_map([key], VirtualKey::from_row)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// 某密钥自周期起点以来的成功花费与调用次数。失败的调用不计入。
pub fn virtual_key_usage(
    conn: &Connection,
    key_id: &str,
    start: chrono::DateTime<chrono::Local>,
) -> Result<(f64, i64), AppError> {
    let start = start.with_timezone(&chrono::Utc).to_rfc3339();
    let usage = conn.query_row(
        "SELECT COALESCE(SUM(cost), 0), COUNT(*) FROM request_logs
         WHERE virtual_key_id = ?1 AND status = 'success' AND occurred_at >= ?2",
        params![key_id, start],
        |row| Ok((row.get::<_, f64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    Ok(usage)
}

/// 某密钥自周期起点以来的成功花费合计（美元）。失败的调用不计入。
pub fn virtual_key_spend(
    conn: &Connection,
    key_id: &str,
    start: chrono::DateTime<chrono::Local>,
) -> Result<f64, AppError> {
    let start = start.with_timezone(&chrono::Utc).to_rfc3339();
    let spent: f64 = conn.query_row(
        "SELECT COALESCE(SUM(cost), 0) FROM request_logs
         WHERE virtual_key_id = ?1 AND status = 'success' AND occurred_at >= ?2",
        params![key_id, start],
        |row| row.get(0),
    )?;
    Ok(spent)
}

fn generate_key() -> String {
    format!("sk-lumen-{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::logs::insert_log;
    use crate::db::models::RequestLog;
    use crate::db::open_in_memory;
    use chrono::{Duration, Local, Utc};

    fn sample_log(
        occurred: chrono::DateTime<Local>,
        cost: f64,
        status: &str,
        key_id: Option<&str>,
    ) -> RequestLog {
        RequestLog {
            id: uuid::Uuid::new_v4().to_string(),
            occurred_at: occurred.with_timezone(&Utc).to_rfc3339(),
            endpoint: "/v1/chat/completions".to_string(),
            method: "POST".to_string(),
            route_alias: Some("lumen/x".to_string()),
            route_id: None,
            upstream_model_id: None,
            upstream_model_name: None,
            model_real: None,
            provider_id: None,
            virtual_key_id: key_id.map(str::to_string),
            kind: "chat".to_string(),
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_in_input: false,
            reasoning_tokens: 0,
            cost,
            usage_source: "provider".to_string(),
            status: status.to_string(),
            http_status: Some(200),
            latency_ms: Some(10),
            error_message: None,
            request_id: None,
            is_stream: false,
        }
    }

    fn input(id: Option<String>, enabled: bool) -> VirtualKeyInput {
        VirtualKeyInput {
            id,
            key: Some("sk-lumen-test".to_string()),
            name: "测试密钥".to_string(),
            enabled,
            quota_limit: Some(5.0),
            quota_period: "monthly".to_string(),
        }
    }

    #[test]
    fn saves_and_finds_quota_fields() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let saved = save_virtual_key(&conn, &input(None, true)).unwrap();
        assert_eq!(saved.quota_limit, Some(5.0));
        assert_eq!(saved.quota_period, "monthly");

        let found = find_enabled_virtual_key(&conn, "sk-lumen-test")
            .unwrap()
            .expect("应能查到启用密钥");
        assert_eq!(found.id, saved.id);
    }

    #[test]
    fn duplicate_key_reports_a_friendly_error() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        save_virtual_key(&conn, &input(None, true)).unwrap();
        // 同一个 key、不同 id：命中 UNIQUE(key)，应翻译成面向用户的提示而非裸 SQL 错误。
        let error = save_virtual_key(&conn, &input(None, true)).unwrap_err();
        assert!(matches!(error, AppError::Message(_)), "got {error:?}");
    }

    #[test]
    fn disabled_key_is_not_found() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let saved = save_virtual_key(&conn, &input(None, true)).unwrap();
        save_virtual_key(&conn, &input(Some(saved.id.clone()), false)).unwrap();
        assert!(find_enabled_virtual_key(&conn, "sk-lumen-test")
            .unwrap()
            .is_none());
    }

    #[test]
    fn spend_sums_only_successful_logs_in_period() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let now = Local::now();
        let start = now - Duration::hours(2);

        insert_log(
            &conn,
            &sample_log(now - Duration::minutes(30), 1.5, "success", Some("k1")),
        )
        .unwrap();
        // 失败不计
        insert_log(
            &conn,
            &sample_log(now - Duration::minutes(20), 9.9, "error", Some("k1")),
        )
        .unwrap();
        // 周期外不计
        insert_log(
            &conn,
            &sample_log(now - Duration::hours(5), 7.0, "success", Some("k1")),
        )
        .unwrap();
        // 其他密钥不计
        insert_log(
            &conn,
            &sample_log(now - Duration::minutes(10), 3.0, "success", Some("k2")),
        )
        .unwrap();

        let spent = virtual_key_spend(&conn, "k1", start).unwrap();
        assert!((spent - 1.5).abs() < 1e-9, "spent was {spent}");
    }

    /// 手动跑的粗略基准：`cargo test bench_key_usage -- --nocapture --ignored`。
    #[test]
    #[ignore]
    fn bench_key_usage() {
        use crate::db::demo::{inject_demo, DemoScenario};
        use crate::gateway::quota::{period_start, QuotaPeriod};
        use chrono::Local;
        use rusqlite::params;

        let path = std::env::temp_dir().join("lumen-bench-key-usage.db");
        let _ = std::fs::remove_file(&path);
        let db = crate::db::open(&path).unwrap();
        {
            let conn = db.lock().unwrap();
            inject_demo(&conn, DemoScenario::Rich).unwrap();
        }
        let conn = db.lock().unwrap();
        let now = Local::now();
        let start = period_start(QuotaPeriod::Monthly, now);
        let start_iso = start.with_timezone(&Utc).to_rfc3339();

        let sql = "SELECT COALESCE(SUM(cost), 0), COUNT(*) FROM request_logs
             WHERE virtual_key_id = ?1 AND status = 'success' AND occurred_at >= ?2";
        {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows = stmt
                .query_map(params!["x", &start_iso], |row| row.get::<_, String>(3))
                .unwrap();
            for row in rows {
                println!("PLAN: {}", row.unwrap());
            }
        }
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        let key_ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM virtual_keys").unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        println!("TOTAL ROWS: {total} / KEYS: {}", key_ids.len());

        for round in 0..2 {
            let t = std::time::Instant::now();
            for id in &key_ids {
                let _ = virtual_key_usage(&conn, id, start).unwrap();
            }
            println!("round {round} ({} keys): {:?}", key_ids.len(), t.elapsed());
        }
        drop(conn);
        let _ = std::fs::remove_file(&path);
    }
}
