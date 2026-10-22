//! Projects stored file-tag readings into editable import metadata.
//!
//! Reads embedded ID3v1/ID3v2/Vorbis-comment/MP4-ilst tags from a rip's audio
//! files and reads them into the same `ReleaseValues` a catalog release's
//! reading produces. The editable confirmation page lets the user correct
//! anything the tags got wrong.
//!
//! File tags name no catalog, so a release seeded from them carries no
//! records. The release provenance is `FileMetadata`. Lookup signals such as OCR,
//! DiscID, and barcode are not part of this path.
//!
//! Year comes from any tag carrying a date. Source codecs are physical audio
//! facts, not release media, so the pressing's media stay unstated.

use super::file_tag_snapshot::{
    extract_file_tag_snapshot, non_empty, FileTagFact, FileTagSnapshot, LoftyFileTagReader,
};
use super::release_values::{ReleaseValues, TrackCredits, TrackValues};
use crate::cue_flac::CueSheet;
use crate::import::folder_scanner::{CategorizedFiles, ScannedFile};
use crate::import::{ArtistCredit, ImportError};
use crate::pressing::Pressing;
#[cfg(test)]
use crate::util::content_type::ContentType;
use std::path::{Path, PathBuf};

/// Read the embedded tags of a rip's audio files.
///
/// `audio_files` is the rip's audio files in the order its tracks play; each
/// track is numbered by its file's DISCNUMBER/TRACKNUMBER tags, else by the
/// numbers its name starts with, else by its position.
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
pub(crate) fn read_file_tags(
    audio_files: &[PathBuf],
    folder_name: Option<&str>,
) -> Result<ReleaseValues, ImportError> {
    if audio_files.is_empty() {
        return Err(ImportError::FileTags {
            detail: "file-tag seeding requires at least one audio file".to_string(),
        });
    }

    let scanned = audio_files
        .iter()
        .map(|path| -> Result<ScannedFile, ImportError> {
            let metadata = std::fs::metadata(path)
                .map_err(|error| ImportError::file_tags("stat", path, error))?;
            let relative_path = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .ok_or_else(|| ImportError::FileTags {
                    detail: format!("audio file path {} has no filename", path.display()),
                })?;
            Ok(ScannedFile::new(
                path.clone(),
                relative_path,
                metadata.len(),
                super::file_tag_snapshot::modified_at_nanos(path, &metadata)?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let snapshot = extract_file_tag_snapshot(&scanned, 0, 0, &LoftyFileTagReader)?;
    file_tag_facts_values(&snapshot.files, folder_name)
}

fn file_tag_facts_values(
    extracted: &[FileTagFact],
    folder_name: Option<&str>,
) -> Result<ReleaseValues, ImportError> {
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

    let tracks: Vec<TrackValues> = extracted
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

            // The tags' numbers, else the ones the file's name starts with. A
            // file stating no disc claims no grouping; one stating no track is
            // numbered by its position.
            let numbers = t.numbers.or_file_name(&t.observation.relative_path);
            let side = numbers.disc.and_then(|number| i32::try_from(number).ok());
            let number = numbers.track.and_then(|number| i32::try_from(number).ok());

            TrackValues {
                title,
                side,
                number,
                stated_length_ms: None,
                credits: file_tag_credits(t.track_artist.as_deref()),
                roles: Vec::new(),
                works: Vec::new(),
            }
        })
        .collect();

    Ok(file_tag_release_values(
        album_title,
        &album_artist_name,
        year,
        tracks,
    ))
}

/// The credit of a file-tag ARTIST/PERFORMER value. The source provides only
/// a display name here, so the sort name and source artist ids stay absent.
fn file_tag_artist(name: &str) -> ArtistCredit {
    ArtistCredit {
        name: name.to_string(),
        sort_name: None,
        musicbrainz_artist_id: None,
        discogs_artist_id: None,
    }
}

/// The track's own artist when the source carries an ARTIST/PERFORMER,
/// whether or not it matches the album artist; else the album's.
fn file_tag_credits(artist: Option<&str>) -> TrackCredits {
    match artist {
        Some(name) => TrackCredits::Own(vec![file_tag_artist(name)]),
        None => TrackCredits::AlbumArtists,
    }
}

/// The values the file-tag and CUE-sheet readers share. A track artist the
/// album artist does not name is an album artist too, after it, once per
/// name.
fn file_tag_release_values(
    album_title: String,
    album_artist_name: &str,
    year: Option<i32>,
    tracks: Vec<TrackValues>,
) -> ReleaseValues {
    let mut album_credits = vec![file_tag_artist(album_artist_name)];
    for track in &tracks {
        let TrackCredits::Own(credits) = &track.credits else {
            continue;
        };
        for credit in credits {
            if !album_credits
                .iter()
                .any(|known| crate::text_match::same_artist_name(&known.name, &credit.name))
            {
                album_credits.push(credit.clone());
            }
        }
    }
    ReleaseValues {
        album_title,
        album_credits,
        album_year: year,
        pressing: Pressing {
            year,
            ..Pressing::blank()
        },
        release_roles: Vec::new(),
        works: Default::default(),
        tracks,
    }
}

/// What a folder's stored tag reading says its audio units are, one track per
/// unit in the folder's order: a loose file's tags for it, and a bound CUE
/// sheet's tracks for the slices it carves.
pub(crate) fn file_metadata_values(
    categorized: &CategorizedFiles,
    snapshot: &FileTagSnapshot,
    folder_name: Option<&str>,
) -> Result<ReleaseValues, ImportError> {
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

    use super::audio_layout::{audio_layout, UnitContribution};
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
        return file_tag_facts_values(&loose_facts, folder_name);
    }
    let release = cue_sheets_release(&sheets, folder_name);
    let mut loose_tracks = if loose_facts.is_empty() {
        Vec::new()
    } else {
        file_tag_facts_values(&loose_facts, folder_name)?.tracks
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
                    .map_err(|error| ImportError::FileTags {
                        detail: format!("CUE disc number exceeds supported range: {error}"),
                    })?;
                    tracks.extend(
                        bound
                            .sheet
                            .playable_tracks()
                            .map(|track| cue_track_values(track, side)),
                    );
                }
            }
            UnitContribution::SpokenFor => {}
        }
    }
    Ok(file_tag_release_values(
        release.album_title,
        &release.album_artist_name,
        release.year,
        tracks,
    ))
}

/// The album facts a folder's carving sheets state.
struct SheetsRelease {
    album_title: String,
    album_artist_name: String,
    year: Option<i32>,
}

/// The release a folder's carving sheets describe, before its tracks: the
/// album title from the first sheet's `TITLE` (the folder name when none has
/// one), the artist from the first `PERFORMER`, the year from the first
/// `REM DATE`. The tracks are the folder's layout's to lay down.
fn cue_sheets_release(sheets: &[&CueSheet], folder_name: Option<&str>) -> SheetsRelease {
    // Blank is allowed — the editable file-metadata form gates save on a title.
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
    SheetsRelease {
        album_title,
        album_artist_name,
        year,
    }
}

fn cue_track_values(track: &crate::cue_flac::CueTrack, side: i32) -> TrackValues {
    TrackValues {
        title: non_empty(track.title.clone()).unwrap_or_default(),
        side: Some(side),
        number: i32::try_from(track.number)
            .ok()
            .filter(|number| *number > 0),
        stated_length_ms: None,
        credits: file_tag_credits(non_empty(track.performer.clone()).as_deref()),
        roles: Vec::new(),
        works: Vec::new(),
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
