//! When two catalog numbers are one number, and where a line of text can
//! print one.

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

desktop_only! {
    use super::fold::folded;

    /// The text cut where a catalog number printed in it can begin or end,
    /// each piece squashed. A hyphen, dot or slash with a letter or digit on
    /// both sides joins what it stands between into one number, so
    /// `7559-61571-2` is one piece and neither `61571-2` nor `7559` is
    /// printed there. Anything else that is not a letter or a digit — a
    /// space, a bracket, a separator with a space beside it — cuts the text,
    /// so a number printed with spaces, like `MCD 11600`, is a run of pieces
    /// that together print it.
    ///
    /// Run together the pieces are [`squash`] of the text, as
    /// [`super::words`] are: each piece is one or more of its words.
    pub(crate) fn catalog_words(text: &str) -> Vec<String> {
        let chars: Vec<char> = folded(text).collect();
        let mut pieces = Vec::new();
        let mut piece = String::new();
        for (at, &c) in chars.iter().enumerate() {
            let keeps_whole = joins(c) && joined(&chars, at);
            if c.is_alphanumeric() {
                piece.push(c);
            } else if !keeps_whole && !piece.is_empty() {
                pieces.push(std::mem::take(&mut piece));
            }
        }
        if !piece.is_empty() {
            pieces.push(piece);
        }
        pieces
    }

    /// A mark that keeps a number whole when letters or digits stand on both
    /// sides of it: `WPCR-80001`, `CDP7.46437`, `AB12/2`, and the Unicode
    /// hyphens a sleeve's typesetting may print for `-`.
    fn joins(c: char) -> bool {
        matches!(c, '-' | '.' | '/' | '\u{2010}' | '\u{2011}')
    }

    /// Whether the characters on both sides of `at` are letters or digits.
    fn joined(chars: &[char], at: usize) -> bool {
        at.checked_sub(1)
            .and_then(|before| chars.get(before))
            .is_some_and(|c| c.is_alphanumeric())
            && chars.get(at + 1).is_some_and(|c| c.is_alphanumeric())
    }
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

    #[test]
    fn a_joined_hyphen_dot_or_slash_keeps_a_number_whole() {
        assert_eq!(catalog_words("AB12-2"), vec!["ab122"]);
        assert_eq!(catalog_words("6789-12345-2"), vec!["6789123452"]);
        assert_eq!(catalog_words("CD1.234/5"), vec!["cd12345"]);
        assert_eq!(catalog_words("AB12\u{2010}2"), vec!["ab122"]);
    }

    #[test]
    fn spaces_brackets_and_loose_separators_cut_numbers() {
        assert_eq!(
            catalog_words("Artist - Album [AB12-2] (XYZ 100)"),
            vec!["artist", "album", "ab122", "xyz", "100"]
        );
        assert_eq!(catalog_words("AB12 / CD34"), vec!["ab12", "cd34"]);
        assert_eq!(catalog_words("-AB12- 2."), vec!["ab12", "2"]);
        assert_eq!(catalog_words("AB12--2"), vec!["ab12", "2"]);
    }

    #[test]
    fn the_pieces_run_together_are_the_squashed_text() {
        for text in ["Ärtist - 7559-61571-2 [AB.12]", "  ", "x/-y"] {
            assert_eq!(catalog_words(text).concat(), squash(text), "{text}");
        }
    }
}
