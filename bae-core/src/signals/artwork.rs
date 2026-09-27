//! How far the artwork pass has got. Not part of [`Signals`](super::Signals),
//! since a stored snapshot has no pass to report on, so it rides beside the
//! snapshot on the `SignalsUpdated` event.

use super::LookupFailure;

/// The artwork pass over a candidate's images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtworkScan {
    /// Nothing to read: no images, or no analyzer on this platform.
    Absent,
    /// Reading the `position`th of `total`. `current` is the image's
    /// candidate-relative path, and `None` for a library release's image.
    Reading {
        current: Option<String>,
        position: u32,
        total: u32,
    },
    Done {
        total: u32,
    },
    /// The run does not read cover art, so what the images say is unknown
    /// rather than empty.
    Off,
    /// Reading stopped at a failure; `read` images had been read before it.
    Failed {
        failure: LookupFailure,
        read: u32,
        total: u32,
    },
}
