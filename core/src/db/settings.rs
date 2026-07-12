//! Typed settings. The `settings` table is key/value TEXT, but nothing
//! outside this module touches it — all access goes through [`AppSettings`],
//! so the table never degenerates into an untyped dumping ground and every
//! key has exactly one parser and one serializer.

use super::{now_rfc3339, DbError};
use rusqlite::{params, Connection};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// The Sims 4 Mods folder chosen during onboarding.
    pub mods_folder: Option<PathBuf>,
    /// App data root (defaults beside the database at first run; the shell
    /// layer decides the platform-appropriate default).
    pub data_folder: Option<PathBuf>,
    pub backup_folder: Option<PathBuf>,
    pub quarantine_folder: Option<PathBuf>,
    /// Root-relative prefixes excluded from scans (stored normalized `/`).
    pub scan_excluded: Vec<PathBuf>,
    /// `.ts4script` deeper than this many levels below the root is flagged.
    pub script_depth_limit: usize,
    /// Hash during the scan walk (slower first scan) vs. as a separate pass.
    pub hash_on_scan: bool,
    /// Halt multi-step operations on first failure (safe default).
    pub stop_on_error: bool,
    pub theme: String,
    pub reduced_motion: bool,
    /// CurseForge API key for the future Patch Center. Lives only in the
    /// local database — never in the repo, never sent anywhere except the
    /// CurseForge API itself once Phase 3 ships.
    pub curseforge_api_key: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            mods_folder: None,
            data_folder: None,
            backup_folder: None,
            quarantine_folder: None,
            scan_excluded: Vec::new(),
            script_depth_limit: 1,
            hash_on_scan: false,
            stop_on_error: true,
            theme: "light".into(),
            reduced_motion: false,
            curseforge_api_key: None,
        }
    }
}

const K_MODS_FOLDER: &str = "mods_folder";
const K_DATA_FOLDER: &str = "data_folder";
const K_BACKUP_FOLDER: &str = "backup_folder";
const K_QUARANTINE_FOLDER: &str = "quarantine_folder";
const K_SCAN_EXCLUDED: &str = "scan_excluded";
const K_SCRIPT_DEPTH: &str = "script_depth_limit";
const K_HASH_ON_SCAN: &str = "hash_on_scan";
const K_STOP_ON_ERROR: &str = "stop_on_error";
const K_THEME: &str = "theme";
const K_REDUCED_MOTION: &str = "reduced_motion";
const K_CURSEFORGE_KEY: &str = "curseforge_api_key";

pub fn load(conn: &Connection) -> Result<AppSettings, DbError> {
    let mut s = AppSettings::default();
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (key, value) = row?;
        match key.as_str() {
            K_MODS_FOLDER => s.mods_folder = non_empty_path(&value),
            K_DATA_FOLDER => s.data_folder = non_empty_path(&value),
            K_BACKUP_FOLDER => s.backup_folder = non_empty_path(&value),
            K_QUARANTINE_FOLDER => s.quarantine_folder = non_empty_path(&value),
            K_SCAN_EXCLUDED => {
                s.scan_excluded = serde_json::from_str::<Vec<String>>(&value)
                    .unwrap_or_default()
                    .into_iter()
                    .map(PathBuf::from)
                    .collect()
            }
            K_SCRIPT_DEPTH => {
                if let Ok(v) = value.parse::<usize>() {
                    s.script_depth_limit = v;
                }
            }
            K_HASH_ON_SCAN => s.hash_on_scan = value == "true",
            K_STOP_ON_ERROR => s.stop_on_error = value == "true",
            K_THEME => s.theme = value,
            K_REDUCED_MOTION => s.reduced_motion = value == "true",
            K_CURSEFORGE_KEY => {
                s.curseforge_api_key = if value.is_empty() { None } else { Some(value) }
            }
            // Unknown keys are ignored (forward compatibility), never dropped.
            _ => {}
        }
    }
    Ok(s)
}

pub fn save(conn: &mut Connection, s: &AppSettings) -> Result<(), DbError> {
    let excluded: Vec<String> = s
        .scan_excluded
        .iter()
        .map(|p| super::rel_to_db_string(p))
        .collect();
    let excluded_json = serde_json::to_string(&excluded)
        .expect("Vec<String> serialization cannot fail");

    let pairs: Vec<(&str, String)> = vec![
        (K_MODS_FOLDER, path_str(&s.mods_folder)),
        (K_DATA_FOLDER, path_str(&s.data_folder)),
        (K_BACKUP_FOLDER, path_str(&s.backup_folder)),
        (K_QUARANTINE_FOLDER, path_str(&s.quarantine_folder)),
        (K_SCAN_EXCLUDED, excluded_json),
        (K_SCRIPT_DEPTH, s.script_depth_limit.to_string()),
        (K_HASH_ON_SCAN, s.hash_on_scan.to_string()),
        (K_STOP_ON_ERROR, s.stop_on_error.to_string()),
        (K_THEME, s.theme.clone()),
        (K_REDUCED_MOTION, s.reduced_motion.to_string()),
        (
            K_CURSEFORGE_KEY,
            s.curseforge_api_key.clone().unwrap_or_default(),
        ),
    ];

    let tx = conn.transaction()?;
    {
        let mut upsert = tx.prepare(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
        )?;
        let now = now_rfc3339();
        for (k, v) in &pairs {
            upsert.execute(params![k, v, now])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn non_empty_path(value: &str) -> Option<PathBuf> {
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

fn path_str(p: &Option<PathBuf>) -> String {
    p.as_ref()
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn defaults_are_safe() {
        let d = AppSettings::default();
        assert!(d.stop_on_error, "halt-on-failure must be the default");
        assert_eq!(d.script_depth_limit, 1);
        assert_eq!(d.theme, "light");
    }

    #[test]
    fn empty_database_loads_defaults() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(load(db.conn()).unwrap(), AppSettings::default());
    }

    #[test]
    fn full_round_trip_preserves_every_field() {
        let mut db = Database::open_in_memory().unwrap();
        let s = AppSettings {
            mods_folder: Some(PathBuf::from("C:/Users/M/Documents/EA/The Sims 4/Mods")),
            data_folder: Some(PathBuf::from("C:/Users/M/AppData/PlumbobLibraryData")),
            backup_folder: Some(PathBuf::from("D:/SimsBackups")),
            quarantine_folder: Some(PathBuf::from("D:/SimsQuarantine")),
            scan_excluded: vec![PathBuf::from("Disabled"), PathBuf::from("WIP/Drafts")],
            script_depth_limit: 2,
            hash_on_scan: true,
            stop_on_error: false,
            theme: "light".into(),
            reduced_motion: true,
            curseforge_api_key: Some("cf-test-key-not-real".into()),
        };
        save(db.conn_mut(), &s).unwrap();
        assert_eq!(load(db.conn()).unwrap(), s);
    }

    #[test]
    fn resaving_updates_rather_than_duplicates() {
        let mut db = Database::open_in_memory().unwrap();
        let mut s = AppSettings::default();
        save(db.conn_mut(), &s).unwrap();
        s.script_depth_limit = 3;
        save(db.conn_mut(), &s).unwrap();
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'script_depth_limit'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(load(db.conn()).unwrap().script_depth_limit, 3);
    }

    #[test]
    fn corrupt_values_fall_back_to_defaults_instead_of_failing() {
        let mut db = Database::open_in_memory().unwrap();
        db.conn_mut()
            .execute(
                "INSERT INTO settings (key, value, updated_at)
                 VALUES ('script_depth_limit', 'not-a-number', '2026'),
                        ('scan_excluded', '{broken json', '2026')",
                [],
            )
            .unwrap();
        let s = load(db.conn()).unwrap();
        assert_eq!(s.script_depth_limit, AppSettings::default().script_depth_limit);
        assert!(s.scan_excluded.is_empty());
    }
}
