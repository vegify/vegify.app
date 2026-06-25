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
        .invoke_handler(ttipc::handler(db.into_procedures()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
