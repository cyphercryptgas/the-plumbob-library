# Safety Model

The product promise: **every mutation is planned, previewed, backed up,
hash-verified, journaled, and reversible.** This document maps each
guarantee to the code that enforces it and the test that proves it.
"Trust us" is not a mechanism; everything below is.

## The contract, end to end

For any bulk change (quarantine is the shipped example):

1. **Game guard** — refuse while The Sims 4 runs
   (`src-tauri/src/game.rs`, checked in `service::execute_quarantine` and
   every restore). Moving files the game holds open corrupts sessions.
2. **Plan from database truth** — the selection resolves to rows with known
   sizes and hashes (`db::files::files_by_ids`); missing-on-disk or
   already-quarantined selections refuse up front.
3. **Preview** — read-only (`service::preview_quarantine`); the interface
   shows exactly which files, total bytes, and warnings before anything
   happens.
4. **Snapshot first, all-or-nothing** — `ops::create_snapshot` copies every
   file, verifies each copy's hash, writes `manifest.json`, and **removes
   itself entirely on any failure** (test:
   `failed_snapshot_removes_partial_backup`). A backup that silently lacks
   files is worse than no backup.
5. **Hash-verified moves** — `ops::verified_move` re-hashes at the
   destination; on mismatch against the expected hash it **rolls the move
   back** (test: `stale_expected_hash_rolls_the_move_back`). The expected
   hash comes from the database row, so a file changed since planning
   refuses to move — stale-plan protection.
6. **Journal** — every step streams into `operations`/`operation_steps` via
   `SqliteJournal` (test: `journal_persists_operation_and_steps`). Journal
   write failures never abort in-flight filesystem work (aborting mid-move
   is the greater danger) but surface via `finish()` and must be checked.
7. **Records after verification** — file rows flip to `quarantined` only
   after the filesystem succeeded (`record_quarantine_outcome`).
8. **Restore path** — quarantine restore verifies the stored copy against
   the hash recorded at quarantine time and **never overwrites** an occupied
   original path (tests: `restore_returns_file_to_original_path`,
   `restore_refuses_occupied_original_path`).

## Containment (nothing escapes the roots)

`paths::SafeRoot` canonicalizes its root (via `dunce`, so Windows `\\?\`
prefixes never poison comparisons) and validates every path it hands out:

* relative inputs reject `..` and absolute components (test:
  `rejects_parent_traversal_in_relative_path`,
  `rejects_absolute_path_passed_as_relative`);
* symlinks that escape the root are detected by canonicalizing the deepest
  existing ancestor (test: `rejects_symlink_escaping_root`);
* planned destinations that don't exist yet are validated the same way
  (test: `rejects_planned_path_with_embedded_traversal`);
* comparisons are case-insensitive, matching Windows semantics (test:
  `case_insensitive_mode_matches_windows_semantics`).

Quarantine requests that attempt traversal fail as a step and, under the
default stop-on-error policy, halt the remaining plan untouched (test:
`quarantine_rejects_traversal_and_halts_plan`).

The interface never receives generic filesystem power: the webview
capability grants only `core:default` and `dialog:default`; "reveal in file
manager" is a typed command gated to paths inside the Mods, quarantine,
backup, or app-data roots (`service::reveal_in_explorer`).

## Content identity

SHA-256, streamed in 1 MiB chunks (`hashing::sha256_file`), cancellable, with
a test that crosses buffer boundaries. Rules downstream of hashing:

* a **changed** file's stored hash is **cleared** during reconciliation —
  a stale hash is worse than none (test:
  `changed_files_clear_stale_hashes_and_request_rehash`);
* duplicate groups form only from size + full-hash equality — never names;
* a **corrupt backup refuses to restore** rather than replacing a live file
  with damaged bytes (test: `corrupt_backup_refuses_to_restore`);
* overwriting during backup restore requires an explicit flag, and the
  interface only offers "Replace it" **after** a restore actually reports an
  occupied destination — the dangerous option is never the default.

## Scan honesty

* A **cancelled** scan skips the missing pass entirely: files the walker
  never reached are unvisited, not gone (test:
  `cancelled_scans_never_mark_unreached_files_missing`).
* **Quarantined** rows are never flipped to missing by a rescan — we moved
  them (end-to-end pipeline test in `db/mod.rs`).
* **Excluded prefixes** never produce false missing — skipped isn't gone
  (test: `excluded_prefixes_do_not_produce_false_missing`).
* Symlinks are recorded and never followed; unreadable entries are nonfatal,
  collected errors; reconciliation is one transaction and a poisoned-trigger
  test proves partial failures roll back the scan row itself.

## Structural refusals

* Backup/quarantine folders inside the Mods folder are refused (scan churn,
  self-quarantine) — enforced in both `save_settings` and
  `service::resolve_roots`.
* Destinations are never overwritten anywhere in the core; collision-free
  naming (` (2)`, ` (3)`) is a caller decision, used only inside the
  app-owned quarantine tree.
* The database opens with foreign keys ON; every multi-row mutation is
  transactional.

## What this model does not claim

No malware scanning, no mod-compatibility judgment, no game-version
knowledge. The app makes file operations *safe and reversible*; it does not
make mods *good*. Package-content analysis (DBPF) is a Phase 2 capability
and is flagged off until it truly exists.
