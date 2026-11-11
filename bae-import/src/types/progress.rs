//! An import's progress.
//!
//! Every import follows the same flow, whether the tracks are individual files
//! or one container plus a CUE sheet, and whether the metadata came from
//! MusicBrainz, Discogs, the files' own tags, or direct entry:
//!
//! 1. **Preparation** ([`PrepareStep`]) — validate that the stored candidate's
//!    source files still have the identities recorded by the scan.
//! 2. **Running** ([`ImportPhase`]) — read and hash each file where it already
//!    sits (no bytes move, no transcode), measure per-track loudness by decoding,
//!    and write every row in one transaction.
//!
//! An import always lands as a local, playable release. An
//! import to the cloud
//! ([`ImportDestination::Cloud`](super::ImportDestination::Cloud))
//! then uploads to the cloud in the background.

/// Progress updates during import
#[cfg(desktop)]
#[derive(Debug, Clone)]
pub enum ImportProgress {
    /// A preparation step, before the running phases begin.
    Preparing {
        import_id: String,
        step: PrepareStep,
    },
    Progress {
        id: String,
        /// `None` while a phase has begun but has no measurable fraction yet.
        percent: Option<u8>,
        /// Which running phase this progress belongs to. The phases run in order:
        /// read and register the files in place, measure loudness, finalize.
        phase: ImportPhase,
        import_id: String,
    },
    Complete {
        id: String,
        import_id: String,
        album_id: String,
    },
    RemoteUploadQueued {
        id: String,
        import_id: String,
        album_id: String,
    },
    Failed {
        error: String,
        import_id: String,
    },
    /// A person cancelled the import before it wrote anything. The candidate
    /// is as it was before the import was asked for.
    Cancelled {
        import_id: String,
    },
}

#[cfg(desktop)]
impl ImportProgress {
    /// The import this reports on.
    pub fn import_id(&self) -> &str {
        match self {
            Self::Preparing { import_id, .. }
            | Self::Progress { import_id, .. }
            | Self::Complete { import_id, .. }
            | Self::RemoteUploadQueued { import_id, .. }
            | Self::Failed { import_id, .. }
            | Self::Cancelled { import_id } => import_id,
        }
    }
}

/// The running phase of an import, after phase-0 preparation. Emitted as each
/// transition begins so the UI can name the work in progress. Every import is
/// local-in-place: the source files are read and hashed where they sit, then
/// each track is decoded to measure loudness, then the rows are written.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportPhase {
    /// Reading and hashing each source file before it is registered. Per-file
    /// progress fills the percent.
    ReadingFiles,
    /// Decoding each track to measure its loudness and true peak. Frames
    /// measured fill the percent, on whole-percent moves.
    MeasuringLoudness,
    /// Writing the album/release/track rows and committing the import.
    Finalizing,
}

/// Preparation steps, emitted by the import worker before the running phases
/// ([`ImportPhase`]) begin, in this order.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareStep {
    /// Claimed, waiting for the worker.
    Queued,
    /// Checking every file is the one the scan saw.
    ValidatingSourceFiles,
    /// Asking Discogs for the pictures of the artists the import creates.
    /// Reported only by an import that creates one.
    FetchingArtistPictures,
}

/// Which step of an import is in progress, for the candidate progress UI. The
/// UI localizes each step; bae-library no longer renders display text for it.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStep {
    Preparing(PrepareStep),
    Running(ImportPhase),
}
