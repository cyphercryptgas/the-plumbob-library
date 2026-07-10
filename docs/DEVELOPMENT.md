# Development Guide

Built for a GitHub-web-UI + CI workflow: no local toolchain required.

## The loop

1. Edit or upload files through the GitHub web interface (full-file
   replacements).
2. Every push to `main` triggers two workflows:
   * **CI** (`ci.yml`) — the 92-test safety-core suite on **Ubuntu and
     Windows** runners, plus frontend typecheck + production build.
   * **Windows Installer** (`release.yml`) — full desktop build via
     `tauri-apps/tauri-action@v1`; the NSIS `…-setup.exe` appears in that
     run's **Artifacts** section (Actions tab → the run → Artifacts).
3. Install the artifact, validate the GUI, report issues with screenshots.

## Uploading this repository via the GitHub web UI

* Drag whole **folders** into "Upload files" — subfolder structure (including
  `.github/`) is preserved.
* Practical batches: root files → `core/` → `src/` → `src-tauri/` →
  `.github/` → `docs/` → `fixtures/`. Keep each drag under ~100 files.
* **Must be committed:** `package-lock.json` (CI's `npm ci` requires it) and
  `core/Cargo.lock`. `src-tauri/` has no lockfile yet — the first CI run
  resolves and builds with current stable; that's expected.
* Never commit `node_modules/`, `dist/`, `core/target/`, `src-tauri/target/`,
  `src-tauri/gen/` (all gitignored).

## Local commands (optional, any machine with the toolchains)

```bash
cargo test --manifest-path core/Cargo.toml   # Rust ≥ 1.75
npm ci && npm run typecheck && npm run build # Node ≥ 20
npm run tauri dev                            # full desktop shell (Rust ≥ 1.77 + platform webview deps)
```

## First-run validation (important)

Point onboarding at a **test** Mods folder first — generate one with
`fixtures/generate_demo_library.py` (or use the prebuilt demo zip) — and
exercise scan → duplicates → quarantine → restore there before ever
selecting a real library. The engines are tested, but the GUI's first
runtime pass deserves a sandbox.

## Renaming the product

Edit exactly three files: `core/src/product.rs`, `src/lib/product.ts`,
`src-tauri/tauri.conf.json` (productName + window title). Nothing else may
hardcode the name; `docs/` and `README.md` prose mention it normally.

## Plateau history

| Plateau | Delivered |
|---|---|
| 1 | Safety core (paths, scan, hashing, duplicates, ops engines) — 50 tests |
| 2 | SQLite layer (migrations, reconciliation, journal, records, settings) — 92 tests |
| 3 | Tauri 2 shell: 22 typed commands, game guard, installer-per-commit workflow |
| 4 | Typed IPC layer, app shell, Onboarding / Dashboard / Settings |
| 5 | Library, Duplicate Center, Quarantine, Backups, Activity + shared quarantine flow |
| 6 | Docs, research, fixtures, final validation, deliverable |
