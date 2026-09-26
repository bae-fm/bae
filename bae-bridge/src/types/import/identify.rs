//! The identify pipeline as a surface reads it: what a candidate's runs ask
//! about, the badge row they project to, the ledger they draw while they go,
//! and the state each one settles on.
//!
//! Split from `candidate` along the boundary the domain already has: that
//! module is what a candidate's files are, this one is what identification
//! did with them.

use super::super::*;

/// What a candidate's identification asks about — the signals its runs leave
/// out and the catalog numbers they look up — and what it is to make of the
/// answers. Mirrors `bae_core::import::LookupChoices`.
///
/// One value, sent whole. A control that changes one part reads the candidate
/// detail's current value, changes that part, and sends the result back.
/// Changing what the run looks up is what starts the run that reads it;
/// striking a number out of the candidate's text starts none, and the
/// candidate's next detail carries the answers ranked by it.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct BridgeLookupChoices {
    /// Whether the run leaves the candidate's disc ID out.
    pub disc_id_excluded: bool,
    /// The barcode values the run leaves out. A set, each value once, sorted;
    /// naming every code the candidate carries is how the barcode stays out
    /// altogether.
    pub excluded_barcodes: Vec<String>,
    /// The catalog numbers the run looks up, each on its own, in the order
    /// they were chosen.
    pub chosen_catalogs: Vec<String>,
    /// The words the title search asks for, where the person typed them.
    /// `None` searches by what the draft calls the release.
    pub search_words: Option<BridgeSearchWords>,
    /// The catalog numbers the candidate's own text carries that the person
    /// struck out, so a release carrying one earns no catalog agreement from
    /// the text. A set, each value once; a number can be looked up and struck
    /// out at once.
    pub discounted_catalogs: Vec<String>,
}

/// The words a person typed for the title search, in place of the draft's
/// own. Mirrors `bae_core::import::SearchWords`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSearchWords {
    pub album: String,
    /// Blank searches by the title alone.
    pub artist: String,
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeSearchWords = bae_core::import::SearchWords,
    from_core: pub(crate) fn,
    into_core: pub fn,
    fields: { album, artist },
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeLookupChoices = bae_core::import::LookupChoices,
    from_core: pub(crate) fn,
    into_core: pub fn,
    fields: {
        disc_id_excluded,
        excluded_barcodes,
        chosen_catalogs,
        search_words: (opt BridgeSearchWords),
        discounted_catalogs,
    },
}

/// A signal value paired with its origin — a catalog candidate or a barcode
/// code. Mirrors `bae_core::signals::SourcedValue`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeSourcedValue {
    pub value: String,
    pub origin: BridgeSignalOrigin,
    /// The candidate-relative path of the file the value was read off — the id
    /// a gallery tile and a file row are keyed by, so a surface can put the
    /// value on the file it came from. `None` where the origin names no file.
    pub origin_path: Option<String>,
    /// Where on that image the value was read, for an origin that is an image
    /// and a detector that reports where it looked. `None` otherwise.
    pub region: Option<BridgeImageRegion>,
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeSourcedValue = bae_core::signals::SourcedValue,
    from_core: pub(crate) fn,
    fields: {
        value,
        origin: (BridgeSignalOrigin),
        origin_path,
        region: (opt BridgeImageRegion),
    },
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeImageRegion = bae_core::signals::ImageRegion,
    from_core: pub(crate) fn,
    fields: { x, y, width, height },
}

/// Which kind of signal a toolbar badge represents. Mirrors
/// `bae_core::identify::SignalKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalKind {
    DiscId,
    Barcode,
    Catalog,
}

/// Why a metadata lookup failed. Mirrors `bae_core::signals::LookupFailure`.
/// The locale never crosses the bridge: the UI resolves a localized line per
/// variant (`bridge_lookup_failure_key`) and renders `Provider`'s status as
/// the message argument. `Diagnostic` carries opaque, log-only detail — never
/// translated, never shown as primary copy.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLookupFailure {
    /// Transport/connection failure — no HTTP response.
    Network,
    /// An HTTP error response from the metadata provider, with its status
    /// code when one was observed.
    Provider { status: Option<u16> },
    /// The request timed out before a response arrived.
    Timeout,
    /// Artwork analysis failed before barcode/text extraction finished.
    ArtworkAnalysis,
    /// A local error (DB load, "not found", a compute task panic). `detail`
    /// is the opaque error chain — log-only, never translated.
    Diagnostic { detail: String },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeLookupFailure = bae_core::signals::LookupFailure,
    from_core: pub(crate) fn,
    variants: {
        Network,
        Provider { status },
        Timeout,
        ArtworkAnalysis,
        Diagnostic { detail },
    },
}

/// Localization key for a lookup failure's user-facing line, or `None` for
/// `Diagnostic` (no translated copy — the UI shows a generic line plus the opaque
/// `detail`). `Provider` resolves to the status-bearing line when a code was
/// observed and a no-status fallback when not, so the UI never has to decide
/// which message a missing status takes. One source of these keys for every
/// platform.
#[uniffi::export]
pub fn bridge_lookup_failure_key(failure: BridgeLookupFailure) -> Option<String> {
    match failure {
        BridgeLookupFailure::Network => Some("core.lookup.failure.network".to_string()),
        BridgeLookupFailure::Provider { status: Some(_) } => {
            Some("core.lookup.failure.provider".to_string())
        }
        BridgeLookupFailure::Provider { status: None } => {
            Some("core.lookup.failure.provider_unknown".to_string())
        }
        BridgeLookupFailure::Timeout => Some("core.lookup.failure.timeout".to_string()),
        BridgeLookupFailure::ArtworkAnalysis => {
            Some("core.lookup.failure.artwork_analysis".to_string())
        }
        BridgeLookupFailure::Diagnostic { .. } => None,
    }
}

/// Localization key for a lookup failure's brief reason: the few words a line
/// that already names the source and the step ends with — "timed out", "busy
/// (503)". Total, `Diagnostic` included, since a brief line has no room for
/// the opaque detail. A 429 or 503 is the provider refusing for now rather
/// than a fault in what was asked, so it reads as busy rather than as an
/// error number. One source of these keys for every platform.
#[uniffi::export]
pub fn bridge_lookup_failure_brief_key(failure: BridgeLookupFailure) -> String {
    match failure {
        BridgeLookupFailure::Network => "core.lookup.failure.brief.network",
        BridgeLookupFailure::Provider {
            status: Some(429 | 503),
        } => "core.lookup.failure.brief.busy",
        BridgeLookupFailure::Provider { status: Some(_) } => "core.lookup.failure.brief.provider",
        BridgeLookupFailure::Provider { status: None } => {
            "core.lookup.failure.brief.provider_unknown"
        }
        BridgeLookupFailure::Timeout => "core.lookup.failure.brief.timeout",
        BridgeLookupFailure::ArtworkAnalysis => "core.lookup.failure.brief.artwork_analysis",
        BridgeLookupFailure::Diagnostic { .. } => "core.lookup.failure.brief.diagnostic",
    }
    .to_string()
}

/// The live lookup state of one toolbar badge. Mirrors
/// `bae_core::identify::SignalState`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalState {
    LookingUp,
    Found { count: u32 },
    NoMatch,
    Skipped,
    Failed { failure: BridgeLookupFailure },
}

/// One of the values a signal could take, for the signals that offer a choice.
/// Mirrors `bae_core::identify::SignalOption`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignalOption {
    pub value: String,
    pub origin: BridgeSignalOrigin,
    /// Whether the identify run asks about this one. Several options of a
    /// signal can be chosen at once.
    pub chosen: bool,
}

/// The value a badge shows, and where it was read. Mirrors
/// `bae_core::identify::ToolbarValue`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeToolbarValue {
    pub value: String,
    pub origin: BridgeToolbarOrigin,
}

/// Where a badge's value was read. Mirrors
/// `bae_core::identify::ToolbarOrigin`.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeToolbarOrigin {
    /// The disc's table of contents (LOG/CUE).
    DiscToc,
    Value {
        origin: BridgeSignalOrigin,
    },
}

/// One badge in the signals toolbar — a pre-shaped row the UI renders without
/// deriving anything. Mirrors `bae_core::identify::ToolbarSignal`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeToolbarSignal {
    pub kind: BridgeSignalKind,
    /// The value the badge shows and where it was read; `None` when the
    /// signal has nothing to show.
    pub shown: Option<BridgeToolbarValue>,
    pub state: BridgeSignalState,
    pub excluded: bool,
    /// The values this signal offers, each marked when the run asks about it.
    /// Empty for the disc ID, which has one value the badge itself stands for,
    /// and for a signal the candidate carries no value of.
    pub options: Vec<BridgeSignalOption>,
}

/// The candidate's full signals toolbar — the ordered badge list. Mirrors a
/// `Vec<bae_core::identify::ToolbarSignal>`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignalsToolbar {
    pub signals: Vec<BridgeToolbarSignal>,
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeSignalState = bae_core::identify::SignalState,
    from_core: fn,
    variants: {
        LookingUp,
        Found { count },
        NoMatch,
        Skipped,
        Failed { failure: (BridgeLookupFailure) },
    },
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeSignalOption = bae_core::identify::SignalOption,
    from_core: fn,
    fields: { value, origin: (BridgeSignalOrigin), chosen },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeSignalKind = bae_core::identify::SignalKind,
    from_core: fn,
    variants: { DiscId, Barcode, Catalog },
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeToolbarValue = bae_core::identify::ToolbarValue,
    from_core: fn,
    fields: { value, origin: (BridgeToolbarOrigin) },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeToolbarOrigin = bae_core::identify::ToolbarOrigin,
    from_core: fn,
    variants: { DiscToc, Value(origin: (BridgeSignalOrigin)) },
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeToolbarSignal = bae_core::identify::ToolbarSignal,
    from_core: fn,
    fields: {
        kind: (BridgeSignalKind),
        shown: (opt BridgeToolbarValue),
        state: (BridgeSignalState),
        excluded,
        options: (each BridgeSignalOption),
    },
}

#[cfg(feature = "desktop")]
impl BridgeSignalsToolbar {
    pub(crate) fn from_core(toolbar: Vec<bae_core::identify::ToolbarSignal>) -> Self {
        BridgeSignalsToolbar {
            signals: toolbar
                .into_iter()
                .map(BridgeToolbarSignal::from_core)
                .collect(),
        }
    }
}

/// How one provider's lookup of one value is going — one cell of the run's
/// ledger. Mirrors `bae_core::identify::LookupView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeLookupState {
    /// Not asked yet: the provider's walk through the codes has not reached
    /// this one.
    Queued,
    /// Never asked: the provider's walk ended at an earlier code.
    NotAsked,
    LookingUp,
    /// The lookup named releases: how many pressings, and the album cards
    /// they fold into, so a surface can show what the count stands for.
    Found {
        count: u32,
        groups: Vec<BridgeReleaseGroup>,
    },
    NoMatch,
    Failed {
        failure: BridgeLookupFailure,
    },
}

/// One place a value was read. Mirrors `bae_core::identify::ValueSource`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeValueSource {
    pub origin: BridgeSignalOrigin,
    /// The candidate-relative path of the file, where the origin is a file.
    pub file: Option<String>,
    /// Where on that image the value was read, where the detector said.
    pub region: Option<BridgeImageRegion>,
}

/// One provider's cell of a value's row. Mirrors
/// `bae_core::identify::ProviderCell`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeProviderCell {
    pub source: BridgeCatalog,
    pub lookup: BridgeLookupState,
}

/// One value extraction found, as a row of the ledger: where it was found
/// and every provider's lookup of it. Mirrors
/// `bae_core::identify::SignalValueRow`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignalValueRow {
    pub value: String,
    /// Every place the value was read, in the order it was read there.
    pub sources: Vec<BridgeValueSource>,
    /// Whether the person left this value out of the run, so no provider was
    /// asked about it. Always false for a catalog number: a row exists only for
    /// a number the run looks up.
    pub excluded: bool,
    /// One per provider in the run, in the run's provider order.
    pub cells: Vec<BridgeProviderCell>,
}

/// Which kind of artifact a disc ID was read off. Mirrors
/// `bae_core::identify::DiscIdFileKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDiscIdFileKind {
    Log,
    Cue,
}

/// The file a disc ID was read off. Mirrors `bae_core::identify::DiscIdFile`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeDiscIdFile {
    pub kind: BridgeDiscIdFileKind,
    /// The candidate-relative path.
    pub file: String,
}

/// The disc-ID step of a run: read off a LOG or CUE, then looked up on
/// MusicBrainz — the one provider with a disc-ID endpoint, so one lookup and
/// no cells. Mirrors `bae_core::identify::DiscIdStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDiscIdStep {
    /// Extraction has not reported yet.
    Reading,
    /// No LOG or CUE to read one off.
    Absent,
    /// A LOG or CUE was there and no disc ID could be derived from it.
    ReadFailed { failure: BridgeLookupFailure },
    Read {
        disc_id: String,
        /// The file it came from. `None` for a release re-identified from
        /// its stored tracks.
        source: Option<BridgeDiscIdFile>,
        lookup: BridgeLookupState,
    },
    /// A disc ID was read and the one source that answers disc IDs is not among
    /// the run's providers, so nothing looked it up. The value stands with no
    /// count, and there is nothing here for a person to switch.
    ReadNotAsked {
        disc_id: String,
        source: Option<BridgeDiscIdFile>,
    },
    /// A disc ID was read and the person left it out of the run. The value
    /// stands with no count, and asking about it again is theirs to do.
    LeftOut {
        disc_id: String,
        source: Option<BridgeDiscIdFile>,
    },
}

/// The barcode step of a run: read off the artwork and the CUE sheets, then
/// every provider tries the codes in order on its own. Mirrors
/// `bae_core::identify::BarcodeStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeBarcodeStep {
    /// No barcode source at all.
    Absent,
    /// There was a source and it held no code.
    NoCodes,
    /// Reading the candidate's barcodes failed, so no provider was asked.
    ScanFailed { failure: BridgeLookupFailure },
    /// One row per code. While `scanning`, the artwork is still being read
    /// and more rows may come.
    Rows {
        scanning: bool,
        rows: Vec<BridgeSignalValueRow>,
    },
}

/// One catalog number extraction found and the run is not looking up: a
/// tile to activate. Mirrors `bae_core::identify::CatalogCandidateView`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeCatalogCandidate {
    pub value: String,
    pub sources: Vec<BridgeValueSource>,
}

/// The catalog step of a run: the run looks up only the numbers the person
/// picks out of the ones extraction found, each on its own. Mirrors
/// `bae_core::identify::CatalogStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCatalogStep {
    /// Extraction found no catalog number to offer, and is not still looking.
    NoneFound,
    Numbers {
        /// Whether the artwork is still being read, so more may come.
        scanning: bool,
        /// The chosen numbers, in the order they were chosen.
        rows: Vec<BridgeSignalValueRow>,
        /// The numbers not chosen, in the order they were first seen: once
        /// the run settles, the ones no offered release confirms. Folded
        /// behind their count.
        candidates: Vec<BridgeCatalogCandidate>,
    },
}

/// One catalog number the candidate's text states about a release the run is
/// offering — a chip in the Catalog # row. Mirrors
/// `bae_core::identify::CatalogAgreementView`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCatalogAgreement {
    pub value: String,
    /// Whether the person struck it out, so the releases carrying it earn no
    /// catalog agreement from the text. The chip stands either way.
    pub discounted: bool,
}

/// The title-search step of a run: the candidate's own words, asked of every
/// provider at once when the three identifiers named nothing between them.
/// Mirrors `bae_core::identify::SearchStepView`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSearchStep {
    /// The identifiers answered; no search was needed.
    NotNeeded,
    /// Nothing to search by: the draft has no title.
    NoTitle,
    /// The identifiers are still being looked up; these words are searched
    /// if they name nothing.
    Waiting { album: String, artist: String },
    Searched {
        album: String,
        /// Blank where the draft names no album artist; the title alone was
        /// searched.
        artist: String,
        /// One per provider in the run, in the run's provider order.
        cells: Vec<BridgeProviderCell>,
    },
}

/// A run as its ledger: the three identifiers and the title search behind
/// them, each with what extraction produced for it and every provider's lookup
/// of it. Mirrors `bae_core::identify::IdentifyRunView`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeIdentifyRun {
    /// The providers the run asks, in the order their cells are listed.
    pub providers: Vec<BridgeCatalog>,
    pub disc_id: BridgeDiscIdStep,
    pub barcode: BridgeBarcodeStep,
    pub catalog: BridgeCatalogStep,
    pub search: BridgeSearchStep,
}

/// The disc-ID signal. Mirrors `bae_core::signals::DiscIdSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDiscIdSignal {
    Computed {
        disc_id: String,
        track_count: u32,
        /// The candidate-relative path of the LOG or CUE it was derived from —
        /// the id that file's row is keyed by, so a surface can put the disc ID
        /// on it. `None` for a release re-identified from its stored tracks.
        source_file: Option<String>,
    },
    Absent {
        track_count: u32,
    },
    Failed {
        failure: BridgeLookupFailure,
        track_count: u32,
    },
}

/// The barcode signal — the UPC/EAN code payloads with their origins. Mirrors
/// `bae_core::signals::BarcodeSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeBarcodeSignal {
    Scanning {
        codes: Vec<BridgeSourcedValue>,
    },
    Settled {
        codes: Vec<BridgeSourcedValue>,
    },
    Failed {
        failure: BridgeLookupFailure,
        codes: Vec<BridgeSourcedValue>,
    },
    Absent,
}

/// The classified-text signal. Catalogs carry their origin (for the Refine
/// badges); free text doesn't (autocomplete only). Mirrors
/// `bae_core::signals::TextSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTextSignal {
    Scanning {
        catalogs: Vec<BridgeSourcedValue>,
        free_text: Vec<String>,
    },
    Settled {
        catalogs: Vec<BridgeSourcedValue>,
        free_text: Vec<String>,
    },
    Failed {
        failure: BridgeLookupFailure,
        catalogs: Vec<BridgeSourcedValue>,
        free_text: Vec<String>,
    },
}

/// The signals extracted from one candidate's files. Mirrors
/// `bae_core::signals::Signals`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignals {
    pub disc_id: BridgeDiscIdSignal,
    pub barcode: BridgeBarcodeSignal,
    pub text: BridgeTextSignal,
}

/// What the candidate's own text agrees with about one result — the per-row
/// badges, and what ordered the rows. Mirrors `bae_core::identify::Agreements`.
///
/// `disc_id` and `barcode` are the lookups that returned the release.
/// `catalog` is either the catalog lookup or the number being printed in the
/// folder's text; `label`, `year` and `country` are the text alone. A field the
/// source does not state cannot be agreed with.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAgreements {
    pub disc_id: bool,
    pub barcode: bool,
    pub catalog: bool,
    pub label: bool,
    pub year: bool,
    pub country: bool,
}

/// The rows agreement left out of a state's matches — real answers a real
/// lookup returned that the intersection discarded, and the ones the folder's
/// own text says nothing about — offered behind the list's "more" disclosure.
/// A card the matches are on carries its own rows set aside; only an album
/// none of whose rows is offered is a card here. Their statuses and badges are
/// in the state's own maps. Empty when nothing was narrowed. Mirrors
/// `bae_core::identify::NarrowedOutView`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeNarrowedOut {
    /// The cards none of whose rows is offered.
    pub groups: Vec<BridgeReleaseGroup>,
    /// How many rows were set aside, on every card.
    pub count: u32,
}

/// Current identify-pipeline state for one candidate. One variant per state;
/// the UI reducer switches on the variant to render the right banner and
/// update the candidate.
///
/// A settled state carries the run it settled as, so the ledger stays up
/// beside the matches. It carries none when extraction handed the run nothing
/// to lay out — a folder with no disc ID, no barcode source and no catalog
/// number, or a verdict stood back up from the store.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeIdentifyState {
    Idle,
    /// Lookups in flight, laid out as the run's ledger, with the matches the
    /// answered lookups have combined to so far — shaped exactly as `Found`'s,
    /// so a surface lists them the same way. The pipeline transitions to a
    /// terminal state once every step settles.
    Triangulating {
        run: BridgeIdentifyRun,
        groups: Vec<BridgeReleaseGroup>,
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        /// What the answers so far leave out of `groups` — the same list the
        /// settled state lands on, as it stands.
        narrowed_out: BridgeNarrowedOut,
    },
    Found {
        run: Option<BridgeIdentifyRun>,
        /// The matches as group cards, ranked — most agreed with first, the
        /// UI renders them in the order they arrive and sorts nothing. Usually
        /// one card; signals that named different releases give several.
        groups: Vec<BridgeReleaseGroup>,
        /// Library status per release, offered or set aside, keyed by release
        /// id, so the UI looks up a row's status directly without re-indexing
        /// a flat list.
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        track_count: u32,
        /// Per-pressing agreements keyed by release id, offered or set aside —
        /// the per-row badges, and what ordered the rows.
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        /// What the agreement left out, for the surface to offer behind a
        /// disclosure: the count of every row set aside, and the cards none of
        /// whose rows is offered.
        narrowed_out: BridgeNarrowedOut,
        /// The catalog numbers the candidate's text states about the offered
        /// releases, as the Catalog # row's chips.
        catalog_agreements: Vec<BridgeCatalogAgreement>,
    },
    NotFoundAnywhere {
        run: Option<BridgeIdentifyRun>,
    },
    /// Nothing to look up — no disc-ID artifact and no barcode source. The UI
    /// offers manual search. Distinct from `NotFoundAnywhere` (signals ran,
    /// matched nothing). The run is there when extraction found catalog
    /// numbers the person can still activate.
    ManualOnly {
        track_count: u32,
        run: Option<BridgeIdentifyRun>,
    },
    /// At least one automatic provider lookup failed. The stored failure waits
    /// for an explicit re-run rather than being retried by the queue sweep.
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
        narrowed_out: BridgeNarrowedOut,
        catalog_agreements: Vec<BridgeCatalogAgreement>,
    },
}

/// Which automatic lookup failed, and — where several providers answer one —
/// which provider. The disc-ID endpoint is MusicBrainz's alone and release
/// details come from the source that named the release, so those name none.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeIdentifyFailure {
    DiscId {
        failure: BridgeLookupFailure,
    },
    /// Reading the candidate's barcodes failed, so no provider was asked.
    BarcodeScan {
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
    /// One provider could not answer the title search the run fell back on.
    Search {
        source: BridgeCatalog,
        failure: BridgeLookupFailure,
    },
    ReleaseDetails {
        failure: BridgeLookupFailure,
    },
}
