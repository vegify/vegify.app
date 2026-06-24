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

/// Changeset blob store dir — the S3 stand-in for dev. Override with SYNC_BLOB_DIR; prod points
/// this at an S3 bucket (via an on-demand Lambda or the AWS SDK), scale-to-zero.
pub fn blob_dir() -> String {
    std::env::var("SYNC_BLOB_DIR").unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.sync-blobs")
            .to_string_lossy()
            .into_owned()
    })
}

/// Open the on-device DB with the production S3 blob store when `SYNC_S3_BUCKET` is set
/// (scale-to-zero changeset transport, AWS-native); otherwise the local-dir store (the offline
/// default). `SYNC_S3_ENDPOINT` empty ⇒ real AWS (region-based); set ⇒ S3-compatible (e.g. MinIO).
/// Credentials come from `SYNC_S3_ACCESS_KEY`/`SYNC_S3_SECRET_KEY`, falling back to the standard
/// `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`.
pub fn open_db() -> Result<data::Db, data::DataError> {
    let db = db_path();
    match std::env::var("SYNC_S3_BUCKET") {
        Ok(bucket) if !bucket.is_empty() => {
            let region = std::env::var("SYNC_S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
            let endpoint = std::env::var("SYNC_S3_ENDPOINT").unwrap_or_default();
            let access = std::env::var("SYNC_S3_ACCESS_KEY")
                .or_else(|_| std::env::var("AWS_ACCESS_KEY_ID"))
                .unwrap_or_default();
            let secret = std::env::var("SYNC_S3_SECRET_KEY")
                .or_else(|_| std::env::var("AWS_SECRET_ACCESS_KEY"))
                .unwrap_or_default();
            let store = data::S3BlobStore::new(&bucket, &region, &endpoint, &access, &secret)?;
            data::Db::open_with(&db, Box::new(store))
        }
        _ => data::Db::open(&db, &blob_dir()),
    }
}

/// Generate the typed TypeScript client from the DAL trait into src/bindings.ts.
pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
    ttipc::Bindings::new()
        .register::<data::VegifyDataProcedures>()
        .export_to(out)?;
    Ok(())
}
