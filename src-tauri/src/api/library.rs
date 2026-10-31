use super::*;

#[taurpc::procedures(path = "library")]
pub trait Library {
    async fn collections<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<Collection>>;
    async fn create_collection<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        name: String,
    ) -> ApiResult<Collection>;
    async fn rename_collection<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
        name: String,
    ) -> ApiResult<()>;
    async fn delete_collection<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()>;
    async fn scripts<R: tauri::Runtime>(
        app_handle: tauri::AppHandle<R>,
        query: LibraryQuery,
    ) -> ApiResult<Vec<ScriptSummary>>;
}

#[derive(Clone)]
pub struct LibraryImpl;

#[taurpc::resolvers]
impl Library for LibraryImpl {
    async fn collections<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
    ) -> ApiResult<Vec<Collection>> {
        let state = app_handle.state::<App>();
        state.db.call(|s| s.collections(false).map_err(error)).await
    }
    async fn create_collection<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        name: String,
    ) -> ApiResult<Collection> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state
            .db
            .call(move |s| s.create_collection(&name).map_err(error))
            .await
    }
    async fn rename_collection<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
        name: String,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state
            .db
            .call(move |s| s.rename_collection(&id, &name))
            .await
    }
    async fn delete_collection<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        id: String,
    ) -> ApiResult<()> {
        let state = app_handle.state::<App>();
        state.require_entry()?;
        state.db.call(move |s| s.delete_collection(&id)).await
    }
    async fn scripts<R: tauri::Runtime>(
        self,
        app_handle: tauri::AppHandle<R>,
        query: LibraryQuery,
    ) -> ApiResult<Vec<ScriptSummary>> {
        let state = app_handle.state::<App>();
        state
            .db
            .call(move |s| {
                s.list_scripts(
                    query.collection_id.as_deref(),
                    &query.query,
                    query.favorite,
                    query.trash,
                )
                .map_err(error)
            })
            .await
    }
}
