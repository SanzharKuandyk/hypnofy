use crate::{
    audio::{AudioManager, PlayerState},
    config::{AppSettings, AssetManifest, SettingsView},
    providers::{GenerationRequest, ProviderManager, TextModel},
    speech::{SpeechManager, SpeechStatus, SynthesisRequest, Voice},
    storage::{Ambience, Collection, Script, ScriptPatch, ScriptSummary, Store},
};
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tauri::State;

pub struct AppState {
    pub store: Store,
    pub providers: Arc<ProviderManager>,
    pub speech: Arc<SpeechManager>,
    pub audio: AudioManager,
    pub settings: Arc<RwLock<AppSettings>>,
    pub assets: Arc<AssetManifest>,
    pub data_dir: PathBuf,
}

#[tauri::command]
pub fn list_collections(state: State<AppState>) -> Result<Vec<Collection>, String> {
    state.store.collections(false)
}
#[tauri::command]
pub fn create_collection(state: State<AppState>, name: String) -> Result<Collection, String> {
    state.store.create_collection(&name)
}
#[tauri::command]
pub fn list_scripts(
    state: State<AppState>,
    collection_id: Option<String>,
    query: String,
    favorite: bool,
    trash: bool,
) -> Result<Vec<ScriptSummary>, String> {
    state
        .store
        .list_scripts(collection_id.as_deref(), &query, favorite, trash)
}
#[tauri::command]
pub fn get_script(state: State<AppState>, id: String) -> Result<Script, String> {
    state.store.get_script(&id)
}
#[tauri::command]
pub fn create_script(
    state: State<AppState>,
    collection_id: Option<String>,
    title: Option<String>,
    body: Option<String>,
) -> Result<Script, String> {
    state.store.create_script(
        collection_id.as_deref(),
        title.as_deref(),
        body.as_deref().unwrap_or_default(),
    )
}
#[tauri::command]
pub fn update_script(state: State<AppState>, patch: ScriptPatch) -> Result<Script, String> {
    state.store.update_script(patch)
}
#[tauri::command]
pub fn duplicate_script(state: State<AppState>, id: String) -> Result<Script, String> {
    state.store.duplicate_script(&id)
}
#[tauri::command]
pub fn set_script_deleted(state: State<AppState>, id: String, deleted: bool) -> Result<(), String> {
    state.store.set_deleted(&id, deleted)
}
#[tauri::command]
pub fn import_text(
    state: State<AppState>,
    path: String,
    collection_id: Option<String>,
) -> Result<Script, String> {
    state
        .store
        .import_text(&PathBuf::from(path), collection_id.as_deref())
}
#[tauri::command]
pub fn export_text(state: State<AppState>, id: String, path: String) -> Result<(), String> {
    state.store.export_text(&id, &PathBuf::from(path))
}
#[tauri::command]
pub fn list_ambience(state: State<AppState>) -> Result<Vec<Ambience>, String> {
    state.store.ambience()
}
#[tauri::command]
pub fn import_ambience(state: State<AppState>, path: String) -> Result<Ambience, String> {
    state.store.import_ambience(&PathBuf::from(path))
}
#[tauri::command]
pub async fn list_text_models(state: State<'_, AppState>) -> Result<Vec<TextModel>, String> {
    Ok(state.providers.models().await)
}
#[tauri::command]
pub async fn install_model_runtime(state: State<'_, AppState>) -> Result<(), String> {
    let asset = state.assets.by_kind("llama-engine")?;
    let executable = crate::assets::install(&state.data_dir, asset).await?;
    let mut settings = state.settings.read().map_err(|_| "Application settings are unavailable")?.clone();
    settings.llama_server_path = executable.to_string_lossy().into_owned();
    state.store.save_app_settings(&settings)?;
    *state.settings.write().map_err(|_| "Application settings are unavailable")? = settings;
    Ok(())
}
#[tauri::command]
pub fn start_generation(
    app: tauri::AppHandle,
    state: State<AppState>,
    request: GenerationRequest,
) -> Result<String, String> {
    state.providers.clone().start(app, request)
}
#[tauri::command]
pub fn cancel_generation(state: State<AppState>, job_id: String) {
    state.providers.cancel(&job_id)
}
#[tauri::command]
pub fn speech_status(state: State<AppState>) -> SpeechStatus {
    state.speech.status()
}
#[tauri::command]
pub fn list_voices(state: State<AppState>) -> Vec<Voice> {
    state.speech.voices()
}
#[tauri::command]
pub fn install_voice_pack(app: tauri::AppHandle, state: State<AppState>) -> Result<String, String> {
    state.speech.clone().install(app)
}
#[tauri::command]
pub fn start_synthesis(
    app: tauri::AppHandle,
    state: State<AppState>,
    request: SynthesisRequest,
) -> Result<String, String> {
    state.speech.clone().synthesize(app, request)
}
#[tauri::command]
pub fn cancel_synthesis(state: State<AppState>, job_id: String) {
    state.speech.cancel(&job_id)
}
#[tauri::command]
pub fn start_playback(
    state: State<AppState>,
    manifest_path: String,
    ambience_id: Option<String>,
    narration_volume: f32,
    ambience_volume: f32,
) -> Result<PlayerState, String> {
    let ambience = ambience_id
        .as_deref()
        .map(|id| state.store.ambience_path(id))
        .transpose()?;
    state.audio.start(
        &PathBuf::from(manifest_path),
        ambience.as_deref(),
        narration_volume,
        ambience_volume,
    )
}
#[tauri::command]
pub fn toggle_playback(state: State<AppState>) -> Result<PlayerState, String> {
    state.audio.toggle()
}
#[tauri::command]
pub fn append_playback(state: State<AppState>, manifest_path: String) -> Result<PlayerState, String> {
    state.audio.append(&PathBuf::from(manifest_path))
}
#[tauri::command]
pub fn stop_playback(state: State<AppState>) -> Result<(), String> {
    state.audio.stop()
}
#[tauri::command]
pub fn seek_playback(state: State<AppState>, seconds: f32) -> Result<PlayerState, String> {
    state.audio.seek(seconds)
}
#[tauri::command]
pub fn set_playback_volumes(
    state: State<AppState>,
    narration: f32,
    ambience: f32,
) -> Result<(), String> {
    state.audio.volumes(narration, ambience)
}
#[tauri::command]
pub fn player_state(state: State<AppState>) -> Result<PlayerState, String> {
    state.audio.state()
}
#[tauri::command]
pub fn get_app_settings(state: State<AppState>) -> Result<SettingsView, String> {
    let preferences = state
        .settings
        .read()
        .map_err(|_| "Application settings are unavailable")?
        .clone();
    Ok(SettingsView {
        preferences,
        data_directory: state.data_dir.to_string_lossy().into_owned(),
        assets: state.assets.assets.clone(),
    })
}
#[tauri::command]
pub fn save_app_settings(
    state: State<AppState>,
    settings: AppSettings,
) -> Result<SettingsView, String> {
    settings.validate()?;
    state.store.save_app_settings(&settings)?;
    *state
        .settings
        .write()
        .map_err(|_| "Application settings are unavailable")? = settings;
    get_app_settings(state)
}
