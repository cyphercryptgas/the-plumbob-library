//! SQLite persistence layer.
//!
//! Ownership: all database access lives in this module tree, behind typed
//! repository functions with parameterized queries. The Tauri command layer
//! (plateau 3) calls these; the frontend never sees SQL.
//!
//! Conventions:
//! * timestamps are RFC 3339 UTC TEXT
//! * booleans are INTEGER 0/1
//! * relative paths are stored with `/` separators and compared NOCASE,
//!   matching Windows filesystem semantics
//! * every multi-row mutation runs inside a transaction

pub mod catalog;
pub mod dupes;
pub mod files;
pub mod ops;
pub mod packages;
pub mod profiles;
pub mod settings;
pub mod troubleshoot;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not prepare database location {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("migration {name} failed: {source}")]
    Migration {
        name: &'static str,
        source: rusqlite::Error,
    },
}

/// Ordered, embedded migrations. Applied transactionally; `PRAGMA
/// user_version` tracks the count applied. Never edit a shipped migration —
/// append a new one.
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_initial",
        include_str!("../../migrations/0001_initial.sql"),
    ),
    (
        "0002_seed_categories",
        include_str!("../../migrations/0002_seed_categories.sql"),
    ),
    (
        "0003_package_resources",
        include_str!("../../migrations/0003_package_resources.sql"),
    ),
    (
        "0004_troubleshoot",
        include_str!("../../migrations/0004_troubleshoot.sql"),
    ),
    (
        "0005_profiles",
        include_str!("../../migrations/0005_profiles.sql"),
    ),
];

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (creating if needed) a file-backed database, configure pragmas,
    /// and apply any pending migrations.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| DbError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let conn = Connection::open(path)?;
        configure(&conn)?;
        let mut db = Self { conn };
        db.migrate_to(MIGRATIONS.len())?;
        Ok(db)
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let mut db = Self::open_in_memory_unmigrated()?;
        db.migrate_to(MIGRATIONS.len())?;
        Ok(db)
    }

    /// In-memory database with no migrations applied — lets tests exercise
    /// the stepwise upgrade path exactly as an old install would experience.
    pub fn open_in_memory_unmigrated() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        configure(&conn)?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn schema_version(&self) -> Result<i64, DbError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    pub fn latest_schema_version() -> i64 {
        MIGRATIONS.len() as i64
    }

    /// Apply migrations up to (and including) `target`, each in its own
    /// transaction. Re-running is a no-op for already-applied migrations.
    pub fn migrate_to(&mut self, target: usize) -> Result<(), DbError> {
        let current: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, (name, sql)) in MIGRATIONS.iter().enumerate().take(target) {
            if (i as i64) < current {
                continue;
            }
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)
                .map_err(|source| DbError::Migration { name, source })?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }
}

fn configure(conn: &Connection) -> Result<(), DbError> {
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    // WAL improves concurrent read behavior on file-backed databases; on
    // in-memory databases SQLite reports a different mode, which is fine.
    let _mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub(crate) fn opt_rfc3339(t: &Option<chrono::DateTime<chrono::Utc>>) -> Option<String> {
    t.map(|v| v.to_rfc3339())
}

pub(crate) fn parse_rfc3339(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

/// Canonical database representation of a root-relative path: `/` separators
/// on every platform, so a library scanned on Windows and inspected on
/// another OS agrees with itself.
pub fn rel_to_db_string(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_migration_reaches_latest_version_with_all_tables() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(
            db.schema_version().unwrap(),
            Database::latest_schema_version()
        );
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN (
                    'mods','files','creators','categories','tags','mod_tags',
                    'collections','collection_mods','scans','scan_errors',
                    'duplicate_groups','duplicate_group_files','backups',
                    'backup_entries','quarantine_entries','operations',
                    'operation_steps','settings')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 18, "every spec §10.4 table must exist");
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate_to(MIGRATIONS.len()).unwrap();
        db.migrate_to(MIGRATIONS.len()).unwrap();
        assert_eq!(
            db.schema_version().unwrap(),
            Database::latest_schema_version()
        );
    }

    #[test]
    fn stepwise_upgrade_path_matches_fresh_install() {
        let mut db = Database::open_in_memory_unmigrated().unwrap();
        assert_eq!(db.schema_version().unwrap(), 0);
        db.migrate_to(1).unwrap();
        assert_eq!(db.schema_version().unwrap(), 1);
        // Old install upgrading later:
        db.migrate_to(MIGRATIONS.len()).unwrap();
        assert_eq!(
            db.schema_version().unwrap(),
            Database::latest_schema_version()
        );
        let cats: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .unwrap();
        assert!(cats >= 27, "category seed must apply on upgrade too");
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let db = Database::open_in_memory().unwrap();
        let err = db.conn().execute(
            "INSERT INTO files (mod_id, current_filename, absolute_path, relative_path,
                file_type, first_seen_at, last_seen_at)
             VALUES (99999, 'x.package', '/x', 'x.package', 'package', '2026', '2026')",
            [],
        );
        assert!(err.is_err(), "dangling mod_id must be rejected");
    }

    #[test]
    fn useful_indexes_exist() {
        let db = Database::open_in_memory().unwrap();
        for idx in [
            "idx_files_relative_path",
            "idx_files_sha256",
            "idx_files_size",
            "idx_operation_steps_operation",
            "idx_duplicate_group_files_file",
        ] {
            let found: i64 = db
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [idx],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(found, 1, "missing index {idx}");
        }
    }

    #[test]
    fn relative_path_uniqueness_is_case_insensitive() {
        let db = Database::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO files (current_filename, absolute_path, relative_path,
                    file_type, first_seen_at, last_seen_at)
                 VALUES ('Hair.package', '/m/Hair.package', 'CAS/Hair.package',
                    'package', '2026', '2026')",
                [],
            )
            .unwrap();
        let dup = db.conn().execute(
            "INSERT INTO files (current_filename, absolute_path, relative_path,
                file_type, first_seen_at, last_seen_at)
             VALUES ('hair.package', '/m/hair.package', 'cas/HAIR.package',
                'package', '2026', '2026')",
            [],
        );
        assert!(
            dup.is_err(),
            "NOCASE uniqueness must match Windows semantics"
        );
    }

    #[test]
    fn seeded_hierarchy_links_children_to_parents() {
        let db = Database::open_in_memory().unwrap();
        let parent: String = db
            .conn()
            .query_row(
                "SELECT p.name FROM categories c JOIN categories p ON c.parent_id = p.id
                 WHERE c.name = 'Hair'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(parent, "CAS");
        let parent: String = db
            .conn()
            .query_row(
                "SELECT p.name FROM categories c JOIN categories p ON c.parent_id = p.id
                 WHERE c.name = 'Furniture'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(parent, "Build/Buy");
    }

    #[test]
    fn rel_to_db_string_normalizes_separators() {
        let p = Path::new("CAS").join("Hair").join("curls.package");
        assert_eq!(rel_to_db_string(&p), "CAS/Hair/curls.package");
    }

    /// End-to-end regression: scan a real temp tree → reconcile → hash →
    /// detect duplicates → quarantine with the SQLite journal → verify every
    /// table tells the truth → delete a file on disk → rescan → missing.
    #[test]
    fn end_to_end_scan_to_quarantine_pipeline() {
        use crate::ops::{quarantine_files, QuarantineRequest};
        use crate::paths::SafeRoot;
        use crate::scan::{hash_files, scan, ScanOptions};
        use std::fs;
        use std::sync::atomic::AtomicBool;

        let mods_dir = tempfile::tempdir().unwrap();
        let data_dir = tempfile::tempdir().unwrap();
        fs::write(mods_dir.path().join("keeper.package"), b"same-bytes").unwrap();
        fs::create_dir_all(mods_dir.path().join("Downloads")).unwrap();
        fs::write(
            mods_dir.path().join("Downloads/keeper (1).package"),
            b"same-bytes",
        )
        .unwrap();
        fs::write(mods_dir.path().join("unique.package"), b"only-copy").unwrap();
        let quarantine_dir = data_dir.path().join("Quarantine");
        fs::create_dir_all(&quarantine_dir).unwrap();

        let mods = SafeRoot::new(mods_dir.path()).unwrap();
        let qroot = SafeRoot::new(&quarantine_dir).unwrap();
        let cancel = AtomicBool::new(false);

        let mut db = Database::open_in_memory().unwrap();

        // Scan → reconcile → hash
        let mut report = scan(&mods, &ScanOptions::default(), &cancel, |_| {});
        let summary = files::reconcile_scan(db.conn_mut(), &report, "initial", &[]).unwrap();
        assert_eq!(summary.new_files, 3);
        assert_eq!(summary.needs_hash.len(), 3);

        let hash_errors = hash_files(&mut report.files, &cancel, |_| {});
        assert!(hash_errors.is_empty());
        let updates: Vec<(i64, String)> = summary
            .needs_hash
            .iter()
            .map(|(id, abs)| {
                let f = report
                    .files
                    .iter()
                    .find(|f| &f.absolute_path == abs)
                    .unwrap();
                (*id, f.sha256.clone().unwrap())
            })
            .collect();
        files::update_hashes(db.conn_mut(), &updates).unwrap();

        // Duplicates
        let facts = dupes::load_file_facts(db.conn()).unwrap();
        let groups = crate::duplicates::group_exact(&facts);
        assert_eq!(groups.len(), 1);
        dupes::replace_exact_groups(db.conn_mut(), &groups).unwrap();
        let views = dupes::list_open_exact_groups(db.conn()).unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].members.len(), 2);
        let keep = views[0].recommended_file_id.unwrap();
        let victim = views[0].members.iter().find(|m| m.file_id != keep).unwrap();
        assert!(victim.relative_path.contains("keeper (1)"));

        // Quarantine the redundant copy, journaled straight into SQLite.
        let outcome = {
            let mut journal = ops::SqliteJournal::new(db.conn());
            let outcome = quarantine_files(
                &mods,
                &qroot,
                &[QuarantineRequest {
                    source_relative: std::path::PathBuf::from(&victim.relative_path),
                    reason: "exact duplicate".into(),
                    expected_sha256: None,
                }],
                true,
                &mut journal,
            );
            journal.finish().unwrap();
            outcome
        };
        assert_eq!(outcome.completed.len(), 1);
        ops::record_quarantine_outcome(db.conn_mut(), &outcome).unwrap();

        let op = ops::operation_by_uid(db.conn(), &outcome.operation_id)
            .unwrap()
            .expect("operation row persisted");
        assert_eq!(op.status, "completed");
        assert_eq!(op.operation_type, "quarantine");
        let q = ops::list_quarantine(db.conn(), false).unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].status, "quarantined");

        // Rescan: quarantined file must NOT be flipped to missing; the two
        // survivors are unchanged.
        let report2 = scan(&mods, &ScanOptions::default(), &cancel, |_| {});
        let summary2 = files::reconcile_scan(db.conn_mut(), &report2, "incremental", &[]).unwrap();
        assert_eq!(summary2.new_files, 0);
        assert_eq!(summary2.unchanged_files, 2);
        assert_eq!(summary2.missing_files, 0);

        // Real external deletion → missing on the next scan.
        fs::remove_file(mods_dir.path().join("unique.package")).unwrap();
        let report3 = scan(&mods, &ScanOptions::default(), &cancel, |_| {});
        let summary3 = files::reconcile_scan(db.conn_mut(), &report3, "incremental", &[]).unwrap();
        assert_eq!(summary3.missing_files, 1);
        let counts = files::library_counts(db.conn()).unwrap();
        assert_eq!(counts.missing, 1);
        assert_eq!(counts.quarantined, 1);
    }
}
