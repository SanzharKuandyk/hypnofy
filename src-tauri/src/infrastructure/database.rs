use crate::{
    domain::{ApiResult, AppError},
    storage::Store,
};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
};
use tokio::sync::oneshot;

type Operation = Box<dyn FnOnce(&Store) + Send>;

#[derive(Clone)]
pub struct Database {
    inner: Arc<Worker>,
    pub recovered: Arc<Vec<crate::domain::JobSnapshot>>,
}

struct Worker {
    sender: Option<mpsc::Sender<Operation>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take()
            && thread.thread().id() != std::thread::current().id()
        {
            let _ = thread.join();
        }
    }
}

impl Database {
    pub fn open(path: PathBuf) -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::channel::<Operation>();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("hypnofy-database".into())
            .spawn(move || match Store::open(path) {
                Ok(store) => {
                    let _ = crate::application::cleanup::recover_installations(&store, false);
                    let recovered = store
                        .recover_jobs()
                        .map_err(|error| anyhow::anyhow!("{error:?}"));
                    let _ = ready_tx.send(recovered);
                    for operation in receiver {
                        operation(&store);
                    }
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            })?;
        let recovered = Arc::new(ready_rx.recv()??);
        Ok(Self {
            inner: Arc::new(Worker {
                sender: Some(sender),
                thread: Some(thread),
            }),
            recovered,
        })
    }

    pub async fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Store) -> ApiResult<T> + Send + 'static,
    ) -> ApiResult<T> {
        let (sender, receiver) = oneshot::channel();
        self.inner
            .sender
            .as_ref()
            .ok_or_else(|| AppError::from("Database worker closed"))?
            .send(Box::new(move |store| {
                let _ = sender.send(operation(store));
            }))
            .map_err(|_| AppError::from("Database worker is unavailable"))?;
        receiver
            .await
            .map_err(|_| AppError::from("Database worker stopped"))?
    }
}
