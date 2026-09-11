use std::sync::Arc;

use chrono::Local;
use serde::Serialize;
use tauri::State;

use crate::db::keys::{
    delete_virtual_key, get_virtual_key, list_virtual_keys, save_virtual_key, virtual_key_usage,
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

/// 单个虚拟密钥在其自身额度周期内的用量摘要，供密钥页书写台展示。
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

#[tauri::command]
pub async fn query_virtual_key_usage_cmd(
    state: State<'_, Arc<AppState>>,
    key_id: String,
) -> Result<KeyUsageDto, AppError> {
    let lookup_id = key_id.clone();
    let key = with_db(&state.db, move |conn| get_virtual_key(conn, &lookup_id))
        .await?
        .ok_or_else(|| AppError::message("未找到虚拟密钥"))?;
    let period = QuotaPeriod::parse(&key.quota_period);
    let start = period_start(period, Local::now());
    let usage_id = key_id.clone();
    let (spent, calls) =
        with_db(&state.db, move |conn| virtual_key_usage(conn, &usage_id, start)).await?;
    Ok(KeyUsageDto {
        key_id,
        spent: (spent * 100.0).round() / 100.0,
        calls,
        limit: key.quota_limit,
        period: key.quota_period,
        period_start: start.with_timezone(&chrono::Utc).to_rfc3339(),
    })
}
