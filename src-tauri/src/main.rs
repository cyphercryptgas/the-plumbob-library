// Prevent an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod curse_api;
mod game;
mod service;
mod state;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // App data lives in the platform-standard location for our
            // identifier (on Windows: %APPDATA%\com.moetech.plumbob).
            let data_dir = app.path().app_data_dir()?;
            let app_state = state::AppState::initialize(&data_dir)?;
            app.manage(app_state);
            // Window title comes from the centralized product constant so a
            // product rename never leaves a stale title behind.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(plumbob_core::product::PRODUCT_NAME);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::detect_mods_folder,
            commands::validate_mods_folder,
            commands::game_running,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_library_counts,
            commands::list_files,
            commands::count_files,
            commands::list_duplicate_groups,
            commands::list_conflicts,
            commands::list_suspected_duplicates,
            commands::set_duplicate_group_status,
            commands::preview_quarantine,
            commands::execute_quarantine,
            commands::restore_quarantined_file,
            commands::troubleshoot_active,
            commands::troubleshoot_start,
            commands::troubleshoot_verdict,
            commands::troubleshoot_abort,
            commands::troubleshoot_reconcile,
            commands::list_profiles,
            commands::active_profile,
            commands::create_profile,
            commands::rename_profile,
            commands::set_active_profile,
            commands::delete_profile,
            commands::set_files_enabled,
            commands::preview_switch_profile,
            commands::switch_profile,
            commands::check_curse_updates,
            commands::curse_status,
            commands::open_external,
            commands::get_thumbnails,
            commands::prepare_thumbnails,
            commands::thumbnail_census,
            commands::creators_overview,
            commands::reverify_matches,
            commands::apply_update,
            commands::merge_files,
            commands::list_quarantine,
            commands::list_backups,
            commands::list_backup_entries,
            commands::restore_backup_entry,
            commands::list_operations,
            commands::list_operation_steps,
            commands::reveal_in_explorer
        ])
        .run(tauri::generate_context!())
        .expect("failed to launch the application shell");
}
