-- 0011: the first CASP field sequence was wrong — every part landed in
-- "other". Clear the lot; the calibrating reader repopulates on scan.
UPDATE files SET cas_subcategory = NULL;
