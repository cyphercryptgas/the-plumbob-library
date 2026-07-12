-- 0007: CurseForge update radar.
--
-- Files gain a CurseForge fingerprint (MurmurHash2, seed 1, whitespace
-- stripped — their matching scheme, byte for byte). Matches from the last
-- check are cached locally so the Patch Center renders instantly; only
-- anonymous fingerprints ever leave the machine.

ALTER TABLE files ADD COLUMN curse_fingerprint INTEGER;

CREATE TABLE curse_matches (
    file_id            INTEGER PRIMARY KEY
                       REFERENCES files(id) ON DELETE CASCADE,
    curse_mod_id       INTEGER NOT NULL,
    curse_file_id      INTEGER NOT NULL,
    mod_name           TEXT NOT NULL,
    website_url        TEXT,
    matched_file_name  TEXT NOT NULL,
    matched_file_date  TEXT NOT NULL,
    latest_file_id     INTEGER NOT NULL,
    latest_file_name   TEXT NOT NULL,
    latest_file_date   TEXT NOT NULL,
    update_available   INTEGER NOT NULL DEFAULT 0,
    checked_at         TEXT NOT NULL
);

CREATE INDEX idx_curse_matches_update ON curse_matches(update_available);
