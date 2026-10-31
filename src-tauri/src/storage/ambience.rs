use super::*;
use std::path::Path;

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
