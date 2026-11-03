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
                errs.push(format!("{id}: `{{{arg}}}` is used but not declared in `args`"));
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
#[path = "check_tests.rs"]
mod tests;
