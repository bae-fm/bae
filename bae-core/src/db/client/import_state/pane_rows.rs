//! What the pane's own controls write, and the read that draws it: the cover,
//! the album fields, the track rows, the failure an import left behind, and
//! the failure the last import left behind.
//!
//! Each write is one row, or one column of one row, and each is addressed by
//! the candidate's content hash. They hang off `import_candidate_state`, so a
//! write with no state row under it is refused rather than absorbed: the edit
//! form is drawn only under a pick, and a pick writes that row.

mod writes;

use super::verdict_rows::unreadable;
use super::*;
use crate::db::client::candidate_state_rows::{require_state_row, save_cover, COVER_COLUMNS};
use crate::import::{
    ArtistAssignment, AudioFile, CandidateDraft, CandidateEditField, CandidateTrack,
    CandidateTrackEdit, CoverSelection, ExistingArtist, NewArtistSeed, RawPressingEdit,
    RawTrackEdit, TrackArtistAssignments, TrackEditState, TrackFileAuthor,
};

const EDIT_COLUMNS: &str = "content_hash, album_title, album_year, year, format, \
     label, catalog_number, country, barcode";

const TRACK_COLUMNS: &str = "content_hash, track_id, position, title, \
     artist_assignment_kind, side, track_number, named_by_source, dropped, file_author, \
     file_kind, file_id, sheet_id, slice_index";

fn advance_metadata_revision(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<u64, DbError> {
    let revision = sql
        .query_row(
            "UPDATE import_candidate_state \
             SET metadata_revision = metadata_revision + 1 \
             WHERE content_hash = ? RETURNING metadata_revision",
            [content_hash],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| {
            DbError::Message(format!(
                "metadata revision write for {content_hash} has no candidate state row"
            ))
        })?;
    u64::try_from(revision)
        .map_err(|_| DbError::Message("candidate metadata revision is negative".to_string()))
}

pub(super) fn require_metadata_revision(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    expected: u64,
) -> Result<(), DbError> {
    let stored: i64 = sql
        .query_row(
            "SELECT metadata_revision FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| DbError::Message("candidate metadata row is missing".into()))?;
    if u64::try_from(stored).ok() != Some(expected) {
        return Err(DbError::Message(format!(
            "candidate metadata changed from revision {expected}"
        )));
    }
    Ok(())
}

pub(super) fn require_file_edit_revision(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    expected: u64,
) -> Result<(), DbError> {
    let stored: i64 = sql
        .query_row(
            "SELECT edit_revision FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| DbError::Message("candidate file-edit row is missing".into()))?;
    if u64::try_from(stored).ok() != Some(expected) {
        return Err(DbError::Message(format!(
            "candidate files changed from revision {expected}"
        )));
    }
    Ok(())
}

pub(super) fn delete_cover(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_cover WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

pub(super) fn save_edit_field(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    field: CandidateEditField,
    value: &str,
) -> Result<(), DbError> {
    require_state_row(sql, content_hash, "metadata edit")?;
    let column = field.column();
    let changed = sql.execute(
        &format!("UPDATE import_candidate_edit SET {column} = ? WHERE content_hash = ?"),
        params![value, content_hash],
    )?;
    if changed != 1 {
        return Err(DbError::Message(format!(
            "metadata draft field write changed {changed} rows; expected exactly one"
        )));
    }
    Ok(())
}

fn save_edit_field_and_advance(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    field: CandidateEditField,
    value: &str,
) -> Result<u64, DbError> {
    save_edit_field(sql, content_hash, field, value)?;
    advance_metadata_revision(sql, content_hash)
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

pub(super) fn save_track_edit(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    edit: &CandidateTrackEdit,
) -> Result<(), DbError> {
    require_state_row(sql, content_hash, "track edit")?;
    if let TrackEditState::Edited(track) = &edit.state {
        let changed = sql.execute(
            "UPDATE import_candidate_track SET title = ?, artist_assignment_kind = ?, \
                 side = ?, track_number = ? WHERE content_hash = ? AND track_id = ?",
            params![
                track.title,
                assignment_kind_column(&track.artist_assignments),
                track.side,
                track.track_number,
                content_hash,
                edit.track_id,
            ],
        )?;
        if changed != 1 {
            return Err(DbError::Message(format!(
                "track draft edit changed {changed} rows for {}; expected exactly one",
                edit.track_id
            )));
        }
        sql.execute(
            "DELETE FROM import_candidate_track_artist_assignment \
             WHERE content_hash = ? AND track_id = ?",
            params![content_hash, edit.track_id],
        )?;
        if let TrackArtistAssignments::Explicit(assignments) = &track.artist_assignments {
            insert_track_artist_assignments(sql, content_hash, &edit.track_id, assignments)?;
        }
    }
    update_track_decision(sql, content_hash, edit)
}

fn replace_track_artist_assignments(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    track_ids: &[String],
    assignments: &TrackArtistAssignments,
) -> Result<(), DbError> {
    require_state_row(sql, content_hash, "track artist fill")?;
    let (assignment_kind, explicit) = match assignments {
        TrackArtistAssignments::AlbumArtists => ("album_artists", None),
        TrackArtistAssignments::Explicit(assignments) => ("explicit", Some(assignments.as_slice())),
    };
    for track_id in track_ids {
        let changed = sql.execute(
            "UPDATE import_candidate_track SET artist_assignment_kind = ? \
             WHERE content_hash = ? AND track_id = ?",
            params![assignment_kind, content_hash, track_id],
        )?;
        if changed != 1 {
            return Err(DbError::Message(format!(
                "track artist fill changed {changed} rows for {track_id}; expected exactly one"
            )));
        }
        sql.execute(
            "DELETE FROM import_candidate_track_artist_assignment \
             WHERE content_hash = ? AND track_id = ?",
            params![content_hash, track_id],
        )?;
        if let Some(explicit) = explicit {
            insert_track_artist_assignments(sql, content_hash, track_id, explicit)?;
        }
    }
    Ok(())
}

fn update_track_decision(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    edit: &CandidateTrackEdit,
) -> Result<(), DbError> {
    let dropped = matches!(&edit.state, TrackEditState::Dropped);
    let (file_kind, file_id, sheet_id, slice_index) = mapping_file_columns(edit.file());
    let changed = sql.execute(
        "UPDATE import_candidate_track SET dropped = ?, \
             file_author = CASE \
                 WHEN file_kind IS ? AND file_id IS ? AND sheet_id IS ? AND slice_index IS ? \
                 THEN file_author ELSE 'user' END, \
             file_kind = ?, \
             file_id = ?, sheet_id = ?, slice_index = ? \
         WHERE content_hash = ? AND track_id = ?",
        params![
            dropped,
            file_kind,
            file_id,
            sheet_id,
            slice_index,
            file_kind,
            file_id,
            sheet_id,
            slice_index,
            content_hash,
            edit.track_id,
        ],
    )?;
    if changed != 1 {
        return Err(DbError::Message(format!(
            "track decision edit changed {changed} rows for {}; expected exactly one",
            edit.track_id
        )));
    }
    Ok(())
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
