// Generates BOTH generated-types artifacts from the Rust source of truth (run:
// `pnpm --filter desktop gen:bindings` / `just bindings`):
//   1. apps/desktop/src/bindings.ts — the desktop's typed IPC client (ttipc, from the DAL trait).
//   2. packages/api-types/index.ts — the WEB's wire types (@vegify/api-types): vegify-core's serde
//      shapes exported types-only, so apps/web/src/content.ts imports the contract instead of
//      hand-mirroring it. Same specta pin, same serde-aware formatting as ttipc's own emission.
use std::path::PathBuf;

fn main() {
    vegify_lib::export_bindings().expect("failed to export TS bindings");
    println!("wrote apps/desktop/src/bindings.ts");
    export_api_types().expect("failed to export @vegify/api-types");
    println!("wrote packages/api-types/index.ts");
}

fn export_api_types() -> Result<(), Box<dyn std::error::Error>> {
    use vegify_core as vc;
    // Every wire shape the web consumes over /api/content/* — the exact set content.ts used to
    // hand-mirror. Referenced types (Reading, Amount, …) ride in automatically via specta.
    let types = specta::Types::default()
        .register::<vc::Visibility>()
        .register::<vc::Reading>()
        .register::<vc::Amount>()
        .register::<vc::AggregatedNutrition>()
        .register::<vc::RecipeCard>()
        .register::<vc::Profile>()
        .register::<vc::RecipeItem>()
        .register::<vc::RecipeView>()
        .register::<vc::RecipeEditItem>()
        .register::<vc::RecipeEditData>()
        .register::<vc::IngredientCard>()
        .register::<vc::IngredientSearchResult>()
        .register::<vc::IngredientEditData>();
    let body = specta_typescript::Typescript::default().export(&types, specta_serde::Format)?;
    let header = "\
// @vegify/api-types — GENERATED from vegify-core's wire shapes (the server's serde source of
// truth). Do not edit; regenerate with `just bindings` (CI's build-types job fails on drift).
";
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packages/api-types/index.ts");
    std::fs::create_dir_all(out.parent().unwrap())?;
    std::fs::write(out, format!("{header}\n{body}"))?;
    Ok(())
}
