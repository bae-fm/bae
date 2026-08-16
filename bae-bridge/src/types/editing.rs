#[cfg(feature = "desktop")]
use super::*;

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSearchQueryKind {
    General,
    CatalogNumber,
    Barcode,
}

/// Returned from `search_for_candidate`. Echoes the `tab` and `source` the
/// search ran against so the caller can route results into the matching
/// (tab, source) slot — the user may have changed tabs or sources during
/// the await.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateSearchResults {
    pub tab: BridgeSearchQueryKind,
    pub source: BridgeMetadataSource,
    /// Results grouped into release-group cards, one card per group with its
    /// pressings beneath.
    pub groups: Vec<BridgeReleaseGroup>,
    /// Per-release library dupe statuses, looked up by release id.
    pub statuses: Vec<BridgeLibraryStatus>,
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseDetail {
    pub release_id: String,
    pub source: BridgeMetadataSource,
    /// Per-source release group: MB release-group ID for MusicBrainz,
    /// Discogs master ID for Discogs. `None` when the source didn't
    /// surface a group — the picked release commits without a group
    /// identity row, but Approximate is still meaningful as "I don't
    /// claim this specific pressing."
    pub source_group_id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    pub track_count: u32,
    pub tracks: Vec<BridgeReleaseTrack>,
    pub cover_art: Vec<BridgeRemoteCover>,
    pub default_cover: Option<BridgeCoverChoice>,
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseTrack {
    pub title: String,
    pub artist: Option<String>,
    pub duration_ms: Option<u64>,
    /// Raw position string as the metadata source reports it ("A1", "1",
    /// "1-2", or arbitrary prose). Shown verbatim in the import preview.
    pub position: String,
    pub side: u32,
}

/// What picking a release in the import search gives the confirmation pane.
///
/// `detail` is display: covers, source positions, the track count to reconcile
/// against the folder. `seed` is the metadata editor's starting value, projected
/// from the release exactly as the commit worker maps it — so the UI must seed
/// the editor from `seed`, never from `detail`, or an untouched artist list reads
/// as an edit at commit and the release loses its secondary album artists.
///
/// `seed` arrives already masked for `claim.choice` (an album-level claim blanks
/// the pressing block), so the UI binds it straight to the editor. The claim
/// itself came in on the pick, and lowering it is another pick — so there is no
/// re-shaping for the UI to do either way.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleasePrefetch {
    pub detail: BridgeReleaseDetail,
    pub seed: BridgeReleaseUserEdit,
    pub claim: BridgeClaimLine,
    /// The picked release's own pressing fields, whatever the claim reaches to
    /// — what claiming this pressing exactly is a claim *about*. The control
    /// that makes that claim restores these values, and
    /// `bridge_claim_for_edit` reads an edit against them.
    pub exact_pressing: BridgeRawPressingEdit,
    /// The file↔release pairing this pick produces: every source unit the
    /// folder offers with the track committing makes of it, the editable row
    /// inside the row that produces it. Empty for a key that names no scanned
    /// folder.
    pub mapping: BridgeMappingTable,
}

/// What identified the picked release. Mirrors
/// `bae_core::import::ClaimEvidence`. It explains the pick and decides nothing:
/// the UI renders it as the claim sentence's trailing clause, and the claim
/// itself is the user's, carried on the pick.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeClaimEvidence {
    /// The disc's table of contents matched this release and no other.
    DiscIdAlone,
    /// The disc's table of contents matched, and `match_count` releases came
    /// back for it.
    DiscIdShared { match_count: u32 },
    /// A barcode read off the packaging matched.
    Barcode,
    /// A catalog number, or a search the user typed, found it.
    Search,
}

/// The release header's claim line. Mirrors `bae_core::import::ClaimLine`.
///
/// Two facts: `choice` is what the import claims you physically hold, and
/// `release` is the release the metadata was read from. They coincide only for
/// a pressing claim; `level` says which of the two sentences the line reads as,
/// which side of the header's claim control is in force, and — since only an
/// album claim leaves the metadata's release unsaid — whether the second line
/// naming it is drawn.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeClaimLine {
    /// The claim this import will record, and what commit writes.
    pub choice: BridgeIdentityChoice,
    /// How far the claim reaches, as the user set it.
    pub level: BridgeClaimLevel,
    pub evidence: BridgeClaimEvidence,
    /// The picked release by its pressing facts — format, year, country and
    /// catalog number, `·`-joined. `None` when the source states none of them,
    /// and the sentence then reads without a description.
    pub release: Option<String>,
    /// The picked release's track count, where the source stated one. Rendered
    /// on the metadata-from line.
    pub track_count: Option<u32>,
}

/// Mirror of `bae_core::import::ReleaseUserEdit` — a normalized, validated
/// metadata edit ready to apply.
///
/// `tracks` MUST line up with the release's existing tracks in order; edits
/// cannot add or remove tracks. `album_artist_names` is positional —
/// element 0 becomes the primary album artist (`album.artist_id`),
/// subsequent elements get higher `album_artists.position` rows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseUserEdit {
    pub album_title: String,
    pub album_artist_names: Vec<String>,
    pub pressing: BridgePressingEdit,
    pub tracks: Vec<BridgeTrackUserEdit>,
}

/// Mirror of `bae_core::import::PressingEdit`. Groups the six pressing
/// fields a release carries; per-field `None` means "this field isn't set".
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePressingEdit {
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
}

/// One per existing track, in track order. `artist_names` empty means the
/// track shares the album artist (no per-track artist rows). Non-empty is
/// positional — element N becomes `track_artists.position = N`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackUserEdit {
    pub title: String,
    pub side: i32,
    pub track_number: Option<i32>,
    pub artist_names: Vec<String>,
    /// Which of the folder's audio holds this track's samples. An import's rows
    /// are its track slots, so this is the pairing the user left and the one the
    /// commit writes; a row with no audio is a slot nobody answered and does not
    /// become a track. The library's metadata editor never re-binds files, so
    /// its rows carry `None`.
    pub file: Option<BridgeAudioFile>,
}

/// The audio a track's samples come from. Mirrors
/// `bae_core::import::AudioFile`. `file_id` is the file's identity within the
/// release (its relative path), the same id the file-roles table and the sheet
/// bindings use.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAudioFile {
    /// The whole file holds this one track.
    Standalone { file_id: String },
    /// One of several tracks the track sheet `sheet_id` carves out of
    /// `file_id`. `index` counts that sheet's playable tracks from zero.
    SheetSlice {
        file_id: String,
        sheet_id: String,
        index: u32,
    },
}

/// The tally above the slot table: how many files the folder offers against how
/// many tracks the source names, and which way they disagree. Mirror of
/// bae-core's `SlotReconciliation`.
///
/// Arrives computed rather than left to each UI to subtract, and it is stated
/// rather than enforced — a disagreement is something to read, never something
/// that disables the commit.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSlotReconciliation {
    Agrees { count: u32 },
    MoreFiles { files: u32, tracks: u32 },
    MoreTracks { files: u32, tracks: u32 },
}

/// Whether a slot row's two lengths — the file's own, probed off disk, and the
/// one the source states — are far enough apart that the row should say so.
///
/// Core's judgement, not each surface's: how much two rips of one track may
/// legitimately differ is one question, and two UIs each picking a number is
/// two answers to it. `false` when either side has no number, because there is
/// nothing to compare, which is not the same as agreeing.
///
/// Asked per row as it renders rather than carried on the slot, so a row the
/// user re-points at a different file is answered about the pairing it has now.
/// It marks a row; it disables nothing.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_lengths_disagree(probed_ms: Option<u64>, source_ms: Option<u64>) -> bool {
    bae_core::import::lengths_disagree(probed_ms, source_ms)
}

/// The catalog key naming the reconciliation line.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_slot_reconciliation_key(reconciliation: BridgeSlotReconciliation) -> String {
    match reconciliation {
        BridgeSlotReconciliation::Agrees { .. } => "core.import.reconciliation.agrees",
        BridgeSlotReconciliation::MoreFiles { .. } => "core.import.reconciliation.more_files",
        BridgeSlotReconciliation::MoreTracks { .. } => "core.import.reconciliation.more_tracks",
    }
    .to_string()
}

/// Which disc of the release one track sheet's entries become. Mirror of
/// bae-core's `SheetDisc`.
///
/// Cue filenames are arbitrary — `CD1.cue` may hold disc two — so this is a
/// decision, set through `AppHandle::set_sheet_disc`, and never something a UI
/// reads off a name.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSheetDisc {
    /// The sheet's entries are the release's disc `number`, counting from one.
    Disc { number: u32 },
    /// The sheet contributes nothing to the tracklist. Its container is loose
    /// audio again.
    Ignored,
}

/// What one of the folder's files is, as a row of the mapping table. Mirror of
/// bae-core's `MappingRole`.
///
/// Narrower than the role the scan proposes: a track sheet is not a row here —
/// it heads a group of rows — and an image is not one either, because the
/// images are one gallery row rather than a row each.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMappingRole {
    Audio,
    Document,
    Other,
}

/// A file of the folder, as the mapping table's left half shows it. Mirror of
/// bae-core's `MappingFile`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingFile {
    /// The file's identity within the release (its relative path) — the id
    /// `AppHandle::set_file_role` and the sheet bindings take.
    pub file_id: String,
    /// The file's own name, without its directory prefix.
    pub name: String,
    pub size: u64,
    /// Absolute path — what auditioning this row plays.
    pub local_path: String,
    /// Probed playing time in milliseconds, where the folder's audio has been
    /// read. `None` for anything that is not audio, for audio nothing could be
    /// read from, and while no release is picked.
    pub probed_duration_ms: Option<u64>,
    pub role: BridgeMappingRole,
    /// The roles this file can be put in, the one in force first. Empty when
    /// its role is nobody's decision to make.
    pub alternatives: Vec<BridgeFileRoleChoice>,
    /// The role in force as a choice — what a picker shows selected. `None`
    /// exactly when `alternatives` is empty.
    pub role_choice: Option<BridgeFileRoleChoice>,
}

/// One entry of a track sheet, as the mapping table's left half shows it.
/// Mirror of bae-core's `MappingEntry`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingEntry {
    pub sheet_id: String,
    /// Counts this sheet's playable entries from zero — the index the audio
    /// binding carries.
    pub index: u32,
    /// The number the sheet prints for this entry.
    pub number: u32,
    pub title: Option<String>,
    /// How long the sheet says this entry runs, in milliseconds.
    pub duration_ms: Option<u64>,
    /// The container this entry's samples come from — what auditioning plays.
    pub container_id: String,
    pub container_name: String,
    pub container_local_path: String,
}

/// The left half of a mapping row: what the folder offers for it. Mirror of
/// bae-core's `MappingSource`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingSource {
    /// A file the folder holds, whole.
    File { file: BridgeMappingFile },
    /// One entry of a track sheet, carved out of the container it is bound to.
    SheetEntry { entry: BridgeMappingEntry },
    /// The source names a track this folder has nothing for: the left half is
    /// empty, and the row is offered the folder's audio to point it at.
    Missing,
}

/// The right half of a mapping row: what committing makes of the source unit.
/// Mirror of bae-core's `MappingBecomes`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingBecomes {
    /// A track of the release being committed. The row edits it in place, and
    /// `bridge_mapping_tracks` reads the edited rows back out in commit order.
    Track {
        track: BridgeRawTrackEdit,
        /// The source's own position string — `A1`, `1`, `1-2`, or prose —
        /// where the picked release names one for this track.
        source_position: Option<String>,
        source_duration_ms: Option<u64>,
    },
    /// Carried with the release, not one of its tracks.
    Kept,
    /// No release is picked yet, so what this becomes is the open question.
    AwaitingPick,
}

/// One source unit and the track committing makes of it. Mirror of bae-core's
/// `MappingUnit`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingUnit {
    pub source: BridgeMappingSource,
    pub becomes: BridgeMappingBecomes,
}

/// The audio a track sheet describes. Mirror of bae-core's `MappingContainer`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingContainer {
    pub file_id: String,
    pub name: String,
    pub size: u64,
}

/// A track sheet, as the header of the group of rows it carves. Mirror of
/// bae-core's `SheetGroup`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSheetGroup {
    /// The sheet's `file_id` — the id `AppHandle::set_sheet_binding` and
    /// `AppHandle::set_sheet_disc` take.
    pub sheet_id: String,
    pub name: String,
    /// Absolute path — what opening the sheet to read it reaches.
    pub local_path: String,
    pub bound: BridgeSheetBound,
    pub assignment: BridgeSheetDisc,
    /// The discs this sheet may be assigned to, counting from one.
    pub disc_options: Vec<u32>,
}

/// What a track sheet describes, with the facts its header shows about it.
/// Mirror of bae-core's `SheetBound`.
///
/// `BridgeSheetBinding` enriched by the container's name and size: a header
/// states both which audio a sheet is on and why it is on none, and carrying
/// the binding separately would be a second way to say the first.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSheetBound {
    /// The sheet describes this audio.
    Describes { container: BridgeMappingContainer },
    /// It describes nothing: the directive named audio that is not in the
    /// folder, named several and only some are here, or the user cleared the
    /// binding. `requested` is what the directive asked for, so the header can
    /// say what the sheet was looking for while it offers the folder's own
    /// audio instead.
    Unresolved { requested: Vec<String> },
    /// The directive resolved, but bae cannot carve tracks out of that codec.
    /// The UI localizes `codec` through `bridge_sheet_refused_codec_key`.
    RefusedCodec {
        container: BridgeMappingContainer,
        codec: String,
    },
}

/// One of the folder's images, as the gallery shows it. Mirror of bae-core's
/// `MappingImage`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingImage {
    /// The file's identity within the release (its relative path).
    pub file_id: String,
    /// The file's own name, without its directory prefix.
    pub name: String,
    pub size: u64,
    /// Absolute path — what a thumbnail and the lightbox read.
    pub local_path: String,
    /// Whether this is the image that leads the release.
    pub is_cover: bool,
}

/// One row of the mapping table. Mirror of bae-core's `MappingRow`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingRow {
    /// One source unit and what it becomes.
    Unit { unit: BridgeMappingUnit },
    /// A track sheet and the entries it carves, which are its child rows.
    Sheet {
        sheet: BridgeSheetGroup,
        entries: Vec<BridgeMappingUnit>,
    },
    /// Every image the folder holds, shown as one gallery.
    Images { images: Vec<BridgeMappingImage> },
    /// A directory whose files all do the same job, shown as one row.
    Directory { directory: BridgeCollapsedDirectory },
}

/// The mapping table: every source unit the folder offers, alongside the track
/// committing makes of it. Mirror of bae-core's `MappingTable`.
///
/// One structure, not two lists to keep aligned: the editable track row lives
/// *inside* the row that produces it, so removing a row removes both halves and
/// no index addresses anything.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingTable {
    pub rows: Vec<BridgeMappingRow>,
    /// The tally over the rows that become tracks. `None` when there is nothing
    /// to reconcile the folder against — no release is picked, or the tracklist
    /// was read off the folder's own files and so cannot disagree with it.
    pub reconciliation: Option<BridgeSlotReconciliation>,
}

/// What committing a folder as Unknown produces: the release its own files
/// describe, and the mapping table that lands each of its audio units on one of
/// those tracks.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUnknownMapping {
    pub seed: BridgeReleaseUserEdit,
    pub mapping: BridgeMappingTable,
}

/// The table's track rows in commit order — what the editor shapes into the
/// release it writes. Core decides the order, so the two desktop surfaces
/// cannot commit two different tracklists from one table.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_mapping_tracks(table: BridgeMappingTable) -> Vec<BridgeRawTrackEdit> {
    bae_core::import::mapping_tracks(&table.into_core())
        .into_iter()
        .map(BridgeRawTrackEdit::from_core)
        .collect()
}

/// Write an edited track row back onto the row that commits it, found by the
/// track's own id. A row nothing matches leaves the table alone.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_mapping_with_track(
    table: BridgeMappingTable,
    track: BridgeRawTrackEdit,
) -> BridgeMappingTable {
    BridgeMappingTable::from_core(bae_core::import::mapping_with_track(
        table.into_core(),
        track.into_core(),
    ))
}

/// Drop the row committing the track with `track_id` — the Drop action on a
/// track the release names that this folder has nothing for. Nothing is
/// persisted: the folder is unchanged, the release is committed without it.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_mapping_without_track(
    table: BridgeMappingTable,
    track_id: String,
) -> BridgeMappingTable {
    BridgeMappingTable::from_core(bae_core::import::mapping_without_track(
        table.into_core(),
        &track_id,
    ))
}

/// Drop every row the file `file_id` backs — the Exclude action, once the role
/// change that persists it has landed. One container backs every entry of the
/// sheet bound to it, so that sheet's whole group leaves with it.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_mapping_without_file(
    table: BridgeMappingTable,
    file_id: String,
) -> BridgeMappingTable {
    BridgeMappingTable::from_core(bae_core::import::mapping_without_file(
        table.into_core(),
        &file_id,
    ))
}

/// Raw edit-metadata form values, exactly as the editor holds them — text
/// as typed, not yet normalized. Mirrors `bae_core::import::RawReleaseEdit`.
/// The editor binds directly to this shape and calls `shape_release_edit` to
/// normalize + validate it into a wire `BridgeReleaseUserEdit`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawReleaseEdit {
    pub album_title: String,
    /// Comma-separated artist text in positional order, as typed.
    pub album_artist_text: String,
    pub pressing: BridgeRawPressingEdit,
    pub tracks: Vec<BridgeRawTrackEdit>,
}

/// Raw pressing fields as the editor holds them. Mirrors
/// `bae_core::import::RawPressingEdit`: each is the text the user typed,
/// empty meaning "not set"; `year` is text (parsed at shape time).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawPressingEdit {
    pub year: String,
    pub format: String,
    pub label: String,
    pub catalog_number: String,
    pub country: String,
    pub barcode: String,
}

/// One raw track row from the editor. Mirrors
/// `bae_core::import::RawTrackEdit`: `id` is the stable `ForEach` row
/// identity; `artist_text` empty means "share the album artist".
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawTrackEdit {
    pub id: String,
    pub title: String,
    pub artist_text: String,
    pub side: i32,
    pub track_number: Option<i32>,
    /// The audio bound to this row. An editor must carry it through untouched:
    /// dropping it when rebuilding a row from its text fields is what unpairs a
    /// track the user had already paired.
    pub file: Option<BridgeAudioFile>,
}

/// Why a release edit can't be saved. An FFI mirror of bae-core's
/// `EditValidationError`; the UI renders each variant by resolving its
/// localization key — see `bridge_validation_reason_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeValidationReason {
    EmptyAlbumTitle,
    NoAlbumArtist,
    InvalidYear,
}

impl BridgeValidationReason {
    /// The catalog key the UI resolves against the generated `Core` string
    /// table — the single source of the variant→key mapping for every platform.
    /// Only the desktop edit flow produces a validation reason, so the mapping
    /// is compiled there (and under test for the cross-check).
    #[cfg(feature = "desktop")]
    pub(crate) fn loc_key(self) -> &'static str {
        match self {
            Self::EmptyAlbumTitle => "core.import.validation.empty_album_title",
            Self::NoAlbumArtist => "core.import.validation.no_album_artist",
            Self::InvalidYear => "core.import.validation.invalid_year",
        }
    }
}

/// Outcome of shaping a raw edit form (`shape_release_edit`). `Valid` carries
/// the savable wire edit; `Invalid` carries the typed reason it can't be saved.
/// The editor enables Save on `Valid` and renders the localized reason on
/// `Invalid` — bae-core decides which reason, the UI localizes it.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeShapeResult {
    Valid { edit: BridgeReleaseUserEdit },
    Invalid { reason: BridgeValidationReason },
}
