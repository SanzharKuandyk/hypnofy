use super::*;
use crate::domain::*;

pub(super) fn migrate_sessions(tx: &rusqlite::Transaction<'_>) -> anyhow::Result<()> {
    for table in ["scripts", "script_revisions"] {
        let rows: Vec<(String, String)> = {
            let mut statement = tx.prepare(&format!("SELECT id,session_json FROM {table}"))?;
            statement
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        for (id, raw) in rows {
            let settings: SessionSettings = serde_json::from_str(&raw)?;
            tx.execute(
                &format!("UPDATE {table} SET session_json=?2 WHERE id=?1"),
                params![id, serde_json::to_string(&settings)?],
            )?;
        }
    }
    Ok(())
}

pub(super) fn migrate(tx: &rusqlite::Transaction<'_>) -> anyhow::Result<()> {
    tx.execute_batch(include_str!("../../migrations/0005_rewrite.sql"))?;
    for table in ["scripts", "script_revisions"] {
        let rows: Vec<(String, Option<String>)> = {
            let mut statement = tx.prepare(&format!("SELECT id,document_json FROM {table}"))?;
            statement
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        for (id, raw) in rows {
            if let Some(raw) = raw {
                let value: Value = serde_json::from_str(&raw)?;
                match document::upgrade(value) {
                    Ok(value) => {
                        tx.execute(
                            &format!("UPDATE {table} SET document_json=?2 WHERE id=?1"),
                            params![id, serde_json::to_string(&value)?],
                        )?;
                    }
                    Err(_) => {
                        tx.execute("INSERT INTO legacy_documents(source_table,source_id,original_json) VALUES(?1,?2,?3)",params![table,id,raw])?;
                    }
                }
            }
        }
    }
    let linked: Vec<(String, String, String)> = {
        let mut statement = tx.prepare("SELECT id,name,path FROM linked_models")?;
        statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?
    };
    for (id, name, path) in linked {
        let model = ModelRecord {
            id: id.clone(),
            name: name.clone(),
            path: Some(path),
            provider: "nobodywho".into(),
            capability: "text".into(),
            architecture: None,
            ownership: "linked".into(),
            status: "available".into(),
            reason: Some("Migrated model; verify with NobodyWho before use".into()),
            fingerprint: None,
            context_tokens: None,
            voices: Vec::new(),
        };
        tx.execute(
            "INSERT OR IGNORE INTO installed_models(id,name,record_json) VALUES(?1,?2,?3)",
            params![id, name, serde_json::to_string(&model)?],
        )?;
    }
    tx.execute("INSERT OR IGNORE INTO rewrite_settings(key,value) SELECT 'retired_preferences',value FROM settings WHERE key='app_preferences'",[])?;
    let raw: Option<String> = tx
        .query_row(
            "SELECT value FROM settings WHERE key='app_preferences'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(raw) = raw
        && let Ok(old) = serde_json::from_str::<Value>(&raw)
    {
        let mut preferences = Preferences::defaults();
        if let Some(theme) = old["theme"]
            .as_str()
            .filter(|v| ["dark", "light", "system"].contains(v))
        {
            preferences.theme = theme.into();
        }
        if let Some(minutes) = old["keepModelMinutes"].as_u64().filter(|v| *v <= 120) {
            preferences.idle_model_minutes = minutes as u32;
        }
        let provider = old["defaultProvider"].as_str().unwrap_or("");
        if provider == "gguf" {
            let id = old["defaultModel"]
                .as_str()
                .and_then(|v| v.strip_prefix("gguf:"));
            let path = old["ggufModelPath"].as_str().unwrap_or("");
            let candidate: Option<String> = tx
                .query_row(
                    "SELECT id FROM linked_models WHERE id=?1 OR path=?2 LIMIT 1",
                    params![id, path],
                    |r| r.get(0),
                )
                .optional()?;
            preferences.text_model_id = candidate;
        } else if provider == "openrouter"
            && let Some(remote) = old["defaultModel"].as_str().filter(|v| !v.is_empty())
        {
            let id = uuid::Uuid::new_v4().to_string();
            let remote = remote.strip_prefix("openrouter:").unwrap_or(remote);
            let model = ModelRecord {
                id: id.clone(),
                name: remote.into(),
                provider: "openrouter".into(),
                capability: "text".into(),
                architecture: None,
                path: Some(remote.into()),
                ownership: "remote".into(),
                status: "needs-auth".into(),
                reason: Some("Refresh OpenRouter models to verify the existing connection".into()),
                fingerprint: None,
                context_tokens: None,
                voices: Vec::new(),
            };
            tx.execute(
                "INSERT OR IGNORE INTO installed_models(id,name,record_json) VALUES(?1,?2,?3)",
                params![id, model.name, serde_json::to_string(&model)?],
            )?;
            preferences.text_model_id = Some(id);
        }
        tx.execute(
            "INSERT OR IGNORE INTO rewrite_settings(key,value) VALUES('preferences',?1)",
            [serde_json::to_string(&preferences)?],
        )?;
    }
    tx.execute("PRAGMA user_version=5", [])?;
    Ok(())
}

impl Store {
    pub fn mark_provider_unavailable(&self, provider: &str) -> ApiResult<()> {
        for mut model in self
            .model_records()?
            .into_iter()
            .filter(|m| m.provider == provider)
        {
            model.status = "needs-auth".into();
            model.reason = Some("Connect this provider before generating".into());
            self.model_save(&model)?;
        }
        Ok(())
    }
    pub fn staging_register(&self, id: &str, root: &str, path: &str) -> ApiResult<()> {
        self.conn()
            .map_err(AppError::from)?
            .execute(
                "INSERT INTO owned_staging(id,root,path) VALUES(?1,?2,?3)",
                params![id, root, path],
            )
            .map_err(|e| AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn staging_remove(&self, id: &str) -> ApiResult<()> {
        self.conn()
            .map_err(AppError::from)?
            .execute("DELETE FROM owned_staging WHERE id=?1", [id])
            .map_err(|e| AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn staging_records(&self) -> ApiResult<Vec<(String, String, String)>> {
        let conn = self.conn().map_err(AppError::from)?;
        let mut statement = conn
            .prepare("SELECT id,root,path FROM owned_staging")
            .map_err(|e| AppError::from(e.to_string()))?;
        statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| AppError::from(e.to_string()))?
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::from(e.to_string()))
    }
    pub fn model_usage(&self, id: &str) -> ApiResult<Vec<ScriptSummary>> {
        let conn = self.conn().map_err(AppError::from)?;
        let mut statement=conn.prepare("SELECT s.id,s.collection_id,c.name,s.title,s.favorite,s.updated_at,s.revision FROM scripts s JOIN collections c ON c.id=s.collection_id WHERE s.last_model_id=?1 OR s.session_json LIKE ?2 OR s.document_json LIKE ?2 ORDER BY s.title COLLATE NOCASE").map_err(|e|AppError::from(e.to_string()))?;
        statement
            .query_map(params![id, format!("%{id}/%")], |r| {
                Ok(ScriptSummary {
                    id: r.get(0)?,
                    collection_id: r.get(1)?,
                    collection_name: r.get(2)?,
                    title: r.get(3)?,
                    favorite: r.get(4)?,
                    updated_at: r.get(5)?,
                    revision: r.get(6)?,
                })
            })
            .map_err(|e| AppError::from(e.to_string()))?
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::from(e.to_string()))
    }
    pub fn job_save(&self, job: &JobSnapshot) -> ApiResult<()> {
        self.conn().map_err(AppError::from)?.execute("INSERT INTO job_history(id,record_json) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET record_json=excluded.record_json",params![job.id,serde_json::to_string(job).map_err(|e|AppError::from(e.to_string()))?]).map_err(|e|AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn recover_jobs(&self) -> ApiResult<Vec<JobSnapshot>> {
        let rows: Vec<String> = {
            let conn = self.conn().map_err(AppError::from)?;
            let mut stmt = conn
                .prepare("SELECT record_json FROM job_history")
                .map_err(|e| AppError::from(e.to_string()))?;
            stmt.query_map([], |r| r.get(0))
                .map_err(|e| AppError::from(e.to_string()))?
                .collect::<Result<_, _>>()
                .map_err(|e| AppError::from(e.to_string()))?
        };
        let mut output = Vec::new();
        for row in rows {
            let mut value: Value =
                serde_json::from_str(&row).map_err(|e| AppError::from(e.to_string()))?;
            if let Some(message) = value["error"].as_str().map(str::to_owned) {
                value["error"] = serde_json::to_value(AppError::from(message))
                    .map_err(|e| AppError::from(e.to_string()))?;
            }
            let old_result = value["resultId"].as_str().map(str::to_owned);
            let mut job: JobSnapshot =
                serde_json::from_value(value).map_err(|e| AppError::from(e.to_string()))?;
            if job.state == JobStatus::Completed && job.result.is_none() {
                job.result = JobResult::completed(job.kind, old_result);
            }
            if ["queued", "running", "cancelling"].contains(&job.state.as_str()) {
                job.state = crate::domain::JobStatus::Failed;
                job.phase = "Interrupted by restart".into();
                job.error=Some("The app stopped before this job completed. Your saved work is safe; start the operation again.".into());
                job.text.clear();
                job.sequence = job.sequence.saturating_add(1);
                self.job_save(&job)?;
            }
            output.push(job);
        }
        Ok(output)
    }
    pub fn legacy_document(&self, id: &str) -> ApiResult<Option<String>> {
        self.conn().map_err(AppError::from)?.query_row("SELECT original_json FROM legacy_documents WHERE source_table='scripts' AND source_id=?1",[id],|r|r.get(0)).optional().map_err(|e|AppError::from(e.to_string()))
    }
    pub fn preferences(&self) -> ApiResult<Preferences> {
        self.json_get("preferences")?
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|_| AppError::from("Saved preferences are invalid"))
            .map(|p| p.unwrap_or_else(Preferences::defaults))
    }

    pub fn json_get(&self, key: &str) -> ApiResult<Option<String>> {
        self.conn()
            .map_err(AppError::from)?
            .query_row(
                "SELECT value FROM rewrite_settings WHERE key=?1",
                [key],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::from(e.to_string()))
    }

    pub fn json_set(&self, key: &str, value: &str) -> ApiResult<()> {
        self.conn().map_err(AppError::from)?.execute("INSERT INTO rewrite_settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(|e|AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn model_records(&self) -> ApiResult<Vec<ModelRecord>> {
        let conn = self.conn().map_err(AppError::from)?;
        let mut statement = conn
            .prepare("SELECT record_json FROM installed_models ORDER BY name COLLATE NOCASE")
            .map_err(|e| AppError::from(e.to_string()))?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| AppError::from(e.to_string()))?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| AppError::from(e.to_string()))?)
                .map_err(|e| AppError::from(e.to_string()))
        })
        .collect()
    }

    pub fn model_save(&self, model: &ModelRecord) -> ApiResult<()> {
        self.conn().map_err(AppError::from)?.execute("INSERT INTO installed_models(id,name,record_json) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET name=excluded.name,record_json=excluded.record_json",params![model.id,model.name,serde_json::to_string(model).map_err(|e|AppError::from(e.to_string()))?]).map_err(|e|AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn model_remove(&self, id: &str) -> ApiResult<()> {
        self.conn()
            .map_err(AppError::from)?
            .execute("DELETE FROM installed_models WHERE id=?1", [id])
            .map_err(|e| AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn rename_collection(&self, id: &str, name: &str) -> ApiResult<()> {
        if id == "inbox" || name.trim().is_empty() || name.chars().count() > 80 {
            return Err(
                "Enter a collection name of 1–80 characters. My scripts cannot be renamed.".into(),
            );
        }
        self.conn()
            .map_err(AppError::from)?
            .execute(
                "UPDATE collections SET name=?2,updated_at=?3 WHERE id=?1",
                params![id, name.trim(), now()],
            )
            .map_err(|e| AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn delete_collection(&self, id: &str) -> ApiResult<()> {
        if id == "inbox" {
            return Err("My scripts cannot be deleted".into());
        }
        let mut conn = self.conn().map_err(AppError::from)?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::from(e.to_string()))?;
        tx.execute("UPDATE scripts SET collection_id='inbox',revision=revision+1,updated_at=?2 WHERE collection_id=?1",params![id,now()]).map_err(|e|AppError::from(e.to_string()))?;
        tx.execute("DELETE FROM collections WHERE id=?1", [id])
            .map_err(|e| AppError::from(e.to_string()))?;
        tx.commit().map_err(|e| AppError::from(e.to_string()))
    }

    pub fn bulk(&self, request: BulkRequest) -> ApiResult<()> {
        if request.ids.is_empty() || request.ids.len() > 5000 {
            return Err("Select between 1 and 5000 scripts".into());
        }
        let mut conn = self.conn().map_err(AppError::from)?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::from(e.to_string()))?;
        for id in request.ids {
            let changed=match request.operation.as_str() {
                "trash"=>tx.execute("UPDATE scripts SET deleted_at=?2,updated_at=?2,revision=revision+1 WHERE id=?1",params![id,now()]),
                "restore"=>tx.execute("UPDATE scripts SET deleted_at=NULL,updated_at=?2,revision=revision+1 WHERE id=?1",params![id,now()]),
                "favorite"=>tx.execute("UPDATE scripts SET favorite=1,updated_at=?2,revision=revision+1 WHERE id=?1",params![id,now()]),
                "unfavorite"=>tx.execute("UPDATE scripts SET favorite=0,updated_at=?2,revision=revision+1 WHERE id=?1",params![id,now()]),
                "move"=>tx.execute("UPDATE scripts SET collection_id=?2,updated_at=?3,revision=revision+1 WHERE id=?1",params![id,request.collection_id.as_deref().ok_or_else(||AppError::from("Choose a collection"))?,now()]),
                "delete"=> {
                    let deleted:bool=tx.query_row("SELECT deleted_at IS NOT NULL FROM scripts WHERE id=?1",[&id],|r|r.get(0)).map_err(|e|AppError::from(e.to_string()))?;
                    if !deleted {return Err("Only scripts in Trash can be permanently deleted".into());}
                    tx.execute("DELETE FROM renders WHERE script_id=?1",[&id]).map_err(|e|AppError::from(e.to_string()))?;
                    tx.execute("DELETE FROM render_records WHERE script_id=?1",[&id]).map_err(|e|AppError::from(e.to_string()))?;
                    tx.execute("DELETE FROM suggestions WHERE script_id=?1",[&id]).map_err(|e|AppError::from(e.to_string()))?;
                    tx.execute("DELETE FROM scripts WHERE id=?1",[&id])
                },
                _=>return Err("Unsupported bulk operation".into())
            }.map_err(|e|AppError::from(e.to_string()))?;
            if changed != 1 {
                return Err("A selected script no longer exists".into());
            }
        }
        tx.commit().map_err(|e| AppError::from(e.to_string()))
    }

    pub fn revisions(&self, id: &str) -> ApiResult<Vec<RevisionRecord>> {
        let conn = self.conn().map_err(AppError::from)?;
        let mut statement=conn.prepare("SELECT revision,title,saved_at FROM script_revisions WHERE script_id=?1 ORDER BY revision DESC").map_err(|e|AppError::from(e.to_string()))?;
        statement
            .query_map([id], |r| {
                Ok(RevisionRecord {
                    revision: r.get(0)?,
                    title: r.get(1)?,
                    saved_at: r.get(2)?,
                })
            })
            .map_err(|e| AppError::from(e.to_string()))?
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::from(e.to_string()))
    }

    pub fn restore_revision(&self, id: &str, revision: i32) -> ApiResult<Script> {
        let current = self.get_script(id).map_err(AppError::from)?;
        let (title,raw,session):(String,String,String)=self.conn().map_err(AppError::from)?.query_row("SELECT title,document_json,session_json FROM script_revisions WHERE script_id=?1 AND revision=?2",params![id,revision],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e|AppError::from(e.to_string()))?;
        self.update_script(ScriptPatch {
            id: id.into(),
            title,
            document: serde_json::from_str(&raw).map_err(|e| AppError::from(e.to_string()))?,
            session: serde_json::from_str(&session).map_err(|e| AppError::from(e.to_string()))?,
            expected_revision: current.revision,
            collection_id: current.collection_id,
            favorite: current.favorite,
            last_model_id: current.last_model_id,
        })
        .map_err(AppError::from)
    }

    pub fn suggestion_save(&self, value: &Suggestion) -> ApiResult<()> {
        self.conn()
            .map_err(AppError::from)?
            .execute(
                "INSERT OR REPLACE INTO suggestions(id,script_id,record_json) VALUES(?1,?2,?3)",
                params![
                    value.id,
                    value.script_id,
                    serde_json::to_string(value).map_err(|e| AppError::from(e.to_string()))?
                ],
            )
            .map_err(|e| AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn suggestions(&self, id: &str) -> ApiResult<Vec<Suggestion>> {
        let conn = self.conn().map_err(AppError::from)?;
        let mut stmt = conn
            .prepare("SELECT record_json FROM suggestions WHERE script_id=?1")
            .map_err(|e| AppError::from(e.to_string()))?;
        let rows = stmt
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|e| AppError::from(e.to_string()))?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| AppError::from(e.to_string()))?)
                .map_err(|e| AppError::from(e.to_string()))
        })
        .collect()
    }

    pub fn suggestion_discard(&self, id: &str) -> ApiResult<()> {
        self.conn()
            .map_err(AppError::from)?
            .execute("DELETE FROM suggestions WHERE id=?1", [id])
            .map_err(|e| AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn render_save(&self, value: &RenderRecord) -> ApiResult<()> {
        self.conn()
            .map_err(AppError::from)?
            .execute(
                "INSERT INTO render_records(id,script_id,record_json) VALUES(?1,?2,?3)",
                params![
                    value.id,
                    value.script_id,
                    serde_json::to_string(value).map_err(|e| AppError::from(e.to_string()))?
                ],
            )
            .map_err(|e| AppError::from(e.to_string()))?;
        Ok(())
    }

    pub fn render_records(&self, id: &str) -> ApiResult<Vec<RenderRecord>> {
        let conn = self.conn().map_err(AppError::from)?;
        let mut statement = conn
            .prepare(
                "SELECT record_json FROM render_records WHERE script_id=?1 ORDER BY rowid DESC",
            )
            .map_err(|e| AppError::from(e.to_string()))?;
        let rows = statement
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|e| AppError::from(e.to_string()))?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| AppError::from(e.to_string()))?)
                .map_err(|e| AppError::from(e.to_string()))
        })
        .collect()
    }
}

impl Store {
    pub fn retained_render_ids(&self) -> ApiResult<std::collections::HashSet<String>> {
        let conn = self.conn().map_err(|e| AppError::from(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT id FROM render_records WHERE json_extract(record_json,'$.preview')=0")
            .map_err(|e| AppError::from(e.to_string()))?;
        stmt.query_map([], |r| r.get(0))
            .map_err(|e| AppError::from(e.to_string()))?
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::from(e.to_string()))
    }

    pub fn remove_unplayed_previews(&self) -> ApiResult<()> {
        // Audio references are removed only after the files are released and deleted.
        let conn = self.conn().map_err(|e| AppError::from(e.to_string()))?;
        let mut statement = conn
            .prepare("SELECT id FROM render_records WHERE json_extract(record_json,'$.preview')=1")
            .map_err(|e| AppError::from(e.to_string()))?;
        let ids: Vec<String> = statement
            .query_map([], |r| r.get(0))
            .map_err(|e| AppError::from(e.to_string()))?
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::from(e.to_string()))?;
        for id in ids {
            if !self
                .data_dir
                .join("audio/previews")
                .join(format!("{id}.wav"))
                .exists()
            {
                conn.execute("DELETE FROM render_records WHERE id=?1", [id])
                    .map_err(|e| AppError::from(e.to_string()))?;
            }
        }
        Ok(())
    }
}
