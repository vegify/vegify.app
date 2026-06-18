//! Vegify desktop (local-first spike) — library crate. Holds the typed DAL (`data`) and the
//! TS bindings exporter. The Tauri runtime entry lives in main.rs so generating bindings
//! (`cargo run --bin gen-bindings`) doesn't pull in `tauri::generate_context!`.
pub mod data;

use std::path::PathBuf;

/// Embedded-replica config: (local replica path, sync URL of the sqld primary, auth token).
/// Dev defaults sync from the local docker sqld on :8080 with no token. A real build overrides
/// LIBSQL_SYNC_URL / LIBSQL_AUTH_TOKEN (the Fargate sqld) and LIBSQL_REPLICA_PATH (OS app-data dir).
pub fn db_config() -> (String, String, String) {
    let path = std::env::var("LIBSQL_REPLICA_PATH").unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.vegify-replica.db")
            .to_string_lossy()
            .into_owned()
    });
    let sync_url =
        std::env::var("LIBSQL_SYNC_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let auth_token = std::env::var("LIBSQL_AUTH_TOKEN").unwrap_or_default();
    (path, sync_url, auth_token)
}

/// Generate the typed TypeScript client from the DAL trait into src/bindings.ts.
pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
    ttipc::Bindings::new()
        .register::<data::VegifyDataProcedures>()
        .export_to(out)?;
    Ok(())
}
