-- 0012: election v2 crowned an impostor column (small, varied, in-range —
-- everything BodyType is, except shared across a part's swatches). Wipe
-- again; election v3 requires sibling agreement.
UPDATE files SET cas_subcategory = NULL;
