//! What a list's releases print that says two catalogs' albums are one, when
//! no document links them.
//!
//! A MusicBrainz release and a Discogs release on one list that print the
//! same barcode are, as far as anything printed on them says, one product, so
//! their albums are one album. Failing a barcode, the same catalog number
//! under the same label says it too. Either is taken only where the two
//! releases' titles share a word that says which album they are: a barcode
//! typed onto the wrong record, or a number a label reused for another
//! album, is caught by a title that shares nothing.
//!
//! This joins albums, never pressings: whether the two records are one
//! pressing is what `pressing_evidence` weighs, and a year or country they
//! disagree on keeps them two rows of the one album.

use super::{names_other_album, AlbumLink, AlbumStatement, Found, GroupToRead};
use crate::import::search::MetadataResult;
use crate::import::types::MetadataRef;

/// A release on the list, as reading albums reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub release: MetadataRef,
    /// The album its catalog files it under.
    pub album: Option<String>,
    /// The other catalogs' releases its record names as the same release.
    pub links: Vec<MetadataRef>,
    pub printed: Printed,
}

impl Listed {
    pub(crate) fn of(result: &MetadataResult) -> Self {
        Self {
            release: MetadataRef::new(result.source, result.release_id.clone()),
            album: result.source_group_id.clone(),
            links: result.links.clone(),
            printed: Printed::of(result),
        }
    }
}

/// What a release prints that can say which album it is, read into the form
/// two releases' are compared in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Printed {
    /// Its barcodes' comparison keys.
    barcodes: Vec<String>,
    /// Each label it states both halves of: the label as the trade-word rule
    /// reads its name, and the number's key.
    numbers: Vec<(String, String)>,
    /// The words of its title that say which album it is: its bracketed
    /// tails and its stop words left out.
    title: Vec<String>,
}

impl Printed {
    pub(crate) fn of(result: &MetadataResult) -> Self {
        Self {
            barcodes: result
                .barcodes
                .iter()
                .filter_map(|stated| crate::barcode::comparison_key(stated).ok())
                .collect(),
            numbers: result
                .labels
                .iter()
                .filter_map(|label| {
                    Some((
                        crate::identify::label::stated(label.name()?)?,
                        crate::util::text::catalog_key(label.catalog_number()?)?,
                    ))
                })
                .collect(),
            title: title_words(&result.title),
        }
    }

    fn shares_barcode(&self, other: &Self) -> bool {
        self.barcodes.iter().any(|key| other.barcodes.contains(key))
    }

    /// Whether the two print one catalog number under one label.
    fn shares_catalog_number(&self, other: &Self) -> bool {
        self.numbers.iter().any(|number| other.numbers.contains(number))
    }

    /// Whether the two titles share a word that says which album they are.
    fn shares_title_word(&self, other: &Self) -> bool {
        self.title.iter().any(|word| other.title.contains(word))
    }
}

/// The albums `group`'s releases on the list are, as what they print says,
/// each with the release pair that says it: the other catalog's releases on
/// the list printing a barcode one of the group's releases prints, and — where
/// none does — printing one of its catalog numbers under the same label.
pub(super) fn albums(group: &GroupToRead, on_list: &[Listed]) -> Vec<AlbumLink> {
    let by_barcode = pairs(group, on_list, Printed::shares_barcode, |musicbrainz_release, release| {
        AlbumStatement::Barcode {
            musicbrainz_release,
            release,
        }
    });
    if !by_barcode.is_empty() {
        return by_barcode;
    }
    pairs(group, on_list, Printed::shares_catalog_number, |musicbrainz_release, release| {
        AlbumStatement::CatalogNumber {
            musicbrainz_release,
            release,
        }
    })
}

/// The albums of the other catalogs' releases on the list that `shares`
/// pairs with one of `group`'s releases, and whose titles share a word.
fn pairs(
    group: &GroupToRead,
    on_list: &[Listed],
    shares: fn(&Printed, &Printed) -> bool,
    stated: fn(String, MetadataRef) -> AlbumStatement,
) -> Vec<AlbumLink> {
    let mut found = Found::default();
    for ours in &group.releases {
        for theirs in on_list {
            let Some(album) = &theirs.album else {
                continue;
            };
            if names_other_album(theirs.release.catalog)
                && shares(&ours.printed, &theirs.printed)
                && ours.printed.shares_title_word(&theirs.printed)
            {
                found.push(
                    MetadataRef::new(theirs.release.catalog, album.clone()),
                    stated(ours.release.key.clone(), theirs.release.clone()),
                );
            }
        }
    }
    found.links
}

/// A title's words, without its bracketed tails — "(Remastered)", "[Deluxe
/// Edition]" — and without the words that say nothing about which album it
/// is.
fn title_words(title: &str) -> Vec<String> {
    let bare = crate::signals::candidate_text::strip_trailing_brackets(title);
    crate::identify::agreements::words(&bare)
        .into_iter()
        .filter(|word| !STOP_WORDS.contains(&word.as_str()))
        .collect()
}

/// Articles, conjunctions and prepositions, in the languages record titles
/// are most often in, compared the way a title's words are read.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "at", "by", "for", "from", "in", "of", "on", "or", "the", "to", "with",
    "das", "de", "del", "der", "des", "die", "du", "el", "et", "la", "le", "les", "los", "und",
    "y",
];

#[cfg(test)]
#[path = "on_list_tests.rs"]
mod tests;
