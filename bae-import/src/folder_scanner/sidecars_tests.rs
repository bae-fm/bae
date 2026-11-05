//! A folder's sidecar files: what a folder kept as separate releases or read
//! as one carries beside them, a folder still downloading, and what makes
//! them invalid.

use super::*;
use crate::folder_scanner::tests::*;

// ── A folder's sidecar files ────────────────────────────────────────────────

/// The sidecars a scan of `root` says, read under `decisions`.
fn sidecars(root: &Path, decisions: FolderReleaseDecisions) -> Vec<FolderSidecar> {
    scan_for_candidates_with_decisions_collect(root.to_path_buf(), decisions)
        .into_iter()
        .filter_map(|item| match item {
            WalkItem::Entry(FolderEntry::Sidecar(sidecar)) => Some(sidecar),
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
        [("cover.jpg", &FileRole::Artwork), ("Scans/booklet.txt", &FileRole::Document)]
    );
    assert!(files.iter().all(|entry| entry.file.version.path.starts_with(&album)));
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
    let combined_album =
        scan_for_candidates_with_decisions_collect(root.clone(), FolderReleaseDecisions::default());
    let album_release = combined_album
        .iter()
        .find_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate)))
                if candidate.path == album =>
            {
                Some(candidate)
            }
            _ => None,
        })
        .expect("the album folder reads as one release");
    assert!(album_release.files.files.iter().any(|entry| entry.file.relative_path == "cover.jpg"));

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
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate)))
                if candidate.path == artist =>
            {
                Some(candidate)
            }
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
            WalkItem::Entry(FolderEntry::Sidecar(sidecar)) => Some(sidecar.folder.as_path()),
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

/// A damaged image among a folder's files is carried as one, never as its
/// artwork: the release that takes the files in is still its audio.
#[test]
fn a_damaged_image_is_carried_in_the_sidecar_as_one() {
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
    let [sidecar] = found.as_slice() else {
        panic!("one sidecar: {found:?}");
    };
    let SidecarFiles::Valid(files) = &sidecar.files else {
        panic!("a damaged image leaves the files readable: {:?}", sidecar.files);
    };
    assert_eq!(
        files
            .iter()
            .map(|entry| (entry.file.relative_path.as_str(), &entry.role))
            .collect::<Vec<_>>(),
        [("cover.jpg", &FileRole::DamagedImage), ("Scans/booklet.txt", &FileRole::Document)]
    );
}

/// A zero-byte track among a folder's files is said as the defect it is, the
/// way a release holding it would be.
#[test]
fn a_zero_byte_track_makes_the_sidecar_invalid() {
    let (_temp, root, album) = album_of_discs(&["Disc 1", "Disc 2"]);
    std::fs::write(album.join("bonus.flac"), b"").unwrap();
    let found = sidecars(
        &root,
        readings(&[(
            "Artist/Album",
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User,
        )]),
    );
    let [sidecar] = found.as_slice() else {
        panic!("one sidecar: {found:?}");
    };
    let SidecarFiles::Invalid(files) = &sidecar.files else {
        panic!("a zero-byte track leaves the files unimportable: {:?}", sidecar.files);
    };
    assert_eq!(
        files.iter().map(|file| (file.relative_path.as_str(), file.role)).collect::<Vec<_>>(),
        [
            ("bonus.flac", ProposedRole::DamagedAudio { damage: AudioDamage::Empty }),
            ("cover.jpg", ProposedRole::Image),
            ("Scans/booklet.txt", ProposedRole::Document),
        ],
        "every file, as the scan read it"
    );
}

/// A release's sidecar subfolders attach to the candidate by category:
/// `booklet/*.png` becomes artwork (keeping its `booklet/` prefix), and
/// `Info/Tracklist.txt` becomes a document.
#[test]
fn subfolder_sidecars_attach_by_category() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        file("Album/booklet/page1.png", FileKind::Png),
        file("Album/booklet/page2.png", FileKind::Png),
        file("Album/Info/Tracklist.txt", FileKind::TracklistTxt),
    ]);
    let result = run_scenario(entries);
    let c = result.candidate("Album");

    let booklet_paths: Vec<_> = c
        .files
        .artwork()
        .filter(|a| a.relative_path.starts_with("booklet/"))
        .map(|a| a.relative_path.as_str())
        .collect();
    assert_eq!(booklet_paths.len(), 2, "booklet artwork: {booklet_paths:?}");

    assert!(
        c.files.documents().any(|d| d.relative_path.ends_with("Tracklist.txt")),
        "Info/Tracklist.txt should be a document, got {:?}",
        c.files.documents().map(|d| d.relative_path.as_str()).collect::<Vec<_>>(),
    );
}

/// The folder is the release, so an unrecognized sidecar is carried like every
/// other file: it becomes a row the import writes, and it counts toward the
/// hash. The pair that fails if someone narrows either set back — and they must
/// stay one set, or the fingerprint stops describing the payload it identifies.
#[test]
fn an_unrecognized_sidecar_is_carried_and_hashed() {
    const SIDECARS: [&str; 5] = ["rip.ffp", "rip.md5", "rip.nfo", "rip.par2", "rip.sfv"];

    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("Bare");
    let with_sidecars = tmp.path().join("Sidecars");
    for dir in [&bare, &with_sidecars] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("01.flac"), fake_flac()).unwrap();
    }
    for sidecar in SIDECARS {
        std::fs::write(with_sidecars.join(sidecar), b"scene notes").unwrap();
    }

    let bare_files = scan_files(&bare);
    let sidecar_files = scan_files(&with_sidecars);
    assert_eq!(
        sidecar_files
            .files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Other))
            .map(|entry| entry.file.file_name.as_str())
            .collect::<Vec<_>>(),
        SIDECARS.to_vec(),
        "each sidecar is listed under the role for what the scan doesn't recognize",
    );

    // The import writes a row for every file the hash covers.
    let carried: Vec<_> =
        sidecar_files.release_files().map(|file| file.file_name.clone()).collect();
    for sidecar in SIDECARS {
        assert!(
            carried.contains(&sidecar.to_string()),
            "{sidecar} must become a file row, got {carried:?}",
        );
    }

    assert_ne!(
        bare_files.content_hash(),
        sidecar_files.content_hash(),
        "a file the release carries must count toward the hash",
    );
}

/// Two of a folder's own files showing one name leave them unimportable, the
/// way a release holding them would be.
#[cfg(target_os = "linux")]
#[test]
fn two_files_showing_one_name_make_the_sidecar_invalid() {
    let (_temp, root, album) = album_of_discs(&["Disc 1", "Disc 2"]);
    std::fs::write(album.join(r"Caf\xE9.txt"), "one").unwrap();
    std::fs::write(album.join(latin1_name(".txt")), "two").unwrap();

    let found = sidecars(
        &root,
        readings(&[(
            "Artist/Album",
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User,
        )]),
    );

    let [sidecar] = found.as_slice() else {
        panic!("one sidecar: {found:?}");
    };
    let SidecarFiles::Invalid(files) = &sidecar.files else {
        panic!("two files showing one name leave the files unimportable: {:?}", sidecar.files);
    };
    assert_eq!(
        InvalidReason::of(files),
        InvalidReason::SameName { name: r"Caf\xE9.txt".to_string() }
    );
}
