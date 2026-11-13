//! Saving a library release's edit session: the edited fields, and every row
//! a catalog applied in the session makes, with the release's new records and
//! album, in one transaction.

use super::release::AlbumFields;
use super::*;

/// What a save writes over the release's rows.
#[cfg(desktop)]
pub enum ReleaseEditWrite {
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

/// Why a save of an edit session did not commit. Nothing of it landed.
#[cfg(desktop)]
#[derive(Debug, thiserror::Error)]
pub enum ReleaseEditSaveError {
    /// A track the applied rows rewrite is no longer the release's: another
    /// device's write took it after the session was checked against the
    /// release.
    #[error("release {release_id} holds no track {track_id} to write")]
    TrackGone { release_id: String, track_id: String },
    #[error(transparent)]
    Write(#[from] IdentityWriteError),
}

/// A store failure is the save's write failing in the store.
#[cfg(desktop)]
impl From<DbError> for ReleaseEditSaveError {
    fn from(error: DbError) -> Self {
        Self::Write(error.into())
    }
}

/// The first track of `rows` the release no longer holds, if any.
#[cfg(desktop)]
fn track_gone_on(tx: &CovenWrite<'_>, rows: &ReleaseRows) -> Result<Option<String>, DbError> {
    let held: HashSet<String> = tx
        .query("SELECT id FROM tracks WHERE release_id = ?", params![rows.release.id], |row| {
            row.get(0)
        })?
        .into_iter()
        .collect();
    Ok(rows.tracks.iter().find(|track| !held.contains(&track.id)).map(|track| track.id.clone()))
}

#[cfg(desktop)]
impl Records {
    /// Save an edit session: move the release and replace its records where
    /// `change` says (see `set_records_on`), then write
    /// `write`, in one transaction — all of it, or none. Applied rows
    /// naming a track the release no longer holds are refused before
    /// anything is written.
    pub async fn save_release_edit_atomic(
        &self,
        change: Option<RecordsChange>,
        write: ReleaseEditWrite,
        album_fields: AlbumFields,
    ) -> Result<(), ReleaseEditSaveError> {
        let now_dt = self.store.now_utc();
        let labels = LabelParts::new(
            &*self.ids,
            self.store.now_utc(),
            match &write {
                ReleaseEditWrite::Fields(metadata) => &metadata.release.pressing.labels,
                ReleaseEditWrite::Applied { rows, .. } => &rows.release.pressing.labels,
            },
        );
        let release_id = match &write {
            ReleaseEditWrite::Fields(metadata) => metadata.release.id.clone(),
            ReleaseEditWrite::Applied { rows, .. } => rows.release.id.clone(),
        };
        let release_id_written = release_id.clone();
        let album_id_written = match &write {
            ReleaseEditWrite::Fields(metadata) => metadata.album.id.clone(),
            ReleaseEditWrite::Applied { rows, .. } => rows.album.id.clone(),
        };
        let gone = artist_resolution::write_resolving_identities(&self.store, move |sql| {
            let emptiable = Emptiable::of_releases(&sql, &[release_id_written.as_str()])?
                .including_album(&sql, &album_id_written)?;
            if let ReleaseEditWrite::Applied { rows, .. } = &write {
                if let Some(track_id) = track_gone_on(&sql, rows)? {
                    return Ok(Some(track_id));
                }
            }
            if let Some(change) = &change {
                super::release_records::set_records_on(&sql, change, now_dt)?;
            }
            let now = now_dt.to_rfc3339();
            match &write {
                ReleaseEditWrite::Fields(metadata) => write_release_metadata_on(
                    &sql,
                    metadata,
                    album_fields,
                    &now,
                    &label_write(&labels),
                ),
                ReleaseEditWrite::Applied { rows, picked } => write_applied_rows_on(
                    &sql,
                    rows,
                    picked,
                    album_fields,
                    &now,
                    &label_write(&labels),
                ),
            }?;
            emptiable.remove_emptied(&sql)?;
            Ok(None)
        })
        .await?;
        match gone {
            Some(track_id) => Err(ReleaseEditSaveError::TrackGone { release_id, track_id }),
            None => Ok(()),
        }
    }
}

/// Write `rows` over the release they name: its album's fields and credits,
/// its pressing, its mediums and each track's place, title and credits, and
/// its roles and works, every one the rows hold and no other.
#[cfg(desktop)]
fn write_applied_rows_on(
    tx: &CovenWrite<'_>,
    rows: &ReleaseRows,
    picked: &[String],
    album_fields: AlbumFields,
    now: &str,
    labels: &super::LabelWrite<'_>,
) -> Result<(), IdentityWriteError> {
    let resolved = ArtistCredits { credits: &rows.artists, picked }.resolve_on(tx)?;
    resolved.write_on(tx)?;
    let artist_ids = resolved.id_map(&rows.artists);
    let album_artists = relink_album_artists(&rows.album_artists, &artist_ids);
    let track_artists = relink_track_artists(&rows.track_artists, &artist_ids);
    let release_roles =
        relink(&rows.release_artist_roles, &artist_ids, "release artist role", |role| {
            &mut role.artist_id
        });
    let track_roles = relink(&rows.track_artist_roles, &artist_ids, "track artist role", |role| {
        &mut role.artist_id
    });
    let work_artists = relink(&rows.work_graph.work_artists, &artist_ids, "work artist", |link| {
        &mut link.artist_id
    });

    let album = &rows.album;
    if album_fields == AlbumFields::Written {
        tx.execute(
            "UPDATE albums SET title = ?, year = ?, is_compilation = ? WHERE id = ?",
            params![album.title, album.year, album.is_compilation, album.id],
        )?;
        replace_album_artists(tx, &album.id, &album_artists, now)?;
    }
    super::release::update_release_pressing_on(tx, &rows.release, labels)?;

    // Each medium row is keyed by its release and position, so one the
    // release keeps is rewritten in place; tracks move onto the new rows
    // before a medium no track sits on any more goes.
    let release_id = &rows.release.id;
    for medium in &rows.mediums {
        tx.execute(
            "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT (id, release_id) DO UPDATE SET medium = excluded.medium, \
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
    // The save checked, in this write, that the release holds every track
    // these rows name.
    for track in &rows.tracks {
        tx.execute(
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
    }
    let kept: Vec<&str> = rows.mediums.iter().map(|medium| medium.id.as_str()).collect();
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
        tx.execute("DELETE FROM track_artist_roles WHERE track_id = ?", params![track_id])?;
        tx.execute("DELETE FROM track_works WHERE track_id = ?", params![track_id])?;
    }
    tx.execute("DELETE FROM release_artist_roles WHERE release_id = ?", params![release_id])?;
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

#[cfg(all(test, desktop))]
#[path = "release_edit_save_tests.rs"]
mod tests;
