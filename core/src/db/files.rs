//! Files + scans repository: turns a [`crate::scan::ScanReport`] into
//! persistent truth, without ever letting the database claim more than the
//! filesystem verified.
//!
//! Reconciliation rules:
//! * unknown relative path → inserted (`first_seen_at` = now)
//! * known, same size + mtime → `last_seen_at`/`last_scan_id` refreshed only
//! * known, size or mtime differs → metadata updated and any stored hash
//!   **cleared** (a stale hash is worse than none); caller re-hashes
//! * known but unseen → marked missing, unless quarantined (physically moved
//!   by us, not lost) or under an excluded prefix (not scanned ≠ gone)
//! * previously missing and seen again → un-missed

use super::{now_rfc3339, opt_rfc3339, rel_to_db_string, DbError};
use crate::scan::{FileKind, ScanReport};
use serde::Serialize;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub fn kind_to_str(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Package => "package",
        FileKind::Ts4Script => "ts4script",
        FileKind::ArchiveZip => "zip",
        FileKind::ArchiveRar => "rar",
        FileKind::Archive7z => "7z",
        FileKind::Image => "image",
        FileKind::Document => "document",
        FileKind::Config => "config",
        FileKind::Unsupported => "unsupported",
    }
}

#[derive(Debug, Default)]
pub struct ReconcileSummary {
    pub scan_id: i64,
    pub new_files: usize,
    pub changed_files: usize,
    pub unchanged_files: usize,
    pub missing_files: usize,
    pub reappeared_files: usize,
    /// Files (id, absolute path) inserted or changed without a hash — the
    /// service layer runs the streaming hash pass and calls [`update_hashes`].
    pub needs_hash: Vec<(i64, PathBuf)>,
}

struct ExistingRow {
    id: i64,
    rel: String,
    size: i64,
    modified: Option<String>,
    missing: bool,
    status: String,
    enabled: bool,
}

/// Apply a scan report inside a single transaction.
pub fn reconcile_scan(
    conn: &mut Connection,
    report: &ScanReport,
    scan_type: &str,
    excluded: &[PathBuf],
) -> Result<ReconcileSummary, DbError> {
    let tx = conn.transaction()?;
    let now = now_rfc3339();
    let started = (chrono::Utc::now() - chrono::Duration::milliseconds(report.duration_ms as i64))
        .to_rfc3339();

    tx.execute(
        "INSERT INTO scans (started_at, completed_at, scan_type, files_seen, bytes_seen,
            errors, cancelled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            started,
            now,
            scan_type,
            report.files.len() as i64,
            report.total_bytes as i64,
            report.errors.len() as i64,
            report.cancelled
        ],
    )?;
    let scan_id = tx.last_insert_rowid();

    {
        let mut insert_err =
            tx.prepare("INSERT INTO scan_errors (scan_id, path, message) VALUES (?1, ?2, ?3)")?;
        for e in &report.errors {
            insert_err.execute(params![scan_id, e.path.to_string_lossy(), e.message])?;
        }
    }

    // One pass over the whole table instead of a query per scanned file.
    let mut existing: HashMap<String, ExistingRow> = HashMap::new();
    {
        let mut stmt = tx.prepare(
            "SELECT id, relative_path, size_bytes, modified_at_fs, missing, status,
                    enabled
             FROM files",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ExistingRow {
                id: r.get(0)?,
                rel: r.get(1)?,
                size: r.get(2)?,
                modified: r.get(3)?,
                missing: r.get::<_, i64>(4)? != 0,
                status: r.get(5)?,
                enabled: r.get::<_, i64>(6)? != 0,
            })
        })?;
        for row in rows {
            let row = row?;
            existing.insert(row.rel.to_lowercase(), row);
        }
    }

    let mut summary = ReconcileSummary::default();
    summary.scan_id = scan_id;
    let mut seen: HashSet<String> = HashSet::with_capacity(report.files.len());

    {
        let mut insert = tx.prepare(
            "INSERT INTO files (current_filename, original_filename, absolute_path,
                relative_path, extension, file_type, sha256, size_bytes, created_at_fs,
                modified_at_fs, first_seen_at, last_seen_at, last_scan_id, enabled,
                missing, depth, zero_byte, deep_script, status)
             VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?15, 0, ?12,
                ?13, ?14, 'current')",
        )?;
        let mut update_changed = tx.prepare(
            "UPDATE files SET current_filename = ?2, absolute_path = ?3, extension = ?4,
                file_type = ?5, sha256 = ?6, size_bytes = ?7, created_at_fs = ?8,
                modified_at_fs = ?9, last_seen_at = ?10, last_scan_id = ?11,
                missing = 0, enabled = ?15, depth = ?12, zero_byte = ?13, deep_script = ?14,
                status = CASE WHEN status = 'missing' THEN 'current' ELSE status END
             WHERE id = ?1",
        )?;
        let mut update_unchanged = tx.prepare(
            "UPDATE files SET absolute_path = ?2, last_seen_at = ?3, last_scan_id = ?4,
                missing = 0,
                status = CASE WHEN status = 'missing' THEN 'current' ELSE status END
             WHERE id = ?1",
        )?;

        // When both `X.package` and `X.package.off` exist on disk, the
        // enabled form owns the row; the disabled twin is ignored this scan
        // (still visible on disk, resolvable by the user).
        let mut chosen: HashMap<String, &crate::scan::ScannedFile> =
            HashMap::with_capacity(report.files.len());
        for f in &report.files {
            let key = rel_to_db_string(&f.relative_path).to_lowercase();
            match chosen.entry(key) {
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(f);
                }
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    if f.enabled && !o.get().enabled {
                        o.insert(f);
                    }
                }
            }
        }
        for f in chosen.values().copied() {
            let rel_s = rel_to_db_string(&f.relative_path);
            let key = rel_s.to_lowercase();
            seen.insert(key.clone());
            let mtime = opt_rfc3339(&f.modified_at);

            match existing.get(&key) {
                None => {
                    insert.execute(params![
                        f.file_name,
                        f.absolute_path.to_string_lossy(),
                        rel_s,
                        f.extension,
                        kind_to_str(f.kind),
                        f.sha256,
                        f.size_bytes as i64,
                        opt_rfc3339(&f.created_at),
                        mtime,
                        now,
                        scan_id,
                        f.depth as i64,
                        f.zero_byte,
                        f.deep_script,
                        f.enabled,
                    ])?;
                    let id = tx.last_insert_rowid();
                    summary.new_files += 1;
                    if f.sha256.is_none() {
                        summary.needs_hash.push((id, f.absolute_path.clone()));
                    }
                }
                Some(row) => {
                    if row.missing {
                        summary.reappeared_files += 1;
                    }
                    let changed = row.size != f.size_bytes as i64
                        || row.modified != mtime
                        || row.enabled != f.enabled;
                    if changed {
                        // Stale hashes are cleared unless the scanner already
                        // re-hashed this file in the same pass.
                        update_changed.execute(params![
                            row.id,
                            f.file_name,
                            f.absolute_path.to_string_lossy(),
                            f.extension,
                            kind_to_str(f.kind),
                            f.sha256,
                            f.size_bytes as i64,
                            opt_rfc3339(&f.created_at),
                            mtime,
                            now,
                            scan_id,
                            f.depth as i64,
                            f.zero_byte,
                            f.deep_script,
                            f.enabled,
                        ])?;
                        summary.changed_files += 1;
                        if f.sha256.is_none() {
                            summary.needs_hash.push((row.id, f.absolute_path.clone()));
                        }
                    } else {
                        update_unchanged.execute(params![
                            row.id,
                            f.absolute_path.to_string_lossy(),
                            now,
                            scan_id
                        ])?;
                        summary.unchanged_files += 1;
                    }
                }
            }
        }

        // Missing pass — only for rows that were genuinely eligible to be
        // seen by this scan. A cancelled scan skips it entirely: files the
        // walker never reached are unvisited, not gone.
        if !report.cancelled {
            let mut mark_missing =
                tx.prepare("UPDATE files SET missing = 1, status = 'missing' WHERE id = ?1")?;
            for (key, row) in &existing {
                if seen.contains(key) || row.missing || row.status == "quarantined" {
                    continue;
                }
                let under_excluded = excluded
                    .iter()
                    .any(|ex| Path::new(&row.rel).starts_with(ex));
                if under_excluded {
                    continue;
                }
                mark_missing.execute(params![row.id])?;
                summary.missing_files += 1;
            }
        }
    }

    tx.commit()?;
    Ok(summary)
}

/// Persist hashes computed by the streaming pass. Transactional.
pub fn update_hashes(conn: &mut Connection, hashes: &[(i64, String)]) -> Result<(), DbError> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare("UPDATE files SET sha256 = ?2 WHERE id = ?1")?;
        for (id, hash) in hashes {
            stmt.execute(params![id, hash])?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCounts {
    pub total_files: i64,
    pub total_bytes: i64,
    pub missing: i64,
    pub zero_byte: i64,
    pub unsupported: i64,
    pub archives: i64,
    pub deep_scripts: i64,
    pub packages: i64,
    pub scripts: i64,
    pub quarantined: i64,
    pub disabled: i64,
}

/// Dashboard aggregates in a single indexed pass.
pub fn library_counts(conn: &Connection) -> Result<LibraryCounts, DbError> {
    Ok(conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(size_bytes), 0),
                COALESCE(SUM(missing), 0),
                COALESCE(SUM(zero_byte), 0),
                COALESCE(SUM(CASE WHEN file_type = 'unsupported' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN file_type IN ('zip','rar','7z') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(deep_script), 0),
                COALESCE(SUM(CASE WHEN file_type = 'package' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN file_type = 'ts4script' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'quarantined' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN enabled = 0 AND status = 'current' THEN 1 ELSE 0 END), 0)
         FROM files",
        [],
        |r| {
            Ok(LibraryCounts {
                total_files: r.get(0)?,
                total_bytes: r.get(1)?,
                missing: r.get(2)?,
                zero_byte: r.get(3)?,
                unsupported: r.get(4)?,
                archives: r.get(5)?,
                deep_scripts: r.get(6)?,
                packages: r.get(7)?,
                scripts: r.get(8)?,
                quarantined: r.get(9)?,
                disabled: r.get(10)?,
            })
        },
    )?)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    pub id: i64,
    pub relative_path: String,
    pub absolute_path: String,
    pub current_filename: String,
    pub file_type: String,
    pub size_bytes: i64,
    pub sha256: Option<String>,
    pub status: String,
    pub missing: bool,
    pub zero_byte: bool,
    pub deep_script: bool,
    pub depth: i64,
    pub modified_at_fs: Option<String>,
    pub mod_id: Option<i64>,
    pub parse_status: Option<String>,
    pub enabled: bool,
    pub category: Option<String>,
    pub cas_subcategory: Option<String>,
    pub creator: Option<String>,
    pub creator_display: Option<String>,
}

const FILE_ROW_COLUMNS: &str = "id, relative_path, absolute_path, current_filename, file_type,
    size_bytes, sha256, status, missing, zero_byte, deep_script, depth, modified_at_fs,
    mod_id, parse_status, enabled, category, cas_subcategory, creator, creator_display";

fn map_file_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FileRow> {
    Ok(FileRow {
        id: r.get(0)?,
        relative_path: r.get(1)?,
        absolute_path: r.get(2)?,
        current_filename: r.get(3)?,
        file_type: r.get(4)?,
        size_bytes: r.get(5)?,
        sha256: r.get(6)?,
        status: r.get(7)?,
        missing: r.get::<_, i64>(8)? != 0,
        zero_byte: r.get::<_, i64>(9)? != 0,
        deep_script: r.get::<_, i64>(10)? != 0,
        depth: r.get(11)?,
        modified_at_fs: r.get(12)?,
        mod_id: r.get(13)?,
        parse_status: r.get(14)?,
        enabled: r.get::<_, i64>(15)? != 0,
        category: r.get(16)?,
        cas_subcategory: r.get(17)?,
        creator: r.get(18)?,
        creator_display: r.get(19)?,
    })
}

/// Named status filters for the Library screen. Fixed SQL fragments only —
/// the filter name never reaches SQL as data.
fn filter_clause(filter: Option<&str>) -> Result<&'static str, DbError> {
    Ok(match filter.unwrap_or("all") {
        "all" | "" => "1=1",
        "packages" => "file_type = 'package'",
        "scripts" => "file_type = 'ts4script'",
        "archives" => "file_type IN ('zip','rar','7z')",
        "zero-byte" => "zero_byte = 1",
        "deep-scripts" => "deep_script = 1",
        "missing" => "missing = 1",
        "quarantined" => "status = 'quarantined'",
        "disabled" => "enabled = 0 AND status = 'current'",
        "cat_cas" => "category = 'cas'",
        "sub_hats" => "cas_subcategory = 'hats'",
        "sub_hair" => "cas_subcategory = 'hair'",
        "sub_face" => "cas_subcategory = 'face'",
        "sub_fullbody" => "cas_subcategory = 'fullbody'",
        "sub_tops" => "cas_subcategory = 'tops'",
        "sub_bottoms" => "cas_subcategory = 'bottoms'",
        "sub_shoes" => "cas_subcategory = 'shoes'",
        "sub_accessories" => "cas_subcategory = 'accessories'",
        "sub_skin" => "cas_subcategory = 'skin'",
        "sub_other" => "cas_subcategory = 'other'",
        "cat_buildbuy" => "category = 'buildbuy'",
        "cat_animations" => "category = 'animations'",
        "cat_gameplay" => "category = 'gameplay'",
        "cat_scripts" => "category = 'scripts'",
        "cat_other" => "category = 'other'",
        // File dates are RFC 3339 ('T' separator); date('now') is plain
        // YYYY-MM-DD — comparing on substr keeps both sides day-precision.
        // modified_at_fs is the file's own date (creator builds span years);
        // first_seen_at only backstops the rare null.
        "date_7" => "substr(COALESCE(modified_at_fs, first_seen_at), 1, 10) >= date('now', '-7 days')",
        "date_30" => "substr(COALESCE(modified_at_fs, first_seen_at), 1, 10) >= date('now', '-30 days')",
        "date_90" => "substr(COALESCE(modified_at_fs, first_seen_at), 1, 10) >= date('now', '-90 days')",
        "date_old" => "substr(COALESCE(modified_at_fs, first_seen_at), 1, 10) < date('now', '-90 days')",
        "unreadable" => "parse_status IS NOT NULL AND parse_status != 'ok'",
        other => {
            return Err(DbError::Sqlite(rusqlite::Error::InvalidParameterName(
                format!("unknown library filter: {other}"),
            )))
        }
    })
}

/// Paginated listing with optional case-insensitive substring search and an
/// optional named status filter.
pub fn list_files(
    conn: &Connection,
    search: Option<&str>,
    filter: Option<&str>,
    creator: Option<&str>,
    sort: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<FileRow>, DbError> {
    let clause = filter_clause(filter)?;
    // Sort keys are matched here, never interpolated from user text.
    // "Date" means the file's own date: an imported library is "first seen"
    // all at once, but modified_at_fs spans years of creator builds.
    let order = match sort.unwrap_or("name") {
        "added_desc" => {
            "COALESCE(modified_at_fs, first_seen_at) DESC, relative_path COLLATE NOCASE"
        }
        "added_asc" => {
            "COALESCE(modified_at_fs, first_seen_at) ASC, relative_path COLLATE NOCASE"
        }
        _ => "relative_path COLLATE NOCASE",
    };
    let sql = format!(
        "SELECT {FILE_ROW_COLUMNS}
         FROM files
         WHERE (?1 IS NULL OR relative_path LIKE '%' || ?1 || '%')
           AND (?2 IS NULL OR creator = ?2) AND {clause}
         ORDER BY {order}
         LIMIT ?3 OFFSET ?4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![search, creator, limit, offset], map_file_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Total row count for the same search + filter, for honest paging labels.
pub fn count_files(
    conn: &Connection,
    search: Option<&str>,
    filter: Option<&str>,
    creator: Option<&str>,
) -> Result<i64, DbError> {
    let clause = filter_clause(filter)?;
    let sql = format!(
        "SELECT COUNT(*) FROM files
         WHERE (?1 IS NULL OR relative_path LIKE '%' || ?1 || '%')
           AND (?2 IS NULL OR creator = ?2) AND {clause}"
    );
    Ok(conn.query_row(&sql, params![search, creator], |r| r.get(0))?)
}

/// Fetch specific rows by id (e.g. a quarantine selection). Order follows
/// relative path; ids that don't exist are simply absent from the result.
pub fn files_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<FileRow>, DbError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT {FILE_ROW_COLUMNS} FROM files WHERE id IN ({placeholders})
         ORDER BY relative_path COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), map_file_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Look up a file id by its root-relative path (NOCASE, normalized `/`).
pub fn file_id_by_relative_path(conn: &Connection, rel: &Path) -> Result<Option<i64>, DbError> {
    let rel_s = rel_to_db_string(rel);
    let mut stmt = conn.prepare("SELECT id FROM files WHERE relative_path = ?1")?;
    let mut rows = stmt.query_map(params![rel_s], |r| r.get::<_, i64>(0))?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::scan::{FileKind, ScanIssue, ScannedFile};
    use chrono::{TimeZone, Utc};

    fn mk_file(rel: &str, size: u64, mtime_min: u32, kind: FileKind) -> ScannedFile {
        let modified = Utc.with_ymd_and_hms(2026, 7, 1, 12, mtime_min, 0).unwrap();
        ScannedFile {
            absolute_path: PathBuf::from(format!("/mods/{rel}")),
            relative_path: PathBuf::from(rel),
            file_name: Path::new(rel)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            extension: Path::new(rel)
                .extension()
                .map(|e| e.to_string_lossy().into_owned()),
            kind,
            size_bytes: size,
            modified_at: Some(modified),
            created_at: None,
            depth: Path::new(rel).components().count().saturating_sub(1),
            zero_byte: size == 0,
            deep_script: false,
            sha256: None,
            enabled: true,
        }
    }

    fn report(files: Vec<ScannedFile>) -> ScanReport {
        let total_bytes = files.iter().map(|f| f.size_bytes).sum();
        ScanReport {
            files,
            empty_dirs: vec![],
            symlinks_skipped: vec![],
            errors: vec![],
            cancelled: false,
            total_bytes,
            duration_ms: 5,
        }
    }

    #[test]
    fn first_scan_inserts_everything_as_new() {
        let mut db = Database::open_in_memory().unwrap();
        let r = report(vec![
            mk_file("a.package", 10, 0, FileKind::Package),
            mk_file("CAS/Hair/b.package", 20, 1, FileKind::Package),
        ]);
        let s = reconcile_scan(db.conn_mut(), &r, "initial", &[]).unwrap();
        assert_eq!(s.new_files, 2);
        assert_eq!(s.needs_hash.len(), 2);
        assert_eq!(s.changed_files + s.unchanged_files + s.missing_files, 0);

        let stored: String = db
            .conn()
            .query_row(
                "SELECT relative_path FROM files WHERE current_filename = 'b.package'",
                [],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(stored, "CAS/Hair/b.package");
    }

    #[test]
    fn unchanged_files_refresh_last_seen_without_duplication() {
        let mut db = Database::open_in_memory().unwrap();
        let r = report(vec![mk_file("a.package", 10, 0, FileKind::Package)]);
        reconcile_scan(db.conn_mut(), &r, "initial", &[]).unwrap();
        let s2 = reconcile_scan(db.conn_mut(), &r, "incremental", &[]).unwrap();
        assert_eq!(s2.new_files, 0);
        assert_eq!(s2.unchanged_files, 1);
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM files", [], |x| x.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn changed_files_clear_stale_hashes_and_request_rehash() {
        let mut db = Database::open_in_memory().unwrap();
        let r1 = report(vec![mk_file("a.package", 10, 0, FileKind::Package)]);
        let s1 = reconcile_scan(db.conn_mut(), &r1, "initial", &[]).unwrap();
        update_hashes(db.conn_mut(), &[(s1.needs_hash[0].0, "oldhash".into())]).unwrap();

        // Same path, new size + mtime → changed.
        let r2 = report(vec![mk_file("a.package", 999, 5, FileKind::Package)]);
        let s2 = reconcile_scan(db.conn_mut(), &r2, "incremental", &[]).unwrap();
        assert_eq!(s2.changed_files, 1);
        assert_eq!(s2.needs_hash.len(), 1, "changed file must be re-hashed");

        let hash: Option<String> = db
            .conn()
            .query_row("SELECT sha256 FROM files", [], |x| x.get(0))
            .unwrap();
        assert!(hash.is_none(), "a stale hash is worse than none");
    }

    #[test]
    fn unseen_files_become_missing_and_reappearance_heals_them() {
        let mut db = Database::open_in_memory().unwrap();
        let both = report(vec![
            mk_file("a.package", 10, 0, FileKind::Package),
            mk_file("b.package", 20, 1, FileKind::Package),
        ]);
        reconcile_scan(db.conn_mut(), &both, "initial", &[]).unwrap();

        let only_a = report(vec![mk_file("a.package", 10, 0, FileKind::Package)]);
        let s2 = reconcile_scan(db.conn_mut(), &only_a, "incremental", &[]).unwrap();
        assert_eq!(s2.missing_files, 1);
        let status: String = db
            .conn()
            .query_row(
                "SELECT status FROM files WHERE current_filename = 'b.package'",
                [],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(status, "missing");

        let s3 = reconcile_scan(db.conn_mut(), &both, "incremental", &[]).unwrap();
        assert_eq!(s3.reappeared_files, 1);
        assert_eq!(s3.missing_files, 0);
        let (missing, status): (i64, String) = db
            .conn()
            .query_row(
                "SELECT missing, status FROM files WHERE current_filename = 'b.package'",
                [],
                |x| Ok((x.get(0)?, x.get(1)?)),
            )
            .unwrap();
        assert_eq!(missing, 0);
        assert_eq!(status, "current");
    }

    #[test]
    fn excluded_prefixes_do_not_produce_false_missing() {
        let mut db = Database::open_in_memory().unwrap();
        let both = report(vec![
            mk_file("a.package", 10, 0, FileKind::Package),
            mk_file("Disabled/b.package", 20, 1, FileKind::Package),
        ]);
        reconcile_scan(db.conn_mut(), &both, "initial", &[]).unwrap();

        // Next scan excludes Disabled/ — its files were not scanned, which is
        // not the same thing as gone.
        let only_a = report(vec![mk_file("a.package", 10, 0, FileKind::Package)]);
        let s = reconcile_scan(
            db.conn_mut(),
            &only_a,
            "incremental",
            &[PathBuf::from("Disabled")],
        )
        .unwrap();
        assert_eq!(s.missing_files, 0);
    }

    #[test]
    fn case_variant_paths_reconcile_to_the_same_row() {
        let mut db = Database::open_in_memory().unwrap();
        let r1 = report(vec![mk_file("CAS/Hair.package", 10, 0, FileKind::Package)]);
        reconcile_scan(db.conn_mut(), &r1, "initial", &[]).unwrap();
        // Windows may report different casing between scans.
        let r2 = report(vec![mk_file("cas/hair.package", 10, 0, FileKind::Package)]);
        let s2 = reconcile_scan(db.conn_mut(), &r2, "incremental", &[]).unwrap();
        assert_eq!(s2.new_files, 0);
        assert_eq!(s2.unchanged_files, 1);
        assert_eq!(s2.missing_files, 0);
    }

    #[test]
    fn scan_errors_are_persisted_with_the_scan() {
        let mut db = Database::open_in_memory().unwrap();
        let mut r = report(vec![]);
        r.errors.push(ScanIssue {
            path: PathBuf::from("/mods/Locked"),
            message: "permission denied".into(),
        });
        let s = reconcile_scan(db.conn_mut(), &r, "initial", &[]).unwrap();
        let (count, msg): (i64, String) = db
            .conn()
            .query_row(
                "SELECT COUNT(*), MAX(message) FROM scan_errors WHERE scan_id = ?1",
                [s.scan_id],
                |x| Ok((x.get(0)?, x.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(msg, "permission denied");
    }

    #[test]
    fn library_counts_aggregate_flags_correctly() {
        let mut db = Database::open_in_memory().unwrap();
        let r = report(vec![
            mk_file("a.package", 10, 0, FileKind::Package),
            mk_file("s.ts4script", 5, 1, FileKind::Ts4Script),
            mk_file("z.zip", 7, 2, FileKind::ArchiveZip),
            mk_file("junk.xyz", 3, 3, FileKind::Unsupported),
            mk_file("empty.package", 0, 4, FileKind::Package),
        ]);
        reconcile_scan(db.conn_mut(), &r, "initial", &[]).unwrap();
        let c = library_counts(db.conn()).unwrap();
        assert_eq!(c.total_files, 5);
        assert_eq!(c.total_bytes, 25);
        assert_eq!(c.zero_byte, 1);
        assert_eq!(c.unsupported, 1);
        assert_eq!(c.archives, 1);
        assert_eq!(c.packages, 2);
        assert_eq!(c.scripts, 1);
        assert_eq!(c.quarantined, 0);
    }

    #[test]
    fn list_files_searches_and_paginates() {
        let mut db = Database::open_in_memory().unwrap();
        let r = report(vec![
            mk_file("CAS/Hair/curls.package", 10, 0, FileKind::Package),
            mk_file("CAS/Hair/waves.package", 10, 1, FileKind::Package),
            mk_file("BuildBuy/sofa.package", 10, 2, FileKind::Package),
        ]);
        reconcile_scan(db.conn_mut(), &r, "initial", &[]).unwrap();
        let hair = list_files(db.conn(), Some("hair"), None, None, None, 50, 0).unwrap();
        assert_eq!(hair.len(), 2);
        let page = list_files(db.conn(), None, None, None, None, 2, 0).unwrap();
        assert_eq!(page.len(), 2);
        let rest = list_files(db.conn(), None, None, None, None, 2, 2).unwrap();
        assert_eq!(rest.len(), 1);
    }

    #[test]
    fn creators_overview_counts_files_and_curse_matches() {
        let mut db = Database::open_in_memory().unwrap();
        let r = report(vec![
            mk_file("KUTTOE_a.package", 10, 0, FileKind::Package),
            mk_file("KUTTOE_b.package", 10, 1, FileKind::Package),
            mk_file("loose.package", 10, 2, FileKind::Package),
        ]);
        reconcile_scan(db.conn_mut(), &r, "initial", &[]).unwrap();
        let ids: Vec<i64> = list_files(db.conn(), None, None, None, None, 50, 0)
            .unwrap()
            .into_iter()
            .map(|f| f.id)
            .collect();
        set_creator(db.conn(), ids[0], "kuttoe", "KUTTOE").unwrap();
        set_creator(db.conn(), ids[1], "kuttoe", "KUTTOE").unwrap();
        set_creator(db.conn(), ids[2], "", "").unwrap();
        crate::db::curse::replace_matches(
            db.conn_mut(),
            &[crate::db::curse::MatchRecord {
                file_id: ids[0],
                curse_mod_id: 7,
                curse_file_id: None,
                mod_name: "Kuttoe Traits".into(),
                website_url: None,
                matched_file_name: None,
                matched_file_date: None,
                latest_file_id: 1,
                latest_file_name: "x".into(),
                latest_file_date: "2026-01-01T00:00:00Z".into(),
                update_available: false,
                match_kind: "name",
                confidence: Some(0.9),
                allow_distribution: None,
            }],
        )
        .unwrap();
        let overview = creators_overview(db.conn()).unwrap();
        assert_eq!(overview.len(), 1, "uncredited rows excluded");
        assert_eq!(overview[0].key, "kuttoe");
        assert_eq!(overview[0].files, 2);
        assert_eq!(overview[0].on_curse, 1);
        let only = list_files(db.conn(), None, None, Some("kuttoe"), None, 50, 0).unwrap();
        assert_eq!(only.len(), 2);
        assert_eq!(count_files(db.conn(), None, None, Some("kuttoe")).unwrap(), 2);
    }

    #[test]
    fn named_filters_narrow_and_count_matches() {
        let mut db = Database::open_in_memory().unwrap();
        let mut zero = mk_file("empty.package", 0, 0, FileKind::Package);
        zero.zero_byte = true;
        let mut deep = mk_file("A/B/deep.ts4script", 10, 2, FileKind::Ts4Script);
        deep.deep_script = true;
        let r = report(vec![
            zero,
            deep,
            mk_file("fine.package", 10, 0, FileKind::Package),
            mk_file("bundle.zip", 10, 0, FileKind::ArchiveZip),
        ]);
        reconcile_scan(db.conn_mut(), &r, "initial", &[]).unwrap();

        let zeroes = list_files(db.conn(), None, Some("zero-byte"), None, None, 50, 0).unwrap();
        assert_eq!(zeroes.len(), 1);
        assert_eq!(zeroes[0].relative_path, "empty.package");
        assert_eq!(count_files(db.conn(), None, Some("zero-byte"), None).unwrap(), 1);

        assert_eq!(
            list_files(db.conn(), None, Some("deep-scripts"), None, None, 50, 0)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(count_files(db.conn(), None, Some("archives"), None).unwrap(), 1);
        assert_eq!(count_files(db.conn(), None, Some("packages"), None).unwrap(), 2);
        assert!(list_files(db.conn(), None, Some("nonsense"), None, None, 50, 0).is_err());
    }

    #[test]
    fn cancelled_scans_never_mark_unreached_files_missing() {
        let mut db = Database::open_in_memory().unwrap();
        let both = report(vec![
            mk_file("a.package", 10, 0, FileKind::Package),
            mk_file("b.package", 20, 1, FileKind::Package),
        ]);
        reconcile_scan(db.conn_mut(), &both, "initial", &[]).unwrap();

        // The user cancelled after the walker saw only `a` — `b` was simply
        // never reached and must NOT be declared missing.
        let mut partial = report(vec![mk_file("a.package", 10, 0, FileKind::Package)]);
        partial.cancelled = true;
        let s = reconcile_scan(db.conn_mut(), &partial, "incremental", &[]).unwrap();
        assert_eq!(s.missing_files, 0);
        let status: String = db
            .conn()
            .query_row(
                "SELECT status FROM files WHERE current_filename = 'b.package'",
                [],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(status, "current");
    }

    #[test]
    fn files_by_ids_fetches_exact_rows() {
        let mut db = Database::open_in_memory().unwrap();
        let r = report(vec![
            mk_file("a.package", 10, 0, FileKind::Package),
            mk_file("b.package", 20, 1, FileKind::Package),
            mk_file("c.package", 30, 2, FileKind::Package),
        ]);
        let s = reconcile_scan(db.conn_mut(), &r, "initial", &[]).unwrap();
        let ids: Vec<i64> = s.needs_hash.iter().map(|(id, _)| *id).collect();
        let picked = files_by_ids(db.conn(), &ids[..2]).unwrap();
        assert_eq!(picked.len(), 2);
        assert!(files_by_ids(db.conn(), &[]).unwrap().is_empty());
        assert!(files_by_ids(db.conn(), &[9999]).unwrap().is_empty());
    }

    #[test]
    fn transaction_rolls_back_when_reconcile_fails_midway() {
        let mut db = Database::open_in_memory().unwrap();
        // A poisoned trigger guarantees a failure partway through the batch,
        // proving the transaction unwinds file rows AND the scan row itself.
        db.conn()
            .execute_batch(
                "CREATE TRIGGER poison BEFORE INSERT ON files
                 WHEN NEW.current_filename = 'boom.package'
                 BEGIN SELECT RAISE(ABORT, 'poisoned'); END;",
            )
            .unwrap();
        let r = report(vec![
            mk_file("ok.package", 10, 0, FileKind::Package),
            mk_file("boom.package", 11, 1, FileKind::Package),
        ]);
        let err = reconcile_scan(db.conn_mut(), &r, "initial", &[]);
        assert!(err.is_err());
        let files: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM files", [], |x| x.get(0))
            .unwrap();
        let scans: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM scans", [], |x| x.get(0))
            .unwrap();
        assert_eq!(files, 0, "partial scan writes must roll back");
        assert_eq!(scans, 0, "the scan row itself must roll back");
    }
}

#[cfg(test)]
mod disabled_mod_tests {
    use super::*;
    use crate::db::Database;
    use crate::db::ops::record_toggle_outcome;
    use crate::scan::{scan, split_disabled, FileKind, ScanOptions};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    fn scan_into(db: &mut Database, root: &crate::paths::SafeRoot) -> ReconcileSummary {
        let report = scan(root, &ScanOptions::default(), &AtomicBool::new(false), |_| {});
        reconcile_scan(db.conn_mut(), &report, "test", &[]).unwrap()
    }

    fn row(db: &Database, rel: &str) -> FileRow {
        let id: i64 = db
            .conn()
            .query_row(
                "SELECT id FROM files WHERE relative_path = ?1",
                [rel],
                |r| r.get(0),
            )
            .unwrap();
        files_by_ids(db.conn(), &[id]).unwrap().remove(0)
    }

    #[test]
    fn split_disabled_recognizes_only_mods() {
        assert_eq!(
            split_disabled("Foo.package.off"),
            Some(("Foo.package", FileKind::Package))
        );
        assert_eq!(
            split_disabled("mc.ts4script.off"),
            Some(("mc.ts4script", FileKind::Ts4Script))
        );
        assert_eq!(split_disabled("notes.txt.off"), None);
        assert_eq!(split_disabled("Foo.package"), None);
        assert_eq!(split_disabled(".off"), None);
    }

    #[test]
    fn a_pre_disabled_file_scans_under_its_logical_identity() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("cc")).unwrap();
        fs::write(tmp.path().join("cc/Hair.package.off"), b"payload").unwrap();
        let root = crate::paths::SafeRoot::new(tmp.path()).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        scan_into(&mut db, &root);
        let r = row(&db, "cc/Hair.package");
        assert!(!r.enabled);
        assert_eq!(r.file_type, "package");
        assert_eq!(r.current_filename, "Hair.package.off");
        assert!(r.absolute_path.ends_with("Hair.package.off"));
        assert_eq!(library_counts(db.conn()).unwrap().disabled, 1);
    }

    #[test]
    fn scans_sync_manual_renames_both_directions() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("A.package"), b"payload-a").unwrap();
        let root = crate::paths::SafeRoot::new(tmp.path()).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        scan_into(&mut db, &root);
        assert!(row(&db, "A.package").enabled);

        fs::rename(tmp.path().join("A.package"), tmp.path().join("A.package.off")).unwrap();
        let s = scan_into(&mut db, &root);
        assert_eq!(s.missing_files, 0, "a manual disable is not a disappearance");
        let r = row(&db, "A.package");
        assert!(!r.enabled);
        assert_eq!(r.current_filename, "A.package.off");

        fs::rename(tmp.path().join("A.package.off"), tmp.path().join("A.package")).unwrap();
        scan_into(&mut db, &root);
        assert!(row(&db, "A.package").enabled);
    }

    #[test]
    fn missing_means_neither_physical_form_exists() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("B.package"), b"payload-b").unwrap();
        let root = crate::paths::SafeRoot::new(tmp.path()).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        scan_into(&mut db, &root);
        fs::remove_file(tmp.path().join("B.package")).unwrap();
        let s = scan_into(&mut db, &root);
        assert_eq!(s.missing_files, 1);
        assert!(row(&db, "B.package").missing);
    }

    #[test]
    fn when_both_forms_exist_the_enabled_one_owns_the_row() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("C.package"), b"enabled-form").unwrap();
        fs::write(tmp.path().join("C.package.off"), b"disabled-twin").unwrap();
        let root = crate::paths::SafeRoot::new(tmp.path()).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        scan_into(&mut db, &root);
        let r = row(&db, "C.package");
        assert!(r.enabled);
        assert_eq!(r.current_filename, "C.package");
        let total: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 1, "the twin never becomes a second row");
    }

    #[test]
    fn toggle_roundtrip_is_verified_and_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("D.package"), b"precious-bytes").unwrap();
        let original = crate::hashing::sha256_file(&tmp.path().join("D.package")).unwrap();
        let root = crate::paths::SafeRoot::new(tmp.path()).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        scan_into(&mut db, &root);
        let sha = row(&db, "D.package").sha256;

        let mut j = crate::ops::VecJournal::default();
        let req = crate::ops::ToggleRequest {
            relative_path: PathBuf::from("D.package"),
            expected_sha256: sha.clone(),
        };
        let out = crate::ops::set_files_enabled(&root, &[req.clone()], false, "mods_disable", true, &mut j);
        assert_eq!(out.completed.len(), 1);
        assert!(tmp.path().join("D.package.off").exists());
        assert!(!tmp.path().join("D.package").exists());
        record_toggle_outcome(db.conn_mut(), &out).unwrap();
        let r = row(&db, "D.package");
        assert!(!r.enabled);
        assert_eq!(r.current_filename, "D.package.off");

        let back = crate::ops::set_files_enabled(&root, &[req], true, "mods_enable", true, &mut j);
        assert_eq!(back.completed.len(), 1);
        record_toggle_outcome(db.conn_mut(), &back).unwrap();
        let r = row(&db, "D.package");
        assert!(r.enabled);
        assert_eq!(
            crate::hashing::sha256_file(&tmp.path().join("D.package")).unwrap(),
            original,
            "the bytes come home identical"
        );
    }

    #[test]
    fn toggling_refuses_when_the_target_name_is_occupied() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("E.package"), b"real").unwrap();
        fs::write(tmp.path().join("E.package.off"), b"stray").unwrap();
        let root = crate::paths::SafeRoot::new(tmp.path()).unwrap();
        let mut j = crate::ops::VecJournal::default();
        let out = crate::ops::set_files_enabled(
            &root,
            &[crate::ops::ToggleRequest {
                relative_path: PathBuf::from("E.package"),
                expected_sha256: None,
            }],
            false,
            "mods_disable",
            true,
            &mut j,
        );
        assert!(out.completed.is_empty());
        assert_eq!(out.failed.len(), 1);
        assert_eq!(
            fs::read(tmp.path().join("E.package")).unwrap(),
            b"real",
            "nothing moved"
        );
    }

    #[test]
    fn a_stale_hash_refuses_to_toggle() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("F.package"), b"before").unwrap();
        let stale = crate::hashing::sha256_file(&tmp.path().join("F.package")).unwrap();
        let root = crate::paths::SafeRoot::new(tmp.path()).unwrap();
        fs::write(tmp.path().join("F.package"), b"tampered!").unwrap();
        let mut j = crate::ops::VecJournal::default();
        let out = crate::ops::set_files_enabled(
            &root,
            &[crate::ops::ToggleRequest {
                relative_path: PathBuf::from("F.package"),
                expected_sha256: Some(stale),
            }],
            false,
            "mods_disable",
            true,
            &mut j,
        );
        assert_eq!(out.failed.len(), 1);
        assert!(tmp.path().join("F.package").exists(), "refused, not moved");
    }
}

/// Every current, present package with its on-disk path — the thumbnail
/// prewarm's worklist.
pub fn package_paths(conn: &Connection) -> Result<Vec<(i64, String)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, absolute_path FROM files
         WHERE file_type = 'package' AND missing = 0 AND status = 'current'
         ORDER BY relative_path COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// CAS packages whose subcategory hasn't been read yet.
pub fn cas_needing_subcategory(conn: &Connection) -> Result<Vec<(i64, String)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, absolute_path FROM files
         WHERE category = 'cas' AND cas_subcategory IS NULL
           AND missing = 0 AND status = 'current'",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_cas_subcategory(conn: &Connection, id: i64, sub: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET cas_subcategory = ?2 WHERE id = ?1",
        params![id, sub],
    )?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorRow {
    pub key: String,
    pub display: String,
    pub files: i64,
    /// Files by this creator matched on CurseForge (name radar or exact) —
    /// the fingerprint/identity join, surfaced.
    pub on_curse: i64,
}

pub fn creators_overview(conn: &Connection) -> Result<Vec<CreatorRow>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT f.creator, COALESCE(MAX(f.creator_display), f.creator),
                COUNT(*), COUNT(m.file_id)
         FROM files f
         LEFT JOIN curse_matches m ON m.file_id = f.id
         WHERE f.creator IS NOT NULL AND f.creator <> ''
           AND f.missing = 0 AND f.status = 'current'
         GROUP BY f.creator
         ORDER BY COUNT(*) DESC, f.creator",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(CreatorRow {
                key: r.get(0)?,
                display: r.get(1)?,
                files: r.get(2)?,
                on_curse: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Every current file's name, with whether creator attribution is still
/// pending — the whole library feeds frequency promotion each scan.
pub fn creator_worklist(conn: &Connection) -> Result<Vec<(i64, String, bool)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, current_filename, creator IS NULL FROM files
         WHERE missing = 0 AND status = 'current'",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_creator(
    conn: &Connection,
    id: i64,
    key: &str,
    display: &str,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET creator = ?2, creator_display = ?3 WHERE id = ?1",
        params![id, key, display],
    )?;
    Ok(())
}

/// The paths the updater needs to swap one file safely.
pub fn file_paths(
    conn: &Connection,
    id: i64,
) -> Result<Option<(String, String, String)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT absolute_path, relative_path, current_filename
         FROM files WHERE id = ?1 AND missing = 0 AND status = 'current'",
    )?;
    let row = stmt
        .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .next()
        .transpose()?;
    Ok(row)
}

/// A file the user removed through an app action (merge, etc.) — the
/// same verdict a scan would reach, recorded immediately.
pub fn mark_removed(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET missing = 1, status = 'missing' WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}
