use super::*;
use crate::CandidatePreparedAssets;
use bae_util::content_type::ContentType;

/// Make the remote cover bytes stored under the hash the value's: in place
/// when there are bytes, removed when there are none.
pub(super) fn store_asset_rows(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    assets: &CandidatePreparedAssets,
) -> Result<(), DbError> {
    match &assets.remote_cover {
        Some(image) => sql.execute(
            "INSERT INTO import_candidate_remote_cover_asset \
                 (content_hash, content_type, bytes) VALUES (?, ?, ?) \
             ON CONFLICT (content_hash) DO UPDATE SET \
                 content_type = excluded.content_type, bytes = excluded.bytes",
            params![content_hash, image.content_type.as_str(), image.bytes],
        )?,
        None => sql.execute(
            "DELETE FROM import_candidate_remote_cover_asset WHERE content_hash = ?",
            [content_hash],
        )?,
    };
    Ok(())
}

/// The rows under the hash as stored. The save that writes them checks
/// them against the cover ([`crate::preparation::CandidatePreparation::validate`]).
pub(super) fn load_asset_rows_on(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
    content_hash: &str,
) -> Result<CandidatePreparedAssets, DbError> {
    let remote_cover = sql
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
    Ok(CandidatePreparedAssets { remote_cover })
}

#[cfg(test)]
#[path = "prepared_asset_rows_tests.rs"]
mod tests;
