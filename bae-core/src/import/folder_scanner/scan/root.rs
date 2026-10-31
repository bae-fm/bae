//! A pass over a whole watched root.
//!
//! The folders directly under the root are the units it is read in: what one
//! yields depends on nothing beside it (see [`scan_top_level_folder`]), so a
//! pass reads several at once, each by a walk of its own, and says what each
//! yields on the calling thread as it is read. It begins them newest first by
//! the latest date each has (see [`FolderTimes`]), so a folder just added, or
//! one whose own contents just changed, shows its releases before the rest of
//! the root is read.

use super::super::folder_times::newest_first;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// What the walk of one folder under the root says to the calling thread.
enum Said {
    /// It reached this directory.
    Directory(PathBuf),
    /// It yielded this.
    Item(ScanItem),
    /// It read through the folder at `index` under the root: the files the
    /// root's own release takes from it, or why it could not be read.
    Read {
        index: usize,
        lent: Result<Vec<FileEntry>, FolderScanError>,
    },
}

pub(crate) fn scan_for_candidates_with_reader_cancellable_and_directories<R, F, D>(
    reader: &R,
    root: PathBuf,
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
    debug!("Scanning for candidates in: {:?}", root);
    if let Ok(metadata) = fs::metadata(&root) {
        if !metadata.is_dir() {
            return Err(FolderScanError::NotADirectory { path: root });
        }
    }
    let walk = Walk::new(
        ScanRoot {
            root: &root,
            stored: readings.stored,
            cancellation,
            reads,
        },
        reader,
        readings.decisions,
        readings.new_grouping_key,
    );
    on_directory(root.clone());
    let root_listing = walk.listing(Path::new(""))?;
    let direct_audio = root_listing
        .files
        .iter()
        .any(|file| file.size > 0 && is_audio_file(&file.path));
    let mut direct_scope_files = root_listing.files;
    let walk_order =
        newest_first(&reader.folder_times(&root, &root_listing.directories, cancellation)?);
    for lent in read_folders_at_once(
        &walk,
        &root_listing.directories,
        &walk_order,
        &mut on_directory,
        &mut on_item,
    )? {
        direct_scope_files.extend(lent);
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

/// Read each of `folders`, directly under `walk`'s root, up to
/// [`FOLDERS_AT_ONCE`] at a time, each by a walk of its own over the same
/// readings, beginning them in `walk_order`, which holds each index into
/// `folders` once. What they reach and yield goes to `on_directory` and
/// `on_item` on the calling thread as it arrives: one folder's in the order
/// its walk says it, the folders' interleaved as they are read. Returns the
/// files each folder holding no audio lends the root's own release, in
/// `folders`' order.
///
/// The first folder that fails stops the others and fails the pass with its
/// error; cancelling the pass stops every folder's walk.
fn read_folders_at_once<R, F, D>(
    walk: &Walk<'_, R>,
    folders: &[PathBuf],
    walk_order: &[usize],
    on_directory: &mut D,
    on_item: &mut F,
) -> Result<Vec<Vec<FileEntry>>, FolderScanError>
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
        ..
    } = walk;
    // Cancelled with the pass, and by the first folder that fails, so no
    // walk begins another folder or reads further into its own.
    let stop = scan.cancellation.child();
    let next = AtomicUsize::new(0);
    let mut lent: Vec<Option<Vec<FileEntry>>> = folders.iter().map(|_| None).collect();
    let mut failure = None;
    std::thread::scope(|threads| {
        // Bounded, so the walks wait on the calling thread as it waits on
        // whoever takes what they yield.
        let (said_tx, said_rx) = std::sync::mpsc::sync_channel(FOLDERS_AT_ONCE);
        for _ in 0..FOLDERS_AT_ONCE.min(folders.len()) {
            let said_tx = said_tx.clone();
            let (stop, next) = (&stop, &next);
            let scan = ScanRoot {
                cancellation: stop,
                ..*scan
            };
            let (reader, decisions, new_grouping_key) = (*reader, *decisions, *new_grouping_key);
            threads.spawn(move || {
                // A walk's look-ahead is its own folder's, so each walk is
                // its own.
                let walk = Walk::new(scan, reader, decisions, new_grouping_key);
                // Nothing takes what the walks say once the calling thread
                // has stopped listening, so they stop too.
                let say = |said| {
                    if said_tx.send(said).is_err() {
                        stop.cancel();
                    }
                };
                while !stop.is_cancelled() {
                    let Some(&index) = walk_order.get(next.fetch_add(1, Ordering::Relaxed)) else {
                        break;
                    };
                    let folder = &folders[index];
                    let lent = scan_top_level_folder(
                        &walk,
                        folder,
                        &mut |directory| say(Said::Directory(directory)),
                        &mut |item| say(Said::Item(item)),
                    )
                    .map(|scanned| match scanned.contains_audio {
                        true => Vec::new(),
                        false => scanned.all_files,
                    });
                    let failed = lent.is_err();
                    say(Said::Read { index, lent });
                    // Said before the others stop, so the folder that
                    // failed is the first the calling thread hears fail.
                    if failed {
                        stop.cancel();
                    }
                }
            });
        }
        drop(said_tx);
        for said in said_rx {
            match said {
                Said::Directory(directory) => on_directory(directory),
                Said::Item(item) => on_item(item),
                Said::Read {
                    index,
                    lent: Ok(files),
                } => lent[index] = Some(files),
                // A folder stopped by an earlier failure says it was
                // cancelled; the pass fails with what stopped it.
                Said::Read {
                    lent: Err(error), ..
                } => {
                    failure.get_or_insert(error);
                }
            }
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    // A walk that saw the pass cancelled between two folders stops without
    // an error of its own.
    scan.cancellation.check()?;
    Ok(lent
        .into_iter()
        .map(|files| files.expect("a pass nothing stopped reads every folder"))
        .collect())
}
