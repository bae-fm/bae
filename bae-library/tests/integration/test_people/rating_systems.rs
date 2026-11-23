use super::*;
use bae_people::people::{
    RatedItems, RatingSystem, RatingSystemError, RatingSystems, RatingSystemsSettings,
};

/// Robin, the library's admin, shows albums with a heart, and Rowan's device
/// shows the same once it syncs. Rowan is a member, not an admin: coven
/// refuses his change on his own device, which keeps showing Robin's
/// (§5.3, Rating systems).
#[test]
fn only_an_admin_changes_the_library_s_rating_systems() {
    on_app_runtime(async {
        let household = Household::new().await;
        let owner_dir = bae_util::test_files::TestFolder::new();
        let owner = household.owners_library(owner_dir.path()).await;

        let invite = household.invite("guest-key").await;
        let guest_dir = bae_util::test_files::TestFolder::new();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let joining = household.join(&invite.code, &guest, &control);
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let guest = loaded(joined.unwrap().unwrap());
        let library_dir = bae_util::test_files::TestFolder::new();
        let (rowan_library, _rowan) = joined_profile(guest, &household, library_dir.path()).await;

        owner.manager().attach_and_start_sync().await.unwrap();
        owner.people().set_rating_system(RatedItems::Albums, RatingSystem::Heart).await.unwrap();
        let hearts = RatingSystems { albums: RatingSystem::Heart, tracks: RatingSystem::Stars };
        let mut rowans = rowan_library.people().subscribe_rating_systems();
        assert_eq!(
            query_matching(&mut rowans, |settings| settings.systems == hearts).await,
            RatingSystemsSettings { systems: hearts, can_change: false }
        );

        let refused = rowan_library
            .people()
            .set_rating_system(RatedItems::Tracks, RatingSystem::Hidden)
            .await;
        assert!(matches!(refused, Err(RatingSystemError::NotAdmin)), "{refused:?}");
        let mut after = rowan_library.people().subscribe_rating_systems();
        assert_eq!(query_matching(&mut after, |_| true).await.systems, hearts);

        rowan_library.manager().close().await.unwrap();
        owner.manager().close().await.unwrap();
    });
}
