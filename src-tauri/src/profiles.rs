use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{AccountState, ActiveAccountKind, OfflineProfile};

const KEY_ACTIVE_KIND: &str = "active_account_kind";
const KEY_MS_SIGNED_IN: &str = "microsoft_signed_in";
const KEY_MS_GAMERTAG: &str = "microsoft_gamertag";
const KEY_ACTIVE_OFFLINE_PROFILE: &str = "active_offline_profile_id";

/// Create a local, clearly-labeled offline profile. This never touches
/// Minecraft/Microsoft authentication -- it is purely a display name
/// Blocklight uses for its own UI, not a substitute for a licensed
/// Minecraft account.
pub fn create_offline_profile(
    conn: &Connection,
    display_name: &str,
) -> Result<OfflineProfile, AppError> {
    let trimmed = display_name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 24 {
        return Err(AppError::Internal(
            "player name must be 1-24 characters".into(),
        ));
    }

    let profile = OfflineProfile {
        id: Uuid::new_v4().to_string(),
        display_name: trimmed.to_string(),
        created_at: Utc::now().to_rfc3339(),
    };

    conn.execute(
        "INSERT INTO offline_profiles (id, display_name, created_at) VALUES (?1, ?2, ?3)",
        params![profile.id, profile.display_name, profile.created_at],
    )?;

    set_setting(conn, KEY_ACTIVE_OFFLINE_PROFILE, &profile.id)?;
    set_setting(conn, KEY_ACTIVE_KIND, "offline")?;

    Ok(profile)
}

pub fn list_offline_profiles(conn: &Connection) -> Result<Vec<OfflineProfile>, AppError> {
    let mut stmt =
        conn.prepare("SELECT id, display_name, created_at FROM offline_profiles ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(OfflineProfile {
            id: row.get(0)?,
            display_name: row.get(1)?,
            created_at: row.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Switch which *kind* of account is active (Microsoft vs. offline).
/// Never done automatically on connection loss -- only ever in response
/// to an explicit user action, per spec ("Do not automatically log users
/// out of their Microsoft account merely because they temporarily lose
/// internet access"). Note this does **not** change which offline
/// profile is active -- see `switch_to_offline_profile` for that.
pub fn set_active_account(conn: &Connection, kind: ActiveAccountKind) -> Result<(), AppError> {
    let raw = match kind {
        ActiveAccountKind::Microsoft => "microsoft",
        ActiveAccountKind::Offline => "offline",
    };
    set_setting(conn, KEY_ACTIVE_KIND, raw)
}

/// Switches to a specific offline profile. This is the piece that was
/// previously missing: the old command only ever re-confirmed the
/// active *kind* was "offline" without recording *which* profile, so
/// switching between two offline profiles silently did nothing once
/// more than one existed.
pub fn switch_to_offline_profile(conn: &Connection, profile_id: &str) -> Result<(), AppError> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM offline_profiles WHERE id = ?1",
        params![profile_id],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Err(AppError::Internal(
            "that offline profile no longer exists".into(),
        ));
    }
    set_setting(conn, KEY_ACTIVE_OFFLINE_PROFILE, profile_id)?;
    set_setting(conn, KEY_ACTIVE_KIND, "offline")?;
    Ok(())
}

/// Deletes a local offline profile. If it was the active one, falls
/// back to another remaining profile automatically so the person isn't
/// left with `active_kind: offline` pointing at nothing; if none remain,
/// `get_account_state`'s lookup already resolves a dangling id to `None`
/// (see its `.optional()` below), which correctly sends them back to
/// the first-run choice rather than leaving the app in a broken state.
pub fn delete_offline_profile(conn: &Connection, profile_id: &str) -> Result<(), AppError> {
    let was_active = get_setting(conn, KEY_ACTIVE_OFFLINE_PROFILE)?.as_deref() == Some(profile_id);

    conn.execute(
        "DELETE FROM offline_profiles WHERE id = ?1",
        params![profile_id],
    )?;

    if was_active {
        if let Some(next) = list_offline_profiles(conn)?.first() {
            set_setting(conn, KEY_ACTIVE_OFFLINE_PROFILE, &next.id)?;
        }
    }

    Ok(())
}

/// Record the outcome of a real Microsoft OAuth2 device-code flow. This
/// function only stores what the launcher's UI needs to display (signed
/// in yes/no, gamertag) -- it never fabricates or bypasses the sign-in
/// itself. See `MicrosoftAuth` in providers/mod.rs for where a real
/// client ID must be plugged in.
pub fn set_microsoft_session(
    conn: &Connection,
    signed_in: bool,
    gamertag: Option<&str>,
) -> Result<(), AppError> {
    set_setting(conn, KEY_MS_SIGNED_IN, if signed_in { "1" } else { "0" })?;
    if let Some(tag) = gamertag {
        set_setting(conn, KEY_MS_GAMERTAG, tag)?;
    }
    Ok(())
}

pub fn get_account_state(conn: &Connection) -> Result<AccountState, AppError> {
    let active_kind_raw = get_setting(conn, KEY_ACTIVE_KIND)?.unwrap_or_else(|| "offline".into());
    let active_kind = if active_kind_raw == "microsoft" {
        ActiveAccountKind::Microsoft
    } else {
        ActiveAccountKind::Offline
    };

    let microsoft_signed_in = get_setting(conn, KEY_MS_SIGNED_IN)?.as_deref() == Some("1");
    let microsoft_gamertag = get_setting(conn, KEY_MS_GAMERTAG)?;

    let active_profile_id = get_setting(conn, KEY_ACTIVE_OFFLINE_PROFILE)?;
    let offline_profile = match active_profile_id {
        Some(id) => conn
            .query_row(
                "SELECT id, display_name, created_at FROM offline_profiles WHERE id = ?1",
                params![id],
                |row| {
                    Ok(OfflineProfile {
                        id: row.get(0)?,
                        display_name: row.get(1)?,
                        created_at: row.get(2)?,
                    })
                },
            )
            .optional()?,
        None => None,
    };

    Ok(AccountState {
        active_kind,
        microsoft_signed_in,
        microsoft_gamertag,
        offline_profile,
    })
}

fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?)
}
