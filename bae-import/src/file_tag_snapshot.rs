//! A stable reading of one import candidate's embedded metadata.

use super::file_tag_reader::{FileTagRead, FileTagReader, TagReadFailure};
use super::folder_scanner::ScannedFile;
use super::track_numbers::TrackNumbers;
use super::ImportError;
use bae_catalogs::StoreMarker;
use bae_util::content_type::ContentType;
use coven::{FileLease, LeasePlace};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileObservation {
    pub relative_path: String,
    pub size: u64,
    pub modified_at_ns: i64,
}

impl FileObservation {
    /// `file` as the scan saw it: what a rescan compares a stored reading's
    /// file against to tell whether it changed.
    pub(crate) fn scanned(file: &ScannedFile) -> Self {
        Self {
            relative_path: file.relative_path.clone(),
            size: file.version.size,
            modified_at_ns: file.version.modified_at_ns,
        }
    }

    /// `file` as it stands on disk now: the size and modification time a stat
    /// reads.
    pub(crate) fn of(file: &ScannedFile) -> Result<Self, crate::folder_scanner::FolderScanError> {
        let metadata = std::fs::metadata(&file.version.path).map_err(|source| {
            crate::folder_scanner::FolderScanError::io(&file.version.path, source)
        })?;
        Ok(Self {
            relative_path: file.relative_path.clone(),
            size: metadata.len(),
            modified_at_ns: crate::folder_scanner::file_modified_at_ns(
                &file.version.path,
                &metadata,
            )?,
        })
    }
}

/// One audio file's tags, beside the file as they were read from it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileTagFact {
    pub observation: FileObservation,
    pub tags: TagReading,
}

/// What one audio file's tags were read as.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TagReading {
    /// What they state.
    Stated(FileTags),
    /// They do not parse, for this reason, so they state nothing: the file's
    /// name places and titles its track.
    Unparseable(String),
}

impl TagReading {
    /// What the tags state: nothing, where they do not parse.
    pub(crate) fn stated(&self) -> &FileTags {
        static NOTHING: FileTags = FileTags::untagged();
        match self {
            Self::Stated(tags) => tags,
            Self::Unparseable(_) => &NOTHING,
        }
    }
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
}

impl FileTags {
    /// What a file carrying no tag states: nothing.
    pub(crate) const fn untagged() -> Self {
        Self {
            title: None,
            track_artists: Vec::new(),
            album_title: None,
            album_artists: Vec::new(),
            compilation: false,
            years: Vec::new(),
            numbers: TrackNumbers::unstated(),
        }
    }
}

/// What the label or store that issued a file wrote into its tags.
/// Identification reads these from the file on each run, so a scan's stored
/// reading keeps none of them.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DeliveryTags {
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
    /// files in the same order, each the size and modification time it was
    /// read at. A reading is a reading of the files it names and of nothing
    /// else, so a candidate holding any other audio — or none at all — holds
    /// no reading.
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
                || fact.observation.size != file.version.size
                || fact.observation.modified_at_ns != file.version.modified_at_ns
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
        let Self { scan_generation: _, file_edit_revision, files, embedded_cover } = self;
        *file_edit_revision == other.file_edit_revision
            && *files == other.files
            && *embedded_cover == other.embedded_cover
    }
}

/// A tag reading stored for a release, as a rescan takes it to read the
/// release's files again: each file's tags, and the picture only when the file
/// it came from has not changed, since a changed file is read from disk
/// whatever the stored picture was.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredTagReading {
    pub files: Vec<FileTagFact>,
    pub cover: StoredCover,
}

/// What a [`StoredTagReading`] holds of the picture its release embeds.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StoredCover {
    /// No file embeds a picture.
    Absent,
    Picture(EmbeddedCoverFact),
    /// The first file to embed one is `source_relative_path`, which has
    /// changed since: its stored picture is not taken.
    Replaced {
        source_relative_path: String,
    },
}

pub(crate) fn observe_audio_files(
    audio_files: &[ScannedFile],
) -> Result<Vec<FileObservation>, ImportError> {
    audio_files.iter().map(observe_file).collect()
}

/// `file`'s tags as `reader` reads them, or why they do not parse, with the
/// file as it was while they were read: the scanned size, at the modification
/// time it has now. A read that fails at the file itself says nothing of the
/// file, and fails. The file opens in `place`.
pub(crate) fn read_observed(
    file: &ScannedFile,
    reader: &dyn FileTagReader,
    place: LeasePlace,
) -> Result<(FileObservation, Result<FileTagRead, String>), ImportError> {
    let before = observe_file(file)?;
    let read = reader.read(&file.version.path, place);
    let after = observe_file(file)?;
    if before != after {
        return Err(changed_file_error(file));
    }
    match read {
        Ok(read) => Ok((before, Ok(read))),
        Err(TagReadFailure::Unparseable(reason)) => Ok((before, Err(reason))),
        Err(TagReadFailure::Fault(detail)) => Err(ImportError::FileTags { detail }),
    }
}

/// One audio file's tags as they were read from it, with what the label or
/// store that issued it wrote there: nothing, where its tags do not parse.
pub(crate) struct DeliveredFileTags {
    pub fact: FileTagFact,
    pub delivery: DeliveryTags,
}

/// The tags of each of `audio_files` as `reader` reads them now, in their
/// order, each file opened in a place of `lease`, one after the other. Call it
/// off the async workers.
pub(crate) fn read_delivered_file_tags(
    audio_files: &[ScannedFile],
    reader: &dyn FileTagReader,
    lease: &FileLease,
) -> Result<Vec<DeliveredFileTags>, ImportError> {
    audio_files
        .iter()
        .map(|file| {
            let (observation, read) = read_observed(file, reader, lease.place_blocking())?;
            let (tags, delivery) = match read {
                Ok(read) => (TagReading::Stated(read.tags), read.delivery),
                Err(reason) => (TagReading::Unparseable(reason), DeliveryTags::default()),
            };
            Ok(DeliveredFileTags { fact: FileTagFact { observation, tags }, delivery })
        })
        .collect()
}

pub(crate) fn extract_file_tag_snapshot(
    audio_files: &[ScannedFile],
    scan_generation: u64,
    file_edit_revision: u64,
    reader: &dyn FileTagReader,
    lease: &FileLease,
) -> Result<FileTagSnapshot, ImportError> {
    let mut facts = Vec::with_capacity(audio_files.len());
    let mut embedded_cover = None;
    for file in audio_files {
        let (before, read) = read_observed(file, reader, lease.place_blocking())?;
        let tags = match read {
            Ok(read) => {
                if embedded_cover.is_none() {
                    embedded_cover =
                        read.embedded_cover.map(|(data, content_type)| EmbeddedCoverFact {
                            source_relative_path: file.relative_path.clone(),
                            content_type,
                            data,
                        });
                }
                TagReading::Stated(read.tags)
            }
            Err(reason) => TagReading::Unparseable(reason),
        };
        facts.push(FileTagFact { observation: before, tags });
    }
    Ok(FileTagSnapshot { scan_generation, file_edit_revision, files: facts, embedded_cover })
}

/// `file` as it stands now, refused when its size is no longer the scanned
/// one.
fn observe_file(file: &ScannedFile) -> Result<FileObservation, ImportError> {
    let observation = FileObservation::of(file)
        .map_err(|error| ImportError::FileTags { detail: error.to_string() })?;
    if observation.size != file.version.size {
        return Err(changed_file_error(file));
    }
    Ok(observation)
}

fn changed_file_error(file: &ScannedFile) -> ImportError {
    ImportError::FileTags {
        detail: format!(
            "{} changed after its import candidate was scanned; rescan before reading file tags",
            file.version.path.display()
        ),
    }
}

#[cfg(test)]
#[path = "file_tag_snapshot_tests.rs"]
mod tests;
