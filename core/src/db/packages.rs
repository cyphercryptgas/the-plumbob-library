//! Package-resource storage and conflict detection (Phase 2).
//!
//! The parse pass is read-only and **content-keyed incremental**: a package
//! is (re)indexed only when its sha256 differs from the fingerprint the
//! stored index was read from. Unchanged files cost nothing on rescans, and
//! a corrupt file is retried only if its bytes change.
//!
//! Conflict semantics implement the researched noise policy
//! (docs/RESEARCH.md, "Phase 2 research"):
//!
//! * a conflict is a resource key present in **2+ packages with differing
//!   content** — members that are byte-identical files are the Duplicates
//!   feature's territory, and a key shared only among identical files makes
//!   no in-game difference;
//! * groups whose shared keys are all presentation-only (images/thumbnails)
//!   are labeled low severity;
//! * groups whose members share a mod link or top-level folder are flagged
//!   likely intentional (overrides within one mod are usually by design);
//! * members are ordered by relative path (case-insensitive), matching the
//!   community understanding that load order is name-based; the last member
//!   is the presumptive winner. This is an approximation and is labeled as
//!   such in the interface.

use crate::dbpf::{self, DbpfError, PackageIndex};
use crate::paths::SafeRoot;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::DbError;

/// Files whose stored index is missing or stale relative to their current
/// content fingerprint. Only healthy, present, hashed packages qualify.
pub fn files_needing_parse(conn: &Connection) -> Result<Vec<(i64, String)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, relative_path FROM files
         WHERE file_type = 'package'
           AND status = 'current' AND missing = 0 AND zero_byte = 0
           AND sha256 IS NOT NULL
           AND (parsed_sha256 IS NULL OR parsed_sha256 != sha256)
         ORDER BY relative_path COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Store a successfully read index for a file, replacing any previous rows,
/// and stamp the parse as belonging to the file's current fingerprint.
pub fn record_package_index(
    conn: &Connection,
    file_id: i64,
    index: &PackageIndex,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM package_resources WHERE file_id = ?1",
        params![file_id],
    )?;
    {
        let mut insert = conn.prepare(
            "INSERT INTO package_resources (file_id, type_id, group_id, instance)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for key in &index.keys {
            insert.execute(params![
                file_id,
                key.type_id as i64,
                key.group_id as i64,
                key.instance as i64
            ])?;
        }
    }
    conn.execute(
        "UPDATE files SET resource_count = ?2, parsed_sha256 = sha256,
                          parse_status = 'ok', parse_error = NULL
         WHERE id = ?1",
        params![file_id, index.keys.len() as i64],
    )?;
    Ok(())
}

/// Record a parse failure. The failure is stamped against the current
/// fingerprint so the file is not futilely re-parsed every scan; any stale
/// rows from an older version of the file are removed.
pub fn record_parse_error(
    conn: &Connection,
    file_id: i64,
    error: &DbpfError,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM package_resources WHERE file_id = ?1",
        params![file_id],
    )?;
    conn.execute(
        "UPDATE files SET resource_count = NULL, parsed_sha256 = sha256,
                          parse_status = ?2, parse_error = ?3
         WHERE id = ?1",
        params![file_id, parse_error_kind(error), error.to_string()],
    )?;
    Ok(())
}

fn parse_error_kind(error: &DbpfError) -> &'static str {
    match error {
        DbpfError::Io(_) => "io",
        DbpfError::NotDbpf => "not-dbpf",
        DbpfError::UnsupportedVersion { .. } => "unsupported-version",
        DbpfError::UnsupportedIndexFlags(_) => "unsupported-index",
        DbpfError::Truncated { .. } => "truncated",
        DbpfError::CorruptIndex(_) => "corrupt",
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsePassOutcome {
    pub parsed_ok: usize,
    pub parse_errors: usize,
    pub cancelled: bool,
}

/// Index every package whose content changed since it was last indexed.
/// Runs in one transaction; on cancellation the work already done commits
/// (each file's parse is individually valid) and the remainder picks up on
/// the next scan for free, because staleness is content-keyed per file.
pub fn run_parse_pass(
    conn: &mut Connection,
    mods_root: &SafeRoot,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<ParsePassOutcome, DbError> {
    let pending = files_needing_parse(conn)?;
    let total = pending.len();
    let mut outcome = ParsePassOutcome::default();

    let tx = conn.transaction()?;
    for (done, (file_id, rel)) in pending.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            outcome.cancelled = true;
            break;
        }
        let absolute = match mods_root.resolve_relative(Path::new(&rel)) {
            Ok(p) => p,
            Err(_) => {
                // Containment failure on a stored path — refuse the file,
                // never guess. Recorded as an io-class parse error.
                record_parse_error(
                    &tx,
                    file_id,
                    &DbpfError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "path escaped the mods folder",
                    )),
                )?;
                outcome.parse_errors += 1;
                continue;
            }
        };
        match dbpf::read_package_index(&absolute) {
            Ok(index) => {
                record_package_index(&tx, file_id, &index)?;
                outcome.parsed_ok += 1;
            }
            Err(err) => {
                record_parse_error(&tx, file_id, &err)?;
                outcome.parse_errors += 1;
            }
        }
        progress(done + 1, total);
    }
    tx.commit()?;
    Ok(outcome)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictMember {
    pub file_id: i64,
    pub relative_path: String,
    pub absolute_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictKey {
    pub type_id: u32,
    pub tgi: String,
    pub type_name: Option<&'static str>,
    pub presentation_only: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictGroup {
    /// Ordered by relative path, case-insensitive — the community-understood
    /// load order. The last member is the presumptive winner.
    pub members: Vec<ConflictMember>,
    pub shared_key_count: usize,
    /// Up to [`SAMPLE_KEYS`] shared keys for display.
    pub sample_keys: Vec<ConflictKey>,
    /// "gameplay" if any shared key is a non-presentation type, otherwise
    /// "presentation".
    pub severity: String,
    pub likely_intentional: bool,
}

const SAMPLE_KEYS: usize = 12;

/// A key present in more distinct packages than this is boilerplate, not a
/// collision: popular CC tools stamp a fixed-instance resource into every
/// package they save, producing one shared key across hundreds of unrelated
/// files (observed live: ~250 packages sharing a single unknown-type key).
/// Real collisions involve a handful of packages. Ubiquitous keys are
/// excluded, and the interface says so.
const UBIQUITOUS_KEY_FILE_THRESHOLD: usize = 12;

/// Group conflicting resource keys by the exact set of files sharing them.
/// Two files overlapping on forty keys are one group with forty keys; a
/// three-file overlap on a different key is its own group.
pub fn list_conflict_groups(conn: &Connection) -> Result<Vec<ConflictGroup>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT pr.type_id, pr.group_id, pr.instance,
                f.id, f.relative_path, f.absolute_path, f.sha256, f.mod_id
         FROM package_resources pr
         JOIN files f ON f.id = pr.file_id
         WHERE f.status = 'current' AND f.missing = 0
           AND (pr.type_id, pr.group_id, pr.instance) IN (
                SELECT p2.type_id, p2.group_id, p2.instance
                FROM package_resources p2
                JOIN files f2 ON f2.id = p2.file_id
                WHERE f2.status = 'current' AND f2.missing = 0
                GROUP BY p2.type_id, p2.group_id, p2.instance
                HAVING COUNT(DISTINCT p2.file_id) > 1)
         ORDER BY pr.type_id, pr.group_id, pr.instance",
    )?;

    struct Row {
        type_id: i64,
        group_id: i64,
        instance: i64,
        file_id: i64,
        relative_path: String,
        absolute_path: String,
        sha256: Option<String>,
        mod_id: Option<i64>,
    }
    let rows = stmt.query_map([], |r| {
        Ok(Row {
            type_id: r.get(0)?,
            group_id: r.get(1)?,
            instance: r.get(2)?,
            file_id: r.get(3)?,
            relative_path: r.get(4)?,
            absolute_path: r.get(5)?,
            sha256: r.get(6)?,
            mod_id: r.get(7)?,
        })
    })?;

    // Assemble per-key member sets.
    struct Member {
        file_id: i64,
        relative_path: String,
        absolute_path: String,
        sha256: Option<String>,
        mod_id: Option<i64>,
    }
    let mut per_key: BTreeMap<(i64, i64, i64), Vec<Member>> = BTreeMap::new();
    for row in rows {
        let row = row?;
        let members = per_key
            .entry((row.type_id, row.group_id, row.instance))
            .or_default();
        // The same key can appear twice inside one file; one membership is
        // enough here.
        if !members.iter().any(|m| m.file_id == row.file_id) {
            members.push(Member {
                file_id: row.file_id,
                relative_path: row.relative_path,
                absolute_path: row.absolute_path,
                sha256: row.sha256,
                mod_id: row.mod_id,
            });
        }
    }

    // Keep keys whose members are not all byte-identical, then merge keys
    // sharing the same file set into one group.
    struct Pending {
        members: Vec<(i64, String, String, Option<i64>)>,
        keys: Vec<(i64, i64, i64)>,
    }
    let mut by_fileset: BTreeMap<Vec<i64>, Pending> = BTreeMap::new();
    for (key, members) in per_key {
        let mut distinct_hashes: Vec<&str> =
            members.iter().filter_map(|m| m.sha256.as_deref()).collect();
        distinct_hashes.sort_unstable();
        distinct_hashes.dedup();
        if distinct_hashes.len() < 2 {
            // Identical content everywhere (or unhashed): no in-game
            // difference — the Duplicates feature owns byte-identical files.
            continue;
        }
        if members.len() > UBIQUITOUS_KEY_FILE_THRESHOLD {
            // Tool-stamp boilerplate, not a collision (see the constant's
            // documentation).
            continue;
        }
        let mut ids: Vec<i64> = members.iter().map(|m| m.file_id).collect();
        ids.sort_unstable();
        let entry = by_fileset.entry(ids).or_insert_with(|| Pending {
            members: members
                .iter()
                .map(|m| {
                    (
                        m.file_id,
                        m.relative_path.clone(),
                        m.absolute_path.clone(),
                        m.mod_id,
                    )
                })
                .collect(),
            keys: Vec::new(),
        });
        entry.keys.push(key);
    }

    let mut groups = Vec::new();
    for (_ids, pending) in by_fileset {
        let mut members = pending.members;
        // Name-based load order: case-insensitive path sort; last loads last
        // and presumptively wins.
        members.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

        let shared_key_count = pending.keys.len();
        let mut gameplay = false;
        let mut sample_keys = Vec::new();
        for (t, g, i) in &pending.keys {
            let type_id = *t as u32;
            let presentation = dbpf::type_is_presentation_only(type_id);
            if !presentation {
                gameplay = true;
            }
            if sample_keys.len() < SAMPLE_KEYS {
                let key = dbpf::ResourceKey {
                    type_id,
                    group_id: *g as u32,
                    instance: *i as u64,
                };
                sample_keys.push(ConflictKey {
                    type_id,
                    tgi: key.tgi_string(),
                    type_name: dbpf::resource_type_name(type_id),
                    presentation_only: presentation,
                });
            }
        }

        let likely_intentional = shares_mod(&members) || shares_top_folder(&members);

        groups.push(ConflictGroup {
            members: members
                .into_iter()
                .map(
                    |(file_id, relative_path, absolute_path, _)| ConflictMember {
                        file_id,
                        relative_path,
                        absolute_path,
                    },
                )
                .collect(),
            shared_key_count,
            sample_keys,
            severity: if gameplay { "gameplay" } else { "presentation" }.into(),
            likely_intentional,
        });
    }

    // Gameplay severity first, then by breadth of overlap, then by path.
    groups.sort_by(|a, b| {
        let sev = |g: &ConflictGroup| if g.severity == "gameplay" { 0 } else { 1 };
        sev(a)
            .cmp(&sev(b))
            .then(b.shared_key_count.cmp(&a.shared_key_count))
            .then_with(|| {
                a.members[0]
                    .relative_path
                    .to_lowercase()
                    .cmp(&b.members[0].relative_path.to_lowercase())
            })
    });
    Ok(groups)
}

fn shares_mod(members: &[(i64, String, String, Option<i64>)]) -> bool {
    let first = members[0].3;
    first.is_some() && members.iter().all(|m| m.3 == first)
}

fn shares_top_folder(members: &[(i64, String, String, Option<i64>)]) -> bool {
    let top = |path: &str| -> Option<String> {
        let mut parts = path.split('/');
        let first = parts.next()?;
        // A file at the root has no folder; it can't vouch for intent.
        parts.next()?;
        Some(first.to_lowercase())
    };
    let first = match top(&members[0].1) {
        Some(t) => t,
        None => return false,
    };
    members.iter().all(|m| top(&m.1).as_deref() == Some(&first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{files as db_files, Database};
    use crate::dbpf::testutil::build_package;
    use crate::hashing;
    use crate::scan::{self, ScanOptions};
    use std::fs;
    use std::sync::atomic::AtomicBool;

    const CASP: u32 = 0x034AEECB;
    const TUNING: u32 = 0x62E94D38;
    const THUMB: u32 = 0x3C1AF1F2;

    /// Write real synthetic packages into a temp mods root, then run the
    /// real pipeline: scan → reconcile → hash → parse. Returns the open
    /// database and the root.
    fn pipeline_with(
        packages: &[(&str, Vec<(u32, u32, u64)>)],
        raw_files: &[(&str, &[u8])],
    ) -> (Database, tempfile::TempDir, SafeRoot) {
        let dir = tempfile::tempdir().unwrap();
        for (rel, keys) in packages {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, build_package(1, 0, keys)).unwrap();
        }
        for (rel, bytes) in raw_files {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
        }

        let root = SafeRoot::new(dir.path()).unwrap();
        let mut db = Database::open_in_memory().unwrap();
        run_scan_and_hash(&mut db, &root);
        (db, dir, root)
    }

    fn run_scan_and_hash(db: &mut Database, root: &SafeRoot) {
        let cancel = AtomicBool::new(false);
        let opts = ScanOptions {
            excluded_relative: Vec::new(),
            script_depth_limit: 1,
        };
        let report = scan::scan(root, &opts, &cancel, |_| {});
        let summary = db_files::reconcile_scan(db.conn_mut(), &report, "test", &[]).unwrap();
        let mut updates = Vec::new();
        for (id, abs) in &summary.needs_hash {
            updates.push((*id, hashing::sha256_file(abs).unwrap()));
        }
        db_files::update_hashes(db.conn_mut(), &updates).unwrap();
    }

    fn parse_all(db: &mut Database, root: &SafeRoot) -> ParsePassOutcome {
        let cancel = AtomicBool::new(false);
        run_parse_pass(db.conn_mut(), root, &cancel, |_, _| {}).unwrap()
    }

    fn file_id(db: &Database, rel: &str) -> i64 {
        db.conn()
            .query_row(
                "SELECT id FROM files WHERE relative_path = ?1",
                params![rel],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn migration_adds_parse_columns_and_resource_table() {
        let db = Database::open_in_memory().unwrap();
        let cols: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('files')
                 WHERE name IN ('parsed_sha256','parse_status','parse_error')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cols, 3);
        db.conn()
            .execute("SELECT type_id FROM package_resources LIMIT 0", [])
            .ok();
    }

    fn seed_categorized(db: &Database, rel: &str, ft: &str, parsed: bool) -> i64 {
        db.conn()
            .execute(
                "INSERT INTO files (current_filename, absolute_path, relative_path,
                    file_type, size_bytes, first_seen_at, last_seen_at, parse_status)
                 VALUES (?1, ?2, ?1, ?3, 1, '2026-01-01T00:00:00Z',
                         '2026-01-01T00:00:00Z', ?4)",
                params![rel, format!("/m/{rel}"), ft,
                        if parsed { Some("ok") } else { None }],
            )
            .unwrap();
        db.conn().last_insert_rowid()
    }

    fn add_resource(db: &Database, file_id: i64, type_id: i64) {
        db.conn()
            .execute(
                "INSERT INTO package_resources (file_id, type_id, group_id, instance)
                 VALUES (?1, ?2, 0, 1)",
                params![file_id, type_id],
            )
            .unwrap();
    }

    fn category_of(db: &Database, id: i64) -> Option<String> {
        db.conn()
            .query_row("SELECT category FROM files WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn the_type_census_counts_files_not_rows() {
        let db = Database::open_in_memory().unwrap();
        let a = seed_categorized(&db, "a.package", "package", true);
        let b = seed_categorized(&db, "b.package", "package", true);
        let c = seed_categorized(&db, "c.package", "package", true);
        add_resource(&db, a, 0x0111_1111);
        add_resource(&db, a, 0x0111_1111); // duplicate row, same file
        add_resource(&db, b, 0x0111_1111);
        add_resource(&db, b, 0x0222_2222);
        add_resource(&db, c, 0x0333_3333); // outside the id set
        let census = resource_type_census(db.conn(), &[a, b]).unwrap();
        assert_eq!(census[0], (0x0111_1111, 2), "two files, not three rows");
        assert_eq!(census[1], (0x0222_2222, 1));
        assert!(census.iter().all(|(t, _)| *t != 0x0333_3333));
    }

    #[test]
    fn categories_follow_the_resource_census_with_priority() {
        let db = Database::open_in_memory().unwrap();
        let cas = seed_categorized(&db, "hair.package", "package", true);
        add_resource(&db, cas, 0x034AEECB_i64); // CAS Part
        let bb = seed_categorized(&db, "sofa.package", "package", true);
        add_resource(&db, bb, 0xC0DB5AE7_u32 as i64); // Object Definition
        let anim = seed_categorized(&db, "poses.package", "package", true);
        add_resource(&db, anim, 0x6B20C4F3_i64); // Animation Clip
        let tune = seed_categorized(&db, "cheats.package", "package", true);
        add_resource(&db, tune, 0x62E94D38_i64); // Tuning
        let script = seed_categorized(&db, "mc.ts4script", "ts4script", false);
        let stray = seed_categorized(&db, "stray.package", "package", true);
        add_resource(&db, stray, 0x220557DA_i64); // String Table only
        let unparsed = seed_categorized(&db, "new.package", "package", false);
        // Priority: a CAS item that also ships clips is CAS, not a pose pack.
        let mixed = seed_categorized(&db, "acc_rig.package", "package", true);
        add_resource(&db, mixed, 0x6B20C4F3_i64);
        add_resource(&db, mixed, 0x034AEECB_i64);

        classify_categories(db.conn()).unwrap();

        assert_eq!(category_of(&db, cas).as_deref(), Some("cas"));
        assert_eq!(category_of(&db, bb).as_deref(), Some("buildbuy"));
        assert_eq!(category_of(&db, anim).as_deref(), Some("animations"));
        assert_eq!(category_of(&db, tune).as_deref(), Some("gameplay"));
        assert_eq!(category_of(&db, script).as_deref(), Some("scripts"));
        assert_eq!(category_of(&db, stray).as_deref(), Some("other"));
        assert_eq!(category_of(&db, unparsed), None);
        assert_eq!(category_of(&db, mixed).as_deref(), Some("cas"));
        // Idempotent: a second pass changes nothing.
        classify_categories(db.conn()).unwrap();
        assert_eq!(category_of(&db, mixed).as_deref(), Some("cas"));
    }

    #[test]
    fn parse_pass_indexes_packages_and_records_errors() {
        let (mut db, _dir, root) = pipeline_with(
            &[
                ("A/one.package", vec![(CASP, 0, 0x11), (TUNING, 0, 0x22)]),
                ("B/two.package", vec![(CASP, 0, 0x33)]),
            ],
            &[("B/broken.package", b"DBPF short")],
        );
        let outcome = parse_all(&mut db, &root);
        assert_eq!(outcome.parsed_ok, 2);
        assert_eq!(outcome.parse_errors, 1);
        assert!(!outcome.cancelled);

        let one = file_id(&db, "A/one.package");
        let (count, status): (i64, String) = db
            .conn()
            .query_row(
                "SELECT resource_count, parse_status FROM files WHERE id = ?1",
                params![one],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(status, "ok");

        let broken = file_id(&db, "B/broken.package");
        let status: String = db
            .conn()
            .query_row(
                "SELECT parse_status FROM files WHERE id = ?1",
                params![broken],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "truncated");
    }

    #[test]
    fn parse_is_content_keyed_incremental() {
        let (mut db, dir, root) = pipeline_with(&[("a.package", vec![(CASP, 0, 0x11)])], &[]);
        assert_eq!(parse_all(&mut db, &root).parsed_ok, 1);
        // Nothing changed: second pass finds no work — including no retry of
        // anything already parsed.
        let second = parse_all(&mut db, &root);
        assert_eq!(second.parsed_ok + second.parse_errors, 0);

        // Content changes → rescan updates the hash → the file re-parses.
        fs::write(
            dir.path().join("a.package"),
            build_package(1, 0, &[(CASP, 0, 0x11), (TUNING, 0, 0x99)]),
        )
        .unwrap();
        run_scan_and_hash(&mut db, &root);
        let third = parse_all(&mut db, &root);
        assert_eq!(third.parsed_ok, 1);
        let id = file_id(&db, "a.package");
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM package_resources WHERE file_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "old rows replaced, not stacked");
    }

    #[test]
    fn parse_errors_are_not_retried_until_content_changes() {
        let (mut db, _dir, root) = pipeline_with(&[], &[("bad.package", b"not a dbpf at all")]);
        let first = parse_all(&mut db, &root);
        assert_eq!(first.parse_errors, 1);
        let second = parse_all(&mut db, &root);
        assert_eq!(second.parsed_ok + second.parse_errors, 0);
    }

    #[test]
    fn cancelled_pass_commits_partial_work_and_resumes() {
        let (mut db, _dir, root) = pipeline_with(
            &[
                ("a.package", vec![(CASP, 0, 0x1)]),
                ("b.package", vec![(CASP, 0, 0x2)]),
                ("c.package", vec![(CASP, 0, 0x3)]),
            ],
            &[],
        );
        let cancel = AtomicBool::new(false);
        let outcome = run_parse_pass(db.conn_mut(), &root, &cancel, |done, _| {
            if done == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();
        assert!(outcome.cancelled);
        assert_eq!(outcome.parsed_ok, 1);
        // The remainder is simply still pending — the next pass finishes it.
        let resumed = parse_all(&mut db, &root);
        assert_eq!(resumed.parsed_ok, 2);
    }

    #[test]
    fn deleting_a_file_row_cascades_its_resources() {
        let (mut db, _dir, root) = pipeline_with(&[("a.package", vec![(CASP, 0, 0x1)])], &[]);
        parse_all(&mut db, &root);
        let id = file_id(&db, "a.package");
        db.conn()
            .execute("DELETE FROM files WHERE id = ?1", params![id])
            .unwrap();
        let left: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM package_resources", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn instance_bit_cast_round_trips_high_values() {
        let big = 0xDEADBEEF12345678u64; // above i64::MAX as unsigned
        let (mut db, _dir, root) = pipeline_with(&[("a.package", vec![(TUNING, 5, big)])], &[]);
        parse_all(&mut db, &root);
        let stored: i64 = db
            .conn()
            .query_row("SELECT instance FROM package_resources", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored as u64, big);
    }

    #[test]
    fn conflicts_require_differing_content() {
        // A and B are byte-identical files sharing a key: Duplicates
        // territory, not a conflict. Adding C with different content makes
        // it a real three-way conflict.
        let same = vec![(TUNING, 0, 0xAA)];
        let (mut db, _dir, root) = pipeline_with(
            &[("Mods1/a.package", same.clone()), ("Mods2/b.package", same)],
            &[],
        );
        parse_all(&mut db, &root);
        assert!(list_conflict_groups(db.conn()).unwrap().is_empty());

        let (mut db2, _dir2, root2) = pipeline_with(
            &[
                ("Mods1/a.package", vec![(TUNING, 0, 0xAA)]),
                ("Mods2/b.package", vec![(TUNING, 0, 0xAA)]),
                ("Mods3/c.package", vec![(TUNING, 0, 0xAA), (CASP, 0, 0x1)]),
            ],
            &[],
        );
        parse_all(&mut db2, &root2);
        let groups = list_conflict_groups(db2.conn()).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 3);
        assert_eq!(groups[0].severity, "gameplay");
        // a/b are byte-identical; c differs — the group still lists all
        // three because all three compete for the key.
    }

    #[test]
    fn keys_merge_by_file_set_and_order_is_name_based() {
        let (mut db, _dir, root) = pipeline_with(
            &[
                (
                    "Zeta/late.package",
                    vec![(TUNING, 0, 0x1), (TUNING, 0, 0x2), (TUNING, 0, 0x3)],
                ),
                (
                    "Alpha/early.package",
                    vec![
                        (TUNING, 0, 0x1),
                        (TUNING, 0, 0x2),
                        (TUNING, 0, 0x3),
                        (CASP, 9, 9),
                    ],
                ),
            ],
            &[],
        );
        parse_all(&mut db, &root);
        let groups = list_conflict_groups(db.conn()).unwrap();
        assert_eq!(
            groups.len(),
            1,
            "three shared keys, one file set, one group"
        );
        assert_eq!(groups[0].shared_key_count, 3);
        assert_eq!(groups[0].sample_keys.len(), 3);
        // Name-based order: Alpha loads first, Zeta last (presumptive winner).
        assert!(groups[0].members[0].relative_path.starts_with("Alpha/"));
        assert!(groups[0].members[1].relative_path.starts_with("Zeta/"));
    }

    #[test]
    fn presentation_only_overlaps_are_low_severity() {
        let (mut db, _dir, root) = pipeline_with(
            &[
                ("A/x.package", vec![(THUMB, 0, 0x7), (CASP, 0, 0xA1)]),
                ("B/y.package", vec![(THUMB, 0, 0x7), (CASP, 0, 0xB2)]),
            ],
            &[],
        );
        parse_all(&mut db, &root);
        let groups = list_conflict_groups(db.conn()).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].severity, "presentation");
        assert!(groups[0].sample_keys[0].presentation_only);
    }

    #[test]
    fn ubiquitous_keys_are_boilerplate_not_conflicts() {
        // Regression (found in real-library validation): a fixed key stamped
        // by CC tooling into many packages must not form a mega conflict
        // group, while a genuine few-package collision on another key still
        // surfaces.
        let stamp = (0x545AC67Au32, 0, 0x5747_0000_0000_0001u64);
        let mut packages: Vec<(String, Vec<(u32, u32, u64)>)> = (0..14)
            .map(|i| {
                (
                    format!("Mods{i}/pkg{i}.package"),
                    vec![stamp, (CASP, 0, 0x9000 + i as u64)],
                )
            })
            .collect();
        packages.push((
            "RealA/a.package".into(),
            vec![(TUNING, 0, 0xFEED), (CASP, 0, 0xA1)],
        ));
        packages.push((
            "RealB/b.package".into(),
            vec![(TUNING, 0, 0xFEED), (CASP, 0, 0xB2)],
        ));
        let refs: Vec<(&str, Vec<(u32, u32, u64)>)> = packages
            .iter()
            .map(|(rel, keys)| (rel.as_str(), keys.clone()))
            .collect();
        let (mut db, _dir, root) = pipeline_with(&refs, &[]);
        parse_all(&mut db, &root);
        let groups = list_conflict_groups(db.conn()).unwrap();
        assert_eq!(groups.len(), 1, "only the genuine two-package collision");
        assert_eq!(groups[0].members.len(), 2);
        assert!(groups[0].members[0].relative_path.starts_with("RealA/"));
    }

    #[test]
    fn same_top_folder_is_likely_intentional() {
        let (mut db, _dir, root) = pipeline_with(
            &[
                ("CoolMod/base.package", vec![(TUNING, 0, 0xE)]),
                (
                    "CoolMod/addon.package",
                    vec![(TUNING, 0, 0xE), (CASP, 1, 1)],
                ),
            ],
            &[],
        );
        parse_all(&mut db, &root);
        let groups = list_conflict_groups(db.conn()).unwrap();
        assert!(groups[0].likely_intentional);

        let (mut db2, _dir2, root2) = pipeline_with(
            &[
                ("ModA/base.package", vec![(TUNING, 0, 0xE)]),
                ("ModB/clash.package", vec![(TUNING, 0, 0xE), (CASP, 1, 1)]),
            ],
            &[],
        );
        parse_all(&mut db2, &root2);
        assert!(!list_conflict_groups(db2.conn()).unwrap()[0].likely_intentional);
    }
}

// ---------------------------------------------------------------------------
// In-game category classification
// ---------------------------------------------------------------------------

/// What a mod *is*, derived from its resource census. The type constants
/// are the same researched set `crate::dbpf::type_name` documents; the
/// priority order means a CAS item that also ships animation clips reads
/// as CAS, not as a pose pack.
///
/// Values: `cas` · `buildbuy` · `animations` · `gameplay` · `scripts` ·
/// `other` (parsed, but none of the known families) · NULL (unsupported
/// or not yet parsed).
pub fn classify_categories(conn: &Connection) -> Result<usize, DbError> {
    const CAS: &str = "55242443, 22681673, 3936561885, 55867754, 55959718, 108833297";
    //                CASP        GEOM      CAS Preset  SkinTone  BoneDelta BlendGeo
    const BUILDBUY: &str = "832458525, 3235601127, 3540272417, 3548561239, 62178845";
    //                     ObjCatalog ObjDef       ObjSlot      Footprint   Light
    const ANIM: &str = "1797309683"; // Animation Clip
    const GAMEPLAY: &str = "1659456824, 1415235194"; // Tuning (binary), SimData
    let sql = format!(
        "UPDATE files SET category = CASE
            WHEN file_type = 'ts4script' THEN 'scripts'
            WHEN file_type <> 'package' THEN NULL
            WHEN parse_status IS NULL OR parse_status <> 'ok' THEN NULL
            WHEN EXISTS (SELECT 1 FROM package_resources r
                         WHERE r.file_id = files.id AND r.type_id IN ({CAS}))
                THEN 'cas'
            WHEN EXISTS (SELECT 1 FROM package_resources r
                         WHERE r.file_id = files.id AND r.type_id IN ({BUILDBUY}))
                THEN 'buildbuy'
            WHEN EXISTS (SELECT 1 FROM package_resources r
                         WHERE r.file_id = files.id AND r.type_id IN ({ANIM}))
                THEN 'animations'
            WHEN EXISTS (SELECT 1 FROM package_resources r
                         WHERE r.file_id = files.id AND r.type_id IN ({GAMEPLAY}))
                THEN 'gameplay'
            WHEN EXISTS (SELECT 1 FROM package_resources r
                         WHERE r.file_id = files.id)
                THEN 'other'
            ELSE NULL
         END"
    );
    Ok(conn.execute(&sql, [])?)
}

/// How many of the given files carry each resource type — the ground-truth
/// instrument for expanding thumbnail decoding without guessing constants.
pub fn resource_type_census(
    conn: &Connection,
    file_ids: &[i64],
) -> Result<Vec<(u32, i64)>, DbError> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS census_ids (id INTEGER PRIMARY KEY);
         DELETE FROM census_ids;",
    )?;
    {
        let mut ins = conn.prepare("INSERT OR IGNORE INTO census_ids (id) VALUES (?1)")?;
        for id in file_ids {
            ins.execute([id])?;
        }
    }
    let mut stmt = conn.prepare(
        "SELECT r.type_id, COUNT(DISTINCT r.file_id) AS files
         FROM package_resources r
         JOIN census_ids c ON c.id = r.file_id
         GROUP BY r.type_id
         ORDER BY files DESC, r.type_id
         LIMIT 14",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)? as u32, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    conn.execute("DELETE FROM census_ids", [])?;
    Ok(rows)
}
