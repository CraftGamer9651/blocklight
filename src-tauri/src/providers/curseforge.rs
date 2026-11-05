use async_trait::async_trait;
use serde::Deserialize;
use std::sync::RwLock;

use crate::error::AppError;
use crate::models::{
    ContentType, Dependency, DependencyKind, FileHashes, Platform, Project, ProjectFile,
    ProjectVersion,
};

use super::ContentProvider;

const API_BASE: &str = "https://api.curseforge.com/v1";
const MINECRAFT_GAME_ID: u32 = 432;

// CurseForge classIds for Minecraft content. These are stable, widely
// used constants (the same ones tools like ferium/packwiz rely on) --
// CurseForge's API is id-based, not slug-based, which is why a slug must
// be resolved through a filtered search before anything else can happen.
const CLASS_MODS: u32 = 6;
const CLASS_RESOURCE_PACKS: u32 = 12;
const CLASS_MODPACKS: u32 = 4471;
const CLASS_SHADERS: u32 = 6552;

const KNOWN_LOADERS: [&str; 4] = ["Forge", "Fabric", "Quilt", "NeoForge"];

pub struct CurseForgeProvider {
    client: reqwest::Client,
    api_key: RwLock<Option<String>>,
}

impl CurseForgeProvider {
    pub fn new(api_key: Option<String>) -> Self {
        CurseForgeProvider {
            client: reqwest::Client::new(),
            api_key: RwLock::new(api_key),
        }
    }

    pub fn set_api_key(&self, key: Option<String>) {
        *self.api_key.write().unwrap() = key;
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.read().unwrap().is_some()
    }

    fn require_key(&self) -> Result<String, AppError> {
        self.api_key
            .read()
            .unwrap()
            .clone()
            .ok_or(AppError::MissingCurseForgeKey)
    }

    fn class_id_for(content_type: ContentType) -> u32 {
        match content_type {
            ContentType::Mod => CLASS_MODS,
            ContentType::ResourcePack => CLASS_RESOURCE_PACKS,
            ContentType::ShaderPack => CLASS_SHADERS,
            ContentType::Modpack => CLASS_MODPACKS,
        }
    }

    fn class_id_to_content_type(class_id: u32) -> ContentType {
        match class_id {
            CLASS_RESOURCE_PACKS => ContentType::ResourcePack,
            CLASS_SHADERS => ContentType::ShaderPack,
            CLASS_MODPACKS => ContentType::Modpack,
            _ => ContentType::Mod,
        }
    }

    /// CurseForge's public site is slug-based but its API is id-based, so
    /// a slug must be resolved via a filtered search first. Tried across
    /// every content-type class since a bare slug doesn't tell us the
    /// class up front (the caller usually already knows it from the URL,
    /// but this keeps the provider usable from free-text search too).
    async fn resolve_mod_id(&self, slug: &str, hint: Option<ContentType>) -> Result<RawMod, AppError> {
        let key = self.require_key()?;
        let classes = match hint {
            Some(ct) => vec![Self::class_id_for(ct)],
            None => vec![CLASS_MODS, CLASS_RESOURCE_PACKS, CLASS_SHADERS, CLASS_MODPACKS],
        };

        for class_id in classes {
            let resp = self
                .client
                .get(format!("{API_BASE}/mods/search"))
                .header("x-api-key", &key)
                .header("Accept", "application/json")
                .query(&[
                    ("gameId", MINECRAFT_GAME_ID.to_string()),
                    ("classId", class_id.to_string()),
                    ("slug", slug.to_string()),
                ])
                .send()
                .await?
                .error_for_status()
                .map_err(AppError::from)?;

            let parsed: RawSearchResponse = resp.json().await?;
            if let Some(m) = parsed.data.into_iter().next() {
                return Ok(m);
            }
        }

        Err(AppError::ProjectUnavailable)
    }

    async fn fetch_mod_by_id(&self, mod_id: u32) -> Result<RawMod, AppError> {
        let key = self.require_key()?;
        let resp = self
            .client
            .get(format!("{API_BASE}/mods/{mod_id}"))
            .header("x-api-key", &key)
            .header("Accept", "application/json")
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::ProjectUnavailable);
        }
        let resp = resp.error_for_status().map_err(AppError::from)?;
        let parsed: RawModResponse = resp.json().await?;
        Ok(parsed.data)
    }
}

#[async_trait]
impl ContentProvider for CurseForgeProvider {
    fn platform(&self) -> Platform {
        Platform::CurseForge
    }

    async fn get_project(&self, project_ref: &str) -> Result<Project, AppError> {
        // Accept either a raw numeric id (fast path) or a slug (search path).
        let raw = if let Ok(id) = project_ref.parse::<u32>() {
            self.fetch_mod_by_id(id).await?
        } else {
            self.resolve_mod_id(project_ref, None).await?
        };
        Ok(raw.into_project())
    }

    async fn list_versions(&self, project: &Project) -> Result<Vec<ProjectVersion>, AppError> {
        let key = self.require_key()?;
        let mod_id: u32 = project
            .id
            .parse()
            .map_err(|_| AppError::Internal("invalid curseforge mod id".into()))?;

        let resp = self
            .client
            .get(format!("{API_BASE}/mods/{mod_id}/files"))
            .header("x-api-key", &key)
            .header("Accept", "application/json")
            .query(&[("pageSize", "50")])
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::ProjectUnavailable);
        }
        let resp = resp.error_for_status().map_err(AppError::from)?;
        let parsed: RawFilesResponse = resp.json().await?;
        Ok(parsed
            .data
            .into_iter()
            .map(|f| f.into_version(mod_id))
            .collect())
    }

    async fn get_version(
        &self,
        project: &Project,
        version_ref: &str,
    ) -> Result<ProjectVersion, AppError> {
        let key = self.require_key()?;
        let mod_id: u32 = project
            .id
            .parse()
            .map_err(|_| AppError::Internal("invalid curseforge mod id".into()))?;
        let file_id: u32 = version_ref
            .parse()
            .map_err(|_| AppError::InvalidLink(version_ref.to_string()))?;

        let resp = self
            .client
            .get(format!("{API_BASE}/mods/{mod_id}/files/{file_id}"))
            .header("x-api-key", &key)
            .header("Accept", "application/json")
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::ProjectUnavailable);
        }
        let resp = resp.error_for_status().map_err(AppError::from)?;
        let parsed: RawFileResponse = resp.json().await?;
        Ok(parsed.data.into_version(mod_id))
    }

    async fn search(&self, query: &str, limit: u32) -> Result<Vec<Project>, AppError> {
        let key = self.require_key()?;
        let resp = self
            .client
            .get(format!("{API_BASE}/mods/search"))
            .header("x-api-key", &key)
            .header("Accept", "application/json")
            .query(&[
                ("gameId", MINECRAFT_GAME_ID.to_string()),
                ("searchFilter", query.to_string()),
                ("pageSize", limit.min(50).to_string()),
            ])
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::from)?;

        let parsed: RawSearchResponse = resp.json().await?;
        Ok(parsed.data.into_iter().map(RawMod::into_project).collect())
    }
}

fn map_relation_type(raw: u32) -> DependencyKind {
    match raw {
        3 => DependencyKind::Required,
        2 => DependencyKind::Optional,
        5 => DependencyKind::Incompatible,
        1 | 6 => DependencyKind::Embedded,
        _ => DependencyKind::Optional,
    }
}

/// CurseForge mixes Minecraft versions and loader names in one
/// `gameVersions` string array; split them using the known loader list.
fn split_game_versions(raw: &[String]) -> (Vec<String>, Vec<String>) {
    let mut mc_versions = Vec::new();
    let mut loaders = Vec::new();
    for v in raw {
        if KNOWN_LOADERS.iter().any(|l| l.eq_ignore_ascii_case(v)) {
            loaders.push(v.to_ascii_lowercase());
        } else {
            mc_versions.push(v.clone());
        }
    }
    (mc_versions, loaders)
}

#[derive(Debug, Deserialize)]
struct RawSearchResponse {
    data: Vec<RawMod>,
}

#[derive(Debug, Deserialize)]
struct RawModResponse {
    data: RawMod,
}

#[derive(Debug, Deserialize)]
struct RawMod {
    id: u32,
    #[serde(rename = "gameId")]
    #[allow(dead_code)]
    game_id: u32,
    name: String,
    slug: String,
    summary: String,
    #[serde(rename = "classId")]
    class_id: Option<u32>,
    logo: Option<RawLogo>,
    authors: Vec<RawAuthor>,
    links: RawLinks,
}

impl RawMod {
    fn into_project(self) -> Project {
        let content_type = self
            .class_id
            .map(CurseForgeProvider::class_id_to_content_type)
            .unwrap_or(ContentType::Mod);
        Project {
            platform: Platform::CurseForge,
            id: self.id.to_string(),
            slug: self.slug,
            name: self.name,
            author: self
                .authors
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            summary: self.summary,
            icon_url: self.logo.map(|l| l.thumbnail_url),
            content_type,
            project_url: self.links.website_url,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawLogo {
    #[serde(rename = "thumbnailUrl")]
    thumbnail_url: String,
}

#[derive(Debug, Deserialize)]
struct RawAuthor {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawLinks {
    #[serde(rename = "websiteUrl")]
    website_url: String,
}

#[derive(Debug, Deserialize)]
struct RawFilesResponse {
    data: Vec<RawFile>,
}

#[derive(Debug, Deserialize)]
struct RawFileResponse {
    data: RawFile,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    id: u32,
    #[serde(rename = "modId")]
    mod_id: u32,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "downloadUrl")]
    download_url: Option<String>,
    #[serde(rename = "fileLength")]
    file_length: u64,
    #[serde(rename = "gameVersions")]
    game_versions: Vec<String>,
    hashes: Vec<RawHash>,
    dependencies: Vec<RawFileDependency>,
    #[serde(rename = "fileDate")]
    file_date: String,
}

impl RawFile {
    fn into_version(self, fallback_mod_id: u32) -> ProjectVersion {
        let (game_versions, loaders) = split_game_versions(&self.game_versions);
        let sha1 = self
            .hashes
            .iter()
            .find(|h| h.algo == 1)
            .map(|h| h.value.clone());

        let download_url = self.download_url.unwrap_or_default();
        let file = ProjectFile {
            filename: self.file_name,
            download_url,
            size_bytes: self.file_length,
            hashes: FileHashes { sha1, sha512: None },
            primary: true,
        };

        ProjectVersion {
            id: self.id.to_string(),
            project_id: self.mod_id.to_string().into(),
            version_number: self.display_name,
            game_versions,
            loaders,
            dependencies: self
                .dependencies
                .into_iter()
                .map(|d| d.into_dependency())
                .collect(),
            files: vec![file],
            date_published: self.file_date,
        }
        .with_project_id_fallback(fallback_mod_id)
    }
}

// Small helper trait extension kept local to this module: RawFile always
// carries its own modId, but this guards against a `0` default should
// deserialization ever produce one.
trait WithProjectIdFallback {
    fn with_project_id_fallback(self, fallback: u32) -> Self;
}

impl WithProjectIdFallback for ProjectVersion {
    fn with_project_id_fallback(mut self, fallback: u32) -> Self {
        if self.project_id.is_empty() || self.project_id == "0" {
            self.project_id = fallback.to_string();
        }
        self
    }
}

#[derive(Debug, Deserialize)]
struct RawHash {
    value: String,
    algo: u32,
}

#[derive(Debug, Deserialize)]
struct RawFileDependency {
    #[serde(rename = "modId")]
    mod_id: u32,
    #[serde(rename = "relationType")]
    relation_type: u32,
}

impl RawFileDependency {
    fn into_dependency(self) -> Dependency {
        Dependency {
            project_id: self.mod_id.to_string(),
            version_id: None,
            project_name: None,
            kind: map_relation_type(self.relation_type),
        }
    }
}
