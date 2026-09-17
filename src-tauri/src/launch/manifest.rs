use serde::Deserialize;

use crate::error::AppError;

use super::paths::versions_dir;
use super::version_json::VersionJson;

const MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Deserialize)]
struct RawManifest {
    versions: Vec<RawManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct RawManifestEntry {
    id: String,
    url: String,
}

/// Looks up a Minecraft version's metadata URL in Mojang's manifest.
/// Always fetched fresh (it's a small file and versions get added over
/// time), unlike the per-version JSON below which is cached once found.
async fn find_version_url(client: &reqwest::Client, minecraft_version: &str) -> Result<String, AppError> {
    let manifest: RawManifest = client
        .get(MANIFEST_URL)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .json()
        .await?;

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
