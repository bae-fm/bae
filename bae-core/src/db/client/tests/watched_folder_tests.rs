use crate::import::watched_folder::host_root;

/// Two overlapping folders added at once: the overlap check and the insert
/// are one decision, so exactly one of them is watched and the other is
/// refused. With the check read ahead of the write, both saw an empty list and
/// both were stored — a store `load_watched_import_folders` then refuses to
/// read.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn overlapping_folders_added_at_once_keep_only_one() {
    for _ in 0..20 {
        let (db, _tmp) = super::temp_db().await;
        let outer = host_root("/music");
        let inner = host_root("/music/inner");
        let (first, second) = tokio::join!(
            db.add_watched_import_folder(&outer),
            db.add_watched_import_folder(&inner),
        );

        let added = [&first, &second]
            .iter()
            .filter(|result| matches!(result, Ok(true)))
            .count();
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
        db.load_watched_import_folders()
            .await
            .expect("the stored folders do not overlap");
    }
}

/// Whether a folder is watched is the answer for that folder alone: the
/// watched folder is, a folder inside it or beside it is not.
#[tokio::test]
async fn a_folder_is_watched_only_by_its_own_key() {
    let (db, _tmp) = super::temp_db().await;
    let watched = host_root("/music");
    db.add_watched_import_folder(&watched).await.unwrap();

    assert!(db.is_watched_import_folder(&watched).await.unwrap());
    assert!(!db
        .is_watched_import_folder(&host_root("/music/inner"))
        .await
        .unwrap());
    assert!(!db
        .is_watched_import_folder(&host_root("/musical"))
        .await
        .unwrap());
}

/// The watched folder above a folder is the one holding it, never the folder
/// itself, and the watched folders below one are those inside it, in their
/// order, never one beside it whose name only begins with its own.
#[tokio::test]
async fn the_watched_folders_above_and_below_a_folder_are_read_by_path() {
    let (db, _tmp) = super::temp_db().await;
    for folder in ["/music/b", "/music/a", "/musical"] {
        db.add_watched_import_folder(&host_root(folder))
            .await
            .unwrap();
    }

    assert_eq!(
        db.watched_import_folder_above(&host_root("/music/a/disc"))
            .await
            .unwrap(),
        Some(host_root("/music/a"))
    );
    assert_eq!(
        db.watched_import_folder_above(&host_root("/music/a"))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        db.watched_import_folders_below(&host_root("/music"))
            .await
            .unwrap(),
        [host_root("/music/b"), host_root("/music/a")]
    );
    assert_eq!(
        db.watched_import_folders_below(&host_root("/music/a"))
            .await
            .unwrap(),
        Vec::<String>::new()
    );
}
