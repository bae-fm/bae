//! The draft a release editor shows beside the form it holds, which one read
//! of the library makes with the form bound to its statements as JSON: how
//! the form's album artists stand to the library, the album a save files the
//! release under, the release's cover and files, and its tracks on the
//! mediums that the catalog the person applied gives them or else on the
//! release's own, each beside the form row that edits it with its picked
//! artists as the library holds them.

use super::form_assignments::{assignment_rows, assignment_rows_sql};
use super::*;
use crate::tables::draft_rows::{
    assignment_kind_column, assignments_json_sql, assignments_of, decoded, DraftAlbumRow,
};
use crate::{AlbumPick, DraftAlbum, RawTrackEdit, TrackArtistAssignments};

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
/// person applied a catalog, and each list as a JSON array.
struct FormParams {
    release: String,
    applied: bool,
    album_artists: String,
    track_artists: String,
    form_tracks: String,
    mediums: String,
    places: String,
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

/// One applied medium as `:mediums` binds it.
#[derive(serde::Serialize)]
struct MediumJson {
    position: u16,
    carrier: Option<&'static str>,
    sides: u8,
}

/// Where the applied catalog places a form track, as `:places` binds it.
#[derive(serde::Serialize)]
struct PlaceJson<'a> {
    track_id: &'a str,
    medium: u16,
    side: u8,
    number: u32,
}

/// `value` as the JSON a statement binds.
fn json(value: &impl serde::Serialize, what: &str) -> Result<String, DbError> {
    serde_json::to_string(value).map_err(|error| DbError::encode(what, error))
}

impl FormParams {
    fn of(form: &ReleaseEditForm) -> Result<Self, DbError> {
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
        let mut mediums = Vec::new();
        let mut places = Vec::new();
        if let Some(applied) = &form.applied {
            for (index, medium) in applied.draft.mediums.iter().enumerate() {
                mediums.push(MediumJson {
                    position: bae_types::mediums::medium_position(index).get(),
                    carrier: medium.carrier.map(bae_types::pressing::Medium::key),
                    sides: medium.sides.get(),
                });
            }
            for track in &applied.draft.tracks {
                places.push(PlaceJson {
                    track_id: &track.edit.id,
                    medium: track.place.medium.get(),
                    side: track.place.side.get(),
                    number: track.place.number.get(),
                });
            }
        }
        let (pick_kind, pick_album) = match &form.album_pick {
            AlbumPick::Records => ("records", None),
            AlbumPick::New => ("new", None),
            AlbumPick::Album { album_id } => ("album", Some(album_id.clone())),
        };
        Ok(Self {
            release: form.release_id.clone(),
            applied: form.applied.is_some(),
            album_artists: json(&album_artists, "a release editor's album artists")?,
            track_artists: json(&track_artists, "a release editor's track artists")?,
            form_tracks: json(&form_tracks, "a release editor's tracks")?,
            mediums: json(&mediums, "a release editor's applied mediums")?,
            places: json(&places, "a release editor's applied track places")?,
            records: bae_records::records::release_records::records_json(
                form.applied.as_ref().map_or(&[], |applied| applied.records.as_slice()),
            ),
            pick_kind,
            pick_album,
        })
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
/// holds it, or the new album they picked; else, once they applied a
/// catalog, the album its records name (a new one where they name none the
/// library holds); else the album the release is in.
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
                 WHERE NOT EXISTS (SELECT 1 FROM pick) AND :applied \
                 UNION ALL \
                 SELECT 'named', r.album_id FROM releases AS r \
                 WHERE r.id = :release AND NOT EXISTS (SELECT 1 FROM pick) AND NOT :applied) \
             SELECT choice.kind, a.id, a.title, {credits} \
             FROM choice LEFT JOIN albums AS a ON a.id = choice.album_id",
            credits = bae_records::records::query::album_credits_json_sql(),
        ),
        named_params! {
            ":pick_kind": form.pick_kind,
            ":pick_album": form.pick_album,
            ":applied": form.applied,
            ":release": form.release,
            ":records": form.records,
        },
        DraftAlbumRow::of_row,
    )?;
    row.album(&format!("the editor of release {}", form.release))
}

/// The rows bae reads a form's draft from: the form, how its album artists
/// stand to the library, the album a save files it under, the release's
/// cover, its tracks with their audio, each with its form row, whether the
/// release has tracks, the release's files, and its records: the applied
/// catalog's while the form holds one, else the release's own.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReleaseEditDraftRows {
    form: ReleaseEditForm,
    album_artist_standing: Option<crate::ArtistFieldStanding>,
    album: DraftAlbum,
    cover_version: Option<String>,
    tracks: Vec<DraftTrackRow>,
    has_tracks: bool,
    files: bae_records::DbReleaseFiles,
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    has_records: bool,
}

/// One track of a form's draft as its statement returns it: what the
/// library holds about the track, the form row that edits it as JSON (`None`
/// when the form holds no row for the track), and how the form row's own
/// artists stand to the library as JSON (`None` when it has none of its
/// own).
#[derive(Debug, Clone, PartialEq)]
struct DraftTrackRow {
    track: bae_records::DbReleaseEditTrack,
    form: Option<String>,
    artist_standing: Option<String>,
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
        album_artist_standing: album_artist_standing_on(sql, params.credits())?,
        album: release_edit_album_on(sql, &params)?,
        cover_version: sql.query_row(
            &format!("SELECT {}", bae_records::records::files::cover_version_sql(":release")),
            release,
            |row| row.get(0),
        )?,
        tracks: draft_tracks_on(sql, &params)?,
        has_tracks: sql.query_row(
            "SELECT EXISTS (SELECT 1 FROM tracks WHERE release_id = :release)",
            release,
            |row| row.get(0),
        )?,
        files: bae_records::records::release_files_on(sql, ":release", release)?,
        records: match &form.applied {
            Some(applied) => applied.records.clone(),
            None => bae_records::records::read::get_release_records_on(sql, &params.release)?,
        },
        has_records: sql.query_row(
            "SELECT CASE WHEN :applied THEN json_array_length(:records) > 0 \
                    ELSE EXISTS (SELECT 1 FROM release_records WHERE release_id = :release) END",
            named_params! {
                ":applied": params.applied,
                ":records": params.records,
                ":release": params.release,
            },
            |row| row.get(0),
        )?,
    }))
}

/// The tracks of the form's release in release order, on the mediums the
/// catalog the person applied gives them or else on the release's own, each
/// with the form row that edits it and how that row's own artists stand to
/// the library.
fn draft_tracks_on(
    sql: &impl coven::SqlRead,
    form: &FormParams,
) -> Result<Vec<DraftTrackRow>, DbError> {
    let mediums = "(SELECT j.value ->> 'position' AS id, :release AS release_id, \
                           j.value ->> 'position' AS position, j.value ->> 'carrier' AS medium, \
                           j.value ->> 'sides' AS side_count \
                    FROM json_each(:mediums) AS j \
                    UNION ALL \
                    SELECT id, release_id, position, medium, side_count FROM release_mediums \
                    WHERE release_id = :release AND NOT :applied)";
    let places = "(SELECT j.value ->> 'track_id' AS id, j.value ->> 'medium' AS medium_id, \
                          j.value ->> 'side' AS side, j.value ->> 'number' AS track_number \
                   FROM json_each(:places) AS j \
                   UNION ALL \
                   SELECT id, medium_id, side, track_number FROM tracks \
                   WHERE release_id = :release AND NOT :applied)";
    let credits = form.credits();
    let mut params = credits.params();
    params.extend([
        (":form_tracks", &form.form_tracks as &dyn rusqlite::ToSql),
        (":release", &form.release),
        (":applied", &form.applied),
        (":mediums", &form.mediums),
        (":places", &form.places),
    ]);
    Ok(sql.query(
        &format!(
            "WITH {standing_clauses} \
             SELECT library.*, \
                    (SELECT {track} FROM {FORM_TRACKS} AS f WHERE f.id = library.id) AS form, \
                    {standing} AS artist_standing \
             FROM ({tracks}) AS library ORDER BY library.position",
            standing_clauses = artist_standing_clauses(credits),
            track = form_track_json_sql(),
            standing = track_artist_standing_sql("library.id"),
            tracks = bae_records::records::release_edit_tracks_sql(
                ":release",
                bae_records::records::MediumsSource::Draft { mediums, places },
            ),
        ),
        params.as_slice(),
        |row| {
            Ok(DraftTrackRow {
                track: bae_records::records::release_edit_track_of_row(row)?,
                form: row.get("form")?,
                artist_standing: row.get("artist_standing")?,
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
    ) -> Result<crate::ReleaseEditTrackRow, DbError> {
        let track_id = self.track.track.id.clone();
        let form = self
            .form
            .ok_or_else(|| unreadable(crate::ReleaseEditLoadError::MissingFormRow { track_id }))?;
        let row: FormTrackRow = decoded(&form, "a track's form row")?;
        Ok(crate::ReleaseEditTrackRow {
            edit: row.form_row()?,
            context: crate::ReleaseEditTrackContext::of_track(self.track).map_err(unreadable)?,
            artist_standing: self
                .artist_standing
                .as_deref()
                .map(artist_field_standing_of_json)
                .transpose()?,
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
            album_artist_standing,
            album,
            cover_version,
            tracks,
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
            sides: has_tracks.then(|| crate::ReleaseEditTrackSide::lay_out(rows)),
            files: files
                .has_others
                .then(|| crate::ReleaseEditDisplayContext::carried_files(&files)),
            album,
            records,
            has_records,
            album_artist_standing,
            changed,
            invalid,
        })
    }
}
