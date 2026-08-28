//! Projects stored file-tag readings into editable import metadata.
//!
//! Reads embedded ID3v1/ID3v2/Vorbis-comment/MP4-ilst tags from a rip's audio
//! files and projects them into the same `ParsedAlbum` shape that
//! `map_mb_response_to_db` and `map_discogs_to_db` produce. The editable
//! confirmation page lets the user correct anything the tags got wrong.
//!
//! `ParsedAlbum::identities` is always empty because file tags make no external
//! identity claim. The release provenance is `FileTags`. Lookup signals
//! such as OCR, DiscID, and barcode are not part of this path.
//!
//! Format comes from the probed codec, year from any tag carrying a date. Both
//! stay `None` when not determinable rather than being defaulted.

use super::assemble::{
    assemble_parsed_album, AlbumArtistScope, ArtistRef, ReleaseIr, TrackEvent, TrackIr, TrackNumber,
};
use super::file_tag_snapshot::{
    extract_file_tag_snapshot, non_empty, probe_content_type, FileTagFact, FileTagSnapshot,
    LoftyFileTagReader,
};
use super::ParsedAlbum;
use crate::cue_flac::CueSheet;
use crate::db::Pressing;
use crate::import::folder_scanner::{CategorizedFiles, ScannedFile};
use crate::import::ImportError;
#[cfg(test)]
use crate::util::content_type::ContentType;
use coven::Clock;
use coven::IdProvider;
use std::path::{Path, PathBuf};

pub use super::file_tag_snapshot::read_embedded_cover;

/// Map the embedded tags of a rip's audio files to a `ParsedAlbum`.
///
/// `audio_files` is the rip's audio files in order; that order is the fallback
/// track ordering when DISCNUMBER/TRACKNUMBER tags are absent or partial.
/// `folder_name` is the rip's containing folder — the album-title fallback when
/// no file carries an ALBUM tag.
///
/// Seeds whatever the files carry and leaves the rest for the user. Missing
/// album-level fields become editable blanks, never errors: the album title
/// falls back to `folder_name` then to empty, the artist to empty, a missing
/// track TITLE to the file stem. The form's save-gate (`RawReleaseEdit::shape` →
/// `EmptyAlbumTitle` / `NoAlbumArtist`) makes the user fill any blank before
/// committing, so no fabricated default is ever written.
///
/// Errors only when `audio_files` is empty (an album has at least one track) or
/// a file fails to open or have its tags read.
pub fn map_file_tags_to_db(
    audio_files: &[PathBuf],
    folder_name: Option<&str>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    if audio_files.is_empty() {
        return Err(ImportError::FileTags {
            detail: "file-tag seeding requires at least one audio file".to_string(),
        });
    }

    let scanned = audio_files
        .iter()
        .map(|path| -> Result<ScannedFile, ImportError> {
            let size = std::fs::metadata(path)
                .map_err(|error| ImportError::FileTags {
                    detail: format!("failed to stat {}: {error}", path.display()),
                })?
                .len();
            let relative_path = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .ok_or_else(|| ImportError::FileTags {
                    detail: format!("audio file path {} has no filename", path.display()),
                })?;
            Ok(ScannedFile::new(path.clone(), relative_path, size))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let snapshot = extract_file_tag_snapshot(&scanned, 0, 0, &LoftyFileTagReader)?;
    map_file_tag_facts_to_db(&snapshot.files, folder_name, clock, ids)
}

fn map_file_tag_facts_to_db(
    extracted: &[FileTagFact],
    folder_name: Option<&str>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    if extracted.is_empty() {
        return Err(ImportError::FileTags {
            detail: "file-tag seeding requires at least one audio file".to_string(),
        });
    }

    // A blank title here is a prompt to the user, not a committed value — the
    // editable form gates save on a non-empty one.
    let album_title = extracted
        .iter()
        .find_map(|t| t.album_title.as_ref())
        .cloned()
        .or_else(|| folder_name.map(str::to_string))
        .unwrap_or_default();

    // ALBUMARTIST, else ARTIST — rippers commonly populate only ARTIST for a
    // single-artist album. No folder-name fallback: the folder name is the album,
    // not a reliable artist.
    let album_artist_name = extracted
        .iter()
        .find_map(|t| t.album_artist.as_ref())
        .or_else(|| extracted.iter().find_map(|t| t.track_artist.as_ref()))
        .cloned()
        .unwrap_or_default();

    let year = extracted.iter().find_map(|t| t.year).map(|y| y as i32);

    // The rip's actual codec, not editorial pressing info. A failed probe leaves
    // the editable field blank rather than guessing from the extension.
    let format = extracted[0]
        .content_type
        .as_ref()
        .map(|content_type| content_type.display_name().to_string());

    // Side comes from DISCNUMBER, track_number from TRACKNUMBER. The positional
    // fallback (index within side, by file order) applies only on a side where NO
    // file is tagged: backfilling a position onto an untagged file that shares a
    // side with tagged ones would collide with the real values (an untagged file
    // landing on position 1 beside a TRACKNUMBER=1 file). On a partially-tagged
    // side the untagged files stay `None` for the user to assign.
    let side_of = |t: &FileTagFact| match t.disc_number {
        Some(0) | None => 1,
        Some(d) if d > i32::MAX as u32 => i32::MAX,
        Some(d) => d as i32,
    };
    let mut side_has_tagged_track: std::collections::HashMap<i32, bool> =
        std::collections::HashMap::new();
    for t in extracted.iter() {
        let entry = side_has_tagged_track.entry(side_of(t)).or_insert(false);
        *entry = *entry || t.track_number.is_some();
    }

    let tracks: Vec<TrackIr> = extracted
        .iter()
        .map(|t| {
            // The scanner only admits files with a recognised audio extension, so
            // every path here has a stem. A `None` would mean that invariant broke
            // upstream — panic rather than fabricate a "Track N" placeholder.
            let title = t.title.clone().unwrap_or_else(|| {
                Path::new(&t.observation.relative_path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .expect("scanned audio file relative path has a stem")
            });

            let side = side_of(t);
            let side_has_tagged = side_has_tagged_track.get(&side).copied().unwrap_or(false);
            let number = match t.track_number {
                Some(0) => TrackNumber::Explicit(None),
                Some(n) if n > i32::MAX as u32 => TrackNumber::Explicit(None),
                Some(n) => TrackNumber::Explicit(Some(n as i32)),
                // Untagged file on a side that has tagged siblings — leave it
                // for the user rather than backfill a colliding position.
                None if side_has_tagged => TrackNumber::Explicit(None),
                // Fully-untagged side — positional by file order, numbered by
                // the assembler's per-side pass.
                None => TrackNumber::PerSide,
            };

            TrackIr {
                title,
                side,
                number,
                source_position: None,
                events: file_tag_credit_events(t.track_artist.as_deref()),
            }
        })
        .collect();

    Ok(assemble_parsed_album(
        file_tag_release_ir(album_title, &album_artist_name, year, format, tracks),
        clock,
        ids,
    ))
}

/// An [`ArtistRef`] for a file-tag ARTIST/PERFORMER value. The source provides
/// only a display name here, so the sort name and source artist ids stay absent.
fn file_tag_artist_ref(name: &str) -> ArtistRef {
    ArtistRef {
        name: name.to_string(),
        sort_name: None,
        musicbrainz_artist_id: None,
        discogs_artist_id: None,
    }
}

/// The track's credit events: a single display credit at position 0 when the
/// source carries an ARTIST/PERFORMER, else none. The junction row is emitted
/// whenever a name is present, regardless of whether it matches the album
/// artist — the junction is the source of truth for per-track credits.
fn file_tag_credit_events(artist: Option<&str>) -> Vec<TrackEvent> {
    match artist {
        Some(name) => vec![TrackEvent::Credit {
            position: 0,
            artist: file_tag_artist_ref(name),
        }],
        None => Vec::new(),
    }
}

/// The [`ReleaseIr`] shared by the file-tag and CUE-sheet seeders. `identities`
/// is always empty (File Tags makes no external identity claim); provenance is
/// `FileTags`; `album_artist_scope` is `FullPool` so a divergent per-track
/// artist also becomes an album artist.
fn file_tag_release_ir(
    album_title: String,
    album_artist_name: &str,
    year: Option<i32>,
    format: Option<String>,
    tracks: Vec<TrackIr>,
) -> ReleaseIr {
    ReleaseIr {
        album_title,
        primary_artist: file_tag_artist_ref(album_artist_name),
        additional_artists: Vec::new(),
        album_year: year,
        is_compilation: false,
        pressing: Pressing {
            year,
            format,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        metadata_provenance: Some(crate::import::MetadataProvenance::FileTags),
        album_artist_scope: AlbumArtistScope::FullPool,
        release_roles: Vec::new(),
        tracks,
        identities: Vec::new(),
    }
}

pub(crate) fn map_file_tag_snapshot_to_db(
    categorized: &CategorizedFiles,
    snapshot: &FileTagSnapshot,
    folder_name: Option<&str>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    let audio_file_ids = categorized
        .audio()
        .map(|file| file.relative_path.as_str())
        .collect::<Vec<_>>();
    let snapshot_file_ids = snapshot
        .files
        .iter()
        .map(|fact| fact.observation.relative_path.as_str())
        .collect::<Vec<_>>();
    if audio_file_ids != snapshot_file_ids {
        return Err(ImportError::FileTags {
            detail: "file-tag snapshot does not describe the candidate's current audio files"
                .to_string(),
        });
    }

    let mut carving = categorized.carving_sheets();
    if carving.is_empty() {
        return map_file_tag_facts_to_db(&snapshot.files, folder_name, clock, ids);
    }
    // Sorted by the disc each sheet is assigned to, not by the name it or its
    // container happens to carry, so this tracklist comes out in the same order
    // `track_slots::audio_units` lays the folder's audio down — the two are
    // zipped into track slots.
    carving.sort_by(|a, b| {
        a.disc_number()
            .cmp(&b.disc_number())
            .then_with(|| natord::compare_ignore_case(&a.file.relative_path, &b.file.relative_path))
    });
    let sheets = carving.iter().map(|b| b.sheet).collect::<Vec<_>>();
    let first_carving_audio = carving[0].audio.relative_path.as_str();
    let format = snapshot
        .files
        .iter()
        .find(|fact| fact.observation.relative_path == first_carving_audio)
        .and_then(|fact| fact.content_type.as_ref())
        .map(|content_type| content_type.display_name().to_string());
    map_cue_sheets_with_format(&sheets, folder_name, format, clock, ids)
}

/// Map a CUE-backed rip's parsed sheets to a [`ParsedAlbum`] for the File Tags
/// path. Where [`map_file_tags_to_db`] seeds one track per file, here the track
/// structure comes from the playable CUE `TRACK` entries: title from each
/// `TITLE`, per-track artist from each `PERFORMER`. Album-level fields come from
/// the sheet header (`TITLE` / `PERFORMER` / `REM DATE`), the title falling back
/// to the folder name. `sheets` and `audio_files` are one-per-pair in disc order
/// — the same order `track_slots` lays the folder's audio down — so side is the
/// 1-based disc index and track numbers run per sheet.
pub fn map_cue_sheets_to_db(
    sheets: &[&CueSheet],
    audio_files: &[&Path],
    folder_name: Option<&str>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    let format = audio_files
        .first()
        .and_then(|path| probe_content_type(path))
        .map(|content_type| content_type.display_name().to_string());
    map_cue_sheets_with_format(sheets, folder_name, format, clock, ids)
}

fn map_cue_sheets_with_format(
    sheets: &[&CueSheet],
    folder_name: Option<&str>,
    format: Option<String>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    if sheets.is_empty() {
        return Err(ImportError::FileTags {
            detail: "CUE seeding requires at least one sheet".to_string(),
        });
    }

    // Blank is allowed — the editable File Tags form gates save on a title.
    let album_title = sheets
        .iter()
        .find_map(|s| non_empty(s.title.clone()))
        .or_else(|| folder_name.map(str::to_string))
        .unwrap_or_default();

    let album_artist_name = sheets
        .iter()
        .find_map(|s| non_empty(s.performer.clone()))
        .unwrap_or_default();

    let year = sheets
        .iter()
        .find_map(|s| year_from_cue_date(s.date.as_deref()));

    // Each sheet is one side and its playable tracks are already in order, so
    // per-side numbering by the assembler reproduces `position + 1` exactly.
    let mut tracks: Vec<TrackIr> = Vec::new();
    for (disc_index, sheet) in sheets.iter().enumerate() {
        let side = disc_index as i32 + 1;
        for track in sheet.playable_tracks() {
            // A CUE track without a TITLE is rare; seed a blank for the user
            // rather than fabricate a placeholder.
            let title = non_empty(track.title.clone()).unwrap_or_default();
            tracks.push(TrackIr {
                title,
                side,
                number: TrackNumber::PerSide,
                source_position: None,
                events: file_tag_credit_events(non_empty(track.performer.clone()).as_deref()),
            });
        }
    }

    Ok(assemble_parsed_album(
        file_tag_release_ir(album_title, &album_artist_name, year, format, tracks),
        clock,
        ids,
    ))
}

/// Parse a year from a CUE `REM DATE` value. Rippers write a bare year
/// ("1970"), a range ("2000 / 2004"), or a full date; take the first 4-digit
/// run.
fn year_from_cue_date(date: Option<&str>) -> Option<i32> {
    let digits = date?
        .split(|c: char| !c.is_ascii_digit())
        .find(|part| part.len() >= 4)?;
    Some(
        digits[..4]
            .parse::<i32>()
            .expect("four ASCII digits parse as i32"),
    )
}

#[cfg(test)]
#[path = "file_tag_mapper_tests.rs"]
mod tests;
