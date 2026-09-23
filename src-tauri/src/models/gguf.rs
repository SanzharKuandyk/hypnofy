use super::*;

impl ProviderManager {
    pub async fn verify_gguf(&self, model_id: &str) -> Result<(), String> {
        if !self.jobs.is_idle()? {
            return Err("Finish the active draft before verifying a GGUF model".into());
        }
        let linked = self.store.linked_model(model_id)?;
        // A path may now contain different bytes; require a fresh load before recording its fingerprint.
        if let Some(mut running) = self.llama.lock().await.take() {
            let _ = running.child.kill().await;
        }
        let cancel = AtomicBool::new(false);
        self.ensure_llama(PathBuf::from(&linked.path), &cancel)
            .await?;
        self.store
            .mark_gguf_verified(&linked, &self.runtime_version)
    }

    pub(super) async fn generate_gguf(
        &self,
        app: &AppHandle,
        id: &str,
        r: &GenerationRequest,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let model_id = r
            .model_id
            .strip_prefix("gguf:")
            .ok_or("Invalid GGUF model")?;
        let linked = self.store.linked_model(model_id)?;
        let (endpoint, token) = self
            .ensure_llama(PathBuf::from(linked.path), cancel)
            .await?;
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let max_tokens = 4000;
        let response=self.client.post(format!("{endpoint}/v1/chat/completions")).bearer_auth(token).json(&serde_json::json!({"model":"local","messages":[{"role":"user","content":prompt(r)}],"stream":true,"temperature":0.72,"top_p":0.9,"max_tokens":max_tokens,"chat_template_kwargs":{"enable_thinking":false}})).send().await.map_err(|e|format!("GGUF generation failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("llama.cpp returned {}", response.status()));
        }
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut finished = false;
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            pending.push_str(&String::from_utf8_lossy(&chunk.map_err(|e| e.to_string())?));
            while let Some(index) = pending.find('\n') {
                let line = pending[..index].trim().to_string();
                pending.drain(..=index);
                let Some(json) = line.strip_prefix("data: ") else {
                    continue;
                };
                if json == "[DONE]" {
                    finished = true;
                    continue;
                }
                let value: serde_json::Value = serde_json::from_str(json)
                    .map_err(|error| format!("llama.cpp sent an invalid response: {error}"))?;
                if let Some(message) = value["error"]["message"].as_str() {
                    return Err(format!("llama.cpp generation failed: {message}"));
                }
                if let Some(text) = value["choices"][0]["delta"]["content"].as_str()
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
        if !cancel.load(Ordering::Relaxed) && !finished {
            return Err("llama.cpp stopped before completing the draft".into());
        }
        Ok(())
    }

    async fn ensure_llama(
        &self,
        model: PathBuf,
        cancel: &AtomicBool,
    ) -> Result<(String, String), String> {
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
        let mut child = Command::new(server)
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
            .kill_on_drop(true)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Could not start llama.cpp: {e}"))?;
        for _ in 0..360 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill().await;
                return Err("GGUF load cancelled".into());
            }
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                return Err(format!(
                    "llama.cpp exited while loading the model ({status})"
                ));
            }
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
        let _ = child.kill().await;
        Err("The GGUF model could not be loaded within 90 seconds".into())
    }
}
