//! The draft a release editor shows beside the form it holds, which one read
//! of the library makes with the form bound to its statements as JSON: the
//! form's album artists, each with how it stands to the library, the album a
//! save files the release under, the release's cover and files, and its
//! tracks on the mediums that the catalog the person applied gives them or
//! else on the release's own, each beside the form row that edits it with its
//! picked artists as the library holds them.

use super::super::layout_bindings::{json, mediums_json, moves_json, places_json};
use super::form_assignments::{assignment_rows, assignment_rows_sql};
use super::*;
use crate::tables::draft_rows::{
    assignment_kind_column, assignments_json_sql, assignments_of, decoded, DraftAlbumRow,
};
use crate::{AlbumPick, DraftAlbum, RawTrackEdit, TrackArtistAssignments};
use bae_records::records::layout::LayoutParams;

/// The form's tracks as a subquery over the parameter `:form_tracks`: the
/// library track each row edits, its title, and whether it takes the album's
/// artists.
const FORM_TRACKS: &str = "(SELECT j.value ->> 'id' AS id, j.value ->> 'title' AS title, \
                                   j.value ->> 'kind' AS kind \
                            FROM json_each(:form_tracks) AS j)";

/// One form track as a JSON object over the row `f` of [`FORM_TRACKS`],
/// with its own artists as the library holds the ones the person picked.
fn form_track_json_sql() -> String {
    format!(
        "json_object('id', f.id, 'title', f.title, 'kind', f.kind, 'assignments', json({}))",
        assignments_json_sql(&assignment_rows_sql(":track_artists"), "assignment.track_id = f.id")
    )
}

/// The form as the draft's statements bind it: the release, whether the
/// person applied values and whether they replaced its records, and each
/// list as a JSON array.
pub(super) struct FormParams {
    release: String,
    applied: bool,
    /// Whether the form links the release to records of its own, an
    /// apply's or none, in place of the release's.
    relinked: bool,
    /// Whether those records name any, so they name the album a save files
    /// the release under.
    linked: bool,
    album_artists: String,
    track_artists: String,
    form_tracks: String,
    /// The applied catalog's mediums and places, and the discs picked for
    /// CUE sheets, as the layout's statements bind them.
    mediums: String,
    places: String,
    moves: String,
    /// The audio files picked for CUE sheets' `FILE` lines, as the sheet
    /// files' statements bind them.
    sheet_files: String,
    records: String,
    pick_kind: &'static str,
    pick_album: Option<String>,
}

/// One form track as `:form_tracks` binds it.
#[derive(serde::Serialize)]
struct FormTrackJson<'a> {
    id: &'a str,
    title: &'a str,
    kind: &'static str,
}

impl FormParams {
    pub(super) fn of(form: &ReleaseEditForm) -> Result<Self, DbError> {
        let edit = &form.edit;
        let album_artists: Vec<_> = assignment_rows(None, &edit.album_artist_assignments).collect();
        let track_artists: Vec<_> = edit
            .tracks
            .iter()
            .enumerate()
            .flat_map(|(place, track)| {
                let own: &[crate::Credited] = match &track.artist_assignments {
                    TrackArtistAssignments::AlbumArtists => &[],
                    TrackArtistAssignments::Explicit(assignments) => assignments,
                };
                assignment_rows(Some((track.id.as_str(), place)), own)
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
                form.sheet_places.iter().map(|(sheet, side)| (sheet.as_str(), *side)),
                "a release editor's picked sheet sides",
            )?,
            sheet_files: bae_records::records::sheet_files::sheet_file_picks_json(
                &form.sheet_files,
            )?,
            records: bae_records::records::release_records::records_json(
                form.records.as_deref().unwrap_or_default(),
            ),
            pick_kind,
            pick_album,
        })
    }

    /// What the layout's statements read of the form.
    pub(super) fn layout(&self) -> LayoutParams<'_> {
        LayoutParams {
            release_id: Some(&self.release),
            applied: self.applied,
            mediums: &self.mediums,
            places: &self.places,
            carving: "[]",
            moves: &self.moves,
        }
    }

    /// The form's artist fields, as the standing clauses read them.
    fn credits(&self) -> CreditSource<'_> {
        CreditSource::Form {
            album_artists: &self.album_artists,
            track_artists: &self.track_artists,
        }
    }
}

/// The album a save of the form files its release under, which one
/// statement reads: the library album the person picked while the library
/// holds it, or the new album they picked; else, once an applied catalog
/// linked the release to its records, the album they name (a new one where
/// they name none the library holds); else, with the release's own records
/// or none, the album the release is in.
fn release_edit_album_on(
    sql: &impl coven::SqlRead,
    form: &FormParams,
) -> Result<DraftAlbum, DbError> {
    use bae_records::records::release_records::{records_target_album_sql, RECORDS_OF_JSON};
    let target = records_target_album_sql(RECORDS_OF_JSON, ":release");
    let row = sql.query_row(
        &format!(
            "WITH pick (kind, album_id) AS ( \
                 SELECT 'new', NULL WHERE :pick_kind = 'new' \
                 UNION ALL \
                 SELECT 'album', picked.id FROM albums AS picked \
                 WHERE :pick_kind = 'album' AND picked.id = :pick_album), \
             choice (kind, album_id) AS ( \
                 SELECT kind, album_id FROM pick \
                 UNION ALL \
                 SELECT CASE WHEN target.target_album_id IS NULL THEN 'none' ELSE 'named' END, \
                        target.target_album_id \
                 FROM ({target}) AS target \
                 WHERE NOT EXISTS (SELECT 1 FROM pick) AND :linked \
                 UNION ALL \
                 SELECT 'named', r.album_id FROM releases AS r \
                 WHERE r.id = :release AND NOT EXISTS (SELECT 1 FROM pick) AND NOT :linked) \
             SELECT choice.kind, a.id, a.title, {credits} \
             FROM choice LEFT JOIN albums AS a ON a.id = choice.album_id",
            credits = bae_records::records::query::album_credits_json_sql(),
        ),
        named_params! {
            ":pick_kind": form.pick_kind,
            ":pick_album": form.pick_album,
            ":linked": form.linked,
            ":release": form.release,
            ":records": form.records,
        },
        DraftAlbumRow::of_row,
    )?;
    row.album(&format!("the editor of release {}", form.release))
}

/// The rows bae reads a form's draft from: the form, its album artists with
/// how each stands to the library, the album a save files it under, the
/// release's cover, its tracks with their audio, each with its form row, whether the
/// release has tracks, the release's files, and its records: the applied
/// catalog's while the form holds one, else the release's own.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReleaseEditDraftRows {
    form: ReleaseEditForm,
    album_artists: Option<crate::ArtistField>,
    album: DraftAlbum,
    cover_version: Option<String>,
    tracks: Vec<DraftTrackRow>,
    /// The sides and discs a CUE sheet can go on once the form's layout is
    /// written.
    side_choices: Vec<bae_records::DbSheetSideChoice>,
    /// The names of the files a save reads first; `None` when it reads none.
    files_to_read: Option<Vec<String>>,
    has_tracks: bool,
    files: bae_records::DbReleaseFiles,
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    has_records: bool,
}

/// One track of a form's draft as its statement returns it: what the
/// library holds about the track, the form row that edits it as JSON (`None`
/// when the form holds no row for the track), and the form row's own artists
/// with how each stands to the library as JSON (`None` when it has none of
/// its own).
#[derive(Debug, Clone, PartialEq)]
struct DraftTrackRow {
    track: bae_records::DbReleaseEditTrack,
    form: Option<String>,
    artists: Option<String>,
}

/// One form track as [`form_track_json_sql`] writes it.
#[derive(serde::Deserialize)]
struct FormTrackRow {
    id: String,
    title: String,
    kind: String,
    assignments: serde_json::Value,
}

impl FormTrackRow {
    /// The form row this track is.
    fn form_row(self) -> Result<RawTrackEdit, DbError> {
        Ok(RawTrackEdit {
            artist_assignments: match self.kind.as_str() {
                "album_artists" => TrackArtistAssignments::AlbumArtists,
                "explicit" => TrackArtistAssignments::Explicit(assignments_of(
                    &self.assignments.to_string(),
                    "track artists",
                )?),
                other => return Err(DbError::unreadable("a form track's artist kind", other)),
            },
            id: self.id,
            title: self.title,
            file: None,
        })
    }
}

/// The rows of `form`'s draft, one statement per part; `None` once the
/// library no longer holds the form's release.
pub(crate) fn release_edit_draft_rows_on(
    sql: &impl coven::SqlRead,
    form: &ReleaseEditForm,
) -> Result<Option<ReleaseEditDraftRows>, DbError> {
    let params = FormParams::of(form)?;
    let release = named_params! { ":release": params.release };
    let held: bool = sql.query_row(
        "SELECT EXISTS (SELECT 1 FROM releases WHERE id = :release)",
        release,
        |row| row.get(0),
    )?;
    if !held {
        return Ok(None);
    }
    Ok(Some(ReleaseEditDraftRows {
        form: form.clone(),
        album_artists: album_artist_field_on(sql, params.credits())?,
        album: release_edit_album_on(sql, &params)?,
        cover_version: sql.query_row(
            &format!("SELECT {}", bae_records::records::files::cover_version_sql(":release")),
            release,
            |row| row.get(0),
        )?,
        tracks: draft_tracks_on(sql, &params)?,
        side_choices: bae_records::records::layout::laid_sheet_side_choices_on(
            sql,
            &params.layout(),
        )?,
        files_to_read: bae_records::records::sheet_files::sheet_files_to_read_on(
            sql,
            ":release",
            ":sheet_files",
            named_params! { ":release": params.release, ":sheet_files": params.sheet_files },
        )?,
        has_tracks: sql.query_row(
            "SELECT EXISTS (SELECT 1 FROM tracks WHERE release_id = :release)",
            release,
            |row| row.get(0),
        )?,
        files: bae_records::records::release_files_on(
            sql,
            ":release",
            ":sheet_files",
            named_params! { ":release": params.release, ":sheet_files": params.sheet_files },
        )?,
        records: match &form.records {
            Some(records) => records.clone(),
            None => bae_records::records::read::get_release_records_on(sql, &params.release)?,
        },
        has_records: sql.query_row(
            "SELECT CASE WHEN :relinked THEN json_array_length(:records) > 0 \
                    ELSE EXISTS (SELECT 1 FROM release_records WHERE release_id = :release) END",
            named_params! {
                ":relinked": params.relinked,
                ":records": params.records,
                ":release": params.release,
            },
            |row| row.get(0),
        )?,
    }))
}

/// The tracks of the form's release in release order, on the mediums and
/// with the listed lengths the catalog the person applied gives them or else
/// the release's own, each
/// with the form row that edits it and that row's own artists with how each
/// stands to the library.
fn draft_tracks_on(
    sql: &impl coven::SqlRead,
    form: &FormParams,
) -> Result<Vec<DraftTrackRow>, DbError> {
    let mediums = bae_records::records::layout::laid_mediums_source();
    let places = bae_records::records::layout::laid_places_source();
    let layout = form.layout();
    let credits = form.credits();
    let mut params = credits.params();
    params.push((":form_tracks", &form.form_tracks as &dyn rusqlite::ToSql));
    params.push((":sheet_files", &form.sheet_files as &dyn rusqlite::ToSql));
    params.extend(layout.named());
    Ok(sql.query(
        &format!(
            "WITH {standing_clauses} \
             SELECT library.*, \
                    (SELECT {track} FROM {FORM_TRACKS} AS f WHERE f.id = library.id) AS form, \
                    {artists} AS artists \
             FROM ({tracks}) AS library ORDER BY library.placed_position",
            standing_clauses = artist_field_clauses(credits),
            track = form_track_json_sql(),
            artists = track_artist_field_sql("library.id"),
            tracks = bae_records::records::release_edit_tracks_sql(
                ":release",
                bae_records::records::MediumsSource::Draft { mediums: &mediums, places: &places },
                ":sheet_files",
            ),
        ),
        params.as_slice(),
        |row| {
            Ok(DraftTrackRow {
                track: bae_records::records::release_edit_track_of_row(row)?,
                form: row.get("form")?,
                artists: row.get("artists")?,
            })
        },
    )?)
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
        let row: FormTrackRow = decoded(&form, "a track's form row")?;
        Ok(crate::types::ReleaseEditTrackRow {
            edit: row.form_row()?,
            context: crate::ReleaseEditTrackContext::of_track(self.track).map_err(unreadable)?,
            artists: self.artists.as_deref().map(artist_field_of_json).transpose()?,
        })
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
            side_choices,
            files_to_read,
            has_tracks,
            files,
            records,
            has_records,
        } = self;
        let changed = form.changed();
        let invalid = form.edit.shape().err();
        let release_id = form.release_id;
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
            sides: has_tracks.then(|| crate::types::lay_out_tracks(rows, &side_choices)),
            files: files
                .has_others
                .then(|| crate::ReleaseEditDisplayContext::carried_files(&files)),
            album,
            records: has_records.then_some(records),
            pressing_offers: form.edit.pressing.offers(),
            album_artists,
            changed,
            files_to_read,
            invalid,
        })
    }
}
