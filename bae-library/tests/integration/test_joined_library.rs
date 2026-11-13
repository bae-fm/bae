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
        handle.start_sync().await.unwrap();
        let answer = handle.retry_now().await.unwrap().await;
        assert!(matches!(answer, coven::RetryOutcome::Failed(_)), "{answer:?}");
        let state = handle.sync_status().await.unwrap().state;
        assert!(matches!(state, coven::SyncState::NoAccess(_)), "{state:?}");
        handle.close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Rowan's join is approved, and the storage refuses Rowan's phone before
/// it has loaded anything: the library doesn't know Rowan yet, so nothing
/// asks for a profile, and coven keeps the name Robin typed in the invite.
/// Once the storage lets the phone in, the next open of the library asks
/// for Rowan's profile under that name, and coven keeps the name no longer
/// (§5.4).
#[test]
fn a_join_whose_load_is_refused_asks_for_the_profile_on_the_next_open() {
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
        assert_eq!(handle.joined_invitee().await.unwrap().as_deref(), Some("Rowan"));
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
        assert_eq!(reopened.joined_invitee().await.unwrap(), None);
        library.manager().close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}
