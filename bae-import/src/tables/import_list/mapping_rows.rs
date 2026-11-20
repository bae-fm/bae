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
use crate::folder_scanner::{SheetBindingOffer, SheetReferenceOptions};
use crate::mapping::{
    MappingContainer, MappingEntry, MappingFile, MappingFileRow, MappingImage, MappingRole,
    MappingSource, MappingTable, MappingTrackRun, MappingTrackSide, SheetBound, SheetBoundAudio,
    SheetGroup, TrackMapping,
};
use crate::ImportError;

/// The mapping table as one JSON document, for the release the scan stores
/// at the key the statement binds as `:key`, over the draft stored under its
/// content hash. Each row carries its draft track whole (`track_json_sql`)
/// and the track's own artists, each with how it stands to the library
/// (`artist_field_clauses`), so the rows need no join to another read's
/// draft. `key` is the key the statement binds. The disc numbers a sheet can
/// take, 1 up to the count of resolved sheets, number those sheets: a
/// recursive CTE that counted up to them would make coven rerun the
/// statement for a write to any row of the tables it reads.
/// The mediums a sheet of the folder `:key` can be put on, with the columns
/// of `release_mediums`, for the statement of [`mapping_sql`]: the draft's
/// mediums, a disc of one side at each position up to the count of the
/// folder's bound sheets, so that each sheet can go on a disc of its own,
/// and the disc each sheet is on, each with the sides the draft gives it or
/// the side a sheet is on, whichever is more.
const PANE_MEDIUMS: &str = "(SELECT reach.position AS id, :key AS release_id, reach.position, \
            m.medium, \
            MAX(COALESCE(m.side_count, 1), \
                COALESCE((SELECT MAX(sheet_side) FROM f \
                          WHERE sheet_place = 'side' AND sheet_medium = reach.position), 1)) \
                AS side_count \
     FROM (SELECT 1 AS position \
           UNION SELECT position FROM import_candidate_medium \
                 WHERE content_hash = (SELECT content_hash FROM candidate) \
           UNION SELECT ROW_NUMBER() OVER (ORDER BY position) FROM f \
                 WHERE role = 'track_sheet' AND sheet_binding = 'resolved' \
           UNION SELECT sheet_medium FROM f WHERE sheet_place = 'side') AS reach \
     LEFT JOIN import_candidate_medium AS m \
         ON m.content_hash = (SELECT content_hash FROM candidate) \
        AND m.position = reach.position)";

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
{places},
rows AS (
    SELECT t.position, t.title, t.track_id,
           (SELECT {track} FROM import_candidate_track AS track
            WHERE track.content_hash = t.content_hash AND track.track_id = t.track_id)
               AS track_json,
           p.side_kind, p.side_ordinal, p.number,
           u.position IS NOT NULL
               AND u.file_kind IS t.file_kind AND u.file_id IS t.file_id
               AND u.sheet_id IS t.sheet_id AND u.slice_index IS t.slice_index AS unit_matches,
           u.file_kind, u.sheet_id, u.file_id AS unit_file_id, u.slice_index,
           u.duration_ms AS unit_ms,
           u.start_sample, u.end_sample, a.json AS audio,
           ct.number AS cue_number, ct.start_cue_frames * 1000 / 75 AS cue_start_ms,
           CASE WHEN t.stated_length_ms IS NOT NULL AND u.duration_ms IS NOT NULL
                     AND bae_lengths_disagree(u.duration_ms, t.stated_length_ms)
                THEN t.stated_length_ms END AS listed_ms
    FROM import_candidate_track AS t
    JOIN place AS p ON p.id = t.track_id AND p.release_id = t.content_hash
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
           side_kind IS NOT LAG(side_kind) OVER w OR side_ordinal IS NOT LAG(side_ordinal) OVER w
               AS side_start,
           side_kind IS NOT LAG(side_kind) OVER w OR side_ordinal IS NOT LAG(side_ordinal) OVER w
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
side_choice_rows AS (SELECT * FROM {side_choices}),
side_choices AS (
    SELECT json_group_array(json_object('medium', c.medium, 'side', c.side, 'kind', c.kind,
                                        'ordinal', c.ordinal) ORDER BY c.medium, c.side) AS json
    FROM side_choice_rows AS c
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
               'place', s.sheet_place,
               -- The choice of the side the sheet is on; every side of a
               -- disc that is not a record or cassette is the disc, which
               -- its first side names.
               'assigned', CASE WHEN s.sheet_place = 'side' THEN json((
                   SELECT json_object('medium', c.medium, 'side', c.side, 'kind', c.kind,
                                      'ordinal', c.ordinal)
                   FROM side_choice_rows AS c
                   WHERE c.medium = s.sheet_medium
                     AND c.side = CASE WHEN c.kind = 'sided' THEN s.sheet_side ELSE 1 END)) END,
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
               'side_choices', CASE WHEN s.sheet_binding = 'resolved'
                                    THEN json((SELECT json FROM side_choices))
                                    ELSE json('[]') END)
    FROM f AS s LEFT JOIN sheet_audio_counts AS n ON n.sheet = s.relative_path
    WHERE s.role = 'track_sheet'
),
runs AS (
    SELECT side_no, run_no, MIN(side_kind) AS side_kind, MIN(side_ordinal) AS side_ordinal,
           json_object(
               'sheet', json((SELECT json FROM sheets WHERE relative_path = n.sheet_id)),
               'rows', json_group_array(json_object(
                   'track', json(track_json), 'matches', unit_matches,
                   'artists', json({track_artists}),
                   'number', number,
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
    SELECT json_group_array(json_object('kind', side_kind, 'ordinal', side_ordinal,
                                        'runs', json(runs_json)) ORDER BY side_no) AS json
    FROM (SELECT side_no, MIN(side_kind) AS side_kind, MIN(side_ordinal) AS side_ordinal,
                 json_group_array(json(json) ORDER BY run_no) AS runs_json
          FROM runs GROUP BY side_no)
),
{unplayed}
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
                   WHERE f.relative_path IN (SELECT file_id FROM unplayed_files))),
    'untitled', (SELECT COUNT(*) FILTER (WHERE bae_trim(title) = '') FROM rows),
    'tracks', (SELECT COUNT(*) FROM rows),
    'units', (SELECT COUNT(*) FROM scan_candidate_unit
              WHERE watched_folder_path = (SELECT watched_folder_path FROM candidate) AND candidate_path = :key))",
        track = crate::tables::import_state::track_json_sql(),
        standing_clauses = super::artist_field_clauses(super::CreditSource::Candidate(
            crate::tables::import_state::OneCandidate::Scanned(key)
        )),
        track_artists = super::track_artist_field_sql("n.track_id"),
        side_choices = bae_records::records::sheet_side_choices_sql(PANE_MEDIUMS),
        unplayed = bae_records::records::files::unplayed_files_cte(
            "(SELECT relative_path AS file_id FROM f)",
            "(SELECT unit_file_id AS audio_file_id, sheet_id AS sheet_file_id FROM rows)",
        ),
        places = bae_records::records::track_places_ctes(
            bae_records::records::MediumsSource::Draft {
                mediums: "(SELECT position AS id, content_hash AS release_id, position, medium, \
                                  side_count FROM import_candidate_medium \
                           WHERE content_hash = (SELECT content_hash FROM candidate))",
                places: "(SELECT track_id AS id, content_hash AS release_id, medium AS medium_id, \
                                 side, track_number, stated_length_ms, position \
                          FROM import_candidate_track \
                          WHERE content_hash = (SELECT content_hash FROM candidate))",
            },
            "m.release_id = (SELECT content_hash FROM candidate)",
        ),
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
    /// The track's own artists, each with how it stands to the library, as
    /// the standing clauses wrote them; `None` when it has none of its own.
    artists: Option<super::ArtistFieldJson>,
    #[serde(deserialize_with = "flag")]
    matches: bool,
    /// The track's number on its side.
    number: u32,
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
    place: String,
    assigned: Option<SideChoiceJson>,
    bound_audio: BoundAudioJson,
    unbound_references: Option<Vec<ReferenceJson>>,
    side_choices: Vec<SideChoiceJson>,
}

/// One side or disc a sheet can go on, as `sheet_side_choices_sql` names it.
#[derive(serde::Deserialize)]
struct SideChoiceJson {
    medium: u32,
    side: u32,
    kind: String,
    ordinal: u32,
}

impl SideChoiceJson {
    fn read(self) -> Result<bae_records::DbSheetSideChoice, ImportError> {
        bae_records::DbSheetSideChoice::of_columns(self.medium, self.side, &self.kind, self.ordinal)
            .map_err(|detail| ImportError::Internal { detail })
    }
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
    /// The side's kind and ordinal, as `track_places_ctes` names them.
    kind: String,
    ordinal: u32,
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
            let unplaced = |reason: String| internal(format!("a track side: {reason}"));
            let mut runs = Vec::new();
            for run in side.runs {
                let mut mappings = Vec::new();
                let sheet = run.sheet.map(sheet_group).transpose()?;
                for row in run.rows {
                    let position = bae_records::records::track_position_of(
                        &side.kind,
                        side.ordinal,
                        row.number,
                    )
                    .map_err(unplaced)?;
                    mappings.push(track_mapping(row, position.text())?);
                }
                runs.push(match sheet {
                    Some(sheet) => MappingTrackRun::Sheet { sheet, entries: mappings },
                    None => MappingTrackRun::Tracks(mappings),
                });
            }
            track_sides.push(MappingTrackSide {
                side: bae_records::records::track_side_of(&side.kind, side.ordinal)
                    .map_err(unplaced)?,
                runs,
            });
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

fn carried_role(role: &str) -> Result<MappingRole, ImportError> {
    match role {
        "audio" => Ok(MappingRole::Audio),
        "artwork" => Ok(MappingRole::Image),
        "document" => Ok(MappingRole::Document),
        "damaged_image" => Ok(MappingRole::DamagedImage),
        "other" => Ok(MappingRole::Other),
        other => Err(ImportError::Internal {
            detail: format!(
                "the statement returned an unknown role {other} for a file no track plays from"
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
    let assigned = match (sheet.place.as_str(), sheet.assigned) {
        ("side", Some(choice)) => Some(choice.read()?),
        ("ignored", None) => None,
        (place, assigned) => {
            return Err(internal(format!(
                "track sheet {} is {place} with the choice {}",
                sheet.sheet_id,
                if assigned.is_some() { "of a side" } else { "of none" },
            )))
        }
    };
    let side_choices =
        sheet.side_choices.into_iter().map(SideChoiceJson::read).collect::<Result<_, _>>()?;
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
        assigned,
        side_choices,
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

fn track_mapping(row: RowJson, position: String) -> Result<TrackMapping, ImportError> {
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
    let artists = row.artists.map(super::ArtistFieldJson::field).transpose()?;
    Ok(TrackMapping {
        source,
        track: track.edit.as_edit(),
        position,
        length_ms: unit.length_ms,
        listed_length_ms: row.listed_ms,
        artists,
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
