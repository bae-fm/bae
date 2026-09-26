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
        excluded_barcodes: vec!["0123456789012".to_string(), "9999999999999".to_string()],
        chosen_catalogs: vec!["WPCR-80001".to_string()],
        search_words: None,
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
        excluded_barcodes: vec!["0123456789012".to_string()],
        chosen_catalogs: vec!["WPCR-80001".to_string()],
        search_words: None,
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

/// A number in both lists is the person contradicting themselves, and the
/// write settles it the one way it can go: struck out is not chosen.
#[tokio::test(flavor = "multi_thread")]
async fn a_struck_out_number_is_written_as_not_chosen() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .set_candidate_lookup_choices(
            &key,
            LookupChoices {
                chosen_catalogs: vec!["WPCR-80001".to_string(), "NJ 8255".to_string()],
                discounted_catalogs: vec!["NJ-8255".to_string()],
                ..LookupChoices::default()
            },
        )
        .await
        .unwrap();
    let stored = pane(&handle, &key).await.lookup_choices;
    shut_down(handle).await;
    assert_eq!(stored.chosen_catalogs, vec!["WPCR-80001".to_string()]);
    assert_eq!(stored.discounted_catalogs, vec!["NJ-8255".to_string()]);
}

/// The folder prints `NJ-8255` and the picked record carries `NJ 8255`: the
/// agreement the person sees kept is the number, and keeping it chooses it —
/// in the folder's own spelling, which is what the marks fold sightings by.
#[tokio::test(flavor = "multi_thread")]
async fn picking_a_record_the_folder_prints_the_number_of_chooses_it() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_settled_text(&handle, &hash, "NJ-8255").await;
    seed_mb_release_with_catalog(handle.library_manager.providers(), "chosen-mb-rel-1", "NJ 8255");

    pick(&handle, &key, "chosen-mb-rel-1").await;

    let chosen = pane(&handle, &key).await.lookup_choices.chosen_catalogs;
    shut_down(handle).await;
    assert_eq!(chosen, vec!["NJ-8255".to_string()]);
}

/// A number the folder does not print is nothing the person saw kept, so the
/// pick chooses nothing.
#[tokio::test(flavor = "multi_thread")]
async fn picking_a_record_whose_number_the_folder_does_not_print_chooses_nothing() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_settled_text(&handle, &hash, "NJ-8255").await;
    seed_mb_release_with_catalog(handle.library_manager.providers(), "chosen-mb-rel-2", "ZZ 9999");

    pick(&handle, &key, "chosen-mb-rel-2").await;

    let chosen = pane(&handle, &key).await.lookup_choices.chosen_catalogs;
    shut_down(handle).await;
    assert!(chosen.is_empty(), "nothing was confirmed: {chosen:?}");
}

/// A number the person struck out stays struck out through the pick: it was
/// not kept, so it is not chosen.
#[tokio::test(flavor = "multi_thread")]
async fn picking_a_record_whose_number_is_struck_out_chooses_nothing() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_settled_text(&handle, &hash, "NJ-8255").await;
    handle
        .set_candidate_lookup_choices(
            &key,
            LookupChoices {
                discounted_catalogs: vec!["NJ-8255".to_string()],
                ..LookupChoices::default()
            },
        )
        .await
        .unwrap();
    seed_mb_release_with_catalog(handle.library_manager.providers(), "chosen-mb-rel-3", "NJ 8255");

    pick(&handle, &key, "chosen-mb-rel-3").await;

    let stored = pane(&handle, &key).await.lookup_choices;
    shut_down(handle).await;
    assert!(stored.chosen_catalogs.is_empty(), "{stored:?}");
    assert_eq!(stored.discounted_catalogs, vec!["NJ-8255".to_string()]);
}

/// A second record with the same number confirms what is already chosen:
/// the number is chosen once.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_pick_with_the_same_number_chooses_it_once() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_settled_text(&handle, &hash, "NJ-8255").await;
    seed_mb_release_with_catalog(handle.library_manager.providers(), "chosen-mb-rel-4", "NJ 8255");
    seed_mb_release_with_catalog(handle.library_manager.providers(), "chosen-mb-rel-5", "NJ-8255");

    pick(&handle, &key, "chosen-mb-rel-4").await;
    pick(&handle, &key, "chosen-mb-rel-5").await;

    let chosen = pane(&handle, &key).await.lookup_choices.chosen_catalogs;
    shut_down(handle).await;
    assert_eq!(chosen, vec!["NJ-8255".to_string()]);
}

/// Store the signals a run settled on for the fixture's candidate: its own
/// text stating `printed` — the folder's name — classified as a catalog
/// number sighting in that spelling.
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
            candidate: crate::import::CandidateAsRead {
                content_hash: hash.to_string(),
                file_edit_revision: prep.file_edit_revision,
                metadata_revision: prep.metadata_revision,
            },
            folder_path: String::new(),
            verdict: crate::identify::TerminalVerdict::ManualOnly {
                track_count: 1,
                ledger: None,
            },
            signals: crate::signals::Signals {
                disc_id: crate::signals::DiscIdSignal::Absent { track_count: 1 },
                barcode: crate::signals::BarcodeSignal::Absent,
                text: crate::signals::TextSignal::Settled {
                    catalogs: vec![crate::signals::SourcedValue::new(
                        printed.to_string(),
                        crate::signals::TextOrigin::FolderName,
                    )],
                    free_text: Vec::new(),
                },
                text_pool: vec![crate::signals::TextLine {
                    text: printed.to_string(),
                    origin: crate::signals::TextOrigin::FolderName,
                    file: None,
                    region: None,
                }],
                durations: crate::import::probe::SourceDurations::totalling(1_000),
            },
            metadata: None,
        })
        .await
        .unwrap();
    assert!(stored, "the settled signals land on the fixture candidate");
}

async fn pick(handle: &ImportServiceHandle, key: &str, release_id: &str) {
    handle
        .select_candidate_metadata_provenance(
            key.to_string(),
            crate::import::MetadataProvenance::ExternalRelease {
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
/// group: a group would have the pick fetch its front cover from the archive,
/// which no test serves.
fn seed_mb_release_with_catalog(
    providers: &crate::providers::Providers,
    release_id: &str, catalog_number: &str) {
    let response = crate::musicbrainz::MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
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
