use std::sync::Arc;

use chrono::Local;
use serde::Serialize;
use tauri::State;

use crate::db::stats::Period;
use crate::db::telemetry::{query_snapshot, TelemetrySnapshot};
use crate::error::AppError;
use crate::gateway::models::{self, RemoteModel};
use crate::gateway::probe::{self, ProbeResult};
use crate::state::AppState;

/// 遥测快照 + 当前冷却中的上游模型，一次 IPC 返回。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryDto {
    #[serde(flatten)]
    pub snapshot: TelemetrySnapshot,
    pub cooling: Vec<String>,
}

/// `period` 决定吞吐曲线的桶粒度（日 = 整点、周 / 月 = 整天）；缺省为日。
/// `anchor` 用于查询任意历史周期，缺省为当前时间。生成速度与连通性固定近 24 小时。
#[tauri::command]
pub async fn query_telemetry_cmd(
    state: State<'_, Arc<AppState>>,
    period: Option<String>,
    anchor: Option<String>,
) -> Result<TelemetryDto, AppError> {
    let period = match period.as_deref().filter(|value| !value.is_empty()) {
        Some(value) => Period::parse(value)?,
        None => Period::Day,
    };
    let anchor = match anchor.filter(|value| !value.is_empty()) {
        Some(raw) => chrono::DateTime::parse_from_rfc3339(&raw)
            .map(|value| value.with_timezone(&Local))
            .map_err(|_| AppError::message(format!("无法解析时间：{raw}")))?,
        None => Local::now(),
    };
    let cooling = state.cooling_snapshot();
    let snapshot = query_snapshot(&state.db, period, anchor).await?;
    Ok(TelemetryDto { snapshot, cooling })
}

/// 手动连通性测试：对指定模型（缺省首个启用）在指定协议端点（缺省全部）各发一次最小消息请求。
#[tauri::command]
pub async fn test_provider_cmd(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    model_id: Option<String>,
    protocol: Option<String>,
) -> Result<Vec<ProbeResult>, AppError> {
    probe::probe(state.inner().clone(), provider_id, model_id, protocol).await
}

/// 拉取上游某协议端点暴露的模型列表，供登记模型时选择；`protocol` 缺省用第一个端点。
#[tauri::command]
pub async fn list_remote_models_cmd(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    protocol: Option<String>,
) -> Result<Vec<RemoteModel>, AppError> {
    models::list_remote_models(state.inner().clone(), provider_id, protocol).await
}
