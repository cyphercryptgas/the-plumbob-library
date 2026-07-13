//! Persistence for the CurseForge update radar: which files still need a
//! fingerprint, the fingerprint↔file map for querying, and the locally
//! cached results of the last check.

use super::{now_rfc3339, DbError};
use rusqlite::{params, Connection};
use serde::Serialize;

/// Files eligible for the radar that have no fingerprint yet.
pub fn files_needing_fingerprint(
    conn: &Connection,
) -> Result<Vec<(i64, String)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, absolute_path FROM files
         WHERE curse_fingerprint IS NULL
           AND missing = 0 AND status = 'current'
           AND file_type IN ('package', 'ts4script')
         ORDER BY relative_path COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_fingerprint(conn: &Connection, file_id: i64, fp: u32) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET curse_fingerprint = ?2 WHERE id = ?1",
        params![file_id, i64::from(fp)],
    )?;
    Ok(())
}

/// Every fingerprinted, eligible file — the query set for a check.
pub fn fingerprint_pairs(conn: &Connection) -> Result<Vec<(u32, i64)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT curse_fingerprint, id FROM files
         WHERE curse_fingerprint IS NOT NULL
           AND missing = 0 AND status = 'current'
           AND file_type IN ('package', 'ts4script')",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)? as u32, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// One resolved match, ready for the cache.
#[derive(Clone, Debug)]
pub struct MatchRecord {
    pub file_id: i64,
    pub curse_mod_id: i64,
    pub curse_file_id: Option<i64>,
    pub mod_name: String,
    pub website_url: Option<String>,
    pub matched_file_name: Option<String>,
    pub matched_file_date: Option<String>,
    pub latest_file_id: i64,
    pub latest_file_name: String,
    pub latest_file_date: String,
    pub update_available: bool,
    /// 'fingerprint' (exact bytes) or 'name' (approximate).
    pub match_kind: &'static str,
    pub confidence: Option<f64>,
}

/// A check replaces the whole cache atomically — the radar always shows
/// one coherent snapshot with one `checked_at`.
pub fn replace_matches(
    conn: &mut Connection,
    records: &[MatchRecord],
) -> Result<String, DbError> {
    let checked_at = now_rfc3339();
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM curse_matches", [])?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO curse_matches
                (file_id, curse_mod_id, curse_file_id, mod_name, website_url,
                 matched_file_name, matched_file_date, latest_file_id,
                 latest_file_name, latest_file_date, update_available,
                 match_kind, confidence, checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                 ?14)",
        )?;
        for m in records {
            stmt.execute(params![
                m.file_id,
                m.curse_mod_id,
                m.curse_file_id,
                m.mod_name,
                m.website_url,
                m.matched_file_name,
                m.matched_file_date,
                m.latest_file_id,
                m.latest_file_name,
                m.latest_file_date,
                m.update_available,
                m.match_kind,
                m.confidence,
                checked_at,
            ])?;
        }
    }
    tx.commit()?;
    Ok(checked_at)
}

/// One radar row: an eligible file plus its cached match, if any.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseStatusRow {
    pub file_id: i64,
    pub relative_path: String,
    pub current_filename: String,
    pub enabled: bool,
    pub fingerprinted: bool,
    pub curse_mod_id: Option<i64>,
    pub latest_file_id: Option<i64>,
    pub mod_name: Option<String>,
    pub website_url: Option<String>,
    pub matched_file_name: Option<String>,
    pub matched_file_date: Option<String>,
    pub latest_file_name: Option<String>,
    pub latest_file_date: Option<String>,
    pub update_available: bool,
    pub match_kind: Option<String>,
    pub confidence: Option<f64>,
    pub checked_at: Option<String>,
}

pub fn status(conn: &Connection) -> Result<Vec<CurseStatusRow>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.relative_path, f.current_filename, f.enabled,
                f.curse_fingerprint IS NOT NULL,
                m.curse_mod_id, m.latest_file_id,
                m.mod_name, m.website_url, m.matched_file_name,
                m.matched_file_date, m.latest_file_name, m.latest_file_date,
                COALESCE(m.update_available, 0), m.match_kind, m.confidence,
                m.checked_at
         FROM files f
         LEFT JOIN curse_matches m ON m.file_id = f.id
         WHERE f.missing = 0 AND f.status = 'current'
           AND f.file_type IN ('package', 'ts4script')
         ORDER BY COALESCE(m.update_available, 0) DESC,
                  m.mod_name COLLATE NOCASE,
                  f.relative_path COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(CurseStatusRow {
                file_id: r.get(0)?,
                relative_path: r.get(1)?,
                current_filename: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                fingerprinted: r.get::<_, i64>(4)? != 0,
                curse_mod_id: r.get(5)?,
                latest_file_id: r.get(6)?,
                mod_name: r.get(7)?,
                website_url: r.get(8)?,
                matched_file_name: r.get(9)?,
                matched_file_date: r.get(10)?,
                latest_file_name: r.get(11)?,
                latest_file_date: r.get(12)?,
                update_available: r.get::<_, i64>(13)? != 0,
                match_kind: r.get(14)?,
                confidence: r.get(15)?,
                checked_at: r.get(16)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reverify_mutations_scope_to_the_terms_files() {
        let mut db = crate::db::Database::open_in_memory().unwrap();
        let r = vec![
            seed(&db, "a1.package", "package"),
            seed(&db, "a2.package", "package"),
            seed(&db, "b1.package", "package"),
        ];
        // Two terms, same mod, different files: term A owns files 0+1,
        // term B owns file 2.
        upsert_lookup(db.conn(), "term a", Some(77), Some("Shared Mod"), Some(0.6)).unwrap();
        upsert_lookup(db.conn(), "term b", Some(77), Some("Shared Mod"), Some(0.6)).unwrap();
        let mk = |fid| MatchRecord {
            file_id: fid,
            curse_mod_id: 77,
            curse_file_id: None,
            mod_name: "Shared Mod".into(),
            website_url: None,
            matched_file_name: None,
            matched_file_date: None,
            latest_file_id: 5,
            latest_file_name: "x".into(),
            latest_file_date: "2026-01-01T00:00:00Z".into(),
            update_available: false,
            match_kind: "name",
            confidence: Some(0.6),
        };
        replace_matches(db.conn_mut(), &[mk(r[0]), mk(r[1]), mk(r[2])]).unwrap();
        update_name_confidence(db.conn(), &[r[0], r[1]], 77, 0.9).unwrap();
        let deleted = delete_name_matches(db.conn(), &[r[2]], 77).unwrap();
        assert_eq!(deleted, 1, "only term B's file dropped");
        null_lookup(db.conn(), "term b").unwrap();
        let lookups = name_lookup_rows(db.conn()).unwrap();
        assert_eq!(lookups.len(), 1, "term b cleared for future re-search");
        assert_eq!(lookups[0].0, "term a");
        let confs: Vec<Option<f64>> = {
            let mut stmt = db
                .conn()
                .prepare("SELECT confidence FROM curse_matches ORDER BY file_id")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(confs, vec![Some(0.9), Some(0.9)], "term A boosted, intact");
    }

    use super::*;
    use crate::db::Database;

    fn seed(db: &Database, rel: &str, ft: &str) -> i64 {
        db.conn()
            .execute(
                "INSERT INTO files (current_filename, absolute_path, relative_path,
                    file_type, size_bytes, first_seen_at, last_seen_at)
                 VALUES (?1, ?2, ?1, ?3, 1, '2026-01-01T00:00:00Z',
                         '2026-01-01T00:00:00Z')",
                params![rel, format!("/m/{rel}"), ft],
            )
            .unwrap();
        db.conn().last_insert_rowid()
    }

    #[test]
    fn fingerprint_lifecycle_needs_then_pairs() {
        let db = Database::open_in_memory().unwrap();
        let a = seed(&db, "a.package", "package");
        let s = seed(&db, "s.ts4script", "ts4script");
        seed(&db, "notes.txt", "unsupported");
        let need = files_needing_fingerprint(db.conn()).unwrap();
        assert_eq!(need.len(), 2, "only mods need fingerprints");
        set_fingerprint(db.conn(), a, 0xDEAD_BEEF).unwrap();
        set_fingerprint(db.conn(), s, 7).unwrap();
        assert!(files_needing_fingerprint(db.conn()).unwrap().is_empty());
        let mut pairs = fingerprint_pairs(db.conn()).unwrap();
        pairs.sort();
        assert_eq!(pairs, vec![(7, s), (0xDEAD_BEEF, a)]);
    }

    #[test]
    fn a_check_replaces_the_cache_atomically_and_status_joins() {
        let mut db = Database::open_in_memory().unwrap();
        let a = seed(&db, "a.package", "package");
        let b = seed(&db, "b.package", "package");
        set_fingerprint(db.conn(), a, 1).unwrap();
        let rec = MatchRecord {
            file_id: a,
            curse_mod_id: 100,
            curse_file_id: Some(555),
            mod_name: "UI Cheats".into(),
            website_url: Some("https://example.test/ui".into()),
            matched_file_name: Some("UICheats_v1.package".into()),
            matched_file_date: Some("2026-01-01T00:00:00Z".into()),
            latest_file_id: 556,
            latest_file_name: "UICheats_v2.package".into(),
            latest_file_date: "2026-06-01T00:00:00Z".into(),
            update_available: true,
            match_kind: "fingerprint",
            confidence: None,
        };
        replace_matches(db.conn_mut(), &[rec.clone()]).unwrap();
        let rows = status(db.conn()).unwrap();
        assert_eq!(rows.len(), 2);
        // Updates sort first.
        assert_eq!(rows[0].file_id, a);
        assert!(rows[0].update_available);
        assert_eq!(rows[0].mod_name.as_deref(), Some("UI Cheats"));
        assert!(rows[0].fingerprinted);
        let unknown = rows.iter().find(|r| r.file_id == b).unwrap();
        assert!(!unknown.fingerprinted);
        assert!(unknown.mod_name.is_none());
        // A new check wipes the old snapshot.
        replace_matches(db.conn_mut(), &[]).unwrap();
        assert!(status(db.conn()).unwrap().iter().all(|r| r.mod_name.is_none()));
    }
}

// ---------------------------------------------------------------------------
// Name-lookup cache (tier-2)
// ---------------------------------------------------------------------------

/// Every term ever searched, hit or miss — misses are cached too so a
/// resumed or repeated check never re-asks CurseForge the same question.
pub fn known_lookups(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, Option<i64>>, DbError> {
    let mut stmt =
        conn.prepare("SELECT term, curse_mod_id FROM curse_name_lookups")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)))?
        .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
    Ok(rows)
}

pub fn upsert_lookup(
    conn: &Connection,
    term: &str,
    curse_mod_id: Option<i64>,
    mod_name: Option<&str>,
    confidence: Option<f64>,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO curse_name_lookups
            (term, curse_mod_id, mod_name, confidence, checked_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(term) DO UPDATE SET
            curse_mod_id = excluded.curse_mod_id,
            mod_name = excluded.mod_name,
            confidence = excluded.confidence,
            checked_at = excluded.checked_at",
        params![term, curse_mod_id, mod_name, confidence, now_rfc3339()],
    )?;
    Ok(())
}

/// Eligible files with what the name tier needs: the logical file name and
/// the disk mtime (the honest "your build" date for approximate matches).
pub struct EligibleFile {
    pub id: i64,
    pub file_name: String,
    pub mtime: Option<String>,
    /// Canonical creator key ('' = examined, uncredited).
    pub creator: Option<String>,
    pub creator_display: Option<String>,
}

pub fn eligible_files(conn: &Connection) -> Result<Vec<EligibleFile>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, current_filename, modified_at_fs, creator, creator_display
         FROM files
         WHERE missing = 0 AND status = 'current'
           AND file_type IN ('package', 'ts4script')",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(EligibleFile {
                id: r.get(0)?,
                file_name: r.get(1)?,
                mtime: r.get(2)?,
                creator: r.get(3)?,
                creator_display: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Every cached name-lookup that currently points at a mod — the
/// population the re-verify pass judges.
pub fn name_lookup_rows(
    conn: &Connection,
) -> Result<Vec<(String, i64, Option<f64>)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT term, curse_mod_id, confidence FROM curse_name_lookups
         WHERE curse_mod_id IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn id_list(ids: &[i64]) -> String {
    ids.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Remove one term's name-kind rows for one mod, scoped to that term's
/// files — terms can share a mod, and another term's verdict is its own.
pub fn delete_name_matches(
    conn: &Connection,
    file_ids: &[i64],
    mod_id: i64,
) -> Result<usize, DbError> {
    if file_ids.is_empty() || file_ids.len() > 500 {
        return Ok(0);
    }
    let n = conn.execute(
        &format!(
            "DELETE FROM curse_matches
             WHERE match_kind = 'name' AND curse_mod_id = ?1
               AND file_id IN ({})",
            id_list(file_ids)
        ),
        params![mod_id],
    )?;
    Ok(n)
}

pub fn update_name_confidence(
    conn: &Connection,
    file_ids: &[i64],
    mod_id: i64,
    confidence: f64,
) -> Result<(), DbError> {
    if file_ids.is_empty() || file_ids.len() > 500 {
        return Ok(());
    }
    conn.execute(
        &format!(
            "UPDATE curse_matches SET confidence = ?2
             WHERE match_kind = 'name' AND curse_mod_id = ?1
               AND file_id IN ({})",
            id_list(file_ids)
        ),
        params![mod_id, confidence],
    )?;
    Ok(())
}

/// A dropped verdict clears the lookup so a future Check may re-search
/// the term under current standards.
pub fn null_lookup(conn: &Connection, term: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE curse_name_lookups
         SET curse_mod_id = NULL, mod_name = NULL, confidence = NULL
         WHERE term = ?1",
        params![term],
    )?;
    Ok(())
}
