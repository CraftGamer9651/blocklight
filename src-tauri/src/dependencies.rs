use crate::compatibility;
use crate::error::AppError;
use crate::models::{Dependency, DependencyCheck, DependencyKind, Instance};
use crate::providers::ProviderRegistry;

/// Resolves every required/optional dependency of a version into a full
/// `DependencyCheck`: what project it is, which version would be
/// installed, whether it's already present, and whether it's compatible.
/// Incompatible/embedded dependencies are skipped -- "incompatible" is a
/// conflict warning rather than something to install, and "embedded"
/// dependencies already ship inside the main file.
pub async fn resolve_all(
    registry: &ProviderRegistry,
    instance: &Instance,
    dependencies: &[Dependency],
) -> Vec<DependencyCheck> {
    let mut out = Vec::new();

    for dep in dependencies {
        if matches!(dep.kind, DependencyKind::Incompatible | DependencyKind::Embedded) {
            continue;
        }
        if dep.project_id.is_empty() {
            continue;
        }

        out.push(resolve_one(registry, instance, dep).await);
    }

    out
}

async fn resolve_one(
    registry: &ProviderRegistry,
    instance: &Instance,
    dep: &Dependency,
) -> DependencyCheck {
    // A dependency belongs to the same platform as the version that
    // declared it; the caller passes dependencies already scoped to one
    // provider, so we try both providers is unnecessary -- the registry
    // call below is keyed by whichever platform originally supplied
    // `dep`. We infer that from context by trying Modrinth first only
    // when the id looks like a Modrinth base62 id, else CurseForge. In
    // practice `resolve_all` is always called once per provider per
    // install, so platform is threaded in by the caller via `dep` itself
    // being platform-homogeneous.
    let platform = infer_platform(&dep.project_id);
    let provider = registry.for_platform(platform);

    let already_installed = instance
        .installed
        .iter()
        .any(|i| i.project_id == dep.project_id);

    let project = match provider.get_project(&dep.project_id).await {
        Ok(p) => p,
        Err(_) => {
            return DependencyCheck {
                dependency: dep.clone(),
                resolved_project: None,
                resolved_version: None,
                already_installed,
                compatible: false,
            }
        }
    };

    let versions = provider.list_versions(&project).await.unwrap_or_default();
    let compat = compatibility::evaluate(instance, &versions);

    DependencyCheck {
        dependency: dep.clone(),
        resolved_project: Some(project),
        resolved_version: compat.selected_version.clone(),
        already_installed,
        compatible: compat.compatible,
    }
}

/// Modrinth ids are base62 and always exactly 8 characters; CurseForge ids
/// are purely numeric. This is enough to disambiguate without threading an
/// explicit platform through every dependency record.
fn infer_platform(id: &str) -> crate::models::Platform {
    if id.chars().all(|c| c.is_ascii_digit()) {
        crate::models::Platform::CurseForge
    } else {
        crate::models::Platform::Modrinth
    }
}

pub fn all_compatible(checks: &[DependencyCheck]) -> Result<(), AppError> {
    if checks.iter().any(|c| !c.already_installed && !c.compatible) {
        return Err(AppError::NoCompatibleVersion);
    }
    Ok(())
}
