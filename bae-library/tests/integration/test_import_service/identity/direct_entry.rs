use super::*;

#[tokio::test]
async fn direct_entry_import_records_no_catalog_and_reads_no_tags() {
    bae_util::test_logs::install();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("direct-entry-album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track Title.flac"]);

    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: bae_import::CandidateKey::from(b"direct-entry-candidate".to_vec()),
            source: bae_import::release_candidate::CandidateSource {
                path: album_dir,
                scope: bae_import::ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            selected_cover: None,
            destination: ImportDestination::ThisDevice,
            pick: None,
            read_file_tags: false,
            user_edit: Some(ReleaseUserEdit {
                album_title: "Album Title".to_string(),
                album_artist_assignments: vec![bae_import::Credited::plain(
                    ArtistAssignment::named("Artist Name"),
                )],
                album_year: None,
                pressing: bae_types::pressing::Pressing::blank(),
                tracks: vec![TrackUserEdit {
                    title: "Track Title".to_string(),
                    artist_assignments: TrackArtistAssignments::AlbumArtists,
                    file: Some(bae_import::AudioFile::Standalone {
                        file_id: "01 Track Title.flac".into(),
                    }),
                }],
            }),
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) =
        bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await;

    assert!(f.library.records().get_release_records(&release_id).await.unwrap().is_empty());
    assert_eq!(
        f.library.records().find_album_by_id(&album_id).await.unwrap().unwrap().title,
        "Album Title"
    );
}
