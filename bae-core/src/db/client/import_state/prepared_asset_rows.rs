use super::*;
use crate::import::{CandidatePreparedAssets, CoverSelection};
use crate::util::content_type::ContentType;

/// Replace the remote cover bytes stored under the hash with the value's.
pub(super) fn replace_asset_rows(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    assets: &CandidatePreparedAssets,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_remote_cover_asset WHERE content_hash = ?",
        [content_hash],
    )?;
    if let Some(image) = &assets.remote_cover {
        sql.execute(
            "INSERT INTO import_candidate_remote_cover_asset \
                 (content_hash, content_type, bytes) VALUES (?, ?, ?)",
            params![content_hash, image.content_type.as_str(), image.bytes],
        )?;
    }
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
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<CandidatePreparedAssets, DbError> {
    let remote_cover = sql
        .query_row(
            "SELECT content_type, bytes FROM import_candidate_remote_cover_asset \
             WHERE content_hash = ?",
            [content_hash],
            |row| {
                Ok(crate::import::cover_art::RemoteImage {
                    content_type: ContentType::from_mime(&row.get::<_, String>(0)?),
                    bytes: row.get(1)?,
                })
            },
        )
        .optional()?;
    Ok(CandidatePreparedAssets { remote_cover })
}
