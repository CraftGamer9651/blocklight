//! Version lists for the New Instance dialog: Minecraft versions, loader
//! builds, and the Java version Mojang recommends. Read-only lookups;
//! nothing here touches an instance.

use serde::Deserialize;
use tauri::State;

use crate::commands::AppState;
use crate::error::AppError;
use crate::launch::loader_profile::{self, LoaderVersionOption};
use crate::launch::manifest::{self, ManifestVersion};
use crate::launch::neoforge;
use crate::models::Loader;

const FORGE_PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";

#[tauri::command]
pub async fn list_minecraft_versions(
    state: State<'_, AppState>,
) -> Result<Vec<ManifestVersion>, AppError> {
    manifest::list_versions(&state.http).await
}

/// Loader builds available for a Minecraft version. An empty list means
/// "none published" (the dialog then falls back to "Latest", resolved at
/// launch); an error means the lookup itself failed.
#[tauri::command]
pub async fn list_loader_versions(
    state: State<'_, AppState>,
    loader: String,
    minecraft_version: String,
) -> Result<Vec<LoaderVersionOption>, AppError> {
    if !state.network.is_online() {
        return Err(AppError::Offline);
    }
    match loader.to_ascii_lowercase().as_str() {
        "fabric" => {
            loader_profile::list_loader_versions(&state.http, Loader::Fabric, &minecraft_version)
                .await
        }
        "quilt" => {
            loader_profile::list_loader_versions(&state.http, Loader::Quilt, &minecraft_version)
                .await
        }
        "neoforge" => neoforge::list_neoforge_versions(&state.http, &minecraft_version).await,
        "forge" => Ok(list_forge_versions(&state.http, &minecraft_version).await),
        _ => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn recommended_java(
    state: State<'_, AppState>,
    minecraft_version: String,
) -> Result<Option<u32>, AppError> {
    manifest::recommended_java(&state.http, &minecraft_version).await
}

#[derive(Debug, Deserialize)]
struct ForgePromotions {
    promos: std::collections::HashMap<String, String>,
}

/// Forge only publishes a "recommended" and a "latest" build per
/// Minecraft version in its promotions file. Best effort: any failure
/// yields an empty list rather than an error, since Forge launching isn't
/// implemented and this only feeds a dropdown.
async fn list_forge_versions(
    client: &reqwest::Client,
    minecraft_version: &str,
) -> Vec<LoaderVersionOption> {
    let Ok(resp) = client.get(FORGE_PROMOTIONS).send().await else {
        return Vec::new();
    };
    let Ok(resp) = resp.error_for_status() else {
        return Vec::new();
    };
    let Ok(promos) = resp.json::<ForgePromotions>().await else {
        return Vec::new();
    };

    let mut out: Vec<LoaderVersionOption> = Vec::new();
    for suffix in ["latest", "recommended"] {
        if let Some(version) = promos.promos.get(&format!("{minecraft_version}-{suffix}")) {
            if !out.iter().any(|o| &o.version == version) {
                out.push(LoaderVersionOption {
                    version: version.clone(),
                    stable: suffix == "recommended",
                });
            }
        }
    }
    out
}
