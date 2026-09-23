use super::*;
use crate::import::MetadataAuthor;

/// Who wrote the draft travels on the pane's own value, so a surface reading
/// the detail knows whether the pick in front of it is the person's or
/// identification's without inferring it from anything else.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_the_person_made_names_them_as_the_author() {
    let StoredCandidate {
        handle,
        key,
        tmp: _tmp,
        ..
    } = stored_candidate().await;
    assert_eq!(
        pane(&handle, &key).await.metadata_author,
        MetadataAuthor::Nobody,
        "a candidate nobody has picked for carries no author"
    );

    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileMetadata,
        )
        .await
        .unwrap();

    assert_eq!(
        pane(&handle, &key).await.metadata_author,
        MetadataAuthor::Person
    );
    shut_down(handle).await;
}

/// Clearing the draft is the person's act on it like a pick is: the blank
/// draft it leaves is theirs, not a draft nobody has touched.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_the_draft_leaves_the_person_its_author() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert_eq!(
        pane(&handle, &key).await.metadata_author,
        MetadataAuthor::Person
    );

    handle.clear_candidate_metadata(key.clone()).await.unwrap();

    let cleared = pane(&handle, &key).await;
    assert_eq!(cleared.metadata_author, MetadataAuthor::Person);
    assert_eq!(cleared.metadata_provenance, None);
    shut_down(handle).await;
}
