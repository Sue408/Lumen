use rusqlite::{params, Connection};

use super::models::{Route, RouteInput, RouteTarget, RouteWithTargets};
use crate::error::AppError;

pub fn list_routes(conn: &Connection) -> Result<Vec<RouteWithTargets>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM routes ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], Route::from_row)?;
    let mut routes = Vec::new();
    for row in rows {
        let route = row?;
        let targets = list_targets(conn, &route.id)?;
        routes.push(RouteWithTargets { route, targets });
    }
    Ok(routes)
}

pub fn get_route(conn: &Connection, id: &str) -> Result<Option<RouteWithTargets>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM routes WHERE id = ?1")?;
    let mut rows = stmt.query_map([id], Route::from_row)?;
    match rows.next() {
        Some(row) => {
            let route = row?;
            let targets = list_targets(conn, &route.id)?;
            Ok(Some(RouteWithTargets { route, targets }))
        }
        None => Ok(None),
    }
}

fn list_targets(conn: &Connection, route_id: &str) -> Result<Vec<RouteTarget>, AppError> {
    let mut stmt = conn
        .prepare("SELECT * FROM route_targets WHERE route_id = ?1 ORDER BY priority ASC, rowid ASC")?;
    let rows = stmt.query_map([route_id], RouteTarget::from_row)?;
    let mut targets = Vec::new();
    for row in rows {
        targets.push(row?);
    }
    Ok(targets)
}

pub fn save_route(conn: &Connection, input: &RouteInput) -> Result<RouteWithTargets, AppError> {
    let id = input
        .id
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let created_at = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO routes (id, alias, display_name, enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            alias = excluded.alias,
            display_name = excluded.display_name,
            enabled = excluded.enabled",
        params![id, input.alias, input.display_name, input.enabled as i64, created_at],
    )?;
    tx.execute("DELETE FROM route_targets WHERE route_id = ?1", [&id])?;
    for target in &input.targets {
        tx.execute(
            "INSERT INTO route_targets (id, route_id, upstream_model_id, priority, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                id,
                target.upstream_model_id,
                target.priority,
                target.enabled as i64,
            ],
        )?;
    }
    tx.commit()?;
    get_route(conn, &id)?.ok_or_else(|| AppError::Message("保存路由失败".into()))
}

pub fn delete_route(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM routes WHERE id = ?1", [id])?;
    Ok(())
}

/// 返回启用中的别名及其展示名，供 `/v1/models` 使用。
pub fn list_enabled_aliases(conn: &Connection) -> Result<Vec<(String, String)>, AppError> {
    let mut stmt =
        conn.prepare("SELECT alias, display_name FROM routes WHERE enabled = 1 ORDER BY alias ASC")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut aliases = Vec::new();
    for row in rows {
        aliases.push(row?);
    }
    Ok(aliases)
}
