-- 0014: creators, read from filename conventions (bracketed leads and
-- underscore prefixes). NULL = not yet attempted; '' = examined and
-- uncredited; otherwise the canonical (lowercase) creator key.
ALTER TABLE files ADD COLUMN creator TEXT;
ALTER TABLE files ADD COLUMN creator_display TEXT;
CREATE INDEX idx_files_creator ON files(creator);
