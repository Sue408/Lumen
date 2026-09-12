pub mod attribution;
#[cfg(debug_assertions)]
pub mod demo;
pub mod keys;
pub mod logs;
pub mod migrations;
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

/// 清空全部业务数据（提供商 / 模型 / 路由 / 密钥 / 日志），保留 `settings`。
/// 仅由显式用户操作（重置 / 演示注入）调用；版本升级不再触发清空。
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

pub fn open(path: &Path) -> Result<Db, AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    tune(&conn)?;
    migrations::configure(&conn)?;
    // 启动时刷新可能过期的优化器统计；统计未变化时 `optimize` 近乎零成本。
    conn.execute_batch("PRAGMA optimize;")?;
    Ok(Arc::new(Mutex::new(conn)))
}

/// 单连接的运行时调优。
///
/// - `journal_mode = WAL`：读写不互相阻塞，配合单写连接即可。
/// - `synchronous = NORMAL`：WAL 下提交不逐次 fsync。崩溃不会损坏数据库，但断电
///   可能丢掉最近若干个已提交事务——对逐请求写账的热路径，这是主要收益点。
/// - 其余为读放大、锁竞争与临时表参数。
fn tune(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 5000_i64)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", -8000_i64)?;
    conn.pragma_update(None, "mmap_size", 268_435_456_i64)?;
    Ok(())
}

#[cfg(test)]
pub fn open_in_memory() -> Result<Db, AppError> {
    let conn = Connection::open_in_memory()?;
    migrations::configure(&conn)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_applies_runtime_tuning_pragmas() {
        let dir = std::env::temp_dir().join(format!("lumen-tune-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = open(&dir.join("tune.db")).unwrap();

        let conn = db.lock().unwrap();
        let journal: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(journal.to_lowercase(), "wal");
        // synchronous: OFF=0 / NORMAL=1 / FULL=2
        let synchronous: i64 = conn
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        assert_eq!(synchronous, 1);
        // temp_store: DEFAULT=0 / FILE=1 / MEMORY=2
        let temp_store: i64 = conn
            .pragma_query_value(None, "temp_store", |row| row.get(0))
            .unwrap();
        assert_eq!(temp_store, 2);
        let busy_timeout: i64 = conn
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(busy_timeout, 5000);
        drop(conn);

        std::fs::remove_dir_all(&dir).ok();
    }
}
