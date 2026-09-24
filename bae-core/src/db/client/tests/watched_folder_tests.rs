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
