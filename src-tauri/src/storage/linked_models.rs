use super::*;

impl Store {
    pub fn linked_models(&self) -> Result<Vec<LinkedModel>, String> {
        let conn = self.conn()?;
        let mut statement = conn
            .prepare("SELECT id,name,path,verified_bytes,verified_mtime_ns,verified_runtime FROM linked_models ORDER BY lower(name)")
            .map_err(err)?;
        statement
            .query_map([], linked_from_row)
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)
    }

    pub fn linked_model(&self, id: &str) -> Result<LinkedModel, String> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT id,name,path,verified_bytes,verified_mtime_ns,verified_runtime FROM linked_models WHERE id=?1",
            [id],
            linked_from_row,
        )
        .optional()
        .map_err(err)?
        .ok_or_else(|| "Linked model was not found".into())
    }

    pub fn link_gguf(&self, path: &Path, existing_id: Option<&str>) -> Result<LinkedModel, String> {
        if path
            .extension()
            .and_then(|part| part.to_str())
            .is_none_or(|part| !part.eq_ignore_ascii_case("gguf"))
        {
            return Err("Choose a .gguf model file".into());
        }
        let canonical = path.canonicalize().map_err(err)?;
        if !canonical.is_file() {
            return Err("Model path is not a file".into());
        }
        let mut file = fs::File::open(&canonical).map_err(err)?;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic).map_err(err)?;
        if &magic != b"GGUF" {
            return Err("File is not a GGUF model".into());
        }
        let name = canonical
            .file_name()
            .and_then(|part| part.to_str())
            .ok_or("Model filename is invalid")?
            .to_owned();
        let id = existing_id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let conn = self.conn()?;
        if existing_id.is_some() {
            if conn
                .execute(
                    "UPDATE linked_models SET name=?2,path=?3,updated_at=?4,verified_bytes=NULL,verified_mtime_ns=NULL,verified_runtime=NULL WHERE id=?1",
                    params![id, name, canonical.to_string_lossy(), now()],
                )
                .map_err(err)?
                != 1
            {
                return Err("Linked model was not found".into());
            }
        } else {
            conn.execute(
                "INSERT INTO linked_models(id,name,path,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",
                params![id, name, canonical.to_string_lossy(), now()],
            )
            .map_err(err)?;
        }
        Ok(LinkedModel {
            id,
            name,
            path: canonical.to_string_lossy().into_owned(),
            verified_bytes: None,
            verified_mtime_ns: None,
            verified_runtime: None,
        })
    }

    pub fn gguf_verified(&self, linked: &LinkedModel, runtime: &str) -> bool {
        let Ok((bytes, mtime_ns)) = fingerprint(Path::new(&linked.path)) else {
            return false;
        };
        linked.verified_bytes == Some(bytes)
            && linked.verified_mtime_ns == Some(mtime_ns)
            && linked.verified_runtime.as_deref() == Some(runtime)
    }

    /// Store the exact file fingerprint only after llama.cpp reports a loaded model.
    pub fn mark_gguf_verified(&self, linked: &LinkedModel, runtime: &str) -> Result<(), String> {
        let (bytes, mtime_ns) = fingerprint(Path::new(&linked.path))?;
        let conn = self.conn()?;
        if conn.execute(
            "UPDATE linked_models SET verified_bytes=?3,verified_mtime_ns=?4,verified_runtime=?5,updated_at=?6 WHERE id=?1 AND path=?2",
            params![linked.id, linked.path, bytes, mtime_ns, runtime, now()],
        ).map_err(err)? != 1 {
            return Err("The linked model changed during verification. Relink and retry.".into());
        }
        Ok(())
    }

    pub fn unlink_gguf(&self, id: &str) -> Result<(), String> {
        let conn = self.conn()?;
        if conn
            .execute("DELETE FROM linked_models WHERE id=?1", [id])
            .map_err(err)?
            != 1
        {
            return Err("Linked model was not found".into());
        }
        Ok(())
    }
}

fn linked_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LinkedModel> {
    Ok(LinkedModel {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        verified_bytes: row.get(3)?,
        verified_mtime_ns: row.get(4)?,
        verified_runtime: row.get(5)?,
    })
}

fn fingerprint(path: &Path) -> Result<(i64, i64), String> {
    let metadata = fs::metadata(path).map_err(err)?;
    let bytes = i64::try_from(metadata.len()).map_err(|_| "Model file is too large")?;
    let mtime_ns = metadata
        .modified()
        .map_err(err)?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(err)?
        .as_nanos();
    let mtime_ns = i64::try_from(mtime_ns).map_err(|_| "Model file timestamp is invalid")?;
    Ok((bytes, mtime_ns))
}
