use std::sync::Arc;

use tauri::State;

use crate::db::keys::{delete_virtual_key, list_virtual_keys, save_virtual_key};
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
