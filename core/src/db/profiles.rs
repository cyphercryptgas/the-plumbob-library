//! Profile persistence. A profile names the person or setup holding the
//! save; exactly one may be active (enforced by a partial unique index),
//! and the active profile's name is what the welcome header greets.
//!
//! Each profile also owns a set of files it keeps disabled. The ACTIVE
//! profile's set live-tracks reality — [`sync_active_set`] runs after every
//! operation that changes enabled states — while inactive profiles hold
//! their sets frozen until switched to via a [`switch_plan`].

use super::{now_rfc3339, DbError};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
    /// How many currently-known files this profile keeps disabled.
    pub disabled_count: i64,
}

fn row_to_view(r: &rusqlite::Row<'_>) -> Result<ProfileView, rusqlite::Error> {
    Ok(ProfileView {
        id: r.get(0)?,
        name: r.get(1)?,
        created_at: r.get(2)?,
        updated_at: r.get(3)?,
        is_active: r.get::<_, i64>(4)? != 0,
        disabled_count: r.get(5)?,
    })
}

const COLS: &str = "p.id, p.name, p.created_at, p.updated_at, p.is_active,
    (SELECT COUNT(*) FROM profile_disabled d
       JOIN files f ON f.id = d.file_id AND f.status = 'current'
     WHERE d.profile_id = p.id)";

pub fn list_profiles(conn: &Connection) -> Result<Vec<ProfileView>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM profiles p ORDER BY p.name COLLATE NOCASE"
    ))?;
    let rows = stmt
        .query_map([], row_to_view)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn active_profile(conn: &Connection) -> Result<Option<ProfileView>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM profiles p WHERE p.is_active = 1"),
            [],
            row_to_view,
        )
        .optional()?)
}

/// Create a profile. The very first profile becomes active automatically —
/// creating your name and then not being greeted would be a small cruelty.
pub fn create_profile(conn: &mut Connection, name: &str) -> Result<ProfileView, DbError> {
    let now = now_rfc3339();
    let tx = conn.transaction()?;
    let count: i64 = tx.query_row("SELECT COUNT(*) FROM profiles", [], |r| r.get(0))?;
    tx.execute(
        "INSERT INTO profiles (name, created_at, updated_at, is_active)
         VALUES (?1, ?2, ?2, ?3)",
        params![name, now, i64::from(count == 0)],
    )?;
    let id = tx.last_insert_rowid();
    // A new profile is a named snapshot of the setup you have right now.
    tx.execute(
        "INSERT INTO profile_disabled (profile_id, file_id)
         SELECT ?1, id FROM files WHERE enabled = 0 AND status = 'current'",
        [id],
    )?;
    let view = tx.query_row(
        &format!("SELECT {COLS} FROM profiles p WHERE p.id = ?1"),
        [id],
        row_to_view,
    )?;
    tx.commit()?;
    Ok(view)
}

pub fn rename_profile(conn: &Connection, id: i64, name: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE profiles SET name = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, name, now_rfc3339()],
    )?;
    Ok(())
}

/// Make one profile active; the partial unique index guarantees the swap
/// can never leave two.
pub fn set_active_profile(conn: &mut Connection, id: i64) -> Result<(), DbError> {
    let now = now_rfc3339();
    let tx = conn.transaction()?;
    tx.execute("UPDATE profiles SET is_active = 0 WHERE is_active = 1", [])?;
    let changed = tx.execute(
        "UPDATE profiles SET is_active = 1, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    if changed == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows.into());
    }
    tx.commit()?;
    Ok(())
}

pub fn delete_profile(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM profiles WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn first_profile_becomes_active_automatically() {
        let mut db = Database::open_in_memory().unwrap();
        let p = create_profile(db.conn_mut(), "Michael").unwrap();
        assert!(p.is_active);
        let second = create_profile(db.conn_mut(), "Guest").unwrap();
        assert!(!second.is_active);
        assert_eq!(active_profile(db.conn()).unwrap().unwrap().id, p.id);
    }

    #[test]
    fn names_are_unique_case_insensitively() {
        let mut db = Database::open_in_memory().unwrap();
        create_profile(db.conn_mut(), "Michael").unwrap();
        let err = create_profile(db.conn_mut(), "MICHAEL").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
    }

    #[test]
    fn activation_swaps_and_can_never_leave_two() {
        let mut db = Database::open_in_memory().unwrap();
        let a = create_profile(db.conn_mut(), "A").unwrap();
        let b = create_profile(db.conn_mut(), "B").unwrap();
        set_active_profile(db.conn_mut(), b.id).unwrap();
        let list = list_profiles(db.conn()).unwrap();
        assert_eq!(list.iter().filter(|p| p.is_active).count(), 1);
        assert!(list.iter().find(|p| p.id == b.id).unwrap().is_active);
        assert!(!list.iter().find(|p| p.id == a.id).unwrap().is_active);
    }

    #[test]
    fn activating_a_missing_profile_fails_cleanly() {
        let mut db = Database::open_in_memory().unwrap();
        create_profile(db.conn_mut(), "A").unwrap();
        assert!(set_active_profile(db.conn_mut(), 999).is_err());
        // The failed transaction must not have deactivated the survivor.
        assert!(active_profile(db.conn()).unwrap().is_some());
    }

    fn seed_file(db: &mut Database, rel: &str, enabled: bool) -> i64 {
        db.conn()
            .execute(
                "INSERT INTO files (current_filename, absolute_path, relative_path,
                    file_type, size_bytes, first_seen_at, last_seen_at, enabled)
                 VALUES (?1, ?2, ?1, 'package', 1, '2026-01-01T00:00:00Z',
                         '2026-01-01T00:00:00Z', ?3)",
                rusqlite::params![rel, format!("/m/{rel}"), enabled],
            )
            .unwrap();
        db.conn().last_insert_rowid()
    }

    fn set_enabled(db: &Database, id: i64, enabled: bool) {
        db.conn()
            .execute(
                "UPDATE files SET enabled = ?2 WHERE id = ?1",
                rusqlite::params![id, enabled],
            )
            .unwrap();
    }

    #[test]
    fn creating_a_profile_snapshots_the_current_setup() {
        let mut db = Database::open_in_memory().unwrap();
        seed_file(&mut db, "on.package", true);
        seed_file(&mut db, "off.package", false);
        let p = create_profile(db.conn_mut(), "Michael").unwrap();
        assert_eq!(p.disabled_count, 1);
    }

    #[test]
    fn the_active_profile_live_tracks_disk_truth() {
        let mut db = Database::open_in_memory().unwrap();
        let a = seed_file(&mut db, "a.package", true);
        let p = create_profile(db.conn_mut(), "Michael").unwrap();
        assert_eq!(p.disabled_count, 0);
        set_enabled(&db, a, false);
        sync_active_set(db.conn_mut()).unwrap();
        assert_eq!(
            list_profiles(db.conn()).unwrap()[0].disabled_count,
            1,
            "toggles write through to the active set"
        );
        set_enabled(&db, a, true);
        sync_active_set(db.conn_mut()).unwrap();
        assert_eq!(list_profiles(db.conn()).unwrap()[0].disabled_count, 0);
        // With nothing active, sync is a harmless no-op.
        let id = list_profiles(db.conn()).unwrap()[0].id;
        delete_profile(db.conn(), id).unwrap();
        sync_active_set(db.conn_mut()).unwrap();
    }

    #[test]
    fn switch_plan_is_set_algebra_with_honest_unavailability() {
        let mut db = Database::open_in_memory().unwrap();
        let x = seed_file(&mut db, "x.package", true);
        let y = seed_file(&mut db, "y.package", true);
        let z = seed_file(&mut db, "z.package", true);
        // Profile A: current setup (nothing disabled) — becomes active.
        create_profile(db.conn_mut(), "A").unwrap();
        // Profile B captured while y and z were disabled.
        set_enabled(&db, y, false);
        set_enabled(&db, z, false);
        let b = create_profile(db.conn_mut(), "B").unwrap();
        assert_eq!(b.disabled_count, 2);
        // Back to A's world plus x disabled by hand.
        set_enabled(&db, y, true);
        set_enabled(&db, z, true);
        set_enabled(&db, x, false);
        sync_active_set(db.conn_mut()).unwrap();

        let plan = switch_plan(db.conn(), b.id).unwrap();
        let dis: Vec<_> = plan.to_disable.iter().map(|t| t.relative_path.as_str()).collect();
        let ena: Vec<_> = plan.to_enable.iter().map(|t| t.relative_path.as_str()).collect();
        assert_eq!(dis, vec!["y.package", "z.package"]);
        assert_eq!(ena, vec!["x.package"]);
        assert!(plan.unavailable.is_empty());

        // z goes missing since B was captured: reported, never dropped.
        db.conn()
            .execute("UPDATE files SET missing = 1 WHERE id = ?1", [z])
            .unwrap();
        let plan = switch_plan(db.conn(), b.id).unwrap();
        assert_eq!(
            plan.to_disable.iter().map(|t| t.relative_path.as_str()).collect::<Vec<_>>(),
            vec!["y.package"]
        );
        assert_eq!(plan.unavailable, vec!["z.package".to_string()]);
    }

    #[test]
    fn rename_and_delete_behave() {
        let mut db = Database::open_in_memory().unwrap();
        let p = create_profile(db.conn_mut(), "Draft").unwrap();
        rename_profile(db.conn(), p.id, "Michael").unwrap();
        assert_eq!(
            list_profiles(db.conn()).unwrap()[0].name,
            "Michael".to_string()
        );
        delete_profile(db.conn(), p.id).unwrap();
        assert!(list_profiles(db.conn()).unwrap().is_empty());
        assert!(active_profile(db.conn()).unwrap().is_none());
    }
}

// ---------------------------------------------------------------------------
// Mod sets
// ---------------------------------------------------------------------------

/// Rewrite the ACTIVE profile's disabled set from disk truth. Cheap (the
/// set is sparse) and impossible to drift; call after any operation that
/// changes enabled states. A no-op when no profile is active.
pub fn sync_active_set(conn: &mut Connection) -> Result<(), DbError> {
    let tx = conn.transaction()?;
    let active: Option<i64> = tx
        .query_row(
            "SELECT id FROM profiles WHERE is_active = 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = active {
        tx.execute("DELETE FROM profile_disabled WHERE profile_id = ?1", [id])?;
        tx.execute(
            "INSERT INTO profile_disabled (profile_id, file_id)
             SELECT ?1, id FROM files WHERE enabled = 0 AND status = 'current'",
            [id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// One side of a planned switch: the file plus what the verified rename
/// needs to know about it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedToggle {
    pub file_id: i64,
    pub relative_path: String,
    pub sha256: Option<String>,
}

/// The read-only diff between the library's current enabled state and a
/// target profile's stored set. Files the target wants disabled that are
/// now missing or quarantined land in `unavailable` — reported, never
/// silently dropped from intent.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchPlan {
    pub to_disable: Vec<PlannedToggle>,
    pub to_enable: Vec<PlannedToggle>,
    pub unavailable: Vec<String>,
}

pub fn switch_plan(conn: &Connection, target_id: i64) -> Result<SwitchPlan, DbError> {
    let mut plan = SwitchPlan::default();
    let mut stmt = conn.prepare(
        "SELECT f.id, f.relative_path, f.sha256, f.missing, f.status
         FROM profile_disabled d JOIN files f ON f.id = d.file_id
         WHERE d.profile_id = ?1 AND f.enabled = 1
         ORDER BY f.relative_path COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([target_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, i64>(3)? != 0,
            r.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (file_id, relative_path, sha256, missing, status) = row?;
        if missing || status != "current" {
            plan.unavailable.push(relative_path);
        } else {
            plan.to_disable.push(PlannedToggle {
                file_id,
                relative_path,
                sha256,
            });
        }
    }
    let mut stmt = conn.prepare(
        "SELECT f.id, f.relative_path, f.sha256
         FROM files f
         WHERE f.enabled = 0 AND f.status = 'current' AND f.missing = 0
           AND f.id NOT IN
               (SELECT file_id FROM profile_disabled WHERE profile_id = ?1)
         ORDER BY f.relative_path COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([target_id], |r| {
        Ok(PlannedToggle {
            file_id: r.get(0)?,
            relative_path: r.get(1)?,
            sha256: r.get(2)?,
        })
    })?;
    for row in rows {
        plan.to_enable.push(row?);
    }
    Ok(plan)
}
