#[tokio::test]
async fn direct_entry_import_stores_no_metadata_provenance_or_external_identity() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("direct-entry-album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track Title.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "direct-entry-candidate".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: None,
            user_edit: Some(ReleaseUserEdit {
                album_title: "Album Title".to_string(),
                album_artist_assignments: vec![ArtistAssignment::new("Artist Name")],
                pressing: PressingEdit::blank(),
                tracks: vec![TrackUserEdit {
                    title: "Track Title".to_string(),
                    side: 1,
                    track_number: Some(1),
                    artist_assignments: TrackArtistAssignments::AlbumArtists,
                    file: None,
                }],
            }),
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.metadata_provenance, None);
    assert!(
        f.db
            .get_release_identities(&release_id)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.db.find_album_by_id(&album_id)
            .await
            .unwrap()
            .unwrap()
            .title,
        "Album Title"
    );
}
