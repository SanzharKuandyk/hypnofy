use super::*;

#[taurpc::procedures(path = "jobs")]
pub trait Jobs {
    async fn list<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<JobSnapshot>>;
    async fn snapshot<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<JobSnapshot>;
    async fn cancel<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()>;
    #[taurpc(event)]
    async fn changed(snapshot: JobSnapshot);
}

#[derive(Clone)]
pub struct JobsImpl;

#[taurpc::resolvers]
impl Jobs for JobsImpl {
    async fn list<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<JobSnapshot>> {
        let state = app_handle.state::<App>();
        state.jobs.list()
    }
    async fn snapshot<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<JobSnapshot> {
        let state = app_handle.state::<App>();
        state
            .jobs
            .get(&id)?
            .snapshot
            .lock()
            .map(|s| s.clone())
            .map_err(|_| AppError::from("Job unavailable"))
    }
    async fn cancel<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.jobs.cancel(&id)
    }
}
