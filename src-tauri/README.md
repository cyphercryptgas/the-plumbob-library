# src-tauri — desktop shell

The Tauri 2 shell: a deliberately thin, typed command boundary over the
tested `plumbob-core` crate. It compiles in CI (up-to-date stable Rust on a
Windows runner via `.github/workflows/release.yml`), not in constrained
build containers — all filesystem/database logic lives in `../core`, which
runs its 92-test suite on both Linux and Windows in `ci.yml`.

What lives here:

* `src/main.rs` — builder, dialog plugin, state init, runtime window title
  from the centralized product constant, and the full command registry.
* `src/state.rs` — `Arc`-shared database handle + scan cancellation flags.
* `src/game.rs` — "is The Sims 4 running?" via `sysinfo`; every mutating
  command refuses while the game is open.
* `src/service.rs` — orchestration: root resolution (backup/quarantine
  inside the Mods folder is refused), the scan → reconcile → hash →
  duplicates pipeline with `scan://progress` / `scan://completed` events,
  and the snapshot-first quarantine/restore flows.
* `src/commands.rs` — 22 typed commands; long operations run on blocking
  threads so the interface never freezes.
* `capabilities/default.json` — `core:default` + `dialog:default` only. The
  webview gets no generic filesystem permissions; every mutation goes
  through a typed Rust command.
* `tauri.conf.json` — NSIS-only bundle, per-user install (no admin prompt).
  This file is one of exactly three sanctioned locations for the product
  name (with `core/src/product.rs` and `src/lib/product.ts`).

Installer per commit: every push to `main` runs the release workflow; the
setup `.exe` is downloadable from that run's **Artifacts** section.
