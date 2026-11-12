use crate::{
    domain::*,
    infrastructure::{
        audio::Audio,
        database::Database,
        runtime::{Inference, Jobs},
    },
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone)]
pub struct App {
    _workspace_lock: Arc<std::fs::File>,
    pub db: Database,
    pub root: PathBuf,
    pub jobs: Jobs,
    pub inference: Inference,
    pub audio: Audio,
    pub accepted: Arc<AtomicBool>,
    pub plans: Arc<Mutex<HashMap<String, InstallPlan>>>,
    pub test_mode: bool,
    pub downloads: Arc<tokio::sync::Semaphore>,
    pub model_hub: crate::infrastructure::model_hub::ModelHub,
}

impl App {
    pub fn open(root: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&root)?;
        let workspace_lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("workspace.lock"))?;
        workspace_lock.try_lock().map_err(|_| {
            anyhow::anyhow!("This Hypnofy workspace is already open in another process")
        })?;
        for directory in [
            "audio/renders",
            "audio/previews",
            "audio/chunks",
            "audio/.staging",
        ] {
            std::fs::create_dir_all(root.join(directory))?;
        }
        let db = Database::open(root.clone())?;
        crate::application::cleanup::recover_staging(&root, false)?;
        let inference = Inference::new(root.clone())?;
        let jobs = Jobs::recovered(&db.recovered);
        Ok(Self {
            _workspace_lock: Arc::new(workspace_lock),
            db,
            root,
            jobs,
            inference,
            audio: Audio::new()?,
            accepted: Arc::new(AtomicBool::new(false)),
            plans: Arc::new(Mutex::new(HashMap::new())),
            test_mode: cfg!(debug_assertions)
                && std::env::var_os("HYPNOFY_TEST_DATA_DIR").is_some(),
            downloads: Arc::new(tokio::sync::Semaphore::new(2)),
            model_hub: crate::infrastructure::model_hub::ModelHub::new()?,
        })
    }

    pub fn require_entry(&self) -> ApiResult<()> {
        if !self.accepted.load(Ordering::SeqCst) {
            return Err(AppError::EntryRequired {
                message: "Read and accept the welcome notice first".into(),
            });
        }
        Ok(())
    }

    pub async fn model(&self, id: &str) -> ApiResult<ModelRecord> {
        let id = id.to_owned();
        self.db
            .call(move |s| {
                s.model_records()?
                    .into_iter()
                    .find(|m| m.id == id)
                    .ok_or_else(|| "Model not found".into())
            })
            .await
    }
}
