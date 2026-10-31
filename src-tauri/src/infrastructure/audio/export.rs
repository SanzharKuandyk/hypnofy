use crate::infrastructure::runtime::Job;

pub(crate) fn export_audio(
    source: &std::path::Path,
    target: &std::path::Path,
    ambience: Option<&str>,
    narration_volume: f32,
    ambience_volume: f32,
    job: &Job,
) -> Result<(), String> {
    use rodio::Source;
    let mut narration = hound::WavReader::open(source).map_err(|e| e.to_string())?;
    let mut ambient = ambience
        .map(|p| {
            rodio::Decoder::try_from(std::fs::File::open(p).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    let parent = target.parent().ok_or("Invalid export path")?;
    let temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let stereo = ambient.is_some();
    let channels = if stereo { 2 } else { 1 };
    if narration.duration() as u64 * u64::from(channels) * 2 > u64::from(u32::MAX) - 64 {
        return Err("This session exceeds the WAV format size limit".into());
    }
    let mut writer = hound::WavWriter::new(
        temporary.as_file(),
        hound::WavSpec {
            channels,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|e| e.to_string())?;
    let mut ambient_frame = (0.0_f32, 0.0_f32);
    let mut phase = 0.0_f64;
    let duration = narration.duration();
    for (index, sample) in narration.samples::<i16>().enumerate() {
        if index % 4096 == 0 {
            if job.cancelled() {
                return Err("Cancelled".into());
            }
            job.update(|s| {
                s.state = crate::domain::JobStatus::Running;
                s.phase = "Exporting WAV".into();
                s.progress = Some(index as f64 / f64::from(duration.max(1)));
            });
        }
        let voice = sample.map_err(|e| e.to_string())? as f32 / i16::MAX as f32 * narration_volume;
        if let Some(decoder) = &mut ambient {
            phase += decoder.sample_rate().get() as f64 / 48000.0;
            while phase >= 1.0 {
                let left = match decoder.next() {
                    Some(v) => v,
                    None => {
                        decoder
                            .try_seek(std::time::Duration::ZERO)
                            .map_err(|e| e.to_string())?;
                        decoder.next().unwrap_or(0.0)
                    }
                };
                let right = if decoder.channels().get() > 1 {
                    decoder.next().unwrap_or(left)
                } else {
                    left
                };
                for _ in 2..decoder.channels().get() {
                    let _ = decoder.next();
                }
                ambient_frame = (left, right);
                phase -= 1.0;
            }
            for a in [ambient_frame.0, ambient_frame.1] {
                writer
                    .write_sample(
                        ((voice + a * ambience_volume).clamp(-1.0, 1.0) * i16::MAX as f32) as i16,
                    )
                    .map_err(|e| e.to_string())?;
            }
        } else {
            writer
                .write_sample((voice.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                .map_err(|e| e.to_string())?;
        }
    }
    writer.finalize().map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    job.begin_commit()?;
    temporary.persist(target).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::runtime::Jobs;

    fn audio(path: &std::path::Path, rate: u32, channels: u16, frames: u32) {
        let mut writer = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels,
                sample_rate: rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for frame in 0..frames {
            for _ in 0..channels {
                writer
                    .write_sample(if frame % 2 == 0 { 1000_i16 } else { -1000 })
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn exports_deterministic_mono_and_resampled_looped_stereo() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.wav");
        let ambient = directory.path().join("ambient.wav");
        audio(&source, 48000, 1, 4800);
        audio(&ambient, 44100, 2, 441);
        for mixed in [false, true] {
            let target = directory
                .path()
                .join(if mixed { "mix.wav" } else { "voice.wav" });
            let jobs = Jobs::default();
            let (job, _) = jobs
                .start(&uuid::Uuid::new_v4().to_string(), "audio-export")
                .unwrap();
            export_audio(
                &source,
                &target,
                if mixed {
                    Some(ambient.to_str().unwrap())
                } else {
                    None
                },
                1.0,
                0.1,
                &job,
            )
            .unwrap();
            let mut reader = hound::WavReader::open(&target).unwrap();
            assert_eq!(reader.spec().sample_rate, 48000);
            assert_eq!(reader.spec().channels, if mixed { 2 } else { 1 });
            assert_eq!(reader.duration(), 4800);
            assert_eq!(reader.spec().bits_per_sample, 16);
            assert!(reader.samples::<i16>().any(|s| s.unwrap() != 0));
        }
    }

    #[test]
    fn cancelled_export_preserves_destination_and_removes_temporary_output() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.wav");
        audio(&source, 48000, 1, 4800);
        let target = directory.path().join("existing.wav");
        std::fs::write(&target, b"original export").unwrap();
        let jobs = Jobs::default();
        let id = uuid::Uuid::new_v4().to_string();
        let (job, _) = jobs.start(&id, "audio-export").unwrap();
        jobs.cancel(&id).unwrap();
        assert!(export_audio(&source, &target, None, 1.0, 0.0, &job).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original export");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}
