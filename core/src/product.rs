//! Centralized product identity.
//!
//! Renaming the product should only require editing this file plus its two
//! mirrors that build tooling forces to be literals:
//!   * `src/lib/product.ts` (frontend constant)
//!   * `productName` in `src-tauri/tauri.conf.json`
//! Nothing else in the codebase may hardcode the name.

pub const PRODUCT_NAME: &str = "Motherlode Manager";
pub const PRODUCT_TAGLINE: &str = "Your mods. Organized. Precious.";

/// Reserved (currently unused): stable name for application-owned data.
/// The on-disk identity is `com.moetech.plumbob` — kept deliberately stable
/// across the v0.2.0 rename so existing libraries survive untouched.
pub const DATA_DIR_NAME: &str = "PlumbobLibraryData";

/// Shown in the About screen. Required wording from the product spec.
pub const AFFILIATION_DISCLAIMER: &str = "Motherlode Manager is an independent community tool and is not affiliated with or endorsed by Electronic Arts, Maxis, The Sims, Overwolf, CurseForge, or individual mod creators.";
