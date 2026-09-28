//! Text folded into the forms it is compared in: case and diacritics dropped,
//! and punctuation dropped or kept only as far as each form says.

use unicode_normalization::UnicodeNormalization;

/// The text decomposed, its combining marks dropped (so diacritics go), and
/// lowercased one character at a time.
fn folded(text: &str) -> impl Iterator<Item = char> + '_ {
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(char::to_lowercase)
}

/// A value as it is looked up: lowercase, diacritics folded away, and
/// everything that is not a letter or a digit dropped — which is what makes
/// `WPCR-80001`, `WPCR 80001` and `wpcr80001` one value.
///
/// The one normalization of a catalog number: the text is searched by it, a
/// struck-out number is matched by it, and the sightings a chosen number folds
/// into one mark line are gathered by it.
pub(crate) fn squash(text: &str) -> String {
    folded(text).filter(|c| c.is_alphanumeric()).collect()
}

/// The text's words, each squashed: a word is a run of letters and digits, so
/// `16033-2` is the words `16033` and `2`, and its words run together are
/// [`squash`] of it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn words(text: &str) -> Vec<String> {
    folded(text)
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// The text's words as it writes them, case kept, with the dots between
/// letters dropped: "D.F.C." is the one word `DFC`. What a code or a set of
/// initials is recognized in, where capitals are what say it is one.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn written_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '.')
        .map(|word| word.replace('.', ""))
        .filter(|word| !word.is_empty())
        .collect()
}

/// Whether `word` says nothing about which album or artist a name is: an
/// article, conjunction or preposition, in the languages record titles are
/// most often in. Compared squashed, so "The" and "the" are one word.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn is_stop_word(word: &str) -> bool {
    STOP_WORDS.contains(&squash(word).as_str())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "at", "by", "for", "from", "in", "of", "on", "or", "the", "to", "with",
    "das", "de", "del", "der", "des", "die", "du", "el", "et", "la", "le", "les", "los", "und",
    "y",
];

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
mod tests {
    use super::*;

    #[test]
    fn squash_keeps_only_letters_and_digits() {
        assert_eq!(squash("WPCR-80001"), "wpcr80001");
        assert_eq!(squash("WPCR 80001"), "wpcr80001");
        assert_eq!(squash("Ärtist Näme"), "artistname");
    }

    #[test]
    fn words_are_the_squashed_runs_of_letters_and_digits() {
        assert_eq!(words("16033-2"), vec!["16033", "2"]);
        assert_eq!(words("  Ärtist  Näme! "), vec!["artist", "name"]);
        assert_eq!(words("Ärtist Näme").concat(), squash("Ärtist Näme"));
        assert!(words("··· —").is_empty());
    }

    #[test]
    fn written_words_keep_case_and_drop_dots_between_letters() {
        assert_eq!(written_words("D.F.C. Records"), vec!["DFC", "Records"]);
        assert_eq!(written_words("Made in E.U."), vec!["Made", "in", "EU"]);
    }

    #[test]
    fn normalize_strips_diacritics() {
        assert_eq!(normalize("Café"), "cafe");
        assert_eq!(normalize("Fjörn"), "fjorn");
    }

    #[test]
    fn normalize_lowercases_and_collapses_whitespace() {
        assert_eq!(normalize("  Album   Title  "), "album title");
        assert_eq!(normalize("Album\tTitle"), "album title");
    }

    #[test]
    fn normalize_strips_leading_trailing_nonalnum() {
        assert_eq!(normalize("\"Album Title\""), "album title");
        assert_eq!(normalize("!!!Album!!!"), "album");
    }

    #[test]
    fn normalize_folds_an_artist_name_however_it_is_cased_or_accented() {
        assert_eq!(normalize("Artist Name"), "artist name");
        assert_eq!(normalize("artist name"), "artist name");
        assert_eq!(normalize("ÄRTIST  Name"), "artist name");
        assert_eq!(normalize("Ärtist Name"), "artist name");
    }
}
