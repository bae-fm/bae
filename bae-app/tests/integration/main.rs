//! bae-app's integration tests, one module per area, built as one test
//! binary: the app's bootstrap opening a library, and the services it starts
//! over one.
//!
//! Run one area with a filter on its module: `scripts/cargo.sh test -p
//! bae-app --features test-utils --test integration -- test_startup_sync::`.

mod storage_list;
mod test_bootstrap_fixture;
mod test_bootstrap_pointer;
mod test_close_open_files;
mod test_discogs_switch;
mod test_import_live_query_reruns;
mod test_import_pane;
mod test_import_selection;
mod test_live_reads;
mod test_open_progress;
mod test_scan_cost;
mod test_screen_cost;
mod test_startup_sync;
mod test_storage_state_machine;
mod test_transfer;

/// Where this release's files are, and whether this device pins them.
pub(crate) async fn release_storage(
    db: &bae_store::Store,
    release_id: &str,
) -> bae_views::album_detail::ReleaseStorageState {
    let summary = bae_views::LibraryViews::new(db.clone())
        .find_release_detail(release_id)
        .await
        .unwrap()
        .unwrap()
        .summary;
    summary.storage_state
}

/// Insert an artist + album + a Local release, write its originals under
/// `source_dir`, register the `DbFile` rows and bae's device-local paths. No uploads
/// queued — the test drives the transition itself. Returns (release_id, files).
pub(crate) async fn create_local_release(
    mgr: &bae_library::LibraryManager,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, Vec<u8>)>) {
    create_local_release_in_folder(
        mgr,
        bae_library::test_libraries::TEST_RELEASE_FOLDER,
        source_dir,
        files,
    )
    .await
}

/// [`create_local_release`] for a release imported from the folder
/// `folder_name`.
pub(crate) async fn create_local_release_in_folder(
    mgr: &bae_library::LibraryManager,
    folder_name: &str,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, Vec<u8>)>) {
    let now = chrono::Utc::now();
    // Shared across every release this helper seeds, so only the first call
    // inserts it.
    let artist_id = bae_util::test_ids::test_uuid("test-storage-artist");
    if mgr.get_artist_by_id(&artist_id).await.unwrap().is_none() {
        mgr.insert_artist(&bae_records::DbArtist {
            id: artist_id.to_string(),
            name: "Artist Name".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        })
        .await
        .unwrap();
    }
    let album = bae_records::DbAlbum {
        title: "Album Title".to_string(),
        ..bae_library::test_libraries::test_album()
    };
    let release = bae_records::DbRelease {
        source_folder_name: Some(folder_name.to_string()),
        ..bae_library::test_libraries::test_release(&album.id)
    };
    let release_id = release.id.clone();
    let credit = bae_records::DbAlbumArtist::new_test(&album.id, &artist_id);
    mgr.insert_album_with_release_and_tracks(&album, &[credit], &release, &[], &[], &[])
        .await
        .unwrap();

    tokio::fs::create_dir_all(source_dir).await.unwrap();
    let mut result = Vec::new();
    for (name, data) in files {
        let path = source_dir.join(name);
        tokio::fs::write(&path, data).await.unwrap();
        let db_file = bae_records::NewReleaseFile::new(
            &release_id,
            name,
            bae_util::content_type::ContentType::Flac,
            bae_records::ReleaseFileRole::Other,
            coven::IdSource::new_id(&coven::UuidIds).to_string(),
            now,
        );
        mgr.add_external_file_for_test(&db_file, &path).await.unwrap();
        result.push((name.to_string(), data.to_vec()));
    }
    (release_id, result)
}
