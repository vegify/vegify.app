// Prevents an extra console window on Windows release builds, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    vegify_lib::run()
}
