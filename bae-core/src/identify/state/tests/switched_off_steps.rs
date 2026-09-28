// Steps switched off in the identification settings ask nobody and say so.
// Included by `tests.rs`, sharing the helpers of `signals_and_conflicts.rs`.

const SWITCHED_OFF: crate::identify::NotAskedReason = crate::identify::NotAskedReason::SwitchedOff;

/// A run started with every step on but `off`, with a title to search by.
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
            registered_in: None,
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

/// A disc ID under a run that does not look disc IDs up is asked of nobody,
/// and its row and badge say the step is switched off.
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
            lookup: crate::identify::LookupView::NotAsked {
                reason: SWITCHED_OFF
            },
            ..
        }
    ));
    assert_eq!(
        badge(&state, SignalKind::DiscId).state,
        SignalState::NotAsked {
            reason: SWITCHED_OFF
        }
    );
}

/// A disc ID left out says so even when its step is off, and the run offers
/// manual search.
#[test]
fn a_left_out_disc_id_says_it_was_left_out_whatever_its_step() {
    let mut steps = crate::config::IdentificationSteps::default();
    steps.set(crate::config::IdentificationStep::LookUpDiscIds, false);
    let (state, _) = step(
        IdentifyState::Idle,
        IdentifyEvent::Started {
            providers: vec![MB],
            steps,
            choices: excluding(true, &[]),
            title_search: None,
            registered_in: None,
        },
    );
    let (state, effects) = update(state, disc_only(&[]));
    assert!(effects.is_empty(), "nothing is asked: {effects:?}");
    let left_out = crate::identify::NotAskedReason::LeftOut;
    assert!(matches!(
        ledger_of(&state).disc_id,
        crate::identify::DiscIdStepView::Read {
            lookup: crate::identify::LookupView::NotAsked { reason },
            ..
        } if reason == left_out
    ));
    let disc = badge(&state, SignalKind::DiscId);
    assert!(disc.excluded);
    assert_eq!(disc.state, SignalState::NotAsked { reason: left_out });
    assert!(
        matches!(state, IdentifyState::ManualOnly { .. }),
        "got {state:?}"
    );
}

/// Barcodes under a run that does not look them up stay listed, each cell
/// saying the step is switched off.
#[test]
fn barcodes_the_run_does_not_look_up_are_listed_with_every_cell_off() {
    let state = started_without(vec![MB, DG], crate::config::IdentificationStep::LookUpBarcodes);
    let (state, effects) = update(
        state,
        signals(
            DiscIdSignal::Absent,
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
        .all(|cell| cell.lookup
            == crate::identify::LookupView::NotAsked {
                reason: SWITCHED_OFF
            }));
    assert_eq!(
        badge(&state, SignalKind::Barcode).state,
        SignalState::NotAsked {
            reason: SWITCHED_OFF
        }
    );
}

/// A run that does not search by title never asks it, and says so from the
/// start.
#[test]
fn a_run_that_does_not_search_by_title_never_asks_the_title() {
    let not_asked = crate::identify::SearchStepView::NotAsked {
        reason: SWITCHED_OFF,
    };
    let state = started_without(vec![MB], crate::config::IdentificationStep::SearchByTitle);
    assert_eq!(ledger_of(&state).search, not_asked);
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
    assert_eq!(ledger_of(&state).search, not_asked);
}

/// A run whose lookups are all switched off offers manual search.
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
            registered_in: None,
        },
    );
    let (state, effects) = update(state, disc_only(&[]));
    assert!(effects.is_empty(), "nothing is asked: {effects:?}");
    assert!(
        matches!(state, IdentifyState::ManualOnly { .. }),
        "got {state:?}"
    );
    assert_eq!(
        crate::identify::VerdictSummary::of(&crate::identify::TerminalVerdict::try_from(state).unwrap()).judgement(),
        (false, None)
    );
}

/// A run that does not follow catalog links reads none, even with both
/// catalogs' releases found.
#[test]
fn a_run_that_does_not_follow_catalog_links_reads_none() {
    let state = started_without(
        vec![MB, DG],
        crate::config::IdentificationStep::FollowCatalogLinks,
    );
    let (state, _) = update(
        state,
        signals(
            DiscIdSignal::Absent,
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
    let IdentifyState::Found { context, .. } = &state else {
        panic!("the barcode named a release on each catalog, got {state:?}");
    };
    assert_eq!(
        context.album_links,
        AlbumLinkReading::NotAsked {
            reason: SWITCHED_OFF
        }
    );
}

/// With the cover art left unread, the barcode and catalog steps say so.
#[test]
fn cover_art_left_unread_says_so_in_the_barcode_and_catalog_steps() {
    let state = started_without(vec![MB], crate::config::IdentificationStep::ReadCoverArt);
    let (state, _) = step(
        state,
        IdentifyEvent::SignalsUpdated {
            signals: signals(
                DiscIdSignal::Absent,
                BarcodeSignal::Absent,
                &[],
            ),
            audio: crate::signals::AudioFacts::default(),
            artwork: crate::signals::ArtworkScan::Off,
        },
    );
    let ledger = ledger_of(&state);
    assert_eq!(ledger.barcode, crate::identify::BarcodeStepView::CoverArtOff);
    assert_eq!(ledger.catalog, crate::identify::CatalogStepView::CoverArtOff);
}
