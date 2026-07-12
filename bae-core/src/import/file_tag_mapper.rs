//! File-tag metadata seeding for Unknown imports.
//!
//! Reads embedded ID3v1/ID3v2/Vorbis-comment/MP4-ilst tags from a rip's
//! audio files and projects them into the same `ParsedAlbum` shape that
//! `map_mb_response_to_db` and `map_discogs_to_db` produce. Used when no
//! external identification is available — the user explicitly opts in
//! via "Add as Unknown", and the editable confirmation page lets them
//! correct anything tags got wrong.
//!
//! The returned `ParsedAlbum::identities` is always empty: an Unknown
//! import makes no identity claim. `metadata_source` lands as
//! `FileTags`; `metadata_source_release_id` stays NULL on the release
//! row.
//!
//! Signals (`disc_id`, `barcode`, `catalog_number`) are out of scope —
//! they flow through the signal pipeline regardless of seed source per
//! the parent doc's Signals policy.
//!
//! Format is derived from the codec (`FLAC`, `MP3`, `APE`, `M4A`); year
//! from any tag carrying a date. Both stay `None` if not determinable
//! rather than being defaulted.

use super::assemble::{
    assemble_parsed_album, AlbumArtistScope, ArtistRef, ReleaseIr, TrackEvent, TrackIr, TrackNumber,
};
use super::ParsedAlbum;
use crate::cue_flac::CueSheet;
use crate::db::{Pressing, ReleaseMetadataSource};
use crate::import::folder_scanner::{AudioContent, CategorizedFiles};
use crate::import::ImportError;
use crate::util::content_type::ContentType;
use coven::Clock;
use coven::IdProvider;
use lofty::prelude::*;
use lofty::probe::Probe;
use std::path::{Path, PathBuf};

/// Per-file extracted tag set. One produced per audio file before the
/// mapper aggregates them into album/track shape.
#[derive(Debug, Clone)]
struct FileTags {
    path: PathBuf,
    content_type: Option<ContentType>,
    title: Option<String>,
    track_artist: Option<String>,
    album_title: Option<String>,
    album_artist: Option<String>,
    track_number: Option<u32>,
    disc_number: Option<u32>,
    year: Option<u16>,
}

/// Map the embedded tags of a rip's audio files to a `ParsedAlbum`.
///
/// `audio_files` is the ordered list of audio files in the rip. The
/// order determines fallback track ordering when DISCNUMBER/TRACKNUMBER
/// tags are absent or partial — a sane default for natural-sort folder
/// scans. `folder_name` is the rip's containing folder, used as the
/// album-title fallback when no file carries an ALBUM tag.
///
/// Seeds whatever the files carry and leaves the rest for the user. This
/// is the Unknown path: the user explicitly opted in and gets an editable
/// confirmation form, so missing album-level fields are seeded as
/// editable blanks rather than errors — the album title falls back to
/// `folder_name` (the folder is the album for a typical rip) and then to
/// empty; the artist falls back to empty. The form's save-gate
/// (`RawReleaseEdit::shape` → `EmptyAlbumTitle` / `NoAlbumArtist`) requires
/// the user to fill any blank before committing, so we never write a
/// fabricated default. Per-track TITLE absence falls back to the file stem.
///
/// Errors only when `audio_files` is empty (an album has at least one
/// track) or an individual file fails to open / has an unsupported codec.
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

    let extracted: Vec<FileTags> = audio_files
        .iter()
        .map(|p| read_tags(p))
        .collect::<Result<_, _>>()?;

    // ── Album-level fields ─────────────────────────────────────────────
    // Album title: an ALBUM tag if any file carries one, else the rip's
    // folder name, else empty. The editable form gates save on a non-empty
    // title, so a blank here is a prompt to the user, not a committed value.
    let album_title = extracted
        .iter()
        .find_map(|t| t.album_title.as_ref())
        .cloned()
        .or_else(|| folder_name.map(str::to_string))
        .unwrap_or_default();

    // Album artist: prefer ALBUMARTIST when set; fall back to ARTIST
    // (rippers commonly populate only ARTIST for single-artist albums, so
    // accepting it here matches what those tools actually write); else
    // empty. No folder-name fallback — the folder name is the album, not a
    // reliable artist. The form's save-gate requires a non-empty artist.
    let album_artist_name = extracted
        .iter()
        .find_map(|t| t.album_artist.as_ref())
        .or_else(|| extracted.iter().find_map(|t| t.track_artist.as_ref()))
        .cloned()
        .unwrap_or_default();

    let year = extracted.iter().find_map(|t| t.year).map(|y| y as i32);

    // Format reflects the actual codec of the rip, not editorial pressing
    // info. If probing fails, the editable field stays blank instead of using
    // an extension or container label.
    let format = extracted[0]
        .content_type
        .as_ref()
        .map(|content_type| content_type.display_name().to_string());

    // ── Track seeds: assign side from DISCNUMBER, track_number from
    // TRACKNUMBER. Positional fallback (index within side by file order) is
    // used only on a side where NO file is tagged — backfilling a position
    // onto an untagged file that shares a side with tagged files would collide
    // with the real tag values (e.g. an untagged file getting position 1 next
    // to a TRACKNUMBER=1 file). On a partially-tagged side the untagged files
    // stay `None` for the user to assign in the editor.
    let side_of = |t: &FileTags| match t.disc_number {
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
            // The folder scanner only admits files with a recognised audio
            // extension, so every path here has both an extension and a stem.
            // `file_stem` returning None would mean the scanner's invariant
            // broke upstream — surface it rather than fabricate a "Track N"
            // placeholder. The filename stem is the last-resort title when a
            // file carries no TITLE tag.
            let title = t.title.clone().unwrap_or_else(|| {
                t.path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .expect("audio file path has a stem")
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

/// An [`ArtistRef`] for a file-tag ARTIST/PERFORMER value: the name doubles as
/// its own sort name, and the file tags carry no source artist ids.
fn file_tag_artist_ref(name: &str) -> ArtistRef {
    ArtistRef {
        name: name.to_string(),
        sort_name: Some(name.to_string()),
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
/// is always empty (Unknown makes no identity claim); `metadata_source` is
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
        metadata_source: ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        album_artist_scope: AlbumArtistScope::FullPool,
        release_roles: Vec::new(),
        tracks,
        identities: Vec::new(),
    }
}

/// Map an Unknown import candidate to the metadata seed used by preview and
/// commit. CUE-backed candidates take track layout and names from the parsed
/// CUE; per-track-file candidates take them from embedded file tags.
pub fn map_unknown_candidate_to_db(
    categorized: &CategorizedFiles,
    folder_name: Option<&str>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    match &categorized.audio {
        AudioContent::CueFlacPairs { pairs, .. } => {
            let mut sorted = pairs.iter().collect::<Vec<_>>();
            sorted.sort_by(|a, b| {
                natord::compare(&a.cue_file.relative_path, &b.cue_file.relative_path)
            });
            let sheets = sorted.iter().map(|p| &p.cue_sheet).collect::<Vec<_>>();
            let audio = sorted
                .iter()
                .map(|p| p.audio_file.path.as_path())
                .collect::<Vec<_>>();
            map_cue_sheets_to_db(&sheets, &audio, folder_name, clock, ids)
        }
        AudioContent::TrackFiles { tracks, .. } => {
            let audio_files = tracks.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
            map_file_tags_to_db(&audio_files, folder_name, clock, ids)
        }
    }
}

/// Map a CUE-backed rip's parsed sheets to a [`ParsedAlbum`] for the Unknown
/// path. Unlike [`map_file_tags_to_db`], which seeds one track per file, the
/// track structure comes from playable CUE `TRACK` entries: title from each
/// `TITLE`, per-track artist from each `PERFORMER`. Album-level fields come
/// from the sheet header (`TITLE` / `PERFORMER` / `REM DATE`), the album title
/// falling back to the folder name. `sheets` and `audio_files` are one-per-pair
/// in disc order — the same order `track_to_file_mapper` slices pairs — so side
/// is the 1-based disc index and track numbers run per sheet.
pub fn map_cue_sheets_to_db(
    sheets: &[&CueSheet],
    audio_files: &[&Path],
    folder_name: Option<&str>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    if sheets.is_empty() {
        return Err(ImportError::FileTags {
            detail: "CUE seeding requires at least one sheet".to_string(),
        });
    }

    // Album title: a sheet TITLE if any carries one, else the rip's folder
    // name, else empty (the editable Unknown form gates save on it).
    let album_title = sheets
        .iter()
        .find_map(|s| non_empty(s.title.clone()))
        .or_else(|| folder_name.map(str::to_string))
        .unwrap_or_default();

    // Album artist: the sheet PERFORMER header, else empty.
    let album_artist_name = sheets
        .iter()
        .find_map(|s| non_empty(s.performer.clone()))
        .unwrap_or_default();

    let year = sheets
        .iter()
        .find_map(|s| year_from_cue_date(s.date.as_deref()));

    // Format reflects the actual codec, from the first audio file. Absent if
    // the container can't be probed; the user can correct it in the form.
    let format = audio_files
        .first()
        .and_then(|p| probe_content_type(p))
        .map(|content_type| content_type.display_name().to_string());

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

/// Probe a file's codec through FFmpeg. Used to derive editable format labels
/// from decoder identity rather than from the extension or container.
fn probe_content_type(path: &Path) -> Option<ContentType> {
    let Some(path_str) = path.to_str() else {
        tracing::warn!("failed to probe audio format of non-UTF-8 path: {path:?}");
        return None;
    };
    match crate::audio_codec::probe_audio_from_path(path_str) {
        Some(probe) => Some(probe.content_type),
        None => {
            tracing::warn!("failed to probe audio format of {}", path.display());
            None
        }
    }
}

/// Read the embedded front-cover picture from the first audio file that
/// carries one, mapped to the library's [`ContentType`].
///
/// This is the lowest-priority cover source for an Unknown import: it
/// only feeds the cover pipeline when neither an explicit selection nor a
/// folder image provides one (the caller enforces that ordering). Prefers
/// a `PictureType::CoverFront` picture; falls back to the first embedded
/// picture of any type. Returns `Ok(None)` when no file carries a picture
/// or the picture's MIME isn't a supported image type (e.g. lofty's
/// `Tiff`, which the library doesn't store as a cover) — a missing or
/// unsupported embedded picture simply means there's nothing to seed.
/// Returns `Err` when an audio file can't be opened or its tags can't be
/// read.
pub fn read_embedded_cover(
    audio_files: &[PathBuf],
) -> Result<Option<(Vec<u8>, ContentType)>, ImportError> {
    for path in audio_files {
        let probe = match Probe::open(path) {
            Ok(probe) => probe,
            Err(e) => {
                return Err(ImportError::FileTags {
                    detail: format!(
                        "failed to open {} for embedded cover read: {e}",
                        path.display()
                    ),
                });
            }
        };
        let tagged = match probe.read() {
            Ok(tagged) => tagged,
            Err(e) => {
                return Err(ImportError::FileTags {
                    detail: format!(
                        "failed to read embedded cover tags from {}: {e}",
                        path.display()
                    ),
                });
            }
        };
        let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
            continue;
        };

        let pictures = tag.pictures();
        let picture = pictures
            .iter()
            .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
            .or_else(|| pictures.first());
        let Some(picture) = picture else {
            continue;
        };

        match picture.mime_type().and_then(image_content_type) {
            Some(content_type) => return Ok(Some((picture.data().to_vec(), content_type))),
            // A picture in a MIME type the library can't store as a cover
            // (e.g. TIFF) — skip this file and try the next.
            None => continue,
        }
    }
    Ok(None)
}

/// Map a lofty picture MIME to the library's [`ContentType`], for the
/// image types the cover store supports. Returns `None` for anything else
/// — lofty's `Tiff`, an unrecognized `Unknown(..)` MIME, etc.
fn image_content_type(mime: &lofty::picture::MimeType) -> Option<ContentType> {
    use lofty::picture::MimeType;
    match mime {
        MimeType::Jpeg => Some(ContentType::Jpeg),
        MimeType::Png => Some(ContentType::Png),
        MimeType::Gif => Some(ContentType::Gif),
        MimeType::Bmp => Some(ContentType::Bmp),
        // lofty has no WebP variant; rippers that embed WebP land here.
        MimeType::Unknown(s) => match ContentType::from_mime(s) {
            ct @ (ContentType::Jpeg
            | ContentType::Png
            | ContentType::Gif
            | ContentType::Bmp
            | ContentType::Webp) => Some(ct),
            _ => None,
        },
        _ => None,
    }
}

/// Extract embedded tag values from a single audio file.
fn read_tags(path: &Path) -> Result<FileTags, ImportError> {
    let probe = Probe::open(path).map_err(|e| ImportError::FileTags {
        detail: format!("failed to open {}: {}", path.display(), e),
    })?;
    let tagged = probe.read().map_err(|e| ImportError::FileTags {
        detail: format!("failed to read tags from {}: {}", path.display(), e),
    })?;

    let content_type = probe_content_type(path);

    // Prefer the file format's primary tag (e.g. ID3v2 on MP3, Vorbis on
    // FLAC); fall back to whichever tag is present (e.g. ID3v1-only on
    // an MP3 with no ID3v2). first_tag covers ID3v1 that some rippers
    // still produce.
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let (title, track_artist, album_title, album_artist, track_number, disc_number, year) =
        if let Some(tag) = tag {
            (
                non_empty(tag.title().map(|c| c.to_string())),
                non_empty(tag.artist().map(|c| c.to_string())),
                non_empty(tag.album().map(|c| c.to_string())),
                non_empty(tag.get_string(ItemKey::AlbumArtist).map(|s| s.to_string())),
                tag.track(),
                tag.disk(),
                year_from_tag(tag),
            )
        } else {
            (None, None, None, None, None, None, None)
        };

    Ok(FileTags {
        path: path.to_path_buf(),
        content_type,
        title,
        track_artist,
        album_title,
        album_artist,
        track_number,
        disc_number,
        year,
    })
}

/// Drop a tag string to `None` when it's empty or whitespace-only.
/// Lofty returns `Some("")` for present-but-blank tags (e.g. an
/// `ALBUM=` line in Vorbis comments); treating those as present
/// would let blank album titles and artist names slip past the
/// required-field validation downstream.
fn non_empty(s: Option<String>) -> Option<String> {
    s.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Read a year from the tag. Lofty exposes `date()` as a structured
/// `Timestamp` (the preferred ID3v2.4 / Vorbis "DATE" field); older
/// or alternate fields land in ItemKey::Year, ItemKey::ReleaseDate
/// (TDRL — release date), or ItemKey::OriginalReleaseDate. We accept
/// any of them.
fn year_from_tag(tag: &lofty::tag::Tag) -> Option<u16> {
    if let Some(ts) = tag.date() {
        return Some(ts.year);
    }
    if let Some(s) = tag.get_string(ItemKey::Year) {
        if let Ok(y) = s.parse::<u16>() {
            return Some(y);
        }
    }
    if let Some(s) = tag.get_string(ItemKey::ReleaseDate) {
        if let Some(y) = s.split('-').next().and_then(|y| y.parse::<u16>().ok()) {
            return Some(y);
        }
    }
    if let Some(s) = tag.get_string(ItemKey::OriginalReleaseDate) {
        if let Some(y) = s.split('-').next().and_then(|y| y.parse::<u16>().ok()) {
            return Some(y);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use coven::FixedClock;
    use coven::SequentialIdProvider;
    use lofty::config::WriteOptions;
    use lofty::tag::items::Timestamp;
    use lofty::tag::{Tag, TagType};
    use std::fs;
    use tempfile::TempDir;

    /// Run the mapper with deterministic fakes. Exercises the real
    /// `map_file_tags_to_db`; only the clock/id inputs are faked.
    fn map_tags(audio_files: &[PathBuf]) -> Result<ParsedAlbum, ImportError> {
        map_tags_with_folder(audio_files, None)
    }

    fn map_tags_with_folder(
        audio_files: &[PathBuf],
        folder_name: Option<&str>,
    ) -> Result<ParsedAlbum, ImportError> {
        let clock = FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        let ids = SequentialIdProvider::new("ft");
        map_file_tags_to_db(audio_files, folder_name, &clock, &ids)
    }

    fn fixtures_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
    }

    #[test]
    fn cue_sheet_seeds_one_track_per_cue_entry_not_per_image_file() {
        use crate::cue_flac::{CueIndex, CuePregap, CueSheet, CueTrack, CueTrackMode};
        let mk = |number: u32, title: &str| CueTrack {
            number,
            mode: CueTrackMode::Audio,
            title: Some(title.to_string()),
            performer: Some("Artist Name".to_string()),
            indexes: vec![CueIndex {
                number: 1,
                frames: 0,
                file_reference: "image.flac".to_string(),
            }],
            file_reference: "image.flac".to_string(),
            start_cue_frames: 0,
            pregap: CuePregap::None,
            pregap_cue_frames: None,
            generated_pregap_frames: None,
            end_cue_frames: None,
        };
        let sheet = CueSheet {
            title: Some("Album Title".to_string()),
            performer: Some("Artist Name".to_string()),
            catalog: None,
            date: Some("1970".to_string()),
            tracks: vec![mk(1, "Track One"), mk(2, "Track Two"), mk(3, "Track Three")],
        };
        let clock = FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        let ids = SequentialIdProvider::new("cue");
        // Nonexistent audio path: the codec probe yields None (fine — the
        // seed still carries every CUE track); the point is track count.
        let audio = Path::new("/nonexistent/image.flac");
        let parsed =
            map_cue_sheets_to_db(&[&sheet], &[audio], Some("Folder Name"), &clock, &ids).unwrap();

        // The single-file image is NOT collapsed to one track: one DbTrack per
        // CUE TRACK entry, in order, carrying the CUE's own titles.
        assert_eq!(parsed.tracks.len(), 3);
        assert_eq!(parsed.tracks[0].title, "Track One");
        assert_eq!(parsed.tracks[1].title, "Track Two");
        assert_eq!(parsed.tracks[2].title, "Track Three");
        assert!(parsed.tracks.iter().all(|t| t.side == 1));
        assert_eq!(
            parsed
                .tracks
                .iter()
                .map(|t| t.track_number)
                .collect::<Vec<_>>(),
            vec![Some(1), Some(2), Some(3)]
        );
        assert_eq!(parsed.album.title, "Album Title");
        assert_eq!(parsed.album.year, Some(1970));
        assert_eq!(parsed.artists[0].name, "Artist Name");
    }

    fn cue_track(number: u32, title: &str) -> crate::cue_flac::CueTrack {
        use crate::cue_flac::{CueIndex, CuePregap, CueTrack, CueTrackMode};
        CueTrack {
            number,
            mode: CueTrackMode::Audio,
            title: Some(title.to_string()),
            performer: None,
            indexes: vec![CueIndex {
                number: 1,
                frames: 0,
                file_reference: "image.flac".to_string(),
            }],
            file_reference: "image.flac".to_string(),
            start_cue_frames: 0,
            pregap: CuePregap::None,
            pregap_cue_frames: None,
            generated_pregap_frames: None,
            end_cue_frames: None,
        }
    }

    fn cue_sheet(title: &str, tracks: Vec<crate::cue_flac::CueTrack>) -> CueSheet {
        CueSheet {
            title: Some(title.to_string()),
            performer: Some("Artist Name".to_string()),
            catalog: None,
            date: None,
            tracks,
        }
    }

    /// Multi-disc CUE rip: one sheet per disc. Side is the 1-based disc index
    /// (sheet order); track numbers restart per sheet.
    #[test]
    fn cue_multi_sheet_assigns_side_per_disc() {
        let disc1 = cue_sheet(
            "Album Title",
            vec![cue_track(1, "D1 T1"), cue_track(2, "D1 T2")],
        );
        let disc2 = cue_sheet(
            "Album Title",
            vec![cue_track(1, "D2 T1"), cue_track(2, "D2 T2")],
        );
        let clock = FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        let ids = SequentialIdProvider::new("cue");
        let a1 = Path::new("/nonexistent/disc1.flac");
        let a2 = Path::new("/nonexistent/disc2.flac");

        let parsed =
            map_cue_sheets_to_db(&[&disc1, &disc2], &[a1, a2], Some("Folder"), &clock, &ids)
                .unwrap();

        assert_eq!(parsed.tracks.len(), 4);
        assert_eq!(parsed.tracks[0].side, 1);
        assert_eq!(parsed.tracks[0].track_number, Some(1));
        assert_eq!(parsed.tracks[1].side, 1);
        assert_eq!(parsed.tracks[1].track_number, Some(2));
        assert_eq!(parsed.tracks[2].side, 2);
        assert_eq!(parsed.tracks[2].track_number, Some(1));
        assert_eq!(parsed.tracks[3].side, 2);
        assert_eq!(parsed.tracks[3].track_number, Some(2));
    }

    /// No sheets → error (an album has at least one disc).
    #[test]
    fn cue_empty_sheets_returns_error() {
        let clock = FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        let ids = SequentialIdProvider::new("cue");
        let err = map_cue_sheets_to_db(&[], &[], Some("Folder"), &clock, &ids)
            .expect_err("expected empty sheets to error");
        assert!(
            matches!(&err, ImportError::FileTags { detail } if detail.contains("at least one sheet")),
            "got: {err}"
        );
    }

    /// Copy a fixture into `dest_dir/{name}` and stamp it with the given
    /// tag values. Returns the destination path.
    fn copy_and_tag(
        source: &Path,
        dest_dir: &Path,
        name: &str,
        tag_type: TagType,
        title: Option<&str>,
        artist: Option<&str>,
        album_title: Option<&str>,
        album_artist: Option<&str>,
        year: Option<u16>,
        track: Option<u32>,
        disk: Option<u32>,
    ) -> PathBuf {
        let dest = dest_dir.join(name);
        fs::copy(source, &dest).expect("copy fixture");

        let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
        let mut tag = Tag::new(tag_type);
        if let Some(t) = title {
            tag.set_title(t.to_string());
        }
        if let Some(a) = artist {
            tag.set_artist(a.to_string());
        }
        if let Some(at) = album_title {
            tag.set_album(at.to_string());
        }
        if let Some(aa) = album_artist {
            tag.insert_text(ItemKey::AlbumArtist, aa.to_string());
        }
        if let Some(y) = year {
            tag.set_date(Timestamp {
                year: y,
                month: None,
                day: None,
                hour: None,
                minute: None,
                second: None,
            });
        }
        if let Some(n) = track {
            tag.set_track(n);
        }
        if let Some(d) = disk {
            tag.set_disk(d);
        }
        tagged.insert_tag(tag);
        tagged
            .save_to_path(&dest, WriteOptions::default())
            .expect("save tags");
        dest
    }

    /// FLAC + Vorbis comments: titles, artist, album, year, track numbers
    /// land on the tracks. Format is "FLAC". No identity rows.
    #[test]
    fn flac_with_vorbis_comments_basic() {
        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

        let f1 = copy_and_tag(
            &src,
            temp.path(),
            "01.flac",
            TagType::VorbisComments,
            Some("Track One"),
            Some("Artist Name"),
            Some("Album Title"),
            Some("Artist Name"),
            Some(1999),
            Some(1),
            None,
        );
        let f2 = copy_and_tag(
            &src2,
            temp.path(),
            "02.flac",
            TagType::VorbisComments,
            Some("Track Two"),
            Some("Artist Name"),
            Some("Album Title"),
            Some("Artist Name"),
            Some(1999),
            Some(2),
            None,
        );

        let parsed = map_tags(&[f1, f2]).unwrap();

        assert_eq!(parsed.album.title, "Album Title");
        assert_eq!(parsed.album.year, Some(1999));
        assert_eq!(parsed.release.pressing.year, Some(1999));
        assert_eq!(parsed.release.pressing.format.as_deref(), Some("FLAC"));
        assert_eq!(
            parsed.release.metadata_source,
            ReleaseMetadataSource::FileTags
        );
        assert!(parsed.release.metadata_source_release_id.is_none());

        assert_eq!(parsed.tracks.len(), 2);
        assert_eq!(parsed.tracks[0].title, "Track One");
        assert_eq!(parsed.tracks[0].side, 1);
        assert_eq!(parsed.tracks[0].track_number, Some(1));
        assert_eq!(parsed.tracks[1].title, "Track Two");
        assert_eq!(parsed.tracks[1].track_number, Some(2));

        assert_eq!(parsed.artists.len(), 1, "single artist for whole album");
        assert_eq!(parsed.artists[0].name, "Artist Name");
        assert!(parsed.album_artists.is_empty());
        assert_eq!(
            parsed.track_artists.len(),
            2,
            "every track ARTIST tag emits a junction row, even when it \
             matches the album artist (mirrors MB/Discogs mappers)"
        );
        assert!(parsed
            .track_artists
            .iter()
            .all(|ta| ta.artist_id == parsed.artists[0].id));

        assert!(
            parsed.identities.is_empty(),
            "Unknown imports never claim identity"
        );
    }

    /// Multi-disc rip: DISCNUMBER + TRACKNUMBER produce side groupings.
    #[test]
    fn flac_multi_disc_groups_by_discnumber() {
        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");

        let mut files = Vec::new();
        for (disc, track, name) in [
            (1u32, 1u32, "d1t1.flac"),
            (1, 2, "d1t2.flac"),
            (2, 1, "d2t1.flac"),
            (2, 2, "d2t2.flac"),
        ] {
            files.push(copy_and_tag(
                &src,
                temp.path(),
                name,
                TagType::VorbisComments,
                Some(&format!("Track {}-{}", disc, track)),
                Some("Album Artist"),
                Some("Album Title"),
                Some("Album Artist"),
                None,
                Some(track),
                Some(disc),
            ));
        }

        let parsed = map_tags(&files).unwrap();

        assert_eq!(parsed.tracks.len(), 4);
        assert_eq!(parsed.tracks[0].side, 1);
        assert_eq!(parsed.tracks[0].track_number, Some(1));
        assert_eq!(parsed.tracks[1].side, 1);
        assert_eq!(parsed.tracks[1].track_number, Some(2));
        assert_eq!(parsed.tracks[2].side, 2);
        assert_eq!(parsed.tracks[2].track_number, Some(1));
        assert_eq!(parsed.tracks[3].side, 2);
        assert_eq!(parsed.tracks[3].track_number, Some(2));
    }

    #[test]
    fn out_of_range_tag_numbers_do_not_wrap_negative() {
        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");

        let file = copy_and_tag(
            &src,
            temp.path(),
            "wrapped-tag-numbers.flac",
            TagType::VorbisComments,
            Some("Track Title"),
            Some("Artist Name"),
            Some("Album Title"),
            Some("Artist Name"),
            None,
            Some(u32::MAX),
            Some(u32::MAX),
        );

        let parsed = map_tags(&[file]).unwrap();

        assert_eq!(parsed.tracks[0].side, i32::MAX);
        assert_eq!(parsed.tracks[0].track_number, None);
    }

    /// Per-track artist different from album artist → adds an extra
    /// artist row, and every track ARTIST tag emits a junction row
    /// (the matching one points at the album artist; the divergent one
    /// at the per-track artist). Mirrors map_mb_response_to_db /
    /// map_discogs_to_db, which emit a junction for every credit.
    #[test]
    fn flac_per_track_artist_emits_junction_row() {
        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

        let f1 = copy_and_tag(
            &src,
            temp.path(),
            "01.flac",
            TagType::VorbisComments,
            Some("Track One"),
            Some("Album Artist"),
            Some("Album Title"),
            Some("Album Artist"),
            None,
            Some(1),
            None,
        );
        let f2 = copy_and_tag(
            &src2,
            temp.path(),
            "02.flac",
            TagType::VorbisComments,
            Some("Track Two"),
            Some("Featured Artist"),
            Some("Album Title"),
            Some("Album Artist"),
            None,
            Some(2),
            None,
        );

        let parsed = map_tags(&[f1, f2]).unwrap();

        assert_eq!(parsed.artists.len(), 2);
        assert_eq!(parsed.artists[0].name, "Album Artist");
        assert_eq!(parsed.artists[1].name, "Featured Artist");

        assert_eq!(parsed.track_artists.len(), 2);
        assert_eq!(parsed.track_artists[0].track_id, parsed.tracks[0].id);
        assert_eq!(parsed.track_artists[0].artist_id, parsed.artists[0].id);
        assert_eq!(parsed.track_artists[1].track_id, parsed.tracks[1].id);
        assert_eq!(parsed.track_artists[1].artist_id, parsed.artists[1].id);

        // The divergent per-track artist joins the album-artists junction: the
        // file-tag path scopes album artists to the whole pool, so any artist
        // introduced by a per-track credit also becomes an album artist.
        assert_eq!(parsed.album_artists.len(), 1);
        assert_eq!(parsed.album_artists[0].artist_id, parsed.artists[1].id);
        assert_eq!(parsed.album_artists[0].position, 1);
    }

    /// Missing optional tags (no year, no DISCNUMBER) → year is None,
    /// all tracks land on side 1, track numbers fall back to file order.
    #[test]
    fn missing_optional_tags_yields_none_and_defaults() {
        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

        let f1 = copy_and_tag(
            &src,
            temp.path(),
            "01.flac",
            TagType::VorbisComments,
            Some("Track One"),
            Some("Artist"),
            Some("Album"),
            None,
            None,
            None,
            None,
        );
        let f2 = copy_and_tag(
            &src2,
            temp.path(),
            "02.flac",
            TagType::VorbisComments,
            Some("Track Two"),
            Some("Artist"),
            Some("Album"),
            None,
            None,
            None,
            None,
        );

        let parsed = map_tags(&[f1, f2]).unwrap();
        assert_eq!(parsed.album.year, None);
        assert_eq!(parsed.release.pressing.year, None);
        assert_eq!(parsed.tracks[0].side, 1);
        assert_eq!(parsed.tracks[1].side, 1);
        assert_eq!(parsed.tracks[0].track_number, Some(1));
        assert_eq!(parsed.tracks[1].track_number, Some(2));
    }

    /// A side where some files carry TRACKNUMBER and some don't: the
    /// untagged files must NOT get a positional fallback (it would collide
    /// with the real tag values) — they stay `None` for the user to assign.
    #[test]
    fn partial_tracknumber_side_leaves_untagged_none() {
        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

        // First file untagged, second tagged TRACKNUMBER=1 — old positional
        // fallback would assign the first file position 1 too, colliding.
        let f1 = copy_and_tag(
            &src,
            temp.path(),
            "a.flac",
            TagType::VorbisComments,
            Some("Track A"),
            Some("Artist"),
            Some("Album"),
            None,
            None,
            None,
            None,
        );
        let f2 = copy_and_tag(
            &src2,
            temp.path(),
            "b.flac",
            TagType::VorbisComments,
            Some("Track B"),
            Some("Artist"),
            Some("Album"),
            None,
            None,
            Some(1),
            None,
        );

        let parsed = map_tags(&[f1, f2]).unwrap();
        assert_eq!(
            parsed.tracks[0].track_number, None,
            "untagged file on a partially-tagged side stays None"
        );
        assert_eq!(parsed.tracks[1].track_number, Some(1));
    }

    /// The album-title ladder (ALBUM tag → folder name → empty) and the
    /// album-artist ladder (ALBUMARTIST → ARTIST → empty), with blank and
    /// whitespace-only tags dropping to `None` and taking the same ladder.
    /// The Unknown path never hard-fails on a missing album-level tag — the
    /// editable form gates save on the non-empty fields.
    #[test]
    fn album_and_artist_fallback_ladder() {
        struct Row {
            name: &'static str,
            album: Option<&'static str>,
            artist: Option<&'static str>,
            album_artist: Option<&'static str>,
            folder: Option<&'static str>,
            expect_title: &'static str,
            expect_artist: &'static str,
        }
        let rows = [
            // Album title ladder.
            Row {
                name: "no-album-folder",
                album: None,
                artist: Some("Track Artist"),
                album_artist: None,
                folder: Some("Folder Name"),
                expect_title: "Folder Name",
                expect_artist: "Track Artist",
            },
            Row {
                name: "no-album-no-folder",
                album: None,
                artist: Some("Track Artist"),
                album_artist: None,
                folder: None,
                expect_title: "",
                expect_artist: "Track Artist",
            },
            Row {
                name: "empty-album-folder",
                album: Some(""),
                artist: Some("Track Artist"),
                album_artist: None,
                folder: Some("Folder Name"),
                expect_title: "Folder Name",
                expect_artist: "Track Artist",
            },
            Row {
                name: "ws-album-folder",
                album: Some("   "),
                artist: Some("Track Artist"),
                album_artist: None,
                folder: Some("Folder Name"),
                expect_title: "Folder Name",
                expect_artist: "Track Artist",
            },
            // Album-artist ladder: prefer ALBUMARTIST, fall back to ARTIST,
            // then empty; blank tags drop to None.
            Row {
                name: "albumartist-preferred",
                album: Some("Album Title"),
                artist: Some("Track Artist"),
                album_artist: Some("Album Artist"),
                folder: None,
                expect_title: "Album Title",
                expect_artist: "Album Artist",
            },
            Row {
                name: "artist-fallback",
                album: Some("Album Title"),
                artist: Some("Track Artist"),
                album_artist: None,
                folder: None,
                expect_title: "Album Title",
                expect_artist: "Track Artist",
            },
            Row {
                name: "no-artist-empty",
                album: Some("Album Title"),
                artist: None,
                album_artist: None,
                folder: None,
                expect_title: "Album Title",
                expect_artist: "",
            },
            Row {
                name: "blank-artist-empty",
                album: Some("Album Title"),
                artist: Some(""),
                album_artist: Some(""),
                folder: None,
                expect_title: "Album Title",
                expect_artist: "",
            },
        ];

        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        for row in rows {
            let f = copy_and_tag(
                &src,
                temp.path(),
                &format!("{}.flac", row.name),
                TagType::VorbisComments,
                Some("Track Title"),
                row.artist,
                row.album,
                row.album_artist,
                None,
                None,
                None,
            );
            let parsed = map_tags_with_folder(&[f], row.folder).unwrap();
            assert_eq!(
                parsed.album.title, row.expect_title,
                "title for {}",
                row.name
            );
            // artists[0] is always the primary (album) artist; a divergent
            // per-track ARTIST may add a second row, which isn't the ladder's
            // concern here.
            assert_eq!(
                parsed.artists[0].name, row.expect_artist,
                "artist for {}",
                row.name
            );
            assert_eq!(parsed.album.artist_id, parsed.artists[0].id, "{}", row.name);
            assert_eq!(parsed.tracks[0].title, "Track Title", "{}", row.name);
        }
    }

    /// All tracks completely untagged → folder-name title, empty artist,
    /// filename-stem track titles. A fully-untagged rip is still importable
    /// via the editable form.
    #[test]
    fn all_files_untagged_seeds_from_folder_and_filenames() {
        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        let dest = temp.path().join("untitled-track.flac");
        fs::copy(&src, &dest).unwrap();
        // Don't write any tags — the source FLAC fixture only carries an
        // "encoder=Lavf..." comment which is not ALBUM/ARTIST/TITLE.

        let parsed = map_tags_with_folder(&[dest], Some("Mystery Rip")).unwrap();
        assert_eq!(parsed.album.title, "Mystery Rip");
        assert_eq!(parsed.artists[0].name, "");
        assert_eq!(parsed.tracks[0].title, "untitled-track");
    }

    /// Empty input → error.
    #[test]
    fn empty_input_returns_error() {
        let err = map_tags(&[]).unwrap_err();
        assert!(
            matches!(&err, ImportError::FileTags { detail } if detail.contains("at least one")),
            "got: {err}"
        );
    }

    /// Title falls back to the filename stem when the TITLE tag is
    /// absent (some rips only have a partial set). Other tracks in the
    /// same rip can still satisfy the presence check.
    #[test]
    fn missing_title_falls_back_to_filename_stem() {
        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

        let f1 = copy_and_tag(
            &src,
            temp.path(),
            "no-title.flac",
            TagType::VorbisComments,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let f2 = copy_and_tag(
            &src2,
            temp.path(),
            "02.flac",
            TagType::VorbisComments,
            Some("Has Title"),
            Some("Artist"),
            Some("Album"),
            None,
            None,
            None,
            None,
        );

        let parsed = map_tags(&[f1, f2]).unwrap();
        assert_eq!(parsed.tracks[0].title, "no-title");
        assert_eq!(parsed.tracks[1].title, "Has Title");
    }

    /// MP3 with ID3v2 tags: same projection rules apply, format
    /// derives to "MP3". Uses the production MP3 encoder to mint a
    /// short silence file at test time — no checked-in MP3 fixture
    /// needed.
    #[test]
    fn mp3_with_id3v2_tags() {
        crate::audio_codec::init();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        // 0.5s of stereo 44.1kHz silence is enough for FFmpeg to emit a
        // valid MP3 the tag writer can attach to.
        let samples = vec![0i32; 44_100 / 2 * 2];
        let mp3_bytes =
            crate::audio_codec::encode_to_mp3(&samples, 44_100, 2, &cancel).expect("encode mp3");

        let temp = TempDir::new().unwrap();
        let mp3_path = temp.path().join("01.mp3");
        fs::write(&mp3_path, &mp3_bytes).unwrap();

        let f1 = copy_and_tag(
            &mp3_path,
            temp.path(),
            "tagged.mp3",
            TagType::Id3v2,
            Some("MP3 Track"),
            Some("MP3 Artist"),
            Some("MP3 Album"),
            Some("MP3 Artist"),
            Some(2010),
            Some(1),
            None,
        );

        let parsed = map_tags(&[f1]).unwrap();
        assert_eq!(parsed.album.title, "MP3 Album");
        assert_eq!(parsed.album.year, Some(2010));
        assert_eq!(parsed.release.pressing.format.as_deref(), Some("MP3"));
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].title, "MP3 Track");
        assert_eq!(parsed.artists[0].name, "MP3 Artist");
        assert!(parsed.identities.is_empty());
    }

    /// M4A with MP4 ilst tags. Format derives from the probed codec, so
    /// ALAC-in-MP4 does not collapse to a container label.
    #[test]
    fn m4a_with_mp4_ilst_tags() {
        let temp = TempDir::new().unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures")
            .join("alac")
            .join("silence-alac.m4a");

        let f1 = copy_and_tag(
            &src,
            temp.path(),
            "01.m4a",
            TagType::Mp4Ilst,
            Some("Track One"),
            Some("Artist Name"),
            Some("Album Title"),
            Some("Artist Name"),
            Some(2020),
            Some(1),
            None,
        );

        let parsed = map_tags(&[f1]).unwrap();

        assert_eq!(parsed.album.title, "Album Title");
        assert_eq!(parsed.album.year, Some(2020));
        assert_eq!(parsed.release.pressing.format.as_deref(), Some("ALAC"));
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].title, "Track One");
        assert!(parsed.identities.is_empty());
    }

    #[test]
    fn aac_m4a_with_mp4_ilst_tags_uses_aac_label() {
        let temp = TempDir::new().unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures")
            .join("alac")
            .join("silence-aac.m4a");

        let f1 = copy_and_tag(
            &src,
            temp.path(),
            "01.m4a",
            TagType::Mp4Ilst,
            Some("Track One"),
            Some("Artist Name"),
            Some("Album Title"),
            Some("Artist Name"),
            Some(2020),
            Some(1),
            None,
        );

        let parsed = map_tags(&[f1]).unwrap();

        assert_eq!(parsed.release.pressing.format.as_deref(), Some("AAC"));
    }

    /// A few JPEG SOI/EOI bytes — enough to round-trip as opaque cover
    /// data; the embedded-cover read never decodes the image.
    const JPEG_BYTES: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0xFF, 0xD9];

    /// Copy the FLAC fixture into `dir/{name}` and embed a picture with the
    /// given type and MIME. Returns the destination path.
    fn copy_with_picture(
        dir: &Path,
        name: &str,
        pic_type: lofty::picture::PictureType,
        mime: lofty::picture::MimeType,
        data: &[u8],
    ) -> PathBuf {
        use lofty::config::WriteOptions;
        use lofty::picture::Picture;

        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        let dest = dir.join(name);
        fs::copy(&src, &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title("Track".to_string());
        tag.push_picture(
            Picture::unchecked(data.to_vec())
                .pic_type(pic_type)
                .mime_type(mime)
                .build(),
        );
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
        dest
    }

    /// An embedded front cover is read and mapped to its `ContentType`.
    #[test]
    fn read_embedded_cover_returns_front_cover() {
        let temp = TempDir::new().unwrap();
        let f = copy_with_picture(
            temp.path(),
            "01.flac",
            lofty::picture::PictureType::CoverFront,
            lofty::picture::MimeType::Jpeg,
            JPEG_BYTES,
        );
        let (bytes, content_type) = read_embedded_cover(&[f]).unwrap().expect("cover present");
        assert_eq!(bytes, JPEG_BYTES);
        assert_eq!(content_type, ContentType::Jpeg);
    }

    /// No embedded picture → None (the caller falls through to no cover).
    #[test]
    fn read_embedded_cover_none_when_no_picture() {
        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        let dest = temp.path().join("01.flac");
        fs::copy(&src, &dest).unwrap();
        assert!(read_embedded_cover(&[dest]).unwrap().is_none());
    }

    /// When a file carries both a back and a front cover, the front cover
    /// wins regardless of which was pushed first.
    #[test]
    fn read_embedded_cover_prefers_front_over_other() {
        use lofty::config::WriteOptions;
        use lofty::picture::{MimeType, Picture, PictureType};

        let temp = TempDir::new().unwrap();
        let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
        let dest = temp.path().join("01.flac");
        fs::copy(&src, &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title("Track".to_string());
        // Back cover pushed first, front cover second.
        tag.push_picture(
            Picture::unchecked(vec![0x89, b'P', b'N', b'G'])
                .pic_type(PictureType::CoverBack)
                .mime_type(MimeType::Png)
                .build(),
        );
        tag.push_picture(
            Picture::unchecked(JPEG_BYTES.to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Jpeg)
                .build(),
        );
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();

        let (bytes, content_type) = read_embedded_cover(&[dest])
            .unwrap()
            .expect("cover present");
        assert_eq!(bytes, JPEG_BYTES, "front cover wins over back");
        assert_eq!(content_type, ContentType::Jpeg);
    }

    /// With no front cover, the first embedded picture of any type is used —
    /// a back-cover-only file still yields a cover to seed.
    #[test]
    fn read_embedded_cover_falls_back_to_non_front_picture() {
        let temp = TempDir::new().unwrap();
        let f = copy_with_picture(
            temp.path(),
            "01.flac",
            lofty::picture::PictureType::CoverBack,
            lofty::picture::MimeType::Jpeg,
            JPEG_BYTES,
        );
        let (bytes, content_type) = read_embedded_cover(&[f]).unwrap().expect("cover present");
        assert_eq!(bytes, JPEG_BYTES, "back cover used when no front cover");
        assert_eq!(content_type, ContentType::Jpeg);
    }

    /// A picture in an unsupported MIME (lofty `Tiff`) maps to None rather
    /// than being stored as a cover the library can't render.
    #[test]
    fn read_embedded_cover_none_for_unsupported_mime() {
        let temp = TempDir::new().unwrap();
        let f = copy_with_picture(
            temp.path(),
            "01.flac",
            lofty::picture::PictureType::CoverFront,
            lofty::picture::MimeType::Tiff,
            &[0x49, 0x49, 0x2A, 0x00],
        );
        assert!(read_embedded_cover(&[f]).unwrap().is_none());
    }

    #[test]
    fn read_embedded_cover_returns_err_when_audio_file_cannot_open() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing.flac");

        let err = read_embedded_cover(std::slice::from_ref(&missing)).unwrap_err();

        assert!(
            matches!(&err, ImportError::FileTags { detail }
                if detail.contains("failed to open")
                    && detail.contains("for embedded cover read")
                    && detail.contains(&missing.display().to_string())),
            "expected embedded-cover open error, got {err:?}"
        );
    }

    #[test]
    fn read_embedded_cover_returns_err_when_audio_tags_cannot_be_read() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures")
            .join("audio-format")
            .join("placeholder-dsd.dsf");

        let err = read_embedded_cover(std::slice::from_ref(&path)).unwrap_err();

        assert!(
            matches!(&err, ImportError::FileTags { detail }
                if detail.contains("failed to read embedded cover tags")
                    && detail.contains(&path.display().to_string())),
            "expected embedded-cover tag-read error, got {err:?}"
        );
    }

    #[test]
    fn year_from_tag_reads_structured_date_first() {
        // The structured date() field (ID3v2.4 TDRC / Vorbis DATE) is the
        // primary source, taken ahead of any text-key fallback.
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_date(Timestamp {
            year: 2020,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        });
        tag.insert_text(ItemKey::Year, "1999".to_string());
        assert_eq!(year_from_tag(&tag), Some(2020));

        // A tag with no date information at all yields no year.
        assert_eq!(year_from_tag(&Tag::new(TagType::Id3v2)), None);
    }

    /// With no structured `date()`, year_from_tag falls back to the text keys
    /// in order: Year, then ReleaseDate (TDRL), then OriginalReleaseDate. The
    /// date-shaped keys take the leading four digits.
    #[test]
    fn year_from_tag_reads_text_key_fallbacks() {
        // VorbisComments accepts these text keys (Id3v2 remaps Year onto its
        // structured recording-time frame, so it can't hold a bare Year text
        // item). No structured date() is set, so year_from_tag takes the
        // text-key branch under test.
        let with_key = |key: ItemKey, value: &str| {
            let mut tag = Tag::new(TagType::VorbisComments);
            tag.insert_text(key, value.to_string());
            // Guard: the key must actually be stored, or the test would pass
            // vacuously via the None fallthrough.
            assert!(
                tag.get_string(key).is_some(),
                "VorbisComments did not store {key:?}"
            );
            tag
        };

        assert_eq!(year_from_tag(&with_key(ItemKey::Year, "1988")), Some(1988));
        assert_eq!(
            year_from_tag(&with_key(ItemKey::ReleaseDate, "1991-05-01")),
            Some(1991)
        );
        assert_eq!(
            year_from_tag(&with_key(ItemKey::OriginalReleaseDate, "1972-11")),
            Some(1972)
        );

        // A non-numeric text value is ignored rather than mis-parsed.
        assert_eq!(year_from_tag(&with_key(ItemKey::Year, "notayear")), None);
    }

    #[test]
    fn year_from_cue_date_reads_first_four_digit_run() {
        assert_eq!(year_from_cue_date(Some("1970")), Some(1970));
        assert_eq!(year_from_cue_date(Some("1970-02-03")), Some(1970));
        assert_eq!(year_from_cue_date(Some("2000 / 2004")), Some(2000));
        assert_eq!(year_from_cue_date(Some("rem 99")), None);
        assert_eq!(year_from_cue_date(None), None);
    }

    #[test]
    fn image_content_type_maps_known_and_rejects_non_images() {
        use lofty::picture::MimeType;
        assert_eq!(image_content_type(&MimeType::Jpeg), Some(ContentType::Jpeg));
        assert_eq!(image_content_type(&MimeType::Png), Some(ContentType::Png));
        assert_eq!(image_content_type(&MimeType::Gif), Some(ContentType::Gif));
        assert_eq!(image_content_type(&MimeType::Bmp), Some(ContentType::Bmp));
        // WebP reaches us only as Unknown — lofty has no WebP variant.
        assert_eq!(
            image_content_type(&MimeType::Unknown("image/webp".to_string())),
            Some(ContentType::Webp)
        );
        // A non-image Unknown mime is rejected.
        assert_eq!(
            image_content_type(&MimeType::Unknown("application/octet-stream".to_string())),
            None
        );
        // A non-image known variant is rejected too.
        assert_eq!(image_content_type(&MimeType::Tiff), None);
    }

    #[test]
    fn probe_content_type_labels_m4a_by_codec() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            probe_content_type(&manifest.join("test-fixtures/alac/silence-alac.m4a")),
            Some(ContentType::Alac)
        );
        assert_eq!(
            probe_content_type(&manifest.join("test-fixtures/alac/silence-aac.m4a")),
            Some(ContentType::Aac)
        );
    }

    #[test]
    fn file_tag_format_labels_come_from_probed_content_type() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let fixture_dir = manifest.join("test-fixtures").join("audio-format");
        for (name, expected) in [
            ("placeholder-pcm.wav", "PCM"),
            ("placeholder-pcm.aiff", "PCM"),
            ("placeholder-opus.opus", "Opus"),
            ("placeholder-vorbis.ogg", "Vorbis"),
            ("placeholder-wavpack.wv", "WavPack"),
        ] {
            let parsed = map_tags_with_folder(&[fixture_dir.join(name)], Some("Album Title"))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                parsed.release.pressing.format.as_deref(),
                Some(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn dsd_file_tag_seeding_fails_through_tag_reader() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures")
            .join("audio-format")
            .join("placeholder-dsd.dsf");
        let err = map_tags_with_folder(&[path], Some("Album Title"))
            .expect_err("lofty does not read DSF tags");
        assert!(
            matches!(&err, ImportError::FileTags { detail } if detail.contains("failed to read tags")),
            "{err}"
        );
    }
}
