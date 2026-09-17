mod commands;
mod compatibility;
mod db;
mod dependencies;
mod downloader;
mod error;
mod history;
mod instances;
mod launch;
mod models;
mod msa;
mod network_monitor;
mod profiles;
mod providers;
mod url_resolver;

use std::sync::Arc;

use commands::AppState;
use instances::InstanceStore;
use launch::process::RunningProcesses;
use network_monitor::NetworkMonitor;
use providers::ProviderRegistry;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let db = db::open().expect("failed to open local database");

            let curseforge_key = std::env::var("BLOCKLIGHT_CURSEFORGE_API_KEY").ok();
            let providers = ProviderRegistry::new(curseforge_key);

            let network = NetworkMonitor::new();
            network.start(app.handle().clone());

            let instance_store = InstanceStore::load_or_seed().expect("failed to load instances");

            let http = reqwest::Client::builder()
                .user_agent("Blocklight/0.1.0 (+https://craftgamer9651.github.io/)")
                .build()
                .expect("failed to build http client");

            app.manage(AppState {
                db,
                providers,
                network,
                instances: instance_store,
                http,
                processes: Arc::new(RunningProcesses::new()),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_instances,
            commands::get_instance,
            commands::create_instance,
            commands::validate_link,
            commands::preview_link_install,
            commands::confirm_link_install,
            commands::search_content,
            commands::list_trending_modrinth,
            commands::list_install_history,
            commands::clear_install_history,
            commands::get_connectivity,
            commands::check_offline_readiness,
            commands::list_worlds,
            commands::backup_world,
            commands::launch_instance,
            commands::stop_instance,
            commands::is_instance_running,
            commands::get_account_state,
            commands::create_offline_profile,
            commands::list_offline_profiles,
            commands::activate_offline_profile,
            commands::delete_offline_profile,
            commands::switch_to_microsoft,
            commands::sign_in_with_microsoft,
            commands::set_curseforge_api_key,
            commands::has_curseforge_api_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Blocklight");
}
