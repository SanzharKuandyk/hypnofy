import { invoke } from "@tauri-apps/api/core";

export type Collection = { id: string; name: string; scriptCount: number };
export type ScriptSummary = { id: string; collectionId: string; collectionName: string; title: string; favorite: boolean; updatedAt: string; revision: number };
export type SessionSettings = { voice: string; speed: number; paragraphPause: number; ambienceId: string | null; ambienceVolume: number; narrationVolume: number; ambienceMode: string; sleepMinutes: number | null };
export type Script = ScriptSummary & { body: string; createdAt: string; deletedAt: string | null; session: SessionSettings };
export type Ambience = { id: string; name: string; path: string };
export type TextModel = { provider: string; id: string; label: string; available: boolean; reason: string | null };
export type Voice = { id: string; name: string; accent: string; gender: string; installed: boolean };
export type SpeechStatus = { installed: boolean; downloading: boolean; engineVersion: string; modelVersion: string; downloadBytes: number };
export type PlayerState = { playing: boolean; positionSeconds: number; durationSeconds: number; finished: boolean };
export type AssetConfig = { id: string; kind: string; version: string; source: string; url: string; filename: string; bytes: number; sha256: string; license: string; installDirectory: string; expectedFile: string };
export type AppSettings = { defaultProvider: string; defaultModel: string; ggufModelPath: string; llamaServerPath: string; keepModelMinutes: number };
export type SettingsView = { preferences: AppSettings; dataDirectory: string; assets: AssetConfig[] };

export const api = {
  collections: () => invoke<Collection[]>("list_collections"),
  createCollection: (name: string) => invoke<Collection>("create_collection", { name }),
  scripts: (collectionId: string | null, query: string, favorite: boolean, trash: boolean) => invoke<ScriptSummary[]>("list_scripts", { collectionId, query, favorite, trash }),
  script: (id: string) => invoke<Script>("get_script", { id }),
  createScript: (collectionId?: string | null, title?: string | null, body?: string) => invoke<Script>("create_script", { collectionId, title, body }),
  updateScript: (patch: Script) => invoke<Script>("update_script", { patch: { id: patch.id, title: patch.title, body: patch.body, collectionId: patch.collectionId, favorite: patch.favorite, expectedRevision: patch.revision, session: patch.session } }),
  duplicateScript: (id: string) => invoke<Script>("duplicate_script", { id }),
  setDeleted: (id: string, deleted: boolean) => invoke<void>("set_script_deleted", { id, deleted }),
  importText: (path: string, collectionId?: string | null) => invoke<Script>("import_text", { path, collectionId }),
  exportText: (id: string, path: string) => invoke<void>("export_text", { id, path }),
  ambience: () => invoke<Ambience[]>("list_ambience"),
  importAmbience: (path: string) => invoke<Ambience>("import_ambience", { path }),
  models: () => invoke<TextModel[]>("list_text_models"),
  installModelRuntime: () => invoke<void>("install_model_runtime"),
  generate: (request: object) => invoke<string>("start_generation", { request }),
  cancelGeneration: (jobId: string) => invoke<void>("cancel_generation", { jobId }),
  speechStatus: () => invoke<SpeechStatus>("speech_status"),
  voices: () => invoke<Voice[]>("list_voices"),
  installVoices: () => invoke<string>("install_voice_pack"),
  synthesize: (request: object) => invoke<string>("start_synthesis", { request }),
  cancelSynthesis: (jobId: string) => invoke<void>("cancel_synthesis", { jobId }),
  startPlayback: (manifestPath: string, ambienceId: string | null, narrationVolume: number, ambienceVolume: number) => invoke<PlayerState>("start_playback", { manifestPath, ambienceId, narrationVolume, ambienceVolume }),
  togglePlayback: () => invoke<PlayerState>("toggle_playback"),
  appendPlayback: (manifestPath: string) => invoke<PlayerState>("append_playback", { manifestPath }),
  stopPlayback: () => invoke<void>("stop_playback"),
  seekPlayback: (seconds: number) => invoke<PlayerState>("seek_playback", { seconds }),
  setVolumes: (narration: number, ambience: number) => invoke<void>("set_playback_volumes", { narration, ambience }),
  playerState: () => invoke<PlayerState>("player_state"),
  getSettings: () => invoke<SettingsView>("get_app_settings"),
  saveSettings: (settings: AppSettings) => invoke<SettingsView>("save_app_settings", { settings })
};
