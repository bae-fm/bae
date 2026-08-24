use super::*;

// ── Public types ────────────────────────────────────────────────────────────

/// A file discovered during folder scanning
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
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
    /// An image the folder carries. Which one leads the release is not a
    /// property of the file: it is the cover choice, answered by the stored
    /// row, then the picked release's own art, then these ranked by name.
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

    /// Every decision, in sheet-id order — what the store writes as rows.
    pub fn iter(&self) -> impl Iterator<Item = (&str, SheetDisc)> {
        self.0.iter().map(|(id, disc)| (id.as_str(), *disc))
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

    /// Every decision, in file-id order — what the store writes as rows.
    pub fn iter(&self) -> impl Iterator<Item = (&str, FileRoleChoice)> {
        self.0.iter().map(|(id, choice)| (id.as_str(), *choice))
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

    /// Every decision, in sheet-id order — what the store writes as rows.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &UserSheetBinding)> {
        self.0.iter().map(|(id, binding)| (id.as_str(), binding))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Everything the user has settled about one candidate's files: which audio
/// each track sheet describes, which disc each sheet's entries become, and
/// which files are the release's tracks.
///
/// One value because they settle together — every part is keyed by the same
/// content hash and read by the same scan, and asking for them separately
/// would be several things to keep in step. On disk they are one
/// `import_candidate_file_edit` row per file, its three columns holding
/// whichever of the three that file has a decision about.
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

    pub(super) fn for_hash(&self, content_hash: &str) -> Option<&CandidateFileEdits> {
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
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
/// exists to show one row at a time, and images live in one gallery however
/// many directories they sit in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRowKind {
    Document,
    Other,
}

/// A directory whose files all do the same job, which the roles table shows as
/// one row — `logs/ — 14 documents` — instead of one row each. Nothing in it
/// needs a decision, so listing every file buys nothing and costs the table its
/// readability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapsedDirectory {
    /// The prefix its files carry in [`ScannedFile::dir_prefix`], e.g.
    /// `logs/` — which is also how a renderer tells which files it stands
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
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
            .filter(|entry| matches!(entry.role, FileRole::Artwork))
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
                FileRole::Audio | FileRole::TrackSheet { .. } | FileRole::Artwork => None,
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
pub(super) fn content_hash_of<'a>(files: impl Iterator<Item = &'a ScannedFile>) -> String {
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
