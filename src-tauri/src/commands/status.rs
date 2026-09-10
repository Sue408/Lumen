use std::sync::Arc;

use tauri::State;

use crate::error::AppError;
use crate::gateway;
use crate::state::{AppState, GatewayStatus};

#[tauri::command]
pub fn gateway_status(state: State<'_, Arc<AppState>>) -> GatewayStatus {
    state.status()
}

#[tauri::command]
pub async fn start_gateway(state: State<'_, Arc<AppState>>) -> Result<GatewayStatus, AppError> {
    let state = state.inner().clone();
    if let Err(error) = gateway::start(state.clone()).await {
        state
            .events
            .status(&GatewayStatus::failed(state.port, error.to_string()));
        return Err(error);
    }
    Ok(state.status())
}

#[tauri::command]
pub async fn stop_gateway(state: State<'_, Arc<AppState>>) -> Result<GatewayStatus, AppError> {
    let state = state.inner().clone();
    gateway::stop(&state).await?;
    Ok(state.status())
}
