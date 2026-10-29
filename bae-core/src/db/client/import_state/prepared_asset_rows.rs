use super::*;
use crate::import::{CandidatePreparedAssets, CoverSelection};
use bae_util::content_type::ContentType;

/// Make the remote cover bytes stored under the hash the value's: in place
/// when there are bytes, removed when there are none.
pub(super) fn store_asset_rows(
    sql: &SqlContext<'_, '_>,
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

/// The cover bytes stored under the hash, refused unless they are exactly
/// what `cover` needs: a remote cover's, and none for any other.
pub(super) fn load_prepared_assets_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
    cover: Option<&CoverSelection>,
) -> Result<CandidatePreparedAssets, DbError> {
    let assets = load_asset_rows_on(sql, content_hash)?;
    crate::import::preparation::check_cover_bytes(cover, assets.remote_cover.as_ref())
        .map_err(DbError::Message)?;
    Ok(assets)
}

/// The rows under the hash as stored. No validation: the value they load
/// into carries its own.
pub(super) fn load_asset_rows_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
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
