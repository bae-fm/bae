//! The pane's session: its parts read back as written, a pane command's
//! failure stored until the next command, and identification's results asked
//! for: a stored verdict shown as it stood, else a run owed.

use crate::handle::tests::*;
use crate::identification::tests::*;
use crate::{
    ImportError, MetadataPresentation, PaneCommand, PaneFailure, PaneMove, PaneOutcome, SearchForm,
    SearchTab,
};
use bae_diagnostics::{UiError, UiErrorCategory};

/// The pane's state comes back with the candidate: each write lands on the
/// next read, and a write of one part leaves the others where they were.
#[tokio::test(flavor = "multi_thread")]
async fn the_pane_s_session_reads_back_part_by_part() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    // A picked candidate opens on the draft until told otherwise.
    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().session.presentation,
        MetadataPresentation::Draft
    );

    handle.move_candidate_pane(&key, PaneMove::Search).await.unwrap();
    let form = SearchForm {
        tab: SearchTab::CatalogNumber,
        artist: "Artist".to_string(),
        album: String::new(),
        catalog: "WPCR-80001".to_string(),
        barcode: String::new(),
    };
    handle.set_candidate_search_form(&key, form.clone()).await.unwrap();

    let session = pane(&handle, &key).await.draft().unwrap().session.clone();
    assert_eq!(session.presentation, MetadataPresentation::FindOnline);
    assert_eq!(session.find_online_section, crate::FindOnlineSection::Search);
    assert_eq!(session.search, form);
    assert_eq!(session.error, None);

    // Back to the draft keeps the section Find online opens on.
    handle.move_candidate_pane(&key, PaneMove::Back).await.unwrap();
    let session = pane(&handle, &key).await.draft().unwrap().session.clone();
    assert_eq!(session.presentation, MetadataPresentation::Draft);
    assert_eq!(session.find_online_section, crate::FindOnlineSection::Search);

    shut_down(handle).await;
}

/// A key that names no scanned folder has no session to write.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_write_for_an_unknown_key_is_refused() {
    let (handle, _tmp, _key, _hash) = pane_fixture().await;
    let refused = handle
        .move_candidate_pane(&crate::CandidateKey::test("/nowhere/at/all"), PaneMove::FindOnline)
        .await;
    assert!(refused.is_err());
    shut_down(handle).await;
}

/// A pane command's failure is stored with the candidate as its class and
/// text, beside the command that failed, and survives a read back; the next
/// pane command clears it, whatever it touches.
#[tokio::test(flavor = "multi_thread")]
async fn a_pane_command_s_failure_is_stored_until_the_next_command() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle.move_candidate_pane(&key, PaneMove::FindOnline).await.unwrap();

    let failed = handle
        .run_pane_command(&key, PaneCommand::ReadFileTags, async {
            Err::<(), _>(ImportError::CandidateImportInProgress)
        })
        .await
        .unwrap();
    assert_eq!(failed, PaneOutcome::Failed);
    let session = pane(&handle, &key).await.draft().unwrap().session.clone();
    assert_eq!(
        session.error,
        Some(PaneFailure {
            command: PaneCommand::ReadFileTags,
            error: UiError::diagnostic(
                UiErrorCategory::CandidateImportInProgress,
                ImportError::CandidateImportInProgress
            ),
        })
    );
    assert_eq!(
        session.presentation,
        MetadataPresentation::FindOnline,
        "a stored failure touches nothing else"
    );

    let done = handle
        .run_pane_command(&key, PaneCommand::Import, async { Ok::<_, ImportError>(()) })
        .await
        .unwrap();
    assert_eq!(done, PaneOutcome::Done);
    assert_eq!(pane(&handle, &key).await.draft().unwrap().session.error, None);
    shut_down(handle).await;
}

/// A key that names no stored candidate has no pane failure to clear, and
/// clearing one is not refused: a library release being identified again runs
/// the same search as a folder does.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_the_failure_of_a_key_with_no_candidate_is_nothing() {
    let (handle, _tmp, _key, _hash) = pane_fixture().await;
    handle.clear_pane_failure(&crate::CandidateKey::reidentify("release-1")).await.unwrap();
    shut_down(handle).await;
}

/// Asking for identification's results for a candidate with a stored verdict
/// shows that verdict as it stood, the picked release still the draft's, and
/// asks nothing of the providers: no run is owed.
#[tokio::test(flavor = "multi_thread")]
async fn automatic_shows_a_stored_verdict_without_asking_again() {
    let fixture = Fixture::new("automatic-stored").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture.archive("mb-automatic-1", "rg-automatic-1", &[probed, 0]).await;
    fixture.store_settled_verdict(&dir, "mb-automatic-1", "rg-automatic-1").await;
    let asked = fixture.provider.requests().len();

    assert!(!fixture.import.open_automatic(&key).await.unwrap(), "a stored verdict owes no run");

    assert_eq!(fixture.provider.requests().len(), asked);
    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    assert_eq!(pane.draft().unwrap().session.presentation, crate::MetadataPresentation::FindOnline);
    assert_eq!(
        pane.draft().unwrap().session.find_online_section,
        crate::FindOnlineSection::Automatic
    );
    assert!(
        !matches!(pane.resumed_identify_state, bae_identify::IdentifyState::Idle),
        "the stored verdict is what shows"
    );
    assert!(
        matches!(
            &pane.draft().unwrap().metadata_provenance,
            Some(crate::MetadataProvenance::ExternalRelease { record, .. })
                if record.key == "mb-automatic-1"
        ),
        "the saved pick stays the draft's, for the pane to mark"
    );
}

/// With no stored verdict, asking for identification's results owes a run.
#[tokio::test(flavor = "multi_thread")]
async fn automatic_with_no_stored_verdict_owes_a_run() {
    let fixture = Fixture::new("automatic-first").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.scan(1).await;

    assert!(fixture.import.open_automatic(&key).await.unwrap());
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the candidate reads back")
            .draft()
            .unwrap()
            .session
            .presentation,
        crate::MetadataPresentation::FindOnline
    );
}

/// Import the fixture's candidate, given the artist its draft lacks, and
/// answer with the release it became.
async fn imported(handle: &crate::ImportServiceHandle, key: &crate::CandidateKey) -> String {
    handle
        .set_candidate_album_artists(
            key,
            vec![crate::Credited::plain(crate::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
    let mut events = handle.every_event();
    let import_id = handle.start_import(key).await.unwrap().expect_queued();
    let (release_id, _) = await_import_outcome(&mut events, &import_id)
        .await
        .unwrap_or_else(|error| panic!("the import lands: {error}"));
    release_id
}

/// Whether `answer` says this device already imported the candidate's files,
/// as `release_id`.
fn already_imported<T: std::fmt::Debug>(answer: &Result<T, ImportError>, release_id: &str) -> bool {
    matches!(
        answer,
        Err(ImportError::FilesAlreadyImported { release_id: imported }) if imported == release_id
    )
}

/// A candidate this device imported holds no draft and no pane: a pane
/// command or control that reaches it after the import landed, as one clicked
/// on a pane still drawing the draft does, writes nothing and is answered that
/// its files are in the library, naming the release they became.
#[tokio::test(flavor = "multi_thread")]
async fn an_imported_candidate_s_pane_answers_that_its_files_are_imported() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let release_id = imported(&handle, &key).await;
    let sessions = || async {
        handle.store.every_row_for_test().await.unwrap().remove("import_candidate_session")
    };
    let before = sessions().await;

    let moved = handle.move_candidate_pane(&key, PaneMove::FindOnline).await;
    assert!(already_imported(&moved, &release_id), "{moved:?}");
    let searched = handle.set_candidate_search_form(&key, SearchForm::default()).await;
    assert!(already_imported(&searched, &release_id), "{searched:?}");
    let automatic = handle.open_automatic(&key).await;
    assert!(already_imported(&automatic, &release_id), "{automatic:?}");
    let cleared = handle.clear_pane_failure(&key).await;
    assert!(already_imported(&cleared, &release_id), "{cleared:?}");
    let command = handle
        .run_pane_command(&key, PaneCommand::LinkSharedAlbum, async { Ok::<_, ImportError>(()) })
        .await;
    assert!(already_imported(&command, &release_id), "{command:?}");

    let album = handle.set_candidate_album(&key, None).await;
    assert!(already_imported(&album, &release_id), "{album:?}");
    let artists = handle
        .set_candidate_album_artists(
            &key,
            vec![crate::Credited::plain(crate::ArtistAssignment::named("Other Artist"))],
        )
        .await;
    assert!(already_imported(&artists, &release_id), "{artists:?}");
    let tags = handle.select_candidate_file_tags(key.clone()).await;
    assert!(already_imported(&tags, &release_id), "{tags:?}");
    let again = handle.start_import(&key).await;
    assert!(already_imported(&again, &release_id), "{again:?}");

    assert_eq!(sessions().await, before, "nothing stored a pane for the imported candidate");
    shut_down(handle).await;
}

/// A pane command whose candidate is imported while it runs has no pane to
/// state its failure on: it is answered that the files are in the library.
#[tokio::test(flavor = "multi_thread")]
async fn a_command_whose_candidate_is_imported_meanwhile_answers_that_its_files_are() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let importing = handle.clone();
    let release = std::sync::Arc::new(std::sync::Mutex::new(None));
    let landed = release.clone();
    let key_for_run = key.clone();

    let answer = handle
        .run_pane_command(&key, PaneCommand::KeepOwnDraft, async move {
            *landed.lock().unwrap() = Some(imported(&importing, &key_for_run).await);
            Err::<(), _>(ImportError::CandidateImportInProgress)
        })
        .await;

    let release_id = release.lock().unwrap().clone().expect("the import landed");
    assert!(already_imported(&answer, &release_id), "{answer:?}");
    shut_down(handle).await;
}
