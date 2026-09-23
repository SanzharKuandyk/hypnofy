use super::*;

impl ProviderManager {
    pub(super) async fn openrouter_models(&self) -> Vec<TextModel> {
        if let Ok(cache) = self.openrouter_catalog.lock()
            && let Some((timestamp, models)) = cache.as_ref()
            && timestamp.elapsed() < Duration::from_secs(300)
        {
            return models.clone();
        }
        let key = keyring_core::Entry::new("Hypnofy.OpenRouter", "api-key")
            .ok()
            .and_then(|entry| entry.get_password().ok());
        let Some(key) = key else {
            return vec![TextModel {
                id: "openrouter:connect".into(),
                name: "OpenRouter".into(),
                provider: "openrouter".into(),
                capability: "text".into(),
                source: "remote".into(),
                status: ModelStatus::NeedsAuth,
                reason: Some("Add your API key in Models to use remote generation".into()),
                recovery: Some(RecoveryAction::Authenticate),
                cost: None,
            }];
        };
        let response = self
            .client
            .get("https://openrouter.ai/api/v1/models")
            .bearer_auth(key)
            .timeout(Duration::from_secs(8))
            .send()
            .await;
        let Ok(response) = response else {
            return vec![TextModel {
                id: "openrouter:offline".into(),
                name: "OpenRouter".into(),
                provider: "openrouter".into(),
                capability: "text".into(),
                source: "remote".into(),
                status: ModelStatus::Offline,
                reason: Some("Could not load the OpenRouter model catalog".into()),
                recovery: Some(RecoveryAction::Connect),
                cost: None,
            }];
        };
        if !response.status().is_success() {
            return vec![TextModel {
                id: "openrouter:auth".into(),
                name: "OpenRouter".into(),
                provider: "openrouter".into(),
                capability: "text".into(),
                source: "remote".into(),
                status: ModelStatus::NeedsAuth,
                reason: Some(format!("OpenRouter returned {}", response.status())),
                recovery: Some(RecoveryAction::Authenticate),
                cost: None,
            }];
        }
        let value = match response.json::<serde_json::Value>().await {
            Ok(value) => value,
            Err(_) => return Vec::new(),
        };
        let mut models: Vec<TextModel> = value["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let id = item["id"].as_str()?;
                let output = item
                    .pointer("/architecture/output_modalities")
                    .and_then(|v| v.as_array());
                if !output
                    .is_some_and(|items| items.iter().any(|value| value.as_str() == Some("text")))
                {
                    return None;
                }
                Some(TextModel {
                    id: format!("openrouter:{id}"),
                    name: item["name"].as_str().unwrap_or(id).into(),
                    provider: "openrouter".into(),
                    capability: "text".into(),
                    source: "remote".into(),
                    status: ModelStatus::Ready,
                    reason: None,
                    recovery: None,
                    cost: item
                        .pointer("/pricing/prompt")
                        .and_then(|v| v.as_str())
                        .map(|value| format!("${value}/token input")),
                })
            })
            .collect();
        models.sort_by_key(|model| model.name.to_lowercase());
        if let Ok(mut cache) = self.openrouter_catalog.lock() {
            *cache = Some((Instant::now(), models.clone()));
        }
        models
    }

    pub(super) async fn generate_openrouter(
        &self,
        app: &AppHandle,
        id: &str,
        r: &GenerationRequest,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let model = r
            .model_id
            .strip_prefix("openrouter:")
            .ok_or("Invalid OpenRouter model")?;
        let key = keyring_core::Entry::new("Hypnofy.OpenRouter", "api-key")
            .map_err(|e| e.to_string())?
            .get_password()
            .map_err(|_| "OpenRouter API key is missing")?;
        let response = self.client.post("https://openrouter.ai/api/v1/chat/completions").bearer_auth(key)
            .json(&serde_json::json!({"model":model,"messages":[{"role":"user","content":prompt(r)}],"stream":true,"max_tokens":4000,"provider":{"data_collection":"deny"}}))
            .send().await.map_err(|e| format!("OpenRouter connection failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("OpenRouter returned {}", response.status()));
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
                    .map_err(|error| format!("OpenRouter sent an invalid response: {error}"))?;
                if let Some(message) = value["error"]["message"].as_str() {
                    return Err(format!("OpenRouter generation failed: {message}"));
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
            return Err("OpenRouter stopped before completing the draft".into());
        }
        Ok(())
    }
}
