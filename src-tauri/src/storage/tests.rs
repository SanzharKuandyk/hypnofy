use super::*;
use crate::domain::BulkRequest;
use serde_json::json;

fn patch(script: &Script, text: &str) -> ScriptPatch {
    ScriptPatch {
        id: script.id.clone(),
        title: script.title.clone(),
        document: document::from_plain_text(text),
        last_model_id: None,
        collection_id: script.collection_id.clone(),
        favorite: script.favorite,
        expected_revision: script.revision,
        session: script.session.clone(),
    }
}

#[test]
fn optimistic_revisions_duplicate_trash_and_atomic_bulk() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().into()).unwrap();
    let collection = store.create_collection("Evening").unwrap();
    let original = store
        .create_script(Some(&collection.id), Some("Rest"), "Hello 🌙")
        .unwrap();
    let saved = store
        .update_script(patch(&original, "Breathe gently. [pause:2s]"))
        .unwrap();
    assert_eq!(saved.revision, 2);
    let mut unchanged = patch(&saved, "");
    unchanged.document = saved.document.clone();
    assert_eq!(store.update_script(unchanged).unwrap().revision, 2);
    assert!(store.update_script(patch(&original, "Stale edit")).is_err());
    assert_eq!(store.get_script(&saved.id).unwrap().body, saved.body);
    let copy = store.duplicate_script(&saved.id).unwrap();
    assert_ne!(
        copy.document["content"][0]["attrs"]["id"],
        saved.document["content"][0]["attrs"]["id"]
    );
    assert!(
        store
            .bulk(BulkRequest {
                ids: vec![saved.id.clone(), "missing".into()],
                operation: "trash".into(),
                collection_id: None
            })
            .is_err()
    );
    assert!(store.get_script(&saved.id).unwrap().deleted_at.is_none());
    store.delete_collection(&collection.id).unwrap();
    let moved = store.get_script(&saved.id).unwrap();
    assert_eq!(moved.collection_id, "inbox");
    assert_eq!(moved.revision, 3);
    store
        .bulk(BulkRequest {
            ids: vec![saved.id.clone()],
            operation: "trash".into(),
            collection_id: None,
        })
        .unwrap();
    assert!(!store.revisions(&saved.id).unwrap().is_empty());
    store
        .bulk(BulkRequest {
            ids: vec![saved.id.clone()],
            operation: "restore".into(),
            collection_id: None,
        })
        .unwrap();
    assert!(store.get_script(&saved.id).unwrap().deleted_at.is_none());
    assert!(
        store
            .bulk(BulkRequest {
                ids: vec![saved.id.clone()],
                operation: "delete".into(),
                collection_id: None
            })
            .is_err()
    );
    let restored = store.restore_revision(&saved.id, 1).unwrap();
    assert_eq!(restored.body, "Hello 🌙");
    assert_eq!(restored.revision, 6);
}

#[test]
fn all_legacy_versions_preserve_content_and_create_consistent_backups() {
    for version in 1..=4 {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let script = store
            .create_script(None, Some("Legacy 🌙"), "Keep this text.")
            .unwrap();
        let id = script.id.clone();
        let old = json!({"version":1,"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Keep this text."}]}]});
        let conn = store.conn().unwrap();
        conn.execute(
            "UPDATE scripts SET favorite=1,document_json=?2 WHERE id=?1",
            params![id, old.to_string()],
        )
        .unwrap();
        if version < 2 {
            conn.execute_batch("ALTER TABLE scripts DROP COLUMN document_json;ALTER TABLE scripts DROP COLUMN last_model_id;ALTER TABLE script_revisions DROP COLUMN document_json;").unwrap();
        }
        if version < 3 {
            conn.execute_batch("DROP TABLE linked_models;").unwrap();
        } else {
            conn.execute("INSERT INTO linked_models(id,name,path,created_at,updated_at) VALUES('old-model','Local','C:/Synthetic/model.gguf','old','old')",[]).unwrap();
            if version < 4 {
                conn.execute_batch("ALTER TABLE linked_models DROP COLUMN verified_bytes;ALTER TABLE linked_models DROP COLUMN verified_mtime_ns;ALTER TABLE linked_models DROP COLUMN verified_runtime;").unwrap();
            }
        }
        conn.execute_batch(&format!("PRAGMA user_version={version}"))
            .unwrap();
        drop(conn);
        drop(store);
        let migrated = Store::open(dir.path().into()).unwrap();
        let script = migrated.get_script(&id).unwrap();
        assert_eq!(script.title, "Legacy 🌙");
        assert!(script.favorite);
        assert_eq!(script.document["version"], 2);
        assert_eq!(
            document::plain_text(&script.document).unwrap(),
            "Keep this text."
        );
        if version >= 3 {
            assert_eq!(migrated.model_records().unwrap()[0].status, "available");
        }
        let backup = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("hypnofy-before-")
            })
            .unwrap();
        let backup = Connection::open(backup.path()).unwrap();
        let count: i64 = backup
            .query_row("SELECT COUNT(*) FROM scripts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}

#[test]
fn unsupported_documents_survive_and_failed_migrations_roll_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().into()).unwrap();
    let script = store.create_script(None, None, "Original").unwrap();
    let original =
        json!({"version":1,"type":"doc","content":[{"type":"mystery","text":"Do not lose me"}]})
            .to_string();
    store
        .conn()
        .unwrap()
        .execute(
            "UPDATE scripts SET document_json=?2 WHERE id=?1",
            params![script.id, original],
        )
        .unwrap();
    store
        .conn()
        .unwrap()
        .execute_batch("PRAGMA user_version=4")
        .unwrap();
    drop(store);
    let migrated = Store::open(dir.path().into()).unwrap();
    assert_eq!(
        migrated.legacy_document(&script.id).unwrap(),
        Some(original)
    );
    migrated
        .conn()
        .unwrap()
        .execute(
            "UPDATE scripts SET document_json='invalid' WHERE id=?1",
            [&script.id],
        )
        .unwrap();
    migrated
        .conn()
        .unwrap()
        .execute_batch("PRAGMA user_version=4")
        .unwrap();
    drop(migrated);
    assert!(Store::open(dir.path().into()).is_err());
    let conn = Connection::open(dir.path().join("hypnofy.sqlite3")).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    conn.execute_batch("PRAGMA user_version=99").unwrap();
    drop(conn);
    assert!(Store::open(dir.path().into()).is_err());
}
