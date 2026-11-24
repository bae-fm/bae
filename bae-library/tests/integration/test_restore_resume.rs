//! Finishing a restore onto a new device, or a reset of this device's copy,
//! that stopped after it recorded itself, after bae restarts (§5.6, §6).

use super::dropbox_sign_in::DropboxSignIn;
use super::test_people::{installation_in, on_app_runtime};

/// The owner's library on Dropbox in `installation`, signed in through
/// `sign_in` and closed, with the storage it is on and the person's restore
/// code.
async fn dropbox_library(
    installation: &coven::Coven,
    sign_in: &DropboxSignIn,
) -> (std::sync::Arc<coven::MemoryStorage>, String) {
    use coven::Storage;
    let storage = std::sync::Arc::new(
        coven::MemoryStorage::builder().provider(coven::CloudProvider::Dropbox).build().unwrap(),
    );
    let owner = bae_library::create_library(
        sign_in.configure(
            bae_store::test_devices::test_builder(installation).storage_connector(storage.clone()),
        ),
        bae_store::EnteredName::parse("Household").unwrap(),
        bae_store::EnteredName::parse("Creator").unwrap(),
        "Test Device",
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    owner.authenticate(storage.config().oauth_access().unwrap()).await.unwrap();
    let setup = Box::pin(owner.setup_oauth_storage(storage.config(), &|_| ())).await.unwrap();
    owner.close().await.unwrap();
    (storage, setup.restore_code.as_str().to_string())
}

/// Poll `replacing` one step at a time and drop it at the first step after
/// which `recorded` says the device lists it as under way, as when bae quits
/// there.
async fn stop_once_recorded<T, Recorded: std::future::Future<Output = bool>>(
    recorded: impl Fn() -> Recorded,
    replacing: impl std::future::Future<Output = T>,
) {
    let mut replacing = std::pin::pin!(replacing);
    loop {
        let step = std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(replacing.as_mut(), cx))
        })
        .await;
        assert!(step.is_pending(), "the call finished before its record was seen");
        if recorded().await {
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
        let owner_dir = bae_util::test_files::TestFolder::new();
        let (storage, code) = dropbox_library(&installation_in(&owner_dir), &sign_in).await;
        let laptop_dir = bae_util::test_files::TestFolder::new();
        let laptop = installation_in(&laptop_dir);
        let builder = || {
            sign_in.configure(
                bae_store::test_devices::test_builder(&laptop)
                    .storage_connector(std::sync::Arc::new(storage.for_device())),
            )
        };
        let (_sender, cancel) = tokio::sync::watch::channel(false);
        let laptop = &builder;
        let recorded = move || async move {
            !bae_library::unfinished_restores(&laptop(), &bae_diagnostics::Diagnostics::noop())
                .await
                .unwrap()
                .is_empty()
        };
        stop_once_recorded(
            recorded,
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

/// Dropbox refuses the sign-in a reset kept when it stopped, and it can't be
/// refreshed: finishing the reset as it is fails and asks for a sign-in, and
/// finishing it after signing in again signs the person in once more, then
/// loads the library, and nothing is left under way.
#[test]
fn a_stopped_reset_whose_sign_in_dropbox_refuses_signs_in_again_and_finishes() {
    on_app_runtime(async {
        let sign_in = DropboxSignIn::start().await;
        let dir = bae_util::test_files::TestFolder::new();
        let installation = installation_in(&dir);
        let (storage, _) = dropbox_library(&installation, &sign_in).await;
        let builder = || {
            sign_in.configure(
                bae_store::test_devices::test_builder(&installation)
                    .storage_connector(std::sync::Arc::new(storage.for_device())),
            )
        };
        let [coven::ListedStore::Readable(library)] = &installation.stores().await.unwrap()[..]
        else {
            panic!("one library");
        };
        let store = library.id;
        let listed = &installation;
        let replacing = move || async move {
            listed.stores().await.unwrap().iter().any(|listed| {
                matches!(
                    listed,
                    coven::ListedStore::Readable(coven::StoreInfo {
                        unfinished: Some(coven::Unfinished::Replacement(coven::ReplacedBy::Reset)),
                        ..
                    })
                )
            })
        };
        let noop = bae_diagnostics::Diagnostics::noop();
        stop_once_recorded(
            replacing,
            bae_library::reset_device(builder(), store, "Laptop", |_| {}, &noop),
        )
        .await;
        // The owner's sign-in was the first, and the reset kept it.
        storage.reject_access_token("access-0").await;
        let (_sender, cancel) = tokio::sync::watch::channel(false);

        let refused =
            bae_library::resume_replacement(builder(), store, |_| {}, &cancel, &noop).await;
        assert!(
            matches!(
                &refused,
                Err(bae_library::RestoreFromCodeError::Restore(error)) if matches!(
                    **error,
                    coven::BootstrapError::OAuth(coven::OAuthError::Reauthorize(
                        coven::CloudProvider::Dropbox
                    ))
                )
            ),
            "{refused:?}"
        );
        assert!(replacing().await);
        let presented = sign_in.presented();
        let reset = bae_library::finish_reset_signed_in(
            &installation,
            builder(),
            store,
            |_| {},
            &cancel,
            &noop,
        )
        .await
        .unwrap();
        assert_eq!(sign_in.presented(), presented + 1);
        assert_eq!(reset.store_name().await.unwrap(), "Household");
        reset.close().await.unwrap();
        assert!(!replacing().await);
    });
}
