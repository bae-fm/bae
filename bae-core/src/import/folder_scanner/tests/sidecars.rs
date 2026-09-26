// ── A folder's sidecar files ────────────────────────────────────────────────

/// The sidecars a scan of `root` says, read under `decisions`.
fn sidecars(root: &Path, decisions: FolderReleaseDecisions) -> Vec<FolderSidecar> {
    scan_for_candidates_with_decisions_collect(root.to_path_buf(), decisions)
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Sidecar(sidecar) => Some(sidecar),
            _ => None,
        })
        .collect()
}

/// `Artist/Album` holding `discs`, each a folder of one track, beside a
/// cover and a folder of scans.
fn album_of_discs(discs: &[&str]) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let album = root.join("Artist").join("Album");
    for disc in discs {
        std::fs::create_dir_all(album.join(disc)).unwrap();
        std::fs::write(album.join(disc).join("track.flac"), fake_flac()).unwrap();
    }
    std::fs::create_dir_all(album.join("Scans")).unwrap();
    std::fs::write(album.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(album.join("Scans").join("booklet.txt"), "notes").unwrap();
    (temp_dir, root, album)
}

/// Disc folders kept as releases of their own leave the album folder's files
/// to none of them: the scan says them as the album folder's sidecar, with
/// the audio-free folder below it and every file its role.
#[test]
fn a_folder_kept_as_separate_releases_says_its_own_files() {
    let (_temp, root, album) = album_of_discs(&["Disc 1", "Disc 2", "Disc 3"]);
    let found = sidecars(
        &root,
        readings(&[(
            "Artist/Album",
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User,
        )]),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].folder, album);
    let SidecarFiles::Valid(files) = &found[0].files else {
        panic!("the album's files are readable: {:?}", found[0].files);
    };
    assert_eq!(
        files
            .iter()
            .map(|entry| (entry.file.relative_path.as_str(), &entry.role))
            .collect::<Vec<_>>(),
        [
            ("cover.jpg", &FileRole::Artwork),
            ("Scans/booklet.txt", &FileRole::Document)
        ]
    );
    assert!(files.iter().all(|entry| entry.file.path.starts_with(&album)));
}

/// A folder read as one release owns its files, a folder with tracks of its
/// own gives them to its own release, and a wrapper over one release lends
/// them to it: none of them has a sidecar.
#[test]
fn a_folder_whose_files_a_release_owns_has_no_sidecar() {
    let (_temp, root, _album) = album_of_discs(&["Disc 1", "Disc 2"]);
    assert!(sidecars(&root, FolderReleaseDecisions::default()).is_empty());

    let (_temp, root, album) = album_of_discs(&["Disc 1", "Disc 2"]);
    std::fs::write(album.join("track.flac"), fake_flac()).unwrap();
    assert!(sidecars(&root, FolderReleaseDecisions::default()).is_empty());

    let (_temp, root, _album) = album_of_discs(&["Disc 1"]);
    assert!(sidecars(&root, FolderReleaseDecisions::default()).is_empty());
}

/// A folder read as one release reads files by the rule releases picked
/// together follow: an album folder combined over its discs reads its own
/// cover, while an artist folder combined over releases at different depths —
/// the album's discs kept apart, beside another album — reads neither its own
/// files nor the album folder's, which stay those folders' sidecars.
#[test]
fn a_folder_read_as_one_release_reads_files_by_the_grouping_rule() {
    let (_temp, root, album) = album_of_discs(&["Disc 1", "Disc 2"]);
    let combined_album = scan_for_candidates_with_decisions_collect(
        root.clone(),
        FolderReleaseDecisions::default(),
    );
    let album_release = combined_album
        .iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) if candidate.path == album => Some(candidate),
            _ => None,
        })
        .expect("the album folder reads as one release");
    assert!(album_release
        .files
        .files
        .iter()
        .any(|entry| entry.file.relative_path == "cover.jpg"));

    let artist = root.join("Artist");
    let other = artist.join("Other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("track.flac"), fake_flac()).unwrap();
    std::fs::write(artist.join("artist.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    let items = scan_for_candidates_with_decisions_collect(
        root,
        readings(&[
            (
                "Artist/Album",
                FolderReleaseDecision::KeepAsSeparateReleases,
                FolderReleaseDecisionAuthor::User,
            ),
            (
                "Artist",
                FolderReleaseDecision::CombineAsOneRelease,
                FolderReleaseDecisionAuthor::User,
            ),
        ]),
    );
    let artist_release = items
        .iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) if candidate.path == artist => Some(candidate),
            _ => None,
        })
        .expect("the artist folder reads as one release");
    assert_eq!(
        artist_release
            .files
            .files
            .iter()
            .map(|entry| entry.file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["Album/Disc 1/track.flac", "Album/Disc 2/track.flac", "Other/track.flac"]
    );
    let mut folders: Vec<&Path> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Sidecar(sidecar) => Some(sidecar.folder.as_path()),
            _ => None,
        })
        .collect();
    folders.sort();
    assert_eq!(folders, [artist.as_path(), album.as_path()]);
}

/// A download still running into a folder is said as such, not as a folder
/// with no files of its own.
#[test]
fn a_folder_still_downloading_says_so() {
    let (_temp, root, album) = album_of_discs(&["Disc 1", "Disc 2"]);
    std::fs::write(album.join("booklet.pdf.part"), b"partial").unwrap();
    let found = sidecars(
        &root,
        readings(&[(
            "Artist/Album",
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User,
        )]),
    );
    assert_eq!(
        found.iter().map(|sidecar| &sidecar.files).collect::<Vec<_>>(),
        [&SidecarFiles::Downloading]
    );
}

/// A broken image among a folder's files is said as the defect it is, the
/// way a release holding it would be.
#[test]
fn a_broken_file_makes_the_sidecar_invalid() {
    let (_temp, root, album) = album_of_discs(&["Disc 1", "Disc 2"]);
    std::fs::write(album.join("cover.jpg"), b"not an image").unwrap();
    let found = sidecars(
        &root,
        readings(&[(
            "Artist/Album",
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User,
        )]),
    );
    assert_eq!(
        found.iter().map(|sidecar| &sidecar.files).collect::<Vec<_>>(),
        [&SidecarFiles::Invalid(InvalidReason::CorruptImage {
            path: "cover.jpg".into()
        })]
    );
}
