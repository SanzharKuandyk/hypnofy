use super::*;

#[taurpc::procedures(path = "models")]
pub trait Models {
    async fn open_source<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        url: String,
    ) -> ApiResult<()>;
    async fn search<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        request: HubSearch,
    ) -> ApiResult<HubPage>;
    async fn detail<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        repository: String,
    ) -> ApiResult<HubDetail>;
    async fn usage<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Vec<ScriptSummary>>;
    async fn list<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<ModelRecord>>;
    async fn catalog() -> Vec<CatalogEntry>;
    async fn scan<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        job_id: String,
    ) -> ApiResult<String>;
    async fn link<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        path: String,
        architecture: Option<String>,
        existing_id: Option<String>,
    ) -> ApiResult<ModelRecord>;
    async fn verify<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
        job_id: String,
    ) -> ApiResult<String>;
    async fn plan<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        catalog_id: String,
    ) -> ApiResult<InstallPlan>;
    async fn install<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        plan_id: String,
        job_id: String,
    ) -> ApiResult<String>;
    async fn remove<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()>;
}

#[derive(Clone)]
pub struct ModelsImpl;

#[taurpc::resolvers]
impl Models for ModelsImpl {
    async fn open_source<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        url: String,
    ) -> ApiResult<()> {
        crate::infrastructure::external_links::model_source(&url)?;
        if !app_handle.state::<App>().test_mode {
            tokio::task::spawn_blocking(move || {
                crate::infrastructure::external_links::open_model_source(&url)
            })
            .await
            .map_err(|_| AppError::from("The browser launcher stopped unexpectedly"))??;
        }
        Ok(())
    }
    async fn search<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        request: HubSearch,
    ) -> ApiResult<HubPage> {
        app_handle.state::<App>().model_hub.search(request).await
    }
    async fn detail<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        repository: String,
    ) -> ApiResult<HubDetail> {
        app_handle.state::<App>().model_hub.detail(repository).await
    }
    async fn usage<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<Vec<ScriptSummary>> {
        app_handle
            .state::<App>()
            .db
            .call(move |s| s.model_usage(&id))
            .await
    }
    async fn list<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<ModelRecord>> {
        let state = app_handle.state::<App>();
        state.db.call(|s| s.model_records()).await
    }
    async fn catalog(self) -> Vec<CatalogEntry> {
        downloads::catalog()
    }
    async fn scan<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        job_id: String,
    ) -> ApiResult<String> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let (job, fresh) = state.jobs.start(&job_id, "folder-scan")?;
        if fresh {
            let app = state.inner().clone();
            tokio::spawn(async move {
                job.update(|s| {
                    s.state = crate::domain::JobStatus::Running;
                    s.phase = "Scanning the selected model folder".into();
                });
                let result = application::models::scan(&app, job.clone())
                    .await
                    .map(|_| None)
                    .map_err(|e| format!("{e:?}"));
                job.finish(result);
            });
        }
        Ok(job_id)
    }
    async fn link<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        path: String,
        architecture: Option<String>,
        existing_id: Option<String>,
    ) -> ApiResult<ModelRecord> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let model = application::models::record(
            std::path::Path::new(&path),
            architecture,
            "linked",
            existing_id,
        )?;
        let copy = model.clone();
        state.db.call(move |s| s.model_save(&copy)).await?;
        Ok(model)
    }
    async fn verify<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
        job_id: String,
    ) -> ApiResult<String> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let model = state.model(&id).await?;
        let (job, fresh) = state.jobs.start(&job_id, "model-verification")?;
        if fresh {
            let app = state.inner().clone();
            tokio::spawn(async move {
                let result = application::models::verify(&app, model, job.clone())
                    .await
                    .map(|m| Some(m.id));
                job.finish(result);
            });
        }
        Ok(job_id)
    }
    async fn plan<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        catalog_id: String,
    ) -> ApiResult<InstallPlan> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let p = state.db.call(|s| s.preferences()).await?;
        let plan = downloads::plan(&catalog_id, &p.model_root).await?;
        state
            .plans
            .lock()
            .map_err(|_| AppError::from("Installation plans unavailable"))?
            .insert(plan.id.clone(), plan.clone());
        Ok(plan)
    }
    async fn install<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        plan_id: String,
        job_id: String,
    ) -> ApiResult<String> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        if state.jobs.get(&job_id).is_ok() {
            return Ok(job_id);
        }
        let plan = state
            .plans
            .lock()
            .map_err(|_| AppError::from("Installation plans unavailable"))?
            .remove(&plan_id)
            .ok_or_else(|| AppError::from("Installation plan expired; choose the model again"))?;
        let (job, _) = state.jobs.start(&job_id, "model-install")?;
        let app = state.inner().clone();
        tokio::spawn(async move {
            let result = async {
                let _permit=tokio::select!{permit=app.downloads.clone().acquire_owned()=>permit.map_err(|_|"Download queue closed")?,_=job.wait_cancelled()=>return Err("Cancelled".to_owned())};
                let path = downloads::download(&plan, &job, &app.db).await?;
                let model_path = if plan.catalog.capability == "text" {
                    let mut files = std::fs::read_dir(&path)
                        .map_err(|e| e.to_string())?
                        .filter_map(Result::ok)
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|e| e == "gguf"))
                        .collect::<Vec<_>>();
                    files.sort();
                    files
                        .into_iter()
                        .next()
                        .ok_or("Downloaded model file missing")?
                } else {
                    path
                };
                let mut model = application::models::record(
                    &model_path,
                    plan.catalog.architecture,
                    "managed",
                    Some(plan.id),
                )
                .map_err(|e| format!("{e:?}"))?;
                model.name = plan.catalog.name;
                let copy = model.clone();
                app.db
                    .call(move |s| s.model_save(&copy))
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                let model = application::models::verify(&app, model, job.clone()).await?;
                Ok(Some(model.id))
            }
            .await;
            job.finish(result);
        });
        Ok(job_id)
    }
    async fn remove<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        application::models::remove(&state, id).await
    }
}
