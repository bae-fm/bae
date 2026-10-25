//! What a pass knows it has read of each file.
//!
//! A file is the same file while its absolute path, size and modification
//! time are the ones a read was taken under ([`FileVersion`]): the identity
//! the folder hash fingerprints. What a read found about it then still holds —
//! the stream FFmpeg found in a track and the numbers its tags state, whether
//! an image decodes, what a track sheet parses to. A pass starts from what the
//! root's stored rows say the last pass read, and adds what it reads itself, so
//! it opens no file whose reading it holds: neither one the last pass read nor
//! one this pass read for another reading of its folders.

use super::*;
use crate::import::file_tag_snapshot::FileTags;
use crate::import::track_numbers::TrackNumbers;
use std::sync::Mutex;

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

/// What is known of one track. Its stream and its numbers are read apart: a
/// folder that can't be imported is judged by its streams alone, and a stored
/// reading of a track's tags may outlive the row that measured it.
#[derive(Debug, Clone, PartialEq)]
struct TrackRead {
    stream: Option<Playability>,
    /// The disc and track numbers its tags state.
    numbers: Option<TrackNumbers>,
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
}

/// Every file a pass knows it has read, by its [`FileVersion`].
///
/// Shared by everything one pass reads its folders with, and only by that: a
/// pass over another root, or a later one, starts from the store again.
pub(crate) struct FileReads {
    known: Mutex<Known>,
}

impl FileReads {
    /// Nothing read yet.
    pub(crate) fn new() -> Self {
        Self {
            known: Mutex::new(Known::default()),
        }
    }

    /// What the scan entries stored for one root say their pass read, and
    /// the tags read from their releases' tracks, each by the version of the
    /// file it was read from.
    ///
    /// Two rows that say different things about one version of a file are a
    /// store no scan wrote, so the pass fails rather than take either.
    pub(crate) fn stored(
        entries: &[ScanItem],
        tags: &[(FileVersion, FileTags)],
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
        for (version, tags) in tags {
            known.learn(
                version.clone(),
                FileRead::Track(TrackRead {
                    stream: None,
                    numbers: Some(tags.numbers),
                }),
            )?;
        }
        Ok(Self {
            known: Mutex::new(known),
        })
    }

    fn known(&self) -> std::sync::MutexGuard<'_, Known> {
        self.known.lock().expect("file reads mutex poisoned")
    }

    fn track(&self, file: &ScannedFile) -> Option<TrackRead> {
        match self.known().files.get(&file.version()) {
            Some(FileRead::Track(track)) => Some(track.clone()),
            _ => None,
        }
    }

    /// Keep what was just read of the track `file`, beside what was known.
    fn remember_track(&self, file: &ScannedFile, update: impl FnOnce(&mut TrackRead)) {
        let mut known = self.known();
        let entry = known
            .files
            .entry(file.version())
            .or_insert(FileRead::Track(TrackRead {
                stream: None,
                numbers: None,
            }));
        let FileRead::Track(track) = entry else {
            unreachable!("a file named as audio is only ever read as a track");
        };
        update(track);
    }

    /// What FFmpeg makes of the track `file`: what is known, or a probe of it
    /// now — with its tags' numbers, when it plays and they are not known.
    pub(super) fn stream(&self, file: &ScannedFile) -> Result<Playability, FolderScanError> {
        let known = self.track(file);
        if let Some(stream) = known.as_ref().and_then(|track| track.stream.clone()) {
            return Ok(stream);
        }
        let wants_numbers = known.is_none_or(|track| track.numbers.is_none());
        let (stream, numbers) = probe(file, wants_numbers)?;
        self.remember_track(file, |track| {
            track.stream = Some(stream.clone());
            if numbers.is_some() {
                track.numbers = numbers;
            }
        });
        Ok(stream)
    }

    /// The track `file`'s measurements, probed now when all that is known is
    /// that it plays; `None` when it plays nothing.
    pub(super) fn measured(&self, file: &ScannedFile) -> Result<Option<ScannedAudio>, FolderScanError> {
        match self.stream(file)? {
            Playability::Measured(audio) => Ok(Some(audio)),
            Playability::Unplayable => Ok(None),
            Playability::Playable => {
                let wants_numbers = self.track(file).is_none_or(|track| track.numbers.is_none());
                let (stream, numbers) = probe(file, wants_numbers)?;
                if stream == Playability::Unplayable {
                    tracing::warn!(
                        "{} was stored as playable, and FFmpeg finds no stream bae plays in it",
                        file.path.display()
                    );
                }
                self.remember_track(file, |track| {
                    track.stream = Some(stream.clone());
                    if numbers.is_some() {
                        track.numbers = numbers;
                    }
                });
                Ok(match stream {
                    Playability::Measured(audio) => Some(audio),
                    Playability::Unplayable | Playability::Playable => None,
                })
            }
        }
    }

    /// The disc and track numbers the track `file`'s tags state: what is
    /// known, or a read of its tags now.
    pub(super) fn numbers(&self, file: &ScannedFile) -> Result<TrackNumbers, FolderScanError> {
        if let Some(numbers) = self.track(file).and_then(|track| track.numbers) {
            return Ok(numbers);
        }
        unchanged(file, "changed before its tags could be read")?;
        let numbers = tag_numbers(file);
        unchanged(file, "changed while its tags were being read")?;
        self.remember_track(file, |track| track.numbers = Some(numbers));
        Ok(numbers)
    }

    /// Whether the image at `version` decodes as the picture its name says:
    /// what is known, or a check of its header now. `Err` is a real I/O
    /// fault, never a damaged image.
    pub(super) fn image_decodes(&self, version: FileVersion) -> Result<bool, FolderScanError> {
        if let Some(FileRead::Image { decodes }) = self.known().files.get(&version) {
            return Ok(*decodes);
        }
        let decodes = file_validation::is_valid_image(&version.path).map_err(|error| {
            FolderScanError::Other(format!(
                "Failed to validate image file {:?}: {error}",
                version.path
            ))
        })?;
        self.known()
            .files
            .insert(version, FileRead::Image { decodes });
        Ok(decodes)
    }

    /// The sheet the CUE file `file` parses to — what is known, or a parse of
    /// it now — or `None` when it does not parse.
    pub(super) fn sheet(&self, file: &ScannedFile) -> Option<CueSheet> {
        if let Some(FileRead::Sheet(sheet)) = self.known().files.get(&file.version()) {
            return sheet.clone();
        }
        let sheet = match parse_cue_sheet(&file.path) {
            Ok(sheet) => Some(sheet),
            Err(error) => {
                info!("CUE {:?} does not parse: {error}", file.path);
                None
            }
        };
        self.known()
            .files
            .insert(file.version(), FileRead::Sheet(sheet.clone()));
        sheet
    }

    /// What the tags of a folder that can't be imported call the release,
    /// for one whose playable tracks are `playable`, in name order: what is
    /// known for exactly those files, or `read` of them now.
    pub(super) fn skipped_tags(
        &self,
        playable: &[&ScannedFile],
        read: impl FnOnce() -> Option<SkippedTags>,
    ) -> Option<SkippedTags> {
        let key: Vec<FileVersion> = playable.iter().map(|file| file.version()).collect();
        if let Some(tags) = self.known().skipped_tags.get(&key) {
            return tags.clone();
        }
        let tags = read();
        self.known().skipped_tags.insert(key, tags.clone());
        tags
    }
}

impl Known {
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
                    numbers: None,
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
                numbers: None,
            }),
            ProposedRole::DamagedAudio {
                damage: AudioDamage::Unreadable,
            } => FileRead::Track(TrackRead {
                stream: Some(Playability::Unplayable),
                numbers: None,
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
            numbers: match (known.numbers, other.numbers) {
                (None, numbers) | (numbers, None) => numbers,
                (Some(known), Some(other)) if known == other => Some(known),
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
/// `wants_numbers` — the numbers its tags state, both read from the version
/// the listing found.
fn probe(
    file: &ScannedFile,
    wants_numbers: bool,
) -> Result<(Playability, Option<TrackNumbers>), FolderScanError> {
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
    let numbers = (wants_numbers && matches!(stream, Playability::Measured(_)))
        .then(|| tag_numbers(file));
    unchanged(file, "changed while its audio facts were being read")?;
    Ok((stream, numbers))
}

/// The numbers the track `file`'s tags state. Playable audio whose tags
/// cannot be read is still the folder's audio; its tags just place it
/// nowhere, and its name decides.
fn tag_numbers(file: &ScannedFile) -> TrackNumbers {
    match crate::import::file_tag_snapshot::read_tag_numbers(&file.path) {
        Ok(numbers) => numbers,
        Err(error) => {
            tracing::warn!(
                "{}: its tags could not be read, so its name places it: {error}",
                file.relative_path
            );
            TrackNumbers::unstated()
        }
    }
}
