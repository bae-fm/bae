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
//! Album facts are what the files (or the CUE sheets) that state one say
//! together (`TaggedAlbum::of`): the value they all give, else the first
//! one's in play order, its title without a trailing disc marker. Each file's
//! dates name the album's year and the pressing's by the one year
//! rule (`StatedYears::of`). Source codecs are physical audio facts, not
//! release media, so the pressing's media stay unstated.

use super::file_tag_snapshot::{
    non_empty, years_in_date, FileTagFact, FileTagSnapshot, FileTags,
};
#[cfg(test)]
use super::file_tag_snapshot::{extract_file_tag_snapshot, LoftyFileTagReader};
use super::release_values::{ReleaseValues, TrackCredits, TrackValues};
use crate::cue_flac::CueSheet;
use crate::identify::row_facts::StatedYears;
use crate::import::folder_scanner::CategorizedFiles;
#[cfg(test)]
use crate::import::folder_scanner::ScannedFile;
use crate::import::{ArtistCredit, ImportError};
use crate::pressing::Pressing;
#[cfg(test)]
use bae_util::content_type::ContentType;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

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
/// falls back to `folder_name` where no file states one, the album artist is
/// none where no file names one, and a missing track TITLE falls back to the
/// file stem. The form's save-gate (`RawReleaseEdit::shape` →
/// `EmptyAlbumTitle` / `NoAlbumArtist`) makes the user fill any blank before
/// committing, so no fabricated default is ever written.
///
/// Errors only when `audio_files` is empty (an album has at least one track) or
/// a file fails to open or have its tags read.
///
/// The reader's tests reach it through here, with real tagged files; an import
/// reads a candidate's stored tag reading ([`file_metadata_values`]).
#[cfg(test)]
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
    let album = TaggedAlbum::of(
        extracted.iter().map(|fact| UnitAlbum::of_file(&fact.tags)),
        folder_name,
    );
    Ok(file_tag_release_values(album, tag_tracks(extracted)))
}

/// What one unit that states album facts — a file's tags, or a CUE sheet —
/// says of the album.
struct UnitAlbum {
    title: Option<String>,
    /// The album artists it names (ALBUMARTIST, a sheet's PERFORMER).
    album_artists: Vec<String>,
    /// The track artists it names (a file's ARTIST), which name the album's
    /// artists where no unit names an album artist.
    track_artists: Vec<String>,
    /// Whether it says the album is a compilation: a compilation flag, or the
    /// various-artists credit as its album artist.
    compilation: bool,
    years: StatedYears,
}

impl UnitAlbum {
    fn of_file(tags: &FileTags) -> Self {
        let years: Vec<i32> = tags.years.iter().copied().map(i32::from).collect();
        Self {
            title: tags.album_title.clone(),
            album_artists: tags.album_artists.clone(),
            track_artists: tags.track_artists.clone(),
            compilation: tags.compilation || names_various_artists(&tags.album_artists),
            years: StatedYears::of(&years, None),
        }
    }

    fn of_sheet(sheet: &CueSheet) -> Self {
        let album_artists: Vec<String> = non_empty(sheet.performer.clone()).into_iter().collect();
        let years: Vec<i32> = sheet
            .date
            .as_deref()
            .map(|date| years_in_date(date).map(i32::from).collect())
            .unwrap_or_default();
        Self {
            title: non_empty(sheet.title.clone()),
            compilation: names_various_artists(&album_artists),
            album_artists,
            track_artists: Vec::new(),
            years: StatedYears::of(&years, None),
        }
    }
}

/// Whether `artists` is the one credit a compilation is given.
fn names_various_artists(artists: &[String]) -> bool {
    matches!(artists, [only] if crate::text_match::is_various_artists(only))
}

/// What the folder's units, together, say of the album. Units that state a
/// value differently still give the album one, the first unit's: the discs
/// of one set tag `Album Title (Disc 1)` and `Album Title (Disc 2)`, and a
/// reissue's discs can carry different years.
struct TaggedAlbum {
    title: String,
    /// The album's artists, by name, in order; empty where no unit names
    /// one.
    artists: Vec<String>,
    /// Every unit says the album is a compilation: it is credited to the
    /// various-artists artist, and each track keeps its own artists.
    compilation: bool,
    years: StatedYears,
}

impl TaggedAlbum {
    /// The album `units` describe, given in the order they play (disc, then
    /// track). Each value is the one every unit stating it gives, else the
    /// first such unit's; a title taken from the first unit drops a trailing
    /// disc marker ([`without_disc_marker`]). The title falls back to
    /// `folder_name` where no unit states one. The artists are the album
    /// artists where any unit names one, else the track artists.
    fn of(units: impl Iterator<Item = UnitAlbum>, folder_name: Option<&str>) -> Self {
        let units: Vec<UnitAlbum> = units.collect();
        let title = match given(units.iter().map(|unit| unit.title.as_deref()), |a, b| a == b) {
            Given::Agreed(title) => title.to_string(),
            Given::Disagreed { first } => without_disc_marker(first).to_string(),
            Given::Unstated => folder_name.map(str::to_string).unwrap_or_default(),
        };
        let stated = |artists: &[String]| (!artists.is_empty()).then(|| artists.to_vec());
        let artists = given(
            units.iter().map(|unit| stated(&unit.album_artists)),
            |a, b| same_names(a, b),
        )
        .value()
        .or_else(|| {
            given(
                units.iter().map(|unit| stated(&unit.track_artists)),
                |a, b| same_names(a, b),
            )
            .value()
        })
        .unwrap_or_default();
        let year = |year: fn(&StatedYears) -> Option<i32>| {
            given(units.iter().map(|unit| year(&unit.years)), |a, b| a == b).value()
        };
        Self {
            title,
            artists,
            compilation: units.iter().all(|unit| unit.compilation),
            years: StatedYears {
                album: year(|years| years.album),
                pressing: year(|years| years.pressing),
            },
        }
    }
}

/// What the units stating a value give of it.
enum Given<T> {
    /// Every unit stating it states this.
    Agreed(T),
    /// Units state it differently; `first` is the first one's.
    Disagreed { first: T },
    /// No unit states it.
    Unstated,
}

impl<T> Given<T> {
    /// The value the album takes: the one agreed on, else the first unit's.
    fn value(self) -> Option<T> {
        match self {
            Self::Agreed(value) | Self::Disagreed { first: value } => Some(value),
            Self::Unstated => None,
        }
    }
}

/// What the units stating a value give of it, by `same`; a unit stating
/// nothing (`None`) never disagrees.
fn given<T>(values: impl Iterator<Item = Option<T>>, same: impl Fn(&T, &T) -> bool) -> Given<T> {
    let mut first: Option<T> = None;
    let mut agreed = true;
    for value in values.flatten() {
        match &first {
            None => first = Some(value),
            Some(known) => agreed &= same(known, &value),
        }
    }
    match first {
        Some(value) if agreed => Given::Agreed(value),
        Some(first) => Given::Disagreed { first },
        None => Given::Unstated,
    }
}

/// `title` without the disc marker it ends with, and the separator before
/// it: `Album Title (cd 1)`, `Album Title [disc  09]`, `Album Title CD1`,
/// `Album Title - Disc 1` and `Album Title_D2` are all `Album Title`. A
/// marker is `disc`, `disk`, `cd` or `d` standing as its own word, then a
/// number, the two in brackets or not. `Album Title Volume 1` names no disc
/// and keeps its number, and a title that is nothing but a marker is kept
/// whole. Trailing spaces go either way.
fn without_disc_marker(title: &str) -> &str {
    const MARKERS: &[&str] = &["disc", "disk", "cd", "d"];
    const SEPARATORS: &[char] = &['-', '–', '—', ':', ',', '_', '/', '|'];
    let title = title.trim_end();
    let (inside, open) = match title.chars().last() {
        Some(')') => (&title[..title.len() - 1], Some('(')),
        Some(']') => (&title[..title.len() - 1], Some('[')),
        Some('}') => (&title[..title.len() - 1], Some('{')),
        _ => (title, None),
    };
    let inside = inside.trim_end();
    let number_start = inside.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if number_start == inside.len() {
        return title;
    }
    let before_number = inside[..number_start].trim_end();
    let lowered = before_number.to_ascii_lowercase();
    let Some(marker) = MARKERS.iter().find(|marker| lowered.ends_with(*marker)) else {
        return title;
    };
    let before_marker = &before_number[..before_number.len() - marker.len()];
    if before_marker.chars().next_back().is_some_and(char::is_alphanumeric) {
        return title;
    }
    let before_marker = match open {
        Some(open) => match before_marker.strip_suffix(open) {
            Some(rest) => rest,
            None => return title,
        },
        None if before_marker.ends_with(['(', '[', '{']) => return title,
        None => before_marker,
    };
    let kept = before_marker.trim_end_matches(|c: char| c.is_whitespace() || SEPARATORS.contains(&c));
    if kept.is_empty() {
        title
    } else {
        kept
    }
}

/// Whether two lists of artist names name the same artists in the same
/// order.
fn same_names(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| crate::text_match::same_artist_name(a, b))
}

/// One track as files or a CUE sheet state it, before the folder's discs are
/// laid out as mediums.
struct TagTrack {
    title: String,
    /// The disc it states, `None` where it states none.
    disc: Option<u32>,
    number: Option<std::num::NonZeroU32>,
    /// The artists it names (a file's ARTIST, a CUE track's PERFORMER), in
    /// order.
    artists: Vec<String>,
}

/// The tracks loose files' tags state, one per file.
fn tag_tracks(extracted: &[FileTagFact]) -> Vec<TagTrack> {
    extracted
        .iter()
        .map(|t| {
            // The scanner only admits files with a recognised audio extension, so
            // every path here has a stem. A `None` would mean that invariant broke
            // upstream — panic rather than fabricate a "Track N" placeholder.
            let title = t.tags.title.clone().unwrap_or_else(|| {
                Path::new(&t.observation.relative_path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .expect("scanned audio file relative path has a stem")
            });

            // The tags' numbers, else the ones the file's name starts with. A
            // file stating no disc claims no grouping; one stating no track is
            // numbered by its place on its disc.
            let numbers = t.tags.numbers.or_file_name(&t.observation.relative_path);
            TagTrack {
                title,
                disc: numbers.disc,
                number: numbers.track.and_then(std::num::NonZeroU32::new),
                artists: t.tags.track_artists.clone(),
            }
        })
        .collect()
}

/// The credits of artists files or sheets name. The source provides only
/// display names here, so the sort names and source artist ids stay absent,
/// and no joining words: the line prints the default separator.
fn named_credits(names: &[String]) -> Vec<crate::import::SourceCredit> {
    names
        .iter()
        .map(|name| crate::import::SourceCredit::plain(ArtistCredit::named(name)))
        .collect()
}

/// The values the file-tag and CUE-sheet readers share: one medium per disc
/// the tracks state ([`crate::mediums::by_disc`]). A track naming the album's
/// artists, or none, takes the album's credits; one naming others has its own.
/// A compilation is credited to the various-artists artist, and each of its
/// tracks keeps the artists it names.
fn file_tag_release_values(album: TaggedAlbum, tags: Vec<TagTrack>) -> ReleaseValues {
    let numbering: Vec<_> = tags.iter().map(|track| (track.disc, track.number)).collect();
    let (mediums, places) = crate::mediums::by_disc(&numbering);
    let album_artists = if album.compilation {
        vec![VARIOUS_ARTISTS_NAME.to_string()]
    } else {
        album.artists
    };
    let tracks: Vec<TrackValues> = tags
        .into_iter()
        .zip(places)
        .map(|(track, place)| TrackValues {
            title: track.title,
            place,
            stated_length_ms: None,
            credits: if track.artists.is_empty()
                || (!album.compilation && same_names(&track.artists, &album_artists))
            {
                TrackCredits::AlbumArtists
            } else {
                TrackCredits::Own(named_credits(&track.artists))
            },
            roles: Vec::new(),
            works: Vec::new(),
        })
        .collect();
    ReleaseValues {
        album_title: album.title,
        album_credits: named_credits(&album_artists),
        album_year: album.years.album,
        pressing: Pressing {
            year: album.years.pressing,
            ..Pressing::blank()
        },
        mediums,
        release_roles: Vec::new(),
        works: Default::default(),
        tracks,
    }
}

/// The name a compilation's album credit is given where the tags only flag
/// one.
const VARIOUS_ARTISTS_NAME: &str = "Various Artists";

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
    let album = TaggedAlbum::of(sheets.iter().map(|sheet| UnitAlbum::of_sheet(sheet)), folder_name);
    let mut loose_tracks = tag_tracks(&loose_facts).into_iter();
    let mut tracks = Vec::new();
    for (_, contribution) in layout {
        match contribution {
            UnitContribution::Whole => {
                tracks.push(loose_tracks.next().expect("one track per loose audio file"))
            }
            UnitContribution::Runs(sheets) => {
                for bound in sheets {
                    let disc = bound
                        .disc_number()
                        .expect("carving sheet has a disc number");
                    tracks.extend(
                        bound
                            .sheet
                            .playable_tracks()
                            .map(|track| cue_track(track, disc)),
                    );
                }
            }
            UnitContribution::SpokenFor => {}
        }
    }
    Ok(file_tag_release_values(album, tracks))
}

fn cue_track(track: &crate::cue_flac::CueTrack, disc: u32) -> TagTrack {
    TagTrack {
        title: non_empty(track.title.clone()).unwrap_or_default(),
        disc: Some(disc),
        number: std::num::NonZeroU32::new(track.number),
        artists: non_empty(track.performer.clone()).into_iter().collect(),
    }
}

#[cfg(test)]
#[path = "file_tag_mapper_tests.rs"]
mod tests;
