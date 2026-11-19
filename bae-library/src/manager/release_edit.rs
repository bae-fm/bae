//! A library release's editor's reads and save.
//!
//! The editor holds its form ([`bae_import::ReleaseEditForm`]): the
//! release's values as the library held them when it opened, and what the
//! person did in it: the form, which each change replaces, a catalog the
//! person applied, which replaces every value the release holds (titles,
//! credits, mediums and places, roles and works, its records and the album
//! they name), and the library album the person picked to file it under.
//! Core keeps no form: opening reads one, applying a catalog returns the form
//! with the catalog's values, the editor's draft is one live query whose
//! request is the form, and saving writes the form whole and returns the
//! form opened on what it saved. Nothing re-reads a catalog or the files'
//! tags: the form holds what the apply read.
//!
//! A save checks, in its one write, the release against what its form
//! opened on: the save refuses a release the library no longer holds as it
//! did (another editor saved it, MCP wrote its fields, or a synced edit
//! arrived) with [`LibraryError::ReleaseEditStale`] and writes nothing, so no
//! save overwrites values its editor never showed. Cancel opens the editor
//! again on the release as it is now.

use super::*;

/// The live read of a release editor's draft: the request is the form the
/// editor holds, and each value is the draft of the form last set, or
/// `None` once the form's release is gone. With no form set it delivers
/// nothing.
pub type LiveReleaseEditDraft = bae_store::live_query::LiveRead<
    Option<bae_import::ReleaseEditForm>,
    Option<bae_import::ReleaseEditDraft>,
>;

impl LibraryManager {
    /// The form an editor opens with on a library release: its values as
    /// the library holds them, with nothing edited, applied or picked.
    pub async fn open_release_edit(
        &self,
        release_id: &str,
    ) -> Result<bae_import::ReleaseEditForm, LibraryError> {
        use bae_import::tables::ReleaseEditSeedRefusal as R;
        self.import_tables.read_release_edit_form(release_id).await?.map_err(
            |refusal| match refusal {
                R::ReleaseMissing(release_id) => {
                    LibraryError::release_missing(&release_id, super::ReleaseWork::Edit)
                }
                R::Unreadable(error) => LibraryError::from(error),
            },
        )
    }

    /// The live read of a release editor's draft, which reads nothing until
    /// the editor sets its form; see [`LiveReleaseEditDraft`]. It reads again
    /// after each write that changes what it read: an artist merged in the
    /// library, the album renamed, a cover picked, the release deleted.
    pub fn subscribe_release_edit_draft(&self) -> LiveReleaseEditDraft {
        bae_store::live_query::LiveRead::new(
            self.until_closed(self.import_tables.subscribe_release_edit_draft()),
            |_read, draft| draft,
        )
    }

    /// `form` with the catalog release `link` picks applied: read it against
    /// the release's tracks' stored lengths and its files, as identification
    /// reads them, and return the form holding every value it gives and the
    /// records it names until the save. The form's values become the
    /// reading's, and its draft lays the tracks out at the places it gives
    /// them; the cover is the release's own, which an apply never changes.
    /// The method refuses a release whose tracks the reading cannot lay out,
    /// naming both counts, and tells `steps` about each fetch it waits on.
    pub async fn apply_release_edit_catalog(
        &self,
        form: bae_import::ReleaseEditForm,
        link: &bae_identify::PressingLink,
        steps: &bae_import::CatalogApplySteps,
    ) -> Result<bae_import::ReleaseEditForm, LibraryError> {
        let release_id = form.release_id.as_str();
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
        let track_ids: Vec<String> =
            form.edit.tracks.iter().map(|track| track.id.clone()).collect();
        let applied = bae_import::AppliedCatalog {
            draft: bae_import::reading_draft::library_draft(values, track_ids),
            records: bae_import::service::records_for_commit(&release, &partners),
        };
        Ok(form.applying(applied))
    }

    /// `form` with the library album `album_id` picked to file the release
    /// under. The method refuses an album the library does not hold, for a
    /// caller that names the album itself.
    pub async fn pick_release_edit_album(
        &self,
        form: bae_import::ReleaseEditForm,
        album_id: String,
    ) -> Result<bae_import::ReleaseEditForm, LibraryError> {
        if self.records.find_album_by_id(&album_id).await?.is_none() {
            return Err(LibraryError::AlbumMissing { album_id });
        }
        Ok(bae_import::ReleaseEditForm {
            album_pick: bae_import::AlbumPick::Album { album_id },
            ..form
        })
    }

    /// Save `form`: in one write, the form as the person left it, every
    /// value the catalog the person applied gives (its mediums and places,
    /// roles and works), its records, and the album the release lands in:
    /// the one the person picked, else the one the applied records name. A
    /// form where the person applied no catalog writes the fields, and moves
    /// the release only to an album the person picked. A library album the
    /// person picked that the library no longer holds is no pick, as the
    /// editor's draft shows it. The same write reads the release as this save
    /// wrote it, and the method returns the form opened on that.
    ///
    /// Refused with [`LibraryError::ReleaseEditStale`], writing nothing, when
    /// the library no longer holds the release as the form opened on it.
    /// The write itself finds that out: it reads the release as it stands and
    /// compares it with what the form opened on, so no write lands between
    /// the check and the rows.
    pub async fn save_release_edit(
        &self,
        form: bae_import::ReleaseEditForm,
    ) -> Result<bae_import::ReleaseEditForm, LibraryError> {
        let release_id = form.release_id.clone();
        let release_id = release_id.as_str();
        let edit = form.edit.clone();
        let applied = form.applied.clone();
        let pick = match form.album_pick.clone() {
            bae_import::AlbumPick::Album { album_id }
                if self.records.find_album_by_id(&album_id).await?.is_none() =>
            {
                bae_import::AlbumPick::Records
            }
            pick => pick,
        };
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
        let saved_id = release_id.to_string();
        let still_as_opened = move |sql: &coven::CovenWrite<'_>| {
            bae_import::tables::release_edit_still_as_opened_on(sql, &form)
        };
        let saved = move |sql: &coven::CovenWrite<'_>| {
            bae_import::tables::release_edit_form_on(sql, &saved_id)
        };
        self.records
            .save_release_edit_atomic(change, write, album_fields, still_as_opened, saved)
            .await
            .map_err(|error| release_edit_save_error(error, release_id))
    }
}

/// A save's refusal as the library words it: a release changed since the
/// editor opened is [`LibraryError::ReleaseEditStale`].
fn release_edit_save_error(
    error: bae_records::ReleaseEditSaveError,
    release_id: &str,
) -> LibraryError {
    use bae_records::ReleaseEditRefusal as R;
    LibraryError::of_write(error, &|refusal| match refusal {
        R::Stale => LibraryError::ReleaseEditStale { release_id: release_id.to_string() },
        R::Gone => LibraryError::release_missing(release_id, super::ReleaseWork::Edit),
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
#[path = "release_edit_draft_tests.rs"]
mod draft_tests;
