use crate::{
    domain::{ApiResult, AppError},
    infrastructure::runtime::Job,
};
use futures_util::StreamExt;

fn error(value: impl std::fmt::Display) -> AppError {
    value.to_string().into()
}

fn credential() -> Result<keyring_core::Entry, keyring_core::Error> {
    keyring_core::Entry::new("Hypnofy.OpenRouter", "api-key")
}

pub fn connected() -> bool {
    credential().and_then(|entry| entry.get_password()).is_ok()
}

pub async fn connect(key: &str) -> ApiResult<()> {
    if key.trim().len() < 12 || key.len() > 500 {
        return Err("Enter a valid API key".into());
    }
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(error)?
        .get("https://openrouter.ai/api/v1/key")
        .bearer_auth(key.trim())
        .send()
        .await
        .map_err(|_| AppError::from("Could not reach OpenRouter"))?
        .error_for_status()
        .map_err(|_| AppError::from("OpenRouter rejected this API key"))?;
    credential()
        .map_err(error)?
        .set_password(key.trim())
        .map_err(error)
}

pub fn disconnect() -> ApiResult<()> {
    credential()
        .map_err(error)?
        .delete_credential()
        .map_err(error)
}

pub async fn models() -> ApiResult<serde_json::Value> {
    let key = credential()
        .map_err(error)?
        .get_password()
        .map_err(|_| AppError::from("Connect OpenRouter first"))?;
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(error)?
        .get("https://openrouter.ai/api/v1/models")
        .bearer_auth(key)
        .send()
        .await
        .map_err(error)?
        .error_for_status()
        .map_err(error)?
        .json()
        .await
        .map_err(error)
}

#[derive(Default)]
struct Events {
    pending: Vec<u8>,
    done: bool,
}

impl Events {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > 4_000_000 {
            return Err("Provider event exceeded the supported size".into());
        }
        let mut output = Vec::new();
        while let Some(end) = self.pending.iter().position(|c| *c == b'\n') {
            let line = String::from_utf8(self.pending.drain(..=end).collect())
                .map_err(|_| "Invalid provider text encoding")?;
            let Some(data) = line.trim().strip_prefix("data:").map(str::trim_start) else {
                continue;
            };
            if data == "[DONE]" {
                self.done = true;
                break;
            }
            let value: serde_json::Value =
                serde_json::from_str(data).map_err(|_| "Invalid provider event")?;
            if !value["error"].is_null() {
                return Err("OpenRouter stopped the request. Review its status before retrying; the app did not repeat the paid request.".into());
            }
            if let Some(text) = value
                .pointer("/choices/0/delta/content")
                .and_then(|v| v.as_str())
            {
                output.push(text.to_owned());
            }
        }
        Ok(output)
    }
}

pub async fn generate(model: &str, system: &str, prompt: &str, job: &Job) -> Result<(), String> {
    let key = credential()
        .map_err(|_| "Credential storage unavailable")?
        .get_password()
        .map_err(|_| "Connect OpenRouter first")?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let request=client.post("https://openrouter.ai/api/v1/chat/completions").bearer_auth(key)
        .json(&serde_json::json!({"model":model,"messages":[{"role":"system","content":system},{"role":"user","content":prompt}],"stream":true,"max_tokens":4000,"provider":{"data_collection":"deny"}}));
    let response=tokio::select!{response=request.send()=>response.map_err(|_|"Could not connect to OpenRouter; no request was automatically repeated")?,_=job.wait_cancelled()=>return Err("Cancelled".into())}
        .error_for_status().map_err(|_|"OpenRouter rejected this request")?;
    job.update(|s| {
        s.state = crate::domain::JobStatus::Running;
        s.phase = "Generating with OpenRouter".into();
    });
    let mut stream = response.bytes_stream();
    let mut events = Events::default();
    let mut received = 0;
    loop {
        let chunk = tokio::select! {chunk=stream.next()=>chunk,_=job.wait_cancelled()=>return Err("Cancelled".into())};
        let Some(chunk) = chunk else { break };
        for text in events.push(&chunk.map_err(
            |_| "The provider connection was interrupted. Review the request before retrying.",
        )?)? {
            received += text.len();
            if received > 2_000_000 {
                return Err("Provider output exceeded the supported script size".into());
            }
            job.update(|s| s.text.push_str(&text));
        }
        if events.done {
            break;
        }
    }
    if !events.done {
        return Err("The provider stream ended before confirming completion. No paid request was automatically repeated.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_split_utf8_in_order_and_stops_at_done() {
        let bytes =
            "data: {\"choices\":[{\"delta\":{\"content\":\"Breathe 🌙\"}}]}\n\ndata:[DONE]\n"
                .as_bytes();
        let mut decoder = Events::default();
        let mut text = String::new();
        for byte in bytes {
            for delta in decoder.push(&[*byte]).unwrap() {
                text.push_str(&delta);
            }
        }
        assert_eq!(text, "Breathe 🌙");
        assert!(decoder.done);
    }

    #[test]
    fn provider_errors_and_malformed_events_are_not_success() {
        assert!(
            Events::default()
                .push(b"data: {\"error\":{\"message\":\"failed\"}}\n")
                .is_err()
        );
        assert!(Events::default().push(b"data: not-json\n").is_err());
    }
}
