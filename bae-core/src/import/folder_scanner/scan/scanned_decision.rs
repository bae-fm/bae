use super::*;

/// How the scan reads a folder nothing is stored for, from its children, or
/// `None` when the folder yields fewer than two releases and so has nothing
/// to read one way or the other.
///
/// A child counts only if it holds audio, whatever its name: `Disc 1`,
/// `Disc 2` and `covers` are two discs and a sidecar the release carries, a
/// numbered `Disc 3` with nothing in it is no disc, and a `Bonus` folder with
/// tracks in it is a third release however it is named.
///
/// The children are looked into only as far as the answer needs, because
/// every release below this folder waits on it. When their names can't
/// number one release's discs, the folder is separate releases as soon as it
/// is known to yield two, and the children after that are not looked into.
/// When they can, every child is counted, each up to its second folder of
/// audio. Every listing looked at is held for the walk that follows.
pub(super) fn scanned_decision<R>(
    walk: &Walk<'_, R>,
    holds_audio_directly: bool,
    directories: &[PathBuf],
) -> Result<Option<FolderReleaseDecision>, FolderScanError>
where
    R: DirectoryReader + ?Sized,
{
    let names: Vec<String> = directories
        .iter()
        .map(|directory| {
            directory.file_name().map_or_else(
                || directory.to_string_lossy().into_owned(),
                |name| name.to_string_lossy().into_owned(),
            )
        })
        .collect();
    let mut releases = usize::from(holds_audio_directly);
    if names_could_number_one_release(holds_audio_directly, &names) {
        let mut parts = Vec::with_capacity(directories.len());
        for (directory, name) in directories.iter().zip(names) {
            let audio = match audio_folders_below(walk, directory, 2)? {
                0 => continue,
                1 => PartAudio::OneFolder,
                _ => PartAudio::SeveralFolders,
            };
            parts.push(FolderPart { name, audio });
        }
        releases += parts.len();
        return Ok((releases >= 2)
            .then(|| heuristic_folder_release_decision(holds_audio_directly, &parts)));
    }
    for directory in directories {
        if releases >= 2 {
            break;
        }
        if audio_folders_below(walk, directory, 1)? > 0 {
            releases += 1;
        }
    }
    Ok((releases >= 2).then_some(FolderReleaseDecision::KeepAsSeparateReleases))
}

/// How many folders hold audio of their own in `relative`'s subtree, itself
/// included, counted no further than `limit`. Each listing it reads is held
/// for the walk.
fn audio_folders_below<R>(
    walk: &Walk<'_, R>,
    relative: &Path,
    limit: usize,
) -> Result<usize, FolderScanError>
where
    R: DirectoryReader + ?Sized,
{
    let mut found = 0;
    let mut pending = vec![relative.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let listing = walk.listing(&folder)?;
        if listing
            .files
            .iter()
            .any(|file| file.size > 0 && is_audio_file(&file.path))
        {
            found += 1;
        }
        pending.extend(listing.directories.iter().cloned());
        walk.hold(folder, listing);
        if found == limit {
            break;
        }
    }
    Ok(found)
}
