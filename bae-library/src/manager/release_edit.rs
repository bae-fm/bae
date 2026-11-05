//! A library release's edit sessions.
//!
//! Each open editor owns a session of its own ([`ReleaseEdit`]): the
//! release's values as the library held them when it opened, and what the
//! person did in it: the form with each change written to it as it is made,
//! a catalog applied, which replaces every value the release holds (titles,
//! credits, mediums and places, roles and works, its records and the album
//! they name), and the library album picked to file it under. Every change
//! returns the session's draft, what its editor shows. Sessions on one
//! release share nothing, so one editor's edit, apply or pick is never
//! another's. Saving writes the session whole; dropping it discards it.
//! Nothing re-reads a catalog or the files' tags: the session holds what the
//! apply read.
//!
//! A save checks the release against what its session opened on: one the
//! library no longer holds as it did — another session saved it, MCP wrote
//! its fields, another device's edit synced in — is refused
//! ([`LibraryError::ReleaseEditStale`]) and writes nothing, so no session
//! overwrites values its editor never showed. The editor opens again to see
//! the release as it is now.

use super::*;

/// A release as the editor reads it: its detail, its album's artists, and
/// its tracks' audio in release order.
pub(crate) struct ReleaseEditContext {
    pub(crate) detail: bae_db::DbReleaseDetail,
    pub(crate) album_artists: Vec<bae_db::credits::LibraryCredit>,
    pub(crate) tracks: Vec<bae_db::DbReleaseEditTrack>,
}

/// One open editor's session on a library release.
#[derive(Debug)]
pub struct ReleaseEdit {
    release_id: String,
    /// The release as the library held it when the session opened: what the
    /// editor first showed, and what a save requires the library still holds.
    opened: bae_import::ReleaseEditSeed,
    /// Held across a change's reads, so each change starts from the one
    /// before it.
    work: tokio::sync::Mutex<SessionWork>,
}

/// What the person did in a session: the form as they left it, the tracks
/// and album it shows, and what its save writes beyond the form.
#[derive(Debug, Clone)]
struct SessionWork {
    form: bae_import::RawReleaseEdit,
    /// The tracks beside the form: the release's, or laid out again at the
    /// places an applied catalog gives them.
    display: bae_import::ReleaseEditDisplayContext,
    /// The album a save files the release under.
    album: bae_import::DraftAlbum,
    applied: Option<AppliedCatalog>,
    /// The library album the person picked, over the one the records name.
    picked_album: Option<String>,
}

impl SessionWork {
    /// Whether a save would write anything the session did not open on.
    fn changed(&self, opened: &bae_import::ReleaseEditSeed) -> bool {
        self.form != opened.edit || self.applied.is_some() || self.picked_album.is_some()
    }
}

/// What applying a catalog release in a session read: every value, laid onto
/// the release's tracks in order, and the records the pick names, the
/// applied one first.
#[derive(Debug, Clone)]
struct AppliedCatalog {
    draft: bae_import::LibraryDraft,
    records: Vec<bae_catalogs::records::ReleaseRecord>,
}

impl ReleaseEdit {
    /// The release this session edits.
    pub fn release_id(&self) -> &str {
        &self.release_id
    }

    /// The release as the library held it when the session opened.
    pub fn opened(&self) -> &bae_import::ReleaseEditSeed {
        &self.opened
    }
}

impl LibraryManager {
    /// Open an edit session on a library release: its values as the library
    /// holds them, with nothing edited, applied or picked. Every editor opens
    /// its own; a session on the same release elsewhere is untouched.
    pub async fn open_release_edit(&self, release_id: &str) -> Result<ReleaseEdit, LibraryError> {
        let opened = self.read_release_edit(release_id).await?;
        let work = SessionWork {
            form: opened.edit.clone(),
            display: opened.display.clone(),
            album: opened.album.clone(),
            applied: None,
            picked_album: None,
        };
        Ok(ReleaseEdit {
            release_id: release_id.to_string(),
            opened,
            work: tokio::sync::Mutex::new(work),
        })
    }

    /// The release as the editor reads it, from one read: its detail, its
    /// album's artists, and its tracks' audio in release order.
    pub(crate) async fn find_release_edit_context(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseEditContext>, bae_db::DbError> {
        let release_id = release_id.to_string();
        self.database
            .read(move |sql| {
                let Some(release) = bae_db::client::read::find_release_by_id_on(&sql, &release_id)?
                else {
                    return Ok(None);
                };
                let album_artists =
                    bae_db::client::read::get_artists_for_album_on(&sql, &release.album_id)?;
                let detail = bae_db::client::release_detail_rows_on(&sql, release)?;
                let tracks = bae_db::client::release_edit_tracks_on(&sql, &release_id)?;
                Ok(Some((detail, album_artists, tracks)))
            })
            .process(|rows| {
                Ok(rows.map(|(detail, album_artists, tracks)| ReleaseEditContext {
                    detail: detail.process(),
                    album_artists,
                    tracks,
                }))
            })
            .await
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
    ) -> Result<bae_import::ReleaseEditSeed, LibraryError> {
        let ReleaseEditContext { detail, album_artists, tracks: edit_tracks } = self
            .find_release_edit_context(release_id)
            .await?
            .ok_or_else(|| LibraryError::release_missing(release_id, super::ReleaseWork::Edit))?;
        let release = &detail.release;
        let album = self.database.find_album_by_id(&release.album_id).await?.ok_or_else(|| {
            LibraryError::Refused(format!("Album '{}' not found", release.album_id))
        })?;
        let display = bae_import::ReleaseEditDisplayContext::from_raw(&detail, edit_tracks)?;
        let cover = bae_db::cover_ref(release_id, detail.cover_version.clone());

        let album_artist_assignments: Vec<bae_import::Credited> =
            album_artists.into_iter().map(bae_import::Credited::of_library).collect();

        let mut tracks = Vec::with_capacity(detail.tracks.len());
        for entry in &detail.tracks {
            // Empty when the track has no artist rows of its own — the wire edit
            // reads that as "shares the album artist", matching how
            // `apply_release_metadata_user_edit` writes it back.
            let artists: Vec<bae_import::Credited> =
                entry.artists.clone().into_iter().map(bae_import::Credited::of_library).collect();
            tracks.push((
                entry.track.id.clone(),
                bae_import::TrackUserEdit {
                    title: entry.track.title.clone(),
                    artist_assignments: if artists.is_empty() {
                        bae_import::TrackArtistAssignments::AlbumArtists
                    } else {
                        bae_import::TrackArtistAssignments::Explicit(artists)
                    },
                    // Re-projecting a release's metadata never re-binds its files;
                    // the audio each track already points at stays as it is.
                    file: None,
                },
            ));
        }

        let edit = raw_release_edit_with_persisted_track_ids(
            bae_import::ReleaseUserEdit {
                album_title: album.title.clone(),
                album_artist_assignments,
                album_year: album.year,
                pressing: release.pressing.clone(),
                tracks: tracks.iter().map(|(_, track)| track.clone()).collect(),
            },
            tracks.iter().map(|(id, _)| id.as_str()),
        )?;

        let DbAlbum { id: album_id, title, .. } = album;
        Ok(bae_import::ReleaseEditSeed {
            edit,
            cover,
            display,
            album: bae_import::DraftAlbum::Named { album_id, title },
        })
    }

    /// The session's draft as its editor shows it now.
    pub async fn release_edit_draft(
        &self,
        session: &ReleaseEdit,
    ) -> Result<bae_import::ReleaseEditDraft, LibraryError> {
        let work = session.work.lock().await.clone();
        self.draft_of(session, work).await
    }

    /// Write one album-level field of the form as the person left it.
    /// Returns the session's draft.
    pub async fn edit_release_edit_field(
        &self,
        session: &ReleaseEdit,
        edit: bae_import::DraftFieldEdit,
    ) -> Result<bae_import::ReleaseEditDraft, LibraryError> {
        let work = {
            let mut work = session.work.lock().await;
            work.form.set_field(edit);
            work.clone()
        };
        self.draft_of(session, work).await
    }

    /// Replace the form's ordered album credits. Returns the session's draft.
    pub async fn set_release_edit_album_artists(
        &self,
        session: &ReleaseEdit,
        assignments: Vec<bae_import::Credited>,
    ) -> Result<bae_import::ReleaseEditDraft, LibraryError> {
        let work = {
            let mut work = session.work.lock().await;
            work.form.set_album_artists(assignments);
            work.clone()
        };
        self.draft_of(session, work).await
    }

    /// Write one track row's title and artists as the person left them, by
    /// the row's id. Returns the session's draft. A row the form does not
    /// hold is refused.
    pub async fn set_release_edit_track(
        &self,
        session: &ReleaseEdit,
        track: bae_import::RawTrackEdit,
    ) -> Result<bae_import::ReleaseEditDraft, LibraryError> {
        let id = track.id.clone();
        let work = {
            let mut work = session.work.lock().await;
            if !work.form.set_track(track) {
                return Err(LibraryError::Refused(format!("release edit has no track row '{id}'")));
            }
            work.clone()
        };
        self.draft_of(session, work).await
    }

    /// The draft of a session that has done `work`: what the library holds
    /// for each credit its form carries, and the release's cover as it is now,
    /// which no session holds.
    async fn draft_of(
        &self,
        session: &ReleaseEdit,
        work: SessionWork,
    ) -> Result<bae_import::ReleaseEditDraft, LibraryError> {
        let changed = work.changed(&session.opened);
        let SessionWork { form, display, album, .. } = work;
        let credits: Vec<_> = form.credits().cloned().collect();
        let artist_resolutions = self.database.resolve_artist_credits(&credits).await?;
        let cover = self.cover_ref(session.release_id()).await?;
        let invalid = form.shape().err();
        Ok(bae_import::ReleaseEditDraft {
            edit: form,
            cover,
            display,
            album,
            artist_resolutions,
            changed,
            invalid,
        })
    }

    /// Apply the catalog release `link` picks to `session`: read it against
    /// the release's tracks' stored lengths and its files, as identification
    /// reads them, and hold every value it gives and the records it names
    /// until the save. The form becomes the reading's values and the tracks
    /// are laid out at the places it gives them; the cover is the release's
    /// own, which an apply never changes. Returns the session's draft. A
    /// release whose tracks the reading cannot lay out is refused with both
    /// counts. Each fetch it waits on is told to `steps`.
    pub async fn apply_release_edit_catalog(
        &self,
        session: &ReleaseEdit,
        link: &bae_identify::PressingLink,
        steps: &bae_import::CatalogApplySteps,
    ) -> Result<bae_import::ReleaseEditDraft, LibraryError> {
        let release_id = session.release_id();
        // Nothing names these releases, so they are read and never stored.
        let reads = bae_import::ReleaseReads::new(self.database.clone(), self.catalogs.clone());
        let release = reads.applied_release(&link.record, steps).await?.release;
        let partners: Vec<_> = reads
            .partners(
                &link.record,
                &link.partners,
                bae_catalogs::rate_limiter::CallPriority::Interactive,
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
        let opened = &session.opened;
        let track_ids: Vec<String> =
            opened.display.tracks().map(|track| track.track_id.clone()).collect();
        let draft = bae_import::reading_draft::library_draft(values, track_ids);
        let work = {
            let mut work = session.work.lock().await;
            let mut applied = work.clone();
            applied.form = draft.release_edit();
            applied.display = opened.display.clone().placed(
                draft
                    .tracks
                    .iter()
                    .map(|track| bae_types::mediums::track_position(&track.place, &draft.mediums)),
            );
            applied.applied = Some(AppliedCatalog {
                draft,
                records: bae_import::service::records_for_commit(&release, &partners),
            });
            applied.album = self.session_album(release_id, &applied, opened.album.clone()).await?;
            *work = applied;
            work.clone()
        };
        self.draft_of(session, work).await
    }

    /// Pick the library album `album_id` for `session` to file its release
    /// under, over the one its records name, or (`None`) leave it to its
    /// records. Returns the session's draft, naming the album it now files
    /// the release under. An album the library does not hold cannot be
    /// picked.
    pub async fn set_release_edit_album(
        &self,
        session: &ReleaseEdit,
        album_id: Option<String>,
    ) -> Result<bae_import::ReleaseEditDraft, LibraryError> {
        let release_id = session.release_id();
        if let Some(album_id) = &album_id {
            self.database
                .find_album_by_id(album_id)
                .await?
                .ok_or_else(|| LibraryError::Refused(format!("Album '{album_id}' not found")))?;
        }
        let work = {
            let mut work = session.work.lock().await;
            let mut picked = work.clone();
            picked.picked_album = album_id;
            picked.album =
                self.session_album(release_id, &picked, session.opened.album.clone()).await?;
            *work = picked;
            work.clone()
        };
        self.draft_of(session, work).await
    }

    /// The album a session that has done `work` files the release under: the
    /// one picked, else the one its applied records name (a new one where they
    /// name none the library holds), else `current`, the album the release is
    /// in.
    async fn session_album(
        &self,
        release_id: &str,
        work: &SessionWork,
        current: bae_import::DraftAlbum,
    ) -> Result<bae_import::DraftAlbum, LibraryError> {
        let title = |album_id: String| async move {
            let album =
                self.database.find_album_by_id(&album_id).await?.ok_or_else(|| {
                    LibraryError::Refused(format!("Album '{album_id}' not found"))
                })?;
            Ok::<_, LibraryError>((album_id, album.title))
        };
        if let Some(picked) = &work.picked_album {
            let (album_id, title) = title(picked.clone()).await?;
            return Ok(bae_import::DraftAlbum::Picked { album_id, title });
        }
        let Some(applied) = &work.applied else {
            return Ok(current);
        };
        let change = self.records_change(release_id, applied.records.clone()).await?;
        Ok(match change.new_album {
            Some(_) => bae_import::DraftAlbum::New,
            None => {
                let (album_id, title) = title(change.target_album_id).await?;
                bae_import::DraftAlbum::Named { album_id, title }
            }
        })
    }

    /// Save `session`: in one write, the form as the person left it, every
    /// value a catalog applied in the session (its
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
    pub async fn save_release_edit(&self, session: &ReleaseEdit) -> Result<(), LibraryError> {
        let release_id = session.release_id();
        let SessionWork { form: edit, applied, picked_album: album, .. } =
            session.work.lock().await.clone();
        let shaped = edit.shape()?;
        shaped.validate()?;
        let _writing = self.release_writes.lock().await;
        if !self.read_release_edit(release_id).await?.holds_the_values_of(&session.opened) {
            return Err(LibraryError::ReleaseEditStale { release_id: release_id.to_string() });
        }
        let (current_album_id, release, current_album, existing_tracks) =
            self.load_release_for_edit(release_id).await?;
        let records = match &applied {
            Some(applied) => applied.records.clone(),
            None => self.database.get_release_records(release_id).await?,
        };
        let change = match (&album, &applied) {
            (Some(picked), _) => Some(bae_db::RecordsChange {
                release_id: release_id.to_string(),
                records: records.clone(),
                current_album_id: current_album_id.clone(),
                target_album_id: picked.clone(),
                new_album: None,
            }),
            (None, Some(applied)) => {
                Some(self.records_change(release_id, applied.records.clone()).await?)
            }
            (None, None) => None,
        };
        let target_album_id = change
            .as_ref()
            .map_or(current_album_id.clone(), |change| change.target_album_id.clone());
        // A release the person moved into another album keeps that album's
        // own title and artists.
        let album_fields = if album.as_ref().is_some_and(|picked| *picked != current_album_id) {
            bae_db::AlbumFields::Kept
        } else {
            bae_db::AlbumFields::Written
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
                bae_db::ReleaseEditWrite::Applied {
                    rows: assembled.parsed,
                    picked: assembled.picked.into_iter().collect(),
                }
            }
            None => {
                let album_row = if target_album_id == current_album_id {
                    current_album
                } else {
                    self.database.find_album_by_id(&target_album_id).await?.ok_or_else(|| {
                        LibraryError::Refused(format!("Album '{target_album_id}' not found"))
                    })?
                };
                bae_db::ReleaseEditWrite::Fields(self.release_metadata_rows(
                    album_row,
                    release,
                    &existing_tracks,
                    &shaped.normalized(),
                )?)
            }
        };
        self.database.save_release_edit_atomic(change, write, album_fields).await?;
        Ok(())
    }
}

fn raw_release_edit_with_persisted_track_ids<'a>(
    edit: bae_import::ReleaseUserEdit,
    track_ids: impl ExactSizeIterator<Item = &'a str>,
) -> Result<bae_import::RawReleaseEdit, LibraryError> {
    if edit.tracks.len() != track_ids.len() {
        return Err(LibraryError::Internal(format!(
            "release editor source has {} tracks for {} persisted track IDs",
            edit.tracks.len(),
            track_ids.len()
        )));
    }
    let bae_import::ReleaseUserEdit {
        album_title,
        album_artist_assignments,
        album_year,
        pressing,
        tracks,
    } = edit;
    Ok(bae_import::RawReleaseEdit {
        album_title,
        album_artist_assignments,
        album_year: album_year.map(|year| year.to_string()).unwrap_or_default(),
        pressing: bae_import::RawPressingEdit::from_pressing(&pressing),
        tracks: tracks
            .into_iter()
            .zip(track_ids)
            .map(|(track, id)| bae_import::RawTrackEdit::from_user_edit(track, id.to_string()))
            .collect(),
    })
}

#[cfg(test)]
#[path = "release_edit_tests.rs"]
mod tests;
