//! Path-safety primitives.
//!
//! Every filesystem-touching service in this crate resolves candidate paths
//! through a [`SafeRoot`] before reading or mutating anything. A `SafeRoot`
//! canonicalizes its root once at construction and can then answer, for any
//! candidate path (existing or merely planned), whether it truly lives inside
//! the root after symlink resolution — with case-insensitive comparison to
//! match Windows filesystem semantics where appropriate.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathError {
    #[error("root does not exist or is not a directory: {0}")]
    RootInvalid(PathBuf),
    #[error("could not canonicalize path {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("path escapes the approved root {root}: {candidate}")]
    OutsideRoot { root: PathBuf, candidate: PathBuf },
    #[error("relative path may not contain parent-directory ('..'), root, or prefix components: {0}")]
    IllegalComponent(PathBuf),
    #[error("expected a relative path, got an absolute one: {0}")]
    NotRelative(PathBuf),
}

/// A canonicalized directory that all managed paths must live inside.
#[derive(Debug, Clone)]
pub struct SafeRoot {
    canonical: PathBuf,
    case_insensitive: bool,
}

impl SafeRoot {
    /// Construct with the platform's default filename-case semantics
    /// (case-insensitive on Windows, case-sensitive elsewhere).
    pub fn new(root: &Path) -> Result<Self, PathError> {
        Self::with_case_insensitive(root, cfg!(windows))
    }

    /// Construct with explicit case semantics. Used by tests to exercise the
    /// Windows comparison rules on any host platform.
    pub fn with_case_insensitive(root: &Path, case_insensitive: bool) -> Result<Self, PathError> {
        let canonical = dunce::canonicalize(root).map_err(|source| PathError::Canonicalize {
            path: root.to_path_buf(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(PathError::RootInvalid(canonical));
        }
        Ok(Self {
            canonical,
            case_insensitive,
        })
    }

    pub fn path(&self) -> &Path {
        &self.canonical
    }

    pub fn is_case_insensitive(&self) -> bool {
        self.case_insensitive
    }

    /// Resolve a *relative* path (e.g. from a manifest, a plan, or a database
    /// record) against the root. Rejects absolute paths and any `..`, root,
    /// or prefix component before touching the filesystem, then verifies
    /// containment of the joined result (which also catches symlink escapes
    /// along existing portions of the path).
    pub fn resolve_relative(&self, rel: &Path) -> Result<PathBuf, PathError> {
        if rel.is_absolute() {
            return Err(PathError::NotRelative(rel.to_path_buf()));
        }
        for c in rel.components() {
            match c {
                Component::Normal(_) | Component::CurDir => {}
                _ => return Err(PathError::IllegalComponent(rel.to_path_buf())),
            }
        }
        let joined = self.canonical.join(rel);
        self.contain(&joined)
    }

    /// Verify that `candidate` (existing or planned) resolves inside the
    /// root. Symlinks along the existing portion are resolved, so a link that
    /// points outside the root fails containment even though its textual path
    /// looks internal. Returns the effective (ancestor-canonicalized) path.
    pub fn contain(&self, candidate: &Path) -> Result<PathBuf, PathError> {
        let effective =
            canonicalize_deepest_existing(candidate).map_err(|source| PathError::Canonicalize {
                path: candidate.to_path_buf(),
                source,
            })?;
        if self.component_prefix_matches(&effective) {
            Ok(effective)
        } else {
            Err(PathError::OutsideRoot {
                root: self.canonical.clone(),
                candidate: candidate.to_path_buf(),
            })
        }
    }

    /// The candidate's path relative to the root, after containment checks.
    pub fn relative_of(&self, candidate: &Path) -> Result<PathBuf, PathError> {
        let effective = self.contain(candidate)?;
        let root_len = self.canonical.components().count();
        Ok(effective.components().skip(root_len).collect())
    }

    fn component_prefix_matches(&self, effective: &Path) -> bool {
        let mut root = self.canonical.components();
        let mut cand = effective.components();
        loop {
            match (root.next(), cand.next()) {
                (None, _) => return true,
                (Some(_), None) => return false,
                (Some(r), Some(c)) => {
                    if !component_eq(r, c, self.case_insensitive) {
                        return false;
                    }
                }
            }
        }
    }
}

fn component_eq(a: Component<'_>, b: Component<'_>, case_insensitive: bool) -> bool {
    if !case_insensitive {
        return a == b;
    }
    let a = a.as_os_str().to_string_lossy();
    let b = b.as_os_str().to_string_lossy();
    a.to_lowercase() == b.to_lowercase()
}

/// Canonicalize the deepest existing ancestor of `p`, then re-append the
/// not-yet-existing tail. This lets planned destinations be validated with
/// the same rigor as existing files. Any `..` in a non-existing tail is
/// rejected outright: it cannot be resolved against the filesystem, so it
/// cannot be trusted.
fn canonicalize_deepest_existing(p: &Path) -> std::io::Result<PathBuf> {
    if let Ok(c) = dunce::canonicalize(p) {
        return Ok(c);
    }
    let mut tail: Vec<OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        match cur.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                match cur.file_name() {
                    Some(name) => tail.push(name.to_os_string()),
                    None => {
                        // A path segment like `..` has no file_name; refuse to
                        // guess what it would resolve to.
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!(
                                "cannot verify a planned path containing unresolvable components: {}",
                                p.display()
                            ),
                        ));
                    }
                }
                match dunce::canonicalize(parent) {
                    Ok(mut base) => {
                        for seg in tail.iter().rev() {
                            base.push(seg);
                        }
                        return Ok(base);
                    }
                    Err(_) => cur = parent.to_path_buf(),
                }
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("no existing ancestor for {}", p.display()),
                ));
            }
        }
    }
}

/// Produce a destination path that does not collide with an existing file by
/// appending ` (2)`, ` (3)`, … before the extension.
pub fn collision_free(dest: &Path) -> PathBuf {
    if !dest.exists() {
        return dest.to_path_buf();
    }
    let stem = dest
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = dest.extension().map(|e| e.to_string_lossy().into_owned());
    let parent = dest.parent().map(Path::to_path_buf).unwrap_or_default();
    for n in 2u32.. {
        let name = match &ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 exhausted while searching for a collision-free name")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn root(dir: &Path) -> SafeRoot {
        SafeRoot::new(dir).expect("tempdir should canonicalize")
    }

    #[test]
    fn accepts_existing_file_inside_root() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("a.package");
        fs::write(&f, b"x").unwrap();
        let r = root(tmp.path());
        let contained = r.contain(&f).unwrap();
        assert!(contained.ends_with("a.package"));
    }

    #[test]
    fn accepts_planned_path_in_missing_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        let r = root(tmp.path());
        let planned = tmp.path().join("not-yet").join("deep").join("b.package");
        let contained = r.contain(&planned).unwrap();
        assert!(contained.ends_with("b.package"));
    }

    #[test]
    fn rejects_parent_traversal_in_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        let r = root(tmp.path());
        let err = r
            .resolve_relative(Path::new("../outside.package"))
            .unwrap_err();
        assert!(matches!(err, PathError::IllegalComponent(_)));
    }

    #[test]
    fn rejects_absolute_path_passed_as_relative() {
        let tmp = tempfile::tempdir().unwrap();
        let r = root(tmp.path());
        let abs = tmp.path().join("x.package");
        let err = r.resolve_relative(&abs).unwrap_err();
        assert!(matches!(err, PathError::NotRelative(_)));
    }

    #[test]
    fn rejects_path_outside_root() {
        let tmp = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let outside = other.path().join("evil.package");
        fs::write(&outside, b"x").unwrap();
        let r = root(tmp.path());
        let err = r.contain(&outside).unwrap_err();
        assert!(matches!(err, PathError::OutsideRoot { .. }));
    }

    #[test]
    fn rejects_planned_path_with_embedded_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let r = root(tmp.path());
        // `missing/` does not exist, so the `..` cannot be canonicalized away
        // by the filesystem — the checker must refuse rather than guess.
        let sneaky = tmp.path().join("missing").join("..").join("..").join("evil");
        assert!(r.contain(&sneaky).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_root() {
        let tmp = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let target = other.path().join("real.package");
        fs::write(&target, b"x").unwrap();
        let link = tmp.path().join("looks-internal.package");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let r = root(tmp.path());
        let err = r.contain(&link).unwrap_err();
        assert!(matches!(err, PathError::OutsideRoot { .. }));
    }

    #[test]
    fn case_insensitive_mode_matches_windows_semantics() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("Mods");
        fs::create_dir(&sub).unwrap();
        let f = sub.join("Hair.package");
        fs::write(&f, b"x").unwrap();
        let r = SafeRoot::with_case_insensitive(tmp.path(), true).unwrap();
        // Same textual path with different casing must still be contained.
        let recased = tmp.path().join("MODS").join("HAIR.PACKAGE");
        // On a case-sensitive filesystem this path does not exist, so it is
        // treated as planned — containment must still pass under CI compare.
        assert!(r.contain(&recased).is_ok());

        let strict = SafeRoot::with_case_insensitive(tmp.path(), false).unwrap();
        assert!(strict.contain(&f).is_ok());
    }

    #[test]
    fn relative_of_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("CAS").join("Hair");
        fs::create_dir_all(&sub).unwrap();
        let f = sub.join("curls.package");
        fs::write(&f, b"x").unwrap();
        let r = root(tmp.path());
        let rel = r.relative_of(&f).unwrap();
        assert_eq!(rel, PathBuf::from("CAS/Hair/curls.package"));
        let back = r.resolve_relative(&rel).unwrap();
        assert_eq!(back, r.contain(&f).unwrap());
    }

    #[test]
    fn collision_free_appends_counter_before_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("dup.package");
        fs::write(&f, b"x").unwrap();
        let next = collision_free(&f);
        assert!(next.ends_with("dup (2).package"));
        fs::write(&next, b"y").unwrap();
        let third = collision_free(&f);
        assert!(third.ends_with("dup (3).package"));
        let untouched = tmp.path().join("fresh.package");
        assert_eq!(collision_free(&untouched), untouched);
    }
}
