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
//! Year comes from any tag carrying a date. Source codecs are physical audio
//! facts, not release media, so the pressing format stays blank.

use super::assemble::{
    assemble_parsed_album, AlbumArtistScope, ArtistRef, ReleaseIr, TrackEvent, TrackIr, TrackNumber,
};
use super::file_tag_snapshot::{
    extract_file_tag_snapshot, non_empty, FileTagFact, FileTagSnapshot, LoftyFileTagReader,
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
            let modified = std::fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .map_err(|error| ImportError::FileTags {
                    detail: format!(
                        "failed to read modification time of {}: {error}",
                        path.display()
                    ),
                })?;
            let modified_at_ns = i64::try_from(
                modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| ImportError::FileTags {
                        detail: format!(
                            "modification time of {} is before the Unix epoch",
                            path.display()
                        ),
                    })?
                    .as_nanos(),
            )
            .map_err(|_| ImportError::FileTags {
                detail: format!(
                    "modification time of {} exceeds SQLite's integer range",
                    path.display()
                ),
            })?;
            Ok(ScannedFile::new(
                path.clone(),
                relative_path,
                size,
                modified_at_ns,
            ))
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
    Ok(assemble_parsed_album(
        file_tag_facts_ir(extracted, folder_name)?,
        clock,
        ids,
    ))
}

fn file_tag_facts_ir(
    extracted: &[FileTagFact],
    folder_name: Option<&str>,
) -> Result<ReleaseIr, ImportError> {
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

    Ok(file_tag_release_ir(
        album_title,
        &album_artist_name,
        year,
        tracks,
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
            format: None,
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

    use super::track_slots::{audio_layout, UnitContribution};
    let layout = audio_layout(categorized);
    let loose_facts = layout
        .iter()
        .filter(|(_, contribution)| matches!(contribution, UnitContribution::Whole))
        .map(|(file, _)| {
            snapshot
                .files
                .iter()
                .find(|fact| fact.observation.relative_path == file.relative_path)
                .expect("snapshot file IDs were checked against candidate audio")
                .clone()
        })
        .collect::<Vec<_>>();
    let sheets = layout
        .iter()
        .flat_map(|(_, contribution)| match contribution {
            UnitContribution::Runs(sheets) => {
                sheets.iter().map(|bound| bound.sheet).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    if sheets.is_empty() {
        return map_file_tag_facts_to_db(&loose_facts, folder_name, clock, ids);
    }
    let mut release = cue_sheets_ir(&sheets, folder_name)?;
    let mut loose_tracks = if loose_facts.is_empty() {
        Vec::new()
    } else {
        file_tag_facts_ir(&loose_facts, folder_name)?.tracks
    }
    .into_iter();
    let mut tracks = Vec::new();
    for (_, contribution) in layout {
        match contribution {
            UnitContribution::Whole => {
                tracks.push(loose_tracks.next().expect("one track per loose audio file"))
            }
            UnitContribution::Runs(sheets) => {
                for bound in sheets {
                    let side = i32::try_from(
                        bound
                            .disc_number()
                            .expect("carving sheet has a disc number"),
                    )
                    .map_err(|_| ImportError::FileTags {
                        detail: "CUE disc number exceeds supported range".into(),
                    })?;
                    tracks.extend(
                        bound
                            .sheet
                            .playable_tracks()
                            .map(|track| cue_track_ir(track, side)),
                    );
                }
            }
            UnitContribution::SpokenFor => {}
        }
    }
    release.tracks = tracks;
    Ok(assemble_parsed_album(release, clock, ids))
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
    folder_name: Option<&str>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    map_cue_sheets(sheets, folder_name, clock, ids)
}

fn map_cue_sheets(
    sheets: &[&CueSheet],
    folder_name: Option<&str>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    Ok(assemble_parsed_album(
        cue_sheets_ir(sheets, folder_name)?,
        clock,
        ids,
    ))
}

fn cue_sheets_ir(
    sheets: &[&CueSheet],
    folder_name: Option<&str>,
) -> Result<ReleaseIr, ImportError> {
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
            tracks.push(cue_track_ir(track, side));
        }
    }

    Ok(file_tag_release_ir(
        album_title,
        &album_artist_name,
        year,
        tracks,
    ))
}

fn cue_track_ir(track: &crate::cue_flac::CueTrack, side: i32) -> TrackIr {
    TrackIr {
        title: non_empty(track.title.clone()).unwrap_or_default(),
        side,
        number: TrackNumber::PerSide,
        source_position: None,
        events: file_tag_credit_events(non_empty(track.performer.clone()).as_deref()),
    }
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
