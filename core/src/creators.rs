//! Creator identity, read from the two dominant CC naming conventions:
//!
//! * **Bracketed leads** — `[SIMCREDIBLE] LivingSuite Sofa` — always
//!   credit; the brackets are an explicit byline.
//! * **Underscore prefixes** — `KUTTOE_NewEmotionalTraits` — credit when
//!   the token carries a creator signature (any uppercase) or earns
//!   *frequency promotion* (three or more files share it), which is how
//!   all-lowercase creators get in while one-off generic prefixes stay
//!   out. A stoplist blocks the common content words (`poses_`,
//!   `hair_`, …).
//!
//! Prefixes sometimes credit a mod line rather than a person (a
//! `UICheats_` prefix groups under "UICheats") — that grouping is still
//! the useful one. Precision-first and evidence-adjustable, like the
//! category tables.

use crate::scan::DISABLED_SUFFIX;

const STOP_TOKENS: [&str; 30] = [
    "mod", "mods", "sim", "sims", "sims4", "ts4", "cc", "cas", "bb",
    "buildbuy", "pose", "poses", "hair", "skin", "top", "tops", "dress",
    "set", "sets", "recolor", "recolour", "override", "patch", "female",
    "male", "child", "toddler", "infant", "adult", "the",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// Canonical grouping key (lowercase).
    pub key: String,
    /// As written on the file — the display form.
    pub display: String,
    /// Strong candidates credit on their own; weak ones need promotion.
    pub strong: bool,
}

fn strip_extensions(name: &str) -> &str {
    let stem = name.strip_suffix(DISABLED_SUFFIX).unwrap_or(name);
    stem.strip_suffix(".package")
        .or_else(|| stem.strip_suffix(".ts4script"))
        .unwrap_or(stem)
}

fn plausible(token: &str) -> bool {
    let alpha = token.chars().filter(|c| c.is_alphabetic()).count();
    (3..=24).contains(&token.len())
        && alpha >= 3
        && !STOP_TOKENS.contains(&token.to_lowercase().as_str())
}

/// The creator candidate a filename suggests, if any.
pub fn candidate(file_name: &str) -> Option<Candidate> {
    let stem = strip_extensions(file_name).trim_start();

    // Bracketed byline: [Creator] or (Creator) at the very front.
    if let Some(open) = stem.chars().next().filter(|c| *c == '[' || *c == '(') {
        let close = if open == '[' { ']' } else { ')' };
        if let Some(end) = stem.find(close) {
            let inner = stem[1..end].trim();
            let alpha = inner.chars().filter(|c| c.is_alphabetic()).count();
            if (2..=40).contains(&inner.len()) && alpha >= 2 {
                return Some(Candidate {
                    key: inner.to_lowercase(),
                    display: inner.to_string(),
                    strong: true,
                });
            }
        }
        return None;
    }

    // Underscore prefix: the token before the first underscore.
    let token = stem.split('_').next().filter(|t| stem.contains('_'))?;
    if !plausible(token) {
        return None;
    }
    Some(Candidate {
        key: token.to_lowercase(),
        display: token.to_string(),
        strong: token.chars().any(|c| c.is_uppercase()),
    })
}

/// Resolve candidates across a whole library: strong candidates credit
/// immediately; weak ones need three files sharing the key. Returns, per
/// file id, `Some((key, display))` or `None` for uncredited. Display
/// forms are canonicalized per key, preferring a mixed-case spelling.
pub fn resolve(
    candidates: &[(i64, Option<Candidate>)],
) -> Vec<(i64, Option<(String, String)>)> {
    let mut counts: std::collections::HashMap<&str, usize> = Default::default();
    let mut display: std::collections::HashMap<&str, &str> = Default::default();
    for (_, c) in candidates {
        if let Some(c) = c {
            *counts.entry(c.key.as_str()).or_insert(0) += 1;
            let better = |d: &str| {
                d.chars().any(|ch| ch.is_lowercase()) && d.chars().any(|ch| ch.is_uppercase())
            };
            display
                .entry(c.key.as_str())
                .and_modify(|cur| {
                    if better(&c.display) && !better(cur) {
                        *cur = c.display.as_str();
                    }
                })
                .or_insert(c.display.as_str());
        }
    }
    candidates
        .iter()
        .map(|(id, c)| {
            let credited = c.as_ref().and_then(|c| {
                if c.strong || counts.get(c.key.as_str()).copied().unwrap_or(0) >= 3 {
                    Some((
                        c.key.clone(),
                        display.get(c.key.as_str()).unwrap_or(&c.display.as_str()).to_string(),
                    ))
                } else {
                    None
                }
            });
            (*id, credited)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_library_names_credit_correctly() {
        let cases = [
            ("[SIMCREDIBLE] LivingSuite Sofa v2.package", Some(("simcredible", true))),
            ("[NORTHERN SIBERIA WINDS] CHEEKS N12.package", Some(("northern siberia winds", true))),
            ("[MagicHand] Julian Eyebrows N165.package", Some(("magichand", true))),
            ("[Liliili] Amos Watch_L bracelet.package", Some(("liliili", true))),
            ("KUTTOE_NewEmotionalTraits.package", Some(("kuttoe", true))),
            ("VIBRANTPIXELS_bodypresets_PearBo.package", Some(("vibrantpixels", true))),
            ("SimMattically_MainMenu3.package.off", Some(("simmattically", true))),
            ("LAMATISSE_skinblend_Rosewater.package", Some(("lamatisse", true))),
            ("UICheats_v1.42.package", Some(("uicheats", true))),
            ("simancholy_dress_ruffle.package", Some(("simancholy", false))),
            ("mc_cmd_center.ts4script", None),
            ("poses_couple_v3.package", None),
            ("7cbcd7a91f3e.package", None),
            ("hair.package", None),
        ];
        for (name, expect) in cases {
            let got = candidate(name);
            match expect {
                None => assert!(got.is_none(), "{name} → {got:?}"),
                Some((key, strong)) => {
                    let c = got.expect(name);
                    assert_eq!(c.key, key, "{name}");
                    assert_eq!(c.strong, strong, "{name}");
                }
            }
        }
    }

    #[test]
    fn weak_prefixes_need_three_files_and_displays_canonicalize() {
        let mk = |id, name: &str| (id, candidate(name));
        let trio = vec![
            mk(1, "simancholy_dress_a.package"),
            mk(2, "simancholy_dress_b.package"),
            mk(3, "Simancholy_hair_c.package"),
            mk(4, "lonelyprefix_thing.package"),
            mk(5, "KUTTOE_solo.package"),
        ];
        let resolved = resolve(&trio);
        let get = |id: i64| resolved.iter().find(|(i, _)| *i == id).unwrap().1.clone();
        let (key, disp) = get(1).expect("promoted by frequency");
        assert_eq!(key, "simancholy");
        assert_eq!(disp, "Simancholy", "mixed-case spelling wins the display");
        assert_eq!(get(4), None, "singleton lowercase stays uncredited");
        assert_eq!(get(5).unwrap().0, "kuttoe", "strong credits alone");
    }
}
