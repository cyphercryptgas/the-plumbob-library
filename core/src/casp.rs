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

/// Read the BodyType from a decompressed CASP payload.
pub fn casp_body_type(payload: &[u8]) -> Option<u32> {
    let mut r = Reader { d: payload, pos: 0 };
    let version = r.u32()?;
    if !(VERSION_MIN..=VERSION_MAX).contains(&version) {
        return None;
    }
    r.u32()?; // dataSize
    r.string7()?; // name
    r.skip(4)?; // sortPriority (f32)
    r.u16()?; // swatchOrder
    r.u32()?; // outfitGroup
    r.u32() // bodyType
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

    fn casp_bytes(version: u32, name: &str, body_type: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&version.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // dataSize (unused here)
        assert!(name.len() < 0x80, "single-byte length in fixtures");
        v.push(name.len() as u8);
        v.extend_from_slice(name.as_bytes());
        v.extend_from_slice(&1.0f32.to_le_bytes()); // sortPriority
        v.extend_from_slice(&7u16.to_le_bytes()); // swatchOrder
        v.extend_from_slice(&0u32.to_le_bytes()); // outfitGroup
        v.extend_from_slice(&body_type.to_le_bytes());
        v.extend_from_slice(b"trailing-ignored");
        v
    }

    #[test]
    fn body_type_reads_through_the_variable_name() {
        for (name, bt) in [("yfShoes_Heel", 8u32), ("", 2), ("a-much-longer-name-here", 6)] {
            let bytes = casp_bytes(0x2E, name, bt);
            assert_eq!(casp_body_type(&bytes), Some(bt), "name {name:?}");
        }
    }

    #[test]
    fn out_of_band_versions_and_truncation_yield_none() {
        assert_eq!(casp_body_type(&casp_bytes(0x05, "x", 6)), None);
        assert_eq!(casp_body_type(&casp_bytes(0xFF, "x", 6)), None);
        let mut short = casp_bytes(0x2E, "x", 6);
        short.truncate(9);
        assert_eq!(casp_body_type(&short), None);
        // A 7-bit length that overruns the payload also refuses.
        let mut lying = casp_bytes(0x2E, "", 6);
        lying[8] = 0x7F;
        assert_eq!(casp_body_type(&lying), None);
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
