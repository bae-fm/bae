//! Setting a release's records: which album it lands in, what it leaves of the
//! album it left, and how album records match.

use super::*;
use crate::manager::tests::*;
use bae_records::{DbAlbum, DbAlbumArtist, DbRelease};
use chrono::Utc;

fn discogs_identity(group: &str, release: &str) -> bae_catalogs::records::ReleaseRecord {
    bae_catalogs::records::ReleaseRecord::new(
        &bae_catalogs::records::MetadataRef::new(bae_catalogs::catalog::Catalog::Discogs, release),
        Some(group.to_string()),
    )
}

/// Insert `album`, then one release under it per `(group, release)` MusicBrainz
/// record named — the arrangement every `set_records` test below starts
/// from. The returned releases are in the order the identities were given.
async fn seed_grouped_releases(
    manager: &LibraryManager,
    album: &DbAlbum,
    identities: &[(&str, &str)],
) -> Vec<DbRelease> {
    manager.records.insert_album(album).await.unwrap();
    let mut releases = Vec::with_capacity(identities.len());
    for (group, release_id) in identities {
        let release = create_test_release(&album.id);
        manager.records.insert_release(&release).await.unwrap();
        manager
            .records
            .insert_release_records(&release.id, &[mb_identity(group, release_id)])
            .await
            .unwrap();
        releases.push(release);
    }
    releases
}

#[tokio::test]
async fn set_records_to_file_metadata_moves_release_to_fresh_album() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    let album = create_test_album();
    let release = create_test_release(&album.id);

    manager.records.insert_album(&album).await.unwrap();
    manager.records.insert_release(&release).await.unwrap();
    manager
        .records
        .insert_release_records(&release.id, &[mb_identity("g1", "mb-rel-1")])
        .await
        .unwrap();

    manager.set_records(&release.id, vec![]).await.unwrap();

    // The original album was a one-release album → empty and unlisted now.
    assert!(views(manager).find_album_detail(&album.id).await.unwrap().is_none());

    // Release moved to a brand-new album, holds nothing else.
    let new_album_id =
        manager.records.find_album_id_for_release(&release.id).await.unwrap().unwrap();
    assert_ne!(new_album_id, album.id);
    let siblings = manager.records.get_releases_for_album(&new_album_id).await.unwrap();
    assert_eq!(siblings.len(), 1);
    assert_eq!(siblings[0].id, release.id);

    // Identity rows wiped, metadata source flipped to file_tags.
    let identities = manager.records.get_release_records(&release.id).await.unwrap();
    assert!(identities.is_empty());
}

#[tokio::test]
async fn set_records_replaces_rows_when_the_new_records_fit_the_current_album() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // Album has two releases, both MB identities on group g1.
    let album = create_test_album();
    let releases =
        seed_grouped_releases(manager, &album, &[("g1", "g1-rel"), ("g1", "g1-rel")]).await;
    let release1 = &releases[0];

    // Re-point release1 at another pressing within g1. The new row still
    // agrees with release2's group, so release1 stays put.
    manager.set_records(&release1.id, vec![mb_identity("g1", "mb-rel-99")]).await.unwrap();

    let landing_album_id =
        manager.records.find_album_id_for_release(&release1.id).await.unwrap().unwrap();
    assert_eq!(landing_album_id, album.id);

    let identities = manager.records.get_release_records(&release1.id).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].album_ref().expect("known parent album").key, "g1");
    assert_eq!(identities[0].key(), "mb-rel-99");

    // Source album still holds both releases.
    let siblings = manager.records.get_releases_for_album(&album.id).await.unwrap();
    assert_eq!(siblings.len(), 2);
}

/// A release's records on a catalog the album's other releases don't name
/// can't disagree with them: the release stays in its album.
#[tokio::test]
async fn set_records_on_another_catalog_keeps_the_release_in_its_album() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let releases =
        seed_grouped_releases(manager, &album, &[("g1", "g1-rel"), ("g1", "g1-other")]).await;

    manager
        .set_records(&releases[0].id, vec![discogs_identity("master-1", "d-rel-1")])
        .await
        .unwrap();

    assert_eq!(
        manager.records.find_album_id_for_release(&releases[0].id).await.unwrap().as_deref(),
        Some(album.id.as_str())
    );
}

#[tokio::test]
async fn set_records_creates_a_new_album_when_no_existing_album_fits() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // Two albums, neither matching the new MB group g2.
    let album_a = create_test_album();
    let mut album_b = create_test_album();
    album_b.title = "Other Album".to_string();
    let album_a_releases =
        seed_grouped_releases(manager, &album_a, &[("g1", "g1-rel"), ("g1", "g1-rel")]).await;
    let release_alpha = &album_a_releases[0];
    let release_beta = &album_a_releases[1];
    seed_grouped_releases(manager, &album_b, &[("g3", "g3-rel")]).await;

    // release_alpha takes on a brand-new MB group (g2). Its current
    // album (album_a) holds release_beta on g1, so it can't stay.
    // No other album holds g2 either → fresh album.
    manager.set_records(&release_alpha.id, vec![mb_identity("g2", "g2-rel")]).await.unwrap();

    let landing_album_id =
        manager.records.find_album_id_for_release(&release_alpha.id).await.unwrap().unwrap();
    assert_ne!(landing_album_id, album_a.id);
    assert_ne!(landing_album_id, album_b.id);

    // Source album loses release_alpha but keeps release_beta.
    let source_siblings = manager.records.get_releases_for_album(&album_a.id).await.unwrap();
    assert_eq!(source_siblings.len(), 1);
    assert_eq!(source_siblings[0].id, release_beta.id);
}

#[tokio::test]
async fn set_records_moves_the_release_to_the_matching_album() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // Source album (album_a) carries release_alpha solo at MB g1.
    // Target album (album_b) carries release_other at MB g2 — that
    // matches the new identity we'll set on release_alpha.
    let album_a = create_test_album();
    let mut album_b = create_test_album();
    album_b.title = "Other Album".to_string();
    let release_alpha =
        seed_grouped_releases(manager, &album_a, &[("g1", "g1-rel")]).await.remove(0);
    seed_grouped_releases(manager, &album_b, &[("g2", "g2-rel")]).await;

    manager
        .set_records(&release_alpha.id, vec![mb_identity("g2", "mb-rel-pressing")])
        .await
        .unwrap();

    // release_alpha now lives in album_b alongside release_other.
    let landing_album_id =
        manager.records.find_album_id_for_release(&release_alpha.id).await.unwrap().unwrap();
    assert_eq!(landing_album_id, album_b.id);
    let target_siblings = manager.records.get_releases_for_album(&album_b.id).await.unwrap();
    assert_eq!(target_siblings.len(), 2);

    // album_a was a single-release album → empty and unlisted now.
    assert!(views(manager).find_album_detail(&album_a.id).await.unwrap().is_none());
}

#[tokio::test]
async fn set_records_keeps_the_vacated_album_when_other_releases_remain() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // album_a holds two releases, both on MB g1. Move release_alpha
    // out by giving it a different group.
    let album_a = create_test_album();
    let releases =
        seed_grouped_releases(manager, &album_a, &[("g1", "g1-rel"), ("g1", "g1-rel")]).await;
    let release_alpha = &releases[0];
    let release_beta = &releases[1];

    manager.set_records(&release_alpha.id, vec![mb_identity("g2", "g2-rel")]).await.unwrap();

    // album_a still exists, holds release_beta only.
    let surviving = manager.records.find_album_by_id(&album_a.id).await.unwrap();
    assert!(surviving.is_some());
    let surviving_releases = manager.records.get_releases_for_album(&album_a.id).await.unwrap();
    assert_eq!(surviving_releases.len(), 1);
    assert_eq!(surviving_releases[0].id, release_beta.id);
}

#[tokio::test]
async fn set_records_does_not_touch_metadata_columns() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    let mut album = create_test_album();
    album.title = "Initial Title".to_string();
    album.year = Some(1999);
    manager.records.insert_album(&album).await.unwrap();

    let mut release = create_test_release(&album.id);
    release.pressing.facts = bae_types::pressing::PressingFacts {
        area: Some(bae_types::pressing::area("US")),
        status: Some(bae_types::pressing::ReleaseStatus::Official),
        packaging: Some(bae_types::pressing::Packaging::GatefoldCover),
        discogs_details: vec![bae_types::pressing::DiscogsDetail::Remastered],
        ..bae_types::pressing::made_of(bae_types::pressing::Medium::Vinyl, 1)
    };
    release.pressing.labels = vec![
        bae_types::pressing::ReleaseLabel::of(Some("Label A"), Some("AB 100")),
        bae_types::pressing::ReleaseLabel::of(Some("Label B"), Some("CL 719")),
    ];
    release.pressing.barcodes = vec!["1234567890".to_string(), "5012345678900".to_string()];
    release.pressing.year = Some(1999);
    manager.records.insert_release(&release).await.unwrap();
    manager
        .records
        .insert_release_records(&release.id, &[mb_identity("g1", "mb-rel-1")])
        .await
        .unwrap();

    // Insert a track too — we want to verify it survives.
    manager
        .records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release.id))
        .await
        .unwrap();
    let track = bae_records::DbTrack {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        release_id: release.id.clone(),
        title: "My Track".to_string(),
        medium_id: bae_records::identity::release_medium_id(&release.id, 1),
        side: std::num::NonZeroU8::MIN,
        track_number: std::num::NonZeroU32::MIN,
        duration_ms: 180_000,
        created_at: Utc::now(),
    };
    manager.records.insert_track(&track).await.unwrap();

    manager.set_records(&release.id, vec![discogs_identity("dg1", "dg-rel-1")]).await.unwrap();

    // Pressing fields untouched.
    let after = manager.records.find_release_by_id(&release.id).await.unwrap().unwrap();
    assert_eq!(after.pressing.facts, release.pressing.facts);
    assert_eq!(after.pressing.labels, release.pressing.labels);
    assert_eq!(after.pressing.barcodes, ["1234567890", "5012345678900"]);
    assert_eq!(after.pressing.year, Some(1999));

    // Album-level fields untouched (still in the same album, since
    // both old and new identities are in the only release).
    let after_album = manager.records.find_album_by_id(&after.album_id).await.unwrap().unwrap();
    assert_eq!(after_album.title, "Initial Title");
    assert_eq!(after_album.year, Some(1999));

    // Track survived.
    let tracks = manager.records.get_tracks_for_release(&release.id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "My Track");
}

#[tokio::test]
async fn set_records_to_a_fresh_album_preserves_album_artists() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // Two artists so the album's credit list holds two rows.
    let artist = |id: &str, name: &str| DbArtist {
        id: id.to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    let primary = artist("755ab566-9e71-4a7f-88df-fc5f573f882f", "Primary");
    let secondary = artist("1d4f0221-7e2b-4e87-8376-93eaf8998bd7", "Secondary");
    manager.records.insert_artist(&primary).await.unwrap();
    manager.records.insert_artist(&secondary).await.unwrap();

    // album_a holds release_alpha and release_beta on g1, with both
    // primary + secondary as album artists. We're going to move
    // release_alpha out via a non-fitting identity, forcing the
    // creation of a fresh album.
    let album_a = create_test_album();
    let releases =
        seed_grouped_releases(manager, &album_a, &[("g1", "g1-rel"), ("g1", "g1-rel")]).await;
    let release_alpha = &releases[0];
    for (position, artist_id) in [(0, &primary.id), (1, &secondary.id)] {
        manager
            .records
            .insert_album_artist(&DbAlbumArtist::new(&album_a.id, artist_id, position, Utc::now()))
            .await
            .unwrap();
    }

    // release_alpha takes a different group → can't stay in album_a
    // (g1 disagrees with g2), no other album holds g2 → fresh album.
    manager.set_records(&release_alpha.id, vec![mb_identity("g2", "g2-rel")]).await.unwrap();

    let new_album_id =
        manager.records.find_album_id_for_release(&release_alpha.id).await.unwrap().unwrap();
    assert_ne!(new_album_id, album_a.id);

    // The fresh album carries the same credit list as the source, in order.
    let new_album_artists = manager.records.get_artists_for_album(&new_album_id).await.unwrap();
    let names: Vec<&str> = new_album_artists.iter().map(|a| a.artist.name.as_str()).collect();
    assert_eq!(names, vec!["Primary", "Secondary"]);
}

#[tokio::test]
async fn set_records_clears_primary_when_it_pointed_at_the_moved_release() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let now = Utc::now();
    let beta_track_id = TRACK_BETA.to_string();

    // album_a carries two releases on g1, release_alpha made primary. Move
    // release_alpha out: the release left, release_beta, is the primary.
    let album_a = create_test_album();
    let releases =
        seed_grouped_releases(manager, &album_a, &[("g1", "g1-rel"), ("g1", "g1-rel")]).await;
    let release_alpha = &releases[0];
    let release_beta = &releases[1];

    // A track on release_beta so the primary resolved below has an
    // identifiable target.
    manager
        .records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release_beta.id))
        .await
        .unwrap();
    let beta_track = bae_records::DbTrack {
        id: beta_track_id.clone(),
        release_id: release_beta.id.clone(),
        title: "Track Title".to_string(),
        medium_id: bae_records::identity::release_medium_id(&release_beta.id, 1),
        side: std::num::NonZeroU8::MIN,
        track_number: std::num::NonZeroU32::MIN,
        duration_ms: 180_000,
        created_at: now,
    };
    manager.records.insert_track(&beta_track).await.unwrap();

    // Make release_alpha — the release about to move out — primary.
    manager.records.set_album_primary_release(&album_a.id, &release_alpha.id).await.unwrap();

    manager.set_records(&release_alpha.id, vec![mb_identity("g2", "g2-rel")]).await.unwrap();

    // album_a survives, holding release_beta.
    manager
        .records
        .find_album_by_id(&album_a.id)
        .await
        .unwrap()
        .expect("album should still exist with release_beta");

    // The primary release is the lowest-ordered one left: release_beta.
    let resolved_track_ids =
        manager.records.get_primary_release_track_ids_for_album(&album_a.id).await.unwrap();
    assert_eq!(
        resolved_track_ids,
        Some(vec![beta_track_id.clone()]),
        "the release left in the album is its primary",
    );
}

#[tokio::test]
async fn set_records_atomic_rechecks_the_source_count_inside_the_transaction() {
    // The TOCTOU window: a separate writer lands a release into the source album
    // between `set_records`'s pre-flight read and its atomic call. Drive the atomic
    // API directly with `current_album_id` at the source album, after seeding an
    // extra release into it. The atomic call must NOT delete the source — its
    // in-transaction recheck sees the surviving release.
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    let album_a = create_test_album();
    let release_alpha =
        seed_grouped_releases(manager, &album_a, &[("g1", "g1-rel")]).await.remove(0);

    // Build the fresh-album row the manager would have produced —
    // we're driving the atomic API by hand.
    let now = chrono::Utc::now();
    let fresh_album = DbAlbum {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        title: album_a.title.clone(),
        year: album_a.year,
        is_compilation: album_a.is_compilation,
        created_at: now,
    };

    // Race window: another writer lands release_intruder into
    // album_a after the (hypothetical) pre-flight read but before
    // the atomic call.
    let release_intruder = create_test_release(&album_a.id);
    manager.records.insert_release(&release_intruder).await.unwrap();

    manager
        .records
        .set_records_atomic(bae_records::RecordsChange {
            release_id: release_alpha.id.clone(),
            records: vec![mb_identity("g2", "g2-rel")],
            current_album_id: album_a.id.clone(),
            target_album_id: fresh_album.id.clone(),
            new_album: Some(fresh_album.clone()),
        })
        .await
        .unwrap();

    // Source album survives, holding only release_intruder.
    let survivors = manager.records.get_releases_for_album(&album_a.id).await.unwrap();
    assert_eq!(survivors.len(), 1);
    assert_eq!(survivors[0].id, release_intruder.id);

    // release_alpha landed in the fresh album.
    let fresh_releases = manager.records.get_releases_for_album(&fresh_album.id).await.unwrap();
    assert_eq!(fresh_releases.len(), 1);
    assert_eq!(fresh_releases[0].id, release_alpha.id);
}

#[tokio::test]
async fn album_records_merge_albums_but_never_match_pressings() {
    use bae_catalogs::catalog::Catalog;
    use bae_catalogs::records::{MetadataRef, ReleaseRecord};
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    manager.records.insert_album(&album).await.unwrap();
    let release = create_test_release(&album.id);
    manager.records.insert_release(&release).await.unwrap();
    let records = vec![
        ReleaseRecord::album(&MetadataRef::new(Catalog::Discogs, "42")),
        ReleaseRecord::album(&MetadataRef::new(Catalog::AllMusic, "mw42")),
    ];
    manager.records.insert_release_records(&release.id, &records).await.unwrap();
    assert_eq!(manager.records.get_release_records(&release.id).await.unwrap(), records);
    let name = |release_records: Vec<ReleaseRecord>| {
        let records = manager.records.clone();
        async move { records.album_records_name(&release_records, &[]).await.unwrap() }
    };
    assert_eq!(name(records.clone()).await, Some(album.id.clone()));
    // Numeric Discogs release and master identifiers can coincide; their kinds
    // still denote different objects: a pressing "42" with no album names no
    // album the album record "42" files.
    let pressing = ReleaseRecord::new(&MetadataRef::new(Catalog::Discogs, "42"), Some("42".into()));
    let ungrouped = ReleaseRecord::new(&MetadataRef::new(Catalog::Discogs, "42"), None);
    assert_eq!(name(vec![ungrouped.clone()]).await, None);
    assert_eq!(name(vec![pressing.clone()]).await, Some(album.id.clone()));
    let checks = [
        bae_records::LibraryCheck {
            source: Catalog::Discogs,
            release_id: "42".into(),
            source_group_id: None,
        },
        bae_records::LibraryCheck {
            source: Catalog::Discogs,
            release_id: "42".into(),
            source_group_id: Some("42".into()),
        },
    ];
    let statuses = manager.records.check_releases_in_library(&checks).await.unwrap();
    assert!(!statuses[0].release_in_library() && !statuses[0].album_in_library());
    assert!(!statuses[1].release_in_library() && statuses[1].album_in_library());

    let another_album = create_test_album();
    manager.records.insert_album(&another_album).await.unwrap();
    let another_release = create_test_release(&another_album.id);
    manager.records.insert_release(&another_release).await.unwrap();
    manager.set_records(&another_release.id, vec![pressing.clone()]).await.unwrap();
    assert_eq!(
        manager.records.find_album_id_for_release(&another_release.id).await.unwrap(),
        Some(album.id.clone())
    );
    // A copy of the pressing now files under its album too.
    assert_eq!(name(vec![ungrouped]).await, Some(album.id.clone()));
}

#[tokio::test]
async fn ungrouped_pressings_do_not_invent_album_matches() {
    use bae_catalogs::catalog::Catalog;
    use bae_catalogs::records::{MetadataRef, ReleaseRecord};
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    manager.records.insert_album(&album).await.unwrap();
    let release = create_test_release(&album.id);
    manager.records.insert_release(&release).await.unwrap();
    let records = [ReleaseRecord::new(&MetadataRef::new(Catalog::Discogs, "42"), None)];
    manager.records.insert_release_records(&release.id, &records).await.unwrap();
    assert_eq!(manager.records.get_release_records(&release.id).await.unwrap(), records);
    // A copy of the ungrouped pressing files under its album; an album record
    // of the same number names none.
    assert_eq!(
        manager.records.album_records_name(&records, &[]).await.unwrap(),
        Some(album.id.clone())
    );
    assert_eq!(
        manager
            .records
            .album_records_name(
                &[ReleaseRecord::album(&MetadataRef::new(Catalog::Discogs, "42"))],
                &[]
            )
            .await
            .unwrap(),
        None
    );
}

const TRACK_BETA: &str = "094f4448-f13c-4284-83ea-e362fb2f38aa"; // was "track-beta"
