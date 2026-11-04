use super::*;
use crate::track_numbers::TrackNumbers;

mod play_order;

use play_order::lay_audio_in_play_order;

// ── Extension-based classification (pure, no I/O) ──────────────────────────

/// The text files a folder carries beside its audio: track sheets, rip logs
/// and the AccurateRip reports rippers write beside them, notes, playlists.
const DOCUMENT_EXTENSIONS: &[&str] = &["cue", "log", "accurip", "txt", "m3u", "m3u8"];

/// Extensions used by download clients and browsers to mark an
/// in-progress download. Presence of any of these anywhere in a folder means
/// the folder is mid-download and should not surface as an import candidate.
const PARTIAL_MARKER_EXTENSIONS: &[&str] = &["part", "crdownload", "download", "aria2", "partial"];

/// Check if a file is an audio file based on extension
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ContentTypeHint::from_extension(ext).is_audio())
        .unwrap_or(false)
}

/// Check if a file is an image/artwork file
pub(super) fn is_image_file(path: &Path) -> bool {
    ContentTypeHint::path_is_raster_image(path)
}

/// Whether `path`'s extension is `expected`, in any case.
pub(crate) fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(expected))
}

/// Check if a file is a document file (see `DOCUMENT_EXTENSIONS`)
pub(super) fn is_document_file(path: &Path) -> bool {
    DOCUMENT_EXTENSIONS.iter().any(|extension| has_extension(path, extension))
}

/// Whether a file is a CUE sheet.
pub(crate) fn is_cue_file(path: &Path) -> bool {
    has_extension(path, "cue")
}

/// Whether a directory entry is hidden, which the scan never lists: its name
/// starts with a dot.
pub(crate) fn is_hidden_entry(name: &std::ffi::OsStr) -> bool {
    // The encoded bytes are documented as safe to inspect for ASCII, which a
    // leading `.` is, whatever the rest of the name holds.
    name.as_encoded_bytes().first() == Some(&b'.')
}

/// Check if a file is noise (.DS_Store, Thumbs.db, etc.)
pub(super) fn is_noise_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name == ".DS_Store" || name == "Thumbs.db" || name == "desktop.ini")
        .unwrap_or(false)
}

/// True when `path`'s extension matches a known in-progress-download marker
/// (e.g. `01.flac.part`, `02.flac.crdownload`, `03.aria2`). Match is case-insensitive.
pub(super) fn is_partial_marker_file(path: &Path) -> bool {
    PARTIAL_MARKER_EXTENSIONS.iter().any(|extension| has_extension(path, extension))
}

// ── File categorization ─────────────────────────────────────────────────────

/// The result of categorizing a leaf folder's files: a valid release, or the
/// folder read file by file when it fails validation (two files showing one
/// name, audio bae can't play, or no audio at all). `Err` is reserved for
/// genuine I/O faults, which are not the same as a failed-validation leaf.
#[derive(Debug)]
pub(super) enum CategorizeOutcome {
    Valid(CategorizedFiles),
    Invalid { files: Vec<SkippedFile>, tags: Option<SkippedTags> },
}

/// One file under `release_root` as its name proposes it — and, for an image
/// or a zero-byte track, as its bytes do. A zero-byte track is damaged audio,
/// which makes any release holding it unimportable; an image that will not
/// decode is no such defect: the release is its audio, and the image is
/// carried as a damaged one, never its artwork.
pub(super) fn propose_file(
    entry: &FileEntry,
    release_root: &Path,
    fs_root: &Path,
    reads: &FileReads,
) -> Result<(ScannedFile, ProposedRole), FolderScanError> {
    let relative_from_release = if release_root.as_os_str().is_empty() {
        entry.path.clone()
    } else {
        entry.path.strip_prefix(release_root).unwrap_or(&entry.path).to_path_buf()
    };

    // Joined from the path's components rather than displayed, so the result is
    // `/`-separated on Windows too. A displayed `Path` uses the host's
    // separator, and this string is stored on the row and joined back onto a
    // directory by every other device in the library. A name that isn't UTF-8
    // is its escaped text, so two files can show one name (see
    // `InvalidReason::SameName`).
    let relative_path = relative_path_string(&relative_from_release);

    // The absolute path is fs_root + entry.path.
    let absolute_path = fs_root.join(&entry.path);

    let role = if is_audio_file(&entry.path) {
        if entry.size == 0 {
            info!("{relative_path} is a zero-byte audio file");
            ProposedRole::DamagedAudio { damage: AudioDamage::Empty }
        } else {
            ProposedRole::Audio
        }
    } else if is_cue_file(&entry.path) {
        ProposedRole::Cue
    } else if is_image_file(&entry.path) {
        // As with audio: a damaged image is Ok, Err is a real I/O fault.
        let damaged = entry.size == 0
            || !reads.image_decodes(FileVersion {
                path: absolute_path.clone(),
                size: entry.size,
                modified_at_ns: entry.modified_at_ns,
            })?;
        if damaged {
            info!(
                "{relative_path} is a damaged image; it is carried, and never offered as artwork"
            );
            ProposedRole::DamagedImage
        } else {
            ProposedRole::Image
        }
    } else if is_document_file(&entry.path) {
        ProposedRole::Document
    } else {
        // Unrecognized, and carried anyway — the folder is the release.
        ProposedRole::Other
    };
    Ok((ScannedFile::new(absolute_path, relative_path, entry.size, entry.modified_at_ns), role))
}

/// `proposed`, which failed validation, as its skipped files in name order.
pub(super) fn skipped_files(proposed: &[(ScannedFile, ProposedRole)]) -> Vec<SkippedFile> {
    let mut files: Vec<SkippedFile> = proposed
        .iter()
        .map(|(file, role)| SkippedFile {
            relative_path: file.relative_path.clone(),
            file: file.version(),
            role: *role,
        })
        .collect();
    files.sort_by(|a, b| natord::compare_ignore_case(&a.relative_path, &b.relative_path));
    files
}

/// What the tags of `proposed`'s playable audio call the release: the album
/// title the first of them in name order to state one gives, and that file's
/// album artists' line, else its track artists'. A file whose tags can't be read is
/// passed over, said in the log.
fn skipped_tags(
    proposed: &[(ScannedFile, ProposedRole)],
    reads: &FileReads,
) -> Result<Option<SkippedTags>, FolderScanError> {
    let mut playable: Vec<&ScannedFile> = proposed
        .iter()
        .filter(|(_, role)| *role == ProposedRole::Audio)
        .map(|(file, _)| file)
        .collect();
    if playable.is_empty() {
        return Ok(None);
    }
    playable.sort_by(|a, b| natord::compare_ignore_case(&a.relative_path, &b.relative_path));
    reads.skipped_tags(&playable, || {
        for file in &playable {
            let tags = match reads.tags(file)? {
                Ok(tags) => tags,
                Err(error) => {
                    tracing::warn!(
                        "{}: its tags could not be read, so they name nothing: {error}",
                        file.relative_path
                    );
                    continue;
                }
            };
            if let Some(album_title) = tags.album_title {
                let artists = if tags.album_artists.is_empty() {
                    tags.track_artists
                } else {
                    tags.album_artists
                };
                return Ok(Some(SkippedTags {
                    album_title,
                    artist: (!artists.is_empty()).then(|| {
                        bae_types::credits::credit_line(
                            artists.iter().map(|name| (name.as_str(), None)),
                        )
                    }),
                }));
            }
        }
        Ok(None)
    })
}

/// Categorize a release root's selected files. `fs_root` is the folder
/// being imported — validation reads its actual bytes from disk.
///
/// Every file gets exactly one role, and the roles are *proposals*: a sheet
/// whose `FILE` directive names audio that is not here simply stays unbound,
/// and a `.cue` that will not parse is a document. Only a real defect —
/// audio bae cannot read, or two files showing one name — returns
/// `Invalid(reason)`; an image that will not decode is carried as a damaged
/// image.
///
/// A sheet's binding and disc are what the user also writes, so `stored` is
/// applied over the proposals before anything downstream reads them: the
/// candidate this returns is the folder as the *user* has settled it, not only
/// as its filenames read.
pub(super) fn categorize_files_from_tree(
    tree: &CandidateFileIndex,
    release_root: &Path,
    fs_root: &Path,
    stored: &StoredCandidateEdits,
    parts: &[ReleasePart],
    cancellation: &ScanCancellation,
    reads: &FileReads,
) -> Result<CategorizeOutcome, FolderScanError> {
    let unreadable = |file: &ScannedFile| {
        info!("{} holds no audio bae can play", file.relative_path);
        ProposedRole::DamagedAudio { damage: AudioDamage::Unreadable }
    };
    let mut proposed: Vec<(ScannedFile, ProposedRole)> = Vec::new();
    for entry in tree.all_files_under(release_root) {
        cancellation.check()?;
        let (file, mut role) = propose_file(entry, release_root, fs_root, reads)?;
        if role == ProposedRole::Audio && reads.stream(&file)? == Playability::Unplayable {
            role = unreadable(&file);
        }
        proposed.push((file, role));
    }
    let releasable = |proposed: &[(ScannedFile, ProposedRole)]| {
        shared_name(proposed.iter().map(|(file, _)| file.relative_path.as_str())).is_none()
            && !proposed.iter().any(|(_, role)| matches!(role, ProposedRole::DamagedAudio { .. }))
            && proposed.iter().any(|(_, role)| *role == ProposedRole::Audio)
    };
    // A folder that would be a release needs each track's measurements. A
    // track known only to play, from a skipped folder's row, is measured now.
    if releasable(&proposed) {
        for (file, role) in &mut proposed {
            if *role != ProposedRole::Audio {
                continue;
            }
            match reads.measured(file)? {
                Some(audio) => file.source_audio = Some(audio),
                None => *role = unreadable(file),
            }
        }
    }

    // Every file is read before the folder is judged, so a folder that fails
    // is listed with each of its damaged files, not only the first.
    if !releasable(&proposed) {
        info!("Invalid candidate: two of its files show one name, its audio is damaged, or it has none");
        return Ok(CategorizeOutcome::Invalid {
            tags: skipped_tags(&proposed, reads)?,
            files: skipped_files(&proposed),
        });
    }
    let mut tagged: HashMap<String, TrackNumbers> = HashMap::new();
    for (file, role) in &proposed {
        if *role == ProposedRole::Audio {
            tagged.insert(file.relative_path.clone(), reads.numbers(file)?);
        }
    }

    // One order for everything downstream: the release's own file order.
    //
    // Files sit in name order — natural and case-insensitive, not byte-wise, so
    // `CD10` follows `CD9`, `10.flac` follows `9.flac`, and `cover.jpg` sits
    // with `Cover.jpg` rather than after every capitalized name — and the audio
    // among them in the order it plays (`lay_audio_in_play_order`). It has to
    // be one order because separate consumers zip against each other: the
    // track slots lay the audio down in this order, and the file-metadata
    // import reads embedded tags in it, so a second ordering rule anywhere
    // would pair a file's tags with a different file's samples.
    proposed.sort_by(|a, b| natord::compare_ignore_case(&a.0.relative_path, &b.0.relative_path));
    lay_audio_in_play_order(&mut proposed, &tagged, parts);
    // Of the pictures the tracks' tags embed, the release's draft takes the
    // first in this order.
    reads.keep_first_cover(
        &proposed
            .iter()
            .filter(|(_, role)| *role == ProposedRole::Audio)
            .map(|(file, _)| file)
            .collect::<Vec<_>>(),
    );

    // Parse every CUE exactly once. A sheet that will not parse is not a sheet;
    // it stays a document, and the folder imports without it.
    let mut sheets: HashMap<usize, crate::cue_sheet::CueSheet> = HashMap::new();
    for (index, (file, role)) in proposed.iter_mut().enumerate() {
        if *role != ProposedRole::Cue {
            continue;
        }
        match reads.sheet(file) {
            Some(sheet) => {
                sheets.insert(index, sheet);
            }
            None => {
                info!("CUE {:?} stays a document", file.path);
                *role = ProposedRole::Document;
            }
        }
    }

    let mut files: Vec<CandidateFile> = Vec::with_capacity(proposed.len());
    for (index, (file, proposed_role)) in proposed.into_iter().enumerate() {
        let role = match proposed_role {
            ProposedRole::Audio => FileRole::Audio,
            ProposedRole::Cue => unsettled_sheet(
                sheets
                    .remove(&index)
                    .expect("a file keeps the CUE role only when its sheet parsed"),
            ),
            ProposedRole::Image => FileRole::Artwork,
            ProposedRole::DamagedImage => FileRole::DamagedImage,
            ProposedRole::Document => FileRole::Document,
            ProposedRole::Other => FileRole::Other,
            ProposedRole::DamagedAudio { .. } => {
                unreachable!("a folder holding damaged audio was judged invalid above")
            }
        };
        files.push(CandidateFile { file, role });
    }

    // The user's decisions land over the proposals, and the codec probed for the
    // audio each sheet ends up naming is checked. The hash is what those decisions
    // are stored under, and it covers files only — so computing it here, before
    // any of them is applied, is not an ordering trick: applying one cannot change
    // it.
    let stored = stored
        .for_hash(&content_hash_of(files.iter().map(|entry| &entry.file)))
        .cloned()
        .unwrap_or_default();
    match settle_sheet_bindings(&mut files, &stored.sheet_bindings, cancellation)? {
        SettledBindings::Settled => {}
        // Every audio file here was probed above, and one that could not be
        // was judged before the sheets were read.
        SettledBindings::CorruptAudio { path } => {
            return Err(FolderScanError::Other(format!(
                "a sheet binds {path}, which was probed as playable and holds no audio facts"
            )))
        }
    }
    settle_sheet_discs(&mut files, parts, &stored.sheet_discs);

    Ok(CategorizeOutcome::Valid(CategorizedFiles { files, parts: parts.to_vec() }))
}

/// A parsed sheet before its binding and disc settle. Neither is proposed
/// here: a person's decisions come first, so `settle_sheet_bindings` binds
/// every parsed sheet once those have landed; and a cue filename says nothing
/// about which disc it holds, so `settle_sheet_discs` assigns every parsed
/// sheet against the bindings that end up in force.
fn unsettled_sheet(sheet: crate::cue_sheet::CueSheet) -> FileRole {
    FileRole::TrackSheet {
        sheet,
        binding: SheetBinding::Unresolved { files: Vec::new() },
        disc: SheetDisc::Disc { number: 1 },
    }
}

/// A library release's files on this device, read the way a folder's scan
/// reads its rip files: audio by the facts its import stored, every CUE that
/// parses a track sheet bound to that audio and given its disc by the scan's
/// own rules, and the documents beside them, in name order. A person's
/// decisions about a folder's sheets are kept with the candidate, not the
/// release, so the sheets settle as the scan proposes them. Holds only what a
/// rip reading reads: artwork and other files are left out.
pub(crate) fn categorize_library_files(
    files: impl IntoIterator<Item = ScannedFile>,
) -> CategorizedFiles {
    let mut files: Vec<CandidateFile> = files
        .into_iter()
        .filter_map(|file| {
            let role = if file.source_audio.is_some() {
                FileRole::Audio
            } else if is_cue_file(&file.path) {
                match parse_cue_sheet(&file.path) {
                    Ok(sheet) => unsettled_sheet(sheet),
                    Err(error) => {
                        info!(
                            "CUE {:?} does not parse, so it stays a document: {error}",
                            file.path
                        );
                        FileRole::Document
                    }
                }
            } else if is_document_file(&file.path) {
                FileRole::Document
            } else {
                return None;
            };
            Some(CandidateFile { file, role })
        })
        .collect();
    files.sort_by(|a, b| natord::compare_ignore_case(&a.file.relative_path, &b.file.relative_path));
    let mut categorized = CategorizedFiles { files, parts: Vec::new() };
    categorized
        .apply_candidate_file_edits(&CandidateFileEdits::default())
        .expect("every audio file holds the facts its import stored");
    categorized
}

/// What a file's audio is, as `probe` read it: `None` for audio bae does
/// not play — no sample rate or channels, a codec bae does not decode, or a
/// lossy stream whose bitrate could not be read.
pub fn scanned_audio(probe: &bae_audio::ProbeResult) -> Option<ScannedAudio> {
    if probe.sample_rate == 0 || probe.channels == 0 {
        return None;
    }
    if !probe.content_type.is_supported_audio() {
        return None;
    }
    let bits_per_sample = probe.bits_per_sample.map(i64::from);
    let bitrate_kbps = match bits_per_sample {
        Some(_) => None,
        None => Some(probe.bitrate_kbps?),
    };
    Some(ScannedAudio {
        content_type: probe.content_type.clone(),
        duration_ms: probe.duration.as_millis() as u64,
        format: bae_types::audio_format::AudioFormat {
            codec: probe.content_type.display_name().to_string(),
            sample_rate_hz: i64::from(probe.sample_rate),
            bits_per_sample,
            bitrate_kbps,
            channels: i64::from(probe.channels),
        },
    })
}

#[cfg(test)]
#[path = "categorize_tests.rs"]
mod tests;
