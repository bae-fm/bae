//! A stable reading of one import candidate's embedded metadata.

use super::folder_scanner::ScannedFile;
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileTagFact {
    pub observation: FileObservation,
    pub title: Option<String>,
    pub track_artist: Option<String>,
    pub album_title: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u16>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    /// The code the track's recording is registered under, as the tag writes
    /// it.
    pub isrc: Option<String>,
    /// The label the tag names (Vorbis `LABEL` or `ORGANIZATION`, ID3
    /// `TPUB`, iTunes `LABEL`).
    pub label: Option<String>,
    /// The copyright line, as the tag writes it.
    pub copyright: Option<String>,
    /// The store the tag says sold the file, when it says so.
    pub store: Option<StoreMarker>,
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

    /// Each file's ISRC, as [`crate::isrc::code`] reads its tag, in the files'
    /// order; a file whose tag holds none, or none that reads as one, adds
    /// nothing.
    pub(crate) fn isrcs(&self) -> Vec<String> {
        self.files
            .iter()
            .filter_map(|fact| fact.isrc.as_deref())
            .filter_map(crate::isrc::code)
            .collect()
    }
}

pub(crate) struct FileTagRead {
    pub title: Option<String>,
    pub track_artist: Option<String>,
    pub album_title: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u16>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub isrc: Option<String>,
    pub label: Option<String>,
    pub copyright: Option<String>,
    pub store: Option<StoreMarker>,
    pub embedded_cover: Option<(Vec<u8>, ContentType)>,
}

pub(crate) trait FileTagReader: Send + Sync {
    fn read(&self, path: &Path) -> Result<FileTagRead, ImportError>;
}

pub(crate) struct LoftyFileTagReader;

impl FileTagReader for LoftyFileTagReader {
    fn read(&self, path: &Path) -> Result<FileTagRead, ImportError> {
        let probe =
            Probe::open(path).map_err(|error| ImportError::file_tags("open", path, error))?;
        let file_type = probe.file_type();
        let tagged = probe
            .read()
            .map_err(|error| ImportError::file_tags("read tags from", path, error))?;
        let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
        let legacy_text = match tag.map(TagExt::tag_type) {
            Some(TagType::Id3v2) => legacy_id3v2_text(path, file_type)?,
            Some(TagType::Id3v1) => legacy_id3v1_text(path)?,
            Some(_) | None => LegacyTagText::default(),
        };
        let (title, track_artist, album_title, album_artist, track_number, disc_number, year) =
            match tag {
                Some(tag) => (
                    non_empty(legacy_text.title.or_else(|| tag.title().map(String::from))),
                    non_empty(
                        legacy_text
                            .track_artist
                            .or_else(|| tag.artist().map(String::from)),
                    ),
                    non_empty(
                        legacy_text
                            .album_title
                            .or_else(|| tag.album().map(String::from)),
                    ),
                    non_empty(
                        legacy_text
                            .album_artist
                            .or_else(|| tag.get_string(ItemKey::AlbumArtist).map(String::from)),
                    ),
                    tag.track(),
                    tag.disk(),
                    year_from_tag(tag),
                ),
                None => (None, None, None, None, None, None, None),
            };
        Ok(FileTagRead {
            title,
            track_artist,
            album_title,
            album_artist,
            year,
            track_number,
            disc_number,
            isrc: non_empty(tag.and_then(|tag| tag.get_string(ItemKey::Isrc).map(String::from))),
            label: non_empty(
                legacy_text
                    .label
                    .or_else(|| tag.and_then(|tag| tag.get_string(ItemKey::Label).map(String::from))),
            ),
            copyright: non_empty(legacy_text.copyright.or_else(|| {
                tag.and_then(|tag| tag.get_string(ItemKey::CopyrightMessage).map(String::from))
            })),
            store: store_marker(path, file_type, tag)?,
            embedded_cover: tag.and_then(embedded_cover_from_tag),
        })
    }
}

#[derive(Default)]
struct LegacyTagText {
    title: Option<String>,
    track_artist: Option<String>,
    album_title: Option<String>,
    album_artist: Option<String>,
    label: Option<String>,
    copyright: Option<String>,
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
    Ok(LegacyTagText {
        title: decoded_latin1_frame(tag, "TIT2"),
        track_artist: decoded_latin1_frame(tag, "TPE1"),
        album_title: decoded_latin1_frame(tag, "TALB"),
        album_artist: decoded_latin1_frame(tag, "TPE2"),
        label: decoded_latin1_frame(tag, "TPUB"),
        copyright: decoded_latin1_frame(tag, "TCOP"),
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
        track_artist: decoded_id3v1_field(&tag[33..63]),
        album_title: decoded_id3v1_field(&tag[63..93]),
        album_artist: None,
        label: None,
        copyright: None,
    })
}

fn decoded_id3v1_field(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    (end > 0).then(|| crate::text_encoding::decode_text(&bytes[..end]).text)
}

fn decoded_latin1_frame(tag: &Id3v2Tag, id: &str) -> Option<String> {
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
                .collect::<Vec<_>>()
                .join("/"),
        );
    }
    Some(crate::text_encoding::decode_text(&bytes).text)
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
            title: read.title,
            track_artist: read.track_artist,
            album_title: read.album_title,
            album_artist: read.album_artist,
            year: read.year,
            track_number: read.track_number,
            disc_number: read.disc_number,
            isrc: read.isrc,
            label: read.label,
            copyright: read.copyright,
            store: read.store,
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
        .map_err(|_| ImportError::FileTags {
            detail: format!(
                "modification time of {} is before the Unix epoch",
                path.display()
            ),
        })?;
    i64::try_from(since_epoch.as_nanos()).map_err(|_| ImportError::FileTags {
        detail: format!(
            "modification time of {} exceeds SQLite's integer range",
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

pub(crate) fn year_from_tag(tag: &lofty::tag::Tag) -> Option<u16> {
    if let Some(timestamp) = tag.date() {
        return Some(timestamp.year);
    }
    if let Some(value) = tag.get_string(ItemKey::Year) {
        if let Ok(year) = value.parse::<u16>() {
            return Some(year);
        }
    }
    if let Some(value) = tag.get_string(ItemKey::ReleaseDate) {
        if let Some(year) = value
            .split('-')
            .next()
            .and_then(|year| year.parse::<u16>().ok())
        {
            return Some(year);
        }
    }
    if let Some(value) = tag.get_string(ItemKey::OriginalReleaseDate) {
        if let Some(year) = value
            .split('-')
            .next()
            .and_then(|year| year.parse::<u16>().ok())
        {
            return Some(year);
        }
    }
    None
}

#[cfg(test)]
#[path = "file_tag_snapshot_tests.rs"]
mod tests;
