//! Troubleshooting session persistence.
//!
//! The engine in [`crate::troubleshoot`] owns the state machine; this module
//! owns the rows. Member rows are updated one-by-one as each verified move
//! completes, so the database is never ahead of the disk by more than the
//! single move currently in flight — that is the property the startup
//! reconciler relies on.

use super::{now_rfc3339, DbError};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateView {
    pub file_id: i64,
    pub relative_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: i64,
    pub status: String,
    pub phase: String,
    pub round: i64,
    pub problem_note: Option<String>,
    pub outcome: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub total: i64,
    pub in_count: i64,
    pub out_count: i64,
    pub pool_size: i64,
    /// Populated whenever the suspect pool holds exactly one file — during
    /// the confirmation phase and after a confirmed completion.
    pub candidate: Option<CandidateView>,
}

#[derive(Clone, Debug)]
pub struct MemberRow {
    pub file_id: i64,
    pub relative_path: String,
    pub sha256: Option<String>,
    pub location: String,
    pub holding_relative: Option<String>,
    pub in_pool: bool,
}

/// Files eligible for a session: real, present, game-affecting content.
pub fn enrollable_files(conn: &Connection) -> Result<Vec<(i64, String, Option<String>)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, relative_path, sha256 FROM files
         WHERE missing = 0
           AND status = 'current'
           AND file_type IN ('package', 'ts4script')
         ORDER BY relative_path COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn active_session_id(conn: &Connection) -> Result<Option<i64>, DbError> {
    Ok(conn
        .query_row(
            "SELECT id FROM troubleshoot_sessions WHERE status = 'active'
             ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn create_session(conn: &Connection, note: Option<&str>) -> Result<i64, DbError> {
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO troubleshoot_sessions (created_at, updated_at, problem_note)
         VALUES (?1, ?1, ?2)",
        params![now, note],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_members(
    conn: &mut Connection,
    session_id: i64,
    files: &[(i64, String, Option<String>)],
) -> Result<(), DbError> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO troubleshoot_members
                (session_id, file_id, relative_path, sha256)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (file_id, rel, sha) in files {
            stmt.execute(params![session_id, file_id, rel, sha])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn members(conn: &Connection, session_id: i64) -> Result<Vec<MemberRow>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT file_id, relative_path, sha256, location, holding_relative, in_pool
         FROM troubleshoot_members WHERE session_id = ?1
         ORDER BY relative_path COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([session_id], |r| {
            Ok(MemberRow {
                file_id: r.get(0)?,
                relative_path: r.get(1)?,
                sha256: r.get(2)?,
                location: r.get(3)?,
                holding_relative: r.get(4)?,
                in_pool: r.get::<_, i64>(5)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Record where a member now physically lives. Called after each verified
/// move completes, never before.
pub fn set_member_location(
    conn: &Connection,
    session_id: i64,
    file_id: i64,
    location: &str,
    holding_relative: Option<&str>,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE troubleshoot_members
         SET location = ?3, holding_relative = ?4
         WHERE session_id = ?1 AND file_id = ?2",
        params![session_id, file_id, location, holding_relative],
    )?;
    touch(conn, session_id)?;
    Ok(())
}

/// Shrink the suspect pool to the members currently at `keep_location`;
/// everyone else is exonerated in place.
pub fn shrink_pool(conn: &Connection, session_id: i64, keep_location: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE troubleshoot_members SET in_pool = 0
         WHERE session_id = ?1 AND in_pool = 1 AND location <> ?2",
        params![session_id, keep_location],
    )?;
    touch(conn, session_id)?;
    Ok(())
}

pub fn set_phase_round(
    conn: &Connection,
    session_id: i64,
    phase: &str,
    round: i64,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE troubleshoot_sessions
         SET phase = ?2, round = ?3, updated_at = ?4
         WHERE id = ?1",
        params![session_id, phase, round, now_rfc3339()],
    )?;
    Ok(())
}

pub fn complete(
    conn: &Connection,
    session_id: i64,
    status: &str,
    outcome: &str,
    culprit_file_id: Option<i64>,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE troubleshoot_sessions
         SET status = ?2, outcome = ?3, culprit_file_id = ?4, updated_at = ?5
         WHERE id = ?1",
        params![session_id, status, outcome, culprit_file_id, now_rfc3339()],
    )?;
    Ok(())
}

fn touch(conn: &Connection, session_id: i64) -> Result<(), DbError> {
    conn.execute(
        "UPDATE troubleshoot_sessions SET updated_at = ?2 WHERE id = ?1",
        params![session_id, now_rfc3339()],
    )?;
    Ok(())
}

pub fn session_view(conn: &Connection, session_id: i64) -> Result<SessionView, DbError> {
    let (id, status, phase, round, problem_note, outcome, created_at, updated_at) = conn
        .query_row(
            "SELECT id, status, phase, round, problem_note, outcome,
                    created_at, updated_at
             FROM troubleshoot_sessions WHERE id = ?1",
            [session_id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            },
        )?;
    let (total, in_count, pool_size): (i64, i64, i64) = conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN location = 'in' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(in_pool), 0)
         FROM troubleshoot_members WHERE session_id = ?1",
        [session_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let candidate = if pool_size == 1 {
        conn.query_row(
            "SELECT file_id, relative_path FROM troubleshoot_members
             WHERE session_id = ?1 AND in_pool = 1",
            [session_id],
            |r| {
                Ok(CandidateView {
                    file_id: r.get(0)?,
                    relative_path: r.get(1)?,
                })
            },
        )
        .optional()?
    } else {
        None
    };
    Ok(SessionView {
        id,
        status,
        phase,
        round,
        problem_note,
        outcome,
        created_at,
        updated_at,
        total,
        in_count,
        out_count: total - in_count,
        pool_size,
        candidate,
    })
}
