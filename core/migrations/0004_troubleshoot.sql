-- 0004: the 50/50 troubleshooting assistant.
--
-- A session is a persistent binary search over the library: which single
-- file causes the problem the user is seeing in-game? The session and its
-- members are the durable truth about which files are where — the engine
-- updates member rows as each verified move completes, so a crash at any
-- moment leaves a state the startup reconciler can heal from disk.
--
-- Design decisions recorded here so the schema reads honestly:
--   * Exonerated halves stay set aside until the session ends (fewer moves,
--     cleaner tests); everything returns in one verified restore.
--   * No pre-session full backup: files are moved, not copied, with hash
--     verification on every move — the holding copies ARE the originals.
--   * A confirmed culprit is handed to the existing quarantine system.

CREATE TABLE troubleshoot_sessions (
    id               INTEGER PRIMARY KEY,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    -- active | completed | aborted
    status           TEXT NOT NULL DEFAULT 'active',
    -- Resting phases only (never a mid-move state):
    --   baseline   awaiting "does the problem happen with everything in?"
    --   testing    a round is arranged; awaiting the user's verdict
    --   confirming only the candidate is out; awaiting the final verdict
    phase            TEXT NOT NULL DEFAULT 'baseline',
    round            INTEGER NOT NULL DEFAULT 0,
    problem_note     TEXT,
    -- no_problem | culprit_confirmed | inconclusive | aborted
    outcome          TEXT,
    culprit_file_id  INTEGER REFERENCES files(id) ON DELETE SET NULL
);

CREATE INDEX idx_ts_sessions_status ON troubleshoot_sessions(status);

CREATE TABLE troubleshoot_members (
    session_id        INTEGER NOT NULL
                      REFERENCES troubleshoot_sessions(id) ON DELETE CASCADE,
    file_id           INTEGER NOT NULL
                      REFERENCES files(id) ON DELETE CASCADE,
    -- Snapshot of the file's identity at enrollment; restores use these even
    -- if the live files table changes underneath the session.
    relative_path     TEXT NOT NULL,
    sha256            TEXT,
    -- in  = at its original place under the Mods root
    -- out = set aside under the holding root
    location          TEXT NOT NULL DEFAULT 'in',
    -- Path relative to the holding root while set aside (records the exact
    -- collision-free destination the move landed on).
    holding_relative  TEXT,
    -- Still in the suspect pool? Exonerated members keep their location
    -- until the final restore.
    in_pool           INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (session_id, file_id)
);

CREATE INDEX idx_ts_members_pool ON troubleshoot_members(session_id, in_pool);
