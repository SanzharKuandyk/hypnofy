use super::Store;
use crate::domain::{ApiResult, AppError, RenderRecord, StoredRecording};
use rusqlite::OptionalExtension;

fn error(value: impl std::fmt::Display) -> AppError {
    AppError::from(value.to_string())
}

impl Store {
    pub fn recording_library(&self) -> ApiResult<Vec<StoredRecording>> {
        let conn = self.conn().map_err(error)?;
        let mut statement=conn.prepare("SELECT r.record_json,COALESCE(s.title,'Deleted script'),CASE WHEN s.id IS NULL THEN 'deleted' WHEN s.deleted_at IS NOT NULL THEN 'trash' ELSE 'active' END,EXISTS(SELECT 1 FROM rewrite_settings d WHERE d.key='render-delete:'||r.id) FROM render_records r LEFT JOIN scripts s ON s.id=r.script_id UNION ALL SELECT d.value,'Deleted script','deleted',1 FROM rewrite_settings d WHERE d.key LIKE 'render-delete:%' AND d.value!='{}' AND NOT EXISTS(SELECT 1 FROM render_records r WHERE d.key='render-delete:'||r.id)").map_err(error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            })
            .map_err(error)?;
        rows.map(|row| {
            let (json, script_title, script_status, pending_deletion) = row.map_err(error)?;
            Ok(StoredRecording {
                render: serde_json::from_str::<RenderRecord>(&json)
                    .map_err(|e| AppError::from(e.to_string()))?,
                script_title,
                script_status,
                pending_deletion,
            })
        })
        .collect()
    }

    pub fn render_script(&self, id: &str) -> ApiResult<Option<String>> {
        self.conn()
            .map_err(error)?
            .query_row(
                "SELECT script_id FROM render_records WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(error)
    }

    pub fn can_play_render(&self, id: &str) -> ApiResult<bool> {
        self.conn().map_err(error)?.query_row("SELECT EXISTS(SELECT 1 FROM render_records r JOIN scripts s ON s.id=r.script_id WHERE r.id=?1 AND s.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM rewrite_settings d WHERE d.key='render-delete:'||r.id))",[id],|r|r.get(0)).map_err(error)
    }

    pub fn can_export_render(&self, id: &str) -> ApiResult<bool> {
        self.conn().map_err(error)?.query_row("SELECT EXISTS(SELECT 1 FROM render_records r WHERE r.id=?1 AND NOT EXISTS(SELECT 1 FROM rewrite_settings d WHERE d.key='render-delete:'||r.id))", [id], |r| r.get(0)).map_err(error)
    }

    pub fn queue_recording_removal(&self, id: &str) -> ApiResult<()> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err("Invalid recording ID".into());
        }
        if self.json_get(&format!("render-delete:{id}"))?.is_some() {
            return Ok(());
        }
        if self.render_script(id)?.is_none() {
            return Err("Recording not found".into());
        }
        self.json_set(&format!("render-delete:{id}"), "{}")
    }

    pub fn queue_orphaned_recording(&self, record: &RenderRecord) -> ApiResult<()> {
        if uuid::Uuid::parse_str(&record.id).is_err() || self.render_script(&record.id)?.is_some() {
            return Err("Recording is invalid or still referenced".into());
        }
        self.json_set(
            &format!("render-delete:{}", record.id),
            &serde_json::to_string(record).map_err(error)?,
        )
    }

    pub fn pending_recording_removals(&self) -> ApiResult<Vec<String>> {
        let conn = self.conn().map_err(error)?;
        let mut statement = conn
            .prepare("SELECT substr(key,15) FROM rewrite_settings WHERE key LIKE 'render-delete:%'")
            .map_err(error)?;
        statement
            .query_map([], |r| r.get(0))
            .map_err(error)?
            .collect::<Result<_, _>>()
            .map_err(error)
    }

    pub fn finish_recording_removal(&self, id: &str) -> ApiResult<()> {
        let mut conn = self.conn().map_err(error)?;
        let tx = conn.transaction().map_err(error)?;
        tx.execute("DELETE FROM render_records WHERE id=?1", [id])
            .map_err(error)?;
        tx.execute("DELETE FROM renders WHERE id=?1", [id])
            .map_err(error)?;
        tx.execute(
            "DELETE FROM rewrite_settings WHERE key=?1",
            [format!("render-delete:{id}")],
        )
        .map_err(error)?;
        tx.commit().map_err(error)
    }
}
