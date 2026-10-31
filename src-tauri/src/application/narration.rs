use crate::{
    document,
    domain::*,
    infrastructure::{
        database::Database,
        downloads,
        runtime::{Inference, Job},
    },
    storage::Script,
};
use chrono::Utc;
use hypnofy_inference_protocol::{Architecture, Device, Operation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Unit {
    Speech {
        text: String,
        voice_id: String,
        speed: f32,
        block_ids: Vec<String>,
    },
    Silence {
        duration_ms: u32,
        block_ids: Vec<String>,
    },
}

fn apply(value: &Value, voice: &mut String, speed: &mut f32) {
    if let Some(id) = value["voiceId"].as_str() {
        *voice = id.into();
    }
    if let Some(s) = value["speed"].as_f64() {
        *speed = s as f32;
    }
}

pub fn passage_document(document: &Value, ids: &[String]) -> Result<Value, String> {
    fn select(node: &Value, ids: &[String]) -> Option<Value> {
        if node["attrs"]["id"]
            .as_str()
            .is_some_and(|id| ids.iter().any(|selected| selected == id))
        {
            return Some(node.clone());
        }
        let content: Vec<Value> = node["content"]
            .as_array()?
            .iter()
            .filter_map(|node| select(node, ids))
            .collect();
        if content.is_empty() {
            return None;
        }
        let mut node = node.clone();
        node["content"] = serde_json::json!(content);
        Some(node)
    }
    let document = select(document, ids).ok_or("The selected passage no longer exists")?;
    document::plain_text(&document)?;
    Ok(document)
}

pub fn compile(script: &Script) -> Result<Vec<Unit>, String> {
    document::plain_text(&script.document)?;
    let mut units = Vec::new();
    let mut previous_group: Option<String> = None;
    let mut pending: Option<(u32, bool)> = None;
    fn blocks<'a>(node: &'a Value, result: &mut Vec<&'a Value>) {
        if ["paragraph", "heading", "pause"].contains(&node["type"].as_str().unwrap_or("")) {
            result.push(node);
        } else if let Some(children) = node["content"].as_array() {
            for child in children {
                blocks(child, result);
            }
        }
    }
    let mut flat = Vec::new();
    blocks(&script.document, &mut flat);
    for block in flat {
        let block_id = block["attrs"]["id"]
            .as_str()
            .ok_or("Passage ID missing")?
            .to_owned();
        if block["type"] == "pause" {
            pending = None;
            units.push(Unit::Silence {
                block_ids: vec![block_id],
                duration_ms: block["attrs"]["durationMs"]
                    .as_u64()
                    .ok_or("Pause missing")? as u32,
            });
            previous_group = None;
            continue;
        }
        let group = block["attrs"]["flowGroupId"].as_str().map(str::to_owned);
        let continuous = group.is_some() && group == previous_group;
        let starts_with_pause = block["content"]
            .as_array()
            .and_then(|a| a.first())
            .is_some_and(|n| n["type"] == "inlinePause");
        if let Some((ms, explicit)) = pending.take()
            && ms > 0
            && (explicit || (!continuous && !starts_with_pause))
        {
            units.push(Unit::Silence {
                duration_ms: ms,
                block_ids: vec![block_id.clone()],
            });
        }
        let mut voice = script.session.voice_id.clone().unwrap_or_default();
        let mut speed = script.session.speed;
        apply(&block["attrs"]["delivery"], &mut voice, &mut speed);
        let mut first_child = true;
        if let Some(children) = block["content"].as_array() {
            for child in children {
                if child["type"] == "inlinePause" {
                    units.push(Unit::Silence {
                        block_ids: vec![block_id.clone()],
                        duration_ms: child["attrs"]["durationMs"]
                            .as_u64()
                            .ok_or("Pause missing")? as u32,
                    });
                    continue;
                }
                let text = if child["type"] == "hardBreak" {
                    " ".into()
                } else {
                    child["text"].as_str().unwrap_or("").to_owned()
                };
                if text.is_empty() {
                    continue;
                }
                let mut v = voice.clone();
                let mut s = speed;
                for mark in child["marks"].as_array().into_iter().flatten() {
                    if mark["type"] == "delivery" {
                        apply(&mark["attrs"], &mut v, &mut s);
                    }
                }
                if v.is_empty() {
                    return Err("Choose a voice before rendering".into());
                }
                if let Some(Unit::Speech {
                    text: previous,
                    voice_id,
                    speed,
                    block_ids,
                }) = units.last_mut()
                    && voice_id == &v
                    && *speed == s
                {
                    if first_child
                        && !previous.ends_with(char::is_whitespace)
                        && !text.starts_with(char::is_whitespace)
                    {
                        previous.push(' ');
                    }
                    previous.push_str(&text);
                    if !block_ids.contains(&block_id) {
                        block_ids.push(block_id.clone());
                    }
                    first_child = false;
                    continue;
                }
                units.push(Unit::Speech {
                    block_ids: vec![block_id.clone()],
                    text,
                    voice_id: v,
                    speed: s,
                });
                first_child = false;
            }
        }
        let explicit = block["attrs"]["pauseAfterMs"].as_u64();
        pending = if block["content"]
            .as_array()
            .and_then(|a| a.last())
            .is_some_and(|n| n["type"] == "inlinePause")
            && explicit.is_none()
        {
            None
        } else {
            Some((
                explicit
                    .map(|v| v as u32)
                    .unwrap_or(script.session.paragraph_pause_ms),
                explicit.is_some(),
            ))
        };
        previous_group = group;
    }
    if let Some((ms, true)) = pending
        && ms > 0
    {
        units.push(Unit::Silence {
            duration_ms: ms,
            block_ids: Vec::new(),
        });
    }
    units.retain(|u| !matches!(u,Unit::Speech{text,..} if text.trim().is_empty()));
    if units.is_empty() {
        return Err("Write some script text before rendering".into());
    }
    Ok(units)
}

pub fn chunks(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut rest = text.trim();
    while rest.chars().count() > 700 {
        let end = rest
            .char_indices()
            .nth(700)
            .map(|(i, _)| i)
            .unwrap_or(rest.len());
        let boundary = rest[..end]
            .rfind(['.', '!', '?', ';', '\n'])
            .filter(|i| *i > end / 3)
            .map(|i| i + 1)
            .or_else(|| rest[..end].rfind(char::is_whitespace))
            .unwrap_or(end);
        chunks.push(rest[..boundary].to_owned());
        rest = rest[boundary..].trim_start();
    }
    if !rest.is_empty() {
        chunks.push(rest.into());
    }
    chunks
}

pub async fn render(
    db: Database,
    inference: Inference,
    job: Job,
    script: Script,
    root: PathBuf,
    preview: bool,
) -> Result<String, String> {
    let units = compile(&script)?;
    let models = db
        .call(|s| s.model_records())
        .await
        .map_err(|e| format!("{e:?}"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let staging = root.join("audio/.staging");
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let workspace = tempfile::Builder::new()
        .prefix("render-")
        .tempdir_in(&staging)
        .map_err(|e| e.to_string())?;
    let cache = root.join("audio/chunks");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let output = workspace.path().join("render.wav");
    let mut writer = hound::WavWriter::create(
        &output,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|e| e.to_string())?;
    let mut frames = 0_u64;
    let mut used = Vec::new();
    let mut passages = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        let start_ms = (frames / 48).min(u64::from(u32::MAX)) as u32;
        if job.cancel.load(Ordering::SeqCst) {
            return Err("Cancelled".into());
        }
        job.update(|s| {
            s.state = crate::domain::JobStatus::Running;
            s.phase = format!("Rendering passage {} of {}", index + 1, units.len());
            s.progress = Some(index as f64 / units.len() as f64);
        });
        match unit {
            Unit::Silence { duration_ms, .. } => {
                let count = u64::from(*duration_ms) * 48;
                for _ in 0..count {
                    writer.write_sample(0_i16).map_err(|e| e.to_string())?;
                }
                frames += count;
            }
            Unit::Speech {
                text,
                voice_id,
                speed,
                ..
            } => {
                let (model_id, voice) = voice_id
                    .rsplit_once('/')
                    .ok_or("Voice selection is invalid")?;
                let model = models
                    .iter()
                    .find(|m| m.id == model_id && m.capability == "speech" && m.status == "ready")
                    .ok_or("Voice model is unavailable. Verify or relink it in Models & voices.")?;
                if !model.voices.iter().any(|v| v.id == *voice_id) {
                    return Err("Voice is not available in this model".into());
                }
                let path = downloads::model_path(model)?;
                let fingerprint = downloads::fingerprint(&path)?;
                if model.fingerprint.as_ref() != Some(&fingerprint) {
                    return Err("Voice model changed. Verify it again.".into());
                }
                let language = model
                    .voices
                    .iter()
                    .find(|v| v.id == *voice_id)
                    .ok_or("Voice missing")?
                    .language
                    .clone();
                for text in chunks(text) {
                    let hash = crate::document::hex(&Sha256::digest(format!(
                        "d045f340:{fingerprint}:{voice_id}:{speed}:{language}:{text}"
                    )));
                    let cached = cache.join(format!("{hash}.wav"));
                    if !cached.exists() {
                        let work = workspace.path().join(&hash);
                        let result = inference
                            .run(
                                job.clone(),
                                Operation::Synthesize {
                                    model_path: path.to_string_lossy().into_owned(),
                                    architecture: if model.architecture.as_deref()
                                        == Some("supertonic")
                                    {
                                        Architecture::Supertonic
                                    } else {
                                        Architecture::Kokoro
                                    },
                                    voice: voice.into(),
                                    language: language.clone(),
                                    speed: *speed,
                                    text,
                                    device: Device::Auto,
                                },
                                work,
                            )
                            .await?
                            .audio_path
                            .ok_or("Worker did not produce audio")?;
                        if job.cancel.load(Ordering::SeqCst) {
                            return Err("Cancelled".into());
                        }
                        if std::fs::rename(&result, &cached).is_err() && !cached.exists() {
                            return Err("Could not publish the speech unit".into());
                        }
                    }
                    frames += append_wav(&cached, &mut writer)?;
                    used.push(hash);
                }
            }
        }
        let block_ids = match unit {
            Unit::Speech { block_ids, .. } | Unit::Silence { block_ids, .. } => block_ids.clone(),
        };
        passages.push(RenderPassage {
            block_ids,
            start_ms,
            end_ms: (frames / 48).min(u64::from(u32::MAX)) as u32,
        });
    }
    writer.finalize().map_err(|e| e.to_string())?;
    if job.cancel.load(Ordering::SeqCst) {
        return Err("Cancelled".into());
    }
    job.begin_commit()?;
    let directory = root.join(if preview {
        "audio/previews"
    } else {
        "audio/renders"
    });
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let destination = directory.join(format!("{id}.wav"));
    std::fs::rename(output, &destination).map_err(|e| e.to_string())?;
    let record = RenderRecord {
        id: id.clone(),
        script_id: script.id,
        revision: script.revision as i32,
        content_hash: document::content_hash(&script.document),
        duration_ms: (frames / 48).min(u64::from(u32::MAX)) as u32,
        created_at: Utc::now().to_rfc3339(),
        preview,
        passages,
    };
    let manifest = serde_json::json!({"record":record,"units":used});
    std::fs::write(directory.join(format!("{id}.json")), manifest.to_string())
        .map_err(|e| e.to_string())?;
    if let Err(error) = db.call(move |s| s.render_save(&record)).await {
        let _ = std::fs::remove_file(&destination);
        return Err(format!("{error:?}"));
    }
    Ok(id)
}

fn append_wav(
    path: &Path,
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
) -> Result<u64, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    } else {
        let scale = 2_f32.powi(i32::from(spec.bits_per_sample) - 1);
        reader
            .samples::<i32>()
            .map(|v| v.map(|v| v as f32 / scale))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let channels = usize::from(spec.channels);
    if channels == 0 || spec.sample_rate == 0 {
        return Err("Invalid audio format".into());
    }
    let mono: Vec<f32> = samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    let count = mono.len() as u64 * 48000 / u64::from(spec.sample_rate);
    for frame in 0..count {
        let position = frame as f64 * f64::from(spec.sample_rate) / 48000.0;
        let i = position as usize;
        let fraction = (position - i as f64) as f32;
        let a = *mono.get(i).unwrap_or(&0.0);
        let b = *mono.get(i + 1).unwrap_or(&a);
        let fade = (frame.min(count.saturating_sub(frame + 1)) as f32 / 240.0).min(1.0);
        let sample = (a + (b - a) * fraction).clamp(-1.0, 1.0) * fade;
        writer
            .write_sample((sample * i16::MAX as f32) as i16)
            .map_err(|e| e.to_string())?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_do_not_split_unicode() {
        let text = "🌙 ".repeat(1000);
        assert!(chunks(&text).iter().all(|c| c.chars().count() <= 700));
    }

    fn fixture(document: Value) -> Script {
        Script {
            id: "fixture".into(),
            collection_id: "inbox".into(),
            collection_name: "My scripts".into(),
            title: "Test".into(),
            body: String::new(),
            content_hash: document::content_hash(&document),
            document,
            last_model_id: None,
            favorite: false,
            created_at: String::new(),
            updated_at: String::new(),
            deleted_at: None,
            revision: 1,
            session: crate::storage::SessionSettings {
                voice_id: Some("model/emma".into()),
                speed: 1.0,
                ..Default::default()
            },
        }
    }

    #[test]
    fn explicit_boundaries_replace_automatic_silence_and_continuous_text_keeps_spaces() {
        let mut doc = document::from_plain_text("First.\n\n[pause:1.5s]\n\nSecond.");
        let plan = compile(&fixture(doc.clone())).unwrap();
        assert_eq!(
            plan.iter()
                .filter_map(|u| if let Unit::Silence { duration_ms, .. } = u {
                    Some(*duration_ms)
                } else {
                    None
                })
                .collect::<Vec<_>>(),
            vec![1500]
        );
        doc = document::from_plain_text("First.\n\nSecond.");
        for node in doc["content"].as_array_mut().unwrap() {
            node["attrs"]["flowGroupId"] = serde_json::json!("group");
        }
        let plan = compile(&fixture(doc)).unwrap();
        assert_eq!(plan.len(), 1);
        assert!(matches!(&plan[0],Unit::Speech{text,..} if text=="First. Second."));
    }

    #[test]
    fn inline_overrides_and_passage_preview_preserve_the_source() {
        let mut doc = document::from_plain_text("Breathe gently.");
        doc["content"][0]["attrs"]["delivery"] = serde_json::json!({"speed":0.85});
        doc["content"][0]["content"][0]["marks"] =
            serde_json::json!([{"type":"delivery","attrs":{"speed":0.7,"voiceId":"other/voice"}}]);
        let plan = compile(&fixture(doc.clone())).unwrap();
        assert!(
            matches!(&plan[0],Unit::Speech{speed,voice_id,..} if *speed==0.7&&voice_id=="other/voice")
        );
        let id = doc["content"][0]["attrs"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(passage_document(&doc, &[id]).unwrap(), doc);
        assert!(passage_document(&doc, &["missing".into()]).is_err());
    }
}
