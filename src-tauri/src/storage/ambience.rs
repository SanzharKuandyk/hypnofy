use super::*;

impl Store {
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
}
