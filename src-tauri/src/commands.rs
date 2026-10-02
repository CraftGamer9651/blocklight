use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use crate::compatibility;
use crate::db::Db;
use crate::dependencies;
use crate::downloader;
use crate::error::AppError;
use crate::history;
use crate::instances::{self, InstanceStore, OfflineReadiness, WorldSummary};
use crate::launch::{self, process::RunningProcesses};
use crate::models::{
    AccountState, ActiveAccountKind, ConnectivityState, ContentType, Instance,
    InstallHistoryEntry, InstalledContent, Loader, OfflineProfile, Platform, Project,
    ResolvedInstall,
};
use crate::msa;
use crate::network_monitor::NetworkMonitor;
use crate::profiles;
use crate::providers::ProviderRegistry;
use crate::url_resolver;

pub struct AppState {
    pub db: Db,
    pub providers: ProviderRegistry,
    pub network: NetworkMonitor,
    pub instances: InstanceStore,
    pub http: reqwest::Client,
    pub processes: Arc<RunningProcesses>,
}

#[tauri::command]
pub fn list_instances(state: State<AppState>) -> Vec<Instance> {
    state.instances.list()
}

#[tauri::command]
pub fn get_instance(state: State<AppState>, instance_id: String) -> Result<Instance, AppError> {
    state.instances.get(&instance_id)
}

#[tauri::command]
pub fn create_instance(
    state: State<AppState>,
    name: String,
    minecraft_version: String,
    loader: String,
    loader_version: Option<String>,
    java_major_version: Option<u32>,
) -> Result<Instance, AppError> {
    let parsed_loader = match loader.to_ascii_lowercase().as_str() {
        "fabric" => Loader::Fabric,
        "forge" => Loader::Forge,
        "neoforge" => Loader::NeoForge,
        "quilt" => Loader::Quilt,
        "minecraft" => Loader::Minecraft,
        other => return Err(AppError::Internal(format!("unknown loader: {other}"))),
    };

    state.instances.create(
        &name,
        &minecraft_version,
        parsed_loader,
        loader_version.filter(|v| !v.trim().is_empty()),
        java_major_version,
    )
}

#[tauri::command]
pub fn set_instance_jvm_arguments(
    state: State<AppState>,
    instance_id: String,
    args: Vec<String>,
) -> Result<Instance, AppError> {
    state.instances.update_jvm_arguments(&instance_id, args)
}

// ... rest of file unchanged ...

#[tauri::command]
pub async fn launch_instance(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<(), AppError> {
    if state.processes.is_running(&instance_id) {
        return Err(AppError::Internal("this instance is already running".into()));
    }

    let instance = state.instances.get(&instance_id)?;

    let account = {
        let conn = state.db.0.lock().unwrap();
        profiles::get_account_state(&conn)?
    };

    let identity = match account.active_kind {
        ActiveAccountKind::Offline => {
            let name = account
                .offline_profile
                .map(|p| p.display_name)
                .unwrap_or_else(|| "Player".to_string());
            launch::args::offline_identity(&name)
        }
        ActiveAccountKind::Microsoft => {
            return Err(AppError::Internal(
                "Launching with a signed-in Microsoft account isn't wired up yet. Switch to an \
                 offline profile in Settings to launch this instance."
                    .into(),
            ));
        }
    };

    let java = launch::java::require_java(instance.java_major_version).await?;
    let version = launch::resolve_version_json(&state.http, &app, &instance_id, &java, &instance).await?;
    let profile_key = launch::profile_key(&instance);
    let prepared = launch::download::prepare(&app, &state.http, &instance_id, &profile_key, &version).await?;
    let game_dir = PathBuf::from(&instance.instance_dir);
    let built = launch::args::build_command(
        &version,
        &prepared,
        &game_dir,
        &identity,
        &instance.custom_jvm_arguments,
    );

    launch::process::spawn(app, state.processes.clone(), instance_id, &java, built)
}
