//! The typed IPC boundary. Commands are thin: validate input, delegate to
//! the service layer, translate errors into human-readable strings. Heavy
//! operations run via `spawn_blocking` so the interface stays responsive.

use crate::service::{self, UiResult};
use crate::state::AppState;
use plumbob_core::db::{self, settings::AppSettings};
use plumbob_core::product;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};

// ---------------------------------------------------------------------------
// App identity & settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> serde_json::Value {
    json!({
        "name": product::PRODUCT_NAME,
        "tagline": product::PRODUCT_TAGLINE,
        "disclaimer": product::AFFILIATION_DISCLAIMER,
        "version": env!("CARGO_PKG_VERSION"),
        "dataDir": state.data_dir.to_string_lossy(),
        "dbPath": state.data_dir.join("plumbob.db").to_string_lossy(),
    })
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> UiResult<AppSettings> {
    let guard = service::lock_db(&state.db)?;
    db::settings::load(guard.conn()).map_err(service::err_str)
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> UiResult<()> {
    if let Some(mods) = &settings.mods_folder {
        if !mods.is_dir() {
            return Err("The chosen Mods folder doesn't exist or isn't a folder.".to_string());
        }
        for (label, candidate) in [
            ("backup", &settings.backup_folder),
            ("quarantine", &settings.quarantine_folder),
        ] {
            if let Some(path) = candidate {
                if path.starts_with(mods) {
                    return Err(format!(
                        "The {label} folder can't be inside the Mods folder."
                    ));
                }
            }
        }
    }
    let mut guard = service::lock_db(&state.db)?;
    db::settings::save(guard.conn_mut(), &settings).map_err(service::err_str)
}

// ---------------------------------------------------------------------------
// Onboarding helpers
// ---------------------------------------------------------------------------

/// Best-effort detection of the standard Sims 4 Mods folder. Localized
/// Documents folders exist in the wild; when detection misses, the interface
/// falls back to the folder picker rather than guessing.
#[tauri::command]
pub fn detect_mods_folder() -> Option<String> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let home = PathBuf::from(home);
    let candidates = [
        home.join("Documents")
            .join("Electronic Arts")
            .join("The Sims 4")
            .join("Mods"),
        home.join("OneDrive")
            .join("Documents")
            .join("Electronic Arts")
            .join("The Sims 4")
            .join("Mods"),
    ];
    candidates
        .iter()
        .find(|p| p.is_dir())
        .map(|p| p.to_string_lossy().into_owned())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModsFolderCheck {
    pub exists: bool,
    pub is_directory: bool,
    pub top_level_entries: usize,
    pub has_resource_cfg: bool,
    pub has_sims_files: bool,
}

/// Shallow, read-only sanity check of a candidate Mods folder.
#[tauri::command]
pub fn validate_mods_folder(path: String) -> ModsFolderCheck {
    let p = PathBuf::from(&path);
    let exists = p.exists();
    let is_directory = p.is_dir();
    let mut top_level_entries = 0usize;
    let mut has_resource_cfg = false;
    let mut has_sims_files = false;
    if is_directory {
        if let Ok(read) = std::fs::read_dir(&p) {
            for entry in read.flatten().take(5000) {
                top_level_entries += 1;
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name == "resource.cfg" {
                    has_resource_cfg = true;
                }
                if name.ends_with(".package") || name.ends_with(".ts4script") {
                    has_sims_files = true;
                }
            }
        }
    }
    ModsFolderCheck {
        exists,
        is_directory,
        top_level_entries,
        has_resource_cfg,
        has_sims_files,
    }
}

#[tauri::command]
pub fn game_running() -> bool {
    crate::game::sims_running()
}

// ---------------------------------------------------------------------------
// Scan lifecycle
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    scan_type: Option<String>,
) -> UiResult<service::ScanOutcome> {
    if state.scan_in_progress.swap(true, Ordering::SeqCst) {
        return Err("A scan is already running.".to_string());
    }
    state.cancel_scan.store(false, Ordering::SeqCst);
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    let cancel = state.cancel_scan.clone();
    let in_progress = state.scan_in_progress.clone();
    let label = scan_type.unwrap_or_else(|| "manual".to_string());

    let joined = tauri::async_runtime::spawn_blocking(move || {
        service::run_scan_pipeline(&app, &dbm, &data_dir, &label, &cancel)
    })
    .await
    .map_err(|e| format!("The scan task failed unexpectedly: {e}"));
    in_progress.store(false, Ordering::SeqCst);
    joined?
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> UiResult<()> {
    state.cancel_scan.store(true, Ordering::SeqCst);
    Ok(())
}

// ---------------------------------------------------------------------------
// Library queries
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_library_counts(state: State<'_, AppState>) -> UiResult<db::files::LibraryCounts> {
    let guard = service::lock_db(&state.db)?;
    db::files::library_counts(guard.conn()).map_err(service::err_str)
}

#[tauri::command]
pub fn list_files(
    state: State<'_, AppState>,
    search: Option<String>,
    filter: Option<String>,
    creator: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> UiResult<Vec<db::files::FileRow>> {
    let guard = service::lock_db(&state.db)?;
    db::files::list_files(
        guard.conn(),
        search.as_deref(),
        filter.as_deref(),
        creator.as_deref(),
        sort.as_deref(),
        limit.unwrap_or(200).clamp(1, 1000),
        offset.unwrap_or(0).max(0),
    )
    .map_err(service::err_str)
}

#[tauri::command]
pub fn count_files(
    state: State<'_, AppState>,
    search: Option<String>,
    filter: Option<String>,
    creator: Option<String>,
) -> UiResult<i64> {
    let guard = service::lock_db(&state.db)?;
    db::files::count_files(guard.conn(), search.as_deref(), filter.as_deref(), creator.as_deref())
        .map_err(service::err_str)
}

#[tauri::command]
pub fn list_duplicate_groups(
    state: State<'_, AppState>,
) -> UiResult<Vec<db::dupes::DuplicateGroupView>> {
    let guard = service::lock_db(&state.db)?;
    db::dupes::list_open_exact_groups(guard.conn()).map_err(service::err_str)
}

#[tauri::command]
pub fn list_conflicts(
    state: State<'_, AppState>,
) -> UiResult<Vec<db::packages::ConflictGroup>> {
    let guard = service::lock_db(&state.db)?;
    db::packages::list_conflict_groups(guard.conn()).map_err(service::err_str)
}

#[tauri::command]
pub fn list_suspected_duplicates(
    state: State<'_, AppState>,
) -> UiResult<Vec<db::dupes::SuspectedDuplicateGroup>> {
    let guard = service::lock_db(&state.db)?;
    db::dupes::list_suspected_duplicates(guard.conn()).map_err(service::err_str)
}

#[tauri::command]
pub fn set_duplicate_group_status(
    state: State<'_, AppState>,
    group_id: i64,
    status: String,
) -> UiResult<()> {
    if !["open", "resolved", "dismissed"].contains(&status.as_str()) {
        return Err("Unknown duplicate group status.".to_string());
    }
    let guard = service::lock_db(&state.db)?;
    db::dupes::set_group_status(guard.conn(), group_id, &status).map_err(service::err_str)
}

// ---------------------------------------------------------------------------
// Quarantine & restore flows
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn preview_quarantine(
    state: State<'_, AppState>,
    file_ids: Vec<i64>,
) -> UiResult<service::QuarantinePreview> {
    service::preview_quarantine(&state.db, &file_ids)
}

#[tauri::command]
pub async fn execute_quarantine(
    app: AppHandle,
    state: State<'_, AppState>,
    file_ids: Vec<i64>,
    reason: String,
    resolve_group_id: Option<i64>,
) -> UiResult<service::QuarantineOutcomeDto> {
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::execute_quarantine(&app, &dbm, &data_dir, &file_ids, &reason, resolve_group_id)
    })
    .await
    .map_err(|e| format!("The quarantine task failed unexpectedly: {e}"))?
}

#[tauri::command]
pub async fn restore_quarantined_file(
    app: AppHandle,
    state: State<'_, AppState>,
    entry_id: i64,
) -> UiResult<String> {
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::restore_quarantined_file(&app, &dbm, &data_dir, entry_id)
    })
    .await
    .map_err(|e| format!("The restore task failed unexpectedly: {e}"))?
}

#[tauri::command]
pub fn list_quarantine(
    state: State<'_, AppState>,
    include_restored: Option<bool>,
) -> UiResult<Vec<db::ops::QuarantineView>> {
    let guard = service::lock_db(&state.db)?;
    db::ops::list_quarantine(guard.conn(), include_restored.unwrap_or(false))
        .map_err(service::err_str)
}

// ---------------------------------------------------------------------------
// Backups & activity
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> UiResult<Vec<db::ops::BackupView>> {
    // Best-effort disk backfill first — the page self-heals.
    let _ = service::import_snapshots(&state.db, &state.data_dir);
    let guard = service::lock_db(&state.db)?;
    db::ops::list_backups(guard.conn()).map_err(service::err_str)
}

#[tauri::command]
pub fn list_backup_entries(
    state: State<'_, AppState>,
    backup_id: i64,
) -> UiResult<Vec<db::ops::BackupEntryView>> {
    let guard = service::lock_db(&state.db)?;
    db::ops::backup_entries(guard.conn(), backup_id).map_err(service::err_str)
}

#[tauri::command]
pub async fn restore_backup_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    backup_id: i64,
    source_path: String,
    overwrite: Option<bool>,
) -> UiResult<String> {
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::restore_backup_entry(
            &app,
            &dbm,
            &data_dir,
            backup_id,
            &source_path,
            overwrite.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| format!("The restore task failed unexpectedly: {e}"))?
}

#[tauri::command]
pub fn list_operations(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> UiResult<Vec<db::ops::OperationView>> {
    let guard = service::lock_db(&state.db)?;
    db::ops::list_operations(guard.conn(), limit.unwrap_or(100).clamp(1, 500))
        .map_err(service::err_str)
}

#[tauri::command]
pub fn list_operation_steps(
    state: State<'_, AppState>,
    operation_row_id: i64,
) -> UiResult<Vec<db::ops::OperationStepView>> {
    let guard = service::lock_db(&state.db)?;
    db::ops::operation_steps(guard.conn(), operation_row_id).map_err(service::err_str)
}

// ---------------------------------------------------------------------------
// Reveal
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn reveal_in_explorer(state: State<'_, AppState>, path: String) -> UiResult<()> {
    service::reveal_in_explorer(&state.db, &state.data_dir, &path)
}

// ---------------------------------------------------------------------------
// Troubleshooter (the 50/50 assistant)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn troubleshoot_active(
    state: State<'_, AppState>,
) -> UiResult<Option<plumbob_core::troubleshoot::SessionView>> {
    service::troubleshoot_active(&state.db)
}

#[tauri::command]
pub async fn troubleshoot_start(
    state: State<'_, AppState>,
    note: Option<String>,
) -> UiResult<plumbob_core::troubleshoot::SessionView> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err(
            "A scan is running. Let it finish before troubleshooting — the two \
             can't safely move files at the same time."
                .to_string(),
        );
    }
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::troubleshoot_start(&dbm, &data_dir, note.as_deref())
    })
    .await
    .map_err(|e| format!("The troubleshoot task failed unexpectedly: {e}"))?
}

#[tauri::command]
pub async fn troubleshoot_verdict(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    problem_present: bool,
) -> UiResult<plumbob_core::troubleshoot::SessionView> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err(
            "A scan is running. Let it finish before troubleshooting — the two \
             can't safely move files at the same time."
                .to_string(),
        );
    }
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::troubleshoot_verdict(&app, &dbm, &data_dir, session_id, problem_present)
    })
    .await
    .map_err(|e| format!("The troubleshoot task failed unexpectedly: {e}"))?
}

#[tauri::command]
pub async fn troubleshoot_abort(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
) -> UiResult<plumbob_core::troubleshoot::SessionView> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err(
            "A scan is running. Let it finish before troubleshooting — the two \
             can't safely move files at the same time."
                .to_string(),
        );
    }
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::troubleshoot_abort(&app, &dbm, &data_dir, session_id)
    })
    .await
    .map_err(|e| format!("The troubleshoot task failed unexpectedly: {e}"))?
}

#[tauri::command]
pub fn troubleshoot_reconcile(
    state: State<'_, AppState>,
    session_id: i64,
) -> UiResult<plumbob_core::troubleshoot::ReconcileReport> {
    service::troubleshoot_reconcile(&state.db, &state.data_dir, session_id)
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_profiles(
    state: State<'_, AppState>,
) -> UiResult<Vec<plumbob_core::db::profiles::ProfileView>> {
    service::list_profiles(&state.db)
}

#[tauri::command]
pub fn active_profile(
    state: State<'_, AppState>,
) -> UiResult<Option<plumbob_core::db::profiles::ProfileView>> {
    service::active_profile(&state.db)
}

#[tauri::command]
pub fn create_profile(
    state: State<'_, AppState>,
    name: String,
) -> UiResult<plumbob_core::db::profiles::ProfileView> {
    service::create_profile(&state.db, &name)
}

#[tauri::command]
pub fn rename_profile(
    state: State<'_, AppState>,
    profile_id: i64,
    name: String,
) -> UiResult<()> {
    service::rename_profile(&state.db, profile_id, &name)
}

#[tauri::command]
pub fn set_active_profile(state: State<'_, AppState>, profile_id: i64) -> UiResult<()> {
    service::set_active_profile(&state.db, profile_id)
}

#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, profile_id: i64) -> UiResult<()> {
    service::delete_profile(&state.db, profile_id)
}

#[tauri::command]
pub async fn set_files_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    file_ids: Vec<i64>,
    enabled: bool,
) -> UiResult<service::ToggleOutcomeDto> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err(
            "A scan is running. Let it finish before enabling or disabling \
             mods."
                .to_string(),
        );
    }
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::set_files_enabled(&app, &dbm, &data_dir, &file_ids, enabled)
    })
    .await
    .map_err(|e| format!("The toggle task failed unexpectedly: {e}"))?
}

#[tauri::command]
pub fn preview_switch_profile(
    state: State<'_, AppState>,
    profile_id: i64,
) -> UiResult<plumbob_core::db::profiles::SwitchPlan> {
    service::preview_switch_profile(&state.db, profile_id)
}

#[tauri::command]
pub async fn switch_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: i64,
) -> UiResult<service::SwitchOutcomeDto> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err(
            "A scan is running. Let it finish before switching profiles."
                .to_string(),
        );
    }
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::switch_profile(&app, &dbm, &data_dir, profile_id)
    })
    .await
    .map_err(|e| format!("The profile switch failed unexpectedly: {e}"))?
}

// ---------------------------------------------------------------------------
// Patch Center
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn check_curse_updates(
    app: AppHandle,
    state: State<'_, AppState>,
) -> UiResult<service::PatchCheckSummary> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err(
            "A scan is running. Let it finish before checking for updates."
                .to_string(),
        );
    }
    let dbm = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::check_curse_updates(&app, &dbm)
    })
    .await
    .map_err(|e| format!("The update check failed unexpectedly: {e}"))?
}

#[tauri::command]
pub fn curse_status(
    state: State<'_, AppState>,
) -> UiResult<Vec<plumbob_core::db::curse::CurseStatusRow>> {
    service::curse_status(&state.db)
}

#[tauri::command]
pub fn open_external(url: String) -> UiResult<()> {
    let allowed = url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("curseforge://install");
    if !allowed {
        return Err("Only web links and CurseForge app links can be opened.".to_string());
    }
    open::that(&url).map_err(|e| format!("Couldn't open the browser: {e}"))
}

#[tauri::command]
pub async fn get_thumbnails(
    state: State<'_, AppState>,
    file_ids: Vec<i64>,
) -> UiResult<Vec<service::ThumbDto>> {
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::thumbnails(&dbm, &data_dir, &file_ids)
    })
    .await
    .map_err(|e| format!("The thumbnail task failed unexpectedly: {e}"))?
}

#[tauri::command]
pub async fn prepare_thumbnails(
    app: AppHandle,
    state: State<'_, AppState>,
) -> UiResult<service::PrepareOutcome> {
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::prepare_thumbnails(&app, &dbm, &data_dir)
    })
    .await
    .map_err(|e| format!("The thumbnail prewarm failed unexpectedly: {e}"))?
}

#[tauri::command]
pub async fn thumbnail_census(
    state: State<'_, AppState>,
) -> UiResult<service::CensusReport> {
    let dbm = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::thumbnail_census(&dbm, &data_dir)
    })
    .await
    .map_err(|e| format!("The census failed unexpectedly: {e}"))?
}

#[tauri::command]
pub fn creators_overview(
    state: State<'_, AppState>,
) -> UiResult<Vec<db::files::CreatorRow>> {
    let guard = service::lock_db(&state.db)?;
    db::files::creators_overview(guard.conn()).map_err(service::err_str)
}

#[tauri::command]
pub async fn reverify_matches(
    app: AppHandle,
    state: State<'_, AppState>,
) -> UiResult<service::ReverifyOutcome> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || service::reverify_matches(&app, &db))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn apply_update(
    state: State<'_, AppState>,
    file_id: i64,
) -> UiResult<service::UpdateOutcome> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err("A scan is running. Let it finish before updating files.".to_string());
    }
    let db = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service::apply_update(&db, &data_dir, file_id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn merge_files(
    state: State<'_, AppState>,
    file_ids: Vec<i64>,
    label: Option<String>,
) -> UiResult<service::MergeOutcome> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err("A scan is running. Let it finish before merging.".to_string());
    }
    let db = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || service::merge_files(&db, &data_dir, &file_ids, label.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn plan_auto_merge(state: State<'_, AppState>) -> UiResult<service::AutoMergePlan> {
    service::plan_auto_merge(&state.db)
}

#[tauri::command]
pub fn title_plan(
    state: State<'_, AppState>,
    file_ids: Option<Vec<i64>>,
    today: bool,
) -> UiResult<service::TitlePlan> {
    service::title_plan(&state.db, file_ids, today)
}

#[tauri::command]
pub fn title_apply(
    state: State<'_, AppState>,
    file_ids: Option<Vec<i64>>,
    today: bool,
) -> UiResult<service::TitleOutcome> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err("A scan is running. Let it finish before renaming.".to_string());
    }
    service::title_apply(&state.db, file_ids, today)
}

#[tauri::command]
pub fn merge_mode_status(state: State<'_, AppState>) -> UiResult<service::MergeModeStatus> {
    service::merge_mode_status(&state.db)
}

#[tauri::command]
pub async fn auto_merge_run(state: State<'_, AppState>) -> UiResult<service::MergeModeOutcome> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err("A scan is running. Let it finish first.".to_string());
    }
    let db = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || service::auto_merge_run(&db, &data_dir))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn un_merge_run(state: State<'_, AppState>) -> UiResult<service::UnMergeOutcome> {
    if state.scan_in_progress.load(Ordering::SeqCst) {
        return Err("A scan is running. Let it finish first.".to_string());
    }
    let db = state.db.clone();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || service::un_merge_run(&db, &data_dir))
        .await
        .map_err(|e| e.to_string())?
}
