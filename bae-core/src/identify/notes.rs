//! Whether the folder's text names what sets a row apart from the rows beside
//! it in its catalogs' notes — the free text a record's document writes about
//! which pressing it is (see [`crate::import::search::MetadataResult::notes`]).
//!
//! Catalogs often write what tells look-alike pressings apart — where one was
//! made, who pressed it — only as free text, so the text is read as nothing
//! more than words: no country or company is parsed out of it, on purpose.
//! The point it gives is the ranking's last, so a word matched by chance only
//! reorders rows that were equal on everything else.
//!
//! A row's words are its records' notes split by [`words`]. The words another
//! of the rows writes too say nothing about which of them is on the desk; the
//! rest set the row apart, and the row is named when the folder's text states
//! one of those as a whole word. Words shorter than three characters and
//! words of digits alone do not count: the first are fragments — the "W" of
//! "W. Germany" — and the second are the codes a matrix inscription carries,
//! which one catalog entry transcribes and a look-alike's may not, where the
//! folder writes its catalog number and year for fields ranked above this.

use super::agreements::CandidateText;
use crate::import::search::MetadataResult;
use crate::text_match::{is_stop_word, words};
use std::collections::HashSet;

/// For each row, as its records, whether the folder's text states a word of
/// its notes no other row's notes write.
pub(crate) fn names_what_sets_each_apart<'a>(
    rows: impl IntoIterator<Item = &'a [MetadataResult]>,
    text: &CandidateText,
) -> Vec<bool> {
    let words: Vec<HashSet<String>> = rows.into_iter().map(note_words).collect();
    words
        .iter()
        .enumerate()
        .map(|(row, own)| {
            own.iter().any(|word| {
                !words
                    .iter()
                    .enumerate()
                    .any(|(other, theirs)| other != row && theirs.contains(word))
                    && text.states(word)
            })
        })
        .collect()
}

/// The words a row's records' notes write that count.
fn note_words(records: &[MetadataResult]) -> HashSet<String> {
    records
        .iter()
        .flat_map(|record| &record.notes)
        .flat_map(|note| words(note))
        .filter(|word| {
            word.chars().count() >= 3 && !word.chars().all(char::is_numeric) && !is_stop_word(word)
        })
        .collect()
}
