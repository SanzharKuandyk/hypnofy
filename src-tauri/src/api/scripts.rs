use super::*;
use crate::application::markdown::{export as markdown_export, import as markdown_document};

#[taurpc::procedures(path = "scripts")]
pub trait Scripts {
    async fn export_many<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        ids: Vec<String>,
        directory: String,
        format: String,
        job_id: String,
    ) -> ApiResult<String>;
    async fn get<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Script>;
    async fn create<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        collection_id: Option<String>,
    ) -> ApiResult<Script>;
    async fn save<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        patch: ScriptPatch,
    ) -> ApiResult<Script>;
    async fn duplicate<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Script>;
    async fn bulk<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        request: BulkRequest,
    ) -> ApiResult<LibraryMutation>;
    async fn revisions<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Vec<RevisionRecord>>;
    async fn restore_revision<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
        revision: i32,
    ) -> ApiResult<Script>;
    async fn import_file<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        path: String,
        collection_id: Option<String>,
    ) -> ApiResult<Script>;
    async fn export_file<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
        path: String,
    ) -> ApiResult<()>;
    async fn legacy_document<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Option<String>>;
}

#[derive(Clone)]
pub struct ScriptsImpl;

#[taurpc::resolvers]
impl Scripts for ScriptsImpl {
    async fn export_many<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        ids: Vec<String>,
        directory: String,
        format: String,
        job_id: String,
    ) -> ApiResult<String> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        if state.jobs.get(&job_id).is_ok() {
            return Ok(job_id);
        }
        if ids.is_empty() || ids.len() > 5000 || !["txt", "md", "json"].contains(&format.as_str()) {
            return Err("Select 1–5000 scripts and an export format".into());
        }
        let scripts = state
            .db
            .call(move |s| {
                ids.into_iter()
                    .map(|id| s.get_script(&id).map_err(error))
                    .collect::<ApiResult<Vec<_>>>()
            })
            .await?;
        let (job, fresh) = state.jobs.start(&job_id, "script-export")?;
        if fresh {
            tokio::spawn(async move {
                let writer_job = job.clone();
                let result = tokio::task::spawn_blocking(move || {
                    application::exporting::batch(
                        scripts,
                        std::path::Path::new(&directory),
                        &format,
                        &writer_job,
                    )
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|value| value)
                .map(|path| Some(path.to_string_lossy().into_owned()));
                job.finish(result);
            });
        }
        Ok(job_id)
    }
    async fn get<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Script> {
        let state = app_handle.state::<App>();
        state
            .db
            .call(move |s| s.get_script(&id).map_err(error))
            .await
    }
    async fn create<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        collection_id: Option<String>,
    ) -> ApiResult<Script> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state
            .db
            .call(move |s| {
                s.create_script(collection_id.as_deref(), None, "")
                    .map_err(error)
            })
            .await
    }
    async fn save<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        patch: ScriptPatch,
    ) -> ApiResult<Script> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state
            .db
            .call(move |s| {
                let current = s.get_script(&patch.id).map_err(error)?;
                if current.revision != patch.expected_revision {
                    return Err(AppError::RevisionConflict {
                        message: "The script changed elsewhere. Reload or save a copy.".into(),
                        current_revision: current.revision as i32,
                    });
                }
                if current.title == patch.title
                    && current.document == patch.document
                    && current.collection_id == patch.collection_id
                    && current.favorite == patch.favorite
                    && current.last_model_id == patch.last_model_id
                    && serde_json::to_string(&current.session).ok()
                        == serde_json::to_string(&patch.session).ok()
                {
                    return Ok(current);
                }
                s.update_script(patch).map_err(error)
            })
            .await
    }
    async fn duplicate<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Script> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state
            .db
            .call(move |s| s.duplicate_script(&id).map_err(error))
            .await
    }
    async fn bulk<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        request: BulkRequest,
    ) -> ApiResult<LibraryMutation> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        application::recordings::mutate_scripts(&state, request).await
    }
    async fn revisions<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Vec<RevisionRecord>> {
        let state = app_handle.state::<App>();
        state.db.call(move |s| s.revisions(&id)).await
    }
    async fn restore_revision<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
        revision: i32,
    ) -> ApiResult<Script> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state
            .db
            .call(move |s| s.restore_revision(&id, revision))
            .await
    }
    async fn import_file<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        path: String,
        collection_id: Option<String>,
    ) -> ApiResult<Script> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let path = std::path::PathBuf::from(path);
        let metadata = std::fs::metadata(&path).map_err(error)?;
        if metadata.len() > 10_000_000 {
            return Err("Script import files are limited to 10 MB".into());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|_| AppError::from("The file is not valid UTF-8"))?;
        let mut title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported script")
            .to_owned();
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let preferences = state.db.call(|s| s.preferences()).await?;
        let mut session = crate::storage::SessionSettings::from_preferences(&preferences);
        let mut favorite = false;
        let mut last_model_id = None;
        let doc = match extension.as_str() {
            "json" => {
                let value: serde_json::Value = serde_json::from_str(&text).map_err(error)?;
                let value = if value["format"] == "hypnofy-script" {
                    if value["version"] != 1 {
                        return Err("This script export uses an unsupported version".into());
                    }
                    value["script"].clone()
                } else {
                    value
                };
                if let Some(name) = value["title"].as_str() {
                    title = name.to_owned();
                }
                if let Some(settings) = value.get("session") {
                    session = serde_json::from_value(settings.clone()).map_err(error)?;
                }
                favorite = value["favorite"].as_bool().unwrap_or(false);
                last_model_id = value["lastModelId"]
                    .as_str()
                    .filter(|id| id.len() <= 256)
                    .map(str::to_owned);
                crate::document::upgrade(value.get("document").cloned().unwrap_or(value))
                    .map_err(error)?
            }
            "txt" => crate::document::from_plain_text(&text),
            "md" => markdown_document(&text),
            _ => return Err("Import a .txt, .md or Hypnofy .json file".into()),
        };
        state
            .db
            .call(move |s| {
                s.import_document(
                    collection_id.as_deref(),
                    &title,
                    doc,
                    session,
                    favorite,
                    last_model_id,
                )
                .map_err(error)
            })
            .await
    }
    async fn export_file<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
        path: String,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        let script = state
            .db
            .call(move |s| s.get_script(&id).map_err(error))
            .await?;
        let path = std::path::PathBuf::from(path);
        let content = match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "json" => serde_json::to_string_pretty(
                &serde_json::json!({"format":"hypnofy-script","version":1,"script":script}),
            )
            .map_err(error)?,
            "md" => markdown_export(&script.document),
            "txt" => script.body,
            _ => return Err("Choose .txt, .md or .json".into()),
        };
        let parent = path
            .parent()
            .ok_or_else(|| AppError::from("Invalid export location"))?;
        use std::io::Write;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(error)?;
        temporary.write_all(content.as_bytes()).map_err(error)?;
        temporary.as_file().sync_all().map_err(error)?;
        temporary.persist(path).map_err(error)?;
        Ok(())
    }
    async fn legacy_document<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Option<String>> {
        let state = app_handle.state::<App>();
        state.db.call(move |s| s.legacy_document(&id)).await
    }
}
