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

## Table groups (19 tables; migrations 0001 + 0003)

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
* **Package awareness (migration 0003)** — `package_resources`
  (file_id → resource type/group/instance; `ON DELETE CASCADE`; instance is
  a u64 bit-cast into SQLite's signed INTEGER, which preserves equality —
  display always uses the hex TGI form), plus parse bookkeeping columns on
  `files`: `parsed_sha256`, `parse_status`, `parse_error`. Parse staleness
  is **content-keyed**: a package re-parses only when its sha256 no longer
  matches `parsed_sha256`, so unchanged files cost nothing and a corrupt
  file is retried only if its bytes change.
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
