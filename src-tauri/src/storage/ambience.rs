use super::*;
use std::path::Path;

impl Store {
    pub fn rename_ambience(&self, id: &str, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err("Use a sound name of 1–80 characters".into());
        }
        if self
            .conn()?
            .execute("UPDATE ambience SET name=?2 WHERE id=?1", params![id, name])
            .map_err(err)?
            != 1
        {
            return Err("Sound not found".into());
        }
        Ok(())
    }

    pub fn remove_ambience(&self, id: &str) -> Result<(), String> {
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(err)?;
        let used: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM scripts WHERE json_extract(session_json,'$.ambienceId')=?1 UNION ALL SELECT 1 FROM script_revisions WHERE json_extract(session_json,'$.ambienceId')=?1)",[id],|r|r.get(0)).map_err(err)?;
        if used {
            return Err("This sound is used by a script or saved revision. Keep it to preserve that session.".into());
        }
        let path: String = tx
            .query_row("SELECT path FROM ambience WHERE id=?1", [id], |r| r.get(0))
            .map_err(err)?;
        let path = std::path::PathBuf::from(path);
        let owned = self
            .data_dir
            .join("audio/ambience")
            .canonicalize()
            .map_err(err)?;
        if path
            .parent()
            .ok_or("Invalid sound path")?
            .canonicalize()
            .map_err(err)?
            != owned
            || path.file_stem().and_then(|s| s.to_str()) != Some(id)
            || fs::symlink_metadata(&path)
                .map_err(err)?
                .file_type()
                .is_symlink()
        {
            return Err("Sound ownership could not be verified".into());
        }
        tx.execute("DELETE FROM ambience WHERE id=?1", [id])
            .map_err(err)?;
        fs::remove_file(&path)
            .map_err(|_| "The sound is in use. Stop playback or export and retry.")?;
        tx.commit().map_err(err)
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
        let decoder = rodio::Decoder::try_from(fs::File::open(source).map_err(err)?)
            .map_err(|_| "The ambience file could not be decoded")?;
        drop(decoder);
        let id = Uuid::new_v4().to_string();
        let destination = self
            .data_dir
            .join("audio/ambience")
            .join(format!("{id}.{ext}"));
        let mut temporary = tempfile::NamedTempFile::new_in(
            destination.parent().ok_or("Invalid ambience directory")?,
        )
        .map_err(err)?;
        std::io::copy(&mut fs::File::open(source).map_err(err)?, &mut temporary).map_err(err)?;
        temporary.as_file().sync_all().map_err(err)?;
        temporary.persist(&destination).map_err(err)?;
        let name = source
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("Ambience")
            .to_string();
        let conn = self.conn()?;
        let result = conn.execute(
            "INSERT INTO ambience VALUES(?1,?2,?3,?4)",
            params![id, name, destination.to_string_lossy(), now()],
        );
        if let Err(error) = result {
            let _ = fs::remove_file(&destination);
            return Err(err(error));
        }
        Ok(Ambience {
            id,
            name,
            path: destination.to_string_lossy().into_owned(),
        })
    }
}
