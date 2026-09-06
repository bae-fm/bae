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

pub(super) fn replace_prepared_assets(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    cover: Option<&CoverSelection>,
    source_discogs_artist_ids: &BTreeSet<String>,
    assets: &CandidatePreparedAssets,
) -> Result<(), DbError> {
    validate_remote_cover(cover, assets.remote_cover.as_ref())?;
    replace_source_artist_rows(sql, content_hash, source_discogs_artist_ids)?;
    let expected = required_discogs_artist_ids_for_write(sql, content_hash)?;
    let actual: BTreeSet<_> = assets
        .artist_images
        .iter()
        .map(|asset| asset.discogs_artist_id().to_string())
        .collect();
    if actual.len() != assets.artist_images.len() {
        return Err(DbError::Message(
            "candidate artist assets contain a duplicate Discogs artist ID".into(),
        ));
    }
    if actual != expected {
        return Err(DbError::Message(format!(
            "candidate artist assets do not match the draft's Discogs artist IDs: expected {expected:?}, got {actual:?}"
        )));
    }

    replace_remote_cover_asset(sql, content_hash, cover, assets.remote_cover.as_ref())?;
    replace_artist_asset_rows(sql, content_hash, &assets.artist_images)?;
    sql.execute(
        "INSERT INTO import_candidate_asset_preparation (content_hash) VALUES (?) \
         ON CONFLICT (content_hash) DO NOTHING",
        [content_hash],
    )?;
    Ok(())
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

fn required_discogs_artist_ids_for_write(
    sql: &SqlContext<'_, '_>,
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

pub(super) fn replace_artist_assets_for_stored_draft(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    source_discogs_artist_ids: &BTreeSet<String>,
    assets: &[PreparedArtistImage],
) -> Result<(), DbError> {
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
    replace_source_artist_rows(sql, content_hash, source_discogs_artist_ids)?;
    let expected = required_discogs_artist_ids_for_write(sql, content_hash)?;
    let actual: BTreeSet<_> = assets
        .iter()
        .map(|asset| asset.discogs_artist_id().to_string())
        .collect();
    if actual.len() != assets.len() {
        return Err(DbError::Message(
            "candidate artist assets contain a duplicate Discogs artist ID".into(),
        ));
    }
    if actual != expected {
        return Err(DbError::Message(format!(
            "candidate artist assets do not match the stored draft's Discogs artist IDs: expected {expected:?}, got {actual:?}"
        )));
    }
    replace_artist_asset_rows(sql, content_hash, assets)
}

pub(super) fn replace_artist_assets_after_file_edit(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    source_discogs_artist_ids: Option<&BTreeSet<String>>,
    assets: &[PreparedArtistImage],
) -> Result<(), DbError> {
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
    let empty = BTreeSet::new();
    replace_source_artist_rows(
        sql,
        content_hash,
        source_discogs_artist_ids.unwrap_or(&empty),
    )?;
    let expected = required_discogs_artist_ids_for_write(sql, content_hash)?;
    let by_id: std::collections::HashMap<_, _> = assets
        .iter()
        .map(|asset| (asset.discogs_artist_id(), asset))
        .collect();
    if by_id.len() != assets.len() {
        return Err(DbError::Message(
            "candidate artist assets contain a duplicate Discogs artist ID".into(),
        ));
    }
    if let Some(missing) = expected.iter().find(|id| !by_id.contains_key(id.as_str())) {
        return Err(DbError::Message(format!(
            "candidate file edit has no prepared image answer for Discogs artist {missing}"
        )));
    }
    let retained = assets
        .iter()
        .filter(|asset| expected.contains(asset.discogs_artist_id()))
        .cloned()
        .collect::<Vec<_>>();
    replace_artist_asset_rows(sql, content_hash, &retained)
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

pub(super) fn replace_remote_cover_asset(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    cover: Option<&CoverSelection>,
    image: Option<&crate::import::cover_art::RemoteImage>,
) -> Result<(), DbError> {
    validate_remote_cover(cover, image)?;
    sql.execute(
        "DELETE FROM import_candidate_remote_cover_asset WHERE content_hash = ?",
        [content_hash],
    )?;
    if let Some(image) = image {
        sql.execute(
            "INSERT INTO import_candidate_remote_cover_asset \
                 (content_hash, content_type, bytes) VALUES (?, ?, ?)",
            params![content_hash, image.content_type.as_str(), image.bytes],
        )?;
    }
    Ok(())
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn invalidate_prepared_assets(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_asset_preparation WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
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

pub(super) fn load_prepared_assets_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
    cover: Option<&CoverSelection>,
) -> Result<CandidatePreparedAssets, DbError> {
    require_preparation_marker(sql, content_hash)?;
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
    let assets = CandidatePreparedAssets {
        remote_cover,
        artist_images,
    };
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
