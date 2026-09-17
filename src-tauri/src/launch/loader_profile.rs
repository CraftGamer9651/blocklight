use serde::Deserialize;

use crate::error::AppError;
use crate::models::Loader;

use super::manifest::fetch_vanilla_version_json;
use super::version_json::VersionJson;

struct LoaderMeta {
    /// e.g. "https://meta.fabricmc.net" -- the profile endpoint is always
    /// `{base}{list_prefix}/{mc}` and `{base}{list_prefix}/{mc}/{build}/profile/json`.
    base: &'static str,
    list_prefix: &'static str,
}

fn meta_for(loader: Loader) -> Result<LoaderMeta, AppError> {
    match loader {
        Loader::Fabric => Ok(LoaderMeta {
            base: "https://meta.fabricmc.net",
            list_prefix: "/v2/versions/loader",
        }),
        Loader::Quilt => Ok(LoaderMeta {
            base: "https://meta.quiltmc.org",
            list_prefix: "/v3/versions/loader",
        }),
        Loader::Forge => Err(AppError::Internal(
            "Forge launching isn't implemented -- only NeoForge is, since Forge (pre-1.13 in \
             particular) has installer quirks NeoForge's simpler, consistently-modern installer \
             format doesn't. NeoForge and Fabric/Quilt instances can launch normally."
                .into(),
        )),
        Loader::NeoForge => Err(AppError::Internal(
            "internal error: NeoForge should be resolved via neoforge::resolve_neoforge_profile, \
             not this Fabric/Quilt-oriented path"
                .into(),
        )),
        Loader::Minecraft => Err(AppError::Internal(
            "this instance has no mod loader selected -- use vanilla launching instead".into(),
        )),
    }
}

#[derive(Debug, Deserialize)]
struct LoaderVersionEntry {
    loader: LoaderBuildInfo,
}

#[derive(Debug, Deserialize)]
struct LoaderBuildInfo {
    version: String,
    #[serde(default)]
    stable: bool,
}

async fn resolve_loader_version(
    client: &reqwest::Client,
    meta: &LoaderMeta,
    minecraft_version: &str,
) -> Result<String, AppError> {
    let url = format!("{}{}/{}", meta.base, meta.list_prefix, minecraft_version);
    let entries: Vec<LoaderVersionEntry> = client
        .get(&url)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .json()
        .await?;

    entries
        .iter()
        .find(|e| e.loader.stable)
        .or_else(|| entries.first())
        .map(|e| e.loader.version.clone())
        .ok_or_else(|| {
            AppError::Internal(format!(
                "no loader build is published for Minecraft {minecraft_version}"
            ))
        })
}

/// Fetches the Fabric/Quilt profile JSON, then merges it with the
/// vanilla version JSON it `inheritsFrom`: the loader's `mainClass` and
/// libraries take priority, everything else needed to actually run the
/// game (asset index, client jar, Java version) comes from vanilla.
pub async fn resolve_loader_profile(
    client: &reqwest::Client,
    loader: Loader,
    minecraft_version: &str,
    loader_version: Option<&str>,
) -> Result<VersionJson, AppError> {
    let meta = meta_for(loader)?;

    let resolved_version = match loader_version {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => resolve_loader_version(client, &meta, minecraft_version).await?,
    };

    let profile_url = format!(
        "{}{}/{}/{}/profile/json",
        meta.base, meta.list_prefix, minecraft_version, resolved_version
    );
    let child: VersionJson = client
        .get(&profile_url)
        .send()
        .await?
        .error_for_status()
        .map_err(AppError::from)?
        .json()
        .await?;

    let parent = fetch_vanilla_version_json(client, minecraft_version).await?;
    Ok(merge_with_vanilla(child, parent))
}

/// Merges a loader's own version JSON (Fabric/Quilt's `profile/json`
/// response, or NeoForge's extracted `version.json`) with vanilla's:
/// the loader's `mainClass` and libraries take priority, everything
/// else needed to actually run the game (asset index, client download,
/// Java version) comes from vanilla. Shared by `resolve_loader_profile`
/// above and NeoForge's installer pipeline (`neoforge.rs`), since both
/// produce a "child" profile that inherits the rest from vanilla the
/// same way.
pub(crate) fn merge_with_vanilla(child: VersionJson, parent: VersionJson) -> VersionJson {
    VersionJson {
        id: child.id,
        main_class: child.main_class,
        inherits_from: None,
        asset_index: parent.asset_index,
        assets: parent.assets,
        downloads: parent.downloads,
        libraries: {
            let mut libs = child.libraries;
            libs.extend(parent.libraries);
            libs
        },
        java_version: parent.java_version,
        // The child's own arguments only ever add a handful of extra
        // game args; vanilla's jvm args (memory, library path,
        // classpath placeholder) still need to apply, so jvm comes from
        // the parent and game entries are combined.
        arguments: merge_arguments(child.arguments, parent.arguments),
        minecraft_arguments: parent.minecraft_arguments,
        version_type: parent.version_type,
    }
}

fn merge_arguments(
    child: Option<super::version_json::Arguments>,
    parent: Option<super::version_json::Arguments>,
) -> Option<super::version_json::Arguments> {
    let mut game = parent.as_ref().map(|a| a.game.clone()).unwrap_or_default();
    if let Some(c) = &child {
        game.extend(c.game.clone());
    }
    let jvm = parent.map(|a| a.jvm).unwrap_or_default();
    Some(super::version_json::Arguments { game, jvm })
}
