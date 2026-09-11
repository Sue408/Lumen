pub mod attribution;
#[cfg(debug_assertions)]
pub mod demo;
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
    icon         TEXT,
    icon_tint    TEXT NOT NULL DEFAULT 'ink',
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
    cache_read_price     REAL NOT NULL DEFAULT 0,
    cache_creation_price REAL NOT NULL DEFAULT 0,
    context_window INTEGER NOT NULL DEFAULT 0,
    capabilities   TEXT NOT NULL DEFAULT '[]',
    icon          TEXT,
    icon_tint     TEXT NOT NULL DEFAULT 'ink',
    enabled       INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS routes (
    id            TEXT PRIMARY KEY,
    alias         TEXT NOT NULL UNIQUE,
    display_name  TEXT NOT NULL,
    protocol      TEXT NOT NULL DEFAULT 'openai',
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
    id           TEXT PRIMARY KEY,
    key          TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    quota_limit  REAL,
    quota_period TEXT NOT NULL DEFAULT 'monthly',
    created_at   TEXT NOT NULL
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
    model_real          TEXT,
    provider_id         TEXT,
    virtual_key_id      TEXT,
    kind                TEXT NOT NULL DEFAULT 'chat',
    input_tokens        INTEGER NOT NULL DEFAULT 0,
    output_tokens       INTEGER NOT NULL DEFAULT 0,
    total_tokens        INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens   INTEGER NOT NULL DEFAULT 0,
    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens    INTEGER NOT NULL DEFAULT 0,
    cost                REAL NOT NULL DEFAULT 0,
    usage_source        TEXT NOT NULL DEFAULT 'missing',
    status              TEXT NOT NULL,
    http_status         INTEGER,
    latency_ms          INTEGER,
    error_message       TEXT,
    request_id          TEXT,
    is_stream           INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_request_logs_occurred ON request_logs(occurred_at);
CREATE INDEX IF NOT EXISTS idx_request_logs_alias ON request_logs(route_alias);
CREATE INDEX IF NOT EXISTS idx_request_logs_status ON request_logs(status);
-- 统计查询几乎都是「status = 'success' + occurred_at 区间」：复合索引让规划器
-- 在 status 上定位后直接走时间范围，而不是先扫完所有成功行再逐行比时间。
CREATE INDEX IF NOT EXISTS idx_request_logs_status_occurred ON request_logs(status, occurred_at);
-- 密钥额度查询走「virtual_key_id + status + occurred_at 区间」，还要 SUM(cost)：
-- 带上 cost 变成覆盖索引，既不用扫全表也不用回表；网关每请求查额度同走此路。
CREATE INDEX IF NOT EXISTS idx_request_logs_key_status_occurred
    ON request_logs(virtual_key_id, status, occurred_at, cost);
-- 用量口径的筛选与「待处理」的 OR 都按 usage_source 取行：有它，存疑计数走覆盖
-- 索引、attention 的 OR 走 MULTI-INDEX OR，否则两者都是全表扫描。
CREATE INDEX IF NOT EXISTS idx_request_logs_usage_source ON request_logs(usage_source);
"#;

/// 每次修改 `SCHEMA` 就 +1；启动时版本不符即重建空库（pre-launch 阶段不做逐列迁移）。
const SCHEMA_VERSION: i64 = 6;

/// 依赖外键的表按子表在前顺序清空，避免重建时的外键约束。
const DROP_ALL: &str = "
DROP TABLE IF EXISTS route_targets;
DROP TABLE IF EXISTS routes;
DROP TABLE IF EXISTS upstream_models;
DROP TABLE IF EXISTS providers;
DROP TABLE IF EXISTS virtual_keys;
DROP TABLE IF EXISTS request_logs;
DROP TABLE IF EXISTS settings;
";

fn reset(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    conn.execute_batch(DROP_ALL)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// 清空全部业务数据（提供商 / 模型 / 路由 / 密钥 / 日志），保留 `settings`。
pub fn clear_business_data(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let result = conn.execute_batch(
        "DELETE FROM route_targets;
         DELETE FROM routes;
         DELETE FROM upstream_models;
         DELETE FROM providers;
         DELETE FROM virtual_keys;
         DELETE FROM request_logs;",
    );
    conn.pragma_update(None, "foreign_keys", "ON")?;
    result?;
    Ok(())
}

fn configure(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        if version != 0 {
            tracing::warn!("数据库 schema 版本 {version} 与当前 {SCHEMA_VERSION} 不符，已重建空库");
        }
        reset(conn)?;
    }
    conn.execute_batch(SCHEMA)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
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
