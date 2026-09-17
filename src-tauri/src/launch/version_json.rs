use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Mojang's per-version JSON. This is also exactly the shape Fabric and
/// Quilt's meta "profile/json" endpoints return (see `loader_profile.rs`),
/// which is what makes merging them with the vanilla JSON straightforward
/// -- both sides are this same type. Only the fields Blocklight's launch
/// pipeline actually reads are modeled; anything else in the real JSON is
/// simply ignored by serde.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    #[serde(rename = "assetIndex")]
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersionRef>,
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "type")]
    pub version_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Downloads {
    pub client: Option<DownloadEntry>,
    /// Mojang's official obfuscation mappings for the client. Not
    /// something you'd normally need directly -- Blocklight only reads
    /// this to satisfy NeoForge installer references to it (see
    /// `launch::neoforge::special_case_url`), since Mojang only
    /// publishes these here, not through any Maven repository.
    pub client_mappings: Option<DownloadEntry>,
    pub server: Option<DownloadEntry>,
    pub server_mappings: Option<DownloadEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DownloadEntry {
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JavaVersionRef {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

/// A single dependency. Two shapes exist in the wild and both are
/// modeled here as optional fields on one struct:
///   - Modern Mojang format: `name` + `downloads.artifact` (+ optionally
///     `downloads.classifiers` for old-style natives).
///   - Fabric/Quilt format: just `name` + a bare `url` (a Maven repo
///     base), with the path derived from `name` -- see `download.rs`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Library {
    pub name: String,
    pub url: Option<String>,
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Old-style natives declaration: maps an OS name to a classifier
    /// key in `downloads.classifiers` (e.g. `{"linux": "natives-linux"}`).
    pub natives: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Artifact {
    pub path: Option<String>,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    pub action: String, // "allow" | "disallow"
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OsRule {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Value>,
    #[serde(default)]
    pub jvm: Vec<Value>,
}

/// Current platform's Mojang-style OS name ("windows" / "osx" / "linux").
pub fn current_os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        _ => "linux",
    }
}

/// Evaluates a Mojang `rules` array against the current platform. No
/// rules means always included. Otherwise the **last matching rule
/// wins**, exactly like the official launcher: start disallowed, and
/// each rule whose conditions match overwrites the running answer.
///
/// Blocklight doesn't support demo mode, custom resolution, or
/// quick-play, so every `features` condition is evaluated as if all of
/// those features are permanently off.
pub fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let os_matches = rule
            .os
            .as_ref()
            .and_then(|os| os.name.as_deref())
            .map(|name| name == current_os_name())
            .unwrap_or(true);

        let features_match = rule
            .features
            .as_ref()
            .map(|f| f.values().all(|required| !*required))
            .unwrap_or(true);

        if os_matches && features_match {
            allowed = rule.action == "allow";
        }
    }
    allowed
}
