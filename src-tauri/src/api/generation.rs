use super::*;

#[taurpc::procedures(path = "generation")]
pub trait Generation {
    async fn start<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        request: DraftRequest,
    ) -> ApiResult<String>;
    async fn suggestions<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        script_id: String,
    ) -> ApiResult<Vec<Suggestion>>;
    async fn discard<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()>;
}

#[derive(Clone)]
pub struct GenerationImpl;

#[taurpc::resolvers]
impl Generation for GenerationImpl {
    async fn start<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        request: DraftRequest,
    ) -> ApiResult<String> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let id = request.job_id.clone();
        let (job, fresh) = state.jobs.start(&id, "text-generation")?;
        if fresh {
            let app = state.inner().clone();
            tokio::spawn(async move {
                let result = application::generation::run(app, request, job.clone())
                    .await
                    .map(Some);
                job.finish(result);
            });
        }
        Ok(id)
    }
    async fn suggestions<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        script_id: String,
    ) -> ApiResult<Vec<Suggestion>> {
        let state = app_handle.state::<App>();
        state.db.call(move |s| s.suggestions(&script_id)).await
    }
    async fn discard<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.db.call(move |s| s.suggestion_discard(&id)).await
    }
}
