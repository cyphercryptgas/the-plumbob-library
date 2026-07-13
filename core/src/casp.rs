//! CAS part (CASP) payload reading — just enough to learn each part's
//! BodyType, which drives the CAS subcategory chips. The layout parsed
//! here is the community-documented field sequence:
//!
//!   u32 version · u32 dataSize · 7-bit-length string name ·
//!   f32 sortPriority · u16 swatchOrder · u32 outfitGroup · u32 bodyType
//!
//! Field order can shift across CASP versions, so parsing is gated to a
//! broad observed version band and anything outside it — or any read that
//! runs off the end — honestly yields `None` rather than a wrong chip.
//! The tests construct CASP bytes to this exact layout; the field is the
//! falsifier, and an incorrect chip in the wild indicts the band, not the
//! reader.

/// The CASP resource type id (see `resource_type_name`).
pub const CASP_TYPE: u32 = 0x034A_EECB;

/// Versions this reader accepts. Outside the band → `None`.
const VERSION_MIN: u32 = 0x20;
const VERSION_MAX: u32 = 0x7F;

struct Reader<'a> {
    d: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u16(&mut self) -> Option<u16> {
        let v = self.d.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_le_bytes([v[0], v[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        let v = self.d.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        self.d.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(())
    }
    /// .NET-style 7-bit-encoded length prefix, then that many bytes.
    fn string7(&mut self) -> Option<()> {
        let mut len: usize = 0;
        let mut shift = 0u32;
        loop {
            let b = *self.d.get(self.pos)?;
            self.pos += 1;
            len |= ((b & 0x7F) as usize) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return None;
            }
        }
        self.skip(len)
    }
}

/// A concrete way to read BodyType: which prefix alignment to use, and
/// how many bytes past the name the field sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scheme {
    /// How many u32 fields sit between dataSize and the name.
    pub pre_u32s: u8,
    pub offset: usize,
}

/// The plausible BodyType range (the community enum tops out in the low
/// forties). Anything outside is a misread, never a category.
const BODY_TYPE_MAX: u32 = 43;

fn prefix_cursor(payload: &[u8], pre_u32s: u8) -> Option<usize> {
    let mut r = Reader { d: payload, pos: 0 };
    let version = r.u32()?;
    if !(VERSION_MIN..=VERSION_MAX).contains(&version) {
        return None;
    }
    r.u32()?; // dataSize / TGI offset
    for _ in 0..pre_u32s {
        r.u32()?;
    }
    r.string7()?; // name
    Some(r.pos)
}

/// Read the u32 a scheme points at, gated to the plausible range.
pub fn body_type_with(payload: &[u8], scheme: Scheme) -> Option<u32> {
    let cursor = prefix_cursor(payload, scheme.pre_u32s)?;
    let mut r = Reader { d: payload, pos: cursor };
    r.skip(scheme.offset)?;
    let v = r.u32()?;
    if (1..=BODY_TYPE_MAX).contains(&v) {
        Some(v)
    } else {
        None
    }
}

/// Elect the scheme by evidence: across sample payloads, the true
/// BodyType column is the position whose values are overwhelmingly
/// in-range, diverse (a real wardrobe has hair *and* tops *and* shoes),
/// and not a constant. Returns `None` when nothing qualifies — better
/// unlabeled than wrong.
pub fn calibrate(samples: &[&[u8]]) -> Option<Scheme> {
    calibrate_verbose(samples).0
}

/// The election plus a one-line verdict for the diagnostics card —
/// either the winning scheme with its numbers, or the nearest miss.
pub fn calibrate_verbose(samples: &[&[u8]]) -> (Option<Scheme>, String) {
    let files: Vec<Vec<&[u8]>> = samples.iter().map(|p| vec![*p]).collect();
    match elect(&files) {
        Elect::Won(s, cov, dis) => (
            Some(s),
            format!(
                "elected pre={} off={} · coverage {:.0}% · {} distinct types",
                s.pre_u32s,
                s.offset,
                cov * 100.0,
                dis
            ),
        ),
        Elect::Miss(s, cov, why) => (
            None,
            format!(
                "no scheme elected ({why}) · best coverage {:.0}% at pre={} off={}",
                cov * 100.0,
                s.pre_u32s,
                s.offset
            ),
        ),
        Elect::TooFew(n) => (None, format!("only {n} readable samples")),
    }
}

enum Elect {
    Won(Scheme, f32, usize),
    Miss(Scheme, f32, &'static str),
    TooFew(usize),
}

/// Values a real CC wardrobe concentrates in: the core garment slots.
const CORE_SLOTS: std::ops::RangeInclusive<u32> = 1..=8;

fn elect(files: &[Vec<&[u8]>]) -> Elect {
    if files.len() < 8 {
        return Elect::TooFew(files.len());
    }
    let mut best: Option<(Scheme, f32, usize)> = None;
    let mut nearest: Option<(Scheme, f32, &'static str)> = None;
    for pre_u32s in 0u8..=2 {
        for offset in (0..=60usize).step_by(2) {
            let scheme = Scheme { pre_u32s, offset };
            let mut counts = std::collections::HashMap::new();
            let mut hits = 0usize;
            let mut core = 0usize;
            let mut multi = 0usize;
            let mut agree = 0usize;
            for siblings in files {
                let reads: Vec<Option<u32>> = siblings
                    .iter()
                    .map(|p| body_type_with(p, scheme))
                    .collect();
                if let Some(Some(v)) = reads.first() {
                    hits += 1;
                    *counts.entry(*v).or_insert(0usize) += 1;
                    if CORE_SLOTS.contains(v) {
                        core += 1;
                    }
                }
                if reads.len() >= 2 {
                    multi += 1;
                    if reads.iter().all(|r| r.is_some())
                        && reads.windows(2).all(|w| w[0] == w[1])
                    {
                        agree += 1;
                    }
                }
            }
            let coverage = hits as f32 / files.len() as f32;
            let note: &'static str = if coverage < 0.9 {
                "coverage"
            } else if counts.len() < 3
                || counts.values().copied().max().unwrap_or(0) * 100 > hits * 85
            {
                "diversity"
            } else if core * 100 < hits * 25 {
                // A wardrobe without hair, tops, bottoms, or shoes isn't one.
                "wardrobe prior"
            } else if multi >= 5 && agree * 100 < multi * 90 {
                // Swatches of one part share a BodyType; impostors vary.
                "sibling agreement"
            } else {
                ""
            };
            if nearest.as_ref().map_or(true, |(_, cvg, _)| coverage > *cvg) {
                nearest = Some((scheme, coverage, if note.is_empty() { "tie" } else { note }));
            }
            if !note.is_empty() {
                continue;
            }
            let distinct = counts.len();
            let better = match &best {
                None => true,
                Some((b, cov, dis)) => {
                    coverage > *cov
                        || (coverage == *cov && distinct > *dis)
                        || (coverage == *cov && distinct == *dis && offset < b.offset)
                }
            };
            if better {
                best = Some((scheme, coverage, distinct));
            }
        }
    }
    match best {
        Some((s, cov, dis)) => Elect::Won(s, cov, dis),
        None => {
            let (s, cov, why) =
                nearest.unwrap_or((Scheme { pre_u32s: 0, offset: 0 }, 0.0, "coverage"));
            Elect::Miss(s, cov, why)
        }
    }
}

/// CASP field offsets shift across versions, so a mixed library defeats
/// any single scheme (a field inserted in a newer version pushes
/// BodyType further out). Partition by version and elect independently
/// inside each homogeneous cohort; classify each file with its own
/// version's winner. Cohorts too small to elect stay unlabeled.
pub fn calibrate_by_version(
    files: &[Vec<&[u8]>],
) -> (std::collections::HashMap<u32, Scheme>, String) {
    let mut cohorts: std::collections::HashMap<u32, Vec<Vec<&[u8]>>> =
        std::collections::HashMap::new();
    for siblings in files {
        let Some(p) = siblings.first() else { continue };
        if p.len() >= 4 {
            let v = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
            if (VERSION_MIN..=VERSION_MAX).contains(&v) {
                cohorts.entry(v).or_default().push(siblings.clone());
            }
        }
    }
    let mut order: Vec<u32> = cohorts.keys().copied().collect();
    order.sort_by_key(|v| std::cmp::Reverse(cohorts[v].len()));
    let mut elected = std::collections::HashMap::new();
    let mut parts = Vec::new();
    for v in order {
        let group = &cohorts[&v];
        match elect(group) {
            Elect::Won(s, cov, _) => {
                parts.push(format!(
                    "0x{v:02X}→pre{} off{} ({:.0}%)",
                    s.pre_u32s,
                    s.offset,
                    cov * 100.0
                ));
                elected.insert(v, s);
            }
            Elect::Miss(_, cov, why) => {
                parts.push(format!("0x{v:02X}→none ({why}, best {:.0}%)", cov * 100.0))
            }
            Elect::TooFew(n) => parts.push(format!("0x{v:02X}→too few ({n})")),
        }
    }
    let verdict = if parts.is_empty() {
        "no readable CASP payloads".to_string()
    } else {
        parts.join(" · ")
    };
    (elected, verdict)
}

/// Read a payload's BodyType using its own version's elected scheme.
pub fn body_type_versioned(
    payload: &[u8],
    schemes: &std::collections::HashMap<u32, Scheme>,
) -> Option<u32> {
    if payload.len() < 4 {
        return None;
    }
    let v = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    body_type_with(payload, *schemes.get(&v)?)
}

/// The reference parser, field for field per s4pi's CASPartResource —
/// version, TGI offset, presetCount (always 0), a 7-bit-byte-length
/// UTF-16BE name, then the fixed run to the variable flag list, then the
/// keyed run to bodyType. Version branches: parmFlags2 ≥0x27,
/// excludePartFlags2 ≥0x29, u64 modifier region ≥0x24, 6-byte flags
/// ≥0x25 (4-byte pairs before), createDescriptionKey ≥0x2B. BodyType has
/// no fixed offset — the flag count moves it — which is why every
/// offset election was structurally doomed.
pub fn body_type_reference(payload: &[u8]) -> Option<u32> {
    let mut r = Reader { d: payload, pos: 0 };
    let version = r.u32()?;
    if !(0x1B..=0x7F).contains(&version) {
        return None;
    }
    r.u32()?; // TGI offset
    if r.u32()? != 0 {
        return None; // presetCount: the reference throws on non-zero
    }
    r.string7()?; // byte-length-prefixed UTF-16BE name
    r.skip(4 + 2 + 4 + 4 + 1)?; // sortPriority..parmFlags
    if version >= 0x27 {
        r.skip(1)?; // parmFlags2
    }
    r.skip(8)?; // excludePartFlags
    if version >= 0x29 {
        r.skip(8)?; // excludePartFlags2
    }
    r.skip(if version >= 0x24 { 8 } else { 4 })?; // modifier region
    let flags = r.u32()?;
    if flags > 4096 {
        return None;
    }
    let per = if version >= 0x25 { 6 } else { 4 };
    r.skip(per * flags as usize)?;
    r.skip(4 + 4 + 4)?; // price, titleKey, descriptionKey
    if version >= 0x2B {
        r.skip(4)?; // createDescriptionKey
    }
    r.skip(1)?; // uniqueTextureSpace
    let bt = r.u32()?;
    if (1..=BODY_TYPE_MAX).contains(&bt) {
        Some(bt)
    } else {
        None
    }
}

/// Reference-parse every sibling and demand they agree — swatches of one
/// part share a BodyType, so disagreement means a bad parse, not a chip.
pub fn body_type_checked(siblings: &[&[u8]]) -> Option<u32> {
    let mut it = siblings.iter().map(|p| body_type_reference(p));
    let first = it.next()??;
    for r in it {
        if r != Some(first) {
            return None;
        }
    }
    Some(first)
}

/// Map a BodyType to the subcategory chips. Buckets are deliberately
/// coarse and evidence-adjustable; unknown values land in "other".
pub fn subcategory_for(body_type: u32) -> &'static str {
    match body_type {
        1 => "hats",
        2 => "hair",
        3 | 4 => "face",
        5 => "fullbody",
        6 => "tops",
        7 => "bottoms",
        8 => "shoes",
        9..=13 | 24..=28 | 36..=38 => "accessories",
        29..=35 => "skin",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn casp_versioned(
        version: u32,
        pre_u32s: u8,
        pad_before_body: usize,
        name: &str,
        body_type: u32,
        salt: u32,
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&version.to_le_bytes());
        v.extend_from_slice(&0x0200u32.to_le_bytes());
        for k in 0..pre_u32s {
            v.extend_from_slice(&(salt % 3 + u32::from(k)).to_le_bytes());
        }
        assert!(name.len() < 0x80);
        v.push(name.len() as u8);
        v.extend_from_slice(name.as_bytes());
        v.extend_from_slice(&1.5f32.to_le_bytes());
        v.extend_from_slice(&(salt as u16).to_le_bytes());
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        for b in 0..pad_before_body {
            v.push(0xC0 | (b as u8 & 0x0F)); // version-inserted fields
        }
        v.extend_from_slice(&body_type.to_le_bytes());
        v.extend_from_slice(&(0x1000 + salt).to_le_bytes());
        v
    }

    fn casp_bytes(pre_u32s: u8, name: &str, body_type: u32, salt: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&0x2Eu32.to_le_bytes());
        v.extend_from_slice(&0x0200u32.to_le_bytes()); // dataSize
        for k in 0..pre_u32s {
            // Real libraries vary here — the variance is what defeats the
            // wrong alignment's accidental compensating offset.
            v.extend_from_slice(&(salt % 3 + u32::from(k)).to_le_bytes());
        }
        assert!(name.len() < 0x80);
        v.push(name.len() as u8);
        v.extend_from_slice(name.as_bytes());
        v.extend_from_slice(&1.5f32.to_le_bytes()); // sortPriority
        v.extend_from_slice(&(salt as u16).to_le_bytes()); // swatchOrder
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // outfitGroup (out of range)
        v.extend_from_slice(&body_type.to_le_bytes()); // the planted column
        v.extend_from_slice(&(0x1000 + salt).to_le_bytes()); // trailing noise
        v
    }

    fn wardrobe(pre_u32s: u8) -> Vec<Vec<u8>> {
        let types = [2u32, 6, 6, 7, 8, 2, 26, 5, 31, 6, 8, 1];
        types
            .iter()
            .enumerate()
            .map(|(i, bt)| {
                casp_bytes(pre_u32s, &format!("part{i:02}long"), *bt, i as u32)
            })
            .collect()
    }

    #[test]
    fn calibration_finds_the_planted_column_under_both_alignments() {
        for pre in 0u8..=2 {
            let corpus = wardrobe(pre);
            let refs: Vec<&[u8]> = corpus.iter().map(|v| v.as_slice()).collect();
            let scheme = calibrate(&refs).expect("scheme elected");
            assert_eq!(scheme.pre_u32s, pre);
            assert_eq!(scheme.offset, 10, "sortPriority + swatchOrder + outfitGroup");
            assert_eq!(body_type_with(&corpus[0], scheme), Some(2));
            assert_eq!(body_type_with(&corpus[4], scheme), Some(8));
        }
    }

    #[test]
    fn noise_and_constants_elect_nothing() {
        let noise: Vec<Vec<u8>> = (0..12u32)
            .map(|i| {
                let mut v = vec![0u8; 64];
                v[0..4].copy_from_slice(&0x2Eu32.to_le_bytes());
                v[8] = 3;
                for (j, b) in v.iter_mut().enumerate().skip(12) {
                    *b = (i as u8).wrapping_mul(37).wrapping_add(j as u8) | 0x80;
                }
                v
            })
            .collect();
        let refs: Vec<&[u8]> = noise.iter().map(|v| v.as_slice()).collect();
        assert!(calibrate(&refs).is_none());
        // A column that's one constant value everywhere is refused too.
        let flat: Vec<Vec<u8>> = (0..12).map(|i| casp_bytes(0, "x", 6, i)).collect();
        let refs: Vec<&[u8]> = flat.iter().map(|v| v.as_slice()).collect();
        assert!(calibrate(&refs).is_none(), "all-tops corpus lacks diversity proof");
    }

    /// Today's field bug, encoded: a lower-offset field with small varied
    /// in-range values (outfitGroup-like, differing per swatch) satisfies
    /// every v2 gate and wins the lower-offset tiebreak. Sibling agreement
    /// must reject it and elect the column swatches share.
    fn casp_with_impostor(name: &str, body_type: u32, file_salt: u32, swatch: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&0x2Eu32.to_le_bytes());
        v.extend_from_slice(&0x0200u32.to_le_bytes());
        v.extend_from_slice(&(file_salt % 3).to_le_bytes()); // presetCount
        v.push(name.len() as u8);
        v.extend_from_slice(name.as_bytes());
        v.extend_from_slice(&1.5f32.to_le_bytes()); // off 0..4
        v.extend_from_slice(&(swatch as u16).to_le_bytes()); // off 4..6
        let impostor = 1 + (file_salt * 5 + swatch * 7) % 40; // off 6..10
        v.extend_from_slice(&impostor.to_le_bytes());
        v.extend_from_slice(&body_type.to_le_bytes()); // off 10..14
        v.extend_from_slice(&(0x1000 + file_salt).to_le_bytes());
        v
    }

    fn casp_reference(
        version: u32,
        name_chars: usize,
        flag_count: u32,
        body_type: u32,
        salt: u32,
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&version.to_le_bytes());
        v.extend_from_slice(&0x0400u32.to_le_bytes()); // TGI offset
        v.extend_from_slice(&0u32.to_le_bytes()); // presetCount
        let byte_len = name_chars * 2; // UTF-16BE
        assert!(byte_len < 0x80);
        v.push(byte_len as u8);
        for i in 0..name_chars {
            v.extend_from_slice(&[0x00, b'a' + (i as u8 % 26)]);
        }
        v.extend_from_slice(&1.5f32.to_le_bytes()); // sortPriority
        v.extend_from_slice(&(salt as u16).to_le_bytes()); // secondarySortIndex
        v.extend_from_slice(&(0xAAAA_0000 + salt).to_le_bytes()); // propertyID
        v.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // auralMaterialHash
        v.push(0x03); // parmFlags
        if version >= 0x27 {
            v.push(0x01); // parmFlags2
        }
        v.extend_from_slice(&0x11u64.to_le_bytes()); // excludePartFlags
        if version >= 0x29 {
            v.extend_from_slice(&0x22u64.to_le_bytes());
        }
        if version >= 0x24 {
            v.extend_from_slice(&0x33u64.to_le_bytes());
        } else {
            v.extend_from_slice(&0x33u32.to_le_bytes());
        }
        v.extend_from_slice(&flag_count.to_le_bytes());
        for f in 0..flag_count {
            if version >= 0x25 {
                v.extend_from_slice(&(f as u16).to_le_bytes());
                v.extend_from_slice(&(0x40 + f).to_le_bytes());
            } else {
                v.extend_from_slice(&(f as u16).to_le_bytes());
                v.extend_from_slice(&(f as u16).to_le_bytes());
            }
        }
        v.extend_from_slice(&100u32.to_le_bytes()); // deprecatedPrice
        v.extend_from_slice(&0x51u32.to_le_bytes()); // partTitleKey
        v.extend_from_slice(&0x52u32.to_le_bytes()); // partDescriptionKey
        if version >= 0x2B {
            v.extend_from_slice(&0x53u32.to_le_bytes());
        }
        v.push(0x00); // uniqueTextureSpace
        v.extend_from_slice(&body_type.to_le_bytes());
        v.extend_from_slice(&2u32.to_le_bytes()); // bodySubType
        v.extend_from_slice(&0x3333u32.to_le_bytes()); // ageGender
        v
    }

    #[test]
    fn reference_parse_reads_body_type_across_versions_and_flag_counts() {
        // 0x2A lacks createDescriptionKey; flag counts move the field.
        for (version, flags) in [(0x2Au32, 0u32), (0x2A, 7), (0x2E, 3), (0x33, 11), (0x1B, 5)] {
            for bt in [2u32, 6, 8, 26, 31] {
                let p = casp_reference(version, 9, flags, bt, 4);
                assert_eq!(
                    body_type_reference(&p),
                    Some(bt),
                    "v=0x{version:02X} flags={flags} bt={bt}"
                );
            }
        }
    }

    #[test]
    fn reference_parse_refuses_bad_prefixes_and_ranges() {
        let mut nonzero_preset = casp_reference(0x2E, 4, 2, 6, 0);
        nonzero_preset[8] = 1;
        assert_eq!(body_type_reference(&nonzero_preset), None);
        let out_of_range = casp_reference(0x2E, 4, 2, 999, 0);
        assert_eq!(body_type_reference(&out_of_range), None);
        let mut short = casp_reference(0x2E, 4, 2, 6, 0);
        short.truncate(30);
        assert_eq!(body_type_reference(&short), None);
    }

    #[test]
    fn sibling_check_demands_agreement() {
        let a = casp_reference(0x2E, 6, 4, 6, 1);
        let b = casp_reference(0x2E, 6, 9, 6, 2); // different flags, same part
        let c = casp_reference(0x2E, 6, 2, 7, 3); // a different bodyType
        assert_eq!(body_type_checked(&[&a, &b]), Some(6));
        assert_eq!(body_type_checked(&[&a, &b, &c]), None);
    }

    #[test]
    fn sibling_agreement_defeats_the_impostor_column() {
        let types = [2u32, 6, 6, 7, 8, 2, 26, 5, 31, 6, 8, 1];
        let corpus: Vec<Vec<Vec<u8>>> = types
            .iter()
            .enumerate()
            .map(|(i, bt)| {
                (0..3u32)
                    .map(|sw| casp_with_impostor(&format!("part{i:02}nm"), *bt, i as u32, sw))
                    .collect()
            })
            .collect();
        let files: Vec<Vec<&[u8]>> = corpus
            .iter()
            .map(|sibs| sibs.iter().map(|v| v.as_slice()).collect())
            .collect();
        let (schemes, verdict) = calibrate_by_version(&files);
        let s = schemes.get(&0x2E).copied().expect("cohort elected");
        assert_eq!(s.offset, 10, "impostor at off 6 must lose: {verdict}");
        assert_eq!(body_type_versioned(files[0][0], &schemes), Some(2));
        assert_eq!(body_type_versioned(files[4][0], &schemes), Some(8));
    }

    #[test]
    fn a_wardrobe_without_garments_is_refused() {
        // Diverse, agreeing, in-range — but flooding 14..=23, where no real
        // CC library lives. The prior must refuse it.
        let corpus: Vec<Vec<u8>> = (0..12u32)
            .map(|i| casp_versioned(0x2E, 1, 0, &format!("odd{i:02}name"), 14 + (i % 10), i))
            .collect();
        let files: Vec<Vec<&[u8]>> = corpus.iter().map(|v| vec![v.as_slice()]).collect();
        let (schemes, verdict) = calibrate_by_version(&files);
        assert!(schemes.is_empty(), "{verdict}");
        assert!(verdict.contains("wardrobe prior"), "{verdict}");
    }

    #[test]
    fn version_cohorts_elect_their_own_offsets() {
        let types = [2u32, 6, 6, 7, 8, 2, 26, 5, 31, 6, 8, 1];
        let mut corpus: Vec<Vec<u8>> = Vec::new();
        for (i, bt) in types.iter().enumerate() {
            corpus.push(casp_versioned(0x2E, 1, 0, &format!("old{i:02}name"), *bt, i as u32));
        }
        for (i, bt) in types.iter().enumerate() {
            corpus.push(casp_versioned(0x33, 1, 12, &format!("new{i:02}name"), *bt, i as u32));
        }
        let refs: Vec<&[u8]> = corpus.iter().map(|v| v.as_slice()).collect();
        // The mixed corpus defeats single-scheme election — this is the
        // 68%-coverage failure from the field, reproduced.
        assert!(calibrate(&refs).is_none(), "single scheme must fail on mixed versions");
        let files: Vec<Vec<&[u8]>> = corpus.iter().map(|v| vec![v.as_slice()]).collect();
        let (schemes, verdict) = calibrate_by_version(&files);
        assert_eq!(schemes.get(&0x2E), Some(&Scheme { pre_u32s: 1, offset: 10 }));
        assert_eq!(schemes.get(&0x33), Some(&Scheme { pre_u32s: 1, offset: 22 }));
        assert!(verdict.contains("0x2E→pre1 off10"), "{verdict}");
        assert_eq!(body_type_versioned(&corpus[0], &schemes), Some(2));
        assert_eq!(body_type_versioned(&corpus[16], &schemes), Some(8));
    }

    #[test]
    fn out_of_range_reads_are_misses_not_other() {
        let bytes = casp_bytes(0, "part", 999, 0);
        let scheme = Scheme { pre_u32s: 0, offset: 10 };
        assert_eq!(body_type_with(&bytes, scheme), None);
    }

    #[test]
    fn buckets_cover_the_known_map() {
        assert_eq!(subcategory_for(2), "hair");
        assert_eq!(subcategory_for(6), "tops");
        assert_eq!(subcategory_for(8), "shoes");
        assert_eq!(subcategory_for(25), "accessories");
        assert_eq!(subcategory_for(31), "skin");
        assert_eq!(subcategory_for(999), "other");
    }
}
