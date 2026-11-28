//! The draft a release editor shows beside the form it holds, which one read
//! of the library makes with the form bound to its statements as JSON: the
//! form's album artists, each with how it stands to the library, the album a
//! save files the release under, the release's cover and files, and its
//! tracks on the mediums that the catalog the person applied gives them or
//! else on the release's own, each beside the form row that edits it with its
//! picked artists as the library holds them.

use super::super::layout_bindings::{json, mediums_json, moves_json, places_json};
use super::form_assignments::assignment_rows;
use super::*;
use crate::tables::draft_rows::{assignment_kind_column, assignments_of, decoded, DraftAlbumRow};
use crate::{AlbumPick, DraftAlbum, RawTrackEdit, TrackArtistAssignments};
use bae_records::records::layout::LibraryLayout;

/// One form track as `:form_tracks` binds it.
#[derive(serde::Serialize)]
struct FormTrackJson<'a> {
    id: &'a str,
    title: &'a str,
    kind: &'static str,
}

/// The form as the draft's reads bind it: the release, whether the person
/// applied values, and each list as a JSON array.
pub(super) struct FormParams {
    pub(super) release: String,
    applied: bool,
    /// Whether the form links the release to records of its own, an
    /// apply's or none, in place of the release's.
    pub(super) relinked: bool,
    /// Whether the form links the release to records of its own, an
    /// apply's or none, in place of the release's, and those name any, so
    /// they name the album a save files the release under.
    linked: bool,
    album_artists: String,
    track_artists: String,
    /// The form's tracks, as [`FormDraftTracks`] binds them.
    form_tracks: String,
    /// The applied catalog's mediums and places, and the discs picked for
    /// CUE sheets, as the layout binds them.
    mediums: String,
    places: String,
    moves: String,
    /// What the form does to the release's CUE sheets and tracks, as the
    /// sheet files' and the layout's reads bind it.
    pub(super) edits: String,
    pub(super) records: String,
    pick_kind: &'static str,
    pick_album: Option<String>,
}

impl FormParams {
    pub(super) fn of(form: &ReleaseEditForm) -> Result<Self, DbError> {
        let edit = &form.edit;
        let album_artists: Vec<_> = assignment_rows(None, &edit.album_artist_assignments).collect();
        let track_artists: Vec<_> = edit
            .tracks
            .iter()
            .flat_map(|track| {
                let own: &[crate::Credited] = match &track.artist_assignments {
                    TrackArtistAssignments::AlbumArtists => &[],
                    TrackArtistAssignments::Explicit(assignments) => assignments,
                };
                assignment_rows(Some(track.id.as_str()), own)
            })
            .collect();
        let form_tracks: Vec<_> = edit
            .tracks
            .iter()
            .map(|track| FormTrackJson {
                id: &track.id,
                title: &track.title,
                kind: assignment_kind_column(&track.artist_assignments),
            })
            .collect();
        let applied = form.applied.as_ref();
        let (pick_kind, pick_album) = match &form.album_pick {
            AlbumPick::Records => ("records", None),
            AlbumPick::New => ("new", None),
            AlbumPick::Album { album_id } => ("album", Some(album_id.clone())),
        };
        Ok(Self {
            release: form.release_id.clone(),
            applied: form.applied.is_some(),
            relinked: form.records.is_some(),
            linked: form.records.as_ref().is_some_and(|records| !records.is_empty()),
            album_artists: json(&album_artists, "a release editor's album artists")?,
            track_artists: json(&track_artists, "a release editor's track artists")?,
            form_tracks: json(&form_tracks, "a release editor's tracks")?,
            mediums: mediums_json(applied, "a release editor's applied mediums")?,
            places: places_json(applied, "a release editor's applied track places")?,
            moves: moves_json(
                form.sheet_places
                    .iter()
                    .filter_map(|(sheet, place)| place.side().map(|side| (sheet.as_str(), side))),
                "a release editor's picked sheet sides",
            )?,
            edits: form.sheet_edits().json()?,
            records: bae_records::records::release_records::records_json(
                form.records.as_deref().unwrap_or_default(),
            ),
            pick_kind,
            pick_album,
        })
    }

    /// What the layout reads of the form.
    pub(super) fn layout(&self) -> LibraryLayout {
        LibraryLayout {
            release_id: self.release.clone(),
            applied: self.applied,
            mediums: self.mediums.clone(),
            places: self.places.clone(),
            moves: self.moves.clone(),
            edits: self.edits.clone(),
        }
    }
}

// The album a save of the form files its release under: the library album
// the person picked while the library holds it, or the new album they picked;
// else, once an applied catalog linked the release to its records, the album
// they name (a new one where they name none the library holds); else, with
// the release's own records or none, the album the release is in. `kind` says
// which of those names it, and `album` is the library album it names.
sqlive_compose::fragment!(
    FiledAlbum(
        release: String, pick_kind: String, pick_album: Option<String>, linked: bool,
        records: String
    ) { kind: String },
    r#"
    WITH picked AS (
        SELECT titled.*
        FROM @bae_records::records::fragments::AlbumTitle AS titled
        WHERE :pick_kind = 'album' AND titled.album_id = :pick_album),
    pick (kind, album_id) AS (
        SELECT 'new', NULL WHERE :pick_kind = 'new'
        UNION ALL
        SELECT 'album', picked.album_id FROM picked),
    filed AS (
        SELECT target.*
        FROM @bae_records::records::fragments::FiledByRecords(
            release: :release,
            records: (SELECT record.key, record.value ->> 0, record.value ->> 1,
                             record.value ->> 2, record.value ->> 3
                      FROM json_each(:records) AS record)) AS target),
    kept AS (
        SELECT held.*
        FROM @bae_records::records::fragments::ReleaseAlbum AS held
        WHERE held.release_id = :release),
    choice (kind, album_id) AS (
        SELECT pick.kind, pick.album_id FROM pick
        UNION ALL
        SELECT CASE WHEN filed.target_album_id IS NULL THEN 'none' ELSE 'named' END,
               filed.target_album_id
        FROM filed
        WHERE NOT EXISTS (SELECT 1 FROM pick) AND :linked
        UNION ALL
        SELECT 'named', kept.album_id FROM kept
        WHERE NOT EXISTS (SELECT 1 FROM pick) AND NOT :linked)
    SELECT choice.kind AS kind, album.*
    FROM choice
    LEFT JOIN @bae_records::records::fragments::LibraryAlbum AS album
           ON album.album_id = choice.album_id
"#
);

// The tracks of a form's release as the form lays them out
// ([`DraftEditorTrackRows`](bae_records::records::editor_tracks::DraftEditorTrackRows)),
// each with the row of the form that edits it as the JSON object `form` (NULL
// when the form holds no row for it). `form` has the row's `own` artists where
// it has any: its assignments, with each picked artist as the library holds it,
// and how they stand to the library.
sqlive_compose::fragment!(
    FormDraftTrackRows(
        release: String, applied: bool, mediums: String, places: String, moves: String,
        edits: String, album_artists: String, track_artists: String, form_tracks: String
    ) { form: Option<String> },
    r#"
    WITH fields AS (
        SELECT own.*
        FROM @crate::tables::release_edit::artist_fields::FormArtistFields(
            album_artists: :album_artists, track_artists: :track_artists) AS own
        WHERE own.part = 1),
    edited AS (
        SELECT row.value ->> 'id' AS id, row.value ->> 'title' AS title,
               row.value ->> 'kind' AS kind
        FROM json_each(:form_tracks) AS row)
    SELECT shown.*,
           CASE WHEN edited.id IS NOT NULL THEN json_object(
               'id', edited.id, 'title', edited.title, 'kind', edited.kind,
               'own', json(CASE WHEN fields.track_id IS NOT NULL THEN json_object(
                   'assignments', json(fields.assignments),
                   'standing', json(fields.field)) END)) END AS form
    FROM @bae_records::records::editor_tracks::DraftEditorTrackRows(
        release: :release, applied: :applied, mediums: :mediums, places: :places,
        moves: :moves, edits: :edits) AS shown
    LEFT JOIN edited ON edited.id = shown.row_id
    LEFT JOIN fields ON fields.track_id = shown.row_id
"#
);

// The album a save of the form files its release under ([`FiledAlbum`]),
// beside each of the form's tracks in their order ([`FormDraftTrackRows`]);
// one row with no track when the form lays out none, and none once the
// library no longer holds the release. The album is read with the tracks, not
// with the rest of the draft, because one statement of them all opens more
// tables than SQLite allows.
sqlive_compose::read!(
    writer FormDraftTracks(
        release: String, applied: bool, mediums: String, places: String, moves: String,
        edits: String, album_artists: String, track_artists: String, form_tracks: String,
        pick_kind: String, pick_album: Option<String>, linked: bool, records: String
    ),
    r#"
    SELECT filed.*, track.*
    FROM @crate::tables::release_edit::draft::FiledAlbum(
        release: :release, pick_kind: :pick_kind, pick_album: :pick_album, linked: :linked,
        records: :records) AS filed
    LEFT JOIN @crate::tables::release_edit::draft::FormDraftTrackRows(
        release: :release, applied: :applied, mediums: :mediums, places: :places,
        moves: :moves, edits: :edits, album_artists: :album_artists,
        track_artists: :track_artists, form_tracks: :form_tracks) AS track
      ON 1
    ORDER BY track.placed_medium, track.placed_position
"#
);

// Whether a track the save of the edits `edits` deletes from the release
// `release` has a rating, a play in the history or a playlist entry.
sqlive_compose::fragment!(
    DroppedKeptData(release: String, edits: String),
    r#"
    WITH gone AS (
        SELECT dropped.*
        FROM @bae_records::records::sheet_fragments::DroppedSheetTracks(
            release: :release, edits: :edits) AS dropped)
    SELECT data.*
    FROM @bae_people::people::fragments::TracksHaveKeptData(
        asked: (SELECT track_id FROM gone)) AS data
"#
);

// The draft of a form beside its tracks: the release as its editor shows it,
// with no row once the library holds no such release; the album's artist
// field, none when the album has none; whether a track the save deletes has
// kept data; the files a save reads first; and the release's files.
sqlive_compose::read!(
    writer EditorDraftHead(
        release: String, relinked: bool, form_records: String, album_artists: String,
        track_artists: String, applied: bool, mediums: String, edits: String
    ),
    r#"
    SELECT head.*, artist.*, kept.*, sheets.*, listed.*
    FROM @bae_records::records::release::EditedHead(
        release: :release, relinked: :relinked, records: :form_records) AS head
    LEFT JOIN @crate::tables::release_edit::artist_fields::FormArtistFields(
        album_artists: :album_artists, track_artists: :track_artists) AS artist
      ON artist.part = 0
    JOIN @crate::tables::release_edit::draft::DroppedKeptData(release: :release, edits: :edits)
        AS kept
      ON 1
    JOIN @bae_records::records::sheet_files::SheetFilesToRead(release: :release, edits: :edits)
        AS sheets
      ON 1
    JOIN @bae_records::records::editor_files::EditorFiles(
        release: :release, applied: :applied, mediums: :mediums, edits: :edits) AS listed
      ON 1
"#
);

/// The rows bae reads a form's draft from: the form, its album artists with
/// how each stands to the library, the album a save files it under, the
/// release's cover, its tracks with their audio, whether the release has
/// tracks, the release's files, and its records: the applied catalog's while
/// the form holds one, else the release's own.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReleaseEditDraftRows {
    form: ReleaseEditForm,
    album_artists: Option<crate::ArtistField>,
    album: DraftAlbum,
    cover_version: Option<String>,
    tracks: Vec<DraftRow>,
    /// The names of the files a save reads first; `None` when it reads none.
    files_to_read: Option<Vec<String>>,
    has_tracks: bool,
    files: bae_records::DbReleaseFiles,
    /// Whether a track the save deletes has a rating, a play in the history
    /// or a playlist entry.
    deletes_track_data: bool,
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    shows_records: bool,
}

/// One row of a form's draft tracks: a track the library holds, a track the
/// form's save adds, or a side no track sits on.
#[derive(Debug, Clone, PartialEq)]
enum DraftRow {
    Stored(DraftTrackRow),
    Added(bae_records::DbReleaseEditAdded),
    EmptySide(bae_types::mediums::TrackSide),
}

/// One track the library holds, as a form's draft read returns it: what the
/// library holds about the track, and the form row that edits it as JSON
/// (`None` when the form holds no row for the track).
#[derive(Debug, Clone, PartialEq)]
struct DraftTrackRow {
    track: bae_records::DbReleaseEditTrack,
    form: Option<String>,
}

/// The form row of a track as [`FormDraftTracks`] writes it.
#[derive(serde::Deserialize)]
struct FormTrackRow {
    id: String,
    title: String,
    kind: String,
    own: Option<OwnArtists>,
}

/// A form track's own artists as [`FormDraftTracks`] writes them: its
/// assignments as the library holds them, and how they stand to the library.
#[derive(serde::Deserialize)]
struct OwnArtists {
    assignments: serde_json::Value,
    standing: serde_json::Value,
}

impl FormTrackRow {
    /// The form row this track is, and how its own artists stand to the
    /// library (`None` when it has none of its own).
    fn decoded(self) -> Result<(RawTrackEdit, Option<crate::ArtistField>), DbError> {
        let artist_assignments = match self.kind.as_str() {
            "album_artists" => TrackArtistAssignments::AlbumArtists,
            "explicit" => TrackArtistAssignments::Explicit(match &self.own {
                Some(own) => assignments_of(&own.assignments.to_string(), "track artists")?,
                None => Vec::new(),
            }),
            other => return Err(DbError::unreadable("a form track's artist kind", other)),
        };
        let standing =
            self.own.map(|own| artist_field_of_json(&own.standing.to_string())).transpose()?;
        let edit = RawTrackEdit { id: self.id, title: self.title, artist_assignments, file: None };
        Ok((edit, standing))
    }
}

/// The reads of `form`'s draft: its head and its tracks.
pub(crate) fn release_edit_draft_read(
    form: &ReleaseEditForm,
) -> Result<coven::Both<EditorDraftHeadParams, FormDraftTracksParams>, DbError> {
    let form = FormParams::of(form)?;
    let head = EditorDraftHeadParams {
        release: form.release.clone(),
        relinked: form.relinked,
        form_records: if form.relinked { form.records.clone() } else { "[]".to_string() },
        album_artists: form.album_artists.clone(),
        track_artists: form.track_artists.clone(),
        applied: form.applied,
        mediums: form.mediums.clone(),
        edits: form.edits.clone(),
    };
    let tracks = FormDraftTracksParams {
        release: form.release,
        applied: form.applied,
        mediums: form.mediums,
        places: form.places,
        moves: form.moves,
        edits: form.edits,
        album_artists: form.album_artists,
        track_artists: form.track_artists,
        form_tracks: form.form_tracks,
        pick_kind: form.pick_kind.to_string(),
        pick_album: form.pick_album,
        linked: form.linked,
        records: form.records,
    };
    Ok(coven::Both(head, tracks))
}

/// The rows of `form`'s draft as [`release_edit_draft_read`] returned them;
/// `None` once the library no longer holds the form's release.
pub(crate) fn release_edit_draft_rows_of(
    form: &ReleaseEditForm,
    (head, tracks): (Vec<EditorDraftHeadRow>, Vec<FormDraftTracksRow>),
) -> Result<Option<ReleaseEditDraftRows>, DbError> {
    let Some(row) = bae_store::at_most_one::<EditorDraftHeadParams, _>(head)? else {
        return Ok(None);
    };
    let EditorDraftHeadRow { head, artist, kept, sheets, listed } = row;
    let release_id = &form.release_id;
    // Every row of the tracks' read holds the album a save files the release
    // under.
    let filed = tracks.first().map(|row| row.filed.clone()).ok_or_else(|| {
        DbError::unreadable(
            format!("the album a save of release {release_id} files it under"),
            None::<String>,
        )
    })?;
    let edited = bae_records::records::edited_release_of(head)?;
    Ok(Some(ReleaseEditDraftRows {
        form: form.clone(),
        album_artists: artist.map(|artist| artist_field_of_json(&artist.field)).transpose()?,
        album: DraftAlbumRow::of_choice(filed.kind, filed.album)?
            .album(&format!("the editor of release {release_id}"))?,
        cover_version: edited.cover_version,
        tracks: tracks
            .into_iter()
            .filter_map(|row| row.track)
            .map(draft_row_of)
            .collect::<Result<_, _>>()?,
        files_to_read: bae_records::records::sheet_files::sheet_files_to_read_of(sheets)?,
        has_tracks: edited.has_tracks,
        files: bae_records::records::release_files_of(listed)?,
        deletes_track_data: kept.data.any(),
        records: form.records.clone().unwrap_or(edited.records),
        shows_records: edited.shows_records,
    }))
}

/// A row of the form's tracks as [`FormDraftTrackRows`] gives it: a track of
/// the form's release in release order, or a track its save adds, on the
/// mediums and with the listed lengths the catalog the person applied gives
/// them or else the release's own, each track the library holds with the form
/// row that edits it.
fn draft_row_of(row: FormDraftTrackRows) -> Result<DraftRow, DbError> {
    Ok(match bae_records::records::draft_release_edit_row_of(row.shown)? {
        bae_records::DbReleaseEditRow::Stored(track) => {
            DraftRow::Stored(DraftTrackRow { track: *track, form: row.form })
        }
        bae_records::DbReleaseEditRow::Added(added) => DraftRow::Added(added),
        bae_records::DbReleaseEditRow::EmptySide(side) => DraftRow::EmptySide(side),
    })
}

impl DraftTrackRow {
    /// The row of the editor's track table this track is. `unreadable` says
    /// what makes the track one the editor can't show.
    fn table_row(
        self,
        unreadable: &impl Fn(crate::ReleaseEditLoadError) -> DbError,
    ) -> Result<crate::types::ReleaseEditTrackRow, DbError> {
        let track_id = self.track.track.id.clone();
        let form = self
            .form
            .ok_or_else(|| unreadable(crate::ReleaseEditLoadError::MissingFormRow { track_id }))?;
        let (edit, artists) = decoded::<FormTrackRow>(&form, "a track's form row")?.decoded()?;
        Ok(crate::types::ReleaseEditTrackRow {
            edit,
            context: crate::ReleaseEditTrackContext::of_track(self.track).map_err(unreadable)?,
            audition: crate::TrackAudition::LibraryTrack,
            artists,
        })
    }
}

impl DraftRow {
    /// The entry of the editor's track table this row is.
    fn table_row(
        self,
        unreadable: &impl Fn(crate::ReleaseEditLoadError) -> DbError,
    ) -> Result<crate::types::ReleaseEditTableRow, DbError> {
        let added = match self {
            Self::Stored(track) => {
                return track.table_row(unreadable).map(crate::types::ReleaseEditTableRow::Track)
            }
            Self::Added(added) => added,
            Self::EmptySide(side) => return Ok(crate::types::ReleaseEditTableRow::EmptySide(side)),
        };
        let title = crate::service::added_track_title(&added.file);
        Ok(crate::types::ReleaseEditTableRow::Track(crate::types::ReleaseEditTrackRow {
            edit: RawTrackEdit {
                id: added.file.file_id.clone(),
                title,
                artist_assignments: TrackArtistAssignments::AlbumArtists,
                file: None,
            },
            context: crate::ReleaseEditTrackContext {
                side: added.position.side(),
                source: crate::MappingContainer {
                    file_id: added.file.file_id,
                    name: added.file.name,
                },
                start_ms: 0,
                duration_ms: added.duration_ms,
                listed_length_ms: None,
                position_text: added.position.text(),
                sheet: None,
            },
            audition: crate::TrackAudition::Unsaved,
            artists: None,
        }))
    }
}

impl ReleaseEditDraftRows {
    /// The draft the editor shows beside its form: the tracks on the table's
    /// sides, whether a save would write anything the editor did not open
    /// on, and why the editor can't save the form, when it can't.
    pub(crate) fn draft(self) -> Result<ReleaseEditDraft, DbError> {
        let Self {
            form,
            album_artists,
            album,
            cover_version,
            tracks,
            files_to_read,
            has_tracks,
            files,
            deletes_track_data,
            records,
            shows_records,
        } = self;
        let changed = form.changed();
        let invalid = form.edit.shape().err();
        let release_id = form.release_id.clone();
        let unreadable = |error: crate::ReleaseEditLoadError| {
            DbError::unreadable(format!("the tracks of release {release_id}"), error)
        };
        let rows = tracks
            .into_iter()
            .map(|track| track.table_row(&unreadable))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ReleaseEditDraft {
            cover: bae_records::cover_ref(&release_id, cover_version),
            source_audio: files.source_audio.clone(),
            images: (files.image_count > 0).then(|| {
                files
                    .images
                    .iter()
                    .map(|image| crate::ReleaseImage {
                        file_id: image.id.clone(),
                        name: crate::mapping::file_name(&image.original_filename).to_string(),
                        source: bae_records::GallerySource::ReleaseFile {
                            file_id: image.id.clone(),
                        },
                        key: crate::CoverKey::File(image.id.clone()),
                    })
                    .collect()
            }),
            sides: has_tracks
                .then(|| crate::types::lay_out_tracks(rows, album_artists.is_some()))
                .transpose()?,
            files: files
                .has_others
                .then(|| {
                    files
                        .carried
                        .into_iter()
                        .map(|carried| match carried {
                            bae_records::DbCarriedFile::File(file) => {
                                Ok(crate::MappingFileRow::File(crate::types::carried_file(&file)))
                            }
                            bae_records::DbCarriedFile::Sheet(sheet) => {
                                let sheet_id = sheet.file_id.clone();
                                crate::types::sheet_without_tracks_group(sheet)
                                    .map(crate::MappingFileRow::Sheet)
                                    .map_err(|reason| {
                                        DbError::unreadable(
                                            format!("CUE sheet {sheet_id} in release {release_id}'s files"),
                                            reason,
                                        )
                                    })
                            }
                        })
                        .collect::<Result<Vec<_>, DbError>>()
                })
                .transpose()?,
            album,
            records: shows_records.then_some(records),
            pressing_offers: form.edit.pressing.offers(),
            album_artists,
            changed,
            files_to_read,
            deletes_track_data,
            invalid,
        })
    }
}
