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

/// Start an [`ImportService`] over a test library manager — the shared setup
/// every import/playback test binary needs, so none of them keep a private copy
/// that can drift from the others. The cover-art client comes off the manager,
/// which builds a hermetic one.
///
/// [`ImportService`]: bae_core::import::ImportService
pub async fn start_test_import(
    runtime_handle: tokio::runtime::Handle,
    library_manager: bae_core::library::LibraryManager,
) -> bae_core::import::ImportServiceHandle {
    configure_test_discogs(&library_manager);
    library_manager
        .start_import_service(runtime_handle.clone())
        .await
        .expect("test import service starts")
}

/// Configure the provider credential used by seeded Discogs fixtures through
/// the same library-manager capability production uses.
pub fn configure_test_discogs(library_manager: &bae_core::library::LibraryManager) {
    library_manager
        .set_discogs_key(
            "test-discogs-token",
            bae_core::config::DiscogsValidation::Valid,
        )
        .expect("test Discogs key is stored through the library manager");
}

/// Wait for the import worker to finish, returning (release_id, album_id).
///
/// Local imports emit `Complete`. Remote imports emit `RemoteUploadQueued`: the
/// import worker is finished, while remote completion waits for coven upload
/// confirmation. Panics on failure.
pub async fn wait_for_import_complete(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> (String, String) {
    while let Some(progress) = progress_rx.recv().await {
        if let Some(ids) = import_terminal_ids(&progress) {
            return ids;
        }
        if let bae_core::import::ImportProgress::Failed { error, .. } = &progress {
            panic!("Import failed: {}", error);
        }
    }
    panic!("Progress channel closed without completion");
}

/// Like `wait_for_import_complete` but returns Result instead of panicking.
///
/// Used by test fixtures that catch setup errors gracefully (e.g., returning
/// early from a test when a fixture file fails validation).
pub async fn try_wait_for_import_complete(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> Result<(String, String), String> {
    while let Some(progress) = progress_rx.recv().await {
        if let Some(ids) = import_terminal_ids(&progress) {
            return Ok(ids);
        }
        if let bae_core::import::ImportProgress::Failed { error, .. } = &progress {
            return Err(error.clone());
        }
    }
    Err("Progress channel closed without completion".to_string())
}

/// The provenance of an import identified by a Discogs release, with no
/// partner source alongside it — what a [`seed_discogs_test_release`] fixture
/// is imported under.
pub fn discogs_release(release_id: impl Into<String>) -> bae_core::import::MetadataProvenance {
    bae_core::import::MetadataProvenance::ExternalRelease {
        source: bae_core::import::MetadataSource::Discogs,
        release_id: release_id.into(),
        partners: vec![],
    }
}

/// The command a test sends to import one folder, in the shape almost every
/// test wants it: a recursive scan of `folder` under candidate key `"test"`,
/// no chosen cover, stored locally, unpinned, and no user edit over the
/// metadata.
///
/// A test that differs in one of those names that field and takes the rest
/// from here:
///
/// ```ignore
/// ImportCommand {
///     storage_mode: StorageMode::Remote,
///     ..support::folder_import(&import_id, album_dir, MetadataProvenance::FileTags)
/// }
/// ```
pub fn folder_import(
    import_id: &str,
    folder: impl Into<std::path::PathBuf>,
    metadata_provenance: bae_core::import::MetadataProvenance,
) -> bae_core::import::ImportCommand {
    bae_core::import::ImportCommand {
        import_id: import_id.to_string(),
        candidate_key: "test".to_string(),
        source: bae_core::import::release_candidate::CandidateSource::Folder {
            path: folder.into(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
        },
        selected_cover: None,
        storage_mode: bae_core::import::StorageMode::Local,
        pin: false,
        metadata_provenance: Some(metadata_provenance),
        user_edit: None,
    }
}

/// Send one folder import and wait for the worker to finish it, returning
/// (release_id, album_id). The service is started here and dropped on return —
/// by then the import has landed everything it writes.
async fn send_folder_import_and_wait(
    library_manager: &bae_core::library::LibraryManager,
    album_dir: std::path::PathBuf,
    release_id_key: String,
    candidate_key: &str,
    import_id: String,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let import_handle =
        start_test_import(tokio::runtime::Handle::current(), library_manager.clone()).await;
    import_handle
        .send_command(bae_core::import::ImportCommand {
            candidate_key: candidate_key.to_string(),
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
    library_manager: &bae_core::library::LibraryManager,
    album_dir: impl Into<std::path::PathBuf>,
    release_id_key: String,
) -> String {
    let (release_id, _album_id) = send_folder_import_and_wait(
        library_manager,
        album_dir.into(),
        release_id_key,
        "test",
        uuid::Uuid::new_v4().to_string(),
    )
    .await
    .expect("the test import service accepts and completes the folder import");
    release_id
}

/// What an import landed, beside the library manager it landed through. Holds
/// no service of its own, so a fixture can keep it whole while handing the
/// manager to whatever it starts.
pub struct ImportedRelease {
    pub track_ids: Vec<String>,
    pub release_id: String,
    /// The folder the audio was written into, for a test that reads a real file.
    pub album_dir: std::path::PathBuf,
    /// Owns the library's files, so it must outlive the manager.
    pub temp_dir: tempfile::TempDir,
}

/// Import one Discogs-identified release from a folder, on the calling test's
/// runtime: open a fresh library, apply whatever settings the import must see
/// with `configure`, seed `release` for the fake Discogs endpoint, write the
/// audio with `generate_files`, run the import to completion, and read back the
/// tracks it landed. The arrange every playback test binary starts from.
pub async fn imported_release_setup<G, C>(
    release: bae_core::discogs::DiscogsRelease,
    candidate_key: &str,
    import_id: String,
    generate_files: G,
    configure: C,
) -> Result<(bae_core::library::LibraryManager, ImportedRelease), Box<dyn std::error::Error>>
where
    G: FnOnce(&std::path::Path),
    C: FnOnce(&bae_core::library::LibraryManager) -> Result<(), Box<dyn std::error::Error>>,
{
    let (library_manager, album_dir, temp_dir) = setup_test_library_with_album_dir().await;
    configure(&library_manager)?;

    let release_id_key = seed_discogs_test_release(release);
    generate_files(&album_dir);

    let (release_id, _album_id) = send_folder_import_and_wait(
        &library_manager,
        album_dir.clone(),
        release_id_key,
        candidate_key,
        import_id,
    )
    .await?;
    let tracks = library_manager.get_tracks_for_release(&release_id).await?;
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();

    Ok((
        library_manager,
        ImportedRelease {
            track_ids,
            release_id,
            album_dir,
            temp_dir,
        },
    ))
}
