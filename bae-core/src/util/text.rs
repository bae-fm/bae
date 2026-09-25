//! Text normalization shared by metadata matching and stored release marks.

use unicode_normalization::UnicodeNormalization;

/// A value as it is looked up: lowercase, diacritics folded away, and
/// everything that is not a letter or a digit dropped — which is what makes
/// `WPCR-80001`, `WPCR 80001` and `wpcr80001` one value.
///
/// The one normalization of a catalog number: the text is searched by it, a
/// struck-out number is matched by it, and the sightings a chosen number folds
/// into one mark line are gathered by it.
pub(crate) fn squash(text: &str) -> String {
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Text as two spellings of one name compare: NFD decomposed, combining marks
/// dropped (so diacritics go), lowercased, whitespace runs collapsed to one
/// space, and leading and trailing non-alphanumerics stripped. Never displayed.
///
/// The one normalization of a name: candidate text clusters by it, and a
/// library artist's `name_key` is it, so a credit meets the library artist it
/// names however either is cased or accented.
pub(crate) fn normalize(text: &str) -> String {
    let decomposed: String = text
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect();
    let s = decomposed.to_lowercase();
    let mut collapsed = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                collapsed.push(' ');
                prev_space = true;
            }
        } else {
            collapsed.push(c);
            prev_space = false;
        }
    }
    collapsed
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
