// The fourth step: what the run asks when its three identifiers name nothing.

/// A run that reads a title off the candidate's draft, with nothing chosen or
/// excluded.
fn started_searching(providers: Vec<Catalog>, album: &str, artist: &str) -> IdentifyState {
    let (state, effects) = step(
        IdentifyState::Idle,
        IdentifyEvent::Started {
            providers,
            choices: LookupChoices::default(),
            title_search: TitleSearch::of(album, artist),
        },
    );
    assert!(
        effects.is_empty(),
        "with nothing chosen, Started dispatches no effects"
    );
    state
}

fn search_title(source: Catalog, album: &str, artist: &str) -> Effect {
    Effect::SearchTitle {
        source,
        query: TitleSearch {
            album: album.to_string(),
            artist: artist.to_string(),
        },
    }
}

fn search_answered(
    source: Catalog,
    results: Vec<(MetadataResult, LibraryStatus)>,
) -> IdentifyEvent {
    IdentifyEvent::SearchAnswered {
        source,
        outcome: Ok(results),
    }
}

fn search_failed(source: Catalog, failure: LookupFailure) -> IdentifyEvent {
    IdentifyEvent::SearchAnswered {
        source,
        outcome: Err(failure),
    }
}

/// A folder with one barcode and no disc ID: enough for the ledger to have a
/// run to lay out, and a code the providers can miss.
fn one_code(code: &str) -> Signals {
    signals(
        DiscIdSignal::Absent { track_count: 9 },
        BarcodeSignal::Settled {
            codes: artwork_codes(&[code]),
        },
        &[],
    )
}

/// The identifiers coming back empty is what sends the candidate's own title
/// to every provider, and what the search finds is offered as the run's
/// answer.
#[test]
fn the_title_goes_out_once_the_identifiers_name_nothing() {
    let state = started_searching(vec![MB, DG], "Album Title", "Artist Name");
    let (state, effects) = update(state, one_code("BAR"));
    assert_eq!(
        effects,
        vec![lookup_barcode(MB, "BAR"), lookup_barcode(DG, "BAR")],
        "the barcode is asked first, and nothing else yet"
    );

    let (state, effects) = step(state, barcode_missed(MB, "BAR"));
    assert!(effects.is_empty(), "one provider is still out");
    let (state, effects) = step(state, barcode_missed(DG, "BAR"));
    assert_eq!(
        effects,
        vec![
            search_title(MB, "Album Title", "Artist Name"),
            search_title(DG, "Album Title", "Artist Name"),
        ],
        "both misses settle the identifiers, and the title goes out to both"
    );
    assert!(
        matches!(state, IdentifyState::Triangulating { .. }),
        "the run waits on the search, got {state:?}"
    );

    let (state, _) = step(state, search_answered(MB, vec![pair("mb-1", Some("g-x"))]));
    let (state, effects) = step(state, search_answered(DG, Vec::new()));
    assert!(effects.is_empty());
    match state {
        IdentifyState::Found {
            matches,
            provenance,
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert!(provenance[0].by_search);
            assert!(!provenance[0].by_disc_id && !provenance[0].by_barcode);
        }
        other => panic!("expected Found by the title search, got {other:?}"),
    }
}

/// An identifier that named something answers the candidate, so the title is
/// never asked: a code is a stronger claim than a name.
#[test]
fn an_identifier_that_named_something_leaves_the_title_unasked() {
    let state = started_searching(vec![MB], "Album Title", "Artist Name");
    let (state, _) = update(state, one_code("BAR"));
    let (state, effects) = step(
        state,
        barcode_matched(MB, "BAR", vec![pair("mb-1", Some("g-x"))]),
    );
    assert!(
        effects.is_empty(),
        "the barcode answered, so nothing searches by title: {effects:?}"
    );
    match state {
        IdentifyState::Found { provenance, .. } => {
            assert!(provenance[0].by_barcode && !provenance[0].by_search);
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

/// A draft with no title leaves nothing to search by, so the run ends on what
/// the identifiers said rather than waiting on a query it cannot make.
#[test]
fn a_candidate_with_no_title_asks_no_search() {
    let state = started_searching(vec![MB], "   ", "Artist Name");
    let (state, _) = update(state, one_code("BAR"));
    let (state, effects) = step(state, barcode_missed(MB, "BAR"));
    assert!(
        effects.is_empty(),
        "there is nothing to search by: {effects:?}"
    );
    assert!(
        matches!(state, IdentifyState::NotFoundAnywhere { .. }),
        "expected NotFoundAnywhere, got {state:?}"
    );
}

/// One provider failing the search is a failure the run names, with the other
/// provider's matches still standing beside it.
#[test]
fn one_provider_failing_the_search_leaves_the_other_s_matches_standing() {
    let state = started_searching(vec![MB, DG], "Album Title", "Artist Name");
    let (state, _) = update(state, one_code("BAR"));
    let (state, _) = step(state, barcode_missed(MB, "BAR"));
    let (state, _) = step(state, barcode_missed(DG, "BAR"));

    let (state, _) = step(state, search_failed(MB, LookupFailure::Timeout));
    let (state, effects) = step(
        state,
        search_answered(DG, vec![discogs_pair("dg-1", Some("g-x"))]),
    );
    assert!(effects.is_empty());
    match state {
        IdentifyState::Failed {
            failures, matches, ..
        } => {
            assert_eq!(
                failures,
                vec![IdentifyFailure::Search(SourceFailure {
                    source: MB,
                    failure: LookupFailure::Timeout,
                })]
            );
            assert_eq!(matches.len(), 1, "Discogs's match still stands");
            assert_eq!(matches[0].source, DG);
        }
        other => panic!("expected Failed with the surviving match, got {other:?}"),
    }
}

/// Nothing to look up and nothing to search by is the only way a run offers
/// manual search. A folder with a title has something to run, so it gets an
/// answer rather than an offer.
#[test]
fn manual_only_needs_no_identifier_and_no_title() {
    let nothing = || {
        signals(
            DiscIdSignal::Absent { track_count: 9 },
            BarcodeSignal::Absent,
            &[],
        )
    };

    let (state, effects) = update(started_searching(vec![MB], "", ""), nothing());
    assert!(effects.is_empty());
    assert!(
        matches!(state, IdentifyState::ManualOnly { track_count: 9, .. }),
        "no identifier and no title, got {state:?}"
    );

    let (state, effects) = update(
        started_searching(vec![MB], "Album Title", "Artist Name"),
        nothing(),
    );
    assert_eq!(
        effects,
        vec![search_title(MB, "Album Title", "Artist Name")],
        "a title is something to run, even with no identifier"
    );
    let (state, _) = step(state, search_answered(MB, Vec::new()));
    assert!(
        matches!(state, IdentifyState::NotFoundAnywhere { .. }),
        "the search ran and found nothing, got {state:?}"
    );
}

/// The ledger records the words that were searched and one cell per provider,
/// so what the run showed while it searched is what it shows afterwards.
#[test]
fn the_ledger_records_the_words_that_were_searched() {
    let state = started_searching(vec![MB, DG], "Album Title", "Artist Name");
    let (state, _) = update(state, one_code("BAR"));
    let (state, _) = step(state, barcode_missed(MB, "BAR"));
    let (state, _) = step(state, barcode_missed(DG, "BAR"));
    let (state, _) = step(state, search_answered(MB, Vec::new()));
    let (state, _) = step(
        state,
        search_answered(DG, vec![discogs_pair("dg-1", Some("g-x"))]),
    );

    let IdentifyState::Found { ledger, .. } = &state else {
        panic!("expected Found, got {state:?}");
    };
    let ledger = ledger.as_ref().expect("the run had inputs to lay out");
    let crate::identify::SearchStepView::Searched {
        album,
        artist,
        cells,
    } = &ledger.search
    else {
        panic!("expected a searched step, got {:?}", ledger.search);
    };
    assert_eq!(album, "Album Title");
    assert_eq!(artist, "Artist Name");
    assert_eq!(
        cells.iter().map(|cell| cell.source).collect::<Vec<_>>(),
        vec![MB, DG]
    );
    assert!(matches!(
        cells[0].lookup,
        crate::identify::LookupView::NoMatch
    ));
    assert!(matches!(
        cells[1].lookup,
        crate::identify::LookupView::Found { count: 1, .. }
    ));
}

/// A run whose identifiers answered records that the search was not needed;
/// one with no title records that there was nothing to search by.
#[test]
fn a_ledger_says_why_no_search_ran() {
    let matched = {
        let state = started_searching(vec![MB], "Album Title", "Artist Name");
        let (state, _) = update(state, one_code("BAR"));
        step(
            state,
            barcode_matched(MB, "BAR", vec![pair("mb-1", Some("g-x"))]),
        )
        .0
    };
    let IdentifyState::Found { ledger, .. } = &matched else {
        panic!("expected Found, got {matched:?}");
    };
    assert!(matches!(
        ledger.as_ref().expect("a run with inputs").search,
        crate::identify::SearchStepView::NotNeeded
    ));

    let untitled = {
        let state = started_searching(vec![MB], "", "");
        let (state, _) = update(state, one_code("BAR"));
        step(state, barcode_missed(MB, "BAR")).0
    };
    let IdentifyState::NotFoundAnywhere { ledger, .. } = &untitled else {
        panic!("expected NotFoundAnywhere, got {untitled:?}");
    };
    assert!(matches!(
        ledger.as_ref().expect("a run with inputs").search,
        crate::identify::SearchStepView::NoTitle
    ));
}

/// A draft's title searches by its words: the catalog number and edition a
/// tag hangs off the end in brackets are not part of what the catalogs file
/// the release under. A title that is nothing but a bracket searches as it
/// is, and words a person typed are taken as typed.
#[test]
fn a_drafts_title_searches_without_its_bracketed_tails() {
    assert_eq!(
        TitleSearch::of_draft("Album Title [XX34b]", "Artist Name"),
        Some(TitleSearch {
            album: "Album Title".to_string(),
            artist: "Artist Name".to_string(),
        })
    );
    assert_eq!(
        TitleSearch::of_draft("[XX34b]", ""),
        Some(TitleSearch {
            album: "[XX34b]".to_string(),
            artist: String::new(),
        })
    );
    assert_eq!(
        TitleSearch::of("Album Title [XX34b]", ""),
        Some(TitleSearch {
            album: "Album Title [XX34b]".to_string(),
            artist: String::new(),
        })
    );
}
