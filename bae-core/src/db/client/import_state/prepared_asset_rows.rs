use super::*;
use crate::import::{CandidatePreparedAssets, CoverSelection, PreparedArtistImage};
use crate::util::content_type::ContentType;
use std::collections::BTreeSet;

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

fn replace_source_artist_rows(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    source_discogs_artist_ids: &BTreeSet<String>,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_source_artist WHERE content_hash = ?",
        [content_hash],
    )?;
    for discogs_artist_id in source_discogs_artist_ids {
        sql.execute(
            "INSERT INTO import_candidate_source_artist (content_hash, discogs_artist_id) \
             VALUES (?, ?)",
            params![content_hash, discogs_artist_id],
        )?;
    }
    Ok(())
}

fn replace_artist_asset_rows(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    assets: &[PreparedArtistImage],
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_artist_asset WHERE content_hash = ?",
        [content_hash],
    )?;
    for asset in assets {
        match asset {
            PreparedArtistImage::Image {
                discogs_artist_id,
                source_url,
                image,
            } => {
                sql.execute(
                    "INSERT INTO import_candidate_artist_asset \
                         (content_hash, discogs_artist_id, answer, source_url, content_type, bytes) \
                     VALUES (?, ?, 'image', ?, ?, ?)",
                    params![
                        content_hash,
                        discogs_artist_id,
                        source_url,
                        image.content_type.as_str(),
                        image.bytes,
                    ],
                )?;
            }
            PreparedArtistImage::Nothing { discogs_artist_id } => {
                sql.execute(
                    "INSERT INTO import_candidate_artist_asset \
                         (content_hash, discogs_artist_id, answer) VALUES (?, ?, 'nothing')",
                    params![content_hash, discogs_artist_id],
                )?;
            }
        }
    }
    Ok(())
}

const REQUIRED_DISCOGS_ARTIST_IDS_SQL: &str =
    "SELECT discogs_artist_id FROM import_candidate_source_artist \
     WHERE content_hash = ? \
     UNION \
     SELECT assignment.discogs_artist_id \
     FROM import_candidate_album_artist_assignment assignment \
     WHERE assignment.content_hash = ? \
       AND assignment.assignment_kind = 'new' \
       AND assignment.discogs_artist_id IS NOT NULL \
     UNION \
     SELECT assignment.discogs_artist_id \
     FROM import_candidate_track_artist_assignment assignment \
     JOIN import_candidate_track track \
       ON track.content_hash = assignment.content_hash \
      AND track.track_id = assignment.track_id \
      AND track.dropped = 0 \
      AND track.file_kind IS NOT NULL \
     WHERE assignment.content_hash = ? \
       AND assignment.assignment_kind = 'new' \
       AND assignment.discogs_artist_id IS NOT NULL";

fn required_discogs_artist_ids_for_read(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<BTreeSet<String>, DbError> {
    Ok(sql
        .query(
            REQUIRED_DISCOGS_ARTIST_IDS_SQL,
            params![content_hash, content_hash, content_hash],
            |row| row.get(0),
        )?
        .into_iter()
        .collect())
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

pub(super) fn load_source_artist_ids_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<BTreeSet<String>, DbError> {
    Ok(sql
        .query(
            "SELECT discogs_artist_id FROM import_candidate_source_artist \
             WHERE content_hash = ? ORDER BY discogs_artist_id",
            [content_hash],
            |row| row.get(0),
        )?
        .into_iter()
        .collect())
}

/// The rows under the hash as stored, and whether the preparation marker
/// says they are a complete answer set. No validation: the value they load
/// into carries its own.
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

/// Replace every asset row group under the hash from the value: the source
/// artists, the remote cover bytes, the artist image answers, and the marker
/// that says the set is complete.
pub(super) fn replace_asset_rows(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    source_discogs_artist_ids: &BTreeSet<String>,
    assets: &CandidatePreparedAssets,
    prepared: bool,
) -> Result<(), DbError> {
    replace_source_artist_rows(sql, content_hash, source_discogs_artist_ids)?;
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
    replace_artist_asset_rows(sql, content_hash, &assets.artist_images)?;
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
    let expected = required_discogs_artist_ids_for_read(sql, content_hash)?;
    let actual: BTreeSet<_> = assets
        .artist_images
        .iter()
        .map(|asset| asset.discogs_artist_id().to_string())
        .collect();
    if actual.len() != assets.artist_images.len() || actual != expected {
        return Err(DbError::Message(format!(
            "candidate {content_hash} has an incomplete prepared artist asset set: expected {expected:?}, got {actual:?}"
        )));
    }
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
    let rows = sql.query(
        "SELECT discogs_artist_id, answer, source_url, content_type, bytes \
         FROM import_candidate_artist_asset WHERE content_hash = ? \
         ORDER BY discogs_artist_id",
        [content_hash],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
            ))
        },
    )?;
    let mut artist_images = Vec::with_capacity(rows.len());
    for (discogs_artist_id, answer, source_url, content_type, bytes) in rows {
        let asset = match answer.as_str() {
            "nothing" => PreparedArtistImage::Nothing { discogs_artist_id },
            "image" => PreparedArtistImage::Image {
                discogs_artist_id,
                source_url: source_url.ok_or_else(|| {
                    DbError::Message("a prepared artist image has no source URL".into())
                })?,
                image: crate::import::cover_art::RemoteImage {
                    content_type: ContentType::from_mime(&content_type.ok_or_else(|| {
                        DbError::Message("a prepared artist image has no content type".into())
                    })?),
                    bytes: bytes.ok_or_else(|| {
                        DbError::Message("a prepared artist image has no bytes".into())
                    })?,
                },
            },
            other => {
                return Err(DbError::Message(format!(
                    "prepared artist image answer holds {other:?}"
                )))
            }
        };
        artist_images.push(asset);
    }
    Ok(CandidatePreparedAssets {
        remote_cover,
        artist_images,
    })
}
