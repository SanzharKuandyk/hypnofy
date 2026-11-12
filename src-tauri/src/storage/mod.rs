use crate::document;
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::PathBuf, sync::Mutex, time::Duration};
use uuid::Uuid;

pub struct Store {
    connection: Mutex<Connection>,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub script_count: i64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScriptSummary {
    pub id: String,
    pub collection_id: String,
    pub collection_name: String,
    pub title: String,
    pub favorite: bool,
    pub updated_at: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionSettings {
    pub voice_id: Option<String>,
    pub speed: f32,
    pub paragraph_pause_ms: u32,
    pub language: String,
    pub ambience_id: Option<String>,
    pub ambience_volume: f32,
    pub narration_volume: f32,
    pub ambience_mode: String,
    pub sleep_minutes: Option<u32>,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            voice_id: None,
            speed: 1.0,
            paragraph_pause_ms: 2000,
            language: "en-gb".into(),
            ambience_id: None,
            ambience_volume: 0.06,
            narration_volume: 1.0,
            ambience_mode: "loop".into(),
            sleep_minutes: None,
        }
    }
}

impl SessionSettings {
    pub fn from_preferences(preferences: &crate::domain::Preferences) -> Self {
        Self {
            voice_id: preferences.voice_id.clone(),
            speed: preferences.speed,
            paragraph_pause_ms: preferences.paragraph_pause_ms,
            language: preferences.language.clone(),
            narration_volume: preferences.narration_volume,
            ambience_volume: preferences.ambience_volume,
            ..Self::default()
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.speed.is_finite()
            || !(0.5..=2.0).contains(&self.speed)
            || self.paragraph_pause_ms > 30_000
            || !self.ambience_volume.is_finite()
            || !(0.0..=1.0).contains(&self.ambience_volume)
            || !self.narration_volume.is_finite()
            || !(0.0..=1.0).contains(&self.narration_volume)
            || self.sleep_minutes.is_some_and(|m| m > 1440)
        {
            return Err("Narration settings are outside supported ranges".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    pub id: String,
    pub collection_id: String,
    pub collection_name: String,
    pub title: String,
    pub body: String,
    pub content_hash: String,
    #[specta(type = crate::domain::JsonValue)]
    pub document: Value,
    pub last_model_id: Option<String>,
    pub favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub revision: i64,
    pub session: SessionSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScriptPatch {
    pub id: String,
    pub title: String,
    #[specta(type = crate::domain::JsonValue)]
    pub document: Value,
    pub last_model_id: Option<String>,
    pub collection_id: String,
    pub favorite: bool,
    pub expected_revision: i64,
    pub session: SessionSettings,
}

mod ambience;
mod recordings;
mod rewrite;
mod scripts;
mod session;
#[cfg(test)]
mod tests;

impl Store {
    pub fn open(data_dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&data_dir)?;
        for dir in ["audio/chunks", "audio/ambience"] {
            fs::create_dir_all(data_dir.join(dir))?;
        }
        let path = data_dir.join("hypnofy.sqlite3");
        let mut connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::from_secs(3))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;",
        )?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 7 {
            anyhow::bail!("This library was created by a newer Hypnofy version");
        }
        if version < 7 && version > 0 && path.exists() && path.metadata()?.len() > 0 {
            connection.execute_batch("PRAGMA wal_checkpoint(FULL)")?;
            let backup_path = data_dir.join(format!(
                "hypnofy-before-v7-{}.sqlite3",
                Utc::now().format("%Y%m%d%H%M%S")
            ));
            let mut destination = Connection::open(backup_path)?;
            rusqlite::backup::Backup::new(&connection, &mut destination)?.run_to_completion(
                5,
                Duration::from_millis(50),
                None,
            )?;
        }
        let tx = connection.transaction()?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS collections (
               id TEXT PRIMARY KEY, name TEXT NOT NULL COLLATE NOCASE UNIQUE,
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS scripts (
               id TEXT PRIMARY KEY, collection_id TEXT NOT NULL REFERENCES collections(id),
               title TEXT NOT NULL, body TEXT NOT NULL DEFAULT '', favorite INTEGER NOT NULL DEFAULT 0,
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
               revision INTEGER NOT NULL DEFAULT 1, session_json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_scripts_collection ON scripts(collection_id, deleted_at, updated_at DESC);
             CREATE INDEX IF NOT EXISTS idx_scripts_recent ON scripts(deleted_at, updated_at DESC);
             CREATE TABLE IF NOT EXISTS script_revisions (
               id TEXT PRIMARY KEY, script_id TEXT NOT NULL REFERENCES scripts(id) ON DELETE CASCADE,
               revision INTEGER NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL,
               session_json TEXT NOT NULL, saved_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS ambience (
               id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL UNIQUE,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS renders (
               id TEXT PRIMARY KEY, script_id TEXT NOT NULL REFERENCES scripts(id),
               script_revision INTEGER NOT NULL, manifest_json TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             "
        )?;
        if version < 2 {
            tx.execute_batch(
                "ALTER TABLE scripts ADD COLUMN document_json TEXT;
                              ALTER TABLE scripts ADD COLUMN last_model_id TEXT;
                              ALTER TABLE script_revisions ADD COLUMN document_json TEXT;",
            )?;
            let legacy: Vec<(String, String)> = {
                let mut stmt = tx.prepare("SELECT id,body FROM scripts")?;
                stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<Result<_, _>>()?
            };
            for (id, body) in legacy {
                let encoded = serde_json::to_string(&document::from_plain_text(&body))?;
                tx.execute(
                    "UPDATE scripts SET document_json=?2 WHERE id=?1",
                    params![id, encoded],
                )?;
            }
            let old_revisions: Vec<(String, String)> = {
                let mut stmt = tx.prepare("SELECT id,body FROM script_revisions")?;
                stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<Result<_, _>>()?
            };
            for (id, body) in old_revisions {
                let encoded = serde_json::to_string(&document::from_plain_text(&body))?;
                tx.execute(
                    "UPDATE script_revisions SET document_json=?2 WHERE id=?1",
                    params![id, encoded],
                )?;
            }
            tx.execute_batch("PRAGMA user_version=2")?;
        }
        if version < 3 {
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS linked_models (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            ); PRAGMA user_version=3;",
            )?;
        }
        if version < 4 {
            tx.execute_batch(
                "ALTER TABLE linked_models ADD COLUMN verified_bytes INTEGER;
                 ALTER TABLE linked_models ADD COLUMN verified_mtime_ns INTEGER;
                 ALTER TABLE linked_models ADD COLUMN verified_runtime TEXT;
                 PRAGMA user_version=4;",
            )?;
        }
        if version < 5 {
            rewrite::migrate(&tx)?;
        }
        if version < 6 {
            tx.execute_batch(include_str!("../../migrations/0006_owned_staging.sql"))?;
        }
        if version < 7 {
            rewrite::migrate_sessions(&tx)?;
            tx.execute_batch(include_str!(
                "../../migrations/0007_millisecond_settings.sql"
            ))?;
        }
        let now = now();
        tx.execute("INSERT OR IGNORE INTO collections(id,name,created_at,updated_at) VALUES('inbox','My scripts',?1,?1)", [&now])?;
        let violation: Option<String> = tx
            .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
            .optional()?;
        if violation.is_some() {
            anyhow::bail!(
                "The library contains invalid references. Migration was rolled back; its backup is preserved."
            );
        }
        tx.commit()?;
        Ok(Self {
            connection: Mutex::new(connection),
            data_dir,
        })
    }

    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "Storage lock is unavailable".into())
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Ambience {
    pub id: String,
    pub name: String,
    pub path: String,
}

fn now() -> String {
    Utc::now().to_rfc3339()
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
