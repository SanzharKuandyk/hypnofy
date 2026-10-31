use super::*;
use crate::application::cleanup::{cleanup, storage_info};

#[taurpc::procedures(path = "settings")]
pub trait Settings {
    async fn get<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> ApiResult<Preferences>;
    async fn save<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        preferences: Preferences,
    ) -> ApiResult<Preferences>;
    async fn storage<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> ApiResult<StorageInfo>;
    async fn cleanup<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> ApiResult<StorageInfo>;
}

#[derive(Clone)]
pub struct SettingsImpl;

#[taurpc::resolvers]
impl Settings for SettingsImpl {
    async fn get<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Preferences> {
        let state = app_handle.state::<App>();
        state.db.call(|s| s.preferences()).await
    }
    async fn save<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        preferences: Preferences,
    ) -> ApiResult<Preferences> {
        let state = app_handle.state::<App>();
        preferences.validate()?;
        std::fs::create_dir_all(&preferences.model_root).map_err(error)?;
        let check = tempfile::NamedTempFile::new_in(&preferences.model_root)
            .map_err(|_| AppError::from("Model folder is not writable"))?;
        drop(check);
        state
            .db
            .call(move |s| {
                s.json_set(
                    "preferences",
                    &serde_json::to_string(&preferences).map_err(error)?,
                )?;
                Ok(preferences)
            })
            .await
    }
    async fn storage<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<StorageInfo> {
        let state = app_handle.state::<App>();
        storage_info(&state).await
    }
    async fn cleanup<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<StorageInfo> {
        let state = app_handle.state::<App>();
        cleanup(&state).await?;
        storage_info(&state).await
    }
}
