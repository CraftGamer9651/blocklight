use async_trait::async_trait;
use serde::Deserialize;

use crate::error::AppError;
use crate::models::{
    ContentType, Dependency, DependencyKind, FileHashes, Platform, Project, ProjectFile,
    ProjectVersion,
};

use super::ContentProvider;

const API_BASE: &str = "https://api.modrinth.com/v2";

/// Modrinth requires "a uniquely-identifying User-Agent header with all
/// API requests" and blocks generic ones. This identifies Blocklight and
/// a contact point, per their API guidelines.
const USER_AGENT: &str = "CraftGamer9651/blocklight/0.1.0 (https://craftgamer9651.github.io/)";

pub struct ModrinthProvider {
    client: reqwest::Client,
}

impl ModrinthProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to build reqwest client");
        ModrinthProvider { client }
    }

    fn project_url(slug_or_id: &str) -> String {
        format!("{API_BASE}/project/{slug_or_id}")
    }
}

impl Default for ModrinthProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentProvider for ModrinthProvider {
    fn platform(&self) -> Platform {
        Platform::Modrinth
    }

    async fn get_project(&self, project_ref: &str) -> Result<Project, AppError> {
        let resp = self
            .client
            .get(Self::project_url(project_ref))
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::ProjectUnavailable);
        }
        let resp = resp.error_for_status().map_err(AppError::from)?;
        let raw: RawProject = resp.json().await?;

        let author = self.resolve_author(&raw.id).await.unwrap_or_default();

        Ok(raw.into_project(author))
    }

    async fn list_versions(&self, project: &Project) -> Result<Vec<ProjectVersion>, AppError> {
        let url = format!("{API_BASE}/project/{}/version", project.id);
        let resp = self.client.get(url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::ProjectUnavailable);
        }
        let resp = resp.error_for_status().map_err(AppError::from)?;
        let raw: Vec<RawVersion> = resp.json().await?;
        Ok(raw.into_iter().map(RawVersion::into_version).collect())
    }

    async fn get_version(
        &self,
        _project: &Project,
        version_ref: &str,
    ) -> Result<ProjectVersion, AppError> {
        let url = format!("{API_BASE}/version/{version_ref}");
        let resp = self.client.get(url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::ProjectUnavailable);
        }
        let resp = resp.error_for_status().map_err(AppError::from)?;
        let raw: RawVersion = resp.json().await?;
        Ok(raw.into_version())
    }

    async fn search(&self, query: &str, limit: u32) -> Result<Vec<Project>, AppError> {
        let url = format!("{API_BASE}/search");
        let resp = self
            .client
            .get(url)
            .query(&[
                ("query", query.to_string()),
                ("limit", limit.min(50).to_string()),
            ])
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::from)?;

        let raw: RawSearchResponse = resp.json().await?;
        Ok(raw.hits.into_iter().map(RawSearchHit::into_project).collect())
    }
}

impl ModrinthProvider {
    /// Modrinth-specific "top downloaded" browse list for one content
    /// type -- not part of `ContentProvider` since it's not a capability
    /// CurseForge exposes the same way and isn't needed generically
    /// elsewhere. Empty query + a `project_type` facet + sorting by
    /// downloads is Modrinth's documented way to get a popularity-ranked
    /// list without a search term.
    pub async fn list_trending(&self, content_type: ContentType, limit: u32) -> Result<Vec<Project>, AppError> {
        let project_type = match content_type {
            ContentType::Mod => "mod",
            ContentType::ResourcePack => "resourcepack",
            ContentType::ShaderPack => "shader",
            ContentType::Modpack => "modpack",
        };
        // Modrinth's `facets` param is a JSON array of arrays of
        // "key:value" strings (inner arrays are OR'd, outer AND'd); a
        // single filter is just one outer entry with one inner entry.
        let facets = format!(r#"[["project_type:{project_type}"]]"#);

        let url = format!("{API_BASE}/search");
        let resp = self
            .client
            .get(url)
            .query(&[
                ("query", String::new()),
                ("facets", facets),
                ("index", "downloads".to_string()),
                ("limit", limit.min(50).to_string()),
            ])
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::from)?;

        let raw: RawSearchResponse = resp.json().await?;
        Ok(raw.hits.into_iter().map(RawSearchHit::into_project).collect())
    }

    /// The project endpoint only returns a team id, not an author name, so
    /// resolve the team owner's username with one extra request. Best
    /// effort: a failure here shouldn't fail the whole lookup.
    async fn resolve_author(&self, project_id: &str) -> Option<String> {
        let url = format!("{API_BASE}/project/{project_id}/members");
        let resp = self.client.get(url).send().await.ok()?;
        let members: Vec<RawMember> = resp.json().await.ok()?;
        members
            .iter()
            .find(|m| m.role.eq_ignore_ascii_case("owner"))
            .or_else(|| members.first())
            .map(|m| m.user.username.clone())
    }
}

fn map_project_type(raw: &str) -> ContentType {
    match raw {
        "mod" | "plugin" => ContentType::Mod,
        "resourcepack" | "datapack" => ContentType::ResourcePack,
        "shader" => ContentType::ShaderPack,
        "modpack" => ContentType::Modpack,
        _ => ContentType::Mod,
    }
}

fn map_dependency_kind(raw: &str) -> DependencyKind {
    match raw {
        "required" => DependencyKind::Required,
        "optional" => DependencyKind::Optional,
        "incompatible" => DependencyKind::Incompatible,
        "embedded" => DependencyKind::Embedded,
        _ => DependencyKind::Optional,
    }
}

#[derive(Debug, Deserialize)]
struct RawProject {
    id: String,
    slug: String,
    title: String,
    description: String,
    icon_url: Option<String>,
    project_type: String,
}

impl RawProject {
    fn into_project(self, author: String) -> Project {
        Project {
            platform: Platform::Modrinth,
            id: self.id.clone(),
            slug: self.slug.clone(),
            name: self.title,
            author,
            summary: self.description,
            icon_url: self.icon_url,
            content_type: map_project_type(&self.project_type),
            project_url: format!(
                "https://modrinth.com/{}/{}",
                project_type_path(&self.project_type),
                self.slug
            ),
        }
    }
}

fn project_type_path(raw: &str) -> &str {
    match raw {
        "mod" => "mod",
        "resourcepack" => "resourcepack",
        "shader" => "shader",
        "modpack" => "modpack",
        "plugin" => "plugin",
        other => other,
    }
}

#[derive(Debug, Deserialize)]
struct RawMember {
    role: String,
    user: RawUser,
}

#[derive(Debug, Deserialize)]
struct RawUser {
    username: String,
}

#[derive(Debug, Deserialize)]
struct RawVersion {
    id: String,
    project_id: String,
    version_number: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    dependencies: Vec<RawDependency>,
    files: Vec<RawFile>,
    date_published: String,
}

impl RawVersion {
    fn into_version(self) -> ProjectVersion {
        ProjectVersion {
            id: self.id,
            project_id: self.project_id,
            version_number: self.version_number,
            game_versions: self.game_versions,
            loaders: self.loaders,
            dependencies: self.dependencies.into_iter().map(RawDependency::into_dependency).collect(),
            files: self.files.into_iter().map(RawFile::into_file).collect(),
            date_published: self.date_published,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawDependency {
    version_id: Option<String>,
    project_id: Option<String>,
    dependency_type: String,
}

impl RawDependency {
    fn into_dependency(self) -> Dependency {
        Dependency {
            project_id: self.project_id.unwrap_or_default(),
            version_id: self.version_id,
            project_name: None,
            kind: map_dependency_kind(&self.dependency_type),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawFile {
    hashes: RawHashes,
    url: String,
    filename: String,
    primary: bool,
    size: u64,
}

impl RawFile {
    fn into_file(self) -> ProjectFile {
        ProjectFile {
            filename: self.filename,
            download_url: self.url,
            size_bytes: self.size,
            hashes: FileHashes {
                sha1: self.hashes.sha1,
                sha512: self.hashes.sha512,
            },
            primary: self.primary,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawHashes {
    sha1: Option<String>,
    sha512: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSearchResponse {
    hits: Vec<RawSearchHit>,
}

#[derive(Debug, Deserialize)]
struct RawSearchHit {
    project_id: String,
    slug: String,
    title: String,
    author: String,
    description: String,
    icon_url: Option<String>,
    project_type: String,
}

impl RawSearchHit {
    fn into_project(self) -> Project {
        Project {
            platform: Platform::Modrinth,
            id: self.project_id,
            slug: self.slug.clone(),
            name: self.title,
            author: self.author,
            summary: self.description,
            icon_url: self.icon_url,
            content_type: map_project_type(&self.project_type),
            project_url: format!(
                "https://modrinth.com/{}/{}",
                project_type_path(&self.project_type),
                self.slug
            ),
        }
    }
}
