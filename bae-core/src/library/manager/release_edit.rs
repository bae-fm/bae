//! A library release's edit sessions.
//!
//! Each open editor owns a session of its own ([`ReleaseEditSession`]): the
//! release's values as the library held them when it opened, and what the
//! person did in it beyond the form the editor keeps — a catalog applied,
//! which replaces every value the release holds (titles, credits, mediums and
//! places, roles and works, its records and the album they name), and the
//! library album picked to file it under. Sessions on one release share
//! nothing, so one editor's apply or pick is never another's. Saving writes
//! the session whole; dropping it discards it. Nothing re-reads a catalog or
//! the files' tags: the session holds what the apply read.
//!
//! A save checks the release against what its session opened on: one the
//! library no longer holds as it did — another session saved it, MCP wrote
//! its fields, another device's edit synced in — is refused
//! ([`LibraryError::ReleaseEditStale`]) and writes nothing, so no session
//! overwrites values its editor never showed. The editor opens again to see
//! the release as it is now.

use super::*;

/// One open editor's session on a library release.
#[derive(Debug)]
pub struct ReleaseEditSession {
    release_id: String,
    /// The release as the library held it when the session opened: what the
    /// editor first showed, and what a save requires the library still holds.
    opened: crate::import::ReleaseEditSeed,
    work: std::sync::Mutex<SessionWork>,
}

/// What the person did in a session beyond the form: what its save writes
/// beyond the edited fields.
#[derive(Debug, Default, Clone)]
struct SessionWork {
    applied: Option<AppliedCatalog>,
    /// The library album the person picked, over the one the records name.
    picked_album: Option<String>,
}

/// What applying a catalog release in a session read: every value, laid onto
/// the release's tracks in order, and the records the pick names, the
/// applied one first.
#[derive(Debug, Clone)]
struct AppliedCatalog {
    draft: crate::import::LibraryDraft,
    records: Vec<crate::import::ReleaseRecord>,
}

impl ReleaseEditSession {
    /// The release this session edits.
    pub fn release_id(&self) -> &str {
        &self.release_id
    }

    /// The release as the library held it when the session opened: the form,
    /// cover, tracks and album the editor starts from.
    pub fn opened(&self) -> &crate::import::ReleaseEditSeed {
        &self.opened
    }

    fn work(&self) -> std::sync::MutexGuard<'_, SessionWork> {
        self.work
            .lock()
            .expect("a release edit session's lock is never held across a panic")
    }

    /// What the person has done in the session so far.
    fn snapshot(&self) -> SessionWork {
        self.work().clone()
    }
}

impl LibraryManager {
    /// Open an edit session on a library release: its values as the library
    /// holds them, with no catalog applied and no album picked. Every editor
    /// opens its own; a session on the same release elsewhere is untouched.
    pub async fn open_release_edit(
        &self,
        release_id: &str,
    ) -> Result<ReleaseEditSession, LibraryError> {
        Ok(ReleaseEditSession {
            release_id: release_id.to_string(),
            opened: self.read_release_edit(release_id).await?,
            work: std::sync::Mutex::default(),
        })
    }

    /// The editor's seed for `release_id` as the library holds it, filed under
    /// the album it is in.
    ///
    /// The album title and artists, the release's pressing fields and each
    /// track's title and artists, rendered into the raw editor form. A track
    /// with no artist rows of its own seeds an empty artist field ("shares the
    /// album artist"); the album artists seed the album artist field.
    async fn read_release_edit(
        &self,
        release_id: &str,
    ) -> Result<crate::import::ReleaseEditSeed, LibraryError> {
        let (context, edit_tracks) = self
            .database
            .find_release_edit_context(release_id)
            .await?
            .ok_or_else(|| LibraryError::Refused(format!("Release '{release_id}' not found")))?;
        let release = &context.detail.release;
        let album = self
            .database
            .find_album_by_id(&release.album_id)
            .await?
            .ok_or_else(|| {
                LibraryError::Refused(format!("Album '{}' not found", release.album_id))
            })?;
        let display =
            crate::album_detail::ReleaseEditDisplayContext::from_raw(&context.detail, edit_tracks)?;
        let cover =
            crate::album_detail::cover_ref(release_id, context.detail.cover_version.clone());

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
                album_title: album.title.clone(),
                album_artist_assignments,
                album_year: album.year,
                pressing: release.pressing.clone(),
                tracks: tracks.iter().map(|(_, track)| track.clone()).collect(),
            },
            tracks.iter().map(|(id, _)| id.as_str()),
        )?;

        let DbAlbum {
            id: album_id,
            title,
            ..
        } = album;
        Ok(crate::import::ReleaseEditSeed {
            edit,
            cover,
            display,
            album: crate::import::DraftAlbum::Named { album_id, title },
        })
    }

    /// Apply the catalog release `link` picks to `session`: read it against
    /// the release's tracks' stored lengths and its files, as identification
    /// reads them, and hold every value it gives and the records it names
    /// until the save. The form comes back
    /// filled with the reading's values; the cover is the release's own, which
    /// an apply never changes. A release whose tracks the reading cannot lay
    /// out is refused with both counts.
    pub async fn apply_release_edit_catalog(
        &self,
        session: &ReleaseEditSession,
        link: &crate::import::PressingLink,
    ) -> Result<crate::import::ReleaseEditSeed, LibraryError> {
        let release_id = session.release_id();
        let release = crate::import::service::prepare_release(
            self,
            &link.record,
            crate::import::payloads::ReadDepth::Complete,
            bae_catalogs::rate_limiter::CallPriority::Interactive,
        )
        .await?;
        let partners = crate::import::service::prepare_partners(
            self,
            &link.record,
            &link.partners,
            bae_catalogs::rate_limiter::CallPriority::Interactive,
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
        let mut seed = self.read_release_edit(release_id).await?;
        let track_ids: Vec<String> = seed
            .display
            .tracks()
            .map(|track| track.track_id.clone())
            .collect();
        let draft = values.library_draft(track_ids);
        seed.edit = draft.release_edit();
        seed.display = seed.display.placed(
            draft
                .tracks
                .iter()
                .map(|track| crate::mediums::track_position(&track.place, &draft.mediums)),
        );
        let work = {
            let mut work = session.work();
            work.applied = Some(AppliedCatalog {
                draft,
                records: crate::import::service::records_for_commit(&release, &partners),
            });
            work.clone()
        };
        seed.album = self.session_album(release_id, &work, seed.album).await?;
        Ok(seed)
    }

    /// Pick the library album `album_id` for `session` to file its release
    /// under, over the one its records name, or (`None`) leave it to its
    /// records. Returns the album the session now files it under. An album
    /// the library does not hold cannot be picked.
    pub async fn set_release_edit_album(
        &self,
        session: &ReleaseEditSession,
        album_id: Option<String>,
    ) -> Result<crate::import::DraftAlbum, LibraryError> {
        let release_id = session.release_id();
        if let Some(album_id) = &album_id {
            self.database
                .find_album_by_id(album_id)
                .await?
                .ok_or_else(|| LibraryError::Refused(format!("Album '{album_id}' not found")))?;
        }
        let work = {
            let mut work = session.work();
            work.picked_album = album_id;
            work.clone()
        };
        let current = self.read_release_edit(release_id).await?.album;
        self.session_album(release_id, &work, current).await
    }

    /// The album a session that has done `work` files the release under: the
    /// one picked, else the one its applied records name (a new one where they
    /// name none the library holds), else `current`, the album the release is
    /// in.
    async fn session_album(
        &self,
        release_id: &str,
        work: &SessionWork,
        current: crate::import::DraftAlbum,
    ) -> Result<crate::import::DraftAlbum, LibraryError> {
        let title = |album_id: String| async move {
            let album = self
                .database
                .find_album_by_id(&album_id)
                .await?
                .ok_or_else(|| LibraryError::Refused(format!("Album '{album_id}' not found")))?;
            Ok::<_, LibraryError>((album_id, album.title))
        };
        if let Some(picked) = &work.picked_album {
            let (album_id, title) = title(picked.clone()).await?;
            return Ok(crate::import::DraftAlbum::Picked { album_id, title });
        }
        let Some(applied) = &work.applied else {
            return Ok(current);
        };
        let change = self
            .records_change(release_id, applied.records.clone())
            .await?;
        Ok(match change.new_album {
            Some(_) => crate::import::DraftAlbum::New,
            None => {
                let (album_id, title) = title(change.target_album_id).await?;
                crate::import::DraftAlbum::Named { album_id, title }
            }
        })
    }

    /// Save `session` with the form `edit` the person left: in one write, the
    /// edited fields, every value a catalog applied in the session (its
    /// mediums and places, roles and works), its records, and the album the
    /// release lands in — the one the person picked, else the one the applied
    /// records name. A session with nothing applied writes the fields, and
    /// moves the release only to a picked album.
    ///
    /// Refused with [`LibraryError::ReleaseEditStale`], writing nothing, when
    /// the library no longer holds the release as the session opened on it.
    /// No other session's save, and no field edit
    /// ([`Self::apply_release_metadata_user_edit`]), runs on this device
    /// between that check and the write.
    pub async fn save_release_edit(
        &self,
        session: &ReleaseEditSession,
        edit: &crate::import::RawReleaseEdit,
    ) -> Result<(), LibraryError> {
        let release_id = session.release_id();
        let SessionWork {
            applied,
            picked_album: album,
        } = session.snapshot();
        let shaped = edit.shape()?;
        shaped.validate()?;
        let _writing = self.release_writes.lock().await;
        if !self
            .read_release_edit(release_id)
            .await?
            .holds_the_values_of(&session.opened)
        {
            return Err(LibraryError::ReleaseEditStale {
                release_id: release_id.to_string(),
            });
        }
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
                            LibraryError::Refused(format!("Album '{target_album_id}' not found"))
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
