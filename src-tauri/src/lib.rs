mod audio;
mod assets;
mod commands;
mod config;
mod providers;
mod speech;
mod storage;

use commands::AppState;
use config::{AppSettings, AssetManifest};
use providers::ProviderManager;
use speech::SpeechManager;
use std::sync::{Arc, RwLock};
use tauri::Manager;

pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let store = storage::Store::open(data_dir.clone())?;
            let assets = Arc::new(AssetManifest::load().map_err(anyhow::Error::msg)?);
            let preferences = store
                .load_app_settings(AppSettings::defaults(&data_dir))
                .map_err(anyhow::Error::msg)?;
            let settings = Arc::new(RwLock::new(preferences));
            let providers =
                Arc::new(ProviderManager::new(settings.clone()).map_err(anyhow::Error::msg)?);
            let speech = Arc::new(
                SpeechManager::new(data_dir.clone(), &assets).map_err(anyhow::Error::msg)?,
            );
            app.manage(AppState {
                store,
                providers,
                speech,
                audio: audio::AudioManager::new(),
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
            commands::list_text_models,
            commands::start_generation,
            commands::cancel_generation,
            commands::speech_status,
            commands::list_voices,
            commands::install_voice_pack,
            commands::start_synthesis,
            commands::cancel_synthesis,
            commands::start_playback,
            commands::toggle_playback,
            commands::append_playback,
            commands::stop_playback,
            commands::seek_playback,
            commands::set_playback_volumes,
            commands::player_state,
            commands::get_app_settings,
            commands::save_app_settings,
            commands::install_model_runtime,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("Hypnofy failed to start: {error}");
    }
}
