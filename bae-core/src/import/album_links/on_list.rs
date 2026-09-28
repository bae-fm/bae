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

use super::{names_other_album, AlbumLink, AlbumStatement, Found};
use crate::import::search::MetadataResult;
use crate::import::types::{Catalog, MetadataRef};

/// What a release prints that can say which album it is, read into the form
/// two releases' are compared in.
struct Printed {
    /// Its barcodes' comparison keys.
    barcodes: Vec<String>,
    /// Each label it states both halves of: the label's name, and the
    /// number's key.
    numbers: Vec<(crate::identify::label::LabelName, String)>,
    /// The words of its title that say which album it is: its bracketed
    /// tails and its stop words left out.
    title: Vec<String>,
}

impl Printed {
    fn of(result: &MetadataResult) -> Self {
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
                        crate::identify::label::LabelName::of(label.name()?)?,
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
        self.numbers.iter().any(|(label, number)| {
            other
                .numbers
                .iter()
                .any(|(other_label, other_number)| {
                    number == other_number && label.same_label(other_label)
                })
        })
    }

    /// Whether the two titles share a word that says which album they are.
    fn shares_title_word(&self, other: &Self) -> bool {
        self.title.iter().any(|word| other.title.contains(word))
    }
}

/// The albums `group`'s releases on `list` are, as what they print says,
/// each with the release pair that says it: the other catalogs' releases on
/// the list printing a barcode one of the group's releases prints, and —
/// where none does — printing one of its catalog numbers under the same label.
pub(super) fn albums(group: &str, list: &[&MetadataResult]) -> Vec<AlbumLink> {
    let mut ours: Vec<(&MetadataResult, Printed)> = Vec::new();
    let mut theirs: Vec<(&MetadataResult, Printed)> = Vec::new();
    for &result in list {
        let side = if result.source == Catalog::MusicBrainz {
            if result.source_group_id.as_deref() != Some(group) {
                continue;
            }
            &mut ours
        } else if result.source_group_id.is_some() && names_other_album(result.source) {
            &mut theirs
        } else {
            continue;
        };
        // A release the list holds twice — two lookups returning it — is read
        // once.
        let seen = side.iter().any(|(other, _)| {
            other.source == result.source && other.release_id == result.release_id
        });
        if !seen {
            side.push((result, Printed::of(result)));
        }
    }
    let by_barcode = pairs(&ours, &theirs, Printed::shares_barcode, |musicbrainz_release, release| {
        AlbumStatement::Barcode {
            musicbrainz_release,
            release,
        }
    });
    if !by_barcode.is_empty() {
        return by_barcode;
    }
    pairs(&ours, &theirs, Printed::shares_catalog_number, |musicbrainz_release, release| {
        AlbumStatement::CatalogNumber {
            musicbrainz_release,
            release,
        }
    })
}

/// The albums of `theirs` that `shares` pairs with one of `ours`, and whose
/// titles share a word.
fn pairs(
    ours: &[(&MetadataResult, Printed)],
    theirs: &[(&MetadataResult, Printed)],
    shares: fn(&Printed, &Printed) -> bool,
    stated: fn(String, MetadataRef) -> AlbumStatement,
) -> Vec<AlbumLink> {
    let mut found = Found::default();
    for (our, our_print) in ours {
        for (their, their_print) in theirs {
            let Some(album) = &their.source_group_id else {
                continue;
            };
            if shares(our_print, their_print) && our_print.shares_title_word(their_print) {
                found.push(
                    MetadataRef::new(their.source, album.clone()),
                    stated(
                        our.release_id.clone(),
                        MetadataRef::new(their.source, their.release_id.clone()),
                    ),
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
        .filter(|word| !crate::util::text::is_stop_word(word))
        .collect()
}

#[cfg(test)]
#[path = "on_list_tests.rs"]
mod tests;
