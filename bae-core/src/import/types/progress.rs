#[cfg(not(any(target_os = "ios", target_os = "android")))]
use super::MetadataSource;

/// The storage state the user picks for an import. Every import FIRST lands
/// `Local` (files in place, playable immediately); a `Remote` import then
/// transitions to the cloud in the background.
///
/// Pinned-ness is NOT part of this state — it's coven cache state, never a bae
/// property. The user's pin choice rides the remote transition as a transient
/// argument (`pin` on the import command) telling coven whether to populate
/// `storage/pinned/`; it is never persisted.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    /// Files stay in place on this device; never uploaded.
    Local,
    /// Uploaded to the cloud home; `releases.remote` flips true once the upload
    /// lands.
    Remote,
}

/// User's cover art selection for an import.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverSelection {
    /// Remote cover to download (URL + source for attribution)
    Remote(String, MetadataSource),
    /// Local file in the album folder (relative path from album root)
    Local(String),
    /// Artwork embedded in one audio file, identified by that file's relative
    /// path in the File Tags snapshot.
    Embedded(String),
}

/// Progress updates during import
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub enum ImportProgress {
    /// A preparation step, before the running phases begin.
    Preparing {
        import_id: String,
        step: PrepareStep,
        album_title: String,
        artist_name: String,
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
        outbox_revision: u64,
    },
    Failed {
        error: String,
        import_id: String,
    },
}

/// The running phase of an import, after phase-0 preparation. Emitted as each
/// transition begins so the UI can name the work in progress. Every import is
/// local-in-place: the source files are read and hashed where they sit, then
/// each track is decoded to measure loudness, then the rows are written.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
/// ([`ImportPhase`]) begin.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareStep {
    Queued,
    ValidatingSourceFiles,
}

/// Which step of an import is in progress, for the candidate progress UI. The
/// UI localizes each step; bae-core no longer renders display text for it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStep {
    Preparing(PrepareStep),
    Running(ImportPhase),
}
