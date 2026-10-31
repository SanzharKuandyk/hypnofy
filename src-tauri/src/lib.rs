pub mod api;
mod application;
mod bootstrap;
mod document;
pub mod domain;
mod infrastructure;
mod storage;

use tauri::Manager;

pub fn run() {
    // TauRPC starts namespace dispatchers while constructing the router.
    let runtime = tauri::async_runtime::handle();
    let _runtime_guard = runtime.inner().enter();
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let test_root = if cfg!(debug_assertions) {
                std::env::var_os("HYPNOFY_TEST_DATA_DIR").map(std::path::PathBuf::from)
            } else {
                None
            };
            if test_root.is_none()
                && let Ok(store) = windows_native_keyring_store::Store::new()
            {
                keyring_core::set_default_store(store);
            }
            let root = match test_root {
                Some(root) => root,
                None => app.path().app_data_dir()?,
            };
            let state = bootstrap::App::open(root)?;
            if !state.test_mode
                && let Some(window) = app.get_webview_window("main")
            {
                window.show()?;
            }
            let jobs = state.jobs.clone();
            let database = state.db.clone();
            app.manage(state);
            let trigger = api::TauRpcJobsEventTrigger::new(app.handle().clone());
            tauri::async_runtime::spawn(async move {
                let mut sequences = std::collections::HashMap::new();
                loop {
                    if let Ok(snapshots) = jobs.list() {
                        for snapshot in snapshots {
                            if sequences.get(&snapshot.id) != Some(&snapshot.sequence) {
                                sequences.insert(snapshot.id.clone(), snapshot.sequence);
                                let stored = snapshot.clone();
                                if database.call(move |s| s.job_save(&stored)).await.is_err() {
                                    eprintln!("Job progress could not be persisted");
                                }
                                let _ = trigger.changed(snapshot);
                            }
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                }
            });
            Ok(())
        })
        .invoke_handler(api::router().into_handler())
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("Hypnofy could not start: {error}");
    }
}
