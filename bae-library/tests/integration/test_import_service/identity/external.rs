use super::*;

/// A catalog-backed import: the record carries the catalog's key for the
/// release, and the pressing fields (year, every label with its catalog
/// number, and what the pressing is) seed from the picked release.
#[tokio::test]
async fn a_picked_release_writes_its_id_and_pressing_fields() {
    bae_util::test_logs::install();

    let f = ImportFixture::new().await;

    let release = discogs_release_rich("Album Title", "master-exact", &["Track One"]);
    let release_id_key = f.fakes.seed_discogs_test_release(release);

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    f.handle
        .send_command(bae_import::test_imports::folder_import(
            &import_id,
            album_dir,
            bae_import::test_imports::discogs_release(release_id_key.clone()),
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) =
        bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await;

    let release = f.library.records().find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.pressing.year, Some(1996));
    assert_eq!(
        release.pressing.labels,
        vec![
            ReleaseLabel::of(Some("Label A"), Some("AB 100")),
            ReleaseLabel::of(Some("Label B"), Some("CL 719")),
        ]
    );
    assert_eq!(
        release.pressing.facts,
        bae_types::pressing::PressingFacts {
            area: Some(bae_types::pressing::ReleaseArea::Country(
                bae_types::pressing::Country::from_code("US").unwrap()
            )),
            media: vec![bae_types::pressing::MediaCount {
                medium: bae_types::pressing::Medium::Cd,
                count: 1,
            }],
            status: Some(bae_types::pressing::ReleaseStatus::Promotion),
            packaging: None,
            discogs_details: vec![bae_types::pressing::DiscogsDetail::Reissue],
        }
    );

    // The record the draft was read from points at the picked release.
    let records = f.library.records().get_release_records(&release.id).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].catalog(), Catalog::Discogs);
    assert_eq!(
        records[0].album_ref().expect("known parent album").key,
        bae_catalogs::test_releases::discogs_fixture_id("master-exact")
    );
    assert_eq!(records[0].key(), release_id_key);
    assert_eq!(records[0].url(), format!("https://www.discogs.com/release/{release_id_key}"));
}

/// The confirm form's overlay is the whole pressing block, applied on top of
/// the seed: what the user typed lands, and what they left empty stays empty.
#[tokio::test]
async fn a_user_edit_overlays_the_picked_release() {
    bae_util::test_logs::install();

    let f = ImportFixture::new().await;

    let release = discogs_release_rich("Album Title", "master-edit", &["Track One"]);
    let release_id_key = f.fakes.seed_discogs_test_release(release);

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let edit = ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_assignments: vec![bae_import::Credited::plain(ArtistAssignment::named(
            "Artist Edited",
        ))],
        album_year: Some(1977),
        pressing: bae_types::pressing::Pressing {
            // User typed JP — we expect this to land on the release row.
            facts: bae_types::pressing::PressingFacts {
                area: Some(bae_types::pressing::ReleaseArea::Country(
                    bae_types::pressing::Country::from_code("JP").unwrap(),
                )),
                ..Default::default()
            },
            ..bae_types::pressing::Pressing::blank()
        },
        tracks: vec![TrackUserEdit {
            title: "Edited Track".to_string(),
            artist_assignments: TrackArtistAssignments::AlbumArtists,
            file: Some(bae_import::AudioFile::Standalone { file_id: "01 Track One.flac".into() }),
        }],
    };

    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    f.handle
        .send_command(ImportCommand {
            user_edit: Some(edit),
            ..bae_import::test_imports::folder_import(
                &import_id,
                album_dir,
                bae_import::test_imports::discogs_release(release_id_key.clone()),
            )
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) =
        bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await;

    let release = f.library.records().find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(
        release.pressing.facts.area,
        Some(bae_types::pressing::ReleaseArea::Country(
            bae_types::pressing::Country::from_code("JP").unwrap()
        ))
    );
    // The overlay is the whole pressing block, so the fields it leaves empty
    // are written empty over the seed's.
    assert!(release.pressing.year.is_none());
    assert!(release.pressing.facts.media.is_empty());

    let album = f.library.records().find_album_by_id(&release.album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Edited Title");

    let tracks = f.library.records().get_tracks_for_release(&release.id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Edited Track");

    // The record still names the picked pressing — a user edit does not change
    // which catalog release describes it.
    let records = f.library.records().get_release_records(&release.id).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].key(), release_id_key);
}

/// A pick links the candidate to its release; reading the draft from the
/// files' tags afterwards replaces the draft and not the link. The import
/// writes the tags' values under the linked release's record, which no
/// longer reads the draft.
#[tokio::test]
async fn a_linked_draft_read_from_the_tags_imports_under_the_link() {
    bae_util::test_logs::install();

    let f = ImportFixture::new().await;

    let release = discogs_release_rich("Album Title", "master-tags", &["Track One"]);
    let release_id_key = f.fakes.seed_discogs_test_release(release);

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Tagged Title",
        "Tagged Artist",
        None,
        &[TaggedTrack { filename: "01.flac", title: "Tagged Track", track_number: 1 }],
    );

    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    f.handle
        .send_command(ImportCommand {
            read_file_tags: true,
            ..bae_import::test_imports::folder_import(
                &import_id,
                album_dir,
                bae_import::test_imports::discogs_release(release_id_key.clone()),
            )
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) =
        bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await;

    let release = f.library.records().find_release_by_id(&release_id).await.unwrap().unwrap();
    let album = f.library.records().find_album_by_id(&release.album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Tagged Title");
    let tracks = f.library.records().get_tracks_for_release(&release.id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Tagged Track");

    let records = f.library.records().get_release_records(&release.id).await.unwrap();
    assert_eq!(records.len(), 1, "the link's release is recorded: {records:?}");
    assert_eq!(records[0].key(), release_id_key);
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
fn seed_discogs_for_xref(
    f: &ImportFixture,
    release_id: &str,
    master_id: &str,
    title: &str,
) -> String {
    let rendered_master = bae_catalogs::test_releases::discogs_fixture_id(master_id);
    f.fakes.seed_discogs_artist_image("1", None);
    f.fakes.seed_discogs_master(
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
    bae_catalogs::discogs::documents::parse_discogs_release_json(&raw_json)
        .expect("the rendered Discogs release parses");
    f.fakes.seed_discogs_release(release_id, raw_json);
    // MusicBrainz is asked about the release's address and its master's in
    // one request; neither is linked from anything there.
    f.fakes.seed_discogs_page_lookup(&[
        (bae_catalogs::musicbrainz::DiscogsPage::Release(release_id.to_string()), None),
        (bae_catalogs::musicbrainz::DiscogsPage::Master(rendered_master), None),
    ]);
    release_id.to_string()
}

/// Seed an MB release whose url-rels carry a Discogs release URL.
/// Returns the MB release id.
fn seed_mb_with_discogs_xref(
    f: &ImportFixture,
    mb_release_id: &str,
    mb_group_id: &str,
    discogs_release_id: &str,
    title: &str,
) -> String {
    let mut response = bae_catalogs::test_releases::mb_release(mb_release_id, mb_group_id, title);
    response.relations = vec![MbRelation {
        url: Some(MbUrlResource {
            resource: Some(format!("https://www.discogs.com/release/{discogs_release_id}")),
        }),
        ..MbRelation::default()
    }];
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    f.fakes.seed_musicbrainz_release(mb_release_id, raw_json);
    f.fakes.seed_musicbrainz_release_group(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    mb_release_id.to_string()
}

/// An MB-rooted import with a Discogs cross-link writes two records, each
/// carrying its own catalog's key.
#[tokio::test]
async fn a_cross_link_writes_both_catalogs_records() {
    bae_util::test_logs::install();

    let f = ImportFixture::new().await;

    let discogs_id = seed_discogs_for_xref(&f, "90000001", "xref-d-master-exact", "Album Title");
    let mb_id = seed_mb_with_discogs_xref(
        &f,
        "xref-mb-rel-exact",
        "xref-mb-group-exact",
        &discogs_id,
        "Album Title",
    );

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    f.handle
        .send_command(bae_import::test_imports::folder_import(
            &import_id,
            album_dir,
            bae_import::test_imports::DraftSource::Pick(bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    LookupCatalog::MusicBrainz,
                    mb_id.clone(),
                ),
                partners: vec![],
            }),
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) =
        bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await;

    let records = f.library.records().get_release_records(&release_id).await.unwrap();
    assert_eq!(records.len(), 2, "expected MB + Discogs records");

    let mb = records
        .iter()
        .find(|record| record.catalog() == Catalog::MusicBrainz)
        .expect("MusicBrainz record missing");
    assert_eq!(mb.album_ref().expect("known parent album").key, "xref-mb-group-exact");
    assert_eq!(mb.key(), mb_id);

    let discogs = records
        .iter()
        .find(|record| record.catalog() == Catalog::Discogs)
        .expect("Discogs record missing");
    assert_eq!(
        discogs.album_ref().expect("known parent album").key,
        bae_catalogs::test_releases::discogs_fixture_id("xref-d-master-exact")
    );
    assert_eq!(discogs.key(), discogs_id);
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
    bae_util::test_logs::install();

    let f = ImportFixture::new().await;

    let discogs_id = seed_discogs_for_xref(&f, "90000101", "partner-d-master", "Album Title");
    let mb_id = seed_mb_without_xref(&f, "partner-mb-rel", "partner-mb-group", "Album Title");

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    f.handle
        .send_command(bae_import::test_imports::folder_import(
            &import_id,
            album_dir,
            bae_import::test_imports::DraftSource::Pick(bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    LookupCatalog::MusicBrainz,
                    mb_id.clone(),
                ),
                partners: vec![bae_catalogs::records::LookupRef::new(
                    LookupCatalog::Discogs,
                    discogs_id.clone(),
                )],
            }),
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) =
        bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await;

    let records = f.library.records().get_release_records(&release_id).await.unwrap();
    assert_eq!(records.len(), 2, "expected MB + Discogs records");

    let mb = records
        .iter()
        .find(|record| record.catalog() == Catalog::MusicBrainz)
        .expect("MusicBrainz record missing");
    assert_eq!(mb.album_ref().expect("known parent album").key, "partner-mb-group");
    assert_eq!(mb.key(), mb_id);

    let discogs = records
        .iter()
        .find(|record| record.catalog() == Catalog::Discogs)
        .expect("Discogs record missing");
    assert_eq!(
        discogs.album_ref().expect("known parent album").key,
        bae_catalogs::test_releases::discogs_fixture_id("partner-d-master")
    );
    assert_eq!(discogs.key(), discogs_id);
}

/// The partner is what the person picked, so it replaces the Discogs record
/// the MusicBrainz document's url-rels merely suggested: one Discogs row, and
/// it names the picked pressing rather than the cross-referenced one.
#[tokio::test]
async fn a_partner_replaces_an_inferred_record_of_the_same_catalog() {
    bae_util::test_logs::install();

    let f = ImportFixture::new().await;

    let inferred_id = seed_discogs_for_xref(&f, "90000201", "inferred-d-master", "Album Title");
    let picked_id = seed_discogs_for_xref(&f, "90000202", "picked-d-master", "Album Title");
    let mb_id = seed_mb_with_discogs_xref(
        &f,
        "partner-override-mb-rel",
        "partner-override-mb-group",
        &inferred_id,
        "Album Title",
    );

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    f.handle
        .send_command(bae_import::test_imports::folder_import(
            &import_id,
            album_dir,
            bae_import::test_imports::DraftSource::Pick(bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    LookupCatalog::MusicBrainz,
                    mb_id.clone(),
                ),
                partners: vec![bae_catalogs::records::LookupRef::new(
                    LookupCatalog::Discogs,
                    picked_id.clone(),
                )],
            }),
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) =
        bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await;

    let records = f.library.records().get_release_records(&release_id).await.unwrap();
    let discogs: Vec<_> =
        records.iter().filter(|record| record.catalog() == Catalog::Discogs).collect();
    assert_eq!(discogs.len(), 1, "one row per catalog");
    assert_eq!(discogs[0].key(), picked_id, "the picked release outranks the cross-referenced one");
    assert_eq!(
        discogs[0].album_ref().expect("known parent album").key,
        bae_catalogs::test_releases::discogs_fixture_id("picked-d-master")
    );
}

/// Seed an MB release with no Discogs url-rel. Returns the MB release id.
fn seed_mb_without_xref(
    f: &ImportFixture,
    mb_release_id: &str,
    mb_group_id: &str,
    title: &str,
) -> String {
    f.fakes.seed_musicbrainz_response(
        bae_catalogs::test_releases::mb_release(mb_release_id, mb_group_id, title),
        mb_group_id,
    )
}
