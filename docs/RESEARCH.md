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
