use super::*;

// ── Extension-based classification (pure, no I/O) ──────────────────────────

/// Check if a file is an audio file based on extension
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ContentTypeHint::from_extension(ext).is_audio())
        .unwrap_or(false)
}

/// A track sheet's audio file, as FFmpeg probes it.
pub(super) enum CueCodecLabel {
    /// A codec bae can play back from a single-file CUE. Carries the
    /// `CUE+<codec>` label component (e.g. "FLAC", "APE").
    Supported(String),
    /// A readable codec that can't back single-file CUE playback (e.g. MP3,
    /// Vorbis). Carries the codec's display name for the log line: the binding
    /// is refused with the codec named, and the audio imports as one track.
    Unsupported(String),
    /// The file cleared the header-only magic check but FFmpeg can't identify a
    /// playable stream in it — a download truncated after the header, or
    /// otherwise corrupt audio. Surfaces the folder as a corrupt-audio invalid
    /// candidate instead of aborting the scan.
    Unprobeable,
}

/// Codec identity for a track sheet's audio file, for the `CUE+<codec>` format
/// label. The label comes from FFmpeg's probe, never from the extension, because
/// containers such as MP4, Ogg, WAV, and AIFF don't prove the codec by filename.
///
/// `Err` is reserved for a non-UTF-8 path (which FFmpeg can't open at all). A
/// readable file whose codec bae can't play (`Ok(Unsupported)`) costs the sheet
/// its binding; one FFmpeg can't probe (`Ok(Unprobeable)`) is corrupt audio and
/// surfaces its folder as invalid, without aborting the watched-root walk.
pub(super) fn cue_pair_codec_label(path: &Path) -> Result<CueCodecLabel, FolderScanError> {
    let path_str = path.to_str().ok_or_else(|| {
        FolderScanError::Other(format!("CUE audio path is not UTF-8: {}", path.display()))
    })?;
    let Some(probe) = crate::audio_codec::probe_audio_from_path(path_str) else {
        return Ok(CueCodecLabel::Unprobeable);
    };
    match probe.content_type {
        crate::util::content_type::ContentType::Flac
        | crate::util::content_type::ContentType::Ape
        | crate::util::content_type::ContentType::Alac
        | crate::util::content_type::ContentType::Pcm
        | crate::util::content_type::ContentType::WavPack
        | crate::util::content_type::ContentType::Dsd => Ok(CueCodecLabel::Supported(
            probe.content_type.display_name().to_string(),
        )),
        other => Ok(CueCodecLabel::Unsupported(other.display_name().to_string())),
    }
}

/// Check if a file is an image/artwork file
pub(super) fn is_image_file(path: &Path) -> bool {
    ContentTypeHint::path_is_raster_image(path)
}

/// Check if a file is a document file (.cue, .log, .txt, .m3u)
pub(super) fn is_document_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| DOCUMENT_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Check if a file is a CUE file
pub(super) fn is_cue_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase() == "cue")
        .unwrap_or(false)
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
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| PARTIAL_MARKER_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

// ── File categorization ─────────────────────────────────────────────────────

/// The result of categorizing a leaf folder's files: a valid release, or an
/// invalid one carrying the reason it failed validation (corrupt/zero-byte
/// audio, corrupt image, no audio at all). `Err` is reserved for genuine I/O
/// faults, which are not the same as a failed-validation leaf.
#[derive(Debug)]
pub(super) enum CategorizeOutcome {
    Valid(CategorizedFiles),
    Invalid(InvalidReason),
}

/// What the extension says a file is, before the CUE parse and the
/// `FILE`-directive resolution settle the roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProposedRole {
    Audio,
    Cue,
    Image,
    Document,
    Other,
}

/// Filename stems that conventionally name a release's front cover. The scan
/// proposes the first image matching one of these as the cover; every other
/// image is artwork.
pub(super) const COVER_STEMS: &[&str] = &[
    "cover",
    "front",
    "folder",
    "frontcover",
    "front cover",
    "albumart",
    "album art",
];

/// Whether an image's filename stem conventionally names a front cover.
pub(super) fn is_cover_name(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let stem = stem.trim().to_lowercase();
    COVER_STEMS.iter().any(|candidate| *candidate == stem)
}

/// Shorthand for a failed-validation leaf carrying `reason`.
pub(super) fn invalid(reason: InvalidReason) -> Result<CategorizeOutcome, FolderScanError> {
    Ok(CategorizeOutcome::Invalid(reason))
}

/// What settling a folder's sheet bindings produced.
pub(super) enum SettledBindings {
    /// Every sheet settled. `cue_codec` is the probed codec of the first sheet
    /// that stayed bound — the `CUE+<codec>` label's source, and `None` when no
    /// sheet is bound.
    Settled { cue_codec: Option<String> },
    /// A bound sheet names audio FFmpeg cannot read at all. That is a real
    /// defect, not a disagreement about which file the sheet meant, so the
    /// folder is an invalid candidate rather than one with an unbound sheet.
    CorruptAudio { path: String },
}

/// Settle every file's role: the user's decision where they made one, the
/// scan's proposal where they did not.
///
/// Only a file the scan read as audio can move, and only between being one of
/// the release's tracks and not being one. Nothing else is a decision anyone
/// makes here, and a decision about a file that has since stopped being audio —
/// a stored row this build can no longer place — is ignored rather than
/// applied to whatever now sits at that path.
pub(super) fn settle_file_roles(files: &mut [CandidateFile], edits: &FileRoleEdits) {
    for entry in files.iter_mut() {
        if !entry.proposed_audio {
            continue;
        }
        entry.role = match edits.get(&entry.file.relative_path) {
            Some(FileRoleChoice::NotATrack) => FileRole::Other,
            Some(FileRoleChoice::Audio) | None => FileRole::Audio,
        };
    }
}

/// Settle every parsed sheet's binding, and report what the format label needs.
///
/// The user's decision wins where they made one; whatever the sheet already
/// carries — the `FILE` directive's resolution on a fresh scan — stands where
/// they did not. Either way the audio a sheet ends up naming is probed, because
/// bae can only carve tracks out of some containers, and a refusal keeps the
/// codec so both the pane and the picker can say why.
pub(super) fn settle_sheet_bindings(
    files: &mut [CandidateFile],
    edits: &SheetBindingEdits,
    cancellation: &ScanCancellation,
) -> Result<SettledBindings, FolderScanError> {
    // Which relative paths are this folder's audio. A binding naming anything
    // else describes nothing, whether it came from a directive or from a stored
    // decision this build can no longer place.
    let audio: HashMap<&str, PathBuf> = files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Audio))
        .map(|entry| (entry.file.relative_path.as_str(), entry.file.path.clone()))
        .collect();

    let mut settled: Vec<(usize, SheetBinding)> = Vec::new();
    let mut cue_codec: Option<String> = None;
    for (index, entry) in files.iter().enumerate() {
        cancellation.check()?;
        let FileRole::TrackSheet { binding, .. } = &entry.role else {
            continue;
        };
        let binding = match edits.get(&entry.file.relative_path) {
            Some(UserSheetBinding::Describes { file_id }) => SheetBinding::Describes {
                file_id: file_id.clone(),
            },
            Some(UserSheetBinding::Cleared) => SheetBinding::Unresolved,
            None => binding.clone(),
        };
        let SheetBinding::Describes { file_id } = &binding else {
            settled.push((index, binding));
            continue;
        };
        let Some(audio_path) = audio.get(file_id.as_str()) else {
            info!(
                "sheet {} names {file_id}, which is not this folder's audio; it stays unbound",
                entry.file.relative_path,
            );
            settled.push((index, SheetBinding::Unresolved));
            continue;
        };
        let binding = match cue_pair_codec_label(audio_path) {
            Ok(CueCodecLabel::Supported(label)) => {
                cue_codec.get_or_insert(label);
                binding
            }
            Ok(CueCodecLabel::Unsupported(codec)) => {
                info!(
                    "sheet {} names {codec} audio, which bae can't play from a single-file CUE; \
                     the binding is refused and the audio imports as one track",
                    entry.file.relative_path,
                );
                SheetBinding::RefusedCodec {
                    file_id: file_id.clone(),
                    codec,
                }
            }
            Ok(CueCodecLabel::Unprobeable) => {
                info!("Invalid candidate: sheet audio file could not be probed: {file_id}");
                return Ok(SettledBindings::CorruptAudio {
                    path: file_id.clone(),
                });
            }
            // FFmpeg cannot open a non-UTF-8 path, so the audio is unreadable
            // by the only measure that decides a binding.
            Err(e) => {
                info!("Invalid candidate: sheet audio file could not be probed ({e})");
                return Ok(SettledBindings::CorruptAudio {
                    path: file_id.clone(),
                });
            }
        };
        settled.push((index, binding));
    }

    for (index, binding) in settled {
        let FileRole::TrackSheet { binding: slot, .. } = &mut files[index].role else {
            unreachable!("only track-sheet roles were collected above");
        };
        *slot = binding;
    }
    Ok(SettledBindings::Settled { cue_codec })
}

/// Settle every parsed sheet's disc assignment: the user's decision where they
/// made one, and the sheet's own position among the folder's bound sheets where
/// they made none.
///
/// Total over the folder's parsed sheets, and run after
/// [`settle_sheet_bindings`] at every call site, because the position it hands
/// out is a position among the sheets that are *bound*. A sheet nobody bound
/// carves nothing either way, so it takes disc one and says nothing by it.
pub(super) fn settle_sheet_discs(files: &mut [CandidateFile], edits: &SheetDiscEdits) {
    let mut bound_so_far = 0u32;
    for entry in files.iter_mut() {
        let FileRole::TrackSheet { binding, disc, .. } = &mut entry.role else {
            continue;
        };
        let position = if binding.describes().is_some() {
            bound_so_far += 1;
            bound_so_far
        } else {
            1
        };
        *disc = edits
            .get(&entry.file.relative_path)
            .unwrap_or(SheetDisc::Disc { number: position });
    }
}

/// The release's format label: `CUE+<codec>` when a sheet stayed bound — the
/// probed codec, never the extension — and otherwise the audio's own extension,
/// which is what a file-per-track release is called. `None` only when the folder
/// holds no audio at all.
pub(super) fn derive_format_label(
    files: &[CandidateFile],
    cue_codec: Option<String>,
) -> Option<String> {
    if let Some(codec) = cue_codec {
        return Some(format!("CUE+{codec}"));
    }
    let first_audio = files
        .iter()
        .find(|entry| matches!(entry.role, FileRole::Audio))?;
    Some(
        first_audio
            .file
            .path
            .extension()
            .and_then(|e| e.to_str())
            .expect("Audio file must have an extension")
            .to_uppercase(),
    )
}

/// The directory holding a CUE sheet, where its `FILE` references resolve. A
/// CUE path with no parent is a filesystem impossibility for a scanned file,
/// so it's a hard scan error, not an invalid-candidate reason.
pub(super) fn cue_parent_dir(cue_path: &Path) -> Result<&Path, FolderScanError> {
    cue_path
        .parent()
        .ok_or_else(|| FolderScanError::Other(format!("CUE file has no parent: {:?}", cue_path)))
}

/// Categorize a release root's selected files. `fs_root` is the folder
/// being imported — validation reads its actual bytes from disk.
///
/// Every file gets exactly one role, and the roles are *proposals*: a sheet
/// whose `FILE` directive names audio that is not here simply stays unbound,
/// and a `.cue` that will not parse is a document. Only a real defect —
/// unreadable audio or an unreadable image — returns `Invalid(reason)`.
///
/// A sheet's binding is the one role detail the user also writes, so `stored`
/// is applied over the proposals before anything downstream reads them: the
/// candidate this returns is the folder as the *user* has settled it, not only
/// as its filenames read.
pub(super) fn categorize_files_from_tree(
    tree: &CandidateFileIndex,
    release_root: &Path,
    fs_root: &Path,
    stored: &StoredCandidateEdits,
    cancellation: &ScanCancellation,
) -> Result<CategorizeOutcome, FolderScanError> {
    let mut proposed: Vec<(ScannedFile, ProposedRole)> = Vec::new();

    for entry in tree.all_files_under(release_root) {
        cancellation.check()?;
        let relative_from_release = if release_root.as_os_str().is_empty() {
            entry.path.clone()
        } else {
            entry
                .path
                .strip_prefix(release_root)
                .unwrap_or(&entry.path)
                .to_path_buf()
        };

        // Joined from the path's components rather than displayed, so the result is
        // `/`-separated on Windows too. A displayed `Path` uses the host's
        // separator, and this string is stored on the row and joined back onto a
        // directory by every other device in the library.
        let relative_path = relative_from_release
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        // The absolute path is fs_root + entry.path.
        let absolute_path = fs_root.join(&entry.path);

        let role = if is_audio_file(&entry.path) {
            // Ok(false) is corruption (the candidate becomes Invalid); Err is a
            // genuine I/O fault (file vanished, permissions, flaky network
            // mount) — surface it rather than mis-label a system error as
            // corruption and silently drop the whole release.
            let valid = file_validation::is_valid_audio(&absolute_path).map_err(|e| {
                FolderScanError::Other(format!(
                    "Failed to validate audio file {absolute_path:?}: {e}"
                ))
            })?;
            if entry.size == 0 || !valid {
                info!("Invalid candidate: corrupt or zero-byte audio file {relative_path}");
                return invalid(InvalidReason::CorruptAudioFile {
                    path: relative_path.to_string(),
                });
            }
            ProposedRole::Audio
        } else if is_cue_file(&entry.path) {
            ProposedRole::Cue
        } else if is_image_file(&entry.path) {
            // As with audio: Ok(false) is corruption, Err is a real I/O fault.
            let valid = file_validation::is_valid_image(&absolute_path).map_err(|e| {
                FolderScanError::Other(format!(
                    "Failed to validate image file {absolute_path:?}: {e}"
                ))
            })?;
            if entry.size == 0 || !valid {
                info!("Invalid candidate: corrupt or zero-byte image {relative_path}");
                return invalid(InvalidReason::CorruptImage {
                    path: relative_path.to_string(),
                });
            }
            ProposedRole::Image
        } else if is_document_file(&entry.path) {
            ProposedRole::Document
        } else {
            // Unrecognized, and carried anyway — the folder is the release.
            ProposedRole::Other
        };

        proposed.push((
            ScannedFile::new(absolute_path, relative_path, entry.size),
            role,
        ));
    }

    // One order for everything downstream: the release's own file order.
    //
    // Natural, not byte-wise, so `CD10` follows `CD9` and `10.flac` follows
    // `9.flac` — the order a person reading the folder expects. It has to be
    // one order because separate consumers zip against each other: the track
    // slots lay the audio down in this order, and the Unknown import reads
    // embedded tags in it, so a second ordering rule anywhere would pair a
    // file's tags with a different file's samples.
    proposed.sort_by(|a, b| natord::compare(&a.0.relative_path, &b.0.relative_path));

    // Parse every CUE exactly once. A sheet that will not parse is not a sheet;
    // it stays a document, and the folder imports without it.
    let mut sheets: HashMap<usize, crate::cue_flac::CueSheet> = HashMap::new();
    for (index, (file, role)) in proposed.iter_mut().enumerate() {
        if *role != ProposedRole::Cue {
            continue;
        }
        match parse_cue_sheet(&file.path) {
            Ok(sheet) => {
                sheets.insert(index, sheet);
            }
            Err(error) => {
                info!(
                    "CUE {:?} did not parse ({error}); it stays a document",
                    file.path
                );
                *role = ProposedRole::Document;
            }
        }
    }

    // Resolve each sheet's `FILE` directives literally inside the sheet's own
    // directory. A sheet binds only when every reference resolves — a partial
    // layout describes audio that isn't reachable, so it is no better than none
    // — and `describes` names the first reference, the audio the sheet leads
    // with. A single-FILE sheet whose literal path is absent may instead name
    // the unique same-stem audio beside it.
    let audio_paths: Vec<PathBuf> = proposed
        .iter()
        .filter(|(_, role)| *role == ProposedRole::Audio)
        .map(|(file, _)| file.path.clone())
        .collect();
    let audio_by_path: HashMap<&Path, &str> = proposed
        .iter()
        .filter(|(_, role)| *role == ProposedRole::Audio)
        .map(|(file, _)| (file.path.as_path(), file.relative_path.as_str()))
        .collect();
    let mut bindings: BTreeMap<usize, SheetBinding> = BTreeMap::new();
    for (index, sheet) in &sheets {
        let cue_file = &proposed[*index].0;
        let cue_dir = cue_parent_dir(&cue_file.path)?;
        let references = sheet.audio_file_references();
        if references.is_empty() {
            info!(
                "CUE {:?} names no audio file; it stays unbound",
                cue_file.path
            );
            bindings.insert(*index, SheetBinding::Unresolved);
            continue;
        }
        let resolved: Option<Vec<&str>> = references
            .iter()
            .map(|reference| {
                audio_by_path
                    .get(cue_dir.join(reference).as_path())
                    .copied()
            })
            .collect();
        let binding = match resolved {
            Some(resolved) => SheetBinding::Describes {
                file_id: resolved[0].to_string(),
            },
            None => match find_matching_audio_for_cue(&cue_file.path, sheet, &audio_paths) {
                Some(audio_path) => SheetBinding::Describes {
                    file_id: audio_by_path
                        .get(audio_path.as_path())
                        .expect("same-stem match came from this folder's audio")
                        .to_string(),
                },
                None => {
                    info!(
                        "CUE {:?} names audio that is not here; it stays unbound",
                        cue_file.path
                    );
                    SheetBinding::Unresolved
                }
            },
        };
        bindings.insert(*index, binding);
    }

    // One image leads the release: the first conventionally-named image at the
    // release root, or — when the folder keeps its images in a subfolder — the
    // first conventionally-named one anywhere. Sorting by relative path puts
    // `Artwork/front.jpg` before `cover.jpg`, so taking the first match outright
    // would let a nested image outrank the one sitting at the root.
    let cover_index = proposed
        .iter()
        .position(|(file, role)| {
            *role == ProposedRole::Image && is_cover_name(&file.path) && file.dir_prefix.is_none()
        })
        .or_else(|| {
            proposed
                .iter()
                .position(|(file, role)| *role == ProposedRole::Image && is_cover_name(&file.path))
        });

    let mut files: Vec<CandidateFile> = Vec::with_capacity(proposed.len());
    for (index, (file, proposed_role)) in proposed.into_iter().enumerate() {
        let proposed_audio = proposed_role == ProposedRole::Audio;
        let role = match proposed_role {
            ProposedRole::Audio => FileRole::Audio,
            ProposedRole::Cue => FileRole::TrackSheet {
                sheet: sheets
                    .remove(&index)
                    .expect("a file keeps the CUE role only when its sheet parsed"),
                binding: bindings
                    .remove(&index)
                    .expect("every parsed sheet got a binding above"),
                // The scan proposes no disc: a cue filename says nothing about
                // which disc it holds. `settle_sheet_discs` below assigns every
                // parsed sheet, against the bindings that end up in force.
                disc: SheetDisc::Disc { number: 1 },
            },
            ProposedRole::Image if Some(index) == cover_index => FileRole::Cover,
            ProposedRole::Image => FileRole::Artwork,
            ProposedRole::Document => FileRole::Document,
            ProposedRole::Other => FileRole::Other,
        };
        files.push(CandidateFile {
            file,
            role,
            proposed_audio,
        });
    }

    // The user's decisions land over the proposals, and the audio each sheet
    // ends up naming is probed. The hash is what those decisions are stored
    // under, and it covers files only — so computing it here, before any of
    // them is applied, is not an ordering trick: applying one cannot change it.
    let stored = stored
        .for_hash(&content_hash_of(files.iter().map(|entry| &entry.file)))
        .cloned()
        .unwrap_or_default();
    settle_file_roles(&mut files, &stored.file_roles);
    let cue_codec = match settle_sheet_bindings(&mut files, &stored.sheet_bindings, cancellation)? {
        SettledBindings::Settled { cue_codec } => cue_codec,
        SettledBindings::CorruptAudio { path } => {
            return invalid(InvalidReason::CorruptAudioFile { path })
        }
    };
    settle_sheet_discs(&mut files, &stored.sheet_discs);

    let Some(format_label) = derive_format_label(&files, cue_codec) else {
        info!("Invalid candidate: no valid audio files after categorization");
        return invalid(InvalidReason::NoValidAudio);
    };

    Ok(CategorizeOutcome::Valid(CategorizedFiles {
        files,
        format_label,
    }))
}
