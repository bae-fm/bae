//! The downscaled copies a catalog serves of a remote image, which a read
//! selects on the row of the image they belong to: one JSON array per image,
//! smallest first.

use super::DbError;
use bae_catalogs::remote_image::DownscaledCopy;

/// The copies a JSON array of `{url, max_edge}` objects holds.
pub(crate) fn copies_of(json: &str) -> Result<Vec<DownscaledCopy>, DbError> {
    serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("stored image copies", json, error))
}
