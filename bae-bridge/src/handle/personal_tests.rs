use super::*;

/// The playlists as the listing shows them now: a new subscription's first
/// value is what is stored.
async fn playlists_now(store: &bae_store::Store) -> Vec<bae_views::playlists::PlaylistSummary> {
    let views = bae_views::LibraryViews::new(store.clone());
    bae_library::followed::value_matching(&mut views.subscribe_playlists(), |_| true).await.listed
}

#[test]
fn profiles_ratings_playlists_and_history_use_the_same_store() {
    let root = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    let (library, storage) = runtime.block_on(bae_library::test_libraries::storage_library(
        root.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    ));
    runtime.block_on(bae_library::test_libraries::connect_storage(
        library.manager(),
        library.store(),
        &storage,
    ));
    let made = runtime.block_on(bae_library::test_libraries::made_profile(
        library.manager(),
        library.people(),
        "Robin",
    ));
    let store = library.store().clone();
    let records = library.records().clone();
    let people = bae_people::People::new(store.clone());
    let started = super::super::tests::start_test_handle(runtime, library, root);
    let app = started.handle().clone();
    app.runtime.block_on(async {
        let share = app.clone().generate_restore_code().await.unwrap();
        // The QR's code and the code to type are the same, and neither holds
        // the S3 key.
        assert_eq!(crate::code_as_entered(share.code.clone()), share.qr_text);
        let crate::BridgeCodeInfo::Restore { metadata } =
            crate::decode_code_info(share.code).unwrap()
        else {
            panic!("a restore code must not become a named invitation");
        };
        assert!(!metadata.carries_s3_key);
        assert_eq!(metadata.library_id, app.subscribe_config().next().await.unwrap().library_id);
        assert_eq!(metadata.provider, BridgeCloudProvider::S3);
        assert_eq!(
            Some(metadata.location),
            app.clone().storage_location().await.unwrap(),
            "a typed code shows where its library is stored before connecting"
        );

        // Add device starts a pairing: its typed code holds no key, its QR
        // code holds this device's, and its status waits for a request.
        let added = app.clone().add_device().await.unwrap();
        let crate::BridgeCodeInfo::Pairing { metadata: typed } =
            crate::decode_code_info(added.code.clone()).unwrap()
        else {
            panic!("the code Add device shows is a pairing code");
        };
        assert!(!typed.carries_s3_key);
        let crate::BridgeCodeInfo::Pairing { metadata: scanned } =
            crate::decode_code_info(added.qr_text.clone()).unwrap()
        else {
            panic!("the QR code Add device shows is a pairing code");
        };
        assert!(scanned.carries_s3_key);
        let status = app.follow_add_device(added.id.clone()).unwrap();
        assert!(matches!(
            status.next().await.unwrap(),
            BridgeAddDeviceStatus::Waiting { request: None }
        ));
        app.clone().end_add_device(added.id).await.unwrap();

        // Asking again for the profile the person has, by its name, is done.
        app.clone().create_profile("Robin".into()).await.unwrap();
        let profile = BridgePersonProfile::from_core(made.clone());
        app.clone().set_history_sharing(BridgeHistorySharing::Household).await.unwrap();
        assert_eq!(
            people
                .profile(coven::MemberId(profile.id.parse().unwrap()))
                .await
                .unwrap()
                .unwrap()
                .history_sharing,
            bae_people::people::HistorySharing::Household
        );
        let album = bae_records::DbAlbum::new_test("Album", &coven::UuidIds);
        records.insert_album(&album).await.unwrap();
        app.clone()
            .set_rating(BridgeRatingTarget::Album { id: album.id.clone() }, Some(BridgeStars::Four))
            .await
            .unwrap();
        let ratings = people.ratings().await.unwrap();
        assert_eq!(ratings.len(), 1);
        assert_eq!(ratings[0].stars, bae_people::collections::Stars::Four);
        assert_eq!(ratings[0].person.0.to_string(), profile.id);
        app.clone()
            .set_rating(BridgeRatingTarget::Album { id: album.id.clone() }, None)
            .await
            .unwrap();
        assert!(people.ratings().await.unwrap().is_empty());
        let playlist_id = app
            .clone()
            .create_playlist("Evening".into(), BridgeSharing::Private, Vec::new())
            .await
            .unwrap();
        let stored = playlists_now(&store).await;
        assert_eq!(stored[0].id.to_string(), playlist_id);
        assert_eq!(stored[0].sharing, bae_people::people::Sharing::Private);
        assert_eq!(
            app.clone().play_playlist(playlist_id.clone(), None, false).await.unwrap(),
            BridgePlayOutcome::NothingToPlay
        );
        assert_eq!(
            app.clone().play_playlist(playlist_id.clone(), None, true).await.unwrap(),
            BridgePlayOutcome::NothingToPlay
        );
        let track = bae_util::test_ids::test_uuid("playlist-track");
        bae_records::test_fixtures::album_with_tracks(
            &records,
            (&bae_util::test_ids::test_uuid("playlist-album"), "Held"),
            &bae_util::test_ids::test_uuid("playlist-release"),
            &[(&track, "Held track")],
        )
        .await;
        let held = app
            .clone()
            .create_playlist("Held".into(), BridgeSharing::Household, vec![track.clone()])
            .await
            .unwrap();
        assert_eq!(
            app.clone()
                .play_playlist(
                    held.clone(),
                    Some(bae_util::test_ids::test_uuid("gone-position")),
                    false
                )
                .await
                .unwrap(),
            BridgePlayOutcome::StartMissing
        );
        assert_eq!(
            app.clone()
                .play_playlist(
                    held.clone(),
                    Some(bae_util::test_ids::test_uuid("gone-position")),
                    true
                )
                .await
                .unwrap(),
            BridgePlayOutcome::StartMissing
        );
        app.clone().delete_playlist(held).await.unwrap();
        app.clone().rename_playlist(playlist_id.clone(), "Gathering".into()).await.unwrap();
        app.clone()
            .set_playlist_sharing(playlist_id.clone(), BridgeSharing::Household)
            .await
            .unwrap();
        let playlists = playlists_now(&store).await;
        assert_eq!(playlists[0].name, "Gathering");
        assert_eq!(playlists[0].sharing, bae_people::people::Sharing::Household);
        app.clone().delete_playlist(playlist_id).await.unwrap();
        assert!(playlists_now(&store).await.is_empty());
        let members = app.clone().get_members().await.unwrap();
        assert!(members.self_is_admin);
        let device = &members.members[0].active_devices[0];
        assert!(device.is_self);
        assert!(!device.can_remove);
        assert!(members.members[0].former_devices.is_empty());
        assert_eq!(app.clone().people().await.unwrap()[0].membership.id, profile.id);
    });
    app.close_library(None).unwrap();
}

#[test]
fn a_name_is_blank_by_the_rule_that_refuses_it() {
    assert!(entered_name_is_blank(String::new()));
    assert!(entered_name_is_blank(" \t\n".to_string()));
    assert!(!entered_name_is_blank("  Evening ".to_string()));
}
