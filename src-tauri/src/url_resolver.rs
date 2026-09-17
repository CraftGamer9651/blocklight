use url::Url;

use crate::error::AppError;
use crate::models::{ContentType, Platform};

/// What a pasted/dropped/clipboard URL turned out to point at, before any
/// network call is made. Steps 1-4 of the spec's "URL Resolution" list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLink {
    pub platform: Platform,
    pub content_type: ContentType,
    /// Modrinth: slug or base62 id. CurseForge: slug (resolved to a
    /// numeric mod id by the provider, since CurseForge's API is id-based).
    pub project_ref: String,
    /// Present when the URL identifies one specific file/version, e.g.
    /// `.../version/abcd1234` or `.../files/1234567`.
    pub version_ref: Option<String>,
}

pub fn resolve(raw: &str) -> Result<ResolvedLink, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidLink("empty link".into()));
    }

    let url = Url::parse(trimmed).map_err(|_| AppError::InvalidLink(trimmed.to_string()))?;

    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(AppError::InvalidLink(trimmed.to_string()));
    }

    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|seg| !seg.is_empty()).collect())
        .unwrap_or_default();

    match host.as_str() {
        "modrinth.com" | "www.modrinth.com" => resolve_modrinth(&segments),
        "curseforge.com" | "www.curseforge.com" => resolve_curseforge(&segments),
        _ => Err(AppError::UnsupportedLink),
    }
}

fn resolve_modrinth(segments: &[&str]) -> Result<ResolvedLink, AppError> {
    // Supported shapes:
    //   /mod/<slug>                /mod/<slug>/version/<version-id>
    //   /resourcepack/<slug>       /shader/<slug>       /modpack/<slug>
    //   /project/<id-or-slug>      (generic; type resolved via the API)
    if segments.len() < 2 {
        return Err(AppError::InvalidLink("modrinth.com".into()));
    }

    let content_type = match segments[0] {
        "mod" => ContentType::Mod,
        "resourcepack" | "datapack" => ContentType::ResourcePack,
        "shader" => ContentType::ShaderPack,
        "modpack" => ContentType::Modpack,
        // "project" and "plugin" defer to whatever the API reports.
        "project" | "plugin" => ContentType::Mod,
        _ => return Err(AppError::InvalidLink("modrinth.com".into())),
    };

    let project_ref = segments[1].to_string();
    let version_ref = if segments.len() >= 4 && segments[2] == "version" {
        Some(segments[3].to_string())
    } else {
        None
    };

    Ok(ResolvedLink {
        platform: Platform::Modrinth,
        content_type,
        project_ref,
        version_ref,
    })
}

fn resolve_curseforge(segments: &[&str]) -> Result<ResolvedLink, AppError> {
    // Supported shapes:
    //   /minecraft/mc-mods/<slug>            /minecraft/mc-mods/<slug>/files/<file-id>
    //   /minecraft/texture-packs/<slug>      /minecraft/shaders/<slug>
    //   /minecraft/modpacks/<slug>
    if segments.len() < 3 || segments[0] != "minecraft" {
        return Err(AppError::InvalidLink("curseforge.com".into()));
    }

    let content_type = match segments[1] {
        "mc-mods" => ContentType::Mod,
        "texture-packs" => ContentType::ResourcePack,
        "shaders" => ContentType::ShaderPack,
        "modpacks" => ContentType::Modpack,
        _ => return Err(AppError::InvalidLink("curseforge.com".into())),
    };

    let project_ref = segments[2].to_string();
    let version_ref = if segments.len() >= 5 && segments[3] == "files" {
        Some(segments[4].to_string())
    } else {
        None
    };

    Ok(ResolvedLink {
        platform: Platform::CurseForge,
        content_type,
        project_ref,
        version_ref,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modrinth_mod() {
        let r = resolve("https://modrinth.com/mod/sodium").unwrap();
        assert_eq!(r.platform, Platform::Modrinth);
        assert_eq!(r.content_type, ContentType::Mod);
        assert_eq!(r.project_ref, "sodium");
        assert_eq!(r.version_ref, None);
    }

    #[test]
    fn parses_modrinth_version() {
        let r = resolve("https://modrinth.com/mod/sodium/version/abc123").unwrap();
        assert_eq!(r.version_ref, Some("abc123".to_string()));
    }

    #[test]
    fn parses_curseforge_mod() {
        let r = resolve("https://www.curseforge.com/minecraft/mc-mods/jei").unwrap();
        assert_eq!(r.platform, Platform::CurseForge);
        assert_eq!(r.content_type, ContentType::Mod);
        assert_eq!(r.project_ref, "jei");
    }

    #[test]
    fn parses_curseforge_file() {
        let r =
            resolve("https://www.curseforge.com/minecraft/mc-mods/jei/files/1234567").unwrap();
        assert_eq!(r.version_ref, Some("1234567".to_string()));
    }

    #[test]
    fn rejects_unsupported_host() {
        assert!(matches!(
            resolve("https://example.com/mod/sodium"),
            Err(AppError::UnsupportedLink)
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert!(resolve("not a url").is_err());
    }
}
