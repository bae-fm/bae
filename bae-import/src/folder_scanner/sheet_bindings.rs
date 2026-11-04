//! Binding each track sheet to the audio it describes: what a binding is,
//! the bindings the scan settles and a person's stored ones applied over
//! them, the audio a binding may be offered, the codecs and timing a sheet
//! can use, and the discs bound sheets lay out.

use super::*;

/// A track sheet's audio file, as FFmpeg probes it.
pub(super) enum CueCodecLabel {
    /// A codec bae can play back through a CUE.
    Supported,
    /// A readable codec that can't back CUE playback (e.g. MP3,
    /// Vorbis). Carries the codec's display name for the log line: the binding
    /// is refused with the codec named, and the physical files import
    /// independently.
    Unsupported(String),
    /// The file cleared the header-only magic check but FFmpeg can't identify a
    /// playable stream in it — a download truncated after the header, or
    /// otherwise corrupt audio. Surfaces the folder as a corrupt-audio invalid
    /// candidate instead of aborting the scan.
    Unprobeable,
}

/// Whether a track sheet's audio file can back CUE playback. The
/// answer comes from FFmpeg's probe, never from the extension, because containers
/// such as MP4, Ogg, WAV, and AIFF don't prove the codec by filename.
///
/// A readable file whose codec bae can't play (`Unsupported`) costs the sheet
/// its binding; one FFmpeg can't probe (`Unprobeable`) is corrupt audio and
/// surfaces its folder as invalid, without aborting the watched-root walk.
pub(super) fn cue_pair_codec_label(audio: &ScannedFile) -> CueCodecLabel {
    let Some(probe) = &audio.source_audio else {
        return CueCodecLabel::Unprobeable;
    };
    match &probe.content_type {
        bae_util::content_type::ContentType::Flac
        | bae_util::content_type::ContentType::Ape
        | bae_util::content_type::ContentType::Alac
        | bae_util::content_type::ContentType::Pcm
        | bae_util::content_type::ContentType::WavPack
        | bae_util::content_type::ContentType::Dsd => CueCodecLabel::Supported,
        other => CueCodecLabel::Unsupported(other.display_name().to_string()),
    }
}

pub(super) fn sheet_reference_fits_audio(
    sheet: &CueSheet,
    reference: &str,
    audio: &ScannedFile,
) -> bool {
    sheet
        .playable_tracks()
        .filter(|track| track.file_reference == reference)
        .all(|track| track_fits_audio(track, audio))
}

fn track_fits_audio(track: &crate::cue_sheet::CueTrack, audio: &ScannedFile) -> bool {
    audio.source_audio.as_ref().is_some_and(|source_audio| {
        track.duration_ms_with_file_duration(source_audio.duration_ms).is_ok()
    })
}

fn sheet_fits_resolved_audio(sheet: &CueSheet, resolved: &[(&str, &ScannedFile)]) -> bool {
    sheet.playable_tracks().all(|track| {
        resolved_audio_for(resolved, track).is_some_and(|audio| track_fits_audio(track, audio))
    })
}

/// What settling a folder's sheet bindings produced.
pub(super) enum SettledBindings {
    /// Every sheet settled.
    Settled,
    /// A bound sheet names audio FFmpeg cannot read at all. That is a real
    /// defect, not a disagreement about which file the sheet meant, so the
    /// folder is an invalid candidate rather than one with an unbound sheet.
    CorruptAudio { path: String },
}

/// Settle every parsed sheet's binding, against the folder's audio.
///
/// The user's decision wins where they made one; where they did not, the
/// sheet's `FILE` directives resolve against the folder's audio. Either way
/// the codec the scan probed for the audio a sheet ends up naming is checked,
/// because bae can only carve
/// tracks out of some containers, and a refusal keeps the codec so both the
/// pane and the picker can say why. Total over the parsed sheets: whatever a
/// sheet carried in is replaced, so a stale pairing cannot survive a changed
/// folder.
pub(super) fn settle_sheet_bindings(
    files: &mut [CandidateFile],
    edits: &SheetBindingEdits,
    cancellation: &ScanCancellation,
) -> Result<SettledBindings, FolderScanError> {
    // Which relative paths are this folder's audio. A binding naming anything
    // else describes nothing, whether it came from a directive or from a stored
    // decision this build can no longer place.
    let audio: HashMap<&str, &ScannedFile> = files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Audio))
        .map(|entry| (entry.file.relative_path.as_str(), &entry.file))
        .collect();
    let audio_paths: Vec<PathBuf> = audio.values().map(|file| file.path.clone()).collect();
    let audio_by_path: HashMap<&Path, &ScannedFile> =
        audio.values().map(|file| (file.path.as_path(), *file)).collect();

    let mut settled: Vec<(usize, SheetBinding)> = Vec::new();
    for (index, entry) in files.iter().enumerate() {
        cancellation.check()?;
        let FileRole::TrackSheet { sheet, .. } = &entry.role else {
            continue;
        };
        let sheet_id = entry.file.relative_path.as_str();
        let references = sheet.audio_file_references();
        let mut resolved = Vec::new();
        let mut used = BTreeSet::new();
        for reference in &references {
            let named = match edits.get(sheet_id).and_then(|edits| edits.get(reference)) {
                Some(UserSheetBinding::Describes { file_id }) => {
                    audio.get(file_id.as_str()).copied()
                }
                Some(UserSheetBinding::Cleared) => None,
                None => {
                    resolve_cue_audio_path(&entry.file.path, reference, &audio_paths).map(|path| {
                        *audio_by_path
                            .get(path.as_path())
                            .expect("resolved audio belongs to the candidate")
                    })
                }
            };
            if let Some(named) = named {
                if used.insert(named.relative_path.as_str()) {
                    resolved.push((*reference, named));
                } else {
                    info!(
                        "sheet {sheet_id} associates several FILE references with {}",
                        named.relative_path
                    );
                }
            } else {
                info!("sheet {sheet_id} has no audio for FILE {reference}");
            }
        }
        // The last resort, for a sheet whose references name nothing here —
        // the audio was renamed after the rip. A sheet beside exactly as many
        // audio files as it has references, with nobody having bound any of
        // them by hand, takes those files in name order, one per reference.
        // Whether they fit is what the boundary check below decides.
        if !references.is_empty()
            && resolved.len() != references.len()
            && edits.get(sheet_id).is_none()
        {
            let sheet_dir = entry.file.path.parent();
            let mut beside: Vec<&ScannedFile> =
                audio.values().filter(|file| file.path.parent() == sheet_dir).copied().collect();
            if beside.len() == references.len() {
                beside.sort_by(|a, b| {
                    natord::compare_ignore_case(&a.relative_path, &b.relative_path)
                });
                info!(
                    "sheet {sheet_id} names no audio here; taking the {} audio files beside it \
                     in name order",
                    beside.len()
                );
                resolved = references.iter().copied().zip(beside).collect();
            }
        }
        let named_files = || {
            resolved
                .iter()
                .map(|(reference, audio)| SheetAudioFile {
                    file_reference: reference.to_string(),
                    file_id: audio.relative_path.clone(),
                })
                .collect::<Vec<_>>()
        };
        if references.is_empty() || resolved.len() != references.len() {
            settled.push((index, SheetBinding::Unresolved { files: named_files() }));
            continue;
        }
        let mut refused_codec = None;
        for (_, audio) in &resolved {
            match cue_pair_codec_label(audio) {
                CueCodecLabel::Supported => {}
                CueCodecLabel::Unsupported(codec) => {
                    refused_codec.get_or_insert(codec);
                }
                CueCodecLabel::Unprobeable => {
                    info!(
                        "Invalid candidate: sheet audio file could not be probed: {}",
                        audio.relative_path
                    );
                    return Ok(SettledBindings::CorruptAudio { path: audio.relative_path.clone() });
                }
            }
        }
        let binding = if let Some(codec) = refused_codec {
            info!(
                "sheet {sheet_id} names {codec} audio, which bae can't play from a CUE; \
                 the binding is refused and the physical audio files import independently"
            );
            SheetBinding::RefusedCodec { codec }
        } else if !sheet_fits_resolved_audio(sheet, &resolved) {
            let file_ids = resolved
                .iter()
                .map(|(_, audio)| audio.relative_path.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            info!(
                "sheet {sheet_id} has boundaries outside {file_ids}; it stays unbound and \
                 the physical audio files import independently"
            );
            SheetBinding::RefusedTiming { files: named_files() }
        } else {
            SheetBinding::Resolved { files: named_files() }
        };
        settled.push((index, binding));
    }

    for (index, binding) in settled {
        let FileRole::TrackSheet { binding: slot, .. } = &mut files[index].role else {
            unreachable!("only track-sheet roles were collected above");
        };
        *slot = binding;
    }
    Ok(SettledBindings::Settled)
}

/// Settle every parsed sheet's disc assignment: the user's decision where they
/// made one, and the sheet's own position among the folder's carving sheets
/// where they made none.
///
/// Total over the folder's parsed sheets, and run after
/// [`settle_sheet_bindings`] at every call site, because the position it hands
/// out is a position among the sheets that *carve*. A sheet nobody bound
/// carves nothing either way, so it takes disc one and says nothing by it.
///
/// One sheet per audio file. A rip commonly leaves two sheets for one image
/// (`X.cue` beside `X.flac.cue`), so when several bound sheets describe the
/// same audio the first in the folder's file order carves it and the rest are
/// ignored — a sheet the user assigned a disc to claims its audio ahead of
/// any the scan proposed, and one the user ignored claims none.
pub(super) fn settle_sheet_discs(
    files: &mut [CandidateFile],
    parts: &[ReleasePart],
    edits: &SheetDiscEdits,
) {
    let mut claimed: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for entry in files.iter() {
        if let FileRole::TrackSheet { binding: SheetBinding::Resolved { files: audio }, .. } =
            &entry.role
        {
            if matches!(edits.get(&entry.file.relative_path), Some(SheetDisc::Disc { .. })) {
                claimed.extend(audio.iter().map(|file| file.file_id.as_str()));
            }
        }
    }
    let mut ignored: BTreeSet<String> = BTreeSet::new();
    for entry in files.iter() {
        let FileRole::TrackSheet { binding: SheetBinding::Resolved { files: audio }, .. } =
            &entry.role
        else {
            continue;
        };
        match edits.get(&entry.file.relative_path) {
            Some(SheetDisc::Disc { .. }) => continue,
            Some(SheetDisc::Ignored) => {
                ignored.insert(entry.file.relative_path.clone());
                continue;
            }
            None => {}
        }
        if audio.iter().any(|file| claimed.contains(file.file_id.as_str())) {
            ignored.insert(entry.file.relative_path.clone());
        } else {
            claimed.extend(audio.iter().map(|file| file.file_id.as_str()));
        }
    }
    let carving = |entry: &CandidateFile| match &entry.role {
        FileRole::TrackSheet { binding, .. } => {
            binding.is_resolved() && !ignored.contains(&entry.file.relative_path)
        }
        _ => false,
    };
    let layout = DiscLayout::of(files, parts, carving);
    for entry in files.iter_mut() {
        let FileRole::TrackSheet { binding, disc, .. } = &mut entry.role else {
            continue;
        };
        if !binding.is_resolved() || ignored.contains(&entry.file.relative_path) {
            *disc = SheetDisc::Ignored;
            continue;
        }
        *disc = edits
            .get(&entry.file.relative_path)
            .copied()
            .unwrap_or(SheetDisc::Disc { number: layout.sheet_disc(&entry.file.relative_path) });
    }
}

/// Where each disc of a release falls: the disc each carving sheet takes when
/// nobody assigned it one, and the disc each part's loose audio takes.
///
/// A release read from one folder numbers its carving sheets in file order
/// and gives its loose audio no disc. One read from several gives each part a
/// run of discs of its own, in part order: its loose audio first, if it has
/// any, then each of its carving sheets. A sheet in no part — one beside the
/// parts rather than in one — follows them all.
pub struct DiscLayout {
    sheets: HashMap<String, u32>,
    loose: Vec<Option<u32>>,
}

impl DiscLayout {
    pub(crate) fn of(
        files: &[CandidateFile],
        parts: &[ReleasePart],
        carving: impl Fn(&CandidateFile) -> bool,
    ) -> Self {
        let carving = &carving;
        let carved: std::collections::HashSet<&str> = files
            .iter()
            .filter(|entry| carving(entry))
            .flat_map(|entry| match &entry.role {
                FileRole::TrackSheet { binding: SheetBinding::Resolved { files }, .. } => {
                    files.iter().map(|audio| audio.file_id.as_str()).collect()
                }
                _ => Vec::new(),
            })
            .collect();
        let sheets_in = |part: Option<usize>| {
            files
                .iter()
                .filter(move |entry| {
                    carving(entry) && part_of(parts, &entry.file.relative_path) == part
                })
                .map(|entry| entry.file.relative_path.clone())
                .collect::<Vec<_>>()
        };
        let mut sheets = HashMap::new();
        let mut loose = Vec::with_capacity(parts.len());
        let mut next = 0u32;
        if parts.is_empty() {
            for sheet in sheets_in(None) {
                next += 1;
                sheets.insert(sheet, next);
            }
            return Self { sheets, loose };
        }
        for index in 0..parts.len() {
            let has_loose = files.iter().any(|entry| {
                matches!(entry.role, FileRole::Audio)
                    && !carved.contains(entry.file.relative_path.as_str())
                    && part_of(parts, &entry.file.relative_path) == Some(index)
            });
            loose.push(has_loose.then(|| {
                next += 1;
                next
            }));
            for sheet in sheets_in(Some(index)) {
                next += 1;
                sheets.insert(sheet, next);
            }
        }
        for sheet in sheets_in(None) {
            next += 1;
            sheets.insert(sheet, next);
        }
        Self { sheets, loose }
    }

    /// The disc the carving sheet at `relative_path` takes by default.
    pub(crate) fn sheet_disc(&self, relative_path: &str) -> u32 {
        self.sheets.get(relative_path).copied().unwrap_or(1)
    }

    /// The disc the loose audio of the part at `index` is, if the release
    /// numbers parts.
    pub(crate) fn loose_disc(&self, index: usize) -> Option<u32> {
        self.loose.get(index).copied().flatten()
    }
}

/// One `FILE` reference of a track sheet and the folder audio it describes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SheetAudioFile {
    /// The path as the sheet's `FILE` directive spells it.
    pub file_reference: String,
    /// The [`ScannedFile::relative_path`] of the audio it describes.
    pub file_id: String,
}

/// What a track sheet's `FILE` directives resolved to. The scan settles it
/// once, against the folder's audio, and every reader takes the settled
/// pairing from here rather than resolving the directives again. A sheet that
/// describes nothing is a question for the user, never a verdict on the
/// folder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SheetBinding {
    /// Every `FILE` reference resolved through the sheet itself, in the
    /// sheet's reference order. Never empty.
    Resolved { files: Vec<SheetAudioFile> },
    /// The directive named audio that is not in this folder, named several and
    /// only some resolved, or the sheet names none at all.
    Unresolved { files: Vec<SheetAudioFile> },
    /// Every `FILE` reference found its audio, but the sheet's boundaries do
    /// not fit inside it: a track starts or ends past the audio's end, or
    /// before the track ahead of it. The physical audio files import
    /// independently.
    RefusedTiming { files: Vec<SheetAudioFile> },
    /// The directive resolved, but bae can't carve tracks out of that
    /// audio: the codec doesn't back CUE playback. The physical audio files
    /// still import independently.
    RefusedCodec { codec: String },
}

impl SheetBinding {
    /// The audio this sheet describes, one entry per `FILE` reference in the
    /// sheet's order, or `None` when it describes nothing.
    pub fn audio_files(&self) -> Option<&[SheetAudioFile]> {
        match self {
            Self::Resolved { files }
            | Self::Unresolved { files }
            | Self::RefusedTiming { files } => Some(files),
            Self::RefusedCodec { .. } => None,
        }
    }

    /// Whether this sheet describes playable audio.
    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved { .. })
    }
}

/// A track sheet the scan parsed, with whatever its `FILE` directive resolved to.
#[derive(Debug, Clone, Copy)]
pub struct TrackSheetFile<'a> {
    pub file: &'a ScannedFile,
    pub sheet: &'a crate::cue_sheet::CueSheet,
    pub binding: &'a SheetBinding,
    pub disc: SheetDisc,
}

/// A track sheet whose `FILE` directives resolved, paired with the audio it
/// describes — the unit the track mapper and the disc-ID computer consume.
#[derive(Debug, Clone)]
pub struct BoundTrackSheet<'a> {
    pub file: &'a ScannedFile,
    pub sheet: &'a crate::cue_sheet::CueSheet,
    /// Every CUE `FILE` reference paired with the physical audio it resolved
    /// to, in the sheet's reference order.
    pub audio_files: Vec<(&'a str, &'a ScannedFile)>,
    pub disc: SheetDisc,
}

/// The audio a sheet track plays from, among the sheet's `FILE` references
/// resolved to scanned audio.
fn resolved_audio_for<'a>(
    resolved: &[(&str, &'a ScannedFile)],
    track: &crate::cue_sheet::CueTrack,
) -> Option<&'a ScannedFile> {
    resolved
        .iter()
        .find(|(file_reference, _)| *file_reference == track.file_reference)
        .map(|(_, audio)| *audio)
}

impl<'a> BoundTrackSheet<'a> {
    /// The audio one of the sheet's tracks plays from.
    pub fn audio_for(&self, track: &crate::cue_sheet::CueTrack) -> &'a ScannedFile {
        resolved_audio_for(&self.audio_files, track)
            .expect("a bound sheet resolved every playable track's audio")
    }

    /// Whether this sheet carves the release's tracks out of its container.
    ///
    /// One rule, stated once, so "bound but ignored" and "assigned but unbound"
    /// cannot mean two different things: a sheet speaks for its container only
    /// when it is bound, assigned to a disc, and describes something. A sheet
    /// that carves nothing leaves its container a track of its own.
    pub fn carves(&self) -> bool {
        self.disc_number().is_some() && self.sheet.playable_track_count() > 0
    }

    /// The disc this sheet's entries become, or `None` when it is out of the
    /// tracklist.
    pub fn disc_number(&self) -> Option<u32> {
        match self.disc {
            SheetDisc::Disc { number } => Some(number),
            SheetDisc::Ignored => None,
        }
    }
}

/// A track sheet bound to audio that holds nothing bae can read: the error
/// settling a release's sheet decisions over its files gives.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a track sheet is bound to {path}, which holds no audio bae can read")]
pub struct UnreadableAudio {
    pub path: String,
}

/// Whether one of the folder's audio files can back a sheet's binding, decided
/// from its stored scan facts. Offering a file the commit would then reject is
/// the failure the editable binding exists to remove, so the refusal and its
/// reason are settled here rather than left for the commit to discover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetBindingOffer {
    /// The sheet can be bound to this audio.
    Offered,
    /// bae can't carve tracks out of this container: the codec doesn't back
    /// CUE playback. Carries the probed codec so the picker says
    /// which file and why.
    RefusedCodec { codec: String },
    /// The sheet names boundaries outside this file's measured duration.
    RefusedTiming,
    /// FFmpeg can't identify a playable stream in the file at all — a download
    /// truncated after its header, or otherwise broken audio. Nothing can be
    /// carved out of it either.
    RefusedUnreadable,
}

/// One of the folder's audio files, as a choice for a sheet's binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetBindingOption {
    /// The audio's [`ScannedFile::relative_path`] — the id the binding is set
    /// by (`ImportServiceHandle::set_sheet_binding`).
    pub file_id: String,
    pub offer: SheetBindingOffer,
}

/// One CUE FILE reference, its current association, and available choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetReferenceOptions {
    pub file_reference: String,
    pub file_id: Option<String>,
    pub options: Vec<SheetBindingOption>,
}

impl CategorizedFiles {
    /// Every FILE reference with its association and the audio it can use.
    pub fn sheet_binding_options(&self, sheet_file_id: &str) -> Vec<SheetReferenceOptions> {
        let Some(sheet) =
            self.track_sheets().find(|sheet| sheet.file.relative_path == sheet_file_id)
        else {
            return Vec::new();
        };
        sheet
            .sheet
            .audio_file_references()
            .into_iter()
            .map(|reference| {
                let associated = sheet
                    .binding
                    .audio_files()
                    .into_iter()
                    .flatten()
                    .find(|file| file.file_reference == reference);
                SheetReferenceOptions {
                    file_reference: reference.to_owned(),
                    file_id: associated.map(|file| file.file_id.clone()),
                    options: self
                        .audio()
                        .filter(|audio| {
                            !sheet.binding.audio_files().into_iter().flatten().any(|file| {
                                file.file_reference != reference
                                    && file.file_id == audio.relative_path
                            })
                        })
                        .map(|audio| SheetBindingOption {
                            file_id: audio.relative_path.clone(),
                            offer: match cue_pair_codec_label(audio) {
                                CueCodecLabel::Supported
                                    if sheet_reference_fits_audio(
                                        sheet.sheet,
                                        reference,
                                        audio,
                                    ) =>
                                {
                                    SheetBindingOffer::Offered
                                }
                                CueCodecLabel::Supported => SheetBindingOffer::RefusedTiming,
                                CueCodecLabel::Unsupported(codec) => {
                                    SheetBindingOffer::RefusedCodec { codec }
                                }
                                CueCodecLabel::Unprobeable => SheetBindingOffer::RefusedUnreadable,
                            },
                        })
                        .collect(),
                }
            })
            .collect()
    }

    /// Apply the user's sheet decisions over what the scan proposed, and
    /// re-derive everything they decide: what each sheet ends up naming, the
    /// probed codec that can refuse a binding, and which disc each sheet is.
    ///
    /// Disc assignments settle after bindings, because the position a sheet
    /// with no stored decision takes is a position among the sheets that ended
    /// up bound.
    ///
    /// Idempotent, and it only ever *overrides*: a file or sheet with no
    /// decision keeps what it already has, which is what makes a cleared
    /// binding stay cleared instead of springing back to the scan's guess.
    pub fn apply_candidate_file_edits(
        &mut self,
        edits: &CandidateFileEdits,
    ) -> Result<(), UnreadableAudio> {
        match settle_sheet_bindings(
            &mut self.files,
            &edits.sheet_bindings,
            &ScanCancellation::new(),
        )
        .expect("a fresh cancellation token cannot be cancelled")
        {
            SettledBindings::Settled => {
                settle_sheet_discs(&mut self.files, &self.parts, &edits.sheet_discs);
                Ok(())
            }
            SettledBindings::CorruptAudio { path } => Err(UnreadableAudio { path }),
        }
    }
}

#[cfg(test)]
#[path = "sheet_bindings_tests.rs"]
mod tests;
