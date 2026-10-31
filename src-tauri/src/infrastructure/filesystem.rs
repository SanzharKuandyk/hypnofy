use super::runtime::Job;
use std::path::{Path, PathBuf};

/// Enumerate only the selected model tree. Never follow junctions or symlinks
/// outside that tree, and never inspect staging downloads as installed models.
pub fn model_locations(root: &Path, job: &Job) -> Result<Vec<PathBuf>, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "The selected model folder is missing or unreadable")?;
    let mut pending = vec![root.clone()];
    let mut locations = Vec::new();
    let mut visited = 0_u32;
    while let Some(directory) = pending.pop() {
        if job.cancelled() {
            return Err("Cancelled".into());
        }
        if directory.join("model.onnx").is_file() && directory.join("voices").is_dir()
            || directory.join("onnx/tts.json").is_file() && directory.join("voice_styles").is_dir()
        {
            locations.push(directory);
            continue;
        }
        for entry in std::fs::read_dir(directory)
            .map_err(|_| "Part of the chosen model folder cannot be read")?
        {
            if job.cancelled() {
                return Err("Cancelled".into());
            }
            visited += 1;
            if visited > 100_000 {
                return Err("This folder contains over 100,000 entries. Choose a more specific model folder.".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() == ".staging" {
                continue;
            }
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path().canonicalize().map_err(|e| e.to_string())?;
            if !path.starts_with(&root) {
                continue;
            }
            if kind.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
            {
                locations.push(path);
            }
            if visited.is_multiple_of(100) {
                job.update(|s| {
                    s.state = crate::domain::JobStatus::Running;
                    s.phase = format!("Scanning model folder · {visited} entries");
                    s.progress = None;
                });
            }
        }
    }
    locations.sort();
    Ok(locations)
}

pub fn display_path(value: &str) -> String {
    if let Some(unc) = value.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        value.strip_prefix("\\\\?\\").unwrap_or(value).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::runtime::Jobs;

    #[test]
    fn scans_only_models_and_excludes_staging_and_cancelled_work() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".staging/download")).unwrap();
        std::fs::create_dir_all(root.path().join("nested/speech/voices")).unwrap();
        std::fs::write(root.path().join("nested/text.GGUF"), b"GGUF").unwrap();
        std::fs::write(root.path().join("nested/speech/model.onnx"), b"synthetic").unwrap();
        std::fs::write(
            root.path().join(".staging/download/incomplete.gguf"),
            b"GGUF",
        )
        .unwrap();
        let jobs = Jobs::default();
        let id = uuid::Uuid::new_v4().to_string();
        let (job, _) = jobs.start(&id, "folder-scan").unwrap();
        assert_eq!(model_locations(root.path(), &job).unwrap().len(), 2);
        jobs.cancel(&id).unwrap();
        assert!(model_locations(root.path(), &job).is_err());
    }
}
