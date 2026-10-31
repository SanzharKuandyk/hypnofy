use super::*;

impl Store {
    pub fn import_document(
        &self,
        collection: Option<&str>,
        title: &str,
        document: Value,
        session: SessionSettings,
        favorite: bool,
        last_model_id: Option<String>,
    ) -> Result<Script, String> {
        session.validate()?;
        let body = document::plain_text(&document)?;
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 500 {
            return Err("Use a title of 1–500 characters".into());
        }
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        let conn = self.conn()?;
        conn.execute("INSERT INTO scripts(id,collection_id,title,body,favorite,created_at,updated_at,deleted_at,revision,session_json,document_json,last_model_id) VALUES(?1,?2,?3,?4,?5,?6,?6,NULL,1,?7,?8,?9)",params![id,collection.unwrap_or("inbox"),title,body,favorite,timestamp,serde_json::to_string(&session).map_err(err)?,serde_json::to_string(&document).map_err(err)?,last_model_id]).map_err(err)?;
        drop(conn);
        self.get_script(&id)
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
            "SELECT s.id,s.collection_id,c.name,s.title,s.body,s.favorite,s.created_at,s.updated_at,s.deleted_at,s.revision,s.session_json,s.document_json,s.last_model_id FROM scripts s JOIN collections c ON c.id=s.collection_id WHERE s.id=?1",
            [id], |r| {
                let raw:String=r.get(10)?;
                let session = serde_json::from_str(&raw).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        10,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                let raw_document: Option<String> = r.get(11)?;
                let document = match raw_document {
                    Some(value) => serde_json::from_str(&value).map_err(|error| rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(error)))?,
                    None => document::from_plain_text(&r.get::<_, String>(4)?),
                };
                Ok(Script { id:r.get(0)?,collection_id:r.get(1)?,collection_name:r.get(2)?,title:r.get(3)?,body:r.get(4)?,content_hash:document::content_hash(&document),document,last_model_id:r.get(12)?,favorite:r.get(5)?,created_at:r.get(6)?,updated_at:r.get(7)?,deleted_at:r.get(8)?,revision:r.get(9)?,session })
            }
        ).optional().map_err(err)?.ok_or_else(|| "Script was not found".into())
    }

    pub fn create_script(
        &self,
        collection_id: Option<&str>,
        title: Option<&str>,
        body: &str,
    ) -> Result<Script, String> {
        let session =
            SessionSettings::from_preferences(&self.preferences().map_err(|e| format!("{e:?}"))?);
        let conn = self.conn()?;
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        let collection = collection_id.unwrap_or("inbox");
        let title = title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Untitled script");
        let session_json = serde_json::to_string(&session).map_err(err)?;
        conn.execute(
            "INSERT INTO scripts(id,collection_id,title,body,favorite,created_at,updated_at,deleted_at,revision,session_json,document_json) VALUES(?1,?2,?3,?4,0,?5,?5,NULL,1,?6,?7)",
            params![id, collection, title, body, timestamp, session_json, serde_json::to_string(&document::from_plain_text(body)).map_err(err)?],
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
        patch.session.validate()?;
        let body = document::plain_text(&patch.document)?;
        let document_json = serde_json::to_string(&patch.document).map_err(err)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(err)?;
        let current: (String, String, String, i64, Option<String>, String, bool, Option<String>) = tx
            .query_row(
                "SELECT title,body,session_json,revision,document_json,collection_id,favorite,last_model_id FROM scripts WHERE id=?1",
                [&patch.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
            )
            .optional()
            .map_err(err)?
            .ok_or_else(|| "Script was not found".to_string())?;
        if current.3 != patch.expected_revision {
            return Err("This script changed elsewhere. Reload it before saving.".into());
        }
        if current.0 == patch.title.trim()
            && current.1 == body
            && serde_json::from_str::<SessionSettings>(&current.2).map_err(err)? == patch.session
            && current
                .4
                .as_deref()
                .map(serde_json::from_str::<Value>)
                .transpose()
                .map_err(err)?
                .as_ref()
                == Some(&patch.document)
            && current.5 == patch.collection_id
            && current.6 == patch.favorite
            && current.7 == patch.last_model_id
        {
            drop(tx);
            drop(conn);
            return self.get_script(&patch.id);
        }
        tx.execute(
            "INSERT INTO script_revisions(id,script_id,revision,title,body,session_json,saved_at,document_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                Uuid::new_v4().to_string(),
                patch.id,
                current.3,
                current.0,
                current.1,
                current.2,
                now(),
                current.4
            ],
        )
        .map_err(err)?;
        let changed=tx.execute("UPDATE scripts SET title=?2,body=?3,collection_id=?4,favorite=?5,updated_at=?6,revision=revision+1,session_json=?7,document_json=?9,last_model_id=?10 WHERE id=?1 AND revision=?8",params![patch.id,patch.title.trim(),body,patch.collection_id,patch.favorite,now(),serde_json::to_string(&patch.session).map_err(err)?,patch.expected_revision,document_json,patch.last_model_id]).map_err(err)?;
        if changed != 1 {
            return Err("The script could not be saved".into());
        }
        tx.commit().map_err(err)?;
        drop(conn);
        self.get_script(&patch.id)
    }

    pub fn duplicate_script(&self, id: &str) -> Result<Script, String> {
        let source = self.get_script(id)?;
        let duplicate = self.create_script(
            Some(&source.collection_id),
            Some(&format!("{} copy", source.title)),
            &source.body,
        )?;
        let conn = self.conn()?;
        conn.execute(
            "UPDATE scripts SET document_json=?2,last_model_id=?3,session_json=?4,favorite=?5 WHERE id=?1",
            params![
                duplicate.id,
                serde_json::to_string(&document::duplicate(source.document.clone()))
                    .map_err(err)?,
                source.last_model_id,
                serde_json::to_string(&source.session).map_err(err)?,
                source.favorite
            ],
        )
        .map_err(err)?;
        drop(conn);
        self.get_script(&duplicate.id)
    }
}
