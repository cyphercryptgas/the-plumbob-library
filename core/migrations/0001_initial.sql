-- 0001_initial
-- Full Phase-One schema (spec §10.4). Conventions:
--   * timestamps: TEXT, RFC 3339 UTC
--   * booleans:   INTEGER 0/1
--   * paths:      TEXT; relative paths stored with '/' separators
--   * name/path uniqueness uses NOCASE to match Windows filesystem semantics

CREATE TABLE creators (
    id               INTEGER PRIMARY KEY,
    name             TEXT NOT NULL COLLATE NOCASE,
    website_url      TEXT,
    patreon_url      TEXT,
    curseforge_url   TEXT,
    tumblr_url       TEXT,
    other_update_url TEXT,
    notes            TEXT
);
CREATE UNIQUE INDEX idx_creators_name ON creators(name);

CREATE TABLE categories (
    id              INTEGER PRIMARY KEY,
    name            TEXT NOT NULL COLLATE NOCASE,
    parent_id       INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    system_category INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_categories_name ON categories(name);

CREATE TABLE mods (
    id                          INTEGER PRIMARY KEY,
    name                        TEXT NOT NULL,
    creator_id                  INTEGER REFERENCES creators(id) ON DELETE SET NULL,
    description                 TEXT,
    category_id                 INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    source_provider             TEXT,
    source_url                  TEXT,
    source_project_id           TEXT,
    installed_version           TEXT,
    latest_version              TEXT,
    game_version_compatibility  TEXT,
    installation_date           TEXT,
    last_checked_date           TEXT,
    status                      TEXT NOT NULL DEFAULT 'unidentified',
    update_method               TEXT,
    notes                       TEXT,
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);
CREATE INDEX idx_mods_creator ON mods(creator_id);
CREATE INDEX idx_mods_category ON mods(category_id);

CREATE TABLE scans (
    id           INTEGER PRIMARY KEY,
    started_at   TEXT NOT NULL,
    completed_at TEXT,
    scan_type    TEXT NOT NULL,
    files_seen   INTEGER NOT NULL DEFAULT 0,
    bytes_seen   INTEGER NOT NULL DEFAULT 0,
    errors       INTEGER NOT NULL DEFAULT 0,
    cancelled    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE scan_errors (
    id         INTEGER PRIMARY KEY,
    scan_id    INTEGER NOT NULL REFERENCES scans(id) ON DELETE CASCADE,
    path       TEXT NOT NULL,
    error_code TEXT,
    message    TEXT NOT NULL
);
CREATE INDEX idx_scan_errors_scan ON scan_errors(scan_id);

CREATE TABLE files (
    id                INTEGER PRIMARY KEY,
    mod_id            INTEGER REFERENCES mods(id) ON DELETE SET NULL,
    current_filename  TEXT NOT NULL,
    original_filename TEXT,
    absolute_path     TEXT NOT NULL,
    relative_path     TEXT NOT NULL COLLATE NOCASE,
    extension         TEXT,
    file_type         TEXT NOT NULL,
    sha256            TEXT,
    size_bytes        INTEGER NOT NULL DEFAULT 0,
    created_at_fs     TEXT,
    modified_at_fs    TEXT,
    first_seen_at     TEXT NOT NULL,
    last_seen_at      TEXT NOT NULL,
    last_scan_id      INTEGER REFERENCES scans(id) ON DELETE SET NULL,
    resource_count    INTEGER,
    enabled           INTEGER NOT NULL DEFAULT 1,
    missing           INTEGER NOT NULL DEFAULT 0,
    depth             INTEGER NOT NULL DEFAULT 0,
    zero_byte         INTEGER NOT NULL DEFAULT 0,
    deep_script       INTEGER NOT NULL DEFAULT 0,
    status            TEXT NOT NULL DEFAULT 'current',
    -- Pre-grouping assignments (spec §10.2 collects these per file until a
    -- mod association exists; mod-level values live on mods).
    category_id       INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    creator_id        INTEGER REFERENCES creators(id) ON DELETE SET NULL
);
CREATE UNIQUE INDEX idx_files_relative_path ON files(relative_path);
CREATE INDEX idx_files_sha256 ON files(sha256);
CREATE INDEX idx_files_size ON files(size_bytes);
CREATE INDEX idx_files_missing ON files(missing);
CREATE INDEX idx_files_type ON files(file_type);
CREATE INDEX idx_files_mod ON files(mod_id);
CREATE INDEX idx_files_last_scan ON files(last_scan_id);
CREATE INDEX idx_files_status ON files(status);

CREATE TABLE tags (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL COLLATE NOCASE,
    color_token TEXT
);
CREATE UNIQUE INDEX idx_tags_name ON tags(name);

CREATE TABLE mod_tags (
    mod_id INTEGER NOT NULL REFERENCES mods(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (mod_id, tag_id)
);

CREATE TABLE collections (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  TEXT NOT NULL
);

CREATE TABLE collection_mods (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    mod_id        INTEGER NOT NULL REFERENCES mods(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, mod_id)
);

CREATE TABLE duplicate_groups (
    id                    INTEGER PRIMARY KEY,
    duplicate_type        TEXT NOT NULL DEFAULT 'exact',
    confidence            REAL NOT NULL DEFAULT 1.0,
    status                TEXT NOT NULL DEFAULT 'open',
    created_at            TEXT NOT NULL,
    sha256                TEXT,
    size_bytes            INTEGER,
    recommended_file_id   INTEGER REFERENCES files(id) ON DELETE SET NULL,
    recommendation_reason TEXT,
    reclaimable_bytes     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_duplicate_groups_status ON duplicate_groups(status);

CREATE TABLE duplicate_group_files (
    group_id INTEGER NOT NULL REFERENCES duplicate_groups(id) ON DELETE CASCADE,
    file_id  INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, file_id)
);
CREATE INDEX idx_duplicate_group_files_file ON duplicate_group_files(file_id);

CREATE TABLE operations (
    id             INTEGER PRIMARY KEY,
    operation_uid  TEXT NOT NULL,
    operation_type TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'running',
    created_at     TEXT NOT NULL,
    started_at     TEXT,
    completed_at   TEXT,
    backup_id      INTEGER REFERENCES backups(id) ON DELETE SET NULL,
    summary        TEXT,
    error_message  TEXT
);
CREATE UNIQUE INDEX idx_operations_uid ON operations(operation_uid);

CREATE TABLE operation_steps (
    id               INTEGER PRIMARY KEY,
    operation_id     INTEGER NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    step_order       INTEGER NOT NULL,
    action           TEXT NOT NULL,
    source_path      TEXT NOT NULL,
    destination_path TEXT,
    expected_hash    TEXT,
    status           TEXT NOT NULL,
    error_message    TEXT
);
CREATE INDEX idx_operation_steps_operation ON operation_steps(operation_id);

CREATE TABLE backups (
    id           INTEGER PRIMARY KEY,
    created_at   TEXT NOT NULL,
    reason       TEXT NOT NULL,
    root_path    TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'available',
    total_files  INTEGER NOT NULL DEFAULT 0,
    total_bytes  INTEGER NOT NULL DEFAULT 0,
    operation_id INTEGER REFERENCES operations(id) ON DELETE SET NULL
);

CREATE TABLE backup_entries (
    id          INTEGER PRIMARY KEY,
    backup_id   INTEGER NOT NULL REFERENCES backups(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    backup_path TEXT NOT NULL,
    sha256      TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_backup_entries_backup ON backup_entries(backup_id);

CREATE TABLE quarantine_entries (
    id              INTEGER PRIMARY KEY,
    file_id         INTEGER REFERENCES files(id) ON DELETE SET NULL,
    original_path   TEXT NOT NULL,
    quarantine_path TEXT NOT NULL,
    sha256          TEXT,
    reason          TEXT NOT NULL,
    quarantined_at  TEXT NOT NULL,
    restored_at     TEXT,
    status          TEXT NOT NULL DEFAULT 'quarantined',
    operation_id    INTEGER REFERENCES operations(id) ON DELETE SET NULL
);
CREATE INDEX idx_quarantine_status ON quarantine_entries(status);

CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
