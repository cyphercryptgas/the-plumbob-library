# Roadmap

Later phases, in dependency order. Every item below is **capability-flagged
off** until it truly works — the interface lists them under "Planned" and
never renders fake versions. Status source of truth: `FEATURE_STATUS.md`.

## Phase 2 — package awareness ✅ SHIPPED

* DBPF (`.package`) header/index parsing: resource counts, resource-key
  extraction.
* Resource-conflict detection (same resource key in multiple packages) with
  a Conflicts screen distinguishing intentional overrides from collisions.
* "Suspected duplicate" tier (same-name/near-size) beneath exact duplicates,
  clearly labeled as lower confidence.

## Phase 3 — provenance & updates

* Installation manifests: record archive → extracted-files mapping at
  install time, upgrading duplicate recommendations from the current
  "linked to a mod record" approximation to true manifest association.
* CurseForge provider (**Requires external credentials** — the API key
  intake shipped in Settings → Connections with v0.2.0; the key is stored
  only in the local database): metadata, update
  checks, per-mod source links. Strictly read-only against the API at first.
* Patch Center: post-game-update triage — what's flagged, what's stale,
  creator-link jump-offs.

## Phase 4 — workflow tools

* 50/50 assistant (**core engine shipped** — persistent state machine,
  hash-verified arrangements, reconciler; see `docs/TROUBLESHOOTER.md`;
  shell commands and the guided wizard UI are next): guided binary-search
  sessions over the library using the
  existing quarantine engine (snapshot-first, fully reversible by design).
* Profiles (**fully shipped** — identity and the greeting, the verified
  in-place toggle engine with scanner awareness, and per-profile mod
  sets with previewed one-click switching; see `docs/PROFILES.md`).

## Phase 5 — advanced

* Package merging (with the strongest warnings and mandatory backups).
* Organize: category→folder mapping + planned, previewed re-organization
  moves — this also flips `in_expected_category` from constant-false to
  real data in duplicate recommendations.

## Non-goals

Mod discovery/browsing marketplaces, malware scanning, telemetry, ads.
