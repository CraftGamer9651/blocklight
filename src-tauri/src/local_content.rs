use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::AppError;
use crate::models::{ContentType, Instance, Loader, Platform};

/// Management of the files in an instance's `mods/`, `resourcepacks/` and
/// `shaderpacks/` folders: list them, delete them, and copy in files the
/// user picked (for content that isn't on Modrinth/CurseForge).
///
/// The folder on disk is the source of truth for what's installed (so
/// content dropped in by hand shows up too); `Instance::installed` is
/// only consulted to label which entries Blocklight installed from a
/// provider. Everything here is local filesystem access, so it works
/// offline.

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledItem {
    pub file_name: String,
    /// Resource and shader packs may be unzipped folders.
    pub is_folder: bool,
    /// `None` for folders (not worth walking the tree for a size).
    pub size_bytes: Option<u64>,
    pub modified: Option<String>,
    /// `Some` when Blocklight installed this from a provider; `None` for
    /// content added by hand.
    pub platform: Option<Platform>,
    pub project_slug: Option<String>,
    pub version_number: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedItem {
    pub file_name: String,
    /// Set when the file doesn't look right for this instance (wrong
    /// loader, missing pack metadata). It's still copied -- this is a
    /// heads-up, not a block.
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedItem {
    pub file_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AddResult {
    pub added: Vec<AddedItem>,
    pub skipped: Vec<SkippedItem>,
    /// True when the file picker was dismissed without choosing anything.
    pub cancelled: bool,
}

/// What a content type's folder holds: the file extension (without the
/// dot) and whether unzipped folders count too.
pub fn kind(content_type: ContentType) -> Result<(&'static str, bool), AppError> {
    match content_type {
        ContentType::Mod => Ok(("jar", false)),
        ContentType::ResourcePack | ContentType::ShaderPack => Ok(("zip", true)),
        ContentType::Modpack => Err(AppError::Internal(
            "modpacks aren't managed from this page".into(),
        )),
    }
}

pub fn content_dir(instance: &Instance, content_type: ContentType) -> PathBuf {
    PathBuf::from(&instance.instance_dir).join(content_type.install_subdir())
}

fn has_ext(name: &str, ext: &str) -> bool {
    name.to_ascii_lowercase().ends_with(&format!(".{ext}"))
}

/// A name must be a bare file/folder name: no separators, no drive
/// prefixes, no `..`. The frontend only ever sends names it got from
/// `list_items`, but the backend doesn't rely on that. Mods must also be
/// `.jar` files; packs can be folders, so their names have no extension
/// requirement here (`remove_item` checks files separately).
pub fn validate_file_name(content_type: ContentType, name: &str) -> Result<(), AppError> {
    let (ext, folders) = kind(content_type)?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
    {
        return Err(AppError::PathTraversal(name.to_string()));
    }
    if !folders && !has_ext(name, ext) {
        return Err(AppError::Internal(format!("{name} isn't a .{ext} file")));
    }
    Ok(())
}

/// Everything of this content type in the instance's folder, sorted by
/// name, each labelled with where it came from.
pub fn list_items(
    instance: &Instance,
    content_type: ContentType,
) -> Result<Vec<InstalledItem>, AppError> {
    let (ext, folders) = kind(content_type)?;
    let Ok(entries) = fs::read_dir(content_dir(instance, content_type)) else {
        return Ok(Vec::new());
    };

    let mut items = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_name) = entry.file_name().into_string() else {
            continue;
        };
        if file_name.starts_with('.') {
            continue;
        }
        // `fs::metadata` follows symlinks, so a linked file reports its
        // real size and a dangling link is skipped.
        let Ok(meta) = fs::metadata(entry.path()) else {
            continue;
        };
        let is_folder = meta.is_dir();
        if is_folder {
            if !folders {
                continue;
            }
        } else if !(meta.is_file() && has_ext(&file_name, ext)) {
            continue;
        }

        let modified = meta.modified().ok().map(|t| {
            let datetime: DateTime<Utc> = t.into();
            datetime.to_rfc3339()
        });

        let tracked = instance
            .installed
            .iter()
            .find(|c| c.content_type == content_type && c.file_name == file_name);

        items.push(InstalledItem {
            is_folder,
            size_bytes: if is_folder { None } else { Some(meta.len()) },
            modified,
            platform: tracked.map(|c| c.platform),
            project_slug: tracked.map(|c| c.project_slug.clone()),
            version_number: tracked.map(|c| c.version_number.clone()),
            file_name,
        });
    }

    items.sort_by_key(|m| m.file_name.to_lowercase());
    Ok(items)
}

/// Deletes one entry (a file, or an unzipped pack folder). Removing
/// something that's already gone succeeds, so the caller can still clear
/// a stale install record.
pub fn remove_item(
    instance: &Instance,
    content_type: ContentType,
    file_name: &str,
) -> Result<(), AppError> {
    validate_file_name(content_type, file_name)?;
    let (ext, _) = kind(content_type)?;
    let path = content_dir(instance, content_type).join(file_name);

    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if meta.is_dir() {
        // A real folder inside this content folder (symlinks report as
        // symlinks here, not dirs, so a link is never followed).
        fs::remove_dir_all(&path)?;
    } else {
        if !has_ext(file_name, ext) {
            return Err(AppError::Internal(format!("{file_name} isn't a .{ext} file")));
        }
        // For a symlink this removes the link, never its target.
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// Copies each picked file into the instance's folder for this content
/// type. One bad file never aborts the rest: it lands in `skipped` with a
/// reason instead.
pub fn add_from_paths(
    instance: &Instance,
    content_type: ContentType,
    paths: &[PathBuf],
) -> AddResult {
    let mut result = AddResult::default();
    for path in paths {
        match add_one(instance, content_type, path) {
            Ok(added) => result.added.push(added),
            Err(reason) => result.skipped.push(SkippedItem {
                file_name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string()),
                reason,
            }),
        }
    }
    result
}

fn add_one(
    instance: &Instance,
    content_type: ContentType,
    src: &Path,
) -> Result<AddedItem, String> {
    let (ext, _) = kind(content_type).map_err(|e| e.to_string())?;

    let file_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "couldn't read the file name".to_string())?
        .to_string();

    if !has_ext(&file_name, ext) {
        return Err(format!("only .{ext} files can be added here"));
    }
    validate_file_name(content_type, &file_name).map_err(|e| e.to_string())?;

    let meta = fs::metadata(src).map_err(|e| format!("couldn't read the file: {e}"))?;
    if !meta.is_file() {
        return Err("that isn't a file".into());
    }

    // Jars and zips are both zip archives. Opening one as such catches
    // renamed or truncated files before they land in the instance.
    let file = fs::File::open(src).map_err(|e| format!("couldn't open the file: {e}"))?;
    let archive = zip::ZipArchive::new(file)
        .map_err(|_| format!("this isn't a valid .{ext} file (it couldn't be read as an archive)"))?;
    let warning = inspect_archive(&archive, content_type, instance.loader);

    let dir = content_dir(instance, content_type);
    fs::create_dir_all(&dir).map_err(|e| format!("couldn't create the folder: {e}"))?;

    let dest = dir.join(&file_name);
    if fs::symlink_metadata(&dest).is_ok() {
        return Err("something with this name is already installed (remove it first to replace it)".into());
    }

    // Copy to a temporary name that has neither extension, then rename,
    // so an interrupted copy can never leave a half-written file that the
    // game would try to load.
    let tmp = dir.join(format!("{file_name}.blocklight-part"));
    if let Err(e) = fs::copy(src, &tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("couldn't copy the file: {e}"));
    }
    if let Err(e) = fs::rename(&tmp, &dest) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("couldn't finish copying the file: {e}"));
    }

    Ok(AddedItem { file_name, warning })
}

/// A heads-up message if the archive doesn't look like what the tab it
/// was added from expects; `None` when it looks fine or can't be judged.
fn inspect_archive<R: std::io::Read + std::io::Seek>(
    archive: &zip::ZipArchive<R>,
    content_type: ContentType,
    instance_loader: Loader,
) -> Option<String> {
    match content_type {
        ContentType::Mod => {
            let found = loaders_in_jar(archive);
            if loader_matches(instance_loader, &found) {
                None
            } else {
                let labels: Vec<&str> = found.iter().map(|l| l.label()).collect();
                Some(format!(
                    "This looks like a {} mod, but this instance uses {}. It probably won't load.",
                    labels.join(" / "),
                    instance_loader.label()
                ))
            }
        }
        ContentType::ResourcePack => {
            if archive.file_names().any(|n| n == "pack.mcmeta") {
                None
            } else {
                Some(
                    "No pack.mcmeta at the top level of this zip, so Minecraft may not recognize it \
                     as a resource pack. If the pack sits inside a folder in the zip, re-zip its contents instead."
                        .into(),
                )
            }
        }
        ContentType::ShaderPack => {
            if archive.file_names().any(|n| n.starts_with("shaders/")) {
                None
            } else {
                Some(
                    "No shaders folder at the top level of this zip, so your shader mod may not recognize it. \
                     If the pack sits inside a folder in the zip, re-zip its contents instead."
                        .into(),
                )
            }
        }
        ContentType::Modpack => None,
    }
}

/// Which mod loaders a jar declares support for, from the metadata file
/// each loader requires. Empty for jars with none (libraries, coremods),
/// in which case nothing can be said about compatibility.
fn loaders_in_jar<R: std::io::Read + std::io::Seek>(archive: &zip::ZipArchive<R>) -> Vec<Loader> {
    let has = |name: &str| archive.file_names().any(|n| n == name);
    let mut found = Vec::new();
    if has("fabric.mod.json") {
        found.push(Loader::Fabric);
    }
    if has("quilt.mod.json") {
        found.push(Loader::Quilt);
    }
    if has("META-INF/neoforge.mods.toml") {
        found.push(Loader::NeoForge);
    }
    if has("META-INF/mods.toml") {
        found.push(Loader::Forge);
    }
    found
}

/// True unless the jar clearly targets other loaders than the instance's.
/// Deliberately lenient: an unknown jar passes, Quilt runs Fabric mods,
/// and NeoForge accepts `mods.toml` (older NeoForge versions still use
/// that file name), so only unambiguous mismatches are flagged.
fn loader_matches(instance: Loader, found: &[Loader]) -> bool {
    if found.is_empty() {
        return true;
    }
    found.iter().any(|&jar| {
        jar == instance
            || (instance == Loader::Quilt && jar == Loader::Fabric)
            || (instance == Loader::NeoForge && jar == Loader::Forge)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::InstalledContent;

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("blocklight-mods-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn instance_in(dir: &Path, loader: Loader) -> Instance {
        Instance {
            id: "test".into(),
            name: "Test".into(),
            minecraft_version: "1.21.1".into(),
            loader,
            loader_version: None,
            java_major_version: Some(21),
            instance_dir: dir.to_string_lossy().to_string(),
            memory_mb: None,
            jvm_args: None,
            installed: Vec::new(),
        }
    }

    #[test]
    fn file_names_must_be_bare_jars() {
        assert!(validate_file_name(ContentType::Mod, "sodium-0.6.jar").is_ok());
        assert!(validate_file_name(ContentType::Mod, "Sodium.JAR").is_ok());
        for bad in ["", "..", "../evil.jar", "a/b.jar", "a\\b.jar", "C:evil.jar", "notes.txt"] {
            assert!(validate_file_name(ContentType::Mod, bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn loader_compatibility_is_lenient_but_catches_clear_mismatches() {
        assert!(loader_matches(Loader::Fabric, &[]));
        assert!(loader_matches(Loader::Fabric, &[Loader::Fabric]));
        assert!(loader_matches(Loader::Quilt, &[Loader::Fabric]));
        assert!(loader_matches(Loader::NeoForge, &[Loader::Forge]));
        assert!(loader_matches(Loader::NeoForge, &[Loader::Fabric, Loader::NeoForge]));
        assert!(!loader_matches(Loader::NeoForge, &[Loader::Fabric]));
        assert!(!loader_matches(Loader::Forge, &[Loader::NeoForge]));
        assert!(!loader_matches(Loader::Minecraft, &[Loader::Fabric]));
    }

    #[test]
    fn list_shows_only_jars_sorted_and_labels_tracked_ones() {
        let dir = scratch("list");
        let mods = dir.join("mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("b-manual.jar"), b"x").unwrap();
        fs::write(mods.join("A-sodium.jar"), b"xyz").unwrap();
        fs::write(mods.join("readme.txt"), b"x").unwrap();
        fs::write(mods.join("old.jar.disabled"), b"x").unwrap();
        fs::create_dir_all(mods.join("folder.jar")).unwrap();

        let mut instance = instance_in(&dir, Loader::Fabric);
        instance.installed.push(InstalledContent {
            platform: Platform::Modrinth,
            project_id: "AANobbMI".into(),
            project_slug: "sodium".into(),
            content_type: ContentType::Mod,
            version_id: "v1".into(),
            version_number: "0.6.0".into(),
            file_name: "A-sodium.jar".into(),
        });

        let listed = list_items(&instance, ContentType::Mod).unwrap();
        let names: Vec<&str> = listed.iter().map(|m| m.file_name.as_str()).collect();
        assert_eq!(names, vec!["A-sodium.jar", "b-manual.jar"]);
        assert_eq!(listed[0].platform, Some(Platform::Modrinth));
        assert_eq!(listed[0].version_number.as_deref(), Some("0.6.0"));
        assert_eq!(listed[0].size_bytes, Some(3));
        assert_eq!(listed[1].platform, None);
    }

    #[test]
    fn remove_deletes_the_file_and_refuses_escapes() {
        let dir = scratch("remove");
        let mods = dir.join("mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("gone.jar"), b"x").unwrap();
        fs::write(dir.join("precious.jar"), b"x").unwrap();
        let instance = instance_in(&dir, Loader::Fabric);

        remove_item(&instance, ContentType::Mod, "gone.jar").unwrap();
        assert!(!mods.join("gone.jar").exists());
        // Already gone is fine.
        remove_item(&instance, ContentType::Mod, "gone.jar").unwrap();

        assert!(remove_item(&instance, ContentType::Mod, "../precious.jar").is_err());
        assert!(dir.join("precious.jar").exists());
    }

    #[test]
    fn add_rejects_non_jars_and_corrupt_jars() {
        let dir = scratch("add");
        let instance = instance_in(&dir, Loader::Fabric);
        let notes = dir.join("notes.txt");
        let fake = dir.join("fake.jar");
        fs::write(&notes, b"hello").unwrap();
        fs::write(&fake, b"definitely not a zip").unwrap();

        let result = add_from_paths(&instance, ContentType::Mod, &[notes, fake]);
        assert!(result.added.is_empty());
        assert_eq!(result.skipped.len(), 2);
        assert!(!dir.join("mods").join("fake.jar").exists());
    }

    #[test]
    fn packs_list_zips_and_folders_and_remove_both() {
        let dir = scratch("packs");
        let rp = dir.join("resourcepacks");
        fs::create_dir_all(rp.join("unzipped-pack")).unwrap();
        fs::create_dir_all(rp.join(".hidden")).unwrap();
        fs::write(rp.join("faithful.zip"), b"zz").unwrap();
        fs::write(rp.join("notes.txt"), b"x").unwrap();
        let instance = instance_in(&dir, Loader::Fabric);

        let listed = list_items(&instance, ContentType::ResourcePack).unwrap();
        let names: Vec<(&str, bool)> = listed.iter().map(|i| (i.file_name.as_str(), i.is_folder)).collect();
        assert_eq!(names, vec![("faithful.zip", false), ("unzipped-pack", true)]);
        assert_eq!(listed[1].size_bytes, None);

        remove_item(&instance, ContentType::ResourcePack, "unzipped-pack").unwrap();
        remove_item(&instance, ContentType::ResourcePack, "faithful.zip").unwrap();
        assert!(remove_item(&instance, ContentType::ResourcePack, "notes.txt").is_err());
        assert!(rp.join("notes.txt").exists());
        assert_eq!(list_items(&instance, ContentType::ResourcePack).unwrap().len(), 0);
        assert!(list_items(&instance, ContentType::Modpack).is_err());
    }
}
