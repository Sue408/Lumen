use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::state::DEFAULT_PORT;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self { port: DEFAULT_PORT }
    }
}

pub fn get_settings(conn: &Connection) -> Result<Settings, AppError> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'port'", [], |row| {
            row.get(0)
        })
        .optional()?;
    let port = raw
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    Ok(Settings { port })
}

pub fn save_settings(conn: &Connection, settings: &Settings) -> Result<Settings, AppError> {
    if settings.port < 1024 {
        return Err(AppError::message("端口需在 1024–65535 之间"));
    }
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('port', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![settings.port.to_string()],
    )?;
    Ok(settings.clone())
}
