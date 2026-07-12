//! CurseForge fingerprinting and update comparison.
//!
//! CurseForge identifies files by MurmurHash2 (32-bit, seed 1) computed
//! over the file's bytes with all whitespace removed — the bytes 0x09,
//! 0x0A, 0x0D, and 0x20. Matching their scheme byte-for-byte is the whole
//! feature: one bit off and every lookup misses. The implementation here is
//! cross-checked in tests against vectors computed by an independent
//! implementation.
//!
//! Fingerprinting a file is a **two-pass streaming read**: MurmurHash2
//! seeds itself with the input length, and the stripped length isn't known
//! until the bytes have been walked once. Merged CC packages run to
//! gigabytes, so flat memory beats buffering.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

const M: u32 = 0x5bd1_e995;
const R: u32 = 24;
const STRIP: [u8; 4] = [0x09, 0x0a, 0x0d, 0x20];
const CHUNK: usize = 64 * 1024;

#[inline]
fn stripped(b: u8) -> bool {
    !STRIP.contains(&b)
}

/// Incremental 32-bit MurmurHash2 (Appleby's original). Total length must
/// be known at construction — that is the algorithm, not a limitation here.
struct Murmur2 {
    h: u32,
    tail: [u8; 3],
    ntail: usize,
}

impl Murmur2 {
    fn new(seed: u32, total_len: u32) -> Self {
        Self {
            h: seed ^ total_len,
            tail: [0; 3],
            ntail: 0,
        }
    }

    fn mix(&mut self, mut k: u32) {
        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);
        self.h = self.h.wrapping_mul(M);
        self.h ^= k;
    }

    fn write(&mut self, mut data: &[u8]) {
        if self.ntail > 0 {
            while self.ntail < 4 && !data.is_empty() {
                if self.ntail < 3 {
                    self.tail[self.ntail] = data[0];
                } else {
                    // Fourth byte completes a word built from the tail.
                    let k = u32::from(self.tail[0])
                        | u32::from(self.tail[1]) << 8
                        | u32::from(self.tail[2]) << 16
                        | u32::from(data[0]) << 24;
                    self.mix(k);
                    self.ntail = 0;
                    data = &data[1..];
                    break;
                }
                self.ntail += 1;
                data = &data[1..];
            }
            if self.ntail == 4 {
                unreachable!();
            }
            if self.ntail > 0 && data.is_empty() {
                return;
            }
        }
        let words = data.len() / 4;
        for w in 0..words {
            let i = w * 4;
            let k = u32::from(data[i])
                | u32::from(data[i + 1]) << 8
                | u32::from(data[i + 2]) << 16
                | u32::from(data[i + 3]) << 24;
            self.mix(k);
        }
        let rest = &data[words * 4..];
        self.tail[..rest.len()].copy_from_slice(rest);
        self.ntail = rest.len();
    }

    fn finish(mut self) -> u32 {
        if self.ntail >= 3 {
            self.h ^= u32::from(self.tail[2]) << 16;
        }
        if self.ntail >= 2 {
            self.h ^= u32::from(self.tail[1]) << 8;
        }
        if self.ntail >= 1 {
            self.h ^= u32::from(self.tail[0]);
            self.h = self.h.wrapping_mul(M);
        }
        self.h ^= self.h >> 13;
        self.h = self.h.wrapping_mul(M);
        self.h ^= self.h >> 15;
        self.h
    }
}

/// One-shot MurmurHash2 over an in-memory slice.
pub fn murmur2(data: &[u8], seed: u32) -> u32 {
    let mut m = Murmur2::new(seed, data.len() as u32);
    m.write(data);
    m.finish()
}

/// The CurseForge fingerprint of a file: MurmurHash2(seed 1) over the file
/// with whitespace stripped. Two streaming passes; flat memory.
pub fn curse_fingerprint_file(path: &Path) -> io::Result<u32> {
    // Pass one: how long is the stripped content?
    let mut reader = BufReader::with_capacity(CHUNK, File::open(path)?);
    let mut buf = vec![0u8; CHUNK];
    let mut len: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        len += buf[..n].iter().filter(|&&b| stripped(b)).count() as u64;
    }
    // Pass two: hash the stripped stream.
    let mut reader = BufReader::with_capacity(CHUNK, File::open(path)?);
    let mut hasher = Murmur2::new(1, len as u32);
    let mut kept = Vec::with_capacity(CHUNK);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        kept.clear();
        kept.extend(buf[..n].iter().copied().filter(|&b| stripped(b)));
        hasher.write(&kept);
    }
    Ok(hasher.finish())
}

/// Is the mod's latest file an update over the one the user has? Same file
/// id means current; otherwise the dates decide. CurseForge emits RFC 3339
/// with *varying* sub-second precision ("…36.31Z" vs "…36.317Z"), and 'Z'
/// outranks digits lexically — so the fractional parts are compared padded,
/// not as raw strings. The test below exists because the naive comparison
/// looked right and wasn't.
pub fn update_available(
    matched_file_id: i64,
    matched_date: &str,
    latest_file_id: i64,
    latest_date: &str,
) -> bool {
    latest_file_id != matched_file_id && date_newer(matched_date, latest_date)
}

/// Is `candidate` strictly later than `base`? Public so callers picking
/// a latest-file among several can share the same date semantics.
pub fn date_newer(base: &str, candidate: &str) -> bool {
    fn split(s: &str) -> (&str, &str) {
        let s = s
            .strip_suffix('Z')
            .or_else(|| s.strip_suffix("+00:00"))
            .unwrap_or(s);
        match s.split_once('.') {
            Some((whole, frac)) => (whole, frac),
            None => (s, ""),
        }
    }
    let (bw, bf) = split(base);
    let (cw, cf) = split(candidate);
    if cw != bw {
        return cw > bw;
    }
    let width = bf.len().max(cf.len());
    let pad = |f: &str| format!("{f:0<width$}");
    pad(cf) > pad(bf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn murmur2_matches_independent_vectors() {
        let vectors: [(&[u8], u32, u32); 8] = [
            (&[0u8; 0][..], 1, 1540447798),
            (&[97][..], 1, 626045324),
            (&[97, 98][..], 1, 1692487918),
            (&[97, 98, 99][..], 1, 1621425345),
            (&[97, 98, 99, 100][..], 1, 3376380438),
            (&[104, 101, 108, 108, 111][..], 1, 2788266382),
            (&[84, 104, 101, 32, 113, 117, 105, 99, 107, 32, 98, 114, 111, 119, 110, 32, 102, 111, 120, 32, 106, 117, 109, 112, 115, 32, 111, 118, 101, 114, 32, 116, 104, 101, 32, 108, 97, 122, 121, 32, 100, 111, 103][..], 1, 504383975),
            (&[97, 98, 99, 100, 120][..], 1, 380558390)
        ];
        for (data, seed, expected) in vectors {
            assert_eq!(murmur2(data, seed), expected, "input {data:?}");
        }
    }

    #[test]
    fn incremental_writes_equal_one_shot_across_odd_boundaries() {
        let data: Vec<u8> = (0..=255u8).cycle().take(200_003).collect();
        let whole = murmur2(&data, 1);
        for split in [1usize, 2, 3, 4, 5, 7, 64 * 1024 - 1, 100_001] {
            let mut m = Murmur2::new(1, data.len() as u32);
            for chunk in data.chunks(split) {
                m.write(chunk);
            }
            assert_eq!(m.finish(), whole, "split {split}");
        }
    }

    #[test]
    fn fingerprint_strips_exactly_the_four_whitespace_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x.package");
        fs::write(&p, b"ab	 c
d x").unwrap();
        assert_eq!(curse_fingerprint_file(&p).unwrap(), 380558390);
        assert_eq!(
            curse_fingerprint_file(&p).unwrap(),
            murmur2(b"abcdx", 1)
        );
    }

    #[test]
    fn file_fingerprint_equals_in_memory_over_large_mixed_content() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("big.package");
        let data: Vec<u8> = (0..200_000u32).map(|i| (i * 31 % 251) as u8).collect();
        fs::write(&p, &data).unwrap();
        let stripped: Vec<u8> = data.iter().copied().filter(|&b| stripped(b)).collect();
        assert_eq!(
            curse_fingerprint_file(&p).unwrap(),
            murmur2(&stripped, 1)
        );
    }

    #[test]
    fn update_comparison_speaks_curseforge_dates() {
        assert!(update_available(1, "2026-01-01T00:00:00Z", 2, "2026-02-01T00:00:00Z"));
        assert!(!update_available(2, "2026-02-01T00:00:00Z", 2, "2026-02-01T00:00:00Z"));
        // Differing sub-second precision still orders correctly as strings.
        assert!(update_available(1, "2026-01-01T00:00:36.31Z", 2, "2026-01-01T00:00:36.317Z"));
        // The user having something newer than the index is not an update.
        assert!(!update_available(9, "2026-03-01T00:00:00Z", 2, "2026-02-01T00:00:00Z"));
    }
}
