//! The draft's rows — the album fields, the tracks with their decisions, the
//! artist assignments, the cover — as one candidate's save writes them and
//! the pane's read draws them.
//!
//! Every row group hangs off `import_candidate_state` by content hash. The
//! writers here replace a whole group; which groups a candidate save
//! replaces, and under what checks, is `preparation_rows`'s to say.

mod writes;

use super::verdict_rows::unreadable;
use super::*;
use crate::db::client::candidate_state_rows::COVER_COLUMNS;
use crate::import::{
    ArtistAssignment, AudioFile, CandidateDraft, CandidateTrack, CoverSelection, ExistingArtist,
    NewArtistSeed, RawPressingEdit, RawTrackEdit, TrackArtistAssignments, TrackFileAuthor,
};

const EDIT_COLUMNS: &str = "content_hash, album_title, album_year, year, format, \
     label, catalog_number, country, barcode";

const TRACK_COLUMNS: &str = "content_hash, track_id, position, title, \
     artist_assignment_kind, side, track_number, named_by_source, dropped, file_author, \
     file_kind, file_id, sheet_id, slice_index";

pub(super) fn delete_cover(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_cover WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

pub(super) fn replace_draft(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    draft: &CandidateDraft,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_edit WHERE content_hash = ?",
        [content_hash],
    )?;
    insert_draft(sql, content_hash, draft)
}

pub(crate) fn insert_draft(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    draft: &CandidateDraft,
) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO import_candidate_edit \
             (content_hash, album_title, album_year, year, format, label, catalog_number, country, barcode) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            content_hash,
            draft.album_title,
            draft.album_year,
            draft.pressing.year,
            draft.pressing.format,
            draft.pressing.label,
            draft.pressing.catalog_number,
            draft.pressing.country,
            draft.pressing.barcode,
        ],
    )?;
    insert_album_artist_assignments(sql, content_hash, &draft.album_artist_assignments)?;
    for (position, track) in draft.tracks.iter().enumerate() {
        let (file_kind, file_id, sheet_id, slice_index) =
            mapping_file_columns(track.edit.file.as_ref());
        sql.execute(
            &format!(
                "INSERT INTO import_candidate_track ({TRACK_COLUMNS}) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            ),
            params![
                content_hash,
                track.edit.id,
                position as i64,
                track.edit.title,
                assignment_kind_column(&track.edit.artist_assignments),
                track.edit.side,
                track.edit.track_number,
                track.named_by_source,
                track.dropped,
                file_author_column(track.file_author),
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
    Ok(())
}

fn assignment_kind_column(assignments: &TrackArtistAssignments) -> &'static str {
    match assignments {
        TrackArtistAssignments::AlbumArtists => "album_artists",
        TrackArtistAssignments::Explicit(_) => "explicit",
    }
}

fn file_author_column(author: TrackFileAuthor) -> &'static str {
    match author {
        TrackFileAuthor::Automatic => "automatic",
        TrackFileAuthor::User => "user",
    }
}

fn mapping_file_columns(
    file: Option<&AudioFile>,
) -> (Option<&str>, Option<&str>, Option<&str>, Option<i64>) {
    match file {
        None => (None, None, None, None),
        Some(AudioFile::Standalone { file_id }) => {
            (Some("standalone"), Some(file_id.as_str()), None, None)
        }
        Some(AudioFile::SheetSlice {
            file_id,
            sheet_id,
            index,
        }) => (
            Some("sheet_slice"),
            Some(file_id.as_str()),
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
    let mut out = HashMap::with_capacity(rows.len());
    for (content_hash, kind, file_id, url, source) in rows {
        let cover = match kind.as_str() {
            "local" => CoverSelection::Local(
                file_id.ok_or_else(|| DbError::Message("a local cover names no file".into()))?,
            ),
            "embedded" => CoverSelection::Embedded(file_id.ok_or_else(|| {
                DbError::Message("an embedded cover names no source file".into())
            })?),
            "remote" => CoverSelection::Remote(
                url.ok_or_else(|| DbError::Message("a remote cover names no address".into()))?,
                MetadataSource::from_str(
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

/// Every candidate's complete stored draft, or the one `only` names.
pub(crate) fn load_drafts_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, CandidateDraft>, DbError> {
    let album_assignments = load_album_artist_assignments_on(sql, only)?;
    let track_assignments = load_track_artist_assignments_on(sql, only)?;
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
                side: row.get("side")?,
                track_number: row.get("track_number")?,
                named_by_source: row.get::<_, i64>("named_by_source")? == 1,
                dropped: row.get::<_, i64>("dropped")? == 1,
                file_author: row.get("file_author")?,
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
        let file = match row.file_kind.as_deref() {
            None => None,
            Some("standalone") => Some(AudioFile::Standalone {
                file_id: row.file_id.clone().ok_or_else(|| missing("file"))?,
            }),
            Some("sheet_slice") => Some(AudioFile::SheetSlice {
                file_id: row.file_id.clone().ok_or_else(|| missing("file"))?,
                sheet_id: row.sheet_id.clone().ok_or_else(|| missing("sheet"))?,
                index: u32::try_from(row.slice_index.ok_or_else(|| missing("slice"))?)
                    .map_err(|_| missing("a readable slice"))?,
            }),
            Some(other) => return Err(unreadable("file_kind", other)),
        };
        let file_author = match row.file_author.as_str() {
            "automatic" => TrackFileAuthor::Automatic,
            "user" => TrackFileAuthor::User,
            other => return Err(unreadable("file_author", other)),
        };
        tracks
            .entry(row.content_hash)
            .or_default()
            .push(CandidateTrack {
                edit: RawTrackEdit {
                    id: row.track_id,
                    title: row.title,
                    artist_assignments,
                    side: row.side,
                    track_number: row.track_number,
                    file,
                },
                named_by_source: row.named_by_source,
                dropped: row.dropped,
                file_author,
            });
    }
    let rows = sql.query(
        &format!(
            "SELECT {EDIT_COLUMNS} FROM import_candidate_edit \
             WHERE :only IS NULL OR content_hash = :only"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("album_title")?,
                row.get::<_, String>("album_year")?,
                row.get::<_, String>("year")?,
                row.get::<_, String>("format")?,
                row.get::<_, String>("label")?,
                row.get::<_, String>("catalog_number")?,
                row.get::<_, String>("country")?,
                row.get::<_, String>("barcode")?,
            ))
        },
    )?;
    let mut out = HashMap::with_capacity(rows.len());
    for (
        content_hash,
        album_title,
        album_year,
        year,
        format,
        label,
        catalog_number,
        country,
        barcode,
    ) in rows
    {
        out.insert(
            content_hash.clone(),
            CandidateDraft {
                album_title,
                album_artist_assignments: album_assignments
                    .get(&content_hash)
                    .cloned()
                    .unwrap_or_default(),
                album_year,
                pressing: RawPressingEdit {
                    year,
                    format,
                    label,
                    catalog_number,
                    country,
                    barcode,
                },
                tracks: tracks.remove(&content_hash).unwrap_or_default(),
            },
        );
    }
    Ok(out)
}

/// One `import_candidate_track` row as SQLite hands it over.
struct StoredTrackRow {
    content_hash: String,
    track_id: String,
    title: String,
    assignment_kind: String,
    side: i32,
    track_number: Option<i32>,
    named_by_source: bool,
    dropped: bool,
    file_author: String,
    file_kind: Option<String>,
    file_id: Option<String>,
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
}

fn assignment_columns(assignment: &ArtistAssignment) -> AssignmentColumns<'_> {
    match assignment {
        ArtistAssignment::Existing { artist } => AssignmentColumns {
            kind: "existing",
            artist_id: Some(&artist.artist_id),
            name: None,
            sort_name: None,
            musicbrainz_id: None,
            discogs_id: None,
        },
        ArtistAssignment::New { seed } => AssignmentColumns {
            kind: "new",
            artist_id: None,
            name: Some(seed.name.as_str()),
            sort_name: seed.sort_name.as_deref(),
            musicbrainz_id: seed.musicbrainz_artist_id.as_deref(),
            discogs_id: seed.discogs_artist_id.as_deref(),
        },
    }
}

fn assignment_from_columns(
    kind: String,
    artist_id: Option<String>,
    name: Option<String>,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
) -> Result<ArtistAssignment, DbError> {
    match kind.as_str() {
        "existing" => Ok(ArtistAssignment::Existing {
            artist: ExistingArtist {
                artist_id: artist_id.ok_or_else(|| {
                    DbError::Message("an existing artist assignment names no artist".into())
                })?,
                name: name.ok_or_else(|| {
                    DbError::Message("an existing artist assignment names a missing artist".into())
                })?,
                sort_name,
                musicbrainz_artist_id,
                discogs_artist_id,
            },
        }),
        "new" => Ok(ArtistAssignment::New {
            seed: NewArtistSeed {
                name: name.ok_or_else(|| {
                    DbError::Message("a new artist assignment has no name".into())
                })?,
                sort_name,
                musicbrainz_artist_id,
                discogs_artist_id,
            },
        }),
        other => Err(unreadable("artist assignment kind", other)),
    }
}

fn insert_album_artist_assignments(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    assignments: &[ArtistAssignment],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        let columns = assignment_columns(assignment);
        sql.execute(
            "INSERT INTO import_candidate_album_artist_assignment \
             (content_hash, position, assignment_kind, artist_id, name, sort_name, \
              musicbrainz_artist_id, discogs_artist_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position as i64,
                columns.kind,
                columns.artist_id,
                columns.name,
                columns.sort_name,
                columns.musicbrainz_id,
                columns.discogs_id
            ],
        )?;
    }
    Ok(())
}

fn insert_track_artist_assignments(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    track_id: &str,
    assignments: &[ArtistAssignment],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        let columns = assignment_columns(assignment);
        sql.execute(
            "INSERT INTO import_candidate_track_artist_assignment \
             (content_hash, track_id, position, assignment_kind, artist_id, name, sort_name, \
              musicbrainz_artist_id, discogs_artist_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                track_id,
                position as i64,
                columns.kind,
                columns.artist_id,
                columns.name,
                columns.sort_name,
                columns.musicbrainz_id,
                columns.discogs_id
            ],
        )?;
    }
    Ok(())
}

fn load_album_artist_assignments_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<ArtistAssignment>>, DbError> {
    let rows = sql.query(
        "SELECT assignment.content_hash, assignment.assignment_kind, assignment.artist_id, \
                CASE assignment.assignment_kind WHEN 'existing' THEN existing.name \
                    WHEN 'new' THEN assignment.name END, \
                CASE assignment.assignment_kind WHEN 'existing' THEN existing.sort_name \
                    WHEN 'new' THEN assignment.sort_name END, \
                CASE assignment.assignment_kind WHEN 'existing' \
                    THEN existing.musicbrainz_artist_id \
                    WHEN 'new' THEN assignment.musicbrainz_artist_id END, \
                CASE assignment.assignment_kind WHEN 'existing' THEN existing.discogs_artist_id \
                    WHEN 'new' THEN assignment.discogs_artist_id END \
         FROM import_candidate_album_artist_assignment assignment \
         LEFT JOIN artists existing ON existing.id = assignment.artist_id \
         WHERE :only IS NULL OR assignment.content_hash = :only \
         ORDER BY assignment.content_hash, assignment.position",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        },
    )?;
    let mut out: HashMap<String, Vec<ArtistAssignment>> = HashMap::new();
    for (content_hash, kind, artist_id, name, sort_name, musicbrainz_id, discogs_id) in rows {
        out.entry(content_hash)
            .or_default()
            .push(assignment_from_columns(
                kind,
                artist_id,
                name,
                sort_name,
                musicbrainz_id,
                discogs_id,
            )?);
    }
    Ok(out)
}

fn load_track_artist_assignments_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<(String, String), Vec<ArtistAssignment>>, DbError> {
    let rows = sql.query(
        "SELECT assignment.content_hash, assignment.track_id, assignment.assignment_kind, \
                assignment.artist_id, \
                CASE assignment.assignment_kind WHEN 'existing' THEN existing.name \
                    WHEN 'new' THEN assignment.name END, \
                CASE assignment.assignment_kind WHEN 'existing' THEN existing.sort_name \
                    WHEN 'new' THEN assignment.sort_name END, \
                CASE assignment.assignment_kind WHEN 'existing' \
                    THEN existing.musicbrainz_artist_id \
                    WHEN 'new' THEN assignment.musicbrainz_artist_id END, \
                CASE assignment.assignment_kind WHEN 'existing' THEN existing.discogs_artist_id \
                    WHEN 'new' THEN assignment.discogs_artist_id END \
         FROM import_candidate_track_artist_assignment assignment \
         LEFT JOIN artists existing ON existing.id = assignment.artist_id \
         WHERE :only IS NULL OR assignment.content_hash = :only \
         ORDER BY assignment.content_hash, assignment.track_id, assignment.position",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        },
    )?;
    let mut out: HashMap<(String, String), Vec<ArtistAssignment>> = HashMap::new();
    for (content_hash, track_id, kind, artist_id, name, sort_name, musicbrainz_id, discogs_id) in
        rows
    {
        out.entry((content_hash, track_id))
            .or_default()
            .push(assignment_from_columns(
                kind,
                artist_id,
                name,
                sort_name,
                musicbrainz_id,
                discogs_id,
            )?);
    }
    Ok(out)
}
