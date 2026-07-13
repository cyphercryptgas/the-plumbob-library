-- 0015: CurseForge mods carry an allowModDistribution flag. Storing it
-- lets closed-door authors show a pre-disabled Update instead of a
-- click-then-error. NULL = unknown until the next Check populates it.
ALTER TABLE curse_matches ADD COLUMN allow_distribution INTEGER;
