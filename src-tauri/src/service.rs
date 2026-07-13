//! Orchestration between the Tauri command boundary and the tested core.
//! Every mutating flow follows the safety contract: game-closed guard →
//! plan from database truth → recovery snapshot → hash-verified execution →
//! journal → record → event. All heavy work runs on blocking threads (see
//! commands.rs); the database mutex intentionally serializes mutations.

use plumbob_core::db::{self, settings::AppSettings, Database};
use plumbob_core::dbpf;
use plumbob_core::duplicates;
use plumbob_core::hashing;
use plumbob_core::ops::{self, QuarantineRequest, SnapshotEntry};
use plumbob_core::ops::JournalSink as _;
use plumbob_core::paths::SafeRoot;
use plumbob_core::troubleshoot as ts;
use plumbob_core::scan::{self, ScanOptions};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};

pub type UiResult<T> = Result<T, String>;

pub fn err_str<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn lock_db(db: &Mutex<Database>) -> UiResult<MutexGuard<'_, Database>> {
    db.lock()
        .map_err(|_| "Internal error: the database lock was poisoned.".to_string())
}

pub const MSG_NO_MODS_FOLDER: &str =
    "No Mods folder is configured yet. Choose your Sims 4 Mods folder first.";

pub fn ensure_game_closed() -> UiResult<()> {
    if crate::game::sims_running() {
        Err("The Sims 4 appears to be running. Close the game before changing \
             anything in the Mods folder — moving files the game holds open can \
             corrupt a session."
            .to_string())
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Roots
// ---------------------------------------------------------------------------

pub struct Roots {
    pub mods: SafeRoot,
    pub quarantine: SafeRoot,
    pub backups: SafeRoot,
    pub settings: AppSettings,
}

/// Resolve the three working roots from settings, creating the app-owned
/// folders if needed. Backup/quarantine inside the Mods folder is refused —
/// the scanner would churn on them and quarantine could "quarantine itself".
pub fn resolve_roots(dbase: &Database, data_dir: &Path) -> UiResult<Roots> {
    let settings = db::settings::load(dbase.conn()).map_err(err_str)?;
    let mods_path = settings
        .mods_folder
        .clone()
        .ok_or_else(|| MSG_NO_MODS_FOLDER.to_string())?;
    let mods = SafeRoot::new(&mods_path)
        .map_err(|e| format!("The configured Mods folder can't be opened: {e}"))?;

    let quarantine_path = settings
        .quarantine_folder
        .clone()
        .unwrap_or_else(|| data_dir.join("Quarantine"));
    let backup_path = settings
        .backup_folder
        .clone()
        .unwrap_or_else(|| data_dir.join("Backups"));
    std::fs::create_dir_all(&quarantine_path)
        .map_err(|e| format!("Could not prepare the quarantine folder: {e}"))?;
    std::fs::create_dir_all(&backup_path)
        .map_err(|e| format!("Could not prepare the backup folder: {e}"))?;

    let quarantine = SafeRoot::new(&quarantine_path).map_err(err_str)?;
    let backups = SafeRoot::new(&backup_path).map_err(err_str)?;
    if quarantine.path().starts_with(mods.path()) || backups.path().starts_with(mods.path()) {
        return Err(
            "Backup and quarantine folders can't live inside the Mods folder itself."
                .to_string(),
        );
    }
    Ok(Roots {
        mods,
        quarantine,
        backups,
        settings,
    })
}

// ---------------------------------------------------------------------------
// Scan pipeline
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressEvent {
    pub phase: &'static str,
    pub files_seen: u64,
    pub bytes_seen: u64,
    pub hashed: usize,
    pub to_hash: usize,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScanOutcome {
    pub scan_id: i64,
    pub new_files: usize,
    pub changed_files: usize,
    pub unchanged_files: usize,
    pub missing_files: usize,
    pub reappeared_files: usize,
    pub hashed_files: usize,
    pub hash_errors: usize,
    pub duplicate_groups: usize,
    pub packages_parsed: usize,
    pub parse_errors: usize,
    pub scan_errors: usize,
    pub cancelled: bool,
    pub duration_ms: u64,
}

/// Scan → reconcile → hash → refresh duplicate groups, emitting
/// `scan://progress` along the way and `scan://completed` at the end.
/// The database lock is held only for the short write phases, never during
/// the filesystem walk or hashing.
pub fn run_scan_pipeline(
    app: &AppHandle,
    dbm: &Mutex<Database>,
    data_dir: &Path,
    scan_type: &str,
    cancel: &AtomicBool,
) -> UiResult<ScanOutcome> {
    let started = std::time::Instant::now();
    let roots = {
        let guard = lock_db(dbm)?;
        if let Some(id) =
            db::troubleshoot::active_session_id(guard.conn()).map_err(err_str)?
        {
            return Err(format!(
                "Troubleshooting session #{id} is active. Finish or abort the \
                 hunt before scanning — a scan would mark the set-aside half \
                 of your library as missing."
            ));
        }
        resolve_roots(&guard, data_dir)?
    };
    let opts = ScanOptions {
        excluded_relative: roots.settings.scan_excluded.clone(),
        script_depth_limit: roots.settings.script_depth_limit,
    };

    let mut tick: u64 = 0;
    let report = scan::scan(&roots.mods, &opts, cancel, |p| {
        tick += 1;
        if tick % 50 == 0 {
            let _ = app.emit(
                "scan://progress",
                ScanProgressEvent {
                    phase: "scanning",
                    files_seen: p.files_seen,
                    bytes_seen: p.bytes_seen,
                    hashed: 0,
                    to_hash: 0,
                },
            );
        }
    });
    let scan_errors = report.errors.len();
    let cancelled_walk = report.cancelled;

    let summary = {
        let mut guard = lock_db(dbm)?;
        let summary = db::files::reconcile_scan(
            guard.conn_mut(),
            &report,
            scan_type,
            &opts.excluded_relative,
        )
        .map_err(err_str)?;
        db::profiles::sync_active_set(guard.conn_mut()).map_err(err_str)?;
        summary
    };

    // Hash pass. Content identity underpins duplicate detection and every
    // verified operation, so new/changed files are always hashed.
    let to_hash = summary.needs_hash.len();
    let mut updates: Vec<(i64, String)> = Vec::with_capacity(to_hash);
    let mut hash_errors = 0usize;
    for (i, (id, abs)) in summary.needs_hash.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match hashing::sha256_file(abs) {
            Ok(hash) => updates.push((*id, hash)),
            Err(_) => hash_errors += 1,
        }
        if (i + 1) % 25 == 0 {
            let _ = app.emit(
                "scan://progress",
                ScanProgressEvent {
                    phase: "hashing",
                    files_seen: report.files.len() as u64,
                    bytes_seen: report.total_bytes,
                    hashed: i + 1,
                    to_hash,
                },
            );
        }
    }
    let hashed_files = updates.len();
    {
        let mut guard = lock_db(dbm)?;
        db::files::update_hashes(guard.conn_mut(), &updates).map_err(err_str)?;
    }

    // Package index pass (Phase 2). Content-keyed incremental: only files
    // whose fingerprint changed since their last parse do any work. File IO
    // happens outside the database lock, mirroring the hash pass; results
    // are recorded in one transaction.
    let (packages_parsed, parse_errors) = {
        let pending = {
            let guard = lock_db(dbm)?;
            db::packages::files_needing_parse(guard.conn()).map_err(err_str)?
        };
        let to_parse = pending.len();
        let mut results: Vec<(i64, Result<dbpf::PackageIndex, dbpf::DbpfError>)> =
            Vec::with_capacity(to_parse);
        for (i, (file_id, rel)) in pending.into_iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let parsed = match roots.mods.resolve_relative(Path::new(&rel)) {
                Ok(abs) => dbpf::read_package_index(&abs),
                Err(_) => Err(dbpf::DbpfError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "path escaped the mods folder",
                ))),
            };
            results.push((file_id, parsed));
            if (i + 1) % 25 == 0 {
                let _ = app.emit(
                    "scan://progress",
                    ScanProgressEvent {
                        phase: "parsing",
                        files_seen: report.files.len() as u64,
                        bytes_seen: report.total_bytes,
                        hashed: i + 1,
                        to_hash: to_parse,
                    },
                );
            }
        }
        let mut ok = 0usize;
        let mut failed = 0usize;
        {
            let mut guard = lock_db(dbm)?;
            let tx = guard.conn_mut().transaction().map_err(err_str)?;
            for (file_id, parsed) in &results {
                match parsed {
                    Ok(index) => {
                        db::packages::record_package_index(&tx, *file_id, index)
                            .map_err(err_str)?;
                        ok += 1;
                    }
                    Err(err) => {
                        db::packages::record_parse_error(&tx, *file_id, err)
                            .map_err(err_str)?;
                        failed += 1;
                    }
                }
            }
            tx.commit().map_err(err_str)?;
        }
        (ok, failed)
    };

    let duplicate_groups = {
        let mut guard = lock_db(dbm)?;
        // Fresh resources → fresh in-game categories, same lock.
        db::packages::classify_categories(guard.conn()).map_err(err_str)?;
        let facts = db::dupes::load_file_facts(guard.conn()).map_err(err_str)?;
        let groups = duplicates::group_exact(&facts);
        // The insert count excludes fingerprints the user has dismissed, so
        // the reported number matches what the Duplicate Center will show.
        db::dupes::replace_exact_groups(guard.conn_mut(), &groups).map_err(err_str)?
    };

    // Subcategory pass: reads CAS payloads, so it runs after the lock-held
    // classification and never inside it.
    let _ = classify_cas_subtypes(dbm);
    let _ = classify_creators(dbm);

    let outcome = ScanOutcome {
        scan_id: summary.scan_id,
        new_files: summary.new_files,
        changed_files: summary.changed_files,
        unchanged_files: summary.unchanged_files,
        missing_files: summary.missing_files,
        reappeared_files: summary.reappeared_files,
        hashed_files,
        hash_errors,
        duplicate_groups,
        packages_parsed,
        parse_errors,
        scan_errors,
        cancelled: cancelled_walk || cancel.load(Ordering::Relaxed),
        duration_ms: started.elapsed().as_millis() as u64,
    };
    let _ = app.emit("scan://completed", outcome.clone());
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Quarantine
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantinePreview {
    pub files: Vec<db::files::FileRow>,
    pub total_bytes: i64,
    pub files_without_hash: usize,
    pub files_missing_on_disk: usize,
}

pub fn preview_quarantine(dbm: &Mutex<Database>, file_ids: &[i64]) -> UiResult<QuarantinePreview> {
    let guard = lock_db(dbm)?;
    let files = db::files::files_by_ids(guard.conn(), file_ids).map_err(err_str)?;
    if files.len() != file_ids.len() {
        return Err(
            "Some selected files no longer exist in the library records. Re-scan and try again."
                .to_string(),
        );
    }
    let total_bytes = files.iter().map(|f| f.size_bytes).sum();
    let files_without_hash = files.iter().filter(|f| f.sha256.is_none()).count();
    let files_missing_on_disk = files.iter().filter(|f| f.missing).count();
    Ok(QuarantinePreview {
        files,
        total_bytes,
        files_without_hash,
        files_missing_on_disk,
    })
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FailedStep {
    pub path: String,
    pub message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineOutcomeDto {
    pub operation_id: String,
    pub backup_id: i64,
    pub completed: usize,
    pub failed: Vec<FailedStep>,
    pub halted_early: bool,
    pub reclaimed_bytes: i64,
}

/// Guard → snapshot (all-or-nothing) → hash-verified moves → records.
/// The expected hash for every move comes from the database row, so a file
/// that changed since the plan was made refuses to move (stale-plan
/// protection) instead of silently quarantining unknown bytes.
pub fn execute_quarantine(
    app: &AppHandle,
    dbm: &Mutex<Database>,
    data_dir: &Path,
    file_ids: &[i64],
    reason: &str,
    resolve_group_id: Option<i64>,
) -> UiResult<QuarantineOutcomeDto> {
    ensure_game_closed()?;
    if file_ids.is_empty() {
        return Err("Nothing selected to quarantine.".to_string());
    }
    let mut guard = lock_db(dbm)?;
    let roots = resolve_roots(&guard, data_dir)?;
    let rows = db::files::files_by_ids(guard.conn(), file_ids).map_err(err_str)?;
    if rows.len() != file_ids.len() {
        return Err(
            "Some selected files no longer exist in the library records. Re-scan and try again."
                .to_string(),
        );
    }
    if rows.iter().any(|r| r.missing) {
        return Err(
            "A selected file is already missing on disk. Re-scan before quarantining."
                .to_string(),
        );
    }
    if rows.iter().any(|r| r.status == "quarantined") {
        return Err("A selected file is already quarantined.".to_string());
    }
    let rels: Vec<PathBuf> = rows
        .iter()
        .map(|r| PathBuf::from(&r.relative_path))
        .collect();

    // 1) Recovery snapshot before anything moves.
    let (snapshot_dir, manifest) = {
        let mut journal = db::ops::SqliteJournal::new(guard.conn());
        let result = ops::create_snapshot(
            &roots.mods,
            &roots.backups,
            &rels,
            &format!("Automatic backup before quarantine ({reason})"),
            &mut journal,
        );
        journal.finish().map_err(err_str)?;
        result.map_err(|e| format!("Backup failed, so nothing was quarantined: {e}"))?
    };
    let backup_id =
        db::ops::record_snapshot(guard.conn_mut(), &manifest, &snapshot_dir).map_err(err_str)?;

    // 2) Hash-verified moves.
    let requests: Vec<QuarantineRequest> = rows
        .iter()
        .map(|r| QuarantineRequest {
            source_relative: PathBuf::from(&r.relative_path),
            reason: reason.to_string(),
            expected_sha256: r.sha256.clone(),
        })
        .collect();
    let outcome = {
        let mut journal = db::ops::SqliteJournal::new(guard.conn());
        let o = ops::quarantine_files(
            &roots.mods,
            &roots.quarantine,
            &requests,
            roots.settings.stop_on_error,
            &mut journal,
        );
        journal.finish().map_err(err_str)?;
        o
    };
    db::ops::record_quarantine_outcome(guard.conn_mut(), &outcome).map_err(err_str)?;
    db::profiles::sync_active_set(guard.conn_mut()).map_err(err_str)?;

    if let Some(group_id) = resolve_group_id {
        if outcome.failed.is_empty() {
            db::dupes::set_group_status(guard.conn(), group_id, "resolved").map_err(err_str)?;
        }
    }

    let reclaimed_bytes: i64 = rows
        .iter()
        .filter(|r| {
            outcome
                .completed
                .iter()
                .any(|c| c.original_relative.as_path() == Path::new(&r.relative_path))
        })
        .map(|r| r.size_bytes)
        .sum();
    let dto = QuarantineOutcomeDto {
        operation_id: outcome.operation_id.clone(),
        backup_id,
        completed: outcome.completed.len(),
        failed: outcome
            .failed
            .iter()
            .map(|f| FailedStep {
                path: f.source.to_string_lossy().into_owned(),
                message: f.message.clone(),
            })
            .collect(),
        halted_early: outcome.halted_early,
        reclaimed_bytes,
    };
    let _ = app.emit("library://changed", "quarantine");
    Ok(dto)
}

/// Restore a quarantined file to its original relative path, hash-verified
/// against what was recorded at quarantine time. Never overwrites.
pub fn restore_quarantined_file(
    app: &AppHandle,
    dbm: &Mutex<Database>,
    data_dir: &Path,
    entry_id: i64,
) -> UiResult<String> {
    ensure_game_closed()?;
    let mut guard = lock_db(dbm)?;
    let roots = resolve_roots(&guard, data_dir)?;
    let view = db::ops::quarantine_entry_by_id(guard.conn(), entry_id)
        .map_err(err_str)?
        .ok_or_else(|| "That quarantine entry no longer exists.".to_string())?;
    if view.status != "quarantined" {
        return Err("That file was already restored.".to_string());
    }
    let sha256 = view.sha256.clone().ok_or_else(|| {
        "This entry has no recorded hash and can't be verified for safe restore.".to_string()
    })?;
    let entry = ops::QuarantineEntry {
        original_relative: PathBuf::from(&view.original_path),
        stored_absolute: PathBuf::from(&view.quarantine_path),
        sha256,
        reason: view.reason.clone(),
    };
    let restored = {
        let mut journal = db::ops::SqliteJournal::new(guard.conn());
        let result = ops::restore_quarantined(&roots.mods, &entry, &mut journal);
        journal.finish().map_err(err_str)?;
        result.map_err(err_str)?
    };
    db::ops::mark_quarantine_restored(guard.conn_mut(), entry_id).map_err(err_str)?;
    db::profiles::sync_active_set(guard.conn_mut()).map_err(err_str)?;
    let _ = app.emit("library://changed", "restore");
    Ok(restored.to_string_lossy().into_owned())
}

/// Restore one file from a recorded backup. The stored copy is verified
/// against the manifest hash before the live file is touched; overwriting
/// requires an explicit flag from the interface.
pub fn restore_backup_entry(
    app: &AppHandle,
    dbm: &Mutex<Database>,
    data_dir: &Path,
    backup_id: i64,
    source_path: &str,
    overwrite: bool,
) -> UiResult<String> {
    ensure_game_closed()?;
    let guard = lock_db(dbm)?;
    let roots = resolve_roots(&guard, data_dir)?;
    let backup = db::ops::list_backups(guard.conn())
        .map_err(err_str)?
        .into_iter()
        .find(|b| b.id == backup_id)
        .ok_or_else(|| "That backup no longer exists in the records.".to_string())?;
    let entry = db::ops::backup_entries(guard.conn(), backup_id)
        .map_err(err_str)?
        .into_iter()
        .find(|e| e.source_path.eq_ignore_ascii_case(source_path))
        .ok_or_else(|| "That file isn't part of the selected backup.".to_string())?;
    let snap_entry = SnapshotEntry {
        relative_path: PathBuf::from(&entry.source_path),
        sha256: entry.sha256.clone(),
        size_bytes: entry.size_bytes as u64,
    };
    let snapshot_dir = PathBuf::from(&backup.root_path);
    let restored = {
        let mut journal = db::ops::SqliteJournal::new(guard.conn());
        let result = ops::restore_from_snapshot(
            &roots.mods,
            &snapshot_dir,
            &snap_entry,
            overwrite,
            &mut journal,
        );
        journal.finish().map_err(err_str)?;
        result.map_err(err_str)?
    };
    let _ = app.emit("library://changed", "backup-restore");
    Ok(restored.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Reveal in file manager (path-gated)
// ---------------------------------------------------------------------------

/// Open the system file manager with the item selected. Gated to paths
/// inside the library, quarantine, backup, or app-data roots — the interface
/// never gets a generic "open anything" primitive.
pub fn reveal_in_explorer(dbm: &Mutex<Database>, data_dir: &Path, raw_path: &str) -> UiResult<()> {
    let target = dunce::canonicalize(Path::new(raw_path))
        .map_err(|e| format!("That path can't be opened: {e}"))?;
    let mut allowed: Vec<PathBuf> = vec![data_dir.to_path_buf()];
    if let Ok(guard) = dbm.lock() {
        if let Ok(roots) = resolve_roots(&guard, data_dir) {
            allowed.push(roots.mods.path().to_path_buf());
            allowed.push(roots.quarantine.path().to_path_buf());
            allowed.push(roots.backups.path().to_path_buf());
        }
    }
    if !allowed.iter().any(|root| target.starts_with(root)) {
        return Err(
            "Only files inside the library, quarantine, backup, or app data folders can be revealed."
                .to_string(),
        );
    }
    #[cfg(target_os = "windows")]
    {
        // Standard argument quoting wraps the whole "/select,<path>" token in
        // quotes when the path contains spaces, which explorer.exe rejects —
        // it falls back to opening the default folder instead of selecting
        // the file. Pass the command line raw with only the path quoted.
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", target.display()))
            .spawn()
            .map_err(err_str)?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let parent = target.parent().unwrap_or(&target);
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(err_str)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Troubleshooter (the 50/50 assistant)
// ---------------------------------------------------------------------------

/// The holding area for set-aside halves lives beside Quarantine and
/// Backups, never inside the Mods folder.
fn troubleshoot_holding_root(data_dir: &Path) -> UiResult<SafeRoot> {
    let path = data_dir.join("Troubleshoot");
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("Could not prepare the troubleshoot holding folder: {e}"))?;
    SafeRoot::new(&path).map_err(err_str)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TroubleshootProgress {
    done: usize,
    total: usize,
}

/// Collects journal events for later persistence while streaming progress to
/// the interface — a round on a large library reads gigabytes for hash
/// verification, and silence would look like a freeze.
struct EmittingJournal<'a> {
    app: &'a AppHandle,
    event: &'static str,
    events: Vec<plumbob_core::ops::JournalEvent>,
    total: usize,
    done: usize,
    last_emit: std::time::Instant,
}

impl<'a> EmittingJournal<'a> {
    fn new(app: &'a AppHandle, event: &'static str) -> Self {
        Self {
            app,
            event,
            events: Vec::new(),
            total: 0,
            done: 0,
            last_emit: std::time::Instant::now(),
        }
    }
    fn emit(&mut self, force: bool) {
        if force || self.last_emit.elapsed().as_millis() >= 150 {
            let _ = self.app.emit(
                self.event,
                TroubleshootProgress {
                    done: self.done,
                    total: self.total,
                },
            );
            self.last_emit = std::time::Instant::now();
        }
    }
}

impl plumbob_core::ops::JournalSink for EmittingJournal<'_> {
    fn record(&mut self, event: plumbob_core::ops::JournalEvent) {
        use plumbob_core::ops::JournalEvent as E;
        match &event {
            E::OperationStarted { total_steps, .. } => {
                self.total = *total_steps;
                self.done = 0;
                self.emit(true);
            }
            E::StepSucceeded { .. } | E::StepFailed { .. } => {
                self.done += 1;
                self.emit(false);
            }
            E::OperationFinished { .. } => self.emit(true),
        }
        self.events.push(event);
    }
}

/// The engine interleaves database writes with journaling, so it records
/// into an in-memory journal; the events are replayed into SQLite here once
/// the connection is free. A journal write failure is surfaced but never
/// undoes completed filesystem work.
fn replay_journal(
    guard: &Database,
    events: Vec<plumbob_core::ops::JournalEvent>,
) -> UiResult<()> {
    if events.is_empty() {
        return Ok(());
    }
    let mut journal = db::ops::SqliteJournal::new(guard.conn());
    for event in events {
        journal.record(event);
    }
    journal.finish().map_err(err_str)
}

pub fn troubleshoot_active(dbm: &Mutex<Database>) -> UiResult<Option<ts::SessionView>> {
    let guard = lock_db(dbm)?;
    ts::active_session(guard.conn()).map_err(err_str)
}

/// Starting a session enrolls files and rests in baseline — nothing moves,
/// so the game may still be running while the user reads the intro.
pub fn troubleshoot_start(
    dbm: &Mutex<Database>,
    data_dir: &Path,
    note: Option<&str>,
) -> UiResult<ts::SessionView> {
    let mut guard = lock_db(dbm)?;
    let roots = resolve_roots(&guard, data_dir)?;
    let holding = troubleshoot_holding_root(data_dir)?;
    let tsr = ts::TroubleshootRoots {
        mods: &roots.mods,
        holding: &holding,
        quarantine: &roots.quarantine,
    };
    ts::start_session(guard.conn_mut(), &tsr, note).map_err(err_str)
}

/// Verdicts arrange files, so the game-closed guard applies. When a verdict
/// completes the session with a confirmed culprit, the library changed (the
/// culprit is now quarantined) and the frontend is told.
pub fn troubleshoot_verdict(
    app: &AppHandle,
    dbm: &Mutex<Database>,
    data_dir: &Path,
    session_id: i64,
    problem_present: bool,
) -> UiResult<ts::SessionView> {
    ensure_game_closed()?;
    let mut guard = lock_db(dbm)?;
    let roots = resolve_roots(&guard, data_dir)?;
    let holding = troubleshoot_holding_root(data_dir)?;
    let tsr = ts::TroubleshootRoots {
        mods: &roots.mods,
        holding: &holding,
        quarantine: &roots.quarantine,
    };
    let verdict = if problem_present {
        ts::Verdict::ProblemPresent
    } else {
        ts::Verdict::ProblemGone
    };
    let mut journal = EmittingJournal::new(app, "troubleshoot://progress");
    let result = ts::submit_verdict(guard.conn_mut(), &tsr, session_id, verdict, &mut journal);
    replay_journal(&guard, journal.events)?;
    let view = result.map_err(err_str)?;
    if view.outcome.as_deref() == Some("culprit_confirmed") {
        let _ = app.emit("library://changed", "troubleshoot");
    }
    Ok(view)
}

pub fn troubleshoot_abort(
    app: &AppHandle,
    dbm: &Mutex<Database>,
    data_dir: &Path,
    session_id: i64,
) -> UiResult<ts::SessionView> {
    ensure_game_closed()?;
    let mut guard = lock_db(dbm)?;
    let roots = resolve_roots(&guard, data_dir)?;
    let holding = troubleshoot_holding_root(data_dir)?;
    let tsr = ts::TroubleshootRoots {
        mods: &roots.mods,
        holding: &holding,
        quarantine: &roots.quarantine,
    };
    let mut journal = EmittingJournal::new(app, "troubleshoot://progress");
    let result = ts::abort_session(guard.conn_mut(), &tsr, session_id, &mut journal);
    replay_journal(&guard, journal.events)?;
    result.map_err(err_str)
}

/// Heal member rows from disk truth (rows only — no files move), typically
/// when the wizard opens onto an already-active session.
pub fn troubleshoot_reconcile(
    dbm: &Mutex<Database>,
    data_dir: &Path,
    session_id: i64,
) -> UiResult<ts::ReconcileReport> {
    let mut guard = lock_db(dbm)?;
    let roots = resolve_roots(&guard, data_dir)?;
    let holding = troubleshoot_holding_root(data_dir)?;
    let tsr = ts::TroubleshootRoots {
        mods: &roots.mods,
        holding: &holding,
        quarantine: &roots.quarantine,
    };
    ts::reconcile(guard.conn_mut(), &tsr, session_id).map_err(err_str)
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

fn friendly_profile_error(e: plumbob_core::db::DbError) -> String {
    let msg = e.to_string();
    if msg.to_lowercase().contains("unique") {
        "A profile with that name already exists.".to_string()
    } else {
        msg
    }
}

fn validated_profile_name(name: &str) -> UiResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("A profile needs a name.".to_string());
    }
    if trimmed.chars().count() > 40 {
        return Err("Profile names are capped at 40 characters.".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn list_profiles(dbm: &Mutex<Database>) -> UiResult<Vec<db::profiles::ProfileView>> {
    let guard = lock_db(dbm)?;
    db::profiles::list_profiles(guard.conn()).map_err(err_str)
}

pub fn active_profile(dbm: &Mutex<Database>) -> UiResult<Option<db::profiles::ProfileView>> {
    let guard = lock_db(dbm)?;
    db::profiles::active_profile(guard.conn()).map_err(err_str)
}

pub fn create_profile(
    dbm: &Mutex<Database>,
    name: &str,
) -> UiResult<db::profiles::ProfileView> {
    let name = validated_profile_name(name)?;
    let mut guard = lock_db(dbm)?;
    db::profiles::create_profile(guard.conn_mut(), &name).map_err(friendly_profile_error)
}

pub fn rename_profile(dbm: &Mutex<Database>, profile_id: i64, name: &str) -> UiResult<()> {
    let name = validated_profile_name(name)?;
    let guard = lock_db(dbm)?;
    db::profiles::rename_profile(guard.conn(), profile_id, &name)
        .map_err(friendly_profile_error)
}

pub fn set_active_profile(dbm: &Mutex<Database>, profile_id: i64) -> UiResult<()> {
    let mut guard = lock_db(dbm)?;
    db::profiles::set_active_profile(guard.conn_mut(), profile_id).map_err(err_str)
}

pub fn delete_profile(dbm: &Mutex<Database>, profile_id: i64) -> UiResult<()> {
    let guard = lock_db(dbm)?;
    db::profiles::delete_profile(guard.conn(), profile_id).map_err(err_str)
}

// ---------------------------------------------------------------------------
// Enable / disable
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToggleOutcomeDto {
    pub completed: usize,
    pub skipped: usize,
    pub failed: Vec<FailedStep>,
}

/// Guard → validate → verified in-place renames → row sync. Disabled mods
/// never leave their folder; the game just stops seeing them.
pub fn set_files_enabled(
    app: &AppHandle,
    dbm: &Mutex<Database>,
    data_dir: &Path,
    file_ids: &[i64],
    enable: bool,
) -> UiResult<ToggleOutcomeDto> {
    ensure_game_closed()?;
    if file_ids.is_empty() {
        return Err("Nothing selected.".to_string());
    }
    let mut guard = lock_db(dbm)?;
    if let Some(id) =
        db::troubleshoot::active_session_id(guard.conn()).map_err(err_str)?
    {
        return Err(format!(
            "Troubleshooting session #{id} is active. Finish or abort the \
             hunt before enabling or disabling mods — the hunt is moving \
             files and the two must not overlap."
        ));
    }
    let roots = resolve_roots(&guard, data_dir)?;
    let rows = db::files::files_by_ids(guard.conn(), file_ids).map_err(err_str)?;
    if rows.iter().any(|r| r.missing) {
        return Err(
            "A selected file is missing on disk. Re-scan before toggling."
                .to_string(),
        );
    }
    if rows.iter().any(|r| r.status == "quarantined") {
        return Err("Quarantined files are managed from the Quarantine screen.".to_string());
    }
    let (todo, skipped): (Vec<_>, Vec<_>) =
        rows.iter().partition(|r| r.enabled != enable);
    let requests: Vec<ops::ToggleRequest> = todo
        .iter()
        .map(|r| ops::ToggleRequest {
            relative_path: PathBuf::from(&r.relative_path),
            expected_sha256: r.sha256.clone(),
        })
        .collect();
    let mut journal = plumbob_core::ops::VecJournal::default();
    let kind = if enable { "mods_enable" } else { "mods_disable" };
    let outcome = ops::set_files_enabled(&roots.mods, &requests, enable, kind, false, &mut journal);
    replay_journal(&guard, journal.0)?;
    db::ops::record_toggle_outcome(guard.conn_mut(), &outcome).map_err(err_str)?;
    db::profiles::sync_active_set(guard.conn_mut()).map_err(err_str)?;
    if !outcome.completed.is_empty() {
        let _ = app.emit("library://changed", "toggle");
    }
    Ok(ToggleOutcomeDto {
        completed: outcome.completed.len(),
        skipped: skipped.len(),
        failed: outcome
            .failed
            .iter()
            .map(|f| FailedStep {
                path: f.source.to_string_lossy().into_owned(),
                message: f.message.clone(),
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// Profile switching
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcomeDto {
    pub disabled_applied: usize,
    pub enabled_applied: usize,
    pub unavailable: Vec<String>,
    pub failed: Vec<FailedStep>,
    pub activated: bool,
}

/// The read-only diff shown before a switch is confirmed.
pub fn preview_switch_profile(
    dbm: &Mutex<Database>,
    target_id: i64,
) -> UiResult<db::profiles::SwitchPlan> {
    let guard = lock_db(dbm)?;
    db::profiles::switch_plan(guard.conn(), target_id).map_err(err_str)
}

/// Guard → plan → verified renames (both directions, best-effort, live
/// progress) → row sync. The target becomes the active profile only when
/// every rename landed; a partial apply reports its failures and leaves
/// the previous profile active so the switch can simply be retried.
pub fn switch_profile(
    app: &AppHandle,
    dbm: &Mutex<Database>,
    data_dir: &Path,
    target_id: i64,
) -> UiResult<SwitchOutcomeDto> {
    ensure_game_closed()?;
    let mut guard = lock_db(dbm)?;
    if let Some(id) =
        db::troubleshoot::active_session_id(guard.conn()).map_err(err_str)?
    {
        return Err(format!(
            "Troubleshooting session #{id} is active. Finish or abort the \
             hunt before switching profiles — both move files and must not \
             overlap."
        ));
    }
    let roots = resolve_roots(&guard, data_dir)?;
    let plan = db::profiles::switch_plan(guard.conn(), target_id).map_err(err_str)?;

    fn to_request(t: &db::profiles::PlannedToggle) -> ops::ToggleRequest {
        ops::ToggleRequest {
            relative_path: PathBuf::from(&t.relative_path),
            expected_sha256: t.sha256.clone(),
        }
    }
    let empty = |op: String| plumbob_core::ops::BatchOutcome::<plumbob_core::ops::ToggleEntry> {
        operation_id: op,
        completed: Vec::new(),
        failed: Vec::new(),
        halted_early: false,
    };

    let mut journal = EmittingJournal::new(app, "profile://progress");
    let disable_reqs: Vec<ops::ToggleRequest> = plan.to_disable.iter().map(to_request).collect();
    let enable_reqs: Vec<ops::ToggleRequest> = plan.to_enable.iter().map(to_request).collect();
    let dis_out = if disable_reqs.is_empty() {
        empty(String::new())
    } else {
        ops::set_files_enabled(&roots.mods, &disable_reqs, false, "profile_switch", false, &mut journal)
    };
    let ena_out = if enable_reqs.is_empty() {
        empty(String::new())
    } else {
        ops::set_files_enabled(&roots.mods, &enable_reqs, true, "profile_switch", false, &mut journal)
    };
    replay_journal(&guard, journal.events)?;
    db::ops::record_toggle_outcome(guard.conn_mut(), &dis_out).map_err(err_str)?;
    db::ops::record_toggle_outcome(guard.conn_mut(), &ena_out).map_err(err_str)?;

    let failed: Vec<FailedStep> = dis_out
        .failed
        .iter()
        .chain(ena_out.failed.iter())
        .map(|f| FailedStep {
            path: f.source.to_string_lossy().into_owned(),
            message: f.message.clone(),
        })
        .collect();
    let activated = failed.is_empty();
    if activated {
        db::profiles::set_active_profile(guard.conn_mut(), target_id).map_err(err_str)?;
        db::profiles::sync_active_set(guard.conn_mut()).map_err(err_str)?;
    }
    if !dis_out.completed.is_empty() || !ena_out.completed.is_empty() {
        let _ = app.emit("library://changed", "profile");
    }
    Ok(SwitchOutcomeDto {
        disabled_applied: dis_out.completed.len(),
        enabled_applied: ena_out.completed.len(),
        unavailable: plan.unavailable,
        failed,
        activated,
    })
}

// ---------------------------------------------------------------------------
// Patch Center — CurseForge update radar
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PatchProgress {
    phase: String,
    done: usize,
    total: usize,
}

fn emit_patch(app: &AppHandle, phase: &str, done: usize, total: usize) {
    let _ = app.emit(
        "patch://progress",
        PatchProgress {
            phase: phase.to_string(),
            done,
            total,
        },
    );
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PatchCheckSummary {
    pub eligible: usize,
    pub newly_fingerprinted: usize,
    /// Every exact fingerprint hit CurseForge returned, before filtering.
    pub raw_matches: usize,
    /// Hits whose mod belongs to another game — fingerprint collisions the
    /// endpoint leaks despite its game-scoped path. Dropped, and counted so
    /// the number is visible instead of mysterious.
    pub other_game: usize,
    pub matched: usize,
    pub updates: usize,
    pub unknown: usize,
    /// Can CurseForge's matcher find a Sims 4 fingerprint it computed
    /// itself? `Some(false)` means their exact-match index doesn't cover
    /// this game; `None` means the probe couldn't run.
    pub corpus_probe: Option<bool>,
    /// Files matched approximately by name (subset of `matched`).
    pub name_matched: usize,
    /// The name tier hit CurseForge's rate limit or a temporary block and
    /// paused; the lookup cache keeps its progress, so running again
    /// continues.
    pub rate_limited: bool,
    /// Terms not yet searched (per-run cap or a pause). Zero when done.
    pub remaining_terms: usize,
    pub checked_at: String,
}

/// Fingerprint what's missing, ask CurseForge who it knows, compare each
/// match to the mod's latest file, cache the snapshot. Read-only toward
/// the library; the database lock is never held across disk or network.
pub fn check_curse_updates(
    app: &AppHandle,
    dbm: &Mutex<Database>,
) -> UiResult<PatchCheckSummary> {
    let (key, pending) = {
        let guard = lock_db(dbm)?;
        let settings = db::settings::load(guard.conn()).map_err(err_str)?;
        let key = settings
            .curseforge_api_key
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                "No CurseForge API key yet — paste one in Settings → \
                 Connections and try again."
                    .to_string()
            })?;
        let pending =
            db::curse::files_needing_fingerprint(guard.conn()).map_err(err_str)?;
        (key, pending)
    };

    // Phase 1: fingerprint the stragglers. Disk work happens unlocked;
    // rows are written back in small batches.
    let total_pending = pending.len();
    let mut newly_fingerprinted = 0usize;
    let mut batch: Vec<(i64, u32)> = Vec::with_capacity(25);
    emit_patch(app, "Fingerprinting", 0, total_pending);
    for (i, (file_id, absolute)) in pending.iter().enumerate() {
        match plumbob_core::curse::curse_fingerprint_file(Path::new(absolute)) {
            Ok(fp) => {
                batch.push((*file_id, fp));
                newly_fingerprinted += 1;
            }
            Err(_) => { /* unreadable right now — it stays pending */ }
        }
        if batch.len() >= 25 || i + 1 == total_pending {
            let guard = lock_db(dbm)?;
            for (id, fp) in batch.drain(..) {
                db::curse::set_fingerprint(guard.conn(), id, fp).map_err(err_str)?;
            }
            emit_patch(app, "Fingerprinting", i + 1, total_pending);
        }
    }

    let pairs = {
        let guard = lock_db(dbm)?;
        db::curse::fingerprint_pairs(guard.conn()).map_err(err_str)?
    };
    let eligible = pairs.len();

    // Phase 2: who does CurseForge recognize?
    let client = crate::curse_api::CurseClient::new(&key)?;
    let game_id = client.find_sims4_game_id()?;
    // Corpus probe: fetch a popular Sims 4 mod and feed CurseForge's own
    // fingerprint for it back into the matcher. Our hash is certified
    // against the ecosystem crate, so this isolates the remaining suspect.
    let corpus_probe: Option<bool> = (|| -> Result<Option<bool>, String> {
        let Some(sample) = client.sample_mod(game_id)? else {
            return Ok(None);
        };
        let Some(cf_file) = sample
            .latest_files
            .iter()
            .find(|f| f.file_fingerprint != 0)
        else {
            return Ok(None);
        };
        let hits =
            client.match_fingerprints(game_id, &[cf_file.file_fingerprint as u32])?;
        Ok(Some(hits.iter().any(|h| h.mod_id == sample.id)))
    })()
    .unwrap_or(None);

    let fingerprints: Vec<u32> = pairs.iter().map(|(fp, _)| *fp).collect();
    let mut matched_files: Vec<crate::curse_api::CurseFile> = Vec::new();
    if corpus_probe != Some(false) {
        let batches: Vec<&[u32]> = fingerprints.chunks(500).collect();
        emit_patch(app, "Matching against CurseForge", 0, batches.len().max(1));
        for (i, chunk) in batches.iter().enumerate() {
            matched_files.extend(client.match_fingerprints(game_id, chunk)?);
            emit_patch(app, "Matching against CurseForge", i + 1, batches.len());
        }
    }

    // Tier-2 — the name radar. Terms are derived per file, deduplicated,
    // and every search (hit or miss) is cached so a rate-limited run
    // resumes instead of restarting.
    let fp_matched_ids: std::collections::HashSet<i64> = {
        let by_fp: std::collections::HashMap<i64, ()> = matched_files
            .iter()
            .map(|f| (f.file_fingerprint, ()))
            .collect();
        pairs
            .iter()
            .filter(|(fp, _)| by_fp.contains_key(&i64::from(*fp)))
            .map(|(_, id)| *id)
            .collect()
    };
    let eligible_rows = {
        let guard = lock_db(dbm)?;
        db::curse::eligible_files(guard.conn()).map_err(err_str)?
    };
    let mut term_files: std::collections::HashMap<String, Vec<(i64, Option<String>)>> =
        std::collections::HashMap::new();
    let mut term_creator: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for f in &eligible_rows {
        if fp_matched_ids.contains(&f.id) {
            continue;
        }
        let anchored = plumbob_core::curse::search_term_with_creator(
            &f.file_name,
            f.creator_display.as_deref(),
        );
        if let Some(term) = anchored {
            term_files
                .entry(term.clone())
                .or_default()
                .push((f.id, f.mtime.clone()));
            if let Some(key) = f.creator.as_deref().filter(|c| !c.is_empty()) {
                term_creator.entry(term).or_insert_with(|| key.to_string());
            }
        }
    }
    let known = {
        let guard = lock_db(dbm)?;
        db::curse::known_lookups(guard.conn()).map_err(err_str)?
    };
    let mut term_hits: std::collections::HashMap<String, i64> = known
        .iter()
        .filter_map(|(t, m)| m.map(|id| (t.clone(), id)))
        .filter(|(t, _)| term_files.contains_key(t))
        .collect();
    let mut fresh_mods: std::collections::HashMap<i64, crate::curse_api::CurseMod> =
        std::collections::HashMap::new();
    let mut rate_limited = false;
    let mut missing: Vec<String> = term_files
        .keys()
        .filter(|t| !known.contains_key(*t))
        .cloned()
        .collect();
    missing.sort();
    // Politeness engineering, field-taught: Cloudflare answers request
    // storms with 403s that outlive the run. Each check paces itself and
    // handles at most this many new terms; the cache carries the rest.
    const TERMS_PER_RUN: usize = 600;
    let deferred = missing.len().saturating_sub(TERMS_PER_RUN);
    missing.truncate(TERMS_PER_RUN);
    emit_patch(app, "Matching by name", 0, missing.len().max(1));
    let mut searched = 0usize;
    for (i, term) in missing.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        match client.search_mods(game_id, term, 5) {
            Ok(candidates) => {
                let best = candidates
                    .into_iter()
                    .filter_map(|m| {
                        let authors: Vec<String> =
                            m.authors.iter().map(|a| a.name.clone()).collect();
                        plumbob_core::curse::accept_name_match_attributed(
                            term,
                            &m.name,
                            &authors,
                            term_creator.get(term).map(String::as_str),
                        )
                        .map(|sim| (sim, m))
                    })
                    .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                let guard = lock_db(dbm)?;
                match best {
                    Some((sim, m)) => {
                        db::curse::upsert_lookup(
                            guard.conn(),
                            term,
                            Some(m.id),
                            Some(&m.name),
                            Some(f64::from(sim)),
                        )
                        .map_err(err_str)?;
                        term_hits.insert(term.clone(), m.id);
                        fresh_mods.insert(m.id, m);
                    }
                    None => {
                        db::curse::upsert_lookup(guard.conn(), term, None, None, None)
                            .map_err(err_str)?;
                    }
                }
            }
            Err(e) if e.contains("rate-limit") || e.contains("blocking requests") => {
                rate_limited = true;
                break;
            }
            Err(e) => return Err(e),
        }
        searched += 1;
        emit_patch(app, "Matching by name", i + 1, missing.len());
    }
    let remaining_terms = deferred + missing.len().saturating_sub(searched);
    let name_confidence: std::collections::HashMap<String, f64> = {
        let guard = lock_db(dbm)?;
        let mut stmt = guard
            .conn()
            .prepare("SELECT term, confidence FROM curse_name_lookups WHERE curse_mod_id IS NOT NULL")
            .map_err(err_str)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))
            .map_err(err_str)?
            .collect::<Result<std::collections::HashMap<_, _>, _>>()
            .map_err(err_str)?;
        rows
    };

    // Phase 3: resolve the matched mods (names, links, latest files).
    let mut mod_ids: Vec<i64> = matched_files.iter().map(|f| f.mod_id).collect();
    mod_ids.extend(term_hits.values().copied());
    mod_ids.sort_unstable();
    mod_ids.dedup();
    let mut mods: std::collections::HashMap<i64, crate::curse_api::CurseMod> =
        std::collections::HashMap::with_capacity(mod_ids.len());
    mods.extend(fresh_mods);
    mod_ids.retain(|id| !mods.contains_key(id));
    let mod_batches: Vec<&[i64]> = mod_ids.chunks(50).collect();
    emit_patch(app, "Resolving mods", 0, mod_batches.len().max(1));
    for (i, chunk) in mod_batches.iter().enumerate() {
        for m in client.get_mods(chunk)? {
            mods.insert(m.id, m);
        }
        emit_patch(app, "Resolving mods", i + 1, mod_batches.len());
    }


    // Phase 4: compare and cache. The fingerprint endpoint leaks matches
    // from other games (a Minecraft jar proved it in the field), so every
    // hit must belong to a Sims 4 mod or it is dropped — and counted.
    let raw_matches = matched_files.len();
    let mut other_game = 0usize;
    matched_files.retain(|f| match mods.get(&f.mod_id) {
        Some(m) if m.game_id == game_id => true,
        Some(_) => {
            other_game += 1;
            false
        }
        None => false,
    });
    let by_fingerprint: std::collections::HashMap<i64, &crate::curse_api::CurseFile> =
        matched_files
            .iter()
            .map(|f| (f.file_fingerprint, f))
            .collect();
    let mut records: Vec<db::curse::MatchRecord> = Vec::new();
    for (fp, file_id) in &pairs {
        let Some(hit) = by_fingerprint.get(&i64::from(*fp)) else {
            continue;
        };
        let Some(mod_info) = mods.get(&hit.mod_id) else {
            continue;
        };
        let latest = mod_info
            .latest_files
            .iter()
            .max_by(|a, b| {
                if plumbob_core::curse::date_newer(&a.file_date, &b.file_date) {
                    std::cmp::Ordering::Less
                } else if plumbob_core::curse::date_newer(&b.file_date, &a.file_date) {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .cloned()
            .unwrap_or_else(|| hit.to_owned().clone());
        records.push(db::curse::MatchRecord {
            file_id: *file_id,
            curse_mod_id: hit.mod_id,
            curse_file_id: Some(hit.id),
            mod_name: mod_info.name.clone(),
            website_url: mod_info.links.website_url.clone(),
            matched_file_name: Some(hit.file_name.clone()),
            matched_file_date: Some(hit.file_date.clone()),
            latest_file_id: latest.id,
            latest_file_name: latest.file_name.clone(),
            latest_file_date: latest.file_date.clone(),
            update_available: plumbob_core::curse::update_available(
                hit.id,
                &hit.file_date,
                latest.id,
                &latest.file_date,
            ),
            match_kind: "fingerprint",
            allow_distribution: mod_info.allow_mod_distribution,
            confidence: None,
        });
    }
    let mut name_matched = 0usize;
    for (term, files) in &term_files {
        let Some(mod_id) = term_hits.get(term) else { continue };
        let Some(mod_info) = mods.get(mod_id) else { continue };
        let Some(latest) = mod_info.latest_files.iter().max_by(|a, b| {
            if plumbob_core::curse::date_newer(&a.file_date, &b.file_date) {
                std::cmp::Ordering::Less
            } else if plumbob_core::curse::date_newer(&b.file_date, &a.file_date) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        }) else {
            continue;
        };
        for (file_id, mtime) in files {
            name_matched += 1;
            records.push(db::curse::MatchRecord {
                file_id: *file_id,
                curse_mod_id: *mod_id,
                curse_file_id: None,
                mod_name: mod_info.name.clone(),
                website_url: mod_info.links.website_url.clone(),
                matched_file_name: None,
                matched_file_date: None,
                latest_file_id: latest.id,
                latest_file_name: latest.file_name.clone(),
                latest_file_date: latest.file_date.clone(),
                update_available: mtime
                    .as_deref()
                    .map(|m| plumbob_core::curse::date_newer(m, &latest.file_date))
                    .unwrap_or(false),
                match_kind: "name",
                allow_distribution: mod_info.allow_mod_distribution,
                confidence: name_confidence.get(term).copied(),
            });
        }
    }
    let matched = records.len();
    let updates = records.iter().filter(|r| r.update_available).count();
    let checked_at = {
        let mut guard = lock_db(dbm)?;
        db::curse::replace_matches(guard.conn_mut(), &records).map_err(err_str)?
    };
    emit_patch(app, "Done", 1, 1);
    Ok(PatchCheckSummary {
        eligible,
        newly_fingerprinted,
        raw_matches,
        other_game,
        matched,
        updates,
        unknown: eligible.saturating_sub(matched),
        corpus_probe,
        name_matched,
        rate_limited,
        remaining_terms,
        checked_at,
    })
}

pub fn curse_status(
    dbm: &Mutex<Database>,
) -> UiResult<Vec<db::curse::CurseStatusRow>> {
    let guard = lock_db(dbm)?;
    db::curse::status(guard.conn()).map_err(err_str)
}

// ---------------------------------------------------------------------------
// Thumbnails
// ---------------------------------------------------------------------------

/// Bump when decoders change: markers from older generations are stale
/// verdicts and get retried. g3 = DST unshuffle.
const THUMB_GEN: u32 = 3;

fn stale_markers(cache: &Path, id: i64) {
    let _ = std::fs::remove_file(cache.join(format!("{id}.none")));
    let _ = std::fs::remove_file(cache.join(format!("{id}.none2")));
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ThumbDto {
    pub file_id: i64,
    /// Absolute path into the thumbnail cache, or None when the package
    /// carries no extractable image.
    pub path: Option<String>,
}

/// Disk-cached, lazily extracted in-game thumbnails. Rows are fetched
/// under one short lock; all parsing and decompression happens unlocked.
/// The filesystem is the cache: `{id}.png` / `{id}.jpg`, with `{id}.none`
/// remembering packages that yielded nothing so they aren't re-parsed on
/// every visit.
pub fn thumbnails(
    dbm: &Mutex<Database>,
    data_dir: &Path,
    file_ids: &[i64],
) -> UiResult<Vec<ThumbDto>> {
    let cache = data_dir.join("Thumbnails");
    std::fs::create_dir_all(&cache).map_err(err_str)?;
    let rows = {
        let guard = lock_db(dbm)?;
        db::files::files_by_ids(guard.conn(), file_ids).map_err(err_str)?
    };
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let png = cache.join(format!("{}.png", row.id));
        let jpg = cache.join(format!("{}.jpg", row.id));
        let none = cache.join(format!("{}.none.g{THUMB_GEN}", row.id));
        stale_markers(&cache, row.id);
        let cached = if png.exists() {
            Some(png)
        } else if jpg.exists() {
            Some(jpg)
        } else if none.exists() {
            None
        } else if row.missing || row.file_type != "package" {
            None
        } else {
            match plumbob_core::dbpf::extract_thumbnail(Path::new(&row.absolute_path)) {
                Ok(Some((bytes, ext))) => {
                    let target = cache.join(format!("{}.{ext}", row.id));
                    if std::fs::write(&target, &bytes).is_ok() {
                        Some(target)
                    } else {
                        None
                    }
                }
                // Parsed fine, genuinely no image → remember that forever.
                Ok(None) => {
                    let _ = std::fs::write(&none, b"");
                    None
                }
                // IO/corruption — worth retrying another day, no marker.
                Err(_) => None,
            }
        };
        out.push(ThumbDto {
            file_id: row.id,
            path: cached.map(|p| p.to_string_lossy().into_owned()),
        });
    }
    Ok(out)
}

/// Walk every package and fill the thumbnail cache ahead of time, so the
/// gallery never waits. Read-only toward the library; emits
/// `thumbs://progress` as it goes and returns how many images it made.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PrepareOutcome {
    pub generated: usize,
    pub cached: usize,
    pub no_image: usize,
}

pub fn prepare_thumbnails(
    app: &AppHandle,
    dbm: &Mutex<Database>,
    data_dir: &Path,
) -> UiResult<PrepareOutcome> {
    let cache = data_dir.join("Thumbnails");
    std::fs::create_dir_all(&cache).map_err(err_str)?;
    let work = {
        let guard = lock_db(dbm)?;
        db::files::package_paths(guard.conn()).map_err(err_str)?
    };
    let total = work.len();
    let mut generated = 0usize;
    let mut cached = 0usize;
    let mut no_image = 0usize;
    for (i, (id, absolute)) in work.iter().enumerate() {
        let png = cache.join(format!("{id}.png"));
        let jpg = cache.join(format!("{id}.jpg"));
        let none = cache.join(format!("{id}.none.g{THUMB_GEN}"));
        stale_markers(&cache, *id);
        if png.exists() || jpg.exists() {
            cached += 1;
        } else if none.exists() {
            no_image += 1;
        }
        if !(png.exists() || jpg.exists() || none.exists()) {
            match plumbob_core::dbpf::extract_thumbnail(Path::new(absolute)) {
                Ok(Some((bytes, ext))) => {
                    let target = cache.join(format!("{id}.{ext}"));
                    if std::fs::write(target, bytes).is_ok() {
                        generated += 1;
                    }
                }
                Ok(None) => {
                    let _ = std::fs::write(&none, b"");
                    no_image += 1;
                }
                Err(_) => {}
            }
        }
        if i % 8 == 0 || i + 1 == total {
            let _ = app.emit(
                "thumbs://progress",
                serde_json::json!({ "done": i + 1, "total": total }),
            );
        }
    }
    Ok(PrepareOutcome {
        generated,
        cached,
        no_image,
    })
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CensusRow {
    pub type_hex: String,
    pub name: String,
    pub files: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CasProbe {
    pub versions: Vec<String>,
    pub verdict: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CensusReport {
    pub rows: Vec<CensusRow>,
    pub cas_probe: CasProbe,
}

/// What do the packages *without* extractable thumbnails actually
/// contain — plus a CAS probe: the versions seen in real CASP payloads
/// and the calibration verdict, so subcategory failures diagnose
/// themselves from the same card.
pub fn thumbnail_census(
    dbm: &Mutex<Database>,
    data_dir: &Path,
) -> UiResult<CensusReport> {
    let cache = data_dir.join("Thumbnails");
    let work = {
        let guard = lock_db(dbm)?;
        db::files::package_paths(guard.conn()).map_err(err_str)?
    };
    let blanks: Vec<i64> = work
        .iter()
        .filter(|(id, _)| {
            !cache.join(format!("{id}.png")).exists()
                && !cache.join(format!("{id}.jpg")).exists()
        })
        .map(|(id, _)| *id)
        .collect();
    let census = {
        let guard = lock_db(dbm)?;
        db::packages::resource_type_census(guard.conn(), &blanks).map_err(err_str)?
    };
    let rows = census
        .into_iter()
        .map(|(type_id, files)| CensusRow {
            type_hex: format!("0x{type_id:08X}"),
            name: plumbob_core::dbpf::resource_type_name(type_id)
                .unwrap_or("Unknown")
                .to_string(),
            files,
        })
        .collect();

    let cas = {
        let guard = lock_db(dbm)?;
        db::files::cas_needing_subcategory(guard.conn()).map_err(err_str)?
    };
    let mut payloads: Vec<Vec<Vec<u8>>> = Vec::new();
    for (_, absolute) in cas.iter().take(400) {
        if let Ok(sibs) = plumbob_core::dbpf::read_casp_payloads(Path::new(absolute), 3) {
            if !sibs.is_empty() {
                payloads.push(sibs);
            }
        }
    }
    let mut version_counts: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    for sibs in &payloads {
        let p = &sibs[0];
        if p.len() >= 4 {
            let v = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
            *version_counts.entry(v).or_insert(0) += 1;
        }
    }
    let mut versions: Vec<(u32, usize)> = version_counts.into_iter().collect();
    versions.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let versions: Vec<String> = versions
        .into_iter()
        .take(6)
        .map(|(v, n)| format!("0x{v:02X}×{n}"))
        .collect();
    let verdict = if cas.is_empty() {
        "all CAS parts classified".to_string()
    } else {
        let mut total: std::collections::HashMap<u32, usize> = Default::default();
        let mut parsed: std::collections::HashMap<u32, usize> = Default::default();
        for sibs in &payloads {
            let p = &sibs[0];
            if p.len() < 4 {
                continue;
            }
            let v = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
            *total.entry(v).or_insert(0) += 1;
            let refs: Vec<&[u8]> = sibs.iter().map(|q| q.as_slice()).collect();
            if plumbob_core::casp::body_type_checked(&refs).is_some() {
                *parsed.entry(v).or_insert(0) += 1;
            }
        }
        let mut order: Vec<u32> = total.keys().copied().collect();
        order.sort_by_key(|v| std::cmp::Reverse(total[v]));
        let parts: Vec<String> = order
            .into_iter()
            .map(|v| {
                let t = total[&v];
                let p = parsed.get(&v).copied().unwrap_or(0);
                format!("0x{v:02X}→ref {:.0}%", p as f32 * 100.0 / t as f32)
            })
            .collect();
        if parts.is_empty() {
            "no readable CASP payloads".to_string()
        } else {
            parts.join(" · ")
        }
    };
    Ok(CensusReport {
        rows,
        cas_probe: CasProbe { versions, verdict },
    })
}

/// Classify CAS subcategories with a scheme elected from the library
/// itself: sample real CASP payloads, let calibration find the BodyType
/// column, then read every pending part with the winning scheme. If no
/// scheme earns election, nothing is written — unlabeled beats wrong.
pub fn classify_cas_subtypes(dbm: &Mutex<Database>) -> UiResult<usize> {
    let pending = {
        let guard = lock_db(dbm)?;
        db::files::cas_needing_subcategory(guard.conn()).map_err(err_str)?
    };
    if pending.is_empty() {
        return Ok(0);
    }
    let mut payloads: Vec<(i64, Vec<Vec<u8>>)> = Vec::new();
    for (id, absolute) in &pending {
        if let Ok(sibs) = plumbob_core::dbpf::read_casp_payloads(Path::new(absolute), 3) {
            if !sibs.is_empty() {
                payloads.push((*id, sibs));
            }
        }
    }
    let mut done = 0usize;
    for chunk in payloads.chunks(50) {
        let guard = lock_db(dbm)?;
        for (id, sibs) in chunk {
            let refs: Vec<&[u8]> = sibs.iter().map(|p| p.as_slice()).collect();
            if let Some(bt) = plumbob_core::casp::body_type_checked(&refs) {
                let sub = plumbob_core::casp::subcategory_for(bt);
                db::files::set_cas_subcategory(guard.conn(), *id, sub).map_err(err_str)?;
                done += 1;
            }
        }
    }
    Ok(done)
}

/// Read creator identity from every current filename: strong conventions
/// credit alone, lowercase prefixes need three files (frequency
/// promotion computed across the whole library each scan). Writes only
/// rows still pending; '' marks examined-and-uncredited.
pub fn classify_creators(dbm: &Mutex<Database>) -> UiResult<usize> {
    let worklist = {
        let guard = lock_db(dbm)?;
        db::files::creator_worklist(guard.conn()).map_err(err_str)?
    };
    if worklist.iter().all(|(_, _, pending)| !pending) {
        return Ok(0);
    }
    let candidates: Vec<(i64, Option<plumbob_core::creators::Candidate>)> = worklist
        .iter()
        .map(|(id, name, _)| (*id, plumbob_core::creators::candidate(name)))
        .collect();
    let resolved = plumbob_core::creators::resolve(&candidates);
    let pending: std::collections::HashSet<i64> = worklist
        .iter()
        .filter(|(_, _, p)| *p)
        .map(|(id, _, _)| *id)
        .collect();
    let mut done = 0usize;
    for chunk in resolved.chunks(100) {
        let guard = lock_db(dbm)?;
        for (id, credit) in chunk {
            if !pending.contains(id) {
                continue;
            }
            let (key, display) = credit
                .as_ref()
                .map(|(k, d)| (k.as_str(), d.as_str()))
                .unwrap_or(("", ""));
            db::files::set_creator(guard.conn(), *id, key, display).map_err(err_str)?;
            done += 1;
        }
    }
    Ok(done)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReverifyOutcome {
    pub examined: usize,
    pub kept: usize,
    pub boosted: usize,
    pub dropped: usize,
}

/// The final pass: re-judge every cached name-match under the attributed
/// standards. Mods are re-fetched in bulk (authors weren't cached);
/// verdicts keep (confidence refreshed), boost (author-confirmed), or
/// drop (rows deleted, lookup nulled so a future Check may re-search).
pub fn reverify_matches(app: &AppHandle, dbm: &Mutex<Database>) -> UiResult<ReverifyOutcome> {
    let (key, lookups) = {
        let guard = lock_db(dbm)?;
        let settings = db::settings::load(guard.conn()).map_err(err_str)?;
        let key = settings
            .curseforge_api_key
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                "No CurseForge API key yet — paste one in Settings → \
                 Connections and try again."
                    .to_string()
            })?;
        let lookups = db::curse::name_lookup_rows(guard.conn()).map_err(err_str)?;
        (key, lookups)
    };
    if lookups.is_empty() {
        return Ok(ReverifyOutcome { examined: 0, kept: 0, boosted: 0, dropped: 0 });
    }
    let client = crate::curse_api::CurseClient::new(&key)?;
    let mut ids: Vec<i64> = lookups.iter().map(|(_, id, _)| *id).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut mods: std::collections::HashMap<i64, crate::curse_api::CurseMod> =
        std::collections::HashMap::new();
    for chunk in ids.chunks(50) {
        for m in client.get_mods(chunk)? {
            mods.insert(m.id, m);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    // The same term construction the Check uses, so cached terms line up.
    let eligible_rows = {
        let guard = lock_db(dbm)?;
        db::curse::eligible_files(guard.conn()).map_err(err_str)?
    };
    let mut term_files: std::collections::HashMap<String, Vec<i64>> = Default::default();
    let mut term_creator: std::collections::HashMap<String, String> = Default::default();
    for f in &eligible_rows {
        if let Some(term) = plumbob_core::curse::search_term_with_creator(
            &f.file_name,
            f.creator_display.as_deref(),
        ) {
            term_files.entry(term.clone()).or_default().push(f.id);
            if let Some(key) = f.creator.as_deref().filter(|c| !c.is_empty()) {
                term_creator.entry(term).or_insert_with(|| key.to_string());
            }
        }
    }

    let total = lookups.len();
    let mut out = ReverifyOutcome { examined: total, kept: 0, boosted: 0, dropped: 0 };
    for (i, (term, mod_id, old_conf)) in lookups.iter().enumerate() {
        let files: &[i64] = term_files.get(term).map(|v| v.as_slice()).unwrap_or(&[]);
        let verdict = mods.get(mod_id).and_then(|m| {
            let authors: Vec<String> = m.authors.iter().map(|a| a.name.clone()).collect();
            plumbob_core::curse::accept_name_match_attributed(
                term,
                &m.name,
                &authors,
                term_creator.get(term).map(String::as_str),
            )
        });
        let guard = lock_db(dbm)?;
        match verdict {
            Some(conf) => {
                let mod_name = mods.get(mod_id).map(|m| m.name.as_str()).unwrap_or("");
                db::curse::upsert_lookup(
                    guard.conn(),
                    term,
                    Some(*mod_id),
                    Some(mod_name),
                    Some(f64::from(conf)),
                )
                .map_err(err_str)?;
                db::curse::update_name_confidence(guard.conn(), files, *mod_id, f64::from(conf))
                    .map_err(err_str)?;
                out.kept += 1;
                if f64::from(conf) > old_conf.unwrap_or(0.0) + 0.01 {
                    out.boosted += 1;
                }
            }
            None => {
                db::curse::delete_name_matches(guard.conn(), files, *mod_id).map_err(err_str)?;
                db::curse::null_lookup(guard.conn(), term).map_err(err_str)?;
                out.dropped += 1;
            }
        }
        drop(guard);
        if i % 20 == 0 || i + 1 == total {
            emit_patch(app, "Re-verifying matches", i + 1, total);
        }
    }
    Ok(out)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOutcome {
    pub file_id: i64,
    pub bytes: usize,
    pub file_name: String,
    /// Files the new contents share resources with — the heads-up an
    /// update owes you before Conflicts delivers it as a surprise.
    pub overlaps: Vec<String>,
}

fn stem_of(name: &str) -> String {
    let lower = name.to_lowercase();
    let base = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    base.trim_end_matches(".package")
        .trim_end_matches(".ts4script")
        .trim_end_matches(".zip")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// The bytes to install, whether the release is a bare file or a zip.
/// Zips: junk entries are ignored; exactly one usable entry wins, or the
/// one whose name matches ours — genuine ambiguity is an honest wall.
fn select_release_bytes(
    downloaded: Vec<u8>,
    latest_name: &str,
    our_name: &str,
    expect_ext: &str,
) -> Result<Vec<u8>, String> {
    let lower = latest_name.to_lowercase();
    if lower.ends_with(".package") || lower.ends_with(".ts4script") {
        if !lower.ends_with(expect_ext) {
            return Err(format!(
                "The latest release is a {} but this file is a {expect_ext} — \
                 not swapping across types.",
                if lower.ends_with(".package") { ".package" } else { ".ts4script" }
            ));
        }
        return Ok(downloaded);
    }
    if !lower.ends_with(".zip") {
        return Err(format!(
            "The latest release is \"{latest_name}\" — a format the updater \
             doesn't handle. Open Mod to fetch it yourself."
        ));
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(downloaded))
        .map_err(|e| format!("Could not open the release archive: {e}"))?;
    let mut candidates: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("Archive entry unreadable: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let lname = name.to_lowercase();
        if lname.contains("__macosx") || lname.rsplit('/').next().map_or(false, |b| b.starts_with('.')) {
            continue;
        }
        if lname.ends_with(expect_ext) {
            candidates.push((i, name));
        }
    }
    let chosen = match candidates.len() {
        0 => {
            return Err(format!(
                "The archive contains no {expect_ext} files — Open Mod to see \
                 what the author shipped."
            ))
        }
        1 => candidates[0].0,
        _ => {
            let ours = stem_of(our_name);
            let matches: Vec<&(usize, String)> = candidates
                .iter()
                .filter(|(_, n)| {
                    let s = stem_of(n);
                    s == ours || s.contains(&ours) || ours.contains(&s)
                })
                .collect();
            match matches.len() {
                1 => matches[0].0,
                _ => {
                    return Err(format!(
                        "The archive holds {} {expect_ext} files and none of \
                         them clearly matches yours — Open Mod to choose for \
                         yourself.",
                        candidates.len()
                    ))
                }
            }
        }
    };
    let mut entry = archive
        .by_index(chosen)
        .map_err(|e| format!("Archive entry unreadable: {e}"))?;
    let mut out = Vec::with_capacity(entry.size() as usize);
    std::io::Read::read_to_end(&mut entry, &mut out)
        .map_err(|e| format!("Could not extract from the archive: {e}"))?;
    Ok(out)
}

/// Apply one update: download the latest release, verify it looks like
/// what it claims to be, snapshot the current copy through the same
/// journaled backup machinery quarantine uses, then swap atomically.
/// The old filename is kept — contents change, identity doesn't — so
/// every attribution and setting on the row survives. The next scan
/// re-fingerprints (the hash is cleared on purpose).
pub fn apply_update(dbm: &Mutex<Database>, data_dir: &Path, file_id: i64) -> UiResult<UpdateOutcome> {
    let (key, paths, m) = {
        let guard = lock_db(dbm)?;
        let settings = db::settings::load(guard.conn()).map_err(err_str)?;
        let key = settings
            .curseforge_api_key
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                "No CurseForge API key yet — paste one in Settings → \
                 Connections and try again."
                    .to_string()
            })?;
        let paths = db::files::file_paths(guard.conn(), file_id)
            .map_err(err_str)?
            .ok_or_else(|| "That file is no longer in the library.".to_string())?;
        let m = db::curse::match_for_file(guard.conn(), file_id)
            .map_err(err_str)?
            .ok_or_else(|| "No CurseForge match recorded for that file.".to_string())?;
        (key, paths, m)
    };
    let (absolute, relative, _name) = paths;
    let (mod_id, latest_file_id, latest_file_name, _latest_date, update_available) = m;
    if !update_available {
        return Err("That file is already up to date.".to_string());
    }
    let our_lower = absolute.to_lowercase();
    let expects_package = our_lower.ends_with(".package");
    let expect_ext = if expects_package { ".package" } else { ".ts4script" };

    let client = crate::curse_api::CurseClient::new(&key)?;
    let detail = client.get_file(mod_id, latest_file_id)?;
    let url = detail.download_url.ok_or_else(|| {
        "This author hasn't enabled third-party downloads on CurseForge — \
         use Open Mod to update by hand."
            .to_string()
    })?;
    let downloaded = client.download(&url, 500 * 1024 * 1024)?;
    let bytes = select_release_bytes(
        downloaded,
        &latest_file_name,
        &absolute,
        expect_ext,
    )?;
    let looks_right = if expects_package {
        bytes.len() >= 4 && &bytes[..4] == b"DBPF"
    } else {
        bytes.len() >= 4 && &bytes[..4] == b"PK\x03\x04"
    };
    if !looks_right {
        return Err(
            "The downloaded bytes don't look like the file type they claim — \
             nothing was changed."
                .to_string(),
        );
    }

    {
        let guard = lock_db(dbm)?;
        let roots = resolve_roots(&guard, data_dir)?;
        let mut journal = db::ops::SqliteJournal::new(guard.conn());
        ops::create_snapshot(
            &roots.mods,
            &roots.backups,
            &[std::path::PathBuf::from(&relative)],
            &format!("Automatic backup before update ({latest_file_name})"),
            &mut journal,
        )
        .map_err(err_str)?;
        journal.finish().map_err(err_str)?;
    }

    let target = Path::new(&absolute);
    let tmp = target.with_extension("mmnew");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("Could not stage the new file: {e}"))?;
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Could not swap the file into place: {e}")
    })?;

    // Post-update truth: the row's hash and size reflect what's on disk,
    // the resource index reflects the new contents, and any overlap with
    // siblings is reported here instead of ambushing you in Conflicts.
    let sha = plumbob_core::hashing::sha256_bytes(&bytes);
    let mut overlaps: Vec<String> = Vec::new();
    {
        let guard = lock_db(dbm)?;
        let mtime = std::fs::metadata(target)
            .and_then(|m| m.modified())
            .map(chrono::DateTime::<chrono::Utc>::from)
            .unwrap_or_else(|_| chrono::Utc::now());
        db::curse::mark_updated(guard.conn(), file_id, &sha, bytes.len() as i64, mtime)
            .map_err(err_str)?;
        if expects_package {
            if let Ok(index) = plumbob_core::dbpf::read_package_index(target) {
                let keys: Vec<(u32, u32, u64)> = index
                    .keys
                    .iter()
                    .map(|k| (k.type_id, k.group_id, k.instance))
                    .collect();
                db::packages::refresh_file_resources(guard.conn(), file_id, &keys)
                    .map_err(err_str)?;
                overlaps = db::packages::overlapping_files(guard.conn(), file_id)
                    .map_err(err_str)?
                    .into_iter()
                    .map(|(name, shared)| format!("{name} ({shared} shared)"))
                    .collect();
            }
        }
    }
    Ok(UpdateOutcome {
        file_id,
        bytes: bytes.len(),
        file_name: latest_file_name,
        overlaps,
    })
}
