// Alterix entry point: starts the desktop app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    alterix_lib::run()
}
