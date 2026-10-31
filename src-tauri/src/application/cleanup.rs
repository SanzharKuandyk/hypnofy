use crate::{bootstrap::App, domain::*};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

fn error(e: impl std::fmt::Display) -> AppError {
    e.to_string().into()
}

pub async fn storage_info(app: &App) -> ApiResult<StorageInfo> {
    let preferences = app.db.call(|s| s.preferences()).await?;
    let retained = app.db.call(|s| s.retained_render_ids()).await?;
    let playing = app
        .audio
        .synchronized_state()
        .await
        .map_err(error)?
        .render_id;
    let candidates = audio_candidates(&app.root, &retained, playing.as_deref()).map_err(error)?;
    let reclaimable = candidates
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum::<u64>();
    let backups = std::fs::read_dir(&app.root)
        .map_err(error)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_name()
                .to_str()
                .filter(|n| n.starts_with("hypnofy-before-") && n.ends_with(".sqlite3"))
                .map(str::to_owned)
        })
        .collect();
    Ok(StorageInfo {
        available_model_bytes: crate::infrastructure::downloads::available_bytes(
            std::path::Path::new(&preferences.model_root),
        )
        .map(|n| n.to_string()),
        data_directory: app.root.to_string_lossy().into_owned(),
        model_root: preferences.model_root,
        reclaimable_bytes: reclaimable.to_string(),
        backups,
    })
}

pub async fn cleanup(app: &App) -> ApiResult<()> {
    let _maintenance = app.jobs.maintenance()?;
    app.inference.release().await.map_err(error)?;
    recover_staging(&app.root, true).map_err(error)?;
    app.db.call(|s| recover_installations(s, true)).await?;
    let retained = app.db.call(|s| s.retained_render_ids()).await?;
    let playing = app
        .audio
        .synchronized_state()
        .await
        .map_err(error)?
        .render_id;
    for path in audio_candidates(&app.root, &retained, playing.as_deref()).map_err(error)? {
        // Only UUID render files and content-addressed units under fixed owned roots.
        std::fs::remove_file(path)
            .map_err(|_| AppError::from("An audio file is in use; cleanup will retry later"))?;
    }
    app.db.call(|s| s.remove_unplayed_previews()).await?;
    Ok(())
}

pub fn recover_installations(store: &crate::storage::Store, strict: bool) -> ApiResult<()> {
    for (id, root, path) in store.staging_records()? {
        match remove_installation_staging(&id, Path::new(&root), Path::new(&path)) {
            Ok(()) => store.staging_remove(&id)?,
            Err(message) if strict => return Err(message.into()),
            Err(_) => {} // Keep the ownership record; a later cleanup retries locked files.
        }
    }
    Ok(())
}

fn remove_installation_staging(id: &str, root: &Path, path: &Path) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err("Invalid owned staging ID".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    let directory = root.join(".staging");
    if directory.canonicalize().map_err(|e| e.to_string())? != directory {
        return Err("Model staging must remain inside its original root".into());
    }
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if canonical != directory.join(id) || canonical.parent() != Some(directory.as_path()) {
        return Err("Staging ownership check failed".into());
    }
    let marker = canonical.join(".hypnofy-model.json");
    if !marker.exists() {
        return std::fs::remove_dir(canonical)
            .map_err(|_| "Unrecognized staging files were preserved".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(marker).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let recorded_root = PathBuf::from(value["root"].as_str().ok_or("Staging root is missing")?)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if value["id"] != id || recorded_root != root {
        return Err("Staging ownership manifest did not match".into());
    }
    std::fs::remove_dir_all(canonical)
        .map_err(|_| "Owned staging files are in use. Try cleanup again later.".into())
}

fn audio_candidates(
    root: &Path,
    retained: &HashSet<String>,
    playing: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    let mut units = HashSet::new();
    for directory in ["audio/renders", "audio/previews"] {
        let directory = root
            .join(directory)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        for entry in std::fs::read_dir(&directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                continue;
            }
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if uuid::Uuid::parse_str(id).is_err()
                || !path.extension().is_some_and(|e| e == "json" || e == "wav")
            {
                continue;
            }
            if retained.contains(id) || playing == Some(id) {
                if path.extension().is_some_and(|e| e == "json") {
                    let value: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
                    for unit in value["units"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str())
                    {
                        units.insert(unit.to_owned());
                    }
                }
            } else {
                candidates.push(path);
            }
        }
    }
    for entry in std::fs::read_dir(root.join("audio/chunks")).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if entry.file_type().map_err(|e| e.to_string())?.is_file()
            && path.extension().is_some_and(|e| e == "wav")
            && path.file_stem().and_then(|s| s.to_str()).is_some_and(|s| {
                s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) && !units.contains(s)
            })
        {
            candidates.push(path);
        }
    }
    Ok(candidates)
}

/// At process startup no old owned job can still publish output. Windows Job
/// Objects terminate its worker before abandoned directories are considered.
pub fn recover_staging(root: &Path, strict: bool) -> anyhow::Result<()> {
    let directory = root.join("audio/.staging").canonicalize()?;
    for entry in std::fs::read_dir(&directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.file_type()?.is_dir()
            && ["worker-", "render-", "verify-", "draft-"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        {
            let path = entry.path().canonicalize()?;
            if path.parent() == Some(directory.as_path())
                && let Err(error) = std::fs::remove_dir_all(path)
                && strict
            {
                return Err(error.into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_and_playing_audio_stay_protected_while_orphans_are_reclaimed() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for path in ["audio/renders", "audio/previews", "audio/chunks"] {
            std::fs::create_dir_all(root.join(path)).unwrap();
        }
        let saved = uuid::Uuid::new_v4().to_string();
        let orphan = uuid::Uuid::new_v4().to_string();
        let playing = uuid::Uuid::new_v4().to_string();
        for (id, hash, folder) in [
            (&saved, "a".repeat(64), "audio/renders"),
            (&orphan, "b".repeat(64), "audio/renders"),
            (&playing, "c".repeat(64), "audio/previews"),
        ] {
            std::fs::write(root.join(folder).join(format!("{id}.wav")), b"fixture").unwrap();
            std::fs::write(
                root.join(folder).join(format!("{id}.json")),
                serde_json::json!({"units":[hash]}).to_string(),
            )
            .unwrap();
            std::fs::write(
                root.join("audio/chunks").join(format!("{hash}.wav")),
                b"fixture",
            )
            .unwrap();
        }
        let user_file = root.join("my-export.wav");
        std::fs::write(&user_file, b"user export").unwrap();
        let candidates =
            audio_candidates(root, &HashSet::from([saved.clone()]), Some(&playing)).unwrap();
        assert_eq!(candidates.len(), 3);
        assert!(
            candidates.contains(
                &root
                    .join("audio/chunks")
                    .join(format!("{}.wav", "b".repeat(64)))
            )
        );
        for path in candidates {
            std::fs::remove_file(path).unwrap();
        }
        assert!(
            root.join("audio/renders")
                .join(format!("{saved}.wav"))
                .exists()
        );
        assert!(
            root.join("audio/previews")
                .join(format!("{playing}.wav"))
                .exists()
        );
        assert_eq!(std::fs::read(user_file).unwrap(), b"user export");
    }

    #[test]
    fn abandoned_downloads_use_original_ownership_and_never_delete_linked_files() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::open(directory.path().join("app")).unwrap();
        let root = directory.path().join("old-model-root");
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let path = root.join(".staging").join(&id);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join(".hypnofy-model.json"),
            serde_json::json!({"id":id,"root":root}).to_string(),
        )
        .unwrap();
        std::fs::write(path.join("partial.gguf"), b"partial").unwrap();
        let linked = root.join("linked.gguf");
        std::fs::write(&linked, b"GGUF linked").unwrap();
        store
            .staging_register(&id, &root.to_string_lossy(), &path.to_string_lossy())
            .unwrap();
        recover_installations(&store, true).unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read(linked).unwrap(), b"GGUF linked");
        assert!(store.staging_records().unwrap().is_empty());
        let id = uuid::Uuid::new_v4().to_string();
        let outside = root.join(&id);
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), b"keep").unwrap();
        store
            .staging_register(&id, &root.to_string_lossy(), &outside.to_string_lossy())
            .unwrap();
        assert!(recover_installations(&store, true).is_err());
        assert!(outside.join("keep.txt").exists());
    }
}
