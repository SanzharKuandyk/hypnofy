//! Public Hugging Face discovery, independent from verified local inference support.

use crate::domain::*;
use serde_json::Value;

#[derive(Clone)]
pub struct ModelHub {
    client: reqwest::Client,
    base: String,
}

fn repository_valid(repository: &str) -> bool {
    let parts = repository.split('/').collect::<Vec<_>>();
    (1..=2).contains(&parts.len())
        && repository.len() <= 200
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
}

fn optional(value: &Value) -> Option<String> {
    value.as_str().filter(|v| !v.is_empty()).map(str::to_owned)
}

fn string_list(value: &Value) -> Vec<String> {
    if let Some(text) = value.as_str() {
        vec![text.into()]
    } else {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .take(100)
            .collect()
    }
}

fn decode_model(value: &Value) -> ApiResult<HubModel> {
    let repository = optional(&value["id"])
        .or_else(|| optional(&value["modelId"]))
        .filter(|id| repository_valid(id))
        .ok_or_else(|| AppError::from("Invalid model metadata"))?;
    let tags = string_list(&value["tags"]);
    let task = optional(&value["pipeline_tag"]);
    let library = optional(&value["library_name"]);
    let catalog = super::downloads::catalog()
        .into_iter()
        .find(|c| c.repository == repository);
    let compatibility = if let Some(catalog) = catalog {
        HubCompatibility::Catalog {
            catalog_id: catalog.id,
        }
    } else if library.as_deref() == Some("gguf") || tags.iter().any(|t| t == "gguf") {
        HubCompatibility::Candidate{reason:"GGUF file candidate. Download from the source, link it in Hypnofy, then verify architecture compatibility before use.".into()}
    } else {
        HubCompatibility::BrowseOnly{reason:if task.as_deref()==Some("text-to-speech"){"This speech architecture has no Hypnofy adapter. You can browse its model card and files; it cannot be installed for narration here yet."}else{"This repository is not in the verified Hypnofy install catalog. Browsing does not imply runtime support."}.into()}
    };
    let license = optional(&value["cardData"]["license"]).or_else(|| {
        tags.iter()
            .find_map(|t| t.strip_prefix("license:").map(str::to_owned))
    });
    Ok(HubModel {
        repository,
        task,
        library,
        downloads: value["downloads"].as_u64().unwrap_or(0).to_string(),
        likes: value["likes"].as_u64().unwrap_or(0).to_string(),
        license,
        updated_at: optional(&value["lastModified"]).or_else(|| optional(&value["last_modified"])),
        gated: value["gated"]
            .as_bool()
            .unwrap_or_else(|| value["gated"].is_string()),
        compatibility,
        tags,
    })
}

impl ModelHub {
    pub fn new() -> anyhow::Result<Self> {
        let base = if cfg!(debug_assertions) && std::env::var_os("HYPNOFY_TEST_DATA_DIR").is_some()
        {
            std::env::var("HYPNOFY_TEST_HUB_URL")
                .ok()
                .filter(|base| {
                    reqwest::Url::parse(base).is_ok_and(|u| {
                        u.scheme() == "http" && u.host_str() == Some("127.0.0.1") && u.path() == "/"
                    })
                })
                .unwrap_or_else(|| "https://huggingface.co/".into())
        } else {
            "https://huggingface.co/".into()
        };
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .user_agent("Hypnofy model browser")
                .build()?,
            base,
        })
    }

    async fn get(&self, url: reqwest::Url) -> ApiResult<reqwest::Response> {
        self.client.get(url).send().await.map_err(|_|AppError::from("Could not reach the model hub. Check your connection and retry."))?.error_for_status().map_err(|_|AppError::from("The model hub could not return this page. It may be gated, unavailable, or rate limited."))
    }

    pub async fn search(&self, request: HubSearch) -> ApiResult<HubPage> {
        if request.query.len() > 200
            || !["all", "text-generation", "text-to-speech"].contains(&request.task.as_str())
            || !["downloads", "likes", "lastModified"].contains(&request.sort.as_str())
        {
            return Err("Invalid model search".into());
        }
        let mut url = reqwest::Url::parse(&format!("{}api/models", self.base))
            .map_err(|e| AppError::from(e.to_string()))?;
        if let Some(cursor) = request.cursor {
            let next = reqwest::Url::parse(&cursor)
                .map_err(|_| AppError::from("Invalid result cursor"))?;
            if next.origin() != url.origin() || next.path() != url.path() {
                return Err("Invalid result cursor".into());
            }
            url = next;
        } else {
            url.query_pairs_mut()
                .append_pair("search", request.query.trim())
                .append_pair("sort", &request.sort)
                .append_pair("direction", "-1")
                .append_pair("limit", "24")
                .append_pair("full", "true")
                .append_pair("cardData", "true");
            if request.task != "all" {
                url.query_pairs_mut()
                    .append_pair("pipeline_tag", &request.task);
            }
        }
        let response = self.get(url).await?;
        let next_cursor = response
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .and_then(|links| {
                links
                    .split(',')
                    .find(|link| link.contains("rel=\"next\""))
                    .and_then(|link| link.trim().strip_prefix('<'))
                    .and_then(|link| link.split_once('>'))
                    .map(|(url, _)| url.to_owned())
            });
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|_| AppError::from("Invalid model search response"))?;
        let models = values
            .iter()
            .take(24)
            .map(decode_model)
            .collect::<ApiResult<Vec<_>>>()?;
        Ok(HubPage {
            models,
            next_cursor,
        })
    }

    pub async fn detail(&self, repository: String) -> ApiResult<HubDetail> {
        if !repository_valid(&repository) {
            return Err("Invalid model repository".into());
        }
        let mut url = reqwest::Url::parse(&format!("{}api/models/{repository}", self.base))
            .map_err(|e| AppError::from(e.to_string()))?;
        url.query_pairs_mut().append_pair("blobs", "true");
        let value: Value = self
            .get(url)
            .await?
            .json()
            .await
            .map_err(|_| AppError::from("Invalid model details"))?;
        let siblings = value["siblings"].as_array();
        let revision = optional(&value["sha"]).unwrap_or_default();
        let readme = if revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit()) {
            match self
                .client
                .get(format!(
                    "{}{repository}/raw/{revision}/README.md",
                    self.base
                ))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => response
                    .text()
                    .await
                    .ok()
                    .map(|s| s.chars().take(64000).collect()),
                _ => None,
            }
        } else {
            None
        };
        let files = siblings
            .into_iter()
            .flatten()
            .take(1000)
            .filter_map(|v| {
                optional(&v["rfilename"]).map(|path| HubFile {
                    path,
                    bytes: v["size"]
                        .as_u64()
                        .or_else(|| v["lfs"]["size"].as_u64())
                        .map(|size| size.to_string()),
                })
            })
            .collect();
        Ok(HubDetail {
            model: decode_model(&value)?,
            revision,
            languages: string_list(&value["cardData"]["language"]),
            files,
            files_truncated: siblings.is_some_and(|s| s.len() > 1000),
            description: optional(&value["cardData"]["description"]),
            readme,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browsing_never_claims_unverified_speech_is_installable() {
        let qwen=decode_model(&serde_json::json!({"id":"Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice","pipeline_tag":"text-to-speech","tags":["license:apache-2.0"],"downloads":9007199254740993_u64})).unwrap();
        assert!(matches!(
            qwen.compatibility,
            HubCompatibility::BrowseOnly { .. }
        ));
        assert_eq!(qwen.downloads, "9007199254740993");
        assert_eq!(qwen.license.as_deref(), Some("apache-2.0"));
        let supported = decode_model(
            &serde_json::json!({"id":"Supertone/supertonic-3","pipeline_tag":"text-to-speech"}),
        )
        .unwrap();
        assert!(matches!(
            supported.compatibility,
            HubCompatibility::Catalog { .. }
        ));
        assert!(!repository_valid("../secrets"));
        assert!(!repository_valid("Qwen/a?token=x"));
    }
}
