//! Profile persistence. A profile names the person or setup holding the
//! save; exactly one may be active (enforced by a partial unique index),
//! and the active profile's name is what the welcome header greets.
//!
//! The enable/disable mod sets that will belong to each profile arrive in a
//! later migration — this module deliberately stays small until then.

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
}

fn row_to_view(r: &rusqlite::Row<'_>) -> Result<ProfileView, rusqlite::Error> {
    Ok(ProfileView {
        id: r.get(0)?,
        name: r.get(1)?,
        created_at: r.get(2)?,
        updated_at: r.get(3)?,
        is_active: r.get::<_, i64>(4)? != 0,
    })
}

const COLS: &str = "id, name, created_at, updated_at, is_active";

pub fn list_profiles(conn: &Connection) -> Result<Vec<ProfileView>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM profiles ORDER BY name COLLATE NOCASE"
    ))?;
    let rows = stmt
        .query_map([], row_to_view)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn active_profile(conn: &Connection) -> Result<Option<ProfileView>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM profiles WHERE is_active = 1"),
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
    let view = tx.query_row(
        &format!("SELECT {COLS} FROM profiles WHERE id = ?1"),
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
