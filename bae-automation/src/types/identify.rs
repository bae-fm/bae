//! The identify pipeline's shapes as an MCP client reads them, mirroring
//! bae-core's `identify::IdentifyStateView`.

use super::*;

/// Mirrors bae-core's `identify::NotAskedReason`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationNotAskedReason {
    /// The person left the value out of the run.
    LeftOut,
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

/// Mirrors bae-core's `identify::FolderCheck`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationFolderCheck {
    SourceTracksUnknown,
    MediumDisagrees { folder: AutomationMediumConflict },
}

/// Mirrors bae-core's `identify::MediumConflict`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationMediumConflict {
    CdRip,
    NotCdAudio,
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
    NotCdAudio,
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
    NoCodes,
    Rows {
        scanning: bool,
        rows: Vec<AutomationSignalValueRow>,
    },
}

/// Mirrors bae-core's `identify::IsrcStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationIsrcStep {
    Reading,
    Absent,
    Read {
        isrcs: Vec<String>,
        lookup: AutomationLookupState,
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
    Numbers {
        scanning: bool,
        rows: Vec<AutomationSignalValueRow>,
        candidates: Vec<AutomationCatalogCandidate>,
    },
}

/// Mirrors bae-core's `identify::SearchStepView`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationSearchStep {
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
    pub isrc: AutomationIsrcStep,
    pub search: AutomationSearchStep,
}

/// Mirrors bae-core's `identify::RowAgreements`, with the release id it
/// belongs to.
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
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationIdentifyFailure {
    DiscId {
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
    Isrc {
        failure: AutomationLookupFailure,
    },
    Pressing {
        source: AutomationCatalog,
        failure: AutomationLookupFailure,
    },
    ReleaseDetails {
        failure: AutomationLookupFailure,
    },
    ArtistImages {
        failure: AutomationLookupFailure,
    },
    Cover {
        source: AutomationCatalog,
        failure: AutomationLookupFailure,
    },
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
        narrowed_out_count: u32,
    },
    /// A settled state carries the run it settled as, or none when there was
    /// nothing to lay out.
    Found {
        run: Option<AutomationIdentifyRun>,
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        track_count: u32,
        agreements: Vec<AutomationAgreements>,
        narrowed_out_count: u32,
        /// Why the verdict picks none of its releases, when a check against
        /// the folder failed.
        folder_check: Option<AutomationFolderCheck>,
        picks_unattended: bool,
        /// Whether the offered rows are several pressings of one album, which
        /// the folder can be linked to with its pressing unknown.
        offers_shared_album: bool,
    },
    NotFoundAnywhere {
        run: Option<AutomationIdentifyRun>,
    },
    ManualOnly {
        track_count: u32,
        run: Option<AutomationIdentifyRun>,
    },
    /// bae broke on its own side and the run ended there.
    Error {
        failure: AutomationInternalFailure,
    },
    /// A lookup failed, with whatever the lookups that answered still found.
    Failed {
        run: Option<AutomationIdentifyRun>,
        failures: Vec<AutomationIdentifyFailure>,
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        agreements: Vec<AutomationAgreements>,
        narrowed_out_count: u32,
        /// Whether the offered rows are several pressings of one album, as for
        /// `Found`.
        offers_shared_album: bool,
    },
}
