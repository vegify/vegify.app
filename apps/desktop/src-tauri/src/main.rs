// Prevents an extra console window on Windows release builds, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_lib::data::*;

fn main() {
    let db = app_lib::open_db().expect("open vegify db");

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
