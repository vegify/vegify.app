// Generates apps/web-tauri/src/bindings.ts from the DAL trait (run: pnpm --filter web-tauri gen:bindings).
fn main() {
    app_lib::export_bindings().expect("failed to export TS bindings");
    println!("wrote apps/web-tauri/src/bindings.ts");
}
