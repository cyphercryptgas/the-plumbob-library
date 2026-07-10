//! Recursive Mods-folder scanner.
//!
//! The scanner is read-only by construction: it takes a [`SafeRoot`], walks
//! it without following symlinks, and returns a [`ScanReport`] of metadata.
//! Hashing is a separate composable pass ([`hash_files`]) so the service
//! layer can decide when to pay for it (first scan: everything; incremental
//! re-scan: only files whose size/mtime changed).

use crate::hashing::sha256_file_observed;
use crate::paths::SafeRoot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Deterministic classification by extension. This is a *fact* about the
/// file's name, never a claim about its validity — package parsing belongs to
/// the (flagged) Phase-Two analyzer.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Package,
    Ts4Script,
    ArchiveZip,
    ArchiveRar,
    Archive7z,
    Image,
    Document,
    Config,
    Unsupported,
}

pub fn classify(extension: Option<&str>) -> FileKind {
    let ext = match extension {
        Some(e) => e.to_ascii_lowercase(),
        None => return FileKind::Unsupported,
    };
    match ext.as_str() {
        "package" => FileKind::Package,
        "ts4script" => FileKind::Ts4Script,
        "zip" => FileKind::ArchiveZip,
        "rar" => FileKind::ArchiveRar,
        "7z" => FileKind::Archive7z,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => FileKind::Image,
        "txt" | "md" | "rtf" | "pdf" | "doc" | "docx" | "html" | "htm" => FileKind::Document,
        "cfg" | "ini" | "json" | "xml" | "yaml" | "yml" | "toml" | "log" => FileKind::Config,
        _ => FileKind::Unsupported,
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ScannedFile {
    pub absolute_path: PathBuf,
    pub relative_path: PathBuf,
    pub file_name: String,
    pub extension: Option<String>,
    pub kind: FileKind,
    pub size_bytes: u64,
    pub modified_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    /// Number of directories between the root and this file
    /// (a file directly inside the root has depth 0).
    pub depth: usize,
    pub zero_byte: bool,
    /// `.ts4script` deeper than the configured limit — a warning, not a
    /// verdict; script loading depth depends on the game's Resource.cfg.
    pub deep_script: bool,
    /// Filled by [`hash_files`]; `None` until hashed.
    pub sha256: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ScanIssue {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Serialize, Debug, Default)]
pub struct ScanReport {
    pub files: Vec<ScannedFile>,
    pub empty_dirs: Vec<PathBuf>,
    pub symlinks_skipped: Vec<PathBuf>,
    pub errors: Vec<ScanIssue>,
    pub cancelled: bool,
    pub total_bytes: u64,
    pub duration_ms: u128,
}

#[derive(Clone, Debug)]
pub struct ScanOptions {
    /// Root-relative directory prefixes to skip entirely.
    pub excluded_relative: Vec<PathBuf>,
    /// Deepest directory level (relative to the root) at which a
    /// `.ts4script` is considered safely loadable. The game's default
    /// Resource.cfg historically loads scripts at most one subfolder deep,
    /// so the default here is 1. Configurable because users edit
    /// Resource.cfg; documented in docs/SAFETY_MODEL.md.
    pub script_depth_limit: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            excluded_relative: Vec::new(),
            script_depth_limit: 1,
        }
    }
}

pub struct Progress<'a> {
    pub files_seen: u64,
    pub bytes_seen: u64,
    pub current: &'a Path,
}

/// Walk the root and collect metadata. Never mutates. Never follows symlinks.
/// Nonfatal problems (unreadable entries, permission failures) are collected
/// into `errors` and the walk continues; cancellation returns a partial
/// report flagged `cancelled`.
pub fn scan(
    root: &SafeRoot,
    opts: &ScanOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&Progress),
) -> ScanReport {
    let started = Instant::now();
    let mut report = ScanReport::default();
    let root_path = root.path().to_path_buf();
    let excluded = opts.excluded_relative.clone();

    let walker = walkdir::WalkDir::new(&root_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(move |e| {
            if e.depth() == 0 {
                return true;
            }
            match e.path().strip_prefix(&root_path) {
                Ok(rel) => !excluded.iter().any(|ex| rel.starts_with(ex)),
                Err(_) => true,
            }
        });

    let mut files_seen: u64 = 0;

    for entry in walker {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                report.errors.push(ScanIssue {
                    path: err.path().map(Path::to_path_buf).unwrap_or_default(),
                    message: err.to_string(),
                });
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let path = entry.path().to_path_buf();

        if entry.path_is_symlink() {
            // Symlinks are recorded and skipped: following them risks walking
            // (or later mutating) content outside the approved root.
            report.symlinks_skipped.push(path);
            continue;
        }

        if entry.file_type().is_dir() {
            match std::fs::read_dir(&path) {
                Ok(mut rd) => {
                    if rd.next().is_none() {
                        report.empty_dirs.push(path);
                    }
                }
                Err(err) => report.errors.push(ScanIssue {
                    path,
                    message: format!("could not inspect directory: {err}"),
                }),
            }
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(err) => {
                report.errors.push(ScanIssue {
                    path,
                    message: format!("could not read metadata: {err}"),
                });
                continue;
            }
        };

        let relative_path = path
            .strip_prefix(root.path())
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.clone());
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        let kind = classify(extension.as_deref());
        let size_bytes = meta.len();
        let depth = entry.depth().saturating_sub(1);

        let scanned = ScannedFile {
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            modified_at: meta.modified().ok().map(DateTime::<Utc>::from),
            created_at: meta.created().ok().map(DateTime::<Utc>::from),
            zero_byte: size_bytes == 0,
            deep_script: kind == FileKind::Ts4Script && depth > opts.script_depth_limit,
            absolute_path: path,
            relative_path,
            extension,
            kind,
            size_bytes,
            depth,
            sha256: None,
        };

        files_seen += 1;
        report.total_bytes += size_bytes;
        progress(&Progress {
            files_seen,
            bytes_seen: report.total_bytes,
            current: &scanned.absolute_path,
        });
        report.files.push(scanned);
    }

    report.duration_ms = started.elapsed().as_millis();
    report
}

/// Streaming hash pass over scanned files that do not yet have a hash.
/// Returns nonfatal per-file errors; cancellation leaves remaining files
/// unhashed (their `sha256` stays `None`).
pub fn hash_files(
    files: &mut [ScannedFile],
    cancel: &AtomicBool,
    mut progress: impl FnMut(&Progress),
) -> Vec<ScanIssue> {
    let mut errors = Vec::new();
    let mut files_seen: u64 = 0;
    let mut bytes_seen: u64 = 0;
    for f in files.iter_mut().filter(|f| f.sha256.is_none()) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match sha256_file_observed(&f.absolute_path, cancel, |n| bytes_seen += n) {
            Ok(Some(hash)) => {
                f.sha256 = Some(hash);
                files_seen += 1;
                progress(&Progress {
                    files_seen,
                    bytes_seen,
                    current: &f.absolute_path,
                });
            }
            Ok(None) => break, // cancelled mid-file
            Err(err) => errors.push(ScanIssue {
                path: f.absolute_path.clone(),
                message: format!("hashing failed: {err}"),
            }),
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Builds the fixture tree used by most scanner tests:
    ///
    /// ```text
    /// <root>/
    ///   top.package
    ///   top.ts4script                      (depth 0 — safe)
    ///   empty.package                      (zero bytes)
    ///   archive.zip  bundle.rar  packed.7z
    ///   preview.png  readme.txt  Resource.cfg
    ///   mystery.xyz                        (unsupported)
    ///   CAS/Hair/curls.package             (depth 2)
    ///   Scripts/deep/nested/mod.ts4script  (depth 3 — deep)
    ///   EmptyDir/
    ///   Disabled/skip.package              (excluded in some tests)
    /// ```
    fn build_fixture(root: &Path) {
        fs::write(root.join("top.package"), b"pkg-top").unwrap();
        fs::write(root.join("top.ts4script"), b"script-top").unwrap();
        fs::write(root.join("empty.package"), b"").unwrap();
        fs::write(root.join("archive.zip"), b"zip!").unwrap();
        fs::write(root.join("bundle.rar"), b"rar!").unwrap();
        fs::write(root.join("packed.7z"), b"7z!!").unwrap();
        fs::write(root.join("preview.png"), b"png-bytes").unwrap();
        fs::write(root.join("readme.txt"), b"docs").unwrap();
        fs::write(root.join("Resource.cfg"), b"Priority 500").unwrap();
        fs::write(root.join("mystery.xyz"), b"???").unwrap();
        fs::create_dir_all(root.join("CAS").join("Hair")).unwrap();
        fs::write(root.join("CAS/Hair/curls.package"), b"pkg-curls").unwrap();
        fs::create_dir_all(root.join("Scripts/deep/nested")).unwrap();
        fs::write(root.join("Scripts/deep/nested/mod.ts4script"), b"deep").unwrap();
        fs::create_dir(root.join("EmptyDir")).unwrap();
        fs::create_dir(root.join("Disabled")).unwrap();
        fs::write(root.join("Disabled/skip.package"), b"disabled").unwrap();
    }

    fn scan_fixture(opts: &ScanOptions) -> (tempfile::TempDir, ScanReport) {
        let tmp = tempfile::tempdir().unwrap();
        build_fixture(tmp.path());
        let root = SafeRoot::new(tmp.path()).unwrap();
        let cancel = AtomicBool::new(false);
        let report = scan(&root, opts, &cancel, |_| {});
        (tmp, report)
    }

    fn kind_of<'a>(report: &'a ScanReport, name: &str) -> &'a ScannedFile {
        report
            .files
            .iter()
            .find(|f| f.file_name == name)
            .unwrap_or_else(|| panic!("{name} not found in scan"))
    }

    #[test]
    fn classifies_every_recognized_extension() {
        let (_tmp, report) = scan_fixture(&ScanOptions::default());
        assert_eq!(kind_of(&report, "top.package").kind, FileKind::Package);
        assert_eq!(kind_of(&report, "top.ts4script").kind, FileKind::Ts4Script);
        assert_eq!(kind_of(&report, "archive.zip").kind, FileKind::ArchiveZip);
        assert_eq!(kind_of(&report, "bundle.rar").kind, FileKind::ArchiveRar);
        assert_eq!(kind_of(&report, "packed.7z").kind, FileKind::Archive7z);
        assert_eq!(kind_of(&report, "preview.png").kind, FileKind::Image);
        assert_eq!(kind_of(&report, "readme.txt").kind, FileKind::Document);
        assert_eq!(kind_of(&report, "Resource.cfg").kind, FileKind::Config);
        assert_eq!(kind_of(&report, "mystery.xyz").kind, FileKind::Unsupported);
    }

    #[test]
    fn classification_is_case_insensitive_on_extensions() {
        assert_eq!(classify(Some("PACKAGE")), FileKind::Package);
        assert_eq!(classify(Some("Ts4Script")), FileKind::Ts4Script);
        assert_eq!(classify(None), FileKind::Unsupported);
    }

    #[test]
    fn flags_zero_byte_files() {
        let (_tmp, report) = scan_fixture(&ScanOptions::default());
        assert!(kind_of(&report, "empty.package").zero_byte);
        assert!(!kind_of(&report, "top.package").zero_byte);
    }

    #[test]
    fn detects_empty_directories() {
        let (_tmp, report) = scan_fixture(&ScanOptions::default());
        assert_eq!(report.empty_dirs.len(), 1);
        assert!(report.empty_dirs[0].ends_with("EmptyDir"));
    }

    #[test]
    fn computes_depth_below_root() {
        let (_tmp, report) = scan_fixture(&ScanOptions::default());
        assert_eq!(kind_of(&report, "top.package").depth, 0);
        assert_eq!(kind_of(&report, "curls.package").depth, 2);
        assert_eq!(kind_of(&report, "mod.ts4script").depth, 3);
    }

    #[test]
    fn flags_only_scripts_beyond_the_depth_limit() {
        let (_tmp, report) = scan_fixture(&ScanOptions::default());
        assert!(kind_of(&report, "mod.ts4script").deep_script);
        assert!(!kind_of(&report, "top.ts4script").deep_script);
        // Packages are never depth-flagged by this rule.
        assert!(!kind_of(&report, "curls.package").deep_script);
    }

    #[test]
    fn respects_directory_exclusions() {
        let opts = ScanOptions {
            excluded_relative: vec![PathBuf::from("Disabled")],
            ..ScanOptions::default()
        };
        let (_tmp, report) = scan_fixture(&opts);
        assert!(report.files.iter().all(|f| f.file_name != "skip.package"));
        // The excluded directory is not reported as empty either — it was
        // never inspected.
        assert!(report.empty_dirs.iter().all(|d| !d.ends_with("Disabled")));
    }

    #[test]
    fn relative_paths_are_root_relative() {
        let (_tmp, report) = scan_fixture(&ScanOptions::default());
        let curls = kind_of(&report, "curls.package");
        assert_eq!(curls.relative_path, PathBuf::from("CAS/Hair/curls.package"));
        assert!(curls.absolute_path.is_absolute());
    }

    #[test]
    fn cancellation_returns_partial_flagged_report() {
        let tmp = tempfile::tempdir().unwrap();
        build_fixture(tmp.path());
        let root = SafeRoot::new(tmp.path()).unwrap();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let report = scan(&root, &ScanOptions::default(), &cancel, move |p| {
            if p.files_seen >= 2 {
                c2.store(true, Ordering::Relaxed);
            }
        });
        assert!(report.cancelled);
        assert!(report.files.len() >= 2);
        assert!(report.files.len() < 13, "cancellation should stop the walk early");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_recorded_and_never_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("real.package"), b"outside").unwrap();
        build_fixture(tmp.path());
        std::os::unix::fs::symlink(
            outside.path().join("real.package"),
            tmp.path().join("sneaky.package"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), tmp.path().join("sneaky-dir")).unwrap();

        let root = SafeRoot::new(tmp.path()).unwrap();
        let cancel = AtomicBool::new(false);
        let report = scan(&root, &ScanOptions::default(), &cancel, |_| {});
        assert_eq!(report.symlinks_skipped.len(), 2);
        assert!(report.files.iter().all(|f| f.file_name != "real.package"));
        assert!(report.files.iter().all(|f| f.file_name != "sneaky.package"));
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_entries_are_nonfatal_errors() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        build_fixture(tmp.path());
        let locked = tmp.path().join("Locked");
        fs::create_dir(&locked).unwrap();
        fs::write(locked.join("hidden.package"), b"x").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        if fs::read_dir(&locked).is_ok() {
            // Running privileged (e.g. root inside a build container):
            // permission failures cannot be simulated here. CI runs this test
            // unprivileged on both Linux and Windows runners.
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let root = SafeRoot::new(tmp.path()).unwrap();
        let cancel = AtomicBool::new(false);
        let report = scan(&root, &ScanOptions::default(), &cancel, |_| {});

        // Restore permissions so the tempdir can be cleaned up.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(!report.errors.is_empty(), "permission failure must be recorded");
        assert!(
            report.files.iter().any(|f| f.file_name == "top.package"),
            "walk must continue past the failure"
        );
        assert!(!report.cancelled);
    }

    #[test]
    fn hash_pass_fills_hashes_and_reports_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        build_fixture(tmp.path());
        let root = SafeRoot::new(tmp.path()).unwrap();
        let cancel = AtomicBool::new(false);
        let mut report = scan(&root, &ScanOptions::default(), &cancel, |_| {});

        // Simulate an external deletion between scan and hash passes.
        let victim = report
            .files
            .iter()
            .find(|f| f.file_name == "mystery.xyz")
            .unwrap()
            .absolute_path
            .clone();
        fs::remove_file(&victim).unwrap();

        let errors = hash_files(&mut report.files, &cancel, |_| {});
        assert_eq!(errors.len(), 1);
        assert!(errors[0].path.ends_with("mystery.xyz"));

        let hashed = report.files.iter().filter(|f| f.sha256.is_some()).count();
        assert_eq!(hashed, report.files.len() - 1);

        let empty = report
            .files
            .iter()
            .find(|f| f.file_name == "empty.package")
            .unwrap();
        assert_eq!(
            empty.sha256.as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }
}
