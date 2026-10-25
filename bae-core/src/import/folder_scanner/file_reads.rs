//! What a pass knows it has read of each file.
//!
//! A file is the same file while its absolute path, size and modification
//! time are the ones a read was taken under ([`FileVersion`]): the identity
//! the folder hash fingerprints. What a read found about it then still holds —
//! the stream FFmpeg found in a track and what its tags state, whether an image
//! decodes, what a track sheet parses to. A pass starts from what the root's
//! stored rows say the last pass read, and adds what it reads itself, so it
//! opens no file whose reading it holds: neither one the last pass read nor one
//! this pass read for another reading of its folders, nor, when a release's
//! draft is seeded, one the walk read to place the release's tracks.

use super::*;
use crate::import::file_tag_snapshot::{
    EmbeddedCoverFact, FileObservation, FileTagFact, FileTagRead, FileTagReader, FileTagSnapshot,
    FileTags,
};
use crate::import::track_numbers::TrackNumbers;
use crate::import::ImportError;
use crate::util::content_type::ContentType;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// How many bytes of the pictures tracks embed a pass holds at once. A
/// release's draft takes its first track's picture, read by the walk, and is
/// seeded a few releases behind the walk; a pass over a library holds no more
/// than this however many releases it reads, and a draft whose picture it let
/// go reads that one track again.
pub(super) const HELD_COVER_BYTES: usize = 64 * 1024 * 1024;

/// Whether one track plays, as FFmpeg reads it.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Playability {
    /// It holds no stream bae plays.
    Unplayable,
    /// A stream bae plays, as the probe measured it.
    Measured(ScannedAudio),
    /// A stream bae plays, as the row of a folder that can't be imported says.
    /// Such a row keeps no measurements, so a release that takes the track in
    /// measures it.
    Playable,
}

/// What is known of one track. Its stream and its tags are known apart: a
/// folder that can't be imported is judged by its streams alone, and a stored
/// reading of a track's tags may outlive the row that measured it.
#[derive(Debug, Clone, PartialEq)]
struct TrackRead {
    stream: Option<Playability>,
    tags: Option<TagRead>,
}

/// What a track's tags were read as.
#[derive(Debug, Clone, PartialEq)]
enum TagRead {
    /// What they state, and what is known of the picture they embed.
    Read { tags: FileTags, cover: EmbeddedCover },
    /// They could not be read, for this reason.
    Unreadable(String),
}

/// What a pass knows of the picture a track's tags embed.
#[derive(Debug, Clone, PartialEq)]
enum EmbeddedCover {
    /// Nothing: the tags are a stored reading's, which keeps a release's
    /// picture rather than each track's.
    Unknown,
    /// They embed none.
    Absent,
    /// This picture, held while a draft may still take it. `kept` once a
    /// reading of its folder takes it as its release's picture.
    Held {
        data: Vec<u8>,
        content_type: ContentType,
        kept: bool,
    },
    /// They embed one the pass no longer holds.
    Released,
}

#[derive(Debug, Clone, PartialEq)]
enum FileRead {
    Track(TrackRead),
    Image { decodes: bool },
    /// A track sheet, as it parsed: `None` for one that does not.
    Sheet(Option<CueSheet>),
}

#[derive(Default)]
struct Known {
    files: HashMap<FileVersion, FileRead>,
    /// What the tags of a folder that can't be imported call the release, by
    /// the folder's playable tracks in name order: the files that reading
    /// read, in the order it read them.
    skipped_tags: HashMap<Vec<FileVersion>, Option<SkippedTags>>,
    /// The tracks whose pictures are held, in the order they were read.
    held: VecDeque<FileVersion>,
    held_bytes: usize,
}

/// Every file a pass knows it has read, by its [`FileVersion`], and what
/// reads the tags of a track it has not.
///
/// Shared by everything one pass reads its folders and seeds its drafts
/// with, and only by that: a pass over another root, or a later one, starts
/// from the store again.
pub(crate) struct FileReads {
    known: Mutex<Known>,
    tags: Arc<dyn FileTagReader>,
}

impl FileReads {
    /// Nothing read yet; a track's tags are read through `tags`.
    pub(crate) fn new(tags: Arc<dyn FileTagReader>) -> Self {
        Self {
            known: Mutex::new(Known::default()),
            tags,
        }
    }

    /// What the scan entries stored for one root say their pass read, and
    /// the tags read from their releases' tracks, each by the version of the
    /// file it was read from; the tags of any other track are read through
    /// `tags`.
    ///
    /// Two rows that say different things about one version of a file are a
    /// store no scan wrote, so the pass fails rather than take either.
    pub(crate) fn stored(
        entries: &[ScanItem],
        stored_tags: &[(FileVersion, FileTags)],
        tags: Arc<dyn FileTagReader>,
    ) -> Result<Self, FolderScanError> {
        let mut known = Known::default();
        for entry in entries {
            match entry {
                ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                    for file in &candidate.files.files {
                        known.learn_release_file(file)?;
                    }
                }
                ScanItem::Invalid(candidate) => {
                    for file in &candidate.files {
                        known.learn_skipped_file(file)?;
                    }
                    let playable: Vec<FileVersion> = candidate
                        .files
                        .iter()
                        .filter(|file| file.role == ProposedRole::Audio)
                        .map(|file| file.file.clone())
                        .collect();
                    if !playable.is_empty() {
                        known.learn_skipped_tags(playable, candidate.tags.clone())?;
                    }
                }
                // A sheet is never parsed as a sidecar file, so only its
                // images say anything.
                ScanItem::Sidecar(sidecar) => match &sidecar.files {
                    SidecarFiles::Valid(files) => {
                        for file in files {
                            if matches!(file.role, FileRole::Artwork | FileRole::DamagedImage) {
                                known.learn_release_file(file)?;
                            }
                        }
                    }
                    SidecarFiles::Invalid(files) => {
                        for file in files {
                            known.learn_skipped_file(file)?;
                        }
                    }
                    SidecarFiles::Downloading => {}
                },
                ScanItem::Decided { key, .. } => {
                    return Err(FolderScanError::Other(format!(
                        "{} is a folder reading, which is stored as a decision, not as a scan \
                         entry",
                        key.relative_folder_path
                    )))
                }
            }
        }
        for (version, file_tags) in stored_tags {
            known.learn(
                version.clone(),
                FileRead::Track(TrackRead {
                    stream: None,
                    tags: Some(TagRead::Read {
                        tags: file_tags.clone(),
                        cover: EmbeddedCover::Unknown,
                    }),
                }),
            )?;
        }
        Ok(Self {
            known: Mutex::new(known),
            tags,
        })
    }

    fn with_known<T>(&self, action: impl FnOnce(&mut Known) -> T) -> T {
        action(&mut self.known.lock().expect("file reads mutex poisoned"))
    }

    fn stream_known(&self, file: &ScannedFile) -> Option<Playability> {
        self.with_known(|known| known.track(&file.version())?.stream.clone())
    }

    fn tags_known(&self, version: &FileVersion) -> bool {
        self.with_known(|known| {
            known
                .track(version)
                .is_some_and(|track| track.tags.is_some())
        })
    }

    /// What FFmpeg makes of the track `file`: what is known, or a probe of it
    /// now — with a read of its tags, when it plays and they are not known.
    pub(super) fn stream(&self, file: &ScannedFile) -> Result<Playability, FolderScanError> {
        if let Some(stream) = self.stream_known(file) {
            return Ok(stream);
        }
        self.probe(file)
    }

    /// The track `file`'s measurements, probed now when all that is known is
    /// that it plays; `None` when it plays nothing.
    pub(super) fn measured(
        &self,
        file: &ScannedFile,
    ) -> Result<Option<ScannedAudio>, FolderScanError> {
        let stream = match self.stream(file)? {
            Playability::Playable => {
                let stream = self.probe(file)?;
                if stream == Playability::Unplayable {
                    tracing::warn!(
                        "{} was stored as playable, and FFmpeg finds no stream bae plays in it",
                        file.path.display()
                    );
                }
                stream
            }
            stream => stream,
        };
        Ok(match stream {
            Playability::Measured(audio) => Some(audio),
            Playability::Unplayable | Playability::Playable => None,
        })
    }

    /// Probe the track `file` now, reading its tags beside it when it plays
    /// and they are not known, and keep both.
    fn probe(&self, file: &ScannedFile) -> Result<Playability, FolderScanError> {
        let version = file.version();
        let reader = (!self.tags_known(&version)).then_some(self.tags.as_ref());
        let (stream, read) = probe(file, reader)?;
        self.with_known(|known| {
            known.track_mut(&version).stream = Some(stream.clone());
            if let Some(read) = read {
                known.remember_tags(&version, read);
            }
        });
        Ok(stream)
    }

    /// What the track `file`'s tags state — what is known, or a read of them
    /// now — or why they could not be read.
    pub(super) fn tags(&self, file: &ScannedFile) -> Result<Result<FileTags, String>, FolderScanError> {
        let version = file.version();
        if !self.tags_known(&version) {
            unchanged(file, "changed before its tags could be read")?;
            let read = self.tags.read(&file.path);
            unchanged(file, "changed while its tags were being read")?;
            self.with_known(|known| known.remember_tags(&version, read));
        }
        Ok(self.with_known(|known| {
            match &known.track(&version).expect("its tags were just read").tags {
                Some(TagRead::Read { tags, .. }) => Ok(tags.clone()),
                Some(TagRead::Unreadable(error)) => Err(error.clone()),
                None => unreachable!("its tags were just read"),
            }
        }))
    }

    /// The disc and track numbers the track `file`'s tags state. Playable
    /// audio whose tags cannot be read is still the folder's audio; its tags
    /// just place it nowhere, and its name decides.
    pub(super) fn numbers(&self, file: &ScannedFile) -> Result<TrackNumbers, FolderScanError> {
        Ok(match self.tags(file)? {
            Ok(tags) => tags.numbers,
            Err(error) => {
                tracing::warn!(
                    "{}: its tags could not be read, so its name places it: {error}",
                    file.relative_path
                );
                TrackNumbers::unstated()
            }
        })
    }

    /// Settle which held picture the release whose tracks are `audio`, in
    /// play order, takes: the first its tracks embed, which is kept, while
    /// every other one no reading has kept is let go.
    pub(super) fn keep_first_cover(&self, audio: &[&ScannedFile]) {
        self.with_known(|known| {
            let mut found = false;
            let mut others = Vec::new();
            for file in audio {
                let version = file.version();
                let cover = match known.track_mut_if_known(&version) {
                    Some(TrackRead {
                        tags: Some(TagRead::Read { cover, .. }),
                        ..
                    }) => cover,
                    // Until the first picture is found, a track whose picture
                    // is not known could be the one, so none after it can be
                    // told to be no release's.
                    _ if !found => return,
                    _ => continue,
                };
                match cover {
                    EmbeddedCover::Unknown if !found => return,
                    EmbeddedCover::Unknown | EmbeddedCover::Absent => {}
                    EmbeddedCover::Released => found = true,
                    EmbeddedCover::Held { kept, .. } if !found => {
                        *kept = true;
                        found = true;
                    }
                    EmbeddedCover::Held { kept: true, .. } => {}
                    EmbeddedCover::Held { .. } => others.push(version),
                }
            }
            for version in &others {
                known.release(version);
            }
        });
    }

    /// The reading of `audio_files`' tags a release's draft is seeded from,
    /// in their order, with each file's tags and the release's picture taken
    /// from what is known rather than read again: what this pass read, and
    /// `stored` — a reading stored for the release, taken from files under
    /// the same folder — for each file it was read from at the version it has
    /// now. A file nothing knows enough of is read now.
    pub(crate) fn tag_snapshot(
        &self,
        audio_files: &[ScannedFile],
        scan_generation: u64,
        file_edit_revision: u64,
        stored: Option<&FileTagSnapshot>,
    ) -> Result<FileTagSnapshot, ImportError> {
        let mut files = Vec::with_capacity(audio_files.len());
        let mut embedded_cover = None;
        for file in audio_files {
            let observation = FileObservation {
                relative_path: file.relative_path.clone(),
                size: file.size,
                modified_at_ns: file.modified_at_ns,
            };
            let (mut tags, mut cover) = self.with_known(|known| {
                match known.track(&file.version()).and_then(|track| track.tags.as_ref()) {
                    Some(TagRead::Read { tags, cover }) => Ok((
                        Some(tags.clone()),
                        match cover {
                            EmbeddedCover::Absent => CoverRead::Absent,
                            EmbeddedCover::Held {
                                data, content_type, ..
                            } => CoverRead::Picture(data.clone(), content_type.clone()),
                            EmbeddedCover::Unknown | EmbeddedCover::Released => {
                                CoverRead::Unknown
                            }
                        },
                    )),
                    Some(TagRead::Unreadable(error)) => Err(ImportError::FileTags {
                        detail: error.clone(),
                    }),
                    None => Ok((None, CoverRead::Unknown)),
                }
            })?;
            if let Some(stored) = stored {
                if let Some(index) = stored
                    .files
                    .iter()
                    .position(|fact| fact.observation == observation)
                {
                    tags.get_or_insert_with(|| stored.files[index].tags.clone());
                    if cover == CoverRead::Unknown {
                        cover = stored_cover(stored, index)?;
                    }
                }
            }
            let wants_cover = embedded_cover.is_none();
            let (observation, tags, cover) = match tags {
                Some(tags) if !(wants_cover && cover == CoverRead::Unknown) => {
                    (observation, tags, cover)
                }
                _ => {
                    let (observation, read) =
                        crate::import::file_tag_snapshot::read_observed(file, self.tags.as_ref())?;
                    let version = FileVersion {
                        path: file.path.clone(),
                        size: observation.size,
                        modified_at_ns: observation.modified_at_ns,
                    };
                    let cover = match &read.embedded_cover {
                        Some((data, content_type)) => {
                            CoverRead::Picture(data.clone(), content_type.clone())
                        }
                        None => CoverRead::Absent,
                    };
                    let tags = read.tags.clone();
                    self.with_known(|known| known.remember_tags(&version, Ok(read)));
                    (observation, tags, cover)
                }
            };
            if wants_cover {
                if let CoverRead::Picture(data, content_type) = cover {
                    embedded_cover = Some(EmbeddedCoverFact {
                        source_relative_path: file.relative_path.clone(),
                        content_type,
                        data,
                    });
                }
            }
            files.push(FileTagFact { observation, tags });
        }
        Ok(FileTagSnapshot {
            scan_generation,
            file_edit_revision,
            files,
            embedded_cover,
        })
    }

    /// Whether the image at `version` decodes as the picture its name says:
    /// what is known, or a check of its header now. `Err` is a real I/O
    /// fault, never a damaged image.
    pub(super) fn image_decodes(&self, version: FileVersion) -> Result<bool, FolderScanError> {
        if let Some(decodes) = self.with_known(|known| match known.files.get(&version) {
            Some(FileRead::Image { decodes }) => Some(*decodes),
            _ => None,
        }) {
            return Ok(decodes);
        }
        let decodes = file_validation::is_valid_image(&version.path).map_err(|error| {
            FolderScanError::Other(format!(
                "Failed to validate image file {:?}: {error}",
                version.path
            ))
        })?;
        self.with_known(|known| known.files.insert(version, FileRead::Image { decodes }));
        Ok(decodes)
    }

    /// The sheet the CUE file `file` parses to — what is known, or a parse of
    /// it now — or `None` when it does not parse.
    pub(super) fn sheet(&self, file: &ScannedFile) -> Option<CueSheet> {
        if let Some(sheet) = self.with_known(|known| match known.files.get(&file.version()) {
            Some(FileRead::Sheet(sheet)) => Some(sheet.clone()),
            _ => None,
        }) {
            return sheet;
        }
        let sheet = match parse_cue_sheet(&file.path) {
            Ok(sheet) => Some(sheet),
            Err(error) => {
                info!("CUE {:?} does not parse: {error}", file.path);
                None
            }
        };
        self.with_known(|known| {
            known
                .files
                .insert(file.version(), FileRead::Sheet(sheet.clone()))
        });
        sheet
    }

    /// What the tags of a folder that can't be imported call the release,
    /// for one whose playable tracks are `playable`, in name order: what is
    /// known for exactly those files, or `read` of them now.
    pub(super) fn skipped_tags(
        &self,
        playable: &[&ScannedFile],
        read: impl FnOnce() -> Result<Option<SkippedTags>, FolderScanError>,
    ) -> Result<Option<SkippedTags>, FolderScanError> {
        let key: Vec<FileVersion> = playable.iter().map(|file| file.version()).collect();
        if let Some(tags) = self.with_known(|known| known.skipped_tags.get(&key).cloned()) {
            return Ok(tags);
        }
        let tags = read()?;
        self.with_known(|known| known.skipped_tags.insert(key, tags.clone()));
        Ok(tags)
    }
}

/// What a draft's reading knows of the picture one track's tags embed.
#[derive(Debug, Clone, PartialEq)]
enum CoverRead {
    Unknown,
    Absent,
    Picture(Vec<u8>, ContentType),
}

/// What `stored` says of the picture the tags of its file at `index` embed:
/// the reading's picture is the first its files embed, so every file before
/// that one embeds none, and what the files after it embed it does not say.
fn stored_cover(stored: &FileTagSnapshot, index: usize) -> Result<CoverRead, ImportError> {
    let Some(cover) = &stored.embedded_cover else {
        return Ok(CoverRead::Absent);
    };
    let source = stored
        .files
        .iter()
        .position(|fact| fact.observation.relative_path == cover.source_relative_path)
        .ok_or_else(|| ImportError::Internal {
            detail: format!(
                "a stored tag reading's picture is {}'s, which is none of its files",
                cover.source_relative_path
            ),
        })?;
    Ok(match index.cmp(&source) {
        std::cmp::Ordering::Less => CoverRead::Absent,
        std::cmp::Ordering::Equal => {
            CoverRead::Picture(cover.data.clone(), cover.content_type.clone())
        }
        std::cmp::Ordering::Greater => CoverRead::Unknown,
    })
}

impl Known {
    fn track(&self, version: &FileVersion) -> Option<&TrackRead> {
        match self.files.get(version) {
            Some(FileRead::Track(track)) => Some(track),
            _ => None,
        }
    }

    fn track_mut_if_known(&mut self, version: &FileVersion) -> Option<&mut TrackRead> {
        match self.files.get_mut(version) {
            Some(FileRead::Track(track)) => Some(track),
            _ => None,
        }
    }

    fn track_mut(&mut self, version: &FileVersion) -> &mut TrackRead {
        let entry = self
            .files
            .entry(version.clone())
            .or_insert(FileRead::Track(TrackRead {
                stream: None,
                tags: None,
            }));
        let FileRead::Track(track) = entry else {
            unreachable!("a file named as audio is only ever read as a track");
        };
        track
    }

    /// Keep what one read of the track at `version`'s tags gave, holding the
    /// picture they embed.
    fn remember_tags(&mut self, version: &FileVersion, read: Result<FileTagRead, ImportError>) {
        let read = match read {
            Ok(FileTagRead {
                tags,
                embedded_cover,
            }) => TagRead::Read {
                tags,
                cover: match embedded_cover {
                    Some((data, content_type)) => {
                        self.held.push_back(version.clone());
                        self.held_bytes += data.len();
                        EmbeddedCover::Held {
                            data,
                            content_type,
                            kept: false,
                        }
                    }
                    None => EmbeddedCover::Absent,
                },
            },
            Err(error) => TagRead::Unreadable(error.to_string()),
        };
        self.track_mut(version).tags = Some(read);
        while self.held_bytes > HELD_COVER_BYTES {
            let Some(oldest) = self.held.pop_front() else {
                break;
            };
            self.release(&oldest);
        }
    }

    /// Let go of the picture held for the track at `version`, if one is.
    fn release(&mut self, version: &FileVersion) {
        let Some(TrackRead {
            tags: Some(TagRead::Read { cover, .. }),
            ..
        }) = self.track_mut_if_known(version)
        else {
            return;
        };
        if let EmbeddedCover::Held { data, .. } = cover {
            let released = data.len();
            *cover = EmbeddedCover::Released;
            self.held_bytes -= released;
        }
    }

    /// Take `read` for `version`, beside what is known of it.
    fn learn(&mut self, version: FileVersion, read: FileRead) -> Result<(), FolderScanError> {
        use std::collections::hash_map::Entry;
        match self.files.entry(version) {
            Entry::Vacant(entry) => {
                entry.insert(read);
            }
            Entry::Occupied(mut entry) => {
                let merged = merged(entry.get(), &read).ok_or_else(|| {
                    FolderScanError::Other(format!(
                        "the stored scan says two things about {}: {:?} and {read:?}",
                        entry.key().path.display(),
                        entry.get()
                    ))
                })?;
                entry.insert(merged);
            }
        }
        Ok(())
    }

    /// What one file of a release says its pass read.
    fn learn_release_file(&mut self, entry: &CandidateFile) -> Result<(), FolderScanError> {
        let file = &entry.file;
        let read = match &entry.role {
            FileRole::Audio => {
                let audio = file.source_audio.clone().ok_or_else(|| {
                    FolderScanError::Other(format!(
                        "{} is stored as a release's audio without its measurements",
                        file.path.display()
                    ))
                })?;
                FileRead::Track(TrackRead {
                    stream: Some(Playability::Measured(audio)),
                    tags: None,
                })
            }
            FileRole::TrackSheet { sheet, .. } => FileRead::Sheet(Some(sheet.clone())),
            // A `.cue` a release carries as a document is a sheet that did
            // not parse.
            FileRole::Document if is_cue_file(&file.path) => FileRead::Sheet(None),
            FileRole::Artwork => FileRead::Image { decodes: true },
            // A zero-byte image is damaged before its bytes are looked at.
            FileRole::DamagedImage if file.size > 0 => FileRead::Image { decodes: false },
            FileRole::DamagedImage | FileRole::Document | FileRole::Other => return Ok(()),
        };
        self.learn(file.version(), read)
    }

    /// What one file of a folder that can't be imported says its pass read.
    fn learn_skipped_file(&mut self, file: &SkippedFile) -> Result<(), FolderScanError> {
        let read = match file.role {
            ProposedRole::Audio => FileRead::Track(TrackRead {
                stream: Some(Playability::Playable),
                tags: None,
            }),
            ProposedRole::DamagedAudio {
                damage: AudioDamage::Unreadable,
            } => FileRead::Track(TrackRead {
                stream: Some(Playability::Unplayable),
                tags: None,
            }),
            ProposedRole::Image => FileRead::Image { decodes: true },
            ProposedRole::DamagedImage if file.file.size > 0 => FileRead::Image { decodes: false },
            // An empty track or image is damaged before its bytes are looked
            // at, and a folder that can't be imported parses no sheet.
            ProposedRole::DamagedAudio {
                damage: AudioDamage::Empty,
            }
            | ProposedRole::DamagedImage
            | ProposedRole::Cue
            | ProposedRole::Document
            | ProposedRole::Other => return Ok(()),
        };
        self.learn(file.file.clone(), read)
    }

    fn learn_skipped_tags(
        &mut self,
        playable: Vec<FileVersion>,
        tags: Option<SkippedTags>,
    ) -> Result<(), FolderScanError> {
        use std::collections::hash_map::Entry;
        match self.skipped_tags.entry(playable) {
            Entry::Vacant(entry) => {
                entry.insert(tags);
            }
            Entry::Occupied(entry) if *entry.get() == tags => {}
            Entry::Occupied(entry) => {
                return Err(FolderScanError::Other(format!(
                    "the stored scan says two things about the tags of {:?}: {:?} and {tags:?}",
                    entry.key().iter().map(|file| &file.path).collect::<Vec<_>>(),
                    entry.get()
                )))
            }
        }
        Ok(())
    }
}

/// One reading of a file beside another of the same version, or `None` when
/// they disagree.
fn merged(known: &FileRead, other: &FileRead) -> Option<FileRead> {
    match (known, other) {
        (FileRead::Track(known), FileRead::Track(other)) => Some(FileRead::Track(TrackRead {
            stream: match (&known.stream, &other.stream) {
                (None, stream) | (stream, None) => stream.clone(),
                (Some(Playability::Measured(audio)), Some(Playability::Playable))
                | (Some(Playability::Playable), Some(Playability::Measured(audio))) => {
                    Some(Playability::Measured(audio.clone()))
                }
                (Some(known), Some(other)) if known == other => Some(known.clone()),
                (Some(_), Some(_)) => return None,
            },
            tags: match (&known.tags, &other.tags) {
                (None, tags) | (tags, None) => tags.clone(),
                (Some(known), Some(other)) if known == other => Some(known.clone()),
                (Some(_), Some(_)) => return None,
            },
        })),
        (known, other) if known == other => Some(known.clone()),
        _ => None,
    }
}

/// Fail unless `file` is still the version the listing found.
fn unchanged(file: &ScannedFile, what: &str) -> Result<(), FolderScanError> {
    let metadata =
        std::fs::metadata(&file.path).map_err(|source| FolderScanError::io(&file.path, source))?;
    let modified_at_ns = super::scan::file_modified_at_ns(&file.path, &metadata)?;
    if metadata.len() != file.size || modified_at_ns != file.modified_at_ns {
        return Err(FolderScanError::Other(format!(
            "{} {what}",
            file.path.display()
        )));
    }
    Ok(())
}

/// The track `file`'s stream as FFmpeg probes it now, and — when it plays and
/// a `reader` is given — its tags as `reader` reads them, both from the
/// version the listing found.
fn probe(
    file: &ScannedFile,
    reader: Option<&dyn FileTagReader>,
) -> Result<(Playability, Option<Result<FileTagRead, ImportError>>), FolderScanError> {
    let path = file.path.to_str().ok_or_else(|| {
        FolderScanError::Other(format!("audio path is not UTF-8: {}", file.path.display()))
    })?;
    unchanged(file, "changed before its audio facts could be read")?;
    let stream = match crate::audio_codec::probe_audio_from_path(path) {
        None => Playability::Unplayable,
        Some(probe) => match scanned_audio(&probe) {
            Some(audio) => Playability::Measured(audio),
            None => {
                info!(
                    "Invalid candidate: {} is no audio stream bae plays",
                    file.relative_path
                );
                Playability::Unplayable
            }
        },
    };
    let read = reader
        .filter(|_| matches!(stream, Playability::Measured(_)))
        .map(|reader| reader.read(&file.path));
    unchanged(file, "changed while its audio facts were being read")?;
    Ok((stream, read))
}
