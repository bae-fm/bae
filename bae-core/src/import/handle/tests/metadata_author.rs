use super::*;
use crate::import::MetadataAuthor;

/// Who wrote the draft travels on the pane's own value, so a surface reading
/// the detail knows whether the pick in front of it is the person's or
/// identification's without inferring it from anything else.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_the_person_made_names_them_as_the_author() {
    let StoredCandidate { handle, key, tmp: _tmp, .. } = stored_candidate().await;
    assert_eq!(
        pane(&handle, &key).await.metadata_author,
        MetadataAuthor::Nobody,
        "a candidate nobody has picked for carries no author"
    );

    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileTags,
        )
        .await
        .unwrap();

    assert_eq!(
        pane(&handle, &key).await.metadata_author,
        MetadataAuthor::User
    );
    shut_down(handle).await;
}

/// Clearing the draft takes the pick away, and the author with it: the blank
/// draft is anybody's to fill again.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_the_draft_leaves_it_unclaimed() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert_eq!(
        pane(&handle, &key).await.metadata_author,
        MetadataAuthor::User
    );

    handle.clear_candidate_metadata(key.clone()).await.unwrap();

    assert_eq!(
        pane(&handle, &key).await.metadata_author,
        MetadataAuthor::Nobody
    );
    shut_down(handle).await;
}
