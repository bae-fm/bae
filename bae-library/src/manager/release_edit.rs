//! A library release's edit sessions.
//!
//! Each open editor owns a session of its own ([`ReleaseEdit`]), whose rows
//! bae-import keeps (`bae_import::tables::ImportTables`): the release's
//! values as the library held them when it opened, and what the person did
//! in it: the form with each change written to it as the person makes it, a
//! catalog applied, which replaces every value the release holds (titles,
//! credits, mediums and places, roles and works, its records and the album
//! they name), and the library album picked to file it under. Every change
//! is one write to the session's rows, and its editor reads the session's
//! draft from one live query. Sessions on one release share nothing, so one editor's edit, apply or pick is never
//! another's. Saving writes the session whole and opens it again on what it
//! saved; dropping it deletes it. Nothing re-reads a catalog or the files'
//! tags: the session holds what the apply read.
//!
//! A save checks, in its one write, the release against what its session
//! opened on: the save refuses a release the library no longer holds as it
//! did (another session saved it, MCP wrote its fields, or a synced edit
//! arrived) with [`LibraryError::ReleaseEditStale`] and writes nothing, so
//! no session overwrites values its editor never showed. Cancel opens the session again
//! on the release as it is now.

use super::*;
use bae_import::tables::ReleaseEditSessionRefusal;

/// One open editor's session on a library release. Dropping it deletes the
/// session's rows.
#[derive(Debug)]
pub struct ReleaseEdit {
    release_id: String,
    /// The session's row.
    session: i64,
    tables: bae_import::tables::ImportTables,
    diagnostics: Diagnostics,
    runtime: tokio::runtime::Handle,
}

impl ReleaseEdit {
    /// The release this session edits.
    pub fn release_id(&self) -> &str {
        &self.release_id
    }
}

impl Drop for ReleaseEdit {
    fn drop(&mut self) {
        let (tables, diagnostics, session) =
            (self.tables.clone(), self.diagnostics.clone(), self.session);
        // No caller waits on the delete, so the panic callback has no waiter
        // to fail and does nothing; the panic hook reports the panic.
        let _task = self.diagnostics.spawn_reported(
            &self.runtime,
            "delete a closed release edit session",
            async move {
                if let Err(error) = tables.close_release_edit_session(session).await {
                    diagnostics.report_bug("delete a closed release edit session", &error);
                }
            },
            |_panic| {},
        );
    }
}

impl LibraryManager {
    /// Open an edit session on a library release: its values as the library
    /// holds them, with nothing edited, applied or picked. Every editor opens
    /// its own; a session on the same release elsewhere is untouched.
    pub async fn open_release_edit(&self, release_id: &str) -> Result<ReleaseEdit, LibraryError> {
        let tables = self.import_tables.clone();
        let session = tables
            .open_release_edit_session(release_id)
            .await
            .map_err(|error| release_edit_error(error, None))?;
        Ok(ReleaseEdit {
            release_id: release_id.to_string(),
            session,
            tables,
            diagnostics: self.diagnostics.clone(),
            runtime: self.runtime_handle.clone(),
        })
    }

    /// The session's draft as its editor shows it, live: it reads again
    /// after each write that changes what it read (each change of the
    /// session, an artist merged in the library, the album renamed, a cover
    /// picked), and reads `None` once the session's release is gone.
    pub fn subscribe_release_edit_draft(
        &self,
        session: &ReleaseEdit,
    ) -> bae_store::live_query::LiveValues<Option<bae_import::ReleaseEditDraft>> {
        self.values_until_closed(session.tables.subscribe_release_edit_draft(session.session))
    }

    /// Write one change of the form as `change` makes it.
    async fn change_release_edit(
        &self,
        session: &ReleaseEdit,
        change: impl FnOnce(&mut bae_import::RawReleaseEdit) -> Result<(), ReleaseEditSessionRefusal>
            + Send
            + 'static,
    ) -> Result<(), LibraryError> {
        session
            .tables
            .change_release_edit_session(session.session, change)
            .await
            .map_err(|error| release_edit_error(error, Some(session)))
    }

    /// Write one album-level field of the form as the person left it.
    pub async fn edit_release_edit_field(
        &self,
        session: &ReleaseEdit,
        edit: bae_import::DraftFieldEdit,
    ) -> Result<(), LibraryError> {
        self.change_release_edit(session, move |form| {
            form.set_field(edit);
            Ok(())
        })
        .await
    }

    /// Replace the form's ordered album credits.
    pub async fn set_release_edit_album_artists(
        &self,
        session: &ReleaseEdit,
        assignments: Vec<bae_import::Credited>,
    ) -> Result<(), LibraryError> {
        self.change_release_edit(session, move |form| {
            form.set_album_artists(assignments);
            Ok(())
        })
        .await
    }

    /// Write one track row's title and artists as the person left them, by
    /// the row's id. The method refuses a row the form does not hold.
    pub async fn set_release_edit_track(
        &self,
        session: &ReleaseEdit,
        track: bae_import::RawTrackEdit,
    ) -> Result<(), LibraryError> {
        self.change_release_edit(session, move |form| {
            let id = track.id.clone();
            if form.set_track(track) {
                Ok(())
            } else {
                Err(ReleaseEditSessionRefusal::RowMissing(id))
            }
        })
        .await
    }

    /// Apply the catalog release `link` picks to `session`: read it against
    /// the release's tracks' stored lengths and its files, as identification
    /// reads them, and write every value it gives and the records it names
    /// into the session until the save. The form becomes the reading's values
    /// and the draft lays the tracks out at the places it gives them; the
    /// cover is the release's own, which an apply never changes. The method
    /// refuses a release whose tracks the reading cannot lay out, naming both
    /// counts, and tells `steps` about each fetch it waits on.
    pub async fn apply_release_edit_catalog(
        &self,
        session: &ReleaseEdit,
        link: &bae_identify::PressingLink,
        steps: &bae_import::CatalogApplySteps,
    ) -> Result<(), LibraryError> {
        let release_id = session.release_id();
        // Nothing names these releases, so they are read and never stored.
        let release = self.release_reads.applied_release(&link.record, steps).await?.release;
        let partners: Vec<_> = self
            .release_reads
            .partners(
                &link.record,
                &link.partners,
                bae_catalogs::rate_limiter::CallClass::AWAITED,
                steps,
            )
            .await?
            .into_iter()
            .map(|partner| partner.release)
            .collect();
        let audio = self.release_read_audio(release_id).await?;
        let values = release
            .read(&audio)
            .map_err(|refusal| LibraryError::from(bae_import::ImportError::from(refusal)))?;
        if values.tracks.is_empty() {
            return Err(LibraryError::from(bae_import::ImportError::MetadataTrackCount {
                folder_tracks: audio.track_count(),
                release_tracks: 0,
            }));
        }
        let stored = session
            .tables
            .load_release_edit_session(session.session)
            .await?
            .ok_or(LibraryError::ReleaseEditClosed { release_id: release_id.to_string() })?;
        let track_ids: Vec<String> = stored.form.tracks.into_iter().map(|track| track.id).collect();
        let applied = bae_import::tables::AppliedCatalog {
            draft: bae_import::reading_draft::library_draft(values, track_ids),
            records: bae_import::service::records_for_commit(&release, &partners),
        };
        session
            .tables
            .apply_to_release_edit_session(session.session, applied)
            .await
            .map_err(|error| release_edit_error(error, Some(session)))
    }

    /// Record `pick` as the album `session` files its release under: a
    /// library album or a new one over the one its records name, or (nothing)
    /// the one its records name. The method refuses an album the library
    /// does not hold with `LibraryError::AlbumMissing`.
    pub async fn set_release_edit_album(
        &self,
        session: &ReleaseEdit,
        pick: bae_import::AlbumPick,
    ) -> Result<(), LibraryError> {
        session
            .tables
            .pick_release_edit_album(session.session, pick)
            .await
            .map_err(|error| release_edit_error(error, Some(session)))
    }

    /// Open `session` again on the release as the library holds it now,
    /// discarding everything it holds: what Cancel does.
    pub async fn reopen_release_edit(&self, session: &ReleaseEdit) -> Result<(), LibraryError> {
        session
            .tables
            .reseed_release_edit_session(session.session)
            .await
            .map_err(|error| release_edit_error(error, Some(session)))
    }

    /// Save `session`: in one write, the form as the person left it, every
    /// value a catalog applied in the session (its mediums and places, roles
    /// and works), its records, and the album the release lands in: the one
    /// the person picked, else the one the applied records name. A session
    /// with nothing applied writes the fields, and moves the release only to
    /// a picked album. The same write opens the session again on the release
    /// as saved.
    ///
    /// Refused with [`LibraryError::ReleaseEditStale`], writing nothing, when
    /// the library no longer holds the release as the session opened on it.
    /// The write itself finds that out: it reads the release as it stands and
    /// compares it with what the session opened on, so no write lands
    /// between the check and the rows.
    pub async fn save_release_edit(&self, session: &ReleaseEdit) -> Result<(), LibraryError> {
        let release_id = session.release_id();
        let closed = || LibraryError::ReleaseEditClosed { release_id: release_id.to_string() };
        let stored =
            session.tables.load_release_edit_session(session.session).await?.ok_or_else(closed)?;
        let bae_import::tables::StoredReleaseEdit { form: edit, applied, album_pick: pick, .. } =
            stored.clone();
        let shaped = edit.shape()?;
        shaped.validate()?;
        let (current_album_id, release, current_album, existing_tracks) =
            self.load_release_for_edit(release_id).await?;
        let records = match &applied {
            Some(applied) => applied.records.clone(),
            None => self.records.get_release_records(release_id).await?,
        };
        let change = match (&pick, &applied) {
            (bae_import::AlbumPick::Album { album_id }, _) => Some(bae_records::RecordsChange {
                release_id: release_id.to_string(),
                records: records.clone(),
                current_album_id: current_album_id.clone(),
                target_album_id: album_id.clone(),
                new_album: None,
            }),
            (bae_import::AlbumPick::New, _) => {
                let new_album = self.fresh_album_for_release(&current_album_id).await?;
                Some(bae_records::RecordsChange {
                    release_id: release_id.to_string(),
                    records: records.clone(),
                    current_album_id: current_album_id.clone(),
                    target_album_id: new_album.id.clone(),
                    new_album: Some(new_album),
                })
            }
            (bae_import::AlbumPick::Records, Some(applied)) => {
                Some(self.records_change(release_id, applied.records.clone()).await?)
            }
            (bae_import::AlbumPick::Records, None) => None,
        };
        let target_album_id = change
            .as_ref()
            .map_or(current_album_id.clone(), |change| change.target_album_id.clone());
        // A release the person moved into another album keeps that album's
        // own title and artists.
        let album_fields = if matches!(&pick, bae_import::AlbumPick::Album { album_id } if *album_id != current_album_id)
        {
            bae_records::AlbumFields::Kept
        } else {
            bae_records::AlbumFields::Written
        };
        let write = match applied {
            Some(applied) => {
                let mut draft = applied.draft;
                draft.album_title = edit.album_title.clone();
                draft.album_artist_assignments = edit.album_artist_assignments.clone();
                draft.album_year = edit.album_year.clone();
                // The editor sends each label's name and number alone; a
                // label keeping the name the catalog gave it keeps its ids.
                let labels = bae_import::RawLabelEdit::kept_ids(
                    edit.pressing.labels.clone(),
                    &draft.pressing.labels,
                );
                draft.pressing = bae_import::RawPressingEdit { labels, ..edit.pressing.clone() };
                for (track, edited) in draft.tracks.iter_mut().zip(&edit.tracks) {
                    track.edit.title = edited.title.clone();
                    track.edit.artist_assignments = edited.artist_assignments.clone();
                }
                let keys = bae_import::assemble::RowKeys {
                    album_id: target_album_id,
                    release_id: release_id.to_string(),
                    track_id: &|track: &bae_import::LibraryTrack| track.0.clone(),
                };
                let assembled = bae_import::assemble::assemble_rows(
                    &draft,
                    &keys,
                    self.clock.as_ref(),
                    self.ids.as_ref(),
                )?;
                bae_records::ReleaseEditWrite::Applied {
                    rows: assembled.parsed,
                    picked: assembled.picked.into_iter().collect(),
                }
            }
            None => {
                let album_row = if target_album_id == current_album_id {
                    current_album
                } else if let Some(new_album) =
                    change.as_ref().and_then(|change| change.new_album.clone())
                {
                    new_album
                } else {
                    self.records.find_album_by_id(&target_album_id).await?.ok_or_else(|| {
                        LibraryError::AlbumMissing { album_id: target_album_id.clone() }
                    })?
                };
                bae_records::ReleaseEditWrite::Fields(self.release_metadata_rows(
                    album_row,
                    release,
                    &existing_tracks,
                    &shaped.normalized(),
                )?)
            }
        };
        let id = session.session;
        let still_as_opened = move |sql: &coven::CovenWrite<'_>| {
            bae_import::tables::release_edit_still_as_opened_on(sql, id, &stored)
        };
        let reseed = move |sql: &coven::CovenWrite<'_>| {
            bae_import::tables::reseed_release_edit_session_on(sql, id)?.map_err(|refusal| {
                bae_store::DbError::unreadable(
                    format!("release edit session {id} after its save"),
                    refusal,
                )
            })
        };
        self.records
            .save_release_edit_atomic(change, write, album_fields, still_as_opened, reseed)
            .await
            .map_err(|error| release_edit_save_error(error, release_id))
    }
}

/// A session write's refusal as the library words it.
fn release_edit_error(
    error: bae_import::tables::ReleaseEditSessionError,
    session: Option<&ReleaseEdit>,
) -> LibraryError {
    use ReleaseEditSessionRefusal as R;
    LibraryError::of_write(error, &|refusal| match refusal {
        R::Closed(_) => LibraryError::ReleaseEditClosed {
            release_id: session.map(|session| session.release_id.clone()).unwrap_or_default(),
        },
        R::ReleaseMissing(release_id) => {
            LibraryError::release_missing(&release_id, super::ReleaseWork::Edit)
        }
        R::RowMissing(id) => LibraryError::EditRowMissing(id),
        R::AlbumMissing(album_id) => LibraryError::AlbumMissing { album_id },
        R::Unreadable(error) => LibraryError::from(error),
    })
}

/// A save's refusal as the library words it: a release changed since the
/// session opened is [`LibraryError::ReleaseEditStale`].
fn release_edit_save_error(
    error: bae_records::ReleaseEditSaveError,
    release_id: &str,
) -> LibraryError {
    use bae_records::ReleaseEditRefusal as R;
    LibraryError::of_write(error, &|refusal| match refusal {
        R::Stale => LibraryError::ReleaseEditStale { release_id: release_id.to_string() },
        R::Closed => LibraryError::ReleaseEditClosed { release_id: release_id.to_string() },
        R::Identity(refusal) => LibraryError::Identity(refusal),
    })
}

#[cfg(test)]
#[path = "release_edit_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "release_edit_album_tests.rs"]
mod album_tests;

#[cfg(test)]
#[path = "release_edit_session_tests.rs"]
mod session_tests;
