pub mod curseforge;
pub mod modrinth;

use async_trait::async_trait;

use crate::error::AppError;
use crate::models::{Platform, Project, ProjectVersion};

/// Common interface every external content platform must implement. The
/// rest of the app (URL resolver, compatibility engine, dependency
/// resolver, downloader, unified search) only ever talks to this trait,
/// which is what lets a third provider be added later without touching
/// the launcher's core logic.
#[async_trait]
pub trait ContentProvider: Send + Sync {
    /// Which platform this provider implements. Part of the trait's
    /// contract even though current callers already know the platform
    /// from the URL they resolved -- useful for anything that holds a
    /// `&dyn ContentProvider` without that context (tests, future
    /// providers, generic tooling).
    #[allow(dead_code)]
    fn platform(&self) -> Platform;

    /// Look up a project by its platform-specific slug or id.
    async fn get_project(&self, project_ref: &str) -> Result<Project, AppError>;

    /// All available versions/files for a project, newest first.
    async fn list_versions(&self, project: &Project) -> Result<Vec<ProjectVersion>, AppError>;

    /// A single specific version, when the URL identified one exactly.
    async fn get_version(
        &self,
        project: &Project,
        version_ref: &str,
    ) -> Result<ProjectVersion, AppError>;

    /// Free-text search against the platform, used by the unified
    /// search + link import experience.
    async fn search(&self, query: &str, limit: u32) -> Result<Vec<Project>, AppError>;
}

/// Holds one provider per supported platform. Commands ask the registry
/// for "whoever handles Modrinth" rather than constructing clients
/// themselves.
pub struct ProviderRegistry {
    modrinth: modrinth::ModrinthProvider,
    curseforge: curseforge::CurseForgeProvider,
}

impl ProviderRegistry {
    pub fn new(curseforge_api_key: Option<String>) -> Self {
        ProviderRegistry {
            modrinth: modrinth::ModrinthProvider::new(),
            curseforge: curseforge::CurseForgeProvider::new(curseforge_api_key),
        }
    }

    pub fn for_platform(&self, platform: Platform) -> &dyn ContentProvider {
        match platform {
            Platform::Modrinth => &self.modrinth,
            Platform::CurseForge => &self.curseforge,
        }
    }

    /// Takes `&self`, not `&mut self`: the key lives behind
    /// `CurseForgeProvider`'s own interior `RwLock`, which lets this be
    /// called through a plain shared reference to `AppState` rather than
    /// requiring an outer lock around the whole registry -- an outer lock
    /// would have to be a `std::sync::RwLock`, and its read guard isn't
    /// `Send`, which breaks any async command that holds a provider
    /// reference across an `.await`.
    pub fn set_curseforge_api_key(&self, key: Option<String>) {
        self.curseforge.set_api_key(key);
    }

    pub fn has_curseforge_api_key(&self) -> bool {
        self.curseforge.has_api_key()
    }

    /// Direct access to the Modrinth client for calls that aren't part
    /// of the shared `ContentProvider` trait (e.g. the "top downloaded"
    /// browse list, which is Modrinth-specific by design rather than a
    /// generic cross-platform capability).
    pub fn modrinth(&self) -> &modrinth::ModrinthProvider {
        &self.modrinth
    }
}
