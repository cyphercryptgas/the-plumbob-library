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
        db::files::reconcile_scan(guard.conn_mut(), &report, scan_type, &opts.excluded_relative)
            .map_err(err_str)?
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
        let facts = db::dupes::load_file_facts(guard.conn()).map_err(err_str)?;
        let groups = duplicates::group_exact(&facts);
        // The insert count excludes fingerprints the user has dismissed, so
        // the reported number matches what the Duplicate Center will show.
        db::dupes::replace_exact_groups(guard.conn_mut(), &groups).map_err(err_str)?
    };

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
    events: Vec<plumbob_core::ops::JournalEvent>,
    total: usize,
    done: usize,
    last_emit: std::time::Instant,
}

impl<'a> EmittingJournal<'a> {
    fn new(app: &'a AppHandle) -> Self {
        Self {
            app,
            events: Vec::new(),
            total: 0,
            done: 0,
            last_emit: std::time::Instant::now(),
        }
    }
    fn emit(&mut self, force: bool) {
        if force || self.last_emit.elapsed().as_millis() >= 150 {
            let _ = self.app.emit(
                "troubleshoot://progress",
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
    let mut journal = EmittingJournal::new(app);
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
    let mut journal = EmittingJournal::new(app);
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
