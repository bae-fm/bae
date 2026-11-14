//! A join once it finds its approval: the library is this device's, open
//! however its first load went (§5.6).

use super::test_people::{installation_in, on_app_runtime, Household};

/// The diagnostics these tests' bootstrap calls report to: none.
static NO_DIAGNOSTICS: std::sync::LazyLock<bae_diagnostics::Diagnostics> =
    std::sync::LazyLock::new(bae_diagnostics::Diagnostics::noop);

/// A joined library that loaded: its handle.
pub(super) fn loaded(joined: bae_library::JoinedLibrary) -> coven::CovenHandle {
    match joined {
        bae_library::JoinedLibrary::Loaded(handle) => handle,
        bae_library::JoinedLibrary::Published { failure, .. } => {
            panic!("the joined library failed once published: {failure:?}")
        }
    }
}

/// Rowan's join is approved, and the storage refuses Rowan's phone before it
/// has loaded anything. The join still opens the library, with the refusal:
/// no join is listed, and its sync says it has no access (§5.6).
#[test]
fn an_approved_join_whose_load_is_refused_opens_the_library() {
    on_app_runtime(async {
        let household = Household::new().await;
        let invite = household.invite("guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let joining = household.join(&invite.code, &guest, &control);
        let first_slot = coven::ObjectPath::slot(household.owner.store_id(), 1.try_into().unwrap());
        let approving = async {
            household.approve().await;
            household
                .storage
                .set_faults(coven::Faults {
                    read_failures: [(first_slot, coven::StorageFailure::PermissionDenied)].into(),
                    ..coven::Faults::none()
                })
                .await;
        };
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(joining, approving)
        })
        .await
        .unwrap();
        let bae_library::JoinedLibrary::Published { handle, failure } =
            joined.unwrap().expect("the approved join is a library")
        else {
            panic!("the load was refused");
        };
        assert!(
            matches!(&*failure, coven::BootstrapError::Sync(coven::SyncError::Storage(error))
                if error.failure() == coven::StorageFailure::PermissionDenied),
            "{failure:?}"
        );
        assert!(bae_library::unfinished_joins(&household.device(&guest), &NO_DIAGNOSTICS)
            .await
            .unwrap()
            .is_empty());
        // The library opens before it has applied anything, as the app opens
        // it, and nothing it does at open is a synced write, which would fail
        // as not registered: nothing waits to send, and the person has no
        // profile yet.
        let library_dir = tempfile::tempdir().unwrap();
        let library = household.library(handle.clone(), library_dir.path()).await;
        assert_eq!(handle.unsent_work().await.unwrap().writes, 0);
        assert!(library.people().own_person().await.unwrap().is_none());
        assert!(matches!(
            library.people().create_profile("Rowan").await,
            Err(bae_people::people::ProfileError::NotRegistered)
        ));
        handle.start_sync().await.unwrap();
        let answer = handle.retry_now().await.unwrap().await;
        assert!(matches!(answer, coven::RetryOutcome::Failed(_)), "{answer:?}");
        let state = handle.sync_status().await.unwrap().state;
        assert!(matches!(state, coven::SyncState::NoAccess(_)), "{state:?}");
        library.manager().close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Rowan's join is approved, and the storage refuses Rowan's phone before
/// it has loaded anything: the library doesn't know Rowan yet, so it has no
/// profile for them. Once the storage lets the phone in, the next open of the
/// library loads it, and the load that registers Rowan's phone writes their
/// profile under the name Robin typed in the invite (§5.4).
#[test]
fn a_join_whose_load_is_refused_gets_the_profile_when_the_library_loads() {
    on_app_runtime(async {
        let household = Household::new().await;
        let invite = household.invite("guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let joining = household.join(&invite.code, &guest, &control);
        let first_slot = coven::ObjectPath::slot(household.owner.store_id(), 1.try_into().unwrap());
        let approving = async {
            household.approve().await;
            household
                .storage
                .set_faults(coven::Faults {
                    read_failures: [(first_slot, coven::StorageFailure::PermissionDenied)].into(),
                    ..coven::Faults::none()
                })
                .await;
        };
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(joining, approving)
        })
        .await
        .unwrap();
        let bae_library::JoinedLibrary::Published { handle, .. } =
            joined.unwrap().expect("the approved join is a library")
        else {
            panic!("the load was refused");
        };
        let store = handle.store_id();
        handle.close().await.unwrap();

        household.storage.set_faults(coven::Faults::none()).await;
        let reopened = bae_store::configure_store(household.device(&guest), bae_library::SCHEMA)
            .open(store, |_| ())
            .await
            .unwrap();
        let library_dir = tempfile::tempdir().unwrap();
        let library = household.library(reopened.clone(), library_dir.path()).await;
        let mut profiles = library.manager().subscribe_profiles();
        library.manager().attach_and_start_sync().await.unwrap();
        let profiles = bae_library::followed::value_matching(&mut profiles, |profiles| {
            profiles.iter().any(|profile| profile.name == "Rowan")
        })
        .await;
        let rowan = library.people().own_person().await.unwrap().expect("Rowan is in");
        assert!(profiles
            .iter()
            .any(|profile| profile.id == rowan.membership.id && profile.name == "Rowan"));
        library.manager().close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Rowan's join stops while it waits, and what it saved can no longer be
/// read. Listing the joins fails naming Rowan's library instead of listing
/// none; discarding it from this device ends the failure and leaves no join,
/// and discarding again finds nothing. The household's own library is not
/// touched.
#[test]
fn a_stopped_join_whose_settings_cannot_be_read_is_discarded_by_library() {
    on_app_runtime(async {
        let household = Household::new().await;
        let invite = household.invite("guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let (waiting, mut waits) = tokio::sync::watch::channel(false);
        let joining = bae_library::join_with_invite(
            household.device(&guest),
            &invite.code,
            bae_store::EnteredName::parse("Rowan").unwrap(),
            "Guest Device",
            household.sources.clock().clone(),
            |status| {
                if matches!(status, coven::BootstrapStatus::WaitingForApproval { .. }) {
                    waiting.send_replace(true);
                }
            },
            &control,
            &NO_DIAGNOSTICS,
        );
        tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::select! {
                result = joining => panic!("the join ended while waiting: {:?}", result.err()),
                changed = waits.wait_for(|waiting| *waiting) => drop(changed.unwrap()),
            }
        })
        .await
        .unwrap();
        let builder = household.device(&guest);
        let [join] = &bae_library::unfinished_joins(&builder, &NO_DIAGNOSTICS).await.unwrap()[..]
        else {
            panic!("one join under way");
        };
        let store = join.metadata.store_id;
        let found = bae_library::unfinished_join_for(&builder, &store.to_string(), &NO_DIAGNOSTICS)
            .await
            .unwrap();
        assert_eq!(found.as_ref(), Some(join), "the join under way for its own library is found");
        assert!(
            bae_library::unfinished_join_for(&builder, "another-library", &NO_DIAGNOSTICS)
                .await
                .unwrap()
                .is_none(),
            "no join is under way for another library"
        );

        let settings: Vec<_> = walk(guest_dir.path())
            .into_iter()
            .filter(|path| path.file_name().is_some_and(|name| name == "settings.json"))
            .collect();
        let [settings] = &settings[..] else {
            panic!("the join saved one settings file: {settings:?}");
        };
        std::fs::write(settings, b"{").unwrap();

        let error = bae_library::unfinished_joins(&builder, &NO_DIAGNOSTICS)
            .await
            .expect_err("the damaged join fails the listing");
        assert_eq!(
            error.category(),
            bae_diagnostics::UiErrorCategory::UnfinishedInstallationUnreadable {
                library_id: store.to_string()
            },
            "{error:?}"
        );
        assert!(bae_library::unfinished_restores(&builder, &NO_DIAGNOSTICS).await.is_err());

        bae_library::discard_unfinished_installation(&builder, store, &NO_DIAGNOSTICS)
            .await
            .unwrap();
        assert!(!settings.exists());
        assert!(bae_library::unfinished_joins(&builder, &NO_DIAGNOSTICS).await.unwrap().is_empty());
        let again = bae_library::discard_unfinished_installation(&builder, store, &NO_DIAGNOSTICS)
            .await
            .expect_err("nothing is left to discard");
        assert_eq!(again.category(), bae_diagnostics::UiErrorCategory::Config, "{again:?}");
        household.owner.close().await.unwrap();
    });
}

/// Every file under `dir`.
fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}
