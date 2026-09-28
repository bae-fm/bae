//! When two pieces of written text name one thing: the rules identification,
//! lookup, pressing pairing and artist and label identity compare text by.
//! Every such comparison calls a rule here; none folds text its own way.
//!
//! Folding, which the other rules are built on:
//!
//! - [`squash`]: case, diacritics and everything but letters and digits
//!   dropped — one value however it is punctuated.
//! - [`words`]: the squashed runs of letters and digits.
//! - [`written_words`]: the words as written, case kept, dots between letters
//!   dropped — where codes and initials are recognized.
//! - [`normalize`]: case and diacritics dropped, spacing collapsed, ends
//!   trimmed — one name however it is cased or accented.
//! - [`is_stop_word`]: an article, conjunction or preposition, which says
//!   nothing about which album or artist a name is.
//!
//! The rules for each kind of value:
//!
//! - [`catalog_key`]: two catalog numbers are one number.
//! - [`strip_trailing_brackets`]: an album title or folder name without the
//!   bracketed tails that name its edition or catalog number.
//! - [`track_title_key`]: two track titles are one song.
//! - [`album_title_words`]: the words of an album's title that say which
//!   album it is.
//! - [`LabelName`]: two label names name one label.
//! - [`is_various_artists`]: a credit that names a compilation, not an
//!   artist.
//!
//! A barcode is compared by [`crate::barcode::comparison_key`], which lives
//! with the symbology rules it is built on.

mod catalog_number;
mod fold;

pub(crate) use catalog_number::catalog_key;
pub(crate) use fold::{normalize, squash};

desktop_only! {
    mod artist;
    mod label;
    mod title;

    pub(crate) use artist::is_various_artists;
    pub(crate) use fold::{is_stop_word, words, written_words};
    pub(crate) use label::LabelName;
    pub(crate) use title::{album_title_words, strip_trailing_brackets, track_title_key};
}
