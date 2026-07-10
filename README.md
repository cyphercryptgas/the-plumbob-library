# The Plumbob Library

**A safer home for your Sims 4 mods and custom content.**

A Windows-first desktop application (Tauri 2 + Rust + React/TypeScript +
SQLite) for scanning, inventorying, organizing, quarantining, backing up, and
restoring large Sims 4 Mods folders — built safety-first: every mutation is
planned, previewed, backed up, hash-verified, journaled, and reversible.

> The Plumbob Library is an independent community tool and is not affiliated
> with or endorsed by Electronic Arts, Maxis, The Sims, Overwolf, CurseForge,
> or individual mod creators.

## Current status — plateau 6 of 6 (release candidate)

Honest state, matching [`docs/FEATURE_STATUS.md`](docs/FEATURE_STATUS.md):

* **Implemented and tested (92 tests, Linux + Windows CI):** the safety core
  and full SQLite layer — containment, scanner, hashing, duplicates,
  verified moves, quarantine/restore, all-or-nothing snapshots, journal,
  migrations, reconciliation, typed settings.
* **Written and syntax-checked, compiles in CI:** the Tauri 2 shell (22
  typed commands, game guard, snapshot-first quarantine) and `release.yml`,
  which builds an **NSIS Windows installer on every push to `main`**
  (Actions run → Artifacts section).
* **All seven screens built:** Onboarding, Dashboard, Settings, Library,
  Duplicate Center, Quarantine, Backups, Activity — pending their first
  GUI runtime validation via the CI-built installer.
* **Documentation & fixtures:** `docs/` (architecture, safety model, data
  model, development guide, roadmap, cited research) and
  `fixtures/generate_demo_library.py`, which produces a safe test library
  with documented findings for first-run validation.

Nothing in this repository fakes functionality. Unfinished surfaces say so.

## Commands

```bash
# Safety core + SQLite layer (Rust ≥ 1.75; bundled SQLite compiles via cc)
cargo test --manifest-path core/Cargo.toml

# Frontend (Node ≥ 20)
npm install
npm run typecheck
npm run build
npm run dev        # foundation placeholder page
```

## Repository layout

```
core/        Rust safety core — standalone Cargo root, no Tauri dependency,
             testable on conservative toolchains and Windows CI runners
src/         React + TypeScript frontend (Vite, Tailwind design tokens)
src-tauri/   Tauri 2 shell — arrives plateau 3 with CI-built Windows installers
docs/        ARCHITECTURE · SAFETY_MODEL · DATA_MODEL · DEVELOPMENT · ROADMAP · RESEARCH · FEATURE_STATUS
fixtures/    generate_demo_library.py — safe test library with documented findings
```

## Safety principles (non-negotiable)

Every bulk mutation: validate containment → refuse while the game runs →
immutable plan → user review → backup/snapshot → journal → execute → verify
hashes → update database only after filesystem verification → provide restore.
Destinations are never overwritten; partial backups remove themselves; corrupt
backups refuse to restore.
