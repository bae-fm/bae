//! The identify pipeline's shapes as an MCP client reads them: per-signal
//! progress, the failures a lookup can end on, and the state that carries them.
//!
//! Held apart from the rest of the automation types because they mirror one
//! bae-core projection (`identify::IdentifyStateView`) rather than the surface
//! an individual tool answers with.

use super::*;

/// Mirrors bae-core's `identify::LookupView` — how one provider's lookup of
/// one value is going. Mid-flight result payloads reduce to a count; the full
/// match set surfaces only in a terminal state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationLookupState {
    LookingUp,
    Found { count: u32 },
    NoMatch,
    Failed { failure: AutomationLookupFailure },
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
        source_file: Option<String>,
        lookup: AutomationLookupState,
    },
}

/// Mirrors bae-core's `identify::ArtworkStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationArtworkStep {
    Absent,
    Reading {
        current: Option<String>,
        position: u32,
        total: u32,
        barcodes: u32,
        catalogs: u32,
    },
    Read {
        images: u32,
        barcodes: u32,
        catalogs: u32,
    },
    Failed {
        failure: AutomationLookupFailure,
        read: u32,
        total: u32,
    },
}

/// Mirrors bae-core's `identify::BarcodeLookupView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationBarcodeLookupState {
    Trying {
        barcode: String,
        position: u32,
        total: u32,
    },
    Matched {
        barcode: Option<String>,
        count: u32,
    },
    Exhausted,
    Failed {
        failure: AutomationLookupFailure,
    },
}

/// Mirrors bae-core's `identify::ProviderBarcodeLookupView`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationProviderBarcodeLookup {
    pub source: AutomationMetadataSource,
    pub state: AutomationBarcodeLookupState,
}

/// Mirrors bae-core's `identify::BarcodeStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationBarcodeStep {
    AwaitingArtwork,
    Absent,
    NoCodes,
    ScanFailed {
        failure: AutomationLookupFailure,
    },
    Lookups {
        codes: Vec<String>,
        providers: Vec<AutomationProviderBarcodeLookup>,
    },
}

/// Mirrors bae-core's `identify::ProviderLookupView`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationProviderLookup {
    pub source: AutomationMetadataSource,
    pub state: AutomationLookupState,
}

/// Mirrors bae-core's `identify::CatalogStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationCatalogStep {
    NoneFound,
    Unchosen {
        available: u32,
    },
    Chosen {
        value: String,
        lookups: Vec<AutomationProviderLookup>,
    },
}

/// Mirrors bae-core's `identify::IdentifyRunView` — a run in flight as the
/// steps it is taking, each provider's part of each reported on its own.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationIdentifyRun {
    pub disc_id: AutomationDiscIdStep,
    pub artwork: AutomationArtworkStep,
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
    },
    Found {
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        track_count: u32,
        provenance: Vec<AutomationResultProvenance>,
    },
    NotFoundAnywhere,
    ManualOnly {
        track_count: u32,
    },
    /// A lookup failed, with whatever the surviving evidence still found: one
    /// provider failing leaves the other's matches standing. Empty groups mean
    /// nothing answered, or that the failure was resumed from its stored
    /// verdict.
    Failed {
        failures: Vec<AutomationIdentifyFailure>,
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        provenance: Vec<AutomationResultProvenance>,
    },
}
