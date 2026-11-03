//! Localization catalog model and target conversion for bae.
//!
//! The master catalog (`bae-bridge/loc/catalog.toml`) is the single source of
//! truth for strings that originate in shared Rust logic (`core.*`) plus shared
//! chrome opted in by the UI (`ui.*`). Each message `value` is an ICU
//! MessageFormat 1 string — the cross-platform standard. Android consumes that
//! string verbatim at runtime (`android.icu.text.MessageFormat`); only Apple
//! needs a conversion to its String Catalog (`.xcstrings`) shape, which this
//! crate performs.
//!
//! `TARGET_LOCALES` is the locale set bae ships. Everything downstream reads it
//! from here: `emit` fans out one resource set per entry, `check` holds the
//! master catalog to it, and `loc-gen locales` hands it to the Python catalog
//! gates.
//!
//! This crate is the generator's library guts; its own `loc-gen` binary
//! (`src/bin/loc-gen.rs`) drives it — `check` validates the catalog's internal
//! consistency, `emit` writes the native resource files. The completeness
//! cross-check against the Rust message enums lives in bae-bridge, as the
//! `loc_key_coverage` test module in `bae-bridge/src/types.rs`.

pub mod check;
pub mod emit;
pub mod mf1;

use std::collections::BTreeMap;

/// The language every catalog `value` is authored in. Not a target: its slot in
/// a generated resource file comes from `value` itself, never a translation.
pub const SOURCE_LOCALE: &str = "en";

/// The locales bae ships besides the source. This is the one list: `loc-gen`
/// emits a resource set per entry, `loc-gen check` fails a catalog whose
/// `translations` tables don't match it exactly, and `loc-gen locales` prints it
/// for the Python catalog gates. Adding a locale here is the first half of
/// shipping it; the second is a real translation of every message, in every
/// catalog, in that locale.
pub const TARGET_LOCALES: &[&str] = &[
    "de", "fr", "es", "it", "nl", "pl", "cs", "sk", "hu", "ro", "el", "bg", "hr", "uk", "tr", "he",
    "ar", "pt-BR", "pt-PT", "sv", "da", "nb", "fi", "ja", "ko", "zh-Hans", "zh-Hant",
];

/// The parsed master catalog. Keyed by dotted message id (e.g.
/// `core.identify.barcode.looking_up`). `BTreeMap` so every emit is
/// deterministically ordered — generated files diff cleanly.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Catalog {
    pub messages: BTreeMap<String, Message>,
}

/// One catalog entry. `value` is an ICU MessageFormat 1 string.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Message {
    /// Translator-facing note. Optional.
    #[serde(default)]
    pub comment: Option<String>,
    /// Argument name -> type. Declared explicitly (not inferred from the MF
    /// string) so the generator can type-check the catalog against the Rust
    /// message enums. Empty for messages with no arguments.
    #[serde(default)]
    pub args: BTreeMap<String, ArgType>,
    /// The ICU MessageFormat 1 source (English).
    pub value: String,
    /// Per-locale translations of `value`, keyed by catalog locale code (`es`,
    /// `pt-BR`, `zh-Hans`, …). Each is a full ICU MessageFormat 1 string in that
    /// locale — a plural carries the locale's own CLDR categories (Polish
    /// one/few/many/other, Arabic's six), not English's one/other. The table
    /// must name exactly `TARGET_LOCALES`; `check::locale_coverage` is what says
    /// so, because a locale with no entry would otherwise emit the English
    /// `value` at state `new` and ship English under a translated locale.
    #[serde(default)]
    pub translations: BTreeMap<String, String>,
}

/// The argument types the catalog distinguishes. `Int` covers the Rust integer
/// widths that cross the bridge (`i32`/`i64`/`u32`/`u64`); `Str` is `String`.
/// Kept minimal on purpose — extended when a real message needs another type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub enum ArgType {
    Int,
    Str,
}

impl Catalog {
    /// Parse a catalog from TOML source. Fails with the underlying TOML error
    /// (surfaced, not masked) so a malformed catalog breaks the build loudly.
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }
}

/// Check a dotted id's first segment against the two allowed namespaces:
/// `core.` (bridge-originated, completeness-checked against the Rust message
/// enums) and `ui.` (shared chrome opted in by the UI, no Rust producer).
/// Unknown prefixes are an error so a typo'd namespace can't silently skip
/// the completeness check.
pub fn validate_namespace(id: &str) -> Result<(), String> {
    match id.split('.').next() {
        Some("core") | Some("ui") => Ok(()),
        _ => Err(format!("message id {id:?} must start with `core.` or `ui.`")),
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
