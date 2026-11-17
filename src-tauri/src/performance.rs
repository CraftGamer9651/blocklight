//! Performance Pack: a one-click set of well-known optimization mods.
//!
//! Blocklight doesn't ship its own game client. Minecraft's rendering
//! and tick-loop speedups come from mods that patch the game (Sodium for
//! rendering, Lithium for game logic, and so on), so the pack installs
//! those from Modrinth through the same download pipeline link installs
//! use: compatibility check, required-dependency resolution, hash-verified
//! download, install record, history entry.
//!
//! Each mod is installed independently. One without a build for the
//! instance's Minecraft version is reported as skipped and the rest still
//! install, because new Minecraft versions routinely land before every
//! optimization mod catches up.

use serde::Serialize;
use tauri::State;

use crate::commands::AppState;
use crate::compatibility;
use crate::dependencies;
use crate::downloader;
use crate::error::AppError;
use crate::history;
use crate::models::{DependencyKind, Instance, InstalledContent, Loader, Platform};

pub struct PackMod {
    /// Modrinth project slug.
    pub slug: &'static str,
    pub label: &'static str,
    pub purpose: &'static str,
}

const SODIUM: PackMod = PackMod {
    slug: "sodium",
    label: "Sodium",
    purpose: "Rewrites the renderer: much higher FPS and smoother frame times",
};
const EMBEDDIUM: PackMod = PackMod {
    slug: "embeddium",
    label: "Embeddium",
    purpose: "Sodium port for Forge: much higher FPS and smoother frame times",
};
const LITHIUM: PackMod = PackMod {
    slug: "lithium",
    label: "Lithium",
    purpose: "Optimizes game logic, physics and chunk handling",
};
const FERRITECORE: PackMod = PackMod {
    slug: "ferrite-core",
    label: "FerriteCore",
    purpose: "Cuts memory use, which helps heavy modpacks",
};
const MODERNFIX: PackMod = PackMod {
    slug: "modernfix",
    label: "ModernFix",
    purpose: "Faster startup and world loading, lower memory use",
};
const IMMEDIATELYFAST: PackMod = PackMod {
    slug: "immediatelyfast",
    label: "ImmediatelyFast",
    purpose: "Speeds up UI, text and entity batching",
};
const ENTITYCULLING: PackMod = PackMod {
    slug: "entityculling",
    label: "Entity Culling",
    purpose: "Skips rendering entities you can't see",
};

const FABRIC_PACK: &[PackMod] = &[SODIUM, LITHIUM, FERRITECORE, MODERNFIX, IMMEDIATELYFAST, ENTITYCULLING];
// Lithium has no official NeoForge build, so it's left out here.
const NEOFORGE_PACK: &[PackMod] = &[SODIUM, FERRITECORE, MODERNFIX, IMMEDIATELYFAST, ENTITYCULLING];
const FORGE_PACK: &[PackMod] = &[EMBEDDIUM, FERRITECORE, MODERNFIX, IMMEDIATELYFAST, ENTITYCULLING];

/// `None` for vanilla instances: nothing can be injected without a loader.
pub fn preset_for(loader: Loader) -> Option<&'static [PackMod]> {
    match loader {
        Loader::Fabric | Loader::Quilt => Some(FABRIC_PACK),
        Loader::NeoForge => Some(NEOFORGE_PACK),
        Loader::Forge => Some(FORGE_PACK),
        Loader::Minecraft => None,
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '-' | '_' | '+' | ' ' | '.'))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// True if the mod is already in the instance: either Blocklight
/// installed it, or a hand-added jar's file name starts with the mod's
/// name followed by a version or loader tag. That second rule keeps
/// `sodium-fabric-0.6.jar` matching `sodium` while `sodium-extra-...`
/// and `reeses-sodium-options-...` do not.
pub fn is_present(instance: &Instance, slug: &str) -> bool {
    if instance.installed.iter().any(|c| c.project_slug == slug) {
        return true;
    }
    let mods_dir = std::path::Path::new(&instance.instance_dir).join("mods");
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return false;
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .any(|name| file_name_matches(&name, slug))
}

fn file_name_matches(file_name: &str, slug: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    let Some(stem) = lower.strip_suffix(".jar") else {
        return false;
    };
    let stem = normalize(stem);
    let Some(rest) = stem.strip_prefix(&normalize(slug)) else {
        return false;
    };
    rest.is_empty()
        || rest.starts_with(|c: char| c.is_ascii_digit())
        || ["fabric", "neoforge", "forge", "quilt", "mc"]
            .iter()
            .any(|tag| rest.starts_with(tag))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackEntry {
    pub slug: String,
    pub label: String,
    pub purpose: String,
    pub already_installed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSkip {
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackResult {
    pub installed: Vec<String>,
    pub skipped: Vec<PackSkip>,
    pub instance: Instance,
}

/// What the pack would install for this instance, and what's already there.
/// Local-only, so it works offline.
#[tauri::command]
pub fn performance_pack_info(
    state: State<AppState>,
    instance_id: String,
) -> Result<Vec<PackEntry>, AppError> {
    let instance = state.instances.get(&instance_id)?;
    let preset = preset_for(instance.loader).ok_or_else(vanilla_error)?;
    Ok(preset
        .iter()
        .map(|m| PackEntry {
            slug: m.slug.to_string(),
            label: m.label.to_string(),
            purpose: m.purpose.to_string(),
            already_installed: is_present(&instance, m.slug),
        })
        .collect())
}

#[tauri::command]
pub async fn install_performance_pack(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<PackResult, AppError> {
    if !state.network.is_online() {
        return Err(AppError::Offline);
    }

    let first = state.instances.get(&instance_id)?;
    let preset = preset_for(first.loader).ok_or_else(vanilla_error)?;

    let mut installed = Vec::new();
    let mut skipped = Vec::new();

    for m in preset {
        // Re-read each time so dependencies installed for an earlier mod
        // are seen as already present.
        let instance = state.instances.get(&instance_id)?;
        if is_present(&instance, m.slug) {
            skipped.push(PackSkip {
                label: m.label.to_string(),
                reason: "Already installed".to_string(),
            });
            continue;
        }

        match install_one(&state, &instance, m.slug).await {
            Ok(mut names) => installed.append(&mut names),
            Err(AppError::NoCompatibleVersion) => skipped.push(PackSkip {
                label: m.label.to_string(),
                reason: format!(
                    "No build for Minecraft {} on {} yet",
                    instance.minecraft_version,
                    instance.loader.label()
                ),
            }),
            Err(e) => skipped.push(PackSkip {
                label: m.label.to_string(),
                reason: e.to_string(),
            }),
        }
    }

    Ok(PackResult {
        installed,
        skipped,
        instance: state.instances.get(&instance_id)?,
    })
}

fn vanilla_error() -> AppError {
    AppError::Internal(
        "The Performance Pack needs a modded instance (Fabric, Quilt, NeoForge or Forge). \
         Vanilla Minecraft can't load mods."
            .to_string(),
    )
}

/// Installs one Modrinth project plus its *required* dependencies.
/// Optional dependencies are deliberately not pulled in: a performance
/// pack shouldn't quietly add extra mods. Returns the display names of
/// everything newly installed.
async fn install_one(
    state: &AppState,
    instance: &Instance,
    slug: &str,
) -> Result<Vec<String>, AppError> {
    // Quilt loads Fabric mods, and Modrinth lists these under "fabric".
    let mut probe = instance.clone();
    if probe.loader == Loader::Quilt {
        probe.loader = Loader::Fabric;
    }

    let provider = state.providers.for_platform(Platform::Modrinth);
    let project = provider.get_project(slug).await?;
    let versions = provider.list_versions(&project).await?;
    let compat = compatibility::evaluate(&probe, &versions);
    let version = compat.selected_version.ok_or(AppError::NoCompatibleVersion)?;

    let required: Vec<_> = version
        .dependencies
        .iter()
        .filter(|d| d.kind == DependencyKind::Required)
        .cloned()
        .collect();
    let dep_checks = dependencies::resolve_all(&state.providers, &probe, &required).await;
    if dep_checks.iter().any(|c| !c.already_installed && !c.compatible) {
        return Err(AppError::Internal(
            "a required dependency has no compatible version".to_string(),
        ));
    }

    let instance_dir = std::path::PathBuf::from(&instance.instance_dir);
    let mut names = Vec::new();

    for check in dep_checks.iter().filter(|c| !c.already_installed) {
        let (Some(dep_project), Some(dep_version)) =
            (&check.resolved_project, &check.resolved_version)
        else {
            continue;
        };
        let Some(file) = dep_version.primary_file() else {
            continue;
        };
        downloader::download_file(&state.http, file, &instance_dir, dep_project.content_type)
            .await?;
        record(state, &instance.id, dep_project, dep_version, &file.filename)?;
        names.push(dep_project.name.clone());
    }

    let file = version
        .primary_file()
        .ok_or_else(|| AppError::Internal("selected version has no downloadable file".into()))?;
    downloader::download_file(&state.http, file, &instance_dir, project.content_type).await?;
    record(state, &instance.id, &project, &version, &file.filename)?;
    names.push(project.name.clone());

    Ok(names)
}

fn record(
    state: &AppState,
    instance_id: &str,
    project: &crate::models::Project,
    version: &crate::models::ProjectVersion,
    file_name: &str,
) -> Result<(), AppError> {
    state.instances.record_installed(
        instance_id,
        InstalledContent {
            platform: project.platform,
            project_id: project.id.clone(),
            project_slug: project.slug.clone(),
            content_type: project.content_type,
            version_id: version.id.clone(),
            version_number: version.version_number.clone(),
            file_name: file_name.to_string(),
        },
    )?;
    let conn = state.db.0.lock().unwrap();
    history::record(
        &conn,
        &project.name,
        project.platform,
        project.content_type,
        &version.version_number,
        instance_id,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_real_jar_names() {
        assert!(file_name_matches("sodium-fabric-0.6.13+mc1.21.1.jar", "sodium"));
        assert!(file_name_matches("sodium-neoforge-0.6.13.jar", "sodium"));
        assert!(file_name_matches("ferritecore-7.0.0-hotfix-fabric.jar", "ferrite-core"));
        assert!(file_name_matches("lithium-fabric-0.14.3.jar", "lithium"));
        assert!(file_name_matches("Sodium.jar", "sodium"));
    }

    #[test]
    fn ignores_addons_with_similar_names() {
        assert!(!file_name_matches("sodium-extra-0.6.0.jar", "sodium"));
        assert!(!file_name_matches("reeses-sodium-options-1.8.jar", "sodium"));
        assert!(!file_name_matches("sodium-fabric.txt", "sodium"));
    }
}
