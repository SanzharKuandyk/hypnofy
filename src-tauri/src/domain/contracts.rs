use serde::{Deserialize, Serialize};
use specta::Type;

/// Recursive JSON at the validated document boundary. Unlike the upstream
/// inline Value definition this named type can be represented by TypeScript.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(untagged)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(std::collections::BTreeMap<String, JsonValue>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "code", rename_all = "kebab-case")]
pub enum AppError {
    Validation {
        message: String,
        field: Option<String>,
    },
    RevisionConflict {
        message: String,
        current_revision: i32,
    },
    Failure {
        message: String,
        retryable: bool,
    },
    EntryRequired {
        message: String,
    },
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::Failure {
            message,
            retryable: false,
        }
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

pub type ApiResult<T> = Result<T, AppError>;

#[taurpc::ipc_type]
#[derive(Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub theme: String,
    pub model_root: String,
    pub text_model_id: Option<String>,
    pub voice_id: Option<String>,
    pub speed: f32,
    pub paragraph_pause_ms: u32,
    pub language: String,
    pub narration_volume: f32,
    pub ambience_volume: f32,
    pub idle_model_minutes: u32,
}

impl Preferences {
    pub fn defaults() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let model_root = if cfg!(debug_assertions) {
            std::env::var_os("HYPNOFY_TEST_DATA_DIR")
                .map(std::path::PathBuf::from)
                .map(|p| p.join("Models"))
        } else {
            None
        }
        .unwrap_or_else(|| home.join("Models/Hypnofy"));
        Self {
            theme: "dark".into(),
            model_root: model_root.to_string_lossy().into_owned(),
            text_model_id: None,
            voice_id: None,
            speed: 1.0,
            paragraph_pause_ms: 2000,
            language: "en-gb".into(),
            narration_volume: 1.0,
            ambience_volume: 0.06,
            idle_model_minutes: 5,
        }
    }

    pub fn validate(&self) -> ApiResult<()> {
        if !["dark", "light", "system"].contains(&self.theme.as_str())
            || !self.speed.is_finite()
            || !(0.5..=2.0).contains(&self.speed)
            || self.paragraph_pause_ms > 30_000
            || !self.narration_volume.is_finite()
            || !(0.0..=1.0).contains(&self.narration_volume)
            || !self.ambience_volume.is_finite()
            || !(0.0..=1.0).contains(&self.ambience_volume)
            || self.idle_model_minutes > 120
            || !std::path::Path::new(&self.model_root).is_absolute()
        {
            return Err(AppError::Validation {
                message: "Preferences are outside their supported ranges".into(),
                field: None,
            });
        }
        Ok(())
    }
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModelRecord {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub capability: String,
    pub architecture: Option<String>,
    pub path: Option<String>,
    pub ownership: String,
    pub status: String,
    pub reason: Option<String>,
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub context_tokens: Option<u32>,
    pub voices: Vec<Voice>,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub language: String,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub repository: String,
    pub capability: String,
    pub architecture: Option<String>,
    pub description: String,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallFile {
    pub path: String,
    pub bytes: String,
    pub sha256: Option<String>,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub id: String,
    pub catalog: CatalogEntry,
    pub revision: String,
    pub files: Vec<InstallFile>,
    pub total_bytes: String,
    pub destination: String,
    pub license: String,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub id: String,
    pub kind: JobKind,
    pub state: JobStatus,
    pub phase: String,
    pub progress: Option<f64>,
    pub text: String,
    #[serde(default)]
    pub result: Option<JobResult>,
    pub error: Option<AppError>,
    pub sequence: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum JobKind {
    TextGeneration,
    Narration,
    ModelInstall,
    ModelVerification,
    FolderScan,
    AudioExport,
    ScriptExport,
}

impl JobKind {
    pub fn from_name(value: &str) -> ApiResult<Self> {
        match value {
            "text-generation" => Ok(Self::TextGeneration),
            "narration" => Ok(Self::Narration),
            "model-install" => Ok(Self::ModelInstall),
            "model-verification" => Ok(Self::ModelVerification),
            "folder-scan" => Ok(Self::FolderScan),
            "audio-export" => Ok(Self::AudioExport),
            "script-export" => Ok(Self::ScriptExport),
            _ => Err("Unsupported job kind".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum JobStatus {
    Queued,
    Running,
    Cancelling,
    Cancelled,
    Completed,
    Failed,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl PartialEq<&str> for JobStatus {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum JobResult {
    Suggestion { suggestion_id: String },
    Render { render_id: String },
    Model { model_id: String },
    FolderScan,
    AudioExport,
    ScriptExport { directory: String },
}

impl JobResult {
    pub fn completed(kind: JobKind, id: Option<String>) -> Option<Self> {
        match kind {
            JobKind::TextGeneration => id.map(|suggestion_id| Self::Suggestion { suggestion_id }),
            JobKind::Narration => id.map(|render_id| Self::Render { render_id }),
            JobKind::ModelInstall | JobKind::ModelVerification => {
                id.map(|model_id| Self::Model { model_id })
            }
            JobKind::FolderScan => Some(Self::FolderScan),
            JobKind::AudioExport => Some(Self::AudioExport),
            JobKind::ScriptExport => id.map(|directory| Self::ScriptExport { directory }),
        }
    }
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct DraftRequest {
    pub job_id: String,
    pub script_id: String,
    pub base_revision: i32,
    pub model_id: String,
    pub operation: String,
    pub instruction: String,
    pub selected_text: String,
    pub base_content_hash: String,
    pub anchor: Option<TextAnchor>,
    pub range: Option<TextRange>,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct TextAnchor {
    pub block_id: String,
    pub offset_utf16: u32,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct TextRange {
    pub from: TextAnchor,
    pub to: TextAnchor,
    pub selected_text_hash: String,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub id: String,
    pub script_id: String,
    pub base_revision: i32,
    pub text: String,
    pub model_id: String,
    #[serde(default)]
    pub base_content_hash: String,
    #[serde(default)]
    pub anchor: Option<TextAnchor>,
    #[serde(default)]
    pub range: Option<TextRange>,
    #[serde(default)]
    pub operation: String,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct RenderRecord {
    pub id: String,
    pub script_id: String,
    pub revision: i32,
    pub content_hash: String,
    pub duration_ms: u32,
    pub created_at: String,
    pub preview: bool,
    #[serde(default)]
    pub passages: Vec<RenderPassage>,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct RenderPassage {
    pub block_ids: Vec<String>,
    pub start_ms: u32,
    pub end_ms: u32,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackState {
    pub render_id: Option<String>,
    pub playing: bool,
    pub position_ms: u32,
    pub duration_ms: u32,
    pub volume: f32,
    pub ambience_volume: f32,
    pub sleep_remaining_ms: Option<u32>,
    pub error: Option<String>,
    pub current_block_ids: Vec<String>,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub data_directory: String,
    pub model_root: String,
    pub reclaimable_bytes: String,
    pub available_model_bytes: Option<String>,
    pub backups: Vec<String>,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct EntryStatus {
    pub accepted: bool,
    pub notice_version: String,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct RevisionRecord {
    pub revision: i32,
    pub title: String,
    pub saved_at: String,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct LibraryQuery {
    pub collection_id: Option<String>,
    pub query: String,
    pub favorite: bool,
    pub trash: bool,
}

#[taurpc::ipc_type]
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct BulkRequest {
    pub ids: Vec<String>,
    pub operation: String,
    pub collection_id: Option<String>,
}
