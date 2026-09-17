//! NeoForge launching, using the same real installer/processor mechanism
//! every legitimate NeoForge-aware launcher uses -- there is no
//! shortcut here; NeoForge's client jar is genuinely produced by running
//! NeoForge's own installer tooling as a chain of subprocesses.
//!
//! This is scoped to NeoForge specifically, not Forge in general.
//! NeoForge only exists from Minecraft 1.20.1 onward (it's a fork of
//! Forge from that point), and has only ever used this modern,
//! processor-based installer format. Older Forge additionally has to
//! support a much older single-step binary-jar-patching format for
//! pre-1.13 versions, which this module does not attempt.
//!
//! This is, by a wide margin, the least-tested code in Blocklight: it
//! was written from documented knowledge of the installer format, not
//! verified against a live install, because this sandbox cannot run a
//! JVM or a real NeoForge installer. Treat any failure here as
//! informative rather than surprising -- the error message names
//! exactly which processor step failed and why.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::AppHandle;

use crate::error::AppError;

use super::download::{download_client_jar, download_library_list, download_verified, emit_progress};
use super::java::DetectedJava;
use super::loader_profile::merge_with_vanilla;
use super::manifest::fetch_vanilla_version_json;
use super::paths::{libraries_dir, neoforge_work_dir};
use super::version_json::{Library, VersionJson};

const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net";

/// Downloads and runs whatever's needed to produce a launchable NeoForge
/// profile, then merges it with vanilla. Cached: once the processor
/// chain succeeds for a given Minecraft+NeoForge version pair, later
/// calls just re-read the merged result from disk.
pub async fn resolve_neoforge_profile(
    client: &reqwest::Client,
    app: &AppHandle,
    instance_id: &str,
    java: &DetectedJava,
    minecraft_version: &str,
    requested_version: Option<&str>,
) -> Result<VersionJson, AppError> {
    let neoforge_version = match requested_version {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => resolve_latest_neoforge_version(client, minecraft_version).await?,
    };

    let work_dir = neoforge_work_dir(minecraft_version, &neoforge_version)?;
    tokio::fs::create_dir_all(&work_dir).await?;
    let merged_path = work_dir.join("merged_version.json");

    if let Ok(raw) = tokio::fs::read_to_string(&merged_path).await {
        if let Ok(parsed) = serde_json::from_str::<VersionJson>(&raw) {
            return Ok(parsed);
        }
    }

    let installer_jar = work_dir.join("installer.jar");
    let installer_url = format!(
        "{NEOFORGE_MAVEN}/releases/net/neoforged/neoforge/{neoforge_version}/neoforge-{neoforge_version}-installer.jar"
    );
    download_verified(client, &installer_url, &installer_jar, None).await?;

    let profile: InstallProfile = {
        let bytes = read_zip_entry(&installer_jar, "install_profile.json")?;
        serde_json::from_slice(&bytes)
            .map_err(|e| AppError::Internal(format!("couldn't parse install_profile.json: {e}")))?
    };

    let raw_version_path = extract_zip_entry_to(&installer_jar, &profile.json, &work_dir)?;
    let child_version: VersionJson = {
        let text = tokio::fs::read_to_string(&raw_version_path).await?;
        serde_json::from_str(&text)
            .map_err(|e| AppError::Internal(format!("couldn't parse NeoForge version.json: {e}")))?
    };

    let vanilla = fetch_vanilla_version_json(client, minecraft_version).await?;

    let profile_key = format!("{minecraft_version}-neoforge-{neoforge_version}");
    let natives_out = super::paths::natives_dir(&profile_key)?;
    tokio::fs::create_dir_all(&natives_out).await?;
    download_library_list(
        app,
        client,
        instance_id,
        "neoforge_libraries",
        &profile.libraries,
        &natives_out,
    )
    .await?;
    let vanilla_client_jar = download_client_jar(app, client, instance_id, &vanilla).await?;

    run_processors(
        app,
        client,
        instance_id,
        java,
        &installer_jar,
        &work_dir,
        &profile,
        &vanilla_client_jar,
        minecraft_version,
        &vanilla,
    )
    .await?;

    let merged = merge_with_vanilla(child_version, vanilla);

    if let Ok(json) = serde_json::to_string(&merged) {
        let _ = tokio::fs::write(&merged_path, json).await;
    }

    Ok(merged)
}

// ---------------------------------------------------------------------
// Version resolution
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct NeoForgeVersionList {
    versions: Vec<String>,
}

async fn resolve_latest_neoforge_version(
    client: &reqwest::Client,
    minecraft_version: &str,
) -> Result<String, AppError> {
    let prefix = mc_version_to_neoforge_prefix(minecraft_version)?;

    let url = format!("{NEOFORGE_MAVEN}/api/maven/versions/releases/net/neoforged/neoforge");
    let list: NeoForgeVersionList = client
        .get(&url)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .json()
        .await?;

    list.versions
        .into_iter()
        .filter(|v| v.starts_with(&prefix))
        .max_by(|a, b| compare_version_strings(a, b))
        .ok_or_else(|| {
            AppError::Internal(format!(
                "no NeoForge build is published for Minecraft {minecraft_version}"
            ))
        })
}

/// NeoForge's own version numbering mirrors the Minecraft version it
/// targets: Minecraft `1.X.Y` -> NeoForge versions start with `X.Y.`.
/// A two-part Minecraft version like `1.X` normalizes to `X.0.`
/// (NeoForge version numbers always have three components).
fn mc_version_to_neoforge_prefix(minecraft_version: &str) -> Result<String, AppError> {
    let without_leading_one = minecraft_version.strip_prefix("1.").ok_or_else(|| {
        AppError::Internal(format!(
            "{minecraft_version} doesn't look like a Minecraft version NeoForge supports (expected 1.x or 1.x.y)"
        ))
    })?;

    let parts: Vec<&str> = without_leading_one.split('.').collect();
    let normalized = match parts.first() {
        Some(major) if parts.len() >= 2 => format!("{major}.{}", parts[1]),
        Some(major) => format!("{major}.0"),
        None => {
            return Err(AppError::Internal(format!(
                "{minecraft_version} doesn't look like a Minecraft version NeoForge supports"
            )))
        }
    };
    Ok(format!("{normalized}."))
}

fn compare_version_strings(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    parts(a).cmp(&parts(b))
}

// ---------------------------------------------------------------------
// install_profile.json
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct InstallProfile {
    /// Path *inside the installer jar* to the launch-profile JSON to
    /// extract and merge with vanilla, e.g. `"/version.json"`.
    json: String,
    #[serde(default)]
    libraries: Vec<Library>,
    #[serde(default)]
    processors: Vec<ProcessorSpec>,
    #[serde(default)]
    data: HashMap<String, DataEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct DataEntry {
    client: String,
}

#[derive(Debug, Deserialize)]
struct ProcessorSpec {
    /// The processor's own jar, as a bracketed Maven coordinate, e.g.
    /// `"[net.neoforged.installertools:installertools:x.y.z:fatjar]"`.
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    /// Absent or empty means "runs for both sides"; Blocklight only
    /// ever runs the client side.
    #[serde(default)]
    sides: Vec<String>,
}

// ---------------------------------------------------------------------
// Running the processor chain
// ---------------------------------------------------------------------

/// Runs every client-side processor in order, exactly as NeoForge's own
/// installer would: each one is a small Java program (already
/// downloaded as part of `profile.libraries`) invoked with substituted
/// arguments. A failure here means NeoForge's own tooling failed, not
/// Blocklight -- the error names which processor and includes its
/// output so the underlying cause is visible.
async fn run_processors(
    app: &AppHandle,
    client: &reqwest::Client,
    instance_id: &str,
    java: &DetectedJava,
    installer_jar: &Path,
    work_dir: &Path,
    profile: &InstallProfile,
    vanilla_client_jar: &Path,
    minecraft_version: &str,
    vanilla: &VersionJson,
) -> Result<(), AppError> {
    let mut subst: HashMap<String, String> = HashMap::new();
    subst.insert("SIDE".to_string(), "client".to_string());
    subst.insert("ROOT".to_string(), work_dir.to_string_lossy().to_string());
    subst.insert(
        "MINECRAFT_JAR".to_string(),
        vanilla_client_jar.to_string_lossy().to_string(),
    );
    subst.insert("MINECRAFT_VERSION".to_string(), minecraft_version.to_string());
    subst.insert("INSTALLER".to_string(), installer_jar.to_string_lossy().to_string());
    subst.insert("LIBRARY_DIR".to_string(), libraries_dir()?.to_string_lossy().to_string());

    for (key, entry) in &profile.data {
        let resolved = resolve_data_value(&entry.client, work_dir, installer_jar, &profile.libraries, client, vanilla).await?;
        subst.insert(key.clone(), resolved);
    }

    let applicable: Vec<&ProcessorSpec> = profile
        .processors
        .iter()
        .filter(|p| p.sides.is_empty() || p.sides.iter().any(|s| s == "client"))
        .collect();
    let total = applicable.len() as u64;

    for (index, processor) in applicable.iter().enumerate() {
        emit_progress(
            app,
            instance_id,
            "neoforge_install",
            index as u64,
            total,
            Some(processor.jar.clone()),
        );

        let jar_path = PathBuf::from(substitute_token(&processor.jar, &subst, &profile.libraries, client, vanilla).await?);

        let mut classpath = vec![jar_path.clone()];
        for entry in &processor.classpath {
            classpath.push(PathBuf::from(
                substitute_token(entry, &subst, &profile.libraries, client, vanilla).await?,
            ));
        }

        let main_class = find_main_class(&jar_path)?;

        let mut args = Vec::with_capacity(processor.args.len());
        for raw in &processor.args {
            args.push(substitute_token(raw, &subst, &profile.libraries, client, vanilla).await?);
        }

        let sep = if cfg!(windows) { ";" } else { ":" };
        let classpath_string = classpath
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(sep);

        let output = tokio::process::Command::new(&java.executable)
            .arg("-cp")
            .arg(&classpath_string)
            .arg(&main_class)
            .args(&args)
            .output()
            .await?;

        if !output.status.success() {
            return Err(AppError::Internal(format!(
                "NeoForge install step failed ({main_class}, exit {:?}):\n{}\n{}",
                output.status.code(),
                tail_lines(&String::from_utf8_lossy(&output.stdout), 20),
                tail_lines(&String::from_utf8_lossy(&output.stderr), 20),
            )));
        }
    }

    emit_progress(app, instance_id, "neoforge_install", total, total, None);
    Ok(())
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

// ---------------------------------------------------------------------
// Token substitution: {DATA_KEY} and bracketed [maven:coords]
// ---------------------------------------------------------------------

async fn substitute_token(
    raw: &str,
    subst: &HashMap<String, String>,
    libraries: &[Library],
    client: &reqwest::Client,
    vanilla: &VersionJson,
) -> Result<String, AppError> {
    if raw.starts_with('[') && raw.ends_with(']') {
        return Ok(resolve_bracket(raw, libraries, client, vanilla)
            .await?
            .to_string_lossy()
            .to_string());
    }
    let mut out = raw.to_string();
    for (key, value) in subst {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    Ok(out)
}

/// A data-map entry's value is one of: a bracketed Maven coordinate
/// (resolved to an already-downloaded library's path), a path starting
/// with `/` (extracted from inside the installer jar into `work_dir`),
/// or a plain literal string used as-is.
async fn resolve_data_value(
    raw: &str,
    work_dir: &Path,
    installer_jar: &Path,
    libraries: &[Library],
    client: &reqwest::Client,
    vanilla: &VersionJson,
) -> Result<String, AppError> {
    if raw.starts_with('[') && raw.ends_with(']') {
        Ok(resolve_bracket(raw, libraries, client, vanilla)
            .await?
            .to_string_lossy()
            .to_string())
    } else if raw.starts_with('/') {
        let dest = extract_zip_entry_to(installer_jar, raw, &work_dir.join("data"))?;
        Ok(dest.to_string_lossy().to_string())
    } else {
        Ok(raw.to_string())
    }
}

/// Some coordinates NeoForge's data map references aren't real Maven
/// artifacts at all: `net.minecraft:client[:...]:mappings` and
/// `net.minecraft:server[:...]:mappings` mean "Mojang's official
/// obfuscation mappings", and a bare `net.minecraft:client`/`:server`
/// (no classifier) means "the vanilla client/server jar itself". Mojang
/// only publishes these via the vanilla version JSON's own `downloads`
/// block, never through a Maven repository -- every real Forge/NeoForge
/// installer special-cases exactly this, so this does too.
fn special_case_url(coord: &str, vanilla: &VersionJson) -> Option<String> {
    let parts: Vec<&str> = coord.split(':').collect();
    if parts.first() != Some(&"net.minecraft") {
        return None;
    }
    let downloads = vanilla.downloads.as_ref()?;
    let is_mappings = parts.get(3) == Some(&"mappings");
    match (parts.get(1), is_mappings) {
        (Some(&"client"), true) => downloads.client_mappings.as_ref().map(|d| d.url.clone()),
        (Some(&"client"), false) => downloads.client.as_ref().map(|d| d.url.clone()),
        (Some(&"server"), true) => downloads.server_mappings.as_ref().map(|d| d.url.clone()),
        (Some(&"server"), false) => downloads.server.as_ref().map(|d| d.url.clone()),
        _ => None,
    }
}

/// `[group:artifact:version[:classifier][@ext]]` -> the file path that
/// coordinate resolves to. Checks `special_case_url` first (Mojang's
/// own mappings/jars, which aren't real Maven artifacts -- see above),
/// then prefers the exact path/URL a matching entry in `libraries`
/// declares (a handful use a non-standard Maven layout or host), then
/// falls back to the standard computed layout.
///
/// If the resolved path doesn't already exist on disk -- which can
/// happen for a coordinate the bulk library-download pass skipped (an
/// OS `rules` gate that doesn't apply to why a *processor* needs it) or
/// one that was never in `libraries` at all -- this fetches it on
/// demand rather than failing outright.
async fn resolve_bracket(
    raw: &str,
    libraries: &[Library],
    client: &reqwest::Client,
    vanilla: &VersionJson,
) -> Result<PathBuf, AppError> {
    let inner = raw.trim_start_matches('[').trim_end_matches(']');
    let (coord, ext) = match inner.split_once('@') {
        Some((c, e)) => (c, e),
        None => (inner, "jar"),
    };

    let matching = libraries.iter().find(|l| l.name == coord);

    let (path, download_url): (PathBuf, Option<String>) = if let Some(url) = special_case_url(coord, vanilla) {
        (libraries_dir()?.join(maven_coord_path(coord, ext)), Some(url))
    } else if let Some(lib) = matching {
        if let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            let rel = artifact.path.clone().unwrap_or_else(|| maven_coord_path(coord, ext));
            (libraries_dir()?.join(&rel), Some(artifact.url.clone()))
        } else if let Some(base_url) = &lib.url {
            let rel = maven_coord_path(coord, ext);
            let url = format!("{}/{rel}", base_url.trim_end_matches('/'));
            (libraries_dir()?.join(&rel), Some(url))
        } else {
            (libraries_dir()?.join(maven_coord_path(coord, ext)), None)
        }
    } else {
        (libraries_dir()?.join(maven_coord_path(coord, ext)), None)
    };

    if path.exists() {
        return Ok(path);
    }

    let url = download_url
        .unwrap_or_else(|| format!("{NEOFORGE_MAVEN}/releases/{}", maven_coord_path(coord, ext)));

    download_verified(client, &url, &path, None).await.map_err(|_| {
        AppError::Internal(format!(
            "NeoForge's installer needs {coord}, which isn't downloaded and couldn't be fetched \
             from {url} either. This usually means the install_profile format for this NeoForge \
             version differs from what this build expects."
        ))
    })?;

    Ok(path)
}

fn maven_coord_path(coordinate: &str, ext: &str) -> String {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return format!("{}.{ext}", coordinate.replace(':', "/"));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    match parts.get(3) {
        Some(classifier) => format!("{group}/{artifact}/{version}/{artifact}-{version}-{classifier}.{ext}"),
        None => format!("{group}/{artifact}/{version}/{artifact}-{version}.{ext}"),
    }
}

// ---------------------------------------------------------------------
// Reading inside jars/zips
// ---------------------------------------------------------------------

fn read_zip_entry(archive_path: &Path, entry_name: &str) -> Result<Vec<u8>, AppError> {
    let file = std::fs::File::open(archive_path).map_err(|e| {
        AppError::Internal(format!(
            "couldn't open {}: {e} (expected to find it there after downloading/extracting it earlier)",
            archive_path.display()
        ))
    })?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| AppError::Internal(format!("corrupt jar {}: {e}", archive_path.display())))?;
    let name = entry_name.trim_start_matches('/');
    let mut entry = archive
        .by_name(name)
        .map_err(|e| AppError::Internal(format!("{} is missing {name}: {e}", archive_path.display())))?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

fn extract_zip_entry_to(archive_path: &Path, entry_ref: &str, dest_dir: &Path) -> Result<PathBuf, AppError> {
    let name = entry_ref.trim_start_matches('/');
    let bytes = read_zip_entry(archive_path, name)?;
    let dest = dest_dir.join(name);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&dest, &bytes)?;
    Ok(dest)
}

/// A processor jar's `Main-Class` is read from its own manifest rather
/// than assumed, since it varies per tool. Doesn't handle the (rare in
/// practice, for a class name) MANIFEST.MF 72-column continuation-line
/// wrapping the jar spec technically allows.
fn find_main_class(jar_path: &Path) -> Result<String, AppError> {
    let bytes = read_zip_entry(jar_path, "META-INF/MANIFEST.MF")?;
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Main-Class:") {
            return Ok(value.trim().to_string());
        }
    }
    Err(AppError::Internal(format!(
        "couldn't find Main-Class in {}'s manifest",
        jar_path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_prefix_for_patch_version() {
        assert_eq!(mc_version_to_neoforge_prefix("1.21.1").unwrap(), "21.1.");
    }

    #[test]
    fn builds_prefix_for_bare_minor_version() {
        assert_eq!(mc_version_to_neoforge_prefix("1.21").unwrap(), "21.0.");
    }

    #[test]
    fn prefix_does_not_false_match_similar_minor() {
        let prefix = mc_version_to_neoforge_prefix("1.21.1").unwrap();
        assert!(!"21.10.5".starts_with(&prefix));
        assert!("21.1.100".starts_with(&prefix));
    }

    #[test]
    fn maven_bracket_resolves_with_default_ext() {
        assert_eq!(
            maven_coord_path("net.neoforged:neoforge:21.1.100", "jar"),
            "net/neoforged/neoforge/21.1.100/neoforge-21.1.100.jar"
        );
    }

    #[test]
    fn maven_bracket_resolves_with_custom_ext_and_classifier() {
        assert_eq!(
            maven_coord_path("net.neoforged:neoform:1.21.1-x:mappings", "txt"),
            "net/neoforged/neoform/1.21.1-x/neoform-1.21.1-x-mappings.txt"
        );
    }
}
