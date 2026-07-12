# Patch Center

Surviving patch day, and never missing a mod update again.

## Plateau 1 — Update Radar (shipped, v0.7.0)

### The fingerprint

CurseForge identifies files by **MurmurHash2 (32-bit, seed 1) computed
over the file's bytes with all whitespace removed** — exactly the bytes
`0x09`, `0x0A`, `0x0D`, `0x20`. Matching their scheme byte-for-byte is
the entire feature; the implementation in `core/src/curse.rs` is
cross-checked against vectors computed by an independent implementation,
including incremental hashing across every chunk-boundary shape.

Fingerprinting a file is a **two-pass streaming read**: MurmurHash2
seeds itself with the input length, and the stripped length isn't known
until the bytes have been walked once. Merged CC packages run to
gigabytes, so flat memory beats buffering. The first check fingerprints
the whole library once (stored in `files.curse_fingerprint`, migration
0007); afterwards only new files pay the cost, folded into the check.

### The check

1. Fingerprint any eligible file that lacks one (live progress; the
   database lock is never held across disk or network work).
2. `POST /v1/fingerprints/{gameId}` in batches of 500 — the Sims 4 game
   id is discovered from `/v1/games` by slug, never hardcoded.
3. `POST /v1/mods` in batches of 50 for names, links, and latest files.
4. Compare each match to the mod's newest file. CurseForge emits
   RFC 3339 dates with *varying sub-second precision*, and `'Z'` outranks
   digits lexically — the comparator pads fractional parts because the
   naive string comparison looked right and wasn't (there's a test).
5. Replace the local cache (`curse_matches`) atomically: the radar
   always shows one coherent snapshot with one `checked_at`.

### Privacy

Only anonymous fingerprints and mod ids leave the machine. The API key
is sent as a request header and stored nowhere but the local database.
Files CurseForge doesn't recognize are counted honestly as *not on
CurseForge* — normal for Patreon, Tumblr, and merged CC.

## Plateau 2 — Patch-day flow (next)

Watch the game's `GameVersion.txt`; when EA patches, greet the user with
a patch-day checklist that leans on everything already built: snapshot
the current setup as a "Pre-patch" profile, one-click disable all script
mods, test, then re-enable as the radar shows updates landing.
