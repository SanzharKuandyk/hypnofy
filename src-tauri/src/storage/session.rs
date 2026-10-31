use super::SessionSettings;
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SettingsInput {
    #[serde(alias = "voice")]
    voice_id: Option<String>,
    speed: Option<f32>,
    paragraph_pause_ms: Option<u32>,
    paragraph_pause: Option<f32>,
    language: Option<String>,
    ambience_id: Option<String>,
    ambience_volume: Option<f32>,
    narration_volume: Option<f32>,
    ambience_mode: Option<String>,
    sleep_minutes: Option<u32>,
}

impl<'de> Deserialize<'de> for SessionSettings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = SettingsInput::deserialize(deserializer)?;
        let defaults = Self::default();
        let legacy_pause = input
            .paragraph_pause
            .map(|seconds| {
                if !seconds.is_finite() || seconds < 0.0 || seconds > 4_000_000.0 {
                    return Err(serde::de::Error::custom("Invalid legacy paragraph pause"));
                }
                Ok((f64::from(seconds) * 1000.0).round() as u32)
            })
            .transpose()?;
        Ok(Self {
            voice_id: input.voice_id.filter(|v| !v.is_empty()),
            speed: input.speed.unwrap_or(defaults.speed),
            paragraph_pause_ms: input
                .paragraph_pause_ms
                .or(legacy_pause)
                .unwrap_or(defaults.paragraph_pause_ms),
            language: input.language.unwrap_or(defaults.language),
            ambience_id: input.ambience_id,
            ambience_volume: input.ambience_volume.unwrap_or(defaults.ambience_volume),
            narration_volume: input.narration_volume.unwrap_or(defaults.narration_volume),
            ambience_mode: input.ambience_mode.unwrap_or(defaults.ambience_mode),
            sleep_minutes: input.sleep_minutes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_seconds_and_serializes_only_canonical_units() {
        let legacy = serde_json::json!({"voice":"linked/voice", "paragraphPause":1.25,"speed":0.85,"narrationVolume":0.6});
        let session: SessionSettings = serde_json::from_value(legacy).unwrap();
        assert_eq!(session.paragraph_pause_ms, 1250);
        assert_eq!(session.voice_id.as_deref(), Some("linked/voice"));
        assert_eq!(session.narration_volume, 0.6);
        let value = serde_json::to_value(&session).unwrap();
        assert_eq!(value["paragraphPauseMs"], 1250);
        assert!(value.get("paragraphPause").is_none());
        assert!(value.get("voice").is_none());
        let restored: SessionSettings = serde_json::from_value(value).unwrap();
        assert_eq!(restored.paragraph_pause_ms, session.paragraph_pause_ms);
        assert_eq!(restored.language, "en-gb");
    }
}
