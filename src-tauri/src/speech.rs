use crate::config::{AssetConfig, AssetManifest};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Emitter};
use tokio::{io::AsyncWriteExt, process::Command};
use uuid::Uuid;

pub struct SpeechManager {
    data_dir: PathBuf,
    client: reqwest::Client,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    engine: AssetConfig,
    voice_pack: AssetConfig,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub accent: String,
    pub gender: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatus {
    pub installed: bool,
    pub downloading: bool,
    pub engine_version: String,
    pub model_version: String,
    pub download_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechEvent {
    pub job_id: String,
    pub status: String,
    pub progress: f32,
    pub message: String,
    pub manifest_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesisRequest {
    pub script_id: String,
    pub revision: i64,
    pub text: String,
    pub voice: String,
    pub speed: f32,
    pub paragraph_pause: f32,
    pub preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderManifest {
    pub script_id: String,
    pub revision: i64,
    pub voice: String,
    pub speed: f32,
    pub sample_rate: u32,
    pub items: Vec<RenderItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RenderItem {
    Audio { path: String },
    Pause { seconds: f32 },
}

impl SpeechManager {
    pub fn new(data_dir: PathBuf, assets: &AssetManifest) -> Result<Self, String> {
        Ok(Self {
            data_dir,
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(15))
                .timeout(std::time::Duration::from_secs(900))
                .build()
                .map_err(|error| format!("Could not create voice download client: {error}"))?,
            jobs: Mutex::new(HashMap::new()),
            engine: assets.by_kind("engine")?.clone(),
            voice_pack: assets.by_kind("voice-pack")?.clone(),
        })
    }
    fn engine_root(&self) -> PathBuf {
        self.data_dir.join(&self.engine.install_directory)
    }
    fn voice_root(&self) -> PathBuf {
        self.data_dir.join(&self.voice_pack.install_directory).join(
            Path::new(&self.voice_pack.expected_file)
                .parent()
                .unwrap_or(Path::new("")),
        )
    }
    fn executable(&self) -> Option<PathBuf> {
        let filename = Path::new(&self.engine.expected_file)
            .file_name()?
            .to_str()?;
        find_file(&self.engine_root(), filename)
    }
    pub fn status(&self) -> SpeechStatus {
        SpeechStatus {
            installed: self.executable().is_some()
                && self.voice_root().join("model.onnx").is_file(),
            downloading: false,
            engine_version: format!("sherpa-onnx {}", self.engine.version),
            model_version: format!("Kokoro {}", self.voice_pack.version),
            download_bytes: self.engine.bytes + self.voice_pack.bytes,
        }
    }
    pub fn voices(&self) -> Vec<Voice> {
        let installed = self.status().installed;
        [
            ("af_heart", "Heart", "US", "F"),
            ("af_bella", "Bella", "US", "F"),
            ("af_nicole", "Nicole", "US", "F"),
            ("af_sarah", "Sarah", "US", "F"),
            ("af_sky", "Sky", "US", "F"),
            ("am_adam", "Adam", "US", "M"),
            ("am_michael", "Michael", "US", "M"),
            ("bf_emma", "Emma", "UK", "F"),
            ("bf_isabella", "Isabella", "UK", "F"),
            ("bm_george", "George", "UK", "M"),
            ("bm_lewis", "Lewis", "UK", "M"),
        ]
        .into_iter()
        .map(|(id, name, accent, gender)| Voice {
            id: id.into(),
            name: name.into(),
            accent: accent.into(),
            gender: gender.into(),
            installed,
        })
        .collect()
    }
    pub fn install(self: Arc<Self>, app: AppHandle) -> Result<String, String> {
        if self
            .jobs
            .lock()
            .map_err(|_| "Speech jobs are unavailable")?
            .values()
            .any(|_| true)
        {
            return Err("A speech job is already running".into());
        }
        let id = Uuid::new_v4().to_string();
        let flag = Arc::new(AtomicBool::new(false));
        self.jobs
            .lock()
            .map_err(|_| "Speech jobs are unavailable")?
            .insert(id.clone(), flag.clone());
        let job = id.clone();
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = manager.install_inner(&app, &job, &flag).await;
            let event = match result {
                Ok(()) => SpeechEvent {
                    job_id: job.clone(),
                    status: "complete".into(),
                    progress: 1.0,
                    message: "Voices are ready".into(),
                    manifest_path: None,
                    error: None,
                },
                Err(e) => SpeechEvent {
                    job_id: job.clone(),
                    status: "error".into(),
                    progress: 0.0,
                    message: "Voice setup failed".into(),
                    manifest_path: None,
                    error: Some(e),
                },
            };
            let _ = app.emit("speech-event", event);
            if let Ok(mut jobs) = manager.jobs.lock() {
                jobs.remove(&job);
            }
        });
        Ok(id)
    }
    async fn install_inner(
        &self,
        app: &AppHandle,
        id: &str,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        // Download to partial files, verify identity, then inspect archive paths before extraction.
        fs::create_dir_all(self.data_dir.join("downloads")).map_err(err)?;
        fs::create_dir_all(self.engine_root()).map_err(err)?;
        fs::create_dir_all(self.data_dir.join("models")).map_err(err)?;
        let engine_archive = self
            .download(app, id, "Downloading voice engine", &self.engine, cancel)
            .await?;
        if cancel.load(Ordering::Relaxed) {
            return Err("Download cancelled".into());
        }
        let voice_archive = self
            .download(
                app,
                id,
                "Downloading Kokoro voices",
                &self.voice_pack,
                cancel,
            )
            .await?;
        emit(app, id, "Extracting verified engine", 0.94);
        extract_checked(&engine_archive, &self.engine_root()).await?;
        emit(app, id, "Extracting verified voice pack", 0.97);
        extract_checked(&voice_archive, &self.data_dir.join("models")).await?;
        if self.executable().is_none() || !self.voice_root().join("model.onnx").is_file() {
            return Err("Verified archives did not contain the expected runtime files".into());
        }
        Ok(())
    }
    async fn download(
        &self,
        app: &AppHandle,
        id: &str,
        label: &str,
        asset: &AssetConfig,
        cancel: &AtomicBool,
    ) -> Result<PathBuf, String> {
        let path = self.data_dir.join("downloads").join(&asset.filename);
        if path.is_file() && sha_file(&path).map_err(err)? == asset.sha256 {
            return Ok(path);
        }
        let response = self.client.get(&asset.url).send().await.map_err(err)?;
        if !response.status().is_success() {
            return Err(format!("Download returned {}", response.status()));
        }
        if response.content_length().unwrap_or(asset.bytes) > asset.bytes {
            return Err("Download is larger than the reviewed asset".into());
        }
        let temp = path.with_extension("partial");
        let mut file = tokio::fs::File::create(&temp).await.map_err(err)?;
        let mut stream = response.bytes_stream();
        let mut total = 0u64;
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                let _ = tokio::fs::remove_file(&temp).await;
                return Err("Download cancelled".into());
            }
            let bytes = chunk.map_err(err)?;
            total += bytes.len() as u64;
            if total > asset.bytes {
                return Err("Download exceeded its reviewed size".into());
            }
            file.write_all(&bytes).await.map_err(err)?;
            emit(app, id, label, (total as f32 / asset.bytes as f32) * 0.9)
        }
        file.flush().await.map_err(err)?;
        drop(file);
        let actual = sha_file(&temp).map_err(err)?;
        if actual != asset.sha256 {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err("Downloaded asset failed SHA-256 verification".into());
        }
        tokio::fs::rename(temp, &path).await.map_err(err)?;
        Ok(path)
    }
    pub fn synthesize(
        self: Arc<Self>,
        app: AppHandle,
        request: SynthesisRequest,
    ) -> Result<String, String> {
        validate_script(&request.text)?;
        if self.executable().is_none() {
            return Err("Install the local voice pack first".into());
        }
        if !(0.5..=2.0).contains(&request.speed) {
            return Err("Voice speed must be between 0.5× and 2×".into());
        }
        let id = Uuid::new_v4().to_string();
        let flag = Arc::new(AtomicBool::new(false));
        self.jobs
            .lock()
            .map_err(|_| "Speech jobs are unavailable")?
            .insert(id.clone(), flag.clone());
        let job = id.clone();
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = manager.synthesize_inner(&app, &job, &request, &flag).await;
            let event = match result {
                Ok(path) => SpeechEvent {
                    job_id: job.clone(),
                    status: "complete".into(),
                    progress: 1.0,
                    message: "Narration is ready".into(),
                    manifest_path: Some(path.to_string_lossy().into_owned()),
                    error: None,
                },
                Err(e) => SpeechEvent {
                    job_id: job.clone(),
                    status: if flag.load(Ordering::Relaxed) {
                        "cancelled"
                    } else {
                        "error"
                    }
                    .into(),
                    progress: 0.0,
                    message: "Narration stopped".into(),
                    manifest_path: None,
                    error: Some(e),
                },
            };
            let _ = app.emit("speech-event", event);
            if let Ok(mut jobs) = manager.jobs.lock() {
                jobs.remove(&job);
            }
        });
        Ok(id)
    }
    pub fn cancel(&self, id: &str) {
        if let Ok(jobs) = self.jobs.lock()
            && let Some(flag) = jobs.get(id)
        {
            flag.store(true, Ordering::Relaxed)
        }
    }
    async fn synthesize_inner(
        &self,
        app: &AppHandle,
        id: &str,
        r: &SynthesisRequest,
        cancel: &AtomicBool,
    ) -> Result<PathBuf, String> {
        // Immutable chunk keys let edits regenerate only changed passages.
        let executable = self.executable().ok_or("Voice engine is missing")?;
        let voice = self.voice_root();
        let segments = segments(&r.text, r.paragraph_pause)?;
        let total = segments
            .iter()
            .filter(|s| matches!(s, Segment::Text(_)))
            .count()
            .max(1);
        let mut completed = 0;
        let mut items = Vec::new();
        let path = self.data_dir.join("audio").join(format!("render-{}-{}-{id}.json", r.script_id, r.revision));
        for segment in segments {
            if cancel.load(Ordering::Relaxed) {
                return Err("Narration cancelled".into());
            }
            match segment {
                Segment::Pause(seconds) => items.push(RenderItem::Pause { seconds }),
                Segment::Text(text) => {
                    let mut hasher = Sha256::new();
                    hasher.update(format!(
                        "sherpa-{}|kokoro-{}|{}|{:.3}|v1|{}",
                        self.engine.version,
                        self.voice_pack.version,
                        r.voice,
                        r.speed,
                        normalize(&text)
                    ));
                    let key = hex_bytes(&hasher.finalize());
                    let output = self
                        .data_dir
                        .join("audio/chunks")
                        .join(format!("{key}.wav"));
                    if !output.is_file() {
                        let temp = output.with_extension("partial.wav");
                        let output_result = Command::new(&executable)
                            .args([
                                format!("--kokoro-model={}", voice.join("model.onnx").display()),
                                format!("--kokoro-voices={}", voice.join("voices.bin").display()),
                                format!("--kokoro-tokens={}", voice.join("tokens.txt").display()),
                                format!(
                                    "--kokoro-data-dir={}",
                                    voice.join("espeak-ng-data").display()
                                ),
                                format!(
                                    "--kokoro-lexicon={},{}",
                                    voice.join("lexicon-us-en.txt").display(),
                                    voice.join("lexicon-zh.txt").display()
                                ),
                                "--num-threads=2".into(),
                                format!("--sid={}", speaker_id(&r.voice)),
                                format!("--speed={}", r.speed),
                                format!("--output-filename={}", temp.display()),
                                text,
                            ])
                            .output()
                            .await
                            .map_err(err)?;
                        if !output_result.status.success() {
                            let detail = String::from_utf8_lossy(&output_result.stderr);
                            return Err(format!("Kokoro could not synthesize this passage: {}", detail.lines().last().unwrap_or("native engine failed")));
                        }
                        tokio::fs::rename(temp, &output).await.map_err(err)?;
                    }
                    items.push(RenderItem::Audio {
                        path: output.to_string_lossy().into_owned(),
                    });
                    completed += 1;
                    let partial = RenderManifest { script_id: r.script_id.clone(), revision: r.revision, voice: r.voice.clone(), speed: r.speed, sample_rate: 24000, items: items.clone() };
                    tokio::fs::write(&path, serde_json::to_vec(&partial).map_err(err)?).await.map_err(err)?;
                    emit_render(app, id, if completed == 1 { "ready" } else { "chunk" }, completed as f32 / total as f32, &path);
                    emit(
                        app,
                        id,
                        if r.preview {
                            "Preparing preview"
                        } else {
                            "Preparing narration"
                        },
                        completed as f32 / total as f32,
                    );
                }
            }
        }
        let manifest = RenderManifest {
            script_id: r.script_id.clone(),
            revision: r.revision,
            voice: r.voice.clone(),
            speed: r.speed,
            sample_rate: 24000,
            items,
        };
        tokio::fs::write(&path, serde_json::to_vec_pretty(&manifest).map_err(err)?)
            .await
            .map_err(err)?;
        Ok(path)
    }
}

enum Segment {
    Text(String),
    Pause(f32),
}
fn segments(input: &str, paragraph_pause: f32) -> Result<Vec<Segment>, String> {
    let mut result = Vec::new();
    for (pi, paragraph) in input.split("\n\n").enumerate() {
        if pi > 0 && paragraph_pause > 0.0 {
            result.push(Segment::Pause(paragraph_pause))
        }
        let mut rest = paragraph.trim();
        while !rest.is_empty() {
            if let Some(start) = rest.find("[pause:") {
                let before = &rest[..start];
                push_chunks(before, &mut result);
                let after = &rest[start + 7..];
                let end = after.find(']').ok_or_else(|| {
                    "A [pause:…] marker is missing its closing bracket".to_string()
                })?;
                let value = after[..end]
                    .trim_end_matches('s')
                    .parse::<f32>()
                    .map_err(|_| "Pause markers must look like [pause:5s]".to_string())?;
                if !(0.1..=120.0).contains(&value) {
                    return Err("Pause markers must be between 0.1 and 120 seconds".into());
                }
                result.push(Segment::Pause(value));
                rest = &after[end + 1..];
            } else {
                push_chunks(rest, &mut result);
                break;
            }
        }
    }
    Ok(result)
}
fn push_chunks(text: &str, out: &mut Vec<Segment>) {
    let clean = normalize(text);
    if clean.is_empty() {
        return;
    }
    let mut current = String::new();
    for word in clean.split_whitespace() {
        if current.len() + word.len() + 1 > 250 {
            out.push(Segment::Text(current));
            current = String::new()
        }
        if !current.is_empty() {
            current.push(' ')
        }
        current.push_str(word);
        if current.len() > 80 && word.ends_with(['.', '!', '?', ';', ':']) {
            out.push(Segment::Text(current));
            current = String::new()
        }
    }
    if !current.is_empty() {
        out.push(Segment::Text(current))
    }
}
fn validate_script(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("Add some script text before creating narration".into());
    }
    let _ = segments(text, 0.0)?;
    Ok(())
}
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn speaker_id(voice: &str) -> u32 {
    match voice {
        "af_heart" => 3,
        "af_bella" => 2,
        "af_nicole" => 6,
        "af_sarah" => 9,
        "af_sky" => 10,
        "am_adam" => 11,
        "am_michael" => 16,
        "bf_emma" => 21,
        "bf_isabella" => 22,
        "bm_george" => 26,
        "bm_lewis" => 27,
        _ => 3,
    }
}
fn emit(app: &AppHandle, id: &str, message: &str, progress: f32) {
    let _ = app.emit(
        "speech-event",
        SpeechEvent {
            job_id: id.into(),
            status: "working".into(),
            progress,
            message: message.into(),
            manifest_path: None,
            error: None,
        },
    );
}
fn emit_render(app: &AppHandle, id: &str, status: &str, progress: f32, path: &Path) {
    let _ = app.emit("speech-event", SpeechEvent { job_id: id.into(), status: status.into(), progress, message: "Narration is playing while the rest is prepared".into(), manifest_path: Some(path.to_string_lossy().into_owned()), error: None });
}
fn sha_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_bytes(&hasher.finalize()))
}
fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char)
    }
    out
}
async fn extract_checked(archive: &Path, destination: &Path) -> Result<(), String> {
    let listing = Command::new("tar.exe")
        .args(["-tf", &archive.to_string_lossy()])
        .output()
        .await
        .map_err(|e| format!("Windows tar is required: {e}"))?;
    if !listing.status.success() {
        return Err("Could not inspect the downloaded archive".into());
    }
    let names = String::from_utf8_lossy(&listing.stdout);
    if names.lines().count() > 5000
        || names.lines().any(|n| {
            let p = Path::new(n);
            p.is_absolute()
                || p.components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
        })
    {
        return Err("The downloaded archive contains unsafe paths".into());
    }
    fs::create_dir_all(destination).map_err(err)?;
    let status = Command::new("tar.exe")
        .args([
            "-xf",
            &archive.to_string_lossy(),
            "-C",
            &destination.to_string_lossy(),
        ])
        .status()
        .await
        .map_err(err)?;
    if !status.success() {
        return Err("Could not extract the verified archive".into());
    }
    Ok(())
}
fn find_file(root: &Path, name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()?
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = find_file(&path, name)
        {
            return Some(found);
        }
    }
    None
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
