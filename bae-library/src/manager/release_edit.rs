//! A library release's editor's reads and save.
//!
//! The editor holds its form ([`bae_import::ReleaseEditForm`]): the
//! release's values as the library held them when it opened, and what the
//! person did in it: the form, which each change replaces, a catalog the
//! person applied, which replaces every value the release holds (titles,
//! credits, mediums and places, roles and works, its records and the album
//! they name), and the library album the person picked to file it under.
//! Core keeps no form: opening reads one, applying a catalog, the files'
//! tags or the cleared values returns the form with those values, the
//! editor's draft is one live query whose request is the form, and saving
//! writes the form whole and returns the form opened on what it saved.
//! Nothing re-reads a catalog or the files' tags: the form holds what the
//! apply read.
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
        let values = match release.read(&audio) {
            Ok(values) => values,
            Err(refusal) => {
                return Err(LibraryError::from(
                    bae_import::ImportError::of_reading_refusal(&self.records, &release, refusal)
                        .await,
                ))
            }
        };
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

    /// `form` with its metadata cleared: no album title, artist or year, a
    /// blank pressing, and each track untitled, credited to the album's
    /// artists and with no listed length, role or work, on the mediums and
    /// at the places the library holds for the release. The records, the
    /// sides picked for CUE sheets and the album pick stay as they are, and
    /// nothing is written until a save.
    pub async fn clear_release_edit(
        &self,
        form: bae_import::ReleaseEditForm,
    ) -> Result<bae_import::ReleaseEditForm, LibraryError> {
        let cleared = self.import_tables.cleared_release_edit_values(&form).await?;
        Ok(form.applying_values(cleared))
    }

    /// Save `form`: in one write, the form as the person left it, every
    /// value the catalog the person applied gives (its mediums and places,
    /// roles and works), its records, and the album the release lands in:
    /// the one the person picked, else the one the applied records name. A
    /// form where the person applied no catalog writes the fields, and moves
    /// the release only to an album the person picked. A form that binds
    /// CUE sheets' `FILE` lines to other audio files reads those files
    /// first, and the write replaces the audio rows of the sheets' tracks
    /// (see [`bae_import::service::rebound_sheet_audio`]). A form that
    /// ignores a CUE sheet or binds a `FILE` line to no audio deletes the
    /// affected tracks and adds a track for each audio file that then has none (see
    /// [`bae_import::service::rebound_sheet_audio`]). When the library no
    /// longer holds a library artist or album the person picked, `gone` says
    /// what the save does. The same write reads the release as this save
    /// wrote it, and the method returns the form opened on that.
    ///
    /// Refused with [`LibraryError::ReleaseEditStale`], writing nothing, when
    /// the library no longer holds the release as the form opened on it. The
    /// write itself finds that out, and whether the library holds the picked
    /// artists and album: it reads the release, the artists and the album as
    /// they stand, so no write lands between the checks and the rows.
    pub async fn save_release_edit(
        &self,
        form: bae_import::ReleaseEditForm,
        gone: GonePick,
    ) -> Result<bae_import::ReleaseEditForm, LibraryError> {
        let release_id = form.release_id.clone();
        let shaped = form.edit.shape()?;
        shaped.validate()?;
        let opened = self.load_release_for_edit(&release_id).await?;
        let records = match &form.records {
            Some(records) => records.clone(),
            None => self.records.get_release_records(&release_id).await?,
        };
        let plan =
            self.release_edit_plan(&form, &form.album_pick, &shaped, &opened, &records).await?;
        let gone = match (gone, &form.album_pick) {
            (GonePick::Refuse, _) => bae_records::GonePicks::Refuse,
            (GonePick::Unpick, bae_import::AlbumPick::Album { .. }) => {
                bae_records::GonePicks::Unpick {
                    unpicked: Some(
                        self.release_edit_plan(
                            &form,
                            &bae_import::AlbumPick::Records,
                            &shaped,
                            &opened,
                            &records,
                        )
                        .await?,
                    ),
                }
            }
            (GonePick::Unpick, _) => bae_records::GonePicks::Unpick { unpicked: None },
        };
        let rebound = bae_import::service::rebound_sheet_audio(
            &self.import_tables,
            &self.records,
            &self.files,
            &self.diagnostics,
            &self.runtime_handle,
            self.clock.clone(),
            self.ids.clone(),
            &form,
        )
        .await?;
        let lays_out = !form.sheet_places.is_empty()
            || rebound.as_ref().is_some_and(|audio| !audio.dropped.is_empty());
        let saved_id = release_id.clone();
        let (opened_form, layout_form, decisions_form) = (form.clone(), form.clone(), form);
        let still_as_opened = move |sql: &coven::CovenWrite<'_>| {
            bae_import::tables::release_edit_still_as_opened_on(sql, &opened_form)
        };
        let layout = move |sql: &coven::CovenWrite<'_>| {
            lays_out
                .then(|| bae_import::tables::release_edit_layout_on(sql, &layout_form))
                .transpose()
        };
        let bindings = rebound.as_ref().map_or_else(Vec::new, |audio| audio.edits.picks.clone());
        let sheet_decisions = move |sql: &coven::CovenWrite<'_>| {
            bae_import::tables::store_release_edit_sheet_decisions_on(
                sql,
                &decisions_form,
                &bindings,
            )
        };
        let saved = move |sql: &coven::CovenWrite<'_>| {
            bae_import::tables::release_edit_form_on(sql, &saved_id)
        };
        self.records
            .save_release_edit_atomic(
                plan,
                gone,
                still_as_opened,
                layout,
                rebound,
                sheet_decisions,
                saved,
            )
            .await
            .map_err(|error| release_edit_save_error(error, &release_id))
    }

    /// What a save of `form` writes with the album pick `pick`: where the
    /// release goes, its rows, and whether the album keeps its own fields.
    async fn release_edit_plan(
        &self,
        form: &bae_import::ReleaseEditForm,
        pick: &bae_import::AlbumPick,
        shaped: &bae_import::ReleaseUserEdit,
        opened: &(String, DbRelease, DbAlbum, Vec<DbTrack>),
        records: &[bae_catalogs::records::ReleaseRecord],
    ) -> Result<bae_records::ReleaseEditPlan, LibraryError> {
        let release_id = form.release_id.as_str();
        let edit = &form.edit;
        let (current_album_id, release, current_album, existing_tracks) = opened;
        let change = match (pick, &form.records) {
            (bae_import::AlbumPick::Album { album_id }, _) => Some(bae_records::RecordsChange {
                release_id: release_id.to_string(),
                records: records.to_vec(),
                current_album_id: current_album_id.clone(),
                target_album_id: album_id.clone(),
                new_album: None,
            }),
            (bae_import::AlbumPick::New, _) => {
                let new_album = self.fresh_album_for_release(current_album_id).await?;
                Some(bae_records::RecordsChange {
                    release_id: release_id.to_string(),
                    records: records.to_vec(),
                    current_album_id: current_album_id.clone(),
                    target_album_id: new_album.id.clone(),
                    new_album: Some(new_album),
                })
            }
            // Unlinked, the release drops its records and stays in its
            // album.
            (bae_import::AlbumPick::Records, Some(records)) if records.is_empty() => {
                Some(bae_records::RecordsChange {
                    release_id: release_id.to_string(),
                    records: Vec::new(),
                    current_album_id: current_album_id.clone(),
                    target_album_id: current_album_id.clone(),
                    new_album: None,
                })
            }
            (bae_import::AlbumPick::Records, Some(records)) => {
                Some(self.records_change(release_id, records.clone()).await?)
            }
            (bae_import::AlbumPick::Records, None) => None,
        };
        let target_album_id = change
            .as_ref()
            .map_or(current_album_id.clone(), |change| change.target_album_id.clone());
        // A release the person moved into another album keeps that album's
        // own title and artists.
        let album_fields = if matches!(pick, bae_import::AlbumPick::Album { album_id } if album_id != current_album_id)
        {
            bae_records::AlbumFields::Kept
        } else {
            bae_records::AlbumFields::Written
        };
        let write = match &form.applied {
            Some(applied) => {
                let mut draft = applied.clone();
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
                // The write takes the album's id and the edit's title, year
                // and credits, and nothing else of the row, so the album the
                // release is in stands for the one it moves into; the write
                // checks that the library holds that one.
                let album_row = DbAlbum { id: target_album_id, ..current_album.clone() };
                bae_records::ReleaseEditWrite::Fields(self.release_metadata_rows(
                    album_row,
                    release.clone(),
                    existing_tracks,
                    &shaped.clone().normalized(),
                )?)
            }
        };
        Ok(bae_records::ReleaseEditPlan { change, write, album_fields })
    }
}

/// What a save does when the library no longer holds a library artist or
/// album the form picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GonePick {
    /// The pick stops being the pick, as the editor's draft shows it: the
    /// save writes a picked artist as a credit of the values the form holds
    /// for it, and files the release as with no album pick.
    Unpick,
    /// The save refuses, writing nothing, with [`LibraryError::AlbumMissing`]
    /// for the album and [`LibraryError::Identity`] for an artist, for a
    /// caller that named them itself.
    Refuse,
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
        R::AlbumGone { album_id } => LibraryError::AlbumMissing { album_id },
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
