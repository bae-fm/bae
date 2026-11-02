//! What the catalogs say about releases, in bae's terms, and the asking of
//! them: searches and lookups, the documents a release is read from and what
//! they state, the mapping of each catalog's release onto bae's values, and
//! why a lookup failed.

#[cfg(desktop)]
pub mod album_links;
#[cfg(desktop)]
pub mod discogs_mapper;
#[cfg(desktop)]
pub(crate) mod discogs_mediums;
mod error;
pub mod failure;
#[cfg(desktop)]
pub mod isrc;
#[cfg(desktop)]
pub(crate) mod medium_coverage;
#[cfg(desktop)]
pub mod musicbrainz_mapper;
#[cfg(desktop)]
pub mod payloads;
#[cfg(desktop)]
pub mod providers;
#[cfg(desktop)]
pub(crate) mod release_metadata;
#[cfg(desktop)]
pub mod rip;
#[cfg(desktop)]
pub mod search;
#[cfg(desktop)]
pub mod source_release;

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
