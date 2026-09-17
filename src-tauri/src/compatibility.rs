use std::collections::BTreeSet;

use crate::models::{CompatibilityResult, Instance, ProjectVersion, VersionSummary};

/// Steps 6-7 of the spec's URL Resolution flow: compare available files
/// against the current instance and pick the most appropriate one.
/// Never falls back to an incompatible version -- if nothing matches,
/// `compatible` is false and the caller must not install anything.
pub fn evaluate(instance: &Instance, versions: &[ProjectVersion]) -> CompatibilityResult {
    let mut candidates: Vec<&ProjectVersion> = versions
        .iter()
        .filter(|v| {
            v.supports_game_version(&instance.minecraft_version) && v.supports_loader(instance.loader)
        })
        .collect();

    // Newest first: Modrinth/CurseForge both return versions newest-first,
    // but sort defensively by date_published so callers can't be tripped
    // up by provider ordering changes.
    candidates.sort_by(|a, b| b.date_published.cmp(&a.date_published));

    if let Some(best) = candidates.first() {
        return CompatibilityResult {
            compatible: true,
            selected_version: Some((*best).clone()),
            reason: format!(
                "Matches your instance: Minecraft {} on {}.",
                instance.minecraft_version,
                instance.loader.label()
            ),
            available_alternatives: Vec::new(),
        };
    }

    // Build a de-duplicated summary of what *is* available, so the "no
    // compatible version" state can show real alternatives instead of a
    // dead end.
    let mut seen = BTreeSet::new();
    let mut alternatives = Vec::new();
    for v in versions {
        for gv in &v.game_versions {
            let loaders: Vec<&String> = if v.loaders.is_empty() {
                vec![]
            } else {
                v.loaders.iter().collect()
            };
            if loaders.is_empty() {
                let key = (gv.clone(), "minecraft".to_string());
                if seen.insert(key.clone()) {
                    alternatives.push(VersionSummary {
                        minecraft_version: gv.clone(),
                        loader: "Minecraft".to_string(),
                    });
                }
            } else {
                for loader in loaders {
                    let key = (gv.clone(), loader.to_ascii_lowercase());
                    if seen.insert(key) {
                        alternatives.push(VersionSummary {
                            minecraft_version: gv.clone(),
                            loader: capitalize(loader),
                        });
                    }
                }
            }
        }
    }
    // Cap it: this is a summary for a small "Available:" list, not a full
    // version table.
    alternatives.truncate(6);

    CompatibilityResult {
        compatible: false,
        selected_version: None,
        reason: format!(
            "This project does not currently provide a version compatible with Minecraft {} on {}.",
            instance.minecraft_version,
            instance.loader.label()
        ),
        available_alternatives: alternatives,
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FileHashes, Loader, Platform, ProjectFile};

    fn instance() -> Instance {
        Instance {
            id: "inst-1".into(),
            name: "Modded Survival".into(),
            minecraft_version: "1.21.1".into(),
            loader: Loader::Fabric,
            loader_version: None,
            java_major_version: Some(21),
            instance_dir: "/tmp/inst-1".into(),
            installed: vec![],
        }
    }

    fn version(id: &str, mc: &str, loader: &str, date: &str) -> ProjectVersion {
        ProjectVersion {
            id: id.into(),
            project_id: "proj".into(),
            version_number: format!("v-{id}"),
            game_versions: vec![mc.into()],
            loaders: vec![loader.into()],
            dependencies: vec![],
            files: vec![ProjectFile {
                filename: "file.jar".into(),
                download_url: "https://example.invalid/file.jar".into(),
                size_bytes: 1,
                hashes: FileHashes {
                    sha1: None,
                    sha512: None,
                },
                primary: true,
            }],
            date_published: date.into(),
        }
    }

    #[test]
    fn picks_matching_version_over_others() {
        let versions = vec![
            version("v1", "1.20.1", "forge", "2024-01-01T00:00:00Z"),
            version("v2", "1.21.1", "fabric", "2024-06-01T00:00:00Z"),
            version("v3", "1.21.1", "neoforge", "2024-05-01T00:00:00Z"),
        ];
        let result = evaluate(&instance(), &versions);
        assert!(result.compatible);
        assert_eq!(result.selected_version.unwrap().id, "v2");
    }

    #[test]
    fn reports_incompatible_with_alternatives() {
        let versions = vec![
            version("v1", "1.20.1", "forge", "2024-01-01T00:00:00Z"),
            version("v3", "1.21.1", "neoforge", "2024-05-01T00:00:00Z"),
        ];
        let result = evaluate(&instance(), &versions);
        assert!(!result.compatible);
        assert!(result.selected_version.is_none());
        assert_eq!(result.available_alternatives.len(), 2);
    }

    #[test]
    fn ignores_platform_unused_field() {
        // Platform isn't consulted by the matcher; this just documents that
        // compatibility is loader/version based, independent of provider.
        let _ = Platform::Modrinth;
    }
}
