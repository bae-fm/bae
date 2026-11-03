//! People's open invites, as the library lists them from coven's journal.

use crate::sync::test_devices::run_library_test;

/// An invite this device makes is listed from the write that makes it,
/// granting its access, then with its code once a pass has granted it, until
/// it is cancelled, read from coven's open invites as they change.
#[test]
fn an_invite_is_listed_until_it_is_cancelled() {
    run_library_test(async |library| {
        let manager = library.first().manager();
        let views = crate::library::views::LibraryViews::new(library.first().database().clone());
        let mut invites = views.subscribe_invites();
        crate::followed::value_matching(&mut invites, Vec::is_empty).await;
        let invite = manager
            .invite_person(
                coven::MemberRole::Member,
                "Dan".into(),
                coven::InviteAccess::S3AccessKey {
                    access_key_id: "dan-key".into(),
                    secret_access_key: coven::SecretText::new("dan-secret".into()),
                },
                coven::InviteDelivery::Sent { lifetime: coven::InviteLifetime::Day },
            )
            .await
            .unwrap();
        let listed =
            crate::followed::value_matching(&mut invites, |invites| !invites.is_empty()).await;
        assert_eq!(
            listed
                .iter()
                .map(|listed| (listed.invite.id, listed.invite.invitee.clone()))
                .collect::<Vec<_>>(),
            vec![(invite, Some("Dan".to_string()))]
        );
        let granted = crate::followed::value_matching(&mut invites, |invites| {
            invites.iter().any(|listed| {
                matches!(
                    crate::people::invite_shown(&listed.invite),
                    crate::people::InviteShown::Code { .. }
                )
            })
        })
        .await;
        assert!(granted.iter().all(|listed| listed.held.is_none()));
        manager.cancel_invite(&invite).await.unwrap();
        crate::followed::value_matching(&mut invites, Vec::is_empty).await;
    });
}
