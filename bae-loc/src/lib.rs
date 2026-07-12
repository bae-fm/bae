//! Localization catalog model and target conversion for bae.
//!
//! The master catalog (`bae-bridge/loc/catalog.toml`) is the single source of
//! truth for strings that originate in shared Rust logic (`core.*`) plus shared
//! chrome opted in by the UI (`ui.*`). Each message `value` is an ICU
//! MessageFormat 1 string — the cross-platform standard. Android and Windows
//! consume that string verbatim at runtime (`android.icu.text.MessageFormat` /
//! the `MessageFormat` NuGet); only Apple needs a conversion to its String
//! Catalog (`.xcstrings`) shape, which this crate performs.
//!
//! This crate is the generator's library guts; its own `loc-gen` binary
//! (`src/bin/loc-gen.rs`) drives it — `check` validates the catalog's internal
//! consistency, `emit` writes the native resource files. The completeness
//! cross-check against the Rust message enums lives in bae-bridge, as the
//! `loc_key_coverage` test module in `bae-bridge/src/types.rs`.

#![deny(unreachable_pub, dead_code)]

pub mod check;
pub mod emit;
pub mod mf1;

use std::collections::{BTreeMap, BTreeSet};

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
    /// one/few/many/other, Arabic's six), not English's one/other. A locale with
    /// no entry falls back to the English `value`, emitted at state `new`.
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

    /// Every non-source locale named by any message translation. These locales
    /// define the generated resource set; messages without a translation for a
    /// target locale fall back to the source value at emit time.
    pub fn target_locales<'a>(&'a self, src_lang: &str) -> Vec<&'a str> {
        let mut locales = BTreeSet::new();
        for msg in self.messages.values() {
            for locale in msg.translations.keys() {
                if locale != src_lang {
                    locales.insert(locale.as_str());
                }
            }
        }
        locales.into_iter().collect()
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
        _ => Err(format!(
            "message id {id:?} must start with `core.` or `ui.`"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_args_plural_and_plain() {
        let src = r#"
[messages."core.outbox.pending_deletes"]
comment = "Storage queue: number of cloud deletes still pending."
args = { count = "Int" }
value = "{count, plural, one {# pending delete} other {# pending deletes}}"

[messages."core.identify.barcode.looking_up"]
args = { position = "Int", total = "Int" }
value = "Looking up barcode {position} of {total}"

[messages."ui.library.remove_from_device"]
value = "remove this library from this device"
"#;
        let cat = Catalog::from_toml(src).expect("catalog parses");
        assert_eq!(cat.messages.len(), 3);

        let deletes = &cat.messages["core.outbox.pending_deletes"];
        assert_eq!(deletes.args["count"], ArgType::Int);
        assert!(deletes.value.contains("plural"));

        let barcode = &cat.messages["core.identify.barcode.looking_up"];
        assert_eq!(barcode.args.len(), 2);
        assert_eq!(barcode.args["total"], ArgType::Int);

        let chrome = &cat.messages["ui.library.remove_from_device"];
        assert!(chrome.args.is_empty());
        assert_eq!(chrome.comment, None);
    }

    #[test]
    fn validates_namespaces() {
        assert!(validate_namespace("core.error.x").is_ok());
        assert!(validate_namespace("ui.button.ok").is_ok());
        assert!(validate_namespace("misc.oops").is_err());
    }

    #[test]
    fn target_locales_are_derived_from_translation_keys() {
        let src = r#"
[messages."core.one"]
value = "one"
translations = { es = "uno", "zh-Hans" = "一" }

[messages."core.two"]
value = "two"
translations = { es = "dos", en = "two" }
"#;
        let cat = Catalog::from_toml(src).expect("catalog parses");
        assert_eq!(cat.target_locales("en"), vec!["es", "zh-Hans"]);
    }
}
