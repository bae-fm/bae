//! An import's commit: the release's rows, written by db's row writers in
//! the one transaction that also reads the candidate the import commits, so
//! the commit and the check that it still describes the prepared candidate
//! are one.

use super::*;
use bae_records::records::artist_resolution::{relink, relink_album_artists, relink_track_artists};
use bae_records::records::label_resolution::label_write;
use bae_records::records::write::{
    insert_audio_format_row, insert_audio_segment_row, insert_release_artist_role_row,
    insert_release_file_row, insert_release_medium_row, insert_release_row,
    insert_scrub_points_row, insert_track_artist_role_row, insert_track_artist_row,
    insert_track_row, insert_track_work_row, insert_work_artist_row, insert_work_part_row,
    insert_work_row, replace_album_artists, upsert_album_row, upsert_library_image_row,
    write_release_records,
};
use std::collections::BTreeSet;

/// Why an import's commit did not land. Nothing of it was written; the
/// person may ask for the import again.
#[derive(Debug, thiserror::Error)]
pub enum ImportCommitError {
    /// Writing the release's rows failed as any write that decides which
    /// library artists and labels it names fails.
    #[error(transparent)]
    Write(#[from] IdentityWriteError),
    /// The library's artists changed between preparing the commit and
    /// writing it in a way that makes what was prepared for them wrong: the
    /// import fetched pictures for artists that are no longer new, or not for
    /// ones that now are. Importing again fetches against the library as it
    /// now stands.
    #[error("the library's artists changed while this import was being written; import it again")]
    LibraryChanged,
    /// Another import of the same files landed first: this device already
    /// holds `release_id` for the folder hash, and one hash is one release
    /// here.
    #[error("this device already imported these files, as release {release_id}")]
    FilesAlreadyImported { release_id: String },
    /// The candidate is not the one the import was prepared from.
    #[error(transparent)]
    CandidateMoved(#[from] crate::service::CandidateMoved),
    /// The commit was refused, and rolling it back failed too.
    #[error("{operation}; rollback failed: {rollback}")]
    Rollback {
        operation: Box<ImportCommitError>,
        #[source]
        rollback: rusqlite::Error,
    },
    /// The commit was refused, and removing the bytes it staged failed too,
    /// each failure kept.
    #[error("file cleanup failed: {failures:?}; write: {write}")]
    FileCleanup { write: Box<ImportCommitError>, failures: Vec<coven::DbError> },
}

impl From<rusqlite::Error> for ImportCommitError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Write(error.into())
    }
}

impl From<DbError> for ImportCommitError {
    fn from(error: DbError) -> Self {
        Self::Write(error.into())
    }
}

impl From<coven::DbError> for ImportCommitError {
    fn from(error: coven::DbError) -> Self {
        Self::Write(error.into())
    }
}

impl coven::task::CarriesBug for ImportCommitError {
    fn bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Write(error) => error.bug(),
            Self::Rollback { operation, .. } => operation.bug(),
            Self::FileCleanup { write, failures } => {
                write.bug().or_else(|| failures.iter().find_map(coven::task::CarriesBug::bug))
            }
            Self::LibraryChanged | Self::FilesAlreadyImported { .. } | Self::CandidateMoved(_) => {
                None
            }
        }
    }
}

/// A failed rollback or byte removal is kept beside the failure it followed:
/// the write's own error keeps it as that error does, a refusal stays the
/// refusal it is.
impl coven::WriteFailure for ImportCommitError {
    fn with_rollback(self, rollback: rusqlite::Error) -> Self {
        match self {
            Self::Write(error) => Self::Write(error.with_rollback(rollback)),
            operation => Self::Rollback { operation: Box::new(operation), rollback },
        }
    }

    fn with_cleanup(write: Self, failures: Vec<coven::DbError>) -> Self {
        match write {
            Self::Write(error) => Self::Write(IdentityWriteError::with_cleanup(error, failures)),
            write => Self::FileCleanup { write: Box::new(write), failures },
        }
    }
}

/// Refuse the import unless the candidate is still exactly what the import
/// was prepared from. The rows are read here; what must match is
/// [`ImportExpectation::verify`](crate::service::ImportExpectation::verify)'s
/// to say.
fn require_import_commit_guard(
    sql: &CovenWrite<'_>,
    guard: &ImportCommitGuard,
) -> Result<(), ImportCommitError> {
    match guard {
        ImportCommitGuard::Candidate { candidate_key, source, expectation, import_id: _ } => {
            let current = load_committing_candidate(sql, candidate_key, expectation)?;
            Ok(expectation.verify(candidate_key, source, current.as_ref())?)
        }
    }
}

/// End the import attempt the commit is, inside it: the release landing
/// answers the failure an earlier attempt left, and ends the import's claim
/// on the candidate, so no read sees the release beside the import still
/// going or the import gone without its release. That failure stands until
/// this write — an attempt that never commits, cancelled or failing with a
/// failure of its own, leaves it as it found it or replaces it.
fn end_import_attempt_of_commit(
    sql: &CovenWrite<'_>,
    guard: &ImportCommitGuard,
) -> Result<(), DbError> {
    match guard {
        ImportCommitGuard::Candidate { candidate_key, expectation, import_id, .. } => {
            super::delete_failure_on(sql, &expectation.candidate.content_hash)?;
            // The committing import holds its claim: a cancel no longer
            // reaches an import writing its release.
            let ended = super::move_import_on(sql, candidate_key, import_id, None)?;
            assert!(ended, "the committing import {import_id} does not own {candidate_key}");
            Ok(())
        }
    }
}

/// Record, inside the commit, the folder hash of the files this device
/// imported the release from: what places the folder in Done, wherever it is
/// moved. One hash is one release on this device. The import was refused when
/// asked for if this device already held the hash; an import of the same files
/// that landed in between leaves the hash taken, and the unique hash refuses
/// this commit whole as [`ImportCommitError::FilesAlreadyImported`].
fn record_import_origin_on(
    sql: &CovenWrite<'_>,
    guard: &ImportCommitGuard,
    release_id: &str,
) -> Result<(), ImportCommitError> {
    match guard {
        ImportCommitGuard::Candidate { expectation, .. } => {
            use crate::tables::import_origin::{record_on, OriginRecord};
            match record_on(sql, release_id, &expectation.candidate.content_hash)
                .map_err(|error| ImportCommitError::Write(error.into()))?
            {
                OriginRecord::Recorded => Ok(()),
                OriginRecord::Taken { release_id } => {
                    Err(ImportCommitError::FilesAlreadyImported { release_id })
                }
            }
        }
    }
}

/// Delete, inside the commit, what the candidate holds that nothing reads once
/// its release is in: the identify verdict (its releases and ledger go with
/// it), what the person let the lookups use, where the pane was, the artist
/// and label decisions this commit just resolved its credits and labels by,
/// the library album picked for it, and its draft with its source, its cover
/// and its link: the Done row and the imported folder's pane read the
/// library release. The clues stay, since that pane marks the files they
/// were read off, and so do the file decisions, which lay its files out. A
/// commit that does not land deletes none of it, so a retry finds the
/// candidate as the failed attempt left it.
fn delete_what_the_import_spent_on(
    sql: &CovenWrite<'_>,
    guard: &ImportCommitGuard,
) -> Result<(), DbError> {
    match guard {
        ImportCommitGuard::Candidate { expectation, .. } => {
            let content_hash = expectation.candidate.content_hash.as_str();
            delete_verdict(sql, content_hash)?;
            super::lookup_choice_rows::delete_lookup_choices(sql, content_hash)?;
            super::session_rows::delete_session(sql, content_hash)?;
            super::album_pick_rows::delete_album_pick(sql, content_hash)?;
            super::pane_rows::delete_draft(sql, content_hash)?;
            // The cover takes its copies and its fetched bytes.
            super::cover_rows::delete_cover(sql, content_hash)?;
            super::release_link_rows::store_release_link(sql, content_hash, None)
        }
    }
}

/// The stored candidate as a library import finds it at commit time: what
/// the scan lists at the key, and the revisions its preparation is at.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommittingCandidate {
    pub actionable: bool,
    pub source: crate::release_candidate::CandidateSource,
    pub content_hash: String,
    pub file_edit_revision: u64,
    /// The state row's file and metadata revisions, or `None` when no row
    /// is stored under the content hash the import was prepared from.
    pub prepared_revisions: Option<(u64, u64)>,
    pub file_tag_snapshot: Option<crate::file_tag_snapshot::FileTagSnapshot>,
}

/// The stored candidate at `candidate_key` as the commit sees it, or `None`
/// when the scan no longer lists one there.
fn load_committing_candidate(
    sql: &CovenWrite<'_>,
    candidate_key: &crate::CandidateKey,
    expectation: &crate::service::ImportExpectation,
) -> Result<Option<CommittingCandidate>, DbError> {
    let Some(stored) = super::release_groupings::load_candidate_on(sql, candidate_key)? else {
        return Ok(None);
    };
    let candidate = stored.candidate;
    let revisions: Option<(i64, i64)> = sql
        .query_row(
            "SELECT edit_revision, metadata_revision FROM import_candidate_state \
             WHERE content_hash = ?",
            [expectation.candidate.content_hash.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let prepared_revisions = revisions
        .map(|(edit, metadata)| {
            Ok::<_, DbError>((
                to_u64(edit, "import_candidate_state.edit_revision")?,
                to_u64(metadata, "import_candidate_state.metadata_revision")?,
            ))
        })
        .transpose()?;
    let file_tag_snapshot =
        super::folder_scans::load_candidate_file_tag_snapshot(sql, candidate_key)?
            .and_then(|stored| stored.snapshot);
    Ok(Some(CommittingCandidate {
        actionable: stored.actionable,
        source: candidate.source(),
        content_hash: candidate.files.content_hash(),
        file_edit_revision: candidate.file_edit_revision,
        prepared_revisions,
        file_tag_snapshot,
    }))
}

/// The exact candidate state a library import transaction is allowed to
/// consume. The final write checks this inside the transaction before writing
/// any library row.
#[derive(Debug, Clone)]
pub(crate) enum ImportCommitGuard {
    Candidate {
        candidate_key: crate::CandidateKey,
        source: crate::release_candidate::CandidateSource,
        expectation: crate::service::ImportExpectation,
        /// The import committing, whose claim on the candidate the commit
        /// ends with the release.
        import_id: String,
    },
}

/// The row lists an import commit writes around its release — the artist, work,
/// role, audio-format, and record rows that hang off it.
///
/// One group rather than thirteen parameters: each is a plain list the caller
/// either has or does not, and `Default` is every one of them empty, so a
/// caller names only the lists it actually fills.
///
/// Every artist link here — `album_artists`, `track_artists`, the role rows and `work_artists` — names its artist by the
/// id of one of `artists.credits`. The commit resolves those credits to
/// library artists inside its own transaction and writes each link to the
/// artist its credit resolved to.
#[derive(Default)]
pub(crate) struct ImportRows<'a> {
    pub mediums: &'a [DbReleaseMedium],
    pub track_artists: &'a [DbTrackArtist],
    pub album_artists: &'a [DbAlbumArtist],
    pub works: &'a [DbWork],
    pub work_artists: &'a [DbWorkArtist],
    pub work_parts: &'a [DbWorkPart],
    pub track_works: &'a [DbTrackWork],
    pub release_artist_roles: &'a [DbReleaseArtistRole],
    pub track_artist_roles: &'a [DbTrackArtistRole],
    pub artists: bae_records::ArtistCredits<'a>,
    pub audio_formats: &'a [DbAudioFormat],
    pub audio_segments: &'a [DbAudioSegment],
    pub scrub_points: &'a [DbScrubPoints],
    pub records: &'a [bae_catalogs::records::ReleaseRecord],
}

/// Pictures for the new artists an import expects to create. Their bytes are
/// staged before the transaction's SQL runs, so they are prepared against a
/// read of the library just before it; the transaction resolves the credits
/// again and refuses to commit if the new artists it would create are not
/// exactly `expected_new_artists` — a picture is never left without its
/// artist, and no new artist misses the picture prepared for it.
#[derive(Default, Clone, Copy)]
pub(crate) struct NewArtistImages<'a> {
    pub expected_new_artists: &'a [String],
    pub images: &'a [(&'a DbLibraryImage, &'a [u8])],
}

impl ImportTables {
    /// Finalize an import in one transaction. Nothing of it is in the DB yet except
    /// the import record — every row below lands together or not at all.
    pub(crate) async fn finalize_import_atomic(
        &self,
        ids: &dyn coven::IdSource,
        guard: ImportCommitGuard,
        // `None` when the album is an existing one, already in the DB.
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::TrackFile],
        rows: ImportRows<'_>,
        files: Vec<crate::service::PreparedImportFile>,
        library_image: Option<(&DbLibraryImage, &[u8])>,
        artist_images: NewArtistImages<'_>,
    ) -> Result<(), ImportCommitError> {
        let album = album.cloned();
        let release = release.clone();
        let tracks: Vec<DbTrack> = tracks_to_files.iter().map(|tf| tf.db_track.clone()).collect();
        let mediums = rows.mediums.to_vec();
        let track_artists = rows.track_artists.to_vec();
        let album_artists = rows.album_artists.to_vec();
        let works = rows.works.to_vec();
        let work_artists = rows.work_artists.to_vec();
        let work_parts = rows.work_parts.to_vec();
        let track_works = rows.track_works.to_vec();
        let release_artist_roles = rows.release_artist_roles.to_vec();
        let track_artist_roles = rows.track_artist_roles.to_vec();
        let artist_credits = rows.artists.credits.to_vec();
        let picked_artists = rows.artists.picked.to_vec();
        let expected_new_artists: BTreeSet<String> =
            artist_images.expected_new_artists.iter().cloned().collect();
        let audio_formats = rows.audio_formats.to_vec();
        let audio_segments = rows.audio_segments.to_vec();
        let scrub_points = rows.scrub_points.to_vec();
        let library_image = library_image.map(|(image, bytes)| (image.clone(), bytes.to_vec()));
        let artist_images: Vec<(DbLibraryImage, Vec<u8>)> = artist_images
            .images
            .iter()
            .map(|(image, bytes)| ((*image).clone(), (*bytes).to_vec()))
            .collect();
        // Each image's bytes are staged as a coven file, in the order of
        // `library_image` then `artist_images`, which the rows then reference.
        let image_files: Vec<(&'static str, Vec<u8>)> = library_image
            .iter()
            .chain(artist_images.iter())
            .map(|(image, bytes)| (image.image_type.namespace(), bytes.clone()))
            .collect();
        // Each original is staged as a coven file, on this device only or for
        // the cloud as its place says, which its row references.
        let (rows_to_insert, originals): (Vec<NewReleaseFile>, Vec<_>) = files
            .into_iter()
            .map(|prepared| (prepared.row, (prepared.original, prepared.place)))
            .unzip();
        let records = rows.records.to_vec();
        let now_dt = self.store.now_utc();
        let now = now_dt.to_rfc3339();
        let labels = bae_records::LabelParts::new(ids, now_dt, &release.pressing.labels);
        self.store
            .write_staged(
                move |w| {
                    Ok(image_files
                        .into_iter()
                        .map(|(namespace, bytes)| w.put_file(namespace, bytes, coven::Place::Cloud))
                        .collect::<Vec<coven::FileId>>())
                },
                move |sql, image_file_ids| {
                    let original_file_ids = originals
                        .into_iter()
                        .map(|(original, place)| {
                            sql.put_user_file(
                                bae_records::synced::RELEASE_FILES_NAMESPACE,
                                original,
                                place,
                            )
                        })
                        .collect::<Result<Vec<coven::FileId>, _>>()?;
                    let tx = &sql;
                    require_import_commit_guard(tx, &guard)?;
                    end_import_attempt_of_commit(tx, &guard)?;

                    // Every credit is decided here, against the library as it
                    // stands inside this transaction, so an artist another
                    // import committed a moment ago is the one this import
                    // links to. New artists go in and linked ones are filled in
                    // before the links below point at them.
                    let credits =
                        ArtistCredits { credits: &artist_credits, picked: &picked_artists };
                    let resolved = credits.resolve_on(tx)?;
                    let inserted: BTreeSet<String> =
                        resolved.inserts.iter().map(|artist| artist.id.clone()).collect();
                    if inserted != expected_new_artists {
                        return Err(ImportCommitError::LibraryChanged);
                    }
                    resolved.write_on(tx)?;
                    let artist_ids = resolved.id_map(&artist_credits);
                    let album_artists = relink_album_artists(&album_artists, &artist_ids);
                    let track_artists = relink_track_artists(&track_artists, &artist_ids);
                    let release_artist_roles =
                        relink(&release_artist_roles, &artist_ids, "release artist role", |role| {
                            &mut role.artist_id
                        });
                    let track_artist_roles =
                        relink(&track_artist_roles, &artist_ids, "track artist role", |role| {
                            &mut role.artist_id
                        });
                    let work_artists = relink(&work_artists, &artist_ids, "work artist", |link| {
                        &mut link.artist_id
                    });

                    // A new album for a release group carries the group's id, so
                    // its row can be here already, emptied by an earlier delete
                    // or move: the import writes it whole either way.
                    if let Some(album) = &album {
                        upsert_album_row(tx, album)?;
                        replace_album_artists(tx, &album.id, &album_artists, &now)?;
                    }

                    insert_release_row(tx, &release, &label_write(&labels))?;
                    record_import_origin_on(tx, &guard, &release.id)?;
                    // The credits are resolved above, so the artist decisions
                    // this deletes have been read.
                    delete_what_the_import_spent_on(tx, &guard)?;

                    // One row per catalog that describes the release, empty when
                    // none does. The records are uniquely keyed on
                    // `(release_id, catalog)`, so a release never carries two
                    // rows for one catalog.
                    write_release_records(tx, &release.id, &records)?;

                    // Works are globally identified; they go in before their links.
                    for work in &works {
                        insert_work_row(tx, work)?;
                    }

                    for work_artist in &work_artists {
                        insert_work_artist_row(tx, work_artist)?;
                    }

                    for work_part in &work_parts {
                        insert_work_part_row(tx, work_part)?;
                    }

                    for role in &release_artist_roles {
                        insert_release_artist_role_row(tx, role)?;
                    }

                    for medium in &mediums {
                        insert_release_medium_row(tx, medium)?;
                    }

                    // The tracks came out of `tracks_to_files`; their `duration_ms`
                    // was set from the audio each is bound to: its CUE slice or its
                    // whole file.
                    for track in &tracks {
                        insert_track_row(tx, track)?;
                    }

                    for ta in &track_artists {
                        insert_track_artist_row(tx, ta)?;
                    }

                    for track_work in &track_works {
                        insert_track_work_row(tx, track_work)?;
                    }

                    for role in &track_artist_roles {
                        insert_track_artist_role_row(tx, role)?;
                    }

                    for (row, file) in rows_to_insert.iter().zip(original_file_ids) {
                        insert_release_file_row(tx, row, file)?;
                    }

                    for af in &audio_formats {
                        insert_audio_format_row(tx, af)?;
                    }
                    for segment in &audio_segments {
                        insert_audio_segment_row(tx, segment)?;
                    }
                    for points in &scrub_points {
                        insert_scrub_points_row(tx, points)?;
                    }

                    for ((image, _), file) in
                        library_image.iter().chain(artist_images.iter()).zip(&image_file_ids)
                    {
                        upsert_library_image_row(tx, image, file)?;
                    }

                    Ok(())
                },
            )
            .await
            .map(|_| ())
    }
}

impl ImportTables {
    /// Resolve `credits` against the library as it stands, without writing:
    /// what an import committing them now would link and create.
    pub(crate) async fn resolve_artists(
        &self,
        credits: &[DbArtist],
        picked: &[String],
    ) -> Result<
        bae_records::records::artist_resolution::ResolvedArtists,
        bae_records::IdentityWriteError,
    > {
        let (credits, picked) = (credits.to_vec(), picked.to_vec());
        bae_records::records::artist_resolution::read_resolving_identities(
            &self.store,
            move |sql| ArtistCredits { credits: &credits, picked: &picked }.resolve_on(&sql),
        )
        .await
    }
}

#[cfg(test)]
#[path = "import_commit_tests.rs"]
mod tests;
