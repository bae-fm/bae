//! What a release editor's save writes for the tracks of the CUE sheets
//! whose `FILE` lines the person bound to other audio files of the release.
//! Each such track plays new bytes, so the save first reads the sheet and
//! the files its lines then play from on this device and builds every
//! track's audio rows as the import builds them: its format, its segments
//! with the bytes a seek to each boundary lands on, and its length. It then
//! measures the loudness of every track of the release, since the album's
//! is measured over all of them, and keeps the scrub points the moved
//! tracks' decodes give.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use bae_records::records::sheet_files::{DbReleaseAudio, ReboundAudio};
use bae_records::{DbAudioSegmentRole, DbFile};

use crate::extraction::release::{library_file, StagedFiles};
use crate::folder_scanner::{SheetBinding, UserSheetBinding};
use crate::types::{TrackAudio, TrackFile};
use crate::ImportError;

/// The rows a save of `form` writes for the tracks of the CUE sheets whose
/// lines the form binds to other audio files, or `None` when it binds none.
///
/// The person's earlier decisions about the release's sheets apply, with the
/// form's picks over them. A pick whose file can't back its line, because
/// bae can't carve tracks out of its codec or a track of the line ends, or
/// the last one starts, past its end, is refused with
/// [`ImportError::SheetBinding`], as the import pane refuses it. coven reads
/// each file from wherever it has it; one it can't read fails the save.
pub async fn rebound_sheet_audio(
    tables: &crate::tables::ImportTables,
    records: &bae_records::Records,
    budget: &coven::FileBudget,
    diagnostics: &bae_diagnostics::Diagnostics,
    runtime: &tokio::runtime::Handle,
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
    form: &crate::ReleaseEditForm,
) -> Result<Option<ReboundAudio>, ImportError> {
    if form.sheet_files.is_empty() {
        return Ok(None);
    }
    let release_id = form.release_id.as_str();
    let files = records.get_files_for_release(release_id).await?;
    let tracks = records.get_tracks_for_release(release_id).await?;
    let audio = records.get_release_audio(release_id).await?;
    let named = |file_id: &str| {
        files
            .iter()
            .find(|file| file.id == file_id)
            .ok_or_else(|| ImportError::Internal {
                detail: format!("release {release_id} holds no file {file_id}"),
            })
            .map(|file| file.original_filename.clone())
    };

    let mut decisions = tables.load_release_sheet_decisions(release_id).await?;
    let mut sheets: Vec<String> = Vec::new();
    for pick in &form.sheet_files {
        let sheet = named(&pick.sheet_file_id)?;
        decisions.sheet_bindings.set_reference(
            sheet.clone(),
            pick.file_reference.clone(),
            UserSheetBinding::Describes { file_id: named(&pick.audio_file_id)? },
        );
        if !sheets.contains(&sheet) {
            sheets.push(sheet);
        }
    }

    let staged = diagnostics
        .blocking("make a folder for a release's CUE sheets and audio", StagedFiles::create)
        .await?
        .map_err(|error| staging_failed("a folder", error))?;
    let lease = bae_store::lease_one(budget).await;
    for sheet in files.iter().filter(|file| sheets.contains(&file.original_filename)) {
        let bytes = records.read_release_file(&sheet.id, &lease).await?;
        staged
            .write(diagnostics, staged.release_file(&sheet.original_filename), bytes)
            .await?
            .map_err(|error| staging_failed(&sheet.original_filename, error))?;
    }
    let library_files: Vec<_> = files
        .iter()
        .filter(|file| {
            sheets.contains(&file.original_filename)
                || file.role == bae_records::ReleaseFileRole::Audio
        })
        .map(|file| library_file(file.clone(), staged.release_file(&file.original_filename)))
        .collect();
    let analyses = diagnostics
        .blocking("read a release's CUE sheets", move || {
            let categorized =
                crate::folder_scanner::categorize_library_files(library_files, decisions, &lease);
            sheets
                .iter()
                .map(|sheet| sheet_analysis(&categorized, sheet).map(Arc::new))
                .collect::<Result<Vec<_>, _>>()
        })
        .await??;

    let lease = bae_store::lease_one(budget).await;
    let mut file_ids: HashMap<PathBuf, String> = HashMap::new();
    for analysis in &analyses {
        for audio_file in &analysis.audio_files {
            if file_ids.contains_key(&audio_file.path) {
                continue;
            }
            let file = by_path(&files, &staged, &audio_file.path)?;
            staged
                .copy(diagnostics, records, &file.id, audio_file.path.clone(), &lease)
                .await?
                .map_err(|error| staging_failed(&file.original_filename, error))?;
            file_ids.insert(audio_file.path.clone(), file.id.clone());
        }
    }
    drop(lease);

    let moved = moved_tracks(&tracks, &audio, &files, &analyses)?;
    let built = {
        let moved = moved.clone();
        let file_ids = file_ids.clone();
        let budget = budget.clone();
        diagnostics
            .blocking("build the audio formats of a CUE sheet's tracks", move || {
                super::build_audio_formats(&moved, &file_ids, clock.as_ref(), ids.as_ref(), &budget)
            })
            .await??
    };
    // The staged copies were read for where each track starts; the loudness
    // pass reads the release's files through coven.
    drop(staged);

    let moved_ids: HashSet<&str> = moved.iter().map(|track| track.db_track.id.as_str()).collect();
    let mut formats = Vec::with_capacity(tracks.len());
    let mut segments = built.audio_segments.clone();
    let mut measured = Vec::with_capacity(tracks.len());
    for track in &tracks {
        let (format, measured_track) = match moved.iter().position(|m| m.db_track.id == track.id) {
            Some(index) => (built.audio_formats[index].clone(), &moved[index].db_track),
            None => {
                let format =
                    audio.formats.iter().find(|format| format.track_id == track.id).ok_or_else(
                        || ImportError::Internal {
                            detail: format!("track {} has no stored audio format", track.id),
                        },
                    )?;
                segments.extend(
                    audio.segments.iter().filter(|s| s.audio_format_id == format.id).cloned(),
                );
                (format.clone(), track)
            }
        };
        formats.push(format);
        measured.push(measured_track);
    }
    let file_sizes: HashMap<String, u64> =
        files.iter().map(|file| (file.id.clone(), file.file_size)).collect();
    let loudness = crate::loudness::measure_library_tracks(
        records,
        budget,
        runtime,
        &mut formats,
        &segments,
        &measured,
        &file_sizes,
        diagnostics,
    )
    .await?;

    let new_segments: HashSet<String> =
        built.audio_segments.iter().map(|segment| segment.id.clone()).collect();
    Ok(Some(ReboundAudio {
        picks: form.sheet_files.clone(),
        track_lengths: moved
            .iter()
            .map(|track| (track.db_track.id.clone(), track.db_track.duration_ms))
            .collect(),
        formats: formats
            .into_iter()
            .filter(|format| moved_ids.contains(format.track_id.as_str()))
            .collect(),
        segments: built.audio_segments,
        scrub_points: loudness
            .scrub_points
            .into_iter()
            .filter(|points| new_segments.contains(points.audio_segment_id.as_str()))
            .collect(),
        album_loudness_lufs: loudness.album_loudness_lufs,
        album_peak_linear: loudness.album_peak_linear,
    }))
}

/// The sheet `sheet` of `categorized` read with the audio its lines play
/// from, or why that audio can't back it.
fn sheet_analysis(
    categorized: &crate::folder_scanner::CategorizedFiles,
    sheet: &str,
) -> Result<crate::types::CueFlacAnalysis, ImportError> {
    let parsed = categorized
        .track_sheets()
        .find(|parsed| parsed.file.relative_path == sheet)
        .ok_or_else(|| ImportError::UnusableFile { detail: format!("{sheet} no longer parses") })?;
    let detail = match parsed.binding {
        SheetBinding::Resolved { .. } => return crate::probe::sheet_analysis(categorized, sheet),
        SheetBinding::RefusedTiming { .. } => {
            format!("a track of {sheet} ends, or its last track starts, past the audio's end")
        }
        SheetBinding::RefusedCodec { codec } => {
            format!("bae can't carve the tracks of {sheet} out of {codec} audio")
        }
        SheetBinding::Unresolved { .. } => {
            format!("bae can't find the audio of every FILE line of {sheet}")
        }
    };
    Err(ImportError::SheetBinding { detail })
}

/// Each track the picks move, in release order, as the import's track it
/// is: its row with the length the sheet and its new audio give it, and the
/// sheet's track it plays, found by its `FILE` line and where its body
/// starts. A track of a picked sheet that no track of the sheet starts at
/// is a damaged release.
fn moved_tracks(
    tracks: &[bae_records::DbTrack],
    audio: &DbReleaseAudio,
    files: &[DbFile],
    analyses: &[Arc<crate::types::CueFlacAnalysis>],
) -> Result<Vec<TrackFile>, ImportError> {
    let mut moved = Vec::new();
    for track in tracks {
        let Some(carved) = audio.carved.iter().find(|carved| carved.track_id == track.id) else {
            continue;
        };
        let sheet = files
            .iter()
            .find(|file| file.id == carved.sheet_file_id)
            .map(|file| file.original_filename.as_str());
        let Some(analysis) = analyses.iter().find(|analysis| Some(&*analysis.sheet_id) == sheet)
        else {
            continue;
        };
        let damaged = |why: &str| ImportError::Internal {
            detail: format!("track {} of {}: {why}", track.id, analysis.sheet_id),
        };
        let format = audio
            .formats
            .iter()
            .find(|format| format.track_id == track.id)
            .ok_or_else(|| damaged("no stored audio format"))?;
        let start_sample = audio
            .segments
            .iter()
            .find(|s| s.audio_format_id == format.id && s.role == DbAudioSegmentRole::Main)
            .ok_or_else(|| damaged("no stored main segment"))?
            .start_sample;
        let sample_rate = u64::from(format.sample_rate.get());
        let cue_index = analysis
            .cue_sheet
            .playable_tracks()
            .position(|cue_track| {
                cue_track.file_reference == carved.file_reference
                    && crate::cue_sheet::cue_frames_to_samples(
                        cue_track.start_cue_frames,
                        sample_rate,
                    ) == start_sample
            })
            .ok_or_else(|| damaged("no track of the sheet starts where it does"))?;
        let duration_ms =
            crate::probe::sheet_track_duration_ms(analysis, cue_index, &analysis.sheet_id)?;
        moved.push(TrackFile {
            db_track: bae_records::DbTrack { duration_ms, ..track.clone() },
            audio: TrackAudio::CueBacked { cue_pair: Arc::clone(analysis), cue_index },
        });
    }
    Ok(moved)
}

/// The release file `files` holds at `path`, one of `staged`'s paths.
fn by_path<'a>(
    files: &'a [DbFile],
    staged: &StagedFiles,
    path: &std::path::Path,
) -> Result<&'a DbFile, ImportError> {
    files.iter().find(|file| staged.release_file(&file.original_filename) == path).ok_or_else(
        || ImportError::Internal {
            detail: format!(
                "a CUE sheet's audio at {} is none of the release's files",
                path.display()
            ),
        },
    )
}

/// Why bae couldn't copy `what` to read it.
fn staging_failed(what: &str, error: std::io::Error) -> ImportError {
    ImportError::UnusableFile { detail: format!("bae couldn't copy {what} to read it: {error}") }
}
