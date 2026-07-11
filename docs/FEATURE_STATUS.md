# Feature Status

Statuses: **Complete** · **Partial** · **Scaffolded** · **Not implemented** · **Requires external credentials** · **Experimental**

Updated: Phase 2 plateau 3 — Phase 2 complete (Conflicts screen live, suspected duplicates, Library filters, parse pass wired into scans). This file is updated at the end
of every plateau and never claims more than what the test suite and a running
build actually demonstrate.

## Safety core (`core/`, Rust — 92 tests passing on Linux; Windows via CI)

| Capability | Status | Notes |
|---|---|---|
| Path containment (`SafeRoot`) | Complete | Canonicalization, `..`/absolute rejection, symlink-escape detection, case-insensitive (Windows-semantics) comparison, planned-destination validation |
| Collision-free naming | Complete | ` (2)`, ` (3)` … before extension |
| Recursive scanner | Complete | Classification, zero-byte, empty dirs, depth, deep-script warning, exclusions, symlink skip+record, nonfatal error collection, cancellation, progress |
| Streaming SHA-256 | Complete | 1 MiB buffer, cancellation, byte progress; large-file test crosses buffer boundaries |
| Exact duplicate detection | Complete | Size pre-group → hash group; explained keep-recommendation cascade (manifest → category → cleanest path → oldest) |
| Verified move | Complete | Hash verification after move; rollback on stale-hash mismatch; copy-verify-delete fallback for cross-device |
| Quarantine + restore | Complete | Relative-structure-preserving storage, stop-on-error policy, occupied-destination refusal |
| Snapshots (backups) | Complete | All-or-nothing copy with per-file hash verification and `manifest.json`; partial snapshots self-remove |
| Snapshot restore | Complete | Corrupt-backup refusal (manifest hash check before touching live files); staged overwrite |
| Operation journal | Complete | `JournalSink` trait + lifecycle events; persisted to SQLite via `SqliteJournal` |

## Persistence & services (`core/src/db/` — SQLite, bundled)

| Capability | Status | Notes |
|---|---|---|
| Versioned migrations (spec §10.4, all 18 tables) | Complete | Transactional runner on `PRAGMA user_version`; fresh-install and stepwise-upgrade paths tested; FK enforcement + index presence tested |
| Category seed (spec §10.7) | Complete | Full tree incl. CAS and Build/Buy hierarchies, system-flagged |
| Scan reconciliation | Complete | New/changed/unchanged/missing/reappeared; stale hashes cleared on change; NOCASE path identity (Windows semantics); quarantined and excluded-prefix rows never falsely go missing; fully transactional with rollback test |
| Hash persistence | Complete | Batch `update_hashes` after the streaming pass |
| Library counts + listing | Complete | Single-pass aggregates; paginated NOCASE search |
| Duplicate persistence | Complete | Facts loader (skips missing/quarantined), open-group replacement that preserves user-resolved groups, two-query group+member views |
| Operation journal persistence | Complete | `SqliteJournal` writes operations/steps live; journal failures never abort filesystem work and surface via `finish()` |
| Quarantine records | Complete | Entries linked to files + operations; file status flip; restore healing |
| Backup records | Complete | Snapshot manifests persisted with entries; operation ↔ backup linked both ways |
| Typed settings | Complete | `AppSettings` round-trip; corrupt values fall back to defaults; unknown keys ignored |

## Desktop shell (`src-tauri/` — compiles in CI, not yet exercised end-to-end)

| Capability | Status | Notes |
|---|---|---|
| Tauri 2 shell + typed command boundary (22 commands) | Complete (pending first CI build + GUI runtime validation) | Settings, onboarding detection/validation, scan lifecycle with progress events, library queries, snapshot-first quarantine, restores, activity, path-gated reveal |
| Game-running detection & mutation refusal | Complete (pending runtime validation) | `sysinfo` process scan for TS4 executables; every mutating flow checks it |
| Scan pipeline orchestration | Complete | Blocking-thread scan → reconcile → hash → duplicate refresh; DB lock never held during the walk or hashing; cancel supported |
| Snapshot-before-quarantine flow | Complete | All-or-nothing backup recorded before any move; per-file expected-hash (stale-plan) protection |
| Windows installer per commit (`release.yml`, tauri-action@v1, NSIS) | Complete (first run happens on push) | Installer downloadable from each run's Artifacts section; per-user install, no admin prompt |
| Capability lockdown | Complete | `core:default` + `dialog:default` only; no generic FS/shell permissions in the webview |

Honest caveat: this crate cannot compile in the build container (toolchain
too old for Tauri 2), so "Complete" here means fully written, syntax-checked,
built strictly on the unit-tested core API, and awaiting its first CI compile
and screenshot-verified GUI run. Any CI compile error is expected to be
shallow and fixable from the Actions log.

## Interface

| Capability | Status | Notes |
|---|---|---|
| Design tokens (spec §5.3 palette) | Complete | CSS variables + semantic Tailwind mapping; reduced-motion respected |
| Typed IPC layer | Complete | Wire types mirroring serde output, wrappers for all 22 commands, typed event subscriptions, browser-preview gate with honest notice |
| Application shell + sidebar navigation | Complete (pending GUI runtime validation) | Live count badges (duplicates, quarantine), game-status footer, planned features shown honestly as "soon", error banner surface |
| Onboarding | Complete (pending GUI runtime validation) | Auto-detect + folder picker + read-only validation, first scan with live progress, disclaimer shown |
| Dashboard | Complete (pending GUI runtime validation) | Attention pills, library stats, duplicates summary with reclaimable bytes, scan card with live progress/cancel and honest session-outcome summary |
| Settings | Complete (pending GUI runtime validation) | Folder pickers (defaults clearable), exclusion editor, safety toggles, script depth, app info + disclaimer; validation errors surfaced from Rust |
| Library | Complete (pending GUI runtime validation) | Debounced search, pagination, type/status pills, selection → shared quarantine flow, reveal in file manager |
| Duplicate Center | Complete (pending GUI runtime validation) | Keep-selection per group with explained recommendations, backup-first set-aside via shared dialog, dismiss preserved across rescans |
| Quarantine | Complete (pending GUI runtime validation) | Two-step restore with game guard, reveal stored copy, restored-history toggle |
| Backups | Complete (pending GUI runtime validation) | Expandable snapshots, per-file restore, explicit overwrite escalation only when a destination is occupied |
| Activity | Complete (pending GUI runtime validation) | Full journal with expandable hash-verified steps, including failures and why |
| Shared quarantine flow | Complete (pending GUI runtime validation) | One dialog for Library + Duplicates: read-only preview → backup → verified moves → honest outcome incl. per-file failures |

## Advanced (capability-flagged per spec §32)

| Capability | Status |
|---|---|
| DBPF index parsing (read-only: header + resource keys) | Complete | `core/src/dbpf.rs`, 11 tests: bitfield-index variants, hi/lo instance assembly, corrupt/truncated/hostile inputs refused honestly; sourced type-name map |
| Package-resource storage & incremental parse pass | Complete (core) | migration 0003, `core/src/db/packages.rs`; content-keyed staleness, cancel-safe resumable pass; realistic end-to-end tests on real DBPF bytes |
| Resource-conflict detection (queries + noise policy) | Complete (core) | identical-content overlaps routed to Duplicates; ubiquitous tool-stamp keys (>12 packages) excluded as boilerplate; presentation-only overlaps low severity; same-folder/same-mod flagged likely intentional; name-based load-order approximation |
| Suspected duplicates (same name, different content) | Complete (core) | `list_suspected_duplicates`; exact-content pairs excluded by design |
| Parse pass in the scan pipeline | Complete | new "parsing" phase with progress events; file IO outside the DB lock; outcome reports packages indexed / unreadable |
| Conflicts screen | Complete | needs-a-look vs probably-fine split, load-order display with presumptive winner, sample keys, honest caveats (index-level analysis; ts4script not analyzable) |
| Suspected duplicates section (Duplicate Center) | Complete | display + Reveal only — no set-aside from the lower-confidence tier by design |
| Library status filters | Complete | All / Packages / Scripts / Archives / Zero-byte / Deep scripts / Missing / Quarantined / Unreadable, with honest filtered totals |
| CurseForge provider | Not implemented — Requires external credentials (Phase 3) |
| Patch Center, 50/50 assistant, Profiles, Merging | Not implemented (Phases 3–5) |
