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
        printed_pieces(text)
            .into_iter()
            .map(|(_, piece)| super::squash(piece))
            .collect()
    }

    /// The catalog numbers `text` prints, as it writes them, each once in
    /// first-seen order: what a folder's text offers to be picked. Read by
    /// the cut [`catalog_words`] makes, so an offered number is always whole
    /// numbers of its line, and the text prints it by
    /// `CandidateText::prints_catalog`.
    ///
    /// A number is a piece that begins with a capital letter, writes every
    /// letter in capitals and holds three digits or more — `WPCR-80001`,
    /// `CDP7.46437`, `BVCK-15024/5`. A piece of capitals and at most two
    /// digits followed by one space and a piece that begins with a digit is
    /// one number with it — `SD 19244-2`, `Z1 12345` — as long as the two
    /// hold three digits or more. Separators are kept as written, since
    /// MusicBrainz indexes `WPCR-80001` and `WPCR 80001` apart.
    pub(crate) fn printed_catalog_numbers(text: &str) -> Vec<String> {
        let pieces = printed_pieces(text);
        let mut numbers: Vec<String> = Vec::new();
        let mut at = 0;
        while at < pieces.len() {
            let (start, piece) = pieces[at];
            let joined = pieces.get(at + 1).filter(|(next_start, next)| {
                is_prefix(piece)
                    && text[start + piece.len()..*next_start] == *" "
                    && next.starts_with(|c: char| c.is_ascii_digit())
                    && digits(piece) + digits(next) >= 3
            });
            let number = match joined {
                Some((next_start, next)) => {
                    at += 2;
                    Some(&text[start..next_start + next.len()])
                }
                None => {
                    at += 1;
                    (is_capitals(piece) && digits(piece) >= 3).then_some(piece)
                }
            };
            if let Some(number) = number {
                if !numbers.iter().any(|seen| seen == number) {
                    numbers.push(number.to_string());
                }
            }
        }
        numbers
    }

    /// The text cut as [`catalog_words`] cuts it, each piece as written with
    /// the byte it begins at.
    fn printed_pieces(text: &str) -> Vec<(usize, &str)> {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let plain: Vec<char> = chars.iter().map(|(_, c)| *c).collect();
        let mut pieces = Vec::new();
        let mut start: Option<usize> = None;
        for (at, &(byte, c)) in chars.iter().enumerate() {
            if c.is_alphanumeric() {
                start.get_or_insert(byte);
            } else if !(joins(c) && joined(&plain, at)) {
                if let Some(begun) = start.take() {
                    pieces.push((begun, &text[begun..byte]));
                }
            }
        }
        if let Some(begun) = start {
            pieces.push((begun, &text[begun..]));
        }
        pieces
    }

    /// Whether `piece` begins with a capital letter and writes no letter in
    /// lower case.
    fn is_capitals(piece: &str) -> bool {
        piece.starts_with(|c: char| c.is_ascii_uppercase())
            && !piece.chars().any(char::is_lowercase)
    }

    /// Whether `piece` can begin a number printed with a space inside it:
    /// capitals and at most two digits, joined by nothing.
    fn is_prefix(piece: &str) -> bool {
        is_capitals(piece)
            && digits(piece) <= 2
            && piece.chars().all(|c| c.is_ascii_alphanumeric())
    }

    fn digits(piece: &str) -> usize {
        piece.chars().filter(char::is_ascii_digit).count()
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
    fn a_printed_number_is_whole_numbers_of_its_line() {
        assert_eq!(
            printed_catalog_numbers("Label AB 12345-2 (1986) WPCR-80001"),
            vec!["AB 12345-2", "WPCR-80001"]
        );
        assert_eq!(
            printed_catalog_numbers("Z1 12345, MD6-233"),
            vec!["Z1 12345", "MD6-233"]
        );
        assert_eq!(
            printed_catalog_numbers("BVCK-15024/5 BVCP 21011~2"),
            vec!["BVCK-15024/5", "BVCP 21011"]
        );
        // Lower case, too few digits, or a letter piece before a word.
        assert!(printed_catalog_numbers("lowercase-12345 CD 1 SIDE 2 Box 123 AB CD12").is_empty());
    }

    #[test]
    fn every_printed_number_is_printed_by_the_same_cut() {
        let line = "Artist - Album AB 12345-2 [XY-100] Z1 12345";
        let pieces = catalog_words(line);
        for number in printed_catalog_numbers(line) {
            let key = squash(&number);
            let mut run = String::new();
            let printed = pieces.iter().enumerate().any(|(start, _)| {
                run.clear();
                pieces[start..].iter().any(|piece| {
                    run.push_str(piece);
                    run == key
                })
            });
            assert!(printed, "{number}");
        }
    }

    #[test]
    fn the_pieces_run_together_are_the_squashed_text() {
        for text in ["Ärtist - 7559-61571-2 [AB.12]", "  ", "x/-y"] {
            assert_eq!(catalog_words(text).concat(), squash(text), "{text}");
        }
    }
}
