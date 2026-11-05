//! The import commands a test sends over a test store's folder, and waiting
//! for what they land, for the tests of this crate and the crates above it.

fn import_terminal_ids(progress: &crate::ImportProgress) -> Option<(String, String)> {
    match progress {
        crate::ImportProgress::Complete { id, album_id, .. }
        | crate::ImportProgress::RemoteUploadQueued { id, album_id, .. } => {
            Some((id.clone(), album_id.clone()))
        }
        _ => None,
    }
}

/// Wait for the import worker to finish, returning (release_id, album_id).
///
/// Local imports emit `Complete`; remote imports emit `RemoteUploadQueued`,
/// since remote completion waits on the upload. Panics on failure.
pub async fn wait_for_import_complete(
    progress_rx: &mut crate::ImportProgressFeed,
) -> (String, String) {
    try_wait_for_import_complete(progress_rx)
        .await
        .unwrap_or_else(|error| panic!("Import failed: {error}"))
}

/// Like [`wait_for_import_complete`], returning the failure instead of panicking.
pub async fn try_wait_for_import_complete(
    progress_rx: &mut crate::ImportProgressFeed,
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
    progress_rx: &mut crate::ImportProgressFeed,
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
        candidate_key: crate::CandidateKey::from(b"test".to_vec()),
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
