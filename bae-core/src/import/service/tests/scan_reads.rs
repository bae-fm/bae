/// A pass that reads two disc folders as one release probes each disc's audio
/// once: the release reuses the facts the disc folders were read with.
#[tokio::test]
async fn regrouping_disc_folders_probes_each_audio_file_once() {
    let test = setup_import_service().await;
    test.service
        .library_manager
        .set_prefill_with_file_metadata(false)
        .unwrap();
    let root = test.temp.path().join("watched");
    let album = root.join("Album");
    let mut tracks = Vec::new();
    for disc in ["CD1", "CD2"] {
        std::fs::create_dir_all(album.join(disc)).unwrap();
        for n in 1..=2 {
            let path = album.join(disc).join(format!("0{n}.flac"));
            std::fs::write(&path, flac()).unwrap();
            tracks.push(path);
        }
    }
    let root = PathBuf::from(
        crate::import::watched_folder::canonical_absolute_root(&root.to_string_lossy()).unwrap(),
    );
    test.service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();

    let (scan, _) = test.scan();
    scan.rescan(&root).await.unwrap();

    let releases: Vec<usize> = test
        .service
        .library_manager
        .load_folder_scan_items(&root.to_string_lossy())
        .await
        .unwrap()
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.files.audio().count()),
            _ => None,
        })
        .collect();
    assert_eq!(releases, vec![4], "the discs read as one release");
    for track in &tracks {
        assert_eq!(
            crate::audio_codec::probe_opens_for(track),
            1,
            "{} is probed once",
            track.display()
        );
    }
}
