use std::io::Read;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use sha1::{Digest, Sha1};
use sha2::Sha512;
use url::Url;

use crate::error::AppError;
use crate::models::{ContentType, ProjectFile};

/// Every host Blocklight will ever download a file from. Anything else is
/// refused outright, regardless of what a provider response claims --
/// this is the "restrict supported domains" requirement, enforced at the
/// point of download rather than trusted from upstream JSON.
const ALLOWED_HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "objects.githubusercontent.com",
    "edge.forgecdn.net",
    "media.forgecdn.net",
    "mediafilez.forgecdn.net",
];

/// Downloads one file into the correct subfolder of an instance,
/// verifying its integrity and refusing anything that looks unsafe.
/// Returns the final path on disk. Never executes the downloaded content.
pub async fn download_file(
    client: &reqwest::Client,
    file: &ProjectFile,
    instance_dir: &Path,
    content_type: ContentType,
) -> Result<PathBuf, AppError> {
    let url = validate_download_url(&file.download_url)?;
    let filename = validate_filename(&file.filename)?;

    let dest_dir = safe_join(instance_dir, content_type.install_subdir())?;
    tokio::fs::create_dir_all(&dest_dir).await?;
    let dest_path = safe_join(&dest_dir, &filename)?;

    let tmp_path = dest_path.with_extension(format!(
        "{}.part",
        dest_path.extension().and_then(|e| e.to_str()).unwrap_or("download")
    ));

    let resp = client
        .get(url)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?;

    let mut sha1_hasher = Sha1::new();
    let mut sha512_hasher = Sha512::new();
    let mut out = tokio::fs::File::create(&tmp_path).await?;

    let mut stream = resp.bytes_stream();
    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        sha1_hasher.update(&chunk);
        sha512_hasher.update(&chunk);
        out.write_all(&chunk).await?;
    }
    out.flush().await?;
    drop(out);

    verify_hash(&file.filename, &file.hashes, &sha1_hasher, &sha512_hasher, &tmp_path)?;

    tokio::fs::rename(&tmp_path, &dest_path).await?;
    Ok(dest_path)
}

fn validate_download_url(raw: &str) -> Result<Url, AppError> {
    let url = Url::parse(raw)?;
    if url.scheme() != "https" {
        return Err(AppError::InvalidLink(
            "refusing to download over a non-HTTPS connection".into(),
        ));
    }
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if !ALLOWED_HOSTS.iter().any(|h| host == *h) {
        return Err(AppError::InvalidLink(format!(
            "refusing to download from untrusted host: {host}"
        )));
    }
    Ok(url)
}

/// Rejects anything that isn't a bare filename: no path separators, no
/// leading dot (hidden/relative), no `..`, no null bytes, no empty string.
fn validate_filename(name: &str) -> Result<String, AppError> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
        || name == "."
        || name == ".."
        || name.starts_with('.')
    {
        return Err(AppError::PathTraversal(name.to_string()));
    }
    Ok(name.to_string())
}

/// Joins `child` onto `base` and verifies the resulting path is still
/// inside `base` once normalized -- the core defense against a filename
/// like `../../../../etc/whatever` ever escaping the instance directory.
fn safe_join(base: &Path, child: &str) -> Result<PathBuf, AppError> {
    let candidate = base.join(child);
    let normalized = normalize(&candidate);
    let normalized_base = normalize(base);
    if !normalized.starts_with(&normalized_base) {
        return Err(AppError::PathTraversal(child.to_string()));
    }
    Ok(candidate)
}

/// Lexical normalization (no filesystem access needed, so it works even
/// before the destination file exists) that resolves `.` and `..`
/// segments so `safe_join` can reliably compare paths.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn verify_hash(
    filename: &str,
    hashes: &crate::models::FileHashes,
    sha1_hasher: &Sha1,
    sha512_hasher: &Sha512,
    tmp_path: &Path,
) -> Result<(), AppError> {
    if let Some(expected) = &hashes.sha512 {
        let actual = hex::encode(sha512_hasher.clone().finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(tmp_path);
            return Err(AppError::IntegrityCheckFailed {
                file: filename.to_string(),
            });
        }
        return Ok(());
    }
    if let Some(expected) = &hashes.sha1 {
        let actual = hex::encode(sha1_hasher.clone().finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(tmp_path);
            return Err(AppError::IntegrityCheckFailed {
                file: filename.to_string(),
            });
        }
        return Ok(());
    }
    // No hash supplied by the provider (rare) -- proceed, since HTTPS +
    // an allow-listed host already establish transport integrity, but
    // this branch exists explicitly rather than silently.
    Ok(())
}

/// Safely extracts a zip archive into `dest_dir`, rejecting any entry
/// whose normalized path would land outside `dest_dir`. Used for two
/// things: old-style native-library jars during launch preparation
/// (`launch::download`), and it's also the primitive a future modpack
/// installer (parsing a `.mrpack` index, extracting `overrides/`) would
/// reuse -- that feature itself isn't wired up yet, but the safe
/// extraction it would need already is.
pub fn safe_extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<(), AppError> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Internal(format!("corrupt archive: {e}")))?;

    std::fs::create_dir_all(dest_dir)?;
    let normalized_base = normalize(dest_dir);

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::Internal(format!("corrupt archive entry: {e}")))?;

        let Some(entry_path) = entry.enclosed_name() else {
            // `enclosed_name()` already refuses absolute paths and `..`
            // components; anything it rejects, we skip rather than trust.
            continue;
        };

        let dest_path = dest_dir.join(&entry_path);
        let normalized = normalize(&dest_path);
        if !normalized.starts_with(&normalized_base) {
            return Err(AppError::PathTraversal(entry_path.display().to_string()));
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out = std::fs::File::create(&dest_path)?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        std::io::Write::write_all(&mut out, &buf)?;

        // Never carry over executable permission bits from the archive,
        // even on platforms where zip stores unix mode bits.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest_path, std::fs::Permissions::from_mode(0o644));
        }
    }

    Ok(())
}
