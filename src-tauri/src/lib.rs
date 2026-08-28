mod cache;
mod commands;
mod engine;
#[cfg(windows)]
mod ocr;
mod pdfx;
mod state;
mod tree;
mod v2p;
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
            commands::cancel_search,
            commands::get_ocr_words,
            tree::list_tree_dir,
            v2p::commands::v2p_check_env,
            v2p::commands::v2p_check_update,
            v2p::commands::v2p_download_model,
            v2p::commands::v2p_get_organizer_config,
            v2p::commands::v2p_set_organizer_config,
            v2p::commands::v2p_list_organizer_models,
            v2p::commands::v2p_transcribe,
            v2p::commands::v2p_generate_pdf
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
