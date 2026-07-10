-- 0002_seed_categories
-- System categories from spec §10.7. Users can add their own; system rows are
-- marked so the UI can protect them from accidental deletion.

INSERT INTO categories (name, sort_order, system_category) VALUES
    ('Gameplay Mod',          10, 1),
    ('Script Mod',            20, 1),
    ('CAS',                   30, 1),
    ('Build/Buy',             50, 1),
    ('Overrides',             70, 1),
    ('Default Replacements',  80, 1),
    ('Traits',                90, 1),
    ('Careers',              100, 1),
    ('Aspirations',          110, 1),
    ('Poses',                120, 1),
    ('Animations',           130, 1),
    ('Core Libraries',       140, 1),
    ('Unsorted',             900, 1),
    ('Unidentified',         910, 1);

-- CAS children
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Hair',         id, 31, 1 FROM categories WHERE name = 'CAS';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Clothing',     id, 32, 1 FROM categories WHERE name = 'CAS';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Shoes',        id, 33, 1 FROM categories WHERE name = 'CAS';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Accessories',  id, 34, 1 FROM categories WHERE name = 'CAS';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Makeup',       id, 35, 1 FROM categories WHERE name = 'CAS';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Skin Details', id, 36, 1 FROM categories WHERE name = 'CAS';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Presets',      id, 37, 1 FROM categories WHERE name = 'CAS';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Sliders',      id, 38, 1 FROM categories WHERE name = 'CAS';

-- Build/Buy children
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Furniture',    id, 51, 1 FROM categories WHERE name = 'Build/Buy';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Decorations',  id, 52, 1 FROM categories WHERE name = 'Build/Buy';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Lighting',     id, 53, 1 FROM categories WHERE name = 'Build/Buy';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Appliances',   id, 54, 1 FROM categories WHERE name = 'Build/Buy';
INSERT INTO categories (name, parent_id, sort_order, system_category)
SELECT 'Construction', id, 55, 1 FROM categories WHERE name = 'Build/Buy';
