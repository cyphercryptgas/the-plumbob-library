-- 0003_package_resources
--
-- Phase 2: package awareness. Stores the resource index (type/group/instance
-- keys) read from each .package file, plus per-file parse bookkeeping on
-- `files`. Additive only — existing databases upgrade in place, no reset.
--
-- Parse staleness is content-keyed: `parsed_sha256` records which content
-- fingerprint the stored index belongs to. A file re-parses only when its
-- sha256 changes, which also means a permanently corrupt file is retried
-- only if its bytes change — not on every scan.

ALTER TABLE files ADD COLUMN parsed_sha256 TEXT;
ALTER TABLE files ADD COLUMN parse_status TEXT;
ALTER TABLE files ADD COLUMN parse_error TEXT;

CREATE TABLE package_resources (
    id        INTEGER PRIMARY KEY,
    file_id   INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    -- Resource key. instance is a u64 stored as a bit-cast signed INTEGER;
    -- equality (all grouping) is unaffected, and display always goes through
    -- the hex TGI form.
    type_id   INTEGER NOT NULL,
    group_id  INTEGER NOT NULL,
    instance  INTEGER NOT NULL
);

CREATE INDEX idx_pkgres_file ON package_resources(file_id);
CREATE INDEX idx_pkgres_tgi  ON package_resources(type_id, group_id, instance);
