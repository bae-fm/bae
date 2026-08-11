/// A Discogs release with rich pressing fields, used to assert that Exact
/// preserves them and Approximate clears them.
fn discogs_release_rich(title: &str, master_id: &str, tracks: &[&str]) -> DiscogsRelease {
    DiscogsRelease {
        id: synthetic_release_id(title),
        title: title.to_string(),
        year: Some(1996),
        format: vec!["CD".to_string()],
        country: Some("US".to_string()),
        label: vec!["Label Name".to_string()],
        cover_image: None,
        thumb: None,
        catno: Some("CAT-001".to_string()),
        artists: vec![DiscogsArtist {
            id: "discogs-artist-1".to_string(),
            name: "Artist Name".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: tracks
            .iter()
            .enumerate()
            .map(|(i, t)| DiscogsTrack {
                type_: "track".to_string(),
                position: format!("{}", i + 1),
                title: t.to_string(),
                duration: Some("3:00".to_string()),
                artists: vec![],
                extraartists: None,
            })
            .collect(),
        master_id: Some(master_id.to_string()),
    }
}

/// Exact import: identity row carries `source_release_id`; pressing fields
/// (year, format, label, catalog number, country) seed from the picked
/// release.
#[tokio::test]
async fn exact_import_writes_release_id_and_pressing_fields() {
    support::tracing_init();

    let release = discogs_release_rich("Album Title", "master-exact", &["Track One"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key.clone(), MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.pressing.year, Some(1996));
    assert_eq!(release.pressing.format.as_deref(), Some("CD"));
    assert_eq!(release.pressing.label.as_deref(), Some("Label Name"));
    assert_eq!(release.pressing.catalog_number.as_deref(), Some("CAT-001"));
    assert_eq!(release.pressing.country.as_deref(), Some("US"));

    // metadata_source columns point at the picked release.
    assert_eq!(
        release.metadata_source_release_id.as_deref(),
        Some(release_id_key.as_str())
    );

    let identities = f.db.get_release_identities(&release.id).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::Discogs);
    assert_eq!(
        identities[0].source_group_id,
        support::discogs_fixture_id("master-exact")
    );
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some(release_id_key.as_str())
    );
}

/// User-edit overlay applies on top of the Approximate seed: the user
/// can fill country (cleared by Approximate) and the committed value
/// reflects the edit.
#[tokio::test]
async fn approximate_import_with_user_edit_overlay() {
    support::tracing_init();

    let release = discogs_release_rich("Album Title", "master-edit", &["Track One"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let edit = ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_names: vec!["Edited Artist".to_string()],
        pressing: PressingEdit {
            // User typed JP — we expect this to land on the release row.
            country: Some("JP".to_string()),
            ..PressingEdit::blank()
        },
        tracks: vec![TrackUserEdit {
            title: "Edited Track".to_string(),
            side: 1,
            track_number: Some(1),
            artist_names: vec![],
            file: None,
        }],
    };

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(release_id_key.clone(), MetadataSource::Discogs),
            },
            user_edit: Some(edit),
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    // Other pressing fields stay NULL — user didn't fill them and
    // Approximate cleared the seed.
    assert!(release.pressing.year.is_none());
    assert!(release.pressing.format.is_none());

    let album =
        f.db.find_album_by_id(&release.album_id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(album.title, "Edited Title");

    let tracks = f.db.get_tracks_for_release(&release.id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Edited Track");

    // Identity row still NULL release_id — user_edit doesn't affect identity.
    let identities = f.db.get_release_identities(&release.id).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert!(identities[0].source_release_id.is_none());
}

// ── cross-source identity rows ──────────────────────────────────────────────
//
// When MB url-rels link to a Discogs release with a master id (or vice
// versa), the mapper emits two `release_identities` rows. The user's
// identity choice applies to BOTH rows: Exact keeps the per-source
// release IDs on each, Approximate clears both.

/// Seed a Discogs release the MB-rooted import path will resolve via MB
/// url-rels. Returns the Discogs release id.
///
/// The document is rendered as the release endpoint's own JSON and read back
/// through the production parser: it is what the import archives, and every
/// later projection replays from the archived bytes rather than from anything
/// handed over beside them. `master_id` is the fixture's own spelling, rendered
/// numerically as the endpoint numbers its ids.
fn seed_discogs_for_xref(release_id: &str, master_id: &str, title: &str) -> String {
    let rendered_master = support::discogs_fixture_id(master_id);
    bae_core::discogs::client::seed_master_cache(
        &rendered_master,
        Some(1996),
        serde_json::json!({ "id": rendered_master, "year": 1996 }).to_string(),
    );
    let raw_json = serde_json::json!({
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "title": title,
        "year": 1996,
        "country": "US",
        "master_id": rendered_master.parse::<u64>().expect("a rendered master id is numeric"),
        "labels": [{ "name": "Label Name", "catno": "CAT-001" }],
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [{
            "position": "1",
            "title": "Track One",
            "duration": "3:00",
            "type_": "track",
            "artists": [],
        }],
    })
    .to_string();
    let parsed = bae_core::discogs::client::parse_discogs_release_json(&raw_json)
        .expect("the rendered Discogs release parses");
    bae_core::discogs::client::seed_release_cache(release_id, (parsed, raw_json));
    release_id.to_string()
}

/// Seed an MB release whose url-rels carry a Discogs release URL.
/// Returns the MB release id.
fn seed_mb_with_discogs_xref(
    mb_release_id: &str,
    mb_group_id: &str,
    discogs_release_id: &str,
    title: &str,
) -> String {
    let response = MbReleaseResponse {
        id: mb_release_id.to_string(),
        title: title.to_string(),
        date: Some("1996".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(MbReleaseGroupRef {
            id: mb_group_id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![MbTrack {
                position: Some(1),
                number: Some("1".to_string()),
                title: None,
                length: None,
                recording: Some(MbRecording {
                    id: None,
                    title: Some("Track One".to_string()),
                    artist_credit: vec![],
                    relations: vec![],
                }),
                artist_credit: vec![],
            }],
        }],
        relations: vec![],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    };
    let discogs_url = Some(format!(
        "https://www.discogs.com/release/{}",
        discogs_release_id
    ));
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(mb_release_id, (response, discogs_url, raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    mb_release_id.to_string()
}

/// MB-rooted Exact import with a Discogs cross-link writes two identity
/// rows, both carrying their per-source `source_release_id`.
#[tokio::test]
async fn cross_source_exact_writes_both_release_ids() {
    support::tracing_init();

    let discogs_id = seed_discogs_for_xref("90000001", "xref-d-master-exact", "Album Title");
    // MB needs to know about the Discogs URL → release id mapping for
    // the `fetch_mb_xref` path; this test goes the other direction
    // (MB → Discogs via url-rels), but seeding both directions costs
    // nothing and keeps the cache from racing on a stale `None`.
    bae_core::musicbrainz::seed_discogs_url_lookup(&discogs_id, None);
    let mb_id = seed_mb_with_discogs_xref(
        "xref-mb-rel-exact",
        "xref-mb-group-exact",
        &discogs_id,
        "Album Title",
    );

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(mb_id.clone(), MetadataSource::MusicBrainz),
            },
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert_eq!(identities.len(), 2, "expected MB + Discogs identity rows");

    let mb = identities
        .iter()
        .find(|i| i.source == MetadataSource::MusicBrainz)
        .expect("MB identity row missing");
    assert_eq!(mb.source_group_id, "xref-mb-group-exact");
    assert_eq!(mb.source_release_id.as_deref(), Some(mb_id.as_str()));

    let discogs = identities
        .iter()
        .find(|i| i.source == MetadataSource::Discogs)
        .expect("Discogs identity row missing");
    assert_eq!(
        discogs.source_group_id,
        support::discogs_fixture_id("xref-d-master-exact")
    );
    assert_eq!(
        discogs.source_release_id.as_deref(),
        Some(discogs_id.as_str())
    );
}

/// MB-rooted Approximate import with a Discogs cross-link still writes
/// two identity rows, but both have `source_release_id = NULL` — the
/// user's "I don't claim a specific pressing" applies across sources.
#[tokio::test]
async fn cross_source_approximate_nulls_both_release_ids() {
    support::tracing_init();

    let discogs_id = seed_discogs_for_xref("90000002", "xref-d-master-approx", "Album Title");
    bae_core::musicbrainz::seed_discogs_url_lookup(&discogs_id, None);
    let mb_id = seed_mb_with_discogs_xref(
        "xref-mb-rel-approx",
        "xref-mb-group-approx",
        &discogs_id,
        "Album Title",
    );

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(mb_id.clone(), MetadataSource::MusicBrainz),
            },
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert_eq!(identities.len(), 2, "expected MB + Discogs identity rows");

    // Group ids survive — the claim is at the group level for each source.
    let mb = identities
        .iter()
        .find(|i| i.source == MetadataSource::MusicBrainz)
        .expect("MB identity row missing");
    assert_eq!(mb.source_group_id, "xref-mb-group-approx");
    assert!(
        mb.source_release_id.is_none(),
        "MB row release_id should be NULL for Approximate, got {:?}",
        mb.source_release_id,
    );

    let discogs = identities
        .iter()
        .find(|i| i.source == MetadataSource::Discogs)
        .expect("Discogs identity row missing");
    assert_eq!(
        discogs.source_group_id,
        support::discogs_fixture_id("xref-d-master-approx")
    );
    assert!(
        discogs.source_release_id.is_none(),
        "Discogs row release_id should be NULL for Approximate, got {:?}",
        discogs.source_release_id,
    );
}

/// Discogs-rooted Approximate import. MB has a back-link to this Discogs
/// release; the Discogs mapper emits both rows. Approximate clears
/// `source_release_id` on both identity rows and clears the pressing fields the
/// Discogs seed supplied on the release row, but keeps
/// `metadata_source_release_id` so a later re-projection can replay the seed.
#[tokio::test]
async fn cross_source_discogs_rooted_approximate_nulls_both_release_ids() {
    support::tracing_init();

    let discogs_id = seed_discogs_for_xref("90000003", "xref-drooted-d-master", "Album Title");
    // The Discogs commit path reaches MB via the URL-lookup cache.
    let mb_release_id = "xref-drooted-mb-rel".to_string();
    let mb_group_id = "xref-drooted-mb-group".to_string();
    bae_core::musicbrainz::seed_discogs_url_lookup(&discogs_id, Some(mb_release_id.clone()));
    let mb_response = MbReleaseResponse {
        id: mb_release_id.clone(),
        title: "Album Title".to_string(),
        date: None,
        country: None,
        barcode: None,
        artist_credit: vec![],
        release_group: Some(MbReleaseGroupRef {
            id: mb_group_id.clone(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![],
        relations: vec![],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    };
    let mb_raw_json = serde_json::to_string(&mb_response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(&mb_release_id, (mb_response, None, mb_raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        &mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(discogs_id.clone(), MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert_eq!(identities.len(), 2, "expected Discogs + MB identity rows");

    let discogs = identities
        .iter()
        .find(|i| i.source == MetadataSource::Discogs)
        .expect("Discogs identity row missing");
    assert_eq!(
        discogs.source_group_id,
        support::discogs_fixture_id("xref-drooted-d-master")
    );
    assert!(discogs.source_release_id.is_none());

    let mb = identities
        .iter()
        .find(|i| i.source == MetadataSource::MusicBrainz)
        .expect("MB identity row missing");
    assert_eq!(mb.source_group_id, mb_group_id);
    assert!(mb.source_release_id.is_none());

    // The release row: Approximate clears the pressing fields the Discogs seed
    // supplied, but keeps metadata_source_release_id so a later re-projection can
    // replay the seed.
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert!(
        release.pressing.year.is_none(),
        "year should be NULL, got {:?}",
        release.pressing.year
    );
    assert!(
        release.pressing.format.is_none(),
        "format should be NULL, got {:?}",
        release.pressing.format
    );
    assert!(
        release.pressing.label.is_none(),
        "label should be NULL, got {:?}",
        release.pressing.label
    );
    assert!(
        release.pressing.catalog_number.is_none(),
        "catalog_number should be NULL, got {:?}",
        release.pressing.catalog_number
    );
    assert!(
        release.pressing.country.is_none(),
        "country should be NULL, got {:?}",
        release.pressing.country
    );
    assert_eq!(
        release.metadata_source_release_id.as_deref(),
        Some(discogs_id.as_str()),
        "Approximate keeps metadata_source_release_id for re-projection"
    );
}

// ── "Add as Unknown" ────────────────────────────────────────────────────────

/// Unknown commit reads embedded tags, writes zero `release_identities`
/// rows, sets `metadata_source = file_tags`, leaves
/// `metadata_source_release_id = NULL`, and seeds the album / tracks
/// from what's on disk. No external source consulted.
#[tokio::test]
async fn unknown_import_seeds_from_file_tags_and_writes_no_identity() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album From Tags",
        "Artist From Tags",
        Some(2003),
        &[
            TaggedTrack {
                filename: "01.flac",
                title: "Track One",
                track_number: 1,
            },
            TaggedTrack {
                filename: "02.flac",
                title: "Track Two",
                track_number: 2,
            },
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(
        release.metadata_source,
        bae_core::db::ReleaseMetadataSource::FileTags,
    );
    assert!(
        release.metadata_source_release_id.is_none(),
        "Unknown imports must leave metadata_source_release_id NULL, got {:?}",
        release.metadata_source_release_id,
    );
    assert_eq!(release.pressing.year, Some(2003));
    assert_eq!(release.pressing.format.as_deref(), Some("FLAC"));

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert!(
        identities.is_empty(),
        "Unknown imports must write zero identity rows, got {identities:?}",
    );

    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Album From Tags");

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title, "Track One");
    assert_eq!(tracks[1].title, "Track Two");
}

#[tokio::test]
async fn unknown_preview_for_cue_matches_unknown_commit_layout() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("cue-album");
    fs::create_dir_all(&album_dir).unwrap();
    copy_cue_flac_fixture(&album_dir);

    let candidate_key = album_dir.to_string_lossy().into_owned();
    f.handle
        .add_watched_folder(candidate_key.clone())
        .await
        .unwrap();
    f.handle
        .refresh_watched_folder(candidate_key.clone())
        .await
        .unwrap();
    let preview = f
        .handle
        .preview_file_tags_for_folder(candidate_key)
        .await
        .unwrap();

    assert_eq!(preview.album_title, "Test Album");
    assert_eq!(preview.album_artist_names, vec!["Test Artist".to_string()]);
    assert_eq!(preview.pressing.year, None);
    assert_eq!(preview.pressing.format.as_deref(), Some("FLAC"));

    let preview_tracks: Vec<(String, Vec<String>)> = preview
        .tracks
        .iter()
        .map(|t| (t.title.clone(), t.artist_names.clone()))
        .collect();
    assert_eq!(
        preview_tracks,
        vec![
            (
                "Track One (Silence)".to_string(),
                vec!["Test Artist".to_string()],
            ),
            (
                "Track Two (White Noise)".to_string(),
                vec!["Test Artist".to_string()],
            ),
            (
                "Track Three (Brown Noise)".to_string(),
                vec!["Test Artist".to_string()],
            ),
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "cue".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;
    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(preview.album_title, album.title);

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(preview.pressing.year, release.pressing.year);
    assert_eq!(preview.pressing.format, release.pressing.format);

    let album_detail =
        f.db.find_album_detail(&album_id)
            .await
            .unwrap()
            .expect("album detail");
    let committed_album_artist_names: Vec<String> = album_detail
        .artists
        .iter()
        .map(|a| a.name.clone())
        .collect();
    assert_eq!(preview.album_artist_names, committed_album_artist_names);

    let release_detail =
        f.db.find_release_detail(&release_id)
            .await
            .unwrap()
            .expect("release detail");
    let committed_tracks: Vec<(String, Vec<String>)> = release_detail
        .tracks
        .iter()
        .map(|track| {
            (
                track.track.title.clone(),
                track.artists.iter().map(|a| a.name.clone()).collect(),
            )
        })
        .collect();
    assert_eq!(preview_tracks, committed_tracks);
}

/// A tagged rip whose only artwork is embedded in the audio (no folder
/// image, no remote selection) gets that embedded picture as its cover.
#[tokio::test]
async fn unknown_import_seeds_embedded_cover_when_no_folder_image() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files_with_embedded_cover(
        &album_dir,
        "Embedded Cover Album",
        "Artist",
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("embedded cover should be written when no folder/remote image exists");
    assert_eq!(
        cover.source, "embedded",
        "cover must be sourced from the embedded picture"
    );

    // The embedded picture (≤600) keeps its dimensions but the store path
    // re-encodes it to JPEG, so assert on the decoded image, not raw bytes.
    let bytes = support::read_cover_image_blob(&f.library_manager, &release_id)
        .await
        .expect("cover blob readable");
    assert_eq!(
        image::guess_format(&bytes).unwrap(),
        image::ImageFormat::Jpeg
    );
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), EMBEDDED_COVER_DIMS);

    assert_cover_row_describes_stored_bytes(&f, &release_id).await;
}

/// A folder image outranks the embedded picture: when both exist, the
/// folder artwork is the cover and the embedded picture is ignored.
#[tokio::test]
async fn unknown_import_folder_image_wins_over_embedded_cover() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files_with_embedded_cover(
        &album_dir,
        "Both Covers Album",
        "Artist",
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );
    // A folder image alongside the embedded-cover audio. No explicit
    // selection — the auto-pick must still prefer this folder image.
    let scans = album_dir.join("scans");
    fs::create_dir_all(&scans).unwrap();
    fs::write(scans.join("cover.jpg"), embedded_cover_jpeg()).unwrap();

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("a cover should be written");
    assert_eq!(
        cover.source, "local",
        "the folder image must win over the embedded picture, got source {:?}",
        cover.source
    );
}

/// Unknown imports never deduplicate against existing releases — even
/// when an identified release with the same album title is already in
/// the library, an Unknown import lands on a fresh album.
#[tokio::test]
async fn unknown_import_always_creates_a_fresh_album() {
    support::tracing_init();

    // First import: identified, lands on its own album.
    let release = discogs_release_rich("Album Title", "master-existing", &["Track One"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let identified_dir = f.temp_path().join("identified");
    fs::create_dir_all(&identified_dir).unwrap();
    generate_album_files(&identified_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "identified".to_string(),
            folder: identified_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (_, identified_album_id) = support::wait_for_import_complete(&mut rx).await;

    // Second import: Unknown, same album title in tags. Must NOT
    // attach to the identified album.
    let unknown_dir = f.temp_path().join("unknown");
    fs::create_dir_all(&unknown_dir).unwrap();
    generate_tagged_album_files(
        &unknown_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );

    let import_id2 = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id2.clone(),
            candidate_key: "unknown".to_string(),
            folder: unknown_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();
    let mut rx2 = f.handle.subscribe_import(import_id2);
    let (_, unknown_album_id) = support::wait_for_import_complete(&mut rx2).await;

    assert_ne!(
        identified_album_id, unknown_album_id,
        "Unknown import must land on a fresh album",
    );
}

/// User-edit overlay applies on top of the file-tag seed: the user
/// can override album title, artist, year, pressing fields, and track
/// titles via the editor before commit. Persisted metadata reflects
/// the edits.
#[tokio::test]
async fn unknown_import_with_user_edit_overlay() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Wrong Album Title",
        "Wrong Artist",
        Some(1999),
        &[TaggedTrack {
            filename: "01.flac",
            title: "Wrong Track Title",
            track_number: 1,
        }],
    );

    let edit = ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_names: vec!["Edited Artist".to_string()],
        pressing: PressingEdit {
            year: Some(2010),
            format: Some("FLAC".to_string()),
            label: Some("Edited Label".to_string()),
            catalog_number: Some("EDIT-1".to_string()),
            country: Some("JP".to_string()),
            barcode: Some("4943674000000".to_string()),
        },
        tracks: vec![TrackUserEdit {
            title: "Edited Track Title".to_string(),
            side: 1,
            track_number: Some(1),
            artist_names: vec![],
            file: None,
        }],
    };

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: Some(edit),
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.pressing.year, Some(2010));
    assert_eq!(release.pressing.format.as_deref(), Some("FLAC"));
    assert_eq!(release.pressing.label.as_deref(), Some("Edited Label"));
    assert_eq!(release.pressing.catalog_number.as_deref(), Some("EDIT-1"));
    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert_eq!(release.pressing.barcode.as_deref(), Some("4943674000000"));
    assert_eq!(
        release.metadata_source,
        bae_core::db::ReleaseMetadataSource::FileTags,
    );
    assert!(release.metadata_source_release_id.is_none());

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert!(
        identities.is_empty(),
        "user_edit must not introduce identity rows for Unknown",
    );

    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Edited Title");

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Edited Track Title");
}

/// Unknown commit of a rip with no usable album-level tags seeds the
/// album title from the containing folder name rather than failing —
/// the permissive file-tag projection never hard-fails on a missing
/// ALBUM tag (the editable confirmation form gates a blank title before
/// save). The artist falls back to empty for the user to fill.
#[tokio::test]
async fn unknown_import_with_no_tags_seeds_title_from_folder_name() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("Mystery Rip");
    fs::create_dir_all(&album_dir).unwrap();
    // The fixture FLAC carries no Vorbis comments — no ALBUM/ARTIST tag
    // for the projection to read, so the folder name is the album title.
    generate_album_files(&album_dir, &["01.flac", "02.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(
        album.title, "Mystery Rip",
        "untagged rip takes the folder name as its album title",
    );
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(
        release.metadata_source,
        bae_core::db::ReleaseMetadataSource::FileTags,
    );
    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 2, "both untagged files import as tracks");
}
