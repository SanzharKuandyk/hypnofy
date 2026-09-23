use crate::config::AppSettings;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::process::{Child, Command};
use uuid::Uuid;

pub struct ProviderManager {
    client: reqwest::Client,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    llama: tokio::sync::Mutex<Option<LlamaProcess>>,
    settings: Arc<RwLock<AppSettings>>,
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
    pub provider: String,
    pub id: String,
    pub label: String,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationRequest {
    pub provider: String,
    pub model: String,
    pub intention: String,
    pub minutes: u32,
    pub tone: String,
    pub instructions: String,
    pub included_text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationEvent {
    pub job_id: String,
    pub status: String,
    pub text: String,
    pub error: Option<String>,
}

impl ProviderManager {
    pub fn new(settings: Arc<RwLock<AppSettings>>) -> Result<Self, String> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(180))
                .build()
                .map_err(|error| format!("Could not create model client: {error}"))?,
            jobs: Mutex::new(HashMap::new()),
            llama: tokio::sync::Mutex::new(None),
            settings,
        })
    }

    pub async fn models(&self) -> Vec<TextModel> {
        let mut models = Vec::new();
        match self
            .client
            .get("http://127.0.0.1:11434/api/tags")
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(response) => {
                if let Ok(value) = response.json::<serde_json::Value>().await
                    && let Some(items) = value["models"].as_array()
                {
                    for item in items {
                        if let Some(name) = item["name"].as_str() {
                            models.push(TextModel {
                                provider: "ollama".into(),
                                id: name.into(),
                                label: format!("{name} · Ollama"),
                                available: true,
                                reason: None,
                            });
                        }
                    }
                }
            }
            Err(error) => models.push(TextModel {
                provider: "ollama".into(),
                id: "gemma3:latest".into(),
                label: "Gemma 3 · Ollama".into(),
                available: false,
                reason: Some(format!("Ollama is not reachable: {error}")),
            }),
        }
        let settings = match self.settings.read() {
            Ok(value) => value.clone(),
            Err(_) => return models,
        };
        if settings.gguf_model_path.trim().is_empty() {
            return models;
        }
        let qwen = PathBuf::from(&settings.gguf_model_path);
        let server = PathBuf::from(&settings.llama_server_path);
        models.push(TextModel {
            provider: "gguf".into(),
            id: qwen.to_string_lossy().into_owned(),
            label: format!("{} · Local GGUF", qwen.file_name().and_then(|name| name.to_str()).unwrap_or("Local model")),
            available: qwen.is_file() && server.is_file(),
            reason: if !qwen.is_file() {
                Some("The linked GGUF model was not found".into())
            } else if !server.is_file() {
                Some("Install the verified llama.cpp runtime to use GGUF files".into())
            } else {
                None
            },
        });
        models
    }

    pub fn start(
        self: Arc<Self>,
        app: AppHandle,
        request: GenerationRequest,
    ) -> Result<String, String> {
        let id = Uuid::new_v4().to_string();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.jobs
            .lock()
            .map_err(|_| "Generation jobs are unavailable")?
            .insert(id.clone(), cancelled.clone());
        let job_id = id.clone();
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = if request.provider == "ollama" {
                manager
                    .generate_ollama(&app, &job_id, &request, &cancelled)
                    .await
            } else {
                manager
                    .generate_gguf(&app, &job_id, &request, &cancelled)
                    .await
            };
            let event = match result {
                Ok(()) if cancelled.load(Ordering::Relaxed) => GenerationEvent {
                    job_id: job_id.clone(),
                    status: "cancelled".into(),
                    text: String::new(),
                    error: None,
                },
                Ok(()) => GenerationEvent {
                    job_id: job_id.clone(),
                    status: "complete".into(),
                    text: String::new(),
                    error: None,
                },
                Err(error) => GenerationEvent {
                    job_id: job_id.clone(),
                    status: "error".into(),
                    text: String::new(),
                    error: Some(error),
                },
            };
            let _ = app.emit("generation-event", event);
            if let Ok(mut jobs) = manager.jobs.lock() {
                jobs.remove(&job_id);
            }
        });
        Ok(id)
    }

    pub fn cancel(&self, id: &str) {
        if let Ok(jobs) = self.jobs.lock()
            && let Some(flag) = jobs.get(id)
        {
            flag.store(true, Ordering::Relaxed);
        }
    }

    async fn generate_ollama(
        &self,
        app: &AppHandle,
        id: &str,
        r: &GenerationRequest,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let keep_alive = self
            .settings
            .read()
            .map(|settings| format!("{}m", settings.keep_model_minutes))
            .unwrap_or_else(|_| "5m".into());
        let response=self.client.post("http://127.0.0.1:11434/api/generate").json(&serde_json::json!({"model":r.model,"prompt":prompt(r),"stream":true,"keep_alive":keep_alive,"options":{"temperature":0.75}})).send().await.map_err(|e|format!("Could not reach Ollama: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("Ollama returned {}", response.status()));
        }
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            pending.push_str(&String::from_utf8_lossy(&chunk.map_err(|e| e.to_string())?));
            while let Some(index) = pending.find('\n') {
                let line = pending[..index].to_string();
                pending.drain(..=index);
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line)
                    && let Some(text) = value["response"].as_str()
                {
                    let _ = app.emit(
                        "generation-event",
                        GenerationEvent {
                            job_id: id.into(),
                            status: "streaming".into(),
                            text: text.into(),
                            error: None,
                        },
                    );
                }
            }
        }
        Ok(())
    }

    async fn generate_gguf(
        &self,
        app: &AppHandle,
        id: &str,
        r: &GenerationRequest,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let (endpoint, token) = self.ensure_llama(PathBuf::from(&r.model)).await?;
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let max_tokens = (r.minutes * 180).clamp(512, 6000);
        let response=self.client.post(format!("{endpoint}/v1/chat/completions")).bearer_auth(token).json(&serde_json::json!({"model":"local","messages":[{"role":"user","content":prompt(r)}],"stream":true,"temperature":0.72,"top_p":0.9,"max_tokens":max_tokens,"chat_template_kwargs":{"enable_thinking":false}})).send().await.map_err(|e|format!("GGUF generation failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("llama.cpp returned {}", response.status()));
        }
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) { break; }
            pending.push_str(&String::from_utf8_lossy(&chunk.map_err(|e| e.to_string())?));
            while let Some(index) = pending.find('\n') {
                let line = pending[..index].trim().to_string();
                pending.drain(..=index);
                let Some(json) = line.strip_prefix("data: ") else { continue; };
                if json == "[DONE]" { continue; }
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(json)
                    && let Some(text) = value["choices"][0]["delta"]["content"].as_str()
                    && !text.is_empty()
                { let _ = app.emit("generation-event", GenerationEvent { job_id: id.into(), status: "streaming".into(), text: text.into(), error: None }); }
            }
        }
        Ok(())
    }

    async fn ensure_llama(&self, model: PathBuf) -> Result<(String, String), String> {
        // Reuse one authenticated loopback server while its selected model is unchanged.
        if !model.is_file() {
            return Err("The selected GGUF model is missing. Relink it or use Ollama.".into());
        }
        let mut guard = self.llama.lock().await;
        if let Some(running) = guard.as_mut() {
            if running.model_path == model
                && running
                    .child
                    .try_wait()
                    .map_err(|e| e.to_string())?
                    .is_none()
            {
                return Ok((running.endpoint.clone(), running.token.clone()));
            }
            let _ = running.child.kill().await;
        }
        let server = self
            .settings
            .read()
            .map_err(|_| "Model settings are unavailable")?
            .llama_server_path
            .clone();
        let server = PathBuf::from(server);
        if !server.is_file() {
            return Err("The verified llama.cpp runtime is not installed".into());
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        drop(listener);
        let token = Uuid::new_v4().to_string();
        let endpoint = format!("http://127.0.0.1:{port}");
        let child = Command::new(server)
            .args([
                "--model",
                &model.to_string_lossy(),
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--api-key",
                &token,
                "--ctx-size",
                "8192",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Could not start llama.cpp: {e}"))?;
        for _ in 0..80 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if self
                .client
                .get(format!("{endpoint}/health"))
                .bearer_auth(&token)
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
            {
                *guard = Some(LlamaProcess {
                    child,
                    endpoint: endpoint.clone(),
                    token: token.clone(),
                    model_path: model,
                });
                return Ok((endpoint, token));
            }
        }
        let mut child = child;
        let _ = child.kill().await;
        Err("The GGUF model could not be loaded within 20 seconds".into())
    }
}

fn prompt(r: &GenerationRequest) -> String {
    let words = (r.minutes.clamp(1, 60) * 115) as usize;
    format!(
        "Write a polished English hypnosis script intended to be read aloud.\n\nGoal: {}\nLength: approximately {} words / {} minutes\nVoice: {}\nUser guidance: {}\n{}\n\nWriting rules:\n- Return only the narration. No title, headings, notes, markdown, or explanation.\n- Address the listener naturally in second person. Use varied sentence length and concrete sensory imagery.\n- Avoid clichés, repetition, exaggerated promises, and generic motivational filler.\n- Move smoothly through arrival, physical relaxation, deepening, the requested suggestions, integration, and gradual reorientation.\n- Preserve the listener's agency; use invitations such as 'you can' and 'if it feels right'. Make no medical claims.\n- Add [pause:2s] to [pause:8s] only where silence improves spoken pacing. Never describe or speak a pause marker.\n- End grounded, alert, and ready to continue the day unless the goal explicitly asks for sleep.",
        r.intention.trim(),
        words,
        r.minutes,
        r.tone.trim(),
        r.instructions.trim(),
        r.included_text
            .as_deref()
            .map(|v| format!("Material explicitly supplied by the user:\n{v}"))
            .unwrap_or_default()
    )
}
