//! What a search asks each catalog: a typed manual search, or a run's search
//! by catalog number, in each catalog's own terms, and which of the releases a
//! catalog answers with answer it.

use super::MetadataResult;
use crate::discogs::client::DiscogsSearchParams;
use crate::musicbrainz::ReleaseSearchParams;
use bae_types::barcode::{comparison_key, written_digits, Barcode, Unusable};
use bae_types::pressing::ReleaseLabel;
use bae_types::text_match::catalog_key;

/// What one provider is asked: the request in each catalog's own terms, and
/// which of the releases it answers with answer the question.
pub trait ProviderQuery: std::fmt::Debug {
    fn musicbrainz_params(&self) -> ReleaseSearchParams;
    fn discogs_params(&self) -> DiscogsSearchParams;
    /// Drop from `results` what the catalog returned that does not answer.
    fn keep_answers(&self, results: &mut Vec<MetadataResult>);
}

/// A typed manual search, one of three modes. Every configured provider is
/// asked; [`search_source`](super::search_source) runs one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchQuery {
    General { artist: String, album: String },
    CatalogNumber { catalog_number: String },
    Barcode { barcode: String },
}

impl SearchQuery {
    /// Why a barcode query's value names no product, which a surface states
    /// beside what the search turned up. `None` for a code — one whose check
    /// digit fails too — and for every other query.
    pub fn unusable_barcode(&self) -> Option<Unusable> {
        match self {
            SearchQuery::Barcode { barcode } => comparison_key(barcode).err(),
            SearchQuery::General { .. } | SearchQuery::CatalogNumber { .. } => None,
        }
    }
}

impl ProviderQuery for SearchQuery {
    fn keep_answers(&self, results: &mut Vec<MetadataResult>) {
        match self {
            SearchQuery::CatalogNumber { catalog_number } => keep_numbered(catalog_number, results),
            SearchQuery::General { .. } | SearchQuery::Barcode { .. } => {}
        }
    }

    fn musicbrainz_params(&self) -> ReleaseSearchParams {
        match self {
            SearchQuery::General { artist, album } => ReleaseSearchParams {
                artist: Some(artist.clone()),
                album: Some(album.clone()),
                ..Default::default()
            },
            SearchQuery::CatalogNumber { catalog_number } => ReleaseSearchParams {
                catalog_number: Some(catalog_number.clone()),
                ..Default::default()
            },
            SearchQuery::Barcode { barcode } => {
                ReleaseSearchParams { barcode: Some(barcode_query(barcode)), ..Default::default() }
            }
        }
    }

    fn discogs_params(&self) -> DiscogsSearchParams {
        match self {
            SearchQuery::General { artist, album } => DiscogsSearchParams {
                text: (!artist.trim().is_empty()).then(|| artist.clone()),
                release_title: Some(album.clone()),
                ..Default::default()
            },
            SearchQuery::CatalogNumber { catalog_number } => {
                DiscogsSearchParams { catno: Some(catalog_number.clone()), ..Default::default() }
            }
            SearchQuery::Barcode { barcode } => {
                DiscogsSearchParams { barcode: Some(barcode_query(barcode)), ..Default::default() }
            }
        }
    }
}

/// A run's search by catalog number, narrowed to what it knows of the release
/// it looks for. A short number is printed by hundreds of unrelated releases,
/// and a catalog serves them a page at a time; naming the artist and the
/// label leaves the ones that share them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogNumberSearch {
    pub number: String,
    /// The artist the release is credited to. Discogs matches it as words
    /// anywhere on a release, as a title search's artist is (see
    /// [`DiscogsSearchParams::text`]).
    pub artist: Option<String>,
    /// The label the release prints the number under.
    pub label: Option<String>,
}

impl ProviderQuery for CatalogNumberSearch {
    fn keep_answers(&self, results: &mut Vec<MetadataResult>) {
        keep_numbered(&self.number, results);
    }

    fn musicbrainz_params(&self) -> ReleaseSearchParams {
        ReleaseSearchParams {
            artist: self.artist.clone(),
            label: self.label.clone(),
            catalog_number: Some(self.number.clone()),
            ..Default::default()
        }
    }

    fn discogs_params(&self) -> DiscogsSearchParams {
        DiscogsSearchParams {
            text: self.artist.clone(),
            label: self.label.clone(),
            catno: Some(self.number.clone()),
            ..Default::default()
        }
    }
}

/// Keep only the releases printing `number`. Both catalogs match a catalog
/// number loosely — asking for `CL 719` returns `CL 1719` and `WPCL-719` — so
/// a release answers only when one of its labels' numbers is the asked one,
/// compared as [`catalog_key`] compares them.
fn keep_numbered(number: &str, results: &mut Vec<MetadataResult>) {
    let Some(asked) = catalog_key(number) else {
        return;
    };
    results.retain(|result| {
        result
            .labels
            .iter()
            .filter_map(ReleaseLabel::catalog_number)
            .filter_map(catalog_key)
            .any(|stated| stated == asked)
    });
}

/// The barcode a `Barcode` query asks for. A code is asked for in the one
/// spelling [`Barcode`] gives it — the spelling identify's own lookups use —
/// so a code typed with its print spacing, or as a twelve-digit UPC-A, is the
/// same question as the one the run asks. A value that fails its check digit
/// is still asked for, by its digits: a person may be looking for a record
/// that states it that way. Anything not written as a code is asked for as
/// written.
fn barcode_query(barcode: &str) -> String {
    Barcode::stated(barcode)
        .map(Barcode::into_string)
        .or_else(|| written_digits(barcode))
        .unwrap_or_else(|| barcode.to_string())
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
