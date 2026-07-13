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
    pub with_preset_count: bool,
    pub offset: usize,
}

/// The plausible BodyType range (the community enum tops out in the low
/// forties). Anything outside is a misread, never a category.
const BODY_TYPE_MAX: u32 = 43;

fn prefix_cursor(payload: &[u8], with_preset_count: bool) -> Option<usize> {
    let mut r = Reader { d: payload, pos: 0 };
    let version = r.u32()?;
    if !(VERSION_MIN..=VERSION_MAX).contains(&version) {
        return None;
    }
    r.u32()?; // dataSize / TGI offset
    if with_preset_count {
        r.u32()?;
    }
    r.string7()?; // name
    Some(r.pos)
}

/// Read the u32 a scheme points at, gated to the plausible range.
pub fn body_type_with(payload: &[u8], scheme: Scheme) -> Option<u32> {
    let cursor = prefix_cursor(payload, scheme.with_preset_count)?;
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
    if samples.len() < 8 {
        return None;
    }
    let mut best: Option<(Scheme, f32, usize)> = None;
    for with_preset_count in [false, true] {
        for offset in (0..=30).step_by(2) {
            let scheme = Scheme {
                with_preset_count,
                offset,
            };
            let mut vals = Vec::new();
            for p in samples {
                if let Some(v) = body_type_with(p, scheme) {
                    vals.push(v);
                }
            }
            if vals.len() * 10 < samples.len() * 7 {
                continue; // most files must yield a reading
            }
            let coverage = vals.len() as f32 / samples.len() as f32;
            let mut counts = std::collections::HashMap::new();
            for v in &vals {
                *counts.entry(*v).or_insert(0usize) += 1;
            }
            let distinct = counts.len();
            let top = counts.values().copied().max().unwrap_or(0);
            if distinct < 3 || top * 100 > vals.len() * 85 {
                continue; // constants and near-constants aren't wardrobes
            }
            let score = coverage;
            let better = match &best {
                None => true,
                Some((_, s, d)) => score > *s || (score == *s && distinct > *d),
            };
            if better {
                best = Some((scheme, score, distinct));
            }
        }
    }
    best.filter(|(_, score, _)| *score >= 0.9).map(|(s, _, _)| s)
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

    fn casp_bytes(with_preset: bool, name: &str, body_type: u32, salt: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&0x2Eu32.to_le_bytes());
        v.extend_from_slice(&0x0200u32.to_le_bytes()); // dataSize
        if with_preset {
            // Real libraries vary here — the variance is what defeats the
            // wrong alignment's accidental compensating offset.
            v.extend_from_slice(&(salt % 3).to_le_bytes());
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

    fn wardrobe(with_preset: bool) -> Vec<Vec<u8>> {
        let types = [2u32, 6, 6, 7, 8, 2, 26, 5, 31, 6, 8, 1];
        types
            .iter()
            .enumerate()
            .map(|(i, bt)| {
                casp_bytes(with_preset, &format!("part{i:02}long"), *bt, i as u32)
            })
            .collect()
    }

    #[test]
    fn calibration_finds_the_planted_column_under_both_alignments() {
        for with_preset in [false, true] {
            let corpus = wardrobe(with_preset);
            let refs: Vec<&[u8]> = corpus.iter().map(|v| v.as_slice()).collect();
            let scheme = calibrate(&refs).expect("scheme elected");
            assert_eq!(scheme.with_preset_count, with_preset);
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
        let flat: Vec<Vec<u8>> = (0..12).map(|i| casp_bytes(false, "x", 6, i)).collect();
        let refs: Vec<&[u8]> = flat.iter().map(|v| v.as_slice()).collect();
        assert!(calibrate(&refs).is_none(), "all-tops corpus lacks diversity proof");
    }

    #[test]
    fn out_of_range_reads_are_misses_not_other() {
        let bytes = casp_bytes(false, "part", 999, 0);
        let scheme = Scheme { with_preset_count: false, offset: 10 };
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
