//! Managed application state. `Arc`-wrapped so long-running work can move
//! clones onto blocking threads while commands keep borrowing the state.

use plumbob_core::db::Database;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub struct AppState {
    /// Single connection behind a mutex: mutations are exclusive by design —
    /// two overlapping bulk operations on one Mods folder is exactly the kind
    /// of situation this app exists to prevent.
    pub db: Arc<Mutex<Database>>,
    pub data_dir: PathBuf,
    pub cancel_scan: Arc<AtomicBool>,
    pub scan_in_progress: Arc<AtomicBool>,
}

impl AppState {
    pub fn initialize(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(data_dir)?;
        let db = Database::open(&data_dir.join("plumbob.db"))?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            data_dir: data_dir.to_path_buf(),
            cancel_scan: Arc::new(AtomicBool::new(false)),
            scan_in_progress: Arc::new(AtomicBool::new(false)),
        })
    }
}
