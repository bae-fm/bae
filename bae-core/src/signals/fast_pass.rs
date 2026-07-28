//! The fast pass: every non-OCR signal a folder yields, gathered in one blocking
//! hop by [`gather_non_ocr_sources`] — the disc ID, CUE-CATALOG barcodes, and
//! text source lines + brackets, plus the artwork paths for the later OCR phase.
//! The service emits the result as its first `Signals` snapshot.

use super::candidate_text::{
    extract_folder_brackets, parse_filename_stem, strip_path_component, Source, SourcedLine,
};
use crate::import::discid::compute_discid_from_categorized;
use crate::import::folder_scanner::CategorizedFiles;
use crate::signals::{DiscIdSignal, SignalOrigin, SourcedValue};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Maximum size read from a single `.txt` file. Caps a pathological input (a
/// 10 MB booklet transcription) without blowing memory.
const MAX_TEXT_FILE_BYTES: u64 = 100 * 1024;

pub(super) struct FastPass {
    pub(super) lines: Vec<SourcedLine>,
    pub(super) bracket_catalogs: Vec<String>,
    pub(super) artwork_paths: Vec<PathBuf>,
    pub(super) disc_id: DiscIdSignal,
    pub(super) cue_barcodes: Vec<SourcedValue>,
    /// Total playing time of the folder's audio, summed over the same scan the
    /// disc ID came from. `0` when nothing was probed — see
    /// [`crate::signals::Signals::probed_total_duration_ms`].
    pub(super) probed_total_duration_ms: u64,
}

impl FastPass {
    /// No disc ID, no sources — what a failed folder scan yields.
    pub(super) fn empty() -> Self {
        Self {
            lines: Vec::new(),
            bracket_catalogs: Vec::new(),
            artwork_paths: Vec::new(),
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            cue_barcodes: Vec::new(),
            probed_total_duration_ms: 0,
        }
    }
}

/// Total playing time of a candidate's audio, probed file by file.
///
/// Rides the fast pass rather than becoming a second walk: the scan above has
/// already resolved which files are the release's audio, and a CUE image's one
/// file probes to the whole disc, so summing is the total either way.
///
/// `0` when any file fails to probe. A partial sum understates the total and
/// would read downstream as "the durations disagree" — a wrong answer where the
/// honest one is "we don't know", and the Ready rule refuses both, so the
/// distinction costs nothing and the wrong reason is not offered to the user.
fn probe_total_duration_ms(categorized: &CategorizedFiles) -> u64 {
    let mut total_ms: u64 = 0;
    for path in categorized.audio_paths() {
        let Some(path_str) = path.to_str() else {
            warn!(
                "signals: audio path is not UTF-8, so it cannot be probed: {}",
                path.display()
            );
            return 0;
        };
        match crate::audio_codec::probe_audio_from_path(path_str) {
            Some(probe) => total_ms += probe.duration.as_millis() as u64,
            None => {
                debug!(
                    "signals: could not probe {}; the candidate's total duration is unknown",
                    path.display()
                );
                return 0;
            }
        }
    }
    total_ms
}

/// CUE `CATALOG` payloads (the disc's UPC/EAN) from the folder's parsed sheets,
/// bound and unbound, deduped. These are barcode-lookup inputs, not
/// catalog-number filter values.
fn cue_barcodes(categorized: &CategorizedFiles) -> Vec<SourcedValue> {
    let mut out: Vec<SourcedValue> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for sheet in categorized.track_sheets() {
        if let Some(value) = &sheet.sheet.catalog {
            let value = value.trim();
            if !value.is_empty() && seen.insert(value.to_string()) {
                out.push(SourcedValue::new(value.to_string(), SignalOrigin::CueSheet));
            }
        }
    }
    out
}

/// Enumerate and read every non-OCR source. Blocking; the service runs it via
/// `spawn_blocking`. A failure surfaces as missing data for that one source and
/// never aborts extraction.
///
pub(super) fn gather_non_ocr_sources(folder: &Path, categorized: &CategorizedFiles) -> FastPass {
    let mut pass = FastPass::empty();

    // Path components: the candidate folder's own name and its parent's.
    let folder_name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let parent_name = folder
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();

    for raw in [&parent_name, &folder_name] {
        if raw.is_empty() {
            continue;
        }
        if let Some(stripped) = strip_path_component(raw) {
            pass.lines.push(SourcedLine {
                source: Source::PathComponent,
                text: stripped,
            });
        }
        for bracket in extract_folder_brackets(raw) {
            pass.bracket_catalogs.push(bracket);
        }
    }

    // Disc ID, CUE-CATALOG barcodes, and the probed total all come off the same
    // parsed scan, no re-read and no second walk over the audio.
    let track_count = categorized.track_count();
    pass.probed_total_duration_ms = probe_total_duration_ms(categorized);
    pass.disc_id = match compute_discid_from_categorized(categorized) {
        Some(disc_id) => DiscIdSignal::Computed {
            disc_id,
            track_count,
        },
        None => DiscIdSignal::Absent { track_count },
    };
    pass.cue_barcodes = cue_barcodes(categorized);

    // Image + document filenames only; `enumerate_filename_inputs` explains why.
    for p in enumerate_filename_inputs(categorized) {
        for part in parse_filename_stem(&p) {
            pass.lines.push(SourcedLine {
                source: Source::FilenameGeneric(p.clone()),
                text: part,
            });
        }
    }

    // PERFORMER / TITLE from every sheet the scan already parsed, bound or not.
    // No re-read, no second parser.
    for sheet in categorized.track_sheets() {
        for name in cue_sheet_names(sheet.sheet) {
            pass.lines.push(SourcedLine {
                source: Source::CueField,
                text: name,
            });
        }
    }

    // Text files feed the pool one line at a time, like OCR output.
    for doc in text_file_paths(categorized) {
        if let Some(text) = read_capped_text(&doc) {
            for line in text.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    pass.lines.push(SourcedLine {
                        source: Source::TextFile(doc.clone()),
                        text: trimmed.to_string(),
                    });
                }
            }
        }
    }

    pass.artwork_paths = categorized.artwork().map(|f| f.path.clone()).collect();

    pass
}

/// The filenames the classifier should see: artwork and documents.
///
/// Audio filenames are excluded — their stems are almost always track titles,
/// which belong in a track-title pool, not the Artist / Album autocomplete one.
/// Track-sheet filenames are excluded because the sheet's `PERFORMER` / `TITLE`
/// are already harvested as `CueField` lines, so the stem (`Album.cue` →
/// `Album`) would only duplicate path-component signal at lower weight. Ignored
/// files carry no release signal at all.
fn enumerate_filename_inputs(categorized: &CategorizedFiles) -> Vec<PathBuf> {
    categorized
        .artwork()
        .chain(categorized.documents())
        .map(|f| f.path.clone())
        .collect()
}

/// Album- and track-level PERFORMER / TITLE values from an already-parsed CUE
/// sheet, deduped — name tokens for the Artist / Album autocomplete pool.
fn cue_sheet_names(sheet: &crate::cue_flac::CueSheet) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |value: &Option<String>| {
        if let Some(s) = value {
            let s = s.trim();
            if !s.is_empty() && seen.insert(s.to_string()) {
                out.push(s.to_string());
            }
        }
    };
    push(&sheet.performer);
    push(&sheet.title);
    for track in &sheet.tracks {
        push(&track.performer);
        push(&track.title);
    }
    out
}

/// `.txt` documents only. `.log` is rip-technical data with no artist/album
/// content; `.cue` is harvested through its parsed sheet.
fn text_file_paths(categorized: &CategorizedFiles) -> Vec<PathBuf> {
    categorized
        .documents()
        .filter(|f| has_ext(&f.path, "txt"))
        .map(|f| f.path.clone())
        .collect()
}

fn has_ext(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

/// Read a text file, capped at `MAX_TEXT_FILE_BYTES` and decoded through
/// `text_encoding` — so a non-UTF-8 file is decoded, not dropped. `None` only on
/// an I/O error, which is logged so a skip is never silent.
fn read_capped_text(path: &Path) -> Option<String> {
    use std::io::{ErrorKind, Read};

    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            debug!(
                "candidate_text: file vanished after scan: {}",
                path.display()
            );
            return None;
        }
        Err(e) => {
            warn!("candidate_text: open failed for {}: {e}", path.display());
            return None;
        }
    };
    let mut buf = Vec::new();
    if let Err(e) = f.take(MAX_TEXT_FILE_BYTES).read_to_end(&mut buf) {
        warn!("candidate_text: read failed for {}: {e}", path.display());
        return None;
    }
    Some(crate::text_encoding::decode_text(&buf).text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_filename_inputs_skips_all_audio_and_cue() {
        use crate::import::folder_scanner::{CandidateFile, FileRole, ScannedFile};

        let cue = ScannedFile::new(
            PathBuf::from("/rel/Album.cue"),
            "Album.cue".to_string(),
            100,
        );
        let flac = ScannedFile::new(
            PathBuf::from("/rel/Album.flac"),
            "Album.flac".to_string(),
            5_000_000,
        );
        let cover = ScannedFile::new(
            PathBuf::from("/rel/Artist Name - Album.png"),
            "Artist Name - Album.png".to_string(),
            10_000,
        );
        let categorized = CategorizedFiles {
            files: vec![
                CandidateFile {
                    proposed_audio: false,
                    file: cue.clone(),
                    role: FileRole::TrackSheet {
                        sheet: crate::cue_flac::CueSheet {
                            title: None,
                            performer: None,
                            catalog: None,
                            date: None,
                            tracks: Vec::new(),
                        },
                        binding: crate::import::folder_scanner::SheetBinding::Describes {
                            file_id: "Album.flac".to_string(),
                        },
                    },
                },
                CandidateFile {
                    proposed_audio: true,
                    file: flac.clone(),
                    role: FileRole::Audio,
                },
                CandidateFile {
                    proposed_audio: false,
                    file: cover.clone(),
                    role: FileRole::Artwork,
                },
            ],
            format_label: "CUE+FLAC".to_string(),
        };

        let inputs = enumerate_filename_inputs(&categorized);
        assert!(
            !inputs.iter().any(|p| p == &cue.path),
            "CUE names come from the parsed sheet, not the filename pool; got {inputs:?}",
        );
        assert!(
            !inputs.iter().any(|p| p == &flac.path),
            "audio filename stems are almost always track titles — wrong pool \
             for Artist / Album autocomplete; got {inputs:?}",
        );
        assert!(
            inputs.iter().any(|p| p == &cover.path),
            "artwork filenames still contribute — `Artist Name - Album.png` \
             and similar carry real signal; got {inputs:?}",
        );
    }

    /// One categorize yields both the disc ID (from the LOG) and the real track
    /// count — the pair the folder fast pass reads.
    #[test]
    fn test_categorized_yields_discid_and_track_count() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let fixture_log = std::path::Path::new("tests/fixtures/test_album.log");
        std::fs::copy(fixture_log, dir.join("test_album.log")).unwrap();

        let fixture_dir = std::path::Path::new("tests/fixtures/flac");
        std::fs::copy(
            fixture_dir.join("01 Test Track 1.flac"),
            dir.join("01 Test Track 1.flac"),
        )
        .unwrap();
        std::fs::copy(
            fixture_dir.join("02 Test Track 2.flac"),
            dir.join("02 Test Track 2.flac"),
        )
        .unwrap();

        let categorized =
            crate::import::folder_scanner::collect_release_candidate_files_with_scope(
                dir,
                crate::import::ReleaseFileScope::Recursive,
                &crate::import::folder_scanner::StoredCandidateEdits::none(),
            )
            .unwrap();
        let disc_id = crate::import::discid::compute_discid_from_categorized(&categorized);
        let track_count = categorized.track_count();

        assert!(disc_id.is_some(), "LOG fixture should produce a disc ID");
        assert_eq!(
            track_count, 2,
            "track_count must equal the number of audio files, not 0"
        );
    }
}
