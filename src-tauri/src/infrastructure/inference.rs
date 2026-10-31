use super::runtime::Job;
use hypnofy_inference_protocol::{
    Event, Operation, Payload, Request, VERSION, read_frame, write_frame,
};
use std::{
    path::PathBuf,
    process::{Child, ChildStdin},
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct InferenceResult {
    pub audio_path: Option<PathBuf>,
    pub context_tokens: Option<u32>,
}

struct Task {
    job: Job,
    operation: Operation,
    staging: PathBuf,
    reply: tokio::sync::oneshot::Sender<Result<InferenceResult, String>>,
}

enum Message {
    Run(Task),
    Release(tokio::sync::oneshot::Sender<()>),
}

/// One actor owns one heavyweight process, including all handles and its files.
#[derive(Clone)]
pub struct Inference {
    sender: mpsc::Sender<Message>,
}

impl Inference {
    pub fn new(root: PathBuf) -> anyhow::Result<Self> {
        let executable = std::env::current_exe()?.with_file_name("hypnofy-inference-worker.exe");
        let (sender, receiver) = mpsc::channel::<Message>();
        std::thread::Builder::new()
            .name("hypnofy-inference".into())
            .spawn(move || {
                let mut worker: Option<Worker> = None;
                loop {
                    let task = match receiver.recv_timeout(Duration::from_secs(300)) {
                        Ok(Message::Run(task)) => task,
                        Ok(Message::Release(reply)) => {
                            worker = None;
                            let _ = reply.send(());
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            worker = None;
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let result = (|| {
                        if task.job.cancelled() {
                            return Err("Cancelled".into());
                        }
                        if worker.is_none() {
                            worker = Some(Worker::start(&executable, &root)?);
                        }
                        worker
                            .as_mut()
                            .ok_or("Native worker unavailable")?
                            .execute(&task)
                    })();
                    if result.is_err() {
                        worker = None;
                    }
                    // Dropping a failed/cancelled worker waits for process exit and file handles.
                    let _ = task.reply.send(result);
                }
            })?;
        Ok(Self { sender })
    }

    pub async fn run(
        &self,
        job: Job,
        operation: Operation,
        staging: PathBuf,
    ) -> Result<InferenceResult, String> {
        let (reply, receiver) = tokio::sync::oneshot::channel();
        self.sender
            .send(Message::Run(Task {
                job,
                operation,
                staging,
                reply,
            }))
            .map_err(|_| "Native inference queue stopped")?;
        receiver
            .await
            .map_err(|_| "Native inference supervisor stopped")?
    }

    pub async fn release(&self) -> Result<(), String> {
        let (reply, receiver) = tokio::sync::oneshot::channel();
        self.sender
            .send(Message::Release(reply))
            .map_err(|_| "Inference queue closed")?;
        receiver
            .await
            .map_err(|_| "Inference supervisor stopped".into())
    }
}

struct Worker {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<Result<Event, String>>,
    reader: Option<std::thread::JoinHandle<()>>,
    root: tempfile::TempDir,
    #[cfg(windows)]
    _process_job: super::runtime::ProcessJob,
}

impl Worker {
    fn start(executable: &std::path::Path, root: &std::path::Path) -> Result<Self, String> {
        let directory = root.join("audio/.staging");
        let espeak = root.join("runtime/espeak");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let root = tempfile::Builder::new()
            .prefix("worker-")
            .tempdir_in(directory)
            .map_err(|e| e.to_string())?;
        let mut command = std::process::Command::new(executable);
        command.env("NOBODYWHO_ESPEAK_DATA_DIR", espeak);
        command
            .arg(root.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(
            |_| "The development inference worker could not start. Rebuild it beside Hypnofy.exe.",
        )?;
        #[cfg(windows)]
        let process_job = match super::runtime::ProcessJob::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let input = child.stdin.take().ok_or("Worker input unavailable")?;
        let mut output = child.stdout.take().ok_or("Worker output unavailable")?;
        let (sender, events) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            loop {
                match read_frame::<Event>(&mut output) {
                    Ok(Some(event)) => {
                        if sender.send(Ok(event)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            input,
            events,
            reader: Some(reader),
            root,
            #[cfg(windows)]
            _process_job: process_job,
        })
    }

    fn execute(&mut self, task: &Task) -> Result<InferenceResult, String> {
        std::fs::create_dir_all(&task.staging).map_err(|e| e.to_string())?;
        let id = task.job.id()?;
        write_frame(
            &mut self.input,
            &Request {
                version: VERSION,
                job_id: id.clone(),
                operation: task.operation.clone(),
            },
        )
        .map_err(|_| "Native worker connection closed")?;
        task.job.update(|s| {
            s.state = crate::domain::JobStatus::Running;
            s.phase = "Running local inference · automatic device selection".into();
        });
        let started = Instant::now();
        let mut sequence = 0;
        let mut cancel_sent: Option<Instant> = None;
        let mut result = InferenceResult::default();
        loop {
            if task.job.cancelled() {
                if let Some(sent) = cancel_sent {
                    if sent.elapsed() > Duration::from_millis(500) {
                        return Err("Cancelled".into());
                    }
                } else {
                    write_frame(
                        &mut self.input,
                        &Request {
                            version: VERSION,
                            job_id: id.clone(),
                            operation: Operation::Cancel,
                        },
                    )
                    .map_err(|_| "Cancelled")?;
                    cancel_sent = Some(Instant::now());
                }
            }
            if started.elapsed() > Duration::from_secs(1800) {
                return Err("Native inference exceeded its 30-minute operation limit".into());
            }
            let event = match self.events.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => event?,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("The native runtime stopped unexpectedly".into());
                }
            };
            if event.version != VERSION || event.job_id != id || event.sequence <= sequence {
                return Err("Invalid native worker response".into());
            }
            sequence = event.sequence;
            if task.job.cancelled()
                && matches!(
                    event.payload,
                    Payload::TextCompleted { .. }
                        | Payload::AudioCompleted { .. }
                        | Payload::Failed { .. }
                )
            {
                return Err("Cancelled".into());
            }
            match event.payload {
                Payload::ModelInfo { context_tokens } => {
                    result.context_tokens = Some(context_tokens)
                }
                Payload::TextDelta { text } => task.job.update(|s| s.text.push_str(&text)),
                Payload::TextCompleted { text } => {
                    task.job.update(|s| s.text = text);
                    return Ok(result);
                }
                Payload::AudioCompleted { filename } => {
                    if filename != format!("{id}.wav") {
                        return Err("Invalid audio artifact".into());
                    }
                    if task.job.cancelled() {
                        return Err("Cancelled".into());
                    }
                    let destination = task.staging.join(&filename);
                    std::fs::rename(self.root.path().join(filename), &destination)
                        .map_err(|e| e.to_string())?;
                    result.audio_path = Some(destination);
                    return Ok(result);
                }
                Payload::Failed { message, .. } => return Err(message),
                Payload::Ready => return Ok(result),
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
