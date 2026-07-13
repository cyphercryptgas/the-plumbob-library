# Feature Status

Statuses: **Complete** · **Partial** · **Planned** · **Unscheduled**

Updated: v0.19.0 — every planned surface shipped. This file is refreshed at
the end of each release; `CHANGELOG.md` carries the narrative.

## Safety core

| Feature | Status |
| --- | --- |
| Containment-gated scanner, content fingerprints, reconciliation | Complete |
| DBPF package index, content-keyed incremental parse pass | Complete |
| Duplicates (exact + suspected tiers) | Complete |
| Conflicts screen with researched noise policy | Complete |
| Verified quarantine and restore | Complete |
| All-or-nothing snapshots, corrupt-backup refusal | Complete |
| Operations journal (every mutation, per-file steps) | Complete |
| Migrations 0001–0007 | Complete |

## Flagship features

| Feature | Status |
| --- | --- |
| 50/50 Troubleshooting Assistant (resumable, reconciled, field-validated) | Complete |
| Enable/disable engine (`.package ⇄ .package.off`, scanner-aware both ways) | Complete |
| Profiles: identity, live-tracked mod sets, previewed switching | Complete |
| Patch Center: CurseForge Update Radar (probe-verdicted; name tier, cached, paced) | Complete |
| Library gallery: image-first grid (DDS/DST+PNG+JPEG), selectable/expandable tiles, prewarm | Complete |
| CAS subcategories via the CASP reference parser; Creators section with CF join | Complete |
| Cross-guards: scan ⇄ hunt ⇄ toggle ⇄ switch mutual exclusion | Complete |

## Screens (all in the shipped chrome)

Dashboard · Library · Duplicates · Conflicts · Troubleshoot · Quarantine ·
Backups · Activity · Profiles · Patch Center · Settings — **Complete**.
The sidebar's PLANNED section is empty.

## Remaining roadmap

| Feature | Status |
| --- | --- |
| Patch-day flow (GameVersion watch, one-click pre-patch profile) | Planned — Patch Center plateau 2 |
| Package merging | Unscheduled idea |
