use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{ContentType, InstallHistoryEntry, Platform};

/// Record a completed install. Only the fields shown in the "Recently
/// Installed" list are kept -- no file paths, no user identifiers -- per
/// the spec's "do not retain unnecessary personal information" rule.
pub fn record(
    conn: &Connection,
    project_name: &str,
    platform: Platform,
    content_type: ContentType,
    version_number: &str,
    instance_id: &str,
) -> Result<InstallHistoryEntry, AppError> {
    let entry = InstallHistoryEntry {
        id: Uuid::new_v4().to_string(),
        project_name: project_name.to_string(),
        platform,
        content_type,
        version_number: version_number.to_string(),
        instance_id: instance_id.to_string(),
        installed_at: Utc::now().to_rfc3339(),
    };

    conn.execute(
        "INSERT INTO install_history
            (id, project_name, platform, content_type, version_number, instance_id, installed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entry.id,
            entry.project_name,
            serde_json::to_string(&entry.platform).unwrap(),
            serde_json::to_string(&entry.content_type).unwrap(),
            entry.version_number,
            entry.instance_id,
            entry.installed_at,
        ],
    )?;

    Ok(entry)
}

pub fn list(conn: &Connection, limit: u32) -> Result<Vec<InstallHistoryEntry>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, project_name, platform, content_type, version_number, instance_id, installed_at
         FROM install_history
         ORDER BY installed_at DESC
         LIMIT ?1",
    )?;

    let rows = stmt.query_map(params![limit], |row| {
        let platform_raw: String = row.get(2)?;
        let content_type_raw: String = row.get(3)?;
        Ok(InstallHistoryEntry {
            id: row.get(0)?,
            project_name: row.get(1)?,
            platform: serde_json::from_str(&platform_raw).unwrap_or(Platform::Modrinth),
            content_type: serde_json::from_str(&content_type_raw).unwrap_or(ContentType::Mod),
            version_number: row.get(4)?,
            instance_id: row.get(5)?,
            installed_at: row.get(6)?,
        })
    })?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn clear(conn: &Connection) -> Result<(), AppError> {
    conn.execute("DELETE FROM install_history", [])?;
    Ok(())
}
