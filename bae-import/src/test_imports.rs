//! Running the import service over a test store's folder and waiting for
//! what it lands, for the tests of this crate and the crates above it.

fn import_terminal_ids(progress: &crate::ImportProgress) -> Option<(String, String)> {
    match progress {
        crate::ImportProgress::Complete { id, album_id, .. }
        | crate::ImportProgress::RemoteUploadQueued { id, album_id, .. } => {
            Some((id.clone(), album_id.clone()))
        }
        _ => None,
    }
}

/// [`start_import_service`] with the Discogs key stored through `catalogs`,
/// downloading images over a transport that reaches nothing — the shared
/// setup every import/playback test binary needs, so none of them keep a
/// private copy that can drift from the others.
pub async fn start_test_import(
    runtime_handle: tokio::runtime::Handle,
    database: &bae_db::Database,
    catalogs: &crate::CatalogSessions,
    closing: &tokio_util::sync::CancellationToken,
) -> crate::ImportServiceHandle {
    configure_test_discogs(catalogs).await;
    start_import_service(
        runtime_handle,
        database,
        catalogs,
        closing,
        bae_util::http::Http::for_test(),
    )
    .await
}

/// Start an [`ImportService`] over a test library's store, asking the catalogs
/// through `catalogs`, the sessions the library was opened with, and
/// downloading images over `http`. The library's `closing` token ends the
/// service with the library.
///
/// [`ImportService`]: crate::ImportService
pub async fn start_import_service(
    runtime_handle: tokio::runtime::Handle,
    database: &bae_db::Database,
    catalogs: &crate::CatalogSessions,
    closing: &tokio_util::sync::CancellationToken,
    http: bae_util::http::Http,
) -> crate::ImportServiceHandle {
    crate::ImportService::start(
        runtime_handle,
        database.clone(),
        catalogs.clone(),
        bae_catalogs::remote_image::RemoteImageCache::for_test(http),
        bae_diagnostics::Diagnostics::noop(),
        crate::CandidatePreparations::new(database.clone()),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidIds),
        closing.child_token(),
    )
    .await
    .expect("test import service starts")
}

/// Store the Discogs key seeded Discogs fixtures are asked for with, through
/// `catalogs`, the sessions the test built its library over.
pub async fn configure_test_discogs(catalogs: &crate::CatalogSessions) {
    catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .expect("test Discogs key is stored");
}

/// Wait for the import worker to finish, returning (release_id, album_id).
///
/// Local imports emit `Complete`; remote imports emit `RemoteUploadQueued`,
/// since remote completion waits on the upload. Panics on failure.
pub async fn wait_for_import_complete(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::ImportProgress>,
) -> (String, String) {
    try_wait_for_import_complete(progress_rx)
        .await
        .unwrap_or_else(|error| panic!("Import failed: {error}"))
}

/// Like [`wait_for_import_complete`], returning the failure instead of panicking.
pub async fn try_wait_for_import_complete(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::ImportProgress>,
) -> Result<(String, String), String> {
    let end = wait_for_import_end(progress_rx).await;
    match import_terminal_ids(&end) {
        Some(ids) => Ok(ids),
        None => match end {
            crate::ImportProgress::Failed { error, .. } => Err(error),
            _ => unreachable!("an import ends complete, queued for upload, or failed"),
        },
    }
}

/// The import's first `Complete`, `RemoteUploadQueued` or `Failed`. Panics if
/// the progress ends without one or none comes before [`bae_util::test_channels::HANG_GUARD`].
pub async fn wait_for_import_end(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::ImportProgress>,
) -> crate::ImportProgress {
    let ended = async {
        while let Some(progress) = progress_rx.recv().await {
            if import_terminal_ids(&progress).is_some()
                || matches!(progress, crate::ImportProgress::Failed { .. })
            {
                return progress;
            }
        }
        panic!("Progress channel closed without completion");
    };
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, ended).await.unwrap_or_else(|_| {
        panic!("import did not end within {:?}", bae_util::test_channels::HANG_GUARD)
    })
}

/// What a test import reads the candidate's draft from before importing it.
#[derive(Debug, Clone)]
pub enum DraftSource {
    /// The files' own tags; the candidate stays linked to no release.
    FileTags,
    /// A picked release, which the candidate is linked to and whose draft it
    /// reads.
    Pick(bae_identify::PressingLink),
}

impl DraftSource {
    /// `command` reading its draft from this source.
    pub fn onto(self, command: crate::ImportCommand) -> crate::ImportCommand {
        match self {
            Self::FileTags => crate::ImportCommand { pick: None, read_file_tags: true, ..command },
            Self::Pick(link) => {
                crate::ImportCommand { pick: Some(link), read_file_tags: false, ..command }
            }
        }
    }
}

/// The pick of an import identified by a Discogs release, with no partner
/// catalog alongside it — what a
/// [`bae_catalogs::fake_catalogs::FakeCatalogs::seed_discogs_test_release`]
/// fixture is imported under.
pub fn discogs_release(release_id: impl Into<String>) -> DraftSource {
    DraftSource::Pick(bae_identify::PressingLink {
        record: bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::Discogs,
            release_id.into(),
        ),
        partners: vec![],
    })
}

/// The command a test sends to import one folder, in the shape almost every
/// test wants it: a recursive scan of `folder` under candidate key `"test"`,
/// no chosen cover, stored locally, and no user edit over the metadata.
///
/// A test that differs in one of those names that field and takes the rest
/// from here:
///
/// ```ignore
/// ImportCommand {
///     destination: ImportDestination::Remote { pin: false },
///     ..bae_import::test_imports::folder_import(&import_id, album_dir, DraftSource::FileTags)
/// }
/// ```
pub fn folder_import(
    import_id: &str,
    folder: impl Into<std::path::PathBuf>,
    draft_source: DraftSource,
) -> crate::ImportCommand {
    draft_source.onto(crate::ImportCommand {
        import_id: import_id.to_string(),
        candidate_key: crate::CandidateKey::from_bytes(b"test".to_vec()),
        source: crate::release_candidate::CandidateSource {
            path: folder.into(),
            scope: crate::ReleaseFileScope::Recursive,
            parts: Vec::new(),
        },
        selected_cover: None,
        destination: crate::ImportDestination::Local,
        pick: None,
        read_file_tags: false,
        user_edit: None,
    })
}

/// Send one folder import of `album_dir` as the already-seeded Discogs release
/// `release_id_key`, under `candidate_key` and `import_id`, and wait for the
/// worker to finish it, returning (release_id, album_id). The service is
/// started here and dropped on return — by then the import has landed
/// everything it writes.
pub async fn send_folder_import_and_wait(
    database: &bae_db::Database,
    catalogs: &crate::CatalogSessions,
    closing: &tokio_util::sync::CancellationToken,
    album_dir: std::path::PathBuf,
    release_id_key: String,
    candidate_key: &str,
    import_id: String,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let import_handle =
        start_test_import(tokio::runtime::Handle::current(), database, catalogs, closing).await;
    import_handle
        .send_command(crate::ImportCommand {
            candidate_key: crate::CandidateKey::from_bytes(candidate_key.as_bytes().to_vec()),
            ..folder_import(&import_id, album_dir, discogs_release(release_id_key))
        })
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    let mut progress_rx = import_handle.subscribe_import(import_id);
    Ok(wait_for_import_complete(&mut progress_rx).await)
}

/// Import `album_dir` as the already-seeded Discogs release `release_id_key`,
/// under candidate key `"test"` and a fresh import id, and return the release id
/// the import landed. Panics if the import fails.
pub async fn import_folder_and_wait(
    database: &bae_db::Database,
    catalogs: &crate::CatalogSessions,
    closing: &tokio_util::sync::CancellationToken,
    album_dir: impl Into<std::path::PathBuf>,
    release_id_key: String,
) -> String {
    let (release_id, _album_id) = send_folder_import_and_wait(
        database,
        catalogs,
        closing,
        album_dir.into(),
        release_id_key,
        "test",
        uuid::Uuid::new_v4().to_string(),
    )
    .await
    .expect("the test import service accepts and completes the folder import");
    release_id
}
