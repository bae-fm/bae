use super::super::*;

/// A folder the user watches for imports — one candidate-list group.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWatchedFolder {
    /// Absolute path of the watched folder.
    pub path: String,
    /// Final path component — the group header label.
    pub name: String,
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeWatchedFolder = bae_core::import::WatchedFolder,
    from_core: pub fn,
    fields: { path, name },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFileInfo {
    pub name: String,
    pub size: u64,
    /// Directory prefix for display, e.g. "Artwork/". `None` when the file
    /// sits at the candidate-folder root.
    pub dir_prefix: Option<String>,
    /// Filename without directory, e.g. "front.jpg".
    pub file_name: String,
    /// Absolute filesystem path of the file on disk.
    pub local_path: String,
    /// Probe-verified source audio facts. `None` for non-audio files.
    pub audio_format: Option<BridgeAudioFormat>,
}

/// Whether one of a candidate's audio files can back a sheet's binding. Mirror
/// of bae-core's `SheetBindingOffer`. Core decides this by probing, so no UI
/// reads a codec to work out what it may offer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSheetBindingOffer {
    /// The sheet can be bound to this audio.
    Offered,
    /// bae can't carve tracks out of that codec. The UI localizes `codec`
    /// through [`bridge_sheet_refused_codec_key`] — the same wording a sheet
    /// the scan already refused carries.
    RefusedCodec { codec: String },
    /// The sheet names boundaries outside this file's measured duration.
    RefusedTiming,
    /// bae can't read the file at all. Localized, like every other refusal,
    /// through [`bridge_sheet_binding_offer_key`].
    RefusedUnreadable,
}

impl BridgeSheetBindingOffer {
    pub(crate) fn loc_key(&self) -> Option<&'static str> {
        match self {
            Self::Offered => None,
            Self::RefusedCodec { .. } => Some(SHEET_REFUSED_CODEC_KEY),
            Self::RefusedTiming => Some(SHEET_REFUSED_TIMING_KEY),
            Self::RefusedUnreadable => Some(SHEET_REFUSED_UNREADABLE_KEY),
        }
    }
}

/// Localization key for why a file cannot back a sheet's binding — resolved by
/// the UI against the `Core` string table, interpolating `codec` where the
/// variant carries one. `None` for a file that *is* offerable: it needs no
/// reason, which is what makes an offer and a refusal distinguishable without a
/// UI reading the variant.
#[uniffi::export]
pub fn bridge_sheet_binding_offer_key(offer: BridgeSheetBindingOffer) -> Option<String> {
    offer.loc_key().map(str::to_string)
}

/// One of a candidate's audio files, as a choice for a sheet's binding. The set
/// crosses already filtered to what the sheet can use, each refusal carrying
/// its reason: offering a file the commit would reject is the failure the
/// editable binding exists to remove.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSheetBindingOption {
    /// The audio file's `name` (its release-relative path) — the id
    /// `AppHandle::set_sheet_binding` takes, and the one to match against
    /// `BridgeFileInfo.name` for anything else the row shows.
    pub file_id: String,
    pub offer: BridgeSheetBindingOffer,
}

/// Localization key for a refused sheet binding — resolved by the UI against the
/// `Core` string table, with the codec interpolated. One key, so the reason a
/// binding was refused reads the same on every surface.
#[uniffi::export]
pub fn bridge_sheet_refused_codec_key() -> String {
    SHEET_REFUSED_CODEC_KEY.to_string()
}

pub(crate) const SHEET_REFUSED_CODEC_KEY: &str = "core.import.sheet.refused_codec";
pub(crate) const SHEET_REFUSED_TIMING_KEY: &str = "core.import.sheet.refused_timing";
pub(crate) const SHEET_REFUSED_UNREADABLE_KEY: &str = "core.import.sheet.refused_unreadable";

/// The job the scan proposed for one file. Mirror of bae-core's `FileRole`. No
/// UI decides a file's role, and no UI infers a pairing from a filename.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeFileRole {
    Audio,
    /// A parsed track sheet.
    TrackSheet {
        /// Playable tracks the sheet carves.
        track_count: u32,
    },
    Artwork {
        choice: BridgeCoverChoice,
    },
    Document,
    /// In the folder and carried with the release, unrecognized — a scene
    /// sidecar, a stray video, a file with no extension.
    Other,
}

/// The catalog key naming the role in force for a file — the roles table's
/// Role column. Core's concept, so core's wording: two UIs naming these
/// differently is two answers about what the release holds.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_file_role_key(role: &BridgeFileRole) -> String {
    match role {
        BridgeFileRole::Audio => "core.import.role.audio",
        BridgeFileRole::TrackSheet { .. } => "core.import.role.track_sheet",
        BridgeFileRole::Artwork { .. } => "core.import.role.artwork",
        BridgeFileRole::Document => "core.import.role.document",
        BridgeFileRole::Other => "core.import.role.other",
    }
    .to_string()
}

/// The name of the service a pick came from — "MusicBrainz", "Discogs".
///
/// A brand name, so it is not translated and needs no catalog key.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_metadata_source_name(source: crate::types::BridgeMetadataSource) -> String {
    source.name().to_string()
}

/// A role a person can put a file in, as opposed to the whole
/// [`BridgeFileRole`] the scan proposes. Mirror of bae-core's
/// `FileRoleChoice`. Only audio is a decision: an image is an image, and a
/// track sheet's job is decided by what it is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFileRoleChoice {
    /// One of the release's tracks.
    Audio,
    /// Carried with the release — the folder is the release — but not one of
    /// its tracks. What a slot's Exclude action writes.
    NotATrack,
}

/// The catalog key naming one file-role choice.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_file_role_choice_key(choice: BridgeFileRoleChoice) -> String {
    match choice {
        BridgeFileRoleChoice::Audio => "core.import.role.audio",
        BridgeFileRoleChoice::NotATrack => "core.import.role.not_a_track",
    }
    .to_string()
}

/// What a file's role makes of it in the release being imported — the roles
/// table's "Becomes" column, as a consequence rather than as prose. Mirror of
/// bae-core's `FileBecomes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFileBecomes {
    /// Track slots `first`..=`last`, counting the release's slots from one.
    /// `first == last` is the single-slot case a loose audio file produces.
    Slots { first: u32, last: u32 },
    /// Nothing in the tracklist. Still carried with the release.
    NoSlots,
}

/// The catalog key naming what a file becomes. The single-slot case has its own
/// key because "slot 12" and "slots 1–11" are different sentences in most
/// languages, not one sentence with a range in it.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_file_becomes_key(becomes: BridgeFileBecomes) -> String {
    match becomes {
        BridgeFileBecomes::Slots { first, last } if first == last => "core.import.becomes.slot",
        BridgeFileBecomes::Slots { .. } => "core.import.becomes.slots",
        BridgeFileBecomes::NoSlots => "core.import.becomes.not_a_track",
    }
    .to_string()
}

/// One file of a candidate, with the role in force for it and what that role
/// makes of it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateFile {
    pub file: BridgeFileInfo,
    pub role: BridgeFileRole,
    /// Which of the release's track slots this file backs. The one fact the
    /// role does not already say, and what makes the effect of a binding or an
    /// exclusion legible without reading the slot table below.
    pub becomes: BridgeFileBecomes,
    /// The roles this file can be put in, the one in force first. Empty when
    /// its role is nobody's decision to make, which is every file the scan did
    /// not read as audio.
    pub alternatives: Vec<BridgeFileRoleChoice>,
    /// The role in force as a choice — what a picker shows selected. `None`
    /// exactly when `alternatives` is empty.
    pub role_choice: Option<BridgeFileRoleChoice>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateSourceAudio {
    /// Aggregate source-audio facts for the collapsed release line.
    pub summary: BridgeSourceAudioSummary,
    /// Every physical audio file contributing to the summary, in
    /// release-relative path order.
    pub files: Vec<BridgeFileInfo>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateFiles {
    /// Core-derived identity of the audio files behind a File Tags preview.
    pub file_tags_identity: String,
    /// Every file in the folder, each exactly once, in release-relative path
    /// order.
    pub files: Vec<BridgeCandidateFile>,
    /// Core-derived aggregate and physical files for the effective source audio.
    pub source_audio: Option<BridgeCandidateSourceAudio>,
}

/// Phase-0 preparation step, mirroring bae-core's `PrepareStep`. The UI
/// localizes each variant via its catalog key (`bridge_prepare_step_key`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePrepareStep {
    Queued,
    ValidatingSourceFiles,
}

impl BridgePrepareStep {
    pub(crate) fn loc_key(self) -> &'static str {
        match self {
            Self::Queued => "core.import.prepare.queued",
            Self::ValidatingSourceFiles => "core.import.prepare.validating_source_files",
        }
    }
}

/// Running phase, mirroring bae-core's `ImportPhase`. Localized via
/// `bridge_import_phase_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeImportPhase {
    ReadingFiles,
    MeasuringLoudness,
    Finalizing,
}

impl BridgeImportPhase {
    pub(crate) fn loc_key(self) -> &'static str {
        match self {
            Self::ReadingFiles => "core.import.phase.reading_files",
            Self::MeasuringLoudness => "core.import.phase.measuring_loudness",
            Self::Finalizing => "core.import.phase.finalizing",
        }
    }
}

/// Which step of an import is in progress, mirroring bae-core's `ImportStep`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeImportStep {
    Preparing { step: BridgePrepareStep },
    Running { phase: BridgeImportPhase },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgePrepareStep = bae_core::import::PrepareStep,
    from_core: fn,
    variants: { Queued, ValidatingSourceFiles },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeImportPhase = bae_core::import::ImportPhase,
    from_core: fn,
    variants: { ReadingFiles, MeasuringLoudness, Finalizing },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeImportStep = bae_core::import::ImportStep,
    from_core: pub(crate) fn,
    variants: {
        Preparing(step: (BridgePrepareStep)),
        Running(phase: (BridgeImportPhase)),
    },
}

/// Localization key for a prepare step — resolved by the UI against the `Core`
/// string table. One source for every platform.
#[uniffi::export]
pub fn bridge_prepare_step_key(step: BridgePrepareStep) -> String {
    step.loc_key().to_string()
}

/// Localization key for an import phase.
#[uniffi::export]
pub fn bridge_import_phase_key(phase: BridgeImportPhase) -> String {
    phase.loc_key().to_string()
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryStatus {
    pub release_id: String,
    pub release_in_library: bool,
    pub album_in_library: bool,
    pub album_title: Option<String>,
    pub album_id: Option<String>,
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeLibraryStatus = bae_core::db::LibraryStatus,
    from_core: pub(crate) fn,
    fields: {
        release_id,
        release_in_library,
        album_in_library,
        album_title,
        album_id,
    },
}

/// What a candidate's identification asks about: the signals its runs leave
/// out, and the catalog numbers they look up. Mirrors
/// `bae_core::import::LookupChoices`.
///
/// One value, sent whole. A control that changes one part reads the candidate
/// detail's current value, changes that part, and sends the result back —
/// which is also what starts the run that reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct BridgeLookupChoices {
    /// Whether the run leaves the candidate's disc ID out.
    pub disc_id_excluded: bool,
    /// Whether the run leaves the candidate's barcodes out.
    pub barcode_excluded: bool,
    /// The catalog numbers the run looks up, each on its own, in the order
    /// they were chosen.
    pub chosen_catalogs: Vec<String>,
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeLookupChoices = bae_core::import::LookupChoices,
    from_core: pub(crate) fn,
    into_core: pub fn,
    fields: { disc_id_excluded, barcode_excluded, chosen_catalogs },
}

/// Where a signal value was harvested from — what a badge shows on hover
/// ("from Cover OCR", "from the folder name", …). Mirrors
/// `bae_core::signals::SignalOrigin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalOrigin {
    DiscToc,
    CueSheet,
    Artwork,
    FolderName,
    Filename,
    TextFile,
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeSignalOrigin = bae_core::signals::SignalOrigin,
    from_core: pub(crate) fn,
    variants: { DiscToc, CueSheet, Artwork, FolderName, Filename, TextFile },
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
    /// Whether this is the one the identify run is using. At most one option of
    /// a signal is chosen.
    pub chosen: bool,
}

/// One badge in the signals toolbar — a pre-shaped row the UI renders without
/// deriving anything. Mirrors `bae_core::identify::ToolbarSignal`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeToolbarSignal {
    pub kind: BridgeSignalKind,
    pub value: Option<String>,
    pub origin: BridgeSignalOrigin,
    pub state: BridgeSignalState,
    pub excluded: bool,
    /// The values this signal could take. Empty for the disc ID and the
    /// barcode, which have one value each; the catalog's are every number
    /// extracted from the candidate.
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
    BridgeToolbarSignal = bae_core::identify::ToolbarSignal,
    from_core: fn,
    fields: {
        kind: (BridgeSignalKind),
        value,
        origin: (BridgeSignalOrigin),
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
    pub source: BridgeMetadataSource,
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
    /// A disc ID was read and the one source that answers disc IDs was not
    /// asked, so nothing looked it up. The value stands with no count.
    ReadNotAsked {
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
        /// The numbers not chosen, in the order they were first seen.
        candidates: Vec<BridgeCatalogCandidate>,
    },
}

/// A run as its ledger: the three signals, each with what extraction produced
/// for it and every provider's lookup of it. Mirrors
/// `bae_core::identify::IdentifyRunView`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeIdentifyRun {
    /// The providers the run asks, in the order their cells are listed.
    pub providers: Vec<BridgeMetadataSource>,
    pub disc_id: BridgeDiscIdStep,
    pub barcode: BridgeBarcodeStep,
    pub catalog: BridgeCatalogStep,
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

/// The releases agreement left out of a state's matches — real answers a real
/// lookup returned that the intersection discarded, and the ones the folder's
/// own text says nothing about. Shaped exactly as a state's own matches, so a
/// surface lists them the same way. Empty when nothing was narrowed. Mirrors
/// `bae_core::identify::NarrowedOutView`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeNarrowedOut {
    pub groups: Vec<BridgeReleaseGroup>,
    pub library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
    pub agreements: std::collections::HashMap<String, BridgeAgreements>,
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
        /// Library status per matched release, keyed by release id, so the
        /// UI looks up a row's status directly without re-indexing a flat
        /// list.
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        track_count: u32,
        /// Per-pressing agreements keyed by release id — the per-row badges,
        /// and what ordered the rows.
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        /// The releases the agreement left out of `groups`, for the surface to
        /// offer behind a disclosure.
        narrowed_out: BridgeNarrowedOut,
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
    /// with the failures named beside them. `groups` is empty when nothing
    /// answered, and for a failure resumed from its stored verdict.
    Failed {
        run: Option<BridgeIdentifyRun>,
        failures: Vec<BridgeIdentifyFailure>,
        groups: Vec<BridgeReleaseGroup>,
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        agreements: std::collections::HashMap<String, BridgeAgreements>,
        narrowed_out: BridgeNarrowedOut,
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
        source: BridgeMetadataSource,
        failure: BridgeLookupFailure,
    },
    Catalog {
        source: BridgeMetadataSource,
        failure: BridgeLookupFailure,
    },
    ReleaseDetails {
        failure: BridgeLookupFailure,
    },
}

// ── Unified UI event system ─────────────────────────────────────────────
