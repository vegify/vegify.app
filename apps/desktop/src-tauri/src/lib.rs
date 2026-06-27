//! Vegify desktop (local-first spike) — library crate. Holds the typed DAL (`data`) and the
//! TS bindings exporter. The Tauri runtime entry lives in main.rs so generating bindings
//! (`cargo run --bin gen-bindings`) doesn't pull in `tauri::generate_context!`.
pub mod data;

use std::path::PathBuf;

/// Path to the on-device SQLite DB. `DATABASE_PATH` overrides. Debug builds use the repo's seeded
/// .data/vegify.db (sample content without a running server); release (shipped) builds use the
/// per-user OS app-data dir — `<data_dir>/app.vegify.desktop/vegify.db`, matching Tauri's
/// app_data_dir — created by `open_db` on first run, then filled from the server on sign-in.
pub fn db_path() -> String {
    if let Ok(p) = std::env::var("DATABASE_PATH") {
        return p;
    }
    #[cfg(debug_assertions)]
    {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.data/vegify.db")
            .to_string_lossy()
            .into_owned()
    }
    #[cfg(not(debug_assertions))]
    {
        dirs::data_dir()
            .expect("resolve OS data dir")
            .join("app.vegify.desktop")
            .join("vegify.db")
            .to_string_lossy()
            .into_owned()
    }
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
