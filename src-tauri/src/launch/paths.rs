use std::path::PathBuf;

use crate::db::app_data_dir;
use crate::error::AppError;

/// Vanilla client jars and version JSONs and shared across every
/// instance that uses that Minecraft version (`versions/<id>/<id>.jar`),
/// same convention the official launcher and every third-party one uses,
/// so switching instances never re-downloads the same jar twice.
pub fn versions_dir() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("versions"))
}

/// Library jars in Maven layout (`libraries/<group>/<artifact>/<version>/...`),
/// shared across every instance and Minecraft version.
pub fn libraries_dir() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("libraries"))
}

/// Asset objects (`assets/objects/<xx>/<hash>`) and indexes
/// (`assets/indexes/<name>.json`), shared across every instance.
pub fn assets_dir() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("assets"))
}

/// Extracted native libraries (.so/.dylib/.dll) for one specific launch
/// profile. Kept per-profile rather than shared, since which natives are
/// needed depends on the exact library set (which differs slightly
/// between loaders even for the same Minecraft version).
pub fn natives_dir(profile_key: &str) -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("natives").join(profile_key))
}

/// Scratch space for NeoForge's installer run: the downloaded installer
/// jar, its extracted `install_profile.json`/`version.json`/data files,
/// and a cached merged version JSON so the (slow) processor chain only
/// ever runs once per Minecraft+NeoForge version pair.
pub fn neoforge_work_dir(minecraft_version: &str, neoforge_version: &str) -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?
        .join("neoforge")
        .join(format!("{minecraft_version}-{neoforge_version}")))
}
