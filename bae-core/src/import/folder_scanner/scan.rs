use super::*;

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
        .map_err(|_| {
            FolderScanError::Other(format!(
                "modification time is before the Unix epoch: {}",
                path.display()
            ))
        })?;
    i64::try_from(elapsed.as_nanos()).map_err(|_| {
        FolderScanError::Other(format!(
            "modification time exceeds SQLite's integer range: {}",
            path.display()
        ))
    })
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
}

/// The folder one scan pass reads, and what it reads it against: the watched
/// root every relative path is under, the folder key its candidates are stamped
/// with, the file corrections the user has stored, and the flag that stops the
/// pass.
pub(super) struct ScanRoot<'a> {
    root: &'a Path,
    watched_folder_path: &'a str,
    stored: &'a StoredCandidateEdits,
    cancellation: &'a ScanCancellation,
    /// Every audio file's facts this pass has read, so regrouping folders
    /// reads none twice.
    probed: &'a ProbedAudio,
}

/// What a pass reads a root's folders against: the file corrections the user
/// has stored, how each folder reads, and where the key of a grouping the
/// pass proposes itself comes from.
pub(crate) struct ScanReadings<'a> {
    pub(crate) stored: &'a StoredCandidateEdits,
    pub(crate) decisions: &'a FolderReleaseDecisions,
    pub(crate) new_grouping_key: &'a dyn Fn() -> String,
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
    new_grouping_key: &'a dyn Fn() -> String,
}

/// The child folders that are this folder's parts, in listing order — what the
/// scan reads its decision from when nothing is stored for it.
///
/// A folder whose name carries a part number is taken at its word. An
/// unnumbered one is a part only if it holds audio: `Disc 1`, `Disc 2` and
/// `covers` are two parts and a sidecar the release carries, not three parts
/// one of which forgot to number itself, while a `Bonus` folder with tracks in
/// it is a third release however it is named.
///
/// The name is read first because looking is what costs: answering "does this
/// hold audio" means reading the subtree, and doing that for every child
/// before deciding would hold every release below this folder back until the
/// slowest of them had been walked. A sidecar folder is small and holds no
/// audio, which is exactly the walk that ends quickly.
fn part_folder_names<R>(
    reader: &R,
    root: &Path,
    directories: &[PathBuf],
    cancellation: &ScanCancellation,
) -> Result<Vec<String>, FolderScanError>
where
    R: DirectoryReader + ?Sized,
{
    let mut names = Vec::with_capacity(directories.len());
    for directory in directories {
        let name = directory.file_name().map_or_else(
            || directory.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        if folder_part_number(&name).is_none()
            && !holds_audio_below(reader, root, directory, cancellation)?
        {
            continue;
        }
        names.push(name);
    }
    Ok(names)
}

/// Whether this folder or anything below it holds audio — whether it yields a
/// candidate at all.
///
/// Stops at the first audio file, so only an audio-free tree is walked whole.
fn holds_audio_below<R>(
    reader: &R,
    root: &Path,
    relative: &Path,
    cancellation: &ScanCancellation,
) -> Result<bool, FolderScanError>
where
    R: DirectoryReader + ?Sized,
{
    let listing = reader.read(root, relative, cancellation)?;
    if listing
        .files
        .iter()
        .any(|file| file.size > 0 && is_audio_file(&file.path))
    {
        return Ok(true);
    }
    for child in listing.directories {
        if holds_audio_below(reader, root, &child, cancellation)? {
            return Ok(true);
        }
    }
    Ok(false)
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
        scan.probed,
    )
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
        CategorizeOutcome::Invalid(reason) => {
            Ok(Some(ProjectedScanNode::Invalid(InvalidCandidate {
                path,
                name,
                watched_folder_path: scan.watched_folder_path.to_string(),
                display_path,
                grouping,
                reason,
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
        reader,
        decisions,
        new_grouping_key,
    } = walk;
    let root = scan.root;
    let watched_folder_path = scan.watched_folder_path;
    let cancellation = scan.cancellation;
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
    let listing_dirs = listing.directories;
    let mut child_nodes = Vec::new();
    let mut child_nodes_emitted = false;
    let mut contains_audio = direct_audio;
    let mut audio_folders = Vec::new();
    if direct_audio {
        audio_folders.push(relative.to_path_buf());
    }
    let relative_string = relative_path_string(relative);
    // How this folder is read. A stored reading — the user's, or the one an
    // earlier scan settled on — stands. With nothing stored, the scan decides
    // for itself from the parts' names and says so, so the queue gets
    // candidates to work on rather than a card to answer.
    //
    // A reading exists only where it changes something: this folder has to
    // yield two releases or more for combining them to mean anything. Its own
    // tracks are one, and each of its parts is another — which is why the
    // parts are read ahead of the reading and the sidecar folders that yield
    // nothing are left out of both counts. The watched root is never a release
    // itself, so it never decides.
    let reading = match decisions.get(&relative_string) {
        Some(stored) => Some((stored.decision, stored.grouping.clone())),
        None if !listing_dirs.is_empty() => {
            let parts = part_folder_names(*reader, root, &listing_dirs, cancellation)?;
            let yields_several = parts.len() > 1 || (direct_audio && !parts.is_empty());
            if yields_several {
                let decision = heuristic_folder_release_decision(direct_audio, &parts);
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
    let can_stream_collection = ancestors_allow_actionable
        && combined_as.is_none()
        && (!has_direct_files || keep_separate);
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
        let node = candidate_from_files(
            scan,
            all_files.clone(),
            relative,
            relative,
            ReleaseFileScope::Recursive,
            Some(grouping),
            &audio_folders,
        )?;
        let nodes = node.into_iter().collect();
        return Ok(ScannedDirectory {
            all_files,
            contains_audio,
            audio_folders,
            nodes,
            nodes_emitted: false,
        });
    }

    let mut nodes = Vec::new();
    // Whether this folder's own tracks are one of the nodes. Nothing below it
    // announces that node, so a reading that settles here has to.
    let mut holds_its_own_node = false;
    if direct_audio {
        if let Some(node) = candidate_from_files(
            scan,
            direct_scope_files,
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
            }
        }
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

pub(crate) fn scan_for_candidates_with_reader_cancellable_and_directories<R, F, D>(
    reader: &R,
    root: PathBuf,
    readings: &ScanReadings<'_>,
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
    debug!("Scanning for candidates in: {:?}", root);
    if let Ok(metadata) = fs::metadata(&root) {
        if !metadata.is_dir() {
            return Err(FolderScanError::NotADirectory { path: root });
        }
    }
    let watched_folder_path = root.to_string_lossy().into_owned();
    let probed = ProbedAudio::default();
    let walk = Walk {
        scan: ScanRoot {
            root: &root,
            watched_folder_path: &watched_folder_path,
            stored: readings.stored,
            cancellation,
            probed: &probed,
        },
        reader,
        decisions: readings.decisions,
        new_grouping_key: readings.new_grouping_key,
    };
    on_directory(root.clone());
    let root_listing = reader.read(&root, Path::new(""), cancellation)?;
    let direct_audio = root_listing
        .files
        .iter()
        .any(|file| file.size > 0 && is_audio_file(&file.path));
    let mut direct_scope_files = root_listing.files;

    for child in root_listing.directories {
        let child_scan = scan_top_level_folder(&walk, &child, &mut on_directory, &mut on_item)?;
        if !child_scan.contains_audio {
            direct_scope_files.extend(child_scan.all_files.iter().cloned());
        }
    }

    if direct_audio {
        if let Some(node) = candidate_from_files(
            &walk.scan,
            direct_scope_files,
            Path::new(""),
            Path::new(""),
            ReleaseFileScope::Direct,
            None,
            &[],
        )? {
            emit_projected_nodes(vec![node], &mut on_item);
        }
    }
    Ok(())
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
    let probed = ProbedAudio::default();
    let walk = Walk {
        scan: ScanRoot {
            root,
            watched_folder_path: &watched_folder_path,
            stored: readings.stored,
            cancellation,
            probed: &probed,
        },
        reader,
        decisions: readings.decisions,
        new_grouping_key: readings.new_grouping_key,
    };
    scan_top_level_folder(&walk, folder, &mut on_directory, &mut on_item)?;
    Ok(())
}

/// Scan one watched root a directory at a time, to completion. Completed
/// release approximations and unresolved boundaries are emitted before
/// unrelated sibling directories are read.
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
        &ScanCancellation::new(),
        |_| {},
        on_item,
    )
}

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
        &ProbedAudio::default(),
    )? {
        CategorizeOutcome::Valid(files) => Ok(files),
        CategorizeOutcome::Invalid(reason) => Err(reason.into()),
    }
}
