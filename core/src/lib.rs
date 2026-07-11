//! plumbob-core — the filesystem-safety heart of the application.
//!
//! Everything in this crate is platform-portable, Tauri-free, and covered by
//! tests that run against temporary directories only (never a real Mods
//! folder). The Tauri shell in `src-tauri/` is a thin adapter over these
//! services; the SQLite layer binds their results to persistent records.
//!
//! Safety model (expanded in docs/SAFETY_MODEL.md):
//!
//! * Every path that will be read or mutated must resolve inside an approved
//!   [`paths::SafeRoot`], with symlinks resolved and `..` traversal rejected
//!   before the filesystem is touched.
//! * Every mutation verifies content hashes after the filesystem operation
//!   and rolls back what it can when verification fails.
//! * Destinations are never overwritten; collisions are surfaced, not decided.
//! * Every mutating engine reports per-item outcomes to an
//!   [`ops::JournalSink`] and never silently continues past a safety-critical
//!   failure.

pub mod db;
pub mod dbpf;
pub mod duplicates;
pub mod hashing;
pub mod ops;
pub mod paths;
pub mod product;
pub mod scan;
