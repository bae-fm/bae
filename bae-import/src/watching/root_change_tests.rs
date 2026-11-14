//! What a change on disk under a watched root asks to have read again.

use super::*;
use crate::scan_passes::tests::*;

#[test]
fn affected_roots_maps_changed_paths_to_their_watched_roots() {
    let root_a = PathBuf::from("/music/new rips");
    let root_b = PathBuf::from("/downloads/bandcamp");
    let roots = vec![root_a.clone(), root_b.clone()];

    // A change inside one root flags only that root.
    let changed = [Path::new("/music/new rips/Album/01.flac")];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a.clone()]);

    // Changes under both roots flag both, in roots order, deduped.
    let changed = [
        Path::new("/downloads/bandcamp/X/cover.jpg"),
        Path::new("/music/new rips/Y"),
        Path::new("/music/new rips/Z"),
    ];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a, root_b]);

    // A change outside every watched root flags nothing.
    let changed = [Path::new("/elsewhere/file")];
    assert!(affected_roots(&changed, &roots).is_empty());
}

#[test]
fn watcher_error_without_a_mapped_path_rescans_every_root() {
    let roots = vec![PathBuf::from("/music/a"), PathBuf::from("/music/b")];
    assert_eq!(roots_for_watch_error(&[], &roots), roots);
    assert_eq!(roots_for_watch_error(&[PathBuf::from("/outside")], &roots), roots);
    assert_eq!(
        roots_for_watch_error(&[PathBuf::from("/music/b/release")], &roots),
        vec![PathBuf::from("/music/b")]
    );
}

/// A change inside a folder under the root is a reading of that folder; the
/// root is read whole only where the change reaches the root's own release.
#[test]
fn a_change_reads_the_folder_it_is_in_and_the_root_only_where_it_must() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_disc(&root.join("Artist").join("Album"), 1);
    std::fs::write(root.join("notes.txt"), b"notes").unwrap();
    let folders =
        |names: &[&str]| RootChange::Folders(names.iter().map(std::ffi::OsString::from).collect());
    let change = |paths: &[PathBuf], holds: bool| {
        let paths: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        root_change(root, &paths, holds)
    };

    assert_eq!(
        change(&[root.join("Artist/Album/02.flac")], false),
        folders(&["Artist"]),
        "a file inside a folder reads that folder"
    );
    assert_eq!(
        change(&[root.join("Gone/Album/01.flac"), root.join("Gone")], false),
        folders(&["Gone"]),
        "a folder that went away is read as empty"
    );
    assert_eq!(
        change(&[root.join("Artist/.DS_Store"), root.join(".hidden")], false),
        folders(&[]),
        "a hidden entry is never listed, so it changes nothing"
    );
    assert_eq!(
        change(&[root.join("notes.txt")], false),
        folders(&[]),
        "a file beside no tracks at the root belongs to no release"
    );
    assert_eq!(
        change(&[root.join("01.flac")], false),
        RootChange::WholeRoot,
        "audio directly in the root is the root's own release"
    );
    assert_eq!(
        change(&[root.join("Artist/Album/02.flac")], true),
        RootChange::WholeRoot,
        "a root with tracks of its own takes in every folder beside them"
    );
    assert_eq!(
        change(&[root.to_path_buf()], false),
        RootChange::WholeRoot,
        "the root itself changing is the whole root"
    );
}

/// The cheap check of a network folder names what moved: a nested folder
/// that was written to, and a folder that came directly under the root.
#[tokio::test]
async fn the_network_check_names_the_folders_that_moved() {
    let fixture = DecisionFixture::new(["Disc 1", "Disc 2"], 1).await;
    let tables = fixture.test.tables().clone();
    let root = &fixture.root;
    let recorded = tables.load_folder_scan_directories(root).await.unwrap();
    assert_eq!(network_changes(root, &recorded, &bae_util::test_files::lease()), Some(Vec::new()));

    write_disc(&root.join("Artist/Album/Disc 1"), 2);
    write_disc(&root.join("New/Record"), 1);
    let mut changes = network_changes(root, &recorded, &bae_util::test_files::lease())
        .expect("the record answers");
    changes.sort();
    let changes: Vec<&Path> = changes.iter().map(PathBuf::as_path).collect();
    assert_eq!(
        root_change(root, &changes, false),
        RootChange::Folders(["Artist", "New"].into_iter().map(std::ffi::OsString::from).collect())
    );
}
