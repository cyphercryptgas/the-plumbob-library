//! The Title Tool's naming engine: compose `[creator]_[modtype]_[modname]`
//! from what the library already knows about a file. Pure functions,
//! fully tested — the service layer only moves files and rows.

/// Filename-safe field: letters, digits, hyphens. Runs of anything else
/// collapse to one hyphen; each hyphen-token is capitalized.
pub fn sanitize_field(s: &str) -> String {
    let mut out = String::new();
    let mut pending_sep = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.push(c);
        } else {
            pending_sep = true;
        }
    }
    out.split('-')
        .filter(|t| !t.is_empty())
        .map(|t| {
            let mut cs = t.chars();
            match cs.next() {
                Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// The modtype token: CAS subcategory when we know it, else the category.
pub fn type_label(category: Option<&str>, cas_subcategory: Option<&str>) -> String {
    if category == Some("cas") {
        if let Some(sub) = cas_subcategory {
            if !sub.is_empty() {
                return sanitize_field(sub);
            }
        }
    }
    match category {
        Some("cas") => "CAS".to_string(),
        Some("buildbuy") => "BuildBuy".to_string(),
        Some("animations") => "Poses".to_string(),
        Some("gameplay") => "Gameplay".to_string(),
        _ => "Other".to_string(),
    }
}

/// The modname: the CurseForge mod name when matched, else the current
/// filename scrubbed of its extension, bracketed lead, creator tokens,
/// and version-ish digits.
pub fn clean_modname(
    current_filename: &str,
    creator_tokens: &[&str],
    curse_name: Option<&str>,
) -> String {
    if let Some(n) = curse_name {
        let s = sanitize_field(n);
        if !s.is_empty() {
            return truncate_field(&s);
        }
    }
    let mut base = current_filename.to_string();
    for ext in [".package", ".ts4script"] {
        if base.to_lowercase().ends_with(ext) {
            base.truncate(base.len() - ext.len());
        }
    }
    if base.starts_with('[') {
        if let Some(close) = base.find(']') {
            base = base[close + 1..].to_string();
        }
    }
    let lower = base.to_lowercase();
    let mut cut = 0usize;
    for tok in creator_tokens {
        let t = tok.to_lowercase();
        if !t.is_empty() && lower[cut..].trim_start_matches(['_', ' ', '-']).starts_with(&t) {
            let lead = lower[cut..].len() - lower[cut..].trim_start_matches(['_', ' ', '-']).len();
            cut += lead + t.len();
        }
    }
    base = base[cut..].to_string();
    // strip version-ish tokens: v1, 0.3.1.3, 2026 etc. at word level
    let cleaned: String = base
        .split(|c: char| c == '_' || c == ' ' || c == '-' || c == '.')
        .filter(|w| {
            !w.is_empty()
                && !(w.chars().all(|c| c.is_ascii_digit())
                    || (w.len() > 1
                        && (w.starts_with('v') || w.starts_with('V'))
                        && w[1..].chars().all(|c| c.is_ascii_digit() || c == '.')))
        })
        .collect::<Vec<_>>()
        .join("-");
    let s = sanitize_field(&cleaned);
    truncate_field(if s.is_empty() { "Item" } else { &s })
}

fn truncate_field(s: &str) -> String {
    if s.len() <= 48 {
        return s.to_string();
    }
    let mut out = s.to_string();
    while out.len() > 48 {
        match out.rfind('-') {
            Some(i) if i > 8 => out.truncate(i),
            _ => {
                out.truncate(48);
                break;
            }
        }
    }
    out
}

/// The full composed filename (with extension).
pub fn compose(
    creator_display: &str,
    category: Option<&str>,
    cas_subcategory: Option<&str>,
    current_filename: &str,
    creator_key: &str,
    curse_name: Option<&str>,
    extension: &str,
) -> String {
    format!(
        "{}_{}_{}.{}",
        sanitize_field(creator_display),
        type_label(category, cas_subcategory),
        clean_modname(current_filename, &[creator_display, creator_key], curse_name),
        extension
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_and_capitalizes() {
        assert_eq!(sanitize_field("alana mini skirt"), "Alana-Mini-Skirt");
        assert_eq!(sanitize_field("[arethabee]"), "Arethabee");
        assert_eq!(sanitize_field("__x__"), "X");
    }

    #[test]
    fn type_prefers_subcategory() {
        assert_eq!(type_label(Some("cas"), Some("hair")), "Hair");
        assert_eq!(type_label(Some("cas"), None), "CAS");
        assert_eq!(type_label(Some("buildbuy"), None), "BuildBuy");
        assert_eq!(type_label(None, None), "Other");
    }

    #[test]
    fn modname_strips_creator_brackets_and_versions() {
        assert_eq!(
            clean_modname("[arethabee] alana mini skirt.package", &["arethabee"], None),
            "Alana-Mini-Skirt"
        );
        assert_eq!(
            clean_modname("SIMREALIST_Flowfit_0.3.1.3.package", &["SIMREALIST"], None),
            "Flowfit"
        );
        assert_eq!(
            clean_modname("amellce_AskToReadBook.package", &["amellce"], None),
            "AskToReadBook"
        );
    }

    #[test]
    fn curse_name_wins_when_present() {
        assert_eq!(
            clean_modname("whatever_v2.package", &["x"], Some("Ask To Read Book")),
            "Ask-To-Read-Book"
        );
    }

    #[test]
    fn composes_the_convention() {
        assert_eq!(
            compose(
                "arethabee",
                Some("cas"),
                Some("skirt"),
                "[arethabee] alana mini skirt.package",
                "arethabee",
                None,
                "package"
            ),
            "Arethabee_Skirt_Alana-Mini-Skirt.package"
        );
    }
}
