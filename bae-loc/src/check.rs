//! Catalog validation, run by `loc-gen check`, on two axes.
//!
//! `validate` guards each message against the mistakes that would otherwise
//! surface as a wrong or missing string at runtime: an undeclared placeholder, a
//! declared argument the message never uses, a plural over a non-integer, a
//! malformed MF1 value, or an id outside the `core.`/`ui.` namespaces.
//!
//! `locale_coverage` holds the catalog to `TARGET_LOCALES`. A message missing a
//! target locale emits the English source under that locale (see
//! `emit::localized`), so the app ships English while claiming to be
//! translated; a message carrying a locale outside the set emits nothing and the
//! translation is dead weight. Both are silent at runtime, which is why they are
//! a build failure here.
//!
//! The cross-check between catalog ids and the Rust message enums is the
//! `loc_key_coverage` test module in `bae-bridge/src/types.rs`; this module is
//! the catalog-only half.

use std::collections::BTreeMap;

use crate::mf1::{self, Node};
use crate::{validate_namespace, ArgType, Catalog, TARGET_LOCALES};

/// Validate every message. Returns all problems found (not just the first) so a
/// catalog edit gets the full list in one pass.
pub fn validate(cat: &Catalog) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    for (id, msg) in &cat.messages {
        if let Err(e) = validate_namespace(id) {
            errs.push(e);
        }
        let nodes = match mf1::parse(&msg.value) {
            Ok(n) => n,
            Err(e) => {
                errs.push(format!("{id}: {e}"));
                continue;
            }
        };
        let referenced = mf1::referenced_args(&nodes);
        for arg in &referenced {
            if !msg.args.contains_key(arg) {
                errs.push(format!(
                    "{id}: `{{{arg}}}` is used but not declared in `args`"
                ));
            }
        }
        for arg in msg.args.keys() {
            if !referenced.contains(arg) {
                errs.push(format!("{id}: declared argument `{arg}` is never used"));
            }
        }
        for node in &nodes {
            if let Node::Plural { arg, .. } = node {
                if msg.args.get(arg) != Some(&ArgType::Int) {
                    errs.push(format!("{id}: plural argument `{arg}` must be `Int`"));
                }
            }
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Hold every message's `translations` table to `TARGET_LOCALES` exactly.
/// Reported per locale rather than per message: a locale added to (or dropped
/// from) the set is wrong in every message at once, and one line per locale is
/// what the reader needs to act on.
pub fn locale_coverage(cat: &Catalog) -> Result<(), Vec<String>> {
    let mut unshipped: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut untranslated: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, msg) in &cat.messages {
        for locale in msg.translations.keys() {
            if !TARGET_LOCALES.contains(&locale.as_str()) {
                unshipped.entry(locale).or_default().push(id);
            }
        }
        for locale in TARGET_LOCALES {
            if !msg.translations.contains_key(*locale) {
                untranslated.entry(locale).or_default().push(id);
            }
        }
    }

    let mut errs = Vec::new();
    for (locale, ids) in &unshipped {
        errs.push(format!(
            "`{locale}` is not a shipping locale, but {} message(s) translate it (e.g. `{}`)",
            ids.len(),
            ids[0]
        ));
    }
    for (locale, ids) in &untranslated {
        errs.push(format!(
            "`{locale}` ships, but {} message(s) have no translation for it (e.g. `{}`)",
            ids.len(),
            ids[0]
        ));
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(toml: &str) -> Catalog {
        Catalog::from_toml(toml).expect("parses")
    }

    #[test]
    fn accepts_a_well_formed_catalog() {
        let c = cat(r#"
[messages."core.outbox.pending_deletes"]
args = { count = "Int" }
value = "{count, plural, one {# pending delete} other {# pending deletes}}"

[messages."core.identify.barcode.looking_up"]
args = { position = "Int", total = "Int" }
value = "Looking up barcode {position} of {total}"

[messages."ui.library.remove_from_device"]
value = "remove this library from this device"
"#);
        assert!(validate(&c).is_ok());
    }

    #[test]
    fn flags_undeclared_placeholder() {
        let c = cat(r#"
[messages."core.x"]
value = "hi {name}"
"#);
        let errs = validate(&c).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("not declared")), "{errs:?}");
    }

    #[test]
    fn flags_unused_declared_arg() {
        let c = cat(r#"
[messages."core.x"]
args = { name = "Str" }
value = "no placeholder here"
"#);
        let errs = validate(&c).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("never used")), "{errs:?}");
    }

    #[test]
    fn flags_non_int_plural_arg() {
        let c = cat(r#"
[messages."core.x"]
args = { count = "Str" }
value = "{count, plural, one {#} other {#}}"
"#);
        let errs = validate(&c).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("must be `Int`")), "{errs:?}");
    }

    #[test]
    fn flags_bad_namespace() {
        let c = cat(r#"
[messages."misc.x"]
value = "oops"
"#);
        assert!(validate(&c).is_err());
    }

    fn one_message(translations: &str) -> Catalog {
        cat(&format!(
            "[messages.\"core.x\"]\nvalue = \"hi\"\ntranslations = {{ {translations} }}\n"
        ))
    }

    #[test]
    fn flags_a_locale_outside_the_shipping_set() {
        let mut translations: Vec<String> = TARGET_LOCALES
            .iter()
            .map(|l| format!("\"{l}\" = \"hi\""))
            .collect();
        translations.push("\"qq\" = \"hi\"".to_string());
        let errs = locale_coverage(&one_message(&translations.join(", "))).unwrap_err();
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(
            errs[0].contains("`qq` is not a shipping locale"),
            "{errs:?}"
        );
    }

    #[test]
    fn flags_a_shipping_locale_with_no_translation() {
        let errs = locale_coverage(&one_message("de = \"hallo\"")).unwrap_err();
        assert_eq!(errs.len(), TARGET_LOCALES.len() - 1, "{errs:?}");
        assert!(
            errs.iter().any(|e| e.contains("`fr` ships, but 1 message")),
            "{errs:?}"
        );
    }

    #[test]
    fn accepts_exactly_the_shipping_set() {
        let translations: Vec<String> = TARGET_LOCALES
            .iter()
            .map(|l| format!("\"{l}\" = \"hi\""))
            .collect();
        assert!(locale_coverage(&one_message(&translations.join(", "))).is_ok());
    }

    fn real_catalog() -> Catalog {
        let src = include_str!("../../bae-bridge/loc/catalog.toml");
        Catalog::from_toml(src).expect("real catalog parses")
    }

    /// The shipping catalog must always be valid. This gates `cargo test`, so an
    /// invalid edit to `bae-bridge/loc/catalog.toml` fails the build.
    #[test]
    fn real_catalog_is_valid() {
        if let Err(errs) = validate(&real_catalog()) {
            panic!("real catalog invalid:\n  - {}", errs.join("\n  - "));
        }
    }

    /// The shipping catalog translates every locale bae ships and nothing else.
    /// A gap here means the app declares a locale it would serve English under.
    #[test]
    fn real_catalog_translates_exactly_the_shipping_locales() {
        if let Err(errs) = locale_coverage(&real_catalog()) {
            panic!("real catalog locale coverage:\n  - {}", errs.join("\n  - "));
        }
    }
}
