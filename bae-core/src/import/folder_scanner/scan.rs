use super::*;
use std::collections::HashSet;

mod root;
mod scanned_decision;
pub(crate) use root::scan_for_candidates_with_reader_cancellable_and_directories;
use scanned_decision::scanned_decision;

/// How many folders directly under a watched root a pass reads at once.
///
/// A scan of a folder on a network share is round trips — a listing of each
/// directory, a look at each entry in it — and one folder at a time waits out
/// each of them in turn; with several folders in flight their waits overlap.
/// Eight took a rescan of a library of 729 folders on an SMB share, which
/// opens no file, from 65 s to under 10 s, and its first scan from 243 s to
/// 49 s. Bounded, so a pass holds a few folders' readings at once against one
/// volume rather than one per folder.
pub(super) const FOLDERS_AT_ONCE: usize = 8;

// ── Progressive directory walker ───────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct DirectoryListing {
    files: Vec<FileEntry>,
    directories: Vec<PathBuf>,
}

pub(crate) trait DirectoryReader: Send + Sync {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &ScanCancellation,
    ) -> Result<DirectoryListing, FolderScanError>;

    /// The dates of each of `folders`, directly under `root` and relative to
    /// it, in their order: what a pass walks them newest first by. A folder
    /// whose dates can't be read has none; only a cancelled pass fails this.
    fn folder_times(
        &self,
        root: &Path,
        folders: &[PathBuf],
        cancellation: &ScanCancellation,
    ) -> Result<Vec<FolderTimes>, FolderScanError>;
}

pub(crate) struct OsDirectoryReader;

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
                    modified_at_ns: file_modified_at_ns(&path, &metadata)?,
                });
            }
        }
        let compare = |left: &PathBuf, right: &PathBuf| {
            natord::compare_ignore_case(
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

    fn folder_times(
        &self,
        root: &Path,
        folders: &[PathBuf],
        cancellation: &ScanCancellation,
    ) -> Result<Vec<FolderTimes>, FolderScanError> {
        super::folder_times::times_of_folders(root, folders, cancellation)
    }
}

pub(crate) fn file_modified_at_ns(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<i64, FolderScanError> {
    let modified = metadata
        .modified()
        .map_err(|source| FolderScanError::io(path, source))?;
    let elapsed = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            FolderScanError::Other(format!(
                "modification time is before the Unix epoch: {}: {error}",
                path.display()
            ))
        })?;
    i64::try_from(elapsed.as_nanos()).map_err(|error| {
        FolderScanError::Other(format!(
            "modification time exceeds SQLite's integer range: {}: {error}",
            path.display()
        ))
    })
}

/// What stops a scan pass: the import service cancelling it, or, for the
/// folders a pass reads at once, one of them failing.
#[derive(Debug, Clone, Default)]
pub(crate) struct ScanCancellation(tokio_util::sync::CancellationToken);

impl ScanCancellation {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A cancellation that cancelling this one cancels too, and that cancels
    /// on its own without cancelling this one.
    pub(super) fn child(&self) -> Self {
        Self(self.0.child_token())
    }

    pub(crate) fn cancel(&self) {
        self.0.cancel();
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    pub(super) fn check(&self) -> Result<(), FolderScanError> {
        if self.is_cancelled() {
            Err(FolderScanError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum ProjectedScanNode {
    Candidate(FolderCandidate),
    Invalid(InvalidCandidate),
}

#[derive(Debug)]
pub(super) struct ScannedDirectory {
    all_files: Vec<FileEntry>,
    contains_audio: bool,
    /// Every folder in this subtree with audio of its own, root-relative, in
    /// the order the walk reached them: the parts a reading of the subtree as
    /// one release is made of.
    audio_folders: Vec<PathBuf>,
    nodes: Vec<ProjectedScanNode>,
    nodes_emitted: bool,
    /// Every folder in this subtree whose own files no release owns, waiting
    /// on the folders above: a wrapper that lends its one release its whole
    /// subtree takes them after all.
    loose: Vec<LooseFolder>,
}

/// A folder whose own files — directly in it, and in the folders below it
/// that hold no audio — no release read there owns: its sidecar, before it
/// is read.
#[derive(Debug)]
struct LooseFolder {
    /// Root-relative.
    folder: PathBuf,
    files: Vec<FileEntry>,
}

/// The folder one scan pass reads, and what it reads it against: the watched
/// root every relative path is under, the folder key its candidates are stamped
/// with, the file corrections the user has stored, and the flag that stops the
/// pass.
#[derive(Clone, Copy)]
pub(super) struct ScanRoot<'a> {
    root: &'a Path,
    watched_folder_path: &'a str,
    stored: &'a StoredCandidateEdits,
    cancellation: &'a ScanCancellation,
    /// What is known of each file, so the pass reads none it holds a reading
    /// of.
    reads: &'a FileReads,
}

/// What a pass reads a root's folders against: the file corrections the user
/// has stored, how each folder reads, and where the key of a grouping the
/// pass proposes itself comes from.
pub(crate) struct ScanReadings<'a> {
    pub(crate) stored: &'a StoredCandidateEdits,
    pub(crate) decisions: &'a FolderReleaseDecisions,
    pub(crate) new_grouping_key: &'a (dyn Fn() -> String + Sync),
}

/// A key for a grouping nobody has stored yet, for a pass with no store to
/// give it one.
pub fn fresh_grouping_key() -> String {
    format!("grouping:{}", uuid::Uuid::new_v4())
}

/// One walk over a [`ScanRoot`]: how directories are read, the readings each
/// folder under it is read under, and where the key of a grouping the walk
/// proposes itself comes from.
pub(super) struct Walk<'a, R: ?Sized> {
    scan: ScanRoot<'a>,
    reader: &'a R,
    decisions: &'a FolderReleaseDecisions,
    new_grouping_key: &'a (dyn Fn() -> String + Sync),
    /// Listings a folder's reading looked at before the walk reached them,
    /// by root-relative path, each held until the walk takes it, so no folder
    /// is listed twice in one walk.
    read_ahead: std::cell::RefCell<HashMap<PathBuf, DirectoryListing>>,
}

impl<'a, R: DirectoryReader + ?Sized> Walk<'a, R> {
    fn new(
        scan: ScanRoot<'a>,
        reader: &'a R,
        decisions: &'a FolderReleaseDecisions,
        new_grouping_key: &'a (dyn Fn() -> String + Sync),
    ) -> Self {
        Self {
            scan,
            reader,
            decisions,
            new_grouping_key,
            read_ahead: Default::default(),
        }
    }

    /// `relative`'s listing: the one a reading already looked at, or a read
    /// of the folder now.
    fn listing(&self, relative: &Path) -> Result<DirectoryListing, FolderScanError> {
        match self.read_ahead.borrow_mut().remove(relative) {
            Some(listing) => Ok(listing),
            None => self
                .reader
                .read(self.scan.root, relative, self.scan.cancellation),
        }
    }

    /// Hold `listing` for the walk, which reaches `relative` later.
    fn hold(&self, relative: PathBuf, listing: DirectoryListing) {
        self.read_ahead.borrow_mut().insert(relative, listing);
    }
}

pub(crate) fn relative_path_string(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn directory_name(root: &Path, relative: &Path) -> String {
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

pub(super) fn categorize_selected_files(
    scan: &ScanRoot<'_>,
    files: Vec<FileEntry>,
    relative: &Path,
    parts: &[ReleasePart],
) -> Result<CategorizeOutcome, FolderScanError> {
    let tree = CandidateFileIndex::new(files);
    categorize_files_from_tree(
        &tree,
        relative,
        scan.root,
        scan.stored,
        parts,
        scan.cancellation,
        scan.reads,
    )
}

/// `audio_folders`, the folders with audio below `relative` in the order the
/// walk reached them, in the order the discs of a grouping rooted there play:
/// by the part number of the folder directly under `relative` each sits in,
/// when every one of them carries one, so `CD 2` plays before `CD 10` and
/// `1936 - CD 1` before `1935 - CD 2`. Folders that don't all number
/// themselves — parts the person combined — keep the walk's order.
fn in_disc_order(relative: &Path, audio_folders: &[PathBuf]) -> Vec<PathBuf> {
    let numbered: Option<Vec<(u32, &PathBuf)>> = audio_folders
        .iter()
        .map(|folder| {
            let part = folder.strip_prefix(relative).ok()?.components().next()?;
            let number = folder_part_number(&part.as_os_str().to_string_lossy())?;
            Some((number, folder))
        })
        .collect();
    match numbered {
        Some(mut numbered) => {
            // Stable, so folders within one part keep the walk's order.
            numbered.sort_by_key(|(number, _)| *number);
            numbered
                .into_iter()
                .map(|(_, folder)| folder.clone())
                .collect()
        }
        None => audio_folders.to_vec(),
    }
}

/// The parts a grouping rooted at `relative` reads: each folder with audio of
/// its own below it, with the prefix its files take in the release.
fn parts_under(root: &Path, relative: &Path, part_folders: &[PathBuf]) -> Vec<ReleasePart> {
    part_folders
        .iter()
        .map(|folder| {
            let within = folder.strip_prefix(relative).unwrap_or(folder);
            let prefix = relative_path_string(within);
            ReleasePart {
                folder: if folder.as_os_str().is_empty() {
                    root.to_path_buf()
                } else {
                    root.join(folder)
                },
                prefix: if prefix.is_empty() {
                    prefix
                } else {
                    format!("{prefix}/")
                },
            }
        })
        .collect()
}

/// One release read from `files`: rooted at `relative`, shown as the folder
/// at `candidate_relative`, and — when `grouping` names one — read as the
/// folders `part_folders` together.
#[allow(clippy::too_many_arguments)]
pub(super) fn candidate_from_files(
    scan: &ScanRoot<'_>,
    files: Vec<FileEntry>,
    relative: &Path,
    candidate_relative: &Path,
    scope: ReleaseFileScope,
    grouping: Option<String>,
    part_folders: &[PathBuf],
) -> Result<Option<ProjectedScanNode>, FolderScanError> {
    if files.iter().any(|file| is_partial_marker_file(&file.path)) {
        info!(
            "Skipping release approximation {:?}: partial-download marker present",
            relative
        );
        return Ok(None);
    }
    let root = scan.root;
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
    let parts = match grouping {
        Some(_) => parts_under(root, relative, part_folders),
        None => Vec::new(),
    };
    match categorize_selected_files(scan, files, relative, &parts)? {
        CategorizeOutcome::Valid(files) => {
            let file_edit_revision = scan.stored.revision_for_hash(&files.content_hash());
            Ok(Some(ProjectedScanNode::Candidate(FolderCandidate {
                path,
                file_root,
                name,
                files,
                watched_folder_path: scan.watched_folder_path.to_string(),
                scope,
                file_edit_revision,
                display_path,
                grouping,
            })))
        }
        CategorizeOutcome::Invalid { files, tags } => {
            Ok(Some(ProjectedScanNode::Invalid(InvalidCandidate {
                path,
                name,
                watched_folder_path: scan.watched_folder_path.to_string(),
                display_path,
                grouping,
                files,
                tags,
            })))
        }
    }
}

pub(super) fn scan_directory<R, F, D>(
    walk: &Walk<'_, R>,
    relative: &Path,
    ancestors_allow_actionable: bool,
    on_directory: &mut D,
    on_item: &mut F,
) -> Result<ScannedDirectory, FolderScanError>
where
    R: DirectoryReader + ?Sized,
    F: FnMut(ScanItem),
    D: FnMut(PathBuf),
{
    let Walk {
        scan,
        decisions,
        new_grouping_key,
        ..
    } = walk;
    let root = scan.root;
    let watched_folder_path = scan.watched_folder_path;
    let cancellation = scan.cancellation;
    cancellation.check()?;
    on_directory(root.join(relative));
    let listing = walk.listing(relative)?;
    let direct_audio = listing
        .files
        .iter()
        .any(|file| file.size > 0 && is_audio_file(&file.path));
    let has_direct_files = !listing.files.is_empty();
    let wrapper_has_files = !direct_audio && !listing.files.is_empty();
    let mut all_files = listing.files.clone();
    let mut direct_scope_files = listing.files;
    let listing_dirs = listing.directories;
    let mut child_nodes = Vec::new();
    let mut child_nodes_emitted = false;
    let mut contains_audio = direct_audio;
    let mut audio_folders = Vec::new();
    if direct_audio {
        audio_folders.push(relative.to_path_buf());
    }
    let mut loose = Vec::new();
    let relative_string = relative_path_string(relative);
    // How this folder is read. A stored reading — the user's, or the one an
    // earlier scan settled on — stands. With nothing stored, the scan decides
    // for itself from its parts and says so, so the queue gets candidates to
    // work on rather than a card to answer.
    //
    // A reading exists only where it changes something: this folder has to
    // yield two releases or more for combining them to mean anything. Its own
    // tracks are one, and each of its parts is another — which is why the
    // parts are read ahead of the reading and the folders that hold no audio,
    // numbered or not, are left out of both counts. The watched root is never
    // a release itself, so it never decides.
    let reading = match decisions.get(&relative_string) {
        Some(stored) => Some((stored.decision, stored.grouping.clone())),
        None if !listing_dirs.is_empty() => {
            if let Some(decision) = scanned_decision(walk, direct_audio, &listing_dirs)? {
                let grouping = new_grouping_key();
                on_item(ScanItem::Decided {
                    key: FolderReleaseDecisionKey {
                        watched_folder_path: watched_folder_path.to_string(),
                        relative_folder_path: relative_string.clone(),
                    },
                    decision,
                    grouping: grouping.clone(),
                });
                Some((decision, grouping))
            } else {
                None
            }
        }
        None => None,
    };
    let combined_as = match &reading {
        Some((FolderReleaseDecision::CombineAsOneRelease, grouping)) => Some(grouping.clone()),
        _ => None,
    };
    let keep_separate = matches!(
        reading,
        Some((FolderReleaseDecision::KeepAsSeparateReleases, _))
    );
    let can_stream_collection =
        ancestors_allow_actionable && combined_as.is_none() && (!has_direct_files || keep_separate);
    let mut collection_proven = !wrapper_has_files;

    for child in listing_dirs.clone() {
        let child_can_be_actionable = can_stream_collection && collection_proven;
        let child_scan =
            scan_directory(walk, &child, child_can_be_actionable, on_directory, on_item)?;
        contains_audio |= child_scan.contains_audio;
        if !child_scan.contains_audio {
            direct_scope_files.extend(child_scan.all_files.iter().cloned());
        }
        all_files.extend(child_scan.all_files);
        audio_folders.extend(child_scan.audio_folders);
        loose.extend(child_scan.loose);
        if !wrapper_has_files && can_stream_collection {
            let nodes = child_scan.nodes;
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
                    emit_projected_nodes(child_nodes.clone(), on_item);
                    child_nodes_emitted = true;
                }
            } else if wrapper_has_files && collection_proven && can_stream_collection {
                if !child_was_emitted {
                    emit_projected_nodes(child_nodes[child_start..].to_vec(), on_item);
                }
                child_nodes_emitted = true;
            }
        }
    }
    let owns_wrapper_files = !direct_audio && !direct_scope_files.is_empty();

    if let Some(grouping) = combined_as.filter(|_| contains_audio) {
        // The folder read as one release is a grouping of the releases it
        // would be read as apart — its own tracks, and what each folder in it
        // yields — and it reads the files those releases read. A folder's own
        // files come with it by the rule every grouping follows (see
        // `shared_parent`): this folder's when they all sit directly in it,
        // and never those of a folder deeper down, which stay that folder's
        // sidecar as they would beside releases picked together.
        let folder = root.join(relative);
        let mut members: Vec<PathBuf> = child_nodes.iter().map(node_file_root).collect();
        if direct_audio {
            members.push(folder.clone());
        }
        let reads_own_files = crate::import::grouping::shared_parent(
            members
                .iter()
                .map(|member| (watched_folder_path, member.as_path())),
        )
        .as_ref()
            == Some(&folder);
        if owns_wrapper_files && !reads_own_files {
            loose.push(LooseFolder {
                folder: relative.to_path_buf(),
                files: direct_scope_files,
            });
        }
        let unread: HashSet<&Path> = loose
            .iter()
            .flat_map(|folder| folder.files.iter().map(|file| file.path.as_path()))
            .collect();
        let read: Vec<FileEntry> = all_files
            .iter()
            .filter(|file| !unread.contains(file.path.as_path()))
            .cloned()
            .collect();
        let node = candidate_from_files(
            scan,
            read,
            relative,
            relative,
            ReleaseFileScope::Recursive,
            Some(grouping),
            &in_disc_order(relative, &audio_folders),
        )?;
        let nodes = node.into_iter().collect();
        return Ok(ScannedDirectory {
            all_files,
            contains_audio,
            audio_folders,
            nodes,
            nodes_emitted: false,
            loose,
        });
    }

    let mut nodes = Vec::new();
    // Whether this folder's own tracks are one of the nodes. Nothing below it
    // announces that node, so a reading that settles here has to.
    let mut holds_its_own_node = false;
    if direct_audio {
        if let Some(node) = candidate_from_files(
            scan,
            std::mem::take(&mut direct_scope_files),
            relative,
            relative,
            ReleaseFileScope::Direct,
            None,
            &[],
        )? {
            if let ProjectedScanNode::Candidate(candidate) = &node {
                on_item(ScanItem::Discovered(candidate.clone()));
            }
            nodes.push(node);
            holds_its_own_node = true;
        }
    }
    nodes.extend(child_nodes);

    // A collapsed wrapper's files still have one owner when there is exactly
    // one release below it. Keep the release's key and display row, but root
    // its reproducible file scope at the wrapper so sidecars and audio-free
    // siblings survive scan, import, and re-scan.
    let mut collapsed = false;
    if owns_wrapper_files && nodes.len() == 1 {
        if let ProjectedScanNode::Candidate(existing) = &nodes[0] {
            let candidate_relative = existing
                .path
                .strip_prefix(root)
                .map_err(|error| FolderScanError::Other(error.to_string()))?
                .to_path_buf();
            let part_folders: Vec<PathBuf> = existing
                .files
                .parts
                .iter()
                .map(|part| {
                    part.folder
                        .strip_prefix(root)
                        .map(Path::to_path_buf)
                        .map_err(|error| FolderScanError::Other(error.to_string()))
                })
                .collect::<Result<_, _>>()?;
            if let Some(candidate) = candidate_from_files(
                scan,
                all_files.clone(),
                relative,
                &candidate_relative,
                ReleaseFileScope::Recursive,
                existing.grouping.clone(),
                &part_folders,
            )? {
                nodes = vec![candidate];
                collapsed = true;
            }
        }
    }
    if collapsed {
        // The release reads every file below the folder, the files of the
        // folders under it included.
        loose.clear();
    } else if owns_wrapper_files && contains_audio {
        loose.push(LooseFolder {
            folder: relative.to_path_buf(),
            files: direct_scope_files,
        });
    }

    // Children below this folder have already gone out, so the parent will
    // not emit its nodes for it — and one of them is the folder's own tracks,
    // which nothing else has announced.
    if keep_separate && child_nodes_emitted && holds_its_own_node {
        emit_projected_nodes(nodes[..1].to_vec(), on_item);
    }

    Ok(ScannedDirectory {
        all_files,
        contains_audio,
        audio_folders,
        nodes,
        nodes_emitted: child_nodes_emitted,
        loose,
    })
}

/// The folder a projected release reads its files from.
fn node_file_root(node: &ProjectedScanNode) -> PathBuf {
    match node {
        ProjectedScanNode::Candidate(candidate) => candidate.file_root.clone(),
        ProjectedScanNode::Invalid(candidate) => candidate.path.clone(),
    }
}

/// The sidecar of `loose`, its files read the way a release's are. While a
/// download into the folder is running, what it holds is not known yet.
fn sidecar_of(scan: &ScanRoot<'_>, loose: LooseFolder) -> Result<FolderSidecar, FolderScanError> {
    let LooseFolder { folder, files } = loose;
    let files = if files.iter().any(|file| is_partial_marker_file(&file.path)) {
        info!("Sidecar files of {folder:?} are still downloading: partial-download marker present");
        SidecarFiles::Downloading
    } else {
        let tree = CandidateFileIndex::new(files);
        categorize_sidecar(&tree, &folder, scan.root, scan.cancellation, scan.reads)?
    };
    Ok(FolderSidecar {
        watched_folder_path: scan.watched_folder_path.to_string(),
        folder: scan.root.join(&folder),
        files,
    })
}

pub(super) fn emit_projected_nodes<F>(nodes: Vec<ProjectedScanNode>, on_item: &mut F)
where
    F: FnMut(ScanItem),
{
    for node in nodes {
        match node {
            ProjectedScanNode::Candidate(candidate) => on_item(ScanItem::Valid(candidate)),
            ProjectedScanNode::Invalid(candidate) => on_item(ScanItem::Invalid(candidate)),
        }
    }
}

/// Read one folder directly under the watched root, and say everything it
/// yields.
///
/// This is the unit a root is read in. How a folder deep inside reads depends
/// on every folder above it — a kept-separate ancestor names itself on each
/// row, an undecided wrapper names itself as where the rows could be read as
/// one, and a wrapper with one release below it lends that release its own
/// files — and each of those depends on how many releases its other children
/// yield. The root itself does none of that: it is never a release and never
/// decides, so what one of its folders yields depends on nothing outside it.
fn scan_top_level_folder<R, F, D>(
    walk: &Walk<'_, R>,
    folder: &Path,
    on_directory: &mut D,
    on_item: &mut F,
) -> Result<ScannedDirectory, FolderScanError>
where
    R: DirectoryReader + ?Sized,
    F: FnMut(ScanItem),
    D: FnMut(PathBuf),
{
    let mut scanned = scan_directory(walk, folder, true, on_directory, on_item)?;
    if !scanned.nodes_emitted {
        emit_projected_nodes(std::mem::take(&mut scanned.nodes), on_item);
    }
    // Every folder above one of these has been read and none took its files,
    // so they are the folder's to say.
    for loose in std::mem::take(&mut scanned.loose) {
        on_item(ScanItem::Sidecar(sidecar_of(&walk.scan, loose)?));
    }
    Ok(scanned)
}

/// Read one folder directly under `root` again, exactly as a whole-root pass
/// reads it, without reading anything else under the root.
///
/// What a folder deeper down yields is decided by the folders above it up to
/// here and by nothing beside this one (see [`scan_top_level_folder`]), so this
/// is the least that must be read again once any folder in it reads another
/// way.
pub(crate) fn scan_top_level_folder_with_reader<R, F, D>(
    reader: &R,
    root: &Path,
    folder: &Path,
    readings: &ScanReadings<'_>,
    reads: &FileReads,
    cancellation: &ScanCancellation,
    mut on_directory: D,
    mut on_item: F,
) -> Result<(), FolderScanError>
where
    R: DirectoryReader + ?Sized,
    F: FnMut(ScanItem),
    D: FnMut(PathBuf),
{
    cancellation.check()?;
    let mut components = folder.components();
    if !matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) {
        return Err(FolderScanError::Other(format!(
            "{} is not a folder directly under the watched root",
            folder.display()
        )));
    }
    let watched_folder_path = root.to_string_lossy().into_owned();
    let walk = Walk::new(
        ScanRoot {
            root,
            watched_folder_path: &watched_folder_path,
            stored: readings.stored,
            cancellation,
            reads,
        },
        reader,
        readings.decisions,
        readings.new_grouping_key,
    );
    scan_top_level_folder(&walk, folder, &mut on_directory, &mut on_item)?;
    Ok(())
}

/// Scan one watched root to completion, several of the folders directly under
/// it at once, each a directory at a time. What each folder yields is said as
/// soon as it is known, whatever the folders beside it are still reading.
pub fn scan_for_candidates_with_decisions<F>(
    root: PathBuf,
    stored: &StoredCandidateEdits,
    decisions: &FolderReleaseDecisions,
    on_item: F,
) -> Result<(), FolderScanError>
where
    F: FnMut(ScanItem),
{
    scan_for_candidates_with_reader_cancellable_and_directories(
        &OsDirectoryReader,
        root,
        &ScanReadings {
            stored,
            decisions,
            new_grouping_key: &fresh_grouping_key,
        },
        &FileReads::new(Arc::new(
            crate::import::file_tag_snapshot::LoftyFileTagReader,
        )),
        &ScanCancellation::new(),
        |_| {},
        on_item,
    )
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn read_file_subtree<R: DirectoryReader + ?Sized>(
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

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn collect_scoped_entries(
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
/// preserving relative paths, with stored file decisions applied — the folder
/// read on its own, outside any scan. Tests build candidates with it; the app
/// reads folders only through a scan pass.
#[cfg(any(test, feature = "test-utils"))]
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
        &[],
        &ScanCancellation::new(),
        &FileReads::new(Arc::new(
            crate::import::file_tag_snapshot::LoftyFileTagReader,
        )),
    )? {
        CategorizeOutcome::Valid(files) => Ok(files),
        CategorizeOutcome::Invalid { files, .. } => Err(InvalidReason::of(&files).into()),
    }
}
