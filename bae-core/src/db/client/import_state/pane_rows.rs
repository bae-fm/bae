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

use super::verdict_rows::unreadable;
use super::*;
use crate::db::client::candidate_state_rows::COVER_COLUMNS;
use crate::import::{
    ArtistAssignment, ArtistCredit, AudioFile, CandidateDraft, CandidateTrack, CoverSelection,
    Credited, ExistingArtist, MetadataAuthor, RawLabelEdit, RawPressingEdit, RawTrackEdit,
    TrackArtistAssignments,
};
use bae_catalogs::remote_image::{CoverStanding, DownscaledCopy, RemoteCover, RemoteImageSet};
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
    let metadata_draft = load_drafts_on(sql, Some(content_hash))?
        .remove(content_hash)
        .ok_or_else(|| {
            DbError::Message(format!(
                "candidate {content_hash} has no editable metadata draft"
            ))
        })?;
    Ok(DbCandidatePaneRows {
        cover: load_covers_on(sql, Some(content_hash))?.remove(content_hash),
        cover_options: load_cover_options_on(sql, content_hash)?,
        draft: metadata_draft,
        failure: load_failure_on(sql, content_hash)?,
        session: load_session_on(sql, content_hash)?,
    })
}

/// Every candidate's cover choice, or the one `only` names.
pub(crate) fn load_covers_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, CoverSelection>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT {COVER_COLUMNS} FROM import_candidate_cover \
             WHERE :only IS NULL OR content_hash = :only"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("kind")?,
                row.get::<_, Option<String>>("file_id")?,
                row.get::<_, Option<String>>("url")?,
                row.get::<_, Option<String>>("source")?,
            ))
        },
    )?;
    let mut copies: HashMap<String, Vec<DownscaledCopy>> = HashMap::new();
    for (content_hash, max_edge, url) in sql.query(
        "SELECT content_hash, max_edge, url FROM import_candidate_cover_copy \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )? {
        copies
            .entry(content_hash)
            .or_default()
            .push(DownscaledCopy { url, max_edge });
    }
    let mut out = HashMap::with_capacity(rows.len());
    for (content_hash, kind, file_id, url, source) in rows {
        // Only a remote choice has an address for copies to reference.
        let copies = copies.remove(&content_hash).unwrap_or_default();
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
                    copies,
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

/// Who wrote every candidate's draft, or the one `only` names.
pub(crate) fn load_authors_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, MetadataAuthor>, DbError> {
    sql.query(
        "SELECT content_hash, author FROM import_candidate_edit \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?
    .into_iter()
    .map(|(content_hash, author)| Ok((content_hash, author_of(&author)?)))
    .collect()
}

/// Every candidate's complete stored draft, or the one `only` names.
pub(crate) fn load_drafts_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, CandidateDraft>, DbError> {
    let album_assignments = load_album_artist_assignments_on(sql, only)?;
    let track_assignments = load_track_artist_assignments_on(sql, only)?;
    let mut values = values::load_draft_values_on(sql, only)?;
    let track_rows = sql.query(
        &format!(
            "SELECT {TRACK_COLUMNS} FROM import_candidate_track \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, position"
        ),
        named_params! { ":only": only },
        |row| {
            Ok(StoredTrackRow {
                content_hash: row.get("content_hash")?,
                track_id: row.get("track_id")?,
                title: row.get("title")?,
                assignment_kind: row.get("artist_assignment_kind")?,
                medium: row.get("medium")?,
                side: row.get("side")?,
                track_number: row.get("track_number")?,
                stated_length_ms: row.get("stated_length_ms")?,
                file_kind: row.get("file_kind")?,
                file_id: row.get("file_id")?,
                sheet_id: row.get("sheet_id")?,
                slice_index: row.get("slice_index")?,
            })
        },
    )?;
    let mut tracks: HashMap<String, Vec<CandidateTrack>> = HashMap::new();
    for row in track_rows {
        let artist_assignments = match row.assignment_kind.as_str() {
            "album_artists" => TrackArtistAssignments::AlbumArtists,
            "explicit" => TrackArtistAssignments::Explicit(
                track_assignments
                    .get(&(row.content_hash.clone(), row.track_id.clone()))
                    .cloned()
                    .unwrap_or_default(),
            ),
            other => return Err(unreadable("artist assignment kind", other)),
        };
        let missing = |what: &str| {
            DbError::Message(format!(
                "the track stored for {} states no {what}",
                row.track_id
            ))
        };
        let file = match row.file_kind.as_str() {
            "standalone" => AudioFile::Standalone {
                file_id: row.file_id.clone(),
            },
            "sheet_slice" => AudioFile::SheetSlice {
                file_id: row.file_id.clone(),
                sheet_id: row.sheet_id.clone().ok_or_else(|| missing("sheet"))?,
                index: u32::try_from(row.slice_index.ok_or_else(|| missing("slice"))?)
                    .map_err(|error| missing(&format!("a readable slice ({error})")))?,
            },
            other => return Err(unreadable("file_kind", other)),
        };
        let stated_length_ms = row
            .stated_length_ms
            .map(u64::try_from)
            .transpose()
            .map_err(|error| missing(&format!("a readable stated length ({error})")))?;
        let key = (row.content_hash, row.track_id);
        let roles = values.track_roles.remove(&key).unwrap_or_default();
        let works = values.track_works.remove(&key).unwrap_or_default();
        let (content_hash, track_id) = key;
        tracks
            .entry(content_hash)
            .or_default()
            .push(CandidateTrack {
                edit: RawTrackEdit {
                    id: track_id,
                    title: row.title,
                    artist_assignments,
                    file,
                },
                place: crate::mediums::TrackPlace {
                    medium: row.medium,
                    side: row.side,
                    number: row.track_number,
                },
                stated_length_ms,
                roles,
                works,
            });
    }
    let medium_rows = sql.query(
        "SELECT content_hash, medium, side_count FROM import_candidate_medium \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, position",
        named_params! { ":only": only },
        |row| {
            let content_hash: String = row.get("content_hash")?;
            let carrier: Option<String> = row.get("medium")?;
            Ok((content_hash, carrier, row.get("side_count")?))
        },
    )?;
    let mut mediums: HashMap<String, Vec<crate::mediums::ReleaseMedium>> = HashMap::new();
    for (content_hash, carrier, sides) in medium_rows {
        let carrier = carrier
            .map(|key| {
                bae_types::pressing::Medium::from_key(&key)
                    .ok_or_else(|| unreadable("draft medium", &key))
            })
            .transpose()?;
        mediums
            .entry(content_hash)
            .or_default()
            .push(crate::mediums::ReleaseMedium { carrier, sides });
    }
    let rows = sql.query(
        &format!(
            "SELECT {EDIT_COLUMNS} FROM import_candidate_edit \
             WHERE :only IS NULL OR content_hash = :only"
        ),
        named_params! { ":only": only },
        |row| {
            Ok(StoredDraftRow {
                content_hash: row.get("content_hash")?,
                album_title: row.get("album_title")?,
                album_year: row.get("album_year")?,
                year: row.get("year")?,
                labels: read_label_rows(row)?,
                barcodes: super::super::pressing_columns::read_barcodes(row, "barcodes")?,
                facts: super::super::pressing_columns::read_facts(row, "")?,
            })
        },
    )?;
    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        out.insert(
            row.content_hash.clone(),
            CandidateDraft {
                album_title: row.album_title,
                album_artist_assignments: album_assignments
                    .get(&row.content_hash)
                    .cloned()
                    .unwrap_or_default(),
                album_year: row.album_year,
                pressing: RawPressingEdit {
                    year: row.year,
                    labels: row.labels,
                    barcodes: row.barcodes,
                    facts: row.facts,
                },
                mediums: mediums.remove(&row.content_hash).unwrap_or_default(),
                tracks: tracks.remove(&row.content_hash).unwrap_or_default(),
                release_roles: values
                    .release_roles
                    .remove(&row.content_hash)
                    .unwrap_or_default(),
                works: values.works.remove(&row.content_hash).unwrap_or_default(),
            },
        );
    }
    Ok(out)
}

/// One `import_candidate_edit` row as SQLite hands it over.
struct StoredDraftRow {
    content_hash: String,
    album_title: String,
    album_year: String,
    year: String,
    labels: Vec<RawLabelEdit>,
    barcodes: Vec<String>,
    facts: bae_types::pressing::PressingFacts,
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

/// One `import_candidate_track` row as SQLite hands it over.
struct StoredTrackRow {
    content_hash: String,
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
/// library and its credit name, if it names one still there.
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

/// The columns [`StoredAssignment::read`] reads from an assignment table
/// aliased `assignment`, joined to its picked artist as `picked` and that
/// artist's credit name as `picked_credit`.
const STORED_ASSIGNMENT_COLUMNS: &str = "assignment.assignment_kind, assignment.artist_id, \
     CASE assignment.assignment_kind WHEN 'picked' THEN picked.name \
         WHEN 'credit' THEN assignment.name END AS name, \
     CASE assignment.assignment_kind WHEN 'picked' THEN picked.sort_name \
         WHEN 'credit' THEN assignment.sort_name END AS sort_name, \
     CASE assignment.assignment_kind WHEN 'picked' THEN picked.musicbrainz_artist_id \
         WHEN 'credit' THEN assignment.musicbrainz_artist_id END AS musicbrainz_artist_id, \
     CASE assignment.assignment_kind WHEN 'picked' THEN picked.discogs_artist_id \
         WHEN 'credit' THEN assignment.discogs_artist_id END AS discogs_artist_id, \
     picked_credit.name_key AS credit_key, picked_credit.name AS credit_name, \
     assignment.credited_as, assignment.join_phrase";

const STORED_ASSIGNMENT_JOINS: &str =
    "LEFT JOIN artists picked ON picked.id = assignment.artist_id \
     LEFT JOIN artist_credits picked_credit \
         ON picked_credit.artist_id = assignment.artist_id \
        AND picked_credit.name_key = assignment.credit_key";

impl StoredAssignment {
    fn read(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            kind: row.get("assignment_kind")?,
            artist_id: row.get("artist_id")?,
            name: row.get("name")?,
            sort_name: row.get("sort_name")?,
            musicbrainz_artist_id: row.get("musicbrainz_artist_id")?,
            discogs_artist_id: row.get("discogs_artist_id")?,
            credit_key: row.get("credit_key")?,
            credit_name: row.get("credit_name")?,
            credited_as: row.get("credited_as")?,
            join_phrase: row.get("join_phrase")?,
        })
    }

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

pub(crate) fn load_album_artist_assignments_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<Credited>>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT assignment.content_hash, {STORED_ASSIGNMENT_COLUMNS} \
             FROM import_candidate_album_artist_assignment assignment \
             {STORED_ASSIGNMENT_JOINS} \
             WHERE :only IS NULL OR assignment.content_hash = :only \
             ORDER BY assignment.content_hash, assignment.position"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                StoredAssignment::read(row)?,
            ))
        },
    )?;
    let mut out: HashMap<String, Vec<Credited>> = HashMap::new();
    for (content_hash, stored) in rows {
        out.entry(content_hash)
            .or_default()
            .push(stored.credited()?);
    }
    Ok(out)
}

fn load_track_artist_assignments_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<(String, String), Vec<Credited>>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT assignment.content_hash, assignment.track_id, {STORED_ASSIGNMENT_COLUMNS} \
             FROM import_candidate_track_artist_assignment assignment \
             {STORED_ASSIGNMENT_JOINS} \
             WHERE :only IS NULL OR assignment.content_hash = :only \
             ORDER BY assignment.content_hash, assignment.track_id, assignment.position"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("track_id")?,
                StoredAssignment::read(row)?,
            ))
        },
    )?;
    let mut out: HashMap<(String, String), Vec<Credited>> = HashMap::new();
    for (content_hash, track_id, stored) in rows {
        out.entry((content_hash, track_id))
            .or_default()
            .push(stored.credited()?);
    }
    Ok(out)
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

/// The candidate's offered covers, in their order.
pub(super) fn load_cover_options_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<Vec<RemoteCover>, DbError> {
    let mut copies: HashMap<i64, Vec<DownscaledCopy>> = HashMap::new();
    for (position, max_edge, url) in sql.query(
        "SELECT position, max_edge, url FROM import_candidate_cover_option_copy \
         WHERE content_hash = ? ORDER BY position, max_edge",
        [content_hash],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )? {
        copies
            .entry(position)
            .or_default()
            .push(DownscaledCopy { url, max_edge });
    }
    sql.query(
        "SELECT position, url, label, source, standing FROM import_candidate_cover_option \
         WHERE content_hash = ? ORDER BY position",
        [content_hash],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )?
    .into_iter()
    .map(|(position, url, label, source, standing)| {
        Ok(RemoteCover {
            // An option the catalog serves no copies of has none.
            image: RemoteImageSet::with_copies(url, copies.remove(&position).unwrap_or_default()),
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
