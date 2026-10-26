//! A command whose write of a candidate's standing fails fails to the person
//! who asked, and leaves the candidate as it stood.

use super::*;

/// An Import whose claim cannot be written is refused with the failure: no
/// import owns the candidate, none is queued, and the candidate reads as it
/// did before it was asked for.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_whose_claim_cannot_be_written_fails_and_claims_nothing() {
    let (handle, tmp, key, _hash) = pane_fixture().await;
    let before = handle.candidate_runtime(&key).await.unwrap();
    let mut events = handle.every_event();
    crate::import::candidate_runtime::tests::fail_standing_writes(
        tmp.path(),
        &[
            ("INSERT", "NEW.import = 'queued'"),
            ("UPDATE", "NEW.import = 'queued'"),
        ],
    )
    .await;

    let error = handle.start_import(&key).await.unwrap_err();

    assert!(error.to_string().contains("the disk is full"), "{error}");
    assert_eq!(handle.candidate_runtime(&key).await.unwrap(), before);
    assert_eq!(handle.standing_facts_for_test(&key).await, Default::default());
    assert!(
        !std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, ImportEvent::ImportProgress { .. })),
        "no import was queued"
    );
    shut_down(handle).await;
}

/// A cancel of an identification whose end cannot be written fails to the
/// person who asked, and the candidate still reads as being identified.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_whose_end_cannot_be_written_fails_and_ends_nothing() {
    let (handle, tmp, key, _hash) = pane_fixture().await;
    admit(&handle, &key).await;
    start_run(&handle, &key, crate::identify::IdentifyRunId::for_test(1)).await;
    crate::import::candidate_runtime::tests::fail_standing_writes(
        tmp.path(),
        &[
            ("DELETE", "OLD.identification = 'running'"),
            ("UPDATE", "OLD.identification = 'running'"),
        ],
    )
    .await;

    let error = handle.cancel_identification(&key).await.unwrap_err();

    assert!(error.to_string().contains("the disk is full"), "{error}");
    assert_eq!(
        handle.standing_facts_for_test(&key).await.identification,
        Some(crate::import::IdentificationStatus::Running)
    );
    shut_down(handle).await;
}
