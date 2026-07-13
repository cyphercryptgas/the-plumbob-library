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
    fn our_murmur2_is_certified_against_the_ecosystem_crate() {
        // The `murmur2` crate is what furse (and therefore ferium) ship to
        // real CurseForge users. Agreement across sizes, seeds, and every
        // tail length certifies the hash; disagreement anywhere would have
        // explained a zero-match field result.
        let mut state: u32 = 0x1234_5678;
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        };
        for len in (0usize..64).chain([65, 127, 128, 129, 1000, 65_535, 65_536, 200_003]) {
            let data: Vec<u8> = (0..len).map(|_| next()).collect();
            for seed in [0u32, 1, 0xDEAD_BEEF] {
                assert_eq!(
                    murmur2(&data, seed),
                    murmur2::murmur2(&data, seed),
                    "len {len} seed {seed}"
                );
            }
        }
        // And the full CurseForge pipeline shape: strip, then hash, seed 1 —
        // exactly furse::cf_fingerprint.
        let raw: Vec<u8> = (0..50_000usize).map(|_| next()).collect();
        let stripped: Vec<u8> = raw
            .iter()
            .copied()
            .filter(|b| !matches!(b, 9 | 10 | 13 | 32))
            .collect();
        assert_eq!(
            murmur2(&stripped, 1),
            murmur2::murmur2(&stripped, 1)
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

// ---------------------------------------------------------------------------
// Tier-2: name-based matching
// ---------------------------------------------------------------------------

const STOP_TOKENS: [&str; 9] = [
    "by", "the", "and", "for", "mod", "mods", "ts4", "sims4", "ver",
];

fn tokenize(name: &str) -> Vec<String> {
    let mut cleaned = String::with_capacity(name.len() + 8);
    let mut depth = 0i32;
    let mut prev: Option<char> = None;
    let mut prev2: Option<char> = None;
    for ch in name.chars() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            '_' | '-' | '.' | '+' | '~' | '\'' | ',' | '!' | '&' => {
                cleaned.push(' ');
                prev2 = prev;
                prev = Some(' ');
                continue;
            }
            c => {
                // CamelCase seams: aB → a B, and ABc → A Bc (acronym end).
                let lower_to_upper =
                    matches!(prev, Some(p) if p.is_lowercase() || p.is_ascii_digit())
                        && c.is_uppercase();
                let acronym_end = matches!(prev, Some(p) if p.is_uppercase())
                    && matches!(prev2, Some(q) if q.is_uppercase())
                    && c.is_lowercase();
                if lower_to_upper {
                    cleaned.push(' ');
                } else if acronym_end {
                    let kept = cleaned.pop();
                    cleaned.push(' ');
                    if let Some(k) = kept {
                        cleaned.push(k);
                    }
                }
                cleaned.push(c);
                prev2 = prev;
                prev = Some(c);
                continue;
            }
        }
        prev2 = prev;
        prev = None;
    }
    cleaned
        .split_whitespace()
        .map(|t| t.to_lowercase())
        .filter(|t| {
            t.len() > 1
                && !STOP_TOKENS.contains(&t.as_str())
                && !t.chars().all(|c| c.is_ascii_digit())
                && !is_versionish(t)
                && !is_hexish(t)
        })
        .collect()
}

fn is_versionish(t: &str) -> bool {
    let core = t.strip_prefix('v').unwrap_or(t);
    !core.is_empty()
        && core.chars().all(|c| c.is_ascii_digit() || c == '.')
        && core.chars().any(|c| c.is_ascii_digit())
}

fn is_hexish(t: &str) -> bool {
    t.len() >= 6 && t.chars().all(|c| c.is_ascii_hexdigit())
}

/// A CurseForge search term derived from a mod's file name, or `None`
/// when the name carries too little language to search responsibly
/// (hash-named CC, single-token stubs).
pub fn search_term(file_name: &str) -> Option<String> {
    let stem = file_name
        .strip_suffix(crate::scan::DISABLED_SUFFIX)
        .unwrap_or(file_name);
    let stem = stem
        .strip_suffix(".package")
        .or_else(|| stem.strip_suffix(".ts4script"))
        .unwrap_or(stem);
    let tokens = tokenize(stem);
    let alpha: usize = tokens.iter().map(|t| t.chars().filter(|c| c.is_alphabetic()).count()).sum();
    if tokens.len() < 2 || alpha < 6 {
        return None;
    }
    Some(tokens.into_iter().take(6).collect::<Vec<_>>().join(" "))
}

/// How well a candidate mod (name + author names) covers the local term's
/// tokens: the fraction of term tokens found among the candidate's.
pub fn name_similarity(term: &str, mod_name: &str, authors: &[String]) -> f32 {
    let term_tokens: Vec<String> = term.split_whitespace().map(str::to_string).collect();
    if term_tokens.is_empty() {
        return 0.0;
    }
    let mut candidate = tokenize(mod_name);
    for a in authors {
        candidate.extend(tokenize(a));
    }
    let hits = term_tokens
        .iter()
        .filter(|t| candidate.iter().any(|c| c == *t))
        .count();
    hits as f32 / term_tokens.len() as f32
}

/// Accept a name match only when it covers most of the term with at least
/// two shared tokens — approximate, and labeled so everywhere it appears.
pub fn accept_name_match(term: &str, mod_name: &str, authors: &[String]) -> Option<f32> {
    let sim = name_similarity(term, mod_name, authors);
    let shared = (sim * term.split_whitespace().count() as f32).round() as usize;
    if sim >= 0.6 && shared >= 2 {
        Some(sim)
    } else {
        None
    }
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Name acceptance, sharpened by attribution. With a known file creator:
/// candidates whose CurseForge authors match it are accepted at lower
/// similarity with boosted confidence; candidates by *someone else* face
/// a stricter bar — the guard against a generic term matching the wrong
/// creator's work. Without attribution, the legacy rule applies.
pub fn accept_name_match_attributed(
    term: &str,
    mod_name: &str,
    authors: &[String],
    file_creator: Option<&str>,
) -> Option<f32> {
    let Some(creator) = file_creator.filter(|c| !c.is_empty()) else {
        return accept_name_match(term, mod_name, authors);
    };
    let sim = name_similarity(term, mod_name, authors);
    let shared = (sim * term.split_whitespace().count() as f32).round() as usize;
    let ck = squash(creator);
    let author_hit = authors.iter().any(|a| {
        let ak = squash(a);
        ak == ck || (ak.len() >= 4 && ck.contains(&ak)) || (ck.len() >= 4 && ak.contains(&ck))
    });
    if author_hit {
        if sim >= 0.4 && shared >= 1 {
            Some((sim + 0.25).min(1.0))
        } else {
            None
        }
    } else if sim >= 0.85 && shared >= 3 {
        // Author mismatch with a known creator: only a distinctive name
        // survives — aliases differ, but two generic tokens don't earn
        // someone else's byline.
        Some(sim)
    } else {
        None
    }
}

/// A creator-anchored search term for files whose names alone are too
/// thin to query: one content token plus a byline becomes searchable
/// ("hair" by Simancholy → "simancholy hair"). Names with no language at
/// all stay skipped, byline or not.
pub fn search_term_with_creator(
    file_name: &str,
    creator_display: Option<&str>,
) -> Option<String> {
    if let Some(t) = search_term(file_name) {
        return Some(t);
    }
    let creator = creator_display.filter(|c| !c.is_empty())?;
    let stem = file_name.strip_suffix(crate::scan::DISABLED_SUFFIX).unwrap_or(file_name);
    let stem = stem
        .strip_suffix(".package")
        .or_else(|| stem.strip_suffix(".ts4script"))
        .unwrap_or(stem);
    let tokens = tokenize(stem);
    if tokens.is_empty() {
        return None;
    }
    let mut all = tokenize(creator);
    all.extend(tokens);
    if all.len() < 2 {
        return None;
    }
    Some(all.into_iter().take(6).collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod name_tests {
    use super::*;

    #[test]
    fn search_terms_extract_language_and_skip_noise() {
        assert_eq!(
            search_term("KUTTOE_NewEmotionalTraits.package").as_deref(),
            Some("kuttoe new emotional traits")
        );
        assert_eq!(
            search_term("mc_cmd_center_2025.3.ts4script").as_deref(),
            Some("mc cmd center")
        );
        assert_eq!(
            search_term("[SIMCREDIBLE] LivingSuite Sofa v2.package").as_deref(),
            Some("living suite sofa")
        );
        assert_eq!(
            search_term("UICheats_v1.42.package.off").as_deref(),
            Some("ui cheats")
        );
        // Hash-named CC and stubs are skipped, not guessed at.
        assert_eq!(search_term("7cbcd7a91f3e.package"), None);
        assert_eq!(search_term("hair.package"), None);
    }

    #[test]
    fn attribution_boosts_confirmed_authors_and_guards_strangers() {
        // Author confirmed: a modest name match by the right creator
        // clears the bar with boosted confidence.
        let conf = accept_name_match_attributed(
            "kuttoe emotional traits overhaul extra",
            "Emotional Overhaul",
            &["Kuttoe".to_string()],
            Some("kuttoe"),
        )
        .expect("confirmed author accepted");
        assert!(conf > 0.6, "boosted: {conf}");
        // Wrong author + known creator: a decent name match is refused.
        assert!(accept_name_match_attributed(
            "alana skirt",
            "Alana Mini Skirt",
            &["someoneelse".to_string()],
            Some("arethabee"),
        )
        .is_none());
        // No attribution: the legacy rule, unchanged.
        assert!(accept_name_match_attributed(
            "mc cmd center",
            "MC Command Center",
            &["Deaderpool".to_string()],
            None,
        )
        .is_some());
    }

    #[test]
    fn creator_anchored_terms_rescue_thin_names() {
        assert_eq!(
            search_term_with_creator("hair.package", Some("Simancholy")).as_deref(),
            Some("simancholy hair")
        );
        assert_eq!(
            search_term_with_creator("7cbcd7a91f3e.package", Some("Simancholy")),
            None,
            "no language means no search, byline or not"
        );
        assert_eq!(
            search_term_with_creator("KUTTOE_NewEmotionalTraits.package", None).as_deref(),
            Some("kuttoe new emotional traits"),
            "rich names take the ordinary path"
        );
    }

    #[test]
    fn similarity_accepts_real_pairs_and_rejects_strangers() {
        assert!(accept_name_match(
            "kuttoe new emotional traits",
            "New Emotional Traits",
            &["Kuttoe".to_string()]
        )
        .is_some());
        assert!(accept_name_match(
            "mc cmd center",
            "MC Command Center",
            &["Deaderpool".to_string()]
        )
        .is_some());
        assert!(accept_name_match(
            "ui cheats extension",
            "UI Cheats Extension",
            &["weerbesu".to_string()]
        )
        .is_some());
        assert!(accept_name_match(
            "livingsuite sofa",
            "Cottage Kitchen Set",
            &["SIMcredible".to_string()]
        )
        .is_none());
    }
}
