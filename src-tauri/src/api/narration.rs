use super::*;

#[taurpc::procedures(path = "narration")]
pub trait Narration {
    async fn library<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<StoredRecording>>;
    async fn remove<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<RecordingRemoval>;
    async fn plan<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        script_id: String,
    ) -> ApiResult<Vec<application::narration::Unit>>;
    async fn render<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        script_id: String,
        job_id: String,
        preview: bool,
        block_ids: Option<Vec<String>>,
    ) -> ApiResult<String>;
    async fn renders<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        script_id: String,
    ) -> ApiResult<Vec<RenderRecord>>;
    async fn export<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        render_id: String,
        path: String,
        ambience_id: Option<String>,
        job_id: String,
    ) -> ApiResult<String>;
}

#[derive(Clone)]
pub struct NarrationImpl;

#[taurpc::resolvers]
impl Narration for NarrationImpl {
    async fn library<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<StoredRecording>> {
        app_handle
            .state::<App>()
            .db
            .call(|s| s.recording_library())
            .await
    }
    async fn remove<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<RecordingRemoval> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        application::recordings::remove(&state, id).await
    }
    async fn plan<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        script_id: String,
    ) -> ApiResult<Vec<application::narration::Unit>> {
        let state = app_handle.state::<App>();
        let script = state
            .db
            .call(move |s| s.get_script(&script_id).map_err(error))
            .await?;
        application::narration::compile(&script).map_err(error)
    }
    async fn render<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        script_id: String,
        job_id: String,
        preview: bool,
        block_ids: Option<Vec<String>>,
    ) -> ApiResult<String> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let mut script = state
            .db
            .call(move |s| s.get_script(&script_id).map_err(error))
            .await?;
        if preview
            && let Some(ids) = block_ids
            && !ids.is_empty()
        {
            script.document =
                application::narration::passage_document(&script.document, &ids).map_err(error)?;
        }
        let (job, fresh) = state.jobs.start(&job_id, "narration")?;
        if fresh {
            let app = state.inner().clone();
            tokio::spawn(async move {
                let result = application::narration::render(
                    app.db,
                    app.inference,
                    job.clone(),
                    script,
                    app.root,
                    preview,
                )
                .await
                .map(Some);
                job.finish(result);
            });
        }
        Ok(job_id)
    }
    async fn renders<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        script_id: String,
    ) -> ApiResult<Vec<RenderRecord>> {
        let state = app_handle.state::<App>();
        state.db.call(move |s| s.render_records(&script_id)).await
    }
    async fn export<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        render_id: String,
        path: String,
        ambience_id: Option<String>,
        job_id: String,
    ) -> ApiResult<String> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let id = render_id.clone();
        if !state.db.call(move |s| s.can_export_render(&id)).await? {
            return Err("This recording was deleted or is pending deletion".into());
        }
        let source = render_path(&state, &render_id)?;
        let ambience = if let Some(id) = ambience_id {
            Some(
                state
                    .db
                    .call(move |s| {
                        s.ambience()
                            .map_err(error)?
                            .into_iter()
                            .find(|a| a.id == id)
                            .ok_or_else(|| AppError::from("Ambience unavailable"))
                    })
                    .await?
                    .path,
            )
        } else {
            None
        };
        let p = state.audio.state().map_err(error)?;
        let (job, fresh) = state.jobs.start(&job_id, "audio-export")?;
        if fresh {
            tokio::spawn(async move {
                let writer_job = job.clone();
                let result = tokio::task::spawn_blocking(move || {
                    crate::infrastructure::audio::export_audio(
                        &source,
                        std::path::Path::new(&path),
                        ambience.as_deref(),
                        p.volume,
                        p.ambience_volume,
                        &writer_job,
                    )
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r)
                .map(|()| None);
                job.finish(result);
            });
        }
        Ok(job_id)
    }
}

pub(super) fn render_path(app: &App, id: &str) -> ApiResult<std::path::PathBuf> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err("Invalid render".into());
    }
    for directory in ["audio/renders", "audio/previews"] {
        let path = app.root.join(directory).join(format!("{id}.wav"));
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("Render audio is missing. Render the script again.".into())
}
