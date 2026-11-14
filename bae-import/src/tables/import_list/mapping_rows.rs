//! The pane's mapping table, read in one statement.
//!
//! The scan stores the folder's audio layout with its files (the units in play
//! order, each one's length and preview range, and what each track sheet's
//! `FILE` lines can be bound to), and the draft stores each track's place. The
//! statement lays them side by side: a row per draft track joined to its audio
//! unit, the rows grouped into sides and runs, each sheet's header, the files
//! the table carries beside its rows, the gallery of images, and the count of
//! rows nobody has named. The code here only turns that document into the
//! mapping table's types and refuses a draft whose tracks are not the folder's
//! units.

use super::*;
use crate::folder_scanner::{SheetBindingOffer, SheetDisc, SheetReferenceOptions};
use crate::mapping::{
    MappingContainer, MappingEntry, MappingFile, MappingFileRow, MappingImage, MappingRole,
    MappingSource, MappingTable, MappingTrackRun, MappingTrackSide, SheetBound, SheetGroup,
    TrackMapping,
};
use crate::types::CandidateDraft;
use crate::ImportError;
use bae_types::audio_format::AudioFormat;

/// The mapping table as one JSON document, for the candidate at `:wf`, `:cp`
/// whose draft is `:hash`.
const MAPPING_SQL: &str = "
WITH RECURSIVE
f AS (
    SELECT * FROM scan_candidate_file WHERE watched_folder_path = :wf AND candidate_path = :cp
),
audio_json (relative_path, json) AS (
    SELECT relative_path,
           json_object('file_id', relative_path, 'name', file_name, 'size', size,
                       'path', hex(absolute_path), 'content_type', audio_content_type,
                       'sample_rate_hz', audio_sample_rate_hz,
                       'bits_per_sample', audio_bits_per_sample,
                       'bitrate_kbps', audio_bitrate_kbps, 'channels', audio_channels)
    FROM f WHERE role = 'audio'
),
mediums AS (
    SELECT m.position,
           bae_medium_is_sided(m.medium) AS sided,
           COALESCE(SUM(CASE WHEN bae_medium_is_sided(m.medium) THEN m.side_count END)
                    OVER (ORDER BY m.position
                          ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0) AS sides_before,
           COUNT(*) OVER () AS medium_count
    FROM import_candidate_medium AS m WHERE m.content_hash = :hash
),
rows AS (
    SELECT t.position, t.title,
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
           ct.number AS cue_number, ct.title AS cue_title,
           COALESCE(t.stated_length_ms, u.duration_ms) AS duration_ms,
           CASE WHEN t.stated_length_ms IS NOT NULL AND u.duration_ms IS NOT NULL
                THEN bae_lengths_disagree(u.duration_ms, t.stated_length_ms) ELSE 0 END
               AS disagree
    FROM import_candidate_track AS t
    JOIN mediums AS m ON m.position = t.medium
    LEFT JOIN scan_candidate_unit AS u
        ON u.watched_folder_path = :wf AND u.candidate_path = :cp AND u.position = t.position
    LEFT JOIN audio_json AS a ON a.relative_path = u.file_id
    LEFT JOIN scan_cue_track AS ct
        ON ct.watched_folder_path = :wf AND ct.candidate_path = :cp
       AND ct.sheet_relative_path = u.sheet_id AND ct.position = u.cue_track_position
    WHERE t.content_hash = :hash
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
            WHERE ct.watched_folder_path = :wf AND ct.candidate_path = :cp AND ct.mode = 'audio'
            UNION ALL
            SELECT ct.sheet_relative_path, ct.file_reference, ct.position, 1, 0
            FROM scan_cue_track AS ct
            WHERE ct.watched_folder_path = :wf AND ct.candidate_path = :cp AND ct.mode = 'audio'
        )
    ) GROUP BY sheet, file_reference
),
disc_numbers (number) AS (
    SELECT 1
    UNION ALL
    SELECT number + 1 FROM disc_numbers
    WHERE number < (SELECT COUNT(*) FROM f WHERE role = 'track_sheet' AND sheet_binding = 'resolved')
),
disc_options AS (
    SELECT json_group_array(number ORDER BY number) AS json FROM (
        SELECT number FROM disc_numbers
        UNION
        SELECT sheet_disc_number FROM f WHERE sheet_disc = 'disc'
    )
),
sheets (relative_path, json) AS (
    SELECT s.relative_path,
           json_object(
               'sheet_id', s.relative_path, 'name', s.relative_path, 'size', s.size,
               'binding', s.sheet_binding, 'codec', s.sheet_binding_codec,
               'assignment', s.sheet_disc, 'disc_number', s.sheet_disc_number,
               'audio', json((SELECT json_group_array(json(a.json) ORDER BY sa.position)
                              FROM scan_sheet_audio_file AS sa
                              JOIN audio_json AS a ON a.relative_path = sa.audio_relative_path
                              WHERE sa.watched_folder_path = :wf AND sa.candidate_path = :cp
                                AND sa.sheet_relative_path = s.relative_path)),
               'requested', json((SELECT json_group_array(r.file_reference ORDER BY r.seq)
                                  FROM refs AS r
                                  WHERE r.sheet = s.relative_path
                                    AND NOT EXISTS (SELECT 1 FROM scan_sheet_audio_file AS sa
                                                    WHERE sa.watched_folder_path = :wf
                                                      AND sa.candidate_path = :cp
                                                      AND sa.sheet_relative_path = s.relative_path
                                                      AND sa.file_reference = r.file_reference))),
               'references', json((
                   SELECT json_group_array(json_object(
                       'file_reference', r.file_reference,
                       'file_id', (SELECT sa.audio_relative_path FROM scan_sheet_audio_file AS sa
                                   WHERE sa.watched_folder_path = :wf AND sa.candidate_path = :cp
                                     AND sa.sheet_relative_path = s.relative_path
                                     AND sa.file_reference = r.file_reference),
                       'options', json((
                           SELECT json_group_array(json_object(
                               'file_id', o.audio_relative_path, 'offer', o.offer,
                               'codec', o.codec) ORDER BY fo.position)
                           FROM scan_candidate_sheet_offer AS o
                           JOIN f AS fo ON fo.relative_path = o.audio_relative_path
                           WHERE o.watched_folder_path = :wf AND o.candidate_path = :cp
                             AND o.sheet_relative_path = s.relative_path
                             AND o.file_reference = r.file_reference))
                   ) ORDER BY r.seq)
                   FROM refs AS r WHERE r.sheet = s.relative_path)),
               'disc_options', CASE WHEN s.sheet_binding = 'resolved'
                                    THEN json((SELECT json FROM disc_options))
                                    ELSE json('[]') END)
    FROM f AS s WHERE s.role = 'track_sheet'
),
runs AS (
    SELECT side_no, run_no, MIN(side_kind) AS side_kind, MIN(side_value) AS side_value,
           json_object(
               'sheet', json((SELECT json FROM sheets WHERE relative_path = n.sheet_id)),
               'rows', json_group_array(json_object(
                   'track', position, 'matches', unit_matches, 'position', position_text,
                   'duration_ms', duration_ms, 'disagree', disagree,
                   'kind', file_kind, 'audio', json(audio), 'unit_ms', unit_ms,
                   'sheet_id', sheet_id, 'index', slice_index, 'number', cue_number,
                   'title', cue_title, 'start', start_sample, 'end', end_sample)
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
                                          WHERE u.watched_folder_path = :wf
                                            AND u.candidate_path = :cp
                                            AND u.sheet_id = f.relative_path)))),
    'untitled', (SELECT COUNT(*) FILTER (WHERE bae_trim(title) = '') FROM rows),
    'tracks', (SELECT COUNT(*) FROM rows),
    'units', (SELECT COUNT(*) FROM scan_candidate_unit
              WHERE watched_folder_path = :wf AND candidate_path = :cp))";

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
    content_type: String,
    sample_rate_hz: i64,
    bits_per_sample: Option<i64>,
    bitrate_kbps: Option<i64>,
    channels: i64,
}

impl AudioJson {
    fn format(&self) -> AudioFormat {
        AudioFormat {
            codec: bae_util::content_type::ContentType::from_mime(&self.content_type)
                .display_name()
                .to_string(),
            sample_rate_hz: self.sample_rate_hz,
            bits_per_sample: self.bits_per_sample,
            bitrate_kbps: self.bitrate_kbps,
            channels: self.channels,
        }
    }
}

#[derive(serde::Deserialize)]
struct RowJson {
    track: usize,
    #[serde(deserialize_with = "flag")]
    matches: bool,
    position: String,
    duration_ms: Option<u64>,
    #[serde(deserialize_with = "flag")]
    disagree: bool,
    kind: Option<String>,
    audio: Option<AudioJson>,
    unit_ms: Option<u64>,
    sheet_id: Option<String>,
    index: Option<u32>,
    number: Option<u32>,
    title: Option<String>,
    start: Option<u64>,
    end: Option<u64>,
}

#[derive(serde::Deserialize)]
struct ReferenceJson {
    file_reference: String,
    file_id: Option<String>,
    options: Vec<OptionJson>,
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
    binding: String,
    codec: Option<String>,
    assignment: String,
    disc_number: Option<u32>,
    audio: Vec<AudioJson>,
    requested: Vec<String>,
    references: Vec<ReferenceJson>,
    disc_options: Vec<u32>,
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

/// The statement's document, decoded: what the pane's mapping table is made
/// of, before the draft's tracks are laid in.
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

/// Read the mapping table of the candidate at `watched_folder_path` and
/// `key`, whose draft is the one stored under `content_hash`.
pub(super) fn load_mapping_rows_on(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &std::path::Path,
    key: &crate::CandidateKey,
    content_hash: &str,
) -> Result<MappingRows, DbError> {
    let document: String = sql.query_row(
        MAPPING_SQL,
        named_params! {
            ":wf": coven::path_bytes(watched_folder_path),
            ":cp": key,
            ":hash": content_hash,
        },
        |row| row.get(0),
    )?;
    Ok(MappingRows { document })
}

impl MappingRows {
    /// The mapping table over `draft`'s tracks. Refuses a draft whose tracks
    /// are not the folder's audio units in order: the draft is redrawn with
    /// every change to the folder's files, so a mismatch is a stored state
    /// that contradicts itself.
    pub(super) fn table(&self, draft: &CandidateDraft) -> Result<MappingTable, ImportError> {
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
                    mappings.push(track_mapping(row, draft)?);
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
                        duration_ms: None,
                        audio_format: None,
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
        (kind, value) => Err(bad(format!("a track side {kind} {value:?}"))),
    }
}

fn carried_role(role: &str) -> Result<MappingRole, ImportError> {
    match role {
        "artwork" => Ok(MappingRole::Image),
        "document" => Ok(MappingRole::Document),
        "damaged_image" => Ok(MappingRole::DamagedImage),
        "other" => Ok(MappingRole::Other),
        other => Err(ImportError::Internal { detail: format!("a carried file's role {other}") }),
    }
}

fn container(audio: &AudioJson) -> MappingContainer {
    MappingContainer {
        file_id: audio.file_id.clone(),
        name: audio.name.clone(),
        size: audio.size,
        audio_format: audio.format(),
    }
}

fn sheet_group(sheet: SheetJson) -> Result<SheetGroup, ImportError> {
    let internal = |detail: String| ImportError::Internal { detail };
    let bound = match sheet.binding.as_str() {
        "resolved" => match sheet.audio.as_slice() {
            [audio] => SheetBound::Describes(container(audio)),
            [_, _, ..] => SheetBound::DescribesFiles {
                audio_file_count: u32::try_from(sheet.audio.len())
                    .map_err(|error| internal(format!("a sheet's audio count: {error}")))?,
            },
            [] => return Err(internal(format!("{} is bound to no audio", sheet.sheet_id))),
        },
        "unresolved" => SheetBound::Unresolved { requested: sheet.requested },
        "refused_timing" => SheetBound::RefusedTiming,
        "refused_codec" => SheetBound::RefusedCodec {
            codec: sheet.codec.ok_or_else(|| internal("a refused codec has a name".to_owned()))?,
        },
        other => return Err(internal(format!("a sheet binding {other}"))),
    };
    let assignment = match (sheet.assignment.as_str(), sheet.disc_number) {
        ("disc", Some(number)) => SheetDisc::Disc { number },
        ("ignored", None) => SheetDisc::Ignored,
        (other, number) => return Err(internal(format!("a sheet disc {other} {number:?}"))),
    };
    let reference_options = sheet
        .references
        .into_iter()
        .map(|reference| {
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
                                ("refused_codec", Some(codec)) => {
                                    SheetBindingOffer::RefusedCodec { codec }
                                }
                                ("refused_unreadable", None) => {
                                    SheetBindingOffer::RefusedUnreadable
                                }
                                (other, _) => {
                                    return Err(internal(format!("a sheet offer {other}")))
                                }
                            },
                        })
                    })
                    .collect::<Result<Vec<_>, ImportError>>()?,
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;
    Ok(SheetGroup {
        sheet_id: sheet.sheet_id,
        name: sheet.name,
        size: sheet.size,
        bound,
        reference_options,
        assignment,
        disc_options: sheet.disc_options,
    })
}

fn track_mapping(row: RowJson, draft: &CandidateDraft) -> Result<TrackMapping, ImportError> {
    let internal = |detail: String| ImportError::Internal { detail };
    let track = draft
        .tracks
        .get(row.track)
        .ok_or_else(|| internal(format!("the mapping names no draft track {}", row.track)))?;
    if !row.matches {
        return Err(internal(format!(
            "draft track {} plays {:?}, which is not the folder's audio unit there",
            track.edit.id, track.edit.file
        )));
    }
    let audio = row.audio.ok_or_else(|| internal("an audio unit names no file".to_owned()))?;
    let source = match row.kind.as_deref() {
        Some("standalone") => MappingSource::File(MappingFile {
            file_id: audio.file_id.clone(),
            name: audio.file_id.clone(),
            size: audio.size,
            preview_target: Some(bae_track_audio::PreviewTarget::whole_file(audio.path.clone())),
            duration_ms: row.unit_ms,
            audio_format: Some(audio.format()),
            role: MappingRole::Audio,
        }),
        Some("sheet_slice") => MappingSource::SheetEntry(MappingEntry {
            sheet_id: row.sheet_id.ok_or_else(|| internal("a slice names no sheet".to_owned()))?,
            index: row.index.ok_or_else(|| internal("a slice has no index".to_owned()))?,
            number: row.number.ok_or_else(|| internal("a slice has no number".to_owned()))?,
            title: row.title,
            duration_ms: row.unit_ms,
            container_id: audio.file_id.clone(),
            container_name: audio.name.clone(),
            preview_target: bae_track_audio::PreviewTarget::sample_range(
                audio.path.clone(),
                row.start.ok_or_else(|| internal("a slice has no start".to_owned()))?,
                row.end,
            ),
            audio_format: audio.format(),
        }),
        other => return Err(internal(format!("an audio unit of kind {other:?}"))),
    };
    Ok(TrackMapping {
        source,
        track: track.edit.as_edit(),
        position: row.position,
        duration_ms: row.duration_ms,
        lengths_disagree: row.disagree,
    })
}

#[cfg(test)]
impl crate::tables::ImportTables {
    /// The mapping table of the stored candidate `key`, over the draft stored
    /// under `content_hash`.
    pub(crate) async fn load_mapping_table_for_test(
        &self,
        watched_folder_path: &std::path::Path,
        key: &crate::CandidateKey,
        content_hash: &str,
    ) -> Result<MappingTable, ImportError> {
        let (watched_folder_path, key, content_hash) =
            (watched_folder_path.to_path_buf(), key.clone(), content_hash.to_string());
        let (rows, draft) = self
            .store
            .read(move |sql| {
                let draft =
                    crate::tables::import_state::load_pane_rows_on(&sql, &content_hash)?.draft;
                Ok((load_mapping_rows_on(&sql, &watched_folder_path, &key, &content_hash)?, draft))
            })
            .await
            .expect("the rows are read");
        rows.table(&draft)
    }
}
