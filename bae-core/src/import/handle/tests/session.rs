use super::*;
use crate::import::{
    ImportError, MetadataPresentation, PaneCommand, PaneFailure, PaneOutcome, SearchForm,
    SearchTab,
};
use crate::ui::{UiError, UiErrorCategory};

/// The pane's state comes back with the candidate: each write lands on the
/// next read, and a write of one part leaves the others where they were.
#[tokio::test(flavor = "multi_thread")]
async fn the_pane_s_session_reads_back_part_by_part() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    // A picked candidate opens on the draft until told otherwise.
    assert_eq!(
        pane(&handle, &key).await.session.presentation,
        MetadataPresentation::Draft
    );

    handle
        .set_candidate_presentation(&key, MetadataPresentation::FindOnline)
        .await
        .unwrap();
    let form = SearchForm {
        tab: SearchTab::CatalogNumber,
        artist: "Artist".to_string(),
        album: String::new(),
        catalog: "WPCR-80001".to_string(),
        barcode: String::new(),
    };
    handle
        .set_candidate_search_form(&key, form.clone())
        .await
        .unwrap();

    let session = pane(&handle, &key).await.session;
    assert_eq!(session.presentation, MetadataPresentation::FindOnline);
    assert_eq!(session.search, form);
    assert_eq!(session.error, None);

    shut_down(handle).await;
}

/// A key that names no scanned folder has no session to write.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_write_for_an_unknown_key_is_refused() {
    let (handle, _tmp, _key, _hash) = pane_fixture().await;
    let refused = handle
        .set_candidate_presentation("/nowhere/at/all", MetadataPresentation::FindOnline)
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
    handle
        .set_candidate_presentation(&key, MetadataPresentation::FindOnline)
        .await
        .unwrap();

    let failed = handle
        .run_pane_command(&key, PaneCommand::MergeArtists, async {
            Err::<(), _>(ImportError::CandidateImportInProgress)
        })
        .await
        .unwrap();
    assert_eq!(failed, PaneOutcome::Failed);
    let session = pane(&handle, &key).await.session;
    assert_eq!(
        session.error,
        Some(PaneFailure {
            command: PaneCommand::MergeArtists,
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
    assert_eq!(pane(&handle, &key).await.session.error, None);
    shut_down(handle).await;
}

/// A key that names no stored candidate has no pane failure to clear, and
/// clearing one is not refused: a library release being identified again runs
/// the same search as a folder does.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_the_failure_of_a_key_with_no_candidate_is_nothing() {
    let (handle, _tmp, _key, _hash) = pane_fixture().await;
    handle
        .clear_pane_failure("reidentify:release-1")
        .await
        .unwrap();
    shut_down(handle).await;
}
