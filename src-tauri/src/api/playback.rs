use super::*;

use super::narration::render_path;
#[taurpc::procedures(path = "playback")]
pub trait Playback {
    async fn rename_ambience<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
        name: String,
    ) -> ApiResult<()>;
    async fn remove_ambience<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()>;
    async fn preview_ambience<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: Option<String>,
    ) -> ApiResult<()>;
    async fn state<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> ApiResult<PlaybackState>;
    async fn load<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        render_id: String,
    ) -> ApiResult<()>;
    async fn control<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        operation: String,
        value: Option<f32>,
    ) -> ApiResult<()>;
    async fn ambience<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<Ambience>>;
    async fn import_ambience<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        path: String,
    ) -> ApiResult<Ambience>;
    async fn select_ambience<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: Option<String>,
    ) -> ApiResult<()>;
}

#[derive(Clone)]
pub struct PlaybackImpl;

#[taurpc::resolvers]
impl Playback for PlaybackImpl {
    async fn rename_ambience<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
        name: String,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state
            .db
            .call(move |s| s.rename_ambience(&id, &name).map_err(error))
            .await
    }

    async fn remove_ambience<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let audio = state.audio.synchronized_state().await.map_err(error)?;
        if audio.preview_ambience_id.as_deref() == Some(&id) {
            state
                .audio
                .command(Command::PreviewAmbience(None))
                .map_err(error)?;
        }
        if audio.ambience_id.as_deref() == Some(&id) {
            state
                .audio
                .command(Command::Ambience(None))
                .map_err(error)?;
        }
        state.audio.synchronized_state().await.map_err(error)?;
        state
            .db
            .call(move |s| s.remove_ambience(&id).map_err(error))
            .await
    }

    async fn preview_ambience<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: Option<String>,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let track = if let Some(id) = id {
            Some(
                state
                    .db
                    .call(move |s| {
                        s.ambience()
                            .map_err(error)?
                            .into_iter()
                            .find(|a| a.id == id)
                            .map(|a| (a.id, std::path::PathBuf::from(a.path)))
                            .ok_or_else(|| AppError::from("Sound is unavailable"))
                    })
                    .await?,
            )
        } else {
            None
        };
        state
            .audio
            .command(Command::PreviewAmbience(track))
            .map_err(error)?;
        state.audio.synchronized_state().await.map_err(error)?;
        Ok(())
    }
    async fn state<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<PlaybackState> {
        let state = app_handle.state::<App>();
        let snapshot = state.audio.state().map_err(error)?;
        if let Some(id) = snapshot.render_id.clone()
            && !state.db.call(move |s| s.can_play_render(&id)).await?
        {
            state.audio.command(Command::Unload).map_err(error)?;
            return state.audio.synchronized_state().await.map_err(error);
        }
        Ok(snapshot)
    }
    async fn load<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        render_id: String,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state.jobs.ensure_available()?;
        let id = render_id.clone();
        if !state.db.call(move |s| s.can_play_render(&id)).await? {
            return Err("This recording is deleted or its script is in Trash. Restore the script before playing.".into());
        }
        let path = render_path(&state, &render_id)?;
        let reader = hound::WavReader::open(&path).map_err(error)?;
        let duration_ms =
            (u64::from(reader.duration()) * 1000 / u64::from(reader.spec().sample_rate)) as u32;
        drop(reader);
        let passages = std::fs::read(path.with_extension("json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.pointer("/record/passages").cloned())
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        state
            .audio
            .command(Command::Load {
                id: render_id,
                path,
                duration_ms,
                passages,
            })
            .map_err(error)
    }
    async fn control<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        operation: String,
        value: Option<f32>,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        let value = value.unwrap_or(0.0);
        if !value.is_finite() {
            return Err("Invalid playback value".into());
        }
        let command = match operation.as_str() {
            "play" => Command::Play,
            "pause" => Command::Pause,
            "stop" => Command::Stop,
            "unload" => Command::Unload,
            "seek" => Command::Seek(value.max(0.0) as u32),
            "volume" if (0.0..=1.0).contains(&value) => Command::Volume(value),
            "ambience-volume" if (0.0..=1.0).contains(&value) => Command::AmbienceVolume(value),
            "sleep" if (0.0..=1440.0).contains(&value) => Command::Sleep(if value == 0.0 {
                None
            } else {
                Some(value as u32)
            }),
            _ => return Err("Unsupported playback control".into()),
        };
        state.audio.command(command).map_err(error)?;
        if operation == "unload" {
            state.audio.synchronized_state().await.map_err(error)?;
        }
        Ok(())
    }
    async fn ambience<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<Ambience>> {
        let state = app_handle.state::<App>();
        state.db.call(|s| s.ambience().map_err(error)).await
    }
    async fn import_ambience<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        path: String,
    ) -> ApiResult<Ambience> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state
            .db
            .call(move |s| {
                s.import_ambience(std::path::Path::new(&path))
                    .map_err(error)
            })
            .await
    }
    async fn select_ambience<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: Option<String>,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        let path = if let Some(id) = id {
            Some(
                state
                    .db
                    .call(move |s| {
                        s.ambience()
                            .map_err(error)?
                            .into_iter()
                            .find(|a| a.id == id)
                            .map(|a| (a.id, std::path::PathBuf::from(a.path)))
                            .ok_or_else(|| AppError::from("Ambience is unavailable"))
                    })
                    .await?,
            )
        } else {
            None
        };
        state.audio.command(Command::Ambience(path)).map_err(error)
    }
}
