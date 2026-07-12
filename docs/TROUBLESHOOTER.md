# The 50/50 Troubleshooting Assistant

Something is wrong in the game — a crash, a broken interaction, a last
exception — and one of four thousand files is responsible. The 50/50 method
is the community's answer: remove half your mods, test, keep the half that
still shows the problem, repeat. Done by hand it is slow, error-prone, and
terrifying; files get lost, folders get scrambled, and people give up
halfway with their library in pieces.

Motherlode's assistant runs the same method as a **persistent, resumable,
hash-verified session**. The app moves the halves, remembers everything,
and can put every file back exactly where it came from at any moment.

## How a session works

```
start ─▶ baseline ──problem gone──▶ completed (no_problem)
            │ problem present
            ▼
         testing ◀────────────────────────────┐
            │ verdict                          │ pool > 1 → next round
            ▼                                  │
       shrink the suspect pool ────────────────┘
            │ pool == 1
            ▼
        confirming ──problem present──▶ restore all ▶ completed (inconclusive)
            │ problem gone
            ▼
   restore all, quarantine culprit ▶ completed (culprit_confirmed)

abort (any active phase) ▶ restore all ▶ aborted
```

* **Baseline.** The session enrolls every current, *enabled* `.package`
  and `.ts4script` file — a mod the game ignores can't be the culprit. Nothing moves yet: the first question is simply "with
  everything in place, does the problem happen?" If it doesn't, there is
  nothing to hunt and the session closes without having touched a file.
* **Rounds.** The suspect pool is split in half (sorted, deterministic).
  One half stays in Mods, the other is set aside under a managed holding
  area that mirrors each file's relative path. The user launches the game,
  tests, and reports a verdict. Problem present → the culprit is in the
  half that was in; problem gone → it's in the half that was out. The pool
  halves each round: 4,000 files corner one suspect in about twelve tests.
* **Confirmation.** When one suspect remains, the assistant arranges the
  decisive test: only the candidate is out, everything else — including
  every previously exonerated file — comes home. Problem gone confirms the
  culprit; problem present means the cause is an interaction or something
  outside the pool, and the session ends honestly as *inconclusive* with
  everything restored.
* **The culprit** is handed to the existing quarantine system with the
  reason "Troubleshooter: confirmed culprit," so it appears in the
  Quarantine screen with the same restore path as anything else set aside.

## The three design decisions

1. **Exonerated halves stay out until the session ends.** Restoring them
   between rounds would double the file moves (and the risk) for no
   diagnostic gain. Everything returns in one verified restore at the end.
2. **No pre-session full backup.** A 19 GB copy before every hunt would be
   theater: the files are *moved*, never duplicated or deleted, and every
   move verifies the content hash recorded at scan time — a stale or
   tampered file refuses to move at all. The holding copies are the
   originals.
3. **Resting states only.** The database never records "mid-move." Member
   rows update as each verified move lands, so a crash at any instant
   leaves a state where the startup **reconciler** can compare rows against
   the disk and heal the difference. Files found in both places or in
   neither are reported, never auto-resolved.

## Guarantees

* Every move is `verified_move`: rename with copy-verify-delete fallback,
  rollback on hash mismatch, never overwrites.
* Every arrangement is journaled as one operation with per-file steps —
  visible in Activity like everything else.
* A failed arrangement rolls its completed moves back and leaves the
  session resting in the phase it was in; the verdict can simply be
  retried.
* **Abort restores every file to its exact original path**, hash-verified,
  from any active phase.

## Multiple culprits

Binary search corners one culprit at a time. If two independent files each
cause the problem, the search converges on one of them (the confirmation
test tells the truth either way). Quarantine it and run another session for
the next. If the problem needs *both* files present, the confirmation
round comes back *inconclusive* — that is the honest answer, and the note
to the user says so.

## Status

* **Plateau 1 (shipped):** core state machine, migration `0004`,
  arrangement engine with rollback, reconciler, 18 tests.
* **Plateau 2 (shipped):** shell commands (`troubleshoot_start`,
  `troubleshoot_verdict`, `troubleshoot_abort`, `troubleshoot_active`,
  `troubleshoot_reconcile`) behind the game-closed guard, journal replay
  into Activity, reconcile-on-open, the sidebar badge showing suspects
  remaining, and the guided wizard screen.
* **Plateau 3 (shipped):** live arrangement progress, scan↔hunt
  cross-guards, the validation protocol below, and the 0.3.0 release
  notes.
* **Validated 2026-07-12** on a live 4,213-file library (4,172 packages,
  41 scripts): thirteen rounds converged on one file, the confirmation
  round confirmed it, the culprit landed in Quarantine with the
  troubleshooter reason and was restored through the Quarantine screen —
  every other file home, counts unchanged.

## Validating on a real library

A ten-minute dry hunt proves the restore path on real data without
needing a real bug. With the game closed:

1. Note your Dashboard's **Total files** and **Library size**.
2. Open **Troubleshoot** → *Begin the hunt* → answer **"The problem
   happens"** at baseline. Watch the progress bar: roughly half the
   library moves, hash-verified, into `<data>/Troubleshoot/session-N`.
3. Answer one more round (either verdict), then **Abort & restore
   everything**.
4. Check: Dashboard counts unchanged; **Activity** shows the
   `troubleshoot_round` and `troubleshoot_abort` operations with their
   per-file steps; the `Troubleshoot` folder in the app's data directory
   is gone; the game launches with everything present.

If any of those four checks fails, stop and report it — that is exactly
what the validation run exists to catch.
