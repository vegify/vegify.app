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

/// Generate the typed TypeScript client from the DAL trait into src/bindings.ts.
pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
    ttipc::Bindings::new()
        .register::<data::VegifyDataProcedures>()
        .export_to(out)?;
    Ok(())
}
