//! The identify pipeline as a surface reads it: what a candidate's runs ask
//! about, the ledger they draw, and the state each settles on.

use super::super::*;

/// One change a person makes to what a candidate's identification asks about
/// or counts; core applies it to the choices it holds. Mirrors
/// `bae_core::import::LookupChoiceEdit`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLookupChoiceEdit {
    /// Ask about the disc ID again, or leave it out.
    ToggleDiscId,
    /// Leave one barcode out, or ask about it again.
    ToggleBarcode { code: String },
    /// Look one catalog number up, or stop looking it up.
    ToggleCatalog { number: String },
    /// Search by these words, or by the draft's own title when both are blank.
    SearchBy { album: String, artist: String },
    /// Strike one catalog number out of what the folder is taken to state, or
    /// count it again.
    ToggleDiscounted { number: String },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeLookupChoiceEdit = bae_core::import::LookupChoiceEdit,
    into_core: pub fn,
    variants: {
        ToggleDiscId,
        ToggleBarcode { code },
        ToggleCatalog { number },
        SearchBy { album, artist },
        ToggleDiscounted { number },
    },
}

/// The identifying signals, as the apps name them in a failure line, an
/// evidence chip or the run's lookup row. Nothing crosses into it from core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalKind {
    DiscId,
    Barcode,
    Catalog,
    Isrc,
}

/// Why a catalog did not answer a lookup; the UI resolves its line through
/// `bridge_lookup_failure_key`. Mirrors `bae_core::signals::LookupFailure`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLookupFailure {
    /// No HTTP response.
    Network,
    /// An HTTP error response, with its status where one was seen.
    Provider {
        status: Option<u16>,
    },
    Timeout,
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeLookupFailure = bae_core::signals::LookupFailure,
    from_core: pub(crate) fn,
    variants: {
        Network,
        Provider { status },
        Timeout,
    },
}

/// How bae broke on its own side. `detail` is the error chain, shown
/// untranslated. Mirrors `bae_core::signals::InternalFailure`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct BridgeInternalFailure {
    pub detail: String,
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeInternalFailure = bae_core::signals::InternalFailure,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { detail },
}

/// Localization key for a lookup failure's line.
#[uniffi::export]
pub fn bridge_lookup_failure_key(failure: BridgeLookupFailure) -> String {
    match failure {
        BridgeLookupFailure::Network => "core.lookup.failure.network",
        BridgeLookupFailure::Provider { status: Some(_) } => "core.lookup.failure.provider",
        BridgeLookupFailure::Provider { status: None } => "core.lookup.failure.provider_unknown",
        BridgeLookupFailure::Timeout => "core.lookup.failure.timeout",
    }
    .to_string()
}

/// Why a run did not ask about a value, or took a step without asking
/// anyone. Mirrors `bae_core::identify::NotAskedReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeNotAskedReason {
    /// The person left the value out of the run.
    LeftOut,
    /// The step is switched off in the identification settings.
    SwitchedOff,
    /// No catalog the run asks answers this lookup.
    NoCatalog,
}

/// How one provider's lookup of one value is going — one cell of the run's
/// ledger. Mirrors `bae_core::identify::LookupView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeLookupState {
    /// Not asked yet: the codes are still being read off the artwork.
    Queued,
    /// Never asked, for `reason`.
    NotAsked {
        reason: BridgeNotAskedReason,
    },
    LookingUp,
    /// How many pressings the lookup named, and the album cards they fold into.
    Found {
        count: u32,
        groups: Vec<BridgeReleaseGroup>,
    },
    NoMatch,
    Failed {
        failure: BridgeLookupFailure,
    },
}

/// Mirrors `bae_core::identify::ProviderCell`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeProviderCell {
    pub source: BridgeCatalog,
    pub lookup: BridgeLookupState,
}

/// One value and every provider's lookup of it. Mirrors
/// `bae_core::identify::SignalValueRow`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignalValueRow {
    pub value: String,
    /// Whether the person left this value out of the run; always false for a
    /// catalog number.
    pub excluded: bool,
    /// One per provider in the run, in the run's provider order.
    pub cells: Vec<BridgeProviderCell>,
}

/// The disc-ID step: one lookup, since only MusicBrainz answers disc IDs.
/// Mirrors `bae_core::identify::DiscIdStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDiscIdStep {
    Reading,
    /// No LOG or CUE to read one off.
    Absent,
    /// A CUE over audio sampled at a rate no CD plays at, with that rate as
    /// the audio's files state it; `None` when they are not at hand.
    NotCdAudio {
        sample_rate_hz: Option<u32>,
    },
    Read {
        disc_id: String,
        lookup: BridgeLookupState,
    },
}

/// Mirrors `bae_core::identify::BarcodeStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeBarcodeStep {
    /// No barcode source at all.
    Absent,
    /// Cover art is not read and no CUE sheet states a code.
    CoverArtOff,
    /// There was a source and it held no code.
    NoCodes,
    /// One row per code; while `scanning`, more may come.
    Rows {
        scanning: bool,
        rows: Vec<BridgeSignalValueRow>,
    },
}

/// A catalog number the run is not looking up. Mirrors
/// `bae_core::identify::CatalogCandidateView`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeCatalogCandidate {
    pub value: String,
}

/// Mirrors `bae_core::identify::CatalogStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCatalogStep {
    NoneFound,
    /// No catalog number in the folder's own text, and cover art is not read.
    CoverArtOff,
    Numbers {
        /// Whether the artwork is still being read, so more may come.
        scanning: bool,
        /// The chosen numbers, in the order they were chosen.
        rows: Vec<BridgeSignalValueRow>,
        /// The numbers not chosen; once the run settles, the ones no offered
        /// release carries.
        candidates: Vec<BridgeCatalogCandidate>,
    },
}

/// A catalog number the candidate's text states about an offered release.
/// Mirrors `bae_core::identify::CatalogAgreementView`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCatalogAgreement {
    pub value: String,
    /// Whether the person struck it out, so it earns no catalog agreement.
    pub discounted: bool,
}

/// The ISRCs the audio's tags carry: one lookup, since only MusicBrainz is
/// asked. Mirrors `bae_core::identify::IsrcStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeIsrcStep {
    Reading,
    /// No audio file's tags carry one.
    Absent,
    /// Every code, each once, in the files' order.
    Read {
        isrcs: Vec<String>,
        lookup: BridgeLookupState,
    },
}

/// Mirrors `bae_core::identify::SearchStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSearchStep {
    /// Nobody is asked the title, for `reason`, whatever the identifiers find.
    NotAsked { reason: BridgeNotAskedReason },
    /// The identifiers answered; no search was needed.
    NotNeeded,
    /// Nothing to search by: the draft has no title.
    NoTitle,
    /// The identifiers are still being looked up.
    Waiting { album: String, artist: String },
    Searched {
        album: String,
        /// Blank where the title alone was searched.
        artist: String,
        /// One per provider, in the run's provider order.
        cells: Vec<BridgeProviderCell>,
    },
}

/// Mirrors `bae_core::identify::IdentifyRunView`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeIdentifyRun {
    /// The providers the run asks, in the order their cells are listed.
    pub providers: Vec<BridgeCatalog>,
    pub disc_id: BridgeDiscIdStep,
    pub barcode: BridgeBarcodeStep,
    pub catalog: BridgeCatalogStep,
    pub isrc: BridgeIsrcStep,
    pub search: BridgeSearchStep,
}

/// Mirrors `bae_core::signals::TextSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTextSignal {
    Scanning {
        catalogs: Vec<String>,
        free_text: Vec<String>,
    },
    Settled {
        catalogs: Vec<String>,
        free_text: Vec<String>,
    },
    Failed {
        failure: BridgeInternalFailure,
        catalogs: Vec<String>,
        free_text: Vec<String>,
    },
}

/// The part of `bae_core::signals::Signals` a surface reads: the text pools
/// that feed the search form's autocomplete.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignals {
    pub text: BridgeTextSignal,
}

/// What the candidate agrees with about one result: a row's badges, and what
/// ordered the rows. Mirrors `bae_core::identify::RowAgreements`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAgreements {
    pub disc_id: bool,
    pub barcode: bool,
    pub catalog: bool,
    pub label: bool,
    pub year: bool,
    pub country: bool,
    /// The note of the row's records the folder names it by, where the
    /// ranking's notes point went to it: the Notes badge and its tooltip.
    pub notes: Option<String>,
}

/// One candidate's identify state. A settled state carries the run it settled
/// as, or none when there was nothing to lay out.
///
/// `groups` are every card, ranked: the rows agreement set aside sit on their
/// album's card as its sections' `narrowed_out`, and the cards all of whose
/// rows were set aside come after every card that offers one.
/// `narrowed_out_count` is how many rows were set aside across every card —
/// what the list's "more" disclosure counts.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeIdentifyState {
    Idle,
    /// Lookups in flight, with the matches the answers so far combine to,
    /// shaped as `Found`'s.
    Triangulating {
        run: BridgeIdentifyRun,
        groups: Vec<BridgeReleaseGroup>,
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        narrowed_out_count: u32,
    },
    Found {
        run: Option<BridgeIdentifyRun>,
        /// The matches as album cards, already ranked.
        groups: Vec<BridgeReleaseGroup>,
        /// Library status per release, offered or set aside, by release id.
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        track_count: u32,
        /// Agreements per release, offered or set aside, by release id.
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        narrowed_out_count: u32,
        /// The Catalog # row's chips.
        catalog_agreements: Vec<BridgeCatalogAgreement>,
        /// The check against the folder the found release failed: why the
        /// verdict picks none of its releases.
        folder_check: Option<crate::types::BridgeFolderCheck>,
        /// Whether the verdict picks its one release unattended.
        picks_unattended: bool,
    },
    NotFoundAnywhere {
        run: Option<BridgeIdentifyRun>,
    },
    /// Nothing was looked up, so the UI offers manual search rather than
    /// saying nothing matched.
    ManualOnly {
        track_count: u32,
        run: Option<BridgeIdentifyRun>,
    },
    /// bae broke on its own side and the run ended there; the pane states
    /// why and offers another run.
    Error {
        failure: BridgeInternalFailure,
    },
    /// At least one automatic provider lookup failed; only an explicit re-run
    /// retries it.
    ///
    /// It still carries whatever the surviving evidence found: one provider
    /// failing leaves the other's matches standing, and the pane shows them
    /// with the failures named beside them, live or resumed from the stored
    /// verdict. `groups` is empty when nothing that answered returned
    /// anything.
    Failed {
        run: Option<BridgeIdentifyRun>,
        failures: Vec<BridgeIdentifyFailure>,
        groups: Vec<BridgeReleaseGroup>,
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        narrowed_out_count: u32,
        catalog_agreements: Vec<BridgeCatalogAgreement>,
    },
}

/// Which lookup failed, and which provider where several answer it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeIdentifyFailure {
    DiscId {
        failure: BridgeLookupFailure,
    },
    Barcode {
        source: BridgeCatalog,
        failure: BridgeLookupFailure,
    },
    Catalog {
        source: BridgeCatalog,
        failure: BridgeLookupFailure,
    },
    Search {
        source: BridgeCatalog,
        failure: BridgeLookupFailure,
    },
    /// MusicBrainz could not answer the search by the audio's ISRCs.
    Isrc {
        failure: BridgeLookupFailure,
    },
    ReleaseDetails {
        failure: BridgeLookupFailure,
    },
}
