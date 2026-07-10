//! Centralized product identity.
//!
//! "The Plumbob Library" is a working placeholder. Renaming the product
//! should only require editing this file plus its two mirrors that build
//! tooling forces to be literals:
//!   * `src/lib/product.ts` (frontend constant)
//!   * `productName` in `src-tauri/tauri.conf.json`
//! Nothing else in the codebase may hardcode the name.

pub const PRODUCT_NAME: &str = "The Plumbob Library";
pub const PRODUCT_TAGLINE: &str = "A safer home for your Sims 4 mods and custom content.";

/// Folder name used for application-owned data (quarantine, backups, cache).
pub const DATA_DIR_NAME: &str = "PlumbobLibraryData";

/// Shown in the About screen. Required wording from the product spec.
pub const AFFILIATION_DISCLAIMER: &str = "The Plumbob Library is an independent community tool and is not affiliated with or endorsed by Electronic Arts, Maxis, The Sims, Overwolf, CurseForge, or individual mod creators.";
