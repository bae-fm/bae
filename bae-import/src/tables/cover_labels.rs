//! A cover's label as stored beside its image: one JSON text per cover.

use super::DbError;
use bae_catalogs::remote_image::CoverLabel;

/// The text a cover's label is stored as.
pub(crate) fn label_json(label: &CoverLabel) -> Result<String, DbError> {
    serde_json::to_string(label).map_err(|error| DbError::encode("a cover's label", error))
}

/// The label a [`label_json`] text holds.
pub(crate) fn label_of(json: &str) -> Result<CoverLabel, DbError> {
    serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("a stored cover's label", json, error))
}
