use crate::{
    application,
    bootstrap::App,
    domain::*,
    infrastructure::{audio::Command, downloads},
    storage::{Ambience, Collection, Script, ScriptPatch, ScriptSummary},
};
use std::sync::atomic::Ordering;
use tauri::Manager;

fn error(value: impl std::fmt::Display) -> AppError {
    value.to_string().into()
}

mod entry;
use entry::*;
mod library;
use library::*;
mod scripts;
use scripts::*;
mod settings;
use settings::*;
mod models;
use models::*;
mod jobs;
use jobs::*;
mod generation;
use generation::*;
mod narration;
use narration::*;
mod playback;
use playback::*;
mod providers;
use providers::*;

pub use jobs::TauRpcJobsEventTrigger;

pub fn router() -> taurpc::Router<tauri::Wry> {
    taurpc::Router::new()
        .merge(EntryImpl.into_handler())
        .merge(LibraryImpl.into_handler())
        .merge(ScriptsImpl.into_handler())
        .merge(SettingsImpl.into_handler())
        .merge(ModelsImpl.into_handler())
        .merge(JobsImpl.into_handler())
        .merge(GenerationImpl.into_handler())
        .merge(NarrationImpl.into_handler())
        .merge(PlaybackImpl.into_handler())
        .merge(ProvidersImpl.into_handler())
}
