# Motherlode Manager

*Formerly The Plumbob Library — renamed in v0.2.0. All data, internals, and
the on-disk identity (`com.moetech.plumbob`) are unchanged; existing
libraries carry over untouched.*

**Your mods. Organized. Precious.** — a safety-first manager for Sims 4
mods and custom content.

A Windows-first desktop application (Tauri 2 + Rust + React/TypeScript +
SQLite) for scanning, inventorying, organizing, quarantining,
troubleshooting, and updating large Sims 4 Mods folders — built
safety-first: every mutation is planned, previewed, hash-verified,
journaled, and reversible.

*Motherlode Manager is an independent community tool and is not affiliated
with or endorsed by Electronic Arts, Maxis, The Sims, Overwolf, CurseForge,
or individual mod creators.*

## Current status — v0.7.1: every planned surface shipped

Honest state, matching `docs/FEATURE_STATUS.md` and `CHANGELOG.md`. The
sidebar's PLANNED section renders nothing for the first time. **160 core
tests on Linux + Windows CI**; `release.yml` builds an NSIS Windows
installer on every push to `main`, validated in daily use on a real
4,200-file, 19 GB library.

**The safety core** (Phases 1–2): containment-gated scanning with content
fingerprints, a read-only DBPF package index with a content-keyed
incremental parse pass, duplicate detection, a Conflicts screen
implementing a researched noise policy, verified quarantine and restore,
all-or-nothing snapshots, the operations journal, migrations, and startup
reconciliation.

**The flagship features** (Phases 3–5), each with its own design document:

* **50/50 Troubleshooting Assistant** (`docs/TROUBLESHOOTER.md`) — a
  persistent, resumable binary search for the file breaking the game.
  Hash-verified arrangements with live progress, abort restores every
  byte, confirmed culprits are handed to Quarantine. Field-validated on a
  4,213-file library: thirteen rounds, culprit confirmed, quarantined, and
  restored.
* **Profiles** (`docs/PROFILES.md`) — named setups with live-tracked
  disabled sets, built on a verified in-place enable/disable engine
  (`X.package ⇄ X.package.off`) that the scanner fully understands,
  including renames done by hand in Explorer. Switching previews the exact
  diff, then arranges the library to match — journaled, progress-barred,
  activating only on a clean apply.
* **Patch Center: Update Radar** (`docs/PATCH_CENTER.md`) — the library
  checked against CurseForge itself using their fingerprint scheme
  (MurmurHash2, seed 1, whitespace-stripped; proven against independently
  computed vectors). Only anonymous fingerprints ever leave the machine;
  the API key lives in the local database and rides a request header.

**The look**: the full gilded-emerald art direction — engraved cartouche
frames, corner scrolls, spliced finials, paper grain, the lit sidebar with
its starfield and constellations — render-verified against the approved
preview before every ship (`docs/DESIGN_SPEC.md`).

Nothing in this repository fakes functionality. Unfinished surfaces say
so — and right now, none need to.

## Commands

```
# Safety core + feature engines (Rust ≥ 1.75; bundled SQLite compiles via cc)
cargo test --manifest-path core/Cargo.toml

# Frontend (Node ≥ 20)
npm install
npm run typecheck
npm run build
npm run dev        # UI only — use `npm run tauri dev` for a working backend
```

## Repository layout

```
core/        Rust safety core + feature engines — standalone Cargo root, no
             Tauri dependency; 160 tests on conservative toolchains and
             Windows CI runners
src/         React + TypeScript frontend (Vite, Tailwind design tokens) —
             eleven screens in the shipped chrome
src-tauri/   Tauri 2 shell — 42 typed commands, game/scan/hunt guards, live
             progress events, CurseForge client; `release.yml` builds the
             NSIS installer on every push (Actions run → Artifacts)
docs/        ARCHITECTURE · SAFETY_MODEL · DATA_MODEL · DESIGN_SPEC ·
             DEVELOPMENT · ROADMAP · RESEARCH · FEATURE_STATUS ·
             TROUBLESHOOTER · PROFILES · PATCH_CENTER
fixtures/    generate_demo_library.py — safe test library with documented
             findings
```

## Safety principles (non-negotiable)

Every bulk mutation: validate containment → refuse while the game runs →
immutable plan → user review → snapshot where files leave the library →
journal → execute → verify hashes → update the database only after
filesystem verification → provide restore. Destinations are never
overwritten; a changed file refuses to move; partial backups remove
themselves; corrupt backups refuse to restore. The troubleshooter, the
toggle engine, and profile switching all ride the same `verified_move`
and the same journal as quarantine — one safety story, everywhere.
