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
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrations::configure(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
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
