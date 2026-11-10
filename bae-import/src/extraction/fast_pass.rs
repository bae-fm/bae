//! The fast pass: every signal a folder yields without OCR, gathered in one
//! blocking call, plus the artwork to read afterwards.

use super::candidate_text::{extract_folder_brackets, parse_filename_stem, Source, SourcedLine};
use crate::folder_scanner::{CategorizedFiles, FileRole};
use crate::rip_reading::read_rip_artifacts;
use bae_catalogs::{AudioOrigin, AudioSource, DownloadProof};
use bae_identify::signals::{AudioFacts, DiscIdSignal, SourcedValue};
use bae_records::DocumentKind;
use bae_types::barcode::Barcode;
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
            Some(SourcedValue::in_file(code.into_string(), sheet.file.relative_path.clone()))
        })
        .collect()
}

/// Read every non-OCR source. Blocking. A missing text input is skipped;
/// invalid audio timing aborts, since every later track layout needs it.
pub(super) fn gather_non_ocr_sources(
    folders: &[PathBuf],
    categorized: &CategorizedFiles,
) -> Result<FastPass, crate::ImportError> {
    let mut pass = FastPass::empty();

    // Every selected source contributes its folder and parent names, as
    // they are shown: a name that isn't UTF-8 is escaped, not dropped.
    let shown_name = |path: Option<&Path>| {
        path.and_then(Path::file_name).map(|name| coven::path_text(Path::new(name)))
    };
    for folder in folders {
        let folder_name = shown_name(Some(folder)).unwrap_or_default();
        let parent_name = shown_name(folder.parent()).unwrap_or_default();

        // The name goes in as written; the classifier reduces it itself.
        for raw in [&parent_name, &folder_name] {
            if raw.is_empty() {
                continue;
            }
            pass.lines.push(SourcedLine::new(Source::PathComponent, raw.clone()));
            for bracket in extract_folder_brackets(raw) {
                pass.bracket_catalogs.push(bracket);
            }
        }
    }

    // Rip evidence, disc ID, CUE barcodes and the audio facts all come off
    // one scan.
    pass.audio = crate::probe::audio_facts(categorized)?;
    let rip = read_rip_artifacts(categorized);
    pass.origin = rip.origin;
    pass.disc_id = rip.disc_id.into_signal();
    pass.cue_barcodes = cue_barcodes(categorized);

    // Image + document filenames only; `enumerate_filename_inputs` explains why.
    for path in enumerate_filename_inputs(categorized) {
        for part in parse_filename_stem(&path) {
            pass.lines.push(SourcedLine::new(Source::FilenameGeneric { path: path.clone() }, part));
        }
    }

    // PERFORMER / TITLE from every sheet the scan already parsed.
    for sheet in categorized.track_sheets() {
        for name in cue_sheet_names(sheet.sheet) {
            pass.lines.push(SourcedLine::new(
                Source::CueField { file_id: sheet.file.relative_path.clone() },
                name,
            ));
        }
    }

    // The audio's own tags: their album, artists and label feed the pool, and
    // their ISRCs are looked up. A file whose tags do not parse states nothing;
    // a reading that cannot be had is skipped like any other unreadable text.
    let audio_files: Vec<_> = categorized.audio().cloned().collect();
    match crate::file_tag_snapshot::read_delivered_file_tags(
        &audio_files,
        &crate::file_tag_reader::LoftyFileTagReader,
    ) {
        Ok(files) => {
            // Each file's ISRCs, as `isrc::code` reads its tag, in the files'
            // order; a value that reads as no code adds nothing.
            pass.isrcs = files
                .iter()
                .flat_map(|file| &file.delivery.isrcs)
                .filter_map(|isrc| bae_catalogs::isrc::code(isrc))
                .collect();
            pass.track_titles = track_titles(categorized, &files);
            // A file a ripper made reading a disc outweighs a tag, which can
            // be copied.
            if pass.origin.source.is_none() {
                pass.origin.source = download_proof(&files, categorized).map(AudioSource::Download);
            }
            let mut seen = HashSet::new();
            for file in &files {
                let tagged = file.fact.tags.stated();
                for value in tagged
                    .album_title
                    .iter()
                    .chain(&tagged.album_artists)
                    .chain(&tagged.track_artists)
                    .chain(&file.delivery.labels)
                {
                    if seen.insert(value.clone()) {
                        pass.lines.push(SourcedLine::new(
                            Source::FileTag {
                                file_id: file.fact.observation.relative_path.clone(),
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
            path: f.version.path.clone(),
            file_id: Some(f.relative_path.clone()),
        })
        .collect();

    Ok(pass)
}

/// Each track's title, in the tracks' order: its file's title tag, or failing
/// that its file name without the track number, as the text pool reads a file
/// name.
///
/// Empty when a sheet carves tracks out of one file, which no file's title
/// names; when a track has no title either way; and when the files' own
/// numbers do not rise from one file to the next — each file's disc and
/// track tags, or the numbers its name starts with where its tags state none
/// (`TrackNumbers::or_file_name`). A folder whose files are not
/// numbered lays them out in an order nothing states, and titles in that
/// order would tell a tracklist listing them otherwise apart for no reason.
fn track_titles(
    categorized: &CategorizedFiles,
    files: &[crate::file_tag_snapshot::DeliveredFileTags],
) -> Vec<String> {
    let mut titles = Vec::new();
    let mut previous: Option<(Option<u32>, u32)> = None;
    for unit in crate::audio_layout::audio_units(categorized) {
        let crate::AudioFile::Standalone { file_id } = unit else {
            return Vec::new();
        };
        let fact = files
            .iter()
            .map(|file| &file.fact)
            .find(|fact| fact.observation.relative_path == file_id);
        let numbers = match fact {
            Some(fact) => fact.tags.stated().numbers,
            None => crate::track_numbers::TrackNumbers::unstated(),
        }
        .or_file_name(&file_id);
        let Some(number) = numbers.track else {
            return Vec::new();
        };
        let position = (numbers.disc, number);
        if previous.is_some_and(|previous| previous >= position) {
            return Vec::new();
        }
        previous = Some(position);
        let tagged = fact
            .and_then(|fact| fact.tags.stated().title.as_deref())
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

/// The filenames the classifier reads: artwork and documents. Audio stems are
/// track titles, and a sheet's own names are already read from its fields.
fn enumerate_filename_inputs(categorized: &CategorizedFiles) -> Vec<PathBuf> {
    categorized.artwork().chain(categorized.documents()).map(|f| f.version.path.clone()).collect()
}

/// A sheet's album- and track-level PERFORMER / TITLE values, deduped.
fn cue_sheet_names(sheet: &crate::cue_sheet::CueSheet) -> Vec<String> {
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

/// Text documents only; logs hold no names, playlists only the audio's, and
/// sheets are read parsed.
fn text_file_paths(categorized: &CategorizedFiles) -> Vec<PathBuf> {
    categorized.documents_of(DocumentKind::Text).map(|f| f.version.path.clone()).collect()
}

/// Read a text file, capped and decoded whatever its encoding; `None` only on
/// an I/O error, which is logged.
fn read_capped_text(path: &Path) -> Option<String> {
    use std::io::{ErrorKind, Read};

    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            debug!("candidate_text: file vanished after scan: {}", path.display());
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
    Some(bae_util::text_encoding::decode_text(&buf).text)
}

/// What proves the audio a download, from its tags: a store's own marker on a
/// track, or on every track what a label delivers with a download — an ISRC,
/// a phonographic copyright line and the label — where no rip document or
/// track sheet says a disc was read.
fn download_proof(
    files: &[crate::file_tag_snapshot::DeliveredFileTags],
    categorized: &CategorizedFiles,
) -> Option<DownloadProof> {
    if let Some((marker, file)) = files.iter().find_map(|file| {
        file.delivery.store.map(|marker| (marker, file.fact.observation.relative_path.clone()))
    }) {
        return Some(DownloadProof::Store { marker, file });
    }
    let read_off_a_disc = categorized.files.iter().any(|entry| {
        matches!(
            entry.role,
            FileRole::TrackSheet { .. }
                | FileRole::Document {
                    kind: DocumentKind::CueSheet
                        | DocumentKind::RipLog
                        | DocumentKind::AccurateRipReport
                }
        )
    });
    let delivered = !files.is_empty()
        && files.iter().all(|file| {
            !file.delivery.isrcs.is_empty()
                && !file.delivery.labels.is_empty()
                && file
                    .delivery
                    .copyrights
                    .iter()
                    .any(|line| bae_catalogs::rip::states_phonographic_copyright(line))
        });
    (delivered && !read_off_a_disc).then_some(DownloadProof::DeliverySet)
}

#[cfg(test)]
#[path = "fast_pass_tests.rs"]
mod tests;
