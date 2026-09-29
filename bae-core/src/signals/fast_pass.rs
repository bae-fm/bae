//! The fast pass: every signal a folder yields without OCR, gathered in one
//! blocking call, plus the artwork to read afterwards.

use super::candidate_text::{extract_folder_brackets, parse_filename_stem, Source, SourcedLine};
use crate::barcode::Barcode;
use crate::import::discid::read_rip_artifacts;
use crate::import::folder_scanner::CategorizedFiles;
use crate::signals::{AudioFacts, AudioOrigin, AudioSource, DiscIdSignal, DownloadProof, SourcedValue};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Maximum size read from a single `.txt` file.
const MAX_TEXT_FILE_BYTES: u64 = 100 * 1024;

/// One image the artwork pass will read: the path the analyzer opens, and the
/// candidate-relative id a barcode read off it names.
#[derive(Debug, Clone)]
pub(super) struct ArtworkImage {
    pub(super) path: PathBuf,
    /// `None` for a library release's stored cover.
    pub(super) file_id: Option<String>,
}

pub(super) struct FastPass {
    pub(super) lines: Vec<SourcedLine>,
    pub(super) bracket_catalogs: Vec<String>,
    pub(super) artwork: Vec<ArtworkImage>,
    pub(super) origin: AudioOrigin,
    pub(super) disc_id: DiscIdSignal,
    pub(super) cue_barcodes: Vec<SourcedValue>,
    pub(super) audio: AudioFacts,
    /// Each audio file's ISRC, as its tags carry it.
    pub(super) isrcs: Vec<String>,
    /// Each track's title, as its file's tags or name give it.
    pub(super) track_titles: Vec<String>,
}

impl FastPass {
    /// What a failed folder scan yields: nothing.
    pub(super) fn empty() -> Self {
        Self {
            lines: Vec::new(),
            bracket_catalogs: Vec::new(),
            artwork: Vec::new(),
            origin: AudioOrigin::default(),
            disc_id: DiscIdSignal::Absent,
            cue_barcodes: Vec::new(),
            audio: AudioFacts::default(),
            isrcs: Vec::new(),
            track_titles: Vec::new(),
        }
    }
}

/// CUE `CATALOG` barcodes from every parsed sheet, one per sheet that states
/// a code; a field that holds no code is left out (see [`Barcode::stated`]).
fn cue_barcodes(categorized: &CategorizedFiles) -> Vec<SourcedValue> {
    categorized
        .track_sheets()
        .filter_map(|sheet| {
            let catalog = sheet.sheet.catalog.as_deref()?;
            let Some(code) = Barcode::stated(catalog) else {
                debug!(
                    sheet = %sheet.file.relative_path,
                    catalog, "a CUE CATALOG field holds no UPC or EAN"
                );
                return None;
            };
            Some(SourcedValue::in_file(
                code.into_string(),
                sheet.file.relative_path.clone(),
            ))
        })
        .collect()
}

/// Read every non-OCR source. Blocking. A missing text input is skipped;
/// invalid audio timing aborts, since every later track layout needs it.
pub(super) fn gather_non_ocr_sources(
    folders: &[PathBuf],
    categorized: &CategorizedFiles,
) -> Result<FastPass, crate::import::ImportError> {
    let mut pass = FastPass::empty();

    // Every selected source contributes its folder and parent names.
    for folder in folders {
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

        // The name goes in as written; the classifier reduces it itself.
        for raw in [&parent_name, &folder_name] {
            if raw.is_empty() {
                continue;
            }
            pass.lines
                .push(SourcedLine::new(Source::PathComponent, raw.clone()));
            for bracket in extract_folder_brackets(raw) {
                pass.bracket_catalogs.push(bracket);
            }
        }
    }

    // Rip evidence, disc ID, CUE barcodes and the audio facts all come off
    // one scan.
    pass.audio = AudioFacts::of_files(categorized)?;
    let rip = read_rip_artifacts(categorized);
    pass.origin = rip.origin;
    pass.disc_id = rip.disc_id.into_signal();
    pass.cue_barcodes = cue_barcodes(categorized);

    // Image + document filenames only; `enumerate_filename_inputs` explains why.
    for path in enumerate_filename_inputs(categorized) {
        for part in parse_filename_stem(&path) {
            pass.lines.push(SourcedLine::new(
                Source::FilenameGeneric { path: path.clone() },
                part,
            ));
        }
    }

    // PERFORMER / TITLE from every sheet the scan already parsed.
    for sheet in categorized.track_sheets() {
        for name in cue_sheet_names(sheet.sheet) {
            pass.lines.push(SourcedLine::new(
                Source::CueField {
                    file_id: sheet.file.relative_path.clone(),
                },
                name,
            ));
        }
    }

    // The audio's own tags: their album, artists and label feed the pool, and
    // their ISRCs are looked up. A reading that cannot be had is skipped like
    // any other unreadable text.
    let audio_files: Vec<_> = categorized.audio().cloned().collect();
    match crate::import::file_tag_snapshot::extract_file_tag_snapshot(
        &audio_files,
        0,
        0,
        &crate::import::file_tag_snapshot::LoftyFileTagReader,
    ) {
        Ok(tags) => {
            pass.isrcs = tags.isrcs();
            pass.track_titles = track_titles(categorized, &tags);
            // A file a ripper made reading a disc outweighs a tag, which can
            // be copied.
            if pass.origin.source.is_none() {
                pass.origin.source =
                    download_proof(&tags, categorized).map(AudioSource::Download);
            }
            let mut seen = HashSet::new();
            for fact in &tags.files {
                for value in [
                    &fact.album_title,
                    &fact.album_artist,
                    &fact.track_artist,
                    &fact.label,
                ]
                .into_iter()
                .flatten()
                {
                    if seen.insert(value.clone()) {
                        pass.lines.push(SourcedLine::new(
                            Source::FileTag {
                                file_id: fact.observation.relative_path.clone(),
                            },
                            value.clone(),
                        ));
                    }
                }
            }
        }
        Err(error) => warn!("the audio's tags could not be read, so they say nothing: {error}"),
    }

    // Text files feed the pool one line at a time.
    for path in text_file_paths(categorized) {
        if let Some(text) = read_capped_text(&path) {
            for line in text.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    pass.lines.push(SourcedLine::new(
                        Source::TextFile { path: path.clone() },
                        trimmed.to_string(),
                    ));
                }
            }
        }
    }

    pass.artwork = categorized
        .artwork()
        .map(|f| ArtworkImage {
            path: f.path.clone(),
            file_id: Some(f.relative_path.clone()),
        })
        .collect();

    Ok(pass)
}

/// The filenames the classifier reads: artwork and documents. Audio stems are
/// track titles, and a sheet's own names are already read from its fields.
/// Each track's title, in the tracks' order: its file's title tag, or failing
/// that its file name without the track number, as the text pool reads a file
/// name.
///
/// Empty when a sheet carves tracks out of one file, which no file's title
/// names; when a track has no title either way; and when the files' own
/// numbers do not put them in the order they are laid out in — each file's
/// disc and track tags, or failing a track tag the number its name starts
/// with, rising from one file to the next. A folder whose files are not
/// numbered lays them out in an order nothing states, and titles in that
/// order would tell a tracklist listing them otherwise apart for no reason.
fn track_titles(
    categorized: &CategorizedFiles,
    tags: &crate::import::file_tag_snapshot::FileTagSnapshot,
) -> Vec<String> {
    let mut titles = Vec::new();
    let mut previous: Option<(u32, u32)> = None;
    for unit in crate::import::audio_layout::audio_units(categorized) {
        let crate::import::AudioFile::Standalone { file_id } = unit else {
            return Vec::new();
        };
        let fact = tags
            .files
            .iter()
            .find(|fact| fact.observation.relative_path == file_id);
        let Some(number) = fact
            .and_then(|fact| fact.track_number)
            .or_else(|| leading_number(&file_id))
        else {
            return Vec::new();
        };
        let position = (fact.and_then(|fact| fact.disc_number).unwrap_or(0), number);
        if previous.is_some_and(|previous| previous >= position) {
            return Vec::new();
        }
        previous = Some(position);
        let tagged = fact
            .and_then(|fact| fact.title.as_deref())
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string);
        let Some(title) =
            tagged.or_else(|| parse_filename_stem(Path::new(&file_id)).into_iter().next())
        else {
            return Vec::new();
        };
        titles.push(title);
    }
    titles
}

/// The number a file's name starts with, as a track number: `01 - Title`.
fn leading_number(file_id: &str) -> Option<u32> {
    let name = Path::new(file_id).file_name()?.to_str()?.trim_start();
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    (1..=3).contains(&digits.len()).then(|| digits.parse().ok()).flatten()
}

fn enumerate_filename_inputs(categorized: &CategorizedFiles) -> Vec<PathBuf> {
    categorized
        .artwork()
        .chain(categorized.documents())
        .map(|f| f.path.clone())
        .collect()
}

/// A sheet's album- and track-level PERFORMER / TITLE values, deduped.
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

/// `.txt` documents only; logs hold no names and sheets are read parsed.
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

/// Read a text file, capped and decoded whatever its encoding; `None` only on
/// an I/O error, which is logged.
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

/// What proves the audio a download, from its tags: a store's own marker on a
/// track, or on every track what a label delivers with a download — an ISRC,
/// a phonographic copyright line and the label — where no rip document or
/// track sheet says a disc was read.
fn download_proof(
    tags: &crate::import::file_tag_snapshot::FileTagSnapshot,
    categorized: &CategorizedFiles,
) -> Option<DownloadProof> {
    if let Some((marker, file)) = tags.files.iter().find_map(|fact| {
        fact.store
            .map(|marker| (marker, fact.observation.relative_path.clone()))
    }) {
        return Some(DownloadProof::Store { marker, file });
    }
    let read_off_a_disc = categorized.files.iter().any(|entry| {
        crate::import::discid::is_rip_document(&entry.file.path)
            || has_ext(&entry.file.path, "cue")
    });
    let delivered = !tags.files.is_empty()
        && tags.files.iter().all(|fact| {
            fact.isrc.is_some()
                && fact.label.is_some()
                && fact
                    .copyright
                    .as_deref()
                    .is_some_and(super::rip::states_phonographic_copyright)
        });
    (delivered && !read_off_a_disc).then_some(DownloadProof::DeliverySet)
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
            1,
        );
        let flac = ScannedFile::new(
            PathBuf::from("/rel/Album.flac"),
            "Album.flac".to_string(),
            5_000_000,
            1,
        )
        .with_test_flac_audio();
        let cover = ScannedFile::new(
            PathBuf::from("/rel/Artist Name - Album.png"),
            "Artist Name - Album.png".to_string(),
            10_000,
            1,
        );
        let categorized = CategorizedFiles {
            files: vec![
                CandidateFile {
                    file: cue.clone(),
                    role: FileRole::TrackSheet {
                        sheet: crate::cue_flac::CueSheet {
                            title: None,
                            performer: None,
                            catalog: None,
                            date: None,
                            ripper: None,
                            tracks: Vec::new(),
                        },
                        binding: crate::import::folder_scanner::SheetBinding::Resolved {
                            files: vec![crate::import::folder_scanner::SheetAudioFile {
                                file_reference: "Album.wav".to_string(),
                                file_id: "Album.flac".to_string(),
                            }],
                        },
                        disc: crate::import::folder_scanner::SheetDisc::Disc { number: 1 },
                    },
                },
                CandidateFile {
                    file: flac.clone(),
                    role: FileRole::Audio,
                },
                CandidateFile {
                    file: cover.clone(),
                    role: FileRole::Artwork,
                },
            ],
            parts: Vec::new(),
        };

        let inputs: Vec<PathBuf> = enumerate_filename_inputs(&categorized);
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

        let fixture_log = std::path::Path::new("tests/fixtures/logs/test_album.log");
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
        let disc_id = crate::import::discid::read_rip_artifacts(&categorized)
            .disc_id
            .computed();
        let track_count = categorized.track_count();

        assert!(disc_id.is_some(), "LOG fixture should produce a disc ID");
        assert_eq!(
            track_count, 2,
            "track_count must equal the number of audio files, not 0"
        );
    }
}
