//! The draft's rows — the album fields, the tracks with their decisions, the
//! artist assignments — as one candidate's save writes them and the pane's
//! read draws them with the candidate's cover (`cover_rows`).
//!
//! Every row group hangs off the candidate by content hash, and the draft's
//! own provenance and tracks hang off `import_candidate_edit`. A save writes
//! the draft over the one stored (`draft_store`), changing only the rows that
//! differ; which groups a candidate save writes, and under what checks, is
//! `preparation_rows`'s to say.
//! Its tables are `pane_rows.sql`, and `values.sql` and `provenance.sql` for
//! the draft's values and provenance, which every device's library holds.

pub(super) const SQL: &str = include_str!("pane_rows.sql");
pub(super) const VALUES_SQL: &str = values::SQL;
pub(super) const PROVENANCE_SQL: &str = provenance::SQL;

mod provenance;
mod values;

pub(crate) use provenance::{
    insert_provenance, read_provenance, store_provenance, StoredProvenance, PROVENANCE_COLUMNS,
};

use super::*;
use crate::cover_art::PreparedCover;
use crate::{
    ArtistAssignment, AudioFile, CandidateDraft, CandidateTrack, CoverSelection, Credited,
    MetadataAuthor, RawLabelEdit, RawPressingEdit, RawTrackEdit, TrackArtistAssignments,
};
use bae_catalogs::credits::ArtistCredit;
use std::str::FromStr;

const EDIT_COLUMNS: &str = "content_hash, album_title, album_year, year, \
     country, region, media, status, packaging, discogs_details";

const TRACK_COLUMNS: &str = "content_hash, track_id, position, title, \
     artist_assignment_kind, medium, side, track_number, stated_length_ms, \
     file_kind, file_id, sheet_id, slice_index";

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
        other => Err(DbError::unreadable("a draft's stored author", other)),
    }
}

/// Delete the draft of `content_hash`: its source and every row below it
/// hang off the draft row and go with it.
pub(super) fn delete_draft(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_edit WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

/// The draft row of a candidate that has none yet, with everything hanging
/// off it: its album artists, its mediums, its tracks and their values.
pub(super) fn insert_draft(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    draft: &CandidateDraft,
    author: MetadataAuthor,
) -> Result<(), DbError> {
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_edit ({EDIT_COLUMNS}, author, draft_blank, draft_valid) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ),
        rusqlite::params_from_iter(edit_row_values(content_hash, draft, author)),
    )?;
    insert_label_rows(sql, content_hash, &draft.pressing.labels)?;
    insert_barcode_rows(sql, content_hash, &draft.pressing.barcodes)?;
    insert_album_artist_assignments(sql, content_hash, &draft.album_artist_assignments)?;
    insert_draft_body(sql, content_hash, draft)
}

/// The draft row's columns, in [`EDIT_COLUMNS`] order and then its author
/// and whether it is blank and would import.
pub(super) fn edit_row_values(
    content_hash: &str,
    draft: &CandidateDraft,
    author: MetadataAuthor,
) -> Vec<rusqlite::types::Value> {
    use rusqlite::types::Value;
    let release_edit = draft.release_edit();
    let facts = bae_records::records::pressing_columns::FactColumns::of(&draft.pressing.facts);
    let text = |value: &str| Value::Text(value.to_string());
    let optional = |value: Option<&str>| value.map_or(Value::Null, text);
    vec![
        text(content_hash),
        text(&draft.album_title),
        text(&draft.album_year),
        text(&draft.pressing.year),
        optional(facts.country),
        optional(facts.region),
        Value::Text(bae_records::records::pressing_columns::media_column(
            &draft.pressing.facts.media,
        )),
        optional(facts.status),
        optional(facts.packaging),
        Value::Text(facts.discogs_details),
        text(author_column(author)),
        Value::Integer(release_edit.is_blank().into()),
        Value::Integer(release_edit.shape().is_ok().into()),
    ]
}

/// The draft's label rows, as the form holds them, where none are stored.
pub(super) fn insert_label_rows(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    labels: &[RawLabelEdit],
) -> Result<(), DbError> {
    for (position, label) in labels.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_edit_label \
                 (content_hash, position, name, catalog_number, \
                  musicbrainz_label_id, discogs_label_id) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position as i64,
                label.name,
                label.catalog_number,
                label.musicbrainz_label_id,
                label.discogs_label_id
            ],
        )?;
    }
    Ok(())
}

/// The draft's barcode rows, as the form holds them, where none are stored.
pub(super) fn insert_barcode_rows(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    barcodes: &[String],
) -> Result<(), DbError> {
    for (position, barcode) in barcodes.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_edit_barcode (content_hash, position, barcode) \
             VALUES (?, ?, ?)",
            params![content_hash, position as i64, barcode],
        )?;
    }
    Ok(())
}

/// The draft's mediums, tracks and the values hanging off them: the rows
/// below the draft row besides its album artists.
pub(super) fn insert_draft_body(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    draft: &CandidateDraft,
) -> Result<(), DbError> {
    for (index, medium) in draft.mediums.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_medium (content_hash, position, medium, side_count) \
             VALUES (?, ?, ?, ?)",
            params![
                content_hash,
                bae_types::mediums::medium_position(index),
                medium.carrier.map(bae_types::pressing::Medium::key),
                medium.sides,
            ],
        )?;
    }
    // The works first: a track names the works it performs.
    values::insert_draft_values(sql, content_hash, draft)?;
    for (position, track) in draft.tracks.iter().enumerate() {
        insert_track(sql, content_hash, position, track)?;
    }
    Ok(())
}

/// The track at `position` with the rows hanging off it, where none is
/// stored.
fn insert_track(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    position: usize,
    track: &CandidateTrack,
) -> Result<(), DbError> {
    store_track_row(sql, content_hash, position, track)?;
    insert_track_rows(sql, content_hash, track)
}

/// The track's own row at `position`, in place of the one stored with its id.
pub(super) fn store_track_row(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    position: usize,
    track: &CandidateTrack,
) -> Result<(), DbError> {
    let (file_kind, file_id, sheet_id, slice_index) = mapping_file_columns(&track.edit.file);
    let stated_length_ms = track
        .stated_length_ms
        .map(|length| to_i64(length, "a track's stated length"))
        .transpose()?;
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_track ({TRACK_COLUMNS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (content_hash, track_id) DO UPDATE SET \
                 position = excluded.position, title = excluded.title, \
                 artist_assignment_kind = excluded.artist_assignment_kind, \
                 medium = excluded.medium, side = excluded.side, \
                 track_number = excluded.track_number, \
                 stated_length_ms = excluded.stated_length_ms, file_kind = excluded.file_kind, \
                 file_id = excluded.file_id, sheet_id = excluded.sheet_id, \
                 slice_index = excluded.slice_index"
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
    Ok(())
}

/// The rows hanging off a track: its own artists, its roles and its works.
pub(super) fn insert_track_rows(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    track: &CandidateTrack,
) -> Result<(), DbError> {
    if let TrackArtistAssignments::Explicit(assignments) = &track.edit.artist_assignments {
        insert_track_artist_assignments(sql, content_hash, &track.edit.id, assignments)?;
    }
    values::insert_track_values(sql, content_hash, track)
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
        AudioFile::SheetSlice { file_id, sheet_id, index } => {
            ("sheet_slice", file_id.as_str(), Some(sheet_id.as_str()), Some(i64::from(*index)))
        }
    }
}

/// Everything one candidate's pane settled: its cover, its album fields, its
/// row edits, the failure its last import left, and where the pane was.
pub(crate) fn load_pane_rows_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<DbCandidatePaneRows, DbError> {
    let (draft, cover) = load_draft_and_cover_on(sql, content_hash)?;
    Ok(DbCandidatePaneRows {
        cover,
        draft,
        failure: load_failure_on(sql, content_hash)?,
        session: load_session_on(sql, content_hash)?,
    })
}

impl ImportTables {
    /// Everything a person settled about one candidate through its pane.
    pub async fn load_import_candidate_pane_rows(
        &self,
        content_hash: &str,
    ) -> Result<DbCandidatePaneRows, DbError> {
        let content_hash = content_hash.to_string();
        self.store.read(move |sql| load_pane_rows_on(&sql, &content_hash)).await
    }
}

/// Whether the candidate `content_hash` holds a draft row.
pub(super) fn has_draft_row_on(
    sql: &impl bae_store::query::QueryOne,
    content_hash: &str,
) -> Result<bool, DbError> {
    Ok(sql
        .query_row(
            "SELECT 1 FROM import_candidate_edit WHERE content_hash = ?",
            [content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// The candidate's draft and cover: its rows', or, with no draft row, the
/// blank draft of its files and the folder's own cover, which a scan seeds a
/// new candidate with when it reads no tags. A candidate whose release this
/// device deleted holds no draft row, and reads as that until a write stores
/// one. A candidate this device imported holds no draft, so reading one
/// fails.
pub(super) fn load_draft_and_cover_on(
    sql: &(impl bae_store::query::QueryOne + bae_store::query::QueryRows),
    content_hash: &str,
) -> Result<(CandidateDraft, Option<CoverSelection>), DbError> {
    if let Some(draft) = load_drafts_on(sql, CandidateRows::One(content_hash))?.remove(content_hash)
    {
        return Ok((draft, super::cover_rows::load_cover_on(sql, content_hash)?));
    }
    if crate::tables::import_origin::imported_release_on(sql, content_hash)?.is_some() {
        return Err(DbError::Message(format!(
            "candidate {content_hash} was imported, and holds no draft"
        )));
    }
    let key: CandidateKey = sql
        .query_row(
            "SELECT candidate_key FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| DbError::Message(format!("candidate {content_hash} has no state")))?;
    let candidate = crate::tables::release_groupings::load_candidate_on(sql, &key)?
        .map(|stored| stored.candidate)
        .filter(|candidate| candidate.files.content_hash() == content_hash)
        .ok_or_else(|| {
            DbError::Message(format!(
                "candidate {content_hash} holds no draft row, and the scan holds no files of it \
                 at {key} to read its blank one from"
            ))
        })?;
    Ok((candidate.blank_draft(), candidate.blank_cover().as_ref().map(PreparedCover::selection)))
}

/// Every candidate's complete stored draft, or the one `only` names: one row
/// per draft, its lists riding on it as JSON arrays in their stored order.
pub(crate) fn load_drafts_on(
    sql: &(impl bae_store::query::QueryOne + bae_store::query::QueryRows),
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
                    (SELECT json_group_array(json_object( \
                         'name', label.name, 'catalog_number', label.catalog_number, \
                         'musicbrainz_label_id', label.musicbrainz_label_id, \
                         'discogs_label_id', label.discogs_label_id) \
                         ORDER BY label.position) \
                     FROM import_candidate_edit_label AS label \
                     WHERE label.content_hash = edit.content_hash) AS labels, \
                    (SELECT json_group_array(barcode.barcode ORDER BY barcode.position) \
                     FROM import_candidate_edit_barcode AS barcode \
                     WHERE barcode.content_hash = edit.content_hash) AS barcodes, \
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
                barcodes: bae_records::records::pressing_columns::read_barcodes(row, "barcodes")?,
                facts: bae_records::records::pressing_columns::read_facts(row, "")?,
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
                        bae_types::pressing::Medium::from_key(&key).ok_or_else(|| {
                            DbError::unreadable("import_candidate_medium.medium", &key)
                        })
                    })
                    .transpose()?;
                Ok(bae_types::mediums::ReleaseMedium { carrier, sides: medium.sides })
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

/// The draft's label rows as typed, as the read's `labels` array holds them.
fn read_label_rows(row: &rusqlite::Row) -> rusqlite::Result<Vec<RawLabelEdit>> {
    bae_store::read::json_column(row, "labels")
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
            DbError::Message(format!("the track stored for {} states no {what}", self.track_id))
        };
        let artist_assignments = match self.assignment_kind.as_str() {
            "album_artists" => TrackArtistAssignments::AlbumArtists,
            "explicit" => TrackArtistAssignments::Explicit(
                self.assignments
                    .into_iter()
                    .map(StoredAssignment::credited)
                    .collect::<Result<_, _>>()?,
            ),
            other => return Err(DbError::unreadable("a stored artist assignment's kind", other)),
        };
        let file = match self.file_kind.as_str() {
            "standalone" => AudioFile::Standalone { file_id: self.file_id },
            "sheet_slice" => AudioFile::SheetSlice {
                file_id: self.file_id,
                sheet_id: self.sheet_id.ok_or_else(|| missing("sheet"))?,
                index: to_u32(
                    self.slice_index.ok_or_else(|| missing("slice"))?,
                    "import_candidate_track.slice_index",
                )?,
            },
            other => return Err(DbError::unreadable("import_candidate_track.file_kind", other)),
        };
        let stated_length_ms = self
            .stated_length_ms
            .map(|length| to_u64(length, "import_candidate_track.stated_length_ms"))
            .transpose()?;
        Ok(CandidateTrack {
            edit: RawTrackEdit { id: self.track_id, title: self.title, artist_assignments, file },
            place: bae_types::mediums::TrackPlace {
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
        ArtistAssignment::Picked { artist, credit_name } => AssignmentColumns {
            kind: "picked",
            artist_id: Some(&artist.artist_id),
            name: None,
            sort_name: None,
            musicbrainz_id: None,
            discogs_id: None,
            credit_key: credit_name.as_ref().map(|credit_name| credit_name.key.as_str()),
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
                    (Some(key), Some(name)) => Some(bae_records::credits::CreditName { key, name }),
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
            other => return Err(DbError::unreadable("a stored artist assignment's kind", other)),
        };
        Ok(Credited { artist, join: self.join_phrase })
    }
}

fn insert_album_artist_assignments(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        store_album_artist_assignment(sql, content_hash, position, assignment)?;
    }
    Ok(())
}

/// Make the album artist at `position` `assignment`, in place when one is
/// there.
pub(super) fn store_album_artist_assignment(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    position: usize,
    assignment: &Credited,
) -> Result<(), DbError> {
    let columns = assignment_columns(assignment);
    sql.execute(
        "INSERT INTO import_candidate_album_artist_assignment \
         (content_hash, position, assignment_kind, artist_id, name, sort_name, \
          musicbrainz_artist_id, discogs_artist_id, credit_key, credited_as, join_phrase) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (content_hash, position) DO UPDATE SET \
             assignment_kind = excluded.assignment_kind, artist_id = excluded.artist_id, \
             name = excluded.name, sort_name = excluded.sort_name, \
             musicbrainz_artist_id = excluded.musicbrainz_artist_id, \
             discogs_artist_id = excluded.discogs_artist_id, credit_key = excluded.credit_key, \
             credited_as = excluded.credited_as, join_phrase = excluded.join_phrase",
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
    Ok(())
}

fn insert_track_artist_assignments(
    sql: &CovenWrite<'_>,
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

/// Seed a candidate's draft from the folder's file tags, authored by the
/// pre-fill, with provenance naming the tags as its source.
pub(super) fn insert_file_tags_draft(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    draft: &crate::CandidateDraft,
) -> Result<(), DbError> {
    pane_rows::insert_draft(sql, content_hash, draft, crate::MetadataAuthor::Prefill)?;
    insert_provenance(sql, content_hash, &crate::MetadataProvenance::FileMetadata)?;
    Ok(())
}

/// Everything a person settled about one candidate through its pane, keyed by
/// the same content hash the rest of its state is.
///
/// Read as a group by the per-candidate query, which draws the header, the
/// cover and the mapping table from them at once. Kept off
/// [`DbImportCandidateState`] because that one is read for the whole queue and
/// none of these are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCandidatePaneRows {
    /// The cover the user chose. `None` leaves the picked release's default
    /// cover standing.
    pub cover: Option<crate::CoverSelection>,
    /// The candidate's one stored draft: album fields and one row per audio
    /// slot, each carrying its physical decision.
    pub draft: crate::CandidateDraft,
    /// Where the pane was when the person last left this candidate. `None`
    /// before the pane has been touched.
    pub session: Option<crate::CandidateSession>,
    /// The last import of this candidate that failed.
    pub failure: Option<crate::ImportFailure>,
}

#[cfg(test)]
#[path = "pane_rows_tests.rs"]
mod tests;
