//! What the pane's own controls write, and the read that draws it: the cover,
//! the album fields, the track rows, the failure an import left behind, and
//! the measurements a selection took of units identification never reached.
//!
//! Each write is one row, or one column of one row, and each is addressed by
//! the candidate's content hash. They hang off `import_candidate_state`, so a
//! write with no state row under it is refused rather than absorbed: the edit
//! form is drawn only under a pick, and a pick writes that row.

use super::verdict_rows::unreadable;
use super::*;
use crate::import::{
    ArtistAssignment, AudioFile, CandidateEditField, CandidateTrackEdit, CandidateTrackMappingEdit,
    CoverSelection, ExistingArtist, ImportFailure, NewArtistSeed, RawPressingEdit, RawReleaseEdit,
    RawTrackEdit, TrackArtistAssignments, TrackEditState,
};

const COVER_COLUMNS: &str = "content_hash, kind, file_id, url, source";

const EDIT_COLUMNS: &str = "content_hash, album_title, year, format, \
     label, catalog_number, country, barcode";

const TRACK_EDIT_COLUMNS: &str = "content_hash, track_id, position, title, \
     artist_assignment_kind, side, track_number";
const TRACK_MAPPING_COLUMNS: &str =
    "content_hash, track_id, dropped, file_kind, file_id, sheet_id, slice_index";

/// The anchor row must exist before anything hangs off it.
fn require_state_row(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    what: &str,
) -> Result<(), DbError> {
    let present = sql
        .query_row(
            "SELECT 1 FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if present {
        return Ok(());
    }
    Err(DbError::Message(format!(
        "the {what} for {content_hash} has no candidate state row"
    )))
}

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

pub(super) fn save_cover(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    cover: &CoverSelection,
) -> Result<(), DbError> {
    require_state_row(sql, content_hash, "cover choice")?;
    let (kind, file_id, url, source) = match cover {
        CoverSelection::Local(file_id) => ("local", Some(file_id.as_str()), None, None),
        CoverSelection::Remote(url, source) => {
            ("remote", None, Some(url.as_str()), Some(source.as_str()))
        }
    };
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_cover ({COVER_COLUMNS}) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT (content_hash) DO UPDATE SET \
                 kind = excluded.kind, file_id = excluded.file_id, \
                 url = excluded.url, source = excluded.source"
        ),
        params![content_hash, kind, file_id, url, source],
    )?;
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

pub(super) fn replace_draft(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    draft: &RawReleaseEdit,
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
    draft: &RawReleaseEdit,
) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO import_candidate_edit \
             (content_hash, album_title, year, format, label, catalog_number, country, barcode) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            content_hash,
            draft.album_title,
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
        let assignment_kind = match &track.artist_assignments {
            TrackArtistAssignments::AlbumArtists => "album_artists",
            TrackArtistAssignments::Explicit(_) => "explicit",
        };
        sql.execute(
            "INSERT INTO import_candidate_track_edit \
                 (content_hash, track_id, position, title, artist_assignment_kind, side, track_number) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                track.id,
                position as i64,
                track.title,
                assignment_kind,
                track.side,
                track.track_number,
            ],
        )?;
        if let TrackArtistAssignments::Explicit(assignments) = &track.artist_assignments {
            insert_track_artist_assignments(sql, content_hash, &track.id, assignments)?;
        }
    }
    Ok(())
}

pub(super) fn save_track_edit(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    edit: &CandidateTrackEdit,
) -> Result<(), DbError> {
    require_state_row(sql, content_hash, "track edit")?;
    if let TrackEditState::Edited(track) = &edit.state {
        let assignment_kind = match &track.artist_assignments {
            TrackArtistAssignments::AlbumArtists => "album_artists",
            TrackArtistAssignments::Explicit(_) => "explicit",
        };
        let changed = sql.execute(
            "UPDATE import_candidate_track_edit SET title = ?, artist_assignment_kind = ?, \
                 side = ?, track_number = ? WHERE content_hash = ? AND track_id = ?",
            params![
                track.title,
                assignment_kind,
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
    save_track_mapping(
        sql,
        content_hash,
        &CandidateTrackMappingEdit::from_track_edit(edit),
    )
}

pub(super) fn delete_track_mappings(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_track_mapping WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

fn save_track_mapping(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    mapping: &CandidateTrackMappingEdit,
) -> Result<(), DbError> {
    let (file_kind, file_id, sheet_id, slice_index) = match &mapping.file {
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
    };
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_track_mapping ({TRACK_MAPPING_COLUMNS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (content_hash, track_id) DO UPDATE SET \
                 dropped = excluded.dropped, file_kind = excluded.file_kind, \
                 file_id = excluded.file_id, sheet_id = excluded.sheet_id, \
                 slice_index = excluded.slice_index"
        ),
        params![
            content_hash,
            mapping.track_id,
            mapping.dropped,
            file_kind,
            file_id,
            sheet_id,
            slice_index,
        ],
    )?;
    Ok(())
}

/// Everything one candidate's pane settled: its cover, its album fields, its
/// row edits and the failure its last import left.
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
        metadata_draft,
        track_mappings: load_track_mappings_on(sql, Some(content_hash))?
            .remove(content_hash)
            .unwrap_or_default(),
        failure: load_failures_on(sql, Some(content_hash))?.remove(content_hash),
    })
}

/// Every candidate's cover choice, or the one `only` names.
fn load_covers_on(
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

/// Every candidate's complete editable metadata draft, or the one `only` names.
pub(crate) fn load_drafts_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, RawReleaseEdit>, DbError> {
    let album_assignments = load_album_artist_assignments_on(sql, only)?;
    let track_assignments = load_track_artist_assignments_on(sql, only)?;
    let track_rows = sql.query(
        &format!(
            "SELECT {TRACK_EDIT_COLUMNS} FROM import_candidate_track_edit \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, position"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("track_id")?,
                row.get::<_, String>("title")?,
                row.get::<_, String>("artist_assignment_kind")?,
                row.get::<_, i32>("side")?,
                row.get::<_, Option<i32>>("track_number")?,
            ))
        },
    )?;
    let mut tracks: HashMap<String, Vec<RawTrackEdit>> = HashMap::new();
    for (content_hash, track_id, title, assignment_kind, side, track_number) in track_rows {
        let artist_assignments = match assignment_kind.as_str() {
            "album_artists" => TrackArtistAssignments::AlbumArtists,
            "explicit" => TrackArtistAssignments::Explicit(
                track_assignments
                    .get(&(content_hash.clone(), track_id.clone()))
                    .cloned()
                    .unwrap_or_default(),
            ),
            other => return Err(unreadable("artist assignment kind", other)),
        };
        tracks.entry(content_hash).or_default().push(RawTrackEdit {
            id: track_id,
            title,
            artist_assignments,
            side,
            track_number,
            file: None,
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
    for (content_hash, album_title, year, format, label, catalog_number, country, barcode) in rows {
        out.insert(
            content_hash.clone(),
            RawReleaseEdit {
                album_title,
                album_artist_assignments: album_assignments
                    .get(&content_hash)
                    .cloned()
                    .unwrap_or_default(),
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

/// Every candidate's physical track decisions, or the one `only` names.
fn load_track_mappings_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<CandidateTrackMappingEdit>>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT {TRACK_MAPPING_COLUMNS} FROM import_candidate_track_mapping \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, track_id"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("track_id")?,
                row.get::<_, i64>("dropped")?,
                row.get::<_, Option<String>>("file_kind")?,
                row.get::<_, Option<String>>("file_id")?,
                row.get::<_, Option<String>>("sheet_id")?,
                row.get::<_, Option<i64>>("slice_index")?,
            ))
        },
    )?;
    let mut out: HashMap<String, Vec<CandidateTrackMappingEdit>> = HashMap::new();
    for (content_hash, track_id, dropped, file_kind, file_id, sheet_id, slice_index) in rows {
        let missing = |what: &str| {
            DbError::Message(format!(
                "the track mapping stored for {track_id} states no {what}"
            ))
        };
        let file = match file_kind.as_deref() {
            None => None,
            Some("standalone") => Some(AudioFile::Standalone {
                file_id: file_id.ok_or_else(|| missing("file"))?,
            }),
            Some("sheet_slice") => Some(AudioFile::SheetSlice {
                file_id: file_id.ok_or_else(|| missing("file"))?,
                sheet_id: sheet_id.ok_or_else(|| missing("sheet"))?,
                index: u32::try_from(slice_index.ok_or_else(|| missing("slice"))?)
                    .map_err(|_| missing("a readable slice"))?,
            }),
            Some(other) => return Err(unreadable("file_kind", other)),
        };
        out.entry(content_hash)
            .or_default()
            .push(CandidateTrackMappingEdit {
                track_id,
                dropped: dropped == 1,
                file,
            });
    }
    Ok(out)
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

/// The failure the last import of `only` left, or every candidate's.
fn load_failures_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, ImportFailure>, DbError> {
    let rows = sql.query(
        "SELECT content_hash, error, failed_at FROM import_candidate_failure \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("error")?,
                rfc3339_column(row, "failed_at")?,
            ))
        },
    )?;
    Ok(rows
        .into_iter()
        .map(|(content_hash, error, failed_at)| (content_hash, ImportFailure { error, failed_at }))
        .collect())
}

impl Database {
    /// Everything a person settled about one candidate through its pane.
    pub async fn load_import_candidate_pane_rows(
        &self,
        content_hash: &str,
    ) -> Result<DbCandidatePaneRows, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| load_pane_rows_on(&sql, &content_hash))
            .await
    }

    /// Record what some of a candidate's audio units play for, leaving the
    /// rest of its measurements alone — the pane's own read of the units
    /// identification never measured.
    pub async fn save_import_candidate_durations(
        &self,
        content_hash: &str,
        durations: &crate::import::probe::ProbedDurations,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let durations = durations.clone();
        self.call(move |sql| insert_durations(sql, &content_hash, &durations))
            .await
    }

    /// Record that an import of this candidate failed, so the pane still
    /// offers Retry after a relaunch.
    ///
    /// The anchor row is created when nothing has identified or picked the
    /// candidate: an import driven straight from a command has no pick behind
    /// it, and the failure is still a fact about those bytes.
    pub async fn save_import_candidate_failure(
        &self,
        content_hash: &str,
        folder_path: &str,
        edit_revision: u64,
        error: &str,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let folder_path = folder_path.to_string();
        let error = error.to_string();
        let edit_revision = i64::try_from(edit_revision).map_err(|_| {
            DbError::Message(format!(
                "candidate edit revision {edit_revision} exceeds SQLite's integer range"
            ))
        })?;
        let now = self.inner.clock.now().to_rfc3339();
        self.call(move |sql| {
            sql.execute(
                "INSERT INTO import_candidate_state (content_hash, folder_path, edit_revision) \
                 VALUES (?, ?, ?) ON CONFLICT (content_hash) DO NOTHING",
                params![content_hash, folder_path, edit_revision],
            )?;
            sql.execute(
                "INSERT INTO import_candidate_failure (content_hash, error, failed_at) \
                 VALUES (?, ?, ?) \
                 ON CONFLICT (content_hash) DO UPDATE SET \
                     error = excluded.error, failed_at = excluded.failed_at",
                params![content_hash, error, now],
            )?;
            Ok(())
        })
        .await
    }

    /// Forget the last failure — what queueing an import of this candidate
    /// does before the worker takes it.
    pub async fn clear_import_candidate_failure(&self, content_hash: &str) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        self.call(move |sql| {
            sql.execute(
                "DELETE FROM import_candidate_failure WHERE content_hash = ?",
                [&content_hash],
            )?;
            Ok(())
        })
        .await
    }

    /// Record the cover the user chose for this candidate.
    pub async fn save_import_candidate_cover(
        &self,
        content_hash: &str,
        cover: &crate::import::CoverSelection,
    ) -> Result<u64, DbError> {
        let content_hash = content_hash.to_string();
        let cover = cover.clone();
        self.call(move |sql| {
            save_cover(sql, &content_hash, &cover)?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    /// Record one album-level field the user typed.
    pub async fn save_import_candidate_edit_field(
        &self,
        content_hash: &str,
        field: crate::import::CandidateEditField,
        value: &str,
    ) -> Result<u64, DbError> {
        let content_hash = content_hash.to_string();
        let value = value.to_string();
        self.call(move |sql| {
            save_edit_field(sql, &content_hash, field, &value)?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    /// Replace the candidate's ordered album-artist override. An empty list is
    /// rejected because every savable album has an artist; deleting the whole
    /// candidate edit resets the override to its metadata draft.
    pub async fn replace_import_candidate_album_artists(
        &self,
        content_hash: &str,
        assignments: &[crate::import::ArtistAssignment],
    ) -> Result<u64, DbError> {
        if assignments.is_empty() {
            return Err(DbError::Message(
                "a candidate album artist override cannot be empty".into(),
            ));
        }
        let content_hash = content_hash.to_string();
        let assignments = assignments.to_vec();
        self.call(move |sql| {
            require_state_row(sql, &content_hash, "album artist edit")?;
            sql.execute(
                "DELETE FROM import_candidate_album_artist_assignment WHERE content_hash = ?",
                [&content_hash],
            )?;
            insert_album_artist_assignments(sql, &content_hash, &assignments)?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    /// Record one mapping-table row the user changed, or dropped.
    pub async fn save_import_candidate_track_edit(
        &self,
        content_hash: &str,
        edit: &crate::import::CandidateTrackEdit,
    ) -> Result<u64, DbError> {
        let content_hash = content_hash.to_string();
        let edit = edit.clone();
        self.call(move |sql| {
            save_track_edit(sql, &content_hash, &edit)?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }
}
