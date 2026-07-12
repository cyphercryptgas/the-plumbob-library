//! The 50/50 troubleshooting assistant — core engine.
//!
//! A session is a persistent, resumable binary search over the library for
//! the one file causing an in-game problem. The user tests the game between
//! rounds and reports a verdict; the engine arranges each round by moving
//! files between the Mods root and a managed holding area with hash-verified
//! moves, journaling every step through the shared [`JournalSink`].
//!
//! State machine (resting phases only — the database never records a
//! mid-move state; member rows are updated as each move lands):
//!
//! ```text
//! start ─▶ baseline ──gone──▶ completed(no_problem)
//!             │present
//!             ▼
//!          testing ◀────────────┐
//!             │verdict          │ pool > 1: next round
//!             ▼                 │
//!        shrink pool ───────────┘
//!             │ pool == 1
//!             ▼
//!         confirming ──present──▶ restore all ▶ completed(inconclusive)
//!             │gone
//!             ▼
//!   restore all, quarantine culprit ▶ completed(culprit_confirmed)
//!
//! abort (any active phase) ▶ restore all ▶ aborted
//! ```
//!
//! Decisions, stated once: exonerated halves stay set aside until the
//! session ends (fewer moves, cleaner tests); there is no pre-session full
//! backup because files are moved rather than copied — the holding copies
//! are the originals, and every move verifies content hashes both ways; a
//! confirmed culprit is handed to the existing quarantine system so it shows
//! up where set-aside files already live. If the search ends inconclusive,
//! the likely cause is an interaction between files — rerunning the session
//! after quarantining one culprit finds the next.

use crate::db::ops::record_quarantine_outcome;
use crate::db::troubleshoot as store;
use crate::db::{rel_to_db_string, DbError};
use crate::ops::{
    new_operation_id, quarantine_files, verified_move, JournalEvent, JournalSink, OpError,
    QuarantineRequest,
};
use crate::paths::{collision_free, PathError, SafeRoot};
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub use crate::db::troubleshoot::{CandidateView, SessionView};

/// The three managed roots a session moves files between. All access goes
/// through [`SafeRoot`] so nothing can escape its directory.
pub struct TroubleshootRoots<'a> {
    pub mods: &'a SafeRoot,
    pub holding: &'a SafeRoot,
    pub quarantine: &'a SafeRoot,
}

#[derive(Clone, Copy, Debug)]
pub enum Verdict {
    ProblemPresent,
    ProblemGone,
}

#[derive(Debug, thiserror::Error)]
pub enum TsError {
    #[error("a troubleshooting session is already active (#{0})")]
    ActiveSessionExists(i64),
    #[error("no package or script files are available to troubleshoot")]
    EmptyPool,
    #[error("troubleshooting session #{0} is not active")]
    NotActive(i64),
    #[error("session #{id} is resting in phase '{phase}'; that action does not apply")]
    WrongPhase { id: i64, phase: String },
    #[error("arrangement failed at {path}: {message} — completed moves were rolled back")]
    ArrangementFailed { path: PathBuf, message: String },
    #[error("session #{0} broke an internal invariant: {1}")]
    Invariant(i64, String),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Op(#[from] OpError),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
}

/// What the startup reconciler found and fixed. `conflicts` and `missing`
/// are reported, never auto-resolved — a file existing in both places or in
/// neither needs a human decision.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileReport {
    pub healed: usize,
    pub conflicts: Vec<String>,
    pub missing: Vec<String>,
}

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

/// Begin a session: enroll every current package/script file and rest in the
/// baseline phase. No files move until the user confirms the problem exists.
pub fn start_session(
    conn: &mut Connection,
    _roots: &TroubleshootRoots,
    note: Option<&str>,
) -> Result<SessionView, TsError> {
    if let Some(id) = store::active_session_id(conn)? {
        return Err(TsError::ActiveSessionExists(id));
    }
    let files = store::enrollable_files(conn)?;
    if files.is_empty() {
        return Err(TsError::EmptyPool);
    }
    let session_id = store::create_session(conn, note)?;
    store::insert_members(conn, session_id, &files)?;
    Ok(store::session_view(conn, session_id)?)
}

pub fn active_session(conn: &Connection) -> Result<Option<SessionView>, TsError> {
    match store::active_session_id(conn)? {
        Some(id) => Ok(Some(store::session_view(conn, id)?)),
        None => Ok(None),
    }
}

/// Drive the state machine with the user's verdict for the current phase.
pub fn submit_verdict(
    conn: &mut Connection,
    roots: &TroubleshootRoots,
    session_id: i64,
    verdict: Verdict,
    journal: &mut dyn JournalSink,
) -> Result<SessionView, TsError> {
    let view = store::session_view(conn, session_id)?;
    if view.status != "active" {
        return Err(TsError::NotActive(session_id));
    }
    match (view.phase.as_str(), verdict) {
        ("baseline", Verdict::ProblemGone) => {
            // Nothing was ever moved; there is nothing to hunt.
            store::complete(conn, session_id, "completed", "no_problem", None)?;
        }
        ("baseline", Verdict::ProblemPresent) => {
            advance_pool(conn, roots, session_id, view.round, journal)?;
        }
        ("testing", v) => {
            let keep = match v {
                Verdict::ProblemPresent => "in",
                Verdict::ProblemGone => "out",
            };
            store::shrink_pool(conn, session_id, keep)?;
            advance_pool(conn, roots, session_id, view.round, journal)?;
        }
        ("confirming", v) => {
            let candidate = view.candidate.clone().ok_or_else(|| {
                TsError::Invariant(session_id, "confirming with no candidate".into())
            })?;
            match v {
                Verdict::ProblemGone => {
                    // Confirmed. Bring everything home, then hand the culprit
                    // to quarantine through the existing, journaled path.
                    arrange(
                        conn,
                        roots,
                        session_id,
                        "troubleshoot_restore",
                        all_in(conn, session_id)?,
                        journal,
                    )?;
                    let member = member_by_file(conn, session_id, candidate.file_id)?;
                    let request = QuarantineRequest {
                        source_relative: PathBuf::from(&member.relative_path),
                        reason: "Troubleshooter: confirmed culprit".into(),
                        expected_sha256: member.sha256.clone(),
                    };
                    let outcome =
                        quarantine_files(roots.mods, roots.quarantine, &[request], true, journal);
                    if let Some(failure) = outcome.failed.first() {
                        // Everything is safely back in Mods; the session rests
                        // in `confirming` so the verdict can simply be retried.
                        return Err(TsError::ArrangementFailed {
                            path: failure.source.clone(),
                            message: failure.message.clone(),
                        });
                    }
                    record_quarantine_outcome(conn, &outcome)?;
                    store::complete(
                        conn,
                        session_id,
                        "completed",
                        "culprit_confirmed",
                        Some(candidate.file_id),
                    )?;
                    prune_session_dir(roots, session_id);
                }
                Verdict::ProblemPresent => {
                    // The problem happens even with the candidate removed —
                    // an interaction, or something outside the pool.
                    arrange(
                        conn,
                        roots,
                        session_id,
                        "troubleshoot_restore",
                        all_in(conn, session_id)?,
                        journal,
                    )?;
                    store::complete(conn, session_id, "completed", "inconclusive", None)?;
                    prune_session_dir(roots, session_id);
                }
            }
        }
        (phase, _) => {
            return Err(TsError::WrongPhase {
                id: session_id,
                phase: phase.to_string(),
            })
        }
    }
    Ok(store::session_view(conn, session_id)?)
}

/// Abort from any active phase: every member returns to its original path,
/// hash-verified, and the session is closed.
pub fn abort_session(
    conn: &mut Connection,
    roots: &TroubleshootRoots,
    session_id: i64,
    journal: &mut dyn JournalSink,
) -> Result<SessionView, TsError> {
    let view = store::session_view(conn, session_id)?;
    if view.status != "active" {
        return Err(TsError::NotActive(session_id));
    }
    arrange(
        conn,
        roots,
        session_id,
        "troubleshoot_abort",
        all_in(conn, session_id)?,
        journal,
    )?;
    store::complete(conn, session_id, "aborted", "aborted", None)?;
    prune_session_dir(roots, session_id);
    Ok(store::session_view(conn, session_id)?)
}

/// Heal member rows from disk truth after a crash. For every member, the
/// file is looked for at both its Mods path and its recorded holding path;
/// when the row disagrees with an unambiguous disk state, the row is fixed.
pub fn reconcile(
    conn: &mut Connection,
    roots: &TroubleshootRoots,
    session_id: i64,
) -> Result<ReconcileReport, TsError> {
    let mut report = ReconcileReport::default();
    for m in store::members(conn, session_id)? {
        let mods_abs = roots.mods.resolve_relative(Path::new(&m.relative_path))?;
        let holding_abs = m
            .holding_relative
            .as_ref()
            .map(|h| roots.holding.path().join(h));
        let in_mods = mods_abs.exists();
        let in_holding = holding_abs.as_ref().map(|p| p.exists()).unwrap_or(false);
        match (in_mods, in_holding) {
            (true, true) => report.conflicts.push(m.relative_path.clone()),
            (false, false) => report.missing.push(m.relative_path.clone()),
            (true, false) if m.location != "in" => {
                store::set_member_location(conn, session_id, m.file_id, "in", None)?;
                report.healed += 1;
            }
            (false, true) if m.location != "out" => {
                let h = m.holding_relative.as_deref();
                store::set_member_location(conn, session_id, m.file_id, "out", h)?;
                report.healed += 1;
            }
            _ => {}
        }
    }
    Ok(report)
}

// ---------------------------------------------------------------------------
// Arrangement planning and execution
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    In,
    Out,
}

/// After a pool change (or the baseline verdict), arrange the next resting
/// state: another split round while the pool holds more than one suspect, or
/// the confirmation arrangement once it holds exactly one.
fn advance_pool(
    conn: &mut Connection,
    roots: &TroubleshootRoots,
    session_id: i64,
    current_round: i64,
    journal: &mut dyn JournalSink,
) -> Result<(), TsError> {
    let members = store::members(conn, session_id)?;
    let pool: Vec<&store::MemberRow> = members.iter().filter(|m| m.in_pool).collect();
    match pool.len() {
        0 => Err(TsError::Invariant(
            session_id,
            "suspect pool is empty".into(),
        )),
        1 => {
            // Confirmation: only the candidate out, everything else home.
            let candidate_id = pool[0].file_id;
            let targets = members
                .iter()
                .map(|m| {
                    let t = if m.file_id == candidate_id {
                        Target::Out
                    } else {
                        Target::In
                    };
                    (m.clone(), t)
                })
                .collect();
            arrange(
                conn,
                roots,
                session_id,
                "troubleshoot_confirm",
                targets,
                journal,
            )?;
            store::set_phase_round(conn, session_id, "confirming", current_round)?;
            Ok(())
        }
        n => {
            // Split the sorted pool: first half stays in, second half goes
            // out. Exonerated members are not touched.
            let half = n.div_ceil(2);
            let targets = pool
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let t = if i < half { Target::In } else { Target::Out };
                    ((*m).clone(), t)
                })
                .collect();
            arrange(
                conn,
                roots,
                session_id,
                "troubleshoot_round",
                targets,
                journal,
            )?;
            store::set_phase_round(conn, session_id, "testing", current_round + 1)?;
            Ok(())
        }
    }
}

fn all_in(conn: &Connection, session_id: i64) -> Result<Vec<(store::MemberRow, Target)>, TsError> {
    Ok(store::members(conn, session_id)?
        .into_iter()
        .map(|m| (m, Target::In))
        .collect())
}

struct DoneMove {
    file_id: i64,
    from: PathBuf,
    to: PathBuf,
    verified_sha: String,
    reverse_location: &'static str,
    reverse_holding: Option<String>,
}

/// Execute the moves that take the session from its current disk state to
/// the target arrangement. One journaled operation; per-file member updates
/// land after each verified move; any failure rolls this arrangement's
/// completed moves back so the session rests exactly where it was.
fn arrange(
    conn: &mut Connection,
    roots: &TroubleshootRoots,
    session_id: i64,
    kind: &str,
    targets: Vec<(store::MemberRow, Target)>,
    journal: &mut dyn JournalSink,
) -> Result<(), TsError> {
    let moves: Vec<(store::MemberRow, Target)> = targets
        .into_iter()
        .filter(|(m, t)| match t {
            Target::In => m.location != "in",
            Target::Out => m.location != "out",
        })
        .collect();
    if moves.is_empty() {
        return Ok(());
    }
    let operation_id = new_operation_id();
    journal.record(JournalEvent::OperationStarted {
        operation_id: operation_id.clone(),
        kind: kind.into(),
        total_steps: moves.len(),
    });

    let session_dir = format!("session-{session_id}");
    let mut done: Vec<DoneMove> = Vec::new();

    for (step, (m, target)) in moves.iter().enumerate() {
        let step = step + 1;
        let rel = Path::new(&m.relative_path);
        let result = (|| -> Result<DoneMove, TsError> {
            match target {
                Target::Out => {
                    let source = roots.mods.resolve_relative(rel)?;
                    let planned = roots.holding.path().join(&session_dir).join(rel);
                    roots.holding.contain(&planned)?;
                    let destination = collision_free(&planned);
                    let sha = verified_move(&source, &destination, m.sha256.as_deref())?;
                    let holding_rel = destination
                        .strip_prefix(roots.holding.path())
                        .map(rel_to_db_string)
                        .unwrap_or_else(|_| destination.to_string_lossy().into_owned());
                    store::set_member_location(
                        conn,
                        session_id,
                        m.file_id,
                        "out",
                        Some(&holding_rel),
                    )?;
                    journal.record(JournalEvent::StepSucceeded {
                        operation_id: operation_id.clone(),
                        step,
                        action: "set_aside".into(),
                        source: source.clone(),
                        destination: Some(destination.clone()),
                        sha256: Some(sha.clone()),
                    });
                    Ok(DoneMove {
                        file_id: m.file_id,
                        from: source,
                        to: destination,
                        verified_sha: sha,
                        reverse_location: "in",
                        reverse_holding: None,
                    })
                }
                Target::In => {
                    let holding_rel = m.holding_relative.clone().ok_or_else(|| {
                        TsError::Invariant(
                            session_id,
                            format!("{} is out with no holding path", m.relative_path),
                        )
                    })?;
                    let source = roots.holding.path().join(&holding_rel);
                    roots.holding.contain(&source)?;
                    let destination = roots.mods.resolve_relative(rel)?;
                    let sha = verified_move(&source, &destination, m.sha256.as_deref())?;
                    store::set_member_location(conn, session_id, m.file_id, "in", None)?;
                    journal.record(JournalEvent::StepSucceeded {
                        operation_id: operation_id.clone(),
                        step,
                        action: "restore".into(),
                        source: source.clone(),
                        destination: Some(destination.clone()),
                        sha256: Some(sha.clone()),
                    });
                    Ok(DoneMove {
                        file_id: m.file_id,
                        from: source,
                        to: destination,
                        verified_sha: sha,
                        reverse_location: "out",
                        reverse_holding: Some(holding_rel),
                    })
                }
            }
        })();

        match result {
            Ok(done_move) => done.push(done_move),
            Err(e) => {
                journal.record(JournalEvent::StepFailed {
                    operation_id: operation_id.clone(),
                    step,
                    action: "arrange".into(),
                    source: roots
                        .mods
                        .resolve_relative(rel)
                        .unwrap_or_else(|_| PathBuf::from(&m.relative_path)),
                    message: e.to_string(),
                });
                // Roll this arrangement back so the session rests in a state
                // the phase column still describes. Row updates follow each
                // successful reverse move; a file we cannot move back keeps
                // its (accurate) moved-state row.
                let mut rollback_failures = 0usize;
                for dm in done.iter().rev() {
                    match verified_move(&dm.to, &dm.from, Some(&dm.verified_sha)) {
                        Ok(_) => {
                            let _ = store::set_member_location(
                                conn,
                                session_id,
                                dm.file_id,
                                dm.reverse_location,
                                dm.reverse_holding.as_deref(),
                            );
                        }
                        Err(_) => rollback_failures += 1,
                    }
                }
                journal.record(JournalEvent::OperationFinished {
                    operation_id: operation_id.clone(),
                    status: "failed".into(),
                    succeeded: done.len().saturating_sub(rollback_failures),
                    failed: 1 + rollback_failures,
                });
                let path = PathBuf::from(&m.relative_path);
                return Err(TsError::ArrangementFailed {
                    path,
                    message: e.to_string(),
                });
            }
        }
    }

    journal.record(JournalEvent::OperationFinished {
        operation_id,
        status: "completed".into(),
        succeeded: done.len(),
        failed: 0,
    });
    Ok(())
}

fn member_by_file(
    conn: &Connection,
    session_id: i64,
    file_id: i64,
) -> Result<store::MemberRow, TsError> {
    store::members(conn, session_id)?
        .into_iter()
        .find(|m| m.file_id == file_id)
        .ok_or_else(|| TsError::Invariant(session_id, format!("member {file_id} vanished")))
}

/// Best-effort removal of the (now empty) per-session holding tree. Only
/// empty directories are ever removed; any leftover file stops the prune.
fn prune_session_dir(roots: &TroubleshootRoots, session_id: i64) {
    fn prune(dir: &Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    prune(&entry.path());
                }
            }
        }
        let _ = std::fs::remove_dir(dir);
    }
    prune(&roots.holding.path().join(format!("session-{session_id}")));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::hashing::sha256_file;
    use crate::ops::VecJournal;
    use std::collections::HashMap;
    use std::fs;

    struct Harness {
        _tmp: tempfile::TempDir,
        db: Database,
        mods: SafeRoot,
        holding: SafeRoot,
        quarantine: SafeRoot,
    }

    /// Field-level borrows so the roots and the connection can be held at
    /// the same time.
    macro_rules! roots {
        ($h:expr) => {
            TroubleshootRoots {
                mods: &$h.mods,
                holding: &$h.holding,
                quarantine: &$h.quarantine,
            }
        };
    }

    fn harness() -> Harness {
        let tmp = tempfile::tempdir().unwrap();
        for d in ["mods", "holding", "quarantine"] {
            fs::create_dir(tmp.path().join(d)).unwrap();
        }
        Harness {
            mods: SafeRoot::new(&tmp.path().join("mods")).unwrap(),
            holding: SafeRoot::new(&tmp.path().join("holding")).unwrap(),
            quarantine: SafeRoot::new(&tmp.path().join("quarantine")).unwrap(),
            db: Database::open_in_memory().unwrap(),
            _tmp: tmp,
        }
    }

    /// Create real files under Mods and enroll them in the files table.
    fn seed(h: &mut Harness, names: &[&str]) {
        for (i, name) in names.iter().enumerate() {
            let abs = h.mods.path().join(name);
            if let Some(parent) = abs.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&abs, format!("payload-{i}-{name}").repeat(3 + i)).unwrap();
            let sha = sha256_file(&abs).unwrap();
            let ft = if name.ends_with(".ts4script") {
                "ts4script"
            } else {
                "package"
            };
            h.db.conn()
                .execute(
                    "INSERT INTO files (current_filename, absolute_path, relative_path,
                        file_type, sha256, size_bytes, first_seen_at, last_seen_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, '2026-01-01T00:00:00Z',
                             '2026-01-01T00:00:00Z')",
                    rusqlite::params![
                        name.rsplit('/').next().unwrap(),
                        abs.to_string_lossy(),
                        name,
                        ft,
                        sha,
                        abs.metadata().unwrap().len() as i64
                    ],
                )
                .unwrap();
        }
    }

    fn location_of(h: &Harness, session: i64, rel: &str) -> String {
        h.db.conn()
            .query_row(
                "SELECT location FROM troubleshoot_members
                 WHERE session_id = ?1 AND relative_path = ?2",
                rusqlite::params![session, rel],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn original_hashes(h: &Harness, names: &[&str]) -> HashMap<String, String> {
        names
            .iter()
            .map(|n| {
                let p = h.mods.path().join(n);
                ((*n).to_string(), sha256_file(&p).unwrap())
            })
            .collect()
    }

    /// Play the user: report verdicts based on where the planted culprit
    /// physically is right now, until the session completes.
    fn drive(h: &mut Harness, session: i64, culprit: &str) -> SessionView {
        let mut journal = VecJournal::default();
        for _ in 0..40 {
            let view = store::session_view(h.db.conn(), session).unwrap();
            if view.status != "active" {
                return view;
            }
            let verdict = match view.phase.as_str() {
                "baseline" => Verdict::ProblemPresent,
                "testing" => {
                    if location_of(h, session, culprit) == "in" {
                        Verdict::ProblemPresent
                    } else {
                        Verdict::ProblemGone
                    }
                }
                "confirming" => {
                    let candidate = view.candidate.as_ref().unwrap();
                    if candidate.relative_path == culprit {
                        Verdict::ProblemGone
                    } else {
                        Verdict::ProblemPresent
                    }
                }
                other => panic!("unexpected phase {other}"),
            };
            let roots = TroubleshootRoots {
                mods: &h.mods,
                holding: &h.holding,
                quarantine: &h.quarantine,
            };
            submit_verdict(h.db.conn_mut(), &roots, session, verdict, &mut journal).unwrap();
        }
        panic!("session did not complete in 40 verdicts");
    }

    #[test]
    fn start_requires_a_nonempty_pool() {
        let mut h = harness();
        let roots = roots!(h);
        let err = start_session(h.db.conn_mut(), &roots, None).unwrap_err();
        assert!(matches!(err, TsError::EmptyPool));
    }

    #[test]
    fn enrollment_excludes_missing_quarantined_and_unsupported() {
        let mut h = harness();
        seed(&mut h, &["a.package", "b.package"]);
        h.db.conn()
            .execute_batch(
                "INSERT INTO files (current_filename, absolute_path, relative_path, file_type,
                    size_bytes, first_seen_at, last_seen_at, missing)
                 VALUES ('gone.package', '/x/gone.package', 'gone.package', 'package', 1,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 1);
                 INSERT INTO files (current_filename, absolute_path, relative_path, file_type,
                    size_bytes, first_seen_at, last_seen_at, status)
                 VALUES ('q.package', '/x/q.package', 'q.package', 'package', 1,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'quarantined');
                 INSERT INTO files (current_filename, absolute_path, relative_path, file_type,
                    size_bytes, first_seen_at, last_seen_at)
                 VALUES ('notes.txt', '/x/notes.txt', 'notes.txt', 'unsupported', 1,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
            )
            .unwrap();
        let roots = roots!(h);
        let view = start_session(h.db.conn_mut(), &roots, Some("test")).unwrap();
        assert_eq!(view.total, 2);
        assert_eq!(view.pool_size, 2);
        assert_eq!(view.phase, "baseline");
    }

    #[test]
    fn a_second_active_session_is_refused() {
        let mut h = harness();
        seed(&mut h, &["a.package"]);
        let roots = roots!(h);
        let first = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let roots = roots!(h);
        let err = start_session(h.db.conn_mut(), &roots, None).unwrap_err();
        match err {
            TsError::ActiveSessionExists(id) => assert_eq!(id, first.id),
            other => panic!("wrong error: {other}"),
        }
    }

    #[test]
    fn baseline_gone_completes_without_touching_anything() {
        let mut h = harness();
        seed(&mut h, &["a.package", "b.package"]);
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut journal = VecJournal::default();
        let roots = roots!(h);
        let view = submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemGone,
            &mut journal,
        )
        .unwrap();
        assert_eq!(view.status, "completed");
        assert_eq!(view.outcome.as_deref(), Some("no_problem"));
        assert!(
            journal.0.is_empty(),
            "no operations should have been journaled"
        );
        assert!(h.mods.path().join("a.package").exists());
        assert!(h.mods.path().join("b.package").exists());
    }

    #[test]
    fn round_one_splits_the_pool_on_disk_and_in_rows() {
        let mut h = harness();
        seed(
            &mut h,
            &["a.package", "b.package", "c.package", "d.package"],
        );
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut journal = VecJournal::default();
        let roots = roots!(h);
        let view = submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut journal,
        )
        .unwrap();
        assert_eq!(view.phase, "testing");
        assert_eq!(view.round, 1);
        assert_eq!(view.in_count, 2);
        assert_eq!(view.out_count, 2);
        // Sorted split: a, b stay in; c, d go out.
        assert!(h.mods.path().join("a.package").exists());
        assert!(h.mods.path().join("b.package").exists());
        assert!(!h.mods.path().join("c.package").exists());
        assert!(!h.mods.path().join("d.package").exists());
        assert!(h
            .holding
            .path()
            .join(format!("session-{}", s.id))
            .join("c.package")
            .exists());
        assert_eq!(location_of(&h, s.id, "d.package"), "out");
    }

    #[test]
    fn present_keeps_the_in_half_gone_keeps_the_out_half() {
        let mut h = harness();
        seed(
            &mut h,
            &["a.package", "b.package", "c.package", "d.package"],
        );
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut j = VecJournal::default();
        let roots = roots!(h);
        submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut j,
        )
        .unwrap();
        // Problem still present → culprit among {a, b}; c, d exonerated.
        let roots = roots!(h);
        let view = submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut j,
        )
        .unwrap();
        assert_eq!(view.pool_size, 2, "culprit narrowed to the in-half");
        assert_eq!(view.phase, "testing");
        assert_eq!(view.round, 2);
        // The remaining pool split again: a stays in, b goes out —
        // and the exonerated files stay out until the session ends.
        assert_eq!(location_of(&h, s.id, "a.package"), "in");
        assert_eq!(location_of(&h, s.id, "b.package"), "out");
        assert_eq!(location_of(&h, s.id, "c.package"), "out");
        assert_eq!(location_of(&h, s.id, "d.package"), "out");
        // Problem gone with b set aside → culprit is b; a pool of one means
        // confirmation: b stays out and every exonerated file comes home.
        let roots = roots!(h);
        let view =
            submit_verdict(h.db.conn_mut(), &roots, s.id, Verdict::ProblemGone, &mut j).unwrap();
        assert_eq!(view.phase, "confirming");
        assert_eq!(view.candidate.as_ref().unwrap().relative_path, "b.package");
        assert_eq!(location_of(&h, s.id, "a.package"), "in");
        assert_eq!(location_of(&h, s.id, "c.package"), "in");
        assert_eq!(location_of(&h, s.id, "d.package"), "in");
        assert!(h.mods.path().join("c.package").exists());
        assert!(!h.mods.path().join("b.package").exists());
    }

    #[test]
    fn convergence_finds_the_planted_culprit_in_eight() {
        let mut h = harness();
        let names: Vec<String> = (0..8).map(|i| format!("m{i:02}.package")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        seed(&mut h, &refs);
        let originals = original_hashes(&h, &refs);
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let done = drive(&mut h, s.id, "m05.package");
        assert_eq!(done.outcome.as_deref(), Some("culprit_confirmed"));
        assert_eq!(
            done.candidate.as_ref().unwrap().relative_path,
            "m05.package"
        );
        assert!(
            done.round <= 3,
            "8 files should converge in ≤3 rounds, took {}",
            done.round
        );
        // Culprit is quarantined: gone from Mods, present under quarantine,
        // recorded with the troubleshooter reason, file row flipped.
        assert!(!h.mods.path().join("m05.package").exists());
        let (reason, qpath): (String, String) =
            h.db.conn()
                .query_row(
                    "SELECT reason, quarantine_path FROM quarantine_entries
                 WHERE original_path = 'm05.package'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
        assert_eq!(reason, "Troubleshooter: confirmed culprit");
        assert!(std::path::Path::new(&qpath).exists());
        let status: String =
            h.db.conn()
                .query_row(
                    "SELECT status FROM files WHERE relative_path = 'm05.package'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(status, "quarantined");
        // Every innocent file is back, byte-identical.
        for name in refs.iter().filter(|n| **n != "m05.package") {
            let p = h.mods.path().join(name);
            assert!(p.exists(), "{name} should be restored");
            assert_eq!(&sha256_file(&p).unwrap(), originals.get(*name).unwrap());
        }
        // The per-session holding tree is gone.
        assert!(!h.holding.path().join(format!("session-{}", s.id)).exists());
    }

    #[test]
    fn convergence_handles_an_odd_pool() {
        let mut h = harness();
        let names: Vec<String> = (0..7).map(|i| format!("n{i}.package")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        seed(&mut h, &refs);
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let done = drive(&mut h, s.id, "n6.package");
        assert_eq!(done.outcome.as_deref(), Some("culprit_confirmed"));
        assert_eq!(done.candidate.as_ref().unwrap().relative_path, "n6.package");
    }

    #[test]
    fn nested_paths_survive_the_full_hunt() {
        let mut h = harness();
        seed(
            &mut h,
            &[
                "cc/hair/alpha.package",
                "cc/hair/beta.package",
                "scripts/mccc.ts4script",
                "top.package",
            ],
        );
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let done = drive(&mut h, s.id, "scripts/mccc.ts4script");
        assert_eq!(done.outcome.as_deref(), Some("culprit_confirmed"));
        assert!(h.mods.path().join("cc/hair/alpha.package").exists());
        assert!(!h.mods.path().join("scripts/mccc.ts4script").exists());
    }

    #[test]
    fn a_pool_of_one_goes_straight_to_confirmation() {
        let mut h = harness();
        seed(&mut h, &["only.package"]);
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut j = VecJournal::default();
        let roots = roots!(h);
        let view = submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut j,
        )
        .unwrap();
        assert_eq!(view.phase, "confirming");
        assert_eq!(view.round, 0);
        assert!(!h.mods.path().join("only.package").exists());
    }

    #[test]
    fn inconclusive_confirmation_restores_everything() {
        let mut h = harness();
        seed(&mut h, &["a.package", "b.package"]);
        let originals = original_hashes(&h, &["a.package", "b.package"]);
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut j = VecJournal::default();
        // Present → split (a in, b out); Present → pool {a} → confirm (a out, b in);
        // Present again → the problem isn't just `a` → inconclusive.
        for _ in 0..3 {
            let roots = roots!(h);
            submit_verdict(
                h.db.conn_mut(),
                &roots,
                s.id,
                Verdict::ProblemPresent,
                &mut j,
            )
            .unwrap();
        }
        let view = store::session_view(h.db.conn(), s.id).unwrap();
        assert_eq!(view.status, "completed");
        assert_eq!(view.outcome.as_deref(), Some("inconclusive"));
        for (name, hash) in &originals {
            let p = h.mods.path().join(name);
            assert!(p.exists(), "{name} should be home");
            assert_eq!(&sha256_file(&p).unwrap(), hash);
        }
        assert!(!h.holding.path().join(format!("session-{}", s.id)).exists());
    }

    #[test]
    fn abort_mid_hunt_restores_every_byte() {
        let mut h = harness();
        let names: Vec<String> = (0..6).map(|i| format!("f{i}.package")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        seed(&mut h, &refs);
        let originals = original_hashes(&h, &refs);
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut j = VecJournal::default();
        let roots = roots!(h);
        submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut j,
        )
        .unwrap();
        let roots = roots!(h);
        submit_verdict(h.db.conn_mut(), &roots, s.id, Verdict::ProblemGone, &mut j).unwrap();
        let roots = roots!(h);
        let view = abort_session(h.db.conn_mut(), &roots, s.id, &mut j).unwrap();
        assert_eq!(view.status, "aborted");
        assert_eq!(view.in_count, view.total);
        for (name, hash) in &originals {
            let p = h.mods.path().join(name);
            assert!(p.exists(), "{name} should be restored on abort");
            assert_eq!(&sha256_file(&p).unwrap(), hash);
        }
        assert!(!h.holding.path().join(format!("session-{}", s.id)).exists());
    }

    #[test]
    fn a_failed_arrangement_rolls_back_and_leaves_the_phase_untouched() {
        let mut h = harness();
        seed(
            &mut h,
            &["a.package", "b.package", "c.package", "d.package"],
        );
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        // Corrupt `d` after enrollment: its recorded hash is now stale, so
        // the planned move must refuse — after `c` has already moved.
        fs::write(h.mods.path().join("d.package"), "tampered contents").unwrap();
        let mut j = VecJournal::default();
        let roots = roots!(h);
        let err = submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut j,
        )
        .unwrap_err();
        assert!(
            matches!(err, TsError::ArrangementFailed { .. }),
            "got {err}"
        );
        let view = store::session_view(h.db.conn(), s.id).unwrap();
        assert_eq!(view.phase, "baseline", "session must rest where it was");
        assert_eq!(view.round, 0);
        assert_eq!(view.in_count, view.total, "rollback returns every file");
        for name in ["a.package", "b.package", "c.package", "d.package"] {
            assert!(
                h.mods.path().join(name).exists(),
                "{name} should be in Mods"
            );
        }
    }

    #[test]
    fn reconcile_heals_rows_toward_disk_truth() {
        let mut h = harness();
        seed(
            &mut h,
            &["a.package", "b.package", "c.package", "d.package"],
        );
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut j = VecJournal::default();
        let roots = roots!(h);
        submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut j,
        )
        .unwrap();
        // Simulate a crash that moved `c` home without the row update…
        let c_holding = h
            .holding
            .path()
            .join(format!("session-{}", s.id))
            .join("c.package");
        fs::rename(&c_holding, h.mods.path().join("c.package")).unwrap();
        // …and one that lost `d` entirely.
        fs::remove_file(
            h.holding
                .path()
                .join(format!("session-{}", s.id))
                .join("d.package"),
        )
        .unwrap();
        let roots = roots!(h);
        let report = reconcile(h.db.conn_mut(), &roots, s.id).unwrap();
        assert_eq!(report.healed, 1);
        assert_eq!(report.missing, vec!["d.package".to_string()]);
        assert!(report.conflicts.is_empty());
        assert_eq!(location_of(&h, s.id, "c.package"), "in");
    }

    #[test]
    fn reconcile_reports_a_file_present_in_both_places() {
        let mut h = harness();
        seed(&mut h, &["a.package", "b.package"]);
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut j = VecJournal::default();
        let roots = roots!(h);
        submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut j,
        )
        .unwrap();
        // `b` is out; plant a stray copy back at its Mods path.
        fs::write(h.mods.path().join("b.package"), "stray copy").unwrap();
        let roots = roots!(h);
        let report = reconcile(h.db.conn_mut(), &roots, s.id).unwrap();
        assert_eq!(report.conflicts, vec!["b.package".to_string()]);
        assert_eq!(report.healed, 0);
    }

    #[test]
    fn verdicts_are_refused_once_a_session_is_closed() {
        let mut h = harness();
        seed(&mut h, &["a.package"]);
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut j = VecJournal::default();
        let roots = roots!(h);
        submit_verdict(h.db.conn_mut(), &roots, s.id, Verdict::ProblemGone, &mut j).unwrap();
        let roots = roots!(h);
        let err = submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut j,
        )
        .unwrap_err();
        assert!(matches!(err, TsError::NotActive(_)));
        let roots = roots!(h);
        let err = abort_session(h.db.conn_mut(), &roots, s.id, &mut j).unwrap_err();
        assert!(matches!(err, TsError::NotActive(_)));
    }

    #[test]
    fn a_new_session_can_start_after_one_completes() {
        let mut h = harness();
        seed(&mut h, &["a.package", "b.package"]);
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut j = VecJournal::default();
        let roots = roots!(h);
        submit_verdict(h.db.conn_mut(), &roots, s.id, Verdict::ProblemGone, &mut j).unwrap();
        let roots = roots!(h);
        let second = start_session(h.db.conn_mut(), &roots, Some("round two")).unwrap();
        assert_ne!(second.id, s.id);
        assert_eq!(second.status, "active");
        assert_eq!(active_session(h.db.conn()).unwrap().unwrap().id, second.id);
    }

    #[test]
    fn journal_records_one_operation_per_arrangement() {
        let mut h = harness();
        seed(
            &mut h,
            &["a.package", "b.package", "c.package", "d.package"],
        );
        let roots = roots!(h);
        let s = start_session(h.db.conn_mut(), &roots, None).unwrap();
        let mut j = VecJournal::default();
        let roots = roots!(h);
        submit_verdict(
            h.db.conn_mut(),
            &roots,
            s.id,
            Verdict::ProblemPresent,
            &mut j,
        )
        .unwrap();
        let starts: Vec<&JournalEvent> =
            j.0.iter()
                .filter(|e| matches!(e, JournalEvent::OperationStarted { .. }))
                .collect();
        assert_eq!(starts.len(), 1);
        match starts[0] {
            JournalEvent::OperationStarted {
                kind, total_steps, ..
            } => {
                assert_eq!(kind, "troubleshoot_round");
                assert_eq!(*total_steps, 2);
            }
            _ => unreachable!(),
        }
        let finished = j
            .0
            .iter()
            .any(|e| matches!(e, JournalEvent::OperationFinished { status, .. } if status == "completed"));
        assert!(finished);
    }
}
