//! A library release's edit session.
//!
//! The editor opens a release's values; a catalog applied in the session
//! replaces every value the release holds — titles, credits, mediums and
//! places, roles, works, its records and the album they name — and saving
//! writes the session whole, or discarding drops it. Nothing re-reads a
//! catalog or the files' tags: the session holds what the apply read.

use super::release::cover_ref_for;
use super::*;

/// Each open session's catalog apply, by release id: what its save writes
/// beyond the edited fields. A session with none writes the fields alone.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Default, Clone)]
pub(super) struct ReleaseEditSessions(
    std::sync::Arc<std::sync::Mutex<HashMap<String, AppliedCatalog>>>,
);

/// What applying a catalog release in a session read: every value, laid onto
/// the release's tracks in order, and the records the pick names, the
/// applied one first.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Clone)]
struct AppliedCatalog {
    draft: crate::import::LibraryDraft,
    records: Vec<crate::import::ReleaseRecord>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ReleaseEditSessions {
    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<String, AppliedCatalog>> {
        self.0
            .lock()
            .expect("the release edit sessions' lock is never held across a panic")
    }
}

impl LibraryManager {
    /// Open an edit session on a library release: its values as the library
    /// holds them, with no catalog applied. Opening again starts over.
    ///
    /// The read counterpart to `apply_release_metadata_user_edit`.
    /// Reads the album title and artists, the release pressing fields, and the
    /// per-track titles/sides/numbers/artists, projects them into a wire
    /// `ReleaseUserEdit` describing the current state, then renders that into
    /// the raw editor form via `RawReleaseEdit::from_user_edit`. A track with
    /// no artist rows of its own seeds an empty artist field ("shares the album
    /// artist"); the album artists seed the album artist field.
    pub async fn open_release_edit(
        &self,
        release_id: &str,
    ) -> Result<crate::import::ReleaseEditSeed, LibraryError> {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        self.release_edits.sessions().remove(release_id);
        let context = self
            .database
            .find_release_detail_context(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;
        let release = &context.detail.release;
        let album = self
            .database
            .find_album_by_id(&release.album_id)
            .await?
            .ok_or_else(|| {
                LibraryError::Import(format!("Album '{}' not found", release.album_id))
            })?;
        let display = crate::album_detail::ReleaseEditDisplayContext::from_raw(&context.detail)?;
        let cover = cover_ref_for(&self.database, release_id).await?;

        let album_artist_assignments: Vec<crate::import::Credited> = context
            .album_artists
            .into_iter()
            .map(crate::import::Credited::of_library)
            .collect();

        let mut tracks = Vec::with_capacity(context.detail.tracks.len());
        for entry in &context.detail.tracks {
            // Empty when the track has no artist rows of its own — the wire edit
            // reads that as "shares the album artist", matching how
            // `apply_release_metadata_user_edit` writes it back.
            let artists: Vec<crate::import::Credited> = entry
                .artists
                .clone()
                .into_iter()
                .map(crate::import::Credited::of_library)
                .collect();
            tracks.push((
                entry.track.id.clone(),
                crate::import::TrackUserEdit {
                    title: entry.track.title.clone(),
                    artist_assignments: if artists.is_empty() {
                        crate::import::TrackArtistAssignments::AlbumArtists
                    } else {
                        crate::import::TrackArtistAssignments::Explicit(artists)
                    },
                    // Re-projecting a release's metadata never re-binds its files;
                    // the audio each track already points at stays as it is.
                    file: None,
                },
            ));
        }

        let edit = raw_release_edit_with_persisted_track_ids(
            crate::import::ReleaseUserEdit {
                album_title: album.title,
                album_artist_assignments,
                album_year: album.year,
                pressing: release.pressing.clone(),
                tracks: tracks.iter().map(|(_, track)| track.clone()).collect(),
            },
            tracks.iter().map(|(id, _)| id.as_str()),
        )?;

        Ok(crate::import::ReleaseEditSeed {
            edit,
            cover,
            display,
        })
    }

    /// Apply the catalog release `link` picks to the session on
    /// `release_id`: read it against the release's tracks' stored lengths and
    /// its files, as identification reads them, and hold every value it
    /// gives and the records it names until the save. The form comes back
    /// filled with the reading's values; the cover is the release's own, which
    /// an apply never changes. A release whose tracks the reading cannot lay
    /// out is refused with both counts.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn apply_release_edit_catalog(
        &self,
        release_id: &str,
        link: &crate::import::PressingLink,
    ) -> Result<crate::import::ReleaseEditSeed, LibraryError> {
        let release = crate::import::service::prepare_release(
            self,
            &link.record,
            crate::util::rate_limiter::CallPriority::Interactive,
        )
        .await?;
        let partners = crate::import::service::prepare_partners(
            self,
            &link.record,
            &link.partners,
            crate::util::rate_limiter::CallPriority::Interactive,
        )
        .await?;
        let audio = self.release_read_audio(release_id).await?;
        let values = release
            .read(&audio)
            .map_err(|refusal| LibraryError::from(crate::import::ImportError::from(refusal)))?;
        if values.tracks.is_empty() {
            return Err(LibraryError::from(
                crate::import::ImportError::MetadataTrackCount {
                    folder_tracks: u32::try_from(audio.lengths.len())
                        .expect("a release holds fewer than 2^32 tracks"),
                    release_tracks: 0,
                },
            ));
        }
        let mut seed = self.open_release_edit(release_id).await?;
        let track_ids: Vec<String> = seed
            .display
            .tracks
            .iter()
            .map(|track| track.track_id.clone())
            .collect();
        let draft = values.library_draft(track_ids);
        seed.edit = draft.release_edit();
        for (context, track) in seed.display.tracks.iter_mut().zip(&draft.tracks) {
            let position = crate::mediums::track_position(&track.place, &draft.mediums);
            context.position_text = crate::util::format::track_position_text(&position);
            context.side = crate::util::format::track_side(&position);
        }
        self.release_edits.sessions().insert(
            release_id.to_string(),
            AppliedCatalog {
                draft,
                records: crate::import::service::records_for_commit(&release, &partners),
            },
        );
        Ok(seed)
    }

    /// Drop the session on `release_id`: the library keeps what it holds.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn discard_release_edit(&self, release_id: &str) {
        self.release_edits.sessions().remove(release_id);
    }

    /// Save the session on `release_id` with the form `edit` the person left:
    /// in one write, the edited fields, every value a catalog applied in the
    /// session (its mediums and places, roles and works), its records, and the
    /// album the release lands in — `album` where the person picked one, else
    /// the one the applied records name. A session with nothing applied
    /// writes the fields, and moves the release only to a picked album.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn save_release_edit(
        &self,
        release_id: &str,
        edit: &crate::import::RawReleaseEdit,
        album: Option<String>,
    ) -> Result<(), LibraryError> {
        let applied = self.release_edits.sessions().get(release_id).cloned();
        let shaped = edit.shape()?;
        shaped.validate()?;
        let (current_album_id, release, current_album, existing_tracks) =
            self.load_release_for_edit(release_id).await?;
        let records = match &applied {
            Some(applied) => applied.records.clone(),
            None => self.database.get_release_records(release_id).await?,
        };
        let change = match (&album, &applied) {
            (Some(picked), _) => Some(crate::db::RecordsChange {
                release_id: release_id.to_string(),
                records: records.clone(),
                current_album_id: current_album_id.clone(),
                target_album_id: picked.clone(),
                new_album: None,
            }),
            (None, Some(applied)) => Some(
                self.records_change(release_id, applied.records.clone())
                    .await?,
            ),
            (None, None) => None,
        };
        let target_album_id = change.as_ref().map_or(current_album_id.clone(), |change| {
            change.target_album_id.clone()
        });
        // A release the person moved into another album keeps that album's
        // own title and artists.
        let album_fields = if album
            .as_ref()
            .is_some_and(|picked| *picked != current_album_id)
        {
            crate::db::AlbumFields::Kept
        } else {
            crate::db::AlbumFields::Written
        };
        let write = match applied {
            Some(applied) => {
                let mut draft = applied.draft;
                draft.album_title = edit.album_title.clone();
                draft.album_artist_assignments = edit.album_artist_assignments.clone();
                draft.album_year = edit.album_year.clone();
                draft.pressing = edit.pressing.clone();
                for (track, edited) in draft.tracks.iter_mut().zip(&edit.tracks) {
                    track.edit.title = edited.title.clone();
                    track.edit.artist_assignments = edited.artist_assignments.clone();
                }
                let keys = crate::import::assemble::RowKeys {
                    album_id: target_album_id,
                    release_id: release_id.to_string(),
                    track_id: &|track: &crate::import::LibraryTrack| track.0.clone(),
                };
                let assembled = crate::import::assemble::assemble_rows(
                    &draft,
                    &keys,
                    self.clock.as_ref(),
                    self.ids.as_ref(),
                )?;
                crate::db::ReleaseEditWrite::Applied {
                    rows: assembled.parsed,
                    picked: assembled.picked.into_iter().collect(),
                }
            }
            None => {
                let album_row = if target_album_id == current_album_id {
                    current_album
                } else {
                    self.database
                        .find_album_by_id(&target_album_id)
                        .await?
                        .ok_or_else(|| {
                            LibraryError::Import(format!("Album '{target_album_id}' not found"))
                        })?
                };
                crate::db::ReleaseEditWrite::Fields(self.release_metadata_rows(
                    album_row,
                    release,
                    &existing_tracks,
                    &shaped.normalized(),
                )?)
            }
        };
        self.database
            .save_release_edit_atomic(change, write, album_fields)
            .await?;
        self.release_edits.sessions().remove(release_id);
        Ok(())
    }
}

fn raw_release_edit_with_persisted_track_ids<'a>(
    edit: crate::import::ReleaseUserEdit,
    track_ids: impl ExactSizeIterator<Item = &'a str>,
) -> Result<crate::import::RawReleaseEdit, LibraryError> {
    if edit.tracks.len() != track_ids.len() {
        return Err(LibraryError::Internal(format!(
            "release editor source has {} tracks for {} persisted track IDs",
            edit.tracks.len(),
            track_ids.len()
        )));
    }
    let crate::import::ReleaseUserEdit {
        album_title,
        album_artist_assignments,
        album_year,
        pressing,
        tracks,
    } = edit;
    Ok(crate::import::RawReleaseEdit {
        album_title,
        album_artist_assignments,
        album_year: album_year.map(|year| year.to_string()).unwrap_or_default(),
        pressing: crate::import::RawPressingEdit::from_pressing(&pressing),
        tracks: tracks
            .into_iter()
            .zip(track_ids)
            .map(|(track, id)| crate::import::RawTrackEdit::from_user_edit(track, id.to_string()))
            .collect(),
    })
}
