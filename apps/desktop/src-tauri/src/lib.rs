//! Vegify desktop (local-first spike) — library crate. Holds the typed DAL (`data`) and the
//! TS bindings exporter. The Tauri runtime entry lives in main.rs so generating bindings
//! (`cargo run --bin gen-bindings`) doesn't pull in `tauri::generate_context!`.
pub mod data;

use std::path::PathBuf;

/// Path to the on-device SQLite DB. Override with DATABASE_PATH; dev default = the repo's
/// seeded .data/vegify.db (relative to this crate). A real desktop build would seed into the
/// OS app-data dir on first run instead.
pub fn db_path() -> String {
    std::env::var("DATABASE_PATH").unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.data/vegify.db")
            .to_string_lossy()
            .into_owned()
    })
}

/// Open the on-device DB (SQLite at `db_path`): the local content cache + the `_outbox` push queue.
/// Sync runs over the content API now (the server is the source of truth), so there's no blob store
/// to wire — `sign_in` bootstraps a pull and writes auto-sync.
pub fn open_db() -> Result<data::Db, data::DataError> {
    data::Db::open(&db_path())
}

/// Generate the typed TypeScript client from the DAL trait into src/bindings.ts.
pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
    ttipc::Bindings::new()
        .register::<data::VegifyDataProcedures>()
        .export_to(out)?;
    Ok(())
}
