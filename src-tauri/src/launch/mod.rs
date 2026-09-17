pub mod args;
pub mod download;
pub mod java;
pub mod loader_profile;
pub mod manifest;
pub mod neoforge;
pub mod paths;
pub mod process;
pub mod version_json;

use tauri::AppHandle;

use crate::error::AppError;
use crate::models::{Instance, Loader};

use self::java::DetectedJava;
use self::version_json::VersionJson;

/// Identifies one launch profile (Minecraft version + loader + loader
/// version), used to namespace cached natives so different loader
/// combinations for the same Minecraft version never collide.
pub fn profile_key(instance: &Instance) -> String {
    match instance.loader {
        Loader::Minecraft => instance.minecraft_version.clone(),
        other => format!(
            "{}-{}-{}",
            instance.minecraft_version,
            other.label().to_lowercase(),
            instance
                .loader_version
                .clone()
                .unwrap_or_else(|| "latest".to_string())
        ),
    }
}

/// Resolves the version JSON to actually launch: vanilla directly for a
/// loader-less instance, a Fabric/Quilt profile merged with vanilla, or
/// -- for NeoForge -- the real installer/processor pipeline in
/// `neoforge.rs`. Plain Forge deliberately still returns a clear "not
/// implemented" error; see `loader_profile::meta_for`.
///
/// Takes `app`/`instance_id`/`java` (unused by the vanilla/Fabric/Quilt
/// paths) because NeoForge's pipeline needs all three: `app` +
/// `instance_id` to report download/install progress the same way the
/// rest of preparation does, and `java` to actually run NeoForge's
/// installer processors.
pub async fn resolve_version_json(
    client: &reqwest::Client,
    app: &AppHandle,
    instance_id: &str,
    java: &DetectedJava,
    instance: &Instance,
) -> Result<VersionJson, AppError> {
    match instance.loader {
        Loader::Minecraft => {
            manifest::fetch_vanilla_version_json(client, &instance.minecraft_version).await
        }
        Loader::NeoForge => {
            neoforge::resolve_neoforge_profile(
                client,
                app,
                instance_id,
                java,
                &instance.minecraft_version,
                instance.loader_version.as_deref(),
            )
            .await
        }
        loader => {
            loader_profile::resolve_loader_profile(
                client,
                loader,
                &instance.minecraft_version,
                instance.loader_version.as_deref(),
            )
            .await
        }
    }
}
