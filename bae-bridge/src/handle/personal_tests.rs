use super::*;
use std::sync::Arc;

#[test]
fn profiles_ratings_playlists_and_history_use_the_same_store() {
    let root = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    let (manager, db, storage) = runtime.block_on(bae_test_support::storage_library(
        root.path(),
        bae_core::providers::Providers::offline(),
    ));
    runtime.block_on(bae_test_support::connect_storage(&manager, &storage));
    let services = runtime
        .block_on(bae_core::library::AppServices::for_test(manager))
        .unwrap();
    let app = Arc::new(AppHandle::start(services, runtime).unwrap());
    app.runtime.block_on(async {
        let share = app.clone().generate_restore_code().await.unwrap();
        assert!(share.qr_carries_s3_key);
        // The QR's code holds the S3 key; the code to type holds none.
        let crate::BridgeCodeInfo::Restore { metadata: scanned } =
            crate::decode_code_info(share.qr_text).unwrap()
        else {
            panic!("the QR's code is a restore code");
        };
        assert!(scanned.carries_s3_key);
        let crate::BridgeCodeInfo::Restore { metadata } =
            crate::decode_code_info(share.code).unwrap()
        else {
            panic!("a restore code must not become a named invitation");
        };
        assert!(!metadata.carries_s3_key);
        assert_eq!(scanned.library_id, metadata.library_id);
        assert_eq!(metadata.library_id, app.get_config().library_id);
        assert_eq!(metadata.provider, BridgeCloudProvider::S3);
        assert_eq!(
            Some(metadata.location),
            app.clone().storage_location().await.unwrap(),
            "a typed code shows where its library is stored before connecting"
        );

        let profile = app.clone().create_profile("Robin".into()).await.unwrap();
        app.clone()
            .set_history_sharing(BridgeSharing::Household)
            .await
            .unwrap();
        assert_eq!(
            db.profile(coven::MemberId(profile.id.parse().unwrap()))
                .await
                .unwrap()
                .unwrap()
                .history_sharing,
            bae_core::people::Sharing::Household
        );
        db.execute_local_sql_for_test(
            "INSERT INTO albums(id,title,created_at) VALUES('bridge-album','Album','2026-01-01');",
        )
        .await
        .unwrap();
        app.clone()
            .set_rating(
                BridgeRatingTarget::Album {
                    id: "bridge-album".into(),
                },
                Some(BridgeStars::Four),
            )
            .await
            .unwrap();
        let ratings = app.clone().ratings().await.unwrap();
        assert_eq!(ratings.len(), 1);
        assert_eq!(ratings[0].stars, BridgeStars::Four);
        assert_eq!(ratings[0].person, profile.id);
        app.clone()
            .set_rating(
                BridgeRatingTarget::Album {
                    id: "bridge-album".into(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(app.clone().ratings().await.unwrap().is_empty());
        let playlist = app
            .clone()
            .create_playlist("Evening".into(), BridgeSharing::Private)
            .await
            .unwrap();
        assert_eq!(
            playlist.audience,
            BridgeAudience::Circle {
                id: profile.private_circle.clone()
            }
        );
        app.clone()
            .rename_playlist(playlist.id.clone(), "Gathering".into())
            .await
            .unwrap();
        app.clone()
            .set_playlist_sharing(playlist.id.clone(), BridgeSharing::Household)
            .await
            .unwrap();
        let playlists = app.clone().playlists().await.unwrap();
        assert_eq!(playlists[0].name, "Gathering");
        assert_eq!(playlists[0].audience, BridgeAudience::Store);
        app.clone().delete_playlist(playlist.id).await.unwrap();
        assert!(app.clone().playlists().await.unwrap().is_empty());
        let members = app.clone().get_members().await.unwrap();
        assert!(members.self_is_admin);
        let device = &members.members[0].active_devices[0];
        assert!(device.is_self);
        assert!(!device.can_remove);
        assert!(members.members[0].former_devices.is_empty());
        assert_eq!(app.clone().people().await.unwrap()[0].id, profile.id);
    });
    app.close_library().unwrap();
}
