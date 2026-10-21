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

#[test]
fn joining_creates_a_separate_profile_and_cannot_edit_another_person() {
    // The native invite and bootstrap futures use the same worker stack as the app.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(runtime.spawn(async {
            use coven::Storage;
            use std::sync::Arc;
            use std::time::Duration;

            let owner_dir = tempfile::tempdir().unwrap();
            let guest_dir = tempfile::tempdir().unwrap();
            let owner_installation = coven::TestCoven::new();
            let guest_installation = coven::TestCoven::new();
            let clock: coven::ClockRef = Arc::new(coven::SystemClock);
            let storage = Arc::new(
                coven::MemoryStorage::builder()
                    .clock(clock.clone())
                    .build()
                    .unwrap(),
            );
            let owner = bae_core::library::create_library(
                owner_installation
                    .builder(
                        bae_core::config::AppDir::at(owner_dir.path())
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
            let owner_db = bae_core::db::Database::from_handle(owner.clone(), clock.clone());
            let owner_profile = owner_db.create_profile("Robin").await.unwrap();
            let invite = Box::pin(owner.create_invite(
                coven::InviteTarget::NewMember {
                    role: coven::MemberRole::Member,
                },
                coven::InviteAccess::S3AccessKey {
                    access_key_id: "guest-key".into(),
                    secret_access_key: coven::SecretText::new("guest-secret".into()),
                },
            ))
            .await
            .unwrap();
            let mut requests = owner.subscribe_join_requests();
            let (_keep_open, cancel) = tokio::sync::watch::channel(false);
            let joining = bae_core::library::join_with_invite(
                guest_installation
                    .builder(
                        bae_core::config::AppDir::at(guest_dir.path())
                            .unwrap()
                            .store_layout(),
                    )
                    .storage_connector(Arc::new(storage.for_device())),
                &invite.code,
                "Guest Device",
                "Rowan",
                clock,
                |_| {},
                &cancel,
            );
            let approval = async {
                let request = requests
                    .wait_for(|requests| !requests.is_empty())
                    .await
                    .unwrap()[0]
                    .clone();
                owner.approve_join_request(&request).await.unwrap();
            };
            let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
                tokio::join!(joining, approval)
            })
            .await
            .unwrap();
            let (guest, profile) = joined.unwrap().unwrap();
            let profile = profile.unwrap();
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
            owner.close().await.unwrap();
        }))
        .unwrap();
}
