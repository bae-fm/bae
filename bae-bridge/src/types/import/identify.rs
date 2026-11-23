//! The identify pipeline as a surface reads it: what a candidate's runs ask
//! about, the ledger they draw, and the state each settles on.

use super::super::*;

/// One change a person makes to what a candidate's identification asks about
/// or counts; core applies it to the choices it holds. Mirrors
/// `bae_identify::LookupChoiceEdit`.
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
    #[cfg(desktop)]
    BridgeLookupChoiceEdit = bae_identify::LookupChoiceEdit,
    into_core: pub fn,
    variants: {
        ToggleDiscId,
        ToggleBarcode { code },
        ToggleCatalog { number },
        SearchBy { album, artist },
        ToggleDiscounted { number },
    },
}

/// Why a catalog did not answer a lookup; the UI resolves its line through
/// `bridge_lookup_failure_key`. Mirrors `bae_catalogs::LookupFailure`.
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
    #[cfg(desktop)]
    BridgeLookupFailure = bae_catalogs::LookupFailure,
    from_core: pub(crate) fn,
    variants: {
        Network,
        Provider { status },
        Timeout,
    },
}

/// How bae broke on its own side. `detail` is the error chain, shown
/// untranslated. Mirrors `bae_catalogs::InternalFailure`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct BridgeInternalFailure {
    pub detail: String,
}

mirror_struct! {
    #[cfg(desktop)]
    BridgeInternalFailure = bae_catalogs::InternalFailure,
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

/// Why a run did not ask about a value. Mirrors
/// `bae_identify::NotAskedReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeNotAskedReason {
    /// The person left the value out of the run.
    LeftOut,
    /// No catalog the run asks answers this lookup.
    NoCatalog,
}

/// How one provider's lookup of one value is going — one cell of the run's
/// ledger. Mirrors `bae_identify::LookupView`.
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

/// Mirrors `bae_identify::ProviderCell`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeProviderCell {
    pub source: BridgeLookupCatalog,
    pub lookup: BridgeLookupState,
}

/// One value and every provider's lookup of it. Mirrors
/// `bae_identify::SignalValueRow`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignalValueRow {
    pub value: String,
    /// Whether the person left this value out of the run; always false for a
    /// catalog number.
    pub excluded: bool,
    /// One per provider in the run, in the run's provider order.
    pub cells: Vec<BridgeProviderCell>,
}

/// The disc-ID step: one lookup per disc ID, since only MusicBrainz answers
/// disc IDs. Mirrors `bae_identify::DiscIdStepView`.
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
    /// One row per disc ID, in the order they were read.
    Read {
        discs: Vec<BridgeDiscIdRow>,
    },
}

/// One disc ID and its lookup. Mirrors `bae_identify::DiscIdRow`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDiscIdRow {
    pub disc_id: String,
    pub lookup: BridgeLookupState,
}

/// Mirrors `bae_identify::BarcodeStepView`, without its `scanning`,
/// which crosses once for the run as `BridgeIdentifyRun::reading_artwork`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeBarcodeStep {
    /// No barcode source at all.
    Absent,
    /// There was a source and it held no code.
    NoCodes,
    /// One row per code.
    Rows { rows: Vec<BridgeSignalValueRow> },
}

/// A catalog number the text offers that is not in effect. Mirrors
/// `bae_identify::CatalogCandidateView`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeCatalogCandidate {
    pub value: String,
}

/// Mirrors `bae_identify::CatalogStepView`, without its `scanning`,
/// which crosses once for the run as `BridgeIdentifyRun::reading_artwork`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCatalogStep {
    NoneFound,
    Numbers {
        /// The numbers in effect — picked, then confirmed by a found
        /// release — each with its search, then the struck-out ones,
        /// `excluded`.
        rows: Vec<BridgeSignalValueRow>,
        /// The numbers the text offers that are none of those.
        candidates: Vec<BridgeCatalogCandidate>,
    },
}

/// The ISRCs the audio's tags carry: one lookup, since only MusicBrainz is
/// asked. Mirrors `bae_identify::IsrcStepView`.
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

/// Mirrors `bae_identify::SearchStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSearchStep {
    /// No search goes out: the identifiers answered, or the draft has no
    /// title to search by.
    NotNeeded,
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

/// Mirrors `bae_identify::IdentifyRunView`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeIdentifyRun {
    /// The providers the run asks, in the order their cells are listed.
    pub providers: Vec<BridgeLookupCatalog>,
    pub disc_id: BridgeDiscIdStep,
    pub barcode: BridgeBarcodeStep,
    pub catalog: BridgeCatalogStep,
    pub isrc: BridgeIsrcStep,
    pub search: BridgeSearchStep,
    /// Whether the artwork is still being read, so more barcodes or catalog
    /// numbers may come.
    pub reading_artwork: bool,
}

/// Mirrors `bae_identify::signals::TextSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTextSignal {
    Scanning { catalogs: Vec<String>, free_text: Vec<String> },
    Settled { catalogs: Vec<String>, free_text: Vec<String> },
    Failed { failure: BridgeInternalFailure, catalogs: Vec<String>, free_text: Vec<String> },
}

/// The part of `bae_identify::signals::Signals` a surface reads: the text pools
/// that feed the search form's autocomplete.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignals {
    pub text: BridgeTextSignal,
}

/// What the candidate agrees with about one result: a row's badges, and what
/// ordered the rows. Mirrors `bae_identify::RowAgreements`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAgreements {
    pub disc_id: bool,
    pub barcode: bool,
    pub catalog: bool,
    pub label: bool,
    pub year: bool,
    pub country: bool,
    /// The note of each of the row's records the folder names it by, where
    /// the ranking's notes point went to it, each with its catalogs where the
    /// notes differ: the Notes badge and the row's note line. Empty where it
    /// did not.
    pub notes: Vec<BridgeTextStatement>,
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
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        narrowed_out_count: u32,
    },
    Found {
        run: Option<BridgeIdentifyRun>,
        /// The matches as album cards, already ranked.
        groups: Vec<BridgeReleaseGroup>,
        track_count: u32,
        /// Agreements per release, offered or set aside, by release id.
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        narrowed_out_count: u32,
        /// The check against the folder the found release failed: why the
        /// verdict picks none of its releases.
        folder_check: Option<crate::types::BridgeFolderCheck>,
        /// Whether the offered rows are several pressings of one album, which
        /// the folder can be linked to with its pressing unknown.
        offers_shared_album: bool,
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
        /// The failures no cell of `run` shows, each said in a line of its
        /// own: a failure a cell shows is said there, beside its retry.
        /// `None` when a cell shows every failure.
        failure_lines: Option<BridgeFailureLines>,
        groups: Vec<BridgeReleaseGroup>,
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        narrowed_out_count: u32,
        /// Whether the offered rows are several pressings of one album, as
        /// for `Found`.
        offers_shared_album: bool,
    },
}

/// The failures of a failed verdict that no cell of its ledger shows, in the
/// blocks of lines a screen draws them in. Mirrors
/// `bae_identify::FailureLines`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeFailureLines {
    /// Every one, in the order the run met them: the lines of a page that
    /// offers no releases, which the page draws with one Retry. The list holds
    /// at least one failure.
    pub all: Vec<BridgeIdentifyFailure>,
    /// The failed fetches of the release the run picked, which say why bae
    /// did not apply the pick and leave no results missing from the list. The
    /// page draws them with one Retry. `None` when no fetch of the picked
    /// release failed, so the page draws no lines.
    pub pick_failures: Option<Vec<BridgeIdentifyFailure>>,
    /// Each lookup among the other failures, once, in the order the run first
    /// met it: the lines closing a list of offered releases.
    pub missing_searches: Vec<BridgeMissingSearch>,
}

/// A lookup that failed, so a list of offered releases is missing its
/// results. Mirrors `bae_identify::MissingSearch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMissingSearch {
    pub step: BridgeLookupStep,
    pub source: BridgeCatalog,
}

/// A step of identification that asks a catalog. Mirrors
/// `bae_identify::LookupStep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeLookupStep {
    DiscId,
    Barcode,
    Catalog,
    Isrc,
    /// The title search the run fell back on.
    TitleSearch,
    /// The lookup of an offered row's pressing on a catalog that has no
    /// record of it.
    Pressing,
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
    /// One catalog could not answer the lookup of an offered row's pressing
    /// on it, so that row may be missing its record there.
    Pressing {
        source: BridgeCatalog,
        failure: BridgeLookupFailure,
    },
    /// The catalog offering the cover of the release a run picked could not
    /// give it, so the pick was not applied.
    Cover {
        source: BridgeCatalog,
        failure: BridgeLookupFailure,
    },
}

/// The key a library release's re-identify session runs under: what its
/// identify events and its pane name it by.
#[cfg(desktop)]
#[uniffi::export]
pub fn bridge_reidentify_key(release_id: String) -> Vec<u8> {
    Vec::<u8>::from(bae_import::CandidateKey::reidentify(&release_id))
}
