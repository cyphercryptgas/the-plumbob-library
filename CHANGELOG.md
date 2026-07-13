# Changelog

## 1.0.0 — The Bow

Motherlode Manager is complete. From a folder scanner with a safety
journal to a full library: reference-exact package understanding,
thumbnails decoded from EA's own shuffle, creators credited and
browsable, a CurseForge radar that names its confidence, an updater
that unpacks archives and tells the truth afterwards, and a DBPF
writer that merges your collection creator-by-creator — every
destructive step journaled, snapshotted, and reversible. The history
below tells the whole story, defect confessions included.


## 0.28.0 — The Title Tool

* **Files can be named to convention now: `[creator]_[modtype]_[modname]`,
  extracted from the files themselves.** Creator comes from attribution;
  modtype prefers the CAS subcategory (Hair, Skirt, …) and falls back to
  the category; modname takes the CurseForge match's name when there is
  one, else the current filename scrubbed of brackets, creator tokens,
  and version numbers — Alana-Mini-Skirt, Flowfit, AskToReadBook. Files
  without creator attribution are skipped and say so: the convention
  leads with one.
* **Two entry points.** In the Library, select files and Title them —
  a preview confirms before anything moves. On the Dashboard, the Title
  tool quick action (new tag emblem, replacing Scan now — scanning keeps
  its two other buttons) titles everything that arrived today.
* **Renames go through the row**: filesystem and database move together
  — path, name, match history, thumbnails all survive — and the whole
  batch is journaled, so Activity shows each retitle with its thumbnail.
  Name collisions get numbered suffixes.

## 0.27.5 — Disk Bytes Lie

* **The 4 GB wall, root-caused: the planner budgeted disk bytes; the
  writer's ceiling is decompressed bytes.** Merged entries store
  uncompressed, and zlib'd texture packages inflate several-fold — a
  1.2 GB group of sources can legitimately exceed 4 GB re-authored. The
  package index already knows every entry's decompressed size, so the
  planner now budgets on true mem_size totals (from the same index read
  the pre-flight already does) with a 3 GB per-output cap — a full
  gigabyte of margin.
* **One failing group no longer abandons the run.** The auto-merge loop
  records each group's failure, continues, and the final receipt counts
  successes and names the skipped groups.
* **Sidebar hover ring, unclipped**: the first item's gold hover frame
  painted above the scroll container's top edge; the nav has headroom
  now, top and bottom.

## 0.27.4 — Creator-First Merging

* **The planner and the executor stopped contradicting each other**: a
  200-package guard written before auto-merge existed was refusing the
  planner's own 400-file groups. The ceiling is 500 per output now,
  comfortably above anything the planner builds.
* **Groups follow creators now.** Files that share a creator merge
  together — Merged_VIBRANTPIXELS, coherent kits kept whole — and only
  creatorless files fall back to category buckets. Expect many small,
  meaningful merged files instead of a few giants; the confirm dialog
  summarizes the largest groups and counts the rest.


## 0.27.3 — The Thumbnail Cap Confession

* **The merge blocker was never exotic compression — it was a size cap.**
  The merge borrowed the thumbnail extractor's payload reader, which
  politely refuses any entry over 16 MB. A Build/Buy texture is not a
  thumbnail. Payload reads are now cap-parameterized: thumbnails keep
  their 16 MB ceiling, merges get a 512 MB one, and the regression test
  proves an over-thumb-cap entry passes the pipeline byte-faithfully.
* **Merges are per-file tolerant now.** A package whose contents
  genuinely won't decode (corrupt streams included) is named in the
  receipt and left loose; everything readable merges; and only files
  that actually merged are removed. The nothing-partial rule holds for
  every merged output.


## 0.27.2 — The Missing Element & the Stubborn Package

* **Up-to-date rows finally show images — because the row never had an
  image element.** The thumbnail budget fix in 0.27.1 was real but
  beside the point: this row family was born without an <img>. It wears
  the same thumbnail-or-chip block as every other row now.
* **Auto-merge survives stubborn packages.** The field found one: a
  package whose entries use a compression scheme our decompress-
  everything pipeline doesn't decode (EA's RefPack family). Aborting
  that merge was correct — nothing partial is ever written — but
  blocking a whole category group for one file was not. Both merge
  paths now pre-flight every package: undecodable ones stay loose
  (they work fine in-game), are named in the plan dialog and the
  receipt, and the rest merge normally.


## 0.27.1 — Emblems & the Starved Thumbnails

* **Quick actions wear their purposes**: three new icons drawn for the
  job — converging arrows into a package for Merge, a picture frame for
  Prepare thumbnails, a sweep-arm dial for Update radar.
* **Up-to-date rows get their images back.** The thumbnail budget was
  spent in raw status order, where thousands of unmatched rows (NULL
  mod names sort early) starved the up-to-date section entirely.
  Matched rows now load first, then a slice of unmatched for that view.


## 0.27.0 — The Working Dashboard

* **Quick actions now act.** Three navigation shortcuts became three
  workers: **Merge packages** plans the efficient merge — enabled
  packages with no CurseForge identity (matched files stay loose so
  they never lose the updater; disabled files stay out so a merge can't
  silently re-enable them), bucketed by category into comprehensible
  outputs (Merged_CAS, Merged_BuildBuy…), each part capped in count and
  size — then shows you the plan and the skip reasons before a single
  byte moves, and executes group by group through the same journaled,
  reversible path as manual merges. **Prepare thumbnails** runs the
  extraction pass with its arithmetic receipt. **Update radar** runs
  the CurseForge check and reports matched and update counts on the
  spot.
* **History wears the images now.** Backup entries and activity steps
  are joined back to your library rows (content hash first, path as
  fallback), so both pages show each file's thumbnail beside a readable
  name — with reason icons on backups, action verbs on steps, and the
  full paths demoted to fine print instead of leading the row.


## 0.26.0 — The Punch List

* **Backups page, healed at the root.** The listing reads a `backups`
  table that only the quarantine flow ever wrote — updates and merges
  snapshotted real files but never recorded them, and any snapshot made
  before recording existed was invisible forever. Now every snapshot
  site records, **and the listing backfills from disk first**: any
  snapshot folder whose manifest the table doesn't know gets imported
  on the spot. Your quarantine-era backups surface retroactively; every
  update and merge appears immediately.
* **LIB tags everywhere in the Patch Center.** CurseForge mod names
  don't match your filenames, so every row now carries a LIB tag that
  jumps straight to that file in the Library — reveal, disable, or
  manage from there.
* **The stat tiles are buttons.** Updates, Up to date, On CurseForge,
  Not on CurseForge — click any tile and the list below shows exactly
  those files, including the never-before-visible "not on CurseForge"
  population, each row wearing its LIB tag.
* **Date tags on grid tiles** — every file in the Library grid shows
  its modified date under the name.
* **Creators: sort toggle (Most files ⇄ A–Z) and Show all** — the
  "+N more" dead-end is gone; expand the roster whenever you like.


## 0.25.0 — The Merger

* **Packages merge.** Select two or more in the Library and the new
  Merge action combines them into one — the app's first DBPF *writer*,
  built byte-for-byte to the shape our reader (and the game) accept.
  Sources are taken in game load order (case-insensitive by path), so
  when two packages carry the same resource, **the merged file keeps the
  same winner the game already used**. Every entry is decompressed on
  the way in; an entry the reader can't decode aborts the whole merge —
  nothing partial is ever written. Proofs in the suite: a three-source
  merge with a deliberate collision roundtrips through our own reader
  with the load-order winner and byte-faithful payloads, and a merged
  package serves thumbnails through the full pipeline.
* **Reversible by construction.** Originals are snapshotted to Backups
  through the journal before anything moves, then removed from Mods;
  the merged file lands as `Merged_<timestamp>.package`. A merge is a
  restore away from undone. (Two honest notes: entries are stored
  uncompressed, so the merged file is bigger than the sum of its
  compressed parts; and merging changes load-order context relative to
  files *outside* the merge, as with every merge tool.)
* **"Ready first" toggle** in Updates: sorts the updates the app can
  apply for you above the ones that need Open Mod. On by default.


## 0.24.1 — Stale Dates & Closed Doors

* **Updated files now stay updated.** The verdict "update available"
  compares the latest release date against your file's stored modified
  time — and the update pass refreshed the hash and size but not that
  timestamp, so every Check resurrected the row you'd just updated,
  forever. The truth pass now records the on-disk mtime through the
  exact same formatter the scanner uses. Updated rows leave the list
  and stay gone.
* **Closed doors are pre-flagged.** The "author hasn't enabled
  third-party downloads" wall is CurseForge policy, not a bug — but you
  shouldn't need a click to learn it. The mod's allowModDistribution
  flag is now captured on every Check, and those rows show a disabled
  Update with the explanation on hover. (Existing rows learn their flag
  on your next Check; until then the click still answers honestly.)
* Also caught pre-flight: the shell was missing its chrono dependency —
  the installer would have said so less politely.


## 0.24.0 — The Updater, For Real

* **Archive-aware updating.** The field verdict was blunt: CurseForge CC
  ships overwhelmingly as .zip even when it holds one package, so the
  single-file gate made the Updater ornamental. Zips are now unpacked in
  memory: junk entries ignored, exactly one usable file wins, multiple
  candidates resolve by matching your filename — and genuine ambiguity
  is an honest wall, never a guess. Cross-type swaps are refused; the
  extracted bytes still face the magic check.
* **The Conflicts surprise, explained and disarmed.** An update swaps
  *contents*, and a new version can legitimately share resources with a
  sibling you already had — an ≈-match hazard the old code neither
  detected nor mentioned, while also leaving the resource index stale.
  Now every update re-indexes the package immediately, records the true
  hash and size, and the outcome names any overlap on the spot
  ("shares resources with …") instead of letting Conflicts ambush you.
* **Creator tags are clickable** — the click-through I owed: any gold
  pill in the Library, grid or list, jumps to that creator's page with
  their catalog open.


## 0.23.1 — Type Discipline

* Fixed the shell compile error the Windows installer caught: the
  update pass handed the snapshot machinery a `String` path where it
  demands `PathBuf`. The container can't compile the shell (no webkit),
  so the installer is the trial — and it did its job.

## 0.23.0 — The Updater

* **One-click Update in the Patch Center.** For matches whose latest
  release is a single file: the button downloads it, verifies the bytes
  are what they claim (DBPF magic for packages, zip magic for scripts),
  snapshots your current copy through the same journaled backup
  machinery quarantine uses, then swaps atomically. Your filename is
  kept — contents change, identity doesn't — so creator tags, categories
  and enabled state all survive; the hash is cleared so the next scan
  re-fingerprints honestly. Authors who disable third-party downloads,
  and releases packaged as archives, fall back to Open Mod with the
  reason stated plainly.
* Post-re-verify, the radar stands at 470 real matches and 68 honest
  updates — the list this button now acts on.
* README rewritten to match what the app has become.
* Fixed a cosmetic defect pixel-forensics caught: the `accent` color
  token behind active filter chips (Library) and the selected creator
  chip was never defined, so those fills silently rendered as nothing.
  One config line heals all three sites.


## 0.22.0 — The Final Pass

* **576 on CurseForge** — and now every one of them re-judged. The new
  **Re-verify matches** button (Patch Center, beside Check) re-fetches
  the matched mods in bulk — authors were never cached — and runs every
  cached name-match through the attributed standards: **kept**
  (confidence refreshed), **author-boosted**, or **dropped** (rows
  removed and the lookup cleared, so a future Check may re-search the
  term under current rules). Deletion is scoped precisely to each term's
  own files, so terms sharing a mod can't collateral-damage each other —
  that scoping is a test. The outcome prints its arithmetic: examined,
  kept, boosted, dropped.


## 0.21.0 — The Attributed Radar

* **Creator data now sharpens the radar** — the honest answer to
  "richer than names alone": yes for creators, no for images. Two
  mechanisms: **author confirmation** (a file's byline matching a
  candidate's CurseForge authors accepts modest name-matches with
  boosted confidence, while an author *mismatch* demands a distinctive
  name — two generic tokens don't earn someone else's byline) and
  **creator-anchored search** (thin names become searchable: `hair` by
  Simancholy queries as "simancholy hair"). Existing cached matches are
  untouched; the new standards apply to the remaining terms and all
  future ones.
* **Images can't honestly help identification** — our thumbnails are
  in-game renders, CurseForge logos are marketing art; hashing across
  those domains is noise at real bandwidth cost. But the Patch Center's
  rows now wear the thumbnails we already extract, matching the
  Dashboard, Duplicates, and Conflicts.


## 0.20.0 — Bylines Everywhere

* **Every credited file in the Library wears its creator's full name** —
  a gold identity pill leading the badge row on grid tiles and list rows
  alike, with the complete display form on hover for long names.
  Uncredited files stay clean rather than wearing an "Unknown" badge by
  the thousand; attribution keeps improving with each Scan as frequency
  promotion learns new prefixes.


## 0.19.0 — The Creators Section

* **Your library, by byline.** A new Creators screen reads authorship
  from the two dominant CC conventions: bracketed leads
  (`[SIMCREDIBLE]`, `[NORTHERN SIBERIA WINDS]`) always credit;
  underscore prefixes (`KUTTOE_`, `VIBRANTPIXELS_`) credit when they
  carry a creator signature or earn frequency promotion — three files
  sharing a lowercase prefix make it a creator, while a generic-word
  stoplist keeps `poses_` and `hair_` out. Precision-first and
  evidence-adjustable, fixture-tested against the field library's own
  filenames. Attribution runs with every Scan; files it can't credit
  are marked examined rather than retried forever.
* The roster shows each creator with their file count and a ✦ badge for
  works matched on CurseForge — the identity join, surfaced. Click a
  creator for their works as a thumbnail grid with the Library's
  expand actions (Disable/Enable, Reveal) and numbered paging.


## 0.18.0 — The Reference

* **BodyType has no fixed offset — a variable flag list precedes it.**
  Fetched from the s4pi reference at last: the field chain runs
  sortPriority through the exclude flags, then a **flag list (u32 count
  + 6 bytes per flag)** whose length moves every later field, then the
  string keys, then bodyType. No offset election could ever have worked;
  the 68%-at-offset-40 from the old probe was files whose *flag count*
  happened to look like a BodyType. (The bitter footnote: the original
  name parser was correct all along — BigEndianUnicodeString is a
  7-bit *byte*-length prefix, exactly what shipped in v0.13.)
* Classification now uses the **reference parser**: a sequential read of
  the documented chain with its version branches (your 0x2A cohort
  lacks createDescriptionKey; older files use 4-byte flags), gated by
  range and by sibling agreement — swatches must concur or the file
  stays unlabeled. Elections are retired to the regression-test museum.
  The probe line now reports reference-parse coverage per version.
* Fourth and final wipe. Scan reclassifies.


## 0.17.0 — The Impostor & The Third Wipe

* **Teeth in every bucket, diagnosed.** The election crowned an impostor:
  a real field with small, varied, in-range values that satisfied every
  gate — coverage, diversity, non-constancy — because those gates
  described BodyType without uniquely identifying it. The property only
  BodyType has: a package's CASP entries are swatches of one part, so
  they all share it, while swatch-varying impostors differ between
  siblings by definition. Election v3 reads up to three sibling payloads
  per package and requires ≥90% within-package agreement, plus a
  wardrobe prior (a real library concentrates in hair, tops, bottoms,
  shoes — not the exotic teens that flooded "Other"). Both failures are
  now named regression tests: the lower-offset impostor that v2
  provably elects, and the garment-less corpus the prior refuses.
* **Third wipe** of subcategory data (each one cheaper than being
  wrong), and the probe verdict now says *why* a cohort failed —
  coverage, diversity, wardrobe prior, or sibling agreement.


## 0.16.0 — Cohort Calibration

* **Your probe line solved it.** Six CASP versions in the library, best
  single scheme stuck at 68% — the signature of BodyType living at
  *different offsets in different versions* as fields were inserted over
  the years. Calibration now partitions payloads by version and elects a
  scheme inside each homogeneous cohort, then classifies every part with
  its own version's winner. The mixed-version failure from the field is
  reproduced as a regression test; the probe line now reports each
  cohort's verdict individually.
* **The thumbnail engine is complete.** DST delivered: 255 new images,
  "without art" halved to 244 — and the census shows what remains is
  overwhelmingly tuning without any art to extract (SimData and String
  Tables), plus a handful of texture-only CAS packages wearing their
  emblems honestly. No dominant undecoded type remains; new decoders
  wait for census evidence, as always.


## 0.15.0 — The Table Delivers

* **Build/Buy decodes: DST.** The census named it — 0x00B2D882 present
  in all 309 imageless packages, alongside Object Catalog, Definition,
  Light, Footprint, Slot: furniture, wearing EA's DST format (a normal
  DDS whose fourCC reads DST1/DST5 with block bytes shuffled into
  planar streams). The unshuffle is implemented exactly per the s4pi
  reference — stream order fetched from source, not memory — and tested
  by compressing real blocks, applying the reference's forward shuffle,
  and demanding pixel-faithful decode. Malformed data falls through.
* **Markers now carry a decoder generation.** Your 532 "without art"
  were verdicts from a lesser decoder; generation-stamped markers
  invalidate automatically whenever decoding improves, so Re-check
  retries them all — no more one-off amnesties.
* **Calibration reaches further and reports itself.** The BodyType
  election now tries three prefix alignments across sixty bytes, and
  the census card gains a CAS probe line: the CASP versions seen in
  your library and the calibration verdict (elected scheme with its
  numbers, or the nearest miss). If subcategories stay empty, the same
  Copy button now carries the diagnosis.


## 0.14.0 — Calibration & Three Confessions

* **Confession one: the CASP field sequence was wrong**, which is why
  every CAS part landed in "Other". The reader no longer trusts any
  fixed layout — it calibrates against your library: parsing the stable
  prefix under both documented alignments, then electing the BodyType
  column as the position whose values across hundreds of real parts are
  overwhelmingly in the 1–43 enum range, diverse like a real wardrobe,
  and never a constant. Synthetic corpora in tests prove the election
  under both layouts; noise and constants elect nothing; out-of-range
  reads are misses, not "Other". Migration wipes the wrong data — one
  Scan reclassifies.
* **Confession two: the Diagnose-blanks button never shipped.** Forensic
  check of the released zip confirms the census UI was lost before
  staging — every request for "the table" was asking for something that
  didn't exist on your machine. It exists now, with a **Copy table**
  button; string-verified into the bundle this time.
* **Confession three: the vanishing images were a stale-state bug.**
  Re-running Prepare (fast when everything's cached — that part was
  correct) wiped the thumbnail map to force a refresh, but the fetch
  effect never re-ran until the rows changed. An epoch counter fixes
  it, and Prepare now ends with its honest arithmetic: *N new · N
  cached · N without art*.
* One new sniff-guarded image type (0x00B2D882) — a well-attested
  container that costs nothing if wrong, since payloads still must pass
  the PNG/JPEG/DDS magic check.


## 0.13.0 — Subcategories & The Great Marker Amnesty

* **The stale-marker confession.** Before the DDS decoder existed, every
  package *visited* in the grid was marked "no image" — and every pass
  since has honored those verdicts forever. That's why Build/Buy stayed
  dark and CAS looked patchy run to run. All legacy markers are now
  invalidated on sight; the next Prepare re-attempts everything under
  the current decoders.
* **CAS subcategory chips.** Selecting CAS reveals a sub-row — Hats,
  Hair, Face & Sculpts, Tops, Bottoms, Full Body, Shoes, Accessories,
  Skin & Details — read from each part's own BodyType field inside its
  CASP resource (sequential-field parse behind a version gate, fixture
  bytes constructed in tests; anything unreadable stays honestly
  unlabeled and retries next scan). Tiles and rows wear their
  subcategory pill. Run one Scan to classify the library.
* **Duplicates and Conflicts wear thumbnails** on every member row,
  matching the Dashboard.
* Still owed to the workshop: the **Diagnose blanks** table — if
  Build/Buy remains dark after the amnesty, its Unknown hex rows are the
  next decoders' shopping list.


## 0.12.0 — Instruments, Not Guesses

* **Diagnose blanks.** The gallery gains a census: one click lists the
  resource types inside every package that yielded *no* thumbnail —
  labeled with the researched type table plus raw hex, counted per
  package. Build/Buy staying blank after the DDS work means the image
  type table is incomplete; this instrument names exactly what the
  blanks contain so the next decoders are added on evidence. (A
  speculative RLE2 decoder was drafted and deliberately shelved for the
  same reason the category constants once went wrong — no more
  folklore constants.)
* **Recent Findings wear thumbnails.** Dashboard finding rows now lead
  with the file's extracted in-game image where one exists, falling back
  to the emblem chip — click-through to the owning screen unchanged.
* Coming next on this thread, in order: census-informed decoder
  expansion, and CAS subcategory chips (Shoes, Tops, Hair, Sculpts…)
  parsed properly from each CAS part's BodyType field.


## 0.11.0 — The Gallery, Grown Up

* **DDS thumbnails render.** Build/Buy (and most non-CAS) previews are
  DDS-compressed; the extractor now transcodes DXT1/3/5 and uncompressed
  DDS to PNG in pure Rust — proven against DDS files the tests compress
  themselves. The categories that showed letters should light up after
  their cache entries regenerate (Prepare all thumbnails does the whole
  library in one pass).
* **The grid is the Library now.** Image view is the default; every tile
  is selectable (checkbox overlay + gold ring, feeding the same bulk
  Disable / Enable / Set-aside toolbar) and expandable — tap a tile for
  its Disable/Enable and Reveal actions, with the filename always neatly
  beneath. Files with genuinely no embedded art wear designed
  category emblems instead of bare letters.
* **Prepare all thumbnails** pre-extracts the entire library with live
  progress, so no page ever waits again; extractions were already cached
  permanently, and IO hiccups no longer get mistaken for "no image"
  (only a cleanly parsed, imageless package earns a skip marker).
* **Numbered pagination**: « First ‹ neighbors … last-three › Last »
  as a shared component, replacing Previous/Next.


## 0.10.0 — The Gallery

* **Your mods, as they look in game.** The Library gains a Grid ▦ / List ☰
  toggle: tiles show each package's extracted in-game thumbnail, with the
  file name, category badge, and the "off" state (dimmed) underneath.
  Filters, sorting, search, and paging all apply to the grid identically.
* **How extraction works.** The DBPF parser now retains what it used to
  skip — each resource's payload position, sizes, and compression — and a
  new extractor pulls the best image from the thumbnail resource types
  the conflicts research already named, decompressing zlib payloads and
  sniffing PNG/JPEG magic. Anything undecodable is skipped, never fatal;
  DDS-only packages honestly yield no tile (a letter placeholder stands
  in). Proven against synthetic packages the tests construct byte by
  byte, including codec fall-through.
* **Cached forever.** Extractions land in a Thumbnails folder in the
  app's data directory — including "nothing here" markers — so each
  package is parsed for images at most once. First visit to a grid page
  does the work; every visit after is instant.


## 0.9.1 — Politeness Engineering

* **The "rejected API key" wasn't.** CurseForge sits behind Cloudflare,
  which answers request storms with 403 — the same status as a bad key,
  and the radar's first big run (3,000+ searches, unpaced) summoned it.
  401 and 403 now carry separate, honest messages; a block pauses the
  check gracefully like a rate limit instead of hard-failing; and the
  name tier paces itself (200 ms between searches) and handles at most
  600 new terms per run. The cache carries the rest — nothing from the
  blocked run was lost, and the block itself clears on its own, usually
  within the hour.
* **Date sorting means the file's date now.** An imported library is
  "first seen" all at once, so sorting by it collapsed into A–Z order.
  Sort and the date filter chips now key on each file's own
  modification date — a decade of creator builds finally spreads out —
  and the row is labeled "File date" to say what it means.


## 0.9.0 — The Name Radar

* **Tier-2 matching, mandated by evidence.** The corpus probe proved
  CurseForge's exact-match index doesn't cover The Sims 4, so the radar
  now matches by *name*: each file yields a cleaned search term
  (creator prefixes kept, CamelCase split, versions, hashes, and
  bracket-tags dropped — all pinned by fixtures), files sharing a term
  share one search, and candidates are scored by token overlap against
  the mod's name *and* authors. Accepted matches are labeled **≈ name**
  with their confidence, never dressed up as exact.
* **Resumable by construction.** Every term ever searched — hit or
  miss — is cached, so a rate-limited run pauses politely and the next
  Check continues where it stopped instead of starting over.
* **Honest updates for approximate matches.** With no exact file to
  compare, a name match flags an update when CurseForge's latest
  release postdates your file's on-disk time — a heuristic, presented
  as one.
* The dead fingerprint phase is skipped automatically when the probe
  says the index is absent, and the diagnosis line now reports what the
  name tier achieved instead of promising it.
* **Library sorting**: a cycle control next to the count — Name A–Z →
  Newest first → Oldest first.


## 0.8.1 — The Investigation Release

* **Categories fixed.** The classifier guarded on `parse_status =
  'parsed'`; the parse pass writes `'ok'`. Every package fell through to
  unclassified — and the unit test agreed, because its seeder shared the
  same wrong assumption. Both now use the production literal. Run one
  scan and the In-game filters populate.
* **The fingerprint is certified.** Our MurmurHash2 now property-tests
  against the exact `murmur2` crate that furse/ferium ship to real
  CurseForge users — agreement across every size, seed, and tail length.
  Zero matches is provably not the math.
* **So the radar interrogates CurseForge itself.** Each check now runs a
  corpus probe: fetch a popular Sims 4 mod and feed CurseForge's *own*
  fingerprint for it back into their matcher. The verdict prints on the
  screen — either their exact-match index doesn't cover The Sims 4, or
  it does and these exact bytes were simply never uploaded there. Either
  way, name-based matching is the planned next tier, no longer a guess.
* **"Added" date filters** in the Library: last 7 / 30 / 90 days and
  Older, keyed on when Motherlode first saw each file.


## 0.8.0 — Categories & Radar Truth

* **In-game categories.** Every package is classified by what it *is* —
  CAS, Build/Buy, Poses & Animations, Gameplay, Scripts — from its
  resource census, using the same researched type constants the
  conflicts policy stands on. The Library gains an "In game" filter row
  and per-row category badges; classification refreshes with every scan
  (run one scan to backfill an existing library).
* **The radar tells the truth now.** The field run's lone "match" was a
  Minecraft jar from 2014 — CurseForge's fingerprint endpoint leaks
  cross-game collisions despite its game-scoped path. Matches whose mod
  belongs to another game are dropped and *counted*, and the summary
  reports raw hits and ignored collisions so a cold result is a
  diagnosis, not a mystery.
* **"Open Mod" goes to the app.** Update rows now deep-link into the
  CurseForge desktop app (`curseforge://install`), falling back to the
  website when no handler is installed.


## 0.7.1 — Truth in Copy

* The Profiles screen still said its mod-sets were "coming next" two
  releases after they shipped and were field-verified — the honesty
  clause cut both ways and lost. It now describes what the feature
  actually does. The Settings hint likewise stopped calling the Patch
  Center "future" while being the very key it consumes.


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
