use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::gateway::session::DEFAULT_SESSION_HEADERS;
use crate::state::DEFAULT_PORT;

fn default_true() -> bool {
    true
}

pub fn default_session_headers() -> Vec<String> {
    DEFAULT_SESSION_HEADERS
        .iter()
        .map(|name| name.to_string())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub port: u16,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    /// 会话候选头名表（按优先级）。客户端没带任何候选头时该次请求 `session_id` 为 NULL。
    #[serde(default = "default_session_headers")]
    pub session_headers: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            close_to_tray: true,
            session_headers: default_session_headers(),
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
    let session_headers = read_session_headers(conn);
    Ok(Settings {
        port,
        close_to_tray,
        session_headers,
    })
}

/// 读会话候选头名表：解析失败回落默认；解析成功则保留用户的选择（包括空表）。
fn read_session_headers(conn: &Connection) -> Vec<String> {
    let Ok(Some(raw)) = read_value(conn, "session_headers") else {
        return default_session_headers();
    };
    match serde_json::from_str::<Vec<String>>(&raw) {
        Ok(list) => list
            .into_iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect(),
        Err(_) => {
            tracing::warn!("session_headers 设置解析失败，回落默认候选头名表");
            default_session_headers()
        }
    }
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
    let headers = serde_json::to_string(&settings.session_headers)?;
    write_value(conn, "session_headers", &headers)?;
    Ok(settings.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, Db};

    fn settings() -> Settings {
        let db: Db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        get_settings(&conn).unwrap()
    }

    #[test]
    fn defaults_to_the_pinned_session_headers() {
        let settings = settings();
        let expected: Vec<String> = DEFAULT_SESSION_HEADERS
            .iter()
            .map(|name| name.to_string())
            .collect();
        assert_eq!(settings.session_headers, expected);
    }

    #[test]
    fn session_headers_round_trip() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let input = Settings {
            session_headers: vec!["x-custom-session".into()],
            ..Settings::default()
        };
        save_settings(&conn, &input).unwrap();
        let loaded = get_settings(&conn).unwrap();
        assert_eq!(loaded.session_headers, vec!["x-custom-session".to_string()]);
    }

    #[test]
    fn malformed_session_headers_fall_back_to_default() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('session_headers', 'not json')",
            [],
        )
        .unwrap();
        let loaded = get_settings(&conn).unwrap();
        let expected: Vec<String> = DEFAULT_SESSION_HEADERS
            .iter()
            .map(|name| name.to_string())
            .collect();
        assert_eq!(loaded.session_headers, expected);
    }

    #[test]
    fn empty_session_headers_are_preserved() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        save_settings(
            &conn,
            &Settings {
                session_headers: Vec::new(),
                ..Settings::default()
            },
        )
        .unwrap();
        assert!(get_settings(&conn).unwrap().session_headers.is_empty());
    }
}
