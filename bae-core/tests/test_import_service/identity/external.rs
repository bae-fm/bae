/// A Discogs release with rich pressing fields, used to assert that a picked
/// release seeds them.
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

/// A source-backed import: the identity row carries `source_release_id`, and
/// the pressing fields (year, format, label, catalog number, country) seed
/// from the picked release.
#[tokio::test]
async fn a_picked_release_writes_its_id_and_pressing_fields() {
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
            identity_choice: IdentityChoice::Release {
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
        identities[0].source_release_id, release_id_key
    );
}

/// The confirm form's overlay is the whole pressing block, applied on top of
/// the seed: what the user typed lands, and what they left empty stays empty.
#[tokio::test]
async fn a_user_edit_overlays_the_picked_release() {
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
            identity_choice: IdentityChoice::Release {
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
    // The overlay is the whole pressing block, so the fields it leaves empty
    // are written empty over the seed's.
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

    // The identity still names the picked pressing — a user edit is not an
    // identity change.
    let identities = f.db.get_release_identities(&release.id).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source_release_id, release_id_key);
}

// ── cross-source identity rows ──────────────────────────────────────────────
//
// When MB url-rels link to a Discogs release with a master id (or vice
// versa), the mapper emits two `release_identities` rows, each carrying its
// own source's release id.

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

/// An MB-rooted import with a Discogs cross-link writes two identity rows,
/// both carrying their per-source `source_release_id`.
#[tokio::test]
async fn cross_source_writes_both_release_ids() {
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
            identity_choice: IdentityChoice::Release {
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
    assert_eq!(mb.source_release_id, mb_id);

    let discogs = identities
        .iter()
        .find(|i| i.source == MetadataSource::Discogs)
        .expect("Discogs identity row missing");
    assert_eq!(
        discogs.source_group_id,
        support::discogs_fixture_id("xref-d-master-exact")
    );
    assert_eq!(discogs.source_release_id, discogs_id);
}
