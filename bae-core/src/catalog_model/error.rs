//! Why asking a catalog, or reading what it answered, failed.
//!
//! Wire-level errors are kept structurally rather than flattened to a string,
//! so a consumer that needs to tell a MusicBrainz timeout from a malformed
//! payload can read it off the chain.

#[cfg(desktop)]
use bae_catalogs::catalog::Catalog;

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// A MusicBrainz request failed. Network/Timeout/Provider{status}
    /// distinctions preserved from the wire.
    #[cfg(desktop)]
    #[error("MusicBrainz request failed: {0}")]
    MusicBrainz(#[from] bae_catalogs::musicbrainz::MusicBrainzError),

    /// A Wikidata request failed, with the same wire-level distinctions as
    /// a MusicBrainz one.
    #[cfg(desktop)]
    #[error("Wikidata request failed: {0}")]
    Wikidata(#[from] bae_catalogs::wikidata::WikidataError),

    /// A Discogs request failed. InvalidApiKey/NotFound preserved.
    #[cfg(desktop)]
    #[error("Discogs request failed: {0}")]
    Discogs(#[from] bae_catalogs::discogs::client::DiscogsError),

    /// A Discogs operation was requested but no API key is configured.
    #[cfg(desktop)]
    #[error("Discogs API key not configured")]
    DiscogsNotConfigured,

    /// The source responded, but its payload can't be mapped to a release (no
    /// artist credits, missing release_group, medium with no tracks, no track title, ...).
    ///
    /// The field is `catalog`, not `source`: thiserror reserves a field
    /// literally named `source` for the error-chain source, which a `Catalog`
    /// (not an `Error`) can't be.
    #[cfg(desktop)]
    #[error("{} release data cannot be mapped: {detail}", catalog.as_str())]
    SourceData { catalog: Catalog, detail: String },

    /// A catalog's image or an archive gallery could not be read.
    #[error(transparent)]
    RemoteImage(#[from] bae_catalogs::remote_image::RemoteImageError),
}
