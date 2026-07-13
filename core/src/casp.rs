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
    if samples.len() < 8 {
        return (None, format!("only {} readable samples", samples.len()));
    }
    let mut best: Option<(Scheme, f32, usize)> = None;
    let mut nearest: Option<(Scheme, f32)> = None;
    for pre_u32s in 0u8..=2 {
        for offset in (0..=60usize).step_by(2) {
            let scheme = Scheme { pre_u32s, offset };
            let mut counts = std::collections::HashMap::new();
            let mut hits = 0usize;
            for p in samples {
                if let Some(v) = body_type_with(p, scheme) {
                    hits += 1;
                    *counts.entry(v).or_insert(0usize) += 1;
                }
            }
            let coverage = hits as f32 / samples.len() as f32;
            if nearest.as_ref().map_or(true, |(_, c)| coverage > *c) {
                nearest = Some((scheme, coverage));
            }
            if coverage < 0.8 {
                continue;
            }
            let distinct = counts.len();
            let top = counts.values().copied().max().unwrap_or(0);
            if distinct < 3 || top * 100 > hits * 85 {
                continue;
            }
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
        Some((s, cov, dis)) => (
            Some(s),
            format!(
                "elected pre={} off={} · coverage {:.0}% · {} distinct types",
                s.pre_u32s,
                s.offset,
                cov * 100.0,
                dis
            ),
        ),
        None => {
            let (s, cov) = nearest.unwrap_or((Scheme { pre_u32s: 0, offset: 0 }, 0.0));
            (
                None,
                format!(
                    "no scheme elected · best coverage {:.0}% at pre={} off={}",
                    cov * 100.0,
                    s.pre_u32s,
                    s.offset
                ),
            )
        }
    }
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
