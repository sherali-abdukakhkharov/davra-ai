// Release builds are GUI apps on Windows: no console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::login,
            commands::logout,
            commands::install_cli,
            commands::plan_connect,
            commands::apply_connect,
            commands::plan_disconnect,
            commands::apply_disconnect,
            commands::plan_repo,
            commands::apply_repo,
            commands::forget_repo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Davra");
}
