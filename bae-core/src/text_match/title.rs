//! When two titles name one album or one track.
//!
//! A title may carry bracketed parts that name a version, an edition or a
//! catalog number rather than the work: `(Remastered)`, `[Deluxe]`, `[XX34b]`.
//! Two rules take them off, for two kinds of title. An album's title and a
//! folder's name lose only the brackets they end on, since a bracket that
//! opens one — "(Leading Words) Album Title" — is part of the name. A track's
//! title loses every bracketed part, wherever it sits, since a track list
//! writes a featured artist or a version mid-title as often as at its end.

use regex::Regex;
use std::sync::OnceLock;

/// A name without the bracketed tails a person hangs off it — the catalog
/// number, the edition, the year: `Album Title [XX34b] (2020)` is
/// `Album Title`. Every trailing group goes, so a name that is nothing but
/// brackets comes back empty. What a folder name and an album tag are read
/// as when the words alone are wanted.
pub(crate) fn strip_trailing_brackets(raw: &str) -> String {
    static TRAILING_BRACKET: OnceLock<Regex> = OnceLock::new();
    let bracket =
        TRAILING_BRACKET.get_or_init(|| Regex::new(r"\s*[\[\(][^\]\)]*[\]\)]\s*$").unwrap());
    let mut s = raw.trim().to_string();
    // Repeatedly — a name may carry several (`Album Title [Deluxe] (2020)`).
    loop {
        let stripped = bracket.replace(&s, "").into_owned();
        let stripped = stripped.trim_end().to_string();
        if stripped == s {
            break;
        }
        s = stripped;
    }
    s
}

/// An album's title as it names the album: without its trailing bracketed
/// tails (see [`strip_trailing_brackets`]), or whole when it is nothing but
/// brackets — then the brackets are its name.
pub(crate) fn bare_album_title(title: &str) -> String {
    let bare = strip_trailing_brackets(title);
    if bare.is_empty() {
        title.to_string()
    } else {
        bare
    }
}

/// A track title as two are compared: without its bracketed parts, wherever
/// they sit, since they name a version rather than the song — "(Remastered)",
/// "[Live]" — and squashed, see [`super::squash`].
pub(crate) fn track_title_key(title: &str) -> String {
    let mut kept = String::with_capacity(title.len());
    let mut depth = 0u32;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => kept.push(c),
            _ => {}
        }
    }
    super::squash(&kept)
}

/// An album title's words that say which album it is: its trailing
/// bracketed tails — "(Remastered)", "[Deluxe Edition]" — and its stop words
/// left out. Two releases whose titles share one of these are taken to be
/// able to name one album.
pub(crate) fn album_title_words(title: &str) -> Vec<String> {
    super::words(&strip_trailing_brackets(title))
        .into_iter()
        .filter(|word| !super::is_stop_word(word))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_brackets_leave_the_words() {
        assert_eq!(
            strip_trailing_brackets("Album Title [XX34b]"),
            "Album Title"
        );
        assert_eq!(
            strip_trailing_brackets("Album Title (Deluxe) [2020]"),
            "Album Title"
        );
        assert_eq!(strip_trailing_brackets("[XX34b]"), "");
        assert_eq!(strip_trailing_brackets("Album Title"), "Album Title");
    }

    /// A bracket that opens a name, or sits inside it, is part of it.
    #[test]
    fn only_trailing_brackets_go() {
        assert_eq!(
            strip_trailing_brackets("(Leading Words) Album Title?"),
            "(Leading Words) Album Title?"
        );
        assert_eq!(
            strip_trailing_brackets("Album (Middle) Title"),
            "Album (Middle) Title"
        );
    }

    #[test]
    fn an_album_title_is_bare_unless_it_is_nothing_but_brackets() {
        assert_eq!(bare_album_title("Album Title (Remastered)"), "Album Title");
        assert_eq!(bare_album_title("[Untitled]"), "[Untitled]");
    }

    #[test]
    fn a_track_title_loses_every_bracketed_part() {
        assert_eq!(track_title_key("Track Title (Remastered)"), "tracktitle");
        assert_eq!(
            track_title_key("Track [Live] Title (feat. Guest)"),
            "tracktitle"
        );
        assert_eq!(track_title_key("(Intro)"), "");
    }

    #[test]
    fn an_album_title_s_words_leave_out_its_tails_and_stop_words() {
        assert_eq!(
            album_title_words("The Album of Titles (Remastered)"),
            vec!["album", "titles"]
        );
    }
}
