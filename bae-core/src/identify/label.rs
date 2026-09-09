//! Which label a label's name names, without the trade word it trails.
//!
//! A folder writes "Warner Bros."; the sources write "Warner Bros. Records".
//! That is one label. The word a company appends to its name — Records,
//! Recordings, Music, Ltd — says what kind of business it is and nothing about
//! which one it is, so ranking drops it before asking whether the candidate's
//! text states the label.
//!
//! A name that is nothing but trade words — "Records" — names no label at all,
//! and so states nothing about a result.

use std::sync::OnceLock;

/// What `name` says about which label it is: its words, with the trade words
/// it ends on dropped, run together the way the candidate's text is read.
/// `None` when nothing of the name is left.
pub(super) fn stated(name: &str) -> Option<String> {
    let mut words = super::agreements::words(name);
    while let Some(tail) = tails().iter().find(|tail| words.ends_with(tail)) {
        words.truncate(words.len() - tail.len());
    }
    (!words.is_empty()).then(|| words.concat())
}

/// Each trade word as the words it is written with, compared the way the
/// candidate's text is read — case, spacing and punctuation dropped, so
/// "Record Co." and "record co" are one entry. An entry that is written with
/// no words at all would end every name, and is left out. Built once.
fn tails() -> &'static [Vec<String>] {
    static TAILS: OnceLock<Vec<Vec<String>>> = OnceLock::new();
    TAILS.get_or_init(|| {
        TRADE_WORDS
            .iter()
            .map(|entry| super::agreements::words(entry))
            .filter(|tail| !tail.is_empty())
            .collect()
    })
}

/// The words a label's name trails that name no label of their own.
static TRADE_WORDS: &[&str] = &[
    "Records",
    "Recordings",
    "Record Co.",
    "Music",
    "Entertainment",
    "Productions",
    "Label",
    "Ltd",
    "Inc",
    "LLC",
    "GmbH",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every entry is written with words: one that is not would be dropped
    /// from the table, and the trade word it means to name would stay on
    /// every label's name.
    #[test]
    fn every_trade_word_is_written_with_words() {
        for entry in TRADE_WORDS {
            assert!(
                !super::super::agreements::words(entry).is_empty(),
                "{entry} is written with no words"
            );
        }
        assert_eq!(tails().len(), TRADE_WORDS.len());
    }

    /// A name is left with what names the label, however many trade words it
    /// trails and however they are punctuated.
    #[test]
    fn a_name_keeps_only_what_names_the_label() {
        assert_eq!(
            stated("Warner Bros. Records").as_deref(),
            Some("warnerbros")
        );
        assert_eq!(stated("Sony Music Entertainment").as_deref(), Some("sony"));
        assert_eq!(stated("Nonesuch Record Co.").as_deref(), Some("nonesuch"));
        assert_eq!(stated("Ninja Tune").as_deref(), Some("ninjatune"));
    }

    /// Trade words alone name no label.
    #[test]
    fn a_name_of_trade_words_alone_names_nothing() {
        assert_eq!(stated("Records"), None);
        assert_eq!(stated("Music Entertainment"), None);
        assert_eq!(stated(""), None);
        assert_eq!(stated("···"), None);
    }
}
