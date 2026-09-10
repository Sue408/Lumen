pub mod keys;
pub mod logs;
pub mod models;
pub mod providers;
pub mod routes;
pub mod seed;
pub mod settings;
pub mod stats;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::error::AppError;

pub type Db = Arc<Mutex<Connection>>;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS providers (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    base_url     TEXT NOT NULL,
    api_key      TEXT NOT NULL DEFAULT '',
    auth_scheme  TEXT NOT NULL DEFAULT 'bearer',
    protocol     TEXT NOT NULL DEFAULT 'openai',
    extra_headers TEXT NOT NULL DEFAULT '{}',
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS upstream_models (
    id            TEXT PRIMARY KEY,
    provider_id   TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    model_id      TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    input_price   REAL NOT NULL DEFAULT 0,
    output_price  REAL NOT NULL DEFAULT 0,
    enabled       INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS routes (
    id            TEXT PRIMARY KEY,
    alias         TEXT NOT NULL UNIQUE,
    display_name  TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS route_targets (
    id                TEXT PRIMARY KEY,
    route_id          TEXT NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    upstream_model_id TEXT NOT NULL REFERENCES upstream_models(id) ON DELETE CASCADE,
    priority          INTEGER NOT NULL DEFAULT 0,
    enabled           INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS virtual_keys (
    id          TEXT PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS request_logs (
    id                  TEXT PRIMARY KEY,
    occurred_at         TEXT NOT NULL,
    endpoint            TEXT NOT NULL,
    method              TEXT NOT NULL,
    route_alias         TEXT,
    route_id            TEXT,
    upstream_model_id   TEXT,
    upstream_model_name TEXT,
    provider_id         TEXT,
    virtual_key_id      TEXT,
    kind                TEXT NOT NULL DEFAULT 'chat',
    input_tokens        INTEGER NOT NULL DEFAULT 0,
    output_tokens       INTEGER NOT NULL DEFAULT 0,
    total_tokens        INTEGER NOT NULL DEFAULT 0,
    cost                REAL NOT NULL DEFAULT 0,
    status              TEXT NOT NULL,
    http_status         INTEGER,
    latency_ms          INTEGER,
    error_message       TEXT,
    is_stream           INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_request_logs_occurred ON request_logs(occurred_at);
CREATE INDEX IF NOT EXISTS idx_request_logs_alias ON request_logs(route_alias);
CREATE INDEX IF NOT EXISTS idx_request_logs_status ON request_logs(status);
"#;

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, AppError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), AppError> {
    if !column_exists(conn, table, column)? {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"))?;
    }
    Ok(())
}

fn configure(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA)?;
    // 兼容旧库：为已存在的 providers 表补列。
    add_column_if_missing(conn, "providers", "protocol", "TEXT NOT NULL DEFAULT 'openai'")?;
    add_column_if_missing(
        conn,
        "providers",
        "extra_headers",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    Ok(())
}

pub fn open(path: &Path) -> Result<Db, AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

#[cfg(test)]
pub fn open_in_memory() -> Result<Db, AppError> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

/// 在阻塞线程上访问数据库，避免在 handler 中持锁跨 `await`。
pub async fn with_db<T, F>(db: &Db, f: F) -> Result<T, AppError>
where
    F: FnOnce(&Connection) -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    let db = db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db
            .lock()
            .map_err(|_| AppError::Message("数据库锁已中毒".into()))?;
        f(&conn)
    })
    .await?
}
