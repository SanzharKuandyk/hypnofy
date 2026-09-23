use crate::{
    config::{AppSettings, AssetManifest, SettingsView},
    models::{GenerationRequest, ModelStatus, ProviderManager, TextModel},
    storage::{Ambience, Collection, LinkedModel, Script, ScriptPatch, ScriptSummary, Store},
};
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tauri::State;

pub struct AppState {
    pub store: Arc<Store>,
    pub providers: Arc<ProviderManager>,
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
pub fn plan_timing(
    document: serde_json::Value,
    paragraph_pause: f32,
) -> Result<Vec<crate::speech::TimelineSegment>, String> {
    if !paragraph_pause.is_finite() || !(0.0..=30.0).contains(&paragraph_pause) {
        return Err("Paragraph pause must be between 0 and 30 seconds".into());
    }
    crate::speech::timing_plan(&document, paragraph_pause)
}
#[tauri::command]
pub async fn list_text_models(state: State<'_, AppState>) -> Result<Vec<TextModel>, String> {
    state.providers.models().await
}
#[tauri::command]
pub fn link_gguf(
    state: State<AppState>,
    path: String,
    existing_id: Option<String>,
) -> Result<LinkedModel, String> {
    state
        .store
        .link_gguf(std::path::Path::new(&path), existing_id.as_deref())
}
#[tauri::command]
pub fn unlink_gguf(state: State<AppState>, id: String) -> Result<(), String> {
    state.store.unlink_gguf(&id)
}
#[tauri::command]
pub async fn verify_gguf(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.providers.verify_gguf(&id).await
}
#[tauri::command]
pub fn set_openrouter_key(state: State<AppState>, key: String) -> Result<(), String> {
    if key.trim().len() < 12 || key.len() > 500 {
        return Err("Enter a valid OpenRouter API key".into());
    }
    keyring_core::Entry::new("Hypnofy.OpenRouter", "api-key")
        .map_err(|error| error.to_string())?
        .set_password(key.trim())
        .map_err(|error| format!("Windows Credential Manager rejected the key: {error}"))?;
    state.providers.clear_catalog();
    Ok(())
}
#[tauri::command]
pub fn clear_openrouter_key(state: State<AppState>) -> Result<(), String> {
    keyring_core::Entry::new("Hypnofy.OpenRouter", "api-key")
        .map_err(|error| error.to_string())?
        .delete_credential()
        .map_err(|error| error.to_string())?;
    state.providers.clear_catalog();
    Ok(())
}
#[tauri::command]
pub fn openrouter_connected() -> Result<bool, String> {
    Ok(keyring_core::Entry::new("Hypnofy.OpenRouter", "api-key")
        .map_err(|error| error.to_string())?
        .get_password()
        .is_ok())
}
#[tauri::command]
pub async fn install_model_runtime(state: State<'_, AppState>) -> Result<(), String> {
    let asset = state.assets.by_kind("llama-engine")?;
    let executable = crate::assets::install(&state.data_dir, asset).await?;
    let mut settings = state
        .settings
        .read()
        .map_err(|_| "Application settings are unavailable")?
        .clone();
    settings.llama_server_path = executable.to_string_lossy().into_owned();
    state.store.save_app_settings(&settings)?;
    *state
        .settings
        .write()
        .map_err(|_| "Application settings are unavailable")? = settings;
    Ok(())
}
#[tauri::command]
pub async fn start_generation(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: GenerationRequest,
) -> Result<String, String> {
    let script = state.store.get_script(&request.script_id)?;
    if script.revision != request.base_revision {
        return Err("Script changed before generation. Reload it and retry.".into());
    }
    if request.prompt.trim().is_empty()
        || request.prompt.len() > 16_000
        || request
            .selected_text
            .as_ref()
            .is_some_and(|text| text.len() > 24_000)
        || request
            .insertion_anchor
            .is_some_and(|anchor| anchor > 2_000_000)
    {
        return Err("Draft prompt or selected text is outside the supported size".into());
    }
    if !state
        .providers
        .models()
        .await?
        .iter()
        .any(|model| model.id == request.model_id && model.status == ModelStatus::Ready)
    {
        return Err("Selected model is unavailable. Refresh Models for a recovery action.".into());
    }
    state.providers.clone().start(app, request)
}
#[tauri::command]
pub fn cancel_generation(state: State<AppState>, job_id: String) {
    state.providers.cancel(&job_id)
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
    let managed_runtime = state.data_dir.join("engines/llama/llama-server.exe");
    if std::path::Path::new(&settings.llama_server_path) != managed_runtime.as_path() {
        return Err("llama.cpp must be installed from the verified asset manifest".into());
    }
    if !settings.gguf_model_path.is_empty()
        && !settings
            .gguf_model_path
            .to_ascii_lowercase()
            .ends_with(".gguf")
    {
        return Err("Linked model must be a GGUF data file".into());
    }
    state.store.save_app_settings(&settings)?;
    *state
        .settings
        .write()
        .map_err(|_| "Application settings are unavailable")? = settings;
    get_app_settings(state)
}
