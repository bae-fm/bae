//! The identify pipeline. Consumes the [`crate::signals::Signals`] the extraction
//! service produces for a candidate and drives a state machine that matches it
//! against external metadata. The disc-ID and barcode signals are looked up in
//! parallel — triangulation — and once both settle, `combine` reconciles their
//! results into a terminal `Found`, `Conflict`, or `NotFoundAnywhere` carrying
//! per-result provenance.
//!
//! The state machine is a pure reducer (`state::step`). The service
//! (`service::IdentifyServiceHandle`) relays `SignalsUpdated` into it, runs the
//! side effects it asks for (the MB/Discogs lookups), and feeds the results back.
//! Scanning, OCR, and disc-ID derivation belong to `crate::signals`, not here.
//!
//! Every state transition goes out on the `ImportEvent` broadcast channel as one
//! `ImportEvent::IdentifyStateChanged` carrying the full state.
//!
//! Surfaces don't read that state directly. [`view::IdentifyStateView`] shapes it
//! for rendering — folding the matches into their group card, keying provenance by
//! release id, dropping what must not cross — once, so every transport mirrors the
//! same decisions instead of re-making them.

pub mod barcode;
pub mod combine;
pub mod discid;
pub mod service;
pub mod state;
pub mod toolbar;
pub mod view;

pub use combine::{GroupKey, ResultProvenance};
pub use service::IdentifyServiceHandle;
pub use state::{
    BarcodeProgress, DiscidProgress, ExcludedSignal, IdentifyEvent, IdentifySource, IdentifyState,
};
pub use toolbar::{SignalKind, SignalRole, SignalState, ToolbarSignal};
pub use view::{BarcodeProgressView, DiscidProgressView, IdentifyStateView, ResultRow};

use crate::db::{LibraryCheck, LibraryStatus};
use crate::import::search::MetadataResult;
use crate::library::LibraryManager;

/// Pair each result with whether it's already in the library — the payload the
/// lookup-completion events carry.
async fn annotate_with_library_status(
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
