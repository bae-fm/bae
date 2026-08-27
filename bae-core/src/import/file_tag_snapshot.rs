//! A stable reading of one import candidate's embedded metadata.

use super::folder_scanner::ScannedFile;
use super::ImportError;
use crate::util::content_type::ContentType;
use lofty::prelude::*;
use lofty::probe::Probe;
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
    pub content_type: Option<ContentType>,
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
    pub content_type: Option<ContentType>,
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
        let tagged = probe.read().map_err(|error| ImportError::FileTags {
            detail: format!("failed to read tags from {}: {error}", path.display()),
        })?;
        let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
        let (title, track_artist, album_title, album_artist, track_number, disc_number, year) =
            match tag {
                Some(tag) => (
                    non_empty(tag.title().map(|value| value.to_string())),
                    non_empty(tag.artist().map(|value| value.to_string())),
                    non_empty(tag.album().map(|value| value.to_string())),
                    non_empty(
                        tag.get_string(ItemKey::AlbumArtist)
                            .map(|value| value.to_string()),
                    ),
                    tag.track(),
                    tag.disk(),
                    year_from_tag(tag),
                ),
                None => (None, None, None, None, None, None, None),
            };
        Ok(FileTagRead {
            content_type: probe_content_type(path),
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
            content_type: read.content_type,
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

pub(crate) fn probe_content_type(path: &Path) -> Option<ContentType> {
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

    #[derive(Default)]
    struct CountingReader(std::sync::atomic::AtomicUsize);

    impl FileTagReader for CountingReader {
        fn read(&self, _path: &Path) -> Result<FileTagRead, ImportError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(FileTagRead {
                content_type: Some(ContentType::Flac),
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
                ScannedFile::new(path.clone(), format!("{:02}.flac", index + 1), 5)
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
        let file = ScannedFile::new(path, "01.flac".to_string(), 5);
        let reader = CountingReader::default();

        let error = extract_file_tag_snapshot(&[file], 1, 0, &reader).unwrap_err();

        assert!(
            matches!(error, ImportError::FileTags { detail } if detail.contains("changed after"))
        );
        assert_eq!(reader.0.load(std::sync::atomic::Ordering::Relaxed), 0);
    }
}
