# Motherlode Manager — Design Specification

**Status: binding.** The five approved mockups (Motherlode logo,
UIMockup01–04) are the exact visual target for this product — not
inspiration. Every existing screen is styled to this spec; every future
screen must be built to it. Deviations require an explicit product decision
recorded here.

## Translation rule (the one honest caveat)

The mockups are AI-rendered images; the product is a real interface built
from CSS and SVG so it stays crisp at every size and DPI. The translation
is therefore *faithful vector reproduction*: painterly bloom becomes
drop-shadows and gradients, embossed gold becomes hairlines at specified
opacities, sparkles become SVG glyphs. Raster art is used only where it is
the actual asset (the logo). Layout, hierarchy, spacing intent, color, and
typography follow the mockups exactly.

## Identity

* **Name:** Motherlode Manager (centralized in exactly three literals:
  `core/src/product.rs`, `src/lib/product.ts`, `tauri.conf.json`).
* **Tagline:** "Your mods. Organized. Precious."
* **Logo & app icon:** the treasure-chest-and-crystal art, one mark
  everywhere — sidebar lockup, onboarding, notices, Windows icon set,
  taskbar, and installer. True-alpha master at
  `src/assets/logo-master.png` (regenerated 2026-07-11 from the corrected
  clean-background export). Always floats on true transparency — never
  on a tile.

## Palette (CSS custom properties in `src/styles/tokens.css`)

Canvas cream `#f4eddc` · surface `#fffdf6` · soft `#f6efde` ·
sidebar gradient `#154a33 → #0c3021` · primary emerald `#1e6647`
(deep `#14503a`) · antique gold `#c9a45c` (deep `#8a6b2e`) ·
ink `#22302a` / `#4c594f` / `#7b8577` · borders `#e7dcc0` / `#d3c6a4` ·
sidebar ink `#f3edda` (muted `#c7bc97`) · success `#2f7d4f` ·
warning `#8a6420` · danger `#a94438`. Token *names* are semantic and
stable; only values change with themes.

## Chrome rules (v7 treatment, approved 2026-07-11)

* **Cartouche frames:** every major surface — sidebar, hero cards, dark
  panels — wears the gilded cartouche: engraved double edge lines flowing
  into scroll-work corners (twin volutes, acanthus sweep, rosette), with
  spliced jewel finials at edge midpoints on hero elements. Frames are
  zero-layout overlays (`CartoucheFrame` in `ui.tsx`).
  *Engineering constraints, verified by measurement:* frames are built
  from primitives (edge lines as divs whose engraved relief comes from
  background + offset shadows; corners as fixed-size SVGs) — **never**
  CSS `border-image` for long edges, and **never** a `filter` on the
  frame layer; both cause rasterizers to drop edge segments on large
  elements (15% vs 96–100% edge coverage in testing).
* **Texture:** a generated grain tile overlays the canvas (multiply),
  cards (multiply), and dark surfaces (screen). Dark surfaces also carry
  the diamond lattice and a slow light sweep (reduced-motion safe).
* **Sidebar:** floating gilded panel on the padded canvas — radial-lit
  emerald, full cartouche frame with finials, 64-star twinkling field,
  20-node constellation, two ghosted crystal watermarks, light sweep.
  Brand lockup: 104px mark over a green halo, first word of the product
  name in display serif 26px, remaining words letterspaced gold between
  ✦ marks, glowing divider. Nav labels in the display serif; hover gets
  the gold outline glow; the active pill is translucent emerald with a
  metallic gradient rim so the constellation reads through it.
* **Typography:** Playfair Display (bundled via @fontsource) — page
  titles 32px bold with an ivory lift, section titles small-caps serif
  led by a glowing ✦ and trailed by a glowing gold rule, stat values in
  serif.
* **Cards:** lit gradient surface, ivory inner bevel, deep ambient
  shadow, grain overlay, cartouche frame by default (`frame={false}` to
  opt out in dense contexts; `finials` for hero cards).
* **Stat tiles:** gilded-edge tiles, centered — beveled gold medallion
  (radial metal, specular sweep, engraved icon) above a small-caps label
  and a serif value.
* **Buttons:** the emerald jewel (gradient, inner highlight, green
  lift-shadow). Icons remain the hand-drawn 24×24 stroke set.
* **Scrollbars:** metallic gradient thumb, gold on hover.

## Future screens — exact parameters (build when the data exists)

* **All Mods (Phase 3):** thumbnail + name + game + category + version +
  status ("Up to date" / gold "Update Available") + added date, gold-ruled
  table header, search + Filter/Category/Game/Status controls, emerald
  "Add Mod" action — per UIMockup01/04. Thumbnails come from the already-
  indexed package image resources; versions/status from provenance.
* **Mod Details drawer (Phase 3):** hero image, author/added/size, tag
  chips, Update / Open Folder actions, Overview·Files·Notes·Dependencies·
  History tabs — per UIMockup02/03.
* **Collections & Categories:** sidebar entries and screens over the
  schema that has existed since migration 0001.
* **Calendar (Patch Center era):** month grid with backup / mod-update /
  maintenance event chips — per UIMockup02/03.
* **My Vault, Maintenance, Analytics:** styled to this chrome when their
  features land.

## Honesty carve-outs (recorded product decisions)

* **"Collection Value $" and value charts:** no honest data source exists
  for a dollar value of free community mods. The dashboard slot is built
  instead as *Collection Stats* — real numbers (files, size, creators,
  growth from scan history). Same visual weight, true data.
* **"Premium Member" / accounts / avatars:** a business-model decision,
  not a skin; deferred until explicitly decided. No UI pretends accounts
  exist.
* Fake data is never rendered. A screen ships when its data is real.
