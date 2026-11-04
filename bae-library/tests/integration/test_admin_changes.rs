//! An admin change a library's manager asks for, listed from its journaled
//! operation until the person dismisses it (§5.6).

use super::test_people::{installation_in, new_member, on_app_runtime, Household};
use bae_db::admin_change::{AdminChange, AdminChangeKind, AdminChangeState};

/// Robin approves Rowan's request on her laptop. The approval is listed as
/// letting Rowan in, by the name her invite gave him, and is done once her
/// sync has bound his keys; dismissing it ends it.
#[test]
fn an_approval_is_listed_by_the_invites_name_until_dismissed() {
    on_app_runtime(async {
        use std::time::Duration;
        let household = Household::new().await;
        let dir = tempfile::tempdir().unwrap();
        let library = household.owners_library(dir.path()).await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let joining = household.join(&invite.code, &guest, &control);
        let approving = async {
            let mut invites =
                bae_views::LibraryViews::new(library.database().clone()).subscribe_invites();
            let deciding =
                |open: &bae_db::people::OpenInvite| match bae_library::people::invite_shown(
                    &open.invite,
                ) {
                    bae_library::people::InviteShown::Code {
                        progress: bae_library::people::InviteProgress::Deciding(request),
                        ..
                    } => Some(request),
                    bae_library::people::InviteShown::Code { .. }
                    | bae_library::people::InviteShown::Granting => None,
                };
            let request = bae_library::followed::value_matching(&mut invites, |invites| {
                invites.iter().any(|open| deciding(open).is_some())
            })
            .await
            .iter()
            .find_map(deciding)
            .unwrap();
            library.approve_join_request(&request).await.unwrap();
        };
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(joining, approving)
        })
        .await
        .unwrap();
        let (guest, _profile) = joined.unwrap().unwrap();

        let mut changes = library.subscribe_admin_changes();
        let done = bae_library::followed::value_matching(&mut changes, |changes| {
            changes.iter().any(|change| change.state == AdminChangeState::Done)
        })
        .await;
        let [AdminChange {
            operation,
            change: AdminChangeKind::Approve { member, person },
            state: AdminChangeState::Done,
        }] = done.as_slice()
        else {
            panic!("one approval, done: {done:?}");
        };
        assert_eq!((*member, person.as_deref()), (invite.member, Some("Rowan")));
        library.acknowledge_admin_change(*operation).await.unwrap();
        bae_library::followed::value_matching(&mut changes, Vec::is_empty).await;
        guest.close().await.unwrap();
        library.close().await.unwrap();
    });
}
