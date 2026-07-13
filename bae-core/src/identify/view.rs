//! The identify state, shaped for the surfaces that render it.
//!
//! [`IdentifyState`] is the reducer's working shape. It carries the whole
//! [`SignalsContext`] through every state so a toggle or a re-run can re-combine
//! without re-fetching, and it keeps `matches`, `library_statuses` and
//! `provenance` as three index-aligned vectors because that is what `combine`
//! hands it.
//!
//! No surface wants that shape, and every surface wants the same *other* shape:
//! the matches folded into their release-group card, each result paired with its
//! library status, provenance keyed by release id, an in-flight lookup's result
//! payload reduced to a count, and the context's raw inputs — the signal values,
//! the user's exclusions — left behind. Those are domain decisions, so they are
//! made here, once, and a field that must not cross is simply absent from the
//! type.
//!
//! The transports (`bae-bridge`'s uniffi records, `bae-automation`'s JSON) mirror
//! this view into their own wire types field by field and decide nothing.

use super::combine::ResultProvenance;
use super::state::{BarcodeProgress, DiscidProgress, IdentifyState, SignalsContext};
use crate::db::LibraryStatus;
use crate::import::release_group::ReleaseGroup;
use crate::import::search::MetadataResult;
use crate::signals::LookupFailure;

/// One release a signal turned up, together with whether the library already
/// holds it. The two travel as a pair from the lookup onwards; pairing them in
/// the type is what stops a surface from re-associating them by index and
/// getting it wrong.
#[derive(Debug, Clone)]
pub struct ResultRow {
    pub result: MetadataResult,
    pub status: LibraryStatus,
}

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

    /// Every match shares one release group, so they render as one card with the
    /// pressings beneath it.
    Found {
        /// The matches, folded into their group card. `group.pressings` is the
        /// match list, in match order.
        group: ReleaseGroup,
        /// One per pressing; each carries its own `release_id`.
        library_statuses: Vec<LibraryStatus>,
        track_count: u32,
        /// Per-pressing provenance, keyed by release id. `combine` produces it
        /// index-aligned with the matches, and the matches are now inside
        /// `group`, so the alignment is re-expressed as a key here rather than
        /// left for a surface to reconstruct.
        provenance: Vec<(String, ResultProvenance)>,
    },

    /// The signals disagreed. Each one's own results are shown side by side so
    /// the user can pick a side, drop a signal, or search manually.
    Conflict {
        discid_results: Vec<ResultRow>,
        barcode_results: Vec<ResultRow>,
        /// The barcode value that produced `barcode_results`, for the barcode
        /// section's header. `None` when nothing matched.
        matched_barcode: Option<String>,
        track_count: u32,
    },

    NotFoundAnywhere,

    /// No disc-ID artifact and no barcode source: nothing ran, so a surface
    /// offers manual search rather than claiming it looked and found nothing.
    ManualOnly {
        track_count: u32,
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
                context: _,
            } => IdentifyStateView::Triangulating {
                discid: discid.into(),
                barcode: barcode.into(),
            },

            IdentifyState::Found {
                matches,
                library_statuses,
                track_count,
                group,
                provenance,
                context: _,
            } => {
                // `matches` all share `group` and `provenance` is index-aligned
                // with them. Key the provenance by release id before the matches
                // disappear into the group card — after the fold, index alignment
                // is no longer expressible.
                let provenance = matches
                    .iter()
                    .map(|m| m.release_id.clone())
                    .zip(provenance)
                    .collect();
                IdentifyStateView::Found {
                    group: ReleaseGroup::from_group(group, matches),
                    library_statuses,
                    track_count,
                    provenance,
                }
            }

            IdentifyState::Conflict { context } => {
                let SignalsContext {
                    discid_results,
                    barcode_results,
                    matched_barcode,
                    track_count,
                    // The raw signal inputs (`disc_id`, `barcode_codes`,
                    // `catalogs`), the user's `excluded` toggles, and the settled
                    // `barcode_failure` drive triangulation in the reducer. A
                    // surface renders the two settled result sets and the matched
                    // barcode; none of the rest reaches it.
                    disc_id: _,
                    barcode_codes: _,
                    had_barcode_source: _,
                    catalogs: _,
                    excluded: _,
                    barcode_failure: _,
                } = context;
                IdentifyStateView::Conflict {
                    discid_results: result_rows(discid_results),
                    barcode_results: result_rows(barcode_results),
                    matched_barcode,
                    track_count,
                }
            }

            IdentifyState::NotFoundAnywhere { context: _ } => IdentifyStateView::NotFoundAnywhere,

            IdentifyState::ManualOnly {
                track_count,
                context: _,
            } => IdentifyStateView::ManualOnly { track_count },
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

fn result_rows(pairs: Vec<(MetadataResult, LibraryStatus)>) -> Vec<ResultRow> {
    pairs
        .into_iter()
        .map(|(result, status)| ResultRow { result, status })
        .collect()
}
