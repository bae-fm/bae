//! A stable reading of one import candidate's embedded metadata.

use super::folder_scanner::ScannedFile;
use super::track_numbers::TrackNumbers;
use super::ImportError;
use crate::signals::StoreMarker;
use crate::util::content_type::ContentType;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType};
use lofty::id3::v2::{Frame, Id3v2Tag, Id3v2Version};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::TagType;
use lofty::TextEncoding;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileObservation {
    pub relative_path: String,
    pub size: u64,
    pub modified_at_ns: i64,
}

/// One audio file's tags, beside the file as they were read from it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileTagFact {
    pub observation: FileObservation,
    pub tags: FileTags,
}

/// What one file's tags state. A tag that holds a value more than once — two
/// `ARTIST` comments, an ID3v2.4 frame of two values — states each, in its
/// order; a blank value states nothing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileTags {
    pub title: Option<String>,
    pub track_artists: Vec<String>,
    pub album_title: Option<String>,
    pub album_artists: Vec<String>,
    /// Whether the tags mark the file part of a compilation (Vorbis
    /// `COMPILATION`, ID3 `TCMP`, iTunes `cpil`).
    pub compilation: bool,
    /// Every year a date the tags state names — the recording date (`DATE`,
    /// ID3 `TDRC`, MP4 `©day`), `YEAR`, the release date and the original
    /// release date — each once, in the order the tags state them.
    pub years: Vec<u16>,
    /// The disc and track numbers the tags state.
    pub numbers: TrackNumbers,
    /// The codes the track's recording is registered under, as the tag
    /// writes them.
    pub isrcs: Vec<String>,
    /// The labels the tag names (Vorbis `LABEL` or `ORGANIZATION`, ID3
    /// `TPUB`, iTunes `LABEL`).
    pub labels: Vec<String>,
    /// The copyright lines, as the tag writes them.
    pub copyrights: Vec<String>,
    /// The store the tags say sold the file, when they say so.
    pub store: Option<StoreMarker>,
}

impl FileTags {
    /// What a file carrying no tag states: nothing.
    pub(crate) fn untagged() -> Self {
        Self {
            title: None,
            track_artists: Vec::new(),
            album_title: None,
            album_artists: Vec::new(),
            compilation: false,
            years: Vec::new(),
            numbers: TrackNumbers::unstated(),
            isrcs: Vec::new(),
            labels: Vec::new(),
            copyrights: Vec::new(),
            store: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EmbeddedCoverFact {
    pub source_relative_path: String,
    pub content_type: ContentType,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileTagSnapshot {
    pub scan_generation: u64,
    pub file_edit_revision: u64,
    pub files: Vec<FileTagFact>,
    pub embedded_cover: Option<EmbeddedCoverFact>,
}

impl FileTagSnapshot {
    /// Whether this reading was taken from exactly `audio_files`: the same
    /// files in the same order, each the size it was read at. A reading is a
    /// reading of the files it names and of nothing else, so a candidate
    /// holding any other audio — or none at all — holds no reading.
    pub(crate) fn was_read_from<'a>(
        &self,
        audio_files: impl Iterator<Item = &'a ScannedFile>,
    ) -> bool {
        let mut facts = self.files.iter();
        for file in audio_files {
            let Some(fact) = facts.next() else {
                return false;
            };
            if fact.observation.relative_path != file.relative_path
                || fact.observation.size != file.size
            {
                return false;
            }
        }
        facts.next().is_none()
    }

    /// Whether `other` says what this reading says about the same files at the
    /// same file edit revision. The scan stamp is left out: a pass that finds
    /// the candidate unchanged moves its stamp onto the reading it keeps.
    pub(crate) fn same_reading(&self, other: &Self) -> bool {
        let Self {
            scan_generation: _,
            file_edit_revision,
            files,
            embedded_cover,
        } = self;
        *file_edit_revision == other.file_edit_revision
            && *files == other.files
            && *embedded_cover == other.embedded_cover
    }

    /// Each file's ISRCs, as [`crate::isrc::code`] reads its tag, in the
    /// files' order; a value that reads as no code adds nothing.
    pub(crate) fn isrcs(&self) -> Vec<String> {
        self.files
            .iter()
            .flat_map(|fact| &fact.tags.isrcs)
            .filter_map(|isrc| crate::isrc::code(isrc))
            .collect()
    }
}

pub(crate) struct FileTagRead {
    pub tags: FileTags,
    pub embedded_cover: Option<(Vec<u8>, ContentType)>,
}

pub(crate) trait FileTagReader: Send + Sync {
    fn read(&self, path: &Path) -> Result<FileTagRead, ImportError>;
}

pub(crate) struct LoftyFileTagReader;

/// The tag a file's other tags defer to: its format's own, else the first it
/// carries.
fn primary_tag(tagged: &lofty::file::TaggedFile) -> Option<&lofty::tag::Tag> {
    tagged.primary_tag().or_else(|| tagged.first_tag())
}

/// The disc and track numbers `path`'s tags state, read from the same tag
/// [`LoftyFileTagReader`] reads, without its audio properties or artwork.
pub(crate) fn read_tag_numbers(path: &Path) -> Result<TrackNumbers, ImportError> {
    let tagged = Probe::open(path)
        .map_err(|error| ImportError::file_tags("open", path, error))?
        .options(
            ParseOptions::new()
                .read_properties(false)
                .read_cover_art(false),
        )
        .read()
        .map_err(|error| ImportError::file_tags("read tags from", path, error))?;
    Ok(match primary_tag(&tagged) {
        Some(tag) => TrackNumbers::of_tag(tag),
        None => TrackNumbers::unstated(),
    })
}

impl FileTagReader for LoftyFileTagReader {
    fn read(&self, path: &Path) -> Result<FileTagRead, ImportError> {
        let probe =
            Probe::open(path).map_err(|error| ImportError::file_tags("open", path, error))?;
        let file_type = probe.file_type();
        let tagged = probe
            .read()
            .map_err(|error| ImportError::file_tags("read tags from", path, error))?;
        let tag = primary_tag(&tagged);
        let store = store_marker(path, file_type, tag)?;
        let Some(tag) = tag else {
            return Ok(FileTagRead {
                tags: FileTags {
                    store,
                    ..FileTags::untagged()
                },
                embedded_cover: None,
            });
        };
        let legacy = match tag.tag_type() {
            TagType::Id3v2 => legacy_id3v2_text(path, file_type)?,
            TagType::Id3v1 => legacy_id3v1_text(path)?,
            _ => LegacyTagText::default(),
        };
        let values = |legacy: Option<Vec<String>>, key: ItemKey| {
            stated_values(legacy.unwrap_or_else(|| {
                tag.get_strings(key).map(String::from).collect()
            }))
        };
        Ok(FileTagRead {
            tags: FileTags {
                title: non_empty(legacy.title.or_else(|| tag.title().map(String::from))),
                track_artists: values(legacy.track_artists, ItemKey::TrackArtist),
                album_title: non_empty(legacy.album_title.or_else(|| tag.album().map(String::from))),
                album_artists: values(legacy.album_artists, ItemKey::AlbumArtist),
                compilation: tag
                    .get_string(ItemKey::FlagCompilation)
                    .is_some_and(|flag| flag.trim() == "1"),
                years: tag_years(tag),
                numbers: TrackNumbers::of_tag(tag),
                isrcs: values(None, ItemKey::Isrc),
                labels: values(legacy.labels, ItemKey::Label),
                copyrights: values(legacy.copyrights, ItemKey::CopyrightMessage),
                store,
            },
            embedded_cover: embedded_cover_from_tag(tag),
        })
    }
}

/// `values` as a tag states them: each trimmed, blank ones dropped, and a
/// value written twice kept once.
fn stated_values(values: Vec<String>) -> Vec<String> {
    let mut stated: Vec<String> = Vec::with_capacity(values.len());
    for value in values {
        if let Some(value) = non_empty(Some(value)) {
            if !stated.contains(&value) {
                stated.push(value);
            }
        }
    }
    stated
}

/// The text of a tag that names its own encoding wrongly, decoded from its
/// bytes (see [`crate::text_encoding`]). A field is `None` where the tag holds
/// no such text for it, and the tag's own reading of it stands.
#[derive(Default)]
struct LegacyTagText {
    title: Option<String>,
    track_artists: Option<Vec<String>>,
    album_title: Option<String>,
    album_artists: Option<Vec<String>>,
    labels: Option<Vec<String>>,
    copyrights: Option<Vec<String>>,
}

/// The store a file's tags say sold it, if any — see [`StoreMarker`].
fn store_marker(
    path: &Path,
    file_type: Option<FileType>,
    tag: Option<&lofty::tag::Tag>,
) -> Result<Option<StoreMarker>, ImportError> {
    if file_type == Some(FileType::Mp4) && itunes_purchase(path)? {
        return Ok(Some(StoreMarker::ITunesPurchase));
    }
    let bandcamp = tag
        .and_then(|tag| tag.get_string(ItemKey::Comment))
        .is_some_and(|comment| {
            comment.trim_start().starts_with("Visit ") && comment.contains(".bandcamp.com")
        });
    Ok(bandcamp.then_some(StoreMarker::Bandcamp))
}

/// Whether an MP4 file carries an atom iTunes writes only into a purchase.
fn itunes_purchase(path: &Path) -> Result<bool, ImportError> {
    use lofty::mp4::{AtomIdent, Mp4File};
    let file = File::open(path).map_err(|error| ImportError::file_tags("open", path, error))?;
    let mut reader = BufReader::new(file);
    let parsed = Mp4File::read_from(&mut reader, ParseOptions::new().read_properties(false))
        .map_err(|error| ImportError::file_tags("read MP4 atoms from", path, error))?;
    Ok(parsed.ilst().is_some_and(|ilst| {
        [*b"purd", *b"apID", *b"ownr"]
            .iter()
            .any(|atom| ilst.get(&AtomIdent::Fourcc(*atom)).is_some())
    }))
}

trait HasId3v2Tag {
    fn id3v2_tag(&self) -> Option<&Id3v2Tag>;
}

macro_rules! impl_has_id3v2_tag {
    ($($type:path),+ $(,)?) => {
        $(
            impl HasId3v2Tag for $type {
                fn id3v2_tag(&self) -> Option<&Id3v2Tag> {
                    self.id3v2()
                }
            }
        )+
    };
}

impl_has_id3v2_tag!(
    lofty::aac::AacFile,
    lofty::ape::ApeFile,
    lofty::flac::FlacFile,
    lofty::iff::aiff::AiffFile,
    lofty::iff::wav::WavFile,
    lofty::mpeg::MpegFile,
    lofty::musepack::MpcFile,
);

fn legacy_id3v2_text(
    path: &Path,
    file_type: Option<FileType>,
) -> Result<LegacyTagText, ImportError> {
    match file_type {
        Some(FileType::Aac) => read_legacy_id3v2_text::<lofty::aac::AacFile>(path),
        Some(FileType::Aiff) => read_legacy_id3v2_text::<lofty::iff::aiff::AiffFile>(path),
        Some(FileType::Ape) => read_legacy_id3v2_text::<lofty::ape::ApeFile>(path),
        Some(FileType::Flac) => read_legacy_id3v2_text::<lofty::flac::FlacFile>(path),
        Some(FileType::Mpeg) => read_legacy_id3v2_text::<lofty::mpeg::MpegFile>(path),
        Some(FileType::Mpc) => read_legacy_id3v2_text::<lofty::musepack::MpcFile>(path),
        Some(FileType::Wav) => read_legacy_id3v2_text::<lofty::iff::wav::WavFile>(path),
        Some(_) | None => Err(ImportError::FileTags {
            detail: format!(
                "{} exposed an ID3v2 tag through an audio format that cannot preserve its frame encoding",
                path.display()
            ),
        }),
    }
}

fn read_legacy_id3v2_text<F>(path: &Path) -> Result<LegacyTagText, ImportError>
where
    F: AudioFile + HasId3v2Tag,
{
    let file = File::open(path).map_err(|error| ImportError::file_tags("open", path, error))?;
    let mut reader = BufReader::new(file);
    let parsed = F::read_from(&mut reader, ParseOptions::new().read_properties(false))
        .map_err(|error| ImportError::file_tags("read ID3v2 frames from", path, error))?;
    let Some(tag) = parsed.id3v2_tag() else {
        return Err(ImportError::FileTags {
            detail: format!(
                "{} was reported as ID3v2-tagged but its concrete tag was absent",
                path.display()
            ),
        });
    };
    let joined = |values: Option<Vec<String>>| values.map(|values| values.join("/"));
    Ok(LegacyTagText {
        title: joined(decoded_latin1_frame(tag, "TIT2")),
        track_artists: decoded_latin1_frame(tag, "TPE1"),
        album_title: joined(decoded_latin1_frame(tag, "TALB")),
        album_artists: decoded_latin1_frame(tag, "TPE2"),
        labels: decoded_latin1_frame(tag, "TPUB"),
        copyrights: decoded_latin1_frame(tag, "TCOP"),
    })
}

fn legacy_id3v1_text(path: &Path) -> Result<LegacyTagText, ImportError> {
    let file = File::open(path).map_err(|error| ImportError::file_tags("open", path, error))?;
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::End(-128))
        .map_err(|error| ImportError::file_tags("seek to the ID3v1 tag in", path, error))?;
    let mut tag = [0_u8; 128];
    reader
        .read_exact(&mut tag)
        .map_err(|error| ImportError::file_tags("read the ID3v1 tag from", path, error))?;
    if tag[..3] != *b"TAG" {
        return Err(ImportError::FileTags {
            detail: format!(
                "{} was reported as ID3v1-tagged but its tag footer was absent",
                path.display()
            ),
        });
    }
    Ok(LegacyTagText {
        title: decoded_id3v1_field(&tag[3..33]),
        track_artists: decoded_id3v1_field(&tag[33..63]).map(|artist| vec![artist]),
        album_title: decoded_id3v1_field(&tag[63..93]),
        album_artists: None,
        labels: None,
        copyrights: None,
    })
}

fn decoded_id3v1_field(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    (end > 0).then(|| crate::text_encoding::decode_text(&bytes[..end]).text)
}

/// The values of the Latin-1 text frame `id`, each decoded from its bytes: an
/// ID3v2.4 frame separates its values with a NUL, an older one holds one.
fn decoded_latin1_frame(tag: &Id3v2Tag, id: &str) -> Option<Vec<String>> {
    let frame = tag.into_iter().find(|frame| frame.id_str() == id)?;
    let Frame::Text(text) = frame else {
        return None;
    };
    if text.encoding != TextEncoding::Latin1 {
        return None;
    }
    let bytes = text
        .value
        .chars()
        .map(|character| {
            u8::try_from(u32::from(character))
                .expect("Lofty's Latin-1 decoder emits only byte-valued scalars")
        })
        .collect::<Vec<_>>();
    if tag.original_version() == Id3v2Version::V4 {
        return Some(
            bytes
                .split(|byte| *byte == 0)
                .map(|value| crate::text_encoding::decode_text(value).text)
                .collect(),
        );
    }
    Some(vec![crate::text_encoding::decode_text(&bytes).text])
}

pub(crate) fn observe_audio_files(
    audio_files: &[ScannedFile],
) -> Result<Vec<FileObservation>, ImportError> {
    audio_files.iter().map(observe_file).collect()
}

pub(crate) fn extract_file_tag_snapshot(
    audio_files: &[ScannedFile],
    scan_generation: u64,
    file_edit_revision: u64,
    reader: &dyn FileTagReader,
) -> Result<FileTagSnapshot, ImportError> {
    let observations = observe_audio_files(audio_files)?;
    let mut facts = Vec::with_capacity(audio_files.len());
    let mut embedded_cover = None;
    for (file, before) in audio_files.iter().zip(observations) {
        let read = reader.read(&file.path)?;
        let after = observe_file(file)?;
        if before != after {
            return Err(changed_file_error(file));
        }
        if embedded_cover.is_none() {
            embedded_cover = read
                .embedded_cover
                .map(|(data, content_type)| EmbeddedCoverFact {
                    source_relative_path: file.relative_path.clone(),
                    content_type,
                    data,
                });
        }
        facts.push(FileTagFact {
            observation: before,
            tags: read.tags,
        });
    }
    Ok(FileTagSnapshot {
        scan_generation,
        file_edit_revision,
        files: facts,
        embedded_cover,
    })
}

/// Embedded artwork is a file-metadata selection because its bytes belong to the
/// snapshot. Folder artwork remains the candidate's source-neutral fallback.
pub(crate) fn embedded_cover_selection(
    snapshot: &FileTagSnapshot,
) -> Option<super::CoverSelection> {
    snapshot
        .embedded_cover
        .as_ref()
        .and_then(|cover| embedded_cover_of(&cover.source_relative_path, &cover.content_type))
}

/// The selection one embedded cover names, when its bytes are an image the
/// import can store. The one rule, whether the fact is a snapshot in hand or
/// a stored row read back.
pub(crate) fn embedded_cover_of(
    source_relative_path: &str,
    content_type: &ContentType,
) -> Option<super::CoverSelection> {
    content_type
        .is_supported_cover()
        .then(|| super::CoverSelection::Embedded(source_relative_path.to_string()))
}

fn observe_file(file: &ScannedFile) -> Result<FileObservation, ImportError> {
    let metadata = std::fs::metadata(&file.path)
        .map_err(|error| ImportError::file_tags("stat", &file.path, error))?;
    if metadata.len() != file.size {
        return Err(changed_file_error(file));
    }
    Ok(FileObservation {
        relative_path: file.relative_path.clone(),
        modified_at_ns: modified_at_nanos(&file.path, &metadata)?,
        size: metadata.len(),
    })
}

/// The stamp a file is compared by: its modification time as nanoseconds since
/// the Unix epoch, which is how the scan recorded it and how SQLite stores it.
pub(crate) fn modified_at_nanos(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<i64, ImportError> {
    let since_epoch = metadata
        .modified()
        .map_err(|error| ImportError::file_tags("read modification time of", path, error))?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ImportError::FileTags {
            detail: format!(
                "modification time of {} is before the Unix epoch: {error}",
                path.display()
            ),
        })?;
    i64::try_from(since_epoch.as_nanos()).map_err(|error| ImportError::FileTags {
        detail: format!(
            "modification time of {} exceeds SQLite's integer range: {error}",
            path.display()
        ),
    })
}

fn changed_file_error(file: &ScannedFile) -> ImportError {
    ImportError::FileTags {
        detail: format!(
            "{} changed after its import candidate was scanned; rescan before reading file tags",
            file.path.display()
        ),
    }
}

pub(crate) fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn embedded_cover_from_tag(tag: &lofty::tag::Tag) -> Option<(Vec<u8>, ContentType)> {
    let mut pictures = tag.pictures().iter().filter_map(|picture| {
        picture
            .mime_type()
            .and_then(image_content_type)
            .map(|content_type| (picture, content_type))
    });
    let (picture, content_type) = pictures
        .clone()
        .find(|(picture, _)| picture.pic_type() == lofty::picture::PictureType::CoverFront)
        .or_else(|| pictures.next())?;
    Some((picture.data().to_vec(), content_type))
}

pub(crate) fn image_content_type(mime: &lofty::picture::MimeType) -> Option<ContentType> {
    let content_type = ContentType::from_mime(mime.as_str());
    content_type.is_supported_cover().then_some(content_type)
}

/// Every year the dates `tag` states name, each once, in the order stated:
/// the recording date, the year, the release date and the original release
/// date, each as many times as the tag holds it.
fn tag_years(tag: &lofty::tag::Tag) -> Vec<u16> {
    let mut years = Vec::new();
    for key in [
        ItemKey::RecordingDate,
        ItemKey::Year,
        ItemKey::ReleaseDate,
        ItemKey::OriginalReleaseDate,
    ] {
        for value in tag.get_strings(key) {
            for year in years_in_date(value) {
                if !years.contains(&year) {
                    years.push(year);
                }
            }
        }
    }
    years
}

/// The years a date as a tag or CUE sheet writes it names: the first four
/// digits of each run of four or more — a bare year ("1970"), a date
/// ("1970-03-01", "19700301"), a range ("2000 / 2004"). A run of zeros names
/// no year.
pub(crate) fn years_in_date(date: &str) -> impl Iterator<Item = u16> + '_ {
    date.split(|c: char| !c.is_ascii_digit())
        .filter(|run| run.len() >= 4)
        .map(|run| {
            run[..4]
                .parse::<u16>()
                .expect("four ASCII digits parse as u16")
        })
        .filter(|year| *year != 0)
}

#[cfg(test)]
#[path = "file_tag_snapshot_tests.rs"]
mod tests;
