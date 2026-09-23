use super::{ModelStatus, RecoveryAction, TextModel};

/// Keep execution disabled until a version-specific profile and permission probe passes.
pub fn model_status() -> TextModel {
    TextModel {
        id: "opencode:isolated".into(), name: "OpenCode".into(), provider: "opencode".into(),
        capability: "text".into(), source: "remote".into(), status: ModelStatus::Invalid,
        reason: Some("Hypnofy has not verified isolated sessions and disabled tools for this OpenCode installation".into()),
        recovery: Some(RecoveryAction::Connect), cost: None,
    }
}
