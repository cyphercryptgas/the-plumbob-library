//! Exact duplicate detection.
//!
//! Files are pre-grouped by size (an optimization *and* a correctness
//! backstop — differing sizes can never be identical content), then grouped
//! by SHA-256. Each group carries an explained recommendation for which copy
//! to retain, following the spec's rule order:
//!
//!   1. the copy registered to a mod manifest
//!   2. the copy already in its expected category location
//!   3. the copy with the cleanest path
//!   4. otherwise, the oldest copy
//!
//! Nothing here deletes anything. Groups feed a reviewable quarantine plan.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// The facts the recommender is allowed to consider. Populated by the
/// service layer from the database; the recommender itself never touches
/// the filesystem.
#[derive(Clone, Debug)]
pub struct FileFacts {
    pub id: i64,
    pub relative_path: PathBuf,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub modified_at: Option<DateTime<Utc>>,
    pub first_seen_at: Option<DateTime<Utc>>,
    /// True when the file is listed in an installation manifest.
    pub manifest_associated: bool,
    /// True when the file already sits in the category folder the library
    /// expects for its assigned category.
    pub in_expected_category: bool,
}

#[derive(Serialize, Debug)]
pub struct DuplicateGroup {
    pub sha256: String,
    pub size_bytes: u64,
    pub file_ids: Vec<i64>,
    pub recommended_keep: i64,
    pub recommendation_reason: String,
    pub reclaimable_bytes: u64,
}

pub fn group_exact(files: &[FileFacts]) -> Vec<DuplicateGroup> {
    let mut by_size: HashMap<u64, Vec<&FileFacts>> = HashMap::new();
    for f in files {
        // Zero-byte files are all "identical"; they are handled by the
        // zero-byte finding, not the duplicate engine.
        if f.sha256.is_some() && f.size_bytes > 0 {
            by_size.entry(f.size_bytes).or_default().push(f);
        }
    }

    let mut groups = Vec::new();
    for (size, bucket) in by_size {
        if bucket.len() < 2 {
            continue;
        }
        let mut by_hash: HashMap<&str, Vec<&FileFacts>> = HashMap::new();
        for f in &bucket {
            by_hash
                .entry(f.sha256.as_deref().expect("filtered above"))
                .or_default()
                .push(f);
        }
        for (hash, members) in by_hash {
            if members.len() < 2 {
                continue;
            }
            let (keep, reason) = recommend(&members);
            let mut file_ids: Vec<i64> = members.iter().map(|f| f.id).collect();
            file_ids.sort_unstable();
            groups.push(DuplicateGroup {
                sha256: hash.to_string(),
                size_bytes: size,
                reclaimable_bytes: size * (members.len() as u64 - 1),
                recommended_keep: keep,
                recommendation_reason: reason,
                file_ids,
            });
        }
    }

    groups.sort_by(|a, b| {
        b.reclaimable_bytes
            .cmp(&a.reclaimable_bytes)
            .then_with(|| a.sha256.cmp(&b.sha256))
    });
    groups
}

/// Rule cascade. Each rule narrows the candidate set; the first rule that
/// narrows it to exactly one file decides (and names) the recommendation.
fn recommend<'a>(members: &[&'a FileFacts]) -> (i64, String) {
    let mut candidates: Vec<&FileFacts> = members.to_vec();

    let manifest: Vec<&FileFacts> = candidates
        .iter()
        .copied()
        .filter(|f| f.manifest_associated)
        .collect();
    if manifest.len() == 1 {
        return (
            manifest[0].id,
            "kept the copy registered to a mod manifest".into(),
        );
    }
    if !manifest.is_empty() {
        candidates = manifest;
    }

    let categorized: Vec<&FileFacts> = candidates
        .iter()
        .copied()
        .filter(|f| f.in_expected_category)
        .collect();
    if categorized.len() == 1 {
        return (
            categorized[0].id,
            "kept the copy already in its expected category folder".into(),
        );
    }
    if !categorized.is_empty() {
        candidates = categorized;
    }

    let best_path = candidates
        .iter()
        .map(|f| path_untidiness(f))
        .min()
        .expect("group has members");
    let cleanest: Vec<&FileFacts> = candidates
        .iter()
        .copied()
        .filter(|f| path_untidiness(f) == best_path)
        .collect();
    if cleanest.len() == 1 {
        return (
            cleanest[0].id,
            "kept the copy with the cleanest path (no download-duplicate markers, tidiest location)"
                .into(),
        );
    }
    candidates = cleanest;

    let oldest = candidates
        .iter()
        .copied()
        .min_by_key(|f| {
            (
                f.first_seen_at
                    .or(f.modified_at)
                    .unwrap_or(DateTime::<Utc>::MAX_UTC),
                f.id,
            )
        })
        .expect("group has members");
    (
        oldest.id,
        "rules tied — kept the oldest known copy".into(),
    )
}

/// Lower is cleaner. Browser-duplicate markers like `" (1)"` or `"copy"`
/// in the filename dominate — a marked file is never preferred over a
/// clean-named copy no matter how shallow it sits (a `Downloads/file (1)`
/// straggler must not outrank the tidy library copy). Depth breaks ties
/// among equally-clean names; the shorter name breaks remaining ties.
fn path_untidiness(f: &FileFacts) -> (u32, usize, usize) {
    let depth = f.relative_path.components().count().saturating_sub(1);
    let name = f
        .relative_path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut penalty = 0u32;
    if name.contains("copy") {
        penalty += 1;
    }
    if has_paren_number(&name) {
        penalty += 1;
    }
    (penalty, depth, name.len())
}

fn has_paren_number(name: &str) -> bool {
    // Matches "... (1)", "...(23)" style browser duplicates without regex.
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            let mut j = i + 1;
            let mut digits = 0;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                digits += 1;
                j += 1;
            }
            if digits > 0 && j < bytes.len() && bytes[j] == b')' {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn facts(id: i64, path: &str, size: u64, hash: &str) -> FileFacts {
        FileFacts {
            id,
            relative_path: PathBuf::from(path),
            size_bytes: size,
            sha256: Some(hash.to_string()),
            modified_at: None,
            first_seen_at: None,
            manifest_associated: false,
            in_expected_category: false,
        }
    }

    #[test]
    fn same_content_different_names_are_grouped() {
        let files = vec![
            facts(1, "hair.package", 100, "aaa"),
            facts(2, "Downloads/hair (1).package", 100, "aaa"),
            facts(3, "unrelated.package", 100, "bbb"),
        ];
        let groups = group_exact(&files);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].file_ids, vec![1, 2]);
        assert_eq!(groups[0].reclaimable_bytes, 100);
    }

    #[test]
    fn same_size_different_content_is_never_grouped() {
        let files = vec![
            facts(1, "a.package", 64, "aaa"),
            facts(2, "b.package", 64, "bbb"),
        ];
        assert!(group_exact(&files).is_empty());
    }

    #[test]
    fn unhashed_and_zero_byte_files_are_ignored() {
        let mut unhashed = facts(1, "a.package", 64, "aaa");
        unhashed.sha256 = None;
        let files = vec![
            unhashed,
            facts(2, "b.package", 64, "aaa"),
            facts(3, "z1.package", 0, "zzz"),
            facts(4, "z2.package", 0, "zzz"),
        ];
        assert!(group_exact(&files).is_empty());
    }

    #[test]
    fn manifest_association_wins() {
        let mut a = facts(1, "somewhere/deep/hair.package", 100, "aaa");
        a.manifest_associated = true;
        let b = facts(2, "hair.package", 100, "aaa");
        let groups = group_exact(&[a, b]);
        assert_eq!(groups[0].recommended_keep, 1);
        assert!(groups[0].recommendation_reason.contains("manifest"));
    }

    #[test]
    fn expected_category_wins_when_no_manifest() {
        let a = facts(1, "hair.package", 100, "aaa");
        let mut b = facts(2, "03_CAS/Hair/hair.package", 100, "aaa");
        b.in_expected_category = true;
        let groups = group_exact(&[a, b]);
        assert_eq!(groups[0].recommended_keep, 2);
        assert!(groups[0].recommendation_reason.contains("category"));
    }

    #[test]
    fn cleanest_path_beats_browser_duplicates() {
        let a = facts(1, "hair.package", 100, "aaa");
        let b = facts(2, "New folder/hair copy (1).package", 100, "aaa");
        let groups = group_exact(&[a, b]);
        assert_eq!(groups[0].recommended_keep, 1);
        assert!(groups[0].recommendation_reason.contains("cleanest"));
    }

    #[test]
    fn marker_free_name_beats_shallower_junk_copy() {
        // Regression (found in demo-library validation): a "(1)"- or
        // "copy"-marked file that happens to sit shallower — e.g. straight
        // in Downloads/ — must never be recommended over a clean-named copy
        // in a tidy, deeper location.
        let junk = facts(
            1,
            "demo-library/Downloads/pixelpetal-wavy-bob (1).package",
            100,
            "aaa",
        );
        let clean = facts(
            2,
            "demo-library/CAS/Hair/PixelPetal/pixelpetal-wavy-bob.package",
            100,
            "aaa",
        );
        let groups = group_exact(&[junk, clean]);
        assert_eq!(groups[0].recommended_keep, 2);
        assert!(groups[0].recommendation_reason.contains("cleanest"));

        let junk2 = facts(
            3,
            "demo-library/Unsorted/sundayseam-cardigan copy.package",
            200,
            "bbb",
        );
        let clean2 = facts(
            4,
            "demo-library/CAS/Clothing/SundaySeam/sundayseam-cardigan.package",
            200,
            "bbb",
        );
        let groups2 = group_exact(&[junk2, clean2]);
        assert_eq!(groups2[0].recommended_keep, 4);
    }

    #[test]
    fn full_tie_falls_back_to_oldest() {
        let mut a = facts(1, "a.package", 100, "aaa");
        a.first_seen_at = Some(Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap());
        let mut b = facts(2, "b.package", 100, "aaa");
        b.first_seen_at = Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        let groups = group_exact(&[a, b]);
        assert_eq!(groups[0].recommended_keep, 2);
        assert!(groups[0].recommendation_reason.contains("oldest"));
    }

    #[test]
    fn groups_sort_by_recoverable_space() {
        let files = vec![
            facts(1, "small-a.package", 10, "s"),
            facts(2, "small-b.package", 10, "s"),
            facts(3, "big-a.package", 1000, "b"),
            facts(4, "big-b.package", 1000, "b"),
            facts(5, "big-c.package", 1000, "b"),
        ];
        let groups = group_exact(&files);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].reclaimable_bytes, 2000);
        assert_eq!(groups[1].reclaimable_bytes, 10);
    }
}
