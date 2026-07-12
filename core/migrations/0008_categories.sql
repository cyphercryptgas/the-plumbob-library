-- 0008: in-game category classification.
--
-- Each file gains a category derived from its resource census — what the
-- mod *is* in game terms, using the same researched type constants the
-- conflicts policy stands on (see core/src/dbpf.rs::type_name). Scripts
-- classify by extension; packages by which resource families they carry.

ALTER TABLE files ADD COLUMN category TEXT;

-- The classifier's EXISTS probes need this shape.
CREATE INDEX idx_pr_file_type ON package_resources(file_id, type_id);
