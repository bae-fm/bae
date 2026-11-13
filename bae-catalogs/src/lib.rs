//! The catalogs bae asks about releases — MusicBrainz, Discogs, Wikidata —
//! and what their answers are made of: the catalogs bae knows and the pages
//! they publish, the covers they offer and the cache those images are read
//! through, and the rate every request to them is admitted at. Then what the
//! catalogs say about releases, in bae's terms, and the asking of them:
//! searches and lookups, the documents a release is read from and what they
//! state, the mapping of each catalog's release onto bae's values, and why a
//! lookup failed.
//!
//! The clients serve the import pipeline, so a mobile build compiles only
//! what a library on any device names: the catalogs, the covers and their
//! cache, the admission a call is made with, the records a library stores of
//! a catalog's release, and why a lookup failed.

pub mod catalog;
pub mod credits;
mod error;
pub mod failure;
pub mod rate_limiter;
pub mod records;
pub mod remote_image;
pub mod works;

#[cfg(desktop)]
pub mod album_links;
#[cfg(desktop)]
pub mod discogs;
#[cfg(desktop)]
pub mod discogs_mapper;
#[cfg(desktop)]
pub(crate) mod discogs_mediums;
pub(crate) mod discogs_track_scope;
#[cfg(all(any(test, feature = "test-utils"), desktop))]
pub mod fake_catalogs;
#[cfg(desktop)]
pub mod isrc;
#[cfg(desktop)]
pub mod medium_coverage;
#[cfg(desktop)]
pub mod musicbrainz;
#[cfg(desktop)]
pub mod musicbrainz_mapper;
#[cfg(desktop)]
pub mod payloads;
#[cfg(desktop)]
pub mod providers;
#[cfg(desktop)]
pub mod reading;
#[cfg(desktop)]
pub mod release_metadata;
#[cfg(desktop)]
pub mod release_values;
#[cfg(desktop)]
pub mod rip;
#[cfg(desktop)]
pub mod search;
#[cfg(desktop)]
mod serde_helpers;
#[cfg(desktop)]
pub mod source_release;
#[cfg(all(any(test, feature = "test-utils"), desktop))]
pub mod test_documents;
#[cfg(test)]
mod test_fixtures;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_image_host;
#[cfg(all(any(test, feature = "test-utils"), desktop))]
pub mod test_releases;
#[cfg(desktop)]
pub mod wikidata;

pub use error::CatalogError;
pub use failure::{Failure, InternalFailure, LookupFailure};
#[cfg(desktop)]
pub use rip::{AudioOrigin, AudioSource, CdProof, DownloadProof, StoreMarker};
#[cfg(desktop)]
pub use search::{SearchQuery, SourceFailure, SourceLookup};

/// The four-digit year at the head of a metadata date string (`"1998"`,
/// `"1998-05-01"`), or `None` when the value is absent or has no leading year.
#[cfg(desktop)]
pub(crate) fn parse_year(date: Option<&str>) -> Option<i32> {
    date?.split('-').next()?.parse().ok()
}
