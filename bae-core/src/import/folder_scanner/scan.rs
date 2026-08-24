use super::boundary::apply_resolved_boundary;
use super::*;

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

pub(super) struct OsDirectoryReader;

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
    nodes: Vec<ProjectedScanNode>,
    nodes_emitted: bool,
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
    R: DirectoryReader,
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
    R: DirectoryReader,
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

pub(super) fn relative_path_string(path: &Path) -> String {
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
    files: Vec<FileEntry>,
    relative: &Path,
    root: &Path,
    stored: &StoredCandidateEdits,
    cancellation: &ScanCancellation,
) -> Result<CategorizeOutcome, FolderScanError> {
    let tree = CandidateFileIndex::new(files);
    categorize_files_from_tree(&tree, relative, root, stored, cancellation)
}

pub(super) fn candidate_from_files(
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

pub(super) fn scan_directory<R, F, D>(
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
    let listing_dirs = listing.directories;
    let mut child_nodes = Vec::new();
    let mut child_nodes_emitted = false;
    let mut contains_audio = direct_audio;
    let relative_string = relative_path_string(relative);
    // How this folder is read. A stored decision — the user's, or the one an
    // earlier scan settled on — stands. With nothing stored, the scan decides
    // for itself from the parts' names and says so, so the queue gets
    // candidates to work on rather than a card to answer.
    //
    // A decision exists only where it changes something: this folder has to
    // yield two releases or more for combining them to mean anything. Its own
    // tracks are one, and each of its parts is another — which is why the
    // parts are read ahead of the decision and the sidecar folders that yield
    // nothing are left out of both counts. The watched root is never a release
    // itself, so it never decides.
    let (decision, decided_here) = match decisions.get(&relative_string) {
        Some((decision, _)) => (Some(decision), false),
        None if allow_unresolved_boundary && !listing_dirs.is_empty() => {
            let parts = part_folder_names(reader, root, &listing_dirs, cancellation)?;
            let yields_several = parts.len() > 1 || (direct_audio && !parts.is_empty());
            if yields_several {
                (
                    Some(heuristic_folder_release_decision(direct_audio, &parts)),
                    true,
                )
            } else {
                (None, false)
            }
        }
        None => (None, false),
    };
    if decided_here {
        if let Some(decision) = decision {
            on_item(ScanItem::Decided {
                key: FolderReleaseDecisionKey {
                    watched_folder_path: watched_folder_path.to_string(),
                    relative_folder_path: relative_string.clone(),
                },
                decision,
            });
        }
    }
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

    for child in listing_dirs.clone() {
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
    // Whether this folder's own tracks are one of the nodes. Nothing below it
    // announces that node, so a reading that settles here has to.
    let mut holds_its_own_node = false;
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
            holds_its_own_node = true;
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
        // Children below this folder have already gone out, so the parent will
        // not emit its nodes for it — and one of them is the folder's own
        // tracks, which nothing else has announced.
        if child_nodes_emitted && holds_its_own_node {
            emit_projected_nodes(nodes[..1].to_vec(), on_item);
        }
    } else if allow_unresolved_boundary && nodes.len() > 1 {
        // Several releases below and nothing settled about this folder: it is
        // a wrapper the releases happen to sit under — one child folder holding
        // them all, loose files beside it, or both. There is nothing to ask.
        // The folder that yields the several releases has already decided how
        // it reads, and this one is where reading them as one is offered, so
        // every candidate names it and the header above them carries the flip.
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
            emit_projected_nodes(nodes.clone(), on_item);
        }
    }

    Ok(ScannedDirectory {
        all_files,
        contains_audio,
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

pub(super) fn read_file_subtree<R: DirectoryReader>(
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
