use super::*;
use crate::cover_art::PreparedCover;
use crate::CoverSelection;
use bae_util::content_type::ContentType;

/// The candidate's cover `selection` with the bytes stored for it: a remote
/// selection with the image its write fetched, a folder's image as it is.
/// The cover's save writes the two together, so a remote cover stored
/// without its bytes, or bytes stored beside a cover of the folder, is a
/// damaged row, and the read fails naming it.
pub(super) fn load_prepared_cover_on(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
    content_hash: &str,
    selection: Option<CoverSelection>,
) -> Result<Option<PreparedCover>, DbError> {
    let fetched = sql
        .query_row(
            "SELECT content_type, bytes FROM import_candidate_remote_cover_asset \
             WHERE content_hash = ?",
            [content_hash],
            |row| {
                Ok(bae_catalogs::remote_image::RemoteImage {
                    content_type: ContentType::from_mime(&row.get::<_, String>(0)?),
                    bytes: row.get(1)?,
                })
            },
        )
        .optional()?;
    match (selection, fetched) {
        (Some(CoverSelection::Remote(image, source)), Some(fetched)) => {
            Ok(Some(PreparedCover::Remote { image, source, fetched }))
        }
        (Some(CoverSelection::Remote(..)), None) => Err(DbError::Message(format!(
            "candidate {content_hash}'s remote cover has no stored bytes"
        ))),
        (Some(CoverSelection::Local(_) | CoverSelection::Embedded(_)) | None, Some(_)) => {
            Err(DbError::Message(format!(
                "candidate {content_hash} stores remote-cover bytes without a remote cover"
            )))
        }
        (Some(CoverSelection::Local(file_id)), None) => Ok(Some(PreparedCover::Local(file_id))),
        (Some(CoverSelection::Embedded(source_file_id)), None) => {
            Ok(Some(PreparedCover::Embedded(source_file_id)))
        }
        (None, None) => Ok(None),
    }
}

#[cfg(test)]
#[path = "prepared_asset_rows_tests.rs"]
mod tests;
