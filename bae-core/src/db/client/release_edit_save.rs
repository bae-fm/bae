//! Saving a library release's edit session: the edited fields, and every row
//! a catalog applied in the session makes, with the release's new records and
//! album, in one transaction.

use super::release::AlbumFields;
use super::*;
use crate::import::ReleaseRows;

/// What a save writes over the release's rows.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) enum ReleaseEditWrite {
    /// The editable fields only: album title, credits and year, the pressing,
    /// and each track's title and credits. Mediums, places, roles and works
    /// stay as stored.
    Fields(ReleaseMetadataRows),
    /// Every row a catalog applied in the session makes, keyed by the
    /// release's own ids, its tracks' and the album it lands in: it replaces
    /// the release's mediums and places, roles and works as well, so nothing
    /// a previous source stated survives.
    Applied {
        rows: ReleaseRows,
        /// The artist rows that are library artists a person picked.
        picked: Vec<String>,
    },
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl Database {
    /// Save an edit session: move the release and replace its records where
    /// `change` says (see [`super::records::set_records_on`]), then write
    /// `write`, in one transaction — all of it, or none.
    pub(crate) async fn save_release_edit_atomic(
        &self,
        change: Option<RecordsChange>,
        write: ReleaseEditWrite,
        album_fields: AlbumFields,
    ) -> Result<(), ArtistWriteError> {
        let now_dt = chrono::DateTime::<chrono::Utc>::from(self.inner.clock.now());
        self.write_resolving_artists(move |sql| {
            if let Some(change) = &change {
                super::records::set_records_on(&sql, change, now_dt)?;
            }
            let now = now_dt.to_rfc3339();
            match &write {
                ReleaseEditWrite::Fields(metadata) => {
                    write_release_metadata_on(&sql, metadata, album_fields, &now)
                }
                ReleaseEditWrite::Applied { rows, picked } => {
                    write_applied_rows_on(&sql, rows, picked, album_fields, &now)
                }
            }
        })
        .await
    }
}

/// Write `rows` over the release they name: its album's fields and credits,
/// its pressing, its mediums and each track's place, title and credits, and
/// its roles and works, every one the rows hold and no other.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn write_applied_rows_on(
    tx: &SqlContext<'_, '_>,
    rows: &ReleaseRows,
    picked: &[String],
    album_fields: AlbumFields,
    now: &str,
) -> Result<(), ArtistWriteError> {
    let resolved = ArtistCredits {
        credits: &rows.artists,
        picked,
    }
    .resolve_on(tx)?;
    resolved.write_on(tx)?;
    let artist_ids = resolved.id_map(&rows.artists);
    let album_artists = relink_album_artists(&rows.album_artists, &artist_ids)?;
    let track_artists = relink_track_artists(&rows.track_artists, &artist_ids)?;
    let release_roles = relink(
        &rows.release_artist_roles,
        &artist_ids,
        "release artist role",
        |role| &mut role.artist_id,
    )?;
    let track_roles = relink(
        &rows.track_artist_roles,
        &artist_ids,
        "track artist role",
        |role| &mut role.artist_id,
    )?;
    let work_artists = relink(
        &rows.work_graph.work_artists,
        &artist_ids,
        "work artist",
        |link| &mut link.artist_id,
    )?;

    let album = &rows.album;
    if album_fields == AlbumFields::Written {
        tx.execute(
            "UPDATE albums SET title = ?, year = ?, is_compilation = ? WHERE id = ?",
            params![album.title, album.year, album.is_compilation, album.id],
        )?;
        replace_album_artists(tx, &album.id, &album_artists, now)?;
    }
    super::release::update_release_pressing_on(tx, &rows.release)?;

    // Each medium row is keyed by its release and position, so one the
    // release keeps is rewritten in place; tracks move onto the new rows
    // before a medium no track sits on any more goes.
    let release_id = &rows.release.id;
    for medium in &rows.mediums {
        tx.execute(
            "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT (id) DO UPDATE SET medium = excluded.medium, \
                 side_count = excluded.side_count",
            params![
                medium.id,
                medium.release_id,
                medium.position,
                medium.medium.carrier.map(bae_types::pressing::Medium::key),
                medium.medium.sides,
                medium.created_at.to_rfc3339(),
            ],
        )?;
    }
    for track in &rows.tracks {
        let changed = tx.execute(
            "UPDATE tracks SET title = ?, medium_id = ?, side = ?, track_number = ? \
             WHERE id = ? AND release_id = ?",
            params![
                track.title,
                track.medium_id,
                track.side,
                track.track_number,
                track.id,
                release_id,
            ],
        )?;
        if changed != 1 {
            return Err(DbError::Message(format!(
                "release {release_id} holds no track {} to write",
                track.id
            ))
            .into());
        }
    }
    let kept: Vec<&str> = rows
        .mediums
        .iter()
        .map(|medium| medium.id.as_str())
        .collect();
    for stale in tx.query(
        "SELECT id FROM release_mediums WHERE release_id = ?",
        params![release_id],
        |row| row.get::<_, String>(0),
    )? {
        if !kept.contains(&stale.as_str()) {
            tx.execute("DELETE FROM release_mediums WHERE id = ?", params![stale])?;
        }
    }

    let track_ids: Vec<&str> = rows.tracks.iter().map(|track| track.id.as_str()).collect();
    replace_track_artists(tx, &track_ids, &track_artists, now)?;

    // What the previous source credited goes whole; the applied one's rows
    // take its place.
    for track_id in &track_ids {
        tx.execute(
            "DELETE FROM track_artist_roles WHERE track_id = ?",
            params![track_id],
        )?;
        tx.execute(
            "DELETE FROM track_works WHERE track_id = ?",
            params![track_id],
        )?;
    }
    tx.execute(
        "DELETE FROM release_artist_roles WHERE release_id = ?",
        params![release_id],
    )?;
    for work in &rows.work_graph.works {
        insert_work_row(tx, work)?;
    }
    for work_artist in &work_artists {
        insert_work_artist_row(tx, work_artist)?;
    }
    for work_part in &rows.work_graph.work_parts {
        insert_work_part_row(tx, work_part)?;
    }
    for role in &release_roles {
        insert_release_artist_role_row(tx, role)?;
    }
    for role in &track_roles {
        insert_track_artist_role_row(tx, role)?;
    }
    for track_work in &rows.work_graph.track_works {
        insert_track_work_row(tx, track_work)?;
    }
    Ok(())
}
