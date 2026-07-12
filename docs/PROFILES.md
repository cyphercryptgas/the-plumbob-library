# Profiles

A profile names the person — or the setup — holding the save, and will
soon own a full picture of which mods are on.

## Plateau 1 — Identity (shipped, v0.4.0)

Profile records with create / rename / activate / delete, a partial
unique index so the database itself guarantees at most one active
profile, first-profile auto-activation, and the Dashboard greeting
drawn from the active profile's name.

## Plateau 2 — The toggle engine (shipped, v0.5.0)

Disabling a mod is a **verified in-place rename**: `Name.package` ⇄
`Name.package.off`. The game only loads `.package` / `.ts4script`, so
the `.off` form is invisible to it while the file never leaves its
folder.

The rules, exactly:

* Every toggle goes through `verified_move`: hash-checked against the
  indexed fingerprint (a changed file refuses to move), journaled as a
  `mods_disable` / `mods_enable` operation with per-file steps, and it
  refuses outright if the target name is already occupied.
* **The scanner speaks the dialect.** A file ending in `.off` whose stem
  is a mod scans under its *logical* identity — same `relative_path`,
  same record, `enabled = 0`, physical name preserved in
  `current_filename`. Consequences, all tested:
  * Renames done by hand in Explorer sync into the index on the next
    scan, both directions.
  * A file is *missing* only when neither physical form exists.
  * If both forms exist, the enabled one owns the record; the `.off`
    twin stays on disk for the user to resolve.
* `relative_path` is the file's permanent logical identity — quarantine,
  restore, and troubleshooting all key on it, so a disabled file's
  history survives every toggle.
* Disabled mods are excluded from 50/50 hunt enrollment (the game
  ignores them, so they can't be culprits), and toggling is mutually
  exclusive with scans and active hunts.

## Plateau 3 — Mod sets & switching (shipped, v0.6.0)

Each profile owns the set of files it keeps disabled
(`profile_disabled`, migration 0006). The rules, exactly:

* **The active profile live-tracks reality.** After every operation that
  changes enabled states — toggles, scans (which sync manual renames),
  quarantines, restores — the active profile's set is rewritten
  wholesale from disk truth. Cheap, and impossible to drift.
* **Creating a profile snapshots the current setup**, including the
  first auto-activated one. Naming an arrangement is one click.
* **Switching is a previewed diff.** `switch_plan` is pure set algebra
  computed in core (and tested there): files the target keeps off that
  are currently on → disable; files currently off that the target
  doesn't keep off → enable. Files in the target's set that are missing
  or quarantined are `unavailable` — reported to the user, never
  silently dropped from intent.
* **Activation only on a clean apply.** Both directions run as
  best-effort journaled batches (`profile_switch` operations) with live
  progress on `profile://progress`. If any rename fails, the previous
  profile stays active, the failures are named, and retrying applies
  only the shrunken remainder. Every completed rename is individually
  recorded either way.
* Deleting the active profile leaves no profile active: the greeting
  reverts and toggles simply stop write-tracking until another profile
  is activated.
