// Prevents an extra console window on Windows release builds, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_lib::data::*;

fn main() {
    let (path, sync_url, token) = app_lib::db_config();

    // Open the embedded replica and bootstrap it from the primary before the window loads, so the
    // first reads are local + instant (and keep working offline thereafter). libsql is async; the
    // Tauri runtime drives the open + initial sync.
    let db = tauri::async_runtime::block_on(async {
        let db = Db::open(&path, sync_url, token)
            .await
            .expect("open embedded replica");
        if let Err(e) = db.sync().await {
            eprintln!("initial sync failed (continuing with local replica state): {e}");
        }
        db
    });

    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(ttipc::handler(db.into_procedures()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
