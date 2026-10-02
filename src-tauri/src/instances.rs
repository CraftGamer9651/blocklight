use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::app_data_dir;
use crate::error::AppError;
use crate::models::{InstalledContent, Instance, Loader};

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

    pub fn update_jvm_arguments(
        &self,
        instance_id: &str,
        custom_jvm_arguments: Vec<String>,
    ) -> Result<Instance, AppError> {
        let cleaned: Vec<String> = custom_jvm_arguments
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();

        {
            let mut guard = self.instances.lock().unwrap();
            let instance = guard
                .iter_mut()
                .find(|i| i.id == instance_id)
                .ok_or_else(|| AppError::Internal(format!("unknown instance: {instance_id}")))?;

            instance.custom_jvm_arguments = cleaned.clone();
        }

        self.persist()?;
        self.get(instance_id)
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
            custom_jvm_arguments: Vec::new(),
        };

        self.instances.lock().unwrap().push(instance.clone());
        self.persist()?;

        Ok(instance)
    }

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

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = true;
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
        custom_jvm_arguments: Vec::new(),
    }]
}
