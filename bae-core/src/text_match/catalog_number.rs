//! When two catalog numbers are one number.

use super::squash;

/// A catalog number as two of them are compared: squashed, and `None` when
/// that leaves no number. Both catalogs write a release's lack of one as a
/// placeholder — `[none]` on MusicBrainz, `none` on Discogs, and in their
/// data also `None`, `(none)`, `- none` and `-none-` — which all squash to
/// `none`.
pub(crate) fn catalog_key(stated: &str) -> Option<String> {
    let key = squash(stated);
    (!key.is_empty() && key != "none").then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_and_case_make_no_other_number() {
        assert_eq!(catalog_key("WPCR-80001"), catalog_key("wpcr 80001"));
        assert_eq!(catalog_key("WPCR-80001").as_deref(), Some("wpcr80001"));
    }

    #[test]
    fn a_placeholder_is_no_number() {
        for placeholder in [
            "[none]", "none", "None", "(none)", "- none", "-none-", "", "--",
        ] {
            assert_eq!(catalog_key(placeholder), None, "{placeholder}");
        }
    }
}
