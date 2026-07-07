//! The vegify desktop shell: tauri entry point wiring the webview to the
//! local-first data layer in `data.rs`.
// Prevents an extra console window on Windows release builds, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    vegify_lib::run()
}
