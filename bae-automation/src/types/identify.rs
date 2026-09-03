//! The identify pipeline's shapes as an MCP client reads them: per-signal
//! progress, the failures a lookup can end on, and the state that carries them.
//!
//! Held apart from the rest of the automation types because they mirror one
//! bae-core projection (`identify::IdentifyStateView`) rather than the surface
//! an individual tool answers with.

use super::*;

/// Projects bae-core's `identify::DiscidProgress` — mid-flight result payloads
/// reduce to a count; the full match set surfaces only in a terminal state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationDiscidProgress {
    Computing,
    LookingUp,
    Done { n_results: u32 },
    Skipped,
    Failed { failure: AutomationLookupFailure },
}

/// Projects bae-core's `identify::BarcodeProgress` — mid-flight result payloads
/// reduce to a count; the full match set surfaces only in a terminal state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationBarcodeProgress {
    Scanning,
    LookingUp {
        current: String,
        position: u32,
        total: u32,
    },
    Done {
        n_results: u32,
    },
    /// No provider answered, each with its own reason.
    Failed {
        failures: Vec<AutomationSourceFailure>,
    },
    /// Reading the candidate's barcodes failed, so no provider was asked.
    ScanFailed {
        failure: AutomationLookupFailure,
    },
    Skipped,
}

/// One provider that failed one lookup, and how. Mirrors bae-core's
/// `import::SourceFailure`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationSourceFailure {
    pub source: AutomationMetadataSource,
    pub failure: AutomationLookupFailure,
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
    Triangulating {
        discid: AutomationDiscidProgress,
        barcode: AutomationBarcodeProgress,
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
