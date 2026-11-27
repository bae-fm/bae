//! Saving a library release's editor's form: the edited fields, and every
//! row a catalog applied in the editor makes, with the release's new records
//! and album, in one transaction.

use super::release::AlbumFields;
use super::*;
use bae_store::WriteStatements as _;
use bae_util::timestamp::StoredTimestamp as _;

/// What a save writes over the release's rows.
#[cfg(desktop)]
pub enum ReleaseEditWrite {
    /// The editable fields only: album title, credits and year, the pressing,
    /// and each track's title and credits. Mediums, places, listed lengths,
    /// roles and works stay as stored.
    Fields(ReleaseMetadataRows),
    /// Every row a catalog applied in the editor makes, keyed by the
    /// release's own ids, its tracks' and the album it lands in: it replaces
    /// the release's mediums and places, its tracks' listed lengths, roles and
    /// works as well, so nothing a previous source stated survives.
    Applied {
        rows: ReleaseRows,
        /// The artist rows that are library artists a person picked.
        picked: Vec<String>,
    },
}

/// What a save does with a pick of its form that the library no longer
/// holds.
#[cfg(desktop)]
pub enum GonePicks {
    /// The pick stops being the pick, as the release editor shows it: a
    /// picked artist saves as a credit of the values the form holds for it,
    /// and a save that moves the release into a picked album writes
    /// `unpicked`, the plan with no album pick, instead (`None` where the
    /// form picked no album, so a move into an album the library no longer
    /// holds is refused as [`ReleaseEditRefusal::AlbumGone`]).
    Unpick { unpicked: Option<ReleaseEditPlan> },
    /// The save refuses, writing nothing: as
    /// [`IdentityRefusal::ArtistGone`] for a picked artist and as
    /// [`ReleaseEditRefusal::AlbumGone`] for the album, for a caller that
    /// named them itself.
    Refuse,
}

/// What a save writes: where the release goes and its records (see
/// `set_records_on`), the rows it writes over the release's, and whether
/// the album's own fields are written.
#[cfg(desktop)]
pub struct ReleaseEditPlan {
    pub change: Option<RecordsChange>,
    pub write: ReleaseEditWrite,
    pub album_fields: AlbumFields,
}

#[cfg(desktop)]
impl ReleaseEditPlan {
    /// The library album the plan moves the release into, when it moves it
    /// into one the library holds already rather than a new one.
    fn existing_target(&self) -> Option<&str> {
        self.change
            .as_ref()
            .filter(|change| change.new_album.is_none())
            .map(|change| change.target_album_id.as_str())
    }

    fn pressing(&self) -> &bae_types::pressing::Pressing {
        match &self.write {
            ReleaseEditWrite::Fields(metadata) => &metadata.release.pressing,
            ReleaseEditWrite::Applied { rows, .. } => &rows.release.pressing,
        }
    }

    fn release_id(&self) -> &str {
        match &self.write {
            ReleaseEditWrite::Fields(metadata) => &metadata.release.id,
            ReleaseEditWrite::Applied { rows, .. } => &rows.release.id,
        }
    }

    fn album_id(&self) -> &str {
        match &self.write {
            ReleaseEditWrite::Fields(metadata) => &metadata.album.id,
            ReleaseEditWrite::Applied { rows, .. } => &rows.album.id,
        }
    }
}

/// Why an edit of a release, its editor's save or a metadata edit, was
/// refused. Nothing of it landed.
#[cfg(desktop)]
#[derive(Debug, thiserror::Error)]
pub enum ReleaseEditRefusal {
    /// The release is no longer as the edit read it, as the edit's own write
    /// found it.
    #[error("the release changed after it was read")]
    Stale,
    /// The library no longer holds the release.
    #[error("the release is gone")]
    Gone,
    /// The library no longer holds the library album the save moves the
    /// release into, and the caller gave no plan for that.
    #[error("album {album_id} is gone")]
    AlbumGone { album_id: String },
    #[error(transparent)]
    Identity(#[from] IdentityRefusal),
}

/// Why an edit of a release did not commit.
#[cfg(desktop)]
pub type ReleaseEditSaveError = WriteError<ReleaseEditRefusal>;

#[cfg(desktop)]
impl Records {
    /// Write the rows a metadata edit leaves a release, which it made from
    /// `read`, in one transaction (see `write_release_metadata_on`), keeping
    /// what the release's pressing is as the library holds it. The write
    /// reads the release again first, and refuses one that is not as `read`
    /// holds it as [`ReleaseEditRefusal::Stale`], and one the library no
    /// longer holds as [`ReleaseEditRefusal::Gone`].
    pub async fn update_release_metadata_user_edit(
        &self,
        metadata: ReleaseMetadataRows,
        read: super::ReleaseToEdit,
    ) -> Result<(), ReleaseEditSaveError> {
        let pressing =
            PressingParts::new(&*self.ids, self.store.now_utc(), &metadata.release.pressing);
        let now = self.store.now_utc().to_rfc3339();
        self.store
            .write(move |sql| {
                match super::release_to_edit_on(&sql, &read.release.id)? {
                    None => return Err(WriteError::Refused(ReleaseEditRefusal::Gone)),
                    Some(held) if held != read => {
                        return Err(WriteError::Refused(ReleaseEditRefusal::Stale))
                    }
                    Some(_) => {}
                }
                let emptiable = Emptiable::of_releases(&sql, &[metadata.release.id.as_str()])?;
                write_release_metadata_on(
                    &sql,
                    &metadata,
                    AlbumFields::Written,
                    super::release::PressingFactsWrite::Kept,
                    &now,
                    &pressing_write(&pressing),
                    GonePicked::Refuse,
                )
                .map_err(|error| error.map_refusal(ReleaseEditRefusal::Identity))?;
                Ok(emptiable.remove_emptied(&sql)?)
            })
            .await
    }

    /// Save an editor's form: in one transaction, all of it or none, first
    /// check, as `inputs_unchanged` says, that the library still holds what
    /// the save read before it, then write `plan`: move the release and
    /// replace its records where its change says (see `set_records_on`), then
    /// write its rows, then the audio of the tracks `rebound` moves to other
    /// files and the tracks it deletes and adds, then the layout of the
    /// tracks `layout` read before that, when it read one, then what
    /// `sheet_decisions` writes of the person's choices about CUE sheets,
    /// given the picked sides that layout gave its sheets.
    /// `gone` says what the save does with a picked artist or
    /// album the library no longer holds. Last it reads what `saved` reads
    /// of the release as this save wrote it, and returns that.
    /// Before it writes anything, the save refuses a release changed since
    /// the save read it as [`ReleaseEditRefusal::Stale`], and one the
    /// library no longer holds as [`ReleaseEditRefusal::Gone`]; the checks run
    /// in the write, so no other write lands between them and the rows, and
    /// none lands between the rows and what `saved` reads.
    pub async fn save_release_edit_atomic<Saved: Send + 'static>(
        &self,
        plan: ReleaseEditPlan,
        gone: GonePicks,
        inputs_unchanged: impl for<'ctx> FnOnce(&CovenWrite<'ctx>) -> Result<Result<(), ReleaseEditRefusal>, DbError>
            + Send
            + 'static,
        layout: impl for<'ctx> FnOnce(
                &CovenWrite<'ctx>,
            ) -> Result<Option<super::layout::LaidRelease>, DbError>
            + Send
            + 'static,
        rebound: Option<super::sheet_files::ReboundAudio>,
        sheet_decisions: impl for<'ctx> FnOnce(
                &CovenWrite<'ctx>,
                &[(String, bae_types::mediums::MediumSide)],
            ) -> Result<(), DbError>
            + Send
            + 'static,
        saved: impl for<'ctx> FnOnce(&CovenWrite<'ctx>) -> Result<Saved, DbError> + Send + 'static,
    ) -> Result<Saved, ReleaseEditSaveError> {
        let now_dt = self.store.now_utc();
        let ids = self.ids.clone();
        self.store
            .write(move |sql| {
                inputs_unchanged(&sql)?.map_err(WriteError::Refused)?;
                let (unpicked, gone_artists) = match gone {
                    GonePicks::Unpick { unpicked } => (unpicked, GonePicked::AsCredit),
                    GonePicks::Refuse => (None, GonePicked::Refuse),
                };
                let plan = match plan.existing_target() {
                    Some(album_id) if !super::release_records::album_held_on(&sql, album_id)? => {
                        unpicked.ok_or_else(|| {
                            WriteError::Refused(ReleaseEditRefusal::AlbumGone {
                                album_id: album_id.to_string(),
                            })
                        })?
                    }
                    _ => plan,
                };
                let pressing = PressingParts::new(&*ids, now_dt, plan.pressing());
                let emptiable = Emptiable::of_releases(&sql, &[plan.release_id()])?
                    .including_album(&sql, plan.album_id())?;
                if let Some(change) = &plan.change {
                    super::release_records::set_records_on(&sql, change, now_dt, &*ids)?;
                }
                let now = now_dt.to_rfc3339();
                match &plan.write {
                    ReleaseEditWrite::Fields(metadata) => write_release_metadata_on(
                        &sql,
                        metadata,
                        plan.album_fields,
                        super::release::PressingFactsWrite::Written,
                        &now,
                        &pressing_write(&pressing),
                        gone_artists,
                    ),
                    ReleaseEditWrite::Applied { rows, picked } => write_applied_rows_on(
                        &sql,
                        rows,
                        picked,
                        plan.album_fields,
                        &now,
                        &pressing_write(&pressing),
                        gone_artists,
                    ),
                }
                .map_err(|error| error.map_refusal(ReleaseEditRefusal::Identity))?;
                // The layout is read before the rebound audio deletes and adds
                // tracks, since it reads the tracks the release holds now.
                let (mut layout, honored) = match layout(&sql)? {
                    Some(laid) => (Some(laid.layout), laid.honored),
                    None => (None, Vec::new()),
                };
                if let Some(rebound) = &rebound {
                    super::sheet_files::write_rebound_audio_on(&sql, plan.release_id(), rebound)?;
                    for track in layout.iter_mut().flat_map(|layout| &mut layout.tracks) {
                        if let Some(added) =
                            rebound.added.iter().find(|added| added.file_id == track.track_id)
                        {
                            track.track_id.clone_from(&added.track.id);
                        }
                    }
                }
                if let Some(layout) = layout {
                    super::layout::write_layout_on(&sql, plan.release_id(), &layout, &*ids, &now)?;
                }
                sheet_decisions(&sql, &honored)?;
                emptiable.remove_emptied(&sql)?;
                Ok(saved(&sql)?)
            })
            .await
    }
}

#[cfg(desktop)]
sqlive_compose::write!(
    SetTrackRow(
        id: &'p str,
        release_id: &'p str,
        title: &'p str,
        medium_id: &'p str,
        side: std::num::NonZeroU8,
        track_number: std::num::NonZeroU32,
        stated_length_ms: Option<u64>,
    ),
    r#"
    UPDATE tracks
    SET title = :title, medium_id = :medium_id, side = :side,
        track_number = :track_number, stated_length_ms = :stated_length_ms
    WHERE id = :id AND release_id = :release_id
"#
);

#[cfg(desktop)]
sqlive_compose::write!(
    ClearTrackArtistRoles(track_id: &'p str),
    "DELETE FROM track_artist_roles WHERE track_id = :track_id"
);

#[cfg(desktop)]
sqlive_compose::write!(
    ClearTrackWorks(track_id: &'p str),
    "DELETE FROM track_works WHERE track_id = :track_id"
);

#[cfg(desktop)]
sqlive_compose::write!(
    ClearReleaseArtistRoles(release_id: &'p str),
    "DELETE FROM release_artist_roles WHERE release_id = :release_id"
);

/// Write `rows` over the release they name: its album's fields and credits,
/// its pressing, its mediums and each track's place, title, listed length and
/// credits, and its roles and works, every one the rows hold and no other, a
/// picked artist the library no longer holds as `gone` says.
#[cfg(desktop)]
fn write_applied_rows_on(
    tx: &CovenWrite<'_>,
    rows: &ReleaseRows,
    picked: &[String],
    album_fields: AlbumFields,
    now: &str,
    pressing: &super::PressingWrite<'_>,
    gone: GonePicked,
) -> Result<(), IdentityWriteError> {
    let resolved = ArtistCredits { credits: &rows.artists, picked }.resolve_on(tx, gone)?;
    resolved.write_on(tx)?;
    let (album_artists, track_artists) =
        resolved.relinked_credit_links(&rows.artists, &rows.album_artists, &rows.track_artists);
    let artist_ids = resolved.id_map(&rows.artists);
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
        super::release::update_album_fields_on(tx, album)?;
        replace_album_artists(tx, &album.id, &album_artists, now)?;
    }
    super::release::update_release_pressing_on(tx, &rows.release, pressing)?;

    // A medium the release holds at a position keeps its row, rewritten in
    // place, and a position it did not hold gets the new row; tracks move
    // onto the kept rows before a medium no track sits on any more goes.
    let release_id = &rows.release.id;
    let held: std::collections::HashMap<u16, String> = tx
        .query(
            "SELECT position, id FROM release_mediums WHERE release_id = ?",
            params![release_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .into_iter()
        .collect();
    let mut kept: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    for medium in &rows.mediums {
        let id = match held.get(&medium.position.get()) {
            Some(id) => {
                update_release_medium_row(tx, id, &medium.medium)?;
                id.clone()
            }
            None => {
                insert_release_medium_row(tx, medium)?;
                medium.id.clone()
            }
        };
        kept.insert(medium.id.as_str(), id);
    }
    // The save checked, in this write, that the release is as the editor
    // opened on it, so it holds every track these rows name.
    for track in &rows.tracks {
        let medium_id = kept.get(track.medium_id.as_str()).unwrap_or(&track.medium_id);
        tx.run(&SetTrackRowParams {
            id: &track.id,
            release_id,
            title: &track.title,
            medium_id,
            side: track.side,
            track_number: track.track_number,
            stated_length_ms: track.stated_length_ms,
        })?;
    }
    for stale in held.values() {
        if !kept.values().any(|id| id == stale) {
            delete_release_medium_row(tx, stale)?;
        }
    }

    let track_ids: Vec<&str> = rows.tracks.iter().map(|track| track.id.as_str()).collect();
    replace_track_artists(tx, &track_ids, &track_artists, now)?;

    // What the previous source credited goes whole; the applied one's rows
    // take its place.
    for track_id in &track_ids {
        tx.run(&ClearTrackArtistRolesParams { track_id })?;
        tx.run(&ClearTrackWorksParams { track_id })?;
    }
    tx.run(&ClearReleaseArtistRolesParams { release_id })?;
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
