//! Whether a folder's children are one release or several, by their
//! names: numbered parts, labeled disc numbers, and what a person decided.

use super::*;
use crate::folder_scanner::tests::*;

/// Parts named `names`, each holding one folder's audio.
fn discs(names: &[&str]) -> Vec<FolderPart> {
    names
        .iter()
        .map(|name| FolderPart { name: name.to_string(), audio: PartAudio::OneFolder })
        .collect()
}

fn decide(names: &[&str]) -> FolderReleaseDecision {
    heuristic_folder_release_decision(false, &discs(names))
}

/// Twelve discs numbered `disc 01` to `disc 12`, each one folder of
/// tracks, are one release.
#[test]
fn a_zero_padded_run_of_twelve_discs_is_one_release() {
    let names: Vec<String> = (1..=12).map(|disc| format!("disc {disc:02}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    assert_eq!(decide(&names), FolderReleaseDecision::CombineAsOneRelease);
}

/// Two numbered folders that each wrap whole albums — three in the
/// first, one in the second — are four releases however they number
/// themselves.
#[test]
fn a_numbered_part_holding_several_albums_is_no_disc() {
    let parts = [
        FolderPart { name: "01_Artist_Originals".to_string(), audio: PartAudio::SeveralFolders },
        FolderPart { name: "02_Artist_Remasters".to_string(), audio: PartAudio::OneFolder },
    ];
    assert_eq!(
        decide(&["01_Artist_Originals", "02_Artist_Remasters"]),
        FolderReleaseDecision::CombineAsOneRelease,
        "the names alone do number 1 and 2"
    );
    assert_eq!(
        heuristic_folder_release_decision(false, &parts),
        FolderReleaseDecision::KeepAsSeparateReleases
    );
}

/// The children number themselves 1..=N, so they are the parts of one
/// release.
#[test]
fn a_continuous_run_from_one_is_one_release() {
    for names in [
        &["1", "2"][..],
        &["Disc 1", "Disc 2", "Disc 3", "Disc 4"][..],
        &["CD2", "CD1"][..],
        &["Vol. 3", "Vol. 1", "Vol. 2"][..],
    ] {
        assert_eq!(decide(names), FolderReleaseDecision::CombineAsOneRelease, "{names:?}");
    }
}

/// A gap, a run that does not start at one, or a child with no number at
/// all: the folder holds several releases, not one release's parts.
#[test]
fn anything_else_is_several_releases() {
    for names in [
        &["1", "3"][..],
        &["2", "3"][..],
        &["CD1", "CD2", "Bonus"][..],
        &["CD1 Volume 2", "CD2 Volume 3"][..],
        &["Live in Tokyo", "Live in Osaka"][..],
        &["1", "1"][..],
    ] {
        assert_eq!(decide(names), FolderReleaseDecision::KeepAsSeparateReleases, "{names:?}");
    }
}

/// A labeled disc number is authoritative even when other numbers surround
/// it.
#[test]
fn a_labeled_disc_number_wins_over_other_numbers() {
    for names in [
        &["1994 CD1 archive 2001", "1995 CD2 archive 2002"][..],
        &["1994CD1 archive 2001", "1995CD2 archive 2002"][..],
        &["1994 Disc 1 archive 2001", "1995 Disc 2 archive 2002"][..],
        &["1994 Disk 1 archive 2001", "1995 Disk 2 archive 2002"][..],
        &["1994 Vol. 1 archive 2001", "1995 Vol. 2 archive 2002"][..],
        &["1994 Volume 1 archive 2001", "1995 Volume 2 archive 2002"][..],
        &["1994 Volume 1 (CD 1) archive 2001", "1995 Volume 2 (CD 2) archive 2002"][..],
    ] {
        assert_eq!(decide(names), FolderReleaseDecision::CombineAsOneRelease, "{names:?}");
    }
}

/// Without a label, several numbers leave the part number ambiguous.
#[test]
fn unlabeled_multiple_numbers_name_no_part() {
    for names in [
        &["1994 archive 1", "1995 archive 2"][..],
        &["SACD 1994 layer 1", "SACD 1995 layer 2"][..],
        &["éCD1 archive 1994", "éCD2 archive 1995"][..],
    ] {
        assert_eq!(decide(names), FolderReleaseDecision::KeepAsSeparateReleases, "{names:?}");
    }
}

/// Tracks sitting in the folder beside its child folders are their own
/// release, so the folder is not one release's parts.
#[test]
fn a_folder_with_its_own_tracks_holds_several_releases() {
    assert_eq!(
        heuristic_folder_release_decision(true, &discs(&["Disc 1", "Disc 2"])),
        FolderReleaseDecision::KeepAsSeparateReleases
    );
}

/// Names with no part 1 or no part 2, or a folder with tracks of its own,
/// can't be one release's discs whatever the folders hold.
#[test]
fn names_without_parts_one_and_two_cannot_number_a_set() {
    let names = |names: &[&str]| names.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    assert!(names_could_number_one_release(false, &names(&["CD1", "CD2", "Bonus"])));
    assert!(names_could_number_one_release(false, &names(&["Part 01", "Part 02", "Part 99"])));
    assert!(!names_could_number_one_release(false, &names(&["1990 - First", "1992 - Second"])));
    assert!(!names_could_number_one_release(false, &names(&["Release 01", "Release 99"])));
    assert!(!names_could_number_one_release(false, &names(&["Live in Tokyo", "Live in Osaka"])));
    assert!(!names_could_number_one_release(true, &names(&["Disc 1", "Disc 2"])));
}

/// One child is not a set.
#[test]
fn a_single_child_is_not_a_set() {
    assert_eq!(decide(&["Disc 1"]), FolderReleaseDecision::KeepAsSeparateReleases);
}

/// Write one playable track into each folder of `folders` under `root`, with
/// a cover in a `Covers` folder beside it.
fn write_albums(root: &Path, folders: &[&str]) {
    for folder in folders {
        let folder = root.join(folder);
        std::fs::create_dir_all(folder.join("Covers")).unwrap();
        std::fs::write(folder.join("track.flac"), fake_flac()).unwrap();
        std::fs::write(folder.join("Covers/front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    }
}

/// The readings a scan of `root` decides for itself, by folder.
fn decided(items: &[WalkItem]) -> Vec<(String, FolderReleaseDecision)> {
    items
        .iter()
        .filter_map(|item| match item {
            WalkItem::Reading { key, decision, .. } => {
                Some((key.relative_folder.text(), *decision))
            }
            _ => None,
        })
        .collect()
}

fn valid_paths(items: &[WalkItem]) -> Vec<String> {
    let mut paths: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) => {
                Some(candidate.relative_folder.text())
            }
            _ => None,
        })
        .collect();
    paths.sort_unstable();
    paths
}

/// Two numbered folders that wrap whole albums — three in the first, one in
/// the second — number themselves 1 and 2 and are still four releases: a
/// part holding several folders of audio is no disc.
#[test]
fn numbered_folders_of_albums_are_separate_releases() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let collection = "Artist (Originals & Remasters)";
    write_albums(
        &root.join(collection),
        &[
            "01_Artist_Originals/Artist -1984- Album One (Press 11-2X-1001)",
            "01_Artist_Originals/Artist -1985- Album Two (Press 11-2X-1002)",
            "01_Artist_Originals/Artist -1988- Album Three (Press  11-2X-1003)",
            "02_Artist_Remasters/Artist -1992- Album Four (XYZP 104)",
        ],
    );

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

    assert!(
        decided(&items)
            .contains(&(collection.to_string(), FolderReleaseDecision::KeepAsSeparateReleases)),
        "{:?}",
        decided(&items)
    );
    assert_eq!(
        valid_paths(&items),
        vec![
            "Artist (Originals & Remasters)/01_Artist_Originals/Artist -1984- Album One (Press 11-2X-1001)",
            "Artist (Originals & Remasters)/01_Artist_Originals/Artist -1985- Album Two (Press 11-2X-1002)",
            "Artist (Originals & Remasters)/01_Artist_Originals/Artist -1988- Album Three (Press  11-2X-1003)",
            "Artist (Originals & Remasters)/02_Artist_Remasters/Artist -1992- Album Four (XYZP 104)",
        ]
    );
    assert!(items.iter().all(|item| match item {
        WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) =>
            candidate.grouping.is_none(),
        _ => true,
    }));
}

/// `disc 01` to `disc 12`, each one folder of tracks, are one release.
#[test]
fn twelve_numbered_discs_are_one_release() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let discs: Vec<String> = (1..=12).map(|disc| format!("Box Set/disc {disc:02}")).collect();
    write_albums(&root, &discs.iter().map(String::as_str).collect::<Vec<_>>());

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

    assert_eq!(
        decided(&items),
        vec![("Box Set".to_string(), FolderReleaseDecision::CombineAsOneRelease)]
    );
    assert_eq!(valid_paths(&items), vec!["Box Set"]);
    let release = items
        .iter()
        .find_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) => Some(candidate),
            _ => None,
        })
        .expect("the set is one release");
    assert_eq!(release.files.parts.len(), 12);
}

/// A disc whose tracks sit one folder down, under a folder with no audio of
/// its own, still holds one folder's audio.
#[test]
fn discs_whose_tracks_sit_one_folder_down_are_one_release() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    write_albums(&root, &["Box Set/CD1/FLAC", "Box Set/CD2/FLAC"]);

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

    assert_eq!(
        decided(&items),
        vec![("Box Set".to_string(), FolderReleaseDecision::CombineAsOneRelease)]
    );
    assert_eq!(valid_paths(&items), vec!["Box Set"]);
    let release = items
        .iter()
        .find_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) => Some(candidate),
            _ => None,
        })
        .expect("the set is one release");
    assert_eq!(
        release.files.parts.iter().map(|part| part.prefix.as_str()).collect::<Vec<_>>(),
        ["CD1/FLAC/", "CD2/FLAC/"]
    );
}

/// Numbered folders that hold no audio are no discs: an album with artwork in
/// `Disc 1` and `Disc 2` beside its tracks is one release with nothing to
/// decide, and a box of artwork alone yields nothing and decides nothing.
#[test]
fn numbered_folders_without_audio_are_not_parts() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let album = root.join("Artist - Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("track.flac"), fake_flac()).unwrap();
    for folder in ["Artist - Album/Disc 1", "Artist - Album/Disc 2", "Box/Disc 1", "Box/Disc 2"] {
        let folder = root.join(folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("scan.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    }

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

    assert_eq!(decided(&items), vec![]);
    assert_eq!(valid_paths(&items), vec!["Artist - Album"]);
}

/// The listings a folder's reading looks at are the ones the walk goes on
/// to use: every folder is listed once.
#[test]
fn reading_a_folder_lists_nothing_twice() {
    #[derive(Default)]
    struct RecordingReader {
        reads: std::sync::Mutex<Vec<PathBuf>>,
    }

    impl DirectoryReader for RecordingReader {
        fn read(
            &self,
            root: &Path,
            directory: &Path,
            cancellation: &ScanCancellation,
        ) -> Result<DirectoryListing, FolderScanError> {
            self.reads.lock().unwrap().push(directory.to_path_buf());
            OsDirectoryReader.read(root, directory, cancellation)
        }

        fn folder_times(
            &self,
            root: &Path,
            folders: &[PathBuf],
            cancellation: &ScanCancellation,
        ) -> Result<Vec<FolderTimes>, FolderScanError> {
            OsDirectoryReader.folder_times(root, folders, cancellation)
        }
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    write_albums(
        &root,
        &[
            "Collection/01 Originals/Album One",
            "Collection/01 Originals/Album Two",
            "Collection/02 Remasters/Album Three",
            "Box Set/CD1/FLAC",
            "Box Set/CD2",
            "Wrapper/Live One",
            "Wrapper/Live Two",
        ],
    );
    std::fs::create_dir_all(root.join("Box Set/Scans/Booklet")).unwrap();
    let reader = RecordingReader::default();

    scan_with_reader(&reader, root.clone(), |_| {}).unwrap();

    fn folders_below(root: &Path, relative: PathBuf, into: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(root.join(&relative)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                folders_below(root, relative.join(entry.file_name()), into);
            }
        }
        into.push(relative);
    }
    let mut every_folder = Vec::new();
    folders_below(&root, PathBuf::new(), &mut every_folder);
    every_folder.sort();
    let mut reads = reader.reads.into_inner().unwrap();
    reads.sort();
    assert_eq!(reads, every_folder);
}

/// A set's discs play in the order their numbers give, not the order their
/// names sort in: `CD2` before `CD10`, `disc 1` before `disc 02`, and a disc
/// whose name starts with a later year still after the one numbered before
/// it. Each disc's tracks play together, in track order.
#[test]
fn a_set_s_discs_play_in_the_order_they_number_themselves() {
    for discs in [
        &["Box Set/1936 Archive - CD 1", "Box Set/1935 Archive - CD 2"][..],
        &[
            "Box Set/CD1",
            "Box Set/CD2",
            "Box Set/CD3",
            "Box Set/CD4",
            "Box Set/CD5",
            "Box Set/CD6",
            "Box Set/CD7",
            "Box Set/CD8",
            "Box Set/CD9",
            "Box Set/CD10",
        ][..],
        &["Box Set/disc 1", "Box Set/disc 02"][..],
    ] {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Queue");
        for disc in discs {
            let disc = root.join(disc);
            std::fs::create_dir_all(&disc).unwrap();
            for track in ["01.flac", "02.flac"] {
                std::fs::write(disc.join(track), fake_flac()).unwrap();
            }
        }

        let items =
            scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

        let release = items
            .iter()
            .find_map(|item| match item {
                WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) => {
                    Some(candidate)
                }
                _ => None,
            })
            .expect("the set is one release");
        let expected_parts: Vec<String> =
            discs.iter().map(|disc| format!("{}/", disc.trim_start_matches("Box Set/"))).collect();
        assert_eq!(
            release.files.parts.iter().map(|part| part.prefix.clone()).collect::<Vec<_>>(),
            expected_parts
        );
        let expected_audio: Vec<String> = expected_parts
            .iter()
            .flat_map(|part| [format!("{part}01.flac"), format!("{part}02.flac")])
            .collect();
        assert_eq!(
            release.files.audio().map(|file| file.relative_path.clone()).collect::<Vec<_>>(),
            expected_audio
        );
        let discs_in_play_order: Vec<_> = crate::audio_layout::folder_tracks(&release.files)
            .iter()
            .map(|track| (track.disc, track.number.get()))
            .collect();
        let expected_numbers: Vec<_> = (1..=u32::try_from(discs.len()).unwrap())
            .flat_map(|disc| [(Some(disc), 1), (Some(disc), 2)])
            .collect();
        assert_eq!(discs_in_play_order, expected_numbers);
    }
}

#[test]
fn a_wrapper_of_numbered_parts_combines_unless_the_user_says_otherwise() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let wrapper = root.join("Collection").join("Release Wrapper");
    for child in ["Part 01", "Part 02"] {
        let child = wrapper.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), fake_flac()).unwrap();
    }
    std::fs::write(wrapper.join("booklet.txt"), "notes").unwrap();

    let scan = |decisions: FolderReleaseDecisions| {
        scan_projected_items_with_decisions(root.clone(), decisions)
    };

    // `Part 01` and `Part 02` number themselves 1 and 2, so with nothing
    // stored the scan reads the wrapper as one release and says so.
    let read_by_the_scan =
        scan_for_candidates_with_decisions_collect(root.clone(), FolderReleaseDecisions::default());
    assert!(read_by_the_scan.iter().any(|item| matches!(
        item,
        WalkItem::Reading {
            key,
            decision: FolderReleaseDecision::CombineAsOneRelease,
            ..
        } if key.relative_folder == bae_db::RelativeFolder::test("Collection/Release Wrapper")
    )));

    let combined = read_by_the_scan
        .iter()
        .find_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) => Some(candidate),
            _ => None,
        })
        .expect("combined wrapper is actionable");
    assert_eq!(combined.path, wrapper);
    assert_eq!(combined.scope, ReleaseFileScope::Recursive);
    assert_eq!(combined.files.release_files().count(), 3);
    // The release is its grouping, made of the two parts, each a disc run.
    assert!(combined.grouping.is_some());
    assert_eq!(
        combined.files.parts.iter().map(|part| part.prefix.as_str()).collect::<Vec<_>>(),
        ["Part 01/", "Part 02/"]
    );

    let separate = scan(readings(&[(
        "Collection/Release Wrapper",
        FolderReleaseDecision::KeepAsSeparateReleases,
        FolderReleaseDecisionAuthor::User,
    )]));
    let separate: Vec<_> = separate
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            ScanItem::Discovered(_) | ScanItem::Invalid(_) => None,
        })
        .collect();
    assert_eq!(separate.len(), 2);
    assert!(separate.iter().all(|candidate| {
        candidate.scope == ReleaseFileScope::Direct && candidate.grouping.is_none()
    }));
}

#[test]
fn labeled_disc_numbers_win_over_other_numbers_in_part_names() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let wrapper = root.join("Collection").join("Release Wrapper");
    for child in [
        "1935 Archive - CD 1 - Location A",
        "CD 2 - 1935-36 - Location A",
        "1936 Archive - CD 3 - 1937 - Location B",
        "Session 1937-40 - CD 4 - Location C",
        "Archive 1940 - CD 5 - 1941 - Location D",
    ] {
        let child = wrapper.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), fake_flac()).unwrap();
    }

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

    assert!(items.iter().any(|item| matches!(
        item,
        WalkItem::Reading {
            key,
            decision: FolderReleaseDecision::CombineAsOneRelease,
            ..
        } if key.relative_folder == bae_db::RelativeFolder::test("Collection/Release Wrapper")
    )));
    let releases: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) => Some(candidate),
            _ => None,
        })
        .collect();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].path, wrapper);
    assert_eq!(releases[0].scope, ReleaseFileScope::Recursive);
}

/// A child that does hold audio and carries no number still breaks the run:
/// the folder holds several releases, not one release's parts.
#[test]
fn a_part_with_no_number_makes_it_several_releases() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let wrapper = root.join("Collection").join("Release Wrapper");
    for child in ["Release 01", "Release 02", "Bonus"] {
        let child = wrapper.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), fake_flac()).unwrap();
    }

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());
    assert!(items.iter().any(|item| matches!(
        item,
        WalkItem::Reading {
            key,
            decision: FolderReleaseDecision::KeepAsSeparateReleases,
            ..
        } if key.relative_folder == bae_db::RelativeFolder::test("Collection/Release Wrapper")
    )));
    let releases: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) => {
                Some(candidate.relative_folder.text())
            }
            _ => None,
        })
        .collect();
    assert!(releases.contains(&"Collection/Release Wrapper/Release 01".to_string()));
    assert!(releases.contains(&"Collection/Release Wrapper/Bonus".to_string()));
}

/// A box of numbered parts inside a collection of albums: the box is one
/// release, its sibling album is another, and the collection around them is
/// not one release just because it holds both.
#[test]
fn a_numbered_box_inside_a_collection_is_one_release() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    for release in
        ["Collection/Box/Release 01", "Collection/Box/Release 02", "Collection/Release 03"]
    {
        let release = root.join(release);
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(release.join("track.flac"), fake_flac()).unwrap();
    }

    let items = scan_items(&root);
    let mut paths: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.relative_folder.text()),
            _ => None,
        })
        .collect();
    paths.sort_unstable();
    assert_eq!(paths, vec!["Collection/Box", "Collection/Release 03"]);
}
