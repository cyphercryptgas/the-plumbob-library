# Data Model

SQLite, bundled (identical bytes on every platform). Schema lives in
versioned migrations under `core/migrations/`, applied transactionally with
`PRAGMA user_version` tracking. **Shipped migrations are never edited —
append a new one.** Fresh-install and stepwise-upgrade paths are both tested
to produce identical schemas.

## Conventions

* Timestamps: RFC 3339 UTC `TEXT`.
* Booleans: `INTEGER` 0/1.
* Relative paths: `TEXT`, stored with `/` separators on every platform,
  unique **NOCASE** (`idx_files_relative_path`) to match Windows filesystem
  semantics — `CAS/Hair.package` and `cas/hair.package` are the same file.
* Foreign keys ON per connection; WAL journal on file-backed databases;
  every multi-row mutation is one transaction.

## Table groups (18 tables, migration 0001)

* **Inventory** — `files` (per-file truth: paths, type, size, sha256, fs
  timestamps, first/last seen, flags `missing`/`zero_byte`/`deep_script`,
  status, optional pre-grouping `category_id`/`creator_id`), `scans`,
  `scan_errors`.
* **Catalog** — `mods`, `creators`, `categories` (seeded tree in migration
  0002: CAS and Build/Buy hierarchies, system-flagged), `tags`, `mod_tags`,
  `collections`, `collection_mods`. Deleting a mod releases its files
  (`ON DELETE SET NULL`) — file records are never deleted by catalog edits.
* **Duplicates** — `duplicate_groups` (+ recommendation, reason,
  reclaimable bytes), `duplicate_group_files`. Rescans replace only
  `status='open'` exact groups; `resolved`/`dismissed` survive.
* **Operations & safety** — `operations` (integer PK + unique
  `operation_uid` string from the journal), `operation_steps`, `backups` ↔
  `operations` linked both ways, `backup_entries`, `quarantine_entries`.
* **Settings** — key/value `TEXT`, but accessed only through the typed
  `AppSettings` struct (one parser/serializer per key; corrupt values fall
  back to defaults; unknown keys ignored for forward compatibility).

## Status vocabularies

* `files.status`: `current` · `missing` · `quarantined`
* `operations.status`: `running` · `completed` · `partial` · `failed`
* `duplicate_groups.status`: `open` · `resolved` · `dismissed`
* `quarantine_entries.status`: `quarantined` · `restored`
* `backups.status`: `available` (future states reserved)

## Reconciliation invariants (tested)

New rows get `first_seen_at`; unchanged rows refresh `last_seen_at` only;
changed rows (size or mtime drift) **clear** their stored hash and re-queue
for hashing; unseen rows go missing **except** quarantined rows and rows
under excluded prefixes; previously-missing rows that reappear heal to
`current`; cancelled scans skip the missing pass entirely.
