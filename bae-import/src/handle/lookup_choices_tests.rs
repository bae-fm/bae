//! The person's lookup choices: what a run looks up by, read back on the pane,
//! kept over a pick or a new reading of the tags, and asking for another run
//! only when they change what the run looks up.

use super::*;
use crate::handle::tests::*;
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::test_releases::seed_mb_release_with_catalog;
use bae_identify::{ChoiceChange, LookupChoiceEdit, LookupChoices};

fn barcode(code: &str) -> LookupChoiceEdit {
    LookupChoiceEdit::ToggleBarcode { code: code.to_string() }
}

fn discounted(number: &str) -> LookupChoiceEdit {
    LookupChoiceEdit::ToggleDiscounted { number: number.to_string() }
}

/// Make each of `edits` to the candidate's choices, in order, and say what
/// each changed.
async fn edit(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
    edits: impl IntoIterator<Item = LookupChoiceEdit>,
) -> Vec<ChoiceChange> {
    let mut changes = Vec::new();
    for edit in edits {
        changes.push(handle.edit_candidate_lookup_choices(key, edit).await.unwrap());
    }
    changes
}

/// The person's lookup choices read back on the candidate's pane.
#[tokio::test(flavor = "multi_thread")]
async fn the_candidate_s_lookup_choices_read_back_on_its_pane() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().lookup_choices,
        LookupChoices::default(),
        "a candidate nobody has chosen for excludes nothing and chooses nothing"
    );

    edit(
        &handle,
        &key,
        [
            barcode("9999999999999"),
            barcode("0123456789012"),
            catalog("WPCR-80001"),
            discounted("LBL-9"),
        ],
    )
    .await;

    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().lookup_choices,
        LookupChoices {
            disc_id_excluded: false,
            excluded_barcodes: vec!["0123456789012".to_string(), "9999999999999".to_string()],
            chosen_catalogs: vec!["WPCR-80001".to_string()],
            search_words: None,
            discounted_catalogs: vec!["LBL-9".to_string()],
        }
    );
    shut_down(handle).await;
}

/// Applying the files' tags again, the pane's way to start over, keeps what
/// the person chose to look up by.
#[tokio::test(flavor = "multi_thread")]
async fn applying_the_tags_again_keeps_the_lookup_choices() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    edit(&handle, &key, [barcode("0123456789012"), catalog("WPCR-80001")]).await;
    let chosen = pane(&handle, &key).await.draft().unwrap().lookup_choices.clone();

    handle.select_candidate_file_tags(key.clone()).await.unwrap();

    assert_eq!(pane(&handle, &key).await.draft().unwrap().lookup_choices, chosen);
    shut_down(handle).await;
}

/// Two changes made one right after the other both land: each is made to the
/// choices the one before it left, not to a copy read before either.
#[tokio::test(flavor = "multi_thread")]
async fn two_quick_changes_both_land() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let (first, second) = tokio::join!(
        handle.edit_candidate_lookup_choices(&key, LookupChoiceEdit::ToggleDiscId),
        handle.edit_candidate_lookup_choices(&key, catalog("WPCR-80001")),
    );
    first.unwrap();
    second.unwrap();

    let stored = pane(&handle, &key).await.draft().unwrap().lookup_choices.clone();
    shut_down(handle).await;
    assert!(stored.disc_id_excluded);
    assert_eq!(stored.chosen_catalogs, vec!["WPCR-80001".to_string()]);
}

/// A key that names no scanned folder has no candidate to hold a choice.
#[tokio::test(flavor = "multi_thread")]
async fn lookup_choices_for_an_unknown_key_are_refused() {
    let (handle, _tmp, _key, _hash) = pane_fixture().await;
    let refused = handle
        .edit_candidate_lookup_choices(
            &crate::CandidateKey::test("/nowhere/at/all"),
            LookupChoiceEdit::ToggleDiscId,
        )
        .await;
    assert!(refused.is_err());
    shut_down(handle).await;
}

/// Changing what the run looks up asks for another run, and striking a number
/// out is such a change; words that come to the draft's own title again are
/// not.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_change_to_what_a_run_looks_up_asks_for_another_run() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert_eq!(
        edit(&handle, &key, [barcode("0123456789012"), catalog("WPCR-80001")]).await,
        vec![ChoiceChange::Lookups, ChoiceChange::Lookups]
    );
    assert_eq!(
        edit(&handle, &key, [discounted("LBL-9"), discounted("LBL-9")]).await,
        vec![ChoiceChange::Lookups, ChoiceChange::Lookups]
    );
    assert_eq!(
        edit(
            &handle,
            &key,
            [LookupChoiceEdit::SearchBy { album: "  ".to_string(), artist: String::new() }]
        )
        .await,
        vec![ChoiceChange::Ranking],
        "blank words search by the draft's title, as nothing typed did"
    );
    shut_down(handle).await;
}

/// A number both chosen and struck out is written as both, however each list
/// spells it.
#[tokio::test(flavor = "multi_thread")]
async fn a_struck_out_number_is_written_as_still_chosen() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    edit(&handle, &key, [catalog("WPCR-80001"), catalog("NJ 8255"), discounted("NJ-8255")]).await;
    let stored = pane(&handle, &key).await.draft().unwrap().lookup_choices.clone();
    shut_down(handle).await;
    assert_eq!(stored.chosen_catalogs, vec!["WPCR-80001".to_string(), "NJ 8255".to_string()]);
    assert_eq!(stored.discounted_catalogs, vec!["NJ-8255".to_string()]);
}

/// Striking a chosen number out and back asks for a run each way: a struck
/// number is searched by nobody.
#[tokio::test(flavor = "multi_thread")]
async fn striking_a_chosen_number_out_and_back_asks_for_a_run_each_way() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    edit(&handle, &key, [catalog("NJ-8255")]).await;
    let chosen = pane(&handle, &key).await.draft().unwrap().lookup_choices.clone();
    assert_eq!(edit(&handle, &key, [discounted("NJ-8255")]).await, vec![ChoiceChange::Lookups]);
    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().lookup_choices,
        LookupChoices { discounted_catalogs: vec!["NJ-8255".to_string()], ..chosen.clone() }
    );
    assert_eq!(edit(&handle, &key, [discounted("NJ-8255")]).await, vec![ChoiceChange::Lookups]);
    let stored = pane(&handle, &key).await.draft().unwrap().lookup_choices.clone();
    shut_down(handle).await;
    assert_eq!(stored, chosen);
}

/// Picking a record whose number the folder prints chooses nothing for the
/// next run.
#[tokio::test(flavor = "multi_thread")]
async fn picking_a_record_the_folder_prints_the_number_of_chooses_nothing() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    store_settled_text(&handle, &hash, "NJ-8255").await;
    seed_mb_release_with_catalog(&fakes, "chosen-mb-rel-1", "NJ 8255");

    pick(&handle, &key, "chosen-mb-rel-1").await;

    let stored = pane(&handle, &key).await.draft().unwrap().lookup_choices.clone();
    shut_down(handle).await;
    assert_eq!(stored, LookupChoices::default());
}

/// A pick leaves the person's own choices as they were.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_leaves_the_person_s_choices_as_they_were() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    store_settled_text(&handle, &hash, "NJ-8255").await;
    edit(&handle, &key, [barcode("0123456789012"), catalog("WPCR-80001"), discounted("NJ-8255")])
        .await;
    let choices = pane(&handle, &key).await.draft().unwrap().lookup_choices.clone();
    seed_mb_release_with_catalog(&fakes, "chosen-mb-rel-2", "NJ 8255");

    pick(&handle, &key, "chosen-mb-rel-2").await;

    let stored = pane(&handle, &key).await.draft().unwrap().lookup_choices.clone();
    shut_down(handle).await;
    assert_eq!(stored, choices);
}

/// Store settled signals for the fixture's candidate whose folder name,
/// `printed`, is also its one catalog number.
async fn store_settled_text(handle: &ImportServiceHandle, hash: &str, printed: &str) {
    let prep = crate::preparations::stored_preparation(&handle.services.tables, hash)
        .await
        .unwrap()
        .expect("the fixture candidate is prepared");
    let stored = handle
        .services
        .preparations
        .store_verdict(&crate::tables::NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: prep.file_edits.revision,
            candidate_key: crate::CandidateKey::from(Vec::new()),
            verdict: bae_identify::TerminalVerdict::ManualOnly { track_count: 1, ledger: None },
            signals: bae_identify::signals::Signals {
                origin: bae_catalogs::AudioOrigin::default(),
                disc_id: bae_identify::signals::DiscIdSignal::Absent,
                barcode: bae_identify::signals::BarcodeSignal::Absent,
                text: bae_identify::signals::TextSignal::Settled {
                    catalogs: vec![printed.to_string()],
                    free_text: Vec::new(),
                },
                text_pool: vec![bae_identify::signals::TextLine {
                    text: printed.to_string(),
                    origin: bae_identify::signals::TextOrigin::FolderName,
                }],
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: None,
            releases: Vec::new(),
        })
        .await
        .unwrap();
    assert!(stored, "the settled signals land on the fixture candidate");
}
