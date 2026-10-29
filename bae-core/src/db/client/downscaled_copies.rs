//! The downscaled copies a catalog serves of a remote image, read on the row
//! of the image they belong to: one JSON array per image, smallest first.

use super::DbError;
use bae_catalogs::remote_image::DownscaledCopy;

/// The copies `table`, aliased `copy`, holds where `belongs` holds, as a
/// JSON array in `max_edge` order; `[]` for an image with none.
pub(super) fn copies_json_sql(table: &str, belongs: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object('url', copy.url, 'max_edge', copy.max_edge) \
                                  ORDER BY copy.max_edge) \
          FROM {table} AS copy WHERE {belongs})"
    )
}

/// The copies a [`copies_json_sql`] column holds.
pub(super) fn copies_of(json: &str) -> Result<Vec<DownscaledCopy>, DbError> {
    serde_json::from_str(json)
        .map_err(|error| DbError::Message(format!("stored image copies {json:?}: {error}")))
}
