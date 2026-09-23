mod assets;
mod commands;
mod config;
mod document;
mod jobs;
mod models;
mod speech;
mod storage;

use commands::AppState;
use config::{AppSettings, AssetManifest};
use models::ProviderManager;
use std::sync::{Arc, RwLock};
use tauri::Manager;

pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Ok(credential_store) = windows_native_keyring_store::Store::new() {
                keyring_core::set_default_store(credential_store);
            }
            let data_dir = app.path().app_data_dir()?;
            let store = Arc::new(storage::Store::open(data_dir.clone())?);
            let assets = Arc::new(AssetManifest::load().map_err(anyhow::Error::msg)?);
            let mut preferences = store
                .load_app_settings(AppSettings::defaults(&data_dir))
                .map_err(anyhow::Error::msg)?;
            preferences.llama_server_path = data_dir
                .join("engines/llama/llama-server.exe")
                .to_string_lossy()
                .into_owned();
            if !preferences.gguf_model_path.trim().is_empty() {
                if store
                    .linked_models()
                    .map_err(anyhow::Error::msg)?
                    .is_empty()
                {
                    let _ =
                        store.link_gguf(std::path::Path::new(&preferences.gguf_model_path), None);
                }
                preferences.gguf_model_path.clear();
                store
                    .save_app_settings(&preferences)
                    .map_err(anyhow::Error::msg)?;
            }
            let settings = Arc::new(RwLock::new(preferences));
            let runtime_version = assets
                .by_kind("llama-engine")
                .map_err(anyhow::Error::msg)?
                .version
                .clone();
            let providers = Arc::new(
                ProviderManager::new(settings.clone(), store.clone(), runtime_version)
                    .map_err(anyhow::Error::msg)?,
            );
            app.manage(AppState {
                store,
                providers,
                settings,
                assets,
                data_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_collections,
            commands::create_collection,
            commands::list_scripts,
            commands::get_script,
            commands::create_script,
            commands::update_script,
            commands::duplicate_script,
            commands::set_script_deleted,
            commands::import_text,
            commands::export_text,
            commands::list_ambience,
            commands::import_ambience,
            commands::plan_timing,
            commands::list_text_models,
            commands::link_gguf,
            commands::unlink_gguf,
            commands::verify_gguf,
            commands::set_openrouter_key,
            commands::clear_openrouter_key,
            commands::openrouter_connected,
            commands::start_generation,
            commands::cancel_generation,
            commands::get_app_settings,
            commands::save_app_settings,
            commands::install_model_runtime,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("Hypnofy failed to start: {error}");
    }
}
