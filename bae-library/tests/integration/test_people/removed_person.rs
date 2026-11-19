use super::*;
use bae_util::test_ids::test_uuid;
use std::time::Duration;

/// Record a play of `track` as this device's person, under their history
/// setting, long enough to count, and return its row's id.
async fn play(people: &bae_people::People, track: &str) -> uuid::Uuid {
    let mut play = people
        .begin_play(
            coven::IdSource::new_id(&coven::UuidIds),
            track,
            Some(Duration::from_secs(40)),
            bae_util::timestamp::parse_rfc3339("2026-10-18T16:30:00Z").unwrap(),
        )
        .await
        .unwrap()
        .expect("the history setting records plays");
    play.set_played(Duration::from_secs(20));
    assert!(people.record_play(&play).await.unwrap());
    play.entry().id
}

/// Rowan keeps his history to himself ("Only me") and plays a track. An admin
/// removes him from the library. His device keeps his circle and the play in
/// it, and keeps recording his private plays there, which stay on the device
/// (§5.3, §5.6).
#[test]
fn a_removed_person_s_device_keeps_recording_their_private_plays() {
    on_app_runtime(async {
        let household = Household::new().await;
        let owner_dir = tempfile::tempdir().unwrap();
        let owner = household.owners_library(owner_dir.path()).await;
        let invite = household.invite("guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let joining = household.join(&invite.code, &guest, &control);
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let guest = loaded(joined.unwrap().unwrap());
        let library_dir = tempfile::tempdir().unwrap();
        let (rowan_library, rowan) =
            joined_profile(guest.clone(), &household, library_dir.path()).await;
        assert_eq!(rowan.history_sharing, HistorySharing::Private);
        let track = test_uuid("removed-player-track");
        bae_records::test_fixtures::album_with_tracks(
            rowan_library.records(),
            (&test_uuid("removed-player-album"), "Listening"),
            &test_uuid("removed-player-release"),
            &[(&track, "Interlude")],
        )
        .await;
        let people = rowan_library.people();
        let before = play(people, &track).await;

        let mut changes = owner
            .manager()
            .subscribe_admin_changes(bae_people::admin_change::AdminChangePlace::People);
        owner.membership().remove_member(&rowan.id).await.unwrap();
        query_matching(&mut changes, |changes| {
            changes.changes.iter().any(|change| {
                matches!(
                    change.change,
                    bae_people::admin_change::AdminChangeKind::RemovePerson { .. }
                ) && change.state == bae_people::admin_change::AdminChangeState::Done
            })
        })
        .await;
        let mut status =
            guest.subscribe(|sql| Ok::<_, coven::CovenError>(coven::SyncStatus::read(sql)?));
        guest.sync_now();
        query_matching(&mut status, |status| {
            matches!(
                status.state,
                coven::SyncState::Removed { removal: coven::Removal::Member, .. }
            )
        })
        .await;

        let after = play(people, &track).await;
        let private = coven::Audience::Circle(rowan.private_circle);
        let plays = people.history().await.unwrap();
        let mut ids: Vec<_> = plays.iter().map(|play| play.id).collect();
        ids.sort();
        let mut expected = vec![before, after];
        expected.sort();
        assert_eq!(ids, expected);
        assert!(plays.iter().all(|play| play.audience == private && play.person == rowan.id));
        rowan_library.manager().close().await.unwrap();
        owner.manager().close().await.unwrap();
    });
}
