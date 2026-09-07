//! The identify pipeline's shapes as an MCP client reads them: per-signal
//! progress, the failures a lookup can end on, and the state that carries them.
//!
//! Held apart from the rest of the automation types because they mirror one
//! bae-core projection (`identify::IdentifyStateView`) rather than the surface
//! an individual tool answers with.

use super::*;

/// Mirrors bae-core's `identify::LookupView` — how one provider's lookup of
/// one value is going, one cell of the run's ledger.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationLookupState {
    /// Not asked yet: the provider's walk has not reached this code.
    Queued,
    /// Never asked: the provider's walk ended at an earlier code.
    NotAsked,
    LookingUp,
    Found {
        count: u32,
        groups: Vec<AutomationReleaseGroup>,
    },
    NoMatch,
    Failed {
        failure: AutomationLookupFailure,
    },
}

/// Mirrors bae-core's `identify::ValueSource` — one place a value was read.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationValueSource {
    pub origin: AutomationSignalOrigin,
    pub file: Option<String>,
    pub region: Option<AutomationImageRegion>,
}

/// Mirrors bae-core's `identify::ProviderCell`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationProviderCell {
    pub source: AutomationMetadataSource,
    pub lookup: AutomationLookupState,
}

/// Mirrors bae-core's `identify::SignalValueRow` — one value extraction
/// found, where it was found, and every provider's lookup of it.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationSignalValueRow {
    pub value: String,
    pub sources: Vec<AutomationValueSource>,
    pub cells: Vec<AutomationProviderCell>,
}

/// Mirrors bae-core's `identify::DiscIdFileKind`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationDiscIdFileKind {
    Log,
    Cue,
}

/// Mirrors bae-core's `identify::DiscIdFile`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationDiscIdFile {
    pub kind: AutomationDiscIdFileKind,
    pub file: String,
}

/// Mirrors bae-core's `identify::DiscIdStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationDiscIdStep {
    Reading,
    Absent,
    ReadFailed {
        failure: AutomationLookupFailure,
    },
    Read {
        disc_id: String,
        source: Option<AutomationDiscIdFile>,
        lookup: AutomationLookupState,
    },
    /// A disc ID was read and the source that answers disc IDs was not asked.
    ReadNotAsked {
        disc_id: String,
        source: Option<AutomationDiscIdFile>,
    },
}

/// Mirrors bae-core's `identify::BarcodeStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationBarcodeStep {
    Absent,
    NoCodes,
    ScanFailed {
        failure: AutomationLookupFailure,
    },
    Rows {
        scanning: bool,
        rows: Vec<AutomationSignalValueRow>,
    },
}

/// Mirrors bae-core's `identify::CatalogCandidateView` — a number offered
/// but not looked up.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationCatalogCandidate {
    pub value: String,
    pub sources: Vec<AutomationValueSource>,
}

/// Mirrors bae-core's `identify::CatalogStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationCatalogStep {
    NoneFound,
    Numbers {
        scanning: bool,
        rows: Vec<AutomationSignalValueRow>,
        candidates: Vec<AutomationCatalogCandidate>,
    },
}

/// Mirrors bae-core's `identify::IdentifyRunView` — the run as its ledger,
/// each provider's part of each signal reported on its own.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationIdentifyRun {
    pub providers: Vec<AutomationMetadataSource>,
    pub disc_id: AutomationDiscIdStep,
    pub barcode: AutomationBarcodeStep,
    pub catalog: AutomationCatalogStep,
}

/// Mirrors bae-core's `identify::ResultProvenance`, paired with the release id
/// it aligns to (the core type is index-aligned with the match list).
#[derive(Debug, Clone, Serialize)]
pub struct AutomationResultProvenance {
    pub release_id: String,
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub by_catalog: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationIdentifyFailure {
    DiscId {
        failure: AutomationLookupFailure,
    },
    /// Reading the candidate's barcodes failed, so no provider was asked.
    BarcodeScan {
        failure: AutomationLookupFailure,
    },
    Barcode {
        source: AutomationMetadataSource,
        failure: AutomationLookupFailure,
    },
    Catalog {
        source: AutomationMetadataSource,
        failure: AutomationLookupFailure,
    },
    ReleaseDetails {
        failure: AutomationLookupFailure,
    },
}

/// The releases the signals' agreement left out of a state's matches, shaped
/// as its matches are. Empty when the agreement narrowed nothing. Mirrors
/// `bae_core::identify::NarrowedOutView`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AutomationNarrowedOut {
    pub groups: Vec<AutomationReleaseGroup>,
    pub library_statuses: Vec<AutomationLibraryStatus>,
    pub provenance: Vec<AutomationResultProvenance>,
}

/// Projects bae-core's `identify::IdentifyState`. The `SignalsContext`
/// internals that drive core triangulation don't cross; terminal states carry
/// the full match data an MCP client acts on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationIdentifyState {
    Idle,
    /// Lookups in flight, with the matches the answered lookups have combined
    /// to so far, shaped as `Found`'s are.
    Triangulating {
        run: AutomationIdentifyRun,
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        provenance: Vec<AutomationResultProvenance>,
        narrowed_out: AutomationNarrowedOut,
    },
    /// A settled state carries the run it settled as; none when extraction
    /// handed the run nothing to lay out, or the verdict was stood back up
    /// from the store.
    Found {
        run: Option<AutomationIdentifyRun>,
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        track_count: u32,
        provenance: Vec<AutomationResultProvenance>,
        narrowed_out: AutomationNarrowedOut,
    },
    NotFoundAnywhere {
        run: Option<AutomationIdentifyRun>,
    },
    ManualOnly {
        track_count: u32,
        run: Option<AutomationIdentifyRun>,
    },
    /// A lookup failed, with whatever the surviving evidence still found: one
    /// provider failing leaves the other's matches standing. Empty groups mean
    /// nothing answered, or that the failure was resumed from its stored
    /// verdict.
    Failed {
        run: Option<AutomationIdentifyRun>,
        failures: Vec<AutomationIdentifyFailure>,
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        provenance: Vec<AutomationResultProvenance>,
        narrowed_out: AutomationNarrowedOut,
    },
}
