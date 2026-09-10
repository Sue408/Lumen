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
        "INSERT INTO virtual_keys (id, key, name, enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            key = excluded.key,
            name = excluded.name,
            enabled = excluded.enabled",
        params![id, key, input.name, input.enabled as i64, created_at],
    )?;
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

fn generate_key() -> String {
    format!("sk-lumen-{}", uuid::Uuid::new_v4().simple())
}
