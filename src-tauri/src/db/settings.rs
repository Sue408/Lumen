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
    /// 出站代理 URL（如 `http://127.0.0.1:7890`）；`None` 或空串表示直连。
    #[serde(default)]
    pub proxy_url: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            close_to_tray: true,
            session_headers: default_session_headers(),
            proxy_url: None,
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
    let proxy_url = read_value(conn, "proxy_url")?.and_then(|raw| normalize_proxy_url(&raw));
    Ok(Settings {
        port,
        close_to_tray,
        session_headers,
        proxy_url,
    })
}

/// 代理 URL 归一：去空白，空串视为「未配置」（直连）。
fn normalize_proxy_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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
    let proxy = settings
        .proxy_url
        .as_deref()
        .and_then(normalize_proxy_url);
    write_value(conn, "proxy_url", proxy.as_deref().unwrap_or(""))?;
    Ok(settings.clone())
}

/// 开机自启的**用户意图**，与注册表实际状态分开存储。
///
/// `None` 表示从未记录（首次运行或从旧版本升级）。启动时据此重放一次注册，即可修复
/// 「重装后 exe 路径变了、注册表却仍指向旧路径」导致的自启失效。
pub fn autostart_intent(conn: &Connection) -> Option<bool> {
    read_value(conn, "autostart")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<bool>().ok())
}

pub fn set_autostart_intent(conn: &Connection, enabled: bool) -> Result<(), AppError> {
    write_value(conn, "autostart", if enabled { "true" } else { "false" })
}

/// 「随系统启动时自动开启网关」开关。默认关闭。
pub fn autostart_gateway(conn: &Connection) -> bool {
    read_value(conn, "autostart_gateway")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(false)
}

pub fn set_autostart_gateway(conn: &Connection, enabled: bool) -> Result<(), AppError> {
    write_value(
        conn,
        "autostart_gateway",
        if enabled { "true" } else { "false" },
    )
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

    #[test]
    fn defaults_to_direct_connection() {
        assert_eq!(settings().proxy_url, None);
    }

    #[test]
    fn proxy_url_round_trip() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let input = Settings {
            proxy_url: Some("http://127.0.0.1:7890".into()),
            ..Settings::default()
        };
        save_settings(&conn, &input).unwrap();
        assert_eq!(
            get_settings(&conn).unwrap().proxy_url.as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn blank_proxy_url_means_direct() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        save_settings(
            &conn,
            &Settings {
                proxy_url: Some("   ".into()),
                ..Settings::default()
            },
        )
        .unwrap();
        assert_eq!(get_settings(&conn).unwrap().proxy_url, None);

        save_settings(
            &conn,
            &Settings {
                proxy_url: None,
                ..Settings::default()
            },
        )
        .unwrap();
        assert_eq!(get_settings(&conn).unwrap().proxy_url, None);
    }

    #[test]
    fn autostart_intent_starts_unset_then_round_trips() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        assert_eq!(autostart_intent(&conn), None, "未记录时应为 None");
        set_autostart_intent(&conn, true).unwrap();
        assert_eq!(autostart_intent(&conn), Some(true));
        set_autostart_intent(&conn, false).unwrap();
        assert_eq!(autostart_intent(&conn), Some(false));
    }

    #[test]
    fn autostart_gateway_defaults_off_and_round_trips() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        assert!(!autostart_gateway(&conn));
        set_autostart_gateway(&conn, true).unwrap();
        assert!(autostart_gateway(&conn));
    }
}
