use super::*;

#[taurpc::procedures(path = "providers")]
pub trait Providers {
    async fn connected<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> bool;
    async fn connect<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        key: String,
    ) -> ApiResult<()>;
    async fn disconnect<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> ApiResult<()>;
    async fn refresh<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<ModelRecord>>;
}

#[derive(Clone)]
pub struct ProvidersImpl;

#[taurpc::resolvers]
impl Providers for ProvidersImpl {
    async fn connected<R: tauri::Runtime>(self, app_handle: tauri::AppHandle<R>) -> bool {
        if app_handle.state::<App>().test_mode {
            return false;
        }
        crate::infrastructure::providers::openrouter::connected()
    }
    async fn connect<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        key: String,
    ) -> ApiResult<()> {
        let app = app_handle.state::<App>();
        app.require_entry()?;
        if app.test_mode {
            return Err("Credentials are disabled in isolated test workspaces".into());
        }
        crate::infrastructure::providers::openrouter::connect(&key).await
    }
    async fn disconnect<R: tauri::Runtime>(self, app_handle: tauri::AppHandle<R>) -> ApiResult<()> {
        let app = app_handle.state::<App>();
        app.require_entry()?;
        if app.test_mode {
            return Err("Credentials are disabled in isolated test workspaces".into());
        }
        crate::infrastructure::providers::openrouter::disconnect()?;
        app.db
            .call(|s| s.mark_provider_unavailable("openrouter"))
            .await
    }
    async fn refresh<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<ModelRecord>> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        if state.test_mode {
            return Err("Credentials are disabled in isolated test workspaces".into());
        }
        let value = crate::infrastructure::providers::openrouter::models().await?;
        let mut output = Vec::new();
        let inventory = state.db.call(|s| s.model_records()).await?;
        for item in value["data"].as_array().into_iter().flatten().filter(|i| {
            i.pointer("/architecture/output_modalities")
                .and_then(|v| v.as_array())
                .is_some_and(|a| a.iter().any(|s| s == "text"))
        }) {
            let Some(remote) = item["id"].as_str() else {
                continue;
            };
            let existing = inventory
                .iter()
                .find(|m| m.provider == "openrouter" && m.path.as_deref() == Some(remote));
            let model = ModelRecord {
                id: existing
                    .map(|m| m.id.clone())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                name: item["name"].as_str().unwrap_or(remote).into(),
                provider: "openrouter".into(),
                capability: "text".into(),
                architecture: None,
                path: Some(remote.into()),
                ownership: "remote".into(),
                status: "ready".into(),
                reason: None,
                fingerprint: None,
                context_tokens: item["context_length"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok()),
                voices: Vec::new(),
            };
            let copy = model.clone();
            state.db.call(move |s| s.model_save(&copy)).await?;
            output.push(model);
        }
        Ok(output)
    }
}
