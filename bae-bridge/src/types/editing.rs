#[cfg(desktop)]
use super::*;
use super::{
    BridgeCreditLine, BridgeDiscogsDetail, BridgeMediaCount, BridgeMedium, BridgePackaging,
    BridgePressingFacts, BridgeReleaseArea, BridgeReleaseLabel, BridgeReleaseStatus,
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

/// Which side or disc of the release one track sheet's entries become.
/// Mirror of bae-import's `SheetPlace`.
///
/// Cue filenames are arbitrary — `CD1.cue` may hold disc two — so this is a
/// decision, set through `AppHandle::set_sheet_place`, and never something a
/// UI reads off a name.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSheetPlace {
    /// The sheet's entries are on this side of one of the release's mediums.
    On { side: BridgeMediumSide },
    /// The sheet contributes nothing to the tracklist. Its container is loose
    /// audio again.
    Ignored,
}

/// One side of one of a release's mediums, each counted from one. Mirror of
/// bae-types' `MediumSide`.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMediumSide {
    pub medium: u32,
    pub side: u32,
}

/// One side or disc a CUE sheet can be put on, and the heading the release's
/// screens name it by ("Side B", "Disc 2"), which the sheet's place pop-up
/// names it by. Mirror of bae-records' `DbSheetSideChoice`.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSheetSideChoice {
    pub side: BridgeMediumSide,
    pub heading: crate::types::BridgeTrackSide,
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

/// One of a release's audio files, by its id and the name the screens show.
/// Mirror of bae-import's `MappingContainer`.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMappingContainer {
    pub file_id: String,
    pub name: String,
}

/// A track sheet, as the caption over the rows it carves, or as a line of
/// the Files list when it carves none. Mirror of bae-import's `SheetGroup`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSheetGroup {
    /// The sheet's file id: the id the commands that bind its lines and
    /// place it take.
    pub sheet_id: String,
    pub name: String,
    pub bound: BridgeSheetBound,
    /// The sheet's FILE lines that play audio, as the caption's Audio file
    /// pop-up offers them.
    pub bound_audio: BridgeSheetBoundAudio,
    /// The sheet's FILE lines that play no audio, in the sheet's order, each
    /// offered from a menu of its own; `None` where every line plays audio.
    pub unbound_references: Option<Vec<BridgeSheetReferenceOptions>>,
    /// The side or disc the sheet is on, with its heading, which the
    /// caption's place pop-up names; `None` where the sheet is ignored.
    pub assigned: Option<BridgeSheetSideChoice>,
    /// The sides and discs this sheet can be put on, in order: in the
    /// import pane, the sides of the draft's mediums, and a disc at each
    /// position up to the count of the folder's bound sheets that the draft
    /// lacks, so that each sheet can go on a disc of its own.
    pub side_choices: Vec<BridgeSheetSideChoice>,
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

/// One side or disc of a release's track table and the runs of rows on it,
/// headed once: the one shape the import pane's draft and a library
/// release's editor's draft hand the app. Mirror of bae-import's
/// `TrackTableSide`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackTableSide {
    pub side: BridgeTrackSide,
    pub runs: Vec<BridgeTrackTableRun>,
}

/// Consecutive rows of one side that the table draws together. Mirror of
/// bae-import's `TrackTableRun`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTrackTableRun {
    /// A track that plays a whole file, which its row names.
    Track { row: BridgeTrackTableRow },
    /// The tracks one CUE sheet carves on this side, under the sheet's
    /// caption, each row giving where it starts.
    Sheet { sheet: BridgeSheetGroup, rows: Vec<BridgeTrackTableRow> },
}

/// One track of the table. Mirror of bae-import's `TrackTableRow`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackTableRow {
    /// The track as the draft holds it. The row edits it in place.
    pub track: BridgeRawTrackEdit,
    pub source: BridgeTrackSource,
    pub audition: BridgeTrackAudition,
    /// How the track's place reads, rendered by core from the track's own
    /// side and number and the release's format: `8`, `A1`, or `3` beneath
    /// a `Disc 2` heading. The table shows it and never edits it.
    pub position: String,
    /// The length the track's audio measures.
    pub length: BridgeDurationClock,
    /// The length the track's source lists, present only when more than three
    /// seconds from the measured one: core's judgement, so every surface
    /// marks the same rows. The listed length marks a row and disables
    /// nothing.
    pub listed_length: Option<BridgeDurationClock>,
    pub artist: BridgeTrackArtistDisplay,
}

/// What the track's artist field shows. Mirror of bae-import's `TrackArtistDisplay`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTrackArtistDisplay {
    Own { field: BridgeArtistField },
    AlbumArtist,
    Empty,
}

/// What a row's source cell names. Mirror of bae-import's `TrackSource`.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTrackSource {
    /// The file the track plays whole.
    File { name: String },
    /// Where the track starts, as a clock label's fields ("10:22"), in the
    /// file its sheet's caption names.
    SheetSlice { start: BridgeDurationClock },
}

/// What playing a row plays. Mirror of bae-import's `TrackAudition`.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTrackAudition {
    /// The preview player plays this window of one of the folder's files.
    Preview { target: BridgePreviewTarget },
    /// The library plays the release from the row's track, the library
    /// track the row's `track.id` names.
    LibraryTrack,
    /// The row is a track a save of the editor adds, which the library does
    /// not hold yet, so nothing plays it.
    Unsaved,
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
    /// Every image the folder holds, in the scan's authoritative order;
    /// `None` when it holds none.
    pub images: Option<Vec<BridgeMappingImage>>,
    /// The rows that can become release tracks, on the track table's sides;
    /// `None` when the folder offers no track.
    pub track_sides: Option<Vec<BridgeTrackTableSide>>,
    /// The files no track plays from; `None` when there are none.
    pub files: Option<Vec<BridgeMappingFileRow>>,
    /// How many track rows the table holds.
    pub track_count: u32,
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
    Area {
        area: Option<BridgeReleaseArea>,
    },
    /// How many of one carrier the pressing holds: zero removes it, and a
    /// carrier the pressing lacks is added.
    MediumCount {
        media_count: BridgeMediaCount,
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

#[cfg(desktop)]
impl BridgePressingFactEdit {
    pub(crate) fn into_core(self) -> bae_import::PressingFactEdit {
        use bae_import::PressingFactEdit;
        match self {
            Self::Area { area } => PressingFactEdit::Area(area.map(BridgeReleaseArea::into_core)),
            Self::MediumCount { media_count } => {
                PressingFactEdit::MediumCount(media_count.into_core())
            }
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
    from_core: pub(crate) fn,
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
    /// The release's tracks on the track table's sides, each row the form
    /// row that edits the track; `None` when the release has no tracks.
    pub sides: Option<Vec<BridgeTrackTableSide>>,
    /// The files the release carries that no track plays from, in the
    /// release's file order, headed by each CUE sheet the form ignores or
    /// leaves with no audio: what its Files section lists under the tracks.
    /// `None` when it carries none.
    pub files: Option<Vec<BridgeMappingFileRow>>,
    pub album: BridgeDraftAlbum,
    /// The catalogs that describe the release the form holds, the applied
    /// one first; `None` when none does.
    pub records: Option<Vec<BridgeReleaseRecord>>,
    /// The media and Discogs details the form's pressing Add menus offer.
    pub pressing_offers: BridgePressingOffers,
    /// The form's album artists, each with how it stands to the library;
    /// `None` when the form names no album artist.
    pub album_artists: Option<BridgeArtistField>,
    pub changed: bool,
    /// The names of the audio files a save reads before it writes, the ones
    /// the form binds CUE sheets' `FILE` lines to, in name order; `None`
    /// when a save reads none.
    pub files_to_read: Option<Vec<String>>,
    /// Whether the save deletes a track that has a rating, a play in the
    /// history or a playlist entry, which the editor warns about before it
    /// saves.
    pub deletes_track_data: bool,
    pub invalid: Option<BridgeValidationReason>,
}

/// Raw pressing fields as the editor draws them. Mirrors
/// `bae_import::RawPressingEdit`: each text field is the text the user
/// typed, empty meaning "not set"; `year` is text (parsed at shape time).
/// `labels` and `barcodes` are the rows the editor draws: one per label or
/// barcode, none when there is none. `facts` is what the person
/// chose with the form's pickers.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawPressingEdit {
    pub year: String,
    pub labels: Vec<BridgeLabelRow>,
    pub barcodes: Vec<BridgeBarcodeRow>,
    pub facts: BridgePressingFacts,
}

/// What a row of a list the editor draws carries: the list's heading on the
/// first row and the control that adds a row on the last. Mirrors the flags
/// of `bae_import::ListRow`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeListPlace {
    pub labelled: bool,
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
