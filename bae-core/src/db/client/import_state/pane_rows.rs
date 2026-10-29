//! The draft's rows — the album fields, the tracks with their decisions, the
//! artist assignments, the cover — as one candidate's save writes them and
//! the pane's read draws them.
//!
//! Every row group hangs off the candidate by content hash, and the draft's
//! own provenance and tracks hang off `import_candidate_edit`, so replacing
//! the draft row takes them with it. The writers here replace a whole group;
//! which groups a candidate save replaces, and under what checks, is
//! `preparation_rows`'s to say.

mod values;
mod writes;

use super::super::downscaled_copies::{copies_json_sql, copies_of};
use super::verdict_rows::unreadable;
use super::*;
use crate::import::{
    ArtistAssignment, ArtistCredit, AudioFile, CandidateDraft, CandidateTrack, CoverSelection,
    Credited, ExistingArtist, MetadataAuthor, RawLabelEdit, RawPressingEdit, RawTrackEdit,
    TrackArtistAssignments,
};
use bae_catalogs::remote_image::{CoverStanding, RemoteCover, RemoteImageSet};
use std::str::FromStr;

const EDIT_COLUMNS: &str = "content_hash, album_title, album_year, year, \
     labels, barcodes, country, region, media, status, packaging, discogs_details";

const TRACK_COLUMNS: &str = "content_hash, track_id, position, title, \
     artist_assignment_kind, medium, side, track_number, stated_length_ms, \
     file_kind, file_id, sheet_id, slice_index";

pub(super) fn delete_cover(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_cover WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// The column a draft's author is stored as.
fn author_column(author: MetadataAuthor) -> &'static str {
    match author {
        MetadataAuthor::Nobody => "nobody",
        MetadataAuthor::Prefill => "prefill",
        MetadataAuthor::Identification => "identification",
        MetadataAuthor::Person => "person",
    }
}

pub(crate) fn author_of(stored: &str) -> Result<MetadataAuthor, DbError> {
    match stored {
        "nobody" => Ok(MetadataAuthor::Nobody),
        "prefill" => Ok(MetadataAuthor::Prefill),
        "identification" => Ok(MetadataAuthor::Identification),
        "person" => Ok(MetadataAuthor::Person),
        other => Err(unreadable("draft author", other)),
    }
}

/// Replace the draft row and everything hanging off it: its tracks and their
/// artist assignments, and its provenance. The caller writes the new
/// provenance after this returns.
pub(super) fn replace_draft(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    draft: &CandidateDraft,
    author: MetadataAuthor,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_edit WHERE content_hash = ?",
        [content_hash],
    )?;
    insert_draft(sql, content_hash, draft, author)
}

pub(crate) fn insert_draft(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    draft: &CandidateDraft,
    author: MetadataAuthor,
) -> Result<(), DbError> {
    let release_edit = draft.release_edit();
    let facts = super::super::pressing_columns::FactColumns::of(&draft.pressing.facts);
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_edit ({EDIT_COLUMNS}, author, draft_blank, draft_valid) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ),
        params![
            content_hash,
            draft.album_title,
            draft.album_year,
            draft.pressing.year,
            serde_json::to_string(&draft.pressing.labels).expect("label rows serialize"),
            serde_json::to_string(&draft.pressing.barcodes).expect("barcode rows serialize"),
            facts.country,
            facts.region,
            facts.media,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            author_column(author),
            release_edit.is_blank(),
            release_edit.shape().is_ok(),
        ],
    )?;
    insert_album_artist_assignments(sql, content_hash, &draft.album_artist_assignments)?;
    for (index, medium) in draft.mediums.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_medium (content_hash, position, medium, side_count) \
             VALUES (?, ?, ?, ?)",
            params![
                content_hash,
                crate::mediums::medium_position(index),
                medium.carrier.map(bae_types::pressing::Medium::key),
                medium.sides,
            ],
        )?;
    }
    for (position, track) in draft.tracks.iter().enumerate() {
        let (file_kind, file_id, sheet_id, slice_index) = mapping_file_columns(&track.edit.file);
        let stated_length_ms = track
            .stated_length_ms
            .map(|length| to_i64(length, "a track's stated length"))
            .transpose()?;
        sql.execute(
            &format!(
                "INSERT INTO import_candidate_track ({TRACK_COLUMNS}) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            ),
            params![
                content_hash,
                track.edit.id,
                position as i64,
                track.edit.title,
                assignment_kind_column(&track.edit.artist_assignments),
                track.place.medium,
                track.place.side,
                track.place.number,
                stated_length_ms,
                file_kind,
                file_id,
                sheet_id,
                slice_index,
            ],
        )?;
        if let TrackArtistAssignments::Explicit(assignments) = &track.edit.artist_assignments {
            insert_track_artist_assignments(sql, content_hash, &track.edit.id, assignments)?;
        }
    }
    values::insert_draft_values(sql, content_hash, draft)
}

fn assignment_kind_column(assignments: &TrackArtistAssignments) -> &'static str {
    match assignments {
        TrackArtistAssignments::AlbumArtists => "album_artists",
        TrackArtistAssignments::Explicit(_) => "explicit",
    }
}

fn mapping_file_columns(file: &AudioFile) -> (&str, &str, Option<&str>, Option<i64>) {
    match file {
        AudioFile::Standalone { file_id } => ("standalone", file_id.as_str(), None, None),
        AudioFile::SheetSlice {
            file_id,
            sheet_id,
            index,
        } => (
            "sheet_slice",
            file_id.as_str(),
            Some(sheet_id.as_str()),
            Some(i64::from(*index)),
        ),
    }
}

/// Everything one candidate's pane settled: its cover, its album fields, its
/// row edits, the failure its last import left, and where the pane was.
pub(crate) fn load_pane_rows_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<DbCandidatePaneRows, DbError> {
    let metadata_draft = load_drafts_on(sql, CandidateRows::One(content_hash))?
        .remove(content_hash)
        .ok_or_else(|| {
            DbError::Message(format!(
                "candidate {content_hash} has no editable metadata draft"
            ))
        })?;
    Ok(DbCandidatePaneRows {
        cover: load_covers_on(sql, CandidateRows::One(content_hash))?.remove(content_hash),
        cover_options: load_cover_options_on(sql, content_hash)?,
        draft: metadata_draft,
        failure: load_failure_on(sql, content_hash)?,
        session: load_session_on(sql, content_hash)?,
    })
}

/// The cover choice of every candidate `only` names, each with the copies
/// the catalog serves of a remote one.
pub(crate) fn load_covers_on(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
    only: CandidateRows<'_>,
) -> Result<HashMap<String, CoverSelection>, DbError> {
    let only = only.filter("cover.content_hash");
    // Only a remote choice has an address for copies to reference.
    let copies = copies_json_sql(
        "import_candidate_cover_copy",
        "copy.content_hash = cover.content_hash AND copy.image_url = cover.url",
    );
    let rows = sql.query(
        &format!(
            "SELECT cover.content_hash, cover.kind, cover.file_id, cover.url, cover.source, \
                    {copies} AS copies \
             FROM import_candidate_cover AS cover {}",
            only.clause
        ),
        only.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("kind")?,
                row.get::<_, Option<String>>("file_id")?,
                row.get::<_, Option<String>>("url")?,
                row.get::<_, Option<String>>("source")?,
                row.get::<_, String>("copies")?,
            ))
        },
    )?;
    let mut out = HashMap::with_capacity(rows.len());
    for (content_hash, kind, file_id, url, source, copies) in rows {
        let cover = match kind.as_str() {
            "local" => CoverSelection::Local(
                file_id.ok_or_else(|| DbError::Message("a local cover names no file".into()))?,
            ),
            "embedded" => CoverSelection::Embedded(file_id.ok_or_else(|| {
                DbError::Message("an embedded cover names no source file".into())
            })?),
            "remote" => CoverSelection::Remote(
                RemoteImageSet::with_copies(
                    url.ok_or_else(|| DbError::Message("a remote cover names no address".into()))?,
                    copies_of(&copies)?,
                ),
                Catalog::from_str(
                    &source
                        .ok_or_else(|| DbError::Message("a remote cover names no source".into()))?,
                )
                .map_err(DbError::Message)?,
            ),
            other => return Err(unreadable("cover kind", other)),
        };
        out.insert(content_hash, cover);
    }
    Ok(out)
}

/// Every candidate's complete stored draft, or the one `only` names: one row
/// per draft, its lists riding on it as JSON arrays in their stored order.
pub(crate) fn load_drafts_on(
    sql: &SqlReadContext<'_>,
    only: CandidateRows<'_>,
) -> Result<HashMap<String, CandidateDraft>, DbError> {
    let filter = only.filter("edit.content_hash");
    let clause = &filter.clause;
    let album_assignments = assignments_json_sql(
        "import_candidate_album_artist_assignment",
        "assignment.content_hash = edit.content_hash",
    );
    let track_assignments = assignments_json_sql(
        "import_candidate_track_artist_assignment",
        "assignment.content_hash = track.content_hash AND assignment.track_id = track.track_id",
    );
    let rows = sql.query(
        &format!(
            "SELECT {EDIT_COLUMNS}, \
                    {album_assignments} AS album_assignments, \
                    (SELECT json_group_array(json_object( \
                         'carrier', medium.medium, 'sides', medium.side_count) \
                         ORDER BY medium.position) \
                     FROM import_candidate_medium AS medium \
                     WHERE medium.content_hash = edit.content_hash) AS mediums, \
                    (SELECT json_group_array(json_object( \
                         'track_id', track.track_id, 'title', track.title, \
                         'assignment_kind', track.artist_assignment_kind, \
                         'medium', track.medium, 'side', track.side, \
                         'track_number', track.track_number, \
                         'stated_length_ms', track.stated_length_ms, \
                         'file_kind', track.file_kind, 'file_id', track.file_id, \
                         'sheet_id', track.sheet_id, 'slice_index', track.slice_index, \
                         'assignments', json({track_assignments}), \
                         'roles', json({track_roles}), 'works', json({track_works})) \
                         ORDER BY track.position) \
                     FROM import_candidate_track AS track \
                     WHERE track.content_hash = edit.content_hash) AS tracks, \
                    {release_roles} AS release_roles, {works} AS works, {parts} AS parts \
             FROM import_candidate_edit AS edit {clause}",
            track_roles = values::TRACK_ROLES_JSON,
            track_works = values::TRACK_WORKS_JSON,
            release_roles = values::RELEASE_ROLES_JSON,
            works = values::WORKS_JSON,
            parts = values::WORK_PARTS_JSON,
        ),
        filter.params().as_slice(),
        |row| {
            Ok(StoredDraftRow {
                content_hash: row.get("content_hash")?,
                album_title: row.get("album_title")?,
                album_year: row.get("album_year")?,
                year: row.get("year")?,
                labels: read_label_rows(row)?,
                barcodes: super::super::pressing_columns::read_barcodes(row, "barcodes")?,
                facts: super::super::pressing_columns::read_facts(row, "")?,
                album_assignments: row.get("album_assignments")?,
                mediums: row.get("mediums")?,
                tracks: row.get("tracks")?,
                release_roles: row.get("release_roles")?,
                works: row.get("works")?,
                parts: row.get("parts")?,
            })
        },
    )?;
    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        let draft = row.draft()?;
        out.insert(row.content_hash, draft);
    }
    Ok(out)
}

/// A stored JSON column of `what`, decoded.
fn decoded<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<T, DbError> {
    serde_json::from_str(json)
        .map_err(|error| DbError::Message(format!("a draft's stored {what} {json:?}: {error}")))
}

/// One `import_candidate_edit` row as SQLite hands it over, its lists still
/// the JSON arrays [`load_drafts_on`] reads them as.
struct StoredDraftRow {
    content_hash: String,
    album_title: String,
    album_year: String,
    year: String,
    labels: Vec<RawLabelEdit>,
    barcodes: Vec<String>,
    facts: bae_types::pressing::PressingFacts,
    album_assignments: String,
    mediums: String,
    tracks: String,
    release_roles: String,
    works: String,
    parts: String,
}

impl StoredDraftRow {
    fn draft(&self) -> Result<CandidateDraft, DbError> {
        let album_artist_assignments =
            decoded::<Vec<StoredAssignment>>(&self.album_assignments, "album artists")?
                .into_iter()
                .map(StoredAssignment::credited)
                .collect::<Result<_, _>>()?;
        let mediums = decoded::<Vec<StoredMedium>>(&self.mediums, "mediums")?
            .into_iter()
            .map(|medium| {
                let carrier = medium
                    .carrier
                    .map(|key| {
                        bae_types::pressing::Medium::from_key(&key)
                            .ok_or_else(|| unreadable("draft medium", &key))
                    })
                    .transpose()?;
                Ok(crate::mediums::ReleaseMedium {
                    carrier,
                    sides: medium.sides,
                })
            })
            .collect::<Result<_, DbError>>()?;
        let tracks = decoded::<Vec<StoredTrack>>(&self.tracks, "tracks")?
            .into_iter()
            .map(StoredTrack::track)
            .collect::<Result<_, _>>()?;
        Ok(CandidateDraft {
            album_title: self.album_title.clone(),
            album_artist_assignments,
            album_year: self.album_year.clone(),
            pressing: RawPressingEdit {
                year: self.year.clone(),
                labels: self.labels.clone(),
                barcodes: self.barcodes.clone(),
                facts: self.facts.clone(),
            },
            mediums,
            tracks,
            release_roles: values::roles_of(decoded(&self.release_roles, "release roles")?)?,
            works: values::works_of(
                decoded(&self.works, "works")?,
                decoded(&self.parts, "work parts")?,
            ),
        })
    }
}

/// The draft's `labels` column: the form's label rows as typed.
fn read_label_rows(row: &rusqlite::Row) -> rusqlite::Result<Vec<RawLabelEdit>> {
    let labels: String = row.get("labels")?;
    serde_json::from_str::<Vec<RawLabelEdit>>(&labels).map_err(|error| {
        super::super::read::column_conversion_error(
            row,
            "labels",
            format!("label rows {labels:?}: {error}"),
        )
    })
}

/// One `import_candidate_medium` row as the draft's read holds it.
#[derive(serde::Deserialize)]
struct StoredMedium {
    carrier: Option<String>,
    sides: std::num::NonZeroU8,
}

/// One `import_candidate_track` row as the draft's read holds it, with its
/// artist assignments, roles and works.
#[derive(serde::Deserialize)]
struct StoredTrack {
    track_id: String,
    title: String,
    assignment_kind: String,
    medium: std::num::NonZeroU16,
    side: std::num::NonZeroU8,
    track_number: std::num::NonZeroU32,
    stated_length_ms: Option<i64>,
    file_kind: String,
    file_id: String,
    sheet_id: Option<String>,
    slice_index: Option<i64>,
    assignments: Vec<StoredAssignment>,
    roles: Vec<values::StoredRole>,
    works: Vec<String>,
}

impl StoredTrack {
    fn track(self) -> Result<CandidateTrack, DbError> {
        let missing = |what: &str| {
            DbError::Message(format!(
                "the track stored for {} states no {what}",
                self.track_id
            ))
        };
        let artist_assignments = match self.assignment_kind.as_str() {
            "album_artists" => TrackArtistAssignments::AlbumArtists,
            "explicit" => TrackArtistAssignments::Explicit(
                self.assignments
                    .into_iter()
                    .map(StoredAssignment::credited)
                    .collect::<Result<_, _>>()?,
            ),
            other => return Err(unreadable("artist assignment kind", other)),
        };
        let file = match self.file_kind.as_str() {
            "standalone" => AudioFile::Standalone {
                file_id: self.file_id,
            },
            "sheet_slice" => AudioFile::SheetSlice {
                file_id: self.file_id,
                sheet_id: self.sheet_id.ok_or_else(|| missing("sheet"))?,
                index: u32::try_from(self.slice_index.ok_or_else(|| missing("slice"))?)
                    .map_err(|error| missing(&format!("a readable slice ({error})")))?,
            },
            other => return Err(unreadable("file_kind", other)),
        };
        let stated_length_ms = self
            .stated_length_ms
            .map(u64::try_from)
            .transpose()
            .map_err(|error| missing(&format!("a readable stated length ({error})")))?;
        Ok(CandidateTrack {
            edit: RawTrackEdit {
                id: self.track_id,
                title: self.title,
                artist_assignments,
                file,
            },
            place: crate::mediums::TrackPlace {
                medium: self.medium,
                side: self.side,
                number: self.track_number,
            },
            stated_length_ms,
            roles: values::roles_of(self.roles)?,
            works: self.works,
        })
    }
}

struct AssignmentColumns<'a> {
    kind: &'static str,
    artist_id: Option<&'a str>,
    name: Option<&'a str>,
    sort_name: Option<&'a str>,
    musicbrainz_id: Option<&'a str>,
    discogs_id: Option<&'a str>,
    credit_key: Option<&'a str>,
    credited_as: Option<&'a str>,
    join_phrase: Option<&'a str>,
}

fn assignment_columns(credited: &Credited) -> AssignmentColumns<'_> {
    let join_phrase = credited.join.as_deref();
    match &credited.artist {
        ArtistAssignment::Picked {
            artist,
            credit_name,
        } => AssignmentColumns {
            kind: "picked",
            artist_id: Some(&artist.artist_id),
            name: None,
            sort_name: None,
            musicbrainz_id: None,
            discogs_id: None,
            credit_key: credit_name
                .as_ref()
                .map(|credit_name| credit_name.key.as_str()),
            credited_as: None,
            join_phrase,
        },
        ArtistAssignment::Credit { credit } => AssignmentColumns {
            kind: "credit",
            artist_id: None,
            name: Some(credit.name.as_str()),
            sort_name: credit.sort_name.as_deref(),
            musicbrainz_id: credit.musicbrainz_artist_id.as_deref(),
            discogs_id: credit.discogs_artist_id.as_deref(),
            credit_key: None,
            credited_as: credit.credited_as.as_deref(),
            join_phrase,
        },
    }
}

/// One stored assignment row, the picked artist's fields read from the
/// library and its credit name, if it names one still there, as
/// [`assignments_json_sql`] holds it.
#[derive(serde::Deserialize)]
struct StoredAssignment {
    kind: String,
    artist_id: Option<String>,
    name: Option<String>,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credit_key: Option<String>,
    credit_name: Option<String>,
    credited_as: Option<String>,
    join_phrase: Option<String>,
}

/// The assignments `table`, aliased `assignment`, holds where `belongs`
/// holds, as a JSON array in their order: each joined to its picked artist
/// and that artist's credit name.
fn assignments_json_sql(table: &str, belongs: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'kind', assignment.assignment_kind, 'artist_id', assignment.artist_id, \
             'name', CASE assignment.assignment_kind WHEN 'picked' THEN picked.name \
                 WHEN 'credit' THEN assignment.name END, \
             'sort_name', CASE assignment.assignment_kind WHEN 'picked' THEN picked.sort_name \
                 WHEN 'credit' THEN assignment.sort_name END, \
             'musicbrainz_artist_id', CASE assignment.assignment_kind \
                 WHEN 'picked' THEN picked.musicbrainz_artist_id \
                 WHEN 'credit' THEN assignment.musicbrainz_artist_id END, \
             'discogs_artist_id', CASE assignment.assignment_kind \
                 WHEN 'picked' THEN picked.discogs_artist_id \
                 WHEN 'credit' THEN assignment.discogs_artist_id END, \
             'credit_key', picked_credit.name_key, 'credit_name', picked_credit.name, \
             'credited_as', assignment.credited_as, 'join_phrase', assignment.join_phrase) \
             ORDER BY assignment.position) \
         FROM {table} AS assignment \
         LEFT JOIN artists picked ON picked.id = assignment.artist_id \
         LEFT JOIN artist_credits picked_credit \
             ON picked_credit.artist_id = assignment.artist_id \
            AND picked_credit.name_key = assignment.credit_key \
         WHERE {belongs})"
    )
}

impl StoredAssignment {
    /// The draft's credit. A picked artist's credit name another device
    /// removed since prints the artist's own name.
    fn credited(self) -> Result<Credited, DbError> {
        let artist = match self.kind.as_str() {
            "picked" => ArtistAssignment::Picked {
                artist: ExistingArtist {
                    artist_id: self.artist_id.ok_or_else(|| {
                        DbError::Message("a picked artist assignment names no artist".into())
                    })?,
                    name: self.name.ok_or_else(|| {
                        DbError::Message("a picked artist assignment names a missing artist".into())
                    })?,
                    sort_name: self.sort_name,
                    musicbrainz_artist_id: self.musicbrainz_artist_id,
                    discogs_artist_id: self.discogs_artist_id,
                },
                credit_name: match (self.credit_key, self.credit_name) {
                    (Some(key), Some(name)) => Some(crate::credits::CreditName { key, name }),
                    _ => None,
                },
            },
            "credit" => ArtistAssignment::Credit {
                credit: ArtistCredit {
                    name: self
                        .name
                        .ok_or_else(|| DbError::Message("an artist credit has no name".into()))?,
                    sort_name: self.sort_name,
                    musicbrainz_artist_id: self.musicbrainz_artist_id,
                    discogs_artist_id: self.discogs_artist_id,
                    credited_as: self.credited_as,
                },
            },
            other => return Err(unreadable("artist assignment kind", other)),
        };
        Ok(Credited {
            artist,
            join: self.join_phrase,
        })
    }
}

fn insert_album_artist_assignments(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        let columns = assignment_columns(assignment);
        sql.execute(
            "INSERT INTO import_candidate_album_artist_assignment \
             (content_hash, position, assignment_kind, artist_id, name, sort_name, \
              musicbrainz_artist_id, discogs_artist_id, credit_key, credited_as, join_phrase) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position as i64,
                columns.kind,
                columns.artist_id,
                columns.name,
                columns.sort_name,
                columns.musicbrainz_id,
                columns.discogs_id,
                columns.credit_key,
                columns.credited_as,
                columns.join_phrase
            ],
        )?;
    }
    Ok(())
}

fn insert_track_artist_assignments(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    track_id: &str,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        let columns = assignment_columns(assignment);
        sql.execute(
            "INSERT INTO import_candidate_track_artist_assignment \
             (content_hash, track_id, position, assignment_kind, artist_id, name, sort_name, \
              musicbrainz_artist_id, discogs_artist_id, credit_key, credited_as, join_phrase) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                track_id,
                position as i64,
                columns.kind,
                columns.artist_id,
                columns.name,
                columns.sort_name,
                columns.musicbrainz_id,
                columns.discogs_id,
                columns.credit_key,
                columns.credited_as,
                columns.join_phrase
            ],
        )?;
    }
    Ok(())
}

// ── The covers an applied catalog release offers the candidate's picker ──

/// Replace the candidate's offered covers with `options`, in their order.
pub(super) fn replace_cover_options(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    options: &[RemoteCover],
) -> Result<(), DbError> {
    // The copies hang off their option, so this takes them too.
    sql.execute(
        "DELETE FROM import_candidate_cover_option WHERE content_hash = ?",
        [content_hash],
    )?;
    for (position, option) in options.iter().enumerate() {
        let position = i64::try_from(position)
            .map_err(|error| DbError::Message(format!("too many cover options: {error}")))?;
        sql.execute(
            "INSERT INTO import_candidate_cover_option \
                 (content_hash, position, url, label, source, standing) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position,
                option.image.url,
                option.label,
                option.source.as_str(),
                option.standing.as_str(),
            ],
        )?;
        for copy in &option.image.downscaled {
            sql.execute(
                "INSERT INTO import_candidate_cover_option_copy \
                     (content_hash, position, max_edge, url) VALUES (?, ?, ?, ?)",
                params![content_hash, position, copy.max_edge, copy.url],
            )?;
        }
    }
    Ok(())
}

/// The candidate's offered covers, in their order, each with its copies.
pub(super) fn load_cover_options_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<Vec<RemoteCover>, DbError> {
    let copies = copies_json_sql(
        "import_candidate_cover_option_copy",
        "copy.content_hash = offered.content_hash AND copy.position = offered.position",
    );
    sql.query(
        &format!(
            "SELECT offered.url, offered.label, offered.source, offered.standing, {copies} \
             FROM import_candidate_cover_option AS offered \
             WHERE offered.content_hash = ? ORDER BY offered.position"
        ),
        [content_hash],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )?
    .into_iter()
    .map(|(url, label, source, standing, copies)| {
        Ok(RemoteCover {
            image: RemoteImageSet::with_copies(url, copies_of(&copies)?),
            label,
            source: bae_catalogs::catalog::Catalog::from_str(&source).map_err(|error| {
                DbError::Message(format!(
                    "a stored cover option's catalog is unreadable: {error}"
                ))
            })?,
            standing: CoverStanding::from_column(&standing)
                .ok_or_else(|| unreadable("cover option standing", &standing))?,
        })
    })
    .collect()
}
