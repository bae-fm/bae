use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_prepared_before_a_claim_cannot_land_after_the_claim() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let title_before = pane(&handle, &key).await.metadata_draft.album_title;
    let commit = handle.folder_state_commit.lock().await;
    let edit = tokio::spawn({
        let handle = handle.clone();
        let key = key.clone();
        async move {
            handle
                .set_candidate_edit_field(
                    &key,
                    crate::import::CandidateEditField::AlbumTitle,
                    "Racing edit".to_string(),
                )
                .await
        }
    });
    tokio::task::yield_now().await;
    handle.runtime.claim_for_import(&key);
    drop(commit);

    assert!(matches!(
        edit.await.unwrap(),
        Err(crate::import::ImportError::CandidateImportInProgress)
    ));
    assert_eq!(
        pane(&handle, &key).await.metadata_draft.album_title,
        title_before
    );
    shut_down(handle).await;
}
