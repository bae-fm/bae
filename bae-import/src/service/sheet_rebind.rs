//! What a release editor's save writes for the tracks of the CUE sheets
//! whose `FILE` lines the person bound to other audio files of the release.
//! Each such track plays new bytes, so the save first reads the sheet and
//! the files its lines then play from, through coven, and builds every
//! track's audio rows as the import builds them: its format, its segments
//! with the bytes a seek to each boundary lands on, and its length. It then
//! measures the loudness of every track of the release, since the album's
//! is measured over all of them, and keeps the scrub points the moved
//! tracks' decodes give.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use bae_records::records::sheet_files::{DbReleaseAudio, NewTrack, ReboundAudio, SheetFilePick};
use bae_records::{DbAudioSegmentRole, DbFile};

use crate::extraction::release::{library_file, scanned_audio};
use crate::folder_scanner::{SheetBinding, UserSheetBinding};
use crate::library_audio::LibraryImages;
use crate::library_documents::LibraryDocuments;
use crate::types::{TrackAudio, TrackFile};
use crate::ImportError;

/// The rows a save of `form` writes for the tracks of the CUE sheets whose
/// lines the form binds to other audio files and for the tracks it adds, or
/// `None` when the form binds no line to a file or to no audio.
///
/// The person's earlier decisions about the release's sheets apply, with the
/// form's picks over them. A pick whose file can't back its line, because
/// bae can't carve tracks out of its codec or a track of the line ends, or
/// the last one starts, past its end, is refused with
/// [`ImportError::SheetBinding`], as the import pane refuses them. coven
/// reads each file from wherever it has it; one it can't read fails the
/// save.
///
/// Each audio file that loses all its audio gets a track that plays it
/// whole, built from the audio facts the file's import stored, as the
/// import builds a loose file's. The library works out which tracks the
/// save deletes and which files it adds a track for from the form's edits.
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
    let edits = form.sheet_edits();
    if edits.is_empty() {
        return Ok(None);
    }
    let release_id = form.release_id.as_str();
    let files = records.get_files_for_release(release_id).await?;
    let audio = records.get_release_audio(release_id).await?;
    let changes = records.get_sheet_track_changes(release_id, &edits).await?;
    let dropped: HashSet<&str> = changes.dropped.iter().map(String::as_str).collect();
    let tracks: Vec<bae_records::DbTrack> = records
        .get_tracks_for_release(release_id)
        .await?
        .into_iter()
        .filter(|track| !dropped.contains(track.id.as_str()))
        .collect();
    let analyses = if edits.picks.iter().all(|pick| pick.audio_file_id.is_none()) {
        Vec::new()
    } else {
        analyze_rebound_sheets(
            tables,
            records,
            budget,
            diagnostics,
            &files,
            release_id,
            &edits.picks,
        )
        .await?
    };

    let mut file_ids: HashMap<PathBuf, String> = HashMap::new();
    for analysis in &analyses {
        for audio_file in &analysis.audio_files {
            if !file_ids.contains_key(&audio_file.path) {
                let file = by_path(&files, &audio_file.path)?;
                file_ids.insert(audio_file.path.clone(), file.id.clone());
            }
        }
    }

    let mut to_build = moved_tracks(&tracks, &audio, &files, &analyses)?;
    let moved_count = to_build.len();
    let now = bae_store::utc_now(clock.as_ref());
    let added: Vec<NewTrack> = {
        let mut added = Vec::new();
        for plan in changes.added.iter().cloned() {
            let file = files.iter().find(|file| file.id == plan.file_id).ok_or_else(|| {
                ImportError::Internal {
                    detail: format!("release {release_id} holds no file {}", plan.file_id),
                }
            })?;
            let source_audio =
                file.source_audio.clone().ok_or_else(|| ImportError::UnusableFile {
                    detail: format!("{} has no stored audio facts", file.original_filename),
                })?;
            let file_path = PathBuf::from(&file.original_filename);
            file_ids.insert(file_path.clone(), file.id.clone());
            let track = bae_records::DbTrack {
                id: ids.new_id().to_string(),
                release_id: release_id.to_string(),
                title: plan.typed_title.unwrap_or_else(|| super::title_of_file_name(&plan.name)),
                medium_id: changes.medium_id.clone(),
                side: std::num::NonZeroU8::MIN,
                track_number: std::num::NonZeroU32::MIN,
                duration_ms: source_audio.duration_ms,
                stated_length_ms: None,
                created_at: now,
            };
            to_build.push(TrackFile {
                db_track: track.clone(),
                audio: TrackAudio::Standalone {
                    file_path,
                    source_audio: scanned_audio(source_audio.clone()),
                },
            });
            added.push(NewTrack { file_id: plan.file_id, track });
        }
        added
    };
    let built = {
        let to_build = to_build.clone();
        let images = LibraryImages::new(
            records.clone(),
            bae_store::lease_one(budget).await,
            runtime.clone(),
            file_ids.clone(),
        );
        let file_ids = file_ids.clone();
        diagnostics
            .blocking("build the audio formats of a CUE sheet's tracks", move || {
                super::build_audio_formats(
                    &to_build,
                    &file_ids,
                    clock.as_ref(),
                    ids.as_ref(),
                    &images,
                )
            })
            .await??
    };

    let rebuilt_ids: HashSet<&str> =
        to_build.iter().map(|track| track.db_track.id.as_str()).collect();
    let mut formats = Vec::with_capacity(tracks.len() + added.len());
    let mut segments = built.audio_segments.clone();
    let mut measured = Vec::with_capacity(tracks.len() + added.len());
    for track in &tracks {
        let (format, measured_track) =
            match to_build[..moved_count].iter().position(|moved| moved.db_track.id == track.id) {
                Some(index) => (built.audio_formats[index].clone(), &to_build[index].db_track),
                None => {
                    let format = audio
                        .formats
                        .iter()
                        .find(|format| format.track_id == track.id)
                        .ok_or_else(|| ImportError::Internal {
                            detail: format!("track {} has no stored audio format", track.id),
                        })?;
                    segments.extend(
                        audio.segments.iter().filter(|s| s.audio_format_id == format.id).cloned(),
                    );
                    (format.clone(), track)
                }
            };
        formats.push(format);
        measured.push(measured_track);
    }
    for (index, new) in to_build[moved_count..].iter().enumerate() {
        formats.push(built.audio_formats[moved_count + index].clone());
        measured.push(&new.db_track);
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
        edits,
        track_lengths: to_build[..moved_count]
            .iter()
            .map(|track| (track.db_track.id.clone(), track.db_track.duration_ms))
            .collect(),
        added,
        formats: formats
            .into_iter()
            .filter(|format| rebuilt_ids.contains(format.track_id.as_str()))
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

/// The sheets whose lines `rebinds` bind to other audio files, read with the
/// audio their lines then play from: the sheet files and the audio files
/// of the release, with the person's earlier decisions and the picks over
/// them.
async fn analyze_rebound_sheets(
    tables: &crate::tables::ImportTables,
    records: &bae_records::Records,
    budget: &coven::FileBudget,
    diagnostics: &bae_diagnostics::Diagnostics,
    files: &[DbFile],
    release_id: &str,
    rebinds: &[SheetFilePick],
) -> Result<Vec<Arc<crate::types::CueFlacAnalysis>>, ImportError> {
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
    for pick in rebinds {
        let Some(audio_file_id) = &pick.audio_file_id else { continue };
        let sheet = named(&pick.sheet_file_id)?;
        decisions.sheet_bindings.set_reference(
            sheet.clone(),
            pick.file_reference.clone(),
            UserSheetBinding::Describes { file_id: named(audio_file_id)? },
        );
        if !sheets.contains(&sheet) {
            sheets.push(sheet);
        }
    }

    let lease = bae_store::lease_one(budget).await;
    let read: Vec<&DbFile> =
        files.iter().filter(|file| sheets.contains(&file.original_filename)).collect();
    let documents = LibraryDocuments::read(records, &lease, &read).await?;
    drop(lease);
    let library_files: Vec<_> = files
        .iter()
        .filter(|file| {
            sheets.contains(&file.original_filename)
                || file.role == bae_records::ReleaseFileRole::Audio
        })
        .map(|file| library_file(file.clone()))
        .collect();
    diagnostics
        .blocking("read a release's CUE sheets", move || {
            let categorized = crate::folder_scanner::categorize_library_files(
                library_files,
                &documents,
                decisions,
            );
            sheets
                .iter()
                .map(|sheet| sheet_analysis(&categorized, sheet).map(Arc::new))
                .collect::<Result<Vec<_>, _>>()
        })
        .await?
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

/// The release file `files` holds at `path`, the name it has in the
/// release.
fn by_path<'a>(files: &'a [DbFile], path: &std::path::Path) -> Result<&'a DbFile, ImportError> {
    files.iter().find(|file| std::path::Path::new(&file.original_filename) == path).ok_or_else(
        || ImportError::Internal {
            detail: format!(
                "a CUE sheet's audio at {} is none of the release's files",
                path.display()
            ),
        },
    )
}
