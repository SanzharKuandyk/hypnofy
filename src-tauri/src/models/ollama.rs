use super::*;

impl ProviderManager {
    pub(super) async fn generate_ollama(
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
        let model = r
            .model_id
            .strip_prefix("ollama:")
            .ok_or("Invalid Ollama model")?;
        let response=self.client.post("http://127.0.0.1:11434/api/generate").json(&serde_json::json!({"model":model,"prompt":prompt(r),"stream":true,"keep_alive":keep_alive,"options":{"temperature":0.72}})).send().await.map_err(|e|format!("Could not reach Ollama: {e}"))?;
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
                let value: serde_json::Value = serde_json::from_str(&line)
                    .map_err(|error| format!("Ollama sent an invalid response: {error}"))?;
                if let Some(message) = value["error"].as_str() {
                    return Err(format!("Ollama generation failed: {message}"));
                }
                if let Some(text) = value["response"].as_str()
                    && !text.is_empty()
                {
                    let _ = app.emit(
                        "generation-event",
                        GenerationEvent {
                            job_id: id.into(),
                            script_id: r.script_id.clone(),
                            base_revision: r.base_revision,
                            phase: "streaming".into(),
                            delta: Some(text.into()),
                            error: None,
                        },
                    );
                }
            }
        }
        Ok(())
    }
}
