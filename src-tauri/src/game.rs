//! "Is The Sims 4 running?" — the guard every mutating operation checks
//! before touching the Mods folder. Moving files while the game holds them
//! open corrupts saves and crashes sessions; refusing is the only safe
//! answer.

use sysinfo::{ProcessesToUpdate, System};

/// Executable names The Sims 4 has shipped under: 64-bit, legacy 32-bit, and
/// the legacy DX9 build. Compared case-insensitively; extensionless forms
/// cover non-Windows environments.
const SIMS_PROCESS_NAMES: &[&str] = &[
    "ts4_x64.exe",
    "ts4.exe",
    "ts4_dx9_x64.exe",
    "ts4_x64",
    "ts4",
];

pub fn sims_running() -> bool {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes().values().any(|process| {
        let name = process.name().to_string_lossy().to_ascii_lowercase();
        SIMS_PROCESS_NAMES.iter().any(|candidate| name == *candidate)
    })
}
