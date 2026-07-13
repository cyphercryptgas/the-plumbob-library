//! Mutating engines: verified moves, quarantine, snapshots, restore, and the
//! operation journal.
//!
//! Every function here follows the same contract: validate containment →
//! verify preconditions → act → verify content hashes → report to the
//! journal. Destinations are never overwritten. Partial failures are recorded
//! precisely and, by default, halt the remaining plan. A snapshot either
//! completes fully or removes itself — a partial backup that *looks* whole is
//! more dangerous than no backup at all.

use crate::hashing::sha256_file;
use crate::paths::{collision_free, PathError, SafeRoot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error("source is missing or not a regular file: {0}")]
    SourceMissing(PathBuf),
    #[error("destination already occupied: {0}")]
    DestinationOccupied(PathBuf),
    #[error("io failure at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("content verification failed for {path}: expected {expected}, found {found}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        found: String,
    },
}

// ---------------------------------------------------------------------------
// Operation journal
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum JournalEvent {
    OperationStarted {
        operation_id: String,
        kind: String,
        total_steps: usize,
    },
    StepSucceeded {
        operation_id: String,
        step: usize,
        action: String,
        source: PathBuf,
        destination: Option<PathBuf>,
        sha256: Option<String>,
    },
    StepFailed {
        operation_id: String,
        step: usize,
        action: String,
        source: PathBuf,
        message: String,
    },
    OperationFinished {
        operation_id: String,
        status: String,
        succeeded: usize,
        failed: usize,
    },
}

/// Receives journal events as they happen. The SQLite layer implements this
/// to persist an operation record with its steps; tests use [`VecJournal`].
pub trait JournalSink {
    fn record(&mut self, event: JournalEvent);
}

/// In-memory journal for tests and for callers that batch events into the
/// database after the filesystem work completes.
#[derive(Default)]
pub struct VecJournal(pub Vec<JournalEvent>);

impl JournalSink for VecJournal {
    fn record(&mut self, event: JournalEvent) {
        self.0.push(event);
    }
}

static OP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Machine-locally unique, time-sortable operation identifier
/// (`op-20260710T184530123Z-1a2b-0003`). Deliberately not a UUID: these IDs
/// name quarantine/backup folders and correlate journal rows on one machine.
/// They never need global uniqueness, and skipping a randomness dependency
/// keeps the safety core's dependency graph small and auditable.
pub fn new_operation_id() -> String {
    let now = chrono::Utc::now();
    let n = OP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!(
        "op-{}-{:04x}-{:04x}",
        now.format("%Y%m%dT%H%M%S%3fZ"),
        std::process::id() & 0xffff,
        n & 0xffff
    )
}

fn today_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

// ---------------------------------------------------------------------------
// Verified move
// ---------------------------------------------------------------------------

/// Move one file with post-move content verification. Falls back to
/// copy → verify → delete when rename fails (e.g. across filesystems).
///
/// * The destination is never overwritten; collisions are the caller's
///   decision (see [`collision_free`]).
/// * When `expected_sha256` is provided (e.g. from a database record) and the
///   file on disk no longer matches, the move is rolled back and the call
///   fails — this is how stale plans are caught before they do damage.
///
/// Returns the verified hash of the file at its destination.
pub fn verified_move(
    source: &Path,
    destination: &Path,
    expected_sha256: Option<&str>,
) -> Result<String, OpError> {
    if !source.is_file() {
        return Err(OpError::SourceMissing(source.to_path_buf()));
    }
    if destination.exists() {
        return Err(OpError::DestinationOccupied(destination.to_path_buf()));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| OpError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let expected = match expected_sha256 {
        Some(h) => h.to_ascii_lowercase(),
        None => sha256_file(source).map_err(|e| OpError::Io {
            path: source.to_path_buf(),
            source: e,
        })?,
    };

    match std::fs::rename(source, destination) {
        Ok(()) => {
            let found = sha256_file(destination).map_err(|e| OpError::Io {
                path: destination.to_path_buf(),
                source: e,
            })?;
            if found != expected {
                // Roll the rename back: the library must never hold a file it
                // cannot vouch for under a path the database believes in.
                let _ = std::fs::rename(destination, source);
                return Err(OpError::HashMismatch {
                    path: destination.to_path_buf(),
                    expected,
                    found,
                });
            }
            Ok(found)
        }
        Err(_) => {
            // Cross-device or transient rename failure → copy, verify, delete.
            std::fs::copy(source, destination).map_err(|e| OpError::Io {
                path: destination.to_path_buf(),
                source: e,
            })?;
            let found = sha256_file(destination).map_err(|e| OpError::Io {
                path: destination.to_path_buf(),
                source: e,
            })?;
            if found != expected {
                let _ = std::fs::remove_file(destination);
                return Err(OpError::HashMismatch {
                    path: destination.to_path_buf(),
                    expected,
                    found,
                });
            }
            std::fs::remove_file(source).map_err(|e| OpError::Io {
                path: source.to_path_buf(),
                source: e,
            })?;
            Ok(found)
        }
    }
}

// ---------------------------------------------------------------------------
// Enable / disable (in-place rename)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct ToggleRequest {
    /// The file's *logical* path relative to the Mods root (no `.off`).
    pub relative_path: PathBuf,
    /// When provided, the file must still match this hash or the step fails.
    pub expected_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToggleEntry {
    pub relative_path: PathBuf,
    /// The on-disk path after the toggle.
    pub physical_absolute: PathBuf,
    pub sha256: String,
    pub enabled: bool,
}

/// Enable or disable mods by verified in-place rename: `X.package` ⇄
/// `X.package.off`. The file never leaves its folder; the game simply stops
/// (or starts) seeing it. Both directions refuse if the destination name is
/// already occupied — silently replacing a file is exactly what this app
/// exists to prevent.
pub fn set_files_enabled(
    mods: &SafeRoot,
    requests: &[ToggleRequest],
    enable: bool,
    kind: &str,
    stop_on_error: bool,
    journal: &mut dyn JournalSink,
) -> BatchOutcome<ToggleEntry> {
    let operation_id = new_operation_id();
    journal.record(JournalEvent::OperationStarted {
        operation_id: operation_id.clone(),
        kind: kind.into(),
        total_steps: requests.len(),
    });

    let mut completed: Vec<ToggleEntry> = Vec::new();
    let mut failed: Vec<StepFailure> = Vec::new();
    let mut halted_early = false;

    for (i, req) in requests.iter().enumerate() {
        let step = i + 1;
        let result = (|| -> Result<ToggleEntry, OpError> {
            let logical = mods.resolve_relative(&req.relative_path)?;
            let disabled = {
                let mut name = logical
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                name.push_str(crate::scan::DISABLED_SUFFIX);
                logical.with_file_name(name)
            };
            let (source, destination) = if enable {
                (disabled, logical)
            } else {
                (logical, disabled)
            };
            if destination.exists() {
                return Err(OpError::Io {
                    path: destination,
                    source: std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "a file already occupies the target name — resolve it \
                         manually before toggling",
                    ),
                });
            }
            let sha256 = verified_move(&source, &destination, req.expected_sha256.as_deref())?;
            Ok(ToggleEntry {
                relative_path: req.relative_path.clone(),
                physical_absolute: destination,
                sha256,
                enabled: enable,
            })
        })();

        match result {
            Ok(entry) => {
                journal.record(JournalEvent::StepSucceeded {
                    operation_id: operation_id.clone(),
                    step,
                    action: if enable { "enable" } else { "disable" }.into(),
                    source: mods
                        .resolve_relative(&req.relative_path)
                        .unwrap_or_else(|_| req.relative_path.clone()),
                    destination: Some(entry.physical_absolute.clone()),
                    sha256: Some(entry.sha256.clone()),
                });
                completed.push(entry);
            }
            Err(e) => {
                journal.record(JournalEvent::StepFailed {
                    operation_id: operation_id.clone(),
                    step,
                    action: if enable { "enable" } else { "disable" }.into(),
                    source: req.relative_path.clone(),
                    message: e.to_string(),
                });
                failed.push(StepFailure {
                    source: req.relative_path.clone(),
                    message: e.to_string(),
                });
                if stop_on_error {
                    halted_early = true;
                    break;
                }
            }
        }
    }

    journal.record(JournalEvent::OperationFinished {
        operation_id: operation_id.clone(),
        status: if failed.is_empty() { "completed" } else { "failed" }.into(),
        succeeded: completed.len(),
        failed: failed.len(),
    });
    BatchOutcome {
        operation_id,
        completed,
        failed,
        halted_early,
    }
}

// ---------------------------------------------------------------------------
// Quarantine
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct QuarantineRequest {
    /// Path relative to the Mods root.
    pub source_relative: PathBuf,
    pub reason: String,
    /// When provided, the file must still match this hash or the step fails.
    pub expected_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct QuarantineEntry {
    pub original_relative: PathBuf,
    pub stored_absolute: PathBuf,
    pub sha256: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct StepFailure {
    pub source: PathBuf,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct BatchOutcome<T> {
    pub operation_id: String,
    pub completed: Vec<T>,
    pub failed: Vec<StepFailure>,
    pub halted_early: bool,
}

/// Move the requested files out of the Mods root into
/// `<quarantine_root>/<YYYY-MM-DD>/<operation-id>/<original-relative-path>`,
/// preserving relative structure so restoration is unambiguous.
pub fn quarantine_files(
    mods: &SafeRoot,
    quarantine_root: &SafeRoot,
    requests: &[QuarantineRequest],
    stop_on_error: bool,
    journal: &mut dyn JournalSink,
) -> BatchOutcome<QuarantineEntry> {
    let operation_id = new_operation_id();
    let date = today_utc();
    journal.record(JournalEvent::OperationStarted {
        operation_id: operation_id.clone(),
        kind: "quarantine".into(),
        total_steps: requests.len(),
    });

    let mut completed: Vec<QuarantineEntry> = Vec::new();
    let mut failed: Vec<StepFailure> = Vec::new();
    let mut halted_early = false;

    for (i, req) in requests.iter().enumerate() {
        let step = i + 1;
        let result = (|| -> Result<QuarantineEntry, OpError> {
            let source_abs = mods.resolve_relative(&req.source_relative)?;
            let planned = quarantine_root
                .path()
                .join(&date)
                .join(&operation_id)
                .join(&req.source_relative);
            // Belt and suspenders: the planned destination must itself be
            // contained, even though we just built it from trusted parts.
            quarantine_root.contain(&planned)?;
            let destination = collision_free(&planned);
            let sha256 = verified_move(&source_abs, &destination, req.expected_sha256.as_deref())?;
            Ok(QuarantineEntry {
                original_relative: req.source_relative.clone(),
                stored_absolute: destination,
                sha256,
                reason: req.reason.clone(),
            })
        })();

        match result {
            Ok(entry) => {
                journal.record(JournalEvent::StepSucceeded {
                    operation_id: operation_id.clone(),
                    step,
                    action: "quarantine_move".into(),
                    source: req.source_relative.clone(),
                    destination: Some(entry.stored_absolute.clone()),
                    sha256: Some(entry.sha256.clone()),
                });
                completed.push(entry);
            }
            Err(e) => {
                journal.record(JournalEvent::StepFailed {
                    operation_id: operation_id.clone(),
                    step,
                    action: "quarantine_move".into(),
                    source: req.source_relative.clone(),
                    message: e.to_string(),
                });
                failed.push(StepFailure {
                    source: req.source_relative.clone(),
                    message: e.to_string(),
                });
                if stop_on_error {
                    halted_early = i + 1 < requests.len();
                    break;
                }
            }
        }
    }

    let status = if failed.is_empty() {
        "completed"
    } else if completed.is_empty() {
        "failed"
    } else {
        "partial"
    };
    journal.record(JournalEvent::OperationFinished {
        operation_id: operation_id.clone(),
        status: status.into(),
        succeeded: completed.len(),
        failed: failed.len(),
    });

    BatchOutcome {
        operation_id,
        completed,
        failed,
        halted_early,
    }
}

/// Return a quarantined file to its original relative location inside the
/// Mods root. Never overwrites: an occupied original path is an error the
/// user must resolve, not a decision this engine makes for them. Integrity is
/// enforced by [`verified_move`] against the hash recorded at quarantine time.
pub fn restore_quarantined(
    mods: &SafeRoot,
    entry: &QuarantineEntry,
    journal: &mut dyn JournalSink,
) -> Result<PathBuf, OpError> {
    let operation_id = new_operation_id();
    journal.record(JournalEvent::OperationStarted {
        operation_id: operation_id.clone(),
        kind: "restore_quarantined".into(),
        total_steps: 1,
    });

    let outcome = (|| -> Result<PathBuf, OpError> {
        let destination = mods.resolve_relative(&entry.original_relative)?;
        if destination.exists() {
            return Err(OpError::DestinationOccupied(destination));
        }
        verified_move(&entry.stored_absolute, &destination, Some(&entry.sha256))?;
        Ok(destination)
    })();

    match &outcome {
        Ok(destination) => {
            journal.record(JournalEvent::StepSucceeded {
                operation_id: operation_id.clone(),
                step: 1,
                action: "restore_move".into(),
                source: entry.stored_absolute.clone(),
                destination: Some(destination.clone()),
                sha256: Some(entry.sha256.clone()),
            });
            journal.record(JournalEvent::OperationFinished {
                operation_id,
                status: "completed".into(),
                succeeded: 1,
                failed: 0,
            });
        }
        Err(e) => {
            journal.record(JournalEvent::StepFailed {
                operation_id: operation_id.clone(),
                step: 1,
                action: "restore_move".into(),
                source: entry.stored_absolute.clone(),
                message: e.to_string(),
            });
            journal.record(JournalEvent::OperationFinished {
                operation_id,
                status: "failed".into(),
                succeeded: 0,
                failed: 1,
            });
        }
    }
    outcome
}

// ---------------------------------------------------------------------------
// Snapshots (recovery backups)
// ---------------------------------------------------------------------------

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const SNAPSHOT_MANIFEST_NAME: &str = "manifest.json";

#[derive(Serialize, Deserialize, Debug)]
pub struct SnapshotManifest {
    pub schema_version: u32,
    pub operation_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
    pub entries: Vec<SnapshotEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnapshotEntry {
    pub relative_path: PathBuf,
    pub sha256: String,
    pub size_bytes: u64,
}

/// Copy the listed files (relative to the Mods root) into
/// `<backup_root>/<YYYY-MM-DD>/<operation-id>/`, verify every copy's hash,
/// and write a `manifest.json` describing the snapshot.
///
/// All-or-nothing: on any failure the partial snapshot directory is removed
/// and an error returned, because a backup that silently lacks files is worse
/// than a mutation that never starts.
pub fn create_snapshot(
    mods: &SafeRoot,
    backup_root: &SafeRoot,
    relative_files: &[PathBuf],
    reason: &str,
    journal: &mut dyn JournalSink,
) -> Result<(PathBuf, SnapshotManifest), OpError> {
    let operation_id = new_operation_id();
    let snapshot_dir = backup_root.path().join(today_utc()).join(&operation_id);
    backup_root.contain(&snapshot_dir)?;

    journal.record(JournalEvent::OperationStarted {
        operation_id: operation_id.clone(),
        kind: "snapshot".into(),
        total_steps: relative_files.len(),
    });

    std::fs::create_dir_all(&snapshot_dir).map_err(|e| OpError::Io {
        path: snapshot_dir.clone(),
        source: e,
    })?;

    let mut entries: Vec<SnapshotEntry> = Vec::new();
    for (i, rel) in relative_files.iter().enumerate() {
        let step = i + 1;
        match snapshot_one(mods, backup_root, &snapshot_dir, rel) {
            Ok(entry) => {
                journal.record(JournalEvent::StepSucceeded {
                    operation_id: operation_id.clone(),
                    step,
                    action: "snapshot_copy".into(),
                    source: rel.clone(),
                    destination: Some(snapshot_dir.join(rel)),
                    sha256: Some(entry.sha256.clone()),
                });
                entries.push(entry);
            }
            Err(e) => {
                journal.record(JournalEvent::StepFailed {
                    operation_id: operation_id.clone(),
                    step,
                    action: "snapshot_copy".into(),
                    source: rel.clone(),
                    message: e.to_string(),
                });
                journal.record(JournalEvent::OperationFinished {
                    operation_id,
                    status: "failed".into(),
                    succeeded: entries.len(),
                    failed: 1,
                });
                let _ = std::fs::remove_dir_all(&snapshot_dir);
                return Err(e);
            }
        }
    }

    let manifest = SnapshotManifest {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        operation_id: operation_id.clone(),
        created_at: chrono::Utc::now(),
        reason: reason.to_string(),
        entries,
    };
    let manifest_path = snapshot_dir.join(SNAPSHOT_MANIFEST_NAME);
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| OpError::Io {
        path: manifest_path.clone(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })?;
    std::fs::write(&manifest_path, bytes).map_err(|e| OpError::Io {
        path: manifest_path,
        source: e,
    })?;

    journal.record(JournalEvent::OperationFinished {
        operation_id,
        status: "completed".into(),
        succeeded: manifest.entries.len(),
        failed: 0,
    });
    Ok((snapshot_dir, manifest))
}

fn snapshot_one(
    mods: &SafeRoot,
    backup_root: &SafeRoot,
    snapshot_dir: &Path,
    rel: &Path,
) -> Result<SnapshotEntry, OpError> {
    let source = mods.resolve_relative(rel)?;
    if !source.is_file() {
        return Err(OpError::SourceMissing(source));
    }
    let destination = snapshot_dir.join(rel);
    backup_root.contain(&destination)?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| OpError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let source_hash = sha256_file(&source).map_err(|e| OpError::Io {
        path: source.clone(),
        source: e,
    })?;
    let size_bytes = std::fs::copy(&source, &destination).map_err(|e| OpError::Io {
        path: destination.clone(),
        source: e,
    })?;
    let copied_hash = sha256_file(&destination).map_err(|e| OpError::Io {
        path: destination.clone(),
        source: e,
    })?;
    if copied_hash != source_hash {
        return Err(OpError::HashMismatch {
            path: destination,
            expected: source_hash,
            found: copied_hash,
        });
    }
    Ok(SnapshotEntry {
        relative_path: rel.to_path_buf(),
        sha256: copied_hash,
        size_bytes,
    })
}

/// Restore one file from a snapshot into the Mods root.
///
/// The stored copy's hash is verified against the manifest *before* the
/// original is touched — a corrupt backup refuses to restore rather than
/// replacing a live file with damaged bytes. Overwriting an existing file
/// requires `overwrite = true` and stages through a verified temp copy so the
/// unprotected window is as small as the final rename.
/// Restore every entry of a snapshot under ONE journaled operation — the
/// un-merge path. Occupied destinations are skipped and reported, never
/// overwritten; a corrupt backup copy fails its own step and the rest
/// continue. Returns (restored, skipped-with-reasons).
pub fn restore_snapshot_all(
    mods: &SafeRoot,
    snapshot_dir: &Path,
    entries: &[SnapshotEntry],
    journal: &mut dyn JournalSink,
) -> (usize, Vec<(PathBuf, String)>) {
    let operation_id = new_operation_id();
    journal.record(JournalEvent::OperationStarted {
        operation_id: operation_id.clone(),
        kind: "restore_from_snapshot".into(),
        total_steps: entries.len(),
    });
    let mut restored = 0usize;
    let mut skipped: Vec<(PathBuf, String)> = Vec::new();
    for (step, entry) in entries.iter().enumerate() {
        match restore_entry_verified(mods, snapshot_dir, entry, false) {
            Ok(dest) => {
                journal.record(JournalEvent::StepSucceeded {
                    operation_id: operation_id.clone(),
                    step,
                    action: "restore".into(),
                    source: snapshot_dir.join(&entry.relative_path),
                    destination: Some(dest),
                    sha256: Some(entry.sha256.clone()),
                });
                restored += 1;
            }
            Err(e) => {
                let msg = e.to_string();
                journal.record(JournalEvent::StepFailed {
                    operation_id: operation_id.clone(),
                    step,
                    action: "restore".into(),
                    source: snapshot_dir.join(&entry.relative_path),
                    message: msg.clone(),
                });
                skipped.push((entry.relative_path.clone(), msg));
            }
        }
    }
    journal.record(JournalEvent::OperationFinished {
        operation_id,
        status: if skipped.is_empty() { "completed" } else { "partial" }.into(),
        succeeded: restored,
        failed: skipped.len(),
    });
    (restored, skipped)
}

/// The verified single-entry restore both paths share: hash-check the
/// stored copy, place it, and refuse occupied destinations unless told.
pub fn restore_entry_verified(
    mods: &SafeRoot,
    snapshot_dir: &Path,
    entry: &SnapshotEntry,
    overwrite: bool,
) -> Result<PathBuf, OpError> {
    let stored = snapshot_dir.join(&entry.relative_path);
    if !stored.is_file() {
        return Err(OpError::SourceMissing(stored));
    }
    let stored_hash = sha256_file(&stored).map_err(|e| OpError::Io {
        path: stored.clone(),
        source: e,
    })?;
    if stored_hash != entry.sha256 {
        return Err(OpError::HashMismatch {
            path: stored,
            expected: entry.sha256.clone(),
            found: stored_hash,
        });
    }
    let destination = mods.resolve_relative(&entry.relative_path)?;
    if destination.exists() && !overwrite {
        return Err(OpError::DestinationOccupied(destination));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| OpError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    if !destination.exists() {
        std::fs::copy(&stored, &destination).map_err(|e| OpError::Io {
            path: destination.clone(),
            source: e,
        })?;
        let placed = sha256_file(&destination).map_err(|e| OpError::Io {
            path: destination.clone(),
            source: e,
        })?;
        if placed != entry.sha256 {
            let _ = std::fs::remove_file(&destination);
            return Err(OpError::HashMismatch {
                path: destination,
                expected: entry.sha256.clone(),
                found: placed,
            });
        }
        return Ok(destination);
    }
    // overwrite path stays with the original single-entry function.
    Err(OpError::DestinationOccupied(destination))
}

pub fn restore_from_snapshot(
    mods: &SafeRoot,
    snapshot_dir: &Path,
    entry: &SnapshotEntry,
    overwrite: bool,
    journal: &mut dyn JournalSink,
) -> Result<PathBuf, OpError> {
    let operation_id = new_operation_id();
    journal.record(JournalEvent::OperationStarted {
        operation_id: operation_id.clone(),
        kind: "restore_from_snapshot".into(),
        total_steps: 1,
    });

    let outcome = (|| -> Result<PathBuf, OpError> {
        let stored = snapshot_dir.join(&entry.relative_path);
        if !stored.is_file() {
            return Err(OpError::SourceMissing(stored));
        }
        let stored_hash = sha256_file(&stored).map_err(|e| OpError::Io {
            path: stored.clone(),
            source: e,
        })?;
        if stored_hash != entry.sha256 {
            return Err(OpError::HashMismatch {
                path: stored,
                expected: entry.sha256.clone(),
                found: stored_hash,
            });
        }

        let destination = mods.resolve_relative(&entry.relative_path)?;
        if destination.exists() && !overwrite {
            return Err(OpError::DestinationOccupied(destination));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|e| OpError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        if destination.exists() {
            // Stage next to the destination, verify, then swap.
            let mut staged_name = destination
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            staged_name.push_str(".plumbob-restore-tmp");
            let staged = destination.with_file_name(staged_name);
            std::fs::copy(&stored, &staged).map_err(|e| OpError::Io {
                path: staged.clone(),
                source: e,
            })?;
            let staged_hash = sha256_file(&staged).map_err(|e| OpError::Io {
                path: staged.clone(),
                source: e,
            })?;
            if staged_hash != entry.sha256 {
                let _ = std::fs::remove_file(&staged);
                return Err(OpError::HashMismatch {
                    path: staged,
                    expected: entry.sha256.clone(),
                    found: staged_hash,
                });
            }
            std::fs::remove_file(&destination).map_err(|e| OpError::Io {
                path: destination.clone(),
                source: e,
            })?;
            std::fs::rename(&staged, &destination).map_err(|e| OpError::Io {
                path: destination.clone(),
                source: e,
            })?;
        } else {
            std::fs::copy(&stored, &destination).map_err(|e| OpError::Io {
                path: destination.clone(),
                source: e,
            })?;
            let restored_hash = sha256_file(&destination).map_err(|e| OpError::Io {
                path: destination.clone(),
                source: e,
            })?;
            if restored_hash != entry.sha256 {
                let _ = std::fs::remove_file(&destination);
                return Err(OpError::HashMismatch {
                    path: destination,
                    expected: entry.sha256.clone(),
                    found: restored_hash,
                });
            }
        }
        Ok(destination)
    })();

    match &outcome {
        Ok(destination) => {
            journal.record(JournalEvent::StepSucceeded {
                operation_id: operation_id.clone(),
                step: 1,
                action: "snapshot_restore".into(),
                source: entry.relative_path.clone(),
                destination: Some(destination.clone()),
                sha256: Some(entry.sha256.clone()),
            });
            journal.record(JournalEvent::OperationFinished {
                operation_id,
                status: "completed".into(),
                succeeded: 1,
                failed: 0,
            });
        }
        Err(e) => {
            journal.record(JournalEvent::StepFailed {
                operation_id: operation_id.clone(),
                step: 1,
                action: "snapshot_restore".into(),
                source: entry.relative_path.clone(),
                message: e.to_string(),
            });
            journal.record(JournalEvent::OperationFinished {
                operation_id,
                status: "failed".into(),
                succeeded: 0,
                failed: 1,
            });
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashing::sha256_bytes;
    use std::fs;

    struct Fixture {
        _mods_dir: tempfile::TempDir,
        _data_dir: tempfile::TempDir,
        mods: SafeRoot,
        quarantine: SafeRoot,
        backups: SafeRoot,
    }

    fn fixture() -> Fixture {
        let mods_dir = tempfile::tempdir().unwrap();
        let data_dir = tempfile::tempdir().unwrap();
        fs::write(mods_dir.path().join("top.package"), b"top-bytes").unwrap();
        fs::create_dir_all(mods_dir.path().join("CAS/Hair")).unwrap();
        fs::write(mods_dir.path().join("CAS/Hair/dup.package"), b"dup-bytes").unwrap();
        let q = data_dir.path().join("Quarantine");
        let b = data_dir.path().join("Backups");
        fs::create_dir_all(&q).unwrap();
        fs::create_dir_all(&b).unwrap();
        Fixture {
            mods: SafeRoot::new(mods_dir.path()).unwrap(),
            quarantine: SafeRoot::new(&q).unwrap(),
            backups: SafeRoot::new(&b).unwrap(),
            _mods_dir: mods_dir,
            _data_dir: data_dir,
        }
    }

    fn req(rel: &str, reason: &str) -> QuarantineRequest {
        QuarantineRequest {
            source_relative: PathBuf::from(rel),
            reason: reason.into(),
            expected_sha256: None,
        }
    }

    #[test]
    fn verified_move_moves_and_verifies() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a.package");
        fs::write(&src, b"payload").unwrap();
        let dst = tmp.path().join("nested/dir/a.package");
        let hash = verified_move(&src, &dst, None).unwrap();
        assert_eq!(hash, sha256_bytes(b"payload"));
        assert!(!src.exists());
        assert_eq!(fs::read(&dst).unwrap(), b"payload");
    }

    #[test]
    fn verified_move_refuses_occupied_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a.package");
        let dst = tmp.path().join("b.package");
        fs::write(&src, b"one").unwrap();
        fs::write(&dst, b"two").unwrap();
        let err = verified_move(&src, &dst, None).unwrap_err();
        assert!(matches!(err, OpError::DestinationOccupied(_)));
        assert_eq!(fs::read(&src).unwrap(), b"one");
        assert_eq!(fs::read(&dst).unwrap(), b"two");
    }

    #[test]
    fn stale_expected_hash_rolls_the_move_back() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("changed.package");
        fs::write(&src, b"new content the database has not seen").unwrap();
        let dst = tmp.path().join("moved.package");
        let stale = sha256_bytes(b"what the database remembers");
        let err = verified_move(&src, &dst, Some(&stale)).unwrap_err();
        assert!(matches!(err, OpError::HashMismatch { .. }));
        assert!(src.exists(), "source must be restored after rollback");
        assert!(
            !dst.exists(),
            "destination must not retain unverified bytes"
        );
    }

    #[test]
    fn quarantine_moves_preserving_relative_structure() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let outcome = quarantine_files(
            &fx.mods,
            &fx.quarantine,
            &[
                req("top.package", "user selected"),
                req("CAS/Hair/dup.package", "exact duplicate"),
            ],
            true,
            &mut journal,
        );
        assert_eq!(outcome.completed.len(), 2);
        assert!(outcome.failed.is_empty());
        assert!(!outcome.halted_early);

        for entry in &outcome.completed {
            assert!(entry.stored_absolute.is_file());
            assert!(entry
                .stored_absolute
                .strip_prefix(fx.quarantine.path())
                .unwrap()
                .to_string_lossy()
                .contains(&outcome.operation_id));
        }
        let dup = outcome
            .completed
            .iter()
            .find(|e| e.original_relative.ends_with("dup.package"))
            .unwrap();
        assert!(dup
            .stored_absolute
            .to_string_lossy()
            .replace('\\', "/")
            .contains("CAS/Hair/dup.package"));
        assert_eq!(dup.sha256, sha256_bytes(b"dup-bytes"));
        assert!(!fx.mods.path().join("CAS/Hair/dup.package").exists());
        assert!(!fx.mods.path().join("top.package").exists());
    }

    #[test]
    fn quarantine_rejects_traversal_and_halts_plan() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let outcome = quarantine_files(
            &fx.mods,
            &fx.quarantine,
            &[
                req("../evil.package", "attack"),
                req("top.package", "user selected"),
            ],
            true,
            &mut journal,
        );
        assert_eq!(outcome.completed.len(), 0);
        assert_eq!(outcome.failed.len(), 1);
        assert!(outcome.halted_early, "remaining steps must not run");
        assert!(
            fx.mods.path().join("top.package").exists(),
            "later steps must be untouched after a halt"
        );
    }

    #[test]
    fn quarantine_can_continue_past_failures_when_asked() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let outcome = quarantine_files(
            &fx.mods,
            &fx.quarantine,
            &[
                req("ghost.package", "missing on disk"),
                req("top.package", "user selected"),
            ],
            false,
            &mut journal,
        );
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.completed.len(), 1);
        assert!(!outcome.halted_early);
        assert!(!fx.mods.path().join("top.package").exists());
    }

    #[test]
    fn restore_returns_file_to_original_path() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let outcome = quarantine_files(
            &fx.mods,
            &fx.quarantine,
            &[req("CAS/Hair/dup.package", "testing workflow")],
            true,
            &mut journal,
        );
        let entry = &outcome.completed[0];
        let restored = restore_quarantined(&fx.mods, entry, &mut journal).unwrap();
        assert_eq!(fs::read(&restored).unwrap(), b"dup-bytes");
        assert!(!entry.stored_absolute.exists());
        assert!(restored.ends_with("CAS/Hair/dup.package"));
    }

    #[test]
    fn restore_refuses_occupied_original_path() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let outcome = quarantine_files(
            &fx.mods,
            &fx.quarantine,
            &[req("top.package", "user selected")],
            true,
            &mut journal,
        );
        let entry = &outcome.completed[0];
        fs::write(fx.mods.path().join("top.package"), b"newer file").unwrap();
        let err = restore_quarantined(&fx.mods, entry, &mut journal).unwrap_err();
        assert!(matches!(err, OpError::DestinationOccupied(_)));
        assert!(
            entry.stored_absolute.exists(),
            "quarantined copy must survive"
        );
        assert_eq!(
            fs::read(fx.mods.path().join("top.package")).unwrap(),
            b"newer file"
        );
    }

    #[test]
    fn snapshot_copies_verifies_and_writes_manifest() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let (dir, manifest) = create_snapshot(
            &fx.mods,
            &fx.backups,
            &[
                PathBuf::from("top.package"),
                PathBuf::from("CAS/Hair/dup.package"),
            ],
            "before duplicate cleanup",
            &mut journal,
        )
        .unwrap();
        assert_eq!(manifest.schema_version, SNAPSHOT_SCHEMA_VERSION);
        assert_eq!(manifest.entries.len(), 2);
        assert!(dir.join("top.package").is_file());
        assert!(dir.join("CAS/Hair/dup.package").is_file());
        let top = manifest
            .entries
            .iter()
            .find(|e| e.relative_path.ends_with("top.package"))
            .unwrap();
        assert_eq!(top.sha256, sha256_bytes(b"top-bytes"));
        assert_eq!(top.size_bytes, b"top-bytes".len() as u64);

        // Originals are untouched — snapshots copy, never move.
        assert!(fx.mods.path().join("top.package").exists());

        let parsed: SnapshotManifest =
            serde_json::from_slice(&fs::read(dir.join(SNAPSHOT_MANIFEST_NAME)).unwrap()).unwrap();
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.reason, "before duplicate cleanup");
    }

    #[test]
    fn failed_snapshot_removes_partial_backup() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let err = create_snapshot(
            &fx.mods,
            &fx.backups,
            &[
                PathBuf::from("top.package"),
                PathBuf::from("does-not-exist.package"),
            ],
            "doomed",
            &mut journal,
        )
        .unwrap_err();
        assert!(matches!(err, OpError::SourceMissing(_)));
        // No half-backup may remain anywhere under the backup root.
        let leftovers = walkdir::WalkDir::new(fx.backups.path())
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn restore_from_snapshot_recovers_a_mutated_file() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let (dir, manifest) = create_snapshot(
            &fx.mods,
            &fx.backups,
            &[PathBuf::from("top.package")],
            "before rename plan",
            &mut journal,
        )
        .unwrap();
        fs::write(fx.mods.path().join("top.package"), b"botched edit").unwrap();
        let restored =
            restore_from_snapshot(&fx.mods, &dir, &manifest.entries[0], true, &mut journal)
                .unwrap();
        assert_eq!(fs::read(&restored).unwrap(), b"top-bytes");
    }

    #[test]
    fn restore_refuses_overwrite_unless_asked() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let (dir, manifest) = create_snapshot(
            &fx.mods,
            &fx.backups,
            &[PathBuf::from("top.package")],
            "safety",
            &mut journal,
        )
        .unwrap();
        let err = restore_from_snapshot(&fx.mods, &dir, &manifest.entries[0], false, &mut journal)
            .unwrap_err();
        assert!(matches!(err, OpError::DestinationOccupied(_)));
    }

    #[test]
    fn corrupt_backup_refuses_to_restore() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let (dir, manifest) = create_snapshot(
            &fx.mods,
            &fx.backups,
            &[PathBuf::from("top.package")],
            "integrity",
            &mut journal,
        )
        .unwrap();
        // Bit-rot simulation: damage the stored copy.
        fs::write(dir.join("top.package"), b"corrupted!!").unwrap();
        fs::write(fx.mods.path().join("top.package"), b"live edit").unwrap();
        let err = restore_from_snapshot(&fx.mods, &dir, &manifest.entries[0], true, &mut journal)
            .unwrap_err();
        assert!(matches!(err, OpError::HashMismatch { .. }));
        assert_eq!(
            fs::read(fx.mods.path().join("top.package")).unwrap(),
            b"live edit",
            "a corrupt backup must never touch the live file"
        );
    }

    #[test]
    fn journal_records_full_operation_lifecycle() {
        let fx = fixture();
        let mut journal = VecJournal::default();
        let outcome = quarantine_files(
            &fx.mods,
            &fx.quarantine,
            &[req("top.package", "zero-byte")],
            true,
            &mut journal,
        );
        assert_eq!(journal.0.len(), 3);
        assert!(matches!(
            journal.0[0],
            JournalEvent::OperationStarted { .. }
        ));
        assert!(matches!(journal.0[1], JournalEvent::StepSucceeded { .. }));
        match &journal.0[2] {
            JournalEvent::OperationFinished {
                operation_id,
                status,
                succeeded,
                failed,
            } => {
                assert_eq!(operation_id, &outcome.operation_id);
                assert_eq!(status, "completed");
                assert_eq!(*succeeded, 1);
                assert_eq!(*failed, 0);
            }
            other => panic!("expected OperationFinished, got {other:?}"),
        }
    }
}
