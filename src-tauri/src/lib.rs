mod commands;
mod engine;
mod pdfx;
mod state;
mod walker;

use state::SearchState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(SearchState::new())
        .invoke_handler(tauri::generate_handler![
            commands::search_file,
            commands::start_search,
            commands::cancel_search
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
