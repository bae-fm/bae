//! The pane's mapping table, read in one statement.
//!
//! The scan stores the folder's audio layout with its files (the units in play
//! order, each one's length and preview range, and what each track sheet's
//! `FILE` lines can be bound to), and the draft stores each track's place. The
//! statement joins each draft track to its audio unit and returns a row per
//! draft track, the rows grouped into sides and runs, each sheet's header, the
//! files that are not audio, the gallery of images, and the count of rows
//! nobody has named. The code here only turns that document into the
//! mapping table's types and refuses a draft whose tracks are not the folder's
//! units.

use super::*;
use crate::folder_scanner::{SheetBindingOffer, SheetDisc, SheetReferenceOptions};
use crate::mapping::{
    MappingContainer, MappingEntry, MappingFile, MappingFileRow, MappingImage, MappingRole,
    MappingSource, MappingTable, MappingTrackRun, MappingTrackSide, SheetBound, SheetBoundAudio,
    SheetGroup, TrackMapping,
};
use crate::ImportError;

/// The mapping table as one JSON document, for the release the scan stores
/// at the key the statement binds as `:key`, over the draft stored under its
/// content hash. Each row carries its draft track whole (`track_json_sql`)
/// and how the track's own artists stand to the library
/// (`artist_standing_clauses`), so the rows need no join to another read's
/// draft. `key` is the key the statement binds. The disc numbers a sheet can
/// take, 1 up to the count of resolved sheets, number those sheets: a
/// recursive CTE that counted up to them would make coven rerun the
/// statement for a write to any row of the tables it reads.
fn mapping_sql(key: &crate::CandidateKey) -> String {
    format!(
        "
WITH
{standing_clauses},
candidate AS (
    SELECT watched_folder_path, path, content_hash FROM scan_candidate WHERE path = :key LIMIT 1
),
f AS (
    SELECT * FROM scan_candidate_file WHERE watched_folder_path = (SELECT watched_folder_path FROM candidate) AND candidate_path = :key
),
audio_json (relative_path, json) AS (
    SELECT relative_path,
           json_object('file_id', relative_path, 'name', file_name, 'size', size,
                       'path', hex(absolute_path))
    FROM f WHERE role = 'audio'
),
mediums AS (
    SELECT m.position,
           bae_medium_is_sided(m.medium) AS sided,
           COALESCE(SUM(CASE WHEN bae_medium_is_sided(m.medium) THEN m.side_count END)
                    OVER (ORDER BY m.position
                          ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0) AS sides_before,
           COUNT(*) OVER () AS medium_count
    FROM import_candidate_medium AS m WHERE m.content_hash = (SELECT content_hash FROM candidate)
),
rows AS (
    SELECT t.position, t.title, t.track_id,
           (SELECT {track} FROM import_candidate_track AS track
            WHERE track.content_hash = t.content_hash AND track.track_id = t.track_id)
               AS track_json,
           CASE WHEN m.sided THEN 'sided' WHEN m.medium_count > 1 THEN 'disc' ELSE 'flat' END
               AS side_kind,
           CASE WHEN m.sided THEN bae_side_letters(m.sides_before + t.side)
                WHEN m.medium_count > 1 THEN CAST(t.medium AS TEXT) END AS side_value,
           CASE WHEN m.sided THEN bae_side_letters(m.sides_before + t.side) || t.track_number
                ELSE CAST(t.track_number AS TEXT) END AS position_text,
           u.position IS NOT NULL
               AND u.file_kind IS t.file_kind AND u.file_id IS t.file_id
               AND u.sheet_id IS t.sheet_id AND u.slice_index IS t.slice_index AS unit_matches,
           u.file_kind, u.sheet_id, u.slice_index, u.duration_ms AS unit_ms,
           u.start_sample, u.end_sample, a.json AS audio,
           ct.number AS cue_number, ct.start_cue_frames * 1000 / 75 AS cue_start_ms,
           CASE WHEN t.stated_length_ms IS NOT NULL AND u.duration_ms IS NOT NULL
                     AND bae_lengths_disagree(u.duration_ms, t.stated_length_ms)
                THEN t.stated_length_ms END AS listed_ms
    FROM import_candidate_track AS t
    JOIN mediums AS m ON m.position = t.medium
    LEFT JOIN scan_candidate_unit AS u
        ON u.watched_folder_path = (SELECT watched_folder_path FROM candidate) AND u.candidate_path = :key AND u.position = t.position
    LEFT JOIN audio_json AS a ON a.relative_path = u.file_id
    LEFT JOIN scan_cue_track AS ct
        ON ct.watched_folder_path = (SELECT watched_folder_path FROM candidate) AND ct.candidate_path = :key
       AND ct.sheet_relative_path = u.sheet_id AND ct.position = u.cue_track_position
    WHERE t.content_hash = (SELECT content_hash FROM candidate)
),
marked AS (
    SELECT *,
           side_kind IS NOT LAG(side_kind) OVER w OR side_value IS NOT LAG(side_value) OVER w
               AS side_start,
           side_kind IS NOT LAG(side_kind) OVER w OR side_value IS NOT LAG(side_value) OVER w
               OR sheet_id IS NOT LAG(sheet_id) OVER w AS run_start
    FROM rows WINDOW w AS (ORDER BY position)
),
numbered AS (
    SELECT *, SUM(side_start) OVER (ORDER BY position) AS side_no,
              SUM(run_start) OVER (ORDER BY position) AS run_no
    FROM marked
),
refs (sheet, file_reference, seq) AS (
    SELECT sheet, file_reference, MIN(seq) FROM (
        SELECT sheet, file_reference,
               ROW_NUMBER() OVER (PARTITION BY sheet ORDER BY track_position, part, index_position)
                   AS seq
        FROM (
            SELECT ct.sheet_relative_path AS sheet, ci.file_reference, ct.position AS track_position,
                   0 AS part, ci.position AS index_position
            FROM scan_cue_track AS ct
            JOIN scan_cue_index AS ci
                ON (ci.watched_folder_path, ci.candidate_path, ci.sheet_relative_path, ci.track_position)
                 = (ct.watched_folder_path, ct.candidate_path, ct.sheet_relative_path, ct.position)
            WHERE ct.watched_folder_path = (SELECT watched_folder_path FROM candidate) AND ct.candidate_path = :key AND ct.mode = 'audio'
            UNION ALL
            SELECT ct.sheet_relative_path, ct.file_reference, ct.position, 1, 0
            FROM scan_cue_track AS ct
            WHERE ct.watched_folder_path = (SELECT watched_folder_path FROM candidate) AND ct.candidate_path = :key AND ct.mode = 'audio'
        )
    ) GROUP BY sheet, file_reference
),
disc_numbers (number) AS (
    SELECT 1
    UNION
    SELECT ROW_NUMBER() OVER (ORDER BY position) FROM f
    WHERE role = 'track_sheet' AND sheet_binding = 'resolved'
),
disc_options AS (
    SELECT json_group_array(number ORDER BY number) AS json FROM (
        SELECT number FROM disc_numbers
        UNION
        SELECT sheet_disc_number FROM f WHERE sheet_disc = 'disc'
    )
),
reference_json (sheet, seq, bound, json) AS (
    SELECT r.sheet, r.seq, sa.audio_relative_path IS NOT NULL,
           json_object(
               'file_reference', r.file_reference,
               'file_id', sa.audio_relative_path,
               'options', json((
                   SELECT json_group_array(json_object(
                       'file_id', o.audio_relative_path, 'offer', o.offer,
                       'codec', o.codec) ORDER BY fo.position)
                   FROM scan_candidate_sheet_offer AS o
                   JOIN f AS fo ON fo.relative_path = o.audio_relative_path
                   WHERE o.watched_folder_path = (SELECT watched_folder_path FROM candidate) AND o.candidate_path = :key
                     AND o.sheet_relative_path = r.sheet
                     AND o.file_reference = r.file_reference)))
    FROM refs AS r
    LEFT JOIN scan_sheet_audio_file AS sa
        ON sa.watched_folder_path = (SELECT watched_folder_path FROM candidate) AND sa.candidate_path = :key
       AND sa.sheet_relative_path = r.sheet AND sa.file_reference = r.file_reference
),
sheet_audio_counts (sheet, count) AS (
    SELECT sheet_relative_path, COUNT(*) FROM scan_sheet_audio_file
    WHERE watched_folder_path = (SELECT watched_folder_path FROM candidate) AND candidate_path = :key GROUP BY sheet_relative_path
),
sheets (relative_path, json) AS (
    SELECT s.relative_path,
           json_object(
               'sheet_id', s.relative_path, 'name', s.relative_path, 'size', s.size,
               'bound', CASE s.sheet_binding
                   WHEN 'resolved' THEN CASE COALESCE(n.count, 0)
                       WHEN 0 THEN json_object('binding', 'resolved_to_none')
                       WHEN 1 THEN json_object('binding', 'describes', 'audio', json((
                           SELECT a.json FROM scan_sheet_audio_file AS sa
                           JOIN audio_json AS a ON a.relative_path = sa.audio_relative_path
                           WHERE sa.watched_folder_path = (SELECT watched_folder_path FROM candidate) AND sa.candidate_path = :key
                             AND sa.sheet_relative_path = s.relative_path)))
                       ELSE json_object('binding', 'describes_files', 'audio_count', n.count) END
                   WHEN 'unresolved' THEN json_object('binding', 'unresolved', 'requested', json((
                       SELECT json_group_array(r.file_reference ORDER BY r.seq)
                       FROM refs AS r
                       WHERE r.sheet = s.relative_path
                         AND NOT EXISTS (SELECT 1 FROM scan_sheet_audio_file AS sa
                                         WHERE sa.watched_folder_path = (SELECT watched_folder_path FROM candidate)
                                           AND sa.candidate_path = :key
                                           AND sa.sheet_relative_path = s.relative_path
                                           AND sa.file_reference = r.file_reference))))
                   WHEN 'refused_codec' THEN json_object('binding', 'refused_codec', 'codec', s.sheet_binding_codec)
                   ELSE json_object('binding', s.sheet_binding) END,
               'assignment', s.sheet_disc, 'disc_number', s.sheet_disc_number,
               'bound_audio', json((
                   SELECT CASE COUNT(*)
                       WHEN 0 THEN json_object('kind', 'none')
                       WHEN 1 THEN json_object('kind', 'one', 'reference', json(MIN(r.json)))
                       ELSE json_object('kind', 'several',
                                        'references', json_group_array(json(r.json) ORDER BY r.seq))
                   END
                   FROM reference_json AS r
                   WHERE r.sheet = s.relative_path AND r.bound)),
               'unbound_references', CASE WHEN EXISTS (
                       SELECT 1 FROM reference_json AS r
                       WHERE r.sheet = s.relative_path AND NOT r.bound)
                   THEN json((SELECT json_group_array(json(r.json) ORDER BY r.seq)
                              FROM reference_json AS r
                              WHERE r.sheet = s.relative_path AND NOT r.bound))
                   END,
               'disc_options', CASE WHEN s.sheet_binding = 'resolved'
                                    THEN json((SELECT json FROM disc_options))
                                    ELSE json('[]') END)
    FROM f AS s LEFT JOIN sheet_audio_counts AS n ON n.sheet = s.relative_path
    WHERE s.role = 'track_sheet'
),
runs AS (
    SELECT side_no, run_no, MIN(side_kind) AS side_kind, MIN(side_value) AS side_value,
           json_object(
               'sheet', json((SELECT json FROM sheets WHERE relative_path = n.sheet_id)),
               'rows', json_group_array(json_object(
                   'track', json(track_json), 'matches', unit_matches,
                   'artist_standing', json({track_standing}),
                   'position', position_text,
                   'listed_ms', listed_ms,
                   'unit', CASE WHEN file_kind IS NOT NULL THEN json_object(
                       'audio', json(audio), 'length_ms', unit_ms,
                       'source', CASE file_kind
                           WHEN 'sheet_slice' THEN json_object(
                               'kind', file_kind, 'sheet_id', sheet_id, 'index', slice_index,
                               'number', cue_number, 'start_ms', cue_start_ms,
                               'start', start_sample, 'end', end_sample)
                           ELSE json_object('kind', file_kind) END) END)
                   ORDER BY position)) AS json
    FROM numbered AS n GROUP BY side_no, run_no
),
sides AS (
    SELECT json_group_array(json_object('kind', side_kind, 'value', side_value,
                                        'runs', json(runs_json)) ORDER BY side_no) AS json
    FROM (SELECT side_no, MIN(side_kind) AS side_kind, MIN(side_value) AS side_value,
                 json_group_array(json(json) ORDER BY run_no) AS runs_json
          FROM runs GROUP BY side_no)
)
SELECT json_object(
    'images', json((SELECT json_group_array(json_object(
                        'file_id', relative_path, 'name', file_name, 'path', hex(absolute_path),
                        'size', size, 'modified_at_ns', modified_at_ns) ORDER BY position)
                    FROM f WHERE role = 'artwork')),
    'sides', json((SELECT json FROM sides)),
    'files', json((SELECT json_group_array(
                       CASE f.role
                           WHEN 'track_sheet' THEN json_object('sheet', json(s.json))
                           ELSE json_object('file', json_object(
                               'file_id', f.relative_path, 'name', f.relative_path,
                               'size', f.size, 'role', f.role))
                       END ORDER BY f.position)
                   FROM f LEFT JOIN sheets AS s ON s.relative_path = f.relative_path
                   WHERE f.role <> 'audio'
                     AND NOT (f.role = 'track_sheet'
                              AND EXISTS (SELECT 1 FROM scan_candidate_unit AS u
                                          WHERE u.watched_folder_path = (SELECT watched_folder_path FROM candidate)
                                            AND u.candidate_path = :key
                                            AND u.sheet_id = f.relative_path)))),
    'untitled', (SELECT COUNT(*) FILTER (WHERE bae_trim(title) = '') FROM rows),
    'tracks', (SELECT COUNT(*) FROM rows),
    'units', (SELECT COUNT(*) FROM scan_candidate_unit
              WHERE watched_folder_path = (SELECT watched_folder_path FROM candidate) AND candidate_path = :key))",
        track = crate::tables::import_state::track_json_sql(),
        standing_clauses = super::artist_standing_clauses(super::CreditSource::Candidate(
            crate::tables::import_state::OneCandidate::Scanned(key)
        )),
        track_standing = super::track_artist_standing_sql("n.track_id"),
    )
}

/// A SQL truth value, which JSON holds as 0 or 1.
fn flag<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(<i64 as serde::Deserialize>::deserialize(deserializer)? != 0)
}

#[derive(serde::Deserialize)]
struct AudioJson {
    file_id: String,
    name: String,
    size: u64,
    #[serde(deserialize_with = "path_column::from_hex")]
    path: std::path::PathBuf,
}

#[derive(serde::Deserialize)]
struct RowJson {
    /// The draft track the row maps.
    track: crate::tables::import_state::StoredTrack,
    /// How the track's own artists stand to the library, as the standing
    /// clauses wrote it; `None` when it has none of its own.
    artist_standing: Option<super::FieldStandingJson>,
    #[serde(deserialize_with = "flag")]
    matches: bool,
    position: String,
    /// The length the track's source lists. The statement sets it only when
    /// `bae_lengths_disagree` finds that it differs too much from the audio's
    /// own length.
    listed_ms: Option<u64>,
    /// The folder's audio unit at the track's position; `None` when the
    /// folder has fewer units than the draft has tracks.
    unit: Option<UnitJson>,
}

/// One audio unit of the folder: the audio file it plays from, the length
/// it measures, and what part of the file it is.
#[derive(serde::Deserialize)]
struct UnitJson {
    audio: Option<AudioJson>,
    length_ms: u64,
    source: UnitSourceJson,
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum UnitSourceJson {
    /// The whole file.
    Standalone,
    /// One entry a track sheet carves out of the file.
    SheetSlice {
        sheet_id: String,
        index: u32,
        number: u32,
        /// Where the entry starts in its audio, in milliseconds, from the
        /// sheet's INDEX 01.
        start_ms: u64,
        /// The sample the entry starts at in its audio.
        start: u64,
        end: Option<u64>,
    },
}

#[derive(serde::Deserialize)]
struct ReferenceJson {
    file_reference: String,
    file_id: Option<String>,
    options: Vec<OptionJson>,
}

/// A sheet's FILE lines that play audio, tagged by how many there are.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BoundAudioJson {
    None,
    One { reference: ReferenceJson },
    Several { references: Vec<ReferenceJson> },
}

#[derive(serde::Deserialize)]
struct OptionJson {
    file_id: String,
    offer: String,
    codec: Option<String>,
}

#[derive(serde::Deserialize)]
struct SheetJson {
    sheet_id: String,
    name: String,
    size: u64,
    bound: BoundJson,
    assignment: String,
    disc_number: Option<i64>,
    bound_audio: BoundAudioJson,
    unbound_references: Option<Vec<ReferenceJson>>,
    disc_options: Vec<u32>,
}

/// What a track sheet is bound to, tagged by the binding the statement
/// reads from the scan and from how many audio files the sheet is bound to.
#[derive(serde::Deserialize)]
#[serde(tag = "binding", rename_all = "snake_case")]
enum BoundJson {
    /// Resolved, and bound to no audio file, which [`sheet_group`] refuses.
    ResolvedToNone,
    /// Resolved, and bound to this one audio file.
    Describes {
        audio: AudioJson,
    },
    /// Resolved, and bound to this many audio files.
    DescribesFiles {
        audio_count: u32,
    },
    /// Bound to none, with the files its `FILE` lines ask for and the
    /// folder lacks.
    Unresolved {
        requested: Vec<String>,
    },
    RefusedTiming,
    RefusedCodec {
        codec: String,
    },
}

#[derive(serde::Deserialize)]
struct RunJson {
    sheet: Option<SheetJson>,
    rows: Vec<RowJson>,
}

#[derive(serde::Deserialize)]
struct SideJson {
    kind: String,
    value: Option<String>,
    runs: Vec<RunJson>,
}

#[derive(serde::Deserialize)]
struct ImageJson {
    file_id: String,
    name: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    path: std::path::PathBuf,
    size: u64,
    modified_at_ns: i64,
}

#[derive(serde::Deserialize)]
struct CarriedFileJson {
    file_id: String,
    name: String,
    size: u64,
    role: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum CarriedJson {
    File(CarriedFileJson),
    Sheet(SheetJson),
}

/// The statement's document, decoded: the statement's rows, before `table`
/// joins the draft's tracks to them.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MappingRows {
    document: String,
}

#[derive(serde::Deserialize)]
struct Document {
    images: Vec<ImageJson>,
    sides: Vec<SideJson>,
    files: Vec<CarriedJson>,
    untitled: u32,
    tracks: usize,
    units: usize,
}

/// Read the mapping table of the release the scan stores at `key`.
pub(super) fn load_mapping_rows_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
) -> Result<MappingRows, DbError> {
    let document: String =
        sql.query_row(&mapping_sql(key), named_params! { ":key": key }, |row| row.get(0))?;
    Ok(MappingRows { document })
}

impl MappingRows {
    /// The mapping table over the draft's tracks. Refuses a draft whose
    /// tracks are not the folder's audio units in order: every write that
    /// changes the folder's files rebuilds the draft, so a mismatch is a
    /// stored state that contradicts itself.
    pub(super) fn table(&self) -> Result<MappingTable, ImportError> {
        let internal = |detail: String| ImportError::Internal { detail };
        let document: Document = serde_json::from_str(&self.document)
            .map_err(|error| internal(format!("the mapping statement's document: {error}")))?;
        if document.tracks != document.units {
            return Err(internal(format!(
                "the draft holds {} tracks for the folder's {} audio units",
                document.tracks, document.units
            )));
        }
        let mut track_sides = Vec::new();
        for side in document.sides {
            let mut runs = Vec::new();
            for run in side.runs {
                let mut mappings = Vec::new();
                let sheet = run.sheet.map(sheet_group).transpose()?;
                for row in run.rows {
                    mappings.push(track_mapping(row)?);
                }
                runs.push(match sheet {
                    Some(sheet) => MappingTrackRun::Sheet { sheet, entries: mappings },
                    None => MappingTrackRun::Tracks(mappings),
                });
            }
            track_sides.push(MappingTrackSide { side: track_side(&side.kind, side.value)?, runs });
        }
        let files = document
            .files
            .into_iter()
            .map(|carried| {
                Ok(match carried {
                    CarriedJson::Sheet(sheet) => MappingFileRow::Sheet(sheet_group(sheet)?),
                    CarriedJson::File(file) => MappingFileRow::File(MappingFile {
                        file_id: file.file_id,
                        name: file.name,
                        size: file.size,
                        preview_target: None,
                        role: carried_role(&file.role)?,
                    }),
                })
            })
            .collect::<Result<Vec<_>, ImportError>>()?;
        Ok(MappingTable {
            images: document
                .images
                .into_iter()
                .map(|image| MappingImage {
                    file_id: image.file_id,
                    name: image.name,
                    file: crate::folder_scanner::FileVersion {
                        path: image.path,
                        size: image.size,
                        modified_at_ns: image.modified_at_ns,
                    },
                })
                .collect(),
            track_sides,
            files,
            untitled_track_count: document.untitled,
        })
    }
}

fn track_side(
    kind: &str,
    value: Option<String>,
) -> Result<bae_types::mediums::TrackSide, ImportError> {
    let bad = |detail: String| ImportError::Internal { detail };
    match (kind, value) {
        ("sided", Some(side_letter)) => Ok(bae_types::mediums::TrackSide::Sided { side_letter }),
        ("disc", Some(disc)) => Ok(bae_types::mediums::TrackSide::Disc {
            disc: disc.parse().map_err(|error| bad(format!("a disc number {disc}: {error}")))?,
        }),
        ("flat", None) => Ok(bae_types::mediums::TrackSide::Flat),
        (kind, value) => {
            Err(bad(format!("the statement returned an unknown track side {kind} {value:?}")))
        }
    }
}

fn carried_role(role: &str) -> Result<MappingRole, ImportError> {
    match role {
        "artwork" => Ok(MappingRole::Image),
        "document" => Ok(MappingRole::Document),
        "damaged_image" => Ok(MappingRole::DamagedImage),
        "other" => Ok(MappingRole::Other),
        other => Err(ImportError::Internal {
            detail: format!(
                "the statement returned an unknown role {other} for a file that is not audio"
            ),
        }),
    }
}

fn container(audio: &AudioJson) -> MappingContainer {
    MappingContainer { file_id: audio.file_id.clone(), name: audio.name.clone(), size: audio.size }
}

fn sheet_group(sheet: SheetJson) -> Result<SheetGroup, ImportError> {
    let internal = |detail: String| ImportError::Internal { detail };
    let bound = match sheet.bound {
        BoundJson::Describes { audio } => SheetBound::Describes(container(&audio)),
        BoundJson::DescribesFiles { audio_count } => {
            SheetBound::DescribesFiles { audio_file_count: audio_count }
        }
        BoundJson::ResolvedToNone => {
            return Err(internal(format!("{} is bound to no audio", sheet.sheet_id)))
        }
        BoundJson::Unresolved { requested } => SheetBound::Unresolved { requested },
        BoundJson::RefusedTiming => SheetBound::RefusedTiming,
        BoundJson::RefusedCodec { codec } => SheetBound::RefusedCodec { codec },
    };
    let assignment = SheetDisc::from_columns(
        &format!("the disc of track sheet {}", sheet.sheet_id),
        &sheet.assignment,
        sheet.disc_number,
    )?;
    let bound_audio = match sheet.bound_audio {
        BoundAudioJson::None => SheetBoundAudio::None,
        BoundAudioJson::One { reference } => SheetBoundAudio::One(reference_options(reference)?),
        BoundAudioJson::Several { references } => SheetBoundAudio::Several(
            references.into_iter().map(reference_options).collect::<Result<_, _>>()?,
        ),
    };
    let unbound_references = sheet
        .unbound_references
        .map(|references| references.into_iter().map(reference_options).collect())
        .transpose()?;
    Ok(SheetGroup {
        sheet_id: sheet.sheet_id,
        name: sheet.name,
        size: sheet.size,
        bound,
        bound_audio,
        unbound_references,
        assignment,
        disc_options: sheet.disc_options,
    })
}

/// One FILE line of a sheet, with the audio it plays and the choices the
/// caption offers for it.
fn reference_options(reference: ReferenceJson) -> Result<SheetReferenceOptions, ImportError> {
    Ok(SheetReferenceOptions {
        file_reference: reference.file_reference,
        file_id: reference.file_id,
        options: reference
            .options
            .into_iter()
            .map(|option| {
                Ok(crate::folder_scanner::SheetBindingOption {
                    file_id: option.file_id,
                    offer: match (option.offer.as_str(), option.codec) {
                        ("offered", None) => SheetBindingOffer::Offered,
                        ("refused_timing", None) => SheetBindingOffer::RefusedTiming,
                        ("refused_codec", Some(codec)) => SheetBindingOffer::RefusedCodec { codec },
                        ("refused_unreadable", None) => SheetBindingOffer::RefusedUnreadable,
                        (other, _) => {
                            return Err(ImportError::Internal {
                                detail: format!(
                                    "the statement returned an unknown sheet offer {other}"
                                ),
                            })
                        }
                    },
                })
            })
            .collect::<Result<Vec<_>, ImportError>>()?,
    })
}

fn track_mapping(row: RowJson) -> Result<TrackMapping, ImportError> {
    let internal = |detail: String| ImportError::Internal { detail };
    let track = row.track.track()?;
    if !row.matches {
        return Err(internal(format!(
            "draft track {} plays {:?}, which is not the folder's audio unit there",
            track.edit.id, track.edit.file
        )));
    }
    let unit = row.unit.ok_or_else(|| internal("a draft track has no audio unit".to_owned()))?;
    let audio = unit.audio.ok_or_else(|| internal("an audio unit names no file".to_owned()))?;
    let source = match unit.source {
        UnitSourceJson::Standalone => MappingSource::File(MappingFile {
            name: audio.file_id.clone(),
            file_id: audio.file_id,
            size: audio.size,
            preview_target: Some(bae_track_audio::PreviewTarget::whole_file(audio.path)),
            role: MappingRole::Audio,
        }),
        UnitSourceJson::SheetSlice { sheet_id, index, number, start_ms, start, end } => {
            MappingSource::SheetEntry(MappingEntry {
                sheet_id,
                index,
                number,
                start_ms,
                container_id: audio.file_id,
                preview_target: bae_track_audio::PreviewTarget::sample_range(
                    audio.path, start, end,
                ),
            })
        }
    };
    let artist_standing =
        row.artist_standing
            .map(super::FieldStandingJson::standing)
            .transpose()
            .map_err(|stated| internal(format!("a track's artist standing: {stated}")))?;
    Ok(TrackMapping {
        source,
        track: track.edit.as_edit(),
        position: row.position,
        length_ms: unit.length_ms,
        listed_length_ms: row.listed_ms,
        artist_standing,
    })
}

#[cfg(test)]
impl crate::tables::ImportTables {
    /// The mapping table of the release the scan stores at `key`.
    pub(crate) async fn load_mapping_table_for_test(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<MappingTable, ImportError> {
        let key = key.clone();
        self.store
            .read(move |sql| load_mapping_rows_on(&sql, &key))
            .await
            .expect("the rows are read")
            .table()
    }
}
