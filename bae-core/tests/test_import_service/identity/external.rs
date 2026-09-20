/// A Discogs release with rich pressing fields, used to assert that a picked
/// release seeds them.
fn discogs_release_rich(title: &str, master_id: &str, tracks: &[&str]) -> DiscogsRelease {
    let tracks: Vec<(&str, &str)> = tracks.iter().map(|title| (*title, "3:00")).collect();
    DiscogsRelease {
        year: Some(1996),
        format: vec!["CD".to_string()],
        label: vec!["Label Name".to_string()],
        catno: Some("CAT-001".to_string()),
        master_id: Some(master_id.to_string()),
        ..support::discogs_test_release(&synthetic_release_id(title), title, &tracks)
    }
}

/// A catalog-backed import: the record carries the catalog's key for the
/// release, and the pressing fields (year, format, label, catalog number,
/// country) seed from the picked release.
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
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            support::discogs_release(release_id_key.clone()),
        ))
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

    // The record the draft was read from points at the picked release.
    assert!(!release.draft_from_tags);
    let records = f.db.get_release_records(&release.id).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].catalog, Catalog::Discogs);
    assert!(records[0].reads_draft);
    assert_eq!(
        records[0].group_key,
        support::discogs_fixture_id("master-exact")
    );
    assert_eq!(records[0].key, release_id_key);
    assert_eq!(
        records[0].url,
        format!("https://www.discogs.com/release/{release_id_key}")
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
        album_artist_assignments: vec![ArtistAssignment::new("Artist Edited")],
        album_year: Some(1977),
        pressing: PressingEdit {
            // User typed JP — we expect this to land on the release row.
            country: Some("JP".to_string()),
            ..PressingEdit::blank()
        },
        tracks: vec![TrackUserEdit {
            title: "Edited Track".to_string(),
            side: Some(1),
            track_number: Some(1),
            artist_assignments: TrackArtistAssignments::AlbumArtists,
            file: Some(bae_core::import::AudioFile::Standalone {
                file_id: "01 Track One.flac".into(),
            }),
        }],
    };

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            user_edit: Some(edit),
            ..support::folder_import(
                &import_id,
                album_dir,
                support::discogs_release(release_id_key.clone()),
            )
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

    // The record still names the picked pressing — a user edit does not change
    // which catalog release describes it.
    let records = f.db.get_release_records(&release.id).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].key, release_id_key);
}

// ── cross-catalog records ───────────────────────────────────────────────────
//
// When MB url-rels link to a Discogs release with a master id (or vice
// versa), the pick commits two `release_records` rows, each carrying its own
// catalog's release key.

/// Seed a Discogs release the MB-rooted import path will resolve via MB
/// url-rels. Returns the Discogs release id.
///
/// The document is rendered as the release endpoint's own JSON and read back
/// through the production parser: it is what the import archives, and every
/// later projection replays from the archived bytes rather than from anything
/// handed over beside them. `master_id` is the fixture's own spelling, rendered
/// numerically as the endpoint numbers its ids.
fn seed_discogs_for_xref(release_id: &str, master_id: &str, title: &str) -> String {
    support::point_discogs_at_dead_port();
    let rendered_master = support::discogs_fixture_id(master_id);
    bae_core::discogs::client::seed_artist_image_response("1", None);
    bae_core::discogs::client::seed_master_cache(
        &rendered_master,
        serde_json::json!({ "id": rendered_master.parse::<u64>().expect("a rendered master id is numeric"), "year": 1996 }).to_string(),
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
    bae_core::discogs::client::parse_discogs_release_json(&raw_json)
        .expect("the rendered Discogs release parses");
    bae_core::discogs::client::seed_release_cache(release_id, raw_json);
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
    let mut response = support::mb_release(mb_release_id, mb_group_id, title);
    response.relations = vec![MbRelation {
        url: Some(MbUrlResource {
            resource: Some(format!(
                "https://www.discogs.com/release/{discogs_release_id}"
            )),
        }),
        ..MbRelation::default()
    }];
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(mb_release_id, raw_json);
    bae_core::musicbrainz::seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    mb_release_id.to_string()
}

/// An MB-rooted import with a Discogs cross-link writes two records, each
/// carrying its own catalog's key.
#[tokio::test]
async fn a_cross_link_writes_both_catalogs_records() {
    support::tracing_init();

    let discogs_id = seed_discogs_for_xref("90000001", "xref-d-master-exact", "Album Title");
    // MB needs to know about the Discogs URL → release id mapping for
    // the `fetch_mb_xref` path; this test goes the other direction
    // (MB → Discogs via url-rels); the reverse cache must not contain a stale
    // answer from another test.
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
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            MetadataProvenance::ExternalRelease {
                record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, mb_id.clone()),
                partners: vec![],
            },
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let records = f.db.get_release_records(&release_id).await.unwrap();
    assert_eq!(records.len(), 2, "expected MB + Discogs records");

    let mb = records
        .iter()
        .find(|record| record.catalog == Catalog::MusicBrainz)
        .expect("MusicBrainz record missing");
    assert_eq!(mb.group_key, "xref-mb-group-exact");
    assert_eq!(mb.key, mb_id);

    let discogs = records
        .iter()
        .find(|record| record.catalog == Catalog::Discogs)
        .expect("Discogs record missing");
    assert_eq!(
        discogs.group_key,
        support::discogs_fixture_id("xref-d-master-exact")
    );
    assert_eq!(discogs.key, discogs_id);
}

// ── a pick's partners ───────────────────────────────────────────────────────
//
// Find online pairs a MusicBrainz release and a Discogs release into one
// pressing row. Picking that row claims both: the draft is read from the
// primary, and each partner contributes its own catalog's record.

/// A pick carrying a partner writes one record per catalog, each naming the
/// release that catalog lists — even though neither document cross-links the
/// other.
#[tokio::test]
async fn a_pick_with_a_partner_writes_both_records() {
    support::tracing_init();

    let discogs_id = seed_discogs_for_xref("90000101", "partner-d-master", "Album Title");
    bae_core::musicbrainz::seed_discogs_url_lookup(&discogs_id, None);
    let mb_id = seed_mb_without_xref("partner-mb-rel", "partner-mb-group", "Album Title");

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            MetadataProvenance::ExternalRelease {
                record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, mb_id.clone()),
                partners: vec![bae_core::import::MetadataRef::new(
                    Catalog::Discogs,
                    discogs_id.clone(),
                )],
            },
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let records = f.db.get_release_records(&release_id).await.unwrap();
    assert_eq!(records.len(), 2, "expected MB + Discogs records");

    let mb = records
        .iter()
        .find(|record| record.catalog == Catalog::MusicBrainz)
        .expect("MusicBrainz record missing");
    assert_eq!(mb.group_key, "partner-mb-group");
    assert_eq!(mb.key, mb_id);
    assert!(mb.reads_draft, "the draft was read from the primary");

    let discogs = records
        .iter()
        .find(|record| record.catalog == Catalog::Discogs)
        .expect("Discogs record missing");
    assert_eq!(
        discogs.group_key,
        support::discogs_fixture_id("partner-d-master")
    );
    assert_eq!(discogs.key, discogs_id);
    assert!(!discogs.reads_draft);
}

/// The partner is what the person picked, so it replaces the Discogs record
/// the MusicBrainz document's url-rels merely suggested: one Discogs row, and
/// it names the picked pressing rather than the cross-referenced one.
#[tokio::test]
async fn a_partner_replaces_an_inferred_record_of_the_same_catalog() {
    support::tracing_init();

    let inferred_id = seed_discogs_for_xref("90000201", "inferred-d-master", "Album Title");
    let picked_id = seed_discogs_for_xref("90000202", "picked-d-master", "Album Title");
    bae_core::musicbrainz::seed_discogs_url_lookup(&inferred_id, None);
    bae_core::musicbrainz::seed_discogs_url_lookup(&picked_id, None);
    let mb_id = seed_mb_with_discogs_xref(
        "partner-override-mb-rel",
        "partner-override-mb-group",
        &inferred_id,
        "Album Title",
    );

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            MetadataProvenance::ExternalRelease {
                record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, mb_id.clone()),
                partners: vec![bae_core::import::MetadataRef::new(
                    Catalog::Discogs,
                    picked_id.clone(),
                )],
            },
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let records = f.db.get_release_records(&release_id).await.unwrap();
    let discogs: Vec<_> = records
        .iter()
        .filter(|record| record.catalog == Catalog::Discogs)
        .collect();
    assert_eq!(discogs.len(), 1, "one row per catalog");
    assert_eq!(
        discogs[0].key, picked_id,
        "the picked release outranks the cross-referenced one"
    );
    assert_eq!(
        discogs[0].group_key,
        support::discogs_fixture_id("picked-d-master")
    );
}

/// Seed an MB release with no Discogs url-rel. Returns the MB release id.
fn seed_mb_without_xref(mb_release_id: &str, mb_group_id: &str, title: &str) -> String {
    support::seed_mb_release(
        support::mb_release(mb_release_id, mb_group_id, title),
        mb_group_id,
    )
}
