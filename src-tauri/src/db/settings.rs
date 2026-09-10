use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::state::DEFAULT_PORT;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub port: u16,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            close_to_tray: true,
        }
    }
}

fn read_value(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    let raw = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(raw)
}

fn write_value(conn: &Connection, key: &str, value: &str) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_settings(conn: &Connection) -> Result<Settings, AppError> {
    let port = read_value(conn, "port")?
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let close_to_tray = read_value(conn, "close_to_tray")?
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(true);
    Ok(Settings {
        port,
        close_to_tray,
    })
}

pub fn save_settings(conn: &Connection, settings: &Settings) -> Result<Settings, AppError> {
    if settings.port < 1024 {
        return Err(AppError::message("端口需在 1024–65535 之间"));
    }
    write_value(conn, "port", &settings.port.to_string())?;
    write_value(
        conn,
        "close_to_tray",
        if settings.close_to_tray { "true" } else { "false" },
    )?;
    Ok(settings.clone())
}
