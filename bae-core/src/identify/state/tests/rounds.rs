// Included by `tests.rs`; shares the helpers of `signals_and_conflicts.rs`
// and `album_links.rs`.

/// A release of `source` printing `number` under `label`.
fn labelled(source: Catalog, release_id: &str, label: &str, number: &str) -> MetadataResult {
    MetadataResult {
        labels: vec![crate::pressing::ReleaseLabel::of(Some(label), Some(number))],
        ..mk_result_from(source, release_id, Some("g"))
    }
}

/// A run on both catalogs given a disc ID alone, its folder named `folder`.
fn disc_run_of_folder(folder: &str, catalogs: &[&str]) -> IdentifyState {
    let mut disc_signals = disc_only(catalogs);
    disc_signals.text_pool = vec![crate::signals::TextLine {
        text: folder.to_string(),
        origin: crate::signals::TextOrigin::FolderName,
    }];
    let (state, effects) = super::step(
        started_with(vec![MB, DG]),
        IdentifyEvent::SignalsUpdated {
            signals: disc_signals,
            audio: five_tracks(),
            artwork: crate::signals::ArtworkScan::Absent,
        },
    );
    assert_eq!(effects, vec![Effect::LookupDiscid { disc_id: "d".into() }]);
    state
}

fn catalog_lookup(source: Catalog, catalog: &str) -> Effect {
    Effect::LookupCatalog {
        source,
        catalog: catalog.to_string(),
    }
}

/// The disc ID names MusicBrainz releases alone, and the folder prints the
/// number one of them carries: that number comes into effect and is asked
/// of both catalogs, and the Discogs release it finds is pooled with the
/// rest.
#[test]
fn a_number_the_text_prints_that_a_found_release_carries_is_searched_on_both_catalogs() {
    let state = disc_run_of_folder("1986 Germany Label AB 12345-2", &["AB 12345-2"]);
    let (state, effects) = super::step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![
                (
                    labelled(MB, "mb-1", "Label", "AB 12345-2"),
                    LibraryStatus::absent("mb-1"),
                ),
                pair("mb-2", Some("g")),
            ],
        },
    );
    assert!(
        effects.contains(&catalog_lookup(MB, "AB 12345-2"))
            && effects.contains(&catalog_lookup(DG, "AB 12345-2")),
        "the number is searched on both catalogs: {effects:?}"
    );
    let IdentifyState::Triangulating { context, .. } = &state else {
        panic!("the run goes round: {state:?}");
    };
    assert_eq!(context.catalog.confirmed, vec!["AB 12345-2".to_string()]);
    assert!(context.catalog.chosen.is_empty(), "nothing was picked");

    let (state, _) = super::step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "AB 12345-2".to_string(),
            outcome: Ok(vec![(
                labelled(MB, "mb-1", "Label", "AB 12345-2"),
                LibraryStatus::absent("mb-1"),
            )]),
        },
    );
    let reading = super::step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: DG,
            for_catalog: "AB 12345-2".to_string(),
            outcome: Ok(vec![(
                labelled(DG, "dg-1", "Label", "AB 12345-2"),
                LibraryStatus::absent("dg-1"),
            )]),
        },
    );
    let (state, _, _) = answer_rounds(
        (reading.0, effects.into_iter().chain(reading.1).collect()),
        |release| ReleaseDocument {
            labels: vec![crate::pressing::ReleaseLabel::of(
                Some("Label"),
                Some(if release.key == "mb-2" { "ZZ-1" } else { "AB 12345-2" }),
            )],
            ..plain_document(&[])
        },
        |_, _| Vec::new(),
    );
    let rows = rows_of(&state);
    let discogs = rows
        .iter()
        .find(|(result, _, _)| result.release_id == "dg-1")
        .unwrap_or_else(|| panic!("the Discogs answer is pooled: {rows:?}"));
    assert!(discogs.1.by_catalog && !discogs.1.by_disc_id);
    let IdentifyState::Found { ledger, .. } = &state else {
        panic!("expected Found, got {state:?}");
    };
    let Some(crate::identify::view::CatalogStepView::Numbers { rows, .. }) =
        ledger.as_ref().map(|ledger| &ledger.catalog)
    else {
        panic!("the ledger lays the numbers out: {ledger:?}");
    };
    assert_eq!(
        rows.iter().map(|row| row.value.as_str()).collect::<Vec<_>>(),
        vec!["AB 12345-2"],
        "the number in effect is a row with its search"
    );
}

/// A picked number the person struck out is not in effect: nothing searches
/// it.
#[test]
fn a_struck_out_picked_number_is_not_searched() {
    let (_, effects) = started_with_choices(
        vec![MB, DG],
        LookupChoices {
            discounted_catalogs: vec!["AB-1".to_string()],
            ..choosing(&["AB-1", "CD-2"])
        },
    );
    assert_eq!(
        effects,
        vec![catalog_lookup(MB, "CD-2"), catalog_lookup(DG, "CD-2")]
    );
}

/// An offered row whose page names no Discogs release and which prints no
/// barcode is looked up on Discogs by its catalog number, and only a release
/// printing that number under the row's label answers. The run then goes
/// round until a round brings nothing new, asking each key once.
#[test]
fn a_row_with_no_link_and_no_barcode_is_looked_up_by_its_number_under_its_label() {
    let state = disc_run_of_folder("Some Album", &[]);
    let reading = super::step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("mb-1", Some("g"))],
        },
    );
    let (state, effects, asked) = answer_rounds(
        reading,
        |release| ReleaseDocument {
            labels: vec![crate::pressing::ReleaseLabel::of(
                Some("Label One"),
                Some("XY-100"),
            )],
            album_links: match release.catalog {
                Catalog::MusicBrainz => AlbumLinks::Read(Vec::new()),
                _ => AlbumLinks::NotAsked,
            },
            ..plain_document(&[])
        },
        |source, _| match source {
            Catalog::Discogs => vec![
                labelled(DG, "dg-1", "Label One Records", "XY-100"),
                labelled(DG, "dg-2", "Other Label", "XY-100"),
            ],
            _ => Vec::new(),
        },
    );
    let number = PressingKey::CatalogNumber {
        number: "XY-100".to_string(),
        label: "Label One".to_string(),
    };
    assert_eq!(asked.pressings.first(), Some(&(DG, number)));
    let mut keys = asked.pressings.clone();
    keys.dedup();
    assert_eq!(keys, asked.pressings, "each key is asked once");
    assert!(
        effects
            .iter()
            .all(|effect| matches!(effect, Effect::KeepAlbumLinks { .. })),
        "the last round asks nothing: {effects:?}"
    );
    let rows = rows_of(&state);
    let found = |id: &str| rows.iter().find(|(result, _, _)| result.release_id == id);
    let (_, lookup, _) = found("dg-1").expect("the label's release is pooled");
    assert!(lookup.by_pressing);
    assert!(found("dg-2").is_none(), "another label's number is not it");
}

/// A pressing lookup that fails leaves the run failed, naming the catalog.
#[test]
fn a_failed_pressing_lookup_fails_the_run() {
    let state = disc_run_of_folder("Some Album", &[]);
    let mut record = mk_result("mb-1", Some("g"));
    record.barcodes = vec!["5012345678900".to_string()];
    let (mut state, mut effects) = super::step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![(record, LibraryStatus::absent("mb-1"))],
        },
    );
    while let Some(effect) = effects.pop() {
        let event = match effect {
            Effect::ReadReleases { releases, .. } => IdentifyEvent::ReleasesRead {
                read: releases
                    .into_iter()
                    .map(|release| ReleaseReading {
                        release,
                        document: Err(LookupFailure::Network),
                    })
                    .collect(),
            },
            Effect::LookupPressing { source, key } => {
                assert_eq!(
                    (source, &key),
                    (
                        DG,
                        &PressingKey::Barcode {
                            barcode: "5012345678900".to_string()
                        }
                    )
                );
                IdentifyEvent::PressingLookupAnswered {
                    source,
                    key,
                    outcome: Err(LookupFailure::Timeout),
                }
            }
            other => panic!("unexpected {other:?}"),
        };
        let (next, more) = super::step(state, event);
        state = next;
        effects.extend(more);
    }
    let IdentifyState::Failed { failures, .. } = &state else {
        panic!("expected Failed, got {state:?}");
    };
    assert_eq!(
        failures,
        &vec![IdentifyFailure::Pressing(SourceFailure {
            source: DG,
            failure: LookupFailure::Timeout,
        })]
    );
}
