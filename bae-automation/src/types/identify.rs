//! The identify pipeline's shapes as an MCP client reads them, mirroring
//! bae-core's `identify::IdentifyStateView`.

use super::*;

/// Mirrors bae-core's `identify::NotAskedReason`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationNotAskedReason {
    /// The person left the value out of the run.
    LeftOut,
    /// The step is switched off in the identification settings.
    SwitchedOff,
    /// No catalog the run asks answers this lookup.
    NoCatalog,
}

/// Mirrors bae-core's `identify::LookupView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationLookupState {
    /// Not asked yet: the codes are still being read off the artwork.
    Queued,
    NotAsked {
        reason: AutomationNotAskedReason,
    },
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

/// Mirrors bae-core's `identify::ProviderCell`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationProviderCell {
    pub source: AutomationCatalog,
    pub lookup: AutomationLookupState,
}

/// Mirrors bae-core's `identify::SignalValueRow`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationSignalValueRow {
    pub value: String,
    /// Whether the person left this value out of the run.
    pub excluded: bool,
    pub cells: Vec<AutomationProviderCell>,
}

/// Mirrors bae-core's `identify::DiscIdStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationDiscIdStep {
    Reading,
    Absent,
    /// A CUE over audio sampled at a rate no CD plays at.
    NotCdAudio {
        sample_rate_hz: u32,
    },
    ReadFailed {
        failure: AutomationLookupFailure,
    },
    Read {
        disc_id: String,
        lookup: AutomationLookupState,
    },
}

/// Mirrors bae-core's `identify::BarcodeStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationBarcodeStep {
    Absent,
    /// The cover art was left unread and no CUE sheet states a code.
    CoverArtOff,
    NoCodes,
    ScanFailed {
        failure: AutomationLookupFailure,
    },
    Rows {
        scanning: bool,
        rows: Vec<AutomationSignalValueRow>,
    },
}

/// Mirrors bae-core's `identify::CatalogCandidateView`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationCatalogCandidate {
    pub value: String,
}

/// Mirrors bae-core's `identify::CatalogStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationCatalogStep {
    NoneFound,
    /// No catalog number in the folder's own text, and the cover art was left
    /// unread.
    CoverArtOff,
    Numbers {
        scanning: bool,
        rows: Vec<AutomationSignalValueRow>,
        candidates: Vec<AutomationCatalogCandidate>,
    },
}

/// Mirrors bae-core's `identify::CatalogAgreementView`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationCatalogAgreement {
    pub value: String,
    pub discounted: bool,
}

/// Mirrors bae-core's `identify::SearchStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationSearchStep {
    NotAsked {
        reason: AutomationNotAskedReason,
    },
    NotNeeded,
    NoTitle,
    Waiting {
        album: String,
        artist: String,
    },
    Searched {
        album: String,
        artist: String,
        cells: Vec<AutomationProviderCell>,
    },
}

/// Mirrors bae-core's `identify::IdentifyRunView`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationIdentifyRun {
    pub providers: Vec<AutomationCatalog>,
    pub disc_id: AutomationDiscIdStep,
    pub barcode: AutomationBarcodeStep,
    pub catalog: AutomationCatalogStep,
    pub search: AutomationSearchStep,
}

/// Mirrors bae-core's `identify::Agreements`, with the release id it belongs
/// to.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationAgreements {
    pub release_id: String,
    pub disc_id: bool,
    pub barcode: bool,
    pub catalog: bool,
    pub label: bool,
    pub year: bool,
    pub country: bool,
    pub title: bool,
    pub artist: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationIdentifyFailure {
    DiscId {
        failure: AutomationLookupFailure,
    },
    /// Reading the candidate's barcodes failed.
    BarcodeScan {
        failure: AutomationLookupFailure,
    },
    Barcode {
        source: AutomationCatalog,
        failure: AutomationLookupFailure,
    },
    Catalog {
        source: AutomationCatalog,
        failure: AutomationLookupFailure,
    },
    Search {
        source: AutomationCatalog,
        failure: AutomationLookupFailure,
    },
    ReleaseDetails {
        failure: AutomationLookupFailure,
    },
}

/// Mirrors `bae_core::identify::NarrowedOutView`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AutomationNarrowedOut {
    /// The cards none of whose rows is offered.
    pub groups: Vec<AutomationReleaseGroup>,
    /// How many rows were set aside, on every card.
    pub count: u32,
}

/// Mirrors bae-core's `identify::IdentifyStateView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationIdentifyState {
    Idle,
    /// Lookups in flight, with the matches the answers so far combine to.
    Triangulating {
        run: AutomationIdentifyRun,
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        agreements: Vec<AutomationAgreements>,
        narrowed_out: AutomationNarrowedOut,
    },
    /// A settled state carries the run it settled as, or none when there was
    /// nothing to lay out.
    Found {
        run: Option<AutomationIdentifyRun>,
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        track_count: u32,
        agreements: Vec<AutomationAgreements>,
        narrowed_out: AutomationNarrowedOut,
        catalog_agreements: Vec<AutomationCatalogAgreement>,
    },
    NotFoundAnywhere {
        run: Option<AutomationIdentifyRun>,
    },
    ManualOnly {
        track_count: u32,
        run: Option<AutomationIdentifyRun>,
    },
    /// A lookup failed, with whatever the lookups that answered still found.
    Failed {
        run: Option<AutomationIdentifyRun>,
        failures: Vec<AutomationIdentifyFailure>,
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        agreements: Vec<AutomationAgreements>,
        narrowed_out: AutomationNarrowedOut,
        catalog_agreements: Vec<AutomationCatalogAgreement>,
    },
}
