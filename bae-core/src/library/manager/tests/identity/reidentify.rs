// ── re_identify_release ────────────────────────────────────────────
//
// Exact / Approximate fetch through MB / Discogs, so these tests seed the release
// cache and the cover-art lookups first and `prepare_release` reads locally
// instead of hitting the network. The File Tags path makes no external source claim, so it
// needs no seeding.

/// The archived documents under one source release's own key.
async fn archived_for(
    manager: &LibraryManager,
    source: crate::import::PayloadSource,
    source_release_id: &str,
) -> Option<String> {
    manager
        .database
        .load_source_release_payloads(&[(source, source_release_id.to_string())])
        .await
        .unwrap()
        .remove(&(source, source_release_id.to_string()))
}

#[tokio::test]
async fn re_identify_with_file_tags_clears_identities_and_moves_album() {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};
    use std::fs;

    let (manager, _temp_dir) = setup_test_manager().await;

    // Local audio files so the post-`set_identity` reseed can read tags.
    let media = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");
    let mut filenames = Vec::new();
    for (name, title) in [("01.flac", "Tag One"), ("02.flac", "Tag Two")] {
        let dest = media.path().join(name);
        fs::copy(fixtures.join("01 Test Track 1.flac"), &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.to_string());
        tag.set_artist("Tag Artist".to_string());
        tag.set_album("Tag Album".to_string());
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
        filenames.push(name.to_string());
    }

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_provenance = Some(crate::import::MetadataProvenance::ExternalRelease {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: "mb-rel-1".to_string(),
    });
    release.remote = false;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    // Two existing track rows align positionally with the two files.
    insert_n_tracks(&manager.database, &release.id, 2).await;
    let now = Utc::now();
    for name in &filenames {
        let file = crate::db::DbFile::new(
            &release.id,
            name,
            0,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        manager.database.insert_file(&file).await.unwrap();
    }
    // Register the files as coven external refs (in-place files of a Local
    // release) AFTER inserting them, so the file-tag re-read resolves paths.
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &media.path().to_string_lossy())
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", "mb-rel-1")])
        .await
        .unwrap();

    // The source release this one was seeded from has an archived document.
    manager
        .database
        .save_source_release_payloads(&[crate::db::DbSourceReleasePayload {
            source: crate::import::PayloadSource::MusicBrainz,
            source_release_id: "mb-rel-1".to_string(),
            json: r#"{"id":"mb-rel-1"}"#.to_string(),
            fetched_at: Utc::now(),
        }])
        .await
        .unwrap();

    manager
        .re_identify_release(&release.id, crate::import::ReleaseReseed::FileTags)
        .await
        .unwrap();

    // Original (single-release) album is gone; release sits on a
    // fresh one.
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_none());
    let new_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album.id);

    // Identity rows are cleared and the metadata provenance becomes File Tags.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(identities.is_empty());
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_provenance,
        Some(crate::import::MetadataProvenance::FileTags)
    );
    // The archived document describes `mb-rel-1`, not this release, and is
    // shared with every candidate that matched it. Dropping the pointer is what
    // stops it being read here; nothing deletes it.
    assert!(
        archived_for(
            &manager,
            crate::import::PayloadSource::MusicBrainz,
            "mb-rel-1"
        )
        .await
        .is_some(),
        "documents are keyed by the source release, so re-pointing must not delete them"
    );
}

// ── re_identify_release Exact / Approximate (MB cache-seeded) ────
//
// Drive the network-side `prepare_release` through the MB LRU cache
// (`seed_release_cache` + `seed_release_group_json_cache`) and the cover-art
// client's own so these tests don't hit the network. The caches are
// process-global LRUs, so each test uses a unique MB release ID and no other
// test's seed bleeds in.

/// Build a synthetic MB release response with `n` track rows on a
/// single CD medium, plus a release group reference. Suitable for
/// driving `prepare_release` via cache seeding.
fn make_mb_release_for_re_identify(
    release_id: &str,
    release_group_id: &str,
    track_count: usize,
) -> crate::musicbrainz::MbReleaseResponse {
    use crate::musicbrainz::{
        MbArtistCredit, MbArtistRef, MbMedium, MbReleaseGroupRef, MbReleaseResponse, MbTrack,
    };
    MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("2024-01-01".to_string()),
        country: None,
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
            id: release_group_id.to_string(),
            first_release_date: Some("2024-01-01".to_string()),
            relations: Some(vec![]),
        }),
        label_info: vec![],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: (1..=track_count)
                .map(|n| MbTrack {
                    position: Some(n as i64),
                    number: Some(n.to_string()),
                    title: Some(format!("Track {n}")),
                    length: None,
                    recording: None,
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

/// Insert `n` plain track rows for a release. Mirrors the row shape
/// `prepared.parsed.tracks` would produce so the track-count check
/// in `re_identify_release` accepts the picked release.
async fn insert_n_tracks(database: &Database, release_id: &str, n: usize) {
    for i in 1..=n {
        let track = crate::db::DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            title: format!("Track {i}"),
            side: 1,
            track_number: Some(i as i32),
            duration_ms: None,
            discogs_position: None,
            created_at: Utc::now(),
        };
        database.insert_track(&track).await.unwrap();
    }
}

#[tokio::test]
async fn re_identify_release_exact_archives_the_picked_release() {
    use crate::import::{ReleaseReseed, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_provenance = Some(crate::import::MetadataProvenance::ExternalRelease {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: "mb-rel-old".to_string(),
    });

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g-old", "mb-rel-old")])
        .await
        .unwrap();
    insert_n_tracks(&manager.database, &release.id, 3).await;

    // Cache the picked release so `prepare_release` skips the network. The raw
    // JSON is what gets archived under the picked release's own key.
    let new_release_id = "exact-re-identify-mb-rel-new";
    let new_group_id = "exact-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 3);
    // What the archive holds is what the client returned, so the projection that
    // replays it later reads the same release the cache handed over now.
    let new_raw_json = serde_json::to_string(&new_response).unwrap();
    seed_release_cache(new_release_id, (new_response, None, new_raw_json.clone()));
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"exact-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            ReleaseReseed::ExternalRelease {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Identity row updated to the new pressing.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::MusicBrainz);
    assert_eq!(identities[0].source_group_id, new_group_id);
    assert_eq!(identities[0].source_release_id, new_release_id);

    // Pointer columns flipped to the new source release.
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_provenance,
        Some(crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: new_release_id.to_string(),
        })
    );

    // The picked release's documents are archived under its own key, which is
    // what the new pointer names.
    assert_eq!(
        archived_for(
            &manager,
            crate::import::PayloadSource::MusicBrainz,
            new_release_id
        )
        .await
        .as_deref(),
        Some(new_raw_json.as_str())
    );
    assert!(
        archived_for(
            &manager,
            crate::import::PayloadSource::MusicBrainzReleaseGroup,
            new_group_id
        )
        .await
        .is_some(),
        "the release group is archived alongside the release"
    );
}

#[tokio::test]
async fn re_identify_release_rejects_track_count_mismatch() {
    // Re-identify re-points the identity without re-binding any audio, so a
    // source naming a different number of tracks leaves rows with nothing to
    // point at: a 12-track release can't replace a 10-track rip. A folder
    // import maps its own audio into track slots instead, where a count
    // disagreement is a row to look at rather than a refusal.
    use crate::import::{ReleaseReseed, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // Local release has 10 tracks; picked release has 12.
    insert_n_tracks(&manager.database, &release.id, 10).await;

    let new_release_id = "mismatch-re-identify-mb-rel-new";
    let new_group_id = "mismatch-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 12);
    let new_raw_json = serde_json::to_string(&new_response).unwrap();
    seed_release_cache(new_release_id, (new_response, None, new_raw_json));
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"mismatch-re-identify-mb-group-new"}"#.to_string(),
    );

    let err = manager
        .re_identify_release(
            &release.id,
            ReleaseReseed::ExternalRelease {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .expect_err("track-count mismatch must error before identity write");
    let msg = err.to_string();
    assert!(
        msg.contains("Track count mismatch") && msg.contains("10") && msg.contains("12"),
        "error must name both counts so the UI can render a useful banner: {msg}"
    );

    // No identity row written.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(
        identities.is_empty(),
        "mismatched commit must not leave a partial identity row"
    );
}

#[tokio::test]
async fn re_identify_release_followed_by_reset_succeeds() {
    // End to end: after a re-identify commit, `reset_metadata_to_source`
    // projects through the new pointer and reaches the documents that commit
    // archived. A regression here means re-identify pointed the release at a
    // source release whose documents it never wrote.
    use crate::import::{ReleaseReseed, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_provenance = Some(crate::import::MetadataProvenance::FileTags);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    insert_n_tracks(&manager.database, &release.id, 2).await;

    let new_release_id = "reset-re-identify-mb-rel-new";
    let new_group_id = "reset-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 2);
    let new_raw_json = serde_json::to_string(&new_response).unwrap();
    seed_release_cache(new_release_id, (new_response, None, new_raw_json));
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"reset-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            ReleaseReseed::ExternalRelease {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Reset replays the seed through the new pointer. A stale
    // cache would surface here as a missing key, a
    // parse error, or a divergence-guard `Err`. Success means
    // re_identify_release left the cache aligned.
    let edit = manager
        .reset_metadata_to_source(&release.id)
        .await
        .expect("reset must replay through aligned cache after re-identify");
    assert_eq!(edit.album_title, "Album Title");
    assert_eq!(edit.tracks.len(), 2);
}

#[tokio::test]
async fn re_identify_with_file_tags_reseeds_rows_from_file_tags() {
    // A release carrying MusicBrainz-shaped rows, with local audio
    // files whose embedded tags say something different. Re-identifying
    // as File Tags must reseed the album/track rows from those tags — not
    // leave the old MB metadata displayed under a "use my files" claim.
    use crate::import::ReleaseReseed;
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};
    use std::fs;

    let (manager, _temp_dir) = setup_test_manager().await;

    // Local files live in a local folder so `local_file_path`
    // resolves to disk where lofty can read the embedded tags.
    let media = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");

    let tag_file = |name: &str, title: &str| -> String {
        let src = fixtures.join("01 Test Track 1.flac");
        let dest = media.path().join(name);
        fs::copy(&src, &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.to_string());
        tag.set_artist("Tagged Artist".to_string());
        tag.set_album("Tagged Album".to_string());
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
        name.to_string()
    };
    let f1 = tag_file("01.flac", "Tagged One");
    let f2 = tag_file("02.flac", "Tagged Two");

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    // MusicBrainz-shaped pointer; the rows below carry MB metadata.
    release.metadata_provenance = Some(crate::import::MetadataProvenance::ExternalRelease {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: "mb-rel-1".to_string(),
    });
    release.remote = false;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", "mb-rel-1")])
        .await
        .unwrap();

    // MB-shaped track rows — distinct from the embedded tags.
    for (i, (id, title)) in [
        ("08c7ff07-b56a-4e16-8df6-ae2967fa0806", "MB Track One"),
        ("08c7fe07-b56a-4c63-8df6-ad2967fa0653", "MB Track Two"),
    ]
    .into_iter()
    .enumerate()
    {
        let track = crate::db::DbTrack {
            id: id.to_string(),
            release_id: release.id.clone(),
            title: title.to_string(),
            side: 1,
            track_number: Some(i as i32 + 1),
            duration_ms: None,
            discogs_position: None,
            created_at: Utc::now(),
        };
        manager.database.insert_track(&track).await.unwrap();
    }
    let now = Utc::now();
    for name in [&f1, &f2] {
        let file = crate::db::DbFile::new(
            &release.id,
            name,
            0,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        manager.database.insert_file(&file).await.unwrap();
    }
    // Register the in-place files as coven external refs after inserting them.
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &media.path().to_string_lossy())
        .await
        .unwrap();

    manager
        .re_identify_release(&release.id, ReleaseReseed::FileTags)
        .await
        .unwrap();

    // Pointer flipped to file_tags.
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_provenance,
        Some(crate::import::MetadataProvenance::FileTags)
    );

    // Album + track rows now reflect the embedded tags, not the MB seed.
    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    let landing_album = manager
        .database
        .find_album_by_id(&landing_album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album.title, "Tagged Album");

    let tracks = manager
        .database
        .get_tracks_for_release(&release.id)
        .await
        .unwrap();
    let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
    assert!(
        titles.contains(&"Tagged One") && titles.contains(&"Tagged Two"),
        "track rows must carry the embedded tag titles, got {titles:?}"
    );
    assert!(
        !titles.iter().any(|t| t.starts_with("MB ")),
        "old MusicBrainz track titles must be gone, got {titles:?}"
    );
}
