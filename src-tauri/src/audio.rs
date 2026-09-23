use crate::speech::{RenderItem, RenderManifest};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source, source::Zero};
use serde::Serialize;
use std::{
    fs::File,
    io::BufReader,
    num::{NonZeroU16, NonZeroU32},
    path::Path,
    sync::Mutex,
    time::Duration,
};

pub struct AudioManager {
    inner: Mutex<AudioEngine>,
}
struct AudioEngine {
    stream: Option<MixerDeviceSink>,
    narration: Option<Player>,
    ambience: Option<Player>,
    duration: Duration,
    render_path: Option<std::path::PathBuf>,
    queued_items: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub playing: bool,
    pub position_seconds: f32,
    pub duration_seconds: f32,
    pub finished: bool,
}

impl AudioManager {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(AudioEngine {
                stream: None,
                narration: None,
                ambience: None,
                duration: Duration::ZERO,
                render_path: None,
                queued_items: 0,
            }),
        }
    }
    pub fn start(
        &self,
        manifest_path: &Path,
        ambience_path: Option<&Path>,
        narration_volume: f32,
        ambience_volume: f32,
    ) -> Result<PlayerState, String> {
        // Build narration and ambience on one native mixer so playback survives WebView throttling.
        let raw = std::fs::read(manifest_path).map_err(err)?;
        let manifest: RenderManifest = serde_json::from_slice(&raw).map_err(err)?;
        let stream = DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("No audio output is available: {e}"))?;
        let narration = Player::connect_new(stream.mixer());
        let mut duration = Duration::ZERO;
        let queued_items = manifest.items.len();
        for item in manifest.items {
            match item {
                RenderItem::Audio { path } => {
                    let decoder = Decoder::try_from(BufReader::new(File::open(path).map_err(err)?))
                        .map_err(err)?;
                    if let Some(length) = decoder.total_duration() {
                        duration += length
                    }
                    narration.append(decoder)
                }
                RenderItem::Pause { seconds } => {
                    let length = Duration::from_secs_f32(seconds);
                    duration += length;
                    let sample_rate = NonZeroU32::new(manifest.sample_rate)
                        .ok_or_else(|| "Render manifest has an invalid sample rate".to_string())?;
                    narration.append(Zero::new(NonZeroU16::MIN, sample_rate).take_duration(length));
                }
            }
        }
        narration.set_volume(narration_volume.clamp(0.0, 1.5));
        let ambience = if let Some(path) = ambience_path {
            let sink = Player::connect_new(stream.mixer());
            let decoder = Decoder::try_from(BufReader::new(File::open(path).map_err(err)?))
                .map_err(|e| format!("Could not decode ambience: {e}"))?;
            sink.set_volume(ambience_volume.clamp(0.0, 0.5));
            sink.append(decoder.repeat_infinite());
            Some(sink)
        } else {
            None
        };
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Audio state is unavailable")?;
        if let Some(old) = inner.narration.take() {
            old.stop()
        }
        if let Some(old) = inner.ambience.take() {
            old.stop()
        }
        inner.stream = Some(stream);
        inner.narration = Some(narration);
        inner.ambience = ambience;
        inner.duration = duration;
        inner.queued_items = queued_items;
        inner.render_path = Some(manifest_path.to_path_buf());
        drop(inner);
        self.state()
    }
    pub fn append(&self, manifest_path: &Path) -> Result<PlayerState, String> {
        let raw = std::fs::read(manifest_path).map_err(err)?;
        let manifest: RenderManifest = serde_json::from_slice(&raw).map_err(err)?;
        let mut inner = self.inner.lock().map_err(|_| "Audio state is unavailable")?;
        if inner.render_path.as_deref() != Some(manifest_path) { return Err("Playback timeline changed".into()); }
        let start = inner.queued_items;
        let Some(narration) = inner.narration.as_ref() else { return Err("Playback has not started".into()); };
        let mut added = Duration::ZERO;
        for item in manifest.items.iter().skip(start) {
            match item {
                RenderItem::Audio { path } => {
                    let decoder = Decoder::try_from(BufReader::new(File::open(path).map_err(err)?)).map_err(err)?;
                    if let Some(length) = decoder.total_duration() { added += length; }
                    narration.append(decoder);
                }
                RenderItem::Pause { seconds } => {
                    let length = Duration::from_secs_f32(*seconds);
                    added += length;
                    narration.append(Zero::new(NonZeroU16::MIN, NonZeroU32::new(manifest.sample_rate).ok_or("Invalid sample rate")?).take_duration(length));
                }
            }
        }
        inner.queued_items = manifest.items.len();
        inner.duration += added;
        drop(inner);
        self.state()
    }
    pub fn toggle(&self) -> Result<PlayerState, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "Audio state is unavailable")?;
        if let Some(sink) = &inner.narration {
            if sink.is_paused() {
                sink.play();
                if let Some(a) = &inner.ambience {
                    a.play()
                }
            } else {
                sink.pause();
                if let Some(a) = &inner.ambience {
                    a.pause()
                }
            }
        }
        drop(inner);
        self.state()
    }
    pub fn stop(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Audio state is unavailable")?;
        if let Some(s) = inner.narration.take() {
            s.stop()
        }
        if let Some(s) = inner.ambience.take() {
            s.stop()
        }
        inner.stream = None;
        inner.duration = Duration::ZERO;
        inner.render_path = None;
        inner.queued_items = 0;
        Ok(())
    }
    pub fn seek(&self, seconds: f32) -> Result<PlayerState, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "Audio state is unavailable")?;
        if let Some(sink) = &inner.narration {
            sink.try_seek(Duration::from_secs_f32(
                seconds.max(0.0).min(inner.duration.as_secs_f32()),
            ))
            .map_err(err)?
        }
        drop(inner);
        self.state()
    }
    pub fn volumes(&self, narration: f32, ambience: f32) -> Result<(), String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "Audio state is unavailable")?;
        if let Some(s) = &inner.narration {
            s.set_volume(narration.clamp(0.0, 1.5))
        }
        if let Some(s) = &inner.ambience {
            s.set_volume(ambience.clamp(0.0, 0.5))
        }
        Ok(())
    }
    pub fn state(&self) -> Result<PlayerState, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "Audio state is unavailable")?;
        Ok(if let Some(sink) = &inner.narration {
            PlayerState {
                playing: !sink.is_paused() && !sink.empty(),
                position_seconds: sink.get_pos().as_secs_f32(),
                duration_seconds: inner.duration.as_secs_f32(),
                finished: sink.empty(),
            }
        } else {
            PlayerState {
                playing: false,
                position_seconds: 0.0,
                duration_seconds: 0.0,
                finished: true,
            }
        })
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
