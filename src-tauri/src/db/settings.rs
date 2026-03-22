use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub poll_interval_ms: u64,
    pub max_entry_size_bytes: i64,
    pub max_total_size_bytes: i64,
    pub sensitive_auto_expire_secs: i64,
    pub sync_interval_secs: u64,
    pub webp_quality: f32,
    pub excluded_apps: Vec<String>,
    pub sync_sensitive: bool,
    pub mode: AppMode,
    pub require_password_on_open: bool,
    pub login_autostart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    Local,
    Cloud,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            poll_interval_ms: 1000,
            max_entry_size_bytes: 5 * 1024 * 1024,
            max_total_size_bytes: 50 * 1024 * 1024,
            sensitive_auto_expire_secs: 300,
            sync_interval_secs: 30,
            webp_quality: 100.0,
            excluded_apps: vec![
                "1password".into(),
                "bitwarden".into(),
                "keepass".into(),
                "keepassxc".into(),
                "lastpass".into(),
            ],
            sync_sensitive: false,
            mode: AppMode::Local,
            require_password_on_open: false,
            login_autostart: true,
        }
    }
}

pub struct SettingsDb {
    conn: Mutex<Connection>,
}

impl SettingsDb {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn get_settings(&self) -> AppSettings {
        let conn = self.conn.lock().unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'app_settings'",
                [],
                |row| row.get(0),
            )
            .ok();
        json.and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(settings).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('app_settings', ?1)",
            params![json],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_value(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .ok()
    }

    pub fn set_value(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_value(&self, key: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_returned_on_empty_db() {
        let db = SettingsDb::open_in_memory().unwrap();
        let settings = db.get_settings();
        assert_eq!(settings.poll_interval_ms, 1000);
        assert_eq!(settings.mode, AppMode::Local);
        assert!(settings.login_autostart);
    }

    #[test]
    fn save_and_load_settings() {
        let db = SettingsDb::open_in_memory().unwrap();
        let mut settings = AppSettings::default();
        settings.poll_interval_ms = 2000;
        settings.webp_quality = 90.0;
        settings.login_autostart = false;
        db.save_settings(&settings).unwrap();

        let loaded = db.get_settings();
        assert_eq!(loaded.poll_interval_ms, 2000);
        assert_eq!(loaded.webp_quality, 90.0);
        assert!(!loaded.login_autostart);
    }

    #[test]
    fn key_value_store() {
        let db = SettingsDb::open_in_memory().unwrap();
        assert!(db.get_value("test_key").is_none());
        db.set_value("test_key", "test_value").unwrap();
        assert_eq!(db.get_value("test_key").unwrap(), "test_value");
    }
}
