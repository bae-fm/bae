use super::*;

#[test]
fn artists_replaced_by_edits_are_not_reconciled() {
    let now = bae_util::timestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap();
    let mut artists = vec![
        bae_records::DbArtist {
            id: "artist-referenced".into(),
            name: "Artist Name".into(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        },
        bae_records::DbArtist {
            id: "artist-replaced".into(),
            name: "Replaced Artist".into(),
            sort_name: None,
            discogs_artist_id: Some("discogs-replaced".into()),
            musicbrainz_artist_id: None,
            created_at: now,
        },
    ];
    let album = bae_records::DbAlbum {
        id: "album-1".into(),
        title: "Album Title".into(),
        year: None,
        is_compilation: false,
        created_at: now,
    };
    let credits =
        [bae_records::DbAlbumArtist::new(&coven::UuidIds, &album.id, "artist-referenced", 0, now)];

    retain_referenced_artists(&mut artists, Some((&album, &credits)), &[], &[], &[], &[]);

    assert_eq!(artists.len(), 1);
    assert_eq!(artists[0].id, "artist-referenced");
}

#[test]
fn existing_album_does_not_retain_its_transient_album_artist() {
    let now = bae_util::timestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap();
    let mut artists = vec![bae_records::DbArtist {
        id: "artist-album-only".into(),
        name: "Album Artist".into(),
        sort_name: None,
        discogs_artist_id: Some("discogs-album-only".into()),
        musicbrainz_artist_id: None,
        created_at: now,
    }];
    retain_referenced_artists(&mut artists, None, &[], &[], &[], &[]);

    assert!(
        artists.is_empty(),
        "an existing album discards the parsed album and its artist credits"
    );
}

/// The rows of one MusicBrainz pressing of the release group `mb-group`, and
/// the record that names that group.
fn pressing_of_group() -> (bae_records::ReleaseRows, Vec<bae_catalogs::records::ReleaseRecord>) {
    use bae_catalogs::catalog::Catalog;
    use bae_catalogs::records::{MetadataRef, ReleaseRecord};
    let response = bae_catalogs::test_releases::mb_release("mb-release", "mb-group", "Album Title");
    let rows = crate::reading_draft::committed_rows(
        bae_catalogs::test_releases::mb_release_values(&response),
    );
    let records = vec![ReleaseRecord::new(
        &MetadataRef::new(Catalog::MusicBrainz, "mb-release".to_string()),
        Some("mb-group".to_string()),
    )];
    (rows, records)
}

/// Put an album of one release and one track in the library under `album_id`.
async fn seed_album_with_id(test: &crate::service::tests::TestService, album_id: &str) {
    let records = bae_records::test_fixtures::test_records(&test.db);
    let artist = crate::handle::tests::make_artist("Seeded Artist", None, None);
    records.insert_artist(&artist).await.unwrap();
    let mut album = bae_records::DbAlbum::new_test("Seeded Album", &coven::UuidIds);
    album.id = album_id.to_string();
    let release = bae_records::DbRelease::new_test(
        &album.id,
        &coven::IdSource::new_id(&coven::UuidIds).to_string(),
    );
    records
        .insert_album_with_release_and_tracks(
            &album,
            &[bae_records::DbAlbumArtist::new_test(&album.id, &artist.id)],
            &release,
            &[bae_records::DbReleaseMedium::new_test(&release.id)],
            &[bae_records::DbTrack::new_test(
                &release.id,
                &coven::IdSource::new_id(&coven::UuidIds).to_string(),
                "Track 1",
                1,
            )],
            &[],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn a_new_album_pick_beside_the_groups_album_keeps_its_own_id() {
    let test = crate::service::tests::setup_import_service().await;
    let (rows, records) = pressing_of_group();
    let own_id = rows.album.id.clone();
    let group_id = bae_records::identity::album_id_for_records(&records).expect("a named group");
    assert_ne!(own_id, group_id);
    seed_album_with_id(&test, &group_id).await;

    let prepared = test
        .service
        .reconcile_prepared_release(rows, records, crate::AlbumPick::New, Default::default())
        .await
        .unwrap();

    assert_eq!(prepared.db_album.id, own_id);
    assert_eq!(prepared.db_release.album_id, own_id);
    assert_eq!(prepared.existing_album_id, None);
}

#[tokio::test]
async fn a_new_album_pick_with_no_album_in_the_library_takes_the_groups_id() {
    let test = crate::service::tests::setup_import_service().await;
    let (rows, records) = pressing_of_group();
    let group_id = bae_records::identity::album_id_for_records(&records).expect("a named group");

    let prepared = test
        .service
        .reconcile_prepared_release(rows, records, crate::AlbumPick::New, Default::default())
        .await
        .unwrap();

    assert_eq!(prepared.db_album.id, group_id);
    assert_eq!(prepared.db_release.album_id, group_id);
}
