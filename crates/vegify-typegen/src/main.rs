//! vegify-typegen — regenerate packages/api-types/index.ts (the web's wire contract) from the
//! server's TypeCollection (services/api/types). Run via `just bindings`; CI's build-types job
//! reruns it and fails on drift.
//!
//! The desktop's bindings.ts deliberately does NOT come from here: the desktop is an APPLICATION —
//! a leaf, never a dependency — so it regenerates/drift-checks its own bindings in-crate via
//! ttipc's documented test pattern (see apps/desktop/src-tauri/src/lib.rs).
use std::path::PathBuf;

fn main() {
    let body = specta_typescript::Typescript::default()
        .export(&vegify_api_types::api_types(), specta_serde::Format)
        .expect("export api types");
    let header = "\
// @vegify/api-types — GENERATED from the server's wire shapes (vegify-core + services/api handler
// responses; the api TypeCollection in services/api/types). Do not edit; regenerate with
// `just bindings` (CI's build-types job fails on drift).
";
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/api-types/index.ts");
    std::fs::write(&out, format!("{header}\n{body}")).expect("write api-types");
    println!("wrote packages/api-types/index.ts");
}
