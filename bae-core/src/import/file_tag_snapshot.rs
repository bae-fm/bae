//! A stable reading of one import candidate's embedded metadata.

use super::folder_scanner::ScannedFile;
use super::ImportError;
use crate::util::content_type::ContentType;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType};
use lofty::id3::v2::{Frame, Id3v2Tag, Id3v2Version};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::TagType;
use lofty::TextEncoding;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
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

pub(crate) struct FileTagRead {
    pub title: Option<String>,
    pub track_artist: Option<String>,
    pub album_title: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u16>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub embedded_cover: Option<(Vec<u8>, ContentType)>,
}

pub(crate) trait FileTagReader: Send + Sync {
    fn read(&self, path: &Path) -> Result<FileTagRead, ImportError>;
}

pub(crate) struct LoftyFileTagReader;

impl FileTagReader for LoftyFileTagReader {
    fn read(&self, path: &Path) -> Result<FileTagRead, ImportError> {
        let probe = Probe::open(path).map_err(|error| ImportError::FileTags {
            detail: format!("failed to open {}: {error}", path.display()),
        })?;
        let file_type = probe.file_type();
        let tagged = probe.read().map_err(|error| ImportError::FileTags {
            detail: format!("failed to read tags from {}: {error}", path.display()),
        })?;
        let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
        let legacy_text = match tag {
            Some(tag) if tag.tag_type() == TagType::Id3v2 => legacy_id3v2_text(path, file_type)?,
            Some(_) | None => LegacyId3v2Text::default(),
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
            embedded_cover: tag.and_then(embedded_cover_from_tag),
        })
    }
}

#[derive(Default)]
struct LegacyId3v2Text {
    title: Option<String>,
    track_artist: Option<String>,
    album_title: Option<String>,
    album_artist: Option<String>,
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
) -> Result<LegacyId3v2Text, ImportError> {
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

fn read_legacy_id3v2_text<F>(path: &Path) -> Result<LegacyId3v2Text, ImportError>
where
    F: AudioFile + HasId3v2Tag,
{
    let file = File::open(path).map_err(|error| ImportError::FileTags {
        detail: format!("failed to open {}: {error}", path.display()),
    })?;
    let mut reader = BufReader::new(file);
    let parsed =
        F::read_from(&mut reader, ParseOptions::new().read_properties(false)).map_err(|error| {
            ImportError::FileTags {
                detail: format!(
                    "failed to read ID3v2 frames from {}: {error}",
                    path.display()
                ),
            }
        })?;
    let Some(tag) = parsed.id3v2_tag() else {
        return Err(ImportError::FileTags {
            detail: format!(
                "{} was reported as ID3v2-tagged but its concrete tag was absent",
                path.display()
            ),
        });
    };
    Ok(LegacyId3v2Text {
        title: decoded_latin1_frame(tag, "TIT2"),
        track_artist: decoded_latin1_frame(tag, "TPE1"),
        album_title: decoded_latin1_frame(tag, "TALB"),
        album_artist: decoded_latin1_frame(tag, "TPE2"),
    })
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
        });
    }
    Ok(FileTagSnapshot {
        scan_generation,
        file_edit_revision,
        files: facts,
        embedded_cover,
    })
}

/// The cover File Tags applies with its draft. Embedded artwork is part of the
/// same snapshot and therefore leads. Without one, conventional `cover.*` and
/// `folder.*` images lead by case-insensitive path order; every other image is
/// ordered by file size and then by path.
pub(crate) fn default_cover(
    files: &super::folder_scanner::CategorizedFiles,
    snapshot: &FileTagSnapshot,
) -> Option<super::CoverSelection> {
    if let Some(cover) = &snapshot.embedded_cover {
        return Some(super::CoverSelection::Embedded(
            cover.source_relative_path.clone(),
        ));
    }
    super::local_artwork::default_local_cover(files)
}

fn observe_file(file: &ScannedFile) -> Result<FileObservation, ImportError> {
    let metadata = std::fs::metadata(&file.path).map_err(|error| ImportError::FileTags {
        detail: format!("failed to stat {}: {error}", file.path.display()),
    })?;
    if metadata.len() != file.size {
        return Err(changed_file_error(file));
    }
    let modified = metadata.modified().map_err(|error| ImportError::FileTags {
        detail: format!(
            "failed to read modification time of {}: {error}",
            file.path.display()
        ),
    })?;
    let since_epoch = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ImportError::FileTags {
            detail: format!(
                "modification time of {} is before the Unix epoch",
                file.path.display()
            ),
        })?;
    let modified_at_ns =
        i64::try_from(since_epoch.as_nanos()).map_err(|_| ImportError::FileTags {
            detail: format!(
                "modification time of {} exceeds SQLite's integer range",
                file.path.display()
            ),
        })?;
    Ok(FileObservation {
        relative_path: file.relative_path.clone(),
        size: metadata.len(),
        modified_at_ns,
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
    let pictures = tag.pictures();
    let picture = pictures
        .iter()
        .find(|picture| picture.pic_type() == lofty::picture::PictureType::CoverFront)
        .or_else(|| pictures.first())?;
    let content_type = picture.mime_type().and_then(image_content_type)?;
    Some((picture.data().to_vec(), content_type))
}

pub(crate) fn image_content_type(mime: &lofty::picture::MimeType) -> Option<ContentType> {
    use lofty::picture::MimeType;
    match mime {
        MimeType::Jpeg => Some(ContentType::Jpeg),
        MimeType::Png => Some(ContentType::Png),
        MimeType::Gif => Some(ContentType::Gif),
        MimeType::Bmp => Some(ContentType::Bmp),
        MimeType::Unknown(value) => match ContentType::from_mime(value) {
            content_type @ (ContentType::Jpeg
            | ContentType::Png
            | ContentType::Gif
            | ContentType::Bmp
            | ContentType::Webp) => Some(content_type),
            _ => None,
        },
        _ => None,
    }
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

pub fn read_embedded_cover(
    audio_files: &[PathBuf],
) -> Result<Option<(Vec<u8>, ContentType)>, ImportError> {
    for path in audio_files {
        let probe = Probe::open(path).map_err(|error| ImportError::FileTags {
            detail: format!(
                "failed to open {} for embedded cover read: {error}",
                path.display()
            ),
        })?;
        let tagged = probe.read().map_err(|error| ImportError::FileTags {
            detail: format!(
                "failed to read embedded cover tags from {}: {error}",
                path.display()
            ),
        })?;
        if let Some(cover) = tagged
            .primary_tag()
            .or_else(|| tagged.first_tag())
            .and_then(embedded_cover_from_tag)
        {
            return Ok(Some(cover));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACEHOLDER_MP3: &[u8] =
        include_bytes!("../../test-fixtures/audio-format/placeholder-mp3.mp3");

    fn synchsafe(value: usize) -> [u8; 4] {
        [
            ((value >> 21) & 0x7f) as u8,
            ((value >> 14) & 0x7f) as u8,
            ((value >> 7) & 0x7f) as u8,
            (value & 0x7f) as u8,
        ]
    }

    fn legacy_id3v23_frame(id: &[u8; 4], value: &str) -> Vec<u8> {
        let (encoded, _, had_errors) = encoding_rs::WINDOWS_1251.encode(value);
        assert!(!had_errors);
        let size = encoded.len() + 1;
        let mut frame = Vec::with_capacity(10 + size);
        frame.extend_from_slice(id);
        frame.extend_from_slice(&(size as u32).to_be_bytes());
        frame.extend_from_slice(&[0, 0]);
        frame.push(0);
        frame.extend_from_slice(&encoded);
        frame
    }

    fn legacy_cyrillic_mp3() -> Vec<u8> {
        let mut frames = Vec::new();
        frames.extend(legacy_id3v23_frame(b"TIT2", "Название дорожки"));
        frames.extend(legacy_id3v23_frame(b"TPE1", "Имя исполнителя"));
        frames.extend(legacy_id3v23_frame(b"TALB", "Название альбома"));
        frames.extend(legacy_id3v23_frame(b"TPE2", "Исполнитель альбома"));

        let mut file = b"ID3\x03\x00\x00".to_vec();
        file.extend_from_slice(&synchsafe(frames.len()));
        file.extend(frames);

        let existing_tag_size = 10
            + PLACEHOLDER_MP3[6..10]
                .iter()
                .fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
        file.extend_from_slice(&PLACEHOLDER_MP3[existing_tag_size..]);
        file
    }

    fn legacy_cyrillic_multivalue_mp3() -> Vec<u8> {
        let mut frames = legacy_id3v23_frame(b"TPE1", "Исполнитель Один");
        let separator = frames.len();
        frames.extend(legacy_id3v23_frame(b"TPE1", "Исполнитель Два"));
        let second_frame = frames.split_off(separator);
        let second_value = &second_frame[11..];
        let combined_size = frames.len() - 10 + 1 + second_value.len();
        frames[4..8].copy_from_slice(&(combined_size as u32).to_be_bytes());
        frames.push(0);
        frames.extend_from_slice(second_value);

        let mut file = b"ID3\x04\x00\x00".to_vec();
        file.extend_from_slice(&synchsafe(frames.len()));
        file.extend(frames);
        let existing_tag_size = 10
            + PLACEHOLDER_MP3[6..10]
                .iter()
                .fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
        file.extend_from_slice(&PLACEHOLDER_MP3[existing_tag_size..]);
        file
    }

    fn artwork(relative_path: &str, size: u64) -> ScannedFile {
        ScannedFile::new(
            PathBuf::from("/candidate").join(relative_path),
            relative_path.to_string(),
            size,
            1,
            format!("{size:064x}"),
        )
    }

    fn files_with_artwork(
        artwork: impl IntoIterator<Item = ScannedFile>,
    ) -> super::super::folder_scanner::CategorizedFiles {
        use super::super::folder_scanner::{CandidateFile, CategorizedFiles, FileRole};
        CategorizedFiles {
            files: artwork
                .into_iter()
                .map(|file| CandidateFile {
                    file,
                    role: FileRole::Artwork,
                    proposed_audio: false,
                })
                .collect(),
        }
    }

    fn snapshot(embedded_cover: Option<EmbeddedCoverFact>) -> FileTagSnapshot {
        FileTagSnapshot {
            scan_generation: 1,
            file_edit_revision: 0,
            files: Vec::new(),
            embedded_cover,
        }
    }

    #[derive(Default)]
    struct CountingReader(std::sync::atomic::AtomicUsize);

    impl FileTagReader for CountingReader {
        fn read(&self, _path: &Path) -> Result<FileTagRead, ImportError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(FileTagRead {
                title: Some("Track Alpha".to_string()),
                track_artist: Some("Artist Alpha".to_string()),
                album_title: Some("Album Alpha".to_string()),
                album_artist: Some("Artist Alpha".to_string()),
                year: Some(2001),
                track_number: Some(1),
                disc_number: None,
                embedded_cover: None,
            })
        }
    }

    #[test]
    fn lofty_reader_decodes_legacy_cyrillic_id3_text() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("01.mp3");
        std::fs::write(&path, legacy_cyrillic_mp3()).unwrap();

        let read = LoftyFileTagReader.read(&path).unwrap();

        assert_eq!(read.title.as_deref(), Some("Название дорожки"));
        assert_eq!(read.track_artist.as_deref(), Some("Имя исполнителя"));
        assert_eq!(read.album_title.as_deref(), Some("Название альбома"));
        assert_eq!(read.album_artist.as_deref(), Some("Исполнитель альбома"));
    }

    #[test]
    fn lofty_reader_preserves_id3v24_multivalue_normalization() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("01.mp3");
        std::fs::write(&path, legacy_cyrillic_multivalue_mp3()).unwrap();

        let read = LoftyFileTagReader.read(&path).unwrap();

        assert_eq!(
            read.track_artist.as_deref(),
            Some("Исполнитель Один/Исполнитель Два")
        );
    }

    #[test]
    fn extraction_reads_every_file_before_returning_one_snapshot() {
        let dir = tempfile::TempDir::new().unwrap();
        let paths = [dir.path().join("01.flac"), dir.path().join("02.flac")];
        for path in &paths {
            std::fs::write(path, b"audio").unwrap();
        }
        let files = paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                ScannedFile::new(
                    path.clone(),
                    format!("{:02}.flac", index + 1),
                    5,
                    1,
                    crate::util::fs::hash_file(path).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let reader = CountingReader::default();

        let snapshot = extract_file_tag_snapshot(&files, 8, 2, &reader).unwrap();

        assert_eq!(snapshot.files.len(), 2);
        assert_eq!(reader.0.load(std::sync::atomic::Ordering::Relaxed), 2);
        assert_eq!(
            snapshot
                .files
                .iter()
                .map(|fact| &fact.observation)
                .collect::<Vec<_>>(),
            observe_audio_files(&files)
                .unwrap()
                .iter()
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn extraction_refuses_a_file_whose_scanned_size_changed() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("01.flac");
        std::fs::write(&path, b"changed").unwrap();
        let file = ScannedFile::new(path, "01.flac".to_string(), 5, 1, "0".repeat(64));
        let reader = CountingReader::default();

        let error = extract_file_tag_snapshot(&[file], 1, 0, &reader).unwrap_err();

        assert!(
            matches!(error, ImportError::FileTags { detail } if detail.contains("changed after"))
        );
        assert_eq!(reader.0.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn embedded_artwork_leads_every_folder_image() {
        let files = files_with_artwork([artwork("cover.jpg", 1), artwork("folder.png", 1)]);
        let snapshot = snapshot(Some(EmbeddedCoverFact {
            source_relative_path: "01.flac".to_string(),
            content_type: ContentType::Jpeg,
            data: vec![1, 2, 3],
        }));

        assert_eq!(
            default_cover(&files, &snapshot),
            Some(super::super::CoverSelection::Embedded(
                "01.flac".to_string()
            ))
        );
    }

    #[test]
    fn missing_embedded_artwork_uses_source_neutral_folder_selection() {
        let files = files_with_artwork([artwork("scan.jpg", 1), artwork("cover.jpg", 500)]);

        assert_eq!(
            default_cover(&files, &snapshot(None)),
            Some(super::super::CoverSelection::Local("cover.jpg".to_string()))
        );
    }
}
