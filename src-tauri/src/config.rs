use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

const EMBEDDED_ASSET_MANIFEST: &str = include_str!("../assets/asset-manifest.json");

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetManifest {
    pub schema_version: u32,
    pub assets: Vec<AssetConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetConfig {
    pub id: String,
    pub kind: String,
    pub version: String,
    pub source: String,
    pub url: String,
    pub filename: String,
    pub bytes: u64,
    pub sha256: String,
    pub license: String,
    pub install_directory: String,
    pub expected_file: String,
}

impl AssetManifest {
    /// Parse and validate the reviewed, versioned asset manifest bundled with the app.
    pub fn load() -> Result<Self, String> {
        let manifest: Self = serde_json::from_str(EMBEDDED_ASSET_MANIFEST)
            .map_err(|error| format!("Asset manifest is invalid: {error}"))?;
        if manifest.schema_version != 1 {
            return Err("Asset manifest uses an unsupported schema".into());
        }
        for asset in &manifest.assets {
            if !asset.url.starts_with("https://github.com/")
                || asset.sha256.len() != 64
                || asset.bytes == 0
                || !safe_relative_path(&asset.install_directory)
                || !safe_relative_path(&asset.expected_file)
            {
                return Err(format!("Asset '{}' failed manifest validation", asset.id));
            }
        }
        Ok(manifest)
    }

    pub fn by_kind(&self, kind: &str) -> Result<&AssetConfig, String> {
        self.assets
            .iter()
            .find(|asset| asset.kind == kind)
            .ok_or_else(|| format!("The asset manifest has no '{kind}' entry"))
    }
}

fn safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_provider: String,
    pub default_model: String,
    pub gguf_model_path: String,
    pub llama_server_path: String,
    pub keep_model_minutes: u32,
}

impl AppSettings {
    pub fn defaults(data_dir: &Path) -> Self {
        Self {
            default_provider: "ollama".into(),
            default_model: "gemma3:latest".into(),
            gguf_model_path: String::new(),
            llama_server_path: data_dir
                .join("engines/llama/llama-server.exe")
                .to_string_lossy()
                .into_owned(),
            keep_model_minutes: 5,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !["ollama", "gguf", "openrouter", "opencode"].contains(&self.default_provider.as_str()) {
            return Err("Default provider is unsupported".into());
        }
        if self.default_model.chars().count() > 200 || self.keep_model_minutes > 120 {
            return Err("Model settings are outside the supported range".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub preferences: AppSettings,
    pub data_directory: String,
    pub assets: Vec<AssetConfig>,
}
