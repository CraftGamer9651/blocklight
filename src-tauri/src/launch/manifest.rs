use std::time::Duration;

use serde::Deserialize;

use crate::error::AppError;

use super::paths::versions_dir;
use super::version_json::VersionJson;

const MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

/// How long a cached copy of the manifest is trusted before refetching.
/// The manifest changes rarely (new Minecraft versions, not every day),
/// so paying for a fresh download of it on literally every launch --
/// as opposed to the per-version JSON, which is genuinely immutable
/// once published and cached indefinitely below -- was pure overhead.
/// If a version isn't found in a cached-but-still-fresh copy, that's
/// handled as a cache miss too (see `find_version_url`), so a
/// just-released version is never blocked on this window.
const MANIFEST_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Deserialize)]
struct RawManifest {
    versions: Vec<RawManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct RawManifestEntry {
    id: String,
    url: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(rename = "releaseTime", default)]
    release_time: String,
}

/// One row of Mojang's version list, for the New Instance dropdown.
/// `kind` is Mojang's own label: "release", "snapshot", "old_beta", "old_alpha".
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestVersion {
    pub id: String,
    pub kind: String,
    pub release_time: String,
}

async fn fetch_manifest_body(client: &reqwest::Client) -> Result<String, AppError> {
    let body = client
        .get(MANIFEST_URL)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .text()
        .await?;
    Ok(body)
}

/// Reads the cached manifest. `max_age` of `None` accepts a stale copy
/// (used as the offline fallback).
async fn read_cached_manifest(max_age: Option<Duration>) -> Option<String> {
    let path = manifest_cache_path().ok()?;
    if let Some(max_age) = max_age {
        let modified = tokio::fs::metadata(&path).await.ok()?.modified().ok()?;
        let age = std::time::SystemTime::now().duration_since(modified).ok()?;
        if age > max_age {
            return None;
        }
    }
    tokio::fs::read_to_string(&path).await.ok()
}

fn to_manifest_versions(manifest: RawManifest) -> Vec<ManifestVersion> {
    manifest
        .versions
        .into_iter()
        .map(|v| ManifestVersion {
            id: v.id,
            kind: v.kind,
            release_time: v.release_time,
        })
        .collect()
}

/// Every Minecraft version Mojang lists, newest first. Uses the same
/// cached manifest as launching; if the network is down it falls back to
/// a stale cached copy, so the dropdown still works offline once the
/// manifest has been fetched at least once.
pub async fn list_versions(client: &reqwest::Client) -> Result<Vec<ManifestVersion>, AppError> {
    if let Some(text) = read_cached_manifest(Some(MANIFEST_CACHE_TTL)).await {
        if let Ok(manifest) = serde_json::from_str::<RawManifest>(&text) {
            return Ok(to_manifest_versions(manifest));
        }
    }

    match fetch_manifest_body(client).await {
        Ok(body) => {
            if let Ok(path) = manifest_cache_path() {
                if let Some(parent) = path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                let _ = tokio::fs::write(&path, &body).await;
            }
            let manifest: RawManifest = serde_json::from_str(&body)
                .map_err(|e| AppError::Internal(format!("couldn't parse version manifest: {e}")))?;
            Ok(to_manifest_versions(manifest))
        }
        Err(err) => {
            if let Some(text) = read_cached_manifest(None).await {
                if let Ok(manifest) = serde_json::from_str::<RawManifest>(&text) {
                    return Ok(to_manifest_versions(manifest));
                }
            }
            Err(err)
        }
    }
}

/// The Java major version Mojang says this Minecraft version needs, or
/// `None` for old versions whose metadata doesn't say.
pub async fn recommended_java(
    client: &reqwest::Client,
    minecraft_version: &str,
) -> Result<Option<u32>, AppError> {
    let json = fetch_vanilla_version_json(client, minecraft_version).await?;
    Ok(json.java_version.map(|j| j.major_version))
}

fn manifest_cache_path() -> Result<std::path::PathBuf, AppError> {
    Ok(versions_dir()?.join("version_manifest_v2.json"))
}

/// Looks up a Minecraft version's metadata URL in Mojang's manifest,
/// using a locally-cached copy when it's both present and fresh enough,
/// and containing the requested version.
async fn find_version_url(client: &reqwest::Client, minecraft_version: &str) -> Result<String, AppError> {
    if let Some(url) = cached_version_url(minecraft_version).await {
        return Ok(url);
    }

    let body = client
        .get(MANIFEST_URL)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .text()
        .await?;

    // Best-effort cache write: a failure here shouldn't fail the launch
    // that's already in progress, just mean the next one re-fetches too.
    if let Ok(path) = manifest_cache_path() {
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&path, &body).await;
    }

    let manifest: RawManifest = serde_json::from_str(&body)
        .map_err(|e| AppError::Internal(format!("couldn't parse version manifest: {e}")))?;

    manifest
        .versions
        .into_iter()
        .find(|v| v.id == minecraft_version)
        .map(|v| v.url)
        .ok_or_else(|| {
            AppError::Internal(format!(
                "Minecraft {minecraft_version} isn't a version Mojang currently distributes"
            ))
        })
}

/// Returns `Some(url)` only if a cached manifest exists, is within the
/// TTL, and actually lists the requested version -- any other outcome
/// (missing, stale, corrupt, version not listed) is treated uniformly
/// as a cache miss so the caller just falls back to a live fetch.
async fn cached_version_url(minecraft_version: &str) -> Option<String> {
    let path = manifest_cache_path().ok()?;
    let metadata = tokio::fs::metadata(&path).await.ok()?;
    let modified = metadata.modified().ok()?;
    let age = std::time::SystemTime::now().duration_since(modified).ok()?;
    if age > MANIFEST_CACHE_TTL {
        return None;
    }

    let text = tokio::fs::read_to_string(&path).await.ok()?;
    let manifest: RawManifest = serde_json::from_str(&text).ok()?;
    manifest
        .versions
        .into_iter()
        .find(|v| v.id == minecraft_version)
        .map(|v| v.url)
}

/// Fetches (and locally caches under `versions/<id>/<id>.json`) the
/// vanilla version JSON for a Minecraft version.
pub async fn fetch_vanilla_version_json(
    client: &reqwest::Client,
    minecraft_version: &str,
) -> Result<VersionJson, AppError> {
    let cache_path = versions_dir()?
        .join(minecraft_version)
        .join(format!("{minecraft_version}.json"));

    if let Ok(raw) = tokio::fs::read_to_string(&cache_path).await {
        if let Ok(parsed) = serde_json::from_str::<VersionJson>(&raw) {
            return Ok(parsed);
        }
        // Fall through and re-fetch if the cached file is somehow corrupt.
    }

    let url = find_version_url(client, minecraft_version).await?;
    let body = client
        .get(&url)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .text()
        .await?;

    if let Some(parent) = cache_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&cache_path, &body).await?;

    serde_json::from_str(&body)
        .map_err(|e| AppError::Internal(format!("couldn't parse version JSON: {e}")))
}
