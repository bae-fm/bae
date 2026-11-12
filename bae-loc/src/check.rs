//! Catalog validation, run by `loc-gen check`, on two axes.
//!
//! `validate` guards each message against the mistakes that would otherwise
//! surface as a wrong or missing string at runtime: an undeclared placeholder, a
//! declared argument the message never uses, a plural over a non-integer, a
//! malformed MF1 value, an id outside the `core.`/`ui.` namespaces, a source
//! or translation that ends a sentence twice ("。。") or writes an ellipsis as
//! three full stops ("..." for "…") or an apostrophe as a straight quote
//! mark, or an English source that breaks a
//! sentence with a dash.
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
        let texts = std::iter::once(("en", &msg.value))
            .chain(msg.translations.iter().map(|(locale, text)| (locale.as_str(), text)));
        for (locale, text) in texts {
            if let Some(stop) = doubled_sentence_stop(text) {
                errs.push(format!("{id} [{locale}]: `{stop}` ends one sentence twice"));
            }
            // One form for the ellipsis in every locale: the character.
            if text.contains("...") {
                errs.push(format!(
                    "{id} [{locale}]: `...` is an ellipsis written as three full stops; write `…`"
                ));
            }
            // One form for the apostrophe in every locale: the curly one.
            if text.contains('\'') {
                errs.push(format!(
                    "{id} [{locale}]: `'` is an apostrophe written as a straight quote; write `’`"
                ));
            }
        }
        // A pause is a comma and a new thought a new sentence, never a dash.
        // Only the English is checked: Ukrainian writes its omitted "is" as a
        // dash.
        if msg.value.contains('—') || msg.value.contains(" – ") {
            errs.push(format!("{id}: the English source breaks a sentence with a dash"));
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// The marks that end a sentence, in the scripts bae ships.
const SENTENCE_STOPS: &[char] = &['.', '。', '!', '?', '！', '？'];

/// A sentence ended twice: a stop repeated ("。。", "!!") or a full stop beside
/// another stop ("。.", "?."), most often a translator's own full stop kept
/// beside the source's. Three ASCII dots are an ellipsis, which `validate`
/// reports on its own, and "?!" is one deliberate mark, so neither counts.
fn doubled_sentence_stop(text: &str) -> Option<String> {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if !SENTENCE_STOPS.contains(&c) {
            continue;
        }
        let mut run = String::from(c);
        while let Some(&next) = chars.peek() {
            if !SENTENCE_STOPS.contains(&next) {
                break;
            }
            run.push(next);
            chars.next();
        }
        let repeats = run.chars().zip(run.chars().skip(1)).any(|(a, b)| a == b);
        let doubled =
            run != "..." && run.chars().count() > 1 && (repeats || run.contains(['.', '。']));
        if doubled {
            return Some(run);
        }
    }
    None
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
