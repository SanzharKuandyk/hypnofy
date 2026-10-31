use crate::domain::{PlaybackState, RenderPassage};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub enum Command {
    Load {
        id: String,
        path: PathBuf,
        duration_ms: u32,
        passages: Vec<RenderPassage>,
    },
    Play,
    Pause,
    Stop,
    Seek(u32),
    Volume(f32),
    Ambience(Option<PathBuf>),
    AmbienceVolume(f32),
    Sleep(Option<u32>),
    Barrier(tokio::sync::oneshot::Sender<PlaybackState>),
}

#[derive(Clone)]
pub struct Audio {
    sender: mpsc::Sender<Command>,
    state: Arc<Mutex<PlaybackState>>,
}

impl Audio {
    pub fn new() -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let state = Arc::new(Mutex::new(PlaybackState {
            render_id: None,
            playing: false,
            position_ms: 0,
            duration_ms: 0,
            volume: 1.0,
            ambience_volume: 0.06,
            sleep_remaining_ms: None,
            error: None,
            current_block_ids: Vec::new(),
        }));
        let snapshot = state.clone();
        std::thread::Builder::new()
            .name("hypnofy-playback".into())
            .spawn(move || {
                let mut device = None;
                let mut player: Option<Player> = None;
                let mut ambience: Option<Player> = None;
                let mut deadline: Option<Instant> = None;
                let failed=Arc::new(AtomicBool::new(false));
                let mut remembered:Option<PathBuf>=None;
                let mut timeline:Vec<RenderPassage>=Vec::new();
                loop {
                    if failed.swap(false,Ordering::SeqCst){
                        if let Some(p)=player.take(){p.stop();}
                        if let Some(a)=ambience.take(){a.stop();}
                        device=None;
                        if let Ok(mut state)=snapshot.lock(){state.playing=false;state.error=Some("The audio device was disconnected. Choose an output device and press Play to reconnect.".into());}
                    }
                    let command = match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(c) => Some(c),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let result: Result<(), String> = (|| {
                        if let Some(command) = command {
                            match command {
                                Command::Load {
                                    id,
                                    path,
                                    duration_ms,
                                    passages,
                                } => {
                                    if device.is_none() {
                                        device =
                                            Some(open_device(failed.clone())?);
                                    }
                                    if let Some(old) = player.take() {
                                        old.stop();
                                    }
                                    let next = Player::connect_new(
                                        device.as_ref().ok_or("Audio output unavailable")?.mixer(),
                                    );
                                    next.pause();
                                    next.set_volume(
                                        snapshot.lock().map_err(|_| "Playback unavailable")?.volume,
                                    );
                                    next.append(
                                        Decoder::try_from(
                                            std::fs::File::open(&path).map_err(|e| e.to_string())?,
                                        )
                                        .map_err(|e| e.to_string())?,
                                    );
                                    player = Some(next);
                                    remembered=Some(path);
                                    timeline=passages;
                                    let mut s =
                                        snapshot.lock().map_err(|_| "Playback unavailable")?;
                                    s.render_id = Some(id);
                                    s.duration_ms = duration_ms;
                                    s.position_ms = 0;
                                    s.playing = false;
                                    s.error = None;
                                }
                                Command::Play => {
                                    if player.as_ref().is_none_or(Player::empty) {
                                        let path=remembered.as_ref().ok_or("Choose a completed render")?;
                                        if device.is_none(){device=Some(open_device(failed.clone())?);}
                                        let replay=Player::connect_new(device.as_ref().ok_or("Audio unavailable")?.mixer());
                                        let state=snapshot.lock().map_err(|_|"Playback unavailable")?;
                                        replay.set_volume(state.volume);
                                        replay.append(Decoder::try_from(std::fs::File::open(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?);
                                        if player.is_none()&&state.position_ms<state.duration_ms{replay.try_seek(Duration::from_millis(u64::from(state.position_ms))).map_err(|e|e.to_string())?;}
                                        drop(state);player=Some(replay);
                                    }
                                    player.as_ref().ok_or("Choose a completed render")?.play();
                                    if let Some(a) = &ambience {
                                        a.play();
                                    }
                                    let mut state=snapshot.lock().map_err(|_|"Playback unavailable")?;state.playing=true;state.error=None;
                                }
                                Command::Pause => {
                                    if let Some(p) = &player {
                                        p.pause();
                                    }
                                    if let Some(a) = &ambience {
                                        a.pause();
                                    }
                                    snapshot.lock().map_err(|_| "Playback unavailable")?.playing =
                                        false;
                                }
                                Command::Stop => {
                                    if let Some(p) = &player {
                                        p.pause();
                                        let _ = p.try_seek(Duration::ZERO);
                                    }
                                    if let Some(a) = &ambience {
                                        a.pause();
                                    }
                                    let mut s =
                                        snapshot.lock().map_err(|_| "Playback unavailable")?;
                                    s.playing = false;
                                    s.position_ms = 0;
                                }
                                Command::Seek(ms) => {
                                    player
                                        .as_ref()
                                        .ok_or("Choose a completed render")?
                                        .try_seek(Duration::from_millis(u64::from(ms)))
                                        .map_err(|e| e.to_string())?;
                                }
                                Command::Volume(volume) => {
                                    if let Some(p) = &player {
                                        p.set_volume(volume);
                                    }
                                    snapshot.lock().map_err(|_| "Playback unavailable")?.volume =
                                        volume;
                                }
                                Command::Ambience(path) => {
                                    if let Some(old) = ambience.take() {
                                        old.stop();
                                    }
                                    if let Some(path) = path {
                                        if device.is_none() {
                                            device = Some(
                                                open_device(failed.clone())?,
                                            );
                                        }
                                        let p = Player::connect_new(
                                            device.as_ref().ok_or("Audio unavailable")?.mixer(),
                                        );
                                        p.set_volume(
                                            snapshot
                                                .lock()
                                                .map_err(|_| "Playback unavailable")?
                                                .ambience_volume,
                                        );
                                        p.append(
                                            Decoder::try_from(
                                                std::fs::File::open(path)
                                                    .map_err(|e| e.to_string())?,
                                            )
                                            .map_err(|e| e.to_string())?
                                            .repeat_infinite(),
                                        );
                                        if !snapshot
                                            .lock()
                                            .map_err(|_| "Playback unavailable")?
                                            .playing
                                        {
                                            p.pause();
                                        }
                                        ambience = Some(p);
                                    }
                                }
                                Command::AmbienceVolume(volume) => {
                                    if let Some(a) = &ambience {
                                        a.set_volume(volume);
                                    }
                                    snapshot
                                        .lock()
                                        .map_err(|_| "Playback unavailable")?
                                        .ambience_volume = volume;
                                }
                                Command::Sleep(minutes) => {
                                    let state=snapshot.lock().map_err(|_|"Playback unavailable")?;
                                    if let Some(p)=&player{p.set_volume(state.volume);}
                                    if let Some(a)=&ambience{a.set_volume(state.ambience_volume);}
                                    deadline = minutes.map(|m| {
                                        Instant::now() + Duration::from_secs(u64::from(m) * 60)
                                    });
                                }
                                Command::Barrier(reply)=>{if let Ok(state)=snapshot.lock(){let _=reply.send(state.clone());}}
                            }
                        }
                        let mut s = snapshot.lock().map_err(|_| "Playback unavailable")?;
                        if let Some(p) = &player {
                            s.position_ms =
                                p.get_pos().as_millis().min(u128::from(u32::MAX)) as u32;
                            if p.empty() {
                                s.playing = false;
                                if let Some(a) = &ambience {
                                    a.pause();
                                }
                            }
                        }
                        s.current_block_ids=timeline.iter().find(|p|s.position_ms>=p.start_ms&&s.position_ms<p.end_ms).map(|p|p.block_ids.clone()).unwrap_or_default();
                        s.sleep_remaining_ms = deadline.map(|end| {
                            end.saturating_duration_since(Instant::now())
                                .as_millis()
                                .min(u128::from(u32::MAX)) as u32
                        });
                        if let Some(remaining) = s.sleep_remaining_ms {
                            if remaining < 3000 {
                                let fade = remaining as f32 / 3000.0;
                                if let Some(p) = &player {
                                    p.set_volume(s.volume * fade);
                                }
                                if let Some(a) = &ambience {
                                    a.set_volume(s.ambience_volume * fade);
                                }
                            }
                            if remaining == 0 {
                                if let Some(p) = &player {
                                    p.pause();
                                    p.set_volume(s.volume);
                                }
                                if let Some(a) = &ambience {
                                    a.pause();
                                    a.set_volume(s.ambience_volume);
                                }
                                s.playing = false;
                                deadline = None;
                            }
                        }
                        Ok(())
                    })();
                    if let Err(error) = result
                        && let Ok(mut s) = snapshot.lock()
                    {
                        s.error = Some(error);
                        s.playing = false;
                    }
                }
            })
            ?;
        Ok(Self { sender, state })
    }
}

fn open_device(failed: Arc<AtomicBool>) -> Result<MixerDeviceSink, String> {
    DeviceSinkBuilder::from_default_device()
        .map_err(|_| "No audio output device is available")?
        .with_error_callback(move |_| {
            failed.store(true, Ordering::SeqCst);
        })
        .open_sink_or_fallback()
        .map_err(|_| "The audio output device could not be opened".into())
}

impl Audio {
    pub async fn synchronized_state(&self) -> Result<PlaybackState, String> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.command(Command::Barrier(sender))?;
        receiver.await.map_err(|_| "Playback thread stopped".into())
    }
    pub fn command(&self, command: Command) -> Result<(), String> {
        self.sender
            .send(command)
            .map_err(|_| "Playback thread stopped".into())
    }
    pub fn state(&self) -> Result<PlaybackState, String> {
        self.state
            .lock()
            .map(|s| s.clone())
            .map_err(|_| "Playback unavailable".into())
    }
}

mod export;
pub(crate) use export::export_audio;
