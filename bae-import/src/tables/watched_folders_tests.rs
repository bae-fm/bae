//! The watched folders as the store keeps them: each by its own key, in the
//! order they were added, never overlapping.

use crate::tables::watched_path::host_root;
use bae_store::test_fixtures::empty_db;
use bae_store::test_fixtures::temp_db;
use std::path::PathBuf;

/// Two overlapping folders added at once: the overlap check and the insert
/// are one decision, so exactly one of them is watched and the other is
/// refused. With the check read ahead of the write, both saw an empty list and
/// both were stored.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn overlapping_folders_added_at_once_keep_only_one() {
    for _ in 0..20 {
        let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
        let tables = crate::tables::ImportTables::new(db.clone());
        let outer = host_root("/music");
        let inner = host_root("/music/inner");
        let (first, second) = tokio::join!(
            tables.add_watched_import_folder(&outer),
            tables.add_watched_import_folder(&inner),
        );

        let added = [&first, &second].iter().filter(|result| matches!(result, Ok(Some(_)))).count();
        let refused = [&first, &second]
            .iter()
            .filter(|result| {
                matches!(result, Err(error) if error.to_string().contains("cannot overlap"))
            })
            .count();
        assert_eq!(
            (added, refused),
            (1, 1),
            "one folder is watched and the overlapping one refused: {first:?} / {second:?}"
        );
        assert_eq!(tables.load_watched_import_folders().await.unwrap().len(), 1);
    }
}

/// Whether a folder is watched is the answer for that folder alone: the
/// watched folder is, a folder inside it or beside it is not.
#[tokio::test]
async fn a_folder_is_watched_only_by_its_own_key() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let watched = host_root("/music");
    tables.add_watched_import_folder(&watched).await.unwrap();

    assert!(tables.is_watched_import_folder(&watched).await.unwrap());
    assert!(!tables.is_watched_import_folder(&host_root("/music/inner")).await.unwrap());
    assert!(!tables.is_watched_import_folder(&host_root("/musical")).await.unwrap());
}

/// The watched folder above a folder is the one holding it, never the folder
/// itself, and the watched folders below one are those inside it, in their
/// order, never one beside it whose name only begins with its own.
#[tokio::test]
async fn the_watched_folders_above_and_below_a_folder_are_read_by_path() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    for folder in ["/music/b", "/music/a", "/musical"] {
        tables.add_watched_import_folder(&host_root(folder)).await.unwrap();
    }

    assert_eq!(
        tables.watched_import_folder_above(&host_root("/music/a/disc")).await.unwrap(),
        Some(host_root("/music/a"))
    );
    assert_eq!(tables.watched_import_folder_above(&host_root("/music/a")).await.unwrap(), None);
    assert_eq!(
        tables.watched_import_folders_below(&host_root("/music")).await.unwrap(),
        [host_root("/music/b"), host_root("/music/a")]
    );
    assert_eq!(
        tables.watched_import_folders_below(&host_root("/music/a")).await.unwrap(),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn watched_root_overlap_uses_paths_not_sql_patterns() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    for root in ["/music/100%", "/music/name_value"] {
        assert!(tables.add_watched_import_folder(&host_root(root)).await.unwrap().is_some());
    }
    for child in ["/music/100%/child", "/music/name_value/child"] {
        let error = tables.add_watched_import_folder(&host_root(child)).await.unwrap_err();
        assert!(error.to_string().contains("cannot overlap"), "{child}");
    }
}

#[tokio::test]
async fn watched_root_order_survives_middle_removal_and_later_add() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    for root in ["/one", "/two", "/three"] {
        tables.add_watched_import_folder(&host_root(root)).await.unwrap();
    }
    tables.remove_watched_import_folders(vec![host_root("/two")], None).await.unwrap();
    let added = tables.add_watched_import_folder(&host_root("/four")).await.unwrap();
    let paths: Vec<_> = tables
        .load_watched_import_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(paths, vec![host_root("/one"), host_root("/three"), host_root("/four")]);
    assert_eq!(
        added
            .expect("/four was not watched")
            .into_iter()
            .map(|folder| folder.path)
            .collect::<Vec<_>>(),
        paths,
        "the add answers the folders its write left watched"
    );
}

/// Every spelling of a watched folder is the same row, keyed by the canonical
/// spelling.
#[tokio::test]
async fn watched_root_spellings_settle_on_one_row() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let canonical = host_root("/music/rips");
    assert!(tables.add_watched_import_folder(&canonical).await.unwrap().is_some());

    // The drive-lettered, forward-slashed form a `bae://import` link and a
    // `file://` folder drop give on Windows.
    #[cfg(windows)]
    const URL_SPELLINGS: &[&str] = &["C:/music/rips"];
    #[cfg(not(windows))]
    const URL_SPELLINGS: &[&str] = &[];

    let spellings =
        [host_root("/music/rips/"), host_root("/music//rips"), host_root("/music/./rips")];

    for spelling in
        spellings.iter().map(PathBuf::as_path).chain(URL_SPELLINGS.iter().map(std::path::Path::new))
    {
        assert!(
            tables.add_watched_import_folder(spelling).await.unwrap().is_none(),
            "{} is the folder already watched, not a new one",
            spelling.display()
        );
    }
    let paths: Vec<_> = tables
        .load_watched_import_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(paths, vec![canonical]);
}

/// `..` is refused: resolving it without the filesystem is wrong across a
/// symlink.
#[tokio::test]
async fn watched_root_rejects_a_path_climbing_out_of_itself() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let path = host_root("/music/../rips");
    assert!(tables.add_watched_import_folder(&path).await.is_err(), "{}", path.display());
}

/// The add refuses a folder inside a watched one, and a folder holding one,
/// naming the watched folder it overlaps, and stores neither.
#[tokio::test]
async fn the_add_refuses_a_folder_that_overlaps_a_watched_one() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let music = host_root("/music");
    let artist = host_root("/music/artist");
    let album = host_root("/music/artist/album");
    tables.add_watched_import_folder(&artist).await.unwrap();

    for (folder, watched) in [(&album, &artist), (&music, &artist), (&host_root("/"), &artist)] {
        let error = tables.add_watched_import_folder(folder).await.unwrap_err();
        assert!(
            matches!(
                &error,
                crate::ImportError::WatchedFolders(
                    crate::tables::WatchedFolderRefusal::Overlap { folder: refused, watched: named }
                ) if refused == folder && named == watched
            ),
            "{}: {error}",
            folder.display()
        );
    }
    let watched: Vec<_> = tables
        .load_watched_import_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(watched, [artist]);
}

/// A folder beside a watched one, whose name only begins with the watched
/// folder's name, does not overlap it.
#[tokio::test]
async fn a_folder_whose_name_begins_with_a_watched_ones_does_not_overlap_it() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    for folder in ["/music", "/musical", "/music-old"] {
        assert!(
            tables.add_watched_import_folder(&host_root(folder)).await.unwrap().is_some(),
            "{folder}"
        );
    }
}

/// A stored watched folder not in its canonical spelling is damage no write
/// stores, and fails the read.
#[tokio::test]
async fn a_stored_root_not_in_its_canonical_spelling_fails_to_load() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.plant_uncanonical_watched_folder_for_test(&host_root("/music")).await.unwrap();
    let error = tables.load_watched_import_folders().await.unwrap_err();
    assert!(matches!(error, bae_store::DbError::Unreadable { .. }), "{error}");
}
