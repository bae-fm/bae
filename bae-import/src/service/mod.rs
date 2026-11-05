//! The import service: the worker that imports a candidate's release into
//! the library, one import at a time, and its start, which also starts the
//! folder-watch coordinator (`crate::watching`).

use crate::candidate_runtime::CandidateRuntime;
use crate::handle::ImportServiceHandle;
use crate::types::{ImportCommand, ImportDestination, ImportProgress};
use bae_diagnostics::TelemetryEvent;

use {
    crate::types::{ImportPhase, PrepareStep, TrackFile},
    bae_db::WorkGraphRows,
    bae_db::{
        DbAlbum, DbAlbumArtist, DbRelease, DbReleaseArtistRole, DbTrackArtist, DbTrackArtistRole,
        NewReleaseFile,
    },
    std::collections::HashMap,
    std::path::{Path, PathBuf},
    std::sync::Arc,
};

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

pub(crate) mod artist_pictures;
mod content_type;
mod cover_image;
mod file_identity;
mod format_prep;
pub use format_prep::{build_audio_formats, BuiltAudioFormats};
mod importing;
mod progress;
mod reconcile;
mod track_files;
mod worker;

use content_type::resolve_file_content_type;
use track_files::resolve_track_files;

/// Which import run a progress event is about: the run the pane is watching and
/// the candidate row it redraws.
#[derive(Clone, Copy)]
pub(super) struct ImportRun<'a> {
    pub(super) import_id: &'a str,
    pub(super) candidate_key: &'a crate::CandidateKey,
}

/// The files one import writes and the track rows bound to their audio, both
/// read from the same folder.
#[derive(Clone, Copy)]
pub(super) struct ImportFiles<'a> {
    pub(super) discovered: &'a [crate::folder_scanner::CandidateFile],
    pub(super) tracks: &'a [TrackFile],
}

/// The release's rows from `reconcile_prepared_release`. Every artist link
/// names one of `artist_credits`; the commit resolves those to library artists
/// in its own transaction.
struct PreparedMetadata {
    db_album: DbAlbum,
    db_release: DbRelease,
    /// The release's mediums, in order.
    mediums: Vec<bae_db::DbReleaseMedium>,
    /// The release's tracks, each bound to its audio and measured by the
    /// commit (`resolve_track_files`).
    track_rows: Vec<bae_db::TrackRow>,
    existing_album_id: Option<String>,
    track_artists: Vec<DbTrackArtist>,
    /// Empty when the release joins an album already in the library.
    album_artists: Vec<DbAlbumArtist>,
    /// Works resolved; `work_artists` still name their artist by credit.
    work_graph: WorkGraphRows,
    release_artist_roles: Vec<DbReleaseArtistRole>,
    track_artist_roles: Vec<DbTrackArtistRole>,
    /// Every artist the links above name, as the source or the person said.
    artist_credits: Vec<bae_db::DbArtist>,
    /// The credits that are library artists a person picked.
    picked_artists: Vec<String>,
    /// One per catalog that describes this release; empty for file metadata and
    /// direct entry.
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    album_title: String,
}

/// A release-file row and its original, read whole once, with where its
/// destination stages it: on this device only, or for the cloud, kept whole
/// here too once stored when the import asks for that. coven records the
/// original on this device either way, with the hashes its read found.
pub(crate) struct PreparedImportFile {
    pub(crate) row: NewReleaseFile,
    pub(crate) original: coven::PreparedUserFile,
    pub(crate) place: coven::Place,
}

fn destination_label(destination: ImportDestination) -> &'static str {
    match destination {
        ImportDestination::Remote { .. } => "remote",
        ImportDestination::Local => "local",
    }
}

/// What the import worker thread receives. `Shutdown` is sent rather than
/// closing the channel because the last sender lives in the struct whose `Drop`
/// joins the thread, so the channel could never close first.
pub(crate) enum ImportWorkerMessage {
    Import { command: ImportCommand, expectation: ImportExpectation },
    Shutdown,
}

/// The candidate an import commits, as the commit checks it.
pub(super) struct CommittedCandidate {
    pub(super) source: crate::release_candidate::CandidateSource,
    pub(super) expectation: ImportExpectation,
}

#[derive(Debug, Clone)]
pub(crate) struct ImportExpectation {
    pub(crate) candidate: crate::CandidateAsRead,
    pub(crate) file_tag_snapshot: Option<crate::file_tag_snapshot::FileTagSnapshot>,
}

impl ImportExpectation {
    /// Whether `current` is still the candidate this import was prepared from.
    /// Asked inside the library write's transaction, so the answer holds
    /// through the commit.
    pub(crate) fn verify(
        &self,
        candidate_key: &crate::CandidateKey,
        source: &crate::release_candidate::CandidateSource,
        current: Option<&crate::tables::CommittingCandidate>,
    ) -> Result<(), String> {
        let Some(current) = current else {
            return Err(format!("{candidate_key} is no longer a valid import candidate"));
        };
        if !current.actionable
            || current.source != *source
            || current.content_hash != self.candidate.content_hash
            || current.file_edit_revision != self.candidate.file_edit_revision
        {
            return Err(format!("{candidate_key} changed before its import committed"));
        }
        if current.prepared_revisions
            != Some((self.candidate.file_edit_revision, self.candidate.metadata_revision))
        {
            return Err(format!(
                "{candidate_key}'s prepared metadata changed before its import committed"
            ));
        }
        if let Some(snapshot) = &self.file_tag_snapshot {
            if !current
                .file_tag_snapshot
                .as_ref()
                .is_some_and(|current| current.same_reading(snapshot))
            {
                return Err(format!(
                    "{candidate_key}'s file-tag reading changed before its import committed"
                ));
            }
        }
        Ok(())
    }
}

pub struct ImportService {
    commands_rx: mpsc::UnboundedReceiver<ImportWorkerMessage>,
    event_tx: crate::ImportEventBus,
    /// What the bus's events have said about every candidate, held here for
    /// the standing a commit ends.
    runtime: CandidateRuntime,
    /// The library's store, which an import writes its release into.
    database: bae_db::Database,
    /// Import's tables, which an import reads its candidate from.
    tables: crate::tables::ImportTables,
    /// The catalogs an import asks for its new artists' pictures.
    catalogs: crate::CatalogSessions,
    /// Where those pictures are fetched through.
    remote_images: bae_catalogs::remote_image::RemoteImageCache,
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
    import_cancels: crate::import_cancel::ImportCancels,
    /// Where every bug the import worker finds is reported: a write no
    /// caller waits on, and an image the blocking format prep can't read.
    diagnostics: bae_diagnostics::Diagnostics,
}

/// The records a pick commits: the primary's and every partner's, each
/// release's own record outranking what another says about its catalog.
pub fn records_for_commit(
    primary: &bae_catalogs::source_release::SourceRelease,
    partners: &[bae_catalogs::source_release::SourceRelease],
) -> Vec<bae_catalogs::records::ReleaseRecord> {
    let claimed: Vec<_> = std::iter::once(primary)
        .chain(partners)
        .map(bae_catalogs::source_release::SourceRelease::records)
        .collect();
    bae_catalogs::source_release::claimed_records(&claimed)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
pub(crate) mod tests;
