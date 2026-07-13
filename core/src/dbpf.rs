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
const THUMB_TYPES: [u32; 3] = [0x3C1A_F1F2, 0x5B28_2D45, 0x2F7D_0004];

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
            if u64::from(entry.size) > MAX_THUMB_BYTES
                || u64::from(entry.mem_size) > MAX_THUMB_BYTES
            {
                continue;
            }
            if file.seek(SeekFrom::Start(u64::from(entry.position))).is_err() {
                continue;
            }
            let mut raw = vec![0u8; entry.size as usize];
            if file.read_exact(&mut raw).is_err() {
                continue;
            }
            let payload: Vec<u8> = match entry.compression {
                COMP_NONE => raw,
                COMP_ZLIB => {
                    use std::io::Read as _;
                    let mut out = Vec::with_capacity(entry.mem_size as usize);
                    let mut dec = flate2::read::ZlibDecoder::new(raw.as_slice());
                    match dec
                        .by_ref()
                        .take(MAX_THUMB_BYTES)
                        .read_to_end(&mut out)
                    {
                        Ok(_) => out,
                        Err(_) => continue,
                    }
                }
                _ => continue,
            };
            if payload.len() >= 4 && payload[..4] == PNG_MAGIC {
                return Ok(Some((payload, "png")));
            }
            if payload.len() >= 3 && payload[..3] == JPG_MAGIC {
                return Ok(Some((payload, "jpg")));
            }
        }
    }
    Ok(None)
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

    #[test]
    fn corrupt_and_foreign_payloads_fall_through_to_none() {
        let garbage = b"not-zlib-at-all".to_vec();
        let dds = b"DDS |not-extractable".to_vec();
        let pkg = build_package(&[
            (0x3C1A_F1F2, COMP_ZLIB, &garbage, 64),
            (0x3453_CF95, COMP_NONE, &dds, dds.len() as u32),
        ]);
        let (_d, p) = write_tmp(&pkg);
        assert!(extract_thumbnail(&p).unwrap().is_none());
    }
}
