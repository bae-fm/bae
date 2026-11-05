//! The tracks an import writes, each bound to the audio its samples come
//! from and measured by it.

use crate::folder_scanner::{CategorizedFiles, ScannedFile};
use crate::probe::sheet_analysis;
use crate::types::{AudioFile, CueFlacAnalysis, TrackAudio, TrackFile};
use crate::ImportError;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

/// Bind each track to the audio holding its samples and yield the
/// [`TrackFile`]s the run pass consumes.
///
/// `rows` is the mapping the commit settled: one `(track, audio)` pair per row
/// that will be written, in track order. Each row becomes the `DbTrack` of a
/// `TrackFile` with the length its audio measures — the sheet's own timing for
/// a slice, the probe's for a standalone file — and every slice of one sheet
/// shares that sheet's single parsed analysis.
///
/// A row whose title is blank is titled after its audio file's name. An empty
/// title is a track nobody can find again: it renders as a blank row in every
/// list, sorts nowhere, and matches no search. The file's own name is the one
/// fact about that track that is certainly true, and it is what the mapping
/// table showed on that very row — so it is what gets written. Reading the
/// file's embedded tag instead would let a second metadata authority into an
/// import whose authority the user already chose, and would write something
/// the table never displayed.
pub(crate) fn resolve_track_files(
    rows: Vec<(bae_db::TrackRow, AudioFile)>,
    files: &CategorizedFiles,
) -> Result<Vec<TrackFile>, ImportError> {
    debug!("Binding {} tracks to the folder's audio", rows.len());
    let mut analyses: HashMap<String, Arc<CueFlacAnalysis>> = HashMap::new();
    let mut track_files = Vec::with_capacity(rows.len());

    for (mut row, audio) in rows {
        let file = audio_file(files, &audio)?;
        if row.title.trim().is_empty() {
            row.title = file_title(file);
        }
        let (audio, duration_ms) = match &audio {
            AudioFile::Standalone { .. } => {
                let source_audio =
                    file.source_audio.clone().ok_or_else(|| ImportError::UnusableFile {
                        detail: format!("{} has no scanned audio facts", file.relative_path),
                    })?;
                let duration_ms = source_audio.duration_ms;
                (
                    TrackAudio::Standalone { file_path: file.version.path.clone(), source_audio },
                    duration_ms,
                )
            }
            AudioFile::SheetSlice { sheet_id, index, .. } => {
                let analysis = match analyses.get(sheet_id) {
                    Some(analysis) => Arc::clone(analysis),
                    None => {
                        let analysis = Arc::new(sheet_analysis(files, sheet_id)?);
                        analyses.insert(sheet_id.clone(), Arc::clone(&analysis));
                        analysis
                    }
                };
                let cue_index = *index as usize;
                let duration_ms =
                    crate::probe::sheet_track_duration_ms(&analysis, cue_index, sheet_id)?;
                (TrackAudio::CueBacked { cue_pair: analysis, cue_index }, duration_ms)
            }
        };
        let db_track = row.measured(duration_ms);
        track_files.push(TrackFile { db_track, audio });
    }
    Ok(track_files)
}

/// The persisted scanned audio a binding names. Audio that is no longer among
/// the candidate's files is a refusal: the mapping named samples this import
/// cannot read, and the folder changed under the choice. Whether each file on
/// disk is still the one scanned is the import's own check, made once for
/// every release file before any is read
/// ([`super::file_identity::validate_scanned_file_identities`]).
fn audio_file<'a>(
    files: &'a CategorizedFiles,
    audio: &AudioFile,
) -> Result<&'a ScannedFile, ImportError> {
    files.audio().find(|file| file.relative_path == audio.file_id()).ok_or_else(|| {
        ImportError::UnusableFile {
            detail: format!("{} is no longer in the folder", audio.file_id()),
        }
    })
}

/// A file's name without its extension — the title an unnamed row writes.
fn file_title(file: &ScannedFile) -> String {
    std::path::Path::new(&file.file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&file.file_name)
        .to_string()
}

#[cfg(test)]
#[path = "track_files_tests.rs"]
mod tests;
