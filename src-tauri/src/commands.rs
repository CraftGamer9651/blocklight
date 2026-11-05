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
    /// The signed-in Microsoft account's Minecraft Services token, in
    /// memory only -- never written to disk. Needed to change the skin;
    /// empty after a restart until the user signs in again.
    pub mc_session: std::sync::Mutex<Option<crate::skin_core::McSession>>,
    /// Discord "Playing <instance>" status; see `presence.rs`.
    pub presence: Arc<crate::presence::PresenceManager>,
}

// ---------------------------------------------------------------------
// Instances
// ---------------------------------------------------------------------

#[tauri::command]
pub fn list_instances(state: State<AppState>) -> Vec<Instance> {
    state.instances.list()
}

#[tauri::command]
pub fn get_instance(state: State<AppState>, instance_id: String) -> Result<Instance, AppError> {
    state.instances.get(&instance_id)
}

/// Creates a new instance on disk. `loader` must be one of "fabric",
/// "forge", "neoforge", "quilt", "minecraft" -- validated here (rather
/// than reusing the lenient `Loader::parse`, which silently falls back
/// to `Minecraft` for anything unrecognized) so a typo in the frontend
/// surfaces as a clear error instead of a silently wrong loader.
#[tauri::command]
pub fn create_instance(
    state: State<AppState>,
    name: String,
    minecraft_version: String,
    loader: String,
    loader_version: Option<String>,
    java_major_version: Option<u32>,
    memory_mb: Option<u32>,
    jvm_args: Option<String>,
) -> Result<Instance, AppError> {
    let jvm_args = normalize_jvm_args(jvm_args)?;
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
        memory_mb,
        jvm_args,
    )
}

/// Validates custom JVM arguments and normalizes "nothing" to `None`
/// (blank/whitespace-only text clears the setting rather than storing
/// an empty string). Rejects with the specific reason -- unterminated
/// quote, a blocked flag, a stray word -- so the editor can show it.
fn normalize_jvm_args(raw: Option<String>) -> Result<Option<String>, AppError> {
    let Some(raw) = raw else { return Ok(None) };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    launch::user_args::parse(trimmed)
        .map_err(|reason| AppError::Internal(format!("Custom JVM arguments: {reason}")))?;
    Ok(Some(trimmed.to_string()))
}

/// Removes an instance *and deletes its folder from disk* (mods, worlds,
/// everything). Refuses while the game is running, and refuses to delete
/// any folder that isn't strictly inside Blocklight's own `instances`
/// directory. The folder is checked before the instance is dropped from
/// the list, so a refusal leaves everything exactly as it was.
#[tauri::command]
pub fn remove_instance(state: State<AppState>, instance_id: String) -> Result<(), AppError> {
    if state.processes.is_running(&instance_id) {
        return Err(AppError::Internal(
            "stop the game before removing this instance".into(),
        ));
    }

    let instance = state.instances.get(&instance_id)?;
    let root = crate::db::app_data_dir()?.join("instances");
    let to_delete = instances::deletable_instance_dir(&PathBuf::from(&instance.instance_dir), &root)
        .map_err(AppError::Internal)?;

    state.instances.remove(&instance_id)?;

    if let Some(dir) = to_delete {
        std::fs::remove_dir_all(&dir).map_err(|e| {
            AppError::Internal(format!(
                "the instance was removed, but its folder couldn't be fully deleted ({e}). \
                 You can delete it manually: {}",
                dir.display()
            ))
        })?;
    }
    Ok(())
}

/// Opens an instance's folder in the system file manager (Finder on
/// macOS). Done in the backend, looking the path up by instance id,
/// rather than letting the webview open an arbitrary path -- that keeps
/// the frontend's opener permissions narrow.
#[tauri::command]
pub fn open_instance_folder(
    app: tauri::AppHandle,
    state: State<AppState>,
    instance_id: String,
) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;

    let instance = state.instances.get(&instance_id)?;
    let dir = PathBuf::from(&instance.instance_dir);
    // Recreate it if it was deleted outside Blocklight, so the button
    // opens an (empty) folder instead of failing.
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| AppError::Internal(format!("couldn't open the instance folder: {e}")))
}

/// Saves (or clears, with `None`/blank) an instance's custom JVM
/// arguments. Takes effect on the next launch.
#[tauri::command]
pub fn set_instance_jvm_args(
    state: State<AppState>,
    instance_id: String,
    jvm_args: Option<String>,
) -> Result<Instance, AppError> {
    let jvm_args = normalize_jvm_args(jvm_args)?;
    state.instances.set_jvm_args(&instance_id, jvm_args)
}

// ---------------------------------------------------------------------
// Link import
// ---------------------------------------------------------------------

/// Cheap, offline, synchronous validation used for instant "unsupported
/// link" / "invalid link" feedback as the user types or pastes, before
/// any network request is made.
#[tauri::command]
pub fn validate_link(url: String) -> Result<serde_json::Value, AppError> {
    let resolved = url_resolver::resolve(&url)?;
    Ok(serde_json::json!({
        "platform": resolved.platform,
        "contentType": resolved.content_type,
    }))
}

/// Runs the full URL Resolution pipeline (spec steps 1-9): validate,
/// identify platform/project/content type, fetch versions, pick the best
/// compatible one, check for an existing install, and resolve
/// dependencies -- everything the Link Preview card needs, without
/// downloading or installing anything yet.
#[tauri::command]
pub async fn preview_link_install(
    state: State<'_, AppState>,
    url: String,
    instance_id: String,
) -> Result<ResolvedInstall, AppError> {
    if !state.network.is_online() {
        return Err(AppError::Offline);
    }

    let instance = state.instances.get(&instance_id)?;
    let resolved_link = url_resolver::resolve(&url)?;

    let registry = &state.providers;
    let provider = registry.for_platform(resolved_link.platform);

    let project = provider.get_project(&resolved_link.project_ref).await?;

    let compat = if let Some(version_ref) = &resolved_link.version_ref {
        let version = provider.get_version(&project, version_ref).await?;
        compatibility::evaluate(&instance, &[version])
    } else {
        let versions = provider.list_versions(&project).await?;
        compatibility::evaluate(&instance, &versions)
    };

    let already_installed = instance
        .installed
        .iter()
        .find(|c| c.project_id == project.id)
        .cloned();

    let dependency_checks = if let Some(version) = &compat.selected_version {
        dependencies::resolve_all(registry, &instance, &version.dependencies).await
    } else {
        Vec::new()
    };

    Ok(ResolvedInstall {
        project,
        instance_id,
        compatibility: compat,
        already_installed,
        dependencies: dependency_checks,
    })
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSummary {
    pub installed: Vec<InstallHistoryEntry>,
    pub instance: Instance,
}

/// Re-resolves the link server-side (rather than trusting whatever the
/// frontend cached from the preview call) and, only if a compatible
/// version is confirmed, downloads the main file and every required
/// dependency, then re-runs the local readiness check.
#[tauri::command]
pub async fn confirm_link_install(
    state: State<'_, AppState>,
    url: String,
    instance_id: String,
) -> Result<InstallSummary, AppError> {
    if !state.network.is_online() {
        return Err(AppError::Offline);
    }

    let instance = state.instances.get(&instance_id)?;
    let resolved_link = url_resolver::resolve(&url)?;

    let registry = &state.providers;
    let provider = registry.for_platform(resolved_link.platform);
    let project = provider.get_project(&resolved_link.project_ref).await?;

    let compat = if let Some(version_ref) = &resolved_link.version_ref {
        let version = provider.get_version(&project, version_ref).await?;
        compatibility::evaluate(&instance, &[version])
    } else {
        let versions = provider.list_versions(&project).await?;
        compatibility::evaluate(&instance, &versions)
    };

    let version = compat
        .selected_version
        .clone()
        .ok_or(AppError::NoCompatibleVersion)?;

    let dependency_checks = dependencies::resolve_all(registry, &instance, &version.dependencies).await;
    dependencies::all_compatible(&dependency_checks)?;

    let instance_dir = PathBuf::from(&instance.instance_dir);
    let mut installed_entries = Vec::new();

    // Dependencies first, so the main file's requirements are satisfied
    // before it's placed in the instance.
    for check in dependency_checks.iter().filter(|c| !c.already_installed) {
        let (Some(dep_project), Some(dep_version)) =
            (&check.resolved_project, &check.resolved_version)
        else {
            continue;
        };
        let Some(file) = dep_version.primary_file() else {
            continue;
        };

        downloader::download_file(&state.http, file, &instance_dir, dep_project.content_type)
            .await?;

        state.instances.record_installed(
            &instance_id,
            InstalledContent {
                platform: dep_project.platform,
                project_id: dep_project.id.clone(),
                project_slug: dep_project.slug.clone(),
                content_type: dep_project.content_type,
                version_id: dep_version.id.clone(),
                version_number: dep_version.version_number.clone(),
                file_name: file.filename.clone(),
            },
        )?;

        let entry = {
            let conn = state.db.0.lock().unwrap();
            history::record(
                &conn,
                &dep_project.name,
                dep_project.platform,
                dep_project.content_type,
                &dep_version.version_number,
                &instance_id,
            )?
        };
        installed_entries.push(entry);
    }

    let main_file = version
        .primary_file()
        .ok_or_else(|| AppError::Internal("selected version has no downloadable file".into()))?;

    downloader::download_file(&state.http, main_file, &instance_dir, project.content_type).await?;

    state.instances.record_installed(
        &instance_id,
        InstalledContent {
            platform: project.platform,
            project_id: project.id.clone(),
            project_slug: project.slug.clone(),
            content_type: project.content_type,
            version_id: version.id.clone(),
            version_number: version.version_number.clone(),
            file_name: main_file.filename.clone(),
        },
    )?;

    let entry = {
        let conn = state.db.0.lock().unwrap();
        history::record(
            &conn,
            &project.name,
            project.platform,
            project.content_type,
            &version.version_number,
            &instance_id,
        )?
    };
    installed_entries.push(entry);

    let refreshed_instance = state.instances.get(&instance_id)?;

    Ok(InstallSummary {
        installed: installed_entries,
        instance: refreshed_instance,
    })
}

#[tauri::command]
pub async fn search_content(
    state: State<'_, AppState>,
    query: String,
    platform: Option<Platform>,
) -> Result<Vec<Project>, AppError> {
    if !state.network.is_online() {
        return Err(AppError::Offline);
    }
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let registry = &state.providers;
    let platforms = match platform {
        Some(p) => vec![p],
        None => vec![Platform::Modrinth, Platform::CurseForge],
    };

    let mut results = Vec::new();
    for p in platforms {
        let provider = registry.for_platform(p);
        match provider.search(&query, 10).await {
            Ok(mut hits) => results.append(&mut hits),
            // CurseForge without a configured API key shouldn't break a
            // Modrinth-only search; surface it as simply no results from
            // that platform.
            Err(AppError::MissingCurseForgeKey) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(results)
}

/// Modrinth's "most downloaded" list for one content type, with no
/// search term needed -- what powers each library page's default "Top
/// 10" browse view before the person types anything.
#[tauri::command]
pub async fn list_trending_modrinth(
    state: State<'_, AppState>,
    content_type: ContentType,
    limit: Option<u32>,
) -> Result<Vec<Project>, AppError> {
    if !state.network.is_online() {
        return Err(AppError::Offline);
    }
    state
        .providers
        .modrinth()
        .list_trending(content_type, limit.unwrap_or(12))
        .await
}

// ---------------------------------------------------------------------
// Install history
// ---------------------------------------------------------------------

#[tauri::command]
pub fn list_install_history(
    state: State<AppState>,
    limit: Option<u32>,
) -> Result<Vec<InstallHistoryEntry>, AppError> {
    let conn = state.db.0.lock().unwrap();
    history::list(&conn, limit.unwrap_or(50))
}

#[tauri::command]
pub fn clear_install_history(state: State<AppState>) -> Result<(), AppError> {
    let conn = state.db.0.lock().unwrap();
    history::clear(&conn)
}

// ---------------------------------------------------------------------
// Connectivity / offline mode
// ---------------------------------------------------------------------

#[tauri::command]
pub fn get_connectivity(state: State<AppState>) -> ConnectivityState {
    state.network.state()
}

#[tauri::command]
pub fn check_offline_readiness(
    state: State<AppState>,
    instance_id: String,
) -> Result<OfflineReadiness, AppError> {
    let instance = state.instances.get(&instance_id)?;
    Ok(instances::check_offline_readiness(&instance))
}

#[tauri::command]
pub fn list_worlds(
    state: State<AppState>,
    instance_id: String,
) -> Result<Vec<WorldSummary>, AppError> {
    let instance = state.instances.get(&instance_id)?;
    Ok(instances::list_worlds(&instance))
}

/// Explicit, user-triggered only -- never runs automatically. Copies the
/// world rather than touching the original, so there's no way this
/// accidentally deletes or corrupts a save.
#[tauri::command]
pub fn backup_world(
    state: State<AppState>,
    instance_id: String,
    folder_name: String,
) -> Result<String, AppError> {
    let instance = state.instances.get(&instance_id)?;
    instances::backup_world(&instance, &folder_name)
}

// ---------------------------------------------------------------------
// Launching
// ---------------------------------------------------------------------

/// Downloads whatever's missing (client jar, libraries, natives, assets
/// -- already-present files are skipped after a hash check, which is
/// what lets a fully-prepared instance launch with no network at all),
/// resolves a Java runtime, builds the JVM/game arguments, and spawns
/// the game. Returns once the process has *started*, not once it exits
/// -- ongoing output arrives via `game-log` events, and `game-exited`
/// fires when it terminates.
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

    // Parse custom JVM args first: a bad value should fail in a second
    // with its specific reason, not after minutes of downloading.
    let user_jvm_args = match instance.jvm_args.as_deref() {
        Some(raw) => launch::user_args::parse(raw)
            .map_err(|reason| AppError::Internal(format!("Custom JVM arguments: {reason}")))?,
        None => Vec::new(),
    };

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
            // A real online launch needs the live Minecraft Services
            // access token and profile uuid from the sign-in exchange
            // in msa.rs. Blocklight's account state only persists
            // *whether* sign-in succeeded and the gamertag (profiles.rs)
            // -- deliberately not the token itself, which is short-lived
            // and was never given anywhere durable to live. Wiring that
            // through (plus refreshing it when expired) is a real, but
            // separate, follow-up; for now online accounts are asked to
            // launch from an offline profile instead of silently being
            // downgraded to one.
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
    let built = launch::args::build_command(&version, &prepared, &game_dir, &identity, instance.memory_mb, &user_jvm_args);

    // Register the Discord status *before* spawning, so a game that dies
    // instantly can't have its exit processed before its start (which
    // would leave a stale status behind). Rolled back if the spawn fails.
    let detail = if instance.loader == crate::models::Loader::Minecraft {
        format!("Minecraft {}", instance.minecraft_version)
    } else {
        format!("{} {}", instance.loader.label(), instance.minecraft_version)
    };
    state
        .presence
        .game_started(instance_id.clone(), instance.name.clone(), detail);

    let presence = state.presence.clone();
    let exit_id = instance_id.clone();
    let on_exit: Box<dyn FnOnce() + Send + 'static> = Box::new(move || presence.game_exited(&exit_id));

    let spawned = launch::process::spawn(app, state.processes.clone(), instance_id.clone(), &java, built, on_exit);
    if spawned.is_err() {
        state.presence.game_exited(&instance_id);
    }
    spawned
}

#[tauri::command]
pub fn stop_instance(state: State<AppState>, instance_id: String) -> Result<(), AppError> {
    state.processes.stop(&instance_id)
}

#[tauri::command]
pub fn is_instance_running(state: State<AppState>, instance_id: String) -> bool {
    state.processes.is_running(&instance_id)
}

// ---------------------------------------------------------------------
// Accounts / profiles
// ---------------------------------------------------------------------

#[tauri::command]
pub fn get_account_state(state: State<AppState>) -> Result<AccountState, AppError> {
    let conn = state.db.0.lock().unwrap();
    profiles::get_account_state(&conn)
}

#[tauri::command]
pub fn create_offline_profile(
    state: State<AppState>,
    display_name: String,
) -> Result<OfflineProfile, AppError> {
    let conn = state.db.0.lock().unwrap();
    profiles::create_offline_profile(&conn, &display_name)
}

#[tauri::command]
pub fn list_offline_profiles(state: State<AppState>) -> Result<Vec<OfflineProfile>, AppError> {
    let conn = state.db.0.lock().unwrap();
    profiles::list_offline_profiles(&conn)
}

/// Switches to a specific offline profile by id -- see
/// `profiles::switch_to_offline_profile` for why this needs the id at
/// all (the earlier version of this command didn't take one, which was
/// the actual bug: switching between two offline profiles did nothing).
#[tauri::command]
pub fn activate_offline_profile(
    state: State<AppState>,
    profile_id: String,
) -> Result<AccountState, AppError> {
    let conn = state.db.0.lock().unwrap();
    profiles::switch_to_offline_profile(&conn, &profile_id)?;
    profiles::get_account_state(&conn)
}

/// Deletes a local offline profile. Never touches Microsoft sign-in or
/// any other profile beyond falling back to one if the deleted profile
/// was the active one (see `profiles::delete_offline_profile`).
#[tauri::command]
pub fn delete_offline_profile(
    state: State<AppState>,
    profile_id: String,
) -> Result<AccountState, AppError> {
    let conn = state.db.0.lock().unwrap();
    profiles::delete_offline_profile(&conn, &profile_id)?;
    profiles::get_account_state(&conn)
}

/// Only ever switches the *active* profile; never signs the user out of
/// Microsoft, and never fabricates a Microsoft session that didn't come
/// from `sign_in_with_microsoft`.
#[tauri::command]
pub fn switch_to_microsoft(state: State<AppState>) -> Result<AccountState, AppError> {
    let conn = state.db.0.lock().unwrap();
    let current = profiles::get_account_state(&conn)?;
    if !current.microsoft_signed_in {
        return Err(AppError::Internal(
            "sign in with Microsoft first".to_string(),
        ));
    }
    profiles::set_active_account(&conn, ActiveAccountKind::Microsoft)?;
    profiles::get_account_state(&conn)
}

/// Runs the entire device-code sign-in in one call: requests a code,
/// emits `msa-prompt` so the UI can show "go to microsoft.com/link and
/// enter THIS-CODE", then polls until the user finishes in their browser
/// (or the code expires/is declined). The device code itself never
/// leaves this function, let alone the frontend.
#[tauri::command]
pub async fn sign_in_with_microsoft(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AccountState, AppError> {
    use tauri::Emitter;

    if !state.network.is_online() {
        return Err(AppError::Offline);
    }

    let prompt = msa::begin_device_code_sign_in(&state.http).await?;
    let _ = app.emit("msa-prompt", msa::MsaPrompt::from(&prompt));

    let signed_in = msa::poll_and_complete(&state.http, &prompt).await?;

    // Ownership verification must actually gate the sign-in, not just be
    // computed and ignored: an account without a Minecraft entitlement
    // is refused here rather than being let in as if it were valid.
    if !signed_in.owns_minecraft {
        return Err(AppError::MinecraftNotOwned);
    }

    // Keep the live token in memory so account actions that need it (like
    // changing the skin) work for the rest of this run. Not persisted.
    *state.mc_session.lock().unwrap() = Some(crate::skin_core::McSession::new(
        signed_in.mc_access_token,
        signed_in.mc_expires_in,
    ));

    let conn = state.db.0.lock().unwrap();
    profiles::set_microsoft_session(&conn, true, Some(&signed_in.gamertag))?;
    profiles::set_active_account(&conn, ActiveAccountKind::Microsoft)?;
    let mut account_state = profiles::get_account_state(&conn)?;
    account_state.microsoft_gamertag = Some(signed_in.gamertag);
    Ok(account_state)
}

// ---------------------------------------------------------------------
// Discord status
// ---------------------------------------------------------------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordPresenceStatus {
    /// The user's choice (off by default).
    pub enabled: bool,
    /// Whether there's a usable Discord application id right now.
    pub configured: bool,
    /// When not configured, a sentence saying why (nothing set, or set to
    /// something that isn't a Discord Application ID).
    pub config_problem: Option<String>,
}

fn discord_status(state: &AppState) -> Result<DiscordPresenceStatus, AppError> {
    let conn = state.db.0.lock().unwrap();
    Ok(DiscordPresenceStatus {
        enabled: profiles::get_discord_presence_enabled(&conn)?,
        configured: state.presence.is_configured(),
        config_problem: state.presence.config_problem(),
    })
}

#[tauri::command]
pub fn get_discord_presence_status(
    state: State<AppState>,
) -> Result<DiscordPresenceStatus, AppError> {
    discord_status(&state)
}

/// Saves the choice and applies it immediately -- turning it on while a
/// game is running shows it right away, turning it off clears it.
#[tauri::command]
pub fn set_discord_presence_enabled(
    state: State<AppState>,
    enabled: bool,
) -> Result<DiscordPresenceStatus, AppError> {
    {
        let conn = state.db.0.lock().unwrap();
        profiles::set_discord_presence_enabled(&conn, enabled)?;
    }
    state.presence.set_enabled(enabled);
    discord_status(&state)
}

// ---------------------------------------------------------------------
// Skin
// ---------------------------------------------------------------------

/// Whether the active account can change its skin right now, and if not,
/// why. Offline profiles can't (a skin belongs to a Minecraft account);
/// a Microsoft account can once this run has a live token.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinStatus {
    pub can_change: bool,
    pub needs_sign_in: bool,
    pub reason: Option<String>,
}

fn has_live_session(state: &AppState) -> bool {
    state
        .mc_session
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.token().is_some())
}

#[tauri::command]
pub fn get_skin_status(state: State<AppState>) -> Result<SkinStatus, AppError> {
    let account = {
        let conn = state.db.0.lock().unwrap();
        profiles::get_account_state(&conn)?
    };

    if account.active_kind != ActiveAccountKind::Microsoft {
        return Ok(SkinStatus {
            can_change: false,
            needs_sign_in: false,
            reason: Some(
                "Offline profiles don't have a Minecraft skin to change — skins belong to a \
                 Microsoft account. Switch to your Microsoft account to change yours."
                    .to_string(),
            ),
        });
    }

    if has_live_session(&state) {
        Ok(SkinStatus { can_change: true, needs_sign_in: false, reason: None })
    } else {
        Ok(SkinStatus {
            can_change: false,
            needs_sign_in: true,
            reason: Some(
                "Blocklight doesn't keep your Microsoft login between sessions. Sign in again \
                 to change your skin."
                    .to_string(),
            ),
        })
    }
}

/// Downloads the skin behind a SkinMC link so the UI can preview it
/// before anything is applied. Returns the PNG bytes.
#[tauri::command]
pub async fn fetch_skin_from_link(
    state: State<'_, AppState>,
    link: String,
) -> Result<Vec<u8>, AppError> {
    if !state.network.is_online() {
        return Err(AppError::Offline);
    }
    let id = crate::skin_core::parse_skinmc_link(&link).map_err(AppError::InvalidLink)?;
    crate::skins::fetch_skinmc_texture(&state.http, &id).await
}

/// Validates a skin file chosen in the UI (PNG, 64x64 or 64x32) without
/// applying it, so a bad file is rejected before the preview step.
#[tauri::command]
pub fn inspect_skin_png(bytes: Vec<u8>) -> Result<(), AppError> {
    crate::skin_core::inspect_png(&bytes)
        .map(|_| ())
        .map_err(AppError::InvalidLink)
}

/// Uploads `bytes` as the active Microsoft account's skin. Everything is
/// re-checked here rather than trusting the UI: the file is re-validated,
/// the active account must be the Microsoft one, and a live, unexpired
/// token must exist (otherwise `SignInRequired`).
#[tauri::command]
pub async fn apply_skin(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
    variant: String,
) -> Result<(), AppError> {
    if !state.network.is_online() {
        return Err(AppError::Offline);
    }

    crate::skin_core::inspect_png(&bytes).map_err(AppError::InvalidLink)?;
    let variant = crate::skin_core::SkinVariant::parse(&variant)
        .ok_or_else(|| AppError::Internal(format!("unknown skin model: {variant}")))?;

    let account = {
        let conn = state.db.0.lock().unwrap();
        profiles::get_account_state(&conn)?
    };
    if account.active_kind != ActiveAccountKind::Microsoft {
        return Err(AppError::Internal(
            "only a Microsoft account has a skin to change; switch to it first".into(),
        ));
    }

    // Copy the token out so the lock isn't held across the network call.
    let token = state
        .mc_session
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|s| s.token().map(str::to_string))
        .ok_or(AppError::SignInRequired)?;

    let result = crate::skins::upload_skin(&state.http, &token, &bytes, variant).await;
    if matches!(result, Err(AppError::SignInRequired)) {
        // The server says this token is no good; drop it so the UI asks
        // for a fresh sign-in instead of retrying with a dead token.
        *state.mc_session.lock().unwrap() = None;
    }
    result
}

// ---------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------

#[tauri::command]
pub fn set_curseforge_api_key(state: State<AppState>, api_key: Option<String>) {
    state.providers.set_curseforge_api_key(api_key);
}

#[tauri::command]
pub fn has_curseforge_api_key(state: State<AppState>) -> bool {
    state.providers.has_curseforge_api_key()
}
