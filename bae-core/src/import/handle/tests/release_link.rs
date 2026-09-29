//! A candidate's release link and its draft are separate: only a pick, an
//! unattended pick, keeping the folder's own draft, and unlinking change the
//! link, and nothing done to the draft does.

use super::*;
use crate::import::{CandidateEditField, PendingStanding};

async fn standing(handle: &ImportServiceHandle, key: &str) -> PendingStanding {
    pane(handle, key)
        .await
        .live
        .standing
        .expect("the candidate is on Found")
}

fn linked_to(release_id: &str) -> crate::import::ReleaseLink {
    crate::import::ReleaseLink {
        record: crate::import::MetadataRef::new(
            crate::import::Catalog::MusicBrainz,
            release_id.to_string(),
        ),
        partners: vec![],
    }
}

/// The fixture candidate, picked as `release_id`.
async fn picked_fixture(
    release_id: &str,
) -> (ImportServiceHandle, TempDir, String) {
    let (handle, tmp, key, _hash) = pane_fixture().await;
    super::lookup_choices::seed_mb_release_with_catalog(
        handle.library_manager.providers(),
        release_id,
        "CAT-1",
    );
    super::lookup_choices::pick(&handle, &key, release_id).await;
    assert_eq!(standing(&handle, &key).await, PendingStanding::Identified);
    (handle, tmp, key)
}

/// Reading the draft from the files' tags after a pick replaces the draft
/// and leaves the candidate linked to the release it picked.
#[tokio::test(flavor = "multi_thread")]
async fn reading_the_tags_after_a_pick_keeps_the_link() {
    let (handle, _tmp, key) = picked_fixture("linked-then-tags").await;

    handle.select_candidate_file_tags(key.clone()).await.unwrap();

    let after = pane(&handle, &key).await;
    assert_eq!(
        after.metadata_provenance,
        Some(crate::import::MetadataProvenance::FileMetadata),
        "the draft is the files' tags now"
    );
    assert_eq!(after.release_link, Some(linked_to("linked-then-tags")));
    assert_eq!(standing(&handle, &key).await, PendingStanding::Identified);
    shut_down(handle).await;
}

/// Unlinking clears the link alone: the draft read from the release stays
/// as it is, and the folder is unmatched.
#[tokio::test(flavor = "multi_thread")]
async fn unlinking_keeps_the_draft_and_unmatches_the_folder() {
    let (handle, _tmp, key) = picked_fixture("linked-then-unlinked").await;
    let before = pane(&handle, &key).await;

    handle.unlink_candidate_release(key.clone()).await.unwrap();

    let after = pane(&handle, &key).await;
    assert_eq!(after.release_link, None);
    assert_eq!(after.metadata_draft, before.metadata_draft);
    assert_eq!(after.metadata_provenance, before.metadata_provenance);
    assert_eq!(standing(&handle, &key).await, PendingStanding::Unmatched);
    shut_down(handle).await;
}

/// Typing into the draft and clearing it are draft writes, which never touch
/// the link.
#[tokio::test(flavor = "multi_thread")]
async fn editing_or_clearing_the_draft_keeps_the_link() {
    let (handle, _tmp, key) = picked_fixture("linked-then-edited").await;

    handle
        .set_candidate_edit_field(
            &key,
            crate::import::DraftFieldEdit::Text {
                field: CandidateEditField::AlbumTitle,
                value: "Typed Title".to_string(),
            },
        )
        .await
        .unwrap();
    let edited = pane(&handle, &key).await;
    assert_eq!(edited.metadata_draft.album_title, "Typed Title");
    assert_eq!(edited.release_link, Some(linked_to("linked-then-edited")));

    handle.clear_candidate_metadata(key.clone()).await.unwrap();
    let cleared = pane(&handle, &key).await;
    assert!(cleared.metadata_draft.is_blank());
    assert_eq!(cleared.release_link, Some(linked_to("linked-then-edited")));
    assert_eq!(standing(&handle, &key).await, PendingStanding::Identified);
    shut_down(handle).await;
}
