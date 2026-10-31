use crate::domain::*;
use crate::infrastructure::runtime::Job;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

pub fn catalog() -> Vec<CatalogEntry> {
    [
        (
            "qwen-small",
            "Qwen3 · 0.6B",
            "NobodyWho/Qwen_Qwen3-0.6B-GGUF",
            "text",
            None,
            "Small, efficient local writing model.",
        ),
        (
            "qwen-writing",
            "Qwen3 · 4B",
            "Qwen/Qwen3-4B-GGUF",
            "text",
            None,
            "A larger model for detailed writing. Requires more memory.",
        ),
        (
            "kokoro",
            "Kokoro · English voices",
            "NobodyWho/Kokoro-82M",
            "speech",
            Some("kokoro"),
            "Natural English speech, including Emma and George.",
        ),
        (
            "supertonic",
            "Supertonic 3",
            "Supertone/supertonic-3",
            "speech",
            Some("supertonic"),
            "Fast speech with multiple voice styles.",
        ),
    ]
    .into_iter()
    .map(
        |(id, name, repository, capability, architecture, description)| CatalogEntry {
            id: id.into(),
            name: name.into(),
            repository: repository.into(),
            capability: capability.into(),
            architecture: architecture.map(str::to_owned),
            description: description.into(),
        },
    )
    .collect()
}

fn safe_relative(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

pub async fn plan(id: &str, root: &str) -> ApiResult<InstallPlan> {
    let catalog = catalog()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| AppError::from("Model is not in the supported catalog"))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::from(e.to_string()))?;
    let metadata = client
        .get(format!(
            "https://huggingface.co/api/models/{}",
            catalog.repository
        ))
        .send()
        .await
        .map_err(|_| AppError::from("Could not reach Hugging Face"))?
        .error_for_status()
        .map_err(|_| AppError::from("Model source is unavailable"))?
        .json::<serde_json::Value>()
        .await
        .map_err(|_| AppError::from("Invalid model metadata"))?;
    let revision = metadata["sha"]
        .as_str()
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| AppError::from("Model revision is unavailable"))?
        .to_owned();
    let tree = client
        .get(format!(
            "https://huggingface.co/api/models/{}/tree/{revision}?recursive=true",
            catalog.repository
        ))
        .send()
        .await
        .map_err(|_| AppError::from("Could not resolve model files"))?
        .error_for_status()
        .map_err(|_| AppError::from("Model file list unavailable"))?
        .json::<Vec<serde_json::Value>>()
        .await
        .map_err(|_| AppError::from("Invalid file list"))?;
    let mut files = Vec::new();
    for entry in tree {
        let Some(path) = entry["path"].as_str() else {
            continue;
        };
        if entry["type"] != "file" || !safe_relative(path) {
            continue;
        }
        let lower = path.to_ascii_lowercase();
        let keep = if catalog.capability == "text" {
            lower.ends_with(".gguf") && lower.contains("q4_k_m") && !lower.contains("mmproj")
        } else if catalog.architecture.as_deref() == Some("kokoro") {
            [
                "config.json",
                "model.onnx",
                "voices/bf_emma.safetensors",
                "voices/bm_george.safetensors",
                "voices/af_bella.safetensors",
            ]
            .contains(&path)
        } else {
            (lower.starts_with("onnx/") || lower.starts_with("voice_styles/"))
                && (lower.ends_with(".onnx") || lower.ends_with(".json") || lower.ends_with(".bin"))
        };
        if keep {
            let bytes = entry["size"]
                .as_u64()
                .ok_or_else(|| AppError::from("Model file size missing"))?;
            files.push(InstallFile {
                path: path.into(),
                bytes: bytes.to_string(),
                sha256: entry["lfs"]["oid"].as_str().map(str::to_owned),
            });
        }
    }
    if files.is_empty() {
        return Err("No compatible model files were found".into());
    }
    // Split GGUF shards must all be included. Exclude alternate non-sharded files.
    let total = files
        .iter()
        .try_fold(0_u64, |sum, f| {
            sum.checked_add(f.bytes.parse::<u64>().unwrap_or(0))
        })
        .ok_or_else(|| AppError::from("Download is too large"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let root = Path::new(root);
    if !root.is_absolute() {
        return Err("Choose an absolute model folder".into());
    }
    std::fs::create_dir_all(root).map_err(|_| AppError::from("Model folder is not writable"))?;
    let root = root
        .canonicalize()
        .map_err(|e| AppError::from(e.to_string()))?;
    let test = tempfile::NamedTempFile::new_in(&root)
        .map_err(|_| AppError::from("Model folder is not writable"))?;
    drop(test);
    let available = available_bytes(&root);
    if available.is_some_and(|bytes| bytes < total.saturating_add(100_000_000)) {
        return Err("There is not enough disk space for this model".into());
    }
    let mut license = metadata["cardData"]["license"].as_str().map(str::to_owned);
    if license.is_none()
        && catalog.id == "qwen-small"
        && let Ok(response) = client
            .get("https://huggingface.co/api/models/Qwen/Qwen3-0.6B")
            .send()
            .await
        && let Ok(upstream) = response.json::<serde_json::Value>().await
    {
        license = upstream["cardData"]["license"]
            .as_str()
            .map(|name| format!("{name} · upstream Qwen/Qwen3-0.6B"));
    }
    Ok(InstallPlan {
        id: id.clone(),
        destination: root
            .join(if catalog.capability == "text" {
                "text"
            } else {
                "speech"
            })
            .join(&id)
            .to_string_lossy()
            .into_owned(),
        catalog,
        revision,
        files,
        total_bytes: total.to_string(),
        license: license.unwrap_or_else(|| {
            "License metadata unavailable · review the source model terms".into()
        }),
    })
}

pub fn available_bytes(root: &Path) -> Option<u64> {
    let root = root.canonicalize().ok()?;
    sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .filter(|d| {
            root.starts_with(
                d.mount_point()
                    .canonicalize()
                    .unwrap_or_else(|_| d.mount_point().to_path_buf()),
            )
        })
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

pub async fn download(
    plan: &InstallPlan,
    job: &Job,
    db: &super::database::Database,
) -> Result<PathBuf, String> {
    download_from(plan, job, db, "https://huggingface.co").await
}

async fn download_from(
    plan: &InstallPlan,
    job: &Job,
    db: &super::database::Database,
    source: &str,
) -> Result<PathBuf, String> {
    let target = PathBuf::from(&plan.destination);
    let root = target
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid destination")?;
    let staging_root = root.join(".staging");
    tokio::fs::create_dir_all(&staging_root)
        .await
        .map_err(|e| e.to_string())?;
    if staging_root.canonicalize().map_err(|e| e.to_string())? != staging_root {
        return Err("Model staging must stay inside the selected root".into());
    }
    let staging = staging_root.join(&plan.id);
    db.call({
        let id = plan.id.clone();
        let root = root.to_string_lossy().into_owned();
        let path = staging.to_string_lossy().into_owned();
        move |store| store.staging_register(&id, &root, &path)
    })
    .await
    .map_err(|e| format!("{e:?}"))?;
    let result = download_files(plan, job, source, &staging, &target, root).await;
    if result.is_err() {
        let _ = tokio::fs::remove_dir_all(&staging).await;
    }
    if !staging.exists() {
        db.call({
            let id = plan.id.clone();
            move |store| store.staging_remove(&id)
        })
        .await
        .map_err(|e| format!("{e:?}"))?;
    }
    result
}

async fn download_files(
    plan: &InstallPlan,
    job: &Job,
    source: &str,
    staging: &Path,
    target: &Path,
    root: &Path,
) -> Result<PathBuf, String> {
    use tokio::io::AsyncWriteExt;
    if job.cancelled() {
        return Err("Cancelled".into());
    }
    tokio::fs::create_dir(staging)
        .await
        .map_err(|e| e.to_string())?;
    let marker = serde_json::json!({"id":plan.id,"root":root.to_string_lossy(),"repository":plan.catalog.repository,"revision":plan.revision,"name":plan.catalog.name,"capability":plan.catalog.capability,"architecture":plan.catalog.architecture,"license":plan.license,"files":plan.files});
    tokio::fs::write(staging.join(".hypnofy-model.json"), marker.to_string())
        .await
        .map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .map_err(|e| e.to_string())?;
    let total = plan
        .total_bytes
        .parse::<u64>()
        .map_err(|_| "Invalid download size")?;
    let mut completed = 0_u64;
    let mut last = std::time::Instant::now();
    for item in &plan.files {
        if !safe_relative(&item.path) {
            return Err("Unsafe model file path".into());
        }
        if job.cancelled() {
            return Err("Cancelled".into());
        }
        let path = staging.join(&item.path);
        tokio::fs::create_dir_all(path.parent().ok_or("Invalid file path")?)
            .await
            .map_err(|e| e.to_string())?;
        let url = format!(
            "{source}/{}/resolve/{}/{}",
            plan.catalog.repository, plan.revision, item.path
        );
        let response = tokio::select! {
            response=client.get(url).send()=>response.map_err(|e|e.to_string())?,
            _=job.wait_cancelled()=>return Err("Cancelled".into())
        }
        .error_for_status()
        .map_err(|e| e.to_string())?;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
            .map_err(|e| e.to_string())?;
        let mut stream = response.bytes_stream();
        let mut hash = Sha256::new();
        let mut bytes = 0_u64;
        loop {
            let chunk = tokio::select! {
                chunk=stream.next()=>chunk,
                _=job.wait_cancelled()=>return Err("Cancelled".into())
            };
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(|e| e.to_string())?;
            bytes += chunk.len() as u64;
            completed += chunk.len() as u64;
            if bytes > item.bytes.parse::<u64>().map_err(|_| "Invalid size")? {
                return Err("Download exceeds declared size".into());
            }
            hash.update(&chunk);
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            if last.elapsed() > std::time::Duration::from_millis(100) {
                job.update(|s| {
                    s.state = crate::domain::JobStatus::Running;
                    s.phase = format!("Downloading {}", item.path);
                    s.progress = Some(completed as f64 / total.max(1) as f64);
                });
                last = std::time::Instant::now();
            }
        }
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);
        if bytes.to_string() != item.bytes {
            return Err("Downloaded file is incomplete".into());
        }
        let digest = crate::document::hex(&hash.finalize());
        if item
            .sha256
            .as_ref()
            .is_some_and(|expected| expected != &digest)
        {
            return Err("Model checksum verification failed".into());
        }
    }
    if job.cancelled() {
        return Err("Cancelled".into());
    }
    let parent = target.parent().ok_or("Invalid target")?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| e.to_string())?;
    if parent.canonicalize().map_err(|e| e.to_string())? != parent {
        return Err("Installation destination points outside the selected folder".into());
    }
    tokio::fs::rename(staging, target)
        .await
        .map_err(|e| e.to_string())?;
    Ok(target.to_owned())
}

pub fn model_path(model: &ModelRecord) -> Result<PathBuf, String> {
    let path = PathBuf::from(model.path.as_deref().ok_or("Model path missing")?);
    let path = path
        .canonicalize()
        .map_err(|_| "Model files are missing. Relink or reinstall the model.".to_string())?;
    if model.capability == "text" && !path.is_file()
        || model.capability == "speech" && !path.is_dir()
    {
        return Err("Model location has changed".into());
    }
    Ok(path)
}

pub fn fingerprint(path: &Path) -> Result<String, String> {
    let mut hash = Sha256::new();
    fn walk(path: &Path, base: &Path, hash: &mut Sha256) -> Result<(), String> {
        let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Model directories cannot contain symbolic links".into());
        }
        if metadata.is_dir() {
            let mut paths = std::fs::read_dir(path)
                .map_err(|e| e.to_string())?
                .map(|e| e.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            paths.sort();
            for child in paths {
                walk(&child, base, hash)?;
            }
        } else {
            hash.update(
                path.strip_prefix(base)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .as_bytes(),
            );
            hash.update(metadata.len().to_le_bytes());
            hash.update(
                metadata
                    .modified()
                    .map_err(|e| e.to_string())?
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
                    .to_le_bytes(),
            );
        }
        Ok(())
    }
    walk(path, path, &mut hash)?;
    Ok(crate::document::hex(&hash.finalize()))
}

pub fn voices(id: &str, architecture: &str, path: &Path) -> Vec<Voice> {
    let directory = path.join(if architecture == "kokoro" {
        "voices"
    } else {
        "voice_styles"
    });
    let Ok(files) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut voices = Vec::new();
    for file in files.flatten() {
        if !file.file_type().is_ok_and(|f| f.is_file()) {
            continue;
        }
        let path = file.path();
        let expected = if architecture == "kokoro" {
            "safetensors"
        } else {
            "json"
        };
        if !path.extension().is_some_and(|e| e == expected) {
            continue;
        }
        let Some(key) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        else {
            continue;
        };
        let language = if architecture == "supertonic" {
            "en"
        } else {
            match key.get(..2) {
                Some("af" | "am") => "en-us",
                Some("bf" | "bm") => "en-gb",
                Some("ef" | "em") => "es",
                Some("ff" | "fm") => "fr",
                Some("if" | "im") => "it",
                Some("pf" | "pm") => "pt-br",
                _ => continue,
            }
        };
        let name = key.split_once('_').map(|(_, name)| name).unwrap_or(key);
        let mut letters = name.chars();
        let name = letters
            .next()
            .map(|first| first.to_uppercase().collect::<String>() + letters.as_str())
            .unwrap_or_default();
        voices.push(Voice {
            id: format!("{id}/{key}"),
            name,
            language: language.into(),
        });
    }
    voices.sort_by(|a, b| a.name.cmp(&b.name));
    voices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{database::Database, runtime::Jobs};

    fn server(body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        });
        (format!("http://{address}"), thread)
    }

    fn plan(root: &Path, body: &[u8], checksum: &str) -> InstallPlan {
        let id = uuid::Uuid::new_v4().to_string();
        InstallPlan {
            id: id.clone(),
            catalog: catalog().remove(0),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            files: vec![InstallFile {
                path: "model.gguf".into(),
                bytes: body.len().to_string(),
                sha256: Some(checksum.into()),
            }],
            total_bytes: body.len().to_string(),
            destination: root.join("text").join(id).to_string_lossy().into_owned(),
            license: "synthetic test fixture".into(),
        }
    }

    #[test]
    fn publishes_only_verified_downloads_and_cleans_failed_staging() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let directory = tempfile::tempdir().unwrap();
        runtime.block_on(async {
            let db = Database::open(directory.path().join("app")).unwrap();
            let root = directory.path().join("Models");
            std::fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let body = b"GGUF synthetic fixture".to_vec();
            for valid in [true, false] {
                let checksum = if valid {
                    crate::document::hex(&Sha256::digest(&body))
                } else {
                    "0".repeat(64)
                };
                let plan = plan(&root, &body, &checksum);
                let (source, server) = server(body.clone());
                let jobs = Jobs::default();
                let (job, _) = jobs
                    .start(&uuid::Uuid::new_v4().to_string(), "model-install")
                    .unwrap();
                let result = download_from(&plan, &job, &db, &source).await;
                if valid {
                    assert_eq!(
                        std::fs::read(result.unwrap().join("model.gguf")).unwrap(),
                        body
                    );
                } else {
                    assert!(result.is_err());
                    assert!(!Path::new(&plan.destination).exists());
                }
                server.join().unwrap();
                assert!(!root.join(".staging").join(&plan.id).exists());
                assert!(db.call(|s| s.staging_records()).await.unwrap().is_empty());
            }
        });
        directory.close().unwrap();
    }

    #[test]
    fn cancellation_and_unsafe_paths_never_publish_a_model() {
        assert!(!safe_relative("../outside.gguf"));
        assert!(!safe_relative("C:/outside.gguf"));
        assert!(safe_relative("voices/bf_emma.safetensors"));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let directory = tempfile::tempdir().unwrap();
        runtime.block_on(async {
            let db = Database::open(directory.path().join("app")).unwrap();
            let root = directory.path().join("Models");
            std::fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let plan = plan(&root, b"test", &"0".repeat(64));
            let jobs = Jobs::default();
            let id = uuid::Uuid::new_v4().to_string();
            let (job, _) = jobs.start(&id, "model-install").unwrap();
            jobs.cancel(&id).unwrap();
            assert!(
                download_from(&plan, &job, &db, "http://127.0.0.1:9")
                    .await
                    .is_err()
            );
            assert!(!Path::new(&plan.destination).exists());
            assert!(db.call(|s| s.staging_records()).await.unwrap().is_empty());
        });
        directory.close().unwrap();
    }
}
