//! Catalog repository: creators, mods, categories, tags, collections — the
//! organizational vocabulary users attach to files. Lean, fully-parameterized
//! CRUD; richer editing flows arrive with the Library screens.

use super::{now_rfc3339, DbError};
use rusqlite::{params, Connection, OptionalExtension};

// ---------------------------------------------------------------------------
// Creators
// ---------------------------------------------------------------------------

/// Get-or-create by name. NOCASE uniqueness means "FeralPoodles" and
/// "feralpoodles" resolve to the same creator instead of silently forking.
pub fn ensure_creator(conn: &Connection, name: &str) -> Result<i64, DbError> {
    if let Some(id) = conn
        .query_row("SELECT id FROM creators WHERE name = ?1", [name], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?
    {
        return Ok(id);
    }
    conn.execute("INSERT INTO creators (name) VALUES (?1)", [name])?;
    Ok(conn.last_insert_rowid())
}

pub fn set_creator_links(
    conn: &Connection,
    creator_id: i64,
    website_url: Option<&str>,
    patreon_url: Option<&str>,
    curseforge_url: Option<&str>,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE creators SET website_url = ?2, patreon_url = ?3, curseforge_url = ?4
         WHERE id = ?1",
        params![creator_id, website_url, patreon_url, curseforge_url],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Mods
// ---------------------------------------------------------------------------

pub fn create_mod(
    conn: &Connection,
    name: &str,
    creator_id: Option<i64>,
    category_id: Option<i64>,
) -> Result<i64, DbError> {
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO mods (name, creator_id, category_id, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'unidentified', ?4, ?4)",
        params![name, creator_id, category_id, now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Attach files to a mod record (a "mod" groups its .package/.ts4script
/// siblings). Transactional; also stamps the mod's `updated_at`.
pub fn assign_files_to_mod(
    conn: &mut Connection,
    mod_id: i64,
    file_ids: &[i64],
) -> Result<(), DbError> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare("UPDATE files SET mod_id = ?1 WHERE id = ?2")?;
        for fid in file_ids {
            stmt.execute(params![mod_id, fid])?;
        }
    }
    tx.execute(
        "UPDATE mods SET updated_at = ?2 WHERE id = ?1",
        params![mod_id, now_rfc3339()],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn set_mod_source(
    conn: &Connection,
    mod_id: i64,
    provider: Option<&str>,
    url: Option<&str>,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE mods SET source_provider = ?2, source_url = ?3, updated_at = ?4
         WHERE id = ?1",
        params![mod_id, provider, url, now_rfc3339()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryRow {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub sort_order: i64,
    pub system_category: bool,
}

pub fn list_categories(conn: &Connection) -> Result<Vec<CategoryRow>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, parent_id, sort_order, system_category
         FROM categories ORDER BY sort_order, name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(CategoryRow {
            id: r.get(0)?,
            name: r.get(1)?,
            parent_id: r.get(2)?,
            sort_order: r.get(3)?,
            system_category: r.get::<_, i64>(4)? != 0,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn category_id_by_name(conn: &Connection, name: &str) -> Result<Option<i64>, DbError> {
    Ok(conn
        .query_row("SELECT id FROM categories WHERE name = ?1", [name], |r| {
            r.get(0)
        })
        .optional()?)
}

/// Per-file category assignment (used before a file is grouped into a mod).
pub fn set_file_category(
    conn: &Connection,
    file_id: i64,
    category_id: Option<i64>,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE files SET category_id = ?2 WHERE id = ?1",
        params![file_id, category_id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

pub fn ensure_tag(
    conn: &Connection,
    name: &str,
    color_token: Option<&str>,
) -> Result<i64, DbError> {
    if let Some(id) = conn
        .query_row("SELECT id FROM tags WHERE name = ?1", [name], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO tags (name, color_token) VALUES (?1, ?2)",
        params![name, color_token],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn tag_mod(conn: &Connection, mod_id: i64, tag_id: i64) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO mod_tags (mod_id, tag_id) VALUES (?1, ?2)",
        params![mod_id, tag_id],
    )?;
    Ok(())
}

pub fn untag_mod(conn: &Connection, mod_id: i64, tag_id: i64) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM mod_tags WHERE mod_id = ?1 AND tag_id = ?2",
        params![mod_id, tag_id],
    )?;
    Ok(())
}

pub fn mod_tag_names(conn: &Connection, mod_id: i64) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t
         JOIN mod_tags mt ON mt.tag_id = t.id
         WHERE mt.mod_id = ?1 ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([mod_id], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

pub fn create_collection(
    conn: &Connection,
    name: &str,
    description: Option<&str>,
) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO collections (name, description, created_at) VALUES (?1, ?2, ?3)",
        params![name, description, now_rfc3339()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn add_mod_to_collection(
    conn: &Connection,
    collection_id: i64,
    mod_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO collection_mods (collection_id, mod_id) VALUES (?1, ?2)",
        params![collection_id, mod_id],
    )?;
    Ok(())
}

pub fn collection_mod_ids(
    conn: &Connection,
    collection_id: i64,
) -> Result<Vec<i64>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT mod_id FROM collection_mods WHERE collection_id = ?1 ORDER BY mod_id",
    )?;
    let rows = stmt.query_map([collection_id], |r| r.get::<_, i64>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn creators_deduplicate_case_insensitively() {
        let db = Database::open_in_memory().unwrap();
        let a = ensure_creator(db.conn(), "FeralPoodles").unwrap();
        let b = ensure_creator(db.conn(), "feralpoodles").unwrap();
        assert_eq!(a, b);
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM creators", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn mods_link_creators_categories_and_files() {
        let mut db = Database::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO files (current_filename, absolute_path, relative_path,
                    file_type, first_seen_at, last_seen_at)
                 VALUES ('ui.package', '/m/ui.package', 'ui.package', 'package',
                    '2026', '2026'),
                        ('ui.ts4script', '/m/ui.ts4script', 'ui.ts4script', 'ts4script',
                    '2026', '2026')",
                [],
            )
            .unwrap();
        let creator = ensure_creator(db.conn(), "Weerbesu").unwrap();
        let category = category_id_by_name(db.conn(), "Gameplay Mod")
            .unwrap()
            .expect("seeded");
        let mod_id = create_mod(db.conn(), "UI Cheats", Some(creator), Some(category)).unwrap();
        assign_files_to_mod(db.conn_mut(), mod_id, &[1, 2]).unwrap();

        let linked: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM files WHERE mod_id = ?1",
                [mod_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(linked, 2);
    }

    #[test]
    fn seeded_categories_are_queryable_and_ordered() {
        let db = Database::open_in_memory().unwrap();
        let cats = list_categories(db.conn()).unwrap();
        assert!(cats.len() >= 27);
        assert!(cats.iter().all(|c| c.system_category));
        let hair = cats.iter().find(|c| c.name == "Hair").unwrap();
        let cas = cats.iter().find(|c| c.name == "CAS").unwrap();
        assert_eq!(hair.parent_id, Some(cas.id));
    }

    #[test]
    fn file_category_assignment_round_trips() {
        let db = Database::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO files (current_filename, absolute_path, relative_path,
                    file_type, first_seen_at, last_seen_at)
                 VALUES ('h.package', '/m/h.package', 'h.package', 'package',
                    '2026', '2026')",
                [],
            )
            .unwrap();
        let hair = category_id_by_name(db.conn(), "Hair").unwrap().unwrap();
        set_file_category(db.conn(), 1, Some(hair)).unwrap();
        let got: Option<i64> = db
            .conn()
            .query_row("SELECT category_id FROM files WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(got, Some(hair));
    }

    #[test]
    fn tags_attach_detach_and_dedupe() {
        let db = Database::open_in_memory().unwrap();
        let mod_id = create_mod(db.conn(), "Some Mod", None, None).unwrap();
        let tag = ensure_tag(db.conn(), "favorite", Some("dusty-rose")).unwrap();
        let same = ensure_tag(db.conn(), "FAVORITE", None).unwrap();
        assert_eq!(tag, same);
        tag_mod(db.conn(), mod_id, tag).unwrap();
        tag_mod(db.conn(), mod_id, tag).unwrap(); // idempotent
        assert_eq!(mod_tag_names(db.conn(), mod_id).unwrap(), vec!["favorite"]);
        untag_mod(db.conn(), mod_id, tag).unwrap();
        assert!(mod_tag_names(db.conn(), mod_id).unwrap().is_empty());
    }

    #[test]
    fn deleting_a_collection_cascades_membership_but_not_mods() {
        let db = Database::open_in_memory().unwrap();
        let mod_id = create_mod(db.conn(), "Kept Mod", None, None).unwrap();
        let coll = create_collection(db.conn(), "Storytelling", Some("cozy saves")).unwrap();
        add_mod_to_collection(db.conn(), coll, mod_id).unwrap();
        assert_eq!(collection_mod_ids(db.conn(), coll).unwrap(), vec![mod_id]);

        db.conn()
            .execute("DELETE FROM collections WHERE id = ?1", [coll])
            .unwrap();
        let links: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM collection_mods", [], |r| r.get(0))
            .unwrap();
        assert_eq!(links, 0, "membership rows cascade away");
        let mods: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM mods", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mods, 1, "the mod itself survives");
    }

    #[test]
    fn deleting_a_mod_releases_its_files_instead_of_deleting_them() {
        let mut db = Database::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO files (current_filename, absolute_path, relative_path,
                    file_type, first_seen_at, last_seen_at)
                 VALUES ('a.package', '/m/a.package', 'a.package', 'package',
                    '2026', '2026')",
                [],
            )
            .unwrap();
        let mod_id = create_mod(db.conn(), "Doomed", None, None).unwrap();
        assign_files_to_mod(db.conn_mut(), mod_id, &[1]).unwrap();
        db.conn()
            .execute("DELETE FROM mods WHERE id = ?1", [mod_id])
            .unwrap();
        let (count, linked): (i64, Option<i64>) = db
            .conn()
            .query_row("SELECT COUNT(*), MAX(mod_id) FROM files", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(count, 1, "file records are never deleted by catalog edits");
        assert_eq!(linked, None, "mod link is released (ON DELETE SET NULL)");
    }
}
