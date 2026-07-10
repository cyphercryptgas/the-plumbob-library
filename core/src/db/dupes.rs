//! Duplicate-group persistence. The detection logic lives in
//! [`crate::duplicates`]; this module feeds it facts from the database and
//! stores its results for the Duplicate Center.

use super::{parse_rfc3339, DbError};
use crate::duplicates::{DuplicateGroup, FileFacts};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::PathBuf;

/// Facts for every present (non-missing, non-quarantined) file.
///
/// `manifest_associated` is approximated by "linked to a mod record" until
/// installation manifests exist (Phase 3); `in_expected_category` stays false
/// until the Organize feature defines category→folder mappings. Both
/// approximations only affect which duplicate is *recommended* for keeping —
/// never whether files are grouped.
pub fn load_file_facts(conn: &Connection) -> Result<Vec<FileFacts>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, relative_path, size_bytes, sha256, modified_at_fs, first_seen_at,
                (mod_id IS NOT NULL)
         FROM files
         WHERE missing = 0 AND status != 'quarantined'",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, i64>(6)? != 0,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, rel, size, sha256, modified, first_seen, has_mod) = row?;
        out.push(FileFacts {
            id,
            relative_path: PathBuf::from(rel),
            size_bytes: size as u64,
            sha256,
            modified_at: modified.as_deref().and_then(parse_rfc3339),
            first_seen_at: parse_rfc3339(&first_seen),
            manifest_associated: has_mod,
            in_expected_category: false,
        })
    }
    Ok(out)
}

/// Replace all *open* exact groups with the freshly computed set. Groups the
/// user has already resolved or dismissed (any other status) are preserved.
pub fn replace_exact_groups(
    conn: &mut Connection,
    groups: &[DuplicateGroup],
) -> Result<usize, DbError> {
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM duplicate_groups WHERE duplicate_type = 'exact' AND status = 'open'",
        [],
    )?;
    {
        let mut insert_group = tx.prepare(
            "INSERT INTO duplicate_groups (duplicate_type, confidence, status, created_at,
                sha256, size_bytes, recommended_file_id, recommendation_reason,
                reclaimable_bytes)
             VALUES ('exact', 1.0, 'open', ?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut insert_member = tx.prepare(
            "INSERT INTO duplicate_group_files (group_id, file_id) VALUES (?1, ?2)",
        )?;
        let now = super::now_rfc3339();
        for g in groups {
            insert_group.execute(params![
                now,
                g.sha256,
                g.size_bytes as i64,
                g.recommended_keep,
                g.recommendation_reason,
                g.reclaimable_bytes as i64,
            ])?;
            let gid = tx.last_insert_rowid();
            for fid in &g.file_ids {
                insert_member.execute(params![gid, fid])?;
            }
        }
    }
    tx.commit()?;
    Ok(groups.len())
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMemberView {
    pub file_id: i64,
    pub relative_path: String,
    pub size_bytes: i64,
    pub modified_at_fs: Option<String>,
    pub recommended: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroupView {
    pub id: i64,
    pub sha256: Option<String>,
    pub size_bytes: Option<i64>,
    pub reclaimable_bytes: i64,
    pub recommended_file_id: Option<i64>,
    pub recommendation_reason: Option<String>,
    pub members: Vec<DuplicateMemberView>,
}

/// Update a group's lifecycle status (`open` → `resolved` / `dismissed`).
/// Resolved and dismissed groups survive rescans; see [`replace_exact_groups`].
pub fn set_group_status(
    conn: &Connection,
    group_id: i64,
    status: &str,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE duplicate_groups SET status = ?2 WHERE id = ?1",
        params![group_id, status],
    )?;
    Ok(())
}

/// Open exact groups with their members, assembled in two queries (no N+1).
pub fn list_open_exact_groups(conn: &Connection) -> Result<Vec<DuplicateGroupView>, DbError> {
    let mut groups: Vec<DuplicateGroupView> = Vec::new();
    let mut index: HashMap<i64, usize> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, sha256, size_bytes, reclaimable_bytes, recommended_file_id,
                    recommendation_reason
             FROM duplicate_groups
             WHERE duplicate_type = 'exact' AND status = 'open'
             ORDER BY reclaimable_bytes DESC, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DuplicateGroupView {
                id: r.get(0)?,
                sha256: r.get(1)?,
                size_bytes: r.get(2)?,
                reclaimable_bytes: r.get(3)?,
                recommended_file_id: r.get(4)?,
                recommendation_reason: r.get(5)?,
                members: Vec::new(),
            })
        })?;
        for row in rows {
            let g = row?;
            index.insert(g.id, groups.len());
            groups.push(g);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT dgf.group_id, f.id, f.relative_path, f.size_bytes, f.modified_at_fs
             FROM duplicate_group_files dgf
             JOIN files f ON f.id = dgf.file_id
             JOIN duplicate_groups g ON g.id = dgf.group_id
             WHERE g.duplicate_type = 'exact' AND g.status = 'open'
             ORDER BY f.relative_path COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                DuplicateMemberView {
                    file_id: r.get(1)?,
                    relative_path: r.get(2)?,
                    size_bytes: r.get(3)?,
                    modified_at_fs: r.get(4)?,
                    recommended: false,
                },
            ))
        })?;
        for row in rows {
            let (gid, mut member) = row?;
            if let Some(&i) = index.get(&gid) {
                member.recommended = groups[i].recommended_file_id == Some(member.file_id);
                groups[i].members.push(member);
            }
        }
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{files, Database};
    use crate::duplicates::group_exact;
    use crate::scan::{FileKind, ScanReport, ScannedFile};

    fn seeded_db() -> Database {
        let mut db = Database::open_in_memory().unwrap();
        let mk = |rel: &str, size: u64| ScannedFile {
            absolute_path: PathBuf::from(format!("/mods/{rel}")),
            relative_path: PathBuf::from(rel),
            file_name: rel.rsplit('/').next().unwrap().to_string(),
            extension: Some("package".into()),
            kind: FileKind::Package,
            size_bytes: size,
            modified_at: None,
            created_at: None,
            depth: 0,
            zero_byte: false,
            deep_script: false,
            sha256: None,
        };
        let files_list = vec![
            mk("clean.package", 100),
            mk("Downloads/clean (1).package", 100),
            mk("other.package", 100),
        ];
        let total = files_list.iter().map(|f| f.size_bytes).sum();
        let report = ScanReport {
            files: files_list,
            empty_dirs: vec![],
            symlinks_skipped: vec![],
            errors: vec![],
            cancelled: false,
            total_bytes: total,
            duration_ms: 1,
        };
        let s = files::reconcile_scan(db.conn_mut(), &report, "initial", &[]).unwrap();
        // Two identical, one different.
        let updates: Vec<(i64, String)> = s
            .needs_hash
            .iter()
            .map(|(id, abs)| {
                let hash = if abs.to_string_lossy().contains("other") {
                    "hash-b".to_string()
                } else {
                    "hash-a".to_string()
                };
                (*id, hash)
            })
            .collect();
        files::update_hashes(db.conn_mut(), &updates).unwrap();
        db
    }

    #[test]
    fn facts_round_trip_and_groups_persist() {
        let mut db = seeded_db();
        let facts = load_file_facts(db.conn()).unwrap();
        assert_eq!(facts.len(), 3);
        let groups = group_exact(&facts);
        assert_eq!(groups.len(), 1);
        replace_exact_groups(db.conn_mut(), &groups).unwrap();

        let views = list_open_exact_groups(db.conn()).unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].members.len(), 2);
        assert_eq!(views[0].reclaimable_bytes, 100);
        let recommended: Vec<_> = views[0]
            .members
            .iter()
            .filter(|m| m.recommended)
            .collect();
        assert_eq!(recommended.len(), 1);
        assert_eq!(recommended[0].relative_path, "clean.package");
    }

    #[test]
    fn rerunning_replacement_does_not_stack_groups() {
        let mut db = seeded_db();
        let facts = load_file_facts(db.conn()).unwrap();
        let groups = group_exact(&facts);
        replace_exact_groups(db.conn_mut(), &groups).unwrap();
        replace_exact_groups(db.conn_mut(), &groups).unwrap();
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM duplicate_groups", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn resolved_groups_survive_replacement() {
        let mut db = seeded_db();
        let facts = load_file_facts(db.conn()).unwrap();
        let groups = group_exact(&facts);
        replace_exact_groups(db.conn_mut(), &groups).unwrap();
        db.conn()
            .execute("UPDATE duplicate_groups SET status = 'resolved'", [])
            .unwrap();
        replace_exact_groups(db.conn_mut(), &groups).unwrap();
        let (open, resolved): (i64, i64) = db
            .conn()
            .query_row(
                "SELECT SUM(CASE WHEN status='open' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN status='resolved' THEN 1 ELSE 0 END)
                 FROM duplicate_groups",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(open, 1);
        assert_eq!(resolved, 1, "user decisions must not be wiped by a rescan");
    }

    #[test]
    fn set_group_status_removes_group_from_open_list() {
        let mut db = seeded_db();
        let facts = load_file_facts(db.conn()).unwrap();
        let groups = group_exact(&facts);
        replace_exact_groups(db.conn_mut(), &groups).unwrap();
        let open = list_open_exact_groups(db.conn()).unwrap();
        set_group_status(db.conn(), open[0].id, "resolved").unwrap();
        assert!(list_open_exact_groups(db.conn()).unwrap().is_empty());
    }

    #[test]
    fn quarantined_files_never_feed_the_detector() {
        let db = seeded_db();
        db.conn()
            .execute(
                "UPDATE files SET status = 'quarantined'
                 WHERE relative_path LIKE '%(1)%'",
                [],
            )
            .unwrap();
        let facts = load_file_facts(db.conn()).unwrap();
        assert_eq!(facts.len(), 2);
        assert!(group_exact(&facts).is_empty());
    }
}
