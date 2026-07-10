# Architecture

Three layers, one direction of dependency, no layer skipping.

```
React + TypeScript interface (src/)
        │  typed invoke() calls + events        ← camelCase wire format
        ▼
Tauri 2 command boundary (src-tauri/)           ← thin: validate, delegate, translate errors
        │  plain function calls
        ▼
plumbob-core (core/)                            ← ALL logic: fs safety, scan, hash,
                                                   duplicates, ops, SQLite
```

## Why `core/` is a standalone Cargo root

`core/` is deliberately **not** a workspace member of `src-tauri`:

* it stays resolvable and testable on conservative toolchains (distro Rust
  1.75 in build containers) — dependency choices are pinned accordingly
  (`rusqlite 0.31` bundled; no `uuid`; `tempfile` without its randomness
  feature) with the reasons documented in `core/Cargo.toml`;
* CI runs its full test suite on **both Linux and Windows** runners
  (`ci.yml`), so NTFS semantics (case-insensitive paths, collision naming,
  quarantine round-trips) are exercised on the platform the app ships on;
* the Tauri crate compiles only in CI with current stable Rust, and stays a
  thin boundary precisely so that being CI-only is low-risk.

## The command boundary

22 commands (`src-tauri/src/commands.rs`), all typed, all delegating to
`service.rs`. Long operations (`start_scan`, `execute_quarantine`, both
restores) run via `spawn_blocking`; the single SQLite connection sits behind
a mutex **on purpose** — overlapping bulk mutations on one Mods folder is
exactly the situation this app exists to prevent. The scan pipeline holds
the lock only for its short write phases, never during the walk or hashing.

Events: `scan://progress`, `scan://completed`, `library://changed`. The
interface's `AppContext` bumps a `libraryVersion` counter on these, and every
list screen reloads from it — after a quarantine, the Library, Duplicates,
Quarantine, Backups, and Activity screens all refresh themselves.

## Wire format

Every serialized struct uses `#[serde(rename_all = "camelCase")]`;
`src/lib/types.ts` mirrors them field-for-field and `src/lib/commands.ts` is
the only place command names/argument shapes appear. If Rust changes, those
two files change.

## Product-name centralization

Exactly three sanctioned literals: `core/src/product.rs`,
`src/lib/product.ts`, `src-tauri/tauri.conf.json` (productName + window
title, one file). Everything else imports. Renaming the product is a
three-file edit.

## Data location

Windows: `%APPDATA%\com.moetech.plumbob\` — `plumbob.db` plus the default
`Backups/` and `Quarantine/` folders (both relocatable in Settings, never
inside the Mods folder).
