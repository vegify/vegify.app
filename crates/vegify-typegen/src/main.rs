//! vegify-typegen — regenerate every committed artifact derived from the server's TypeCollection
//! (services/api/types): `packages/api-types/index.ts` (the web's wire contract) and
//! `crates/vegify-client-rs/src/generated.rs` (the SDK's wire types, via specta-rust). Run via
//! `just bindings`; CI's build-types job reruns it and fails on drift.
//!
//! The desktop's bindings.ts deliberately does NOT come from here: the desktop is an APPLICATION —
//! a leaf, never a dependency — so it regenerates/drift-checks its own bindings in-crate via
//! ttipc's documented test pattern (see apps/desktop/src-tauri/src/lib.rs).
use std::path::PathBuf;

fn main() {
    write_web_ts();
    write_sdk_rust();
    write_openapi();
}

/// The contract's honest i64 ms-epochs/counts as `number` for the JS-facing emission — the
/// wire-sound opt-out of specta's BigInt guard (serde serializes them as JSON numbers; every
/// live value sits far below 2^53). The desktop's ttipc bindings apply the same remap in-crate.
fn js_remap(types: specta::Types) -> specta::Types {
    specta_util::Remapper::new()
        .dangerous_bigints_as_number()
        .remap_types(types)
}

/// `packages/api-types/index.ts` — the web's wire contract, from the FULL collection.
fn write_web_ts() {
    let body = specta_typescript::Typescript::default()
        .export(
            &js_remap(vegify_api_types::api_types()),
            specta_serde::Format,
        )
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

/// The slice of the contract the SDK speaks over HTTP: the /api/content/pull payload and the DM
/// surface. Auth stays hand-written in the SDK (its response is remapped client-side and isn't
/// in the collection), and so does the bell's `Notification` — its `payload: serde_json::Value`
/// hits specta-rust's anonymous-inline-enum limit (`serde_json::Value.Number.0`; the exporter
/// errors loudly, as designed) until upstream can render `Value` as an opaque. Referenced types
/// (Party, the shared Visibility enum) ride in transitively; the explicit registrations are the
/// readable manifest.
fn sdk_types() -> specta::Types {
    specta::Types::default()
        .register::<vegify_api_types::PullPayload>()
        .register::<vegify_api_types::PullUser>()
        .register::<vegify_api_types::PullRecipe>()
        .register::<vegify_api_types::PullItem>()
        .register::<vegify_api_types::PullIngredient>()
        .register::<vegify_api_types::PullReading>()
        .register::<vegify_api_types::Party>()
        .register::<vegify_api_types::ConversationSummary>()
        .register::<vegify_api_types::Message>()
        .register::<vegify_api_types::Thread>()
}

/// `crates/vegify-client-rs/src/generated.rs` — the SDK's wire types, regenerated as Rust source
/// from the same graph the TS artifacts come from. Field idents ARE the wire names (specta's
/// graph is post-serde), so the plain serde derives reproduce the exact wire with no rename
/// attributes to drift.
fn write_sdk_rust() {
    // The mod declaration in lib.rs carries #[rustfmt::skip], keeping the committed file
    // byte-identical to this emission — otherwise the fmt gate (which would reformat it) and
    // the drift gate (which regenerates it verbatim) would each fail the other's output.
    let header = "\
//! The SDK's wire types — GENERATED from the server's wire contract (vegify-api-types, via
//! specta-rust). Do not edit; regenerate with `just bindings` (CI's build-types job fails on
//! drift). Field idents are the wire names verbatim, so the serde derives carry no renames.
";
    let body = specta_rust::Rust::default()
        .header(header)
        .derive("serde::Serialize")
        .derive("serde::Deserialize")
        .derive("Clone")
        .attribute("#[cfg_attr(feature = \"specta\", derive(specta::Type))]")
        .export(&sdk_types(), specta_serde::Format)
        .expect("export sdk types");
    let out =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vegify-client-rs/src/generated.rs");
    std::fs::write(&out, body).expect("write sdk generated types");
    println!("wrote crates/vegify-client-rs/src/generated.rs");
}

/// `packages/api-types/openapi.json` — the OpenAPI document: schemas from the full collection,
/// paths from the contract crate's route table (`api_operations`). The exporter targets OpenAPI
/// 3.1, whose schema dialect expresses the contract's nullable named responses
/// (`Option<RecipeView>` et al) natively, so no compatibility mode is needed.
fn write_openapi() {
    let doc = specta_openapi::OpenApi::default()
        .title("vegify.app HTTP API")
        .version("0.1.0")
        .description(
            "The typed surface of api.vegify.app: the public read endpoints, the content pull, \
             messages, notifications, and the session probe. Auth flows and write acks still \
             answer ad-hoc JSON and join this document as their shapes graduate into \
             vegify-api-types. Endpoints marked 'Requires bearer' expect an Authorization: \
             Bearer <token> header.",
        )
        .operations(vegify_api_types::api_operations())
        .export(&vegify_api_types::api_types(), specta_serde::Format)
        .expect("export openapi document");
    let out =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/api-types/openapi.json");
    std::fs::write(&out, doc).expect("write openapi.json");
    println!("wrote packages/api-types/openapi.json");
}
