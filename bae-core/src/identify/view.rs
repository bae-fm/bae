//! The identify state, shaped for the surfaces that render it.
//!
//! [`IdentifyState`] is the reducer's working shape. It carries the whole
//! [`SignalsContext`](super::state::SignalsContext) through every state so a
//! toggle or a re-run can re-combine without re-fetching, and it keeps
//! `matches`, `library_statuses` and `provenance` as three index-aligned
//! vectors because that is what `combine` hands it.
//!
//! No surface wants that shape, and every surface wants the same *other* shape:
//! the matches folded into their release-group cards, each result paired with its
//! library status, provenance keyed by release id, an in-flight lookup's result
//! payload reduced to a count, and the context's raw inputs — the signal values,
//! the user's exclusions — left behind. Those are domain decisions, so they are
//! made here, once, and a field that must not cross is simply absent from the
//! type.
//!
//! The transports (`bae-bridge`'s uniffi records, `bae-automation`'s JSON) mirror
//! this view into their own wire types field by field and decide nothing.

use super::combine::ResultProvenance;
use super::state::{BarcodeProgress, DiscidProgress, IdentifyState};
use crate::db::LibraryStatus;
use crate::import::release_group::ReleaseGroup;
use crate::signals::LookupFailure;

/// The disc-ID lookup's progress while triangulating. The results themselves are
/// not here: mid-flight, a surface shows only how many came back ("Disc ID: 3
/// matches") — the set itself surfaces from the terminal state.
#[derive(Debug, Clone)]
pub enum DiscidProgressView {
    Computing,
    LookingUp,
    Done { n_results: u32 },
    Skipped,
    Failed { failure: LookupFailure },
}

/// The barcode lookup's progress while triangulating. `LookingUp` carries the
/// position in the queue so a surface can render "Looking up barcode 2 of 3"; the
/// codes still queued behind it are the reducer's business, not the surface's.
#[derive(Debug, Clone)]
pub enum BarcodeProgressView {
    Scanning,
    LookingUp {
        current: String,
        position: u32,
        total: u32,
    },
    Done {
        n_results: u32,
    },
    Failed {
        failure: LookupFailure,
    },
    Skipped,
}

/// One candidate's identify state as a surface renders it.
#[derive(Debug, Clone)]
pub enum IdentifyStateView {
    Idle,

    /// Both lookups in flight, each with its own progress.
    Triangulating {
        discid: DiscidProgressView,
        barcode: BarcodeProgressView,
    },

    /// The matches, bucketed into their release groups — one card per group,
    /// with its pressings beneath. Usually one card; signals that named
    /// different releases give several, which is the same list of things to
    /// pick from either way.
    Found {
        /// The match list, folded into group cards in match order.
        groups: Vec<ReleaseGroup>,
        /// One per pressing; each carries its own `release_id`.
        library_statuses: Vec<LibraryStatus>,
        track_count: u32,
        /// Per-pressing provenance, keyed by release id. `combine` produces it
        /// index-aligned with the matches, and the matches are now inside the
        /// group cards, so the alignment is re-expressed as a key here rather
        /// than left for a surface to reconstruct.
        provenance: Vec<(String, ResultProvenance)>,
    },

    NotFoundAnywhere,

    /// No disc-ID artifact and no barcode source: nothing ran, so a surface
    /// offers manual search rather than claiming it looked and found nothing.
    ManualOnly {
        track_count: u32,
    },

    Failed {
        failures: Vec<super::IdentifyFailure>,
    },
}

impl From<IdentifyState> for IdentifyStateView {
    fn from(state: IdentifyState) -> Self {
        match state {
            IdentifyState::Idle => IdentifyStateView::Idle,

            // The context rides along for the reducer's benefit; the toolbar
            // projection (`IdentifyState::toolbar`) is what surfaces its values.
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: _,
                context: _,
            } => IdentifyStateView::Triangulating {
                discid: discid.into(),
                barcode: barcode.into(),
            },

            IdentifyState::Found {
                matches,
                library_statuses,
                track_count,
                provenance,
                context: _,
            } => {
                // `provenance` is index-aligned with `matches`. Key it by
                // release id before the matches disappear into the group cards
                // — after the fold, index alignment is no longer expressible.
                let provenance = matches
                    .iter()
                    .map(|m| m.release_id.clone())
                    .zip(provenance)
                    .collect();
                IdentifyStateView::Found {
                    groups: crate::import::release_group::group_results(matches),
                    library_statuses,
                    track_count,
                    provenance,
                }
            }

            IdentifyState::NotFoundAnywhere { context: _ } => IdentifyStateView::NotFoundAnywhere,

            IdentifyState::ManualOnly {
                track_count,
                context: _,
            } => IdentifyStateView::ManualOnly { track_count },

            IdentifyState::Failed {
                failures,
                track_count: _,
                context: _,
            } => IdentifyStateView::Failed { failures },
        }
    }
}

impl From<DiscidProgress> for DiscidProgressView {
    fn from(progress: DiscidProgress) -> Self {
        match progress {
            DiscidProgress::Computing => DiscidProgressView::Computing,
            DiscidProgress::LookingUp => DiscidProgressView::LookingUp,
            // The track count is a settled-state concern — it reaches a surface
            // through the terminal state, not through progress.
            DiscidProgress::Done {
                results,
                track_count: _,
            } => DiscidProgressView::Done {
                n_results: results.len() as u32,
            },
            DiscidProgress::Skipped { track_count: _ } => DiscidProgressView::Skipped,
            DiscidProgress::Failed {
                failure,
                track_count: _,
            } => DiscidProgressView::Failed { failure },
        }
    }
}

impl From<BarcodeProgress> for BarcodeProgressView {
    fn from(progress: BarcodeProgress) -> Self {
        match progress {
            BarcodeProgress::Scanning => BarcodeProgressView::Scanning,
            BarcodeProgress::LookingUp {
                current,
                position,
                total,
                // The queue behind the current code is the reducer's; a surface
                // renders the position, not the backlog.
                remaining: _,
            } => BarcodeProgressView::LookingUp {
                current,
                position,
                total,
            },
            // Which code matched is named by the terminal state's
            // `matched_barcode`, so progress only reports the count.
            BarcodeProgress::Done {
                matched: _,
                results,
            } => BarcodeProgressView::Done {
                n_results: results.len() as u32,
            },
            BarcodeProgress::Failed { failure } => BarcodeProgressView::Failed { failure },
            BarcodeProgress::Skipped => BarcodeProgressView::Skipped,
        }
    }
}
