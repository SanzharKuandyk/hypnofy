use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;

/// Tracks cancellation independently of any model adapter.
#[derive(Default)]
pub struct JobRegistry {
    flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl JobRegistry {
    pub fn register(&self) -> Result<(String, Arc<AtomicBool>), String> {
        let id = Uuid::new_v4().to_string();
        let flag = Arc::new(AtomicBool::new(false));
        self.flags
            .lock()
            .map_err(|_| "Generation jobs are unavailable")?
            .insert(id.clone(), flag.clone());
        Ok((id, flag))
    }

    pub fn cancel(&self, id: &str) {
        if let Ok(flags) = self.flags.lock()
            && let Some(flag) = flags.get(id)
        {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn finish(&self, id: &str) {
        if let Ok(mut flags) = self.flags.lock() {
            flags.remove(id);
        }
    }

    pub fn is_idle(&self) -> Result<bool, String> {
        Ok(self
            .flags
            .lock()
            .map_err(|_| "Generation jobs are unavailable")?
            .is_empty())
    }
}
