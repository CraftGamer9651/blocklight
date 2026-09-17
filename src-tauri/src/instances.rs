use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::app_data_dir;
use crate::error::AppError;
use crate::models::{InstalledContent, Instance, Loader};

/// Full instance/world/Java management (downloading Minecraft versions,
/// installing loaders, provisioning Java runtimes) is its own large
/// feature and out of scope for the link-import and offline-mode specs
/// this app implements. `InstanceStore` provides just enough of a real,
/// disk-backed instance list for those two features to operate against:
/// compatibility checks, dependency checks, "already installed"
/// detection, and offline readiness all read genuine data from here
/// rather than being mocked inline in a command handler.
pub struct InstanceStore {
    path: PathBuf,
    instances: Mutex<Vec<Instance>>,
}

#[derive(Serialize, Deserialize)]
struct InstancesFile {
    instances: Vec<Instance>,
}

impl InstanceStore {
    pub fn load_or_seed() -> Result<Self, AppError> {
        let dir = app_data_dir()?;
        let path = dir.join("instances.json");

        let instances = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str::<InstancesFile>(&raw)
                .map(|f| f.instances)
                .unwrap_or_else(|_| seed_instances(&dir))
        } else {
            let seeded = seed_instances(&dir);
            let store = InstanceStore {
                path: path.clone(),
                instances: Mutex::new(seeded.clone()),
            };
            store.persist()?;
            return Ok(store);
        };

        Ok(InstanceStore {
            path,
            instances: Mutex::new(instances),
        })
    }

    fn persist(&self) -> Result<(), AppError> {
        let instances = self.instances.lock().unwrap().clone();
        let file = InstancesFile { instances };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Instance> {
        self.instances.lock().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Result<Instance, AppError> {
        self.instances
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or_else(|| AppError::Internal(format!("unknown instance: {id}")))
    }

    pub fn record_installed(
        &self,
        instance_id: &str,
        content: InstalledContent,
    ) -> Result<(), AppError> {
        {
            let mut guard = self.instances.lock().unwrap();
            let instance = guard
                .iter_mut()
                .find(|i| i.id == instance_id)
                .ok_or_else(|| AppError::Internal(format!("unknown instance: {instance_id}")))?;

            instance
                .installed
                .retain(|c| c.project_id != content.project_id);
            instance.installed.push(content);
        }
        self.persist()
    }

    /// Creates a new instance backed by a real directory on disk (with
    /// the same `mods/resourcepacks/shaderpacks/saves` layout as the
    /// seeded example), persists it, and returns it. This deliberately
    /// stops at "an instance with a Minecraft version and loader exists" —
    /// it doesn't download Minecraft, a mod loader, or a Java runtime,
    /// which is the larger instance-management feature neither spec this
    /// app implements covers (see the README).
    pub fn create(
        &self,
        name: &str,
        minecraft_version: &str,
        loader: Loader,
        loader_version: Option<String>,
        java_major_version: Option<u32>,
    ) -> Result<Instance, AppError> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(AppError::Internal("instance name can't be empty".into()));
        }
        let trimmed_version = minecraft_version.trim();
        if trimmed_version.is_empty() {
            return Err(AppError::Internal(
                "Minecraft version can't be empty".into(),
            ));
        }

        let data_dir = app_data_dir()?;
        let slug = self.unique_slug(&slugify(trimmed_name));
        let instance_dir = data_dir.join("instances").join(&slug);

        for sub in ["mods", "resourcepacks", "shaderpacks", "saves"] {
            std::fs::create_dir_all(instance_dir.join(sub))?;
        }

        let instance = Instance {
            id: Uuid::new_v4().to_string(),
            name: trimmed_name.to_string(),
            minecraft_version: trimmed_version.to_string(),
            loader,
            loader_version,
            java_major_version,
            instance_dir: instance_dir.to_string_lossy().to_string(),
            installed: Vec::new(),
        };

        self.instances.lock().unwrap().push(instance.clone());
        self.persist()?;

        Ok(instance)
    }

    /// Appends `-2`, `-3`, ... to `base` until it doesn't collide with an
    /// existing instance's directory name, so two instances (e.g. two
    /// people both naming one "Modded Survival") never share a folder.
    fn unique_slug(&self, base: &str) -> String {
        let existing_slugs: Vec<String> = {
            let guard = self.instances.lock().unwrap();
            guard
                .iter()
                .filter_map(|i| {
                    PathBuf::from(&i.instance_dir)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                })
                .collect()
        };

        if !existing_slugs.iter().any(|s| s == base) {
            return base.to_string();
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base}-{n}");
            if !existing_slugs.iter().any(|s| s == &candidate) {
                return candidate;
            }
            n += 1;
        }
    }
}

/// Lowercase, hyphen-separated, filesystem-safe folder name derived from
/// a user-chosen instance name. Never returns an empty string.
fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = true; // avoid a leading dash
    for ch in input.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() {
        "instance".to_string()
    } else {
        trimmed.to_string()
    }
}

fn seed_instances(data_dir: &std::path::Path) -> Vec<Instance> {
    let instances_dir = data_dir.join("instances");
    let survival_dir = instances_dir.join("modded-survival");
    let _ = std::fs::create_dir_all(&survival_dir);
    let _ = std::fs::create_dir_all(survival_dir.join("mods"));
    let _ = std::fs::create_dir_all(survival_dir.join("resourcepacks"));
    let _ = std::fs::create_dir_all(survival_dir.join("shaderpacks"));
    let _ = std::fs::create_dir_all(survival_dir.join("saves"));

    vec![Instance {
        id: Uuid::new_v4().to_string(),
        name: "Modded Survival".to_string(),
        minecraft_version: "1.21.1".to_string(),
        loader: Loader::Fabric,
        loader_version: Some("0.16.9".to_string()),
        java_major_version: Some(21),
        instance_dir: survival_dir.to_string_lossy().to_string(),
        installed: Vec::new(),
    }]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineReadiness {
    pub ready: bool,
    pub missing: Vec<String>,
}

/// Local-only readiness check: does the instance directory and each
/// tracked installed file still exist on disk. This never touches the
/// network, which is exactly what lets it run while offline.
pub fn check_offline_readiness(instance: &Instance) -> OfflineReadiness {
    let mut missing = Vec::new();
    let dir = PathBuf::from(&instance.instance_dir);

    if !dir.exists() {
        return OfflineReadiness {
            ready: false,
            missing: vec!["instance directory".to_string()],
        };
    }

    for content in &instance.installed {
        let subdir = content.content_type.install_subdir();
        let path = dir.join(subdir).join(&content.file_name);
        if !path.exists() {
            missing.push(content.file_name.clone());
        }
    }

    OfflineReadiness {
        ready: missing.is_empty(),
        missing,
    }
}

/// Copies a world folder to `<instance>/backups/<world>-<timestamp>`.
/// Purely local file copying, never touches the network, and never
/// modifies or deletes the original -- matching "never delete or modify
/// a world without explicit confirmation".
pub fn backup_world(instance: &Instance, folder_name: &str) -> Result<String, AppError> {
    // Reuse the same filename-safety rule as downloads: a folder name
    // must be a bare path segment, never `..` or an absolute path.
    if folder_name.is_empty()
        || folder_name.contains('/')
        || folder_name.contains('\\')
        || folder_name == ".."
    {
        return Err(AppError::PathTraversal(folder_name.to_string()));
    }

    let instance_dir = PathBuf::from(&instance.instance_dir);
    let source = instance_dir.join("saves").join(folder_name);
    if !source.join("level.dat").exists() {
        return Err(AppError::Internal("not a valid world folder".into()));
    }

    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let dest = instance_dir
        .join("backups")
        .join(format!("{folder_name}-{stamp}"));

    copy_dir_recursive(&source, &dest)?;
    Ok(dest.to_string_lossy().to_string())
}

fn copy_dir_recursive(src: &std::path::Path, dest: &std::path::Path) -> Result<(), AppError> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSummary {
    pub name: String,
    pub folder_name: String,
    pub save_path: String,
    pub last_played: Option<String>,
}

/// Scans `<instance>/saves` for real world folders (anything containing a
/// `level.dat`). Purely local filesystem access, so it works offline.
pub fn list_worlds(instance: &Instance) -> Vec<WorldSummary> {
    let saves_dir = PathBuf::from(&instance.instance_dir).join("saves");
    let mut worlds = Vec::new();

    let Ok(entries) = std::fs::read_dir(&saves_dir) else {
        return worlds;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("level.dat").exists() {
            continue;
        }
        let folder_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let last_played = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .map(|t| {
                let datetime: chrono::DateTime<Utc> = t.into();
                datetime.to_rfc3339()
            });

        worlds.push(WorldSummary {
            name: folder_name.clone(),
            folder_name,
            save_path: path.to_string_lossy().to_string(),
            last_played,
        });
    }

    worlds
}
