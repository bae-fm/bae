#[cfg(desktop)]
use super::*;
use super::{
    BridgeCreditLine, BridgeDiscogsDetail, BridgeDurationClock, BridgeMediaCount, BridgeMedium,
    BridgePackaging, BridgePressingFacts, BridgeReleaseArea, BridgeReleaseLabel,
    BridgeReleaseStatus, BridgeSourceAudioLayout, BridgeTrackSide,
};

/// Mirror of `bae_import::ReleaseUserEdit` — a normalized, validated
/// metadata edit ready to apply.
///
/// `tracks` MUST line up with the release's existing tracks in order; edits
/// cannot add or remove tracks. `album_artist_assignments` is the album's
/// credit list in order; element *n* is the `album_artists` row at position *n*.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseUserEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<BridgeCredited>,
    pub album_year: Option<i32>,
    pub pressing: BridgePressingEdit,
    pub tracks: Vec<BridgeTrackUserEdit>,
}

/// Mirror of `bae_types::pressing::Pressing` as an edit claims it: a
/// release's identifiers and year, and what it is. Per-field `None` means
/// "this field isn't set"; no labels means none is.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePressingEdit {
    pub year: Option<i32>,
    pub labels: Vec<BridgeReleaseLabel>,
    /// Every barcode the pressing is printed with, in order.
    pub barcodes: Vec<String>,
    pub facts: BridgePressingFacts,
}

/// One per existing track, in track order.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackUserEdit {
    pub title: String,
    pub artist_assignments: BridgeTrackArtistAssignments,
    /// Which of the folder's audio holds this track's samples. An import's rows
    /// are its track slots, so this is the pairing the user left and the one the
    /// commit writes; a row with no audio is a slot nobody answered and does not
    /// become a track. The library's metadata editor never re-binds files, so
    /// its rows carry `None`.
    pub file: Option<BridgeAudioFile>,
}

/// One artist credited on an album or track. Mirror of
/// `bae_import::ArtistAssignment`: a library artist the person picked,
/// or a credit — what a source or the person's typing said, which claims
/// nothing about the library. How a credit stands to the library comes with
/// the credit in the field that holds it ([`BridgeArtistField`]).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeArtistAssignment {
    /// A library artist, printed by `credit_name` (one of its other printed
    /// names) where the person picked one, else by its own name.
    Picked {
        artist: BridgeExistingArtist,
        credit_name: Option<BridgeCreditName>,
    },
    Credit {
        credit: BridgeArtistCredit,
    },
}

/// One place in an album's or track's credit list. Mirror of
/// `bae_import::Credited`: who is credited, and what is printed after
/// it when another credit follows (`None`: ", ").
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCredited {
    pub artist: BridgeArtistAssignment,
    pub join: Option<String>,
}

/// One of an artist's other printed names. Mirror of
/// `bae_records::credits::CreditName`: its folded key and the name as printed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct BridgeCreditName {
    pub key: String,
    pub name: String,
}

/// One of an artist's other printed names with the artist whose row holds
/// it (the artist, or one merged into it). Mirror of
/// `bae_records::credits::ArtistCreditName`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeArtistCreditName {
    pub artist_id: String,
    pub credit: BridgeCreditName,
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
    BridgeCreditName = bae_records::credits::CreditName,
    from_core: pub(crate) fn,
    #[cfg(desktop)]
    into_core: pub(crate) fn,
    fields: { key, name },
}

mirror_struct! {
    BridgeArtistCreditName = bae_records::credits::ArtistCreditName,
    from_core: pub(crate) fn,
    fields: { artist_id, credit: (BridgeCreditName) },
}

mirror_struct! {
    BridgeExistingArtist = bae_records::ExistingArtist,
    from_core: pub(crate) fn,
    #[cfg(desktop)]
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
/// `bae_catalogs::credits::ArtistCredit`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct BridgeArtistCredit {
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
    /// The name the source printed for the artist, where it is another.
    pub credited_as: Option<String>,
}

/// How one assigned artist stands to the library: what its badge says.
/// Mirror of `bae_import::ArtistStanding`.
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
/// `bae_import::ArtistsStanding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeArtistsStanding {
    Library,
    New,
    SomeNew { count: u32 },
    Choose { choices: u32 },
    SomeToChoose { count: u32 },
}

/// One artist field (an album's artists, or a track's own) as the read that
/// returned it found it: each credit with its badge, in the field's order,
/// and the whole field's badge. Mirror of `bae_import::ArtistField`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeArtistField {
    pub standing: BridgeArtistsStanding,
    pub credits: Vec<BridgeStandingCredit>,
}

/// One credit of an artist field with how it stands to the library: what its
/// badge says. Mirror of `bae_import::StandingCredit`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeStandingCredit {
    pub credited: BridgeCredited,
    pub standing: BridgeArtistStanding,
}

/// The line a credit list prints: each credit's printed name, with its join,
/// the last credit's join never kept.
#[cfg(desktop)]
#[uniffi::export]
pub fn bridge_credit_line(credits: Vec<BridgeCredited>) -> BridgeCreditLine {
    let credits: Vec<_> = credits.into_iter().map(BridgeCredited::into_core).collect();
    BridgeCreditLine::from_core(bae_import::credited_line(&credits))
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTrackArtistAssignments {
    AlbumArtists,
    Explicit { assignments: Vec<BridgeCredited> },
}

/// The audio a track's samples come from. Mirrors
/// `bae_import::AudioFile`. `file_id` is the file's identity within the
/// release (its relative path), the same id the sheet bindings use.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAudioFile {
    /// The whole file holds this one track.
    Standalone { file_id: String },
    /// One of several tracks the track sheet `sheet_id` carves out of
    /// `file_id`. `index` counts that sheet's playable tracks from zero.
    SheetSlice { file_id: String, sheet_id: String, index: u32 },
}

/// Which disc of the release one track sheet's entries become. Mirror of
/// bae-library's `SheetDisc`.
///
/// Cue filenames are arbitrary — `CD1.cue` may hold disc two — so this is a
/// decision, set through `AppHandle::set_sheet_disc`, and never something a UI
/// reads off a name.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSheetDisc {
    /// The sheet's entries are the release's disc `number`, counting from one.
    Disc { number: u32 },
    /// The sheet contributes nothing to the tracklist. Its container is loose
    /// audio again.
    Ignored,
}

/// What one of a release's files is, as a row of the mapping table or of a
/// library release's files. Mirror of bae-import's `MappingRole`.
///
/// Narrower than the role the scan proposes: a folder's track sheet is not a
/// row here — it heads a group of rows — and a folder's images live in the
/// table's gallery instead.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMappingRole {
    Audio,
    /// Read as text in the document viewer.
    Document,
    /// An image a library release carries, which has no gallery of its own.
    Image,
    /// Named as an image, but its bytes do not decode as one; carried, never
    /// in the gallery.
    DamagedImage,
    /// Named as audio, and bae can't play it: only a folder that failed
    /// validation holds one.
    DamagedAudio {
        damage: crate::types::BridgeAudioDamage,
    },
    Other,
}

/// A file of the folder, as the mapping table's left half shows it, or a
/// file of a library release, as its editor lists it. Mirror of bae-import's
/// `MappingFile`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingFile {
    /// The id the file is read by: in a folder, its relative path, which the
    /// sheet bindings take; in the library, its release file's id.
    pub file_id: String,
    /// The file's path within the release.
    pub name: String,
    pub size: u64,
    /// The whole-file target when this file currently supplies audio.
    pub preview_target: Option<BridgePreviewTarget>,
    pub role: BridgeMappingRole,
}

/// One entry of a track sheet, as the mapping table's left half shows it.
/// Mirror of bae-library's `MappingEntry`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingEntry {
    pub sheet_id: String,
    /// Counts this sheet's playable entries from zero — the index the audio
    /// binding carries.
    pub index: u32,
    /// The number the sheet prints for this entry.
    pub number: u32,
    /// Where the entry starts in its container, as a clock label's fields
    /// ("10:22"): what the row's source cell shows.
    pub start: BridgeDurationClock,
    /// The container this entry's samples come from — what auditioning plays.
    pub container_id: String,
    /// The exact window of the container that auditioning this entry plays.
    pub preview_target: BridgePreviewTarget,
}

/// The left half of a mapping row: what the folder offers for it. Mirror of
/// bae-library's `MappingSource`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingSource {
    /// A file the folder holds, whole.
    File { file: BridgeMappingFile },
    /// One entry of a track sheet, carved out of the container it is bound to.
    SheetEntry { entry: BridgeMappingEntry },
}

/// One source-to-track mapping row. Mirror of bae-library's `TrackMapping`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackMapping {
    pub source: BridgeMappingSource,
    /// The track of the release being committed. The row edits it in place.
    pub track: BridgeRawTrackEdit,
    /// The position this row commits, rendered by core from the track's own
    /// side and number and the release's format — `8`, `A1`, or `3` beneath a
    /// `Disc 2` heading.
    pub position: String,
    /// The length the row's audio measures, the one the import stores for
    /// the track.
    pub length: BridgeDurationClock,
    /// The length the track's source lists, present only when more than three
    /// seconds from the measured one — core's judgement, so every surface
    /// marks the same rows. The listed length marks a row and disables
    /// nothing.
    pub listed_length: Option<BridgeDurationClock>,
    /// The track's own artists, each with how it stands to the library;
    /// `None` when the track takes its album's artists or names none.
    pub artists: Option<BridgeArtistField>,
}

/// The audio a track sheet describes. Mirror of bae-library's `MappingContainer`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingContainer {
    pub file_id: String,
    pub name: String,
    pub size: u64,
}

/// A track sheet, as the header of the group of rows it carves. Mirror of
/// bae-library's `SheetGroup`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSheetGroup {
    /// The sheet's `file_id` — the id `AppHandle::set_sheet_binding` and
    /// `AppHandle::set_sheet_disc` take.
    pub sheet_id: String,
    pub name: String,
    pub size: u64,
    pub bound: BridgeSheetBound,
    /// The sheet's FILE lines that play audio, as the caption's Audio file
    /// pop-up offers them.
    pub bound_audio: BridgeSheetBoundAudio,
    /// The sheet's FILE lines that play no audio, in the sheet's order, each
    /// offered from a menu of its own; `None` where every line plays audio.
    pub unbound_references: Option<Vec<BridgeSheetReferenceOptions>>,
    pub assignment: BridgeSheetDisc,
    /// The discs this sheet may be assigned to, counting from one.
    pub disc_options: Vec<u32>,
}

/// The FILE lines of a track sheet that play audio, by how the caption's
/// Audio file pop-up offers them. Mirror of bae-import's `SheetBoundAudio`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSheetBoundAudio {
    /// No line plays audio, so the caption names what the sheet describes
    /// as text.
    None,
    /// One line plays audio: its choices sit directly in the pop-up.
    One { reference: BridgeSheetReferenceOptions },
    /// Several lines play audio: the pop-up has a submenu per line, in the
    /// sheet's order.
    Several { references: Vec<BridgeSheetReferenceOptions> },
}

/// What a track sheet describes, with the facts its header shows about it.
/// Mirror of bae-library's `SheetBound`.
///
/// Core's sheet binding enriched with the container facts its header renders.
#[cfg(desktop)]
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

/// One of the folder's images, as the gallery shows it. Mirror of bae-library's
/// `MappingImage`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingImage {
    /// The file's identity within the release (its relative path).
    pub file_id: String,
    /// The file's own name, without its directory prefix.
    pub name: String,
    /// The version the scan read — what a thumbnail and the lightbox draw.
    pub file: BridgeFileVersion,
}

/// Consecutive rows of one side that the table draws together. Mirror of
/// bae-library's `MappingTrackRun`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingTrackRun {
    /// Rows supplied independently rather than carved by a track sheet.
    Tracks { mappings: Vec<BridgeTrackMapping> },
    /// A track sheet and the entries it carves, which are its child rows.
    Sheet { sheet: BridgeSheetGroup, entries: Vec<BridgeTrackMapping> },
}

/// One side or disc of the track table and the runs of rows on it, headed
/// once. Mirror of bae-library's `MappingTrackSide`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingTrackSide {
    pub side: BridgeTrackSide,
    /// Localization key for the side's header word, or `None` when the side
    /// has no heading.
    pub header_key: Option<String>,
    pub runs: Vec<BridgeMappingTrackRun>,
}

/// One row in the files section. Mirror of bae-library's `MappingFileRow`.
#[cfg(desktop)]
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
/// committing makes of it. Mirror of bae-library's `MappingTable`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingTable {
    /// Every image the folder holds, in the scan's authoritative order.
    pub images: Vec<BridgeMappingImage>,
    /// The rows that can become release tracks, laid out by side.
    pub track_sides: Vec<BridgeMappingTrackSide>,
    pub files: Vec<BridgeMappingFileRow>,
    /// How many track rows nobody has named yet. Committing anyway titles
    /// each after its file.
    pub untitled_track_count: u32,
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
}

#[cfg(desktop)]
mirror_enum! {
    BridgeCandidateEditField = bae_import::CandidateEditField,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        AlbumTitle,
        AlbumYear,
        PressingYear,
    },
}

/// One choice of what the pressing is, made with a picker that offers only
/// the vocabulary's values. Mirrors `bae_import::PressingFactEdit`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePressingFactEdit {
    Area { area: Option<BridgeReleaseArea> },
    Media { media: Vec<BridgeMediaCount> },
    Status { status: Option<BridgeReleaseStatus> },
    Packaging { packaging: Option<BridgePackaging> },
    DiscogsDetails { discogs_details: Vec<BridgeDiscogsDetail> },
}

#[cfg(desktop)]
impl BridgePressingFactEdit {
    pub(crate) fn into_core(self) -> bae_import::PressingFactEdit {
        use bae_import::PressingFactEdit;
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
                discogs_details.into_iter().map(BridgeDiscogsDetail::into_core).collect(),
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
/// count of zero is left out, a detail is kept once. A form bound to a value
/// rather than to a draft in core, as a preview's is, applies each choice
/// through this.
#[cfg(desktop)]
#[uniffi::export]
pub fn bridge_apply_pressing_fact(
    facts: BridgePressingFacts,
    edit: BridgePressingFactEdit,
) -> BridgePressingFacts {
    edit.applied_to(facts)
}

/// Raw edit-metadata form values, exactly as the editor shows them — text
/// as typed, not yet normalized. Mirrors `bae_import::RawReleaseEdit`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawReleaseEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<BridgeCredited>,
    pub album_year: String,
    pub pressing: BridgeRawPressingEdit,
    pub tracks: Vec<BridgeRawTrackEdit>,
}

/// What a pressing's Add menus offer: the media and Discogs details its facts
/// do not hold yet. Mirrors `bae_import::PressingOffers`; the draft's read
/// hands it beside the edit, and the menus draw it as given.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePressingOffers {
    pub media: Vec<BridgeMedium>,
    pub discogs_details: Vec<BridgeDiscogsDetail>,
}

/// The library album a draft files under when it is written: an import's, or
/// a library release's editor's. Mirrors `bae_import::DraftAlbum`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDraftAlbum {
    /// A new album, because the person picked none and the draft's records
    /// name none the library holds.
    New,
    /// A new album the person picked, over what the records name.
    PickedNew,
    /// The album the draft's records name; for a library release with no
    /// catalog applied, the album it is in.
    Named { album_id: String, line: BridgeAlbumLine },
    /// The album the person picked, over what the records name.
    Picked { album_id: String, line: BridgeAlbumLine },
}

/// How the album pick names a library album: its title, then its credit
/// line when the album credits anyone. Mirrors `bae_import::AlbumLine`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAlbumLine {
    /// The album credits no one, so its title names it alone.
    TitleAlone {
        title: String,
    },
    TitleAndArtists {
        title: String,
        artist_line: BridgeCreditLine,
    },
}

#[cfg(desktop)]
mirror_enum! {
    BridgeAlbumLine = bae_import::AlbumLine,
    from_core: fn,
    variants: {
        TitleAlone { title },
        TitleAndArtists { title, artist_line: (BridgeCreditLine) },
    },
}

mirror_enum! {
    #[cfg(desktop)]
    BridgeDraftAlbum = bae_import::DraftAlbum,
    from_core: pub(crate) fn,
    variants: {
        New,
        PickedNew,
        Named { album_id, line: (BridgeAlbumLine) },
        Picked { album_id, line: (BridgeAlbumLine) },
    },
}

/// What the person chose for the album a draft files under. Mirrors
/// `bae_import::AlbumPick`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAlbumPick {
    /// Nothing: the album the draft's records name, else the album the
    /// release is in, else a new album.
    Records,
    /// A new album, even where the records name one the library holds.
    New,
    /// This library album, over what the records name.
    Album { album_id: String },
}

mirror_enum! {
    #[cfg(desktop)]
    BridgeAlbumPick = bae_import::AlbumPick,
    into_core: pub(crate) fn,
    variants: {
        Records,
        New,
        Album { album_id },
    },
}

/// What a release editor's save returns: the album the release is on after
/// the save, and the form opened on what the save wrote, which the editor
/// holds from then on.
#[cfg(desktop)]
#[derive(uniffi::Record)]
pub struct BridgeReleaseEditSaved {
    pub album_id: String,
    pub form: std::sync::Arc<crate::BridgeReleaseEditForm>,
}

/// A library release's editor's draft beside the form it holds, which the
/// editor draws itself (`BridgeReleaseEditForm::edit`): the release's cover
/// as it is now, the summary of its audio, its tracks in the table's sides
/// beside their form rows, the files no track plays from, the album a save
/// files the release under, how the form's album artists stand to the
/// library, whether a save would write anything the editor did not open on,
/// and why the form can't be saved, when it can't. Mirrors
/// `bae_import::ReleaseEditDraft`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditDraft {
    pub cover: Option<BridgeImageRef>,
    pub source_audio: Option<BridgeSourceAudioSummary>,
    /// The release's tracks in the sides and runs the table draws, each beside
    /// the form row that edits it; `None` when the release has no tracks.
    pub sides: Option<Vec<BridgeReleaseEditTrackSide>>,
    /// The files the release carries that no track plays from, in the
    /// release's file order: what its Files section lists under the tracks.
    /// `None` when it carries none.
    pub files: Option<Vec<BridgeMappingFile>>,
    pub album: BridgeDraftAlbum,
    /// The catalogs that describe the release the form holds, the applied
    /// one first.
    pub records: Vec<BridgeReleaseRecord>,
    /// Whether `records` holds any.
    pub has_records: bool,
    /// The media and Discogs details the form's pressing Add menus offer.
    pub pressing_offers: BridgePressingOffers,
    /// The form's album artists, each with how it stands to the library;
    /// `None` when the form names no album artist.
    pub album_artists: Option<BridgeArtistField>,
    pub changed: bool,
    /// The names of the audio files a save reads before it writes, the ones
    /// the form binds CUE sheets' `FILE` lines to, in name order; empty when
    /// a save reads none.
    pub files_to_read: Vec<String>,
    pub invalid: Option<BridgeValidationReason>,
}

/// The persisted file a track's body plays from in the release editor.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditTrackSource {
    pub file_id: String,
    pub name: String,
    pub layout: BridgeSourceAudioLayout,
}

/// The CUE sheet that carves a run of the release editor's tracks out of
/// their file, as the run's caption names it. Mirrors
/// `bae_import::ReleaseEditSheet`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditSheet {
    pub file_id: String,
    /// The sheet's file name, as the release's files name it.
    pub name: String,
    /// The disc of the release the sheet is, counted from one: the disc the
    /// editor's form puts its tracks on.
    pub disc_number: u32,
    /// The discs the release holds, which the caption's Disc pop-up offers.
    pub disc_options: Vec<u32>,
    /// The text of the sheet's `FILE` line the run's tracks are under.
    pub file_reference: String,
    /// The audio files the line can play from, which the caption's Audio
    /// file pop-up offers, in file name order.
    pub audio_options: Vec<BridgeReleaseEditAudioOption>,
}

/// One audio file of the release as a choice for a CUE sheet's `FILE` line
/// in the release editor. Mirrors `bae_import::ReleaseEditAudioOption`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditAudioOption {
    pub file_id: String,
    /// The file's name, as the release's files name it.
    pub name: String,
    /// Whether the line plays from this file in the editor's form.
    pub bound: bool,
    /// Whether the file can back the line, and why not when it can't, as
    /// the import pane's Audio file pop-up words it.
    pub offer: crate::types::BridgeSheetBindingOffer,
}

/// Persisted, read-only context beside one editable track row.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditTrackContext {
    pub track_id: String,
    pub source: BridgeReleaseEditTrackSource,
    /// Where the track's body starts in its source file, as a clock label's
    /// fields ("10:22"): what a row under a CUE image's heading shows.
    pub start_clock: BridgeDurationClock,
    /// The track's stored length as a clock label's fields ("3:07").
    pub duration_clock: BridgeDurationClock,
    /// The length the track's source lists, present only when more than
    /// three seconds from the stored one, as the import's mapping table
    /// marks a row.
    pub listed_length: Option<BridgeDurationClock>,
    /// How the track's place reads ("A1", "3"), which the editor shows and
    /// never edits.
    pub position_text: String,
}

/// One row of the editor's track table: the form row that edits a track,
/// the library's facts about the track, and the track's own artists, each
/// with how it stands to the library (`None` when it takes its album's or
/// names none). Mirrors `bae_import::ReleaseEditTrackRow`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditTrackRow {
    pub edit: BridgeRawTrackEdit,
    pub context: BridgeReleaseEditTrackContext,
    pub artists: Option<BridgeArtistField>,
}

/// Consecutive rows of one side that the editor's table draws together.
/// Mirrors `bae_import::ReleaseEditTrackRun`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeReleaseEditTrackRun {
    /// A track whose row names its own source.
    Track { row: BridgeReleaseEditTrackRow },
    /// Two or more consecutive tracks a CUE sheet carves from one file, which
    /// is named once above them with the sheet.
    Sheet {
        sheet: BridgeReleaseEditSheet,
        source: BridgeReleaseEditTrackSource,
        rows: Vec<BridgeReleaseEditTrackRow>,
    },
}

/// One side of the release in the editor's track table and the runs of
/// rows on it. Mirrors `bae_import::ReleaseEditTrackSide`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseEditTrackSide {
    pub side: BridgeTrackSide,
    /// Localization key for the side's header word, or `None` when the side
    /// has no heading.
    pub header_key: Option<String>,
    pub runs: Vec<BridgeReleaseEditTrackRun>,
}

/// Raw pressing fields as the editor draws them. Mirrors
/// `bae_import::RawPressingEdit`: each text field is the text the user
/// typed, empty meaning "not set"; `year` is text (parsed at shape time).
/// `labels` and `barcodes` are the rows the editor draws: one per label or
/// barcode, or one blank row when there is none. `facts` is what the person
/// chose with the form's pickers.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawPressingEdit {
    pub year: String,
    pub labels: Vec<BridgeLabelRow>,
    pub barcodes: Vec<BridgeBarcodeRow>,
    pub facts: BridgePressingFacts,
}

/// What a row of a list the editor draws carries: the list's heading on the
/// first row, the control that adds a row on the last, and a remove control
/// on every row but a lone blank one. Mirrors the flags of
/// `bae_import::ListRow`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeListPlace {
    pub labelled: bool,
    pub removable: bool,
    pub adds_row: bool,
}

/// One label row the editor draws.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeLabelRow {
    pub label: BridgeRawLabelEdit,
    pub place: BridgeListPlace,
}

/// One barcode row the editor draws.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeBarcodeRow {
    pub barcode: String,
    pub place: BridgeListPlace,
}

/// One label row of the editor, as the editor writes it. Mirrors
/// `bae_import::RawLabelEdit`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeRawLabelEdit {
    pub name: String,
    pub catalog_number: String,
}

/// One raw track row from the editor. Mirrors
/// `bae_import::RawTrackEdit`: `id` is the stable `ForEach` row
/// identity and its explicit artist assignment mode.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawTrackEdit {
    pub id: String,
    pub title: String,
    pub artist_assignments: BridgeTrackArtistAssignments,
    /// The audio bound to this row. An editor must carry it through untouched:
    /// dropping it when rebuilding a row from its text fields is what unpairs a
    /// track the user had already paired.
    pub file: Option<BridgeAudioFile>,
}

/// Why an edit can't be saved. An FFI mirror of bae-types'
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
    pub(crate) fn loc_key(self) -> &'static str {
        match self {
            Self::EmptyAlbumTitle => "core.import.validation.empty_album_title",
            Self::NoAlbumArtist => "core.import.validation.no_album_artist",
            Self::EmptyArtistName => "core.import.validation.empty_artist_name",
            Self::InvalidYear => "core.import.validation.invalid_year",
        }
    }
}

mirror_enum! {
    /// Kept here, not as a `From` in bae-library, so bae-library stays unaware of
    /// bridge types.
    BridgeValidationReason = bae_types::edit_validation::EditValidationError,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { EmptyAlbumTitle, NoAlbumArtist, EmptyArtistName, InvalidYear },
}

/// Outcome of shaping a raw edit form (`shape_release_edit`). `Valid` carries
/// the savable wire edit; `Invalid` carries the typed reason it can't be saved.
/// The editor enables Save on `Valid` and renders the localized reason on
/// `Invalid` — bae-library decides which reason, the UI localizes it.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeShapeResult {
    Valid { edit: BridgeReleaseUserEdit },
    Invalid { reason: BridgeValidationReason },
}
