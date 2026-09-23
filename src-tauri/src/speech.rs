use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineSegment {
    pub kind: SegmentKind,
    pub text: Option<String>,
    pub seconds: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SegmentKind {
    Speech,
    Pause,
}

/// A future engine must finish a render before the session becomes playable.
#[allow(dead_code)]
pub trait SpeechProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn render(
        &self,
        segments: &[TimelineSegment],
        output_dir: &std::path::Path,
    ) -> Result<(), String>;
}

pub fn timing_plan(
    document: &serde_json::Value,
    paragraph_pause: f32,
) -> Result<Vec<TimelineSegment>, String> {
    crate::document::plain_text(document)?;
    let mut timeline = Vec::new();
    if let Some(blocks) = document
        .get("content")
        .and_then(serde_json::Value::as_array)
    {
        for (index, block) in blocks.iter().enumerate() {
            if block.get("type").and_then(serde_json::Value::as_str) == Some("pause") {
                let seconds = block
                    .pointer("/attrs/seconds")
                    .and_then(serde_json::Value::as_f64)
                    .ok_or("Pause duration missing")? as f32;
                timeline.push(TimelineSegment {
                    kind: SegmentKind::Pause,
                    text: None,
                    seconds: Some(seconds),
                });
            } else {
                let text = crate::document::plain_text(
                    &serde_json::json!({"version":1,"type":"doc","content":[block]}),
                )?;
                if !text.trim().is_empty() {
                    timeline.push(TimelineSegment {
                        kind: SegmentKind::Speech,
                        text: Some(text),
                        seconds: None,
                    });
                }
                let next_is_pause = blocks
                    .get(index + 1)
                    .and_then(|next| next.get("type"))
                    .and_then(serde_json::Value::as_str)
                    == Some("pause");
                if index + 1 < blocks.len() && !next_is_pause && paragraph_pause > 0.0 {
                    timeline.push(TimelineSegment {
                        kind: SegmentKind::Pause,
                        text: None,
                        seconds: Some(paragraph_pause.min(30.0)),
                    });
                }
            }
        }
    }
    Ok(timeline)
}
