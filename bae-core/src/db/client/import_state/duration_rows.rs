//! What one candidate's audio units play for, as rows.
//!
//! Two kinds, matching the two kinds of
//! [`AudioFile`](crate::import::AudioFile): a whole file, and one entry
//! a bound track sheet carves out of a container. A STRICT primary key column
//! cannot be NULL, so the file kind stores the two sheet columns as `''` and
//! `-1` and the table's CHECKs keep the sentinels and the kind agreeing.

use super::verdict_rows::unreadable;
use super::*;
use crate::import::probe::{ProbedDurations, ProbedUnit};
use crate::import::AudioFile;

const DURATION_COLUMNS: &str =
    "content_hash, kind, relative_path, sheet_relative_path, slice_index, duration_ms";

/// The five key columns one unit is addressed by.
struct UnitKey<'a> {
    kind: &'static str,
    relative_path: &'a str,
    sheet_relative_path: &'a str,
    slice_index: i64,
}

fn unit_key(audio: &AudioFile) -> UnitKey<'_> {
    match audio {
        AudioFile::Standalone { file_id } => UnitKey {
            kind: "file",
            relative_path: file_id,
            sheet_relative_path: "",
            slice_index: -1,
        },
        AudioFile::SheetSlice {
            file_id,
            sheet_id,
            index,
        } => UnitKey {
            kind: "slice",
            relative_path: file_id,
            sheet_relative_path: sheet_id,
            slice_index: i64::from(*index),
        },
    }
}

/// Every duration row this candidate holds. Rows the caller took out are not
/// mentioned: a duration is a fact about bytes, and the write is an upsert.
pub(super) fn insert_durations(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    durations: &ProbedDurations,
) -> Result<(), DbError> {
    for unit in &durations.units {
        let key = unit_key(&unit.audio);
        let duration_ms = unit
            .duration_ms
            .map(|ms| {
                i64::try_from(ms).map_err(|_| {
                    DbError::Message(format!(
                        "{} plays for {ms} ms, past SQLite's integer range",
                        key.relative_path
                    ))
                })
            })
            .transpose()?;
        sql.execute(
            &format!(
                "INSERT INTO import_candidate_file_duration ({DURATION_COLUMNS}) \
                 VALUES (?, ?, ?, ?, ?, ?) \
                 ON CONFLICT (content_hash, kind, relative_path, sheet_relative_path, slice_index) \
                 DO UPDATE SET duration_ms = excluded.duration_ms"
            ),
            params![
                content_hash,
                key.kind,
                key.relative_path,
                key.sheet_relative_path,
                key.slice_index,
                duration_ms,
            ],
        )?;
    }
    Ok(())
}

/// Every duration row under `content_hash`.
pub(super) fn delete_durations(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_file_duration WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// The slice rows alone — what a binding change invalidates, because a
/// different container is being carved. A whole file's own length is a fact
/// about its bytes and survives.
pub(super) fn delete_slice_durations(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_file_duration WHERE content_hash = ? AND kind = 'slice'",
        [content_hash],
    )?;
    Ok(())
}

/// Every candidate's duration rows, or the one `only` names.
pub(super) fn load_durations_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, ProbedDurations>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT {DURATION_COLUMNS} FROM import_candidate_file_duration \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, kind, relative_path, sheet_relative_path, slice_index"
        ),
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("kind")?,
                row.get::<_, String>("relative_path")?,
                row.get::<_, String>("sheet_relative_path")?,
                row.get::<_, i64>("slice_index")?,
                row.get::<_, Option<i64>>("duration_ms")?,
            ))
        },
    )?;
    let mut out: HashMap<String, ProbedDurations> = HashMap::new();
    for (content_hash, kind, relative_path, sheet_relative_path, slice_index, duration_ms) in rows {
        let audio = match kind.as_str() {
            "file" => AudioFile::Standalone {
                file_id: relative_path,
            },
            "slice" => AudioFile::SheetSlice {
                file_id: relative_path,
                sheet_id: sheet_relative_path,
                index: u32::try_from(slice_index).map_err(|_| {
                    DbError::Message(format!("a stored slice is numbered {slice_index}"))
                })?,
            },
            other => return Err(unreadable("kind", other)),
        };
        let duration_ms = duration_ms
            .map(|ms| {
                u64::try_from(ms)
                    .map_err(|_| DbError::Message(format!("a stored duration is {ms} ms")))
            })
            .transpose()?;
        out.entry(content_hash)
            .or_default()
            .units
            .push(ProbedUnit { audio, duration_ms });
    }
    Ok(out)
}
