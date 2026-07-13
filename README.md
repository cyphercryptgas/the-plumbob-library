# Motherlode Manager

A safety-first Sims 4 mod manager for Windows. It inventories your Mods
folder, understands what's inside every package, and never destroys
anything — every risky action is backed up and journaled before it runs.

## What it does

**Library.** Every file, scanned and reconciled: search, categories
(CAS, Build/Buy, Poses, Gameplay, Scripts), CAS subcategories read by a
reference-exact CASP parser (Hair, Tops, Shoes, …), sort by name or file
date, list or thumbnail grid. Enable/disable without moving files.

**Thumbnails.** Extracted straight from the packages: PNG, JPEG, DDS
(DXT1/3/5, BC1/BC3, uncompressed), and EA's shuffled DST — decoded per
the s4pi reference. Census-driven: a Diagnose card names exactly what
the imageless packages contain, so decoding grows on evidence.

**Creators.** Bylines read from filename conventions — bracketed leads
always credit, underscore prefixes credit on a creator signature or
frequency promotion. A roster screen with per-creator galleries; gold
creator pills on every credited file.

**Duplicate Center.** Exact duplicates by content hash, with safe
one-click cleanup (quarantine, never delete).

**Conflicts.** Packages fighting over the same resources, ranked by
severity, with the load-order winner named.

**Troubleshoot.** Guided binary-search over your whole Mods folder to
find the file breaking your game — the classic 50/50 method, automated
and journaled so every step is reversible.

**Profiles.** Named snapshots of which mods are enabled; switch loadouts
in one click.

**Patch Center.** Your library checked against CurseForge. Exact
fingerprints where the index supports them; an attributed name radar
where it doesn't (CurseForge's Sims 4 index can't match its own
fingerprints). Matches are creator-aware — author confirmation boosts,
author mismatch demands a distinctive name — and a Re-verify pass can
re-judge the whole cache whenever standards improve. One-click Update
downloads the latest release, backs up your copy, and swaps it in
(single-file releases; archives fall back to Open Mod).

**Merge.** Combine selected packages into one DBPF — game load order
preserved on collisions, originals journaled to Backups first, fully
reversible.

**Quarantine, Backups, Activity.** The safety spine: nothing is ever
deleted, everything is journaled, everything can come back.

## Principles

Precision over recall; evidence over folklore (parsers come from
reference sources, decoders from the census); wrong data gets wiped and
recomputed rather than patched; every destructive path goes through the
backup journal first.

## Building

Tauri 2 + Rust core + React/TypeScript. `npm install`, then
`npm run tauri build`. CI runs the core test suite; the Windows
Installer workflow produces the installer on every push.
