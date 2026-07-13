//! DBPF (`.package`) index reading — the foundation of package awareness.
//!
//! STRICTLY READ-ONLY. This module reads a package's 96-byte header and its
//! resource index (the list of type/group/instance keys), and nothing else:
//! it never decompresses resource data, never loads resource bodies, and
//! never writes. Reading the index of a multi-megabyte package touches only
//! a few kilobytes.
//!
//! Format (verified against community documentation — see docs/RESEARCH.md,
//! "Phase 2 research"):
//!
//! * Header, 96 bytes: magic `DBPF` @0x00; major u32 @0x04 (= 2); minor u32
//!   @0x08 (docs describe 2.0, packages in the wild are 2.1 — both accepted);
//!   index entry count @0x24; index size @0x2C; index version @0x3C (= 3,
//!   informational); index position @0x40. All little-endian.
//! * Index: a `flags` u32 whose low bits hoist fields that are constant
//!   across every entry into a shared header written once — bit 0 = type,
//!   bit 1 = group, bit 2 = instance-high. Each entry then carries its
//!   remaining fields in order: [type][group][instance-high][instance-low]
//!   [position][file size (bit 31 is a compression flag)][mem size]
//!   [compression u16][committed u16] — 32 bytes minus 4 per constant bit.
//! * A resource's 64-bit instance is `(high << 32) | low`.
//!
//! Unknown flag bits mean an index layout this parser has not verified, so
//! it refuses honestly instead of guessing at field offsets.

use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use thiserror::Error;

pub const HEADER_LEN: u64 = 96;
const MAGIC: [u8; 4] = *b"DBPF";
/// Layout bits this parser understands (constant type / group / instance-hi).
const KNOWN_FLAG_BITS: u32 = 0b111;
/// Real game packages top out around tens of thousands of resources; a count
/// beyond this is corruption or hostility, not content.
const MAX_ENTRIES: u32 = 2_000_000;

#[derive(Debug, Error)]
pub enum DbpfError {
    #[error("could not read package: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a DBPF package (missing DBPF magic)")]
    NotDbpf,
    #[error("unsupported DBPF version {major}.{minor} (expected 2.0 or 2.1)")]
    UnsupportedVersion { major: u32, minor: u32 },
    #[error("unsupported index flags 0x{0:08X} — unknown layout bits set")]
    UnsupportedIndexFlags(u32),
    #[error("package is truncated: layout needs {needed} bytes, file has {actual}")]
    Truncated { needed: u64, actual: u64 },
    #[error("corrupt index: {0}")]
    CorruptIndex(String),
}

/// The identity of one resource inside a package. Two packages containing a
/// resource with the same key are competing for the same slot — the essence
/// of a mod conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceKey {
    pub type_id: u32,
    pub group_id: u32,
    pub instance: u64,
}

impl ResourceKey {
    /// Canonical `TTTTTTTT-GGGGGGGG-IIIIIIIIIIIIIIII` display form used by
    /// community tooling.
    pub fn tgi_string(&self) -> String {
        format!(
            "{:08X}-{:08X}-{:016X}",
            self.type_id, self.group_id, self.instance
        )
    }
}

/// Where a resource's payload lives and how it's stored — retained so
/// thumbnails can be extracted without a second index parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct EntryMeta {
    pub position: u32,
    /// On-disk byte count (bit 31, the compressed marker, already masked).
    pub size: u32,
    pub mem_size: u32,
    /// 0x0000 uncompressed · 0x5A42 zlib · anything else unsupported here.
    pub compression: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageIndex {
    pub major: u32,
    pub minor: u32,
    pub keys: Vec<ResourceKey>,
    /// Parallel to `keys`.
    pub entries: Vec<EntryMeta>,
}

impl PackageIndex {
    pub fn resource_count(&self) -> usize {
        self.keys.len()
    }
}

/// Read a package's resource index from disk. Duplicate keys within one
/// package are preserved as-is — they are themselves a finding.
pub fn read_package_index(path: &Path) -> Result<PackageIndex, DbpfError> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();

    let mut header = [0u8; HEADER_LEN as usize];
    let got = read_up_to(&mut file, &mut header)?;
    if got < 4 || header[0..4] != MAGIC {
        return Err(DbpfError::NotDbpf);
    }
    if (got as u64) < HEADER_LEN {
        return Err(DbpfError::Truncated {
            needed: HEADER_LEN,
            actual: file_len,
        });
    }

    let major = u32le(&header, 0x04);
    let minor = u32le(&header, 0x08);
    if major != 2 || minor > 1 {
        return Err(DbpfError::UnsupportedVersion { major, minor });
    }

    let entry_count = u32le(&header, 0x24);
    let index_pos = u32le(&header, 0x40) as u64;

    if entry_count == 0 {
        // Spec: with zero entries, index size and position are also zero.
        return Ok(PackageIndex {
            major,
            minor,
            keys: Vec::new(),
            entries: Vec::new(),
        });
    }
    if entry_count > MAX_ENTRIES {
        return Err(DbpfError::CorruptIndex(format!(
            "implausible resource count {entry_count}"
        )));
    }

    if index_pos + 4 > file_len {
        return Err(DbpfError::Truncated {
            needed: index_pos + 4,
            actual: file_len,
        });
    }
    file.seek(SeekFrom::Start(index_pos))?;
    let mut flag_bytes = [0u8; 4];
    file.read_exact(&mut flag_bytes)?;
    let flags = u32::from_le_bytes(flag_bytes);
    if flags & !KNOWN_FLAG_BITS != 0 {
        return Err(DbpfError::UnsupportedIndexFlags(flags));
    }
    let const_type = flags & 0b001 != 0;
    let const_group = flags & 0b010 != 0;
    let const_hi = flags & 0b100 != 0;
    let const_count = (flags & KNOWN_FLAG_BITS).count_ones() as u64;
    let per_entry = 32u64 - 4 * const_count;
    let needed = index_pos + 4 + 4 * const_count + entry_count as u64 * per_entry;
    if needed > file_len {
        return Err(DbpfError::Truncated {
            needed,
            actual: file_len,
        });
    }

    // Constants appear once, in bit order: type, group, instance-high.
    let mut constants = [0u32; 3];
    if const_count > 0 {
        let mut cbuf = vec![0u8; (4 * const_count) as usize];
        file.read_exact(&mut cbuf)?;
        let mut coff = 0usize;
        for (present, slot) in [(const_type, 0usize), (const_group, 1), (const_hi, 2)] {
            if present {
                constants[slot] = u32le(&cbuf, coff);
                coff += 4;
            }
        }
    }

    let mut body = vec![0u8; (entry_count as u64 * per_entry) as usize];
    file.read_exact(&mut body)?;

    let mut keys = Vec::with_capacity(entry_count as usize);
    let mut entries = Vec::with_capacity(entry_count as usize);
    let mut off = 0usize;
    for _ in 0..entry_count {
        let type_id = if const_type {
            constants[0]
        } else {
            let v = u32le(&body, off);
            off += 4;
            v
        };
        let group_id = if const_group {
            constants[1]
        } else {
            let v = u32le(&body, off);
            off += 4;
            v
        };
        let hi = if const_hi {
            constants[2]
        } else {
            let v = u32le(&body, off);
            off += 4;
            v
        };
        let lo = u32le(&body, off);
        off += 4;
        let position = u32le(&body, off);
        let size = u32le(&body, off + 4) & 0x7FFF_FFFF;
        let mem_size = u32le(&body, off + 8);
        let compression = u16::from(body[off + 12]) | u16::from(body[off + 13]) << 8;
        off += 16;
        entries.push(EntryMeta {
            position,
            size,
            mem_size,
            compression,
        });
        keys.push(ResourceKey {
            type_id,
            group_id,
            instance: ((hi as u64) << 32) | lo as u64,
        });
    }

    Ok(PackageIndex {
        major,
        minor,
        keys,
        entries,
    })
}

/// Friendly names for the resource types conflict displays care about,
/// sourced from the community type tables (see docs/RESEARCH.md). Unknown
/// types display as raw hex.
pub fn resource_type_name(type_id: u32) -> Option<&'static str> {
    Some(match type_id {
        0x034AEECB => "CAS Part",
        0x0354796A => "Skin Tone",
        0x0355E0A6 => "Bone Delta",
        0x03B4C61D => "Light",
        0x067CAA11 => "Blend Geometry",
        0x015A1849 => "CAS Geometry",
        0xEAA32ADD => "CAS Preset",
        0x220557DA => "String Table",
        0x545AC67A => "SimData",
        0x62E94D38 => "Tuning (binary)",
        0x319E4F1D => "Object Catalog",
        0xC0DB5AE7 => "Object Definition",
        0xD3044521 => "Object Slot",
        0xD382BF57 => "Footprint",
        0x6B20C4F3 => "Animation Clip",
        0x3453CF95 => "Image (DDS)",
        0x2F7D0004 | 0x3C1AF1F2 | 0x5B282D45 => "Image (PNG/thumbnail)",
        _ => return None,
    })
}

/// Presentation-only types (images/thumbnails): overlapping keys of these
/// types affect what something *looks like* in menus, not how the game
/// plays. Community conflict tooling treats them as low-severity; ours will
/// too (see docs/RESEARCH.md).
pub fn type_is_presentation_only(type_id: u32) -> bool {
    matches!(type_id, 0x3453CF95 | 0x2F7D0004 | 0x3C1AF1F2 | 0x5B282D45)
}

fn read_up_to(file: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        let n = file.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

fn u32le(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

/// Test-only synthetic package builder, shared with the database layer's
/// tests so parse-pass and conflict tests run against real DBPF bytes.
#[cfg(test)]
pub mod testutil {
    pub fn put32(out: &mut [u8], off: usize, v: u32) {
        out[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn push32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    /// Byte-exact synthetic package following the verified layout. When a
    /// flag bit is set, the corresponding field must be uniform across
    /// `keys` (asserted).
    pub fn build_package(minor: u32, flags: u32, keys: &[(u32, u32, u64)]) -> Vec<u8> {
        let const_type = flags & 0b001 != 0;
        let const_group = flags & 0b010 != 0;
        let const_hi = flags & 0b100 != 0;
        let const_count = (flags & 0b111).count_ones() as usize;
        let per_entry = 32 - 4 * const_count;
        let index_size = 4 + 4 * const_count + keys.len() * per_entry;

        let mut out = vec![0u8; 96];
        out[0..4].copy_from_slice(b"DBPF");
        put32(&mut out, 0x04, 2);
        put32(&mut out, 0x08, minor);
        put32(&mut out, 0x24, keys.len() as u32);
        put32(&mut out, 0x2C, index_size as u32);
        put32(&mut out, 0x3C, 3);
        put32(&mut out, 0x40, 96);

        push32(&mut out, flags);
        if !keys.is_empty() {
            if const_type {
                push32(&mut out, keys[0].0);
            }
            if const_group {
                push32(&mut out, keys[0].1);
            }
            if const_hi {
                push32(&mut out, (keys[0].2 >> 32) as u32);
            }
        }
        for (i, (t, g, inst)) in keys.iter().enumerate() {
            if const_type {
                assert_eq!(*t, keys[0].0, "builder: type must be uniform");
            } else {
                push32(&mut out, *t);
            }
            if const_group {
                assert_eq!(*g, keys[0].1, "builder: group must be uniform");
            } else {
                push32(&mut out, *g);
            }
            let hi = (inst >> 32) as u32;
            if const_hi {
                assert_eq!(hi, (keys[0].2 >> 32) as u32, "builder: hi must be uniform");
            } else {
                push32(&mut out, hi);
            }
            push32(&mut out, *inst as u32); // instance low
            push32(&mut out, 0x1000 + i as u32); // position (arbitrary)
            push32(&mut out, 0x8000_0040); // file size with compression bit set
            push32(&mut out, 0x40); // mem size
            push32(&mut out, 0x0001_5A42); // compression 0x5A42 + committed 1
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::{build_package, put32};
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn write_temp(bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.package");
        fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    const K1: (u32, u32, u64) = (0x034AEECB, 0x00000000, 0x8470D9250CEE7647);
    const K2: (u32, u32, u64) = (0x220557DA, 0x80000000, 0x0000000000BEEF01);
    const K3: (u32, u32, u64) = (0x545AC67A, 0x00000000, 0xDEADBEEF12345678);

    fn assert_keys(index: &PackageIndex, expected: &[(u32, u32, u64)]) {
        assert_eq!(index.resource_count(), expected.len());
        for (key, (t, g, i)) in index.keys.iter().zip(expected) {
            assert_eq!(key.type_id, *t);
            assert_eq!(key.group_id, *g);
            assert_eq!(key.instance, *i);
        }
    }

    #[test]
    fn parses_full_entries_with_no_constant_fields() {
        let bytes = build_package(1, 0, &[K1, K2, K3]);
        let (_d, path) = write_temp(&bytes);
        let index = read_package_index(&path).unwrap();
        assert_eq!((index.major, index.minor), (2, 1));
        assert_keys(&index, &[K1, K2, K3]);
    }

    #[test]
    fn parses_all_constant_fields() {
        // Same type, group, and instance-high hoisted into the index header.
        let keys = [
            (0x034AEECB, 0x80000000, 0xAAAA0000_00000001),
            (0x034AEECB, 0x80000000, 0xAAAA0000_00000002),
            (0x034AEECB, 0x80000000, 0xAAAA0000_00000003),
        ];
        let bytes = build_package(1, 0b111, &keys);
        let (_d, path) = write_temp(&bytes);
        let index = read_package_index(&path).unwrap();
        assert_keys(&index, &keys);
    }

    #[test]
    fn parses_constant_type_only() {
        let keys = [
            (0x62E94D38, 0x00000000, 0x0000000000000AAA),
            (0x62E94D38, 0x00000001, 0xBBBB0000_00000BBB),
        ];
        let bytes = build_package(1, 0b001, &keys);
        let (_d, path) = write_temp(&bytes);
        let index = read_package_index(&path).unwrap();
        assert_keys(&index, &keys);
    }

    #[test]
    fn accepts_minor_zero_and_one() {
        for minor in [0u32, 1] {
            let bytes = build_package(minor, 0, &[K3]);
            let (_d, path) = write_temp(&bytes);
            let index = read_package_index(&path).unwrap();
            assert_eq!(index.minor, minor);
            // Instance assembles high << 32 | low.
            assert_eq!(index.keys[0].instance, 0xDEADBEEF12345678);
        }
    }

    #[test]
    fn empty_package_has_no_keys() {
        let mut bytes = build_package(1, 0, &[]);
        // Spec: with zero entries, size and position are also zero.
        put32(&mut bytes, 0x2C, 0);
        put32(&mut bytes, 0x40, 0);
        bytes.truncate(96);
        let (_d, path) = write_temp(&bytes);
        let index = read_package_index(&path).unwrap();
        assert!(index.keys.is_empty());
    }

    #[test]
    fn rejects_non_dbpf_and_tiny_files() {
        let (_d1, zip) = write_temp(b"PK\x03\x04 definitely not a package");
        assert!(matches!(read_package_index(&zip), Err(DbpfError::NotDbpf)));
        let (_d2, tiny) = write_temp(b"DB");
        assert!(matches!(read_package_index(&tiny), Err(DbpfError::NotDbpf)));
        let (_d3, short) = write_temp(b"DBPF only a stub");
        assert!(matches!(
            read_package_index(&short),
            Err(DbpfError::Truncated { .. })
        ));
    }

    #[test]
    fn rejects_unsupported_versions() {
        let mut wrong_major = build_package(1, 0, &[K1]);
        put32(&mut wrong_major, 0x04, 1);
        let (_d1, p1) = write_temp(&wrong_major);
        assert!(matches!(
            read_package_index(&p1),
            Err(DbpfError::UnsupportedVersion { major: 1, .. })
        ));

        let mut wrong_minor = build_package(1, 0, &[K1]);
        put32(&mut wrong_minor, 0x08, 5);
        let (_d2, p2) = write_temp(&wrong_minor);
        assert!(matches!(
            read_package_index(&p2),
            Err(DbpfError::UnsupportedVersion { minor: 5, .. })
        ));
    }

    #[test]
    fn rejects_unknown_index_flags() {
        let mut bytes = build_package(1, 0, &[K1]);
        put32(&mut bytes, 96, 0b1000); // unknown layout bit
        let (_d, path) = write_temp(&bytes);
        assert!(matches!(
            read_package_index(&path),
            Err(DbpfError::UnsupportedIndexFlags(0b1000))
        ));
    }

    #[test]
    fn truncated_index_reports_needed_bytes() {
        let bytes = build_package(1, 0, &[K1, K2, K3]);
        let cut = &bytes[..bytes.len() - 5];
        let (_d, path) = write_temp(cut);
        match read_package_index(&path) {
            Err(DbpfError::Truncated { needed, actual }) => {
                assert!(needed > actual);
                assert_eq!(actual, cut.len() as u64);
            }
            other => panic!("expected Truncated, got {other:?}"),
        }
    }

    #[test]
    fn implausible_count_is_corrupt_not_a_huge_allocation() {
        let mut bytes = build_package(1, 0, &[K1]);
        put32(&mut bytes, 0x24, 3_000_000);
        let (_d, path) = write_temp(&bytes);
        assert!(matches!(
            read_package_index(&path),
            Err(DbpfError::CorruptIndex(_))
        ));
    }

    #[test]
    fn tgi_display_and_type_knowledge() {
        let key = ResourceKey {
            type_id: 0x034AEECB,
            group_id: 0x80000000,
            instance: 0x8470D9250CEE7647,
        };
        assert_eq!(key.tgi_string(), "034AEECB-80000000-8470D9250CEE7647");
        assert_eq!(resource_type_name(0x034AEECB), Some("CAS Part"));
        assert_eq!(resource_type_name(0x62E94D38), Some("Tuning (binary)"));
        assert_eq!(resource_type_name(0xDEADBEEF), None);
        assert!(type_is_presentation_only(0x3C1AF1F2));
        assert!(!type_is_presentation_only(0x545AC67A));
    }
}

// ---------------------------------------------------------------------------
// Thumbnail extraction
// ---------------------------------------------------------------------------

const PNG_MAGIC: [u8; 4] = [0x89, b'P', b'N', b'G'];
const JPG_MAGIC: [u8; 3] = [0xFF, 0xD8, 0xFF];
const COMP_NONE: u16 = 0x0000;
const COMP_ZLIB: u16 = 0x5A42;
const MAX_THUMB_BYTES: u64 = 16 * 1024 * 1024;

/// Thumbnail-bearing image types, in preference order — the dedicated
/// thumbnail type first, DDS deliberately excluded (needs conversion).
const THUMB_TYPES: [u32; 5] = [
    0x3C1A_F1F2,
    0x5B28_2D45,
    0x2F7D_0004,
    0x3453_CF95,
    // Well-attested S4 image container; payloads still must pass the
    // PNG/JPEG/DDS sniff, so a wrong guess costs nothing.
    0x00B2_D882,
];

/// Pull the best in-game image out of a package: PNG or JPEG payloads
/// only, decompressing zlib entries, sniffing magic bytes, and skipping —
/// never failing on — anything it can't decode. `Ok(None)` simply means
/// this package carries no extractable thumbnail.
pub fn extract_thumbnail(path: &Path) -> Result<Option<(Vec<u8>, &'static str)>, DbpfError> {
    let index = read_package_index(path)?;
    let mut file = File::open(path)?;
    for wanted in THUMB_TYPES {
        let mut candidates: Vec<&EntryMeta> = index
            .keys
            .iter()
            .zip(index.entries.iter())
            .filter(|(k, _)| k.type_id == wanted)
            .map(|(_, e)| e)
            .collect();
        // Bigger memory size ≈ bigger picture.
        candidates.sort_by_key(|e| std::cmp::Reverse(e.mem_size));
        for entry in candidates {
            let Some(payload) = read_entry_payload(&mut file, entry) else {
                continue;
            };
            if payload.len() >= 4 && payload[..4] == PNG_MAGIC {
                return Ok(Some((payload, "png")));
            }
            if payload.len() >= 3 && payload[..3] == JPG_MAGIC {
                return Ok(Some((payload, "jpg")));
            }
            if payload.len() >= 4 && &payload[..4] == b"DDS " {
                if let Some(png) = dds_to_png(&payload) {
                    return Ok(Some((png, "png")));
                }
                if let Some(un) = unshuffle_dst(&payload) {
                    if let Some(png) = dds_to_png(&un) {
                        return Ok(Some((png, "png")));
                    }
                }
            }
        }
    }
    Ok(None)
}

/// Read one resource's payload: seek, read, and decompress by declared
/// codec. `None` on any IO trouble or an unsupported codec — callers skip.
pub fn read_entry_payload(file: &mut File, entry: &EntryMeta) -> Option<Vec<u8>> {
    if u64::from(entry.size) > MAX_THUMB_BYTES || u64::from(entry.mem_size) > MAX_THUMB_BYTES {
        return None;
    }
    file.seek(SeekFrom::Start(u64::from(entry.position))).ok()?;
    let mut raw = vec![0u8; entry.size as usize];
    file.read_exact(&mut raw).ok()?;
    match entry.compression {
        COMP_NONE => Some(raw),
        COMP_ZLIB => {
            use std::io::Read as _;
            let mut out = Vec::with_capacity(entry.mem_size as usize);
            let mut dec = flate2::read::ZlibDecoder::new(raw.as_slice());
            dec.by_ref().take(MAX_THUMB_BYTES).read_to_end(&mut out).ok()?;
            Some(out)
        }
        _ => None,
    }
}

/// Up to `max_n` CASP payloads from one package — a part's swatches all
/// carry the same BodyType, which is exactly what election v3 verifies.
pub fn read_casp_payloads(path: &Path, max_n: usize) -> Result<Vec<Vec<u8>>, DbpfError> {
    let index = read_package_index(path)?;
    let mut file = File::open(path)?;
    let mut out = Vec::new();
    for (key, entry) in index.keys.iter().zip(index.entries.iter()) {
        if key.type_id != crate::casp::CASP_TYPE {
            continue;
        }
        if let Some(payload) = read_entry_payload(&mut file, entry) {
            out.push(payload);
            if out.len() >= max_n {
                break;
            }
        }
    }
    Ok(out)
}

/// The first CAS part's decompressed payload, if the package has one.
/// BodyType reading happens above this layer, where a calibrated scheme
/// elected across the whole library is available.
pub fn read_casp_payload(path: &Path) -> Result<Option<Vec<u8>>, DbpfError> {
    let index = read_package_index(path)?;
    let mut file = File::open(path)?;
    for (key, entry) in index.keys.iter().zip(index.entries.iter()) {
        if key.type_id != crate::casp::CASP_TYPE {
            continue;
        }
        if let Some(payload) = read_entry_payload(&mut file, entry) {
            return Ok(Some(payload));
        }
    }
    Ok(None)
}

/// Merge statistics — the receipt a merge owes its user.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeStats {
    pub sources: usize,
    pub resources_in: usize,
    pub resources_out: usize,
    /// Same-TGI collisions where a later file (game load order) won.
    pub collisions: usize,
}

/// Write a DBPF2 package: header, payloads, then a flags=0 index of
/// 32-byte entries — byte-for-byte the shape our reader (and fixtures)
/// specify. Entries are stored uncompressed; the game accepts that.
pub fn write_package(resources: &[((u32, u32, u64), Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![0u8; 96];
    out[0..4].copy_from_slice(b"DBPF");
    out[4..8].copy_from_slice(&2u32.to_le_bytes());
    out[8..12].copy_from_slice(&1u32.to_le_bytes()); // minor
    out[0x24..0x28].copy_from_slice(&(resources.len() as u32).to_le_bytes());
    out[0x3C..0x40].copy_from_slice(&3u32.to_le_bytes()); // index minor
    let mut placed: Vec<(u32, u32)> = Vec::new();
    for (_, payload) in resources {
        let pos = out.len() as u32;
        out.extend_from_slice(payload);
        placed.push((pos, payload.len() as u32));
    }
    let index_pos = out.len() as u32;
    out[0x40..0x44].copy_from_slice(&index_pos.to_le_bytes());
    let index_size = 4 + resources.len() * 32;
    out[0x2C..0x30].copy_from_slice(&(index_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // flags: no constants
    for (i, ((type_id, group, instance), _)) in resources.iter().enumerate() {
        let (pos, size) = placed[i];
        out.extend_from_slice(&type_id.to_le_bytes());
        out.extend_from_slice(&group.to_le_bytes());
        out.extend_from_slice(&((*instance >> 32) as u32).to_le_bytes());
        out.extend_from_slice(&(*instance as u32).to_le_bytes());
        out.extend_from_slice(&pos.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes()); // no bit31: uncompressed
        out.extend_from_slice(&size.to_le_bytes()); // mem_size = size
        out.extend_from_slice(&0u16.to_le_bytes()); // compression: none
        out.extend_from_slice(&1u16.to_le_bytes()); // committed
    }
    out
}

/// Merge packages in game load order (callers sort; later wins on TGI
/// collision, mirroring what the game already does with the loose
/// files). Every entry is decompressed on the way in — an entry we
/// can't read aborts the merge rather than silently dropping content.
pub fn merge_packages(paths: &[&Path]) -> Result<(Vec<u8>, MergeStats), DbpfError> {
    let mut map: std::collections::BTreeMap<(u32, u32, u64), Vec<u8>> = Default::default();
    let mut stats = MergeStats {
        sources: paths.len(),
        resources_in: 0,
        resources_out: 0,
        collisions: 0,
    };
    for path in paths {
        let index = read_package_index(path)?;
        let mut file = File::open(path)?;
        for (key, entry) in index.keys.iter().zip(index.entries.iter()) {
            let payload = read_entry_payload(&mut file, entry).ok_or_else(|| {
                DbpfError::CorruptIndex(format!(
                    "unreadable resource 0x{:08X} in {} — merge aborted, nothing written",
                    key.type_id,
                    path.display()
                ))
            })?;
            stats.resources_in += 1;
            let k = (key.type_id, key.group_id, key.instance);
            if map.insert(k, payload).is_some() {
                stats.collisions += 1;
            }
        }
    }
    stats.resources_out = map.len();
    let resources: Vec<((u32, u32, u64), Vec<u8>)> = map.into_iter().collect();
    let total: usize = resources.iter().map(|(_, p)| p.len()).sum();
    if total as u64 + (resources.len() as u64 * 32) + 200 > u32::MAX as u64 {
        return Err(DbpfError::CorruptIndex(
            "merged package would exceed the 4 GB DBPF limit".to_string(),
        ));
    }
    Ok((write_package(&resources), stats))
}

/// Undo EA's DST block-shuffle: a normal DDS header whose fourCC reads
/// DST1/DST5, with block fields split into planar streams for better LZ.
/// Stream order per the s4pi reference — DST1: [4B endpoints]×N then
/// [4B indices]×N; DST5: [2B alpha-endpoints]×N, [4B color-endpoints]×N,
/// [6B alpha-indices]×N, [4B color-indices]×N. Returns a decodable
/// DXT1/DXT5 DDS, or None for anything malformed.
fn unshuffle_dst(dds: &[u8]) -> Option<Vec<u8>> {
    if dds.len() < 128 || &dds[..4] != b"DDS " {
        return None;
    }
    let fourcc: [u8; 4] = dds[84..88].try_into().ok()?;
    let data = &dds[128..];
    let mut out = Vec::with_capacity(dds.len());
    out.extend_from_slice(&dds[..128]);
    match &fourcc {
        b"DST1" => {
            if data.is_empty() || data.len() % 8 != 0 {
                return None;
            }
            let n = data.len() / 8;
            let (s_end, s_idx) = (0usize, 4 * n);
            for i in 0..n {
                out.extend_from_slice(&data[s_end + 4 * i..s_end + 4 * i + 4]);
                out.extend_from_slice(&data[s_idx + 4 * i..s_idx + 4 * i + 4]);
            }
            out[84..88].copy_from_slice(b"DXT1");
        }
        b"DST5" => {
            if data.is_empty() || data.len() % 16 != 0 {
                return None;
            }
            let n = data.len() / 16;
            let (o_a, o_ce, o_ai, o_ci) = (0usize, 2 * n, 6 * n, 12 * n);
            for i in 0..n {
                out.extend_from_slice(&data[o_a + 2 * i..o_a + 2 * i + 2]);
                out.extend_from_slice(&data[o_ai + 6 * i..o_ai + 6 * i + 6]);
                out.extend_from_slice(&data[o_ce + 4 * i..o_ce + 4 * i + 4]);
                out.extend_from_slice(&data[o_ci + 4 * i..o_ci + 4 * i + 4]);
            }
            out[84..88].copy_from_slice(b"DXT5");
        }
        _ => return None,
    }
    Some(out)
}

/// Transcode a DDS thumbnail (DXT1/3/5 or uncompressed BGRA) to PNG so it
/// can render in a plain <img>. Anything unrecognized returns None and the
/// caller simply moves on — a wrong guess must never poison extraction.
fn dds_to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let dds = ddsfile::Dds::read(&mut std::io::Cursor::new(bytes)).ok()?;
    let w = dds.header.width as usize;
    let h = dds.header.height as usize;
    if w == 0 || h == 0 || w > 2048 || h > 2048 {
        return None;
    }
    let data = dds.get_data(0).ok()?;
    let mut rgba = vec![0u8; w * h * 4];
    let mut bc = |f: texpresso::Format, block: usize| -> Option<()> {
        let needed = w.div_ceil(4) * h.div_ceil(4) * block;
        if data.len() < needed {
            return None;
        }
        Some(f.decompress(data, w, h, &mut rgba))
    };
    match dds.get_d3d_format() {
        Some(ddsfile::D3DFormat::DXT1) => bc(texpresso::Format::Bc1, 8)?,
        Some(ddsfile::D3DFormat::DXT3) => bc(texpresso::Format::Bc2, 16)?,
        Some(ddsfile::D3DFormat::DXT5) => bc(texpresso::Format::Bc3, 16)?,
        Some(ddsfile::D3DFormat::A8R8G8B8) | Some(ddsfile::D3DFormat::X8R8G8B8) => {
            if data.len() < w * h * 4 {
                return None;
            }
            let opaque = matches!(
                dds.get_d3d_format(),
                Some(ddsfile::D3DFormat::X8R8G8B8)
            );
            for (i, px) in data.chunks_exact(4).take(w * h).enumerate() {
                rgba[i * 4] = px[2];
                rgba[i * 4 + 1] = px[1];
                rgba[i * 4 + 2] = px[0];
                rgba[i * 4 + 3] = if opaque { 255 } else { px[3] };
            }
        }
        _ => match dds.get_dxgi_format() {
            Some(ddsfile::DxgiFormat::BC1_UNorm) => bc(texpresso::Format::Bc1, 8)?,
            Some(ddsfile::DxgiFormat::BC3_UNorm) => bc(texpresso::Format::Bc3, 16)?,
            _ => return None,
        },
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod thumb_tests {
    use super::*;
    use std::io::Write;

    /// Byte-level package builder mirroring exactly what the parser reads:
    /// 96-byte header (magic, 2.0, count @0x24, index pos @0x40), payloads,
    /// then a flags=0 index of full 32-byte entries.
    fn build_package(resources: &[(u32, u16, &[u8], u32)]) -> Vec<u8> {
        let mut out = vec![0u8; 96];
        out[0..4].copy_from_slice(b"DBPF");
        out[4..8].copy_from_slice(&2u32.to_le_bytes());
        out[8..12].copy_from_slice(&0u32.to_le_bytes());
        out[0x24..0x28].copy_from_slice(&(resources.len() as u32).to_le_bytes());
        let mut placed: Vec<(u32, u32)> = Vec::new();
        for (_, _, payload, _) in resources {
            let pos = out.len() as u32;
            out.extend_from_slice(payload);
            placed.push((pos, payload.len() as u32));
        }
        let index_pos = out.len() as u32;
        out[0x40..0x44].copy_from_slice(&index_pos.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // flags: no constants
        for (i, (type_id, compression, _, mem_size)) in resources.iter().enumerate() {
            let (pos, size) = placed[i];
            out.extend_from_slice(&type_id.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // group
            out.extend_from_slice(&0u32.to_le_bytes()); // instance hi
            out.extend_from_slice(&(i as u32 + 1).to_le_bytes()); // instance lo
            out.extend_from_slice(&pos.to_le_bytes());
            let flagged = size | if *compression != 0 { 0x8000_0000 } else { 0 };
            out.extend_from_slice(&flagged.to_le_bytes());
            out.extend_from_slice(&mem_size.to_le_bytes());
            out.extend_from_slice(&compression.to_le_bytes());
            out.extend_from_slice(&1u16.to_le_bytes()); // committed
        }
        out
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut enc =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    fn png_bytes() -> Vec<u8> {
        let mut v = PNG_MAGIC.to_vec();
        v.extend_from_slice(b"fake-but-magic");
        v
    }

    fn jpg_bytes() -> Vec<u8> {
        let mut v = JPG_MAGIC.to_vec();
        v.extend_from_slice(b"jfif-ish");
        v
    }

    fn write_tmp(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.package");
        std::fs::write(&p, bytes).unwrap();
        (dir, p)
    }

    #[test]
    fn uncompressed_png_extracts_and_meta_round_trips() {
        let png = png_bytes();
        let pkg = build_package(&[(0x3C1A_F1F2, COMP_NONE, &png, png.len() as u32)]);
        let (_d, p) = write_tmp(&pkg);
        let idx = read_package_index(&p).unwrap();
        assert_eq!(idx.entries.len(), 1);
        assert_eq!(idx.entries[0].compression, COMP_NONE);
        assert_eq!(idx.entries[0].size as usize, png.len());
        let (bytes, ext) = extract_thumbnail(&p).unwrap().unwrap();
        assert_eq!(ext, "png");
        assert_eq!(bytes, png);
    }

    #[test]
    fn zlib_jpeg_decompresses_by_declared_codec() {
        let jpg = jpg_bytes();
        let packed = zlib(&jpg);
        let pkg = build_package(&[(0x2F7D_0004, COMP_ZLIB, &packed, jpg.len() as u32)]);
        let (_d, p) = write_tmp(&pkg);
        let (bytes, ext) = extract_thumbnail(&p).unwrap().unwrap();
        assert_eq!(ext, "jpg");
        assert_eq!(bytes, jpg);
    }

    #[test]
    fn preference_order_picks_the_thumbnail_type_first() {
        let png = png_bytes();
        let jpg = jpg_bytes();
        let pkg = build_package(&[
            (0x2F7D_0004, COMP_NONE, &jpg, jpg.len() as u32),
            (0x3C1A_F1F2, COMP_NONE, &png, png.len() as u32),
        ]);
        let (_d, p) = write_tmp(&pkg);
        let (_, ext) = extract_thumbnail(&p).unwrap().unwrap();
        assert_eq!(ext, "png", "0x3C1AF1F2 outranks 0x2F7D0004");
    }

    fn dds_dxt5_bytes(rgba: [u8; 4], w: usize, h: usize) -> Vec<u8> {
        let pixels = vec![rgba; w * h].concat();
        let size = texpresso::Format::Bc3.compressed_size(w, h);
        let mut compressed = vec![0u8; size];
        texpresso::Format::Bc3.compress(
            &pixels,
            w,
            h,
            texpresso::Params::default(),
            &mut compressed,
        );
        let mut dds = ddsfile::Dds::new_d3d(ddsfile::NewD3dParams {
            height: h as u32,
            width: w as u32,
            depth: None,
            format: ddsfile::D3DFormat::DXT5,
            mipmap_levels: None,
            caps2: None,
        })
        .unwrap();
        dds.data = compressed;
        let mut out = Vec::new();
        dds.write(&mut out).unwrap();
        out
    }

    fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        (info.width, info.height, buf)
    }

    fn dds_dxt1_bytes(rgba: [u8; 4], w: usize, h: usize) -> Vec<u8> {
        let pixels = vec![rgba; w * h].concat();
        let size = texpresso::Format::Bc1.compressed_size(w, h);
        let mut compressed = vec![0u8; size];
        texpresso::Format::Bc1.compress(
            &pixels,
            w,
            h,
            texpresso::Params::default(),
            &mut compressed,
        );
        let mut dds = ddsfile::Dds::new_d3d(ddsfile::NewD3dParams {
            height: h as u32,
            width: w as u32,
            depth: None,
            format: ddsfile::D3DFormat::DXT1,
            mipmap_levels: None,
            caps2: None,
        })
        .unwrap();
        dds.data = compressed;
        let mut out = Vec::new();
        dds.write(&mut out).unwrap();
        out
    }

    /// The reference's forward shuffle, so unshuffle is tested against the
    /// documented transform rather than against itself.
    fn shuffle_reference(dds: &[u8]) -> Vec<u8> {
        let data = &dds[128..];
        let mut out = dds[..128].to_vec();
        match &dds[84..88] {
            b"DXT1" => {
                let n = data.len() / 8;
                let (mut ends, mut idxs) = (Vec::new(), Vec::new());
                for b in data.chunks_exact(8) {
                    ends.extend_from_slice(&b[..4]);
                    idxs.extend_from_slice(&b[4..]);
                }
                out.extend(ends);
                out.extend(idxs);
                out[84..88].copy_from_slice(b"DST1");
            }
            b"DXT5" => {
                let n = data.len() / 16;
                let _ = n;
                let (mut a, mut ai, mut ce, mut ci) =
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new());
                for b in data.chunks_exact(16) {
                    a.extend_from_slice(&b[..2]);
                    ai.extend_from_slice(&b[2..8]);
                    ce.extend_from_slice(&b[8..12]);
                    ci.extend_from_slice(&b[12..16]);
                }
                out.extend(a);
                out.extend(ce);
                out.extend(ai);
                out.extend(ci);
                out[84..88].copy_from_slice(b"DST5");
            }
            _ => unreachable!(),
        }
        out
    }

    #[test]
    fn merge_roundtrips_with_load_order_winners() {
        // Three sources; pkg1 and pkg3 collide on (T,0,1) — pkg3 is later
        // in load order and must win. pkg2's entry arrives zlib-compressed
        // and must come out decompressed and byte-faithful.
        let t = 0x1234_5678u32;
        let a = build_package(&[(t, 0, b"payload-A-first", 15)]);
        let raw = b"payload-B-compressed";
        let z = zlib(raw);
        let b = build_package(&[(0x2222_2222, 0x5A42, &z, raw.len() as u32)]);
        let c = build_package(&[(t, 0, b"payload-C-wins!", 15)]);
        let (da, pa) = write_tmp(&a);
        let (db_, pb) = write_tmp(&b);
        let (dc, pc) = write_tmp(&c);
        let (bytes, stats) =
            merge_packages(&[pa.as_path(), pb.as_path(), pc.as_path()]).unwrap();
        assert_eq!(stats.sources, 3);
        assert_eq!(stats.resources_in, 3);
        assert_eq!(stats.resources_out, 2);
        assert_eq!(stats.collisions, 1);
        let (dm, pm) = write_tmp(&bytes);
        let idx = read_package_index(&pm).unwrap();
        assert_eq!(idx.keys.len(), 2);
        let mut file = File::open(&pm).unwrap();
        let mut found = std::collections::HashMap::new();
        for (k, e) in idx.keys.iter().zip(idx.entries.iter()) {
            found.insert(k.type_id, read_entry_payload(&mut file, e).unwrap());
        }
        assert_eq!(found[&t], b"payload-C-wins!".to_vec(), "later load order wins");
        assert_eq!(found[&0x2222_2222], raw.to_vec(), "compressed source decompressed");
        drop((da, db_, dc, dm));
    }

    #[test]
    fn merged_packages_serve_thumbnails() {
        let png = png_bytes();
        let src = build_package(&[(0x3C1A_F1F2, 0, &png, png.len() as u32)]);
        let (_d1, p1) = write_tmp(&src);
        let (bytes, _) = merge_packages(&[p1.as_path()]).unwrap();
        let (_d2, p2) = write_tmp(&bytes);
        let (thumb, ext) = extract_thumbnail(&p2).unwrap().unwrap();
        assert_eq!(ext, "png");
        assert_eq!(thumb, png);
    }

    #[test]
    fn empty_merge_writes_a_readable_shell() {
        let bytes = write_package(&[]);
        let (_d, p) = write_tmp(&bytes);
        let idx = read_package_index(&p).unwrap();
        assert!(idx.keys.is_empty());
    }

    #[test]
    fn dst_shuffled_images_unshuffle_and_decode() {
        for (dds, expect) in [
            (dds_dxt5_bytes([64, 140, 200, 255], 8, 8), [64u8, 140, 200]),
            (dds_dxt1_bytes([200, 90, 40, 255], 8, 8), [200, 90, 40]),
        ] {
            let dst = shuffle_reference(&dds);
            assert_ne!(&dst[128..], &dds[128..], "shuffle actually reorders");
            let pkg = build_package(&[(0x00B2_D882, COMP_NONE, &dst, dst.len() as u32)]);
            let (_d, p) = write_tmp(&pkg);
            let (bytes, ext) = extract_thumbnail(&p).unwrap().unwrap();
            assert_eq!(ext, "png");
            let (w, h, px) = decode_png(&bytes);
            assert_eq!((w, h), (8, 8));
            for c in 0..3 {
                assert!(
                    (i32::from(px[c]) - i32::from(expect[c])).abs() <= 12,
                    "channel {c}: {} vs {}",
                    px[c],
                    expect[c]
                );
            }
        }
    }

    #[test]
    fn malformed_dst_falls_through() {
        let mut dds = dds_dxt5_bytes([10, 10, 10, 255], 8, 8);
        dds[84..88].copy_from_slice(b"DST5");
        dds.truncate(dds.len() - 3); // len % 16 broken
        let pkg = build_package(&[(0x00B2_D882, COMP_NONE, &dds, dds.len() as u32)]);
        let (_d, p) = write_tmp(&pkg);
        assert!(extract_thumbnail(&p).unwrap().is_none());
    }

    #[test]
    fn dds_thumbnails_transcode_to_png() {
        let dds = dds_dxt5_bytes([180, 60, 90, 255], 8, 8);
        let packed = zlib(&dds);
        let pkg = build_package(&[(0x3453_CF95, COMP_ZLIB, &packed, dds.len() as u32)]);
        let (_d, p) = write_tmp(&pkg);
        let (bytes, ext) = extract_thumbnail(&p).unwrap().unwrap();
        assert_eq!(ext, "png");
        let (w, h, pixels) = decode_png(&bytes);
        assert_eq!((w, h), (8, 8));
        // BC3 is lossy; a solid color survives within a small tolerance.
        assert!((i32::from(pixels[0]) - 180).abs() <= 10, "r = {}", pixels[0]);
        assert!((i32::from(pixels[1]) - 60).abs() <= 10, "g = {}", pixels[1]);
        assert!((i32::from(pixels[2]) - 90).abs() <= 10, "b = {}", pixels[2]);
        assert_eq!(pixels[3], 255);
    }

    #[test]
    fn png_bearing_types_still_outrank_dds() {
        let png_res = png_bytes();
        let dds = dds_dxt5_bytes([10, 200, 10, 255], 8, 8);
        let pkg = build_package(&[
            (0x3453_CF95, COMP_NONE, &dds, dds.len() as u32),
            (0x3C1A_F1F2, COMP_NONE, &png_res, png_res.len() as u32),
        ]);
        let (_d, p) = write_tmp(&pkg);
        let (bytes, _) = extract_thumbnail(&p).unwrap().unwrap();
        assert_eq!(bytes, png_res, "the dedicated thumbnail type wins");
    }

    #[test]
    fn corrupt_and_foreign_payloads_fall_through_to_none() {
        let garbage = b"not-zlib-at-all".to_vec();
        let dds = b"DDS |not-extractable".to_vec();
        let pkg = build_package(&[
            (0x3C1A_F1F2, COMP_ZLIB, &garbage, 64),
            (0x3453_CF95, COMP_NONE, &dds, dds.len() as u32),
        ]);
        let (_d, p) = write_tmp(&pkg);
        assert!(
            extract_thumbnail(&p).unwrap().is_none(),
            "a DDS-labeled payload that isn't real DDS is skipped, not fatal"
        );
    }
}
