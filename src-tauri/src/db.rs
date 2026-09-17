use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::AppError;

/// Thin wrapper so `tauri::State<Db>` gives every command a synchronized
/// handle to the single local SQLite file. All data here is local-only:
/// install history and offline profiles, nothing that requires a network
/// round trip, which is what lets these features keep working offline.
pub struct Db(pub Mutex<Connection>);

pub fn app_data_dir() -> Result<PathBuf, AppError> {
    let base = dirs::data_dir().ok_or_else(|| {
        AppError::Internal("could not resolve a local app data directory".into())
    })?;
    let dir = base.join("Blocklight");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn open() -> Result<Db, AppError> {
    let dir = app_data_dir()?;
    let conn = Connection::open(dir.join("blocklight.sqlite3"))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrate(&conn)?;
    Ok(Db(Mutex::new(conn)))
}

fn migrate(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS install_history (
            id            TEXT PRIMARY KEY,
            project_name  TEXT NOT NULL,
            platform      TEXT NOT NULL,
            content_type  TEXT NOT NULL,
            version_number TEXT NOT NULL,
            instance_id   TEXT NOT NULL,
            installed_at  TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_install_history_time
            ON install_history (installed_at DESC);

        CREATE TABLE IF NOT EXISTS offline_profiles (
            id           TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            created_at   TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS app_settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;
    Ok(())
}
