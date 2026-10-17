pub mod capture_provider;
pub mod csdm;
pub mod edit_handoff;
pub mod import;
pub mod import_queue;
pub mod library;

use library::{LibrarySnapshot, MatchDetail};

#[tauri::command]
fn list_matches(path: String) -> Result<LibrarySnapshot, String> {
    library::list_matches(&path).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_match_detail(path: String, match_id: i64) -> Result<MatchDetail, String> {
    library::get_match_detail(&path, match_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_viewer_round(
    path: String,
    match_id: i64,
    round_number: i64,
) -> Result<library::ViewerRoundMeta, String> {
    library::get_viewer_round(&path, match_id, round_number).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_viewer_frames(
    path: String,
    match_id: i64,
    round_number: i64,
    start_tick: i64,
    end_tick: i64,
) -> Result<library::ViewerFrameChunk, String> {
    library::get_viewer_frames(&path, match_id, round_number, start_tick, end_tick)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_utility_finder(path: String, match_id: i64) -> Result<Vec<library::FinderThrow>, String> {
    library::get_utility_finder(&path, match_id).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            use tauri::Manager;
            let directory = app.path().app_data_dir()?;
            let queue = std::sync::Arc::new(
                import_queue::ImportQueue::new(&directory).map_err(std::io::Error::other)?,
            );
            queue.start();
            app.manage(queue);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_matches,
            get_match_detail,
            get_viewer_round,
            get_viewer_frames,
            get_utility_finder,
            edit_handoff::csdm_export_edit_handoff,
            import::csdm_import_demo,
            import::csdm_choose_demo,
            import_queue::csdm_enqueue_demo,
            import_queue::csdm_list_import_jobs,
            csdm::csdm_load_settings,
            csdm::csdm_save_settings,
            csdm::csdm_request,
            csdm::csdm_window_action,
            csdm::csdm_choose_library,
            csdm::csdm_list_demo_files,
            csdm::csdm_choose_demo_folder
        ])
        .run(tauri::generate_context!())
        .expect("DemoStrike candidate failed to start");
}
