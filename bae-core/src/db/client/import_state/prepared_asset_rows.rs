use super::*;
use crate::import::{CandidatePreparedAssets, CoverSelection};
use crate::util::content_type::ContentType;

fn validate_remote_cover(
    cover: Option<&CoverSelection>,
    image: Option<&crate::import::cover_art::RemoteImage>,
) -> Result<(), DbError> {
    match (cover, image) {
        (Some(CoverSelection::Remote(_, _)), Some(_))
        | (Some(CoverSelection::Local(_) | CoverSelection::Embedded(_)) | None, None) => Ok(()),
        (Some(CoverSelection::Remote(_, _)), None) => Err(DbError::Message(
            "a remote candidate cover has no prepared bytes".into(),
        )),
        (Some(CoverSelection::Local(_) | CoverSelection::Embedded(_)) | None, Some(_)) => Err(
            DbError::Message("candidate remote-cover bytes have no remote cover selection".into()),
        ),
    }
}

fn require_preparation_marker(sql: &SqlReadContext<'_>, content_hash: &str) -> Result<(), DbError> {
    let prepared = sql
        .query_row(
            "SELECT 1 FROM import_candidate_asset_preparation WHERE content_hash = ?",
            [content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !prepared {
        return Err(missing_preparation_marker(content_hash));
    }
    Ok(())
}

fn missing_preparation_marker(content_hash: &str) -> DbError {
    DbError::Message(format!(
        "candidate {content_hash} has no complete prepared asset set"
    ))
}

/// The rows under the hash as stored, and whether the preparation marker
/// says they are complete. No validation: the value they load into carries
/// its own.
pub(super) fn load_asset_rows_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<(CandidatePreparedAssets, bool), DbError> {
    let prepared = sql
        .query_row(
            "SELECT 1 FROM import_candidate_asset_preparation WHERE content_hash = ?",
            [content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok((load_asset_rows_unmarked(sql, content_hash)?, prepared))
}

/// Replace every asset row under the hash from the value: the remote cover
/// bytes, and the marker that says they are complete.
pub(super) fn replace_asset_rows(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    assets: &CandidatePreparedAssets,
    prepared: bool,
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
    if prepared {
        sql.execute(
            "INSERT INTO import_candidate_asset_preparation (content_hash) VALUES (?) \
             ON CONFLICT (content_hash) DO NOTHING",
            [content_hash],
        )?;
    } else {
        sql.execute(
            "DELETE FROM import_candidate_asset_preparation WHERE content_hash = ?",
            [content_hash],
        )?;
    }
    Ok(())
}

pub(super) fn load_prepared_assets_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
    cover: Option<&CoverSelection>,
) -> Result<CandidatePreparedAssets, DbError> {
    require_preparation_marker(sql, content_hash)?;
    let assets = load_asset_rows_unmarked(sql, content_hash)?;
    validate_remote_cover(cover, assets.remote_cover.as_ref())?;
    Ok(assets)
}

fn load_asset_rows_unmarked(
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
