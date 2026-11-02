//! Running the import service over a folder and waiting for what it lands.

use crate::discogs::seed_discogs_test_release;
use crate::library::setup_test_library_with_album_dir;

fn import_terminal_ids(progress: &bae_core::import::ImportProgress) -> Option<(String, String)> {
    match progress {
        bae_core::import::ImportProgress::Complete { id, album_id, .. }
        | bae_core::import::ImportProgress::RemoteUploadQueued { id, album_id, .. } => {
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
    database: &bae_core::db::Database,
    catalogs: &bae_core::import::CatalogSessions,
    closing: &bae_core::library::CancellationToken,
) -> bae_core::import::ImportServiceHandle {
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
/// [`ImportService`]: bae_core::import::ImportService
pub async fn start_import_service(
    runtime_handle: tokio::runtime::Handle,
    database: &bae_core::db::Database,
    catalogs: &bae_core::import::CatalogSessions,
    closing: &bae_core::library::CancellationToken,
    http: bae_util::http::Http,
) -> bae_core::import::ImportServiceHandle {
    bae_core::import::ImportService::start(
        runtime_handle,
        database.clone(),
        catalogs.clone(),
        bae_catalogs::remote_image::RemoteImageCache::for_test(http),
        bae_core::diagnostics::Diagnostics::noop(),
        bae_core::import::CandidatePreparations::new(database.clone()),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidIds),
        closing.child_token(),
    )
    .await
    .expect("test import service starts")
}

/// Store the Discogs key seeded Discogs fixtures are asked for with, through
/// `catalogs`, the sessions the test built its library over.
pub async fn configure_test_discogs(catalogs: &bae_core::import::CatalogSessions) {
    catalogs
        .store_discogs_key("test-discogs-token", bae_core::preferences::DiscogsValidation::Valid)
        .await
        .expect("test Discogs key is stored");
}

/// Wait for the import worker to finish, returning (release_id, album_id).
///
/// Local imports emit `Complete`; remote imports emit `RemoteUploadQueued`,
/// since remote completion waits on the upload. Panics on failure.
pub async fn wait_for_import_complete(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> (String, String) {
    try_wait_for_import_complete(progress_rx)
        .await
        .unwrap_or_else(|error| panic!("Import failed: {error}"))
}

/// Like [`wait_for_import_complete`], returning the failure instead of panicking.
pub async fn try_wait_for_import_complete(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> Result<(String, String), String> {
    let end = wait_for_import_end(progress_rx).await;
    match import_terminal_ids(&end) {
        Some(ids) => Ok(ids),
        None => match end {
            bae_core::import::ImportProgress::Failed { error, .. } => Err(error),
            _ => unreachable!("an import ends complete, queued for upload, or failed"),
        },
    }
}

/// The import's first `Complete`, `RemoteUploadQueued` or `Failed`. Panics if
/// the progress ends without one or none comes before [`crate::HANG_GUARD`].
pub async fn wait_for_import_end(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> bae_core::import::ImportProgress {
    let ended = async {
        while let Some(progress) = progress_rx.recv().await {
            if import_terminal_ids(&progress).is_some()
                || matches!(progress, bae_core::import::ImportProgress::Failed { .. })
            {
                return progress;
            }
        }
        panic!("Progress channel closed without completion");
    };
    tokio::time::timeout(crate::HANG_GUARD, ended)
        .await
        .unwrap_or_else(|_| panic!("import did not end within {:?}", crate::HANG_GUARD))
}

/// What a test import reads the candidate's draft from before importing it.
#[derive(Debug, Clone)]
pub enum DraftSource {
    /// The files' own tags; the candidate stays linked to no release.
    FileTags,
    /// A picked release, which the candidate is linked to and whose draft it
    /// reads.
    Pick(bae_core::identify::PressingLink),
}

impl DraftSource {
    /// `command` reading its draft from this source.
    pub fn onto(self, command: bae_core::import::ImportCommand) -> bae_core::import::ImportCommand {
        match self {
            Self::FileTags => {
                bae_core::import::ImportCommand { pick: None, read_file_tags: true, ..command }
            }
            Self::Pick(link) => bae_core::import::ImportCommand {
                pick: Some(link),
                read_file_tags: false,
                ..command
            },
        }
    }
}

/// The pick of an import identified by a Discogs release, with no partner
/// catalog alongside it — what a [`seed_discogs_test_release`] fixture is
/// imported under.
pub fn discogs_release(release_id: impl Into<String>) -> DraftSource {
    DraftSource::Pick(bae_core::identify::PressingLink {
        record: bae_core::catalog_model::records::LookupRef::new(
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
///     ..support::folder_import(&import_id, album_dir, DraftSource::FileTags)
/// }
/// ```
pub fn folder_import(
    import_id: &str,
    folder: impl Into<std::path::PathBuf>,
    draft_source: DraftSource,
) -> bae_core::import::ImportCommand {
    draft_source.onto(bae_core::import::ImportCommand {
        import_id: import_id.to_string(),
        candidate_key: bae_core::import::CandidateKey::from_bytes(b"test".to_vec()),
        source: bae_core::import::release_candidate::CandidateSource {
            path: folder.into(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
            parts: Vec::new(),
        },
        selected_cover: None,
        destination: bae_core::import::ImportDestination::Local,
        pick: None,
        read_file_tags: false,
        user_edit: None,
    })
}

/// Send one folder import and wait for the worker to finish it, returning
/// (release_id, album_id). The service is started here and dropped on return —
/// by then the import has landed everything it writes.
async fn send_folder_import_and_wait(
    database: &bae_core::db::Database,
    catalogs: &bae_core::import::CatalogSessions,
    closing: &bae_core::library::CancellationToken,
    album_dir: std::path::PathBuf,
    release_id_key: String,
    candidate_key: &str,
    import_id: String,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let import_handle =
        start_test_import(tokio::runtime::Handle::current(), database, catalogs, closing).await;
    import_handle
        .send_command(bae_core::import::ImportCommand {
            candidate_key: bae_core::import::CandidateKey::from_bytes(
                candidate_key.as_bytes().to_vec(),
            ),
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
    database: &bae_core::db::Database,
    catalogs: &bae_core::import::CatalogSessions,
    closing: &bae_core::library::CancellationToken,
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

/// What an import landed, beside the library it landed in. Holds no service of
/// its own, so a fixture can keep it whole while handing the library's parts
/// to whatever it starts.
pub struct ImportedRelease {
    pub track_ids: Vec<String>,
    pub release_id: String,
    /// The folder the audio was written into, for a test that reads a real file.
    pub album_dir: std::path::PathBuf,
    /// Owns the library's files, so it must outlive the manager.
    pub temp_dir: tempfile::TempDir,
}

/// Import one Discogs-identified release from a folder, on the calling test's
/// runtime, and return the library beside what it landed: open a fresh library, seed `release` for the fake Discogs
/// endpoint, write the audio with `generate_files`, run the import to
/// completion, and read back the tracks it landed. The arrange every playback
/// test binary starts from.
pub async fn imported_release_setup<G>(
    release: bae_catalogs::discogs::DiscogsRelease,
    candidate_key: &str,
    import_id: String,
    generate_files: G,
) -> Result<
    (
        (
            bae_core::library::LibraryManager,
            bae_core::db::Database,
            bae_core::import::CatalogSessions,
            bae_core::library::CancellationToken,
        ),
        ImportedRelease,
    ),
    Box<dyn std::error::Error>,
>
where
    G: FnOnce(&std::path::Path),
{
    imported_release_setup_with(
        &crate::offline_musicbrainz(),
        &crate::offline_discogs(),
        release,
        candidate_key,
        import_id,
        generate_files,
    )
    .await
}

/// [`imported_release_setup`] over a library asking `musicbrainz` and
/// `discogs`, the test's own fakes, which it keeps to seed the releases it
/// imports next.
pub async fn imported_release_setup_with<G>(
    musicbrainz: &std::sync::Arc<bae_catalogs::musicbrainz::MusicBrainz>,
    discogs: &std::sync::Arc<bae_catalogs::discogs::client::Discogs>,
    release: bae_catalogs::discogs::DiscogsRelease,
    candidate_key: &str,
    import_id: String,
    generate_files: G,
) -> Result<
    (
        (
            bae_core::library::LibraryManager,
            bae_core::db::Database,
            bae_core::import::CatalogSessions,
            bae_core::library::CancellationToken,
        ),
        ImportedRelease,
    ),
    Box<dyn std::error::Error>,
>
where
    G: FnOnce(&std::path::Path),
{
    let ((library_manager, database, catalogs, closing), album_dir, temp_dir) =
        setup_test_library_with_album_dir(crate::providers_asking(musicbrainz, discogs)).await;

    let release_id_key = seed_discogs_test_release(discogs, musicbrainz, release);
    generate_files(&album_dir);

    let (release_id, _album_id) = send_folder_import_and_wait(
        &database,
        &catalogs,
        &closing,
        album_dir.clone(),
        release_id_key,
        candidate_key,
        import_id,
    )
    .await?;
    let tracks = library_manager.get_tracks_for_release(&release_id).await?;
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();

    Ok((
        (library_manager, database, catalogs, closing),
        ImportedRelease { track_ids, release_id, album_dir, temp_dir },
    ))
}
