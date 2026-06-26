// Prevents an extra console window on Windows release builds, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_lib::data::*;
use tauri_plugin_tracing::{Builder as TracingBuilder, LevelFilter};

fn main() {
    let db = app_lib::open_db().expect("open vegify db");

    tauri::Builder::default()
        // Structured tracing (fltsci tauri-plugin-tracing): installs the subscriber so the sync
        // runtime's info!/debug! surface, and backs the JS console bridge. DEBUG in dev, INFO in release.
        .plugin(
            TracingBuilder::new()
                .with_max_level(if cfg!(debug_assertions) { LevelFilter::DEBUG } else { LevelFilter::INFO })
                .with_default_subscriber()
                .build(),
        )
        // Realtime push: a background WebSocket to the server's /ws emits `server-content-changed` on
        // every server-side content change so the frontend pulls immediately (replacing the 60s poll).
        // Runs on its own current-thread tokio runtime so it never depends on Tauri's runtime having the
        // timer/IO drivers enabled, and stays isolated from the sync ttipc/ureq request path.
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(rt) => rt.block_on(app_lib::data::run_ws_push(handle)),
                    Err(e) => tracing::error!(error = %e, "ws push runtime failed to start"),
                }
            });
            Ok(())
        })
        .invoke_handler(ttipc::handler(db.into_procedures()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
