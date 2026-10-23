#![cfg(feature = "test-utils")]

use bae_core::people::{ProfileError, Sharing};
use bae_test_support::{connect_storage, storage_library};

#[tokio::test]
async fn profile_requires_membership_and_is_idempotent_after_setup() {
    let directory = tempfile::tempdir().unwrap();
    let (manager, database, storage) =
        storage_library(directory.path(), bae_core::providers::Providers::offline()).await;
    assert!(matches!(
        manager.create_profile("Robin").await,
        Err(ProfileError::NotRegistered)
    ));
    assert!(database.profiles().await.unwrap().is_empty());
    connect_storage(&manager, &storage).await;
    let profile = manager.create_profile("Robin").await.unwrap();
    assert_eq!(profile.history_sharing, Sharing::Private);
    assert_eq!(manager.create_profile("Robin").await.unwrap(), profile);
    assert!(matches!(
        manager.create_profile("Another Name").await,
        Err(ProfileError::AlreadyExists)
    ));
    let people = manager.people().await.unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].profile.as_ref(), Some(&profile));
    assert!(people[0].membership.as_ref().unwrap().is_self);
    manager.rename_profile("New Name").await.unwrap();
    manager
        .set_history_sharing(Sharing::Household)
        .await
        .unwrap();
    let profile = database.profile(profile.id).await.unwrap().unwrap();
    assert_eq!(profile.name, "New Name");
    assert_eq!(profile.history_sharing, Sharing::Household);
    manager.close().await.unwrap();
}

#[tokio::test]
async fn a_profile_names_its_avatar_once_the_upload_completes() {
    let directory = tempfile::tempdir().unwrap();
    let (manager, database, storage) =
        storage_library(directory.path(), bae_core::providers::Providers::offline()).await;
    connect_storage(&manager, &storage).await;
    let profile = manager.create_profile("Rowan").await.unwrap();
    assert_eq!(manager.avatar(profile.id).await.unwrap(), None);
    manager
        .set_avatar(Some(b"first avatar".to_vec()))
        .await
        .unwrap();
    let first = database.profile(profile.id).await.unwrap().unwrap();
    assert_eq!(
        manager.avatar(profile.id).await.unwrap().unwrap(),
        b"first avatar"
    );
    assert!(manager
        .outbox_snapshot()
        .await
        .unwrap()
        .other_files
        .iter()
        .any(|file| matches!(file.label, bae_core::library::UploadFileLabel::Avatar)));
    manager
        .set_avatar(Some(b"second avatar".to_vec()))
        .await
        .unwrap();
    let second = database.profile(profile.id).await.unwrap().unwrap();
    // The profile row names a new avatar only once its upload completes; this
    // device reads its own new avatar before then.
    assert_eq!(first.avatar_id, None);
    assert_eq!(second.avatar_id, None);
    assert_eq!(
        manager.avatar(profile.id).await.unwrap().unwrap(),
        b"second avatar"
    );
    manager.drain_uploads_for_test().await.unwrap();
    let stored = database.profile(profile.id).await.unwrap().unwrap();
    assert!(stored.avatar_id.is_some(), "the uploaded avatar is named");
    assert_eq!(
        manager.avatar(profile.id).await.unwrap().unwrap(),
        b"second avatar"
    );
    manager.set_avatar(None).await.unwrap();
    assert_eq!(manager.avatar(profile.id).await.unwrap(), None);
    assert_eq!(
        database
            .profile(profile.id)
            .await
            .unwrap()
            .unwrap()
            .avatar_id,
        None
    );
    manager.close().await.unwrap();
}

/// Run a test body on the worker stack the app gives native invite and
/// bootstrap futures.
fn on_app_runtime(body: impl std::future::Future<Output = ()> + Send + 'static) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(runtime.spawn(body)).unwrap();
}

/// An owner's library on shared S3 storage, with Robin's profile.
struct Household {
    _dir: tempfile::TempDir,
    owner: coven::CovenHandle,
    storage: std::sync::Arc<coven::MemoryStorage>,
    clock: coven::ClockRef,
    robin: bae_core::people::PersonProfile,
}

impl Household {
    async fn new() -> Self {
        use coven::Storage;
        let dir = tempfile::tempdir().unwrap();
        let installation = coven::TestCoven::new();
        let clock: coven::ClockRef = std::sync::Arc::new(coven::SystemClock);
        let storage = std::sync::Arc::new(
            coven::MemoryStorage::builder()
                .clock(clock.clone())
                .build()
                .unwrap(),
        );
        let owner = bae_core::library::create_library(
            installation
                .builder(
                    bae_core::config::AppDir::at(dir.path())
                        .unwrap()
                        .store_layout(),
                )
                .storage_connector(storage.clone()),
            bae_core::library_name::LibraryName::parse("Household").unwrap(),
        )
        .await
        .unwrap();
        Box::pin(owner.setup_s3_storage(
            storage.config(),
            "Owner Device",
            "owner-key".into(),
            coven::SecretText::new("owner-secret".into()),
        ))
        .await
        .unwrap();
        let robin = bae_core::db::Database::from_handle(owner.clone(), clock.clone())
            .create_profile("Robin")
            .await
            .unwrap();
        Self {
            _dir: dir,
            owner,
            storage,
            clock,
            robin,
        }
    }

    async fn invite(&self, target: coven::InviteTarget, key: &str) -> coven::Invite {
        Box::pin(self.owner.create_invite(
            target,
            coven::InviteAccess::S3AccessKey {
                access_key_id: key.into(),
                secret_access_key: coven::SecretText::new("guest-secret".into()),
            },
            coven::InviteDelivery::Sent {
                lifetime: coven::InviteLifetime::Day,
            },
        ))
        .await
        .unwrap()
    }

    /// A device of its own, with its own keychain, reaching the household's
    /// storage.
    fn device(
        &self,
        installation: &coven::TestCoven,
        dir: &tempfile::TempDir,
    ) -> coven::CovenBuilder {
        installation
            .builder(
                bae_core::config::AppDir::at(dir.path())
                    .unwrap()
                    .store_layout(),
            )
            .storage_connector(std::sync::Arc::new(self.storage.for_device()))
    }

    /// Approve the first request among the owner's open invites.
    async fn approve(&self) {
        let mut invites = self.owner.subscribe_invites();
        let request = invites
            .wait_for(|invites| invites.iter().any(|invite| invite.request.is_some()))
            .await
            .unwrap()
            .iter()
            .find_map(|invite| invite.request.clone())
            .unwrap();
        self.owner.approve_join_request(&request).await.unwrap();
    }
}

fn new_member(name: &str) -> coven::InviteTarget {
    coven::InviteTarget::NewMember {
        role: coven::MemberRole::Member,
        name: name.into(),
    }
}

/// Rowan's profile takes the name Robin typed in the invite; it is Rowan's
/// own row, which Rowan can't use to edit Robin's (§5.4).
#[test]
fn joining_writes_the_invites_name_and_cannot_edit_another_person() {
    on_app_runtime(async {
        use std::time::Duration;
        let household = Household::new().await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = coven::TestCoven::new();
        let control = bae_core::library::JoinControl::new();
        let joining = bae_core::library::join_with_invite(
            household.device(&guest, &guest_dir),
            &invite.code,
            "Guest Device",
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let (guest, profile) = joined.unwrap().unwrap();
        let profile = profile.expect("a new person's profile").unwrap();
        let owner_profile = household.robin.clone();
        assert_eq!(profile.name, "Rowan");
        assert_ne!(profile.id, owner_profile.id);
        assert_ne!(profile.private_circle, owner_profile.private_circle);
        let members = guest
            .circles()
            .members(profile.private_circle)
            .await
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].member, profile.id);
        let error = guest
            .write(move |sql| {
                sql.execute(
                    "UPDATE people SET name = 'Impersonated' WHERE id = ?",
                    [owner_profile.id.to_string()],
                )?;
                Ok(())
            })
            .await
            .expect_err("only the profile's owner may rename it");
        assert!(
            matches!(
                error,
                coven::CovenError::Database(coven::DbError::NotOwner { .. })
            ),
            "{error:?}"
        );
        guest.close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Rowan loses every device and is invited again under the same member id.
/// The invite names nobody, so joining writes no profile: Rowan keeps the row
/// they had (§5.4).
#[test]
fn a_replacement_invite_writes_no_profile() {
    on_app_runtime(async {
        use std::time::Duration;
        let household = Household::new().await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let first_dir = tempfile::tempdir().unwrap();
        let first = coven::TestCoven::new();
        let control = bae_core::library::JoinControl::new();
        let joining = bae_core::library::join_with_invite(
            household.device(&first, &first_dir),
            &invite.code,
            "Guest Device",
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let (lost, profile) = joined.unwrap().unwrap();
        let rowan = profile.unwrap().unwrap();
        // Rowan's profile reaches storage before the device is lost.
        lost.start_sync().await.unwrap();
        assert!(matches!(
            lost.retry_now().await,
            coven::RetryOutcome::Synced { .. }
        ));
        lost.close().await.unwrap();
        let again = household
            .invite(
                coven::InviteTarget::ReplaceMemberKeys {
                    member: invite.member,
                },
                "replacement-key",
            )
            .await;
        let second_dir = tempfile::tempdir().unwrap();
        let second = coven::TestCoven::new();
        let joining = bae_core::library::join_with_invite(
            household.device(&second, &second_dir),
            &again.code,
            "Replacement Device",
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let (replacement, profile) = joined.unwrap().unwrap();
        assert!(profile.is_none());
        let database =
            bae_core::db::Database::from_handle(replacement.clone(), household.clock.clone());
        assert_eq!(database.profile(rowan.id).await.unwrap(), Some(rowan));
        replacement.close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Rowan closes bae while waiting. The next start lists the join with the
/// invite's names, resumes it without the code, and writes Rowan's profile
/// (§5.6).
#[test]
fn a_stopped_join_resumes_and_writes_the_profile() {
    on_app_runtime(async {
        use std::time::Duration;
        let household = Household::new().await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = coven::TestCoven::new();
        let control = bae_core::library::JoinControl::new();
        let (waiting, mut waits) = tokio::sync::watch::channel(false);
        let joining = bae_core::library::join_with_invite(
            household.device(&guest, &guest_dir),
            &invite.code,
            "Guest Device",
            household.clock.clone(),
            |status| {
                if matches!(status, coven::BootstrapStatus::WaitingForApproval { .. }) {
                    waiting.send_replace(true);
                }
            },
            &control,
        );
        tokio::time::timeout(Duration::from_secs(30), async {
            tokio::select! {
                result = joining => panic!("the join ended while waiting: {:?}", result.err()),
                changed = waits.wait_for(|waiting| *waiting) => drop(changed.unwrap()),
            }
        })
        .await
        .unwrap();
        let builder = household.device(&guest, &guest_dir);
        let [join] = &bae_core::library::unfinished_joins(&builder).await.unwrap()[..] else {
            panic!("one join under way");
        };
        assert_eq!(
            (join.store_name.as_str(), join.invitee.as_deref()),
            ("Household", Some("Rowan"))
        );
        let resuming = bae_core::library::resume_join(
            builder,
            join,
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(resuming, household.approve())
        })
        .await
        .unwrap();
        let (guest, profile) = joined.unwrap().unwrap();
        assert_eq!(profile.unwrap().unwrap().name, "Rowan");
        guest.close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}
