//! Coordinates recording ownership, deletion recovery, and native playback lifetimes.

use crate::{
    bootstrap::App,
    domain::{ApiResult, AppError, BulkRequest, LibraryMutation, RecordingRemoval},
    infrastructure::audio::Command,
    storage::Store,
};
use std::path::Path;

fn remove_files(root: &Path, id: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err("Invalid recording ownership ID".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    for leaf in ["renders", "previews"] {
        let directory = root.join("audio").join(leaf);
        if directory.canonicalize().map_err(|e| e.to_string())? != directory {
            return Err("The recording folder must remain inside application storage".into());
        }
        for extension in ["wav", "json"] {
            let path = directory.join(format!("{id}.{extension}"));
            match std::fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
                Ok(metadata)
                    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() =>
                {
                    return Err("Recording ownership could not be verified".into());
                }
                Ok(_) => {}
            }
            std::fs::remove_file(path).map_err(|_| {
                "The audio file is in use. Close other audio applications and retry.".to_owned()
            })?;
        }
    }
    Ok(())
}

/// Retries only previously queued, owned recording files; locked files retain their records.
pub fn recover_deletions(store: &Store, root: &Path) -> ApiResult<u32> {
    let mut pending = 0;
    for id in store.pending_recording_removals()? {
        if remove_files(root, &id).is_ok() {
            store.finish_recording_removal(&id)?;
        } else {
            pending += 1;
        }
    }
    // Cache failures leave reusable units for Storage cleanup; recording deletion is already committed.
    let _ = super::cleanup::reclaim_speech_cache(store, root);
    Ok(pending)
}

/// Recovers files left by older script deletion, only when their owned manifest matches the ID.
pub fn recover_orphaned_audio(store: &Store, root: &Path) -> ApiResult<u32> {
    let root = root
        .canonicalize()
        .map_err(|e| AppError::from(e.to_string()))?;
    for leaf in ["renders", "previews"] {
        let directory = root.join("audio").join(leaf);
        if directory
            .canonicalize()
            .map_err(|e| AppError::from(e.to_string()))?
            != directory
        {
            return Err("Audio ownership could not be verified".into());
        }
        for entry in std::fs::read_dir(&directory).map_err(|e| AppError::from(e.to_string()))? {
            let entry = entry.map_err(|e| AppError::from(e.to_string()))?;
            let path = entry.path();
            if !entry
                .file_type()
                .map_err(|e| AppError::from(e.to_string()))?
                .is_file()
                || !path.extension().is_some_and(|e| e == "json")
            {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if uuid::Uuid::parse_str(id).is_err() || store.render_script(id)?.is_some() {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            let Ok(record) =
                serde_json::from_value::<crate::domain::RenderRecord>(manifest["record"].clone())
            else {
                continue;
            };
            if record.id == id {
                store.queue_orphaned_recording(&record)?;
            }
        }
    }
    recover_deletions(store, &root)
}

pub async fn remove(app: &App, id: String) -> ApiResult<RecordingRemoval> {
    let _maintenance = app.jobs.maintenance()?;
    app.db
        .call({
            let id = id.clone();
            move |s| s.queue_recording_removal(&id)
        })
        .await?;
    if app
        .audio
        .synchronized_state()
        .await
        .map_err(AppError::from)?
        .render_id
        .as_deref()
        == Some(&id)
    {
        app.audio.command(Command::Unload).map_err(AppError::from)?;
        app.audio
            .synchronized_state()
            .await
            .map_err(AppError::from)?;
    }
    let root = app.root.clone();
    app.db
        .call(move |s| {
            if let Err(message) = remove_files(&root, &id) {
                return Ok(RecordingRemoval::Pending { message });
            }
            s.finish_recording_removal(&id)?;
            let _ = super::cleanup::reclaim_speech_cache(s, &root);
            Ok(RecordingRemoval::Removed)
        })
        .await
}

pub async fn mutate_scripts(app: &App, request: BulkRequest) -> ApiResult<LibraryMutation> {
    let destructive = matches!(request.operation.as_str(), "trash" | "delete");
    let _maintenance = if destructive {
        Some(app.jobs.maintenance()?)
    } else {
        None
    };
    let state = app
        .audio
        .synchronized_state()
        .await
        .map_err(AppError::from)?;
    let unload = if destructive {
        if let Some(id) = state.render_id {
            let owner = app.db.call(move |s| s.render_script(&id)).await?;
            owner.is_some_and(|id| request.ids.contains(&id))
        } else {
            false
        }
    } else {
        false
    };
    let permanent = request.operation == "delete";
    app.db.call(move |s| s.bulk(request)).await?;
    if unload {
        app.audio.command(Command::Unload).map_err(AppError::from)?;
        app.audio
            .synchronized_state()
            .await
            .map_err(AppError::from)?;
    }
    let pending_audio_deletions = if permanent {
        let root = app.root.clone();
        app.db.call(move |s| recover_deletions(s, &root)).await?
    } else {
        0
    };
    Ok(LibraryMutation {
        pending_audio_deletions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RenderRecord;

    fn fixture() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_path_buf()).unwrap();
        for leaf in ["renders", "previews", "chunks"] {
            std::fs::create_dir_all(directory.path().join("audio").join(leaf)).unwrap();
        }
        (directory, store)
    }

    fn recording(store: &Store, units: &[String]) -> RenderRecord {
        let script = store
            .create_script(None, Some("Synthetic recording"), "Breathe.")
            .unwrap();
        let record = RenderRecord {
            id: uuid::Uuid::new_v4().to_string(),
            script_id: script.id,
            revision: 1,
            content_hash: script.content_hash,
            duration_ms: 1000,
            created_at: chrono::Utc::now().to_rfc3339(),
            preview: false,
            pipeline_version: 2,
            passages: Vec::new(),
        };
        store.render_save(&record).unwrap();
        let base = store.data_dir.join("audio").join("renders");
        std::fs::write(base.join(format!("{}.wav", record.id)), b"synthetic PCM").unwrap();
        std::fs::write(
            base.join(format!("{}.json", record.id)),
            serde_json::json!({"units":units}).to_string(),
        )
        .unwrap();
        for unit in units {
            std::fs::write(
                store
                    .data_dir
                    .join("audio")
                    .join("chunks")
                    .join(format!("{unit}.wav")),
                b"cached PCM",
            )
            .unwrap();
        }
        record
    }

    fn bulk(store: &Store, script: &str, operation: &str) {
        store
            .bulk(BulkRequest {
                ids: vec![script.to_owned()],
                operation: operation.into(),
                collection_id: None,
            })
            .unwrap();
    }

    #[test]
    fn trash_preserves_recordings_and_permanent_deletion_reclaims_only_unreferenced_units() {
        let (directory, store) = fixture();
        let shared = "a".repeat(64);
        let unique = "b".repeat(64);
        let first = recording(&store, &[shared.clone(), unique.clone()]);
        let second = recording(&store, std::slice::from_ref(&shared));
        let exported = directory.path().join("user-export.wav");
        std::fs::write(&exported, b"user export").unwrap();
        let audio = directory
            .path()
            .join("audio")
            .join("renders")
            .join(format!("{}.wav", first.id));
        assert!(store.can_play_render(&first.id).unwrap());
        bulk(&store, &first.script_id, "trash");
        assert!(!store.can_play_render(&first.id).unwrap());
        assert!(audio.exists());
        assert_eq!(
            store
                .recording_library()
                .unwrap()
                .iter()
                .find(|r| r.render.id == first.id)
                .unwrap()
                .script_status,
            "trash"
        );
        bulk(&store, &first.script_id, "restore");
        assert!(store.can_play_render(&first.id).unwrap());
        bulk(&store, &first.script_id, "trash");
        bulk(&store, &first.script_id, "delete");
        assert!(
            store
                .recording_library()
                .unwrap()
                .iter()
                .any(|r| r.render.id == first.id && r.pending_deletion)
        );
        assert_eq!(recover_deletions(&store, directory.path()).unwrap(), 0);
        assert!(!audio.exists());
        assert!(store.pending_recording_removals().unwrap().is_empty());
        assert_eq!(store.recording_library().unwrap().len(), 1);
        assert!(store.can_play_render(&second.id).unwrap());
        let cache = directory.path().join("audio").join("chunks");
        assert!(cache.join(format!("{shared}.wav")).exists());
        assert!(!cache.join(format!("{unique}.wav")).exists());
        assert_eq!(std::fs::read(exported).unwrap(), b"user export");
    }

    #[cfg(windows)]
    #[test]
    fn locked_recordings_remain_pending_until_windows_releases_them() {
        use std::os::windows::fs::OpenOptionsExt;
        let (directory, store) = fixture();
        let record = recording(&store, &[]);
        let path = directory
            .path()
            .join("audio")
            .join("renders")
            .join(format!("{}.wav", record.id));
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        store.queue_recording_removal(&record.id).unwrap();
        assert_eq!(recover_deletions(&store, directory.path()).unwrap(), 1);
        assert!(path.exists());
        assert!(store.recording_library().unwrap()[0].pending_deletion);
        assert!(!store.can_play_render(&record.id).unwrap());
        assert!(!store.can_export_render(&record.id).unwrap());
        drop(handle);
        assert_eq!(recover_deletions(&store, directory.path()).unwrap(), 0);
        assert!(!path.exists());
        assert!(store.get_script(&record.script_id).is_ok());
        assert!(store.recording_library().unwrap().is_empty());
        assert!(remove_files(directory.path(), "../outside").is_err());
    }

    #[test]
    fn startup_recovers_older_orphans_only_with_matching_owned_manifests() {
        let (directory, store) = fixture();
        let record = recording(&store, &[]);
        let base = directory.path().join("audio").join("renders");
        store.finish_recording_removal(&record.id).unwrap();
        std::fs::write(
            base.join(format!("{}.json", record.id)),
            serde_json::json!({"record":record,"units":[]}).to_string(),
        )
        .unwrap();
        let unrecognized = uuid::Uuid::new_v4().to_string();
        std::fs::write(base.join(format!("{unrecognized}.wav")), b"unrecognized").unwrap();
        std::fs::write(base.join(format!("{unrecognized}.json")), "{}").unwrap();
        assert_eq!(recover_orphaned_audio(&store, directory.path()).unwrap(), 0);
        assert!(!base.join(format!("{}.wav", record.id)).exists());
        assert!(!base.join(format!("{}.json", record.id)).exists());
        assert!(base.join(format!("{unrecognized}.wav")).exists());
        assert!(store.get_script(&record.script_id).is_ok());
    }
}
