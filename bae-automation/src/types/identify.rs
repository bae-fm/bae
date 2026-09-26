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
    /// Never asked: the lookup's step is switched off in the identification
    /// settings.
    Off,
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
    pub source: AutomationCatalog,
    pub lookup: AutomationLookupState,
}

/// Mirrors bae-core's `identify::SignalValueRow` — one value extraction
/// found, where it was found, and every provider's lookup of it.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationSignalValueRow {
    pub value: String,
    pub sources: Vec<AutomationValueSource>,
    /// Whether the person left this value out of the run, so no provider was
    /// asked about it.
    pub excluded: bool,
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
    /// A CUE was there over audio sampled at a rate no CD plays at, so it was
    /// not read into a disc ID.
    NotCdAudio {
        sample_rate_hz: u32,
    },
    ReadFailed {
        failure: AutomationLookupFailure,
    },
    Read {
        disc_id: String,
        source: Option<AutomationDiscIdFile>,
        lookup: AutomationLookupState,
    },
    /// A disc ID was read and no provider the run asks answers disc IDs.
    ReadNotAsked {
        disc_id: String,
        source: Option<AutomationDiscIdFile>,
    },
    /// A disc ID was read and the person left it out of the run.
    LeftOut {
        disc_id: String,
        source: Option<AutomationDiscIdFile>,
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
    /// Nothing the folder's own text states is a catalog number, and the cover
    /// art was left unread.
    CoverArtOff,
    Numbers {
        scanning: bool,
        rows: Vec<AutomationSignalValueRow>,
        candidates: Vec<AutomationCatalogCandidate>,
    },
}

/// Mirrors bae-core's `identify::CatalogAgreementView` — one catalog number
/// the candidate's text states about a release the run is offering, and
/// whether the person struck it out.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationCatalogAgreement {
    pub value: String,
    pub discounted: bool,
}

/// Mirrors bae-core's `identify::SearchStepView` — the title search the run
/// falls back on when its identifiers name nothing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationSearchStep {
    /// The run does not search by title.
    Off,
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

/// Mirrors bae-core's `identify::IdentifyRunView` — the run as its ledger,
/// each provider's part of each signal reported on its own.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationIdentifyRun {
    pub providers: Vec<AutomationCatalog>,
    pub disc_id: AutomationDiscIdStep,
    pub barcode: AutomationBarcodeStep,
    pub catalog: AutomationCatalogStep,
    pub search: AutomationSearchStep,
    pub album_links: AutomationAlbumLinksStep,
}

/// Mirrors bae-core's `identify::AlbumLinksStepView` — whether the run joins
/// the two catalogs' albums by the links their pages state.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationAlbumLinksStep {
    Followed,
    Off,
}

/// Mirrors bae-core's `identify::Agreements`, paired with the release id it
/// belongs to: what the candidate's own text agrees with about that result,
/// which is what ordered the rows and what each row's badges say.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationAgreements {
    pub release_id: String,
    pub disc_id: bool,
    pub barcode: bool,
    pub catalog: bool,
    pub label: bool,
    pub year: bool,
    pub country: bool,
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
        source: AutomationCatalog,
        failure: AutomationLookupFailure,
    },
    Catalog {
        source: AutomationCatalog,
        failure: AutomationLookupFailure,
    },
    /// One provider could not answer the title search.
    Search {
        source: AutomationCatalog,
        failure: AutomationLookupFailure,
    },
    ReleaseDetails {
        failure: AutomationLookupFailure,
    },
}

/// The rows agreement left out of a state's matches. A card the matches are
/// on carries its own rows set aside as each section's `narrowed_out`; only an
/// album none of whose rows is offered is a card here, and every row's status
/// and badges are in the state's own lists. Empty when nothing was narrowed.
/// Mirrors `bae_core::identify::NarrowedOutView`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AutomationNarrowedOut {
    /// The cards none of whose rows is offered.
    pub groups: Vec<AutomationReleaseGroup>,
    /// How many rows were set aside, on every card.
    pub count: u32,
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
        agreements: Vec<AutomationAgreements>,
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
    /// A lookup failed, with whatever the surviving evidence still found: one
    /// provider failing leaves the other's matches standing, live or resumed
    /// from the stored verdict. Empty groups mean nothing that answered
    /// returned anything.
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
