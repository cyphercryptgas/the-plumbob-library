//! Operation, quarantine, and backup persistence.
//!
//! [`SqliteJournal`] implements [`crate::ops::JournalSink`] so the filesystem
//! engines write their lifecycle straight into `operations` /
//! `operation_steps`. Journal insert failures never interrupt an in-flight
//! filesystem operation (aborting mid-move is more dangerous than a missing
//! log row); instead the first error is retained and surfaced by
//! [`SqliteJournal::finish`], which callers must check.

use super::{now_rfc3339, rel_to_db_string, DbError};
use crate::ops::{BatchOutcome, JournalEvent, JournalSink, QuarantineEntry, SnapshotManifest, ToggleEntry};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;

pub struct SqliteJournal<'c> {
    conn: &'c Connection,
    rows: HashMap<String, i64>,
    last_error: Option<rusqlite::Error>,
}

impl<'c> SqliteJournal<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self {
            conn,
            rows: HashMap::new(),
            last_error: None,
        }
    }

    /// Surface any journal write failure. Call after the filesystem operation
    /// completes; a failed journal means the activity record is incomplete
    /// and the user must be told.
    pub fn finish(self) -> Result<(), DbError> {
        match self.last_error {
            Some(e) => Err(e.into()),
            None => Ok(()),
        }
    }

    fn try_record(&mut self, event: &JournalEvent) -> Result<(), rusqlite::Error> {
        match event {
            JournalEvent::OperationStarted {
                operation_id,
                kind,
                total_steps: _,
            } => {
                let now = now_rfc3339();
                self.conn.execute(
                    "INSERT INTO operations (operation_uid, operation_type, status,
                        created_at, started_at)
                     VALUES (?1, ?2, 'running', ?3, ?3)",
                    params![operation_id, kind, now],
                )?;
                self.rows
                    .insert(operation_id.clone(), self.conn.last_insert_rowid());
                Ok(())
            }
            JournalEvent::StepSucceeded {
                operation_id,
                step,
                action,
                source,
                destination,
                sha256,
            } => {
                let row = self.row_for(operation_id)?;
                self.conn.execute(
                    "INSERT INTO operation_steps (operation_id, step_order, action,
                        source_path, destination_path, expected_hash, status)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'succeeded')",
                    params![
                        row,
                        *step as i64,
                        action,
                        source.to_string_lossy(),
                        destination
                            .as_ref()
                            .map(|d| d.to_string_lossy().into_owned()),
                        sha256
                    ],
                )?;
                Ok(())
            }
            JournalEvent::StepFailed {
                operation_id,
                step,
                action,
                source,
                message,
            } => {
                let row = self.row_for(operation_id)?;
                self.conn.execute(
                    "INSERT INTO operation_steps (operation_id, step_order, action,
                        source_path, status, error_message)
                     VALUES (?1, ?2, ?3, ?4, 'failed', ?5)",
                    params![row, *step as i64, action, source.to_string_lossy(), message],
                )?;
                Ok(())
            }
            JournalEvent::OperationFinished {
                operation_id,
                status,
                succeeded,
                failed,
            } => {
                let row = self.row_for(operation_id)?;
                self.conn.execute(
                    "UPDATE operations SET status = ?2, completed_at = ?3, summary = ?4
                     WHERE id = ?1",
                    params![
                        row,
                        status,
                        now_rfc3339(),
                        format!("{succeeded} step(s) succeeded, {failed} failed")
                    ],
                )?;
                Ok(())
            }
        }
    }

    fn row_for(&self, uid: &str) -> Result<i64, rusqlite::Error> {
        match self.rows.get(uid) {
            Some(id) => Ok(*id),
            None => self.conn.query_row(
                "SELECT id FROM operations WHERE operation_uid = ?1",
                [uid],
                |r| r.get(0),
            ),
        }
    }
}

impl JournalSink for SqliteJournal<'_> {
    fn record(&mut self, event: JournalEvent) {
        if self.last_error.is_some() {
            return;
        }
        if let Err(e) = self.try_record(&event) {
            self.last_error = Some(e);
        }
    }
}

// ---------------------------------------------------------------------------
// Operation queries
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationView {
    pub id: i64,
    pub operation_uid: String,
    pub operation_type: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub summary: Option<String>,
    pub backup_id: Option<i64>,
}

pub fn operation_by_uid(conn: &Connection, uid: &str) -> Result<Option<OperationView>, DbError> {
    Ok(conn
        .query_row(
            "SELECT id, operation_uid, operation_type, status, created_at, completed_at,
                    summary, backup_id
             FROM operations WHERE operation_uid = ?1",
            [uid],
            map_operation,
        )
        .optional()?)
}

pub fn list_operations(conn: &Connection, limit: i64) -> Result<Vec<OperationView>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, operation_uid, operation_type, status, created_at, completed_at,
                summary, backup_id
         FROM operations ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], map_operation)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn map_operation(r: &rusqlite::Row<'_>) -> rusqlite::Result<OperationView> {
    Ok(OperationView {
        id: r.get(0)?,
        operation_uid: r.get(1)?,
        operation_type: r.get(2)?,
        status: r.get(3)?,
        created_at: r.get(4)?,
        completed_at: r.get(5)?,
        summary: r.get(6)?,
        backup_id: r.get(7)?,
    })
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationStepView {
    pub file_id: Option<i64>,
    pub step_order: i64,
    pub action: String,
    pub source_path: String,
    pub destination_path: Option<String>,
    pub expected_hash: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
}

pub fn operation_steps(
    conn: &Connection,
    operation_row_id: i64,
) -> Result<Vec<OperationStepView>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT s.step_order, s.action, s.source_path, s.destination_path,
                s.expected_hash, s.status, s.error_message,
                COALESCE(
                    (SELECT id FROM files WHERE s.expected_hash IS NOT NULL
                       AND sha256 = s.expected_hash LIMIT 1),
                    (SELECT id FROM files WHERE relative_path = s.source_path
                       LIMIT 1)
                ) AS file_id
         FROM operation_steps s
         WHERE s.operation_id = ?1 ORDER BY s.step_order",
    )?;
    let rows = stmt.query_map([operation_row_id], |r| {
        Ok(OperationStepView {
            file_id: r.get(7)?,
            step_order: r.get(0)?,
            action: r.get(1)?,
            source_path: r.get(2)?,
            destination_path: r.get(3)?,
            expected_hash: r.get(4)?,
            status: r.get(5)?,
            error_message: r.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Quarantine records
// ---------------------------------------------------------------------------

/// Persist a completed quarantine batch: link entries to their file rows,
/// flip file status to `quarantined` (and disable), and record where each
/// stored copy lives. Returns the new `quarantine_entries` ids.
/// Sync file rows to a completed toggle batch: `enabled` flips, and the
/// physical name/path columns follow the rename while `relative_path`
/// keeps the file's logical identity.
pub fn record_toggle_outcome(
    conn: &mut Connection,
    outcome: &BatchOutcome<ToggleEntry>,
) -> Result<usize, DbError> {
    let tx = conn.transaction()?;
    let mut updated = 0usize;
    {
        let mut stmt = tx.prepare(
            "UPDATE files
             SET enabled = ?2, current_filename = ?3, absolute_path = ?4
             WHERE relative_path = ?1",
        )?;
        for entry in &outcome.completed {
            let name = entry
                .physical_absolute
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            updated += stmt.execute(params![
                rel_to_db_string(&entry.relative_path),
                entry.enabled,
                name,
                entry.physical_absolute.to_string_lossy(),
            ])?;
        }
    }
    tx.commit()?;
    Ok(updated)
}

pub fn record_quarantine_outcome(
    conn: &mut Connection,
    outcome: &BatchOutcome<QuarantineEntry>,
) -> Result<Vec<i64>, DbError> {
    let tx = conn.transaction()?;
    let op_row: Option<i64> = tx
        .query_row(
            "SELECT id FROM operations WHERE operation_uid = ?1",
            [&outcome.operation_id],
            |r| r.get(0),
        )
        .optional()?;
    let now = now_rfc3339();
    let mut ids = Vec::with_capacity(outcome.completed.len());
    {
        let mut find_file = tx.prepare("SELECT id FROM files WHERE relative_path = ?1")?;
        let mut flip_file =
            tx.prepare("UPDATE files SET status = 'quarantined', enabled = 0 WHERE id = ?1")?;
        let mut insert = tx.prepare(
            "INSERT INTO quarantine_entries (file_id, original_path, quarantine_path,
                sha256, reason, quarantined_at, status, operation_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'quarantined', ?7)",
        )?;
        for entry in &outcome.completed {
            let rel_s = rel_to_db_string(&entry.original_relative);
            let file_id: Option<i64> = find_file.query_row([&rel_s], |r| r.get(0)).optional()?;
            if let Some(fid) = file_id {
                flip_file.execute([fid])?;
            }
            insert.execute(params![
                file_id,
                rel_s,
                entry.stored_absolute.to_string_lossy(),
                entry.sha256,
                entry.reason,
                now,
                op_row
            ])?;
            ids.push(tx.last_insert_rowid());
        }
    }
    tx.commit()?;
    Ok(ids)
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineView {
    pub id: i64,
    pub file_id: Option<i64>,
    pub original_path: String,
    pub quarantine_path: String,
    pub sha256: Option<String>,
    pub reason: String,
    pub quarantined_at: String,
    pub restored_at: Option<String>,
    pub status: String,
}

pub fn list_quarantine(
    conn: &Connection,
    include_restored: bool,
) -> Result<Vec<QuarantineView>, DbError> {
    let sql = if include_restored {
        "SELECT id, file_id, original_path, quarantine_path, sha256, reason,
                quarantined_at, restored_at, status
         FROM quarantine_entries ORDER BY id DESC"
    } else {
        "SELECT id, file_id, original_path, quarantine_path, sha256, reason,
                quarantined_at, restored_at, status
         FROM quarantine_entries WHERE status = 'quarantined' ORDER BY id DESC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| {
        Ok(QuarantineView {
            id: r.get(0)?,
            file_id: r.get(1)?,
            original_path: r.get(2)?,
            quarantine_path: r.get(3)?,
            sha256: r.get(4)?,
            reason: r.get(5)?,
            quarantined_at: r.get(6)?,
            restored_at: r.get(7)?,
            status: r.get(8)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn quarantine_entry_by_id(
    conn: &Connection,
    entry_id: i64,
) -> Result<Option<QuarantineView>, DbError> {
    Ok(conn
        .query_row(
            "SELECT id, file_id, original_path, quarantine_path, sha256, reason,
                    quarantined_at, restored_at, status
             FROM quarantine_entries WHERE id = ?1",
            [entry_id],
            |r| {
                Ok(QuarantineView {
                    id: r.get(0)?,
                    file_id: r.get(1)?,
                    original_path: r.get(2)?,
                    quarantine_path: r.get(3)?,
                    sha256: r.get(4)?,
                    reason: r.get(5)?,
                    quarantined_at: r.get(6)?,
                    restored_at: r.get(7)?,
                    status: r.get(8)?,
                })
            },
        )
        .optional()?)
}

/// After a successful filesystem restore, mark the entry restored and heal
/// the file row.
pub fn mark_quarantine_restored(conn: &mut Connection, entry_id: i64) -> Result<(), DbError> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE quarantine_entries SET restored_at = ?2, status = 'restored'
         WHERE id = ?1",
        params![entry_id, now_rfc3339()],
    )?;
    tx.execute(
        "UPDATE files SET status = 'current', enabled = 1
         WHERE id IN (SELECT file_id FROM quarantine_entries
                      WHERE id = ?1 AND file_id IS NOT NULL)",
        params![entry_id],
    )?;
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Backup records
// ---------------------------------------------------------------------------

/// Persist a completed snapshot's manifest and link it to its operation row.
pub fn record_snapshot(
    conn: &mut Connection,
    manifest: &SnapshotManifest,
    snapshot_dir: &Path,
) -> Result<i64, DbError> {
    let tx = conn.transaction()?;
    let op_row: Option<i64> = tx
        .query_row(
            "SELECT id FROM operations WHERE operation_uid = ?1",
            [&manifest.operation_id],
            |r| r.get(0),
        )
        .optional()?;
    let total_bytes: i64 = manifest.entries.iter().map(|e| e.size_bytes as i64).sum();
    tx.execute(
        "INSERT INTO backups (created_at, reason, root_path, status, total_files,
            total_bytes, operation_id)
         VALUES (?1, ?2, ?3, 'available', ?4, ?5, ?6)",
        params![
            manifest.created_at.to_rfc3339(),
            manifest.reason,
            snapshot_dir.to_string_lossy(),
            manifest.entries.len() as i64,
            total_bytes,
            op_row
        ],
    )?;
    let backup_id = tx.last_insert_rowid();
    {
        let mut insert = tx.prepare(
            "INSERT INTO backup_entries (backup_id, source_path, backup_path, sha256,
                size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for e in &manifest.entries {
            insert.execute(params![
                backup_id,
                rel_to_db_string(&e.relative_path),
                snapshot_dir.join(&e.relative_path).to_string_lossy(),
                e.sha256,
                e.size_bytes as i64
            ])?;
        }
    }
    if let Some(op) = op_row {
        tx.execute(
            "UPDATE operations SET backup_id = ?2 WHERE id = ?1",
            params![op, backup_id],
        )?;
    }
    tx.commit()?;
    Ok(backup_id)
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupView {
    pub id: i64,
    pub created_at: String,
    pub reason: String,
    pub root_path: String,
    pub status: String,
    pub total_files: i64,
    pub total_bytes: i64,
    pub operation_id: Option<i64>,
}

pub fn list_backups(conn: &Connection) -> Result<Vec<BackupView>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, created_at, reason, root_path, status, total_files, total_bytes,
                operation_id
         FROM backups ORDER BY id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(BackupView {
            id: r.get(0)?,
            created_at: r.get(1)?,
            reason: r.get(2)?,
            root_path: r.get(3)?,
            status: r.get(4)?,
            total_files: r.get(5)?,
            total_bytes: r.get(6)?,
            operation_id: r.get(7)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntryView {
    pub source_path: String,
    pub backup_path: String,
    pub sha256: String,
    pub size_bytes: i64,
    /// The library row this entry maps to today (hash first, path
    /// fallback) — history gets to wear the file's thumbnail.
    pub file_id: Option<i64>,
}

pub fn backup_entries(conn: &Connection, backup_id: i64) -> Result<Vec<BackupEntryView>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT e.source_path, e.backup_path, e.sha256, e.size_bytes,
                COALESCE(
                    (SELECT id FROM files WHERE sha256 = e.sha256 LIMIT 1),
                    (SELECT id FROM files WHERE relative_path = e.source_path
                       LIMIT 1)
                ) AS file_id
         FROM backup_entries e
         WHERE e.backup_id = ?1 ORDER BY e.source_path COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([backup_id], |r| {
        Ok(BackupEntryView {
            source_path: r.get(0)?,
            backup_path: r.get(1)?,
            sha256: r.get(2)?,
            size_bytes: r.get(3)?,
            file_id: r.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{files, Database};
    use crate::ops::{create_snapshot, quarantine_files, restore_quarantined, QuarantineRequest};
    use crate::paths::SafeRoot;
    use crate::scan::{scan, ScanOptions};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    struct World {
        _mods_dir: tempfile::TempDir,
        _data_dir: tempfile::TempDir,
        mods: SafeRoot,
        quarantine: SafeRoot,
        backups: SafeRoot,
        db: Database,
    }

    fn world() -> World {
        let mods_dir = tempfile::tempdir().unwrap();
        let data_dir = tempfile::tempdir().unwrap();
        fs::write(mods_dir.path().join("keep.package"), b"keep-bytes").unwrap();
        fs::create_dir_all(mods_dir.path().join("CAS")).unwrap();
        fs::write(mods_dir.path().join("CAS/junk.package"), b"junk-bytes").unwrap();
        let q = data_dir.path().join("Quarantine");
        let b = data_dir.path().join("Backups");
        fs::create_dir_all(&q).unwrap();
        fs::create_dir_all(&b).unwrap();

        let mods = SafeRoot::new(mods_dir.path()).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        let cancel = AtomicBool::new(false);
        let report = scan(&mods, &ScanOptions::default(), &cancel, |_| {});
        files::reconcile_scan(db.conn_mut(), &report, "initial", &[]).unwrap();

        World {
            mods,
            quarantine: SafeRoot::new(&q).unwrap(),
            backups: SafeRoot::new(&b).unwrap(),
            db,
            _mods_dir: mods_dir,
            _data_dir: data_dir,
        }
    }

    #[test]
    fn journal_persists_operation_and_steps() {
        let w = world();
        let outcome = {
            let mut journal = SqliteJournal::new(w.db.conn());
            let outcome = quarantine_files(
                &w.mods,
                &w.quarantine,
                &[QuarantineRequest {
                    source_relative: PathBuf::from("CAS/junk.package"),
                    reason: "user selected".into(),
                    expected_sha256: None,
                }],
                true,
                &mut journal,
            );
            journal.finish().unwrap();
            outcome
        };
        let op = operation_by_uid(w.db.conn(), &outcome.operation_id)
            .unwrap()
            .expect("operation persisted");
        assert_eq!(op.operation_type, "quarantine");
        assert_eq!(op.status, "completed");
        assert!(op.completed_at.is_some());
        assert_eq!(op.summary.as_deref(), Some("1 step(s) succeeded, 0 failed"));

        let steps = operation_steps(w.db.conn(), op.id).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].status, "succeeded");
        assert_eq!(steps[0].action, "quarantine_move");
        assert!(steps[0].expected_hash.is_some());
    }

    #[test]
    fn journal_persists_failed_steps_with_messages() {
        let w = world();
        let mut journal = SqliteJournal::new(w.db.conn());
        let outcome = quarantine_files(
            &w.mods,
            &w.quarantine,
            &[QuarantineRequest {
                source_relative: PathBuf::from("../escape.package"),
                reason: "attack".into(),
                expected_sha256: None,
            }],
            true,
            &mut journal,
        );
        journal.finish().unwrap();
        let op = operation_by_uid(w.db.conn(), &outcome.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(op.status, "failed");
        let steps = operation_steps(w.db.conn(), op.id).unwrap();
        assert_eq!(steps[0].status, "failed");
        assert!(steps[0].error_message.as_deref().unwrap().contains(".."));
    }

    #[test]
    fn quarantine_records_flip_file_status_and_restore_heals_it() {
        let mut w = world();
        let outcome = {
            let mut journal = SqliteJournal::new(w.db.conn());
            let o = quarantine_files(
                &w.mods,
                &w.quarantine,
                &[QuarantineRequest {
                    source_relative: PathBuf::from("CAS/junk.package"),
                    reason: "exact duplicate".into(),
                    expected_sha256: None,
                }],
                true,
                &mut journal,
            );
            journal.finish().unwrap();
            o
        };
        let ids = record_quarantine_outcome(w.db.conn_mut(), &outcome).unwrap();
        assert_eq!(ids.len(), 1);

        let status: String =
            w.db.conn()
                .query_row(
                    "SELECT status FROM files WHERE relative_path = 'CAS/junk.package'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(status, "quarantined");
        assert_eq!(list_quarantine(w.db.conn(), false).unwrap().len(), 1);

        // Filesystem restore, then heal the records.
        let mut journal = SqliteJournal::new(w.db.conn());
        restore_quarantined(&w.mods, &outcome.completed[0], &mut journal).unwrap();
        journal.finish().unwrap();
        mark_quarantine_restored(w.db.conn_mut(), ids[0]).unwrap();

        assert!(list_quarantine(w.db.conn(), false).unwrap().is_empty());
        let all = list_quarantine(w.db.conn(), true).unwrap();
        assert_eq!(all[0].status, "restored");
        assert!(all[0].restored_at.is_some());
        let status: String =
            w.db.conn()
                .query_row(
                    "SELECT status FROM files WHERE relative_path = 'CAS/junk.package'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(status, "current");
    }

    #[test]
    fn snapshots_persist_with_entries_and_operation_link() {
        let mut w = world();
        let (dir, manifest) = {
            let mut journal = SqliteJournal::new(w.db.conn());
            let r = create_snapshot(
                &w.mods,
                &w.backups,
                &[
                    PathBuf::from("keep.package"),
                    PathBuf::from("CAS/junk.package"),
                ],
                "before organization plan",
                &mut journal,
            )
            .unwrap();
            journal.finish().unwrap();
            r
        };
        let backup_id = record_snapshot(w.db.conn_mut(), &manifest, &dir).unwrap();

        let backups = list_backups(w.db.conn()).unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].total_files, 2);
        assert_eq!(backups[0].reason, "before organization plan");
        assert_eq!(backups[0].status, "available");

        let entries = backup_entries(w.db.conn(), backup_id).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| !e.sha256.is_empty()));

        // Operation ↔ backup are linked both ways.
        let op = operation_by_uid(w.db.conn(), &manifest.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(op.backup_id, Some(backup_id));
        assert_eq!(backups[0].operation_id, Some(op.id));
    }
}

/// Is this snapshot directory already recorded?
pub fn has_backup_at(conn: &Connection, root_path: &str) -> Result<bool, DbError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM backups WHERE root_path = ?1",
        params![root_path],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}
