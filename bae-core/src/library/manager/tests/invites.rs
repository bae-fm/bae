//! People's open invites, as the library lists them from coven's journal.

use crate::sync::test_devices::run_library_test;

/// An invite this device makes is listed until it is cancelled, read from
/// coven's open invites as they change.
#[test]
fn an_invite_is_listed_until_it_is_cancelled() {
    run_library_test(async |library| {
        let manager = library.first().manager();
        let mut invites = manager.subscribe_invites();
        crate::followed::value_matching(&mut invites, Vec::is_empty).await;
        let invite = manager
            .invite_person(
                coven::MemberRole::Member,
                "Dan".into(),
                coven::InviteAccess::S3AccessKey {
                    access_key_id: "dan-key".into(),
                    secret_access_key: coven::SecretText::new("dan-secret".into()),
                },
                coven::InviteDelivery::Sent {
                    lifetime: coven::InviteLifetime::Day,
                },
            )
            .await
            .unwrap();
        let listed =
            crate::followed::value_matching(&mut invites, |invites| !invites.is_empty()).await;
        assert_eq!(
            listed
                .iter()
                .map(|listed| (listed.id, listed.invitee.clone()))
                .collect::<Vec<_>>(),
            vec![(invite.id, Some("Dan".to_string()))]
        );
        manager.cancel_invite(&invite.id).await.unwrap();
        crate::followed::value_matching(&mut invites, Vec::is_empty).await;
    });
}
