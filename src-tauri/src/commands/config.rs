use std::sync::Arc;

use chrono::{DateTime, Local};
use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::keys::{
    delete_virtual_key, list_virtual_keys, save_virtual_key, virtual_key_usage,
};
use crate::db::models::{
    Provider, ProviderInput, RouteWithTargets, RouteInput, UpstreamModel, UpstreamModelInput,
    VirtualKey, VirtualKeyInput,
};
use crate::db::providers::{
    delete_provider, delete_upstream_model, list_providers, list_upstream_models, save_provider,
    save_upstream_model,
};
use crate::db::routes::{delete_route, list_routes, save_route};
use crate::db::with_db;
use crate::error::AppError;
use crate::gateway::quota::{period_start, QuotaPeriod};
use crate::state::AppState;

#[tauri::command]
pub async fn list_providers_cmd(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<Provider>, AppError> {
    with_db(&state.db, list_providers).await
}

#[tauri::command]
pub async fn save_provider_cmd(
    state: State<'_, Arc<AppState>>,
    input: ProviderInput,
) -> Result<Provider, AppError> {
    with_db(&state.db, move |conn| save_provider(conn, &input)).await
}

#[tauri::command]
pub async fn delete_provider_cmd(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), AppError> {
    with_db(&state.db, move |conn| delete_provider(conn, &id)).await
}

#[tauri::command]
pub async fn list_upstream_models_cmd(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<UpstreamModel>, AppError> {
    with_db(&state.db, list_upstream_models).await
}

#[tauri::command]
pub async fn save_upstream_model_cmd(
    state: State<'_, Arc<AppState>>,
    input: UpstreamModelInput,
) -> Result<UpstreamModel, AppError> {
    with_db(&state.db, move |conn| save_upstream_model(conn, &input)).await
}

#[tauri::command]
pub async fn delete_upstream_model_cmd(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), AppError> {
    with_db(&state.db, move |conn| delete_upstream_model(conn, &id)).await
}

#[tauri::command]
pub async fn list_routes_cmd(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<RouteWithTargets>, AppError> {
    with_db(&state.db, list_routes).await
}

#[tauri::command]
pub async fn save_route_cmd(
    state: State<'_, Arc<AppState>>,
    input: RouteInput,
) -> Result<RouteWithTargets, AppError> {
    with_db(&state.db, move |conn| save_route(conn, &input)).await
}

#[tauri::command]
pub async fn delete_route_cmd(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), AppError> {
    with_db(&state.db, move |conn| delete_route(conn, &id)).await
}

#[tauri::command]
pub async fn list_virtual_keys_cmd(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<VirtualKey>, AppError> {
    with_db(&state.db, list_virtual_keys).await
}

#[tauri::command]
pub async fn save_virtual_key_cmd(
    state: State<'_, Arc<AppState>>,
    input: VirtualKeyInput,
) -> Result<VirtualKey, AppError> {
    with_db(&state.db, move |conn| save_virtual_key(conn, &input)).await
}

#[tauri::command]
pub async fn delete_virtual_key_cmd(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), AppError> {
    with_db(&state.db, move |conn| delete_virtual_key(conn, &id)).await
}

/// 单个虚拟密钥在其自身额度周期内的用量摘要。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyUsageDto {
    pub key_id: String,
    pub spent: f64,
    pub calls: i64,
    pub limit: Option<f64>,
    pub period: String,
    pub period_start: String,
}

/// 全部虚拟密钥在各自额度周期内的用量，一次取回。
/// 每把的周期可能不同，所以起始点逐把算；密钥数量很少，逐把查询在同一把锁内完成，
/// 省掉的是前端的 N 次 IPC 往返，而非数据库往返。
#[tauri::command]
pub async fn query_virtual_keys_usage_cmd(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<KeyUsageDto>, AppError> {
    with_db(&state.db, |conn| build_key_usages(conn, Local::now())).await
}

fn build_key_usages(conn: &Connection, now: DateTime<Local>) -> Result<Vec<KeyUsageDto>, AppError> {
    let keys = list_virtual_keys(conn)?;
    let mut usages = Vec::with_capacity(keys.len());
    for key in keys {
        let start = period_start(QuotaPeriod::parse(&key.quota_period), now);
        let (spent, calls) = virtual_key_usage(conn, &key.id, start)?;
        usages.push(KeyUsageDto {
            key_id: key.id,
            spent: (spent * 100.0).round() / 100.0,
            calls,
            limit: key.quota_limit,
            period: key.quota_period,
            period_start: start.with_timezone(&chrono::Utc).to_rfc3339(),
        });
    }
    Ok(usages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::keys::save_virtual_key;
    use crate::db::models::VirtualKeyInput;
    use crate::db::open_in_memory;
    use chrono::Duration;
    use rusqlite::params;

    fn key(name: &str, period: &str) -> VirtualKeyInput {
        VirtualKeyInput {
            id: None,
            key: None,
            name: name.to_string(),
            enabled: true,
            quota_limit: Some(5.0),
            quota_period: period.to_string(),
        }
    }

    fn insert_success(conn: &Connection, key_id: &str, occurred: DateTime<Local>, cost: f64) {
        conn.execute(
            "INSERT INTO request_logs
                 (id, occurred_at, endpoint, method, kind, status, cost, virtual_key_id)
             VALUES (?1, ?2, '/v1/chat/completions', 'POST', 'chat', 'success', ?3, ?4)",
            params![
                uuid::Uuid::new_v4().to_string(),
                occurred.with_timezone(&chrono::Utc).to_rfc3339(),
                cost,
                key_id
            ],
        )
        .unwrap();
    }

    #[test]
    fn batch_usage_measures_each_key_in_its_own_period() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let now = Local::now();
        let monthly = save_virtual_key(&conn, &key("月付", "monthly")).unwrap();
        let weekly = save_virtual_key(&conn, &key("周付", "weekly")).unwrap();

        // 每把各一笔「本期起点后」与一笔「久到任何周期之外」
        insert_success(
            &conn,
            &monthly.id,
            period_start(QuotaPeriod::Monthly, now) + Duration::hours(1),
            2.0,
        );
        insert_success(&conn, &monthly.id, now - Duration::days(400), 99.0);
        insert_success(
            &conn,
            &weekly.id,
            period_start(QuotaPeriod::Weekly, now) + Duration::hours(1),
            3.0,
        );
        insert_success(&conn, &weekly.id, now - Duration::days(400), 88.0);

        let usages = build_key_usages(&conn, now).unwrap();
        assert_eq!(usages.len(), 2);

        let month = usages.iter().find(|u| u.key_id == monthly.id).unwrap();
        let week = usages.iter().find(|u| u.key_id == weekly.id).unwrap();
        assert_eq!((month.spent, month.calls), (2.0, 1));
        assert_eq!((week.spent, week.calls), (3.0, 1));

        // 起始点逐把不同，各按自己的周期算
        assert_eq!(
            month.period_start,
            period_start(QuotaPeriod::Monthly, now)
                .with_timezone(&chrono::Utc)
                .to_rfc3339()
        );
        assert_eq!(
            week.period_start,
            period_start(QuotaPeriod::Weekly, now)
                .with_timezone(&chrono::Utc)
                .to_rfc3339()
        );
    }
}
