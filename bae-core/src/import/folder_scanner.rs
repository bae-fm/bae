//! Folder scanner for import release candidates.
//!
//! Each directory with direct audio approximates one release. When a directory
//! also contains audio-bearing descendants, the scanner reports an unresolved
//! boundary instead of guessing whether the parent or descendants are releases.
//! The walk lists one directory at a time and reports candidates as they become
//! available.
use super::file_validation;
use crate::cue_flac::parse_cue_sheet;
use crate::util::content_type_hint::ContentTypeHint;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
// Only `release_decision_removed_keys` names this, and it is desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{debug, info};

const DOCUMENT_EXTENSIONS: &[&str] = &["cue", "log", "txt", "m3u", "m3u8"];

/// Extensions used by download clients and browsers to mark an
/// in-progress download. Presence of any of these anywhere in a folder means
/// the folder is mid-download and should not surface as an import candidate.
const PARTIAL_MARKER_EXTENSIONS: &[&str] = &["part", "crdownload", "download", "aria2", "partial"];

// ── Public types ────────────────────────────────────────────────────────────

/// A file discovered during folder scanning
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScannedFile {
    /// Absolute filesystem path. A scanned file is always on disk.
    pub path: PathBuf,
    /// The file's identity within the release (e.g. `CD1/CDImage.ape`) — used
    /// as the HashMap key throughout the import pipeline and as
    /// `DbFile.original_filename`. Two files may share a bare filename; they
    /// never share a relative_path within one release.
    ///
    /// Always `/`-separated, on every platform: it is stored, synced, and joined
    /// back onto a local directory by whichever device exports or unmanages the
    /// release, so it cannot carry the writing platform's separator (see
    /// [`crate::storage::path_fragment`], which refuses one that does).
    pub relative_path: String,
    /// File size in bytes
    pub size: u64,
    /// Directory prefix of relative_path (e.g. "Disc 1/"). `None` when the
    /// file is at the candidate-folder root.
    pub dir_prefix: Option<String>,
    /// File name without directory prefix.
    pub file_name: String,
}

impl ScannedFile {
    pub fn new(path: PathBuf, relative_path: String, size: u64) -> Self {
        let (dir_prefix, file_name) = match relative_path.rfind('/') {
            Some(idx) => (
                Some(relative_path[..=idx].to_string()),
                relative_path[idx + 1..].to_string(),
            ),
            None => (None, relative_path.clone()),
        };
        Self {
            path,
            relative_path,
            size,
            dir_prefix,
            file_name,
        }
    }
}

/// The job the scan proposes for a file. Every file the scan finds carries
/// exactly one: the scan *proposes*, so nothing is discarded and no folder is
/// refused because a filename disagrees with a sheet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FileRole {
    /// Playable audio.
    Audio,
    /// A track sheet (CUE) that parsed, carrying its parsed sheet, what its
    /// `FILE` directive resolved to, and which disc of the release its entries
    /// become.
    TrackSheet {
        sheet: crate::cue_flac::CueSheet,
        binding: SheetBinding,
        disc: SheetDisc,
    },
    /// The image that leads the release, proposed from the conventional cover
    /// filenames. At most one per folder.
    Cover,
    /// Any other image.
    Artwork,
    /// Readable evidence: a rip log, a tracklist, a playlist, or a CUE that
    /// could not be parsed as a sheet.
    Document,
    /// In the folder and carried with the release, unrecognized: a scene
    /// sidecar (`.nfo`, `.sfv`, `.md5`), a stray video, a file with no
    /// extension. The folder is the release, so it imports, uploads, and comes
    /// back on export like everything else.
    Other,
}

/// What a track sheet's `FILE` directive resolved to. The scan proposes it; a
/// sheet that describes nothing is a question for the user, never a verdict on
/// the folder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SheetBinding {
    /// Bound to the audio named by this [`ScannedFile::relative_path`].
    Describes { file_id: String },
    /// The directive named audio that is not in this folder, named several and
    /// only some resolved, or the sheet names none at all.
    Unresolved,
    /// The directive resolved, but bae can't carve tracks out of that
    /// container: the codec doesn't back single-file CUE playback. The audio
    /// still imports, as one track. Carries the file it named and the probed
    /// codec, so the pane can say which file and why, and so the editor that
    /// makes this binding a user decision can refuse the same pairing up front
    /// instead of failing at commit.
    RefusedCodec { file_id: String, codec: String },
}

impl SheetBinding {
    /// The audio this sheet describes — only a resolved, playable binding.
    pub fn describes(&self) -> Option<&str> {
        match self {
            Self::Describes { file_id } => Some(file_id),
            Self::Unresolved | Self::RefusedCodec { .. } => None,
        }
    }
}

/// Which disc of the release one track sheet's entries become.
///
/// Cue filenames are arbitrary — `CD1.cue` may hold disc two — so the mapping
/// cannot read the order off the names. This is the answer, and like a sheet's
/// binding and a file's role it is the user's to overrule and it survives a
/// restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SheetDisc {
    /// The sheet's entries are the release's disc `number`, counting from one.
    Disc { number: u32 },
    /// The sheet contributes nothing to the tracklist. Its container is loose
    /// audio again, exactly as an unbound sheet leaves it.
    Ignored,
}

/// Every disc assignment the user has decided for one candidate, keyed by the
/// sheet's [`ScannedFile::relative_path`].
///
/// A sheet *absent* from this is not a decision — it takes its own position
/// among the folder's bound sheets, in `relative_path` order.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SheetDiscEdits(BTreeMap<String, SheetDisc>);

impl SheetDiscEdits {
    /// The user's decision for one sheet, or `None` when they have made none.
    pub fn get(&self, sheet_file_id: &str) -> Option<SheetDisc> {
        self.0.get(sheet_file_id).copied()
    }

    /// Record one sheet's decision, replacing any previous one.
    pub fn set(&mut self, sheet_file_id: String, disc: SheetDisc) {
        self.0.insert(sheet_file_id, disc);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A role a person can put a file in, as opposed to the whole [`FileRole`] the
/// scan proposes.
///
/// Only audio is a decision here. Every other role either has no consequence to
/// change — an image is an image — or already has its own editor: a track
/// sheet's job is decided by what it is bound to, so taking a sheet out of the
/// tracklist is clearing its binding, not a second control saying the same
/// thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileRoleChoice {
    /// One of the release's tracks.
    Audio,
    /// Carried with the release — the folder is the release, so it still
    /// imports, uploads, and comes back on export — but not one of its tracks.
    /// This is what a slot's Exclude action writes.
    NotATrack,
}

/// Every file role the user has decided for one candidate, keyed by the file's
/// [`ScannedFile::relative_path`].
///
/// A file *absent* from this is not a decision — the scan's proposal stands.
/// Both variants are therefore stored: putting a file back is as much a
/// decision as taking it out, and re-guessing after either one is the answer
/// that is certainly not what was asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FileRoleEdits(BTreeMap<String, FileRoleChoice>);

impl FileRoleEdits {
    /// The user's decision for one file, or `None` when they have made none.
    pub fn get(&self, file_id: &str) -> Option<FileRoleChoice> {
        self.0.get(file_id).copied()
    }

    /// Record one file's decision, replacing any previous one.
    pub fn set(&mut self, file_id: String, choice: FileRoleChoice) {
        self.0.insert(file_id, choice);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One track sheet's binding as the *user* set it — the second writer of
/// [`SheetBinding`], alongside the scan.
///
/// Stored per candidate and applied over whatever the scan proposed, so the
/// two never fight: the scan proposes on every walk, and this overrides.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UserSheetBinding {
    /// Bound to the audio at this [`ScannedFile::relative_path`].
    Describes { file_id: String },
    /// The user cleared the binding: the sheet is unbound, and the scan's
    /// proposal is *not* restored. Someone who cleared a binding is saying the
    /// guess was wrong, so re-guessing it is the one answer that is certainly
    /// not what they asked for.
    Cleared,
}

/// Every sheet binding the user has decided for one candidate, keyed by the
/// sheet's [`ScannedFile::relative_path`].
///
/// A sheet *absent* from this is not a decision — it means nobody has touched
/// that sheet and the scan's proposal stands. That is why clearing stores
/// [`UserSheetBinding::Cleared`] rather than removing the entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SheetBindingEdits(BTreeMap<String, UserSheetBinding>);

impl SheetBindingEdits {
    /// The user's decision for one sheet, or `None` when they have made none.
    pub fn get(&self, sheet_file_id: &str) -> Option<&UserSheetBinding> {
        self.0.get(sheet_file_id)
    }

    /// Record one sheet's decision, replacing any previous one.
    pub fn set(&mut self, sheet_file_id: String, binding: UserSheetBinding) {
        self.0.insert(sheet_file_id, binding);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Everything the user has settled about one candidate's files: which audio
/// each track sheet describes, which disc each sheet's entries become, and
/// which files are the release's tracks.
///
/// One value because it is one stored row — every part is keyed by the same
/// content hash and read by the same scan, and splitting them would be several
/// things to keep in step.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateFileEdits {
    pub sheet_bindings: SheetBindingEdits,
    pub file_roles: FileRoleEdits,
    pub sheet_discs: SheetDiscEdits,
    pub revision: u64,
}

impl CandidateFileEdits {
    pub fn is_empty(&self) -> bool {
        self.sheet_bindings.is_empty() && self.file_roles.is_empty() && self.sheet_discs.is_empty()
    }
}

/// The file decisions every candidate has stored, keyed by content hash.
///
/// A scan is what computes a candidate's content hash, so the lookup has to
/// happen *inside* the scan: a caller hands in the whole stored set rather than
/// trying to pick one candidate's row without the key that addresses it.
#[derive(Debug, Clone, Default)]
pub struct StoredCandidateEdits(HashMap<String, CandidateFileEdits>);

impl StoredCandidateEdits {
    pub fn new(by_content_hash: HashMap<String, CandidateFileEdits>) -> Self {
        Self(by_content_hash)
    }

    /// Nothing stored: every file and every sheet keeps the scan's proposal.
    pub fn none() -> Self {
        Self::default()
    }

    fn for_hash(&self, content_hash: &str) -> Option<&CandidateFileEdits> {
        self.0.get(content_hash)
    }

    pub(crate) fn revision_for_hash(&self, content_hash: &str) -> u64 {
        self.for_hash(content_hash)
            .map_or(0, |edits| edits.revision)
    }

    /// One candidate's decisions, to be added to and written back. Empty when
    /// it has none yet.
    pub fn take(mut self, content_hash: &str) -> CandidateFileEdits {
        self.0.remove(content_hash).unwrap_or_default()
    }
}

/// Whether one of the folder's audio files can back a sheet's binding, decided
/// at offer time by probing it. Offering a file the commit would then reject is
/// the failure the editable binding exists to remove, so the refusal and its
/// reason are settled here rather than left for the commit to discover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetBindingOffer {
    /// The sheet can be bound to this audio.
    Offered,
    /// bae can't carve tracks out of this container: the codec doesn't back
    /// single-file CUE playback. Carries the probed codec so the picker says
    /// which file and why.
    RefusedCodec { codec: String },
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

/// A file the scan found, and the role in force for it — the scan's proposal,
/// or the user's decision over it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CandidateFile {
    pub file: ScannedFile,
    pub role: FileRole,
    /// Whether the scan read this file as playable audio.
    ///
    /// Kept because [`Self::role`] does not say it once a decision has landed:
    /// a track the user took out of the release reads [`FileRole::Other`],
    /// which is also what an unrecognized sidecar reads. This is what makes
    /// putting it back offerable, and what keeps a JPEG from being offered as
    /// a track.
    pub proposed_audio: bool,
}

impl CandidateFile {
    /// The roles this file can be put in, the one in force first, or empty
    /// when its role is nobody's decision to make.
    pub fn role_alternatives(&self) -> &'static [FileRoleChoice] {
        if self.proposed_audio {
            &[FileRoleChoice::Audio, FileRoleChoice::NotATrack]
        } else {
            &[]
        }
    }

    /// The role in force as a choice — what a picker shows selected. `None`
    /// exactly when [`Self::role_alternatives`] is empty.
    pub fn role_choice(&self) -> Option<FileRoleChoice> {
        if !self.proposed_audio {
            return None;
        }
        Some(match self.role {
            FileRole::Audio => FileRoleChoice::Audio,
            _ => FileRoleChoice::NotATrack,
        })
    }
}

/// What a file's role makes of it in the release being imported — the "Becomes"
/// column, as a consequence rather than as prose.
///
/// Only the tracklist is in it. Which slots a file backs is the one thing the
/// role does not already say, and it is what makes the effect of a binding or
/// an exclusion legible without reading the slot table below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBecomes {
    /// Track slots `first`..=`last`, counting the release's slots from one.
    /// `first == last` is the single-slot case a loose audio file produces.
    Slots { first: u32, last: u32 },
    /// Nothing in the tracklist: an image, a document, a sheet that describes
    /// nothing, the container a bound sheet carves its slots out of, or a file
    /// somebody took out. It is still carried with the release.
    NoSlots,
}

/// The job a collapsed directory's files share. Audio, track sheets and images
/// are deliberately absent: a folder of tracks is exactly what the roles table
/// exists to show one row at a time, and the images are one gallery however
/// many directories they sit in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRowKind {
    Document,
    Other,
}

/// A directory whose files all do the same job, which the roles table shows as
/// one row — `covers/ — 14 images` — instead of one row each. Nothing in it
/// needs a decision, so listing every file buys nothing and costs the table its
/// readability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapsedDirectory {
    /// The prefix its files carry in [`ScannedFile::dir_prefix`], e.g.
    /// `covers/` — which is also how a renderer tells which files it stands
    /// for.
    pub dir_prefix: String,
    pub kind: FileRowKind,
    pub count: u32,
    pub total_size: u64,
}

/// A track sheet the scan parsed, with whatever its `FILE` directive resolved to.
#[derive(Debug, Clone, Copy)]
pub struct TrackSheetFile<'a> {
    pub file: &'a ScannedFile,
    pub sheet: &'a crate::cue_flac::CueSheet,
    pub binding: &'a SheetBinding,
    pub disc: SheetDisc,
}

/// A track sheet whose `FILE` directive resolved, paired with the audio it
/// describes — the unit the track mapper and the disc-ID computer consume.
#[derive(Debug, Clone, Copy)]
pub struct BoundTrackSheet<'a> {
    pub file: &'a ScannedFile,
    pub sheet: &'a crate::cue_flac::CueSheet,
    pub audio: &'a ScannedFile,
    pub disc: SheetDisc,
}

impl BoundTrackSheet<'_> {
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

/// A release's files, each carrying the role the scan proposed for it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CategorizedFiles {
    /// Every file under the release root, in `relative_path` order, each with
    /// the role the scan proposed. All of them are the release's — see
    /// [`Self::release_files`].
    pub files: Vec<CandidateFile>,
    /// e.g. "CUE+FLAC", "CUE+APE", "FLAC", "MP3". Computed during the scan
    /// because a bound sheet's codec comes from an FFmpeg probe, never from the
    /// extension.
    pub format_label: String,
}

impl CategorizedFiles {
    /// The release's audio files, in `relative_path` order.
    pub fn audio(&self) -> impl Iterator<Item = &ScannedFile> {
        self.files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Audio))
            .map(|entry| &entry.file)
    }

    /// The release's images — the proposed cover and everything else.
    pub fn artwork(&self) -> impl Iterator<Item = &ScannedFile> {
        self.files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Cover | FileRole::Artwork))
            .map(|entry| &entry.file)
    }

    /// The release's readable evidence files.
    pub fn documents(&self) -> impl Iterator<Item = &ScannedFile> {
        self.files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Document))
            .map(|entry| &entry.file)
    }

    /// Every file the release carries, in `relative_path` order — the rows the
    /// import writes, the blobs coven uploads, and the set
    /// [`Self::content_hash`] covers. The folder is the release: what you
    /// import is what is stored and what comes back on export, so nothing the
    /// walk keeps is left behind.
    ///
    /// One definition, read by both the payload and the hash. Computing them
    /// from parallel lists would let them drift, and a hash that stopped
    /// describing the payload is the bug that duplicates a release instead of
    /// replacing it.
    pub fn release_files(&self) -> impl Iterator<Item = &ScannedFile> {
        self.files.iter().map(|entry| &entry.file)
    }

    /// Every parsed track sheet, bound or not.
    pub fn track_sheets(&self) -> impl Iterator<Item = TrackSheetFile<'_>> {
        self.files.iter().filter_map(|entry| match &entry.role {
            FileRole::TrackSheet {
                sheet,
                binding,
                disc,
            } => Some(TrackSheetFile {
                file: &entry.file,
                sheet,
                binding,
                disc: *disc,
            }),
            _ => None,
        })
    }

    /// The track sheets whose `FILE` directive resolved, each with the audio it
    /// describes, in the sheets' `relative_path` order.
    pub fn bound_sheets(&self) -> Vec<BoundTrackSheet<'_>> {
        self.track_sheets()
            .filter_map(|sheet| {
                let describes = sheet.binding.describes()?;
                let audio = self.audio().find(|file| file.relative_path == describes)?;
                Some(BoundTrackSheet {
                    file: sheet.file,
                    sheet: sheet.sheet,
                    audio,
                    disc: sheet.disc,
                })
            })
            .collect()
    }

    /// The bound sheets that carve the release's tracks — the ones the
    /// tracklist, the disc IDs and the Unknown seed are all read from.
    pub fn carving_sheets(&self) -> Vec<BoundTrackSheet<'_>> {
        self.bound_sheets()
            .into_iter()
            .filter(BoundTrackSheet::carves)
            .collect()
    }

    /// Total track count across the release: the tracks the carving sheets
    /// carve, or — with no sheet carving — one per audio file.
    pub fn track_count(&self) -> u32 {
        let carving = self.carving_sheets();
        if carving.is_empty() {
            self.audio().count() as u32
        } else {
            carving
                .iter()
                .map(|carving| carving.sheet.playable_track_count() as u32)
                .sum()
        }
    }

    /// This release's audio file paths, in `relative_path` order. The Unknown
    /// import path reads embedded cover art from these, and the signal fast pass
    /// probes their durations.
    pub fn audio_paths(&self) -> Vec<PathBuf> {
        self.audio().map(|file| file.path.clone()).collect()
    }

    /// Stable content fingerprint of this release's file structure: a SHA-256
    /// over the relative path + size of every file in [`Self::release_files`],
    /// sorted so the digest is independent of discovery order. Relative (not
    /// absolute) paths make it location-independent — the same rip hashes
    /// identically under any parent folder. Drives "already imported?"
    /// detection and selects the overwrite target on re-import.
    ///
    /// It hashes exactly what the release carries: same iterator as the
    /// payload, so the fingerprint cannot describe a different set of files
    /// than the one that gets stored.
    ///
    /// It hashes **files**, never roles. A sheet's binding and a file's role
    /// are user decisions stored under this hash, so a hash that moved when one
    /// of them changed would orphan the very row it addresses on every edit.
    ///
    /// That holds only because no role decision removes a file from
    /// [`Self::release_files`]. Taking a file out of the tracklist takes it out
    /// of the *tracks*, not out of the release — the folder is the release, so
    /// the file still imports, uploads and comes back on export. **For whoever
    /// adds a role that does drop a file from the payload:** this stops
    /// holding. Decide deliberately whether such a file leaves the payload, and
    /// therefore whether this hash moves for it. It cannot be both.
    pub fn content_hash(&self) -> String {
        content_hash_of(self.release_files())
    }

    /// What the sheet at `sheet_file_id` can be bound to: the folder's audio,
    /// each file either offered or refused with the reason.
    ///
    /// The refusal is decided *here*, by probing, because offering a file the
    /// commit would then reject is exactly the failure an editable binding
    /// exists to remove. Probing is also why this is asked for when a picker
    /// opens rather than carried on every candidate.
    ///
    /// Empty when the sheet names one audio file per track rather than one for
    /// the whole disc: naming a single file cannot express that layout, so
    /// there is nothing to offer. Empty too when the folder holds no audio, and
    /// when `sheet_file_id` names no parsed sheet.
    pub fn sheet_binding_options(&self, sheet_file_id: &str) -> Vec<SheetBindingOption> {
        let Some(sheet) = self
            .track_sheets()
            .find(|sheet| sheet.file.relative_path == sheet_file_id)
        else {
            return Vec::new();
        };
        if sheet.sheet.single_file().is_none() {
            return Vec::new();
        }
        self.audio()
            .map(|audio| SheetBindingOption {
                file_id: audio.relative_path.clone(),
                offer: match cue_pair_codec_label(&audio.path) {
                    Ok(CueCodecLabel::Supported(_)) => SheetBindingOffer::Offered,
                    Ok(CueCodecLabel::Unsupported(codec)) => {
                        SheetBindingOffer::RefusedCodec { codec }
                    }
                    Ok(CueCodecLabel::Unprobeable) => SheetBindingOffer::RefusedUnreadable,
                    // A path FFmpeg cannot even open is unreadable by the only
                    // measure that matters here.
                    Err(e) => {
                        debug!("{} cannot back a sheet binding: {e}", audio.relative_path);
                        SheetBindingOffer::RefusedUnreadable
                    }
                },
            })
            .collect()
    }

    /// Apply the user's file decisions over what the scan proposed, and
    /// re-derive everything they decide: which files are audio, what each sheet
    /// ends up naming, the codec probe that can refuse a binding, and the
    /// release's format label.
    ///
    /// Roles settle first. A file taken out of the tracklist stops being audio,
    /// so a sheet bound to it describes nothing — settling the bindings against
    /// the roles that are still standing is what keeps those two from
    /// disagreeing. Disc assignments settle last, because the position a sheet
    /// with no stored decision takes is a position among the sheets that ended
    /// up bound.
    ///
    /// Idempotent, and it only ever *overrides*: a file or sheet with no
    /// decision keeps what it already has, which is what makes a cleared
    /// binding stay cleared instead of springing back to the scan's guess.
    pub fn apply_candidate_file_edits(
        &mut self,
        edits: &CandidateFileEdits,
    ) -> Result<(), InvalidReason> {
        settle_file_roles(&mut self.files, &edits.file_roles);
        match settle_sheet_bindings(
            &mut self.files,
            &edits.sheet_bindings,
            &ScanCancellation::new(),
        )
        .expect("a fresh cancellation token cannot be cancelled")
        {
            SettledBindings::Settled { cue_codec } => {
                settle_sheet_discs(&mut self.files, &edits.sheet_discs);
                self.format_label = derive_format_label(&self.files, cue_codec)
                    .ok_or(InvalidReason::NoValidAudio)?;
                Ok(())
            }
            SettledBindings::CorruptAudio { path } => Err(InvalidReason::CorruptAudioFile { path }),
        }
    }

    /// What each file's role makes of it, in [`Self::files`] order — one entry
    /// per file, so the two lists index together.
    ///
    /// Desktop only, like the rest of the import: it reads the same audio-unit
    /// list the track slots are laid out from, so the two cannot disagree about
    /// which slot a file backs.
    ///
    /// The slot numbering is the folder's own, and it exists before any release
    /// is picked: which slots a file backs is decided by the folder's audio and
    /// its bound sheets, never by the tracklist laid alongside them.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn becomes(&self) -> Vec<FileBecomes> {
        let units = crate::import::track_slots::audio_units(self);

        // Which slots each file and each sheet produced, as the half-open run
        // it occupies in the unit list. A sheet's slices are contiguous by
        // construction, and so are a file's, so a first and a last say it all.
        let mut runs: HashMap<&str, (u32, u32)> = HashMap::new();
        let mut spoken_for: BTreeSet<&str> = BTreeSet::new();
        for (index, unit) in units.iter().enumerate() {
            let slot = index as u32 + 1;
            let owner = match unit {
                crate::import::types::AudioFile::Standalone { file_id } => file_id.as_str(),
                crate::import::types::AudioFile::SheetSlice {
                    file_id, sheet_id, ..
                } => {
                    // The container is the sheet's to speak for; its own row
                    // says so by carving nothing.
                    spoken_for.insert(file_id.as_str());
                    sheet_id.as_str()
                }
            };
            runs.entry(owner)
                .and_modify(|(_, last)| *last = slot)
                .or_insert((slot, slot));
        }

        self.files
            .iter()
            .map(|entry| {
                let id = entry.file.relative_path.as_str();
                match runs.get(id) {
                    Some((first, last)) if !spoken_for.contains(id) => FileBecomes::Slots {
                        first: *first,
                        last: *last,
                    },
                    _ => FileBecomes::NoSlots,
                }
            })
            .collect()
    }

    /// The directories the roles table shows as one row instead of one row per
    /// file.
    ///
    /// Collapsing is decided here rather than by each UI, because two UIs
    /// deciding it separately is two answers to one question about the
    /// release's shape. Audio and track sheets never collapse — one row per
    /// track is the point of the table — and neither do images, which the
    /// gallery shows whole wherever in the folder they sit.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn collapsed_directories(&self) -> Vec<CollapsedDirectory> {
        let mut collapsible: BTreeMap<&str, (FileRowKind, u32, u64)> = BTreeMap::new();
        let mut mixed: BTreeSet<&str> = BTreeSet::new();
        for entry in &self.files {
            let Some(dir_prefix) = entry.file.dir_prefix.as_deref() else {
                continue;
            };
            let kind = match entry.role {
                FileRole::Document => Some(FileRowKind::Document),
                FileRole::Other => Some(FileRowKind::Other),
                FileRole::Audio
                | FileRole::TrackSheet { .. }
                | FileRole::Cover
                | FileRole::Artwork => None,
            };
            // A directory holding anything that needs its own row, or holding
            // two different jobs, is not homogeneous — every one of its files
            // gets a row. Recorded rather than removed, because a later file
            // in the same directory would otherwise start the group again.
            let Some(kind) = kind else {
                mixed.insert(dir_prefix);
                continue;
            };
            match collapsible.get_mut(dir_prefix) {
                Some((seen, count, size)) if *seen == kind => {
                    *count += 1;
                    *size += entry.file.size;
                }
                Some(_) => {
                    mixed.insert(dir_prefix);
                }
                None => {
                    collapsible.insert(dir_prefix, (kind, 1, entry.file.size));
                }
            }
        }
        collapsible
            .into_iter()
            // One file is not a group worth hiding.
            .filter(|(dir_prefix, (_, count, _))| *count > 1 && !mixed.contains(dir_prefix))
            .map(
                |(dir_prefix, (kind, count, total_size))| CollapsedDirectory {
                    dir_prefix: dir_prefix.to_string(),
                    kind,
                    count,
                    total_size,
                },
            )
            .collect()
    }
}

/// SHA-256 over the sorted `(relative_path, size)` of every file — the free
/// function [`CategorizedFiles::content_hash`] is the method form of. Separate
/// because the scan needs a candidate's hash while it is still assembling the
/// candidate, to look up the bindings stored under it.
fn content_hash_of<'a>(files: impl Iterator<Item = &'a ScannedFile>) -> String {
    let mut entries: Vec<(&str, u64)> = files
        .map(|file| (file.relative_path.as_str(), file.size))
        .collect();
    entries.sort_unstable();

    let mut hasher = Sha256::new();
    for (path, size) in entries {
        hasher.update(path.as_bytes());
        hasher.update([0u8]);
        hasher.update(size.to_le_bytes());
        hasher.update([b'\n']);
    }
    format!("{:x}", hasher.finalize())
}

/// Why a candidate folder failed validation. The `Display` text is the terse
/// internal form (used by the import-commit error channel); the UI localizes the
/// typed variant for the Skipped tab.
///
/// Only real defects invalidate. A sheet that disagrees with what is on disk —
/// naming absent audio, failing to parse, or naming a codec bae can't play from
/// a single-file CUE — leaves the sheet unbound and the folder importable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum InvalidReason {
    #[error("corrupt or zero-byte audio file: {path}")]
    CorruptAudioFile { path: String },
    #[error("corrupt or zero-byte image: {path}")]
    CorruptImage { path: String },
    #[error("no valid audio files")]
    NoValidAudio,
}

/// A leaf folder that looks like a release (it has audio) but failed validation.
/// It can't be imported, so it carries no files and no identify state — only
/// enough to surface it under the Skipped tab with its reason.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InvalidCandidate {
    /// Root path of the folder that failed validation.
    pub path: PathBuf,
    /// Display name (derived from folder name).
    pub name: String,
    /// Absolute path of the watched folder this was scanned from — the
    /// candidate-list group it belongs to. Equal to the scan root.
    pub watched_folder_path: String,
    pub display_path: String,
    /// Explicit release-structure decisions that exposed this invalid row.
    pub resolved_boundaries: Vec<ResolvedFolderReleaseBoundary>,
    /// Why the folder failed validation — the UI localizes this typed reason.
    pub reason: InvalidReason,
}

/// One item the scan callback yields per leaf folder: a valid release
/// candidate, or an invalid one (looked like a release but failed validation).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ScanItem {
    /// A release approximation completed before its enclosing folder boundary
    /// was known. It is visible scan progress, but identification must wait for
    /// a later [`Self::Valid`] or [`Self::Boundary`] update.
    Discovered(FolderCandidate),
    Valid(FolderCandidate),
    Invalid(InvalidCandidate),
    Boundary(FolderReleaseBoundary),
}

/// Only the durable folder-scan tables key items this way, and those are
/// desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ScanItem {
    pub(crate) fn persisted_key(&self) -> String {
        match self {
            Self::Discovered(candidate) | Self::Valid(candidate) => {
                candidate.path.to_string_lossy().into_owned()
            }
            Self::Invalid(candidate) => candidate.path.to_string_lossy().into_owned(),
            Self::Boundary(boundary) => Path::new(&boundary.key.watched_folder_path)
                .join(&boundary.key.relative_folder_path)
                .to_string_lossy()
                .into_owned(),
        }
    }
}

/// Which files a folder boundary owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReleaseFileScope {
    /// Files directly in the folder plus descendants below children with no
    /// audio of their own.
    Direct,
    /// Every file below the folder.
    Recursive,
}

/// The stable address of one folder-boundary decision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FolderReleaseDecisionKey {
    pub watched_folder_path: String,
    /// `/`-separated path below the watched root. Empty names the root.
    pub relative_folder_path: String,
}

/// The user's explicit interpretation of an ambiguous folder boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FolderReleaseDecision {
    CombineAsOneRelease,
    KeepAsSeparateReleases,
}

/// Which persisted scan entries a set of boundary decisions supersedes. Reads
/// the durable scan entries, so it exists only where scans persist — desktop.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn release_decision_removed_keys(
    persisted_keys: &HashSet<String>,
    decisions: &[(FolderReleaseDecisionKey, FolderReleaseDecision)],
) -> Vec<String> {
    let mut removed = Vec::new();
    for (key, decision) in decisions {
        let boundary_path = Path::new(&key.watched_folder_path).join(&key.relative_folder_path);
        let boundary_key = boundary_path.to_string_lossy();
        removed.extend(persisted_keys.iter().filter_map(|candidate_key| {
            let path = Path::new(candidate_key);
            let superseded = match decision {
                FolderReleaseDecision::CombineAsOneRelease => path.starts_with(&boundary_path),
                FolderReleaseDecision::KeepAsSeparateReleases => {
                    candidate_key == boundary_key.as_ref()
                }
            };
            superseded.then(|| candidate_key.clone())
        }));
    }
    removed.sort();
    removed.dedup();
    removed
}

/// Decisions loaded for one watched root before its scan begins.
#[derive(Debug, Clone, Default)]
pub struct FolderReleaseDecisions(HashMap<String, FolderReleaseDecision>);

impl FolderReleaseDecisions {
    pub fn new(decisions: HashMap<String, FolderReleaseDecision>) -> Self {
        Self(decisions)
    }

    pub(crate) fn get(&self, relative_folder_path: &str) -> Option<FolderReleaseDecision> {
        self.0.get(relative_folder_path).copied()
    }
}

/// A folder row in an unresolved boundary's compact tree.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FolderReleaseTreeRow {
    pub name: String,
    pub display_path: String,
    pub depth: u32,
    pub kind: FolderReleaseTreeRowKind,
    pub decision_key: FolderReleaseDecisionKey,
    /// Enclosing unresolved boundaries that become separate when this row is
    /// resolved. The UI submits only `decision_key`; core persists these with
    /// it in one transaction.
    pub ancestor_decision_keys: Vec<FolderReleaseDecisionKey>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FolderReleaseTreeRowKind {
    Folder,
    Candidate {
        summary: FolderReleaseCandidateSummary,
    },
    Invalid {
        reason: InvalidReason,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FolderReleaseCandidateSummary {
    pub track_count: u32,
    pub format_label: String,
}

/// A folder whose structure admits both one recursive release and several
/// direct release approximations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FolderReleaseBoundary {
    pub key: FolderReleaseDecisionKey,
    pub name: String,
    pub display_path: String,
    pub shared_file_count: u32,
    pub tree_rows: Vec<FolderReleaseTreeRow>,
    /// Tentative candidate paths hidden by this boundary.
    pub candidate_keys: Vec<String>,
}

/// A resolved boundary retained on a row so its context menu can set the
/// opposite interpretation without reconstructing a path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResolvedFolderReleaseBoundary {
    pub key: FolderReleaseDecisionKey,
    pub decision: FolderReleaseDecision,
    pub name: String,
    pub display_path: String,
}

/// A folder candidate detected during filesystem scanning.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FolderCandidate {
    /// Root path of this release
    pub path: PathBuf,
    /// Root passed to the import revalidation walk. It differs from `path`
    /// when audio-free single-child wrappers were collapsed into this row.
    pub file_root: PathBuf,
    /// Display name (derived from folder name)
    pub name: String,
    /// Pre-categorized files for this release
    pub files: CategorizedFiles,
    /// Absolute path of the watched folder this candidate was scanned from —
    /// the candidate-list group it belongs to. Equal to the scan root. The
    /// group's display name comes from the watched-folder list, not here.
    pub watched_folder_path: String,
    /// The exact file ownership rule used to build `files`.
    pub scope: ReleaseFileScope,
    /// Revision of the stored file decisions applied to `files`.
    pub file_edit_revision: u64,
    /// Root-relative path for the queue subtitle, with `/` separators.
    pub display_path: String,
    /// Present when an explicit decision exposed this candidate.
    pub resolved_boundaries: Vec<ResolvedFolderReleaseBoundary>,
    /// Nearest default-separate ancestor that contains multiple release rows.
    /// The UI uses this core-issued key for "Combine as One Release".
    pub combine_ancestor_key: Option<FolderReleaseDecisionKey>,
}

impl FolderCandidate {
    pub fn track_count(&self) -> u32 {
        self.files.track_count()
    }
}

// ── Candidate file index ────────────────────────────────────────────────────

/// A single file entry in a candidate's selected file set.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Path relative to the scan root
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
}

/// A pre-collected list of files for release candidate detection.
///
/// Built by walking the filesystem once.
pub struct CandidateFileIndex {
    files: Vec<FileEntry>,
    dirs: BTreeMap<PathBuf, DirEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum FolderScanError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The scan root exists but is a file, not a directory. A distinct variant
    /// so the message is stable across platforms — `read_dir` on a file reports
    /// "Not a directory" on Unix but "The directory name is invalid" on Windows,
    /// and the import service surfaces this text straight to the user.
    #[error("not a directory: {}", path.display())]
    NotADirectory { path: PathBuf },
    #[error("folder scan cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl FolderScanError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[derive(Debug, Default)]
struct DirEntry {
    files: Vec<usize>,
    subdirs: BTreeSet<PathBuf>,
}

impl CandidateFileIndex {
    pub fn new(files: Vec<FileEntry>) -> Self {
        let mut tree = Self {
            files,
            dirs: BTreeMap::new(),
        };
        tree.rebuild_index();
        tree
    }

    /// All files recursively under `dir` (inclusive).
    fn all_files_under<'a>(&'a self, dir: &Path) -> impl Iterator<Item = &'a FileEntry> {
        let mut indices = Vec::new();
        self.collect_file_indices_under(&Self::normalize_dir(dir), &mut indices);
        indices.into_iter().map(|idx| &self.files[idx])
    }

    fn rebuild_index(&mut self) {
        self.dirs.clear();
        self.dirs.entry(PathBuf::new()).or_default();

        for idx in 0..self.files.len() {
            let parent = {
                let file = &self.files[idx];
                Self::normalize_dir(file.path.parent().unwrap_or_else(|| Path::new("")))
            };
            self.dirs.entry(parent.clone()).or_default().files.push(idx);
            self.index_dir_path(&parent);
        }
    }

    fn index_dir_path(&mut self, dir: &Path) {
        let mut parent = PathBuf::new();
        self.dirs.entry(parent.clone()).or_default();

        for component in dir.components() {
            let child = parent.join(component.as_os_str());
            self.dirs
                .entry(parent.clone())
                .or_default()
                .subdirs
                .insert(child.clone());
            self.dirs.entry(child.clone()).or_default();
            parent = child;
        }
    }

    fn collect_file_indices_under(&self, dir: &Path, out: &mut Vec<usize>) {
        let Some(entry) = self.dirs.get(dir) else {
            return;
        };

        out.extend(entry.files.iter().copied());
        for subdir in &entry.subdirs {
            self.collect_file_indices_under(subdir, out);
        }
    }

    fn normalize_dir(dir: &Path) -> PathBuf {
        if dir.as_os_str().is_empty() || dir == Path::new(".") {
            PathBuf::new()
        } else {
            dir.to_path_buf()
        }
    }
}

// ── Extension-based classification (pure, no I/O) ──────────────────────────

/// Check if a file is an audio file based on extension
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ContentTypeHint::from_extension(ext).is_audio())
        .unwrap_or(false)
}

/// A track sheet's audio file, as FFmpeg probes it.
enum CueCodecLabel {
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
fn cue_pair_codec_label(path: &Path) -> Result<CueCodecLabel, FolderScanError> {
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
fn is_image_file(path: &Path) -> bool {
    ContentTypeHint::path_is_raster_image(path)
}

/// Check if a file is a document file (.cue, .log, .txt, .m3u)
fn is_document_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| DOCUMENT_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Check if a file is a CUE file
fn is_cue_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase() == "cue")
        .unwrap_or(false)
}

/// Check if a file is noise (.DS_Store, Thumbs.db, etc.)
fn is_noise_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name == ".DS_Store" || name == "Thumbs.db" || name == "desktop.ini")
        .unwrap_or(false)
}

/// True when `path`'s extension matches a known in-progress-download marker
/// (e.g. `01.flac.part`, `02.flac.crdownload`, `03.aria2`). Match is case-insensitive.
fn is_partial_marker_file(path: &Path) -> bool {
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
enum CategorizeOutcome {
    Valid(CategorizedFiles),
    Invalid(InvalidReason),
}

/// What the extension says a file is, before the CUE parse and the
/// `FILE`-directive resolution settle the roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProposedRole {
    Audio,
    Cue,
    Image,
    Document,
    Other,
}

/// Filename stems that conventionally name a release's front cover. The scan
/// proposes the first image matching one of these as the cover; every other
/// image is artwork.
const COVER_STEMS: &[&str] = &[
    "cover",
    "front",
    "folder",
    "frontcover",
    "front cover",
    "albumart",
    "album art",
];

/// Whether an image's filename stem conventionally names a front cover.
fn is_cover_name(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let stem = stem.trim().to_lowercase();
    COVER_STEMS.iter().any(|candidate| *candidate == stem)
}

/// Shorthand for a failed-validation leaf carrying `reason`.
fn invalid(reason: InvalidReason) -> Result<CategorizeOutcome, FolderScanError> {
    Ok(CategorizeOutcome::Invalid(reason))
}

/// What settling a folder's sheet bindings produced.
enum SettledBindings {
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
fn settle_file_roles(files: &mut [CandidateFile], edits: &FileRoleEdits) {
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
fn settle_sheet_bindings(
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
fn settle_sheet_discs(files: &mut [CandidateFile], edits: &SheetDiscEdits) {
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
fn derive_format_label(files: &[CandidateFile], cue_codec: Option<String>) -> Option<String> {
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
fn cue_parent_dir(cue_path: &Path) -> Result<&Path, FolderScanError> {
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
fn categorize_files_from_tree(
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
    // with. A miss is a question for the user, not a verdict on the folder.
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
            None => {
                info!(
                    "CUE {:?} names audio that is not here; it stays unbound",
                    cue_file.path
                );
                SheetBinding::Unresolved
            }
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

// ── Progressive directory walker ───────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct DirectoryListing {
    files: Vec<FileEntry>,
    directories: Vec<PathBuf>,
}

pub(crate) trait DirectoryReader {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &ScanCancellation,
    ) -> Result<DirectoryListing, FolderScanError>;
}

struct OsDirectoryReader;

impl DirectoryReader for OsDirectoryReader {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &ScanCancellation,
    ) -> Result<DirectoryListing, FolderScanError> {
        let absolute = root.join(directory);
        let entries =
            fs::read_dir(&absolute).map_err(|source| FolderScanError::io(&absolute, source))?;
        let mut files = Vec::new();
        let mut directories = Vec::new();
        for entry in entries {
            cancellation.check()?;
            let entry = entry.map_err(|source| FolderScanError::io(&absolute, source))?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                return Err(FolderScanError::Other(format!(
                    "directory entry is not UTF-8: {}",
                    path.display()
                )));
            };
            if name.starts_with('.') {
                debug!("ignoring hidden folder-scan entry {}", path.display());
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|error| FolderScanError::Other(error.to_string()))?
                .to_path_buf();
            let file_type = entry
                .file_type()
                .map_err(|source| FolderScanError::io(&path, source))?;
            if file_type.is_dir() {
                directories.push(relative);
                continue;
            }
            let metadata = entry
                .metadata()
                .map_err(|source| FolderScanError::io(&path, source))?;
            if metadata.is_dir() && !file_type.is_symlink() {
                directories.push(relative);
            } else if metadata.is_file() && !is_noise_file(&path) {
                files.push(FileEntry {
                    path: relative,
                    size: metadata.len(),
                });
            }
        }
        let compare = |left: &PathBuf, right: &PathBuf| {
            natord::compare(
                &left
                    .file_name()
                    .expect("a directory entry path has a file name")
                    .to_string_lossy(),
                &right
                    .file_name()
                    .expect("a directory entry path has a file name")
                    .to_string_lossy(),
            )
        };
        files.sort_by(|left, right| compare(&left.path, &right.path));
        directories.sort_by(compare);
        Ok(DirectoryListing { files, directories })
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ScanCancellation(Arc<AtomicBool>);

impl ScanCancellation {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Only the import service cancels a scan in flight, and it is desktop-only.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn check(&self) -> Result<(), FolderScanError> {
        if self.is_cancelled() {
            Err(FolderScanError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
enum ProjectedScanNode {
    Candidate(FolderCandidate),
    Invalid(InvalidCandidate),
    Boundary(FolderReleaseBoundary),
}

#[derive(Debug)]
struct ScannedDirectory {
    all_files: Vec<FileEntry>,
    contains_audio: bool,
    nodes: Vec<ProjectedScanNode>,
    nodes_emitted: bool,
}

fn relative_path_string(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn directory_name(root: &Path, relative: &Path) -> String {
    let path = if relative.as_os_str().is_empty() {
        root
    } else {
        relative
    };
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn categorize_selected_files(
    files: Vec<FileEntry>,
    relative: &Path,
    root: &Path,
    stored: &StoredCandidateEdits,
    cancellation: &ScanCancellation,
) -> Result<CategorizeOutcome, FolderScanError> {
    let tree = CandidateFileIndex::new(files);
    categorize_files_from_tree(&tree, relative, root, stored, cancellation)
}

fn candidate_from_files(
    files: Vec<FileEntry>,
    relative: &Path,
    candidate_relative: &Path,
    root: &Path,
    watched_folder_path: &str,
    scope: ReleaseFileScope,
    resolved_boundaries: Vec<ResolvedFolderReleaseBoundary>,
    stored: &StoredCandidateEdits,
    cancellation: &ScanCancellation,
) -> Result<Option<ProjectedScanNode>, FolderScanError> {
    if files.iter().any(|file| is_partial_marker_file(&file.path)) {
        info!(
            "Skipping release approximation {:?}: partial-download marker present",
            relative
        );
        return Ok(None);
    }
    let path = if candidate_relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(candidate_relative)
    };
    let file_root = if relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    };
    let name = directory_name(root, candidate_relative);
    let display_path = relative_path_string(candidate_relative);
    match categorize_selected_files(files, relative, root, stored, cancellation)? {
        CategorizeOutcome::Valid(files) => {
            let file_edit_revision = stored.revision_for_hash(&files.content_hash());
            Ok(Some(ProjectedScanNode::Candidate(FolderCandidate {
                path,
                file_root,
                name,
                files,
                watched_folder_path: watched_folder_path.to_string(),
                scope,
                file_edit_revision,
                display_path,
                resolved_boundaries,
                combine_ancestor_key: None,
            })))
        }
        CategorizeOutcome::Invalid(reason) => {
            Ok(Some(ProjectedScanNode::Invalid(InvalidCandidate {
                path,
                name,
                watched_folder_path: watched_folder_path.to_string(),
                display_path,
                resolved_boundaries,
                reason,
            })))
        }
    }
}

fn candidate_keys(nodes: &[ProjectedScanNode]) -> Vec<String> {
    let mut keys = Vec::new();
    for node in nodes {
        match node {
            ProjectedScanNode::Candidate(candidate) => {
                keys.push(candidate.path.to_string_lossy().into_owned())
            }
            ProjectedScanNode::Invalid(candidate) => {
                keys.push(candidate.path.to_string_lossy().into_owned())
            }
            ProjectedScanNode::Boundary(boundary) => {
                keys.extend(boundary.candidate_keys.iter().cloned())
            }
        }
    }
    keys
}

fn boundary_tree_rows(
    root: &Path,
    boundary: &Path,
    watched_folder_path: &str,
    nodes: &[ProjectedScanNode],
) -> Vec<FolderReleaseTreeRow> {
    let boundary_relative = boundary
        .strip_prefix(root)
        .expect("a release boundary is below its watched root");
    let mut candidate_summaries = HashMap::new();
    let mut invalid_reasons = HashMap::new();
    for node in nodes {
        match node {
            ProjectedScanNode::Candidate(candidate) => {
                candidate_summaries.insert(
                    candidate.path.clone(),
                    FolderReleaseCandidateSummary {
                        track_count: candidate.track_count(),
                        format_label: candidate.files.format_label.clone(),
                    },
                );
            }
            ProjectedScanNode::Invalid(candidate) => {
                invalid_reasons.insert(candidate.path.clone(), candidate.reason.clone());
            }
            ProjectedScanNode::Boundary(nested) => {
                let nested_root = PathBuf::from(&nested.key.watched_folder_path)
                    .join(&nested.key.relative_folder_path);
                for row in &nested.tree_rows {
                    let absolute = nested_root.join(&row.display_path);
                    match &row.kind {
                        FolderReleaseTreeRowKind::Candidate { summary } => {
                            candidate_summaries.insert(absolute, summary.clone());
                        }
                        FolderReleaseTreeRowKind::Invalid { reason } => {
                            invalid_reasons.insert(absolute, reason.clone());
                        }
                        FolderReleaseTreeRowKind::Folder => {}
                    }
                }
            }
        }
    }
    let mut releases = BTreeSet::new();
    for key in candidate_keys(nodes) {
        releases.insert(PathBuf::from(key));
    }
    let mut descendant_counts: BTreeMap<PathBuf, u32> = BTreeMap::new();
    for absolute in &releases {
        let relative = absolute
            .strip_prefix(boundary)
            .expect("a boundary candidate is below its release boundary");
        let components: Vec<_> = relative.components().collect();
        for end in 0..components.len() {
            let path: PathBuf = components[..=end]
                .iter()
                .map(|component| component.as_os_str())
                .collect();
            *descendant_counts.entry(path).or_default() += 1;
        }
    }
    let mut rows: BTreeMap<String, FolderReleaseTreeRow> = BTreeMap::new();
    let boundary_kind = candidate_summaries
        .get(boundary)
        .cloned()
        .map(|summary| FolderReleaseTreeRowKind::Candidate { summary })
        .or_else(|| {
            invalid_reasons
                .get(boundary)
                .cloned()
                .map(|reason| FolderReleaseTreeRowKind::Invalid { reason })
        });
    if let Some(kind) = boundary_kind {
        let decision_key = FolderReleaseDecisionKey {
            watched_folder_path: watched_folder_path.to_string(),
            relative_folder_path: relative_path_string(boundary_relative),
        };
        rows.insert(
            String::new(),
            FolderReleaseTreeRow {
                name: directory_name(root, boundary),
                display_path: String::new(),
                depth: 0,
                kind,
                decision_key,
                ancestor_decision_keys: Vec::new(),
            },
        );
    }
    let descendant_depth_offset = u32::from(rows.contains_key(""));
    for absolute in releases {
        let relative = absolute
            .strip_prefix(boundary)
            .expect("a boundary candidate is below its release boundary");
        let components: Vec<_> = relative.components().collect();
        for end in 0..components.len() {
            let path: PathBuf = components[..=end]
                .iter()
                .map(|component| component.as_os_str())
                .collect();
            let display_path = relative_path_string(&path);
            let decision_key = FolderReleaseDecisionKey {
                watched_folder_path: watched_folder_path.to_string(),
                relative_folder_path: relative_path_string(&boundary_relative.join(&path)),
            };
            let mut ancestor_decision_keys = vec![FolderReleaseDecisionKey {
                watched_folder_path: watched_folder_path.to_string(),
                relative_folder_path: relative_path_string(boundary_relative),
            }];
            for prefix_end in 0..end {
                let prefix: PathBuf = components[..=prefix_end]
                    .iter()
                    .map(|component| component.as_os_str())
                    .collect();
                if descendant_counts
                    .get(&prefix)
                    .is_some_and(|count| *count > 1)
                {
                    ancestor_decision_keys.push(FolderReleaseDecisionKey {
                        watched_folder_path: watched_folder_path.to_string(),
                        relative_folder_path: relative_path_string(&boundary_relative.join(prefix)),
                    });
                }
            }
            let is_release = end + 1 == components.len();
            let kind = if is_release {
                if let Some(summary) = candidate_summaries.get(&absolute) {
                    FolderReleaseTreeRowKind::Candidate {
                        summary: summary.clone(),
                    }
                } else if let Some(reason) = invalid_reasons.get(&absolute) {
                    FolderReleaseTreeRowKind::Invalid {
                        reason: reason.clone(),
                    }
                } else {
                    FolderReleaseTreeRowKind::Folder
                }
            } else {
                FolderReleaseTreeRowKind::Folder
            };
            let row = FolderReleaseTreeRow {
                name: components[end].as_os_str().to_string_lossy().into_owned(),
                display_path: display_path.clone(),
                depth: end as u32 + descendant_depth_offset,
                kind,
                decision_key,
                ancestor_decision_keys,
            };
            rows.entry(display_path)
                .and_modify(|existing| {
                    if is_release {
                        existing.kind = row.kind.clone();
                    }
                })
                .or_insert(row);
        }
    }
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by(|left, right| natord::compare(&left.display_path, &right.display_path));
    rows
}

fn apply_resolved_boundary(
    nodes: &mut [ProjectedScanNode],
    resolved: &ResolvedFolderReleaseBoundary,
) {
    for node in nodes {
        let resolved_boundaries = match node {
            ProjectedScanNode::Candidate(candidate) => &mut candidate.resolved_boundaries,
            ProjectedScanNode::Invalid(candidate) => &mut candidate.resolved_boundaries,
            ProjectedScanNode::Boundary(_) => continue,
        };
        if !resolved_boundaries
            .iter()
            .any(|existing| existing.key == resolved.key)
        {
            resolved_boundaries.push(resolved.clone());
        }
    }
}

fn scan_directory<R, F, D>(
    reader: &R,
    root: &Path,
    relative: &Path,
    watched_folder_path: &str,
    allow_unresolved_boundary: bool,
    ancestors_allow_actionable: bool,
    decisions: &FolderReleaseDecisions,
    stored: &StoredCandidateEdits,
    cancellation: &ScanCancellation,
    on_directory: &mut D,
    on_item: &mut F,
) -> Result<ScannedDirectory, FolderScanError>
where
    R: DirectoryReader,
    F: FnMut(ScanItem),
    D: FnMut(PathBuf),
{
    cancellation.check()?;
    on_directory(root.join(relative));
    let listing = reader.read(root, relative, cancellation)?;
    let direct_audio = listing
        .files
        .iter()
        .any(|file| file.size > 0 && is_audio_file(&file.path));
    let has_direct_files = !listing.files.is_empty();
    let wrapper_has_files = !direct_audio && !listing.files.is_empty();
    let mut all_files = listing.files.clone();
    let mut direct_scope_files = listing.files;
    let mut child_nodes = Vec::new();
    let mut child_nodes_emitted = false;
    let mut contains_audio = direct_audio;
    let relative_string = relative_path_string(relative);
    let decision = decisions.get(&relative_string);
    let combine = matches!(decision, Some(FolderReleaseDecision::CombineAsOneRelease));
    let keep_separate = matches!(
        decision,
        Some(FolderReleaseDecision::KeepAsSeparateReleases)
    );
    let can_stream_collection = ancestors_allow_actionable
        && !combine
        && (!allow_unresolved_boundary || !has_direct_files || keep_separate);
    let mut collection_proven = !wrapper_has_files;
    let resolved_separate = keep_separate.then(|| ResolvedFolderReleaseBoundary {
        key: FolderReleaseDecisionKey {
            watched_folder_path: watched_folder_path.to_string(),
            relative_folder_path: relative_string.clone(),
        },
        decision: FolderReleaseDecision::KeepAsSeparateReleases,
        name: directory_name(root, relative),
        display_path: relative_string.clone(),
    });

    for child in listing.directories {
        let child_can_be_actionable = can_stream_collection && collection_proven;
        let child_scan = scan_directory(
            reader,
            root,
            &child,
            watched_folder_path,
            true,
            child_can_be_actionable,
            decisions,
            stored,
            cancellation,
            on_directory,
            on_item,
        )?;
        contains_audio |= child_scan.contains_audio;
        if !child_scan.contains_audio {
            direct_scope_files.extend(child_scan.all_files.iter().cloned());
        }
        all_files.extend(child_scan.all_files);
        if !wrapper_has_files && can_stream_collection {
            let mut nodes = child_scan.nodes;
            if let Some(resolved) = &resolved_separate {
                apply_resolved_boundary(&mut nodes, resolved);
            }
            if !child_scan.nodes_emitted {
                emit_projected_nodes(nodes.clone(), on_item);
            }
            child_nodes_emitted |= child_scan.nodes_emitted || !nodes.is_empty();
            child_nodes.extend(nodes);
        } else {
            let child_start = child_nodes.len();
            let child_was_emitted = child_scan.nodes_emitted;
            child_nodes.extend(child_scan.nodes);
            if wrapper_has_files && !collection_proven && child_nodes.len() > 1 {
                collection_proven = true;
                if can_stream_collection {
                    let mut discovered_collection = child_nodes.clone();
                    if let Some(resolved) = &resolved_separate {
                        apply_resolved_boundary(&mut discovered_collection, resolved);
                    }
                    emit_projected_nodes(discovered_collection, on_item);
                    child_nodes_emitted = true;
                }
            } else if wrapper_has_files && collection_proven && can_stream_collection {
                if !child_was_emitted {
                    let mut discovered_child = child_nodes[child_start..].to_vec();
                    if let Some(resolved) = &resolved_separate {
                        apply_resolved_boundary(&mut discovered_child, resolved);
                    }
                    emit_projected_nodes(discovered_child, on_item);
                }
                child_nodes_emitted = true;
            }
        }
    }
    let shared_file_count = if direct_audio {
        0
    } else {
        direct_scope_files.len() as u32
    };
    let owns_wrapper_files = !direct_audio && !direct_scope_files.is_empty();

    if combine && contains_audio {
        let resolved = ResolvedFolderReleaseBoundary {
            key: FolderReleaseDecisionKey {
                watched_folder_path: watched_folder_path.to_string(),
                relative_folder_path: relative_string.clone(),
            },
            decision: FolderReleaseDecision::CombineAsOneRelease,
            name: directory_name(root, relative),
            display_path: relative_string.clone(),
        };
        let node = candidate_from_files(
            all_files.clone(),
            relative,
            relative,
            root,
            watched_folder_path,
            ReleaseFileScope::Recursive,
            vec![resolved],
            stored,
            cancellation,
        )?;
        let nodes = node.into_iter().collect();
        return Ok(ScannedDirectory {
            all_files,
            contains_audio,
            nodes,
            nodes_emitted: false,
        });
    }

    let mut nodes = Vec::new();
    if direct_audio {
        if let Some(node) = candidate_from_files(
            direct_scope_files,
            relative,
            relative,
            root,
            watched_folder_path,
            ReleaseFileScope::Direct,
            Vec::new(),
            stored,
            cancellation,
        )? {
            if let ProjectedScanNode::Candidate(candidate) = &node {
                on_item(ScanItem::Discovered(candidate.clone()));
            }
            nodes.push(node);
        }
    }
    nodes.extend(child_nodes);

    // A collapsed wrapper's files still have one owner when there is exactly
    // one release below it. Keep the leaf as the candidate key/display row,
    // but root its reproducible file scope at the wrapper so sidecars and
    // audio-free siblings survive scan, import, and re-scan.
    if owns_wrapper_files && nodes.len() == 1 {
        if let ProjectedScanNode::Candidate(existing) = &nodes[0] {
            let candidate_relative = existing
                .path
                .strip_prefix(root)
                .map_err(|error| FolderScanError::Other(error.to_string()))?
                .to_path_buf();
            let resolved_boundaries = existing.resolved_boundaries.clone();
            if let Some(candidate) = candidate_from_files(
                all_files.clone(),
                relative,
                &candidate_relative,
                root,
                watched_folder_path,
                ReleaseFileScope::Recursive,
                resolved_boundaries,
                stored,
                cancellation,
            )? {
                nodes = vec![candidate];
            }
        }
    }

    if keep_separate {
        apply_resolved_boundary(
            &mut nodes,
            resolved_separate
                .as_ref()
                .expect("keep-separate decision constructs its boundary"),
        );
    } else if allow_unresolved_boundary && nodes.len() > 1 && (direct_audio || owns_wrapper_files) {
        let absolute = root.join(relative);
        let candidate_keys = candidate_keys(&nodes);
        let key = FolderReleaseDecisionKey {
            watched_folder_path: watched_folder_path.to_string(),
            relative_folder_path: relative_string.clone(),
        };
        let tree_rows = boundary_tree_rows(root, &absolute, watched_folder_path, &nodes);
        let boundary = FolderReleaseBoundary {
            key,
            name: directory_name(root, relative),
            display_path: relative_string,
            shared_file_count,
            tree_rows,
            candidate_keys,
        };
        if child_nodes_emitted {
            on_item(ScanItem::Boundary(boundary.clone()));
        }
        nodes = vec![ProjectedScanNode::Boundary(boundary)];
    } else if allow_unresolved_boundary && nodes.len() > 1 {
        let key = FolderReleaseDecisionKey {
            watched_folder_path: watched_folder_path.to_string(),
            relative_folder_path: relative_string,
        };
        for node in &mut nodes {
            if let ProjectedScanNode::Candidate(candidate) = node {
                candidate.combine_ancestor_key.get_or_insert(key.clone());
            }
        }
        if child_nodes_emitted {
            for node in &nodes {
                if let ProjectedScanNode::Candidate(candidate) = node {
                    on_item(ScanItem::Valid(candidate.clone()));
                }
            }
        }
    }

    Ok(ScannedDirectory {
        all_files,
        contains_audio,
        nodes,
        nodes_emitted: child_nodes_emitted,
    })
}

fn emit_projected_nodes<F>(nodes: Vec<ProjectedScanNode>, on_item: &mut F)
where
    F: FnMut(ScanItem),
{
    for node in nodes {
        match node {
            ProjectedScanNode::Candidate(candidate) => on_item(ScanItem::Valid(candidate)),
            ProjectedScanNode::Invalid(candidate) => on_item(ScanItem::Invalid(candidate)),
            ProjectedScanNode::Boundary(boundary) => on_item(ScanItem::Boundary(boundary)),
        }
    }
}

pub(crate) fn scan_for_candidates_with_reader_cancellable<R, F>(
    reader: &R,
    root: PathBuf,
    stored: &StoredCandidateEdits,
    decisions: &FolderReleaseDecisions,
    cancellation: &ScanCancellation,
    on_item: F,
) -> Result<(), FolderScanError>
where
    R: DirectoryReader,
    F: FnMut(ScanItem),
{
    scan_for_candidates_with_reader_cancellable_and_directories(
        reader,
        root,
        stored,
        decisions,
        cancellation,
        |_| {},
        on_item,
    )
}

pub(crate) fn scan_for_candidates_with_reader_cancellable_and_directories<R, F, D>(
    reader: &R,
    root: PathBuf,
    stored: &StoredCandidateEdits,
    decisions: &FolderReleaseDecisions,
    cancellation: &ScanCancellation,
    mut on_directory: D,
    mut on_item: F,
) -> Result<(), FolderScanError>
where
    R: DirectoryReader,
    F: FnMut(ScanItem),
    D: FnMut(PathBuf),
{
    cancellation.check()?;
    debug!("Scanning for candidates in: {:?}", root);
    if let Ok(metadata) = fs::metadata(&root) {
        if !metadata.is_dir() {
            return Err(FolderScanError::NotADirectory { path: root });
        }
    }
    let watched_folder_path = root.to_string_lossy().into_owned();
    on_directory(root.clone());
    let root_listing = reader.read(&root, Path::new(""), cancellation)?;
    let direct_audio = root_listing
        .files
        .iter()
        .any(|file| file.size > 0 && is_audio_file(&file.path));
    let mut direct_scope_files = root_listing.files;

    for child in root_listing.directories {
        let child_scan = scan_directory(
            reader,
            &root,
            &child,
            &watched_folder_path,
            true,
            true,
            decisions,
            stored,
            cancellation,
            &mut on_directory,
            &mut on_item,
        )?;
        if !child_scan.contains_audio {
            direct_scope_files.extend(child_scan.all_files.iter().cloned());
        }
        if !child_scan.nodes_emitted {
            emit_projected_nodes(child_scan.nodes, &mut on_item);
        }
    }

    if direct_audio {
        if let Some(node) = candidate_from_files(
            direct_scope_files,
            Path::new(""),
            Path::new(""),
            &root,
            &watched_folder_path,
            ReleaseFileScope::Direct,
            Vec::new(),
            stored,
            cancellation,
        )? {
            emit_projected_nodes(vec![node], &mut on_item);
        }
    }
    Ok(())
}

pub(crate) fn scan_for_candidates_with_reader<R, F>(
    reader: &R,
    root: PathBuf,
    stored: &StoredCandidateEdits,
    decisions: &FolderReleaseDecisions,
    on_item: F,
) -> Result<(), FolderScanError>
where
    R: DirectoryReader,
    F: FnMut(ScanItem),
{
    scan_for_candidates_with_reader_cancellable(
        reader,
        root,
        stored,
        decisions,
        &ScanCancellation::new(),
        on_item,
    )
}

/// Scan one watched root a directory at a time. Completed release
/// approximations and unresolved boundaries are emitted before unrelated
/// sibling directories are read.
pub fn scan_for_candidates_with_callback<F>(
    root: PathBuf,
    stored: &StoredCandidateEdits,
    mut on_item: F,
) -> Result<(), FolderScanError>
where
    F: FnMut(ScanItem),
{
    scan_for_candidates_with_reader(
        &OsDirectoryReader,
        root,
        stored,
        &FolderReleaseDecisions::default(),
        |item| {
            if !matches!(item, ScanItem::Discovered(_)) {
                on_item(item);
            }
        },
    )
}

pub fn scan_for_candidates_with_decisions<F>(
    root: PathBuf,
    stored: &StoredCandidateEdits,
    decisions: &FolderReleaseDecisions,
    on_item: F,
) -> Result<(), FolderScanError>
where
    F: FnMut(ScanItem),
{
    scan_for_candidates_with_reader(&OsDirectoryReader, root, stored, decisions, on_item)
}

/// The progressive, cancellable scan the desktop import service drives.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn scan_for_candidates_with_decisions_cancellable_and_directories<F, D>(
    root: PathBuf,
    stored: &StoredCandidateEdits,
    decisions: &FolderReleaseDecisions,
    cancellation: &ScanCancellation,
    on_directory: D,
    on_item: F,
) -> Result<(), FolderScanError>
where
    F: FnMut(ScanItem),
    D: FnMut(PathBuf),
{
    scan_for_candidates_with_reader_cancellable_and_directories(
        &OsDirectoryReader,
        root,
        stored,
        decisions,
        cancellation,
        on_directory,
        on_item,
    )
}

fn read_file_subtree<R: DirectoryReader>(
    reader: &R,
    root: &Path,
    relative: &Path,
    cancellation: &ScanCancellation,
) -> Result<(Vec<FileEntry>, bool), FolderScanError> {
    let listing = reader.read(root, relative, cancellation)?;
    let mut contains_audio = listing
        .files
        .iter()
        .any(|file| file.size > 0 && is_audio_file(&file.path));
    let mut files = listing.files;
    for child in listing.directories {
        let (child_files, child_contains_audio) =
            read_file_subtree(reader, root, &child, cancellation)?;
        files.extend(child_files);
        contains_audio |= child_contains_audio;
    }
    Ok((files, contains_audio))
}

fn collect_scoped_entries(
    root: &Path,
    scope: ReleaseFileScope,
) -> Result<Vec<FileEntry>, FolderScanError> {
    let reader = OsDirectoryReader;
    let cancellation = ScanCancellation::new();
    match scope {
        ReleaseFileScope::Recursive => {
            read_file_subtree(&reader, root, Path::new(""), &cancellation).map(|(files, _)| files)
        }
        ReleaseFileScope::Direct => {
            let listing = reader.read(root, Path::new(""), &cancellation)?;
            let mut files = listing.files;
            for child in listing.directories {
                let (child_files, contains_audio) =
                    read_file_subtree(&reader, root, &child, &cancellation)?;
                if !contains_audio {
                    files.extend(child_files);
                }
            }
            Ok(files)
        }
    }
}

/// Collect one explicit release boundary and give every owned file its role,
/// preserving relative paths, with stored file decisions applied.
///
/// Every caller that re-derives a folder — the commit, the Unknown-import seed,
/// the signal fast pass — goes through here, so none of them can see a shape
/// the user has already corrected.
pub fn collect_release_candidate_files_with_scope(
    release_root: &Path,
    scope: ReleaseFileScope,
    stored: &StoredCandidateEdits,
) -> Result<CategorizedFiles, crate::import::ImportError> {
    let tree = CandidateFileIndex::new(collect_scoped_entries(release_root, scope)?);
    // An invalid folder can't be imported: surface its typed reason so the
    // import-commit caller fails with why the folder is unusable.
    match categorize_files_from_tree(
        &tree,
        &PathBuf::new(),
        release_root,
        stored,
        &ScanCancellation::new(),
    )? {
        CategorizeOutcome::Valid(files) => Ok(files),
        CategorizeOutcome::Invalid(reason) => Err(reason.into()),
    }
}

#[cfg(test)]
mod tests;
