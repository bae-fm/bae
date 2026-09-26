#[cfg(feature = "desktop")]
use super::*;
use super::{
    BridgeDiscogsDetail, BridgeImageRef, BridgeMediaCount, BridgePackaging, BridgePressingFacts,
    BridgeReleaseArea, BridgeReleaseStatus, BridgeSourceAudioLayout, BridgeSourceAudioSummary,
    BridgeTrackSide,
};

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseDetail {
    pub release_id: String,
    pub source: BridgeCatalog,
    /// Per-source release group: MB release-group ID for MusicBrainz,
    /// Discogs master ID for Discogs. `None` when the source didn't
    /// surface a group — the picked release commits without a group
    /// identity row, but Approximate is still meaningful as "I don't
    /// claim this specific pressing."
    pub source_group_id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub barcode: Option<String>,
    pub facts: BridgePressingFacts,
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
    pub side: Option<u32>,
}

/// One kind of identifying signal extracted from a candidate file. Mirrors
/// `bae_core::import::FileEvidence`.
///
/// This is independent of which pressing is selected. The chip goes on the
/// file: the gallery tile for an image barcodes were read from, or the table
/// row for the log or cue a disc ID was computed from.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeFileEvidence {
    pub signal: BridgeEvidenceSignal,
    /// The extracted barcode digits or disc ID.
    pub value: String,
    /// The file's identity within the release (its relative path): the same id
    /// `BridgeMappingImage` and `BridgeMappingFile` carry.
    pub file_id: String,
}

/// A signal that can name the file it was read off. Mirrors
/// `bae_core::import::EvidenceSignal`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeEvidenceSignal {
    /// A barcode read off one of the folder's images.
    Barcode,
    /// A disc ID computed from a rip log or a cue sheet.
    DiscId,
}

/// The catalog key wording one piece of evidence, for the hover the file's
/// tile or row carries. Each message takes the value as its one argument.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_file_evidence_key(evidence: &BridgeFileEvidence) -> String {
    match evidence.signal {
        BridgeEvidenceSignal::Barcode => "core.import.evidence.barcode_in_image",
        BridgeEvidenceSignal::DiscId => "core.import.evidence.disc_id_from_file",
    }
    .to_string()
}

/// Mirror of `bae_core::import::ReleaseUserEdit` — a normalized, validated
/// metadata edit ready to apply.
///
/// `tracks` MUST line up with the release's existing tracks in order; edits
/// cannot add or remove tracks. `album_artist_assignments` is positional —
/// element 0 becomes the primary album artist (`album.artist_id`),
/// subsequent elements get higher `album_artists.position` rows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseUserEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<BridgeArtistAssignment>,
    pub album_year: Option<i32>,
    pub pressing: BridgePressingEdit,
    pub tracks: Vec<BridgeTrackUserEdit>,
}

/// Mirror of `bae_core::pressing::Pressing` as an edit claims it: a
/// release's identifiers and year, and what it is. Per-field `None` means
/// "this field isn't set".
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePressingEdit {
    pub year: Option<i32>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub barcode: Option<String>,
    pub facts: BridgePressingFacts,
}

/// One per existing track, in track order.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackUserEdit {
    pub title: String,
    pub side: Option<i32>,
    pub track_number: Option<i32>,
    pub artist_assignments: BridgeTrackArtistAssignments,
    /// Which of the folder's audio holds this track's samples. An import's rows
    /// are its track slots, so this is the pairing the user left and the one the
    /// commit writes; a row with no audio is a slot nobody answered and does not
    /// become a track. The library's metadata editor never re-binds files, so
    /// its rows carry `None`.
    pub file: Option<BridgeAudioFile>,
}

/// One artist credited on an album or track. Mirror of
/// `bae_core::import::ArtistAssignment`: a library artist the person picked,
/// or a credit — what a source or the person's typing said, which claims
/// nothing about the library. How a credit stands to the library is read
/// beside the draft ([`BridgeResolvedCredit`]) and asked of core with
/// `bridge_artist_standing` / `bridge_artists_standing`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeArtistAssignment {
    Picked { artist: BridgeExistingArtist },
    Credit { credit: BridgeArtistCredit },
}

/// One selected artist already in the library. The assignment carries the
/// fields the editor renders; candidate storage keeps only `artist_id` and
/// resolves these fields again when it is read.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeExistingArtist {
    pub artist_id: String,
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

mirror_struct! {
    BridgeExistingArtist = bae_core::import::ExistingArtist,
    from_core: pub(crate) fn,
    #[cfg(feature = "desktop")]
    into_core: pub(crate) fn,
    fields: {
        artist_id,
        name,
        sort_name,
        musicbrainz_artist_id,
        discogs_artist_id,
    },
}

/// What a source or a person said about an artist. Mirror of
/// `bae_core::import::ArtistCredit`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct BridgeArtistCredit {
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

/// What the library holds for one credit, as it stood when read. Mirror of
/// `bae_core::import::CreditResolution`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCreditResolution {
    /// The credit names this library artist.
    Library { artist: BridgeExistingArtist },
    /// No library artist is this one: committing creates it.
    New,
    /// Several library artists carry the name and no catalog id tells them
    /// apart. Committing creates a new artist unless the person picks one.
    Ambiguous { artists: Vec<BridgeExistingArtist> },
    /// The credit's catalog ids point at library artists that disagree with
    /// it. Committing fails until the person picks one, or merges them.
    Conflicting { artists: Vec<BridgeExistingArtist> },
}

/// One credit and what it resolves to. Mirror of
/// `bae_core::import::ResolvedCredit`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeResolvedCredit {
    pub credit: BridgeArtistCredit,
    pub resolution: BridgeCreditResolution,
}

/// How one assigned artist stands to the library — what its badge says.
/// Mirror of `bae_core::import::ArtistStanding`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeArtistStanding {
    Library,
    New,
    /// Several library artists could be this one; `choices` are the ones the
    /// person may pick.
    Choose {
        choices: Vec<BridgeExistingArtist>,
    },
}

/// How a whole artist field stands to the library. Mirror of
/// `bae_core::import::ArtistsStanding`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeArtistsStanding {
    Library,
    New,
    SomeNew { count: u32 },
    Choose { choices: u32 },
    SomeToChoose { count: u32 },
}

/// How `assignment` stands to the library, read from `resolutions` — the
/// credits' answers the draft's read, or `resolve_artist_credits`, returned.
/// `None` for a credit `resolutions` has no answer for yet.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_artist_standing(
    assignment: BridgeArtistAssignment,
    resolutions: Vec<BridgeResolvedCredit>,
) -> Option<BridgeArtistStanding> {
    let resolutions: Vec<_> = resolutions
        .into_iter()
        .map(BridgeResolvedCredit::into_core)
        .collect();
    assignment
        .into_core()
        .standing(&resolutions)
        .map(BridgeArtistStanding::from_core)
}

/// How the artist field holding `assignments` stands to the library, read
/// from `resolutions`. `None` for an empty field, or one with a credit
/// `resolutions` has no answer for yet.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_artists_standing(
    assignments: Vec<BridgeArtistAssignment>,
    resolutions: Vec<BridgeResolvedCredit>,
) -> Option<BridgeArtistsStanding> {
    let assignments: Vec<_> = assignments
        .into_iter()
        .map(BridgeArtistAssignment::into_core)
        .collect();
    let resolutions: Vec<_> = resolutions
        .into_iter()
        .map(BridgeResolvedCredit::into_core)
        .collect();
    bae_core::import::artists_standing(&assignments, &resolutions)
        .map(BridgeArtistsStanding::from_core)
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTrackArtistAssignments {
    AlbumArtists,
    Explicit {
        assignments: Vec<BridgeArtistAssignment>,
    },
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

/// Whether a slot row's two lengths — the folder's own and the selected
/// release's — are far enough apart that the row should say so.
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
pub fn bridge_lengths_disagree(file_ms: Option<u64>, release_ms: Option<u64>) -> bool {
    bae_core::import::lengths_disagree(file_ms, release_ms)
}

/// The catalog key naming the reconciliation line, or `None` where there is no
/// line to draw.
///
/// Two sides that account for the same rows say nothing the table is not
/// already showing, so an agreement draws nothing. The tally itself stays whole
/// in core — it is what a later edit re-derives a disagreement from — and this
/// is where the decision not to state it lives, once, for both desktops.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_slot_reconciliation_key(reconciliation: BridgeSlotReconciliation) -> Option<String> {
    match reconciliation {
        BridgeSlotReconciliation::Agrees { .. } => None,
        BridgeSlotReconciliation::MoreFiles { .. } => {
            Some("core.import.reconciliation.more_files".to_string())
        }
        BridgeSlotReconciliation::MoreTracks { .. } => {
            Some("core.import.reconciliation.more_tracks".to_string())
        }
    }
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
/// it heads a group of rows — and images live in the table's gallery instead.
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
    /// The whole-file target when this file currently supplies audio.
    pub preview_target: Option<BridgePreviewTarget>,
    /// Playing time in milliseconds from the scan's stored facts. `None` for
    /// non-audio files.
    pub duration_ms: Option<u64>,
    pub audio_format: Option<BridgeAudioFormat>,
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
    /// This slice's stored source duration: the next sheet boundary, or the
    /// scanned container duration closing the final entry, in milliseconds.
    pub duration_ms: Option<u64>,
    /// The container this entry's samples come from — what auditioning plays.
    pub container_id: String,
    pub container_name: String,
    pub container_local_path: String,
    /// The exact window of the container that auditioning this entry plays.
    pub preview_target: BridgePreviewTarget,
    pub audio_format: BridgeAudioFormat,
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

/// The exact candidate revisions on which a rendered source offer is based.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCandidateAsRead {
    pub content_hash: String,
    pub file_edit_revision: u64,
    pub metadata_revision: u64,
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
        /// The position this row commits, rendered by core from the track's
        /// own side and number and the release's format — `8`, `A1`, or `3`
        /// beneath a `Disc 2` heading.
        position: String,
        /// Whether the source's tracklist contains this track — false exactly
        /// for a row that exists only because audio was found for it.
        named_by_source: bool,
    },
    /// Available audio omitted from the release, and the read that offered it.
    NotIncluded {
        audio: BridgeAudioFile,
        candidate: BridgeCandidateAsRead,
    },
    /// No release is picked yet, so what this becomes is the open question.
    AwaitingPick,
}

/// One source-to-track mapping row. Mirror of bae-core's `TrackMapping`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackMapping {
    pub source: BridgeMappingSource,
    pub becomes: BridgeMappingBecomes,
    /// The duration to render: metadata's value where present, otherwise the
    /// candidate's stored probe. Available before metadata is chosen.
    pub duration_ms: Option<u64>,
}

/// The audio a track sheet describes. Mirror of bae-core's `MappingContainer`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingContainer {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub audio_format: BridgeAudioFormat,
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
    pub size: u64,
    /// Absolute path — what opening the sheet to read it reaches.
    pub local_path: String,
    pub bound: BridgeSheetBound,
    /// Current FILE associations and permitted choices, in sheet reference order.
    pub reference_options: Vec<BridgeSheetReferenceOptions>,
    pub assignment: BridgeSheetDisc,
    /// The discs this sheet may be assigned to, counting from one.
    pub disc_options: Vec<u32>,
}

/// What a track sheet describes, with the facts its header shows about it.
/// Mirror of bae-core's `SheetBound`.
///
/// Core's sheet binding enriched with the container facts its header renders.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSheetBound {
    /// The sheet describes this audio.
    Describes {
        container: BridgeMappingContainer,
    },
    /// The sheet describes several audio files. Each track row names its own
    /// physical file, so the group header has no single container.
    DescribesFiles {
        audio_file_count: u32,
    },
    /// It describes nothing: the directive named audio that is not in the
    /// folder, named several and only some are here, or the user cleared the
    /// binding. `requested` is what the directive asked for, so the header can
    /// say what the sheet was looking for while it offers the folder's own
    /// audio instead.
    Unresolved {
        requested: Vec<String>,
    },
    /// The directive resolved, but bae cannot carve tracks out of that codec.
    /// The physical audio files import independently.
    /// The UI localizes `codec` through `bridge_sheet_refused_codec_key`.
    RefusedCodec {
        codec: String,
    },
    RefusedTiming,
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
}

/// What supplies the rows of one side or disc. Mirror of bae-core's
/// `MappingTrackSectionContent`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingTrackSectionContent {
    /// Rows supplied independently rather than carved by a track sheet.
    Tracks { mappings: Vec<BridgeTrackMapping> },
    /// A track sheet and the entries it carves, which are its child rows.
    Sheet {
        sheet: BridgeSheetGroup,
        entries: Vec<BridgeTrackMapping>,
    },
}

/// One side or disc in the track table. Mirror of bae-core's
/// `MappingTrackSection`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingTrackSection {
    pub side: BridgeTrackSide,
    /// Localization key for the section header word, or `None` when the
    /// section has no heading.
    pub header_key: Option<String>,
    pub content: BridgeMappingTrackSectionContent,
}

/// One row in the files section. Mirror of bae-core's `MappingFileRow`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingFileRow {
    File {
        file: BridgeMappingFile,
    },
    /// A sheet that currently carves no track rows and can be assigned audio.
    Sheet {
        sheet: BridgeSheetGroup,
    },
}

/// The mapping table: every source unit the folder offers, alongside the track
/// committing makes of it. Mirror of bae-core's `MappingTable`.
///
/// Each source carries either its included editable track or the exact offer
/// that can add it. Removing a track leaves the available audio visible.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingTable {
    /// Every image the folder holds, in the scan's authoritative order.
    pub images: Vec<BridgeMappingImage>,
    pub track_sections: Vec<BridgeMappingTrackSection>,
    pub files: Vec<BridgeMappingFileRow>,
    /// The tally over the rows that become tracks. `None` when there is nothing
    /// to reconcile the folder against — no release is picked, or the tracklist
    /// was read off the folder's own files and so cannot disagree with it.
    pub reconciliation: Option<BridgeSlotReconciliation>,
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

/// One album-level text field of the import pane's metadata form.
///
/// Each is written on its own as the user leaves it, so the pane holds no copy
/// of the form: the field commits, the per-candidate query redraws. What the
/// pressing is is chosen rather than typed: [`BridgePressingFactEdit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCandidateEditField {
    AlbumTitle,
    AlbumYear,
    PressingYear,
    Label,
    CatalogNumber,
    Barcode,
}

mirror_enum! {
    BridgeCandidateEditField = bae_core::import::CandidateEditField,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        AlbumTitle,
        AlbumYear,
        PressingYear,
        Label,
        CatalogNumber,
        Barcode,
    },
}

/// One choice of what the pressing is, made with a picker that offers only
/// the vocabulary's values. Mirrors `bae_core::import::PressingFactEdit`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePressingFactEdit {
    Area {
        area: Option<BridgeReleaseArea>,
    },
    Media {
        media: Vec<BridgeMediaCount>,
    },
    Status {
        status: Option<BridgeReleaseStatus>,
    },
    Packaging {
        packaging: Option<BridgePackaging>,
    },
    DiscogsDetails {
        discogs_details: Vec<BridgeDiscogsDetail>,
    },
}

#[cfg(feature = "desktop")]
impl BridgePressingFactEdit {
    pub(crate) fn into_core(self) -> bae_core::import::PressingFactEdit {
        use bae_core::import::PressingFactEdit;
        match self {
            Self::Area { area } => PressingFactEdit::Area(area.map(BridgeReleaseArea::into_core)),
            Self::Media { media } => PressingFactEdit::Media(
                media.into_iter().map(BridgeMediaCount::into_core).collect(),
            ),
            Self::Status { status } => {
                PressingFactEdit::Status(status.map(BridgeReleaseStatus::into_core))
            }
            Self::Packaging { packaging } => {
                PressingFactEdit::Packaging(packaging.map(BridgePackaging::into_core))
            }
            Self::DiscogsDetails { discogs_details } => PressingFactEdit::DiscogsDetails(
                discogs_details
                    .into_iter()
                    .map(BridgeDiscogsDetail::into_core)
                    .collect(),
            ),
        }
    }

    /// This choice, applied to the facts a form holds — what a form bound to
    /// a value rather than a candidate does with it.
    pub(crate) fn applied_to(self, facts: BridgePressingFacts) -> BridgePressingFacts {
        let mut facts = facts.into_core();
        self.into_core().apply(&mut facts);
        BridgePressingFacts::from_core(facts)
    }
}

/// `facts` with `edit` applied, the way the draft applies it: a medium with a
/// count of zero is left out, a detail is kept once. The library release
/// editor holds its form itself and applies each choice through this.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_apply_pressing_fact(
    facts: BridgePressingFacts,
    edit: BridgePressingFactEdit,
) -> BridgePressingFacts {
    edit.applied_to(facts)
}

/// Raw edit-metadata form values, exactly as the editor holds them — text
/// as typed, not yet normalized. Mirrors `bae_core::import::RawReleaseEdit`.
/// The editor binds directly to this shape and calls `shape_release_edit` to
/// normalize + validate it into a wire `BridgeReleaseUserEdit`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawReleaseEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<BridgeArtistAssignment>,
    pub album_year: String,
    pub pressing: BridgeRawPressingEdit,
    pub tracks: Vec<BridgeRawTrackEdit>,
}

/// The raw edit form for one library release plus core's answer about whether
/// its stored metadata source can be projected again.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditSeed {
    pub edit: BridgeRawReleaseEdit,
    pub can_reset_to_source: bool,
    pub cover: Option<BridgeImageRef>,
    pub display: BridgeReleaseEditDisplayContext,
}

/// One persisted file that supplies samples for a track in the release editor.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditTrackSource {
    pub file_id: String,
    pub name: String,
    pub layout: BridgeSourceAudioLayout,
}

/// Persisted, read-only context beside one editable track row.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditTrackContext {
    pub track_id: String,
    pub sources: Vec<BridgeReleaseEditTrackSource>,
    pub duration_ms: Option<i64>,
    pub side: BridgeTrackSide,
    pub side_header_key: Option<String>,
}

/// Persisted release facts used to render the shared metadata editor.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditDisplayContext {
    pub source_audio: Option<BridgeSourceAudioSummary>,
    pub tracks: Vec<BridgeReleaseEditTrackContext>,
}

/// Raw pressing fields as the editor holds them. Mirrors
/// `bae_core::import::RawPressingEdit`: each text field is the text the user
/// typed, empty meaning "not set"; `year` is text (parsed at shape time).
/// `facts` is what the person chose with the form's pickers.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawPressingEdit {
    pub year: String,
    pub label: String,
    pub catalog_number: String,
    pub barcode: String,
    pub facts: BridgePressingFacts,
}

/// One raw track row from the editor. Mirrors
/// `bae_core::import::RawTrackEdit`: `id` is the stable `ForEach` row
/// identity and its explicit artist assignment mode.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawTrackEdit {
    pub id: String,
    pub title: String,
    pub artist_assignments: BridgeTrackArtistAssignments,
    pub side: Option<i32>,
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
    EmptyArtistName,
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
            Self::EmptyArtistName => "core.import.validation.empty_artist_name",
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
