// Included by `tests.rs`; shares the helpers of `signals_and_conflicts.rs`.
//
// A step switched off in the identification settings: no lookup goes out for
// it, and it says it is off — never that it looked and found nothing.

/// `Started` with every step taken but `off`, asking `providers`, searching
/// by "Album" when the identifiers name nothing.
fn started_without(providers: Vec<Catalog>, off: crate::config::IdentificationStep) -> IdentifyState {
    let mut steps = crate::config::IdentificationSteps::default();
    steps.set(off, false);
    let (state, effects) = step(
        IdentifyState::Idle,
        IdentifyEvent::Started {
            providers,
            steps,
            choices: LookupChoices::default(),
            title_search: TitleSearch::of("Album", "Artist"),
        },
    );
    assert!(effects.is_empty());
    state
}

fn ledger_of(state: &IdentifyState) -> crate::identify::IdentifyRunView {
    match crate::identify::IdentifyStateView::from(state.clone()) {
        crate::identify::IdentifyStateView::Triangulating { run, .. } => run,
        crate::identify::IdentifyStateView::Found { run, .. }
        | crate::identify::IdentifyStateView::NotFoundAnywhere { run }
        | crate::identify::IdentifyStateView::ManualOnly { run, .. }
        | crate::identify::IdentifyStateView::Failed { run, .. } => {
            run.expect("the run recorded its ledger")
        }
        crate::identify::IdentifyStateView::Idle => panic!("an idle state lays out no run"),
    }
}

/// A disc ID read under a run that does not look disc IDs up is asked of
/// nobody, and its row and badge say the lookup is off.
#[test]
fn a_disc_id_the_run_does_not_look_up_is_off_not_a_no_match() {
    let state = started_without(vec![MB], crate::config::IdentificationStep::LookUpDiscIds);
    let (state, effects) = update(state, disc_only(&[]));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LookupDiscid { .. })),
        "no disc-ID lookup goes out: {effects:?}"
    );
    assert!(matches!(
        ledger_of(&state).disc_id,
        crate::identify::DiscIdStepView::Read {
            lookup: crate::identify::LookupView::Off,
            ..
        }
    ));
    assert_eq!(badge(&state, SignalKind::DiscId).state, SignalState::Off);
}

/// Barcodes read under a run that does not look barcodes up stay listed, each
/// cell off, and no provider is asked about any of them.
#[test]
fn barcodes_the_run_does_not_look_up_are_listed_with_every_cell_off() {
    let state = started_without(vec![MB, DG], crate::config::IdentificationStep::LookUpBarcodes);
    let (state, effects) = update(
        state,
        signals(
            DiscIdSignal::Absent { track_count: 2 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LookupBarcode { .. })),
        "no barcode lookup goes out: {effects:?}"
    );
    let crate::identify::BarcodeStepView::Rows { rows, .. } = ledger_of(&state).barcode else {
        panic!("the codes are still the run's rows");
    };
    assert_eq!(
        rows.iter().map(|row| row.value.as_str()).collect::<Vec<_>>(),
        vec!["A", "B"]
    );
    assert!(rows
        .iter()
        .flat_map(|row| &row.cells)
        .all(|cell| cell.lookup == crate::identify::LookupView::Off));
    assert_eq!(badge(&state, SignalKind::Barcode).state, SignalState::Off);
}

/// A run that does not search by title never sends the title out, however
/// empty-handed its identifiers come back, and says the step is off from the
/// start.
#[test]
fn a_run_that_does_not_search_by_title_never_asks_the_title() {
    let state = started_without(vec![MB], crate::config::IdentificationStep::SearchByTitle);
    assert_eq!(ledger_of(&state).search, crate::identify::SearchStepView::Off);
    let (state, effects) = update(state, one_code("BAR"));
    assert_eq!(effects, vec![lookup_barcode(MB, "BAR")]);
    let (state, effects) = step(state, barcode_missed(MB, "BAR"));
    assert!(
        effects.is_empty(),
        "the identifiers named nothing and no title goes out: {effects:?}"
    );
    assert!(
        matches!(state, IdentifyState::NotFoundAnywhere { .. }),
        "the barcode was asked and named nothing, got {state:?}"
    );
    assert_eq!(ledger_of(&state).search, crate::identify::SearchStepView::Off);
}

/// A run whose only lookup was switched off asked nobody anything, so it
/// offers manual search rather than claiming nothing matched.
#[test]
fn a_run_whose_lookups_are_all_off_offers_manual_search() {
    let mut steps = crate::config::IdentificationSteps::default();
    steps.set(crate::config::IdentificationStep::LookUpDiscIds, false);
    steps.set(crate::config::IdentificationStep::SearchByTitle, false);
    let (state, _) = step(
        IdentifyState::Idle,
        IdentifyEvent::Started {
            providers: vec![MB],
            steps,
            choices: LookupChoices::default(),
            title_search: TitleSearch::of("Album", "Artist"),
        },
    );
    let (state, effects) = update(state, disc_only(&[]));
    assert!(effects.is_empty(), "nothing is asked: {effects:?}");
    assert!(
        matches!(state, IdentifyState::ManualOnly { .. }),
        "got {state:?}"
    );
    assert_eq!(
        crate::identify::classify(&crate::identify::TerminalVerdict::try_from(state).unwrap()),
        crate::identify::QueueClassification::NeedsYou(crate::identify::NeedsYou::NothingToLookUp)
    );
}

/// A run that does not follow catalog links reads no album links, even when
/// what it found holds both catalogs' releases, and says so.
#[test]
fn a_run_that_does_not_follow_catalog_links_reads_none() {
    let state = started_without(
        vec![MB, DG],
        crate::config::IdentificationStep::FollowCatalogLinks,
    );
    let (state, _) = update(
        state,
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A"]),
            },
            &[],
        ),
    );
    let (state, _) = step(
        state,
        barcode_matched(MB, "A", vec![pair("mb-1", Some("g-linked"))]),
    );
    let (state, effects) = step(
        state,
        barcode_matched(DG, "A", vec![discogs_pair("dg-1", Some("7"))]),
    );
    assert!(
        effects.is_empty(),
        "no album links read goes out: {effects:?}"
    );
    assert!(matches!(state, IdentifyState::Found { .. }), "got {state:?}");
    assert_eq!(
        ledger_of(&state).album_links,
        crate::identify::AlbumLinksStepView::Off
    );
}

/// A run with the cover art left unread and nothing else to read a code off
/// says the art was left unread — not that there was no barcode source, and
/// not that it read the art and found none. The catalog step says the same.
#[test]
fn cover_art_left_unread_says_so_in_the_barcode_and_catalog_steps() {
    let state = started_without(vec![MB], crate::config::IdentificationStep::ReadCoverArt);
    let (state, _) = step(
        state,
        IdentifyEvent::SignalsUpdated {
            signals: signals(
                DiscIdSignal::Absent { track_count: 2 },
                BarcodeSignal::Absent,
                &[],
            ),
            artwork: crate::signals::ArtworkScan::Off { total: 2 },
        },
    );
    let ledger = ledger_of(&state);
    assert_eq!(ledger.barcode, crate::identify::BarcodeStepView::CoverArtOff);
    assert_eq!(ledger.catalog, crate::identify::CatalogStepView::CoverArtOff);
}
