use super::*;

fn found() -> LookupView {
    LookupView::Found { count: 1, groups: Vec::new() }
}

/// A chip is off where the value is left out, found where a lookup found
/// releases and on otherwise.
#[test]
fn a_chip_looks_by_what_its_lookups_did() {
    let no_match = LookupView::NoMatch;
    assert_eq!(ChipState::of(false, &[&no_match]).look, ChipLook::On);
    assert_eq!(ChipState::of(false, &[&no_match, &found()]).look, ChipLook::Found);
    let none_found = LookupView::Found { count: 0, groups: Vec::new() };
    assert_eq!(ChipState::of(false, &[&none_found]).look, ChipLook::On);
    let left_out = LookupView::NotAsked { reason: NotAskedReason::LeftOut };
    assert_eq!(ChipState::of(false, &[&left_out]).look, ChipLook::Off);
}

/// A value the person struck out is off whatever its cells say, and asked of
/// nobody.
#[test]
fn a_value_left_out_is_off_and_trails_nothing() {
    let failed = LookupView::Failed { failure: LookupFailure::Timeout };
    let chip = ChipState::of(true, &[&found(), &failed]);
    assert_eq!(chip, ChipState { look: ChipLook::Off, trailing: ChipTrailing::Nothing });
}

/// The words after a value are the first failure, else a lookup still out.
#[test]
fn a_chip_trails_its_first_failure_else_a_lookup_still_out() {
    let failed = LookupView::Failed { failure: LookupFailure::Timeout };
    let out = LookupView::LookingUp;
    assert_eq!(
        ChipState::of(false, &[&out, &failed]).trailing,
        ChipTrailing::Failed { failure: LookupFailure::Timeout }
    );
    assert_eq!(ChipState::of(false, &[&out, &found()]).trailing, ChipTrailing::Searching);
    assert_eq!(ChipState::of(false, &[&found()]).trailing, ChipTrailing::Nothing);
}

/// The kinds that found releases come in the band's order, and a kind with
/// no found chip is left out.
#[test]
fn the_found_kinds_follow_the_band_s_order() {
    let row = |lookup: LookupView| SignalValueRow {
        value: "v".to_string(),
        excluded: false,
        cells: vec![ProviderCell { source: bae_catalogs::catalog::LookupCatalog::Discogs, lookup }],
    };
    let run = IdentifyRunView {
        providers: Vec::new(),
        disc_id: DiscIdStepView::Absent,
        barcode: BarcodeStepView::Rows { scanning: false, rows: vec![row(LookupView::NoMatch)] },
        catalog: CatalogStepView::Numbers {
            scanning: false,
            rows: vec![row(found())],
            candidates: Vec::new(),
            candidate_count: 0,
        },
        isrc: IsrcStepView::Absent,
        search: SearchStepView::NotNeeded,
    };
    assert_eq!(run.found_kinds(), [IdentifierKind::Catalog]);
}
