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
/// of bae-core's `SheetBindingOffer`. Core decides from stored scan facts, so no
/// UI reads a codec to work out what it may offer.
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

/// One CUE FILE reference and the audio choices prepared by core.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSheetReferenceOptions {
    pub file_reference: String,
    pub file_id: Option<String>,
    pub options: Vec<BridgeSheetBindingOption>,
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
        /// Absent for previewable artwork the cover decoder does not support.
        choice: Option<BridgeCoverChoice>,
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

/// The name of the catalog a description came from — "MusicBrainz",
/// "Discogs", "Rate Your Music".
///
/// A brand name, so it is not translated and needs no catalog key.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_catalog_name(catalog: crate::types::BridgeCatalog) -> String {
    catalog.name().to_string()
}

/// Every catalog, in the one order surfaces list them in.
///
/// A surface that names the catalogs describing something reads them in this
/// order, so the order is core's and no surface states it again.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_catalogs() -> Vec<crate::types::BridgeCatalog> {
    bae_core::import::Catalog::ALL
        .map(crate::types::BridgeCatalog::from_core)
        .to_vec()
}

/// The catalogs bae asks — a search, a disc ID, a barcode — in the order
/// surfaces list them. The others hold pages a record links out to; nothing
/// asks them anything, so nothing offers them as something to switch on.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_lookup_catalogs() -> Vec<crate::types::BridgeCatalog> {
    bae_core::import::Catalog::LOOKUP
        .map(crate::types::BridgeCatalog::from_core)
        .to_vec()
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
    /// Artwork supported by the cover decoder, in source order.
    pub cover_files: Vec<BridgeCandidateFile>,
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

// ── Unified UI event system ─────────────────────────────────────────────
