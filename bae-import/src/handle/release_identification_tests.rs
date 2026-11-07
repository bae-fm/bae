use crate::handle::tests::*;

/// A library release's re-identify session keeps its choices in core: each
/// change is made to the ones before it, a run started afterwards reads them
/// without being handed them, and ending the session forgets them.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_session_keeps_its_choices_in_core() {
    use bae_identify::LookupChoiceEdit;
    let (parts, _store, _temp_dir) = setup_test_import().await;
    let handle = parts.start().await.unwrap();
    let key = crate::CandidateKey::reidentify("release-1");
    let release = "release-1".to_string();

    handle
        .edit_release_lookup_choices(release.clone(), LookupChoiceEdit::ToggleDiscId)
        .await
        .unwrap();
    handle
        .edit_release_lookup_choices(
            release.clone(),
            LookupChoiceEdit::ToggleCatalog { number: "WPCR-80001".to_string() },
        )
        .await
        .unwrap();
    handle.identify_release(release.clone()).await.unwrap();

    let held = handle.release_lookup_choices().get(&key).cloned();
    let held = held.expect("the session's choices are held");
    assert!(held.disc_id_excluded);
    assert_eq!(held.chosen_catalogs, vec!["WPCR-80001".to_string()]);

    handle.end_release_identification(&release).await.unwrap();
    assert!(handle.release_lookup_choices().get(&key).is_none());
    shut_down(handle).await;
}
