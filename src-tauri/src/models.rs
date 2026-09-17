use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Modrinth,
    CurseForge,
}

impl Platform {
    /// Human-readable label. Not currently called from the command layer
    /// (the frontend derives its own copy in `lib/format.ts`), but kept
    /// as the canonical Rust-side label for logging, error messages, and
    /// any future non-UI consumer (CLI output, tests).
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Platform::Modrinth => "Modrinth",
            Platform::CurseForge => "CurseForge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Mod,
    ResourcePack,
    ShaderPack,
    Modpack,
}

impl ContentType {
    /// See the note on `Platform::label` -- same reasoning.
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            ContentType::Mod => "Mod",
            ContentType::ResourcePack => "Resource Pack",
            ContentType::ShaderPack => "Shader Pack",
            ContentType::Modpack => "Modpack",
        }
    }

    /// The instance subfolder this content type installs into.
    pub fn install_subdir(&self) -> &'static str {
        match self {
            ContentType::Mod => "mods",
            ContentType::ResourcePack => "resourcepacks",
            ContentType::ShaderPack => "shaderpacks",
            ContentType::Modpack => ".",
        }
    }
}

/// The mod loader an instance runs, or `Minecraft` for loader-less content
/// such as resource packs, where Modrinth reports the loader as `"minecraft"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Fabric,
    Forge,
    NeoForge,
    Quilt,
    Minecraft,
}

impl Loader {
    pub fn parse(raw: &str) -> Loader {
        match raw.to_ascii_lowercase().as_str() {
            "fabric" => Loader::Fabric,
            "forge" => Loader::Forge,
            "neoforge" => Loader::NeoForge,
            "quilt" => Loader::Quilt,
            _ => Loader::Minecraft,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Loader::Fabric => "Fabric",
            Loader::Forge => "Forge",
            Loader::NeoForge => "NeoForge",
            Loader::Quilt => "Quilt",
            Loader::Minecraft => "Minecraft",
        }
    }
}

/// A Minecraft instance managed by Blocklight. This is the compatibility
/// target that link installs are checked against.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub minecraft_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub java_major_version: Option<u32>,
    pub instance_dir: String,
    /// Slugs/ids of content already installed in this instance, keyed by
    /// platform, used for "already installed" detection.
    pub installed: Vec<InstalledContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledContent {
    pub platform: Platform,
    pub project_id: String,
    pub project_slug: String,
    pub content_type: ContentType,
    pub version_id: String,
    pub version_number: String,
    pub file_name: String,
}

/// A project (mod / resource pack / shader pack / modpack) on a provider,
/// normalized to a shape both `ModrinthProvider` and `CurseForgeProvider`
/// produce.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub platform: Platform,
    pub id: String,
    pub slug: String,
    pub name: String,
    pub author: String,
    pub summary: String,
    pub icon_url: Option<String>,
    pub content_type: ContentType,
    pub project_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHashes {
    pub sha1: Option<String>,
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub filename: String,
    pub download_url: String,
    pub size_bytes: u64,
    pub hashes: FileHashes,
    pub primary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub project_id: String,
    pub version_id: Option<String>,
    pub project_name: Option<String>,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVersion {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub dependencies: Vec<Dependency>,
    pub files: Vec<ProjectFile>,
    pub date_published: String,
}

impl ProjectVersion {
    pub fn primary_file(&self) -> Option<&ProjectFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }

    pub fn supports_loader(&self, loader: Loader) -> bool {
        if self.loaders.is_empty() {
            // Some resource-pack/shader entries omit loaders entirely.
            return matches!(loader, Loader::Minecraft);
        }
        self.loaders
            .iter()
            .any(|l| Loader::parse(l) == loader || l.eq_ignore_ascii_case("minecraft"))
    }

    pub fn supports_game_version(&self, mc_version: &str) -> bool {
        self.game_versions.iter().any(|v| v == mc_version)
    }
}

/// Result of matching an instance against a project's available versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityResult {
    pub compatible: bool,
    pub selected_version: Option<ProjectVersion>,
    pub reason: String,
    pub available_alternatives: Vec<VersionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub minecraft_version: String,
    pub loader: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyCheck {
    pub dependency: Dependency,
    pub resolved_project: Option<Project>,
    pub resolved_version: Option<ProjectVersion>,
    pub already_installed: bool,
    pub compatible: bool,
}

/// The full "what will be installed" package the frontend renders as the
/// link preview card and confirms before download.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedInstall {
    pub project: Project,
    pub instance_id: String,
    pub compatibility: CompatibilityResult,
    pub already_installed: Option<InstalledContent>,
    pub dependencies: Vec<DependencyCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallHistoryEntry {
    pub id: String,
    pub project_name: String,
    pub platform: Platform,
    pub content_type: ContentType,
    pub version_number: String,
    pub instance_id: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineProfile {
    pub id: String,
    pub display_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveAccountKind {
    Microsoft,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountState {
    pub active_kind: ActiveAccountKind,
    pub microsoft_signed_in: bool,
    pub microsoft_gamertag: Option<String>,
    pub offline_profile: Option<OfflineProfile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectivityState {
    Online,
    Offline,
}
