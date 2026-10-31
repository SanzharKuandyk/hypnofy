//! Isolated NobodyWho host. The parent owns process cancellation and staging cleanup.

use anyhow::{Context, Result, bail, ensure};
use hypnofy_inference_protocol::{
    Architecture, Device, Event, Operation, Payload, Request, VERSION, read_frame, write_frame,
};
use nobodywho::{
    chat::ChatBuilder,
    llm,
    text_to_speech::{
        KokoroConfig, SupertonicConfig, TextToSpeech, TextToSpeechConfig, TextToSpeechDevice,
    },
};
use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
};

fn local_model(path: &str, directory: bool) -> Result<PathBuf> {
    let path = Path::new(path);
    ensure!(
        path.is_absolute(),
        "A resolved absolute local model path is required"
    );
    let path = path.canonicalize().context("Model is unavailable")?;
    ensure!(
        if directory {
            path.is_dir()
        } else {
            path.is_file()
        },
        "Invalid model location"
    );
    Ok(path)
}

fn emit(writer: &mut impl Write, job_id: &str, sequence: &mut u32, payload: Payload) -> Result<()> {
    *sequence = sequence
        .checked_add(1)
        .context("Event sequence exhausted")?;
    write_frame(
        writer,
        &Event {
            version: VERSION,
            job_id: job_id.to_owned(),
            sequence: *sequence,
            payload,
        },
    )?;
    Ok(())
}

fn execute(
    request: &Request,
    root: &Path,
    writer: &mut impl Write,
    sequence: &mut u32,
    cache: &mut Option<Loaded>,
    cancelled: &Mutex<HashSet<String>>,
) -> Result<()> {
    ensure!(request.version == VERSION, "Unsupported protocol version");
    ensure!(
        !request.job_id.is_empty()
            && request.job_id.len() <= 128
            && request
                .job_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
        "Invalid job ID"
    );
    match &request.operation {
        Operation::Ping => emit(writer, &request.job_id, sequence, Payload::Ready)?,
        Operation::Shutdown | Operation::Cancel => {}
        Operation::Generate {
            model_path,
            prompt,
            system_prompt,
            context_tokens,
            device,
        } => {
            ensure!(
                (512..=32768).contains(context_tokens),
                "Unsupported context size"
            );
            ensure!(!prompt.trim().is_empty(), "Prompt is empty");
            let path = local_model(model_path, false)?;
            let path = path.to_str().context("Model path is not valid Unicode")?;
            let key = format!("text:{path}:{device:?}");
            if !cache.as_ref().is_some_and(|entry| entry.key() == key) {
                *cache = None;
                let model = Arc::new(llm::get_model(
                    path,
                    matches!(device, Device::Auto),
                    None,
                    None,
                    None,
                )?);
                *cache = Some(Loaded::Text { key, model });
            }
            let Some(Loaded::Text { model, .. }) = cache else {
                bail!("Wrong loaded model");
            };
            let model = model.clone();
            ensure!(
                model.is_generative_model(),
                "Model does not support generation"
            );
            let capacity = model.max_ctx();
            emit(
                writer,
                &request.job_id,
                sequence,
                Payload::ModelInfo {
                    context_tokens: capacity,
                },
            )?;
            let context_tokens = (*context_tokens).min(capacity);
            let chat = ChatBuilder::new(model)
                .with_context_size(context_tokens)
                .with_template_variable("enable_thinking".to_owned(), false)
                .with_system_prompt(Some(system_prompt.clone()))
                .build()?;
            let prompt_tokens = chat.tokenize(format!("{system_prompt}\n{prompt}"))?.len();
            if prompt_tokens.saturating_add(512) >= context_tokens as usize {
                emit(
                    writer,
                    &request.job_id,
                    sequence,
                    Payload::Failed {
                        code: "context-limit".into(),
                        message: format!(
                            "The script exceeds this model's {context_tokens}-token working context. Shorten the script or select a model with a larger context."
                        ),
                    },
                )?;
                return Ok(());
            }
            let mut stream = chat.ask(prompt.clone())?;
            let mut count = 0_u32;
            while let Some(text) = stream.next_token()? {
                if cancelled
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Cancellation unavailable"))?
                    .contains(&request.job_id)
                {
                    chat.stop_generation();
                    bail!("Cancelled");
                }
                count += 1;
                emit(
                    writer,
                    &request.job_id,
                    sequence,
                    Payload::TextDelta { text },
                )?;
                if count >= 4096 {
                    chat.stop_generation();
                    break;
                }
            }
            emit(
                writer,
                &request.job_id,
                sequence,
                Payload::TextCompleted {
                    text: stream.completed()?,
                },
            )?;
        }
        Operation::Synthesize {
            model_path,
            architecture,
            voice,
            language,
            speed,
            text,
            device,
        } => {
            ensure!(
                speed.is_finite() && (0.5..=2.0).contains(speed),
                "Speed must be between 0.5 and 2.0"
            );
            ensure!(!text.trim().is_empty(), "Speech text is empty");
            let path = local_model(model_path, true)?;
            let path = path.to_str().context("Model path is not valid Unicode")?;
            let config = match architecture {
                Architecture::Kokoro => {
                    let mut config = KokoroConfig::new(path);
                    config.voice.clone_from(voice);
                    config.language.clone_from(language);
                    config.speed = *speed;
                    TextToSpeechConfig::Kokoro(config)
                }
                Architecture::Supertonic => {
                    let mut config = SupertonicConfig::new(path);
                    config.voice.clone_from(voice);
                    config.language.clone_from(language);
                    config.speed = *speed;
                    TextToSpeechConfig::Supertonic(config)
                }
            };
            let device = match device {
                Device::Auto => TextToSpeechDevice::Auto,
                Device::Cpu => TextToSpeechDevice::Cpu,
            };
            let key =
                format!("speech:{path}:{architecture:?}:{voice}:{language}:{speed}:{device:?}");
            if !cache.as_ref().is_some_and(|entry| entry.key() == key) {
                *cache = None;
                *cache = Some(Loaded::Speech {
                    key,
                    tts: TextToSpeech::with_device(config, device)?,
                });
            }
            let Some(Loaded::Speech { tts, .. }) = cache else {
                bail!("Wrong loaded model");
            };
            let audio = tts.synthesize(text.clone())?;
            ensure!(
                audio.starts_with(b"RIFF") && audio.get(8..12) == Some(b"WAVE"),
                "Invalid WAV result"
            );
            let filename = format!("{}.wav", request.job_id);
            let output = root.join(&filename);
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            file.write_all(&audio)?;
            file.sync_all()?;
            drop(file);
            emit(
                writer,
                &request.job_id,
                sequence,
                Payload::AudioCompleted { filename },
            )?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    // Keep native library logs away from the framed stdout channel. No tracing
    // subscriber is installed here, so model paths and prompts are not logged.
    nobodywho::send_llamacpp_logs_to_tracing();
    let root = std::env::args_os()
        .nth(1)
        .context("An owned staging directory is required")?;
    let root = PathBuf::from(root);
    ensure!(
        root.is_absolute() && root.is_dir(),
        "Staging directory must already exist"
    );
    let root = root.canonicalize()?;
    let mut output = io::stdout().lock();
    let mut cache = None;
    let cancelled = Arc::new(Mutex::new(HashSet::<String>::new()));
    let (sender, receiver) = mpsc::channel();
    let cancellations = cancelled.clone();
    std::thread::spawn(move || {
        let mut input = io::stdin().lock();
        while let Ok(Some(request)) = read_frame::<Request>(&mut input) {
            if matches!(request.operation, Operation::Cancel) && request.version == VERSION {
                if let Ok(mut ids) = cancellations.lock() {
                    ids.insert(request.job_id);
                }
            } else if sender.send(request).is_err() {
                break;
            }
        }
    });
    for request in receiver {
        let mut sequence = 0;
        if execute(
            &request,
            &root,
            &mut output,
            &mut sequence,
            &mut cache,
            &cancelled,
        )
        .is_err()
        {
            // No prompts, paths, or native diagnostics are returned across this channel.
            eprintln!("Native inference operation failed");
            emit(
                &mut output,
                &request.job_id,
                &mut sequence,
                Payload::Failed {
                    code: "runtime-failed".into(),
                    message: "The native inference operation failed. Verify the model and runtime."
                        .into(),
                },
            )?;
        }
        if matches!(request.operation, Operation::Shutdown) {
            if request.version != VERSION {
                bail!("Unsupported protocol version");
            }
            break;
        }
    }
    Ok(())
}

enum Loaded {
    Text { key: String, model: Arc<llm::Model> },
    Speech { key: String, tts: TextToSpeech },
}

impl Loaded {
    fn key(&self) -> &str {
        match self {
            Self::Text { key, .. } | Self::Speech { key, .. } => key,
        }
    }
}
