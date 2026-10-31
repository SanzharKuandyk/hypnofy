use crate::{
    bootstrap::App,
    domain::*,
    infrastructure::{downloads, runtime::Job},
};
use hypnofy_inference_protocol::{Architecture, Device, Operation};
use std::path::{Path, PathBuf};

pub fn record(
    path: &Path,
    architecture: Option<String>,
    ownership: &str,
    id: Option<String>,
) -> ApiResult<ModelRecord> {
    let path = path
        .canonicalize()
        .map_err(|_| AppError::from("The model location is missing"))?;
    let capability = if path.is_file()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
    {
        "text"
    } else if path.is_dir() {
        "speech"
    } else {
        return Err("Choose a GGUF file or a supported speech model folder".into());
    };
    let architecture = if capability == "speech" {
        Some(architecture.unwrap_or_else(|| {
            if path.join("onnx").is_dir() {
                "supertonic".into()
            } else {
                "kokoro".into()
            }
        }))
    } else {
        None
    };
    if capability == "speech"
        && !["kokoro", "supertonic"].contains(&architecture.as_deref().unwrap_or(""))
    {
        return Err("Speech architecture is unsupported".into());
    }
    if capability == "text" {
        use std::io::Read;
        let mut header = [0; 4];
        std::fs::File::open(&path)
            .and_then(|mut f| f.read_exact(&mut header))
            .map_err(|_| AppError::from("Could not read GGUF header"))?;
        if &header != b"GGUF" {
            return Err("File is not a GGUF model".into());
        }
    }
    let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut voices = architecture
        .as_deref()
        .map(|a| downloads::voices(&id, a, &path))
        .unwrap_or_default();
    voices.retain(|v| {
        let key = v.id.rsplit('/').next().unwrap_or("");
        if architecture.as_deref() == Some("kokoro") {
            path.join("voices")
                .join(format!("{key}.safetensors"))
                .exists()
        } else {
            path.join("voice_styles")
                .join(format!("{key}.json"))
                .exists()
        }
    });
    Ok(ModelRecord {
        id,
        name: path
            .file_name()
            .and_then(|p| p.to_str())
            .unwrap_or("Local model")
            .into(),
        provider: "nobodywho".into(),
        capability: capability.into(),
        architecture,
        path: Some(crate::infrastructure::filesystem::display_path(
            &path.to_string_lossy(),
        )),
        ownership: ownership.into(),
        status: "available".into(),
        reason: Some("Verify to load this model before using it".into()),
        fingerprint: None,
        context_tokens: None,
        voices,
    })
}

pub async fn verify(app: &App, mut model: ModelRecord, job: Job) -> Result<ModelRecord, String> {
    let path = downloads::model_path(&model)?;
    let fingerprint = downloads::fingerprint(&path)?;
    let staging = app.root.join("audio/.staging");
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let workspace = tempfile::Builder::new()
        .prefix("verify-")
        .tempdir_in(staging)
        .map_err(|e| e.to_string())?;
    let operation = if model.capability == "text" {
        Operation::Generate {
            model_path: path.to_string_lossy().into_owned(),
            prompt: "Say ready in one word.".into(),
            system_prompt: "Reply with only one word.".into(),
            context_tokens: 2048,
            device: Device::Auto,
        }
    } else {
        let voice = model
            .voices
            .first()
            .ok_or("No compatible voice files were found")?;
        Operation::Synthesize {
            model_path: path.to_string_lossy().into_owned(),
            architecture: if model.architecture.as_deref() == Some("supertonic") {
                Architecture::Supertonic
            } else {
                Architecture::Kokoro
            },
            voice: voice.id.rsplit('/').next().ok_or("Invalid voice")?.into(),
            language: voice.language.clone(),
            speed: 1.0,
            text: "Take a gentle breath.".into(),
            device: Device::Auto,
        }
    };
    let result = app
        .inference
        .run(job.clone(), operation, workspace.path().to_owned())
        .await;
    let result = match result {
        Ok(value) => value,
        Err(error) => {
            model.status = if job.cancelled() {
                "available"
            } else {
                "invalid"
            }
            .into();
            model.reason = Some(if job.cancelled() {
                "Verification cancelled; verify again before use".into()
            } else {
                error.clone()
            });
            model.fingerprint = None;
            let _ = app.db.call(move |s| s.model_save(&model)).await;
            return Err(error);
        }
    };
    if downloads::fingerprint(&path)? != fingerprint {
        return Err("The model files changed during verification. Verify them again.".into());
    }
    job.begin_commit()?;
    model.fingerprint = Some(fingerprint);
    model.context_tokens = result.context_tokens;
    model.status = "ready".into();
    model.reason = None;
    let copy = model.clone();
    app.db
        .call(move |s| s.model_save(&copy))
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(model)
}

pub async fn scan(app: &App, job: Job) -> ApiResult<Vec<ModelRecord>> {
    let preferences = app.db.call(|s| s.preferences()).await?;
    let mut current = app.db.call(|s| s.model_records()).await?;
    let root = PathBuf::from(preferences.model_root);
    let scan_job = job.clone();
    let paths = tokio::task::spawn_blocking(move || {
        crate::infrastructure::filesystem::model_locations(&root, &scan_job)
    })
    .await
    .map_err(|e| AppError::from(e.to_string()))?
    .map_err(AppError::from)?;
    for path in paths {
        if job.cancelled() {
            return Err("Cancelled".into());
        }
        if current.iter().any(|model| {
            model
                .path
                .as_deref()
                .is_some_and(|value| Path::new(value).canonicalize().ok().as_ref() == Some(&path))
        }) {
            continue;
        }
        if let Ok(mut model) = record(&path, None, "linked", None) {
            // A crash after publication but before registration must not turn an
            // owned installation into an unremovable linked model on rescan.
            let directory = if path.is_file() {
                path.parent()
            } else {
                Some(path.as_path())
            };
            if let Some(directory) = directory
                && let Ok(bytes) = std::fs::read(directory.join(".hypnofy-model.json"))
                && let Ok(marker) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && let Some(id) = marker["id"].as_str()
                && uuid::Uuid::parse_str(id).is_ok()
                && let Some(recorded_root) = marker["root"]
                    .as_str()
                    .and_then(|p| Path::new(p).canonicalize().ok())
                && directory == recorded_root.join(&model.capability).join(id)
            {
                model = record(&path, model.architecture, "managed", Some(id.into()))?;
                if let Some(name) = marker["name"].as_str() {
                    model.name = name.into();
                }
            }
            let copy = model.clone();
            app.db.call(move |s| s.model_save(&copy)).await?;
            current.push(model);
        }
    }
    for model in &mut current {
        if job.cancelled() {
            return Err("Cancelled".into());
        }
        if model.provider == "openrouter" {
            continue;
        }
        if let Ok(path) = downloads::model_path(model) {
            if model.status == "ready" && downloads::fingerprint(&path).ok() != model.fingerprint {
                model.status = "available".into();
                model.reason = Some("Model files changed; verify again".into());
            }
        } else {
            model.status = "missing".into();
            model.reason = Some("Relink this model or install it again".into());
        }
        let copy = model.clone();
        app.db.call(move |s| s.model_save(&copy)).await?;
    }
    Ok(current)
}

pub async fn remove(app: &App, id: String) -> ApiResult<()> {
    let model = app.model(&id).await?;
    let _maintenance = app.jobs.maintenance()?;
    app.inference.release().await.map_err(AppError::from)?;
    if model.ownership == "managed" {
        let path = PathBuf::from(
            model
                .path
                .as_deref()
                .ok_or_else(|| AppError::from("Model location missing"))?,
        );
        let directory = if model.capability == "text" {
            path.parent()
                .ok_or_else(|| AppError::from("Invalid installation"))?
                .to_owned()
        } else {
            path
        };
        if directory.exists() {
            let directory = directory
                .canonicalize()
                .map_err(|e| AppError::from(e.to_string()))?;
            let marker =
                std::fs::read_to_string(directory.join(".hypnofy-model.json")).map_err(|_| {
                    AppError::from("Ownership manifest is missing; unlink this model instead")
                })?;
            let marker: serde_json::Value = serde_json::from_str(&marker)
                .map_err(|_| AppError::from("Invalid ownership manifest"))?;
            let root = PathBuf::from(
                marker["root"]
                    .as_str()
                    .ok_or_else(|| AppError::from("Installation root missing"))?,
            )
            .canonicalize()
            .map_err(|e| AppError::from(e.to_string()))?;
            if !directory.starts_with(&root)
                || directory.parent() != Some(root.join(&model.capability).as_path())
                || marker["id"] != id
                || directory.file_name().and_then(|s| s.to_str()) != Some(id.as_str())
                || directory
                    != root
                        .join(&model.capability)
                        .join(&id)
                        .canonicalize()
                        .map_err(|e| AppError::from(e.to_string()))?
            {
                return Err("Installation ownership check failed".into());
            }
            std::fs::remove_dir_all(directory).map_err(|_| {
                AppError::from(
                    "Model files are still in use. Try again when playback and jobs stop.",
                )
            })?;
        }
    }
    app.db
        .call(move |s| {
            s.model_remove(&id)?;
            let mut p = s.preferences()?;
            if p.text_model_id.as_ref() == Some(&id) {
                p.text_model_id = None;
            }
            if p.voice_id
                .as_ref()
                .is_some_and(|v| v.starts_with(&format!("{id}/")))
            {
                p.voice_id = None;
            }
            s.json_set(
                "preferences",
                &serde_json::to_string(&p).map_err(|e| AppError::from(e.to_string()))?,
            )
        })
        .await
}
