use super::super::*;

/// A folder the user watches for imports — one candidate-list group.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWatchedFolder {
    /// Absolute path of the watched folder.
    pub path: String,
    /// Final path component — the group header label.
    pub name: String,
}

#[cfg(feature = "desktop")]
impl BridgeWatchedFolder {
    pub fn from_core(folder: bae_core::import::WatchedFolder) -> Self {
        let bae_core::import::WatchedFolder { path, name } = folder;
        Self { path, name }
    }
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
    /// bae can't read the file at all. Localized through
    /// [`bridge_sheet_refused_unreadable_key`].
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

/// Localization key for audio bae cannot read, refused as a binding for that
/// reason rather than for its codec.
#[uniffi::export]
pub fn bridge_sheet_refused_unreadable_key() -> String {
    SHEET_REFUSED_UNREADABLE_KEY.to_string()
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

#[cfg(feature = "desktop")]
impl BridgeImportStep {
    pub(crate) fn from_core(s: bae_core::import::ImportStep) -> Self {
        use bae_core::import::{ImportPhase, ImportStep, PrepareStep};
        match s {
            ImportStep::Preparing(p) => BridgeImportStep::Preparing {
                step: match p {
                    PrepareStep::Queued => BridgePrepareStep::Queued,
                    PrepareStep::ValidatingSourceFiles => BridgePrepareStep::ValidatingSourceFiles,
                },
            },
            ImportStep::Running(phase) => BridgeImportStep::Running {
                phase: match phase {
                    ImportPhase::ReadingFiles => BridgeImportPhase::ReadingFiles,
                    ImportPhase::MeasuringLoudness => BridgeImportPhase::MeasuringLoudness,
                    ImportPhase::Finalizing => BridgeImportPhase::Finalizing,
                },
            },
        }
    }
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

#[cfg(feature = "desktop")]
impl BridgeLibraryStatus {
    pub(crate) fn from_core(s: bae_core::db::LibraryStatus) -> Self {
        let bae_core::db::LibraryStatus {
            release_id,
            release_in_library,
            album_in_library,
            album_title,
            album_id,
        } = s;
        Self {
            release_id,
            release_in_library,
            album_in_library,
            album_title,
            album_id,
        }
    }
}

/// A signal the user acted on in the toolbar. The disc ID and the barcode are
/// checked until toggled off; the catalog is off until one of the extracted
/// numbers is chosen, and choosing another replaces it — so its variant names
/// the value. Mirrors `bae_core::identify::SignalToggle`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalToggle {
    Disc,
    Barcode,
    Catalog { value: String },
}

impl BridgeSignalToggle {
    #[cfg(feature = "desktop")]
    pub fn into_core(self) -> bae_core::identify::SignalToggle {
        use bae_core::identify::SignalToggle;
        match self {
            Self::Disc => SignalToggle::Disc,
            Self::Barcode => SignalToggle::Barcode,
            Self::Catalog { value } => SignalToggle::Catalog(value),
        }
    }
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

#[cfg(feature = "desktop")]
impl BridgeSignalOrigin {
    fn from_core(o: bae_core::signals::SignalOrigin) -> Self {
        use bae_core::signals::SignalOrigin;
        match o {
            SignalOrigin::DiscToc => BridgeSignalOrigin::DiscToc,
            SignalOrigin::CueSheet => BridgeSignalOrigin::CueSheet,
            SignalOrigin::Artwork => BridgeSignalOrigin::Artwork,
            SignalOrigin::FolderName => BridgeSignalOrigin::FolderName,
            SignalOrigin::Filename => BridgeSignalOrigin::Filename,
            SignalOrigin::TextFile => BridgeSignalOrigin::TextFile,
        }
    }
}

/// A signal value paired with its origin — a catalog candidate or a barcode
/// code. Mirrors `bae_core::signals::SourcedValue`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSourcedValue {
    pub value: String,
    pub origin: BridgeSignalOrigin,
    /// The candidate-relative path of the file the value was read off — the id
    /// a gallery tile and a file row are keyed by, so a surface can put the
    /// value on the file it came from. `None` where the origin names no file.
    pub origin_path: Option<String>,
}

#[cfg(feature = "desktop")]
impl BridgeSourcedValue {
    pub(crate) fn from_core(s: bae_core::signals::SourcedValue) -> Self {
        let bae_core::signals::SourcedValue {
            value,
            origin,
            origin_path,
        } = s;
        Self {
            value,
            origin: BridgeSignalOrigin::from_core(origin),
            origin_path,
        }
    }
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

#[cfg(feature = "desktop")]
impl BridgeLookupFailure {
    pub(crate) fn from_core(f: bae_core::signals::LookupFailure) -> Self {
        use bae_core::signals::LookupFailure;
        match f {
            LookupFailure::Network => BridgeLookupFailure::Network,
            LookupFailure::Provider { status } => BridgeLookupFailure::Provider { status },
            LookupFailure::Timeout => BridgeLookupFailure::Timeout,
            LookupFailure::ArtworkAnalysis => BridgeLookupFailure::ArtworkAnalysis,
            LookupFailure::Diagnostic { detail } => BridgeLookupFailure::Diagnostic { detail },
        }
    }
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

#[cfg(feature = "desktop")]
impl BridgeSignalState {
    fn from_core(s: bae_core::identify::SignalState) -> Self {
        use bae_core::identify::SignalState;
        match s {
            SignalState::LookingUp => BridgeSignalState::LookingUp,
            SignalState::Found { count } => BridgeSignalState::Found { count },
            SignalState::NoMatch => BridgeSignalState::NoMatch,
            SignalState::Skipped => BridgeSignalState::Skipped,
            SignalState::Failed { failure } => BridgeSignalState::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSignalOption {
    fn from_core(o: bae_core::identify::SignalOption) -> Self {
        let bae_core::identify::SignalOption {
            value,
            origin,
            chosen,
        } = o;
        BridgeSignalOption {
            value,
            origin: BridgeSignalOrigin::from_core(origin),
            chosen,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeToolbarSignal {
    fn from_core(s: bae_core::identify::ToolbarSignal) -> Self {
        use bae_core::identify::{SignalKind, ToolbarSignal};
        let ToolbarSignal {
            kind,
            value,
            origin,
            state,
            excluded,
            options,
        } = s;
        BridgeToolbarSignal {
            kind: match kind {
                SignalKind::DiscId => BridgeSignalKind::DiscId,
                SignalKind::Barcode => BridgeSignalKind::Barcode,
                SignalKind::Catalog => BridgeSignalKind::Catalog,
            },
            value,
            origin: BridgeSignalOrigin::from_core(origin),
            state: BridgeSignalState::from_core(state),
            excluded,
            options: options
                .into_iter()
                .map(BridgeSignalOption::from_core)
                .collect(),
        }
    }
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

/// Per-signal disc-ID progress inside `Triangulating`. Settled variants
/// (`Done`, `Skipped`, `Failed`) tell the UI this pipe is finished.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDiscidProgress {
    Computing,
    LookingUp,
    Done {
        n_results: u32,
    },
    /// No disc-ID artifacts (LOG/CUE) available for this candidate.
    Skipped,
    Failed {
        failure: BridgeLookupFailure,
    },
}

/// Per-signal barcode progress inside `Triangulating`. `LookingUp` carries
/// position + total so the UI can render "Looking up barcode 2 of 3."
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeBarcodeProgress {
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
        failures: Vec<BridgeSourceFailure>,
    },
    /// Reading the candidate's barcodes failed, so no provider was asked.
    ScanFailed {
        failure: BridgeLookupFailure,
    },
    /// No artwork to scan.
    Skipped,
}

/// One provider that failed one lookup, and how. Mirrors
/// `bae_core::import::SourceFailure`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSourceFailure {
    pub source: BridgeMetadataSource,
    pub failure: BridgeLookupFailure,
}

#[cfg(feature = "desktop")]
impl BridgeSourceFailure {
    pub(crate) fn from_core(failure: bae_core::import::SourceFailure) -> Self {
        let bae_core::import::SourceFailure { source, failure } = failure;
        Self {
            source: BridgeMetadataSource::from_core(source),
            failure: BridgeLookupFailure::from_core(failure),
        }
    }
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

/// Which signals produced or confirmed one result. Mirrors
/// `bae_core::identify::ResultProvenance` — drives the per-row signal badges.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeResultProvenance {
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub by_catalog: bool,
}

/// Current identify-pipeline state for one candidate. One variant per state;
/// the UI reducer switches on the variant to render the right banner and
/// update the candidate.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeIdentifyState {
    Idle,
    /// Both signals running in parallel. Per-signal progress lets the UI
    /// show side-by-side pipes ("Computing disc-id ✓ · Looking up barcode
    /// 2 of 3..."). The pipeline transitions to a terminal state once
    /// both pipes settle.
    Triangulating {
        discid: BridgeDiscidProgress,
        barcode: BridgeBarcodeProgress,
    },
    Found {
        /// The matches as group cards, in match order — the UI renders one card
        /// per group with its pressings beneath. Usually one; signals that
        /// named different releases give several.
        groups: Vec<BridgeReleaseGroup>,
        /// Library status per matched release, keyed by release id, so the
        /// UI looks up a row's status directly without re-indexing a flat
        /// list.
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        track_count: u32,
        /// Per-pressing provenance keyed by release id — the per-row signal
        /// badges, and which signal produced each match.
        provenance: std::collections::HashMap<String, BridgeResultProvenance>,
    },
    NotFoundAnywhere,
    /// Nothing to look up — no disc-ID artifact and no barcode source. The UI
    /// offers manual search. Distinct from `NotFoundAnywhere` (signals ran,
    /// matched nothing).
    ManualOnly {
        track_count: u32,
    },
    /// At least one automatic provider lookup failed. The stored failure waits
    /// for an explicit re-run rather than being retried by the queue sweep.
    ///
    /// It still carries whatever the surviving evidence found: one provider
    /// failing leaves the other's matches standing, and the pane shows them
    /// with the failures named beside them. `groups` is empty when nothing
    /// answered, and for a failure resumed from its stored verdict.
    Failed {
        failures: Vec<BridgeIdentifyFailure>,
        groups: Vec<BridgeReleaseGroup>,
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        provenance: std::collections::HashMap<String, BridgeResultProvenance>,
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
