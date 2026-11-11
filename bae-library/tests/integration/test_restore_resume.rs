//! Finishing a restore onto a new device that stopped after it recorded
//! itself, after bae restarts (§5.6).

use super::dropbox_sign_in::DropboxSignIn;
use super::test_people::{installation_in, on_app_runtime};

/// The owner's library on Dropbox, signed in through `sign_in` and closed,
/// with the storage it is on and the person's restore code.
async fn dropbox_library(
    dir: &tempfile::TempDir,
    sign_in: &DropboxSignIn,
) -> (std::sync::Arc<coven::MemoryStorage>, String) {
    use coven::Storage;
    let storage = std::sync::Arc::new(
        coven::MemoryStorage::builder().provider(coven::CloudProvider::Dropbox).build().unwrap(),
    );
    let owner = bae_library::create_library(
        sign_in.configure(
            bae_store::test_devices::test_builder(&installation_in(dir))
                .storage_connector(storage.clone()),
        ),
        bae_store::EnteredName::parse("Household").unwrap(),
        bae_store::EnteredName::parse("Creator").unwrap(),
        bae_store::TestSources::system().clock().clone(),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    owner.authenticate(storage.config().oauth_access().unwrap()).await.unwrap();
    let setup = Box::pin(owner.setup_oauth_storage(storage.config(), "Owner Device", &|_| ()))
        .await
        .unwrap();
    owner.close().await.unwrap();
    (storage, setup.restore_code.as_str().to_string())
}

/// Poll `restoring` one step at a time and drop it at the first step after
/// which `builder`'s device lists it as under way, as when bae quits there.
async fn stop_once_recorded<T>(
    builder: impl Fn() -> coven::CovenBuilder,
    restoring: impl std::future::Future<Output = T>,
) {
    let mut restoring = std::pin::pin!(restoring);
    loop {
        let step = std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(restoring.as_mut(), cx))
        })
        .await;
        assert!(step.is_pending(), "the restore finished before its record was seen");
        if !bae_library::unfinished_restores(&builder(), &bae_diagnostics::Diagnostics::noop())
            .await
            .unwrap()
            .is_empty()
        {
            return;
        }
        tokio::task::yield_now().await;
    }
}

/// Dropbox refuses the sign-in a restore kept when it stopped, and it can't
/// be refreshed: finishing the restore signs the person in once more, then
/// loads the library, and nothing is left under way.
#[test]
fn a_stopped_restore_whose_sign_in_dropbox_refuses_signs_in_again_and_finishes() {
    on_app_runtime(async {
        let sign_in = DropboxSignIn::start().await;
        let owner_dir = tempfile::tempdir().unwrap();
        let (storage, code) = dropbox_library(&owner_dir, &sign_in).await;
        let laptop_dir = tempfile::tempdir().unwrap();
        let laptop = installation_in(&laptop_dir);
        let builder = || {
            sign_in.configure(
                bae_store::test_devices::test_builder(&laptop)
                    .storage_connector(std::sync::Arc::new(storage.for_device())),
            )
        };
        let (_sender, cancel) = tokio::sync::watch::channel(false);
        stop_once_recorded(
            builder,
            bae_library::restore_from_code(
                builder(),
                &code,
                None,
                "Laptop",
                |_| {},
                &cancel,
                &bae_diagnostics::Diagnostics::noop(),
            ),
        )
        .await;
        // The owner's sign-in was the first; the restore kept the second.
        storage.reject_access_token("access-1").await;
        let presented = sign_in.presented();

        let [restore] =
            &bae_library::unfinished_restores(&builder(), &bae_diagnostics::Diagnostics::noop())
                .await
                .unwrap()[..]
        else {
            panic!("one restore under way");
        };
        let restored = bae_library::resume_restore(
            builder,
            restore,
            |_| {},
            &cancel,
            &bae_diagnostics::Diagnostics::noop(),
        )
        .await
        .unwrap();
        assert_eq!(sign_in.presented(), presented + 1);
        assert_eq!(restored.store_name().await.unwrap(), "Household");
        restored.close().await.unwrap();
        assert!(bae_library::unfinished_restores(
            &builder(),
            &bae_diagnostics::Diagnostics::noop()
        )
        .await
        .unwrap()
        .is_empty());
    });
}
