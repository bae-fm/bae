//! A release's tracks as the release editor reads their audio: as the library
//! holds them, which the editor opens on ([`release_edit_tracks_on`]), and as
//! a form's draft lays them out ([`draft_release_edit_tracks_on`]).

use super::read::source_audio_layout_of;
use super::release_tracks::{track_position_of, track_side_of};
use super::*;
use bae_store::statement_rows;
use std::collections::BTreeMap;

// The rows of the release editor's tracks, over the release `release` and the
// edits `edits` its form makes to the CUE sheets, with the release's tracks at
// `places` on `mediums`: each track at a place, each track the edits add among
// them, and each side or disc of a medium no track sits on (`empty_side`).
//
// A row gives its side's kind and ordinal and its number there, its stored
// track as a JSON object whose length is the one the edits lay out (a track on
// a line moved to another file plays to that file's end where it played to
// the end of the file before), the length its place lists for it where that
// disagrees with the track's own (`bae_lengths_disagree`), how many audio
// formats it stores and how many main segments they hold, and its first main
// segment as a JSON object: the file it names, where it starts in that file,
// and the name and audio layout of the release file of that id where the
// release holds one. A track a CUE sheet carves has the sheet as a JSON
// object: its name, the side its first track sits on, the choices of side, how
// many of its `FILE` lines play audio, the ones that play none, the line the
// track is under and the audio files that line can play from.
sqlive_compose::fragment!(
    EditorTrackRows(release: String, edits: String)
    with mediums(
        id TEXT, release_id TEXT, position INTEGER, medium TEXT, side_count INTEGER
    ), places(
        id TEXT, release_id TEXT, medium_id TEXT, side INTEGER, track_number INTEGER,
        stated_length_ms INTEGER, position INTEGER
    ) {
        row_id: String,
        placed_medium: Option<i64>,
        placed_position: Option<i64>,
        empty_side: bool,
        side_kind: String,
        side_ordinal: i64,
        place_number: i64,
        listed_length_ms: Option<i64>,
        track: Option<String>,
        audio_format_count: i64,
        main_segment_count: i64,
        main_segment: Option<String>,
        is_added: bool,
        file_duration_ms: Option<i64>,
        sheet: Option<String>
    },
    r#"
    WITH placed AS (
        SELECT place.*
        FROM @crate::records::place_fragments::TrackPlaces(
            mediums: (SELECT * FROM mediums),
            places: (SELECT * FROM places)) AS place),
    sheeted AS (
        SELECT carried.*
        FROM @crate::records::place_fragments::PlacedWithSheetSides(
            placed: (SELECT track_id, position, medium_position, side, number, side_kind,
                            side_ordinal, stated_length_ms
                     FROM placed)) AS carried),
    choices AS (
        SELECT choice.*
        FROM @crate::records::place_fragments::SheetSideChoices(
            mediums: (SELECT * FROM mediums)) AS choice),
    listed_choices AS (
        SELECT listed.*
        FROM @crate::records::place_fragments::SheetSideChoicesJson(
            mediums: (SELECT * FROM mediums)) AS listed),
    candidates AS (
        SELECT track_id AS row_id, 1 AS has_track, medium_position, position, number, side_kind,
               side_ordinal, stated_length_ms, sheet_medium, sheet_side, sheet_kind,
               sheet_ordinal
        FROM sheeted
        UNION ALL
        SELECT 'empty:' || medium || ':' || side, 0, medium, side, 0, kind, ordinal, NULL, NULL,
               NULL, NULL, NULL
        FROM choices),
    counted AS (
        SELECT candidates.*,
               SUM(has_track) OVER (PARTITION BY medium_position) AS tracks_on_medium
        FROM candidates),
    shown AS (
        SELECT row_id, medium_position, position, number, side_kind, side_ordinal,
               stated_length_ms, sheet_medium, sheet_side, sheet_kind, sheet_ordinal,
               NOT has_track AS empty_side
        FROM counted
        WHERE has_track OR tracks_on_medium = 0),
    segments AS MATERIALIZED (
        SELECT segment.*
        FROM @crate::records::sheet_fragments::LaidSegments(
            release: :release, edits: :edits) AS segment),
    main AS MATERIALIZED (
        SELECT track_id, file_id, start_sample * 1000 / sample_rate AS start_ms,
               end_sample IS NULL AND moved AS to_new_end, added,
               ROW_NUMBER() OVER (PARTITION BY track_id ORDER BY segment_index) AS n,
               COUNT(*) OVER (PARTITION BY track_id) AS main_count
        FROM segments WHERE role = 'main'),
    files AS MATERIALIZED (
        SELECT laid_file.*
        FROM @crate::records::sheet_fragments::LaidFiles(
            release: :release, edits: :edits) AS laid_file),
    captions AS MATERIALIZED (
        SELECT caption.*
        FROM @crate::records::sheet_fragments::SheetLineCaptions(
            release: :release, edits: :edits) AS caption),
    laid AS (
        SELECT shown.*, held.id AS held_id, held.release_id, held.title, held.medium_id,
               held.side, held.track_number, held.stated_length_ms AS held_stated_length_ms,
               held.created_at,
               CASE WHEN main.to_new_end
                    THEN file.source_audio_duration_ms - main.start_ms
                    ELSE held.duration_ms END AS laid_duration_ms,
               COALESCE(main.main_count, 0) AS main_count, main.file_id AS main_file_id,
               main.start_ms AS main_start_ms, COALESCE(main.added, 0) AS is_added,
               file.id AS found_file_id, file.original_filename AS main_file_name,
               file.source_audio_layout AS main_file_layout,
               file.source_audio_duration_ms AS file_duration_ms
        FROM shown
        LEFT JOIN tracks AS held ON held.id = shown.row_id
        LEFT JOIN main ON main.track_id = shown.row_id AND main.n = 1
        LEFT JOIN files AS file ON file.id = main.file_id)
    SELECT laid.row_id, laid.medium_position AS placed_medium,
           laid.position AS placed_position, laid.empty_side, laid.side_kind,
           laid.side_ordinal, laid.number AS place_number,
           CASE WHEN laid.stated_length_ms IS NOT NULL
                     AND bae_lengths_disagree(laid.laid_duration_ms, laid.stated_length_ms)
                THEN laid.stated_length_ms END AS listed_length_ms,
           CASE WHEN laid.held_id IS NOT NULL THEN json_object(
               'id', laid.held_id, 'release_id', laid.release_id, 'title', laid.title,
               'medium_id', laid.medium_id, 'side', laid.side,
               'track_number', laid.track_number, 'duration_ms', laid.laid_duration_ms,
               'stated_length_ms', laid.held_stated_length_ms,
               'created_at', laid.created_at) END AS track,
           (SELECT COUNT(*) FROM audio_formats AS stored WHERE stored.track_id = laid.row_id)
               AS audio_format_count,
           laid.main_count AS main_segment_count,
           CASE WHEN laid.main_file_id IS NOT NULL THEN json_object(
               'file_id', laid.main_file_id, 'start_ms', laid.main_start_ms,
               'found', json(CASE WHEN laid.found_file_id IS NOT NULL THEN 'true'
                                  ELSE 'false' END),
               'name', laid.main_file_name, 'layout', laid.main_file_layout) END
               AS main_segment,
           laid.is_added, laid.file_duration_ms,
           (SELECT json_object(
                    'file_id', cut.sheet_file_id, 'name', sheet_file.original_filename,
                    'place', json_object(
                        'medium', laid.sheet_medium, 'side', laid.sheet_side,
                        'kind', laid.sheet_kind, 'ordinal', laid.sheet_ordinal),
                    'side_choices', json((SELECT choices FROM listed_choices)),
                    'line_count', caption.bound_count,
                    'file_reference', cut.file_reference,
                    'unbound_lines', json(caption.unbound),
                    'audio_options', json((
                        SELECT line.value ->> 'options'
                        FROM json_each(caption.bound) AS line
                        WHERE line.value ->> 'file_reference' = cut.file_reference)))
            FROM carved_tracks AS cut
            JOIN release_files AS sheet_file ON sheet_file.id = cut.sheet_file_id
            LEFT JOIN captions AS caption ON caption.sheet_file_id = cut.sheet_file_id
            WHERE cut.track_id = laid.row_id) AS sheet
    FROM laid
"#
);

// The release's tracks as the library holds them: each at its place on the
// release's mediums, on the files it plays from.
sqlive_compose::read!(
    StoredEditorTracks(release: String, edits: String),
    r#"
    SELECT shown.*
    FROM @crate::records::editor_tracks::EditorTrackRows(
        release: :release, edits: :edits,
        mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                         medium.side_count
                  FROM release_mediums AS medium WHERE medium.release_id = :release),
        places: (SELECT held.id, held.release_id, held.medium_id, held.side,
                        held.track_number, held.stated_length_ms, held.position
                 FROM tracks AS held WHERE held.release_id = :release)) AS shown
    WHERE NOT shown.empty_side
    ORDER BY shown.placed_medium, shown.placed_position
"#
);

// The rows of the editor's tracks ([`EditorTrackRows`]) as a release
// editor's form lays them out
// ([`ReleaseLaidPlaces`](super::layout_fragments::ReleaseLaidPlaces)), on the
// mediums the layout starts from
// ([`BaseMediums`](super::layout_fragments::BaseMediums)), which number the
// track table's headings and the choices of a CUE sheet's side.
sqlive_compose::fragment!(
    DraftEditorTrackRows(
        release: String,
        applied: bool,
        mediums: String,
        places: String,
        moves: String,
        edits: String,
    ),
    r#"
    WITH starting AS (
        SELECT base.*
        FROM @crate::records::layout_fragments::BaseMediums(
            release: :release, applied: :applied, mediums: :mediums) AS base),
    laid AS (
        SELECT place.*
        FROM @crate::records::layout_fragments::ReleaseLaidPlaces(
            release: :release, applied: :applied, mediums: :mediums, places: :places,
            moves: :moves, edits: :edits) AS place)
    SELECT shown.*
    FROM @crate::records::editor_tracks::EditorTrackRows(
        release: :release, edits: :edits,
        mediums: (SELECT position, :release, position, carrier, sides FROM starting),
        places: (SELECT track_id, :release, medium, side, number, stated_length_ms, position
                 FROM laid)) AS shown
"#
);

// [`DraftEditorTrackRows`] in release order, for the tests of the layout and
// of the sheet edits.
#[cfg(any(test, feature = "test-utils"))]
sqlive_compose::read!(
    DraftEditorTracks(
        release: String,
        applied: bool,
        mediums: String,
        places: String,
        moves: String,
        edits: String,
    ),
    r#"
    SELECT draft.*
    FROM @crate::records::editor_tracks::DraftEditorTrackRows(
        release: :release, applied: :applied, mediums: :mediums, places: :places,
        moves: :moves, edits: :edits) AS draft
    ORDER BY draft.placed_medium, draft.placed_position
"#
);

/// A row's stored track, as [`EditorTrackRows`] writes it.
#[derive(serde::Deserialize)]
struct TrackColumns {
    id: String,
    release_id: String,
    title: String,
    medium_id: String,
    side: i64,
    track_number: i64,
    duration_ms: i64,
    stated_length_ms: Option<i64>,
    created_at: String,
}

/// The track of `columns`, or why they are no track.
fn track_of_columns(columns: TrackColumns) -> Result<DbTrack, String> {
    let id = columns.id;
    let broken = |column: &str, value: i64| format!("track {id} has {column} {value}");
    Ok(DbTrack {
        side: u8::try_from(columns.side)
            .ok()
            .and_then(std::num::NonZeroU8::new)
            .ok_or_else(|| broken("side", columns.side))?,
        track_number: u32::try_from(columns.track_number)
            .ok()
            .and_then(std::num::NonZeroU32::new)
            .ok_or_else(|| broken("track_number", columns.track_number))?,
        duration_ms: u64::try_from(columns.duration_ms)
            .map_err(|_error| broken("duration_ms", columns.duration_ms))?,
        stated_length_ms: columns
            .stated_length_ms
            .map(|length| {
                u64::try_from(length).map_err(|_error| broken("stated_length_ms", length))
            })
            .transpose()?,
        created_at: bae_util::timestamp::parse_rfc3339(&columns.created_at)
            .map_err(|error| format!("track {id} has an unreadable created_at: {error}"))?,
        release_id: columns.release_id,
        title: columns.title,
        medium_id: columns.medium_id,
        id,
    })
}

/// The first main segment of a row, as [`EditorTrackRows`] writes it.
#[derive(serde::Deserialize)]
struct SegmentJson {
    file_id: String,
    start_ms: u64,
    found: bool,
    name: Option<String>,
    layout: Option<String>,
}

/// The CUE sheet of a row, as [`EditorTrackRows`] writes it.
#[derive(serde::Deserialize)]
struct SheetJson {
    file_id: String,
    name: String,
    place: crate::DbSheetSideColumns,
    side_choices: Vec<crate::DbSheetSideColumns>,
    line_count: u32,
    file_reference: String,
    unbound_lines: Option<Vec<crate::records::sheet_files::DbSheetLine>>,
    audio_options: Vec<DbSheetAudioOption>,
}

/// The document `json` that holds `what` of the row `row_id`.
fn document<T: serde::de::DeserializeOwned>(
    json: &str,
    what: &str,
    row_id: &str,
) -> Result<T, DbError> {
    serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json(format!("the {what} of {row_id}"), json, error))
}

/// One row of the editor's tracks: a track the library holds, the track a
/// save adds for an audio file, with the title `titles` gives it by the
/// file's id, or a side no track sits on.
fn row_of(
    row: EditorTrackRows,
    titles: &BTreeMap<String, String>,
) -> Result<DbReleaseEditRow, DbError> {
    let row_id = row.row_id;
    let unreadable =
        |reason: String| DbError::unreadable(format!("the release editor's row {row_id}"), reason);
    let narrow = |value: i64| {
        u32::try_from(value).map_err(|_error| unreadable(format!("{value} is out of range")))
    };
    let count = |value: i64| {
        u64::try_from(value).map_err(|_error| unreadable(format!("{value} is out of range")))
    };
    let ordinal = narrow(row.side_ordinal)?;
    if row.empty_side {
        return track_side_of(&row.side_kind, ordinal)
            .map(DbReleaseEditRow::EmptySide)
            .map_err(unreadable);
    }
    let position = track_position_of(&row.side_kind, ordinal, narrow(row.place_number)?)
        .map_err(unreadable)?;
    let segment: Option<SegmentJson> =
        row.main_segment.as_deref().map(|json| document(json, "segment", &row_id)).transpose()?;
    if row.is_added {
        let name = segment
            .and_then(|segment| segment.name)
            .ok_or_else(|| unreadable("an added track's file has no name".to_string()))?;
        let duration_ms = row
            .file_duration_ms
            .ok_or_else(|| unreadable("an added track's file has no length".to_string()))?;
        return Ok(DbReleaseEditRow::Added(DbReleaseEditAdded {
            file: super::sheet_files::AddedFile {
                typed_title: titles.get(&row_id).cloned(),
                file_id: row_id.clone(),
                name,
            },
            duration_ms: count(duration_ms)?,
            position,
        }));
    }
    let main_segment = segment
        .map(|segment| {
            let file = match (segment.found, segment.name) {
                (true, Some(name)) => Some(DbReleaseEditSourceFile {
                    name,
                    layout: segment
                        .layout
                        .as_deref()
                        .map(source_audio_layout_of)
                        .transpose()
                        .map_err(unreadable)?,
                }),
                (false, _) => None,
                (true, None) => {
                    return Err(unreadable(format!("release file {} has no name", segment.file_id)))
                }
            };
            Ok(DbReleaseEditSegment { file_id: segment.file_id, start_ms: segment.start_ms, file })
        })
        .transpose()?;
    let sheet = row
        .sheet
        .as_deref()
        .map(|json| {
            let sheet: SheetJson = document(json, "CUE sheet", &row_id)?;
            Ok::<_, DbError>(DbReleaseEditSheet {
                file_id: sheet.file_id,
                name: sheet.name,
                place: sheet.place.decoded().map_err(unreadable)?,
                side_choices: crate::DbSheetSideColumns::decoded_list(sheet.side_choices)
                    .map_err(unreadable)?,
                line_count: sheet.line_count,
                file_reference: sheet.file_reference,
                unbound_lines: sheet.unbound_lines,
                audio_options: sheet.audio_options,
            })
        })
        .transpose()?;
    let track =
        row.track.ok_or_else(|| unreadable("the library holds no such track".to_string()))?;
    let track: TrackColumns = document(&track, "track", &row_id)?;
    Ok(DbReleaseEditRow::Stored(Box::new(DbReleaseEditTrack {
        track: track_of_columns(track).map_err(unreadable)?,
        position,
        listed_length_ms: row.listed_length_ms.map(count).transpose()?,
        audio_format_count: row.audio_format_count,
        main_segment_count: row.main_segment_count,
        main_segment,
        sheet,
    })))
}

/// The tracks of the release `release_id` as the release editor reads their
/// audio, in release order, each on the mediums and files the library holds
/// it on.
pub fn release_edit_tracks_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Vec<DbReleaseEditTrack>, DbError> {
    let params = StoredEditorTracksParams {
        release: release_id.to_string(),
        edits: super::sheet_files::NO_SHEET_EDITS.to_string(),
    };
    statement_rows(sql, &params)?
        .into_iter()
        .map(|row| match row_of(row.shown, &BTreeMap::new())? {
            DbReleaseEditRow::Stored(track) => Ok(*track),
            other => Err(DbError::unreadable(
                format!("a stored track of release {release_id}"),
                format!("{other:?}"),
            )),
        })
        .collect()
}

/// One row of a form's draft tracks ([`DraftEditorTrackRows`]): a track the
/// library holds, the track a save adds for an audio file, with the title
/// `titles` gives it by the file's id, or a side no track sits on.
pub fn draft_release_edit_row_of(
    row: DraftEditorTrackRows,
    titles: &BTreeMap<String, String>,
) -> Result<DbReleaseEditRow, DbError> {
    row_of(row.shown, titles)
}

/// The rows of the release editor's tracks as `layout` lays them out, in
/// release order.
#[cfg(any(test, feature = "test-utils"))]
pub fn draft_release_edit_tracks_on(
    sql: &impl coven::SqlRead,
    layout: &super::layout::LibraryLayout,
    titles: &BTreeMap<String, String>,
) -> Result<Vec<DbReleaseEditRow>, DbError> {
    let params = DraftEditorTracksParams {
        release: layout.release_id.clone(),
        applied: layout.applied,
        mediums: layout.mediums.clone(),
        places: layout.places.clone(),
        moves: layout.moves.clone(),
        edits: layout.edits.clone(),
    };
    statement_rows(sql, &params)?
        .into_iter()
        .map(|row| draft_release_edit_row_of(row.draft, titles))
        .collect()
}
