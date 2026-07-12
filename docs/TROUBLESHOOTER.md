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

* **Baseline.** The session enrolls every current `.package` and
  `.ts4script` file. Nothing moves yet: the first question is simply "with
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
* **Plateau 2 (next):** shell commands, reconcile-on-open, the
  game-running guard, and the guided wizard UI.
* **Plateau 3:** end-to-end validation on the real library and installer.
