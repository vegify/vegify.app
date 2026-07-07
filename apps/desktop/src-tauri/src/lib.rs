//! Vegify native app (local-first) — library crate. Holds the typed DAL (`data`), the TS bindings
//! exporter, and `run()`, the Tauri entry every platform shell shares: the desktop binary calls it
//! from main.rs; on iOS (and Android later) `mobile_entry_point` exposes it as the static-lib entry
//! the generated Xcode project launches. Since `run()` moved in here for the mobile targets,
//! `tauri::generate_context!` compiles with the lib — so `cargo run --example gen_bindings` needs
//! `../dist` to exist first (any `pnpm --filter desktop build` provides it; `just check` orders that
//! automatically).
pub mod data;

use std::path::PathBuf;

/// Path to the on-device SQLite DB — created by `open_db` on first run, then filled from the server
/// on sign-in. Resolution (`DATABASE_PATH` override → debug: the repo's seeded .data/vegify.db →
/// release: the per-user OS app-data dir) lives in vegify-config.
pub fn db_path() -> String {
    vegify_config::desktop::db_path()
}

/// Open the on-device DB (SQLite at `db_path`): the local content cache + the `_outbox` push queue.
/// Sync runs over the content API now (the server is the source of truth), so there's no blob store
/// to wire — `sign_in` bootstraps a pull and writes auto-sync.
pub fn open_db() -> Result<data::Db, data::DataError> {
    let path = db_path();
    // Create the parent dir if absent (the app-data dir on a fresh install). Without it SQLite can't
    // create the file and the app panics at startup — exactly the v0.2.x "can't be opened" failure.
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| data::DataError::Db(format!("create db dir {}: {e}", parent.display())))?;
    }
    data::Db::open(&path)
}

/// Generate the typed TypeScript client from the DAL trait into src/bindings.ts.
pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
    ttipc::Bindings::new()
        .register::<data::VegifyDataProcedures>()
        .export_to(out)?;
    Ok(())
}

// The desktop polices its own bindings (ttipc's documented consumer pattern): the plain test is the
// drift guard (`Bindings::check`), and `just bindings` reruns it with VEGIFY_REGEN_BINDINGS=1 to
// regenerate in place. In-crate on purpose — the desktop is an APPLICATION, a leaf; no tools crate
// may depend on it to reach these types.


/// The Tauri app, platform-independent: every shell (desktop binary, iOS static lib) runs this.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use crate::data::*;

    // Mobile stand-in for the fltsci tracing plugin (which can't compile for iOS/Android — see
    // Cargo.toml): a plain fmt subscriber so the sync runtime's info!/debug! reach the simulator/
    // device console. No JS console bridge on mobile until upstream ships iOS support.
    #[cfg(mobile)]
    tracing_subscriber::fmt()
        .with_max_level(if cfg!(debug_assertions) {
            tracing::level_filters::LevelFilter::DEBUG
        } else {
            tracing::level_filters::LevelFilter::INFO
        })
        .init();

    let db = open_db().expect("open vegify db");

    let builder = tauri::Builder::default();

    // Structured tracing (fltsci tauri-plugin-tracing): installs the subscriber so the sync
    // runtime's info!/debug! surface, and backs the JS console bridge. DEBUG in dev, INFO in release.
    #[cfg(desktop)]
    let builder = {
        use tauri_plugin_tracing::{Builder as TracingBuilder, LevelFilter};
        builder.plugin(
            TracingBuilder::new()
                .with_max_level(if cfg!(debug_assertions) { LevelFilter::DEBUG } else { LevelFilter::INFO })
                .with_default_subscriber()
                .build(),
        )
    };

    builder
        // OS deep links: vegify:// (scheme from tauri.conf.json → CFBundleURLTypes at bundle time) and
        // vegify.app universal links (applinks entitlement + AASA). The frontend consumes opened URLs
        // via onOpenUrl/getCurrent (App.tsx). Needs a built .app bundle — inert under `tauri dev`.
        .plugin(tauri_plugin_deep_link::init())
        // Native notifications: the frontend fires an OS toast when a bell event arrives over the WS
        // push and the window isn't focused (App.tsx listens for `server-notification`).
        .plugin(tauri_plugin_notification::init())
        // Realtime push: a background WebSocket to the server's /ws emits `server-content-changed` on
        // every server-side content change so the frontend pulls immediately (replacing the 60s poll).
        // Runs on its own current-thread tokio runtime so it never depends on Tauri's runtime having the
        // timer/IO drivers enabled, and stays isolated from the sync ttipc/ureq request path.
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(rt) => rt.block_on(crate::data::run_ws_push(handle)),
                    Err(e) => tracing::error!(error = %e, "ws push runtime failed to start"),
                }
            });

            // Menu-bar tray icon (desktop only — mobile has no menu bar): a persistent presence with
            // quick access to the window. The menu is handled here in Rust (no JS capability needed);
            // on macOS the glyph is a template image, so it adapts to the light/dark menu bar
            // automatically.
            #[cfg(desktop)]
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::TrayIconBuilder;
                use tauri::Manager;

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
            }

            Ok(())
        })
        .invoke_handler(ttipc::handler(db.into_procedures()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod bindings_tests {
    #[test]
    fn bindings_ts_is_current() {
        if std::env::var("VEGIFY_REGEN_BINDINGS").is_ok() {
            crate::export_bindings().expect("regenerate bindings.ts");
            println!("wrote apps/desktop/src/bindings.ts");
            return;
        }
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
        ttipc::Bindings::new()
            .register::<crate::data::VegifyDataProcedures>()
            .check(path)
            .expect("bindings.ts drifted from the DAL trait — run `just bindings` and commit it");
    }
}
