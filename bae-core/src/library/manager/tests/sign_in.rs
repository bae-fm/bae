use super::*;

/// Records each authorization URL coven presents, then cancels the sign-in.
#[derive(Default)]
struct RecordingPresenter {
    presented: std::sync::Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl coven::OAuthPresenter for RecordingPresenter {
    fn redirect_uri(&self, _: coven::CloudProvider) -> &str {
        "bae-test:/oauth"
    }

    async fn present(
        &self,
        authorization_url: &str,
    ) -> Result<coven::SecretText, coven::OAuthError> {
        self.presented
            .lock()
            .unwrap()
            .push(authorization_url.to_owned());
        Err(coven::OAuthError::Cancelled)
    }
}

/// A manager whose store signs in through `presenter`, with Full Dropbox and
/// OneDrive applications registered as the apps register them.
async fn manager_signing_in_with(
    presenter: Arc<RecordingPresenter>,
) -> (LibraryManager, Arc<MemoryStorage>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let app_dir = crate::config::AppDir::at(temp_dir.path()).unwrap();
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let storage = Arc::new(
        MemoryStorage::builder()
            .clock(clock.clone())
            .build()
            .unwrap(),
    );
    let handle =
        crate::library::configure_store(coven::TestCoven::new().builder(app_dir.store_layout()))
            .oauth_clients(coven::OAuthClients::new(
                Some(coven::DropboxClients::FullDropbox {
                    app_key: "dropbox-key".into(),
                }),
                Some("onedrive-id".into()),
                clock.clone(),
            ))
            .oauth_presenter(presenter)
            .storage_connector(storage.clone())
            .create_store("Test Library")
            .await
            .unwrap();
    let config = Config::with_defaults(
        handle.store_id(),
        app_dir.registered_library(&handle.store_id().to_string()),
    );
    let manager = LibraryManager::new(
        Database::from_handle(handle, clock.clone()),
        app_dir,
        Arc::new(ConfigHandle::new(config)),
        clock,
        Arc::new(coven::UuidIds),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(crate::util::http::Http::for_test()),
        crate::providers::Providers::offline(),
    );
    (manager, storage, temp_dir)
}

#[tokio::test]
async fn signing_in_to_dropbox_asks_for_the_shared_folder_access_households_need() {
    let presenter = Arc::new(RecordingPresenter::default());
    let (manager, _storage, _temp) = manager_signing_in_with(presenter.clone()).await;

    manager
        .sign_in(crate::config::CloudProvider::Dropbox)
        .await
        .expect_err("the presenter cancels");

    let presented = presenter.presented.lock().unwrap().clone();
    assert_eq!(presented.len(), 1);
    let url = reqwest::Url::parse(&presented[0]).unwrap();
    let scope = url
        .query_pairs()
        .find(|(name, _)| name == "scope")
        .expect("the request names its scope")
        .1
        .into_owned();
    assert!(scope.split(' ').any(|s| s == "sharing.read"), "{scope}");
}

#[tokio::test]
async fn signing_in_again_to_s3_storage_presents_no_sign_in() {
    let presenter = Arc::new(RecordingPresenter::default());
    let (manager, storage, _temp) = manager_signing_in_with(presenter.clone()).await;
    manager
        .database
        .setup_s3_storage(
            storage.config(),
            "Test Device",
            "test-key".into(),
            coven::SecretText::new("test-secret".into()),
        )
        .await
        .unwrap();

    let error = manager
        .sign_in_again("Test Device")
        .await
        .expect_err("an S3 library takes a new access key instead");

    assert!(matches!(error, LibraryError::Internal(_)), "{error}");
    assert!(presenter.presented.lock().unwrap().is_empty());
}
