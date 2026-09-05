//! How far the artwork pass has got: the images are read one at a time, and
//! each one read is a step a surface can show while the barcode and text
//! signals are still accumulating. Not part of [`Signals`](super::Signals) —
//! a stored snapshot has no pass to report on — so it rides beside the
//! snapshot on the `SignalsUpdated` event.

use super::LookupFailure;

/// The artwork pass over a candidate's images. `Absent` when there is nothing
/// to read — no images, or no analyzer on this platform to read them with —
/// which is one fact to everything downstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtworkScan {
    Absent,
    /// Reading `current`, the `position`th of `total`. `current` is the
    /// image's candidate-relative path — the id its file row is keyed by —
    /// and `None` for a library release's stored cover, which is no file of
    /// a scanned folder.
    Reading {
        current: Option<String>,
        position: u32,
        total: u32,
    },
    /// Every image read.
    Done {
        total: u32,
    },
    /// Reading stopped at a failure; `read` images had been read before it.
    Failed {
        failure: LookupFailure,
        read: u32,
        total: u32,
    },
}
