use super::*;
use bae_people::collections::{RatingTarget, Stars};
use bae_people::people::{RatedItems, RatingSystem};
use bae_views::album_detail::{AlbumRating, Hearter, Rater};

/// The album's next detail that `wanted` accepts, failing when none comes.
async fn detail_matching(
    values: &mut coven::ReconfigurableLiveQuery<
        Option<bae_views::album_detail::AlbumDetailRequest>,
        bae_views::AlbumDetailProjection,
        bae_store::DbError,
    >,
    wanted: impl Fn(&bae_views::album_detail::AlbumDetail) -> bool,
) -> bae_views::album_detail::AlbumDetail {
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let event = values.next_inline().await.expect("the album detail's read stays open");
            let projection = event.result.expect("the album detail reads");
            if let Some(detail) = bae_views::album::resolve_album_detail_projection(projection) {
                if wanted(&detail) {
                    return detail;
                }
            }
        }
    })
    .await
    .expect("the album detail never showed the value waited for")
}

/// Rowan rates an album on his own device and Robin's page names him; an
/// admin then removes Rowan, and his rating still names him beside Robin's
/// own (§5.4: a removed person's ratings stay under their identity). Rowan's
/// removed device keeps its copy: his rating shows there as his own, and he
/// can change it.
#[test]
fn a_removed_person_s_rating_still_names_them() {
    on_app_runtime(async {
        let household = Household::new().await;
        let owner_dir = bae_util::test_files::TestFolder::new();
        let owner = household.owners_library(owner_dir.path()).await;
        let album = bae_util::test_ids::test_uuid("removed-rater-album");
        bae_records::test_fixtures::album_with_tracks(
            owner.records(),
            (&album, "Shared"),
            &bae_util::test_ids::test_uuid("removed-rater-release"),
            &[(&bae_util::test_ids::test_uuid("removed-rater-track"), "Only")],
        )
        .await;

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
        let (rowan_library, rowan) = joined_profile(guest, &household, library_dir.path()).await;

        // Rowan's device rates the album once it has the album.
        let mut rowans = bae_views::LibraryViews::new(rowan_library.store().clone())
            .subscribe_album_detail(Some(bae_views::album_detail::AlbumDetailRequest::album(
                album.clone(),
            )));
        detail_matching(&mut rowans, |detail| !detail.releases.is_empty()).await;
        rowan_library
            .people()
            .set_rating(RatingTarget::Album(album.clone()), Some(Stars::Two))
            .await
            .unwrap();

        // Robin rates it too and sees Rowan beside her.
        owner.manager().attach_and_start_sync().await.unwrap();
        owner
            .people()
            .set_rating(RatingTarget::Album(album.clone()), Some(Stars::Four))
            .await
            .unwrap();
        let mut robins = bae_views::LibraryViews::new(owner.store().clone())
            .subscribe_album_detail(Some(bae_views::album_detail::AlbumDetailRequest::album(
                album.clone(),
            )));
        let both = detail_matching(&mut robins, |detail| {
            matches!(&detail.rating, AlbumRating::Stars { raters, .. } if raters.len() == 2)
        })
        .await;
        assert_eq!(
            both.rating,
            AlbumRating::Stars {
                mine: Some(Stars::Four),
                raters: vec![
                    Rater::You,
                    Rater::Person { name: "Rowan".to_string(), stars: Stars::Two }
                ],
            }
        );

        owner.people().set_rating_system(RatedItems::Albums, RatingSystem::Heart).await.unwrap();
        let hearts = detail_matching(&mut robins, |detail| {
            matches!(detail.rating, AlbumRating::Heart { .. })
        })
        .await;
        assert_eq!(
            hearts.rating,
            AlbumRating::Heart { hearted: true, hearters: Some(vec![Hearter::You]) }
        );

        // Rowan's device must receive both changes while it can still sync.
        detail_matching(&mut rowans, |detail| matches!(detail.rating, AlbumRating::Heart { .. }))
            .await;
        owner.people().set_rating_system(RatedItems::Albums, RatingSystem::Stars).await.unwrap();
        let rowans_stars = AlbumRating::Stars {
            mine: Some(Stars::Two),
            raters: vec![
                Rater::You,
                Rater::Person { name: "Robin".to_string(), stars: Stars::Four },
            ],
        };
        detail_matching(&mut rowans, |detail| detail.rating == rowans_stars).await;

        // An admin removes Rowan. His rating stays, under his name.
        let mut changes = owner
            .manager()
            .subscribe_admin_changes(bae_people::admin_change::AdminChangePlace::People);
        owner.membership().remove_member(&rowan.id).await.unwrap();
        bae_library::followed::query_matching(&mut changes, |changes| {
            changes.changes.iter().any(|change| {
                matches!(
                    change.change,
                    bae_people::admin_change::AdminChangeKind::RemovePerson { .. }
                ) && change.state == bae_people::admin_change::AdminChangeState::Done
            })
        })
        .await;
        let after = bae_views::LibraryViews::new(owner.store().clone())
            .find_album_detail(&album)
            .await
            .unwrap()
            .expect("the album is still there");
        assert_eq!(
            after.rating,
            AlbumRating::Stars {
                mine: Some(Stars::Four),
                raters: vec![
                    Rater::You,
                    Rater::Person { name: "Rowan".to_string(), stars: Stars::Two }
                ],
            }
        );

        // Rowan's device learns its person was removed. It keeps its copy:
        // his stars still show as his, and he can still rate.
        let mut people = rowan_library.people().subscribe_people();
        bae_library::followed::query_matching(&mut people, |view| view.own.is_none()).await;
        let rowans_views = bae_views::LibraryViews::new(rowan_library.store().clone());
        let removed = rowans_views
            .find_album_detail(&album)
            .await
            .unwrap()
            .expect("the album is still on the removed device");
        assert_eq!(removed.rating, rowans_stars);
        rowan_library
            .people()
            .set_rating(RatingTarget::Album(album.clone()), Some(Stars::Five))
            .await
            .unwrap();
        let rerated = rowans_views.find_album_detail(&album).await.unwrap().unwrap();
        assert!(matches!(rerated.rating, AlbumRating::Stars { mine: Some(Stars::Five), .. }));
        rowan_library.manager().close().await.unwrap();
        owner.manager().close().await.unwrap();
    });
}
