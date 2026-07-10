//! Streaming SHA-256.
//!
//! Files are read through a fixed-size buffer so a 2 GB merged package costs
//! the same memory as a 2 KB tuning file. Nothing in this module loads whole
//! files into memory.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const HASH_BUF_SIZE: usize = 1024 * 1024;

/// Hash a file's contents, streaming. Returns lowercase hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_BUF_SIZE];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Streaming hash with cooperative cancellation and byte-level progress.
/// Returns `Ok(None)` when cancelled mid-file.
pub fn sha256_file_observed(
    path: &Path,
    cancel: &AtomicBool,
    mut on_bytes: impl FnMut(u64),
) -> std::io::Result<Option<String>> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_BUF_SIZE];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        on_bytes(n as u64);
    }
    Ok(Some(hex(&hasher.finalize())))
}

/// Hash an in-memory buffer. Used by tests and by verification of small
/// generated artifacts (manifests); never used for user mod files.
pub fn sha256_bytes(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn known_vectors() {
        assert_eq!(sha256_bytes(b""), EMPTY_SHA256);
        assert_eq!(sha256_bytes(b"abc"), ABC_SHA256);
    }

    #[test]
    fn file_hash_matches_buffer_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("small.package");
        fs::write(&f, b"abc").unwrap();
        assert_eq!(sha256_file(&f).unwrap(), ABC_SHA256);
    }

    #[test]
    fn large_file_streams_across_buffer_boundaries() {
        // 8 MiB + 3 bytes: exercises multiple full buffers plus a ragged tail.
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("large.package");
        let mut data = Vec::with_capacity(8 * 1024 * 1024 + 3);
        for i in 0..(8 * 1024 * 1024 + 3) {
            data.push((i % 251) as u8);
        }
        fs::write(&f, &data).unwrap();
        assert_eq!(sha256_file(&f).unwrap(), sha256_bytes(&data));
    }

    #[test]
    fn changed_content_changes_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("mutating.package");
        fs::write(&f, b"version one").unwrap();
        let first = sha256_file(&f).unwrap();
        fs::write(&f, b"version two").unwrap();
        let second = sha256_file(&f).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn missing_file_is_an_error_not_a_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("ghost.package");
        assert!(sha256_file(&missing).is_err());
    }

    #[test]
    fn cancellation_returns_none_and_progress_reports_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("cancellable.package");
        fs::write(&f, vec![7u8; 3 * 1024 * 1024]).unwrap();

        let cancel = AtomicBool::new(false);
        let mut seen = 0u64;
        let done = sha256_file_observed(&f, &cancel, |n| seen += n)
            .unwrap()
            .expect("not cancelled");
        assert_eq!(seen, 3 * 1024 * 1024);
        assert_eq!(done, sha256_file(&f).unwrap());

        let cancel_now = AtomicBool::new(true);
        let out = sha256_file_observed(&f, &cancel_now, |_| {}).unwrap();
        assert!(out.is_none());
    }
}
