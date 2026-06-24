// Generates apps/desktop/src/bindings.ts from the DAL trait (run: pnpm --filter desktop gen:bindings).
fn main() {
    app_lib::export_bindings().expect("failed to export TS bindings");
    println!("wrote apps/desktop/src/bindings.ts");
}
