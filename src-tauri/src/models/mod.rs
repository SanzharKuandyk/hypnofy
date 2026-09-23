use crate::{config::AppSettings, jobs::JobRegistry, storage::Store};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::process::{Child, Command};
use uuid::Uuid;
mod gguf;
mod ollama;
mod opencode;
mod openrouter;

pub struct ProviderManager {
    client: reqwest::Client,
    jobs: JobRegistry,
    openrouter_catalog: Mutex<Option<(Instant, Vec<TextModel>)>>,
    llama: tokio::sync::Mutex<Option<LlamaProcess>>,
    settings: Arc<RwLock<AppSettings>>,
    store: Arc<Store>,
    runtime_version: String,
}

struct LlamaProcess {
    child: Child,
    endpoint: String,
    token: String,
    model_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextModel {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub capability: String,
    pub source: String,
    pub status: ModelStatus,
    pub reason: Option<String>,
    pub recovery: Option<RecoveryAction>,
    pub cost: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelStatus {
    Ready,
    Offline,
    Missing,
    Invalid,
    NeedsAuth,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryAction {
    Connect,
    Relink,
    Install,
    Authenticate,
    Verify,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DraftOperation {
    New,
    Continue,
    RewriteSelection,
}

impl DraftOperation {
    fn as_str(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Continue => "continue",
            Self::RewriteSelection => "rewrite-selection",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationRequest {
    pub script_id: String,
    pub base_revision: i64,
    pub model_id: String,
    pub operation: DraftOperation,
    pub prompt: String,
    pub selected_text: Option<String>,
    pub insertion_anchor: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationEvent {
    pub job_id: String,
    pub script_id: String,
    pub base_revision: i64,
    pub phase: String,
    pub delta: Option<String>,
    pub error: Option<GenerationError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationError {
    pub code: String,
    pub message: String,
    pub recovery: Option<String>,
}

impl ProviderManager {
    fn failure(model_id: &str, message: String) -> GenerationError {
        let (code, recovery) = if model_id.starts_with("ollama:") {
            ("ollama-unavailable", Some("connect"))
        } else if model_id.starts_with("gguf:") {
            if message.contains("runtime") {
                ("runtime-missing", Some("install"))
            } else {
                ("gguf-unavailable", Some("relink"))
            }
        } else if model_id.starts_with("openrouter:") {
            if message.contains("401") || message.contains("API key") {
                ("authentication", Some("authenticate"))
            } else {
                ("remote-error", Some("connect"))
            }
        } else {
            ("provider-unsupported", None)
        };
        GenerationError {
            code: code.into(),
            message,
            recovery: recovery.map(str::to_owned),
        }
    }
    pub fn new(
        settings: Arc<RwLock<AppSettings>>,
        store: Arc<Store>,
        runtime_version: String,
    ) -> Result<Self, String> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(180))
                .build()
                .map_err(|error| format!("Could not create model client: {error}"))?,
            jobs: JobRegistry::default(),
            openrouter_catalog: Mutex::new(None),
            llama: tokio::sync::Mutex::new(None),
            settings,
            store,
            runtime_version,
        })
    }

    pub async fn models(&self) -> Result<Vec<TextModel>, String> {
        let mut models = Vec::new();
        match self
            .client
            .get("http://127.0.0.1:11434/api/tags")
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                if let Ok(value) = response.json::<serde_json::Value>().await
                    && let Some(items) = value["models"].as_array()
                {
                    for item in items {
                        if let Some(name) = item["name"].as_str() {
                            models.push(TextModel {
                                id: format!("ollama:{name}"),
                                name: name.into(),
                                provider: "ollama".into(),
                                capability: "text".into(),
                                source: "discovered".into(),
                                status: ModelStatus::Ready,
                                reason: None,
                                recovery: None,
                                cost: None,
                            });
                        }
                    }
                }
            }
            Ok(response) => models.push(TextModel {
                id: "ollama:offline".into(),
                name: "Ollama".into(),
                provider: "ollama".into(),
                capability: "text".into(),
                source: "discovered".into(),
                status: ModelStatus::Offline,
                reason: Some(format!("Ollama returned {}", response.status())),
                recovery: Some(RecoveryAction::Connect),
                cost: None,
            }),
            Err(error) => models.push(TextModel {
                id: "ollama:offline".into(),
                name: "Ollama".into(),
                provider: "ollama".into(),
                capability: "text".into(),
                source: "discovered".into(),
                status: ModelStatus::Offline,
                reason: Some(format!("Ollama is not reachable: {error}")),
                recovery: Some(RecoveryAction::Connect),
                cost: None,
            }),
        }
        let settings = self
            .settings
            .read()
            .map_err(|_| "Model settings are unavailable")?
            .clone();
        for linked in self.store.linked_models()? {
            let file = PathBuf::from(&linked.path);
            let server = PathBuf::from(&settings.llama_server_path);
            let valid = file.is_file()
                && file
                    .extension()
                    .and_then(|v| v.to_str())
                    .is_some_and(|v| v.eq_ignore_ascii_case("gguf"));
            let (status, reason, recovery) = if !valid {
                (
                    ModelStatus::Missing,
                    Some("Linked GGUF file is missing or invalid"),
                    Some(RecoveryAction::Relink),
                )
            } else if !server.is_file() {
                (
                    ModelStatus::Missing,
                    Some("Install the verified llama.cpp runtime"),
                    Some(RecoveryAction::Install),
                )
            } else if !self.store.gguf_verified(&linked, &self.runtime_version) {
                (
                    ModelStatus::Invalid,
                    Some("Verify this file with the installed llama.cpp runtime"),
                    Some(RecoveryAction::Verify),
                )
            } else {
                (ModelStatus::Ready, None, None)
            };
            models.push(TextModel {
                id: format!("gguf:{}", linked.id),
                name: linked.name,
                provider: "gguf".into(),
                capability: "text".into(),
                source: "linked".into(),
                status,
                reason: reason.map(str::to_owned),
                recovery,
                cost: None,
            });
        }
        models.extend(self.openrouter_models().await);
        models.push(opencode::model_status());
        Ok(models)
    }

    pub fn clear_catalog(&self) {
        if let Ok(mut cache) = self.openrouter_catalog.lock() {
            *cache = None;
        }
    }

    pub fn start(
        self: Arc<Self>,
        app: AppHandle,
        request: GenerationRequest,
    ) -> Result<String, String> {
        let (id, cancelled) = self.jobs.register()?;
        let job_id = id.clone();
        let _ = app.emit(
            "generation-event",
            GenerationEvent {
                job_id: id.clone(),
                script_id: request.script_id.clone(),
                base_revision: request.base_revision,
                phase: "queued".into(),
                delta: None,
                error: None,
            },
        );
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = match request
                .model_id
                .split_once(':')
                .map(|(provider, _)| provider)
            {
                Some("ollama") => {
                    manager
                        .generate_ollama(&app, &job_id, &request, &cancelled)
                        .await
                }
                Some("gguf") => {
                    manager
                        .generate_gguf(&app, &job_id, &request, &cancelled)
                        .await
                }
                Some("openrouter") => {
                    manager
                        .generate_openrouter(&app, &job_id, &request, &cancelled)
                        .await
                }
                Some("opencode") => {
                    Err("OpenCode isolation is not supported by the installed CLI version".into())
                }
                _ => Err("Unknown model provider".into()),
            };
            let event = match result {
                _ if cancelled.load(Ordering::Relaxed) => GenerationEvent {
                    job_id: job_id.clone(),
                    script_id: request.script_id.clone(),
                    base_revision: request.base_revision,
                    phase: "cancelled".into(),
                    delta: None,
                    error: None,
                },
                Ok(()) => GenerationEvent {
                    job_id: job_id.clone(),
                    script_id: request.script_id.clone(),
                    base_revision: request.base_revision,
                    phase: "complete".into(),
                    delta: None,
                    error: None,
                },
                Err(error) => GenerationEvent {
                    job_id: job_id.clone(),
                    script_id: request.script_id.clone(),
                    base_revision: request.base_revision,
                    phase: "failed".into(),
                    delta: None,
                    error: Some(Self::failure(&request.model_id, error)),
                },
            };
            let _ = app.emit("generation-event", event);
            manager.jobs.finish(&job_id);
        });
        Ok(id)
    }

    pub fn cancel(&self, id: &str) {
        self.jobs.cancel(id);
    }
}

fn prompt(r: &GenerationRequest) -> String {
    format!(
        "Write a polished English hypnosis script. Return only spoken narration as paragraphs, with no headings or explanation. Use natural pacing, specific sensory detail, gentle invitations, and no medical claims. Operation: {}. Request: {}. {}",
        r.operation.as_str(),
        r.prompt.trim(),
        r.selected_text
            .as_deref()
            .map(|value| format!("Selected text to work with:\n{value}"))
            .unwrap_or_default()
    )
}
