//! A join once it finds its approval: the library is this device's, open
//! however its first load went (§5.6).

use super::test_people::{installation_in, new_member, on_app_runtime, Household};

/// The diagnostics these tests' bootstrap calls report to: none.
static NO_DIAGNOSTICS: std::sync::LazyLock<bae_diagnostics::Diagnostics> =
    std::sync::LazyLock::new(bae_diagnostics::Diagnostics::noop);

/// A joined library that loaded: its handle and the profile the join asked
/// for.
pub(super) fn loaded(
    joined: bae_library::JoinedLibrary,
) -> (coven::CovenHandle, Option<Result<(), bae_people::people::ProfileError>>) {
    let (handle, bae_library::JoinOutcome { profile, published_failure }) = joined;
    if let Some(failure) = published_failure {
        panic!("the joined library failed once published: {failure:?}");
    }
    (handle, profile)
}

/// Rowan's join is approved, and the storage refuses Rowan's phone before it
/// has loaded anything. The join still opens the library, with the refusal:
/// no join is listed, no profile is asked for, since the library doesn't know
/// Rowan yet, and its sync says it has no access (§5.6).
#[test]
fn an_approved_join_whose_load_is_refused_opens_the_library() {
    on_app_runtime(async {
        let household = Household::new().await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
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
        let (handle, bae_library::JoinOutcome { profile, published_failure }) =
            joined.unwrap().expect("the approved join is a library");
        assert!(profile.is_none(), "{profile:?}");
        let failure = published_failure.expect("the load was refused");
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
