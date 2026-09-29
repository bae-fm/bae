//! Keeping a folder's own draft over what its lookup offered: a decision
//! that goes with the verdict it answers.

use super::*;
use crate::import::{NeedsYouReason, PendingStanding};

/// Store a verdict that found nothing for the fixture's candidate.
async fn store_not_found(handle: &ImportServiceHandle, hash: &str) {
    let prep = handle
        .library_manager
        .load_import_candidate_preparation(hash)
        .await
        .unwrap()
        .expect("the fixture candidate is prepared");
    let stored = handle
        .preparations
        .store_verdict(&crate::db::NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: prep.file_edit_revision,
            folder_path: String::new(),
            verdict: crate::identify::TerminalVerdict::NotFoundAnywhere { ledger: None },
            signals: crate::signals::Signals {
                origin: crate::signals::AudioOrigin::default(),
                disc_id: crate::signals::DiscIdSignal::Absent,
                barcode: crate::signals::BarcodeSignal::Absent,
                text: crate::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            metadata: None,
        })
        .await
        .unwrap();
    assert!(stored, "the verdict lands on the fixture candidate");
}

async fn standing(handle: &ImportServiceHandle, key: &str) -> PendingStanding {
    pane(handle, key)
        .await
        .live
        .standing
        .expect("the candidate is on Found")
}

/// Keep my info leaves the draft as it was and makes the folder Unmatched.
#[tokio::test(flavor = "multi_thread")]
async fn keeping_the_draft_makes_the_folder_unmatched() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_not_found(&handle, &hash).await;
    assert_eq!(
        standing(&handle, &key).await,
        PendingStanding::NeedsYou {
            reason: NeedsYouReason::NotFound
        }
    );
    let before = pane(&handle, &key).await.metadata_draft;

    handle.keep_candidate_draft(key.clone()).await.unwrap();

    assert_eq!(standing(&handle, &key).await, PendingStanding::Unmatched);
    assert_eq!(pane(&handle, &key).await.metadata_draft, before);
    shut_down(handle).await;
}

/// A new verdict replaces the one the decision answered, so the decision
/// goes with it.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_verdict_starts_without_the_kept_draft() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_not_found(&handle, &hash).await;
    handle.keep_candidate_draft(key.clone()).await.unwrap();
    assert_eq!(standing(&handle, &key).await, PendingStanding::Unmatched);

    store_not_found(&handle, &hash).await;

    assert_eq!(
        standing(&handle, &key).await,
        PendingStanding::NeedsYou {
            reason: NeedsYouReason::NotFound
        }
    );
    shut_down(handle).await;
}

/// With no lookup stored there is nothing to keep the draft over.
#[tokio::test(flavor = "multi_thread")]
async fn keeping_the_draft_with_no_lookup_is_refused() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert!(handle.keep_candidate_draft(key.clone()).await.is_err());
    assert_eq!(standing(&handle, &key).await, PendingStanding::NotLookedUp);
    shut_down(handle).await;
}

/// Picking a release after keeping the draft answers the folder with it:
/// Identified, and the kept draft stands no more.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_after_keeping_the_draft_is_identified() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_not_found(&handle, &hash).await;
    handle.keep_candidate_draft(key.clone()).await.unwrap();
    super::lookup_choices::seed_mb_release_with_catalog(
        handle.library_manager.providers(),
        "kept-then-picked",
        "CAT-1",
    );

    super::lookup_choices::pick(&handle, &key, "kept-then-picked").await;

    assert_eq!(standing(&handle, &key).await, PendingStanding::Identified);
    let kept = handle
        .library_manager
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .and_then(|state| state.identify)
        .expect("the verdict stands")
        .kept_own_draft;
    assert!(!kept, "the pick answers the folder over the kept draft");
    shut_down(handle).await;
}
