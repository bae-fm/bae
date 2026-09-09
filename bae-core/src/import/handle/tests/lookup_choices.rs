use super::*;
use crate::import::{ChoiceChange, LookupChoices};

/// The choices a person makes about what a run asks come back with the
/// candidate: the pane reads them off its own value rather than out of a run
/// that may not be there.
#[tokio::test(flavor = "multi_thread")]
async fn the_candidate_s_lookup_choices_read_back_on_its_pane() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert_eq!(
        pane(&handle, &key).await.lookup_choices,
        LookupChoices::default(),
        "a candidate nobody has chosen for excludes nothing and chooses nothing"
    );

    let choices = LookupChoices {
        disc_id_excluded: false,
        barcode_excluded: true,
        chosen_catalogs: vec!["WPCR-80001".to_string()],
        discounted_catalogs: vec!["LBL-9".to_string()],
    };
    handle
        .set_candidate_lookup_choices(&key, choices.clone())
        .await
        .unwrap();

    assert_eq!(pane(&handle, &key).await.lookup_choices, choices);
    shut_down(handle).await;
}

/// A key that names no scanned folder has no candidate to hold a choice.
#[tokio::test(flavor = "multi_thread")]
async fn lookup_choices_for_an_unknown_key_are_refused() {
    let (handle, _tmp, _key, _hash) = pane_fixture().await;
    let refused = handle
        .set_candidate_lookup_choices("/nowhere/at/all", LookupChoices::default())
        .await;
    assert!(refused.is_err());
    shut_down(handle).await;
}

/// The two halves of the value are answered apart. Changing what the run
/// looks up says so, because the answers in hand were produced by a question
/// nobody is asking any more; striking a number out of the candidate's text
/// says the answers stand and only their ranking changed.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_change_to_what_a_run_looks_up_asks_for_another_run() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let looked_up = LookupChoices {
        disc_id_excluded: false,
        barcode_excluded: true,
        chosen_catalogs: vec!["WPCR-80001".to_string()],
        discounted_catalogs: Vec::new(),
    };
    assert_eq!(
        handle
            .set_candidate_lookup_choices(&key, looked_up.clone())
            .await
            .unwrap(),
        ChoiceChange::Lookups
    );
    assert_eq!(
        handle
            .set_candidate_lookup_choices(
                &key,
                LookupChoices {
                    discounted_catalogs: vec!["LBL-9".to_string()],
                    ..looked_up.clone()
                }
            )
            .await
            .unwrap(),
        ChoiceChange::Ranking
    );
    assert_eq!(
        handle
            .set_candidate_lookup_choices(&key, looked_up)
            .await
            .unwrap(),
        ChoiceChange::Ranking,
        "writing the same lookups again asks nothing new of the providers"
    );
    shut_down(handle).await;
}
