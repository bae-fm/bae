//! The identify pipeline. Consumes the [`crate::signals::Signals`] the extraction
//! service produces for a candidate and drives a state machine that matches it
//! against external metadata. The disc-ID and barcode signals are looked up in
//! parallel — triangulation — and once they settle, `combine` intersects their
//! results into a terminal `Found` or `NotFoundAnywhere` carrying per-result
//! provenance. A catalog number the user picks out of the ones extracted is a
//! third lookup, and joins the same intersection.
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
//!
//! [`verdict::TerminalVerdict`] is the third projection: what a *terminal*
//! state (`Found`, `NotFoundAnywhere`, `ManualOnly`, `Failed`) persists so it
//! need not be re-fetched on the next launch.
//! [`ready`] reads that stored verdict back and says what the queue needs from
//! the user for that candidate — derived on every read, never stored.

pub mod barcode;
pub mod catalog;
pub mod combine;
pub mod discid;
pub mod ready;
pub mod service;
pub mod state;
pub mod toolbar;
pub mod verdict;
pub mod view;

pub use combine::ResultProvenance;
pub use ready::{
    classify, classify_summary, LeadMatch, NeedsYou, QueueClassification, VerdictKind,
    VerdictSummary,
};
pub use service::{IdentifyRunId, IdentifyServiceHandle};
pub use state::{
    BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyEvent, IdentifyState, SignalToggle,
};
pub use toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
pub use verdict::{IdentifyFailure, TerminalVerdict};
pub use view::{BarcodeProgressView, DiscidProgressView, IdentifyStateView};

use crate::db::{LibraryCheck, LibraryStatus};
use crate::import::search::{MetadataResult, ProviderLookups, SourceFailure};
use crate::import::MetadataSource;
use crate::library::LibraryManager;
use crate::signals::LookupFailure;

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

/// Pair every provider's results with live library status, keeping the
/// providers apart: what one source found stands whether or not the other
/// answered, and each one that did not is named.
///
/// The library check is bae's own work rather than the provider's, so it is run
/// per source: a check that fails over one source's results leaves the other
/// source's results intact and names only the source whose part of the lookup
/// produced nothing usable.
async fn annotate_lookups(
    lookups: ProviderLookups,
    library_manager: &LibraryManager,
) -> (Vec<(MetadataResult, LibraryStatus)>, Vec<SourceFailure>) {
    let mut failures = lookups.failures();
    let answered = [
        (MetadataSource::MusicBrainz, lookups.musicbrainz.ok()),
        (
            MetadataSource::Discogs,
            lookups.discogs.and_then(Result::ok),
        ),
    ];
    let mut annotated = Vec::new();
    for (source, results) in answered {
        let Some(results) = results else { continue };
        if results.is_empty() {
            continue;
        }
        match annotate_with_library_status(results, library_manager).await {
            Ok(results) => annotated.extend(results),
            Err(detail) => failures.push(SourceFailure {
                source,
                failure: LookupFailure::Diagnostic { detail },
            }),
        }
    }
    (annotated, failures)
}
