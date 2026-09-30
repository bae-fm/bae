use super::*;
use crate::import::{ChoiceChange, LookupChoiceEdit, LookupChoices};

fn barcode(code: &str) -> LookupChoiceEdit {
    LookupChoiceEdit::ToggleBarcode {
        code: code.to_string(),
    }
}

fn catalog(number: &str) -> LookupChoiceEdit {
    LookupChoiceEdit::ToggleCatalog {
        number: number.to_string(),
    }
}

fn discounted(number: &str) -> LookupChoiceEdit {
    LookupChoiceEdit::ToggleDiscounted {
        number: number.to_string(),
    }
}

/// Make each of `edits` to the candidate's choices, in order, and say what
/// each changed.
async fn edit(
    handle: &ImportServiceHandle,
    key: &str,
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
        pane(&handle, &key).await.lookup_choices,
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
        pane(&handle, &key).await.lookup_choices,
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

    let stored = pane(&handle, &key).await.lookup_choices;
    shut_down(handle).await;
    assert!(stored.disc_id_excluded);
    assert_eq!(stored.chosen_catalogs, vec!["WPCR-80001".to_string()]);
}

/// A key that names no scanned folder has no candidate to hold a choice.
#[tokio::test(flavor = "multi_thread")]
async fn lookup_choices_for_an_unknown_key_are_refused() {
    let (handle, _tmp, _key, _hash) = pane_fixture().await;
    let refused = handle
        .edit_candidate_lookup_choices("/nowhere/at/all", LookupChoiceEdit::ToggleDiscId)
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
            [LookupChoiceEdit::SearchBy {
                album: "  ".to_string(),
                artist: String::new(),
            }]
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
    edit(
        &handle,
        &key,
        [catalog("WPCR-80001"), catalog("NJ 8255"), discounted("NJ-8255")],
    )
    .await;
    let stored = pane(&handle, &key).await.lookup_choices;
    shut_down(handle).await;
    assert_eq!(
        stored.chosen_catalogs,
        vec!["WPCR-80001".to_string(), "NJ 8255".to_string()]
    );
    assert_eq!(stored.discounted_catalogs, vec!["NJ-8255".to_string()]);
}

/// Striking a chosen number out and back asks for a run each way: a struck
/// number is searched by nobody.
#[tokio::test(flavor = "multi_thread")]
async fn striking_a_chosen_number_out_and_back_asks_for_a_run_each_way() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    edit(&handle, &key, [catalog("NJ-8255")]).await;
    let chosen = pane(&handle, &key).await.lookup_choices;
    assert_eq!(
        edit(&handle, &key, [discounted("NJ-8255")]).await,
        vec![ChoiceChange::Lookups]
    );
    assert_eq!(
        pane(&handle, &key).await.lookup_choices,
        LookupChoices {
            discounted_catalogs: vec!["NJ-8255".to_string()],
            ..chosen.clone()
        }
    );
    assert_eq!(
        edit(&handle, &key, [discounted("NJ-8255")]).await,
        vec![ChoiceChange::Lookups]
    );
    let stored = pane(&handle, &key).await.lookup_choices;
    shut_down(handle).await;
    assert_eq!(stored, chosen);
}

/// Picking a record whose number the folder prints chooses nothing for the
/// next run.
#[tokio::test(flavor = "multi_thread")]
async fn picking_a_record_the_folder_prints_the_number_of_chooses_nothing() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_settled_text(&handle, &hash, "NJ-8255").await;
    seed_mb_release_with_catalog(handle.library_manager.providers(), "chosen-mb-rel-1", "NJ 8255");

    pick(&handle, &key, "chosen-mb-rel-1").await;

    let stored = pane(&handle, &key).await.lookup_choices;
    shut_down(handle).await;
    assert_eq!(stored, LookupChoices::default());
}

/// A pick leaves the person's own choices as they were.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_leaves_the_person_s_choices_as_they_were() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_settled_text(&handle, &hash, "NJ-8255").await;
    edit(
        &handle,
        &key,
        [barcode("0123456789012"), catalog("WPCR-80001"), discounted("NJ-8255")],
    )
    .await;
    let choices = pane(&handle, &key).await.lookup_choices;
    seed_mb_release_with_catalog(handle.library_manager.providers(), "chosen-mb-rel-2", "NJ 8255");

    pick(&handle, &key, "chosen-mb-rel-2").await;

    let stored = pane(&handle, &key).await.lookup_choices;
    shut_down(handle).await;
    assert_eq!(stored, choices);
}

/// Store settled signals for the fixture's candidate whose folder name,
/// `printed`, is also its one catalog number.
async fn store_settled_text(handle: &ImportServiceHandle, hash: &str, printed: &str) {
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
            verdict: crate::identify::TerminalVerdict::ManualOnly {
                track_count: 1,
                ledger: None,
            },
            signals: crate::signals::Signals {
                origin: crate::signals::AudioOrigin::default(),
                disc_id: crate::signals::DiscIdSignal::Absent,
                barcode: crate::signals::BarcodeSignal::Absent,
                text: crate::signals::TextSignal::Settled {
                    catalogs: vec![printed.to_string()],
                    free_text: Vec::new(),
                },
                text_pool: vec![crate::signals::TextLine {
                    text: printed.to_string(),
                    origin: crate::signals::TextOrigin::FolderName,
                }],
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: None,
        })
        .await
        .unwrap();
    assert!(stored, "the settled signals land on the fixture candidate");
}

pub(super) async fn pick(handle: &ImportServiceHandle, key: &str, release_id: &str) {
    handle
        .select_candidate_release(key.to_string(), crate::import::PressingLink {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
        )
        .await
        .unwrap();
}

/// A two-track MusicBrainz release carrying `catalog_number`, in no release
/// group so the pick fetches no cover.
pub(super) fn seed_mb_release_with_catalog(
    providers: &crate::providers::Providers,
    release_id: &str, catalog_number: &str) {
    let response = crate::musicbrainz::MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        disambiguation: None,
        annotation: None,
        date: Some("1996".to_string()),
        country: Some("US".to_string()),
        status: None,
        packaging: None,
        barcode: None,
        artist_credit: vec![crate::musicbrainz::MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(crate::musicbrainz::MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: None,
        label_info: vec![crate::musicbrainz::MbLabelInfo {
            label: Some(crate::musicbrainz::MbLabel {
                name: Some("Label Name".to_string()),
            }),
            catalog_number: Some(catalog_number.to_string()),
        }],
        media: vec![crate::musicbrainz::MbMedium {
            discs: vec![],
            format: Some("CD".to_string()),
            tracks: (1..=2)
                .map(|number| crate::musicbrainz::MbTrack {
                    position: Some(number),
                    number: Some(number.to_string()),
                    title: None,
                    length: None,
                    recording: Some(crate::musicbrainz::MbRecording {
                        id: None,
                        title: Some("Track One".to_string()),
                        artist_credit: vec![],
                        relations: vec![],
                    }),
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    };
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    providers.musicbrainz().seed_release_cache(release_id, raw_json);
}
