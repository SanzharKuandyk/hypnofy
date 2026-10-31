use super::*;

#[taurpc::procedures(path = "entry")]
pub trait Entry {
    async fn status<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> EntryStatus;
    async fn accept<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        notice_version: String,
        acknowledged: bool,
    ) -> ApiResult<EntryStatus>;
    async fn exit<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>);
}

#[derive(Clone)]
pub struct EntryImpl;

#[taurpc::resolvers]
impl Entry for EntryImpl {
    async fn status<R: tauri::Runtime>(self, app_handle: tauri::AppHandle<R>) -> EntryStatus {
        let state = app_handle.state::<App>();
        EntryStatus {
            accepted: state.accepted.load(Ordering::SeqCst),
            notice_version: "2026-11-01".into(),
        }
    }
    async fn accept<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        notice_version: String,
        acknowledged: bool,
    ) -> ApiResult<EntryStatus> {
        let state = app_handle.state::<App>();
        if !acknowledged || notice_version != "2026-11-01" {
            return Err("Read and acknowledge the current notice".into());
        }
        state.accepted.store(true, Ordering::SeqCst);
        Ok(EntryStatus {
            accepted: true,
            notice_version,
        })
    }
    async fn exit<R: tauri::Runtime>(self, app_handle: tauri::AppHandle<R>) {
        app_handle.exit(0);
    }
}
