-- 0010: CAS subcategories, parsed from each CAS part's own BodyType field
-- inside its CASP resource — never guessed from filenames.
ALTER TABLE files ADD COLUMN cas_subcategory TEXT;
