# Changelog

## 0.3.0 — The Troubleshooter Release

The 50/50 assistant, end to end — validated on a live 4,213-file
library: thirteen rounds, culprit confirmed, quarantined, and restored.

* **50/50 troubleshooting assistant.** A persistent, resumable binary
  search for the file causing an in-game problem. Sessions survive
  restarts; every move is hash-verified and journaled; exonerated halves
  stay set aside until one verified restore at the end; abort returns
  every file to its exact original path from any phase. A confirmed
  culprit moves to Quarantine with the reason "Troubleshooter: confirmed
  culprit." Inconclusive hunts say so honestly and restore everything.
  (`docs/TROUBLESHOOTER.md` has the full state machine and design
  decisions.)
* **Guided wizard.** A new Troubleshoot screen walks the hunt one
  question at a time: suspects remaining, set-aside count, estimated
  tests to go, verdict tiles that explain what each answer means before
  you click, and a live progress bar while arrangements move files. The
  sidebar badge counts suspects down while a hunt is running.
* **Reconcile on open.** Opening the wizard onto a live session heals
  session records from disk truth after a crash and reports — never
  auto-resolves — files found in both places or neither.
* **Cross-guards.** Scanning is blocked while a hunt is active (a scan
  would mark the set-aside half as missing), and hunts are blocked while
  a scan runs. Verdicts and aborts refuse while The Sims 4 is open.

## 0.2.0 — The Gilded Release

* **The full art direction, ported for real.** Cartouche frames on
  every surface (built on primitives that no renderer can drop),
  engraved corner scrolls, spliced finials, paper grain, the lit
  emerald sidebar with its starfield, constellation, crystal watermarks
  and light sweep, medallion stat tiles, jewel buttons, Playfair
  Display throughout.
* **The Dashboard composition.** Welcome header with a working library
  search, four hero stats (including real last-backup), Recent Findings
  fed by actual conflict and duplicate records, the dark Library Size
  hero with its own night sky, Recent Activity from the operations
  journal, and wired Quick Actions.
* **CurseForge key intake.** Settings → Connections stores an API key
  in the local database only, ready for the Patch Center.
* **Version truth.** The footer version now comes from a single
  authoritative place; the long-lived v0.1.0 ghost is exorcised.

## 0.1.0 — Foundation

Read-only scanning and fingerprinting, duplicates and conflicts
detection, quarantine with verified restore, snapshots, the operations
journal, and the safety contract everything else is built on: plan →
preview → verify → journal → reversible.
