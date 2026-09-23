use crate::config::AppSettings;
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use uuid::Uuid;

pub struct Store {
    connection: Mutex<Connection>,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub script_count: i64,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSettings {
    pub voice: String,
    pub speed: f32,
    pub paragraph_pause: f32,
    pub ambience_id: Option<String>,
    pub ambience_volume: f32,
    pub narration_volume: f32,
    pub ambience_mode: String,
    pub sleep_minutes: Option<u32>,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            voice: "af_heart".into(),
            speed: 0.9,
            paragraph_pause: 2.0,
            ambience_id: None,
            ambience_volume: 0.063,
            narration_volume: 1.0,
            ambience_mode: "loop".into(),
            sleep_minutes: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    pub id: String,
    pub collection_id: String,
    pub collection_name: String,
    pub title: String,
    pub body: String,
    pub favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub revision: i64,
    pub session: SessionSettings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptPatch {
    pub id: String,
    pub title: String,
    pub body: String,
    pub collection_id: String,
    pub favorite: bool,
    pub expected_revision: i64,
    pub session: SessionSettings,
}

impl Store {
    pub fn open(data_dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&data_dir)?;
        for dir in [
            "audio/chunks",
            "audio/ambience",
            "models",
            "engines",
            "exports",
            "downloads",
        ] {
            fs::create_dir_all(data_dir.join(dir))?;
        }
        let path = data_dir.join("hypnofy.sqlite3");
        let mut connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::from_secs(3))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;",
        )?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 && path.exists() && path.metadata()?.len() > 0 {
            connection.execute_batch("PRAGMA wal_checkpoint(FULL)")?;
            let backup_path = data_dir.join(format!(
                "hypnofy-before-v1-{}.sqlite3",
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
             PRAGMA user_version=1;"
        )?;
        let now = now();
        tx.execute("INSERT OR IGNORE INTO collections(id,name,created_at,updated_at) VALUES('inbox','My scripts',?1,?1)", [&now])?;
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

    pub fn collections(&self, include_trash: bool) -> Result<Vec<Collection>, String> {
        let conn = self.conn()?;
        let deleted = if include_trash {
            "IS NOT NULL"
        } else {
            "IS NULL"
        };
        let sql = format!(
            "SELECT c.id,c.name,COUNT(s.id) FROM collections c LEFT JOIN scripts s ON s.collection_id=c.id AND s.deleted_at {deleted} GROUP BY c.id,c.name ORDER BY lower(c.name)"
        );
        let mut stmt = conn.prepare(&sql).map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Collection {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    script_count: r.get(2)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn create_collection(&self, name: &str) -> Result<Collection, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err("Collection name must be 1–80 characters".into());
        }
        let conn = self.conn()?;
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        conn.execute(
            "INSERT INTO collections VALUES(?1,?2,?3,?3)",
            params![id, name, timestamp],
        )
        .map_err(err)?;
        Ok(Collection {
            id,
            name: name.into(),
            script_count: 0,
        })
    }

    pub fn list_scripts(
        &self,
        collection: Option<&str>,
        query: &str,
        favorite: bool,
        trash: bool,
    ) -> Result<Vec<ScriptSummary>, String> {
        let conn = self.conn()?;
        let pattern = format!("%{}%", query.trim());
        let mut stmt = conn
            .prepare(
                "SELECT s.id,s.collection_id,c.name,s.title,s.favorite,s.updated_at,s.revision
             FROM scripts s JOIN collections c ON c.id=s.collection_id
             WHERE ((?1=1 AND s.deleted_at IS NOT NULL) OR (?1=0 AND s.deleted_at IS NULL))
               AND (?2 IS NULL OR s.collection_id=?2)
               AND (?3=0 OR s.favorite=1)
               AND (?4='' OR s.title LIKE ?5 ESCAPE '\\' OR s.body LIKE ?5 ESCAPE '\\')
             ORDER BY s.favorite DESC,s.updated_at DESC LIMIT 5000",
            )
            .map_err(err)?;
        let rows = stmt
            .query_map(
                params![trash, collection, favorite, query.trim(), pattern],
                |r| {
                    Ok(ScriptSummary {
                        id: r.get(0)?,
                        collection_id: r.get(1)?,
                        collection_name: r.get(2)?,
                        title: r.get(3)?,
                        favorite: r.get(4)?,
                        updated_at: r.get(5)?,
                        revision: r.get(6)?,
                    })
                },
            )
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn get_script(&self, id: &str) -> Result<Script, String> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT s.id,s.collection_id,c.name,s.title,s.body,s.favorite,s.created_at,s.updated_at,s.deleted_at,s.revision,s.session_json FROM scripts s JOIN collections c ON c.id=s.collection_id WHERE s.id=?1",
            [id], |r| {
                let raw:String=r.get(10)?;
                let session = serde_json::from_str(&raw).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        10,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(Script { id:r.get(0)?,collection_id:r.get(1)?,collection_name:r.get(2)?,title:r.get(3)?,body:r.get(4)?,favorite:r.get(5)?,created_at:r.get(6)?,updated_at:r.get(7)?,deleted_at:r.get(8)?,revision:r.get(9)?,session })
            }
        ).optional().map_err(err)?.ok_or_else(|| "Script was not found".into())
    }

    pub fn create_script(
        &self,
        collection_id: Option<&str>,
        title: Option<&str>,
        body: &str,
    ) -> Result<Script, String> {
        let conn = self.conn()?;
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        let collection = collection_id.unwrap_or("inbox");
        let title = title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Untitled script");
        let session = SessionSettings::default();
        let session_json = serde_json::to_string(&session).map_err(err)?;
        conn.execute(
            "INSERT INTO scripts VALUES(?1,?2,?3,?4,0,?5,?5,NULL,1,?6)",
            params![id, collection, title, body, timestamp, session_json],
        )
        .map_err(err)?;
        drop(conn);
        self.get_script(&id)
    }

    pub fn update_script(&self, patch: ScriptPatch) -> Result<Script, String> {
        // Save the prior immutable revision in the same transaction as the optimistic update.
        if patch.title.trim().is_empty() {
            return Err("A script needs a title".into());
        }
        if !(0.5..=2.0).contains(&patch.session.speed) {
            return Err("Voice speed must be between 0.5× and 2×".into());
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(err)?;
        let current: (String, String, String, i64) = tx
            .query_row(
                "SELECT title,body,session_json,revision FROM scripts WHERE id=?1",
                [&patch.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(err)?
            .ok_or_else(|| "Script was not found".to_string())?;
        if current.3 != patch.expected_revision {
            return Err("This script changed elsewhere. Reload it before saving.".into());
        }
        tx.execute(
            "INSERT INTO script_revisions VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                Uuid::new_v4().to_string(),
                patch.id,
                current.3,
                current.0,
                current.1,
                current.2,
                now()
            ],
        )
        .map_err(err)?;
        let changed=tx.execute("UPDATE scripts SET title=?2,body=?3,collection_id=?4,favorite=?5,updated_at=?6,revision=revision+1,session_json=?7 WHERE id=?1 AND revision=?8",params![patch.id,patch.title.trim(),patch.body,patch.collection_id,patch.favorite,now(),serde_json::to_string(&patch.session).map_err(err)?,patch.expected_revision]).map_err(err)?;
        if changed != 1 {
            return Err("The script could not be saved".into());
        }
        tx.commit().map_err(err)?;
        drop(conn);
        self.get_script(&patch.id)
    }

    pub fn duplicate_script(&self, id: &str) -> Result<Script, String> {
        let source = self.get_script(id)?;
        self.create_script(
            Some(&source.collection_id),
            Some(&format!("{} copy", source.title)),
            &source.body,
        )
    }

    pub fn set_deleted(&self, id: &str, deleted: bool) -> Result<(), String> {
        let conn = self.conn()?;
        let value = if deleted { Some(now()) } else { None };
        if conn
            .execute(
                "UPDATE scripts SET deleted_at=?2,updated_at=?3 WHERE id=?1",
                params![id, value, now()],
            )
            .map_err(err)?
            != 1
        {
            return Err("Script was not found".into());
        }
        Ok(())
    }

    pub fn import_text(&self, source: &Path, collection: Option<&str>) -> Result<Script, String> {
        let extension = source
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension != "txt" && extension != "md" {
            return Err("Only .txt and .md files can be imported".into());
        }
        let metadata = fs::metadata(source).map_err(err)?;
        if metadata.len() > 2_000_000 {
            return Err("The text file is larger than 2 MB".into());
        }
        let body = fs::read_to_string(source)
            .map_err(|_| "The file is not valid UTF-8 text".to_string())?;
        let title = source
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("Imported script");
        self.create_script(collection, Some(title), &body)
    }

    pub fn export_text(&self, id: &str, destination: &Path) -> Result<(), String> {
        let script = self.get_script(id)?;
        fs::write(destination, script.body).map_err(err)
    }

    pub fn ambience(&self) -> Result<Vec<Ambience>, String> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT id,name,path FROM ambience ORDER BY lower(name)")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Ambience {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    path: r.get(2)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }

    pub fn import_ambience(&self, source: &Path) -> Result<Ambience, String> {
        let ext = source
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !["wav", "mp3", "flac", "ogg"].contains(&ext.as_str()) {
            return Err("Use a WAV, MP3, FLAC, or Ogg audio file".into());
        }
        if fs::metadata(source).map_err(err)?.len() > 1_000_000_000 {
            return Err("Audio files are limited to 1 GB".into());
        }
        let id = Uuid::new_v4().to_string();
        let destination = self
            .data_dir
            .join("audio/ambience")
            .join(format!("{id}.{ext}"));
        fs::copy(source, &destination).map_err(err)?;
        let name = source
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("Ambience")
            .to_string();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO ambience VALUES(?1,?2,?3,?4)",
            params![id, name, destination.to_string_lossy(), now()],
        )
        .map_err(err)?;
        Ok(Ambience {
            id,
            name,
            path: destination.to_string_lossy().into_owned(),
        })
    }
    pub fn ambience_path(&self, id: &str) -> Result<PathBuf, String> {
        let conn = self.conn()?;
        conn.query_row("SELECT path FROM ambience WHERE id=?1", [id], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .map_err(err)?
        .map(PathBuf::from)
        .ok_or_else(|| "Ambience track was not found".into())
    }

    pub fn load_app_settings(&self, defaults: AppSettings) -> Result<AppSettings, String> {
        let conn = self.conn()?;
        let stored = conn
            .query_row(
                "SELECT value FROM settings WHERE key='app_preferences'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(err)?;
        match stored {
            Some(json) => serde_json::from_str(&json)
                .map_err(|error| format!("Saved application settings are invalid: {error}")),
            None => Ok(defaults),
        }
    }

    pub fn save_app_settings(&self, settings: &AppSettings) -> Result<(), String> {
        settings.validate()?;
        let json = serde_json::to_string(settings).map_err(err)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO settings(key,value) VALUES('app_preferences',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [json],
        )
        .map_err(err)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
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
