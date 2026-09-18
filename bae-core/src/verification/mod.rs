//! Verifying a rip by computing what the databases key on, rather than by
//! reading what a ripper wrote down.
//!
//! A folder with no log, or a log from a ripper that checked nothing, still
//! carries everything needed: the disc's table of contents and the audio
//! itself. From those this module computes AccurateRip's three disc ids and
//! per-track checksums, the CDDB disc id, and the CUETools database's TOCID and
//! CRC-32s — the same numbers those databases hold, so a lookup can compare
//! them. Nothing here touches the network.
//!
//! The formulas come from the rippers and verifiers that already implement them
//! (whipper, ARver, CUETools); the tests pin each one to a published vector or
//! to a fixture log that prints the answer.

mod accuraterip;
mod cddb;
mod ctdb;
mod samples;
#[cfg(test)]
mod test_discs;
mod toc;

pub use accuraterip::{
    crc450, find_pressing_offset, track_checksums, AccurateRipIds, TrackChecksums,
    MAX_PRESSING_OFFSET,
};
pub use cddb::cddb_disc_id;
pub use ctdb::{disc_crc32, toc_string, tocid, track_crc32};
pub use samples::DiscSamples;
pub use toc::{DiscToc, TocTrack};

/// Stereo samples in one CD sector.
pub const SAMPLES_PER_SECTOR: usize = 588;

/// Where a track sits among the disc's audio tracks.
///
/// Both databases leave samples out at the very start of the disc's audio and
/// at its very end, because a drive's read offset makes exactly those bytes
/// differ between two correct rips of the same disc. A one-track disc is both
/// ends at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackEdges {
    pub first: bool,
    pub last: bool,
}

impl TrackEdges {
    /// Neither end of the disc — the shape every track between the first and
    /// last is checksummed with.
    pub const MIDDLE: Self = Self {
        first: false,
        last: false,
    };
}

/// Why a rip cannot be verified by computation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerificationError {
    #[error("no table of contents to verify against: {detail}")]
    InvalidToc { detail: String },
    #[error("nothing to verify: the rip names no audio files")]
    NoAudio,
    #[error("cannot read {path}: {detail}")]
    Read { path: String, detail: String },
    #[error("cannot decode {path}: {detail}")]
    Decode { path: String, detail: String },
    #[error("{path} is not CD audio: {detail}")]
    NotCdAudio { path: String, detail: String },
}
