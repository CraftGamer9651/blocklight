use std::collections::HashMap;
use std::path::PathBuf;

use md5::{Digest, Md5};
use serde_json::Value;

use super::download::PreparedLaunch;
use super::version_json::{rules_allow, Rule, VersionJson};

const LAUNCHER_NAME: &str = "Blocklight";
const LAUNCHER_VERSION: &str = "0.1.0";

/// Bounds for the auto-detected default heap (see `default_memory_mb`).
/// A fixed floor keeps a low-RAM machine from getting a heap too small
/// to run a modern modded instance; a fixed ceiling keeps a high-RAM
/// machine from handing Minecraft (which has no real use for, say, 32
/// of 64 available GB) an oversized heap that just makes GC pauses
/// larger when they do happen.
const MIN_DEFAULT_MEMORY_MB: u32 = 2048;
const MAX_DEFAULT_MEMORY_MB: u32 = 8192;
/// Used only if system memory can't be detected at all.
const FALLBACK_MEMORY_MB: u32 = 4096;

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

/// Builds the identity for a signed-in Microsoft account launch. Not
/// called yet: `commands::launch_instance` currently refuses to launch
/// a Microsoft-active account rather than call this, because the live
/// access token this needs is deliberately never persisted anywhere
/// (see `profiles.rs`) -- there's nothing to pass it yet. Kept, with
/// its real signature, for when that token storage/refresh work lands.
#[allow(dead_code)]
pub fn online_identity(gamertag: &str, uuid: &str, access_token: &str) -> LaunchIdentity {
    LaunchIdentity {
        player_name: gamertag.to_string(),
        uuid: uuid.to_string(),
        access_token: access_token.to_string(),
        user_type: "msa",
    }
}

/// Reproduces vanilla Minecraft's own offline-mode UUID formula --
/// Java's `UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes())`,
/// which is an MD5 digest with the version (3) and variant (RFC 4122)
/// bits fixed up. Not a real Mojang account id; just a stable id derived
/// the same way the official launcher derives one for offline play, so
/// a given profile name always maps to the same player/world data.
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

/// Picks a default heap size when an instance doesn't have one
/// configured explicitly: half of total system RAM, clamped to a sane
/// range. Falls back to a fixed default if system memory can't be
/// detected at all (an unusual environment, or a command this doesn't
/// know how to parse the output of).
pub fn default_memory_mb() -> u32 {
    match system_memory_mb() {
        Some(total) => ((total / 2) as u32).clamp(MIN_DEFAULT_MEMORY_MB, MAX_DEFAULT_MEMORY_MB),
        None => FALLBACK_MEMORY_MB,
    }
}

/// Best-effort total system RAM in MB. One function per platform behind
/// `cfg`, same pattern as `process::kill_pid` -- exactly one of these
/// compiles for any given build, so there's no runtime branching to get
/// wrong, just whichever one matches the target.
#[cfg(target_os = "linux")]
fn system_memory_mb() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

#[cfg(target_os = "macos")]
fn system_memory_mb() -> Option<u64> {
    let output = std::process::Command::new("sysctl").args(["-n", "hw.memsize"]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let bytes: u64 = text.trim().parse().ok()?;
    Some(bytes / 1024 / 1024)
}

#[cfg(target_os = "windows")]
fn system_memory_mb() -> Option<u64> {
    let output = std::process::Command::new("wmic")
        .args(["computersystem", "get", "TotalPhysicalMemory", "/value"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|l| l.trim_start().starts_with("TotalPhysicalMemory="))?;
    let bytes: u64 = line.trim().trim_start_matches("TotalPhysicalMemory=").trim().parse().ok()?;
    Some(bytes / 1024 / 1024)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn system_memory_mb() -> Option<u64> {
    None
}

fn substitute(template: &str, values: &HashMap<&str, String>) -> String {
    let mut out = template.to_string();
    for (key, value) in values {
        out = out.replace(&format!("${{{key}}}"), value);
    }
    out
}

/// Modern `arguments.game`/`arguments.jvm` entries mix plain strings
/// with `{rules, value}` objects; this flattens one such list into
/// substituted strings, dropping any entry whose rules don't match.
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

/// Assembles the full JVM command line: memory flags, JVM args
/// (classpath, native library path, launcher branding), the main class,
/// then game args (player identity, asset/version locations). Handles
/// both the modern `arguments` format and legacy `minecraftArguments`
/// (older versions, which also lack a declared `arguments.jvm` -- a
/// standard fallback set is supplied instead, same as every other
/// launcher does for those versions).
///
/// `user_jvm_args` are the instance's already-parsed custom JVM
/// arguments (see `user_args::parse`). Any Blocklight default they
/// override is dropped first, then they're placed after every
/// launcher/loader JVM flag and before the main class.
pub fn build_command(
    version: &VersionJson,
    prepared: &PreparedLaunch,
    game_directory: &PathBuf,
    identity: &LaunchIdentity,
    memory_mb: Option<u32>,
    user_jvm_args: &[String],
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
    // Used by NeoForge/Forge version JSONs: the module path and a couple
    // of -D properties are built from these.
    values.insert(
        "library_directory",
        prepared.library_dir.to_string_lossy().to_string(),
    );
    values.insert(
        "classpath_separator",
        (if cfg!(windows) { ";" } else { ":" }).to_string(),
    );
    values.insert("clientid", String::new());
    values.insert("auth_xuid", String::new());
    values.insert("resolution_width", "854".to_string());
    values.insert("resolution_height", "480".to_string());

    let heap_mb = memory_mb.unwrap_or_else(default_memory_mb);

    let mut jvm_args = vec![
        // Presizing -Xms to match -Xmx avoids the JVM growing the heap
        // in stages as the game's own early allocation (world load,
        // chunk generation) ramps up -- each grow step is itself a
        // pause, so starting at full size trades a marginally larger
        // upfront reservation for a smoother first few seconds in-game.
        format!("-Xms{heap_mb}M"),
        format!("-Xmx{heap_mb}M"),
        // G1 tuned for shorter/fewer pauses over raw throughput --
        // for a game, frame-time consistency matters more than total
        // work done, which is the opposite trade-off a batch job would
        // want. These are widely-used, publicly documented G1 tuning
        // parameters (Oracle's own G1 tuning guide covers each of
        // them), not anything Blocklight-specific.
        //
        // Deliberately NOT included: -XX:G1NewSizePercent /
        // G1MaxNewSizePercent. They're flagged "experimental" (requiring
        // -XX:+UnlockExperimentalVMOptions first) on some JDK vendors/
        // versions but not others -- exactly the kind of JVM quirk that
        // varies by environment in a way this project can't verify
        // against every distribution. G1's own default new-generation
        // sizing is solid on its own, and a launcher refusing to start
        // the game at all is a far worse failure than a slightly less
        // finely-tuned GC, so this drops the two flags rather than bet
        // reliability on an "unlock everything experimental" switch
        // whose own portability is just as unverified.
        "-XX:+UseG1GC".to_string(),
        "-XX:+ParallelRefProcEnabled".to_string(),
        "-XX:MaxGCPauseMillis=200".to_string(),
        "-XX:G1HeapRegionSize=8M".to_string(),
        "-XX:G1ReservePercent=20".to_string(),
        "-XX:InitiatingHeapOccupancyPercent=15".to_string(),
        // Stops a mod calling System.gc() from forcing a full,
        // stop-the-world collection mid-game.
        "-XX:+DisableExplicitGC".to_string(),
        "-XX:+UseStringDeduplication".to_string(),
        // Commits and zeroes heap pages upfront instead of on first
        // touch. Adds a small, fixed amount of time at JVM startup
        // (roughly proportional to heap size) in exchange for not
        // taking page faults for new heap pages *during* gameplay --
        // deliberately spending a bit of launch time to buy smoother
        // play time, which is the trade the "fast to launch AND fast
        // to play" goal actually calls for here.
        "-XX:+AlwaysPreTouch".to_string(),
        format!("-Dminecraft.launcher.brand={LAUNCHER_NAME}"),
        format!("-Dminecraft.launcher.version={LAUNCHER_VERSION}"),
        // Skips (Neo)Forge's early Swing splash window, saving a little
        // startup time; harmlessly ignored by vanilla/Fabric/Quilt.
        "-Dfml.earlyprogresswindow=false".to_string(),
    ];
    // macOS needs the LWJGL/GLFW event loop to run on the process's
    // first thread; every other official/third-party launcher passes
    // this unconditionally on macOS rather than relying on it being
    // present in every version JSON.
    if cfg!(target_os = "macos") {
        jvm_args.push("-XstartOnFirstThread".to_string());
    }

    // Drop the launcher defaults the user is replacing. Done here -- on
    // Blocklight's own flags only, before the version JSON's args are
    // added -- so loader-mandated flags (module path, --add-opens, ...)
    // are never touched.
    super::user_args::strip_overridden(&mut jvm_args, user_jvm_args);

    let mut game_args: Vec<String>;

    if let Some(arguments) = &version.arguments {
        jvm_args.extend(expand_entries(&arguments.jvm, &values));
        game_args = expand_entries(&arguments.game, &values);
    } else {
        // Legacy version: no arguments.jvm at all, so supply the
        // standard set every launcher hardcodes for these versions.
        jvm_args.push(format!("-Djava.library.path={}", prepared.natives_dir.to_string_lossy()));
        jvm_args.push("-cp".to_string());
        jvm_args.push(values.get("classpath").cloned().unwrap_or_default());

        let raw = version.minecraft_arguments.clone().unwrap_or_default();
        game_args = raw
            .split_whitespace()
            .map(|tok| substitute(tok, &values))
            .collect();
    }

    // If arguments.jvm didn't already include -cp/classpath (some very
    // old modern-format jsons omit it), make sure it's present.
    if !jvm_args.iter().any(|a| a == "-cp" || a == "-classpath") {
        jvm_args.push("-cp".to_string());
        jvm_args.push(values.get("classpath").cloned().unwrap_or_default());
    }

    let mut full = jvm_args;
    // After everything else, so the JVM's last-one-wins rule favors the
    // user's value wherever a loader flag and a custom flag overlap.
    full.extend(user_jvm_args.iter().cloned());
    full.push(version.main_class.clone());
    full.append(&mut game_args);

    BuiltCommand {
        java_args: full,
        working_dir: game_directory.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_is_stable_and_versioned() {
        let a = offline_uuid("Steve");
        let b = offline_uuid("Steve");
        assert_eq!(a, b);
        // Version nibble (3rd group, 1st hex digit) must be '3'.
        assert_eq!(&a[14..15], "3");
    }

    #[test]
    fn different_names_produce_different_uuids() {
        assert_ne!(offline_uuid("Steve"), offline_uuid("Alex"));
    }
}
