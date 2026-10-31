pub use super::inference::Inference;
use crate::domain::{ApiResult, AppError, JobSnapshot};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone)]
pub struct Job {
    pub snapshot: Arc<Mutex<JobSnapshot>>,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
pub struct Jobs {
    entries: Arc<Mutex<HashMap<String, Job>>>,
    maintenance: Arc<AtomicBool>,
}

pub struct Maintenance(Arc<AtomicBool>);

impl Drop for Maintenance {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Jobs {
    pub fn recovered(snapshots: &[JobSnapshot]) -> Self {
        let entries = snapshots
            .iter()
            .map(|snapshot| {
                (
                    snapshot.id.clone(),
                    Job {
                        snapshot: Arc::new(Mutex::new(snapshot.clone())),
                        cancel: Arc::new(AtomicBool::new(false)),
                    },
                )
            })
            .collect();
        Self {
            entries: Arc::new(Mutex::new(entries)),
            maintenance: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn start(&self, id: &str, kind: &str) -> ApiResult<(Job, bool)> {
        let kind = crate::domain::JobKind::from_name(kind)?;
        if uuid::Uuid::parse_str(id).is_err() {
            return Err("Invalid job ID".into());
        }
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| AppError::from("Jobs unavailable"))?;
        if let Some(job) = entries.get(id) {
            if job
                .snapshot
                .lock()
                .map_err(|_| AppError::from("Job unavailable"))?
                .kind
                != kind
            {
                return Err("This job ID belongs to a different operation".into());
            }
            return Ok((job.clone(), false));
        }
        self.ensure_available()?;
        let job = Job {
            snapshot: Arc::new(Mutex::new(JobSnapshot {
                id: id.into(),
                kind,
                state: crate::domain::JobStatus::Queued,
                phase: "Queued".into(),
                progress: None,
                text: String::new(),
                result: None,
                error: None,
                sequence: 0,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        entries.insert(id.into(), job.clone());
        Ok((job, true))
    }

    pub fn ensure_available(&self) -> ApiResult<()> {
        if self.maintenance.load(Ordering::SeqCst) {
            return Err("Storage maintenance is in progress. Try again in a moment.".into());
        }
        Ok(())
    }

    pub fn maintenance(&self) -> ApiResult<Maintenance> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| AppError::from("Jobs unavailable"))?;
        self.ensure_available()?;
        for job in entries.values() {
            if ["queued", "running", "cancelling"].contains(
                &job.snapshot
                    .lock()
                    .map_err(|_| AppError::from("Job unavailable"))?
                    .state
                    .as_str(),
            ) {
                return Err("Wait for active jobs to finish before managing stored files".into());
            }
        }
        self.maintenance.store(true, Ordering::SeqCst);
        Ok(Maintenance(self.maintenance.clone()))
    }

    pub fn get(&self, id: &str) -> ApiResult<Job> {
        self.entries
            .lock()
            .map_err(|_| AppError::from("Jobs unavailable"))?
            .get(id)
            .cloned()
            .ok_or_else(|| "Job not found".into())
    }

    pub fn list(&self) -> ApiResult<Vec<JobSnapshot>> {
        self.entries
            .lock()
            .map_err(|_| AppError::from("Jobs unavailable"))?
            .values()
            .map(|j| {
                j.snapshot
                    .lock()
                    .map(|s| s.clone())
                    .map_err(|_| AppError::from("Job unavailable"))
            })
            .collect()
    }

    pub fn cancel(&self, id: &str) -> ApiResult<()> {
        let job = self.get(id)?;
        let mut state = job
            .snapshot
            .lock()
            .map_err(|_| AppError::from("Job unavailable"))?;
        if ["completed", "failed", "cancelled"].contains(&state.state.as_str())
            || state.phase == "Publishing completed work"
        {
            return Ok(());
        }
        job.cancel.store(true, Ordering::SeqCst);
        state.state = crate::domain::JobStatus::Cancelling;
        state.phase = "Stopping and cleaning up".into();
        state.sequence = state.sequence.saturating_add(1);
        Ok(())
    }
}

impl Job {
    pub fn id(&self) -> Result<String, String> {
        self.snapshot
            .lock()
            .map(|s| s.id.clone())
            .map_err(|_| "Job unavailable".into())
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub async fn wait_cancelled(&self) {
        while !self.cancelled() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    pub fn begin_commit(&self) -> Result<(), String> {
        let mut state = self.snapshot.lock().map_err(|_| "Job unavailable")?;
        if self.cancelled() {
            return Err("Cancelled".into());
        }
        state.phase = "Publishing completed work".into();
        state.sequence = state.sequence.saturating_add(1);
        Ok(())
    }
    pub fn update(&self, f: impl FnOnce(&mut JobSnapshot)) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            f(&mut snapshot);
            snapshot.sequence = snapshot.sequence.saturating_add(1);
        }
    }

    pub fn finish(&self, result: Result<Option<String>, String>) {
        self.update(|s| {
            if result.is_err() && self.cancel.load(Ordering::SeqCst) {
                s.state = crate::domain::JobStatus::Cancelled;
                s.phase = "Cancelled".into();
            } else {
                match result {
                    Ok(id) => {
                        s.state = crate::domain::JobStatus::Completed;
                        s.phase = "Complete".into();
                        s.progress = Some(1.0);
                        s.result = crate::domain::JobResult::completed(s.kind, id);
                    }
                    Err(error) => {
                        s.state = crate::domain::JobStatus::Failed;
                        s.phase = "Failed".into();
                        s.error = Some(error.into());
                    }
                }
            }
            if s.kind == crate::domain::JobKind::TextGeneration {
                s.text.clear();
            }
        });
    }
}

#[cfg(windows)]
pub(super) struct ProcessJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ProcessJob {
    pub(super) fn attach(child: &std::process::Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        // SAFETY: fresh owned kernel handle; structures are initialized and sized.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err("Cannot create native process job".into());
            }
            let job = Self(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle().cast()) == 0
            {
                return Err("Cannot isolate native worker lifetime".into());
            }
            Ok(job)
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintenance_excludes_jobs_and_releases_on_drop() {
        let jobs = Jobs::default();
        let id = uuid::Uuid::new_v4().to_string();
        let guard = jobs.maintenance().unwrap();
        assert!(jobs.start(&id, "folder-scan").is_err());
        drop(guard);
        let (job, _) = jobs.start(&id, "folder-scan").unwrap();
        assert!(jobs.maintenance().is_err());
        job.finish(Ok(None));
        assert!(jobs.maintenance().is_ok());
    }

    #[test]
    fn duplicate_ids_are_idempotent_and_commit_wins_late_cancellation() {
        let jobs = Jobs::default();
        let id = uuid::Uuid::new_v4().to_string();
        let (job, fresh) = jobs.start(&id, "narration").unwrap();
        assert!(fresh);
        assert!(!jobs.start(&id, "narration").unwrap().1);
        assert!(jobs.start(&id, "text-generation").is_err());
        job.begin_commit().unwrap();
        jobs.cancel(&id).unwrap();
        assert!(!job.cancelled());
        job.finish(Ok(Some("render".into())));
        assert_eq!(jobs.list().unwrap()[0].state, "completed");
    }

    #[test]
    fn cancelled_jobs_cannot_commit_and_sequences_advance() {
        let jobs = Jobs::default();
        let id = uuid::Uuid::new_v4().to_string();
        let (job, _) = jobs.start(&id, "narration").unwrap();
        job.update(|s| s.state = crate::domain::JobStatus::Running);
        jobs.cancel(&id).unwrap();
        assert!(job.begin_commit().is_err());
        job.finish(Err("Cancelled".into()));
        let snapshot = jobs.list().unwrap().remove(0);
        assert_eq!(snapshot.state, "cancelled");
        assert!(snapshot.sequence >= 3);
    }
}
