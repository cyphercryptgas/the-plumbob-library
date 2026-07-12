-- 0005: profiles.
--
-- A profile names the person (or the setup) holding the save. Today that
-- powers the welcome greeting and gives the record a home; the enable/
-- disable mod sets that will hang off each profile arrive in a later
-- migration, built on files.enabled.

CREATE TABLE profiles (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    is_active   INTEGER NOT NULL DEFAULT 0
);

-- Names are unique the way Windows thinks about names.
CREATE UNIQUE INDEX idx_profiles_name ON profiles(name COLLATE NOCASE);

-- The database itself enforces "at most one active profile".
CREATE UNIQUE INDEX idx_profiles_single_active
    ON profiles(is_active) WHERE is_active = 1;
