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
//! This crate is the generator's library guts; the `loc-gen` binary in
//! `bae-bridge` drives it and adds the completeness check against the Rust
//! message enums.

pub mod check;
pub mod emit;
pub mod mf1;

use std::collections::BTreeMap;

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

/// A message id namespace. `core.*` is bridge-originated and completeness
/// checked against the Rust message enums; `ui.*` is shared chrome opted in by
/// the UI and is not checked (it has no Rust producer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    Core,
    Ui,
}

impl Catalog {
    /// Parse a catalog from TOML source. Fails with the underlying TOML error
    /// (surfaced, not masked) so a malformed catalog breaks the build loudly.
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }
}

/// Classify a dotted id by its first segment. Unknown prefixes are an error so
/// a typo'd namespace can't silently skip the completeness check.
pub fn namespace_of(id: &str) -> Result<Namespace, String> {
    match id.split('.').next() {
        Some("core") => Ok(Namespace::Core),
        Some("ui") => Ok(Namespace::Ui),
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
    fn classifies_namespaces() {
        assert_eq!(namespace_of("core.error.x").unwrap(), Namespace::Core);
        assert_eq!(namespace_of("ui.button.ok").unwrap(), Namespace::Ui);
        assert!(namespace_of("misc.oops").is_err());
    }
}
