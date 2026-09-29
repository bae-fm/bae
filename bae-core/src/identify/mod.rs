//! The identify pipeline: matches a candidate's [`crate::signals::Signals`]
//! against external metadata. A pure reducer (`state::step`) runs the lookups
//! — disc ID, barcode, chosen catalog numbers, then the album title when those
//! name nothing — and `combine` ranks what they return. The service feeds it
//! extraction snapshots and lookup results and publishes each state as an
//! `ImportEvent::IdentifyStateChanged`. [`view`] shapes a state for surfaces,
//! [`verdict::TerminalVerdict`] is what a terminal state stores, and
//! [`verdict_summary`] reads a stored verdict back as what the queue needs.

pub mod agreements;
pub mod combine;
pub mod discid;
pub mod documents;
pub mod fit;
pub(crate) mod medium;
mod not_asked;
pub(crate) mod notes;
pub(crate) mod row_facts;
pub mod service;
pub mod state;
pub mod toolbar;
pub mod verdict;
pub mod verdict_summary;
pub mod view;

pub use agreements::{Agreements, CandidateText};
pub use combine::{Findings, LibraryStatuses, LookupAnswers, LookupProvenance, NarrowedOut};
pub use fit::{unattended_pick, Declined, TracklistFit, UnattendedPick};
pub use medium::MediumConflict;
pub use not_asked::NotAskedReason;
pub use service::{IdentifyRunId, IdentifyServiceHandle};
pub use state::{
    BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyEvent, IdentifyState, IsrcProgress,
    LookupOutcome, LookupResults, LookupState, ProviderLookup, SearchProgress, TitleSearch,
    ValueLookup,
};
pub use toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
pub use verdict::{IdentifyFailure, TerminalVerdict};
pub use verdict_summary::{FolderCheck, LeadMatch, VerdictKind, VerdictSummary};
pub use view::{
    BarcodeStepView, CatalogAgreementView, CatalogCandidateView, CatalogStepView, DiscIdStepView,
    IdentifyRunView, IdentifyStateView, IsrcStepView, LookupView, ProviderCell, RowAgreements,
    SearchStepView, SignalValueRow,
};

use crate::db::{LibraryCheck, LibraryStatus};
use crate::import::search::MetadataResult;
use crate::library::LibraryManager;

/// Pair each result with whether it's already in the library — the payload the
/// lookup-completion events carry.
pub(crate) async fn annotate_with_library_status(
    results: Vec<MetadataResult>,
    library_manager: &LibraryManager,
) -> Result<Vec<(MetadataResult, LibraryStatus)>, String> {
    let checks: Vec<LibraryCheck> = results.iter().map(LibraryCheck::from).collect();
    let statuses = library_manager
        .check_releases_in_library(&checks)
        .await
        .map_err(|e| format!("Failed to check library status: {e}"))?;
    Ok(results.into_iter().zip(statuses).collect())
}
