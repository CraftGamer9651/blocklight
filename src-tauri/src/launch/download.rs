use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

use crate::downloader::safe_extract_zip;
use crate::error::AppError;

use super::paths::{assets_dir, libraries_dir, natives_dir, versions_dir};
use super::version_json::{Library, VersionJson};

/// Every host the launch pipeline will ever fetch from. Nothing here is
/// user-supplied (unlike mod links), but the same "never trust a URL
/// just because it came from a JSON response" posture applies -- an
/// unexpected host in a library/asset entry is refused rather than
/// silently followed.
const ALLOWED_HOSTS: &[&str] = &[
    "launchermeta.mojang.com",
    "piston-meta.mojang.com",
    "piston-data.mojang.com",
    "resources.download.minecraft.net",
    "libraries.minecraft.net",
    "maven.fabricmc.net",
    "maven.quiltmc.org",
    "maven.neoforged.net",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareProgress {
    pub instance_id: String,
    pub stage: String,
    pub completed: u64,
    pub total: u64,
    pub detail: Option<String>,
}

pub(crate) fn emit_progress(app: &AppHandle, instance_id: &str, stage: &str, completed: u64, total: u64, detail: Option<String>) {
    let _ = app.emit(
        "prepare-progress",
        PrepareProgress {
            instance_id: instance_id.to_string(),
            stage: stage.to_string(),
            completed,
            total,
            detail,
        },
    );
}

/// The result of preparing a launch: everything needed to build the JVM
/// command line.
pub struct PreparedLaunch {
    pub classpath: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub assets_root: PathBuf,
    /// Root of the shared Maven-layout library store; some loaders' JVM
    /// args reference it as `${library_directory}`.
    pub library_dir: PathBuf,
}

pub async fn prepare(
    app: &AppHandle,
    client: &reqwest::Client,
    instance_id: &str,
    profile_key: &str,
    version: &VersionJson,
) -> Result<PreparedLaunch, AppError> {
    let client_jar = download_client_jar(app, client, instance_id, version).await?;
    let (mut classpath, natives) = download_libraries(app, client, instance_id, profile_key, version).await?;
    classpath.push(client_jar);
    download_assets(app, client, instance_id, version).await?;

    Ok(PreparedLaunch {
        classpath,
        natives_dir: natives,
        assets_root: assets_dir()?,
        library_dir: libraries_dir()?,
    })
}

pub(crate) async fn download_client_jar(
    app: &AppHandle,
    client: &reqwest::Client,
    instance_id: &str,
    version: &VersionJson,
) -> Result<PathBuf, AppError> {
    emit_progress(app, instance_id, "client", 0, 1, Some(version.id.clone()));

    let entry = version
        .downloads
        .as_ref()
        .and_then(|d| d.client.as_ref())
        .ok_or_else(|| AppError::Internal("version JSON has no client download".into()))?;

    let dest = versions_dir()?.join(&version.id).join(format!("{}.jar", version.id));
    download_verified(client, &entry.url, &dest, entry.sha1.as_deref(), entry.size).await?;

    emit_progress(app, instance_id, "client", 1, 1, None);
    Ok(dest)
}

async fn download_libraries(
    app: &AppHandle,
    client: &reqwest::Client,
    instance_id: &str,
    profile_key: &str,
    version: &VersionJson,
) -> Result<(Vec<PathBuf>, PathBuf), AppError> {
    let natives_out = natives_dir(profile_key)?;
    tokio::fs::create_dir_all(&natives_out).await?;
    let classpath =
        download_library_list(app, client, instance_id, "libraries", &version.libraries, &natives_out).await?;
    Ok((classpath, natives_out))
}

/// How many libraries (or asset objects) to download at once. Libraries
/// run smaller than assets: they're usually a handful of MB each rather
/// than a few KB, so a more modest concurrency avoids saturating a
/// typical connection with too many large transfers at once while still
/// getting the real win, which is not serializing dozens-to-hundreds of
/// small round trips one at a time (a heavily-modded NeoForge instance
/// can easily have 100+ libraries).
const LIBRARY_CONCURRENCY: usize = 12;
/// Assets are numerous (thousands) but tiny (often just a few KB), so
/// the bottleneck is per-request latency rather than bandwidth --
/// pushing concurrency higher here pays off more than it would for
/// libraries.
const ASSET_CONCURRENCY: usize = 32;

/// Downloads every applicable library in `libraries` (filtering by
/// Mojang `rules`), extracting old-style natives into `natives_out`, and
/// returns the resulting classpath entries -- concurrently (bounded by
/// `LIBRARY_CONCURRENCY`), rather than one at a time, which is what
/// used to make preparing a library-heavy (especially modded) instance
/// far slower than it needed to be. Concurrency is order-preserving
/// (`buffered`, not `buffer_unordered`): results come back in the same
/// order `libraries` was given in, so classpath ordering -- which
/// matters when two libraries could provide the same class -- stays
/// exactly as deterministic as the old sequential loop.
///
/// Factored out of `download_libraries` above so NeoForge's
/// installer-library step (`neoforge.rs`) can reuse the exact same
/// download/rules/natives handling for the installer's own library
/// list, rather than a second, easier-to-drift-out-of-sync copy of it.
pub(crate) async fn download_library_list(
    app: &AppHandle,
    client: &reqwest::Client,
    instance_id: &str,
    stage: &str,
    libraries: &[Library],
    natives_out: &Path,
) -> Result<Vec<PathBuf>, AppError> {
    use super::version_json::rules_allow;

    let applicable: Vec<Library> = libraries
        .iter()
        .filter(|l| rules_allow(&l.rules))
        .cloned()
        .collect();
    let total = applicable.len() as u64;
    let libs_dir = libraries_dir()?;
    let completed = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let jobs = applicable.into_iter().map(|lib| {
        let client = client.clone();
        let libs_dir = libs_dir.clone();
        let natives_out = natives_out.to_path_buf();
        let app = app.clone();
        let instance_id = instance_id.to_string();
        let stage = stage.to_string();
        let completed = completed.clone();

        async move {
            let result = download_one_library(&client, &libs_dir, &natives_out, &lib).await;
            let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            emit_progress(&app, &instance_id, &stage, done, total, Some(lib.name.clone()));
            result
        }
    });

    let results: Vec<Result<Option<PathBuf>, AppError>> =
        futures_util::stream::iter(jobs).buffered(LIBRARY_CONCURRENCY).collect().await;

    let mut classpath = Vec::with_capacity(results.len());
    for r in results {
        if let Some(path) = r? {
            classpath.push(path);
        }
    }
    Ok(classpath)
}

/// Downloads one library's main artifact (if any) and, separately, its
/// old-style native classifier jar (if any), extracting the latter
/// rather than adding it to the classpath. Returns the classpath entry
/// for the main artifact, if it has one.
async fn download_one_library(
    client: &reqwest::Client,
    libs_dir: &Path,
    natives_out: &Path,
    lib: &Library,
) -> Result<Option<PathBuf>, AppError> {
    let mut classpath_entry = None;

    // Main artifact: modern Mojang `downloads.artifact`, or the
    // Fabric/Quilt/NeoForge `name` + bare repo `url` shape.
    if let Some(downloads) = &lib.downloads {
        if let Some(artifact) = &downloads.artifact {
            let rel_path = artifact.path.clone().unwrap_or_else(|| maven_path(&lib.name));
            let dest = libs_dir.join(&rel_path);
            download_verified(client, &artifact.url, &dest, artifact.sha1.as_deref(), artifact.size).await?;
            classpath_entry = Some(dest);
        }
    } else if let Some(base_url) = &lib.url {
        let rel_path = maven_path(&lib.name);
        let url = format!("{}/{rel_path}", base_url.trim_end_matches('/'));
        let dest = libs_dir.join(&rel_path);
        download_verified(client, &url, &dest, None, None).await?;
        classpath_entry = Some(dest);
    }

    // Old-style natives: a separate classifier jar that gets extracted
    // (not added to the classpath).
    if let Some(natives_map) = &lib.natives {
        if let Some(classifier_key) = natives_map.get(super::version_json::current_os_name()) {
            let classifier_key = classifier_key.replace("${arch}", "64");
            if let Some(classifiers) = lib.downloads.as_ref().and_then(|d| d.classifiers.as_ref()) {
                if let Some(artifact) = classifiers.get(&classifier_key) {
                    let rel_path = artifact
                        .path
                        .clone()
                        .unwrap_or_else(|| format!("{}-{classifier_key}.jar", maven_path(&lib.name)));
                    let dest = libs_dir.join(&rel_path);
                    download_verified(client, &artifact.url, &dest, artifact.sha1.as_deref(), artifact.size).await?;
                    safe_extract_zip(&dest, natives_out)?;
                }
            }
        }
    }

    Ok(classpath_entry)
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

async fn download_assets(
    app: &AppHandle,
    client: &reqwest::Client,
    instance_id: &str,
    version: &VersionJson,
) -> Result<(), AppError> {
    let Some(asset_index_ref) = &version.asset_index else {
        return Ok(());
    };

    let indexes_dir = assets_dir()?.join("indexes");
    tokio::fs::create_dir_all(&indexes_dir).await?;
    let index_path = indexes_dir.join(format!("{}.json", asset_index_ref.id));

    let body = if index_path.exists() {
        tokio::fs::read_to_string(&index_path).await?
    } else {
        let text = client
            .get(&asset_index_ref.url)
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::from)?
            .text()
            .await?;
        tokio::fs::write(&index_path, &text).await?;
        text
    };

    let index: AssetIndex = serde_json::from_str(&body)
        .map_err(|e| AppError::Internal(format!("couldn't parse asset index: {e}")))?;

    let objects_dir = assets_dir()?.join("objects");
    let total = index.objects.len() as u64;
    let completed = Arc::new(std::sync::atomic::AtomicU64::new(0));

    // Assets are small and there can be thousands of them, so download
    // with bounded concurrency instead of one at a time (or all at once,
    // which would open way too many sockets).
    let semaphore = Arc::new(Semaphore::new(ASSET_CONCURRENCY));
    let mut tasks = FuturesUnordered::new();

    for object in index.objects.into_values() {
        let client = client.clone();
        let objects_dir = objects_dir.clone();
        let semaphore = semaphore.clone();
        let app = app.clone();
        let instance_id = instance_id.to_string();
        let completed = completed.clone();

        tasks.push(tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok();
            let hash = object.hash;
            let prefix = &hash[0..2.min(hash.len())];
            let dest = objects_dir.join(prefix).join(&hash);
            let url = format!("https://resources.download.minecraft.net/{prefix}/{hash}");
            let _ = download_verified(&client, &url, &dest, Some(&hash), Some(object.size)).await;
            let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if done % 50 == 0 || done == 1 {
                emit_progress(&app, &instance_id, "assets", done, total, None);
            }
        }));
    }

    while tasks.next().await.is_some() {}
    emit_progress(app, instance_id, "assets", total, total, None);

    Ok(())
}

/// Downloads `url` to `dest`, skipping the download entirely if a file
/// already sits at `dest` matching `expected_size` (or, lacking a known
/// size, just existing at all) -- a cheap file-metadata check, not a
/// full re-read-and-rehash. Re-hashing every already-downloaded file on
/// every single launch was previously the single biggest source of
/// needless launch-time overhead once an instance was fully prepared,
/// especially across the thousands of small asset files a typical
/// install has: this turns that from "read and hash every file" into
/// "stat every file". A freshly-downloaded file is still fully
/// sha1-verified below, which is what actually catches a corrupted
/// transfer -- that safety property is unchanged, only the repeat-launch
/// fast path is. HTTPS + host-allowlist enforced the same way
/// `downloader.rs` enforces it for mod downloads.
pub(crate) async fn download_verified(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha1: Option<&str>,
    expected_size: Option<u64>,
) -> Result<(), AppError> {
    let parsed = url::Url::parse(url)?;
    if parsed.scheme() != "https" {
        return Err(AppError::InvalidLink("refusing a non-HTTPS launch asset URL".into()));
    }
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    if !ALLOWED_HOSTS.iter().any(|h| host == *h) {
        return Err(AppError::InvalidLink(format!(
            "refusing to download launch assets from untrusted host: {host}"
        )));
    }

    if dest.exists() {
        match expected_size {
            Some(size) => {
                if let Ok(meta) = tokio::fs::metadata(dest).await {
                    if meta.len() == size {
                        return Ok(());
                    }
                    // Size mismatch (partial/corrupt/stale file) -- fall
                    // through and re-download rather than trusting it.
                }
            }
            None => return Ok(()),
        }
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .bytes()
        .await?;

    if let Some(expected) = expected_sha1 {
        let actual = hex::encode(Sha1::digest(&bytes));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(AppError::IntegrityCheckFailed {
                file: url.to_string(),
            });
        }
    }

    let tmp = dest.with_extension("part");
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, dest).await?;
    Ok(())
}

/// Converts a Maven coordinate (`group:artifact:version` or
/// `group:artifact:version:classifier`) into its standard repository
/// path, used for the Fabric/Quilt library shape that gives a bare name
/// + repo URL instead of a ready-made path.
pub(crate) fn maven_path(coordinate: &str) -> String {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return coordinate.replace(':', "/");
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3);

    match classifier {
        Some(c) => format!("{group}/{artifact}/{version}/{artifact}-{version}-{c}.jar"),
        None => format!("{group}/{artifact}/{version}/{artifact}-{version}.jar"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_maven_path() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.9"),
            "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
        );
    }

    #[test]
    fn builds_maven_path_with_classifier() {
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-linux"),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
        );
    }
}
