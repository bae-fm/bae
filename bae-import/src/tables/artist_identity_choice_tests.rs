//! Per-import answers to conflicting artist identities: what each answer
//! makes of the imported credits, and when it stops holding.

use super::*;
use crate::tables::import_state::tests::*;
use crate::tables::{ImportCommitGuard, ImportRows, NewArtistImages};
use crate::ArtistIdentityConflictChoice as Choice;
use bae_db::test_fixtures::{empty_db, fixed_now};
use bae_db::ArtistCredits;
use bae_util::content_type::ContentType;

async fn fixture() -> (Database, tempfile::TempDir, String, DbArtist, DbArtist, DbArtist) {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let discogs = DbArtist {
        id: bae_util::test_ids::test_uuid("choice-discogs"),
        name: "Artist One".into(),
        sort_name: None,
        discogs_artist_id: Some("discogs-1".into()),
        musicbrainz_artist_id: None,
        created_at: fixed_now(),
    };
    let musicbrainz = DbArtist {
        id: bae_util::test_ids::test_uuid("choice-musicbrainz"),
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-1".into()),
        ..discogs.clone()
    };
    let incoming = DbArtist {
        id: bae_util::test_ids::test_uuid("choice-incoming"),
        musicbrainz_artist_id: musicbrainz.musicbrainz_artist_id.clone(),
        ..discogs.clone()
    };
    db.insert_artist(&discogs).await.unwrap();
    db.insert_artist(&musicbrainz).await.unwrap();
    tables
        .save_import_candidate_failure(&hash, 0, &artist_identity_failure(&discogs, &musicbrainz))
        .await
        .unwrap();
    (db, tmp, hash, discogs, musicbrainz, incoming)
}

#[tokio::test]
async fn every_artist_identity_choice_controls_the_imported_credits_and_ids() {
    for choice in [
        Choice::SameArtist,
        Choice::DiscogsArtist,
        Choice::MusicBrainzArtist,
        Choice::Neither,
        Choice::ChangeMatch,
    ] {
        let (db, tmp, hash, discogs, musicbrainz, incoming) = fixture().await;
        let tables = crate::tables::ImportTables::new(db.clone());
        let credits = [incoming.clone()];
        assert!(
            tables.resolve_artists(&credits, &[], Some(&hash)).await.is_err(),
            "a conflict must wait for the person"
        );
        tables.choose_import_artist_identity(&hash, 0, choice, &coven::UuidIds).await.unwrap();
        if choice == Choice::ChangeMatch {
            assert!(tables.resolve_artists(&credits, &[], Some(&hash)).await.is_err());
            let pane = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
            assert!(pane.failure.is_some());
            assert_eq!(pane.session.unwrap().presentation, crate::MetadataPresentation::FindOnline);
            assert_eq!(db.find_artist_by_id(&discogs.id).await.unwrap().unwrap(), discogs);
            assert_eq!(db.find_artist_by_id(&musicbrainz.id).await.unwrap().unwrap(), musicbrainz);
            continue;
        }
        let resolved = tables.resolve_artists(&credits, &[], Some(&hash)).await.unwrap();
        let chosen = resolved.ids[0].clone();
        match choice {
            Choice::SameArtist | Choice::DiscogsArtist => assert_eq!(chosen, discogs.id),
            Choice::MusicBrainzArtist => assert_eq!(chosen, musicbrainz.id),
            Choice::Neither => {
                assert_ne!(chosen, discogs.id);
                assert_ne!(chosen, musicbrainz.id);
                assert_ne!(chosen, incoming.id);
                assert!(
                    db.find_artist_by_id(&chosen).await.unwrap().is_none(),
                    "the new artist waits for the import transaction"
                );
            }
            Choice::ChangeMatch => unreachable!(),
        }
        // A retry and a separate preview read use the same reserved identity.
        assert_eq!(tables.resolve_artists(&credits, &[], Some(&hash)).await.unwrap(), resolved);
        if choice != Choice::SameArtist {
            assert!(
                tables.resolve_artists(&credits, &[], Some("another-candidate")).await.is_err(),
                "a decision applies only to its own import"
            );
        }
        let now = fixed_now();
        let album = DbAlbum {
            id: bae_util::test_ids::test_uuid("choice-album"),
            title: "Album Title".into(),
            year: None,
            is_compilation: false,
            created_at: now,
        };
        let release =
            DbRelease::new_test(&album.id, &bae_util::test_ids::test_uuid("choice-release"));
        let track = DbTrack::new_test(
            &release.id,
            &bae_util::test_ids::test_uuid("choice-track"),
            "Track Title",
            1,
        );
        let tracks = [crate::TrackFile {
            db_track: track.clone(),
            audio: crate::TrackAudio::Standalone {
                file_path: tmp.path().join("01.flac"),
                source_audio: crate::folder_scanner::ScannedAudio {
                    content_type: ContentType::Flac,
                    duration_ms: 1000,
                    format: bae_types::audio_format::AudioFormat {
                        codec: "FLAC".into(),
                        sample_rate_hz: 44100,
                        bits_per_sample: Some(16),
                        bitrate_kbps: None,
                        channels: 2,
                    },
                },
            },
        }];
        let work = DbWork {
            id: bae_util::test_ids::test_uuid("choice-work"),
            title: "Work Title".into(),
            disambiguation: None,
            work_type: None,
            musicbrainz_work_id: "work-1".into(),
            created_at: now,
        };
        let expected_new: Vec<_> =
            resolved.inserts.iter().map(|artist| artist.id.clone()).collect();
        tables
            .finalize_import_atomic(
                ImportCommitGuard::UncheckedTestSetup { content_hash: Some(hash.clone()) },
                Some(&album),
                &release,
                &tracks,
                ImportRows {
                    mediums: &[bae_db::DbReleaseMedium::new_test(&release.id)],
                    artists: ArtistCredits { credits: &credits, picked: &[] },
                    album_artists: &[DbAlbumArtist::new(&album.id, &incoming.id, 0, now)],
                    track_artists: &[DbTrackArtist::new(&track.id, &incoming.id, 0, now)],
                    works: std::slice::from_ref(&work),
                    track_works: &[DbTrackWork::new(
                        &track.id,
                        &work.id,
                        0,
                        bae_catalogs::catalog::Catalog::MusicBrainz,
                        bae_util::test_ids::test_uuid("choice-track-work"),
                        now,
                    )],
                    work_artists: &[DbWorkArtist::new(
                        &work.id,
                        &incoming.id,
                        0,
                        bae_catalogs::catalog::Catalog::MusicBrainz,
                        now,
                    )],
                    release_artist_roles: &[DbReleaseArtistRole::new(
                        &release.id,
                        &incoming.id,
                        0,
                        bae_catalogs::catalog::Catalog::Discogs,
                        Some("Producer".into()),
                        bae_util::test_ids::test_uuid("choice-release-role"),
                        now,
                    )],
                    track_artist_roles: &[DbTrackArtistRole::new(
                        &track.id,
                        &incoming.id,
                        0,
                        bae_catalogs::catalog::Catalog::MusicBrainz,
                        Some("arranger".into()),
                        bae_util::test_ids::test_uuid("choice-track-role"),
                        now,
                    )],
                    ..Default::default()
                },
                Vec::new(),
                None,
                NewArtistImages { expected_new_artists: &expected_new, images: &[] },
            )
            .await
            .unwrap();
        let expected = chosen.clone();
        db.read(move |sql| {
            for table in [
                "album_artists",
                "track_artists",
                "work_artists",
                "release_artist_roles",
                "track_artist_roles",
            ] {
                let artists: Vec<String> =
                    sql.query(&format!("SELECT artist_id FROM {table}"), [], |row| row.get(0))?;
                assert_eq!(artists.as_slice(), std::slice::from_ref(&expected), "{table}");
            }
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(
            db.get_artists_for_album(&album.id)
                .await
                .unwrap()
                .into_iter()
                .map(|artist| artist.artist.id)
                .collect::<Vec<_>>()
                .as_slice(),
            std::slice::from_ref(&chosen)
        );
        let actual = db.find_artist_by_id(&chosen).await.unwrap().unwrap();
        if choice == Choice::SameArtist {
            assert_eq!(actual.discogs_artist_id, discogs.discogs_artist_id);
            assert_eq!(actual.musicbrainz_artist_id, musicbrainz.musicbrainz_artist_id);
        } else {
            assert_eq!(db.find_artist_by_id(&discogs.id).await.unwrap().unwrap(), discogs);
            assert_eq!(db.find_artist_by_id(&musicbrainz.id).await.unwrap().unwrap(), musicbrainz);
            if choice == Choice::Neither {
                assert_eq!(actual.name, incoming.name);
                assert_eq!(actual.discogs_artist_id, None);
                assert_eq!(actual.musicbrainz_artist_id, None);
            }
        }
        assert_eq!(tables.resolve_artists(&credits, &[], Some(&hash)).await.unwrap().ids, [chosen]);
    }
}

#[tokio::test]
async fn a_stale_artist_identity_choice_changes_nothing() {
    for choice in [
        Choice::SameArtist,
        Choice::DiscogsArtist,
        Choice::MusicBrainzArtist,
        Choice::Neither,
        Choice::ChangeMatch,
    ] {
        let (db, _tmp, hash, discogs, musicbrainz, incoming) = fixture().await;
        let tables = crate::tables::ImportTables::new(db.clone());
        tables
            .choose_import_artist_identity(&hash, 1, choice, &coven::UuidIds)
            .await
            .expect_err("an answer to an old draft cannot change this one");
        assert_eq!(db.find_artist_by_id(&discogs.id).await.unwrap().unwrap(), discogs);
        assert_eq!(db.find_artist_by_id(&musicbrainz.id).await.unwrap().unwrap(), musicbrainz);
        assert!(tables.resolve_artists(&[incoming], &[], Some(&hash)).await.is_err());
        assert!(tables.load_import_candidate_pane_rows(&hash).await.unwrap().failure.is_some());
    }
}

#[tokio::test]
async fn edits_keep_an_answer_but_changing_the_source_discards_it() {
    let (db, _tmp, hash, discogs, _musicbrainz, incoming) = fixture().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables
        .choose_import_artist_identity(&hash, 0, Choice::DiscogsArtist, &coven::UuidIds)
        .await
        .unwrap();
    PaneControls::of_pane_candidate(&db)
        .type_field(&hash, crate::CandidateEditField::AlbumTitle, "Edited Album")
        .await
        .unwrap();
    assert_eq!(
        tables
            .resolve_artists(std::slice::from_ref(&incoming), &[], Some(&hash))
            .await
            .unwrap()
            .ids,
        [discogs.id]
    );
    let mut prep =
        tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
    let expected = crate::tables::CandidateSaveExpectation {
        edit_revision: prep.file_edits.revision,
        metadata_revision: prep.metadata_revision,
        scanned: None,
    };
    prep.metadata.provenance = Some(crate::MetadataProvenance::FileMetadata);
    prep.metadata_revision += 1;
    tables.save_candidate_preparation(prep, expected, Default::default()).await.unwrap();
    assert!(tables.resolve_artists(&[incoming], &[], Some(&hash)).await.is_err());
}

#[tokio::test]
async fn repeated_credits_share_the_neither_artist_and_picked_credits_stay_picked() {
    let (db, _tmp, hash, discogs, _musicbrainz, incoming) = fixture().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.choose_import_artist_identity(&hash, 0, Choice::Neither, &coven::UuidIds).await.unwrap();
    let repeated =
        DbArtist { id: bae_util::test_ids::test_uuid("repeated-credit"), ..incoming.clone() };
    let resolved = tables
        .resolve_artists(
            &[incoming.clone(), repeated, discogs.clone()],
            std::slice::from_ref(&discogs.id),
            Some(&hash),
        )
        .await
        .unwrap();
    assert_eq!(resolved.ids[0], resolved.ids[1]);
    assert_eq!(resolved.ids[2], discogs.id);
    assert_eq!(resolved.inserts.len(), 1);
    let credit = bae_catalogs::credits::ArtistCredit {
        credited_as: None,
        name: incoming.name,
        sort_name: incoming.sort_name,
        discogs_artist_id: incoming.discogs_artist_id,
        musicbrainz_artist_id: incoming.musicbrainz_artist_id,
    };
    let preview = db
        .read(move |sql| {
            crate::tables::artist_identity_choice::resolve_import_credits_on(&sql, &hash, [&credit])
        })
        .await
        .unwrap();
    assert_eq!(preview[0].resolution, bae_db::CreditResolution::New);
}
