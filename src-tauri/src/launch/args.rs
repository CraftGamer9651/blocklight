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

fn offline_uuid(name: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{name}").as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

fn classpath_string(entries: &[PathBuf]) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    entries
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep)
}

fn substitute(template: &str, values: &HashMap<&str, String>) -> String {
    let mut out = template.to_string();
    for (key, value) in values {
        out = out.replace(&format!("${{{key}}}"), value);
    }
    out
}

fn expand_entries(entries: &[Value], values: &HashMap<&str, String>) -> Vec<String> {
    let mut out = Vec::new();
    for entry in entries {
        match entry {
            Value::String(s) => out.push(substitute(s, values)),
            Value::Object(obj) => {
                let rules: Vec<Rule> = obj
                    .get("rules")
                    .and_then(|r| serde_json::from_value(r.clone()).ok())
                    .unwrap_or_default();
                if !rules_allow(&rules) {
                    continue;
                }
                match obj.get("value") {
                    Some(Value::String(s)) => out.push(substitute(s, values)),
                    Some(Value::Array(items)) => {
                        for item in items {
                            if let Value::String(s) = item {
                                out.push(substitute(s, values));
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

pub struct BuiltCommand {
    pub java_args: Vec<String>,
    pub working_dir: PathBuf,
}

pub fn build_command(
    version: &VersionJson,
    prepared: &PreparedLaunch,
    game_directory: &PathBuf,
    identity: &LaunchIdentity,
    custom_jvm_arguments: &[String],
) -> BuiltCommand {
    let mut values: HashMap<&str, String> = HashMap::new();
    values.insert("auth_player_name", identity.player_name.clone());
    values.insert("version_name", version.id.clone());
    values.insert("game_directory", game_directory.to_string_lossy().to_string());
    values.insert("assets_root", prepared.assets_root.to_string_lossy().to_string());
    values.insert(
        "assets_index_name",
        version.assets.clone().unwrap_or_else(|| version.id.clone()),
    );
    values.insert("auth_uuid", identity.uuid.clone());
    values.insert("auth_access_token", identity.access_token.clone());
    values.insert("user_type", identity.user_type.to_string());
    values.insert(
        "version_type",
        version.version_type.clone().unwrap_or_else(|| "release".to_string()),
    );
    values.insert(
        "natives_directory",
        prepared.natives_dir.to_string_lossy().to_string(),
    );
    values.insert("launcher_name", LAUNCHER_NAME.to_string());
    values.insert("launcher_version", LAUNCHER_VERSION.to_string());
    values.insert("classpath", classpath_string(&prepared.classpath));
    values.insert("clientid", String::new());
    values.insert("auth_xuid", String::new());
    values.insert("resolution_width", "854".to_string());
    values.insert("resolution_height", "480".to_string());

    let mut jvm_args = vec![
        format!("-Xmx{DEFAULT_MEMORY_MB}M"),
        format!("-Dminecraft.launcher.brand={LAUNCHER_NAME}"),
        format!("-Dminecraft.launcher.version={LAUNCHER_VERSION}"),
    ];

    if cfg!(target_os = "macos") {
        jvm_args.push("-XstartOnFirstThread".to_string());
    }

    let mut game_args: Vec<String>;

    if let Some(arguments) = &version.arguments {
        jvm_args.extend(expand_entries(&arguments.jvm, &values));
        game_args = expand_entries(&arguments.game, &values);
    } else {
        jvm_args.push(format!("-Djava.library.path={}", prepared.natives_dir.to_string_lossy()));
        jvm_args.push("-cp".to_string());
        jvm_args.push(values.get("classpath").cloned().unwrap_or_default());

        let raw = version.minecraft_arguments.clone().unwrap_or_default();
        game_args = raw
            .split_whitespace()
            .map(|tok| substitute(tok, &values))
            .collect();
    }

    // Let the user override defaults by putting their args last.
    if !custom_jvm_arguments.is_empty() {
        jvm_args.extend(custom_jvm_arguments.iter().cloned());
    }

    if !jvm_args.iter().any(|a| a == "-cp" || a == "-classpath") {
        jvm_args.push("-cp".to_string());
        jvm_args.push(values.get("classpath").cloned().unwrap_or_default());
    }

    let mut full = jvm_args;
    full.push(version.main_class.clone());
    full.append(&mut game_args);

    BuiltCommand {
        java_args: full,
        working_dir: game_directory.clone(),
    }
}
