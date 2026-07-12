-- 0006: profile mod-sets.
--
-- Each profile remembers the set of files it keeps DISABLED (sparse — most
-- of a library stays on). The ACTIVE profile's set live-tracks reality:
-- every toggle and every scan-synced rename writes through to it. Inactive
-- profiles hold their sets frozen until switched to.

CREATE TABLE profile_disabled (
    profile_id  INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    file_id     INTEGER NOT NULL REFERENCES files(id)    ON DELETE CASCADE,
    PRIMARY KEY (profile_id, file_id)
);

CREATE INDEX idx_profile_disabled_file ON profile_disabled(file_id);
