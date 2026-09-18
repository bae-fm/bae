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
