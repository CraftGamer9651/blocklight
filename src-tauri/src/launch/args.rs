use std::collections::HashMap;
use std::path::PathBuf;

use md5::{Digest, Md5};
use serde_json::Value;

use super::download::PreparedLaunch;
use super::version_json::{rules_allow, Rule, VersionJson};

const LAUNCHER_NAME: &str = "Blocklight";
const LAUNCHER_VERSION: &str = "0.1.0";
const DEFAULT_MEMORY_MB: u32 = 2048;

pub struct LaunchIdentity {
    pub player_name: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: &'static str,
}

/// A local offline profile's identity. `access_token` is a fixed
/// placeholder (never a real credential) -- single-player and LAN don't
/// validate it, which is exactly the "legitimate offline play" boundary
/// this is meant to stay inside.
pub fn offline_identity(display_name: &str) -> LaunchIdentity {
    LaunchIdentity {
        player_name: display_name.to_string(),
        uuid: offline_uuid(display_name),
        access_token: "0".to_string(),
        user_type: "legacy",
    }
}

pub fn online_identity(gamertag: &str, uuid: &str, access_token: &str) -> LaunchIdentity {
    LaunchIdentity {
        player_name: gamertag.to_string(),
        uuid: uuid.to_string(),
        access_token: access_token.to_string(),
        user_type: "msa",
    }
}

/// Reproduces vanilla Minecraft's own offline-mode UUID formula --
/* ... clipped in output for brevity ... */
