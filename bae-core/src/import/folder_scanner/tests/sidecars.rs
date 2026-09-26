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

/// A folder read as one release takes in the sidecar files of every folder
/// below it, so none of theirs is said on its own.
#[test]
fn a_folder_read_as_one_release_takes_the_sidecars_below_it() {
    let (_temp, root, _album) = album_of_discs(&["Disc 1", "Disc 2"]);
    let separate_album = |artist: FolderReleaseDecision| {
        readings(&[
            (
                "Artist/Album",
                FolderReleaseDecision::KeepAsSeparateReleases,
                FolderReleaseDecisionAuthor::User,
            ),
            ("Artist", artist, FolderReleaseDecisionAuthor::User),
        ])
    };
    let other = root.join("Artist").join("Other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("track.flac"), fake_flac()).unwrap();
    assert_eq!(
        sidecars(
            &root,
            separate_album(FolderReleaseDecision::KeepAsSeparateReleases)
        )
        .len(),
        1
    );
    assert!(sidecars(&root, separate_album(FolderReleaseDecision::CombineAsOneRelease)).is_empty());
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
