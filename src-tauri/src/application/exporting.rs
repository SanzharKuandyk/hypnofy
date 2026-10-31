use crate::{infrastructure::runtime::Job, storage::Script};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub fn script_content(script: &Script, format: &str) -> Result<String, String> {
    match format {
        "json" => serde_json::to_string_pretty(
            &serde_json::json!({"format":"hypnofy-script","version":1,"script":script}),
        )
        .map_err(|e| e.to_string()),
        "md" => Ok(super::markdown::export(&script.document)),
        "txt" => Ok(script.body.clone()),
        _ => Err("Choose TXT, Markdown or Hypnofy JSON".into()),
    }
}

pub fn batch(
    scripts: Vec<Script>,
    directory: &Path,
    format: &str,
    job: &Job,
) -> Result<PathBuf, String> {
    let directory = directory
        .canonicalize()
        .map_err(|_| "Choose an existing writable export folder")?;
    if !directory.is_dir() {
        return Err("Choose an export folder".into());
    }
    let staging = tempfile::Builder::new()
        .prefix(".hypnofy-export-")
        .tempdir_in(&directory)
        .map_err(|e| e.to_string())?;
    let count = scripts.len();
    for (index, script) in scripts.iter().enumerate() {
        if job.cancelled() {
            return Err("Cancelled".into());
        }
        let title = script
            .title
            .chars()
            .take(64)
            .map(|c| {
                if c.is_control() || "<>:\"/\\|?*".contains(c) {
                    '-'
                } else {
                    c
                }
            })
            .collect::<String>();
        let filename = format!(
            "script-{}-{}.{}",
            index + 1,
            title.trim_end_matches(['.', ' ']),
            format
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staging.path().join(filename))
            .map_err(|e| e.to_string())?;
        file.write_all(script_content(script, format)?.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        job.update(|s| {
            s.state = crate::domain::JobStatus::Running;
            s.phase = format!("Exporting script {} of {count}", index + 1);
            s.progress = Some((index + 1) as f64 / count.max(1) as f64);
        });
    }
    job.begin_commit()?;
    let target = directory.join(format!(
        "Hypnofy export {} {}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"),
        uuid::Uuid::new_v4()
    ));
    std::fs::rename(staging.path(), &target).map_err(|e| e.to_string())?;
    Ok(target)
}
