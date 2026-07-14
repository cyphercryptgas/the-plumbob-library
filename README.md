# Motherlode Manager

A safety-first Sims 4 mod manager for Windows — complete at **1.0**. It
inventories your Mods folder, understands what's inside every package,
and never destroys anything: every risky action is backed up and
journaled before it runs.

## What it does

**Library.** Every file, scanned and reconciled: search, categories
(CAS, Build/Buy, Poses, Gameplay, Scripts), CAS subcategories read by a
reference-exact CASP parser (Hair, Tops, Shoes, …), sort by name or file
date, list or thumbnail grid with modified-date tags on every tile.
Enable/disable without moving files.

**Thumbnails.** Extracted straight from the packages: PNG, JPEG, DDS
(DXT1/3/5, BC1/BC3, uncompressed), and EA's shuffled DST — decoded per
the s4pi reference. Census-driven: a Diagnose card names exactly what
the imageless packages contain.

**Creators.** Bylines read from filename conventions — bracketed leads
always credit, underscore prefixes credit on a creator signature or
frequency promotion. A roster with per-creator galleries, sortable by
volume or A–Z and expandable in full; gold creator pills on every
credited file click through to the creator's page.

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
where it doesn't — matches are creator-aware, and a Re-verify pass can
re-judge the whole cache whenever standards improve. The stat tiles are
buttons: Updates, Up to date, On CurseForge, and Not on CurseForge each
open their own list, every row wearing its thumbnail and a LIB tag that
jumps to the file in the Library. One-click Update downloads the latest
release — bare files and zip archives alike (the archive is unpacked in
memory and the right file chosen or honestly declined) — backs up your
copy, swaps it in place, re-indexes the new contents, and warns if the
new version overlaps a sibling. Authors who disable third-party
downloads are pre-flagged; a Ready-first toggle floats actionable rows.

**Merge mode.** Merging is a switch, not a transform — for when you
want load speed and accept that merged files trade away per-file
superpowers. One click plans creator-first groups of CAS files —
only CAS merges; every other type stays loose by design — takes a
single whole-session backup, merges everything under one journaled
operation, and lights the tile green. Click again to un-merge: outputs
removed, every original restored in one pass. The app writes DBPF
byte-exact to the format it reads; load order decides collisions;
undecodable packages stay loose and are named. Manual Library-selection
merges remain for small, deliberate combinations.

**Title Tool.** Name files to convention — `[creator]_[modtype]_[modname]`
— extracted from the files themselves: attribution supplies the creator,
the CAS subcategory (or category) the type, and the CurseForge match or
a scrubbed filename the name. Select files in the Library and Title
them with a preview, or let the Dashboard quick action title everything
that arrived today. Renames go through the database row, so match
history and thumbnails survive, and every batch is journaled.

**Dashboard.** Live counts and four working quick actions: Title tool,
Merge packages (the planner above), Prepare thumbnails, and Update
radar — each reporting its receipt inline.

**Quarantine, Backups, Activity.** The safety spine: nothing is ever
deleted, everything is journaled, everything can come back. Backups
self-heal from disk (any snapshot folder's manifest is imported on
sight) and both history pages show each file's thumbnail beside a
readable name.

## Principles

Precision over recall; evidence over folklore (parsers come from
reference sources, decoders from the census); wrong data gets wiped and
recomputed rather than patched; every destructive path goes through the
backup journal first.

## Design boundaries

Stated plainly, because a finished app should name its edges: entries in
compression schemes the pipeline doesn't decode (EA's RefPack family)
keep their packages out of merges — those files work in-game and stay
loose, named in every receipt. Merged outputs store entries
uncompressed: larger on disk, faster to load, always faithful.
CurseForge's Sims 4 index can't match its own fingerprints, so name
matching carries the radar — attributed, confidence-scored, and
re-verifiable. A few hundred packages contain no art at all (tuning and
string tables); they are imageless by nature, not by omission.

## Building

Tauri 2 + Rust core + React/TypeScript. `npm install`, then
`npm run tauri build`. CI runs the core test suite; the Windows
Installer workflow produces the installer on every push.
