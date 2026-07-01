// Prevents an extra console window on Windows release builds, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use vegify_lib::data::*;
use tauri_plugin_tracing::{Builder as TracingBuilder, LevelFilter};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

fn main() {
    let db = vegify_lib::open_db().expect("open vegify db");

    tauri::Builder::default()
        // Structured tracing (fltsci tauri-plugin-tracing): installs the subscriber so the sync
        // runtime's info!/debug! surface, and backs the JS console bridge. DEBUG in dev, INFO in release.
        .plugin(
            TracingBuilder::new()
                .with_max_level(if cfg!(debug_assertions) { LevelFilter::DEBUG } else { LevelFilter::INFO })
                .with_default_subscriber()
                .build(),
        )
        // OS deep links: vegify:// (scheme from tauri.conf.json → CFBundleURLTypes at bundle time) and
        // vegify.app universal links (applinks entitlement + AASA). The frontend consumes opened URLs
        // via onOpenUrl/getCurrent (App.tsx). Needs a built .app bundle — inert under `tauri dev`.
        .plugin(tauri_plugin_deep_link::init())
        // Realtime push: a background WebSocket to the server's /ws emits `server-content-changed` on
        // every server-side content change so the frontend pulls immediately (replacing the 60s poll).
        // Runs on its own current-thread tokio runtime so it never depends on Tauri's runtime having the
        // timer/IO drivers enabled, and stays isolated from the sync ttipc/ureq request path.
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(rt) => rt.block_on(vegify_lib::data::run_ws_push(handle)),
                    Err(e) => tracing::error!(error = %e, "ws push runtime failed to start"),
                }
            });

            // Menu-bar tray icon: a persistent presence with quick access to the window. The menu is
            // handled here in Rust (no JS capability needed); on macOS the glyph is a template image,
            // so it adapts to the light/dark menu bar automatically.
            let show = MenuItem::with_id(app, "show", "Show Vegify", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Vegify", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
            TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("Vegify")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(ttipc::handler(db.into_procedures()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
