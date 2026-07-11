# Research Notes

Verified claims underpinning product behavior, with sources. Anything not
listed here is an implementation decision, not a domain fact.

## Mods folder & loading rules

**Location.** The standard folder is
`Documents\Electronic Arts\The Sims 4\Mods` (OneDrive-redirected Documents
folders and localized folder names occur in the wild, which is why
auto-detection falls back to a manual folder picker).

**What the game loads.** Only `.package` and `.ts4script` files are loaded;
compressed archives are invisible to the game until extracted — "If your file
names end in zip, rar, or 7z, you have to uncompress them before the game
will even see them" (EA Forums, "How do I edit my resource.cfg file",
https://forums.ea.com/discussions/the-sims-4-mods-and-custom-content-en/re-how-do-i-edit-my-resource-cfg-file-for-sims-4/9268685).
This is why archives are inventoried and flagged rather than treated as
working content.

**Script depth.** With the default `Resource.cfg`, `.ts4script` files load
from at most one subfolder below the Mods root, while `.package` files load
several levels deep. Sources: EA Forums "Sub Folders" ("Any files with
'.ts4script' at the end can go into folders ONE folder deep. Any files with
'.package' can go into folders 6 folders deep",
https://forums.ea.com/discussions/the-sims-4-mods-and-custom-content-en/sub-folders/9164012);
community guides repeat the one-deep script rule (kemzimamods.com
"How to Organise Your Sims 4 Mods Folder", simsationalchannel.com
"Ultimate Guide to Organizing Your Sims 4 Mods Folder"); SimsWiki documents
the default `Resource.cfg` wildcard lines that produce these depths
(https://simswiki.info/wiki.php?title=Game_Help%3ATS4_Organizing_Custom_Content).

**Resource.cfg is user-editable.** Players extend depth by appending
`PackedFile */.../*.package` lines (EA Forums thread above). Consequence:
the app's deep-script warning depth is a **setting** (`script_depth_limit`,
default 1), not a hardcoded rule — a user with a customized Resource.cfg can
raise it.

**Corrupted Resource.cfg breaks loading.** Deleting it lets the game
regenerate a correct one (EA Forums "Sub Folders" thread). The app treats
`Resource.cfg` as a config-class file and never modifies it.

## Game-running detection

The Sims 4 has shipped executables named `TS4_x64.exe` (64-bit),
`TS4.exe` (legacy 32-bit), and `TS4_DX9_x64.exe` (legacy DX9). The detector
matches these case-insensitively (implementation note, `src-tauri/src/game.rs`).
Community tooling guidance agrees files shouldn't be touched while the game
runs: "Remember to close Sims 4 before using any mod manager. The game locks
files when running" (findingdulcinea.com mod-manager guide).

## Existing tools landscape (June 2026)

* **CurseForge for The Sims 4** — official EA/Maxis-partnered hub and mod
  manager (partnership announced late 2022); strongest for discovering and
  installing mods from its own platform. Windows-first.
* **Sims 4 Mod Manager (GameTimeDev / S4MM)** — popular fan-built organizer
  handling mods from any source; ad-financed, with CurseForge integration;
  community discussion notes the Overwolf/ad relationship
  (gametimedev.de/S4MM/, findingdulcinea.com, tumblr community posts).
* **Sims 4 Studio** — CC viewing/editing companion, not a manager.
* **Sims 4 Tray Importer** — identifies which CC a saved Sim/lot uses.

**The gap this product occupies.** Guides explicitly state that "current mod
managers focus on organization, not error detection… you need separate tools
to find conflicts or outdated content" (findingdulcinea.com
"Best Sims 4 Mod Manager"). None of the surveyed tools center verified,
reversible mutation: content-hash verification of every move, automatic
pre-change snapshots, corrupt-backup refusal, and a complete operation
journal. That safety layer — not discovery, not editing — is this product's
reason to exist.

## Notes on source quality

EA Forums threads and SimsWiki are community sources, not vendor
documentation; EA publishes no formal spec for Mods loading. Multiple
independent sources agree on every claim used above, and the one
behavior-critical number (script depth) is user-configurable in-app
precisely because the underlying game behavior is user-configurable.

---

# Phase 2 research — DBPF packages & conflicts

Verified before the parser was written; the parser implements exactly this
and refuses honestly on anything outside it.

## DBPF binary layout (as implemented in `core/src/dbpf.rs`)

**Header (96 bytes).** Magic `DBPF` @0x00; major u32 @0x04 (= 2); minor u32
@0x08; index entry count @0x24; index size @0x2C; index version @0x3C
(documented "always 3"); index position @0x40; little-endian throughout.
With zero entries, size and position are also zero. Source: the Sims4Tools
DBPF-Format wiki (https://github.com/Kuree/Sims4Tools/wiki/DBPF-Format);
the identical 2.x scheme is documented for Sims 3 on SimsWiki
(https://simswiki.info/Sims_3:DBPF).

**Version note.** The format documentation describes major 2 / minor 0;
Sims 4 packages in the wild are 2.1. The parser accepts minor 0 and 1 and
records which it saw.

**Index.** Begins with a `flags` u32; bits 0/1/2 hoist type / group /
instance-high as constants written once ("essentially a kind of
compression" — Sims4Tools wiki). Full entries are 32 bytes, minus 4 per
constant: [type][group][instance-hi][instance-lo][position][file size, with
bit 31 as a compression flag][mem size][compression u16][committed u16].
The 64-bit instance is `(high << 32) | low`. Field naming and the file-size
high bit are corroborated by an independent C reader
(https://github.com/ytaa/dbpf_reader). Unknown flag bits → the parser
refuses (`UnsupportedIndexFlags`) rather than guessing offsets.

## Resource type identifiers

From the community type tables (Sims4Tools "Packed File Types",
https://github.com/Kuree/Sims4Tools/wiki/Sims-4---Packed-File-Types and the
Sims4Group mirror) plus Maxis-posted resource templates indexed on SimsWiki
(https://simswiki.info/wiki.php?title=Tutorials%3ATS4_General_Modding):
CASP CAS Part 0x034AEECB; TONE Skin Tone 0x0354796A; STBL String Table
0x220557DA; DATA/SimData 0x545AC67A and binary Tuning 0x62E94D38 (Maxis
"Binary Tuning / Sim Data resources" post); COBJ Object Catalog 0x319E4F1D;
OBJD Object Definition 0xC0DB5AE7; Object Slot 0xD3044521; Footprint
0xD382BF57; CAS Geometry 0x015A1849; CAS Preset 0xEAA32ADD; Blend Geometry
0x067CAA11; Bone Delta 0x0355E0A6; Light 0x03B4C61D; Animation Clip
0x6B20C4F3; DDS image 0x3453CF95; PNG/thumbnail images 0x2F7D0004,
0x3C1AF1F2, 0x5B282D45. Unknown types display as raw hex — the map is
cosmetic, never load-bearing.

## Conflict semantics (drives the Phase 2 detector design)

**What a conflict is.** Two or more packages containing at least one
resource with the same type-group-instance key compete for the same slot;
only one wins. Community tooling detects exactly this ("conflicting
ResourceKeys" — Mod Conflict Detector by DmitryMalfatto; a worked example of
three mods colliding on one gfx instance:
https://www.patreon.com/posts/how-to-view-and-127588575).

**Load order is name-based.** "The game only cares about the name" — the
in-game mods screen is a non-editable listing, and players control priority
by renaming files/folders (same Patreon guide). Consequence: our Conflicts
screen orders a group's members by relative path (NOCASE) and labels the
last as the presumptive winner, with an explicit caveat that this is the
community-understood approximation, not an EA-documented guarantee.

**Noise policy** (why not every overlap deserves an alarm):

* *Identical content:* if two files carry byte-identical versions of a
  resource, there is no in-game difference — Better Exceptions deliberately
  omits these (https://thesimstree.com/en/blog/the-sims-tips/how-to-find-and-fix-mod-conflicts-in-sims-4-with-better-exceptions-conflict-detector.html).
  Index-level analysis cannot see content, so: byte-identical *files* are
  routed to the Duplicates feature instead of Conflicts, and the Conflicts
  screen states plainly that same-key-different-content vs
  same-key-same-content cannot be distinguished at this phase.
* *Intentional overrides:* overlaps within one mod/creator are usually by
  design; Better Exceptions badges these "Intentional Override". Ours will
  soften groups whose members share a mod link or parent folder.
* *Presentation-only types:* overlapping images/thumbnails change looks,
  not behavior ("you actually do not have to worry" for non-scripted CC
  resource sharing — community conflict-detector guidance). Image-only
  overlaps are shown as low severity, collapsed by default.
* *Scripts:* `.ts4script` conflicts are not detectable by resource keys at
  all — out of scope for Phase 2, stated honestly in the UI.
