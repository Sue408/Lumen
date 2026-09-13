use rusqlite::Connection;

use crate::error::AppError;

/// 新库的一次性建表语句。所有 DDL 均为 `IF NOT EXISTS`，因此旧库再次执行只用于
/// 幂等补建新增的索引 / 表（结构加列走 `MIGRATIONS`）。
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS providers (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    api_key      TEXT NOT NULL DEFAULT '',
    extra_headers TEXT NOT NULL DEFAULT '{}',
    header_rules TEXT NOT NULL DEFAULT '{}',
    icon         TEXT,
    icon_tint    TEXT NOT NULL DEFAULT 'ink',
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL
);

-- 提供商的协议端点：协议 + 该协议的上游地址与鉴权方式。一个提供商可挂多种协议，
-- 模型仍挂提供商、多协议共享；自然键 `(provider_id, protocol)` 保证每协议至多一条。
CREATE TABLE IF NOT EXISTS provider_endpoints (
    id          TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    protocol    TEXT NOT NULL,
    base_url    TEXT NOT NULL,
    auth_scheme TEXT NOT NULL DEFAULT 'bearer',
    UNIQUE(provider_id, protocol)
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
    icon          TEXT,
    icon_tint     TEXT,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS route_targets (
    id                TEXT PRIMARY KEY,
    route_id          TEXT NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    upstream_model_id TEXT NOT NULL REFERENCES upstream_models(id) ON DELETE CASCADE,
    priority          INTEGER NOT NULL DEFAULT 0,
    enabled           INTEGER NOT NULL DEFAULT 1,
    header_rules      TEXT NOT NULL DEFAULT '{}'
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
    -- 首字节耗时：流式场景下从上游响应头到达至首个数据块。生成速度据此从
    -- 整段 latency 中扣除首字等待，非流式为 NULL。
    ttfb_ms             INTEGER,
    error_message       TEXT,
    request_id          TEXT,
    is_stream           INTEGER NOT NULL DEFAULT 0,
    attempt_index       INTEGER NOT NULL DEFAULT 0,
    session_id          TEXT
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
-- 会话筛选与 `list_sessions` 的 `(virtual_key_id, session_id)` 分组都按 session_id
-- 取行。会话值由候选头名解析而来，客户端未带时为 NULL。
CREATE INDEX IF NOT EXISTS idx_request_logs_session ON request_logs(session_id);

-- 路由目标的自然键：同一路由内同一上游模型只能出现一次。save_route 据此 upsert，
-- 使 target id 稳定（未来日志可引用实际履约 target）。
CREATE UNIQUE INDEX IF NOT EXISTS idx_route_targets_route_model
    ON route_targets(route_id, upstream_model_id);
"#;

/// 最新 schema 版本。每次修改 `SCHEMA` 的**结构**（新建表 / 加列 / 改约束）就 +1，
/// 并在 `MIGRATIONS` 补一条对应目标的增量语句；纯加索引不算（`SCHEMA` 幂等补建即可）。
pub const SCHEMA_VERSION: i64 = 16;

/// 把 `user_version` 从「目标版本 - 1」提升到「目标版本」的增量语句，按目标版本升序。
/// 只允许增量（`ALTER TABLE ADD COLUMN` / `CREATE TABLE` / `CREATE [UNIQUE] INDEX`），
/// 严禁 `DROP`，以保证流水与设置不随升级丢失。
const MIGRATIONS: &[(i64, &str)] = &[
    (
        8,
        "DELETE FROM route_targets
           WHERE rowid NOT IN (
             SELECT MIN(rowid) FROM route_targets GROUP BY route_id, upstream_model_id
           );
         CREATE UNIQUE INDEX IF NOT EXISTS idx_route_targets_route_model
           ON route_targets(route_id, upstream_model_id);",
    ),
    (
        9,
        "ALTER TABLE request_logs ADD COLUMN attempt_index INTEGER NOT NULL DEFAULT 0;",
    ),
    (
        10,
        "ALTER TABLE routes ADD COLUMN icon TEXT;
         ALTER TABLE routes ADD COLUMN icon_tint TEXT;",
    ),
    (
        11,
        "ALTER TABLE request_logs ADD COLUMN session_id TEXT;",
    ),
    (
        12,
        "ALTER TABLE route_targets ADD COLUMN header_rules TEXT NOT NULL DEFAULT '{}';",
    ),
    (
        13,
        "ALTER TABLE providers ADD COLUMN header_rules TEXT NOT NULL DEFAULT '{}';",
    ),
    (
        14,
        "ALTER TABLE request_logs ADD COLUMN ttfb_ms INTEGER;",
    ),
    // v15：协议从提供商单值下沉为「协议端点」。建端点表 → 用旧 provider 的
    // `protocol / base_url / auth_scheme` 回填一条端点 → 重建 providers 去掉这三列。
    // 重建被 upstream_models / provider_endpoints 外键引用，故 configure 在迁移期间关闭外键。
    (
        15,
        "CREATE TABLE IF NOT EXISTS provider_endpoints (
             id          TEXT PRIMARY KEY,
             provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
             protocol    TEXT NOT NULL,
             base_url    TEXT NOT NULL,
             auth_scheme TEXT NOT NULL DEFAULT 'bearer',
             enabled     INTEGER NOT NULL DEFAULT 1,
             UNIQUE(provider_id, protocol)
         );
         INSERT INTO provider_endpoints (id, provider_id, protocol, base_url, auth_scheme, enabled)
         SELECT lower(hex(randomblob(16))), id, protocol, base_url, auth_scheme, 1 FROM providers;
         CREATE TABLE providers_new (
             id            TEXT PRIMARY KEY,
             name          TEXT NOT NULL,
             api_key       TEXT NOT NULL DEFAULT '',
             extra_headers TEXT NOT NULL DEFAULT '{}',
             header_rules  TEXT NOT NULL DEFAULT '{}',
             icon          TEXT,
             icon_tint     TEXT NOT NULL DEFAULT 'ink',
             enabled       INTEGER NOT NULL DEFAULT 1,
             created_at    TEXT NOT NULL
         );
         INSERT INTO providers_new
             (id, name, api_key, extra_headers, header_rules, icon, icon_tint, enabled, created_at)
         SELECT id, name, api_key, extra_headers, header_rules, icon, icon_tint, enabled, created_at
           FROM providers;
         DROP TABLE providers;
         ALTER TABLE providers_new RENAME TO providers;",
    ),
    // v16：端点的启停是伪需求（与 provider/model/target 的 enabled 语义重叠，且是
    // 「路由静默失效」的唯一来源）。重建 provider_endpoints 去掉 `enabled` 列。
    (
        16,
        "CREATE TABLE provider_endpoints_new (
             id          TEXT PRIMARY KEY,
             provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
             protocol    TEXT NOT NULL,
             base_url    TEXT NOT NULL,
             auth_scheme TEXT NOT NULL DEFAULT 'bearer',
             UNIQUE(provider_id, protocol)
         );
         INSERT INTO provider_endpoints_new (id, provider_id, protocol, base_url, auth_scheme)
         SELECT id, provider_id, protocol, base_url, auth_scheme FROM provider_endpoints;
         DROP TABLE provider_endpoints;
         ALTER TABLE provider_endpoints_new RENAME TO provider_endpoints;",
    ),
];

/// 校准数据库版本：
/// - `user_version == 0`：新库，执行 `SCHEMA` 建表；
/// - `0 < v < SCHEMA_VERSION`：逐级执行 `MIGRATIONS`；
/// - `v > SCHEMA_VERSION`：程序过旧，拒绝启动（不清库）；
/// - 最后幂等执行 `SCHEMA` 补建新增索引 / 表，并把版本推到最新。
///
/// 外键约束是**每连接**的、默认关闭。迁移可能重建被外键引用的表（如 v15 重建
/// `providers`），而 SQLite 要求重建期间关闭外键、且该 pragma 在事务内设置无效，
/// 因此必须在 `apply` 的事务之外先关、迁移后开，再 `PRAGMA foreign_key_check`
/// 校验（见 SQLite 官方重建表步骤）。
pub fn configure(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let result = configure_inner(conn);
    // 无论迁移成败都恢复外键，避免连接停留在关闭状态。
    conn.pragma_update(None, "foreign_keys", "ON")?;
    result?;
    ensure_foreign_keys_valid(conn)?;
    Ok(())
}

fn configure_inner(conn: &Connection) -> Result<(), AppError> {
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

/// 关闭外键期间的迁移不产生悬空引用；此处用 `foreign_key_check` 兜底校验。
fn ensure_foreign_keys_valid(conn: &Connection) -> Result<(), AppError> {
    let violations: i64 =
        conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations > 0 {
        return Err(AppError::message(format!(
            "迁移后外键完整性校验失败：{violations} 处悬空引用"
        )));
    }
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

    /// 按当前（最新）结构插入一个 provider；协议已下沉到端点表，故不含协议列。
    fn insert_provider(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO providers
                (id, name, api_key, extra_headers, header_rules, icon, icon_tint, enabled, created_at)
             VALUES (?1, 'P', '', '{}', '{}', NULL, 'ink', 1, '2026-01-01T00:00:00Z')",
            [id],
        )
        .unwrap();
    }

    /// 把最新库降级成 v15 之前的结构：删掉 `provider_endpoints`，回补
    /// `base_url / auth_scheme / protocol` 三列。数据由调用方随后填充。
    fn downgrade_providers_to_pre_v15(conn: &Connection) {
        conn.execute("DROP TABLE provider_endpoints", []).unwrap();
        conn.execute(
            "ALTER TABLE providers ADD COLUMN base_url TEXT NOT NULL DEFAULT ''",
            [],
        )
        .unwrap();
        conn.execute(
            "ALTER TABLE providers ADD COLUMN auth_scheme TEXT NOT NULL DEFAULT 'bearer'",
            [],
        )
        .unwrap();
        conn.execute(
            "ALTER TABLE providers ADD COLUMN protocol TEXT NOT NULL DEFAULT 'openai'",
            [],
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
        conn.execute(
            "INSERT INTO routes (id, alias, display_name, protocol, enabled, created_at)
             VALUES ('r1', 'r', 'R', 'openai', 1, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO request_logs (id, occurred_at, endpoint, method, status)
             VALUES ('l1', '2026-01-01T00:00:00Z', '/v1/chat/completions', 'POST', 'success')",
            [],
        )
        .unwrap();
        // 模拟 v9 旧库：去掉 v10 / v11 才新增的列，数据与更低的版本号都保留。
        conn.execute("ALTER TABLE routes DROP COLUMN icon", []).unwrap();
        conn.execute("ALTER TABLE routes DROP COLUMN icon_tint", []).unwrap();
        // v11 的 session_id 带索引，先撤索引再删列。
        conn.execute("DROP INDEX idx_request_logs_session", []).unwrap();
        conn.execute("ALTER TABLE request_logs DROP COLUMN session_id", [])
            .unwrap();
        conn.execute("ALTER TABLE request_logs DROP COLUMN ttfb_ms", [])
            .unwrap();
        conn.execute("ALTER TABLE route_targets DROP COLUMN header_rules", [])
            .unwrap();
        // v13 的 provider header_rules 同样先撤，模拟 v9 库。
        conn.execute("ALTER TABLE providers DROP COLUMN header_rules", [])
            .unwrap();
        // v15 之前 provider 自带协议列；回补成旧结构并填入待迁移的端点信息。
        downgrade_providers_to_pre_v15(&conn);
        conn.execute(
            "UPDATE providers
                SET base_url = 'https://example.com/v1', protocol = 'openai', auth_scheme = 'bearer'",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 9).unwrap();

        configure(&conn).unwrap();

        assert_eq!(user_version(&conn), SCHEMA_VERSION);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        // 路由不丢，且新列按「继承」语义补齐为 NULL。
        let routes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM routes WHERE icon IS NULL AND icon_tint IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(routes, 1);
        // 流水不丢，且 v9 列按默认值 0 补齐。
        let (logs, attempt): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(MAX(attempt_index), -1) FROM request_logs",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(logs, 1);
        assert_eq!(attempt, 0);
        // v11 列按 NULL 补齐。
        let session_null: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM request_logs WHERE session_id IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_null, 1, "旧流水的 session_id 应为 NULL");
        // v13 列按默认空规则补齐。
        let provider_rules: String = conn
            .query_row("SELECT header_rules FROM providers LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(provider_rules, "{}", "旧 provider 应补空规则");
        // v15：旧 provider 的 protocol/base_url/auth_scheme 迁移为一条端点。
        let (endpoints, protocol, base_url): (i64, String, String) = conn
            .query_row(
                "SELECT COUNT(*), protocol, base_url FROM provider_endpoints",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(endpoints, 1, "旧 provider 应迁移出一条端点");
        assert_eq!(protocol, "openai");
        assert_eq!(base_url, "https://example.com/v1");
    }

    #[test]
    fn migrates_from_legacy_version_without_losing_data() {
        let conn = open_fresh();
        // 构造 v7 旧库：先按最新结构建好，再拆掉 v8 / v9 / v10 引入的结构并回退版本号。
        insert_provider(&conn, "p1");
        conn.execute(
            "INSERT INTO upstream_models
                (id, provider_id, model_id, display_name, input_price, output_price)
             VALUES ('m1', 'p1', 'gpt-4o', 'GPT-4o', 2.5, 10.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO routes (id, alias, display_name, protocol, enabled, created_at)
             VALUES ('r1', 'chat', 'Chat', 'openai', 1, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // v7 的 route_targets 无唯一索引，允许同一 (route, model) 重复。
        conn.execute("DROP INDEX idx_route_targets_route_model", [])
            .unwrap();
        conn.execute(
            "INSERT INTO route_targets (id, route_id, upstream_model_id, priority, enabled)
             VALUES ('t1', 'r1', 'm1', 0, 1), ('t2', 'r1', 'm1', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO virtual_keys
                (id, key, name, enabled, quota_limit, quota_period, created_at)
             VALUES ('k1', 'sk-lumen-x', 'K', 1, 20.0, 'monthly', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('gateway_port', '8787')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO request_logs (id, occurred_at, endpoint, method, status, cost)
             VALUES ('l1', '2026-01-02T00:00:00Z', '/v1/chat/completions', 'POST', 'success', 0.5),
                    ('l2', '2026-01-03T00:00:00Z', '/v1/chat/completions', 'POST', 'error', 1.25)",
            [],
        )
        .unwrap();

        conn.execute("ALTER TABLE request_logs DROP COLUMN attempt_index", [])
            .unwrap();
        // v11 的 session_id 带索引，SQLite 不允许直接删除已索引的列，先撤索引再删列。
        conn.execute("DROP INDEX idx_request_logs_session", []).unwrap();
        conn.execute("ALTER TABLE request_logs DROP COLUMN session_id", [])
            .unwrap();
        conn.execute("ALTER TABLE request_logs DROP COLUMN ttfb_ms", [])
            .unwrap();
        conn.execute("ALTER TABLE route_targets DROP COLUMN header_rules", [])
            .unwrap();
        // v13：provider 头规则列也先撤，模拟 v7 库。
        conn.execute("ALTER TABLE providers DROP COLUMN header_rules", [])
            .unwrap();
        conn.execute("ALTER TABLE routes DROP COLUMN icon", [])
            .unwrap();
        conn.execute("ALTER TABLE routes DROP COLUMN icon_tint", [])
            .unwrap();
        // v15 之前 provider 自带协议列；回补成旧结构并填入待迁移的端点信息。
        downgrade_providers_to_pre_v15(&conn);
        conn.execute(
            "UPDATE providers
                SET base_url = 'https://api.anthropic.com/v1',
                    protocol = 'anthropic',
                    auth_scheme = 'x-api-key'",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 7).unwrap();

        configure(&conn).unwrap();

        assert_eq!(user_version(&conn), SCHEMA_VERSION);

        let table_count = |table: &str| -> i64 {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
        };
        assert_eq!(table_count("providers"), 1);
        assert_eq!(table_count("upstream_models"), 1);
        assert_eq!(table_count("routes"), 1);
        assert_eq!(table_count("virtual_keys"), 1);
        assert_eq!(table_count("settings"), 1);
        assert_eq!(table_count("request_logs"), 2);
        // v8 迁移把重复目标去重为一条。
        assert_eq!(table_count("route_targets"), 1);
        // v12 新列按默认空规则补齐。
        let rules: String = conn
            .query_row("SELECT header_rules FROM route_targets LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rules, "{}", "旧目标应补空规则");

        let (sum, attempt_max): (f64, i64) = conn
            .query_row(
                "SELECT COALESCE(SUM(cost), 0), COALESCE(MAX(attempt_index), -1) FROM request_logs",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!((sum - 1.75).abs() < 1e-9, "金额不得随升级丢失，got {sum}");
        assert_eq!(attempt_max, 0, "v9 新列按默认值补齐");

        // v11 新列按 NULL 补齐：旧流水的会话值未知，不得臆造。
        let sessions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM request_logs WHERE session_id IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sessions, 2, "旧流水的 session_id 应为 NULL");

        let provider_rules: String = conn
            .query_row("SELECT header_rules FROM providers LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(provider_rules, "{}", "旧 provider 应补空规则");

        let port: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'gateway_port'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(port, "8787", "设置不得随升级丢失");

        // v15：旧 provider 的协议列迁移为端点，且不产生悬空外键。
        let (endpoints, protocol, base_url): (i64, String, String) = conn
            .query_row(
                "SELECT COUNT(*), protocol, base_url FROM provider_endpoints",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(endpoints, 1);
        assert_eq!(protocol, "anthropic");
        assert_eq!(base_url, "https://api.anthropic.com/v1");
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
        conn.execute(
            "INSERT INTO provider_endpoints (id, provider_id, protocol, base_url)
             VALUES ('e1', 'p1', 'openai', 'https://example.com/v1')",
            [],
        )
        .unwrap();

        // 删除 provider 应级联删除其 model 与端点。
        conn.execute("DELETE FROM providers WHERE id = 'p1'", [])
            .unwrap();
        let models: i64 = conn
            .query_row("SELECT COUNT(*) FROM upstream_models", [], |row| row.get(0))
            .unwrap();
        let endpoints: i64 = conn
            .query_row("SELECT COUNT(*) FROM provider_endpoints", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(models, 0, "ON DELETE CASCADE 应生效");
        assert_eq!(endpoints, 0, "端点应随 provider 级联删除");

        // 引用不存在的 provider 应被外键拒绝。
        let dangling_model = conn.execute(
            "INSERT INTO upstream_models (id, provider_id, model_id, display_name)
             VALUES ('m2', 'missing', 'x', 'X')",
            [],
        );
        assert!(dangling_model.is_err(), "悬空外键应被拒绝");
        let dangling_endpoint = conn.execute(
            "INSERT INTO provider_endpoints (id, provider_id, protocol, base_url)
             VALUES ('e2', 'missing', 'openai', 'https://x')",
            [],
        );
        assert!(dangling_endpoint.is_err(), "悬空端点外键应被拒绝");
    }

    #[test]
    fn protocol_endpoint_unique_per_provider() {
        let conn = open_fresh();
        insert_provider(&conn, "p1");
        conn.execute(
            "INSERT INTO provider_endpoints (id, provider_id, protocol, base_url)
             VALUES ('e1', 'p1', 'openai', 'https://example.com/v1')",
            [],
        )
        .unwrap();
        let duplicate = conn.execute(
            "INSERT INTO provider_endpoints (id, provider_id, protocol, base_url)
             VALUES ('e2', 'p1', 'openai', 'https://other/v1')",
            [],
        );
        assert!(duplicate.is_err(), "同一 provider 同协议只能一条端点");
    }
}
