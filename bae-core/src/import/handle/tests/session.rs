use super::*;
use crate::import::{MetadataPresentation, SearchForm, SearchTab};

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
    handle
        .set_candidate_pane_error(&key, Some("the cover would not download".to_string()))
        .await
        .unwrap();

    let session = pane(&handle, &key).await.session;
    assert_eq!(session.presentation, MetadataPresentation::FindOnline);
    assert_eq!(session.search, form);
    assert_eq!(
        session.error.as_deref(),
        Some("the cover would not download")
    );

    // Clearing the banner touches nothing else.
    handle.set_candidate_pane_error(&key, None).await.unwrap();
    let session = pane(&handle, &key).await.session;
    assert_eq!(session.error, None);
    assert_eq!(session.presentation, MetadataPresentation::FindOnline);
    assert_eq!(session.search, form);

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
