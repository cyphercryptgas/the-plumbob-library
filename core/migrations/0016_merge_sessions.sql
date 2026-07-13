-- 0016: Merge mode. One auto-merge run = one session: one whole-run
-- backup, its output files remembered, and an active flag the dashboard
-- lights green. Un-merge reverses the active session in one journaled op.
CREATE TABLE merge_sessions (
    id          INTEGER PRIMARY KEY,
    created_at  TEXT NOT NULL,
    backup_id   INTEGER NOT NULL REFERENCES backups(id),
    files       INTEGER NOT NULL,
    groups_n    INTEGER NOT NULL,
    active      INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE merge_session_outputs (
    session_id    INTEGER NOT NULL REFERENCES merge_sessions(id) ON DELETE CASCADE,
    absolute_path TEXT NOT NULL
);
