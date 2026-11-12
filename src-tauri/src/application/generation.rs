use crate::{
    bootstrap::App,
    document,
    domain::*,
    infrastructure::{downloads, runtime::Job},
};
use hypnofy_inference_protocol::{Device, Operation};
use std::sync::atomic::Ordering;

pub async fn run(app: App, request: DraftRequest, job: Job) -> Result<String, String> {
    let script = app
        .db
        .call({
            let id = request.script_id.clone();
            move |s| s.get_script(&id).map_err(AppError::from)
        })
        .await
        .map_err(|e| format!("{e:?}"))?;
    if script.revision != i64::from(request.base_revision) {
        return Err("The script changed. Save it and retry.".into());
    }
    if request.base_content_hash != document::content_hash(&script.document) {
        return Err("The generation snapshot no longer matches the script".into());
    }
    if !["new", "continue", "rewrite-selection"].contains(&request.operation.as_str()) {
        return Err("Unsupported draft operation".into());
    }
    if request.operation == "rewrite-selection" {
        let range = request
            .range
            .as_ref()
            .ok_or("Select a passage to rewrite")?;
        let selected = document::anchored_text(&script.document, range)?;
        if selected != request.selected_text
            || document::text_hash(&selected) != range.selected_text_hash
        {
            return Err("The selected passage changed. Select it again.".into());
        }
    }
    let body = document::plain_text(&script.document)?;
    if request.instruction.trim().is_empty()
        || request.instruction.len() > 16000
        || request.selected_text.len() > 24000
    {
        return Err("Enter an instruction within the supported length".into());
    }
    let prompt = format!(
        "Operation: {}\nInstruction: {}\nCurrent script:\n{}\nSelected text:\n{}",
        request.operation, request.instruction, body, request.selected_text
    );
    let system = "Write a thoughtful self-guided relaxation script. Use gentle invitations, no medical claims, and no tool calls. Return only the requested script text. Write timed silence only as [pause:5s] with the intended numeric seconds (0.5–120). Do not write spoken stage directions such as 'pause 5 seconds'.";
    if prompt.len() > 60000 {
        return Err("This script is too long for the selected generation context. Select a smaller passage.".into());
    }
    let model = app
        .model(&request.model_id)
        .await
        .map_err(|e| format!("{e:?}"))?;
    if model.status != "ready" || model.capability != "text" {
        return Err("Choose an available text model".into());
    }
    if model.provider == "openrouter" {
        if app.test_mode {
            return Err("Cloud credentials are disabled in test workspaces".into());
        }
        let remote = model
            .path
            .as_deref()
            .ok_or("Remote model identity missing")?;
        crate::infrastructure::providers::openrouter::generate(remote, system, &prompt, &job)
            .await?;
    } else {
        let path = downloads::model_path(&model)?;
        if downloads::fingerprint(&path).ok() != model.fingerprint {
            return Err("Model changed; verify it again".into());
        }
        let root = app.root.join("audio/.staging");
        let workspace = tempfile::Builder::new()
            .prefix("draft-")
            .tempdir_in(root)
            .map_err(|e| e.to_string())?;
        app.inference
            .run(
                job.clone(),
                Operation::Generate {
                    model_path: path.to_string_lossy().into_owned(),
                    prompt,
                    system_prompt: system.into(),
                    context_tokens: model.context_tokens.unwrap_or(4096).min(16384),
                    device: Device::Auto,
                },
                workspace.path().to_owned(),
            )
            .await?;
    }
    if job.cancel.load(Ordering::SeqCst) {
        return Err("Cancelled".into());
    }
    let text = job
        .snapshot
        .lock()
        .map_err(|_| "Job unavailable")?
        .text
        .clone();
    if text.trim().is_empty() {
        return Err("The model returned no text".into());
    }
    job.begin_commit()?;
    let suggestion = Suggestion {
        id: request.job_id.clone(),
        script_id: request.script_id,
        base_revision: request.base_revision,
        text,
        model_id: request.model_id,
        base_content_hash: request.base_content_hash,
        anchor: request.anchor,
        range: request.range,
        operation: request.operation,
    };
    app.db
        .call(move |s| s.suggestion_save(&suggestion))
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(request.job_id)
}
