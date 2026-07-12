-- 0009: the name radar (tier-2 CurseForge matching).
--
-- The field-proven verdict: CurseForge's exact-match fingerprint index
-- does not cover The Sims 4 — their own computed fingerprints fail to
-- match themselves. Tier-2 matches by *name*: a cleaned search term per
-- file, searched once per unique term, cached (including negatives) so
-- checks are resumable and cheap. Matches carry their kind and a
-- confidence score, and are always labeled as approximate.

DROP TABLE curse_matches;

CREATE TABLE curse_matches (
    file_id            INTEGER PRIMARY KEY
                       REFERENCES files(id) ON DELETE CASCADE,
    curse_mod_id       INTEGER NOT NULL,
    -- NULL for name matches: no specific CurseForge file corresponds.
    curse_file_id      INTEGER,
    mod_name           TEXT NOT NULL,
    website_url        TEXT,
    matched_file_name  TEXT,
    matched_file_date  TEXT,
    latest_file_id     INTEGER NOT NULL,
    latest_file_name   TEXT NOT NULL,
    latest_file_date   TEXT NOT NULL,
    update_available   INTEGER NOT NULL DEFAULT 0,
    -- 'fingerprint' | 'name'
    match_kind         TEXT NOT NULL DEFAULT 'fingerprint',
    confidence         REAL,
    checked_at         TEXT NOT NULL
);

CREATE INDEX idx_curse_matches_update ON curse_matches(update_available);

-- One row per search term ever tried; curse_mod_id NULL is a cached
-- "searched, nothing confident" so re-checks skip it.
CREATE TABLE curse_name_lookups (
    term          TEXT PRIMARY KEY,
    curse_mod_id  INTEGER,
    mod_name      TEXT,
    confidence    REAL,
    checked_at    TEXT NOT NULL
);
