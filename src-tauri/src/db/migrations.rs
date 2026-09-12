use rusqlite::Connection;

use crate::error::AppError;

/// 新库的一次性建表语句。所有 DDL 均为 `IF NOT EXISTS`，因此旧库再次执行只用于
/// 幂等补建新增的索引 / 表（结构加列走 `MIGRATIONS`）。
pub const SCHEMA: &str = r#"
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
    cache_read_in_input INTEGER NOT NULL DEFAULT 0,
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

-- 路由目标的自然键：同一路由内同一上游模型只能出现一次。save_route 据此 upsert，
-- 使 target id 稳定（未来日志可引用实际履约 target）。
CREATE UNIQUE INDEX IF NOT EXISTS idx_route_targets_route_model
    ON route_targets(route_id, upstream_model_id);
"#;

/// 最新 schema 版本。每次修改 `SCHEMA` 的**结构**（新建表 / 加列 / 改约束）就 +1，
/// 并在 `MIGRATIONS` 补一条对应目标的增量语句；纯加索引不算（`SCHEMA` 幂等补建即可）。
pub const SCHEMA_VERSION: i64 = 8;

/// 把 `user_version` 从「目标版本 - 1」提升到「目标版本」的增量语句，按目标版本升序。
/// 只允许增量（`ALTER TABLE ADD COLUMN` / `CREATE TABLE` / `CREATE [UNIQUE] INDEX`），
/// 严禁 `DROP`，以保证流水与设置不随升级丢失。
const MIGRATIONS: &[(i64, &str)] = &[(
    8,
    "DELETE FROM route_targets
       WHERE rowid NOT IN (
         SELECT MIN(rowid) FROM route_targets GROUP BY route_id, upstream_model_id
       );
     CREATE UNIQUE INDEX IF NOT EXISTS idx_route_targets_route_model
       ON route_targets(route_id, upstream_model_id);",
)];

/// 校准数据库版本：
/// - `user_version == 0`：新库，执行 `SCHEMA` 建表；
/// - `0 < v < SCHEMA_VERSION`：逐级执行 `MIGRATIONS`；
/// - `v > SCHEMA_VERSION`：程序过旧，拒绝启动（不清库）；
/// - 最后幂等执行 `SCHEMA` 补建新增索引 / 表，并把版本推到最新。
///
/// 外键约束是**每连接**的、默认关闭，必须在建库 / 迁移前显式打开。若将来某条迁移
/// 需要重建带外键的表，需在 `apply` 的事务外先关外键、迁移后再打开并
/// `PRAGMA foreign_key_check`（见 SQLite 官方重建表步骤）。
pub fn configure(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(AppError::message(format!(
            "数据库版本 {version} 高于本程序支持的 {SCHEMA_VERSION}，请升级 Lumen"
        )));
    }
    if version > 0 {
        apply(conn, version, MIGRATIONS)?;
    }
    conn.execute_batch(SCHEMA)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

/// 依次执行 `target > from` 的迁移。每条独立事务：失败即中止，且不会提升版本。
fn apply(conn: &Connection, from: i64, migrations: &[(i64, &str)]) -> Result<(), AppError> {
    for (target, sql) in migrations.iter().filter(|(target, _)| *target > from) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", target)?;
        tx.commit()?;
        tracing::info!("数据库 schema 已迁移至版本 {target}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_version(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap()
    }

    fn insert_provider(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO providers
                (id, name, base_url, api_key, auth_scheme, protocol,
                 extra_headers, icon, icon_tint, enabled, created_at)
             VALUES (?1, 'P', 'https://example.com/v1', '', 'bearer', 'openai',
                 '{}', NULL, 'ink', 1, '2026-01-01T00:00:00Z')",
            [id],
        )
        .unwrap();
    }

    fn open_fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        conn
    }

    #[test]
    fn fresh_db_lands_on_latest_version() {
        let conn = open_fresh();
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
    }

    #[test]
    fn migrates_without_losing_rows() {
        let conn = open_fresh();
        insert_provider(&conn, "p1");
        // 模拟一个旧版本库：数据在，版本落后。
        conn.pragma_update(None, "user_version", SCHEMA_VERSION - 1)
            .unwrap();

        configure(&conn).unwrap();

        assert_eq!(user_version(&conn), SCHEMA_VERSION);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn rejects_future_version() {
        let conn = open_fresh();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        let error = configure(&conn).unwrap_err();
        assert!(matches!(error, AppError::Message(_)), "got {error:?}");
        // 拒绝启动不修改版本，更不清库。
        assert_eq!(user_version(&conn), SCHEMA_VERSION + 1);
    }

    #[test]
    fn apply_runs_incremental_step() {
        let conn = open_fresh();
        insert_provider(&conn, "p1");

        apply(
            &conn,
            SCHEMA_VERSION,
            &[(
                SCHEMA_VERSION + 1,
                "ALTER TABLE providers ADD COLUMN scratch TEXT NOT NULL DEFAULT ''",
            )],
        )
        .unwrap();

        assert_eq!(user_version(&conn), SCHEMA_VERSION + 1);
        conn.execute("UPDATE providers SET scratch = 'ok'", [])
            .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn configure_enables_foreign_keys() {
        let conn = open_fresh();
        let on: i64 = conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(on, 1, "外键约束必须在建库 / 迁移前打开");
    }

    #[test]
    fn foreign_keys_cascade_and_reject_dangling_refs() {
        let conn = open_fresh();
        insert_provider(&conn, "p1");
        conn.execute(
            "INSERT INTO upstream_models (id, provider_id, model_id, display_name)
             VALUES ('m1', 'p1', 'gpt', 'GPT')",
            [],
        )
        .unwrap();

        // 删除 provider 应级联删除其 model。
        conn.execute("DELETE FROM providers WHERE id = 'p1'", [])
            .unwrap();
        let models: i64 = conn
            .query_row("SELECT COUNT(*) FROM upstream_models", [], |row| row.get(0))
            .unwrap();
        assert_eq!(models, 0, "ON DELETE CASCADE 应生效");

        // 引用不存在的 provider 应被外键拒绝。
        let dangling = conn.execute(
            "INSERT INTO upstream_models (id, provider_id, model_id, display_name)
             VALUES ('m2', 'missing', 'x', 'X')",
            [],
        );
        assert!(dangling.is_err(), "悬空外键应被拒绝");
    }
}
