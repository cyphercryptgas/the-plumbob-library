# Changelog

## 0.7.0 — The Patch Center, Part One: Update Radar

* **Your library, checked against CurseForge itself.** Every package and
  script is identified by CurseForge's own fingerprint (MurmurHash2,
  seed 1, whitespace stripped — their scheme, byte for byte, proven
  against independent test vectors) and compared to the mod's latest
  release. Results land in the new Patch Center screen: updates with
  one-click mod pages, up-to-date matches, and an honest count of files
  CurseForge simply doesn't know (Patreon and Tumblr CC live there —
  that's normal).
* **Private by construction.** Only anonymous fingerprints and mod ids go
  over the wire; the API key rides in a request header and lives nowhere
  but the local database. Results are cached locally, so the screen
  renders instantly between checks.
* **First run fingerprints the whole library once** (two streaming passes
  per file, flat memory even for gigabyte CC merges) with live phase
  progress; afterwards only new files pay that cost.
* The sidebar's PLANNED section is empty for the first time — every
  screen the preview promised now exists.


## 0.6.0 — Profiles, Part Three: The Switch

* **Profiles are now full mod-sets.** Each profile remembers the files it
  keeps disabled. The active profile live-tracks reality — every toggle,
  every scan-synced rename, every quarantine and restore writes through
  to its set — while inactive profiles hold their sets frozen. Creating
  a profile snapshots your current setup, so "name this arrangement"
  works the way it feels like it should.
* **"Make active" now means switch.** A previewed diff shows exactly what
  will move ("214 to disable, 89 to re-enable") before anything does;
  applying it is one journaled batch of verified in-place renames with
  the live progress bar. Files a profile keeps off that have since gone
  missing or been quarantined are reported honestly, never silently
  dropped. The target becomes active only when every rename lands — a
  partial apply names its failures, leaves the previous profile active,
  and retrying applies only what remains.
* Switching is guarded against running scans and active troubleshooting
  hunts, and shows up in Recent Activity as "Profile switched — library
  arranged."


## 0.5.0 — Profiles, Part Two: The Toggle

* **Enable / disable in place.** A disabled mod becomes `Name.package.off`
  right where it lives — the game stops seeing it, the file never moves,
  and re-enabling renames it back. Every toggle is a hash-verified,
  journaled rename that refuses if the target name is occupied or the
  file changed since it was indexed. Library rows get a Disable/Enable
  action, bulk toggles ride the existing selection, and a Disabled filter
  and Dashboard pill keep the count honest.
* **The scanner understands.** `X.package.off` scans as its logical self —
  same record, `enabled = 0` — so disabling never shows up as one missing
  file plus one unsupported stranger. Scans also *sync* renames done by
  hand in Explorer, in both directions; a file counts as missing only
  when neither form exists; and if both forms exist, the enabled one owns
  the record.
* **Guard rails.** Toggling is blocked while a scan or a troubleshooting
  hunt is running (and vice versa), and disabled mods are excluded from
  hunt enrollment — a file the game ignores can't be the culprit.
* Friendlier operation titles in Recent Activity, and the profile
  placeholder now suggests Michael *or Mackenzie*.


## 0.4.0 — Profiles, Part One

* **Profiles.** Name who's holding the save. The active profile's name is
  who the Dashboard greets — the seam reserved since the welcome header
  shipped, finally filled. Create, rename, activate, and delete from the
  new Profiles screen; the first profile activates itself; the database
  itself enforces that only one can be active.
* Honestly labeled: per-profile enabled/disabled mod sets come next, and
  the screen says so instead of pretending.


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
