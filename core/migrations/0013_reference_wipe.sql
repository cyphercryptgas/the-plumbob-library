-- 0013: elections are over — BodyType has no fixed offset (a variable
-- flag list precedes it). The reference parser reads the field chain
-- sequentially. Wipe the last election's output; scan reclassifies.
UPDATE files SET cas_subcategory = NULL;
