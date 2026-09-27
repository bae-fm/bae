//! Builds the run's ledger, one step at a time.

use super::*;

/// Whether any identifier has found a release yet.
pub(super) fn identifiers_found_something(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
) -> bool {
    let disc = matches!(discid, DiscidProgress::Done { results, .. } if !results.is_empty());
    let code = !barcode.results().is_empty();
    let number = matches!(catalog, CatalogProgress::Lookups { values }
    if values.iter().flat_map(|lookup| &lookup.providers).any(|provider| {
        matches!(&provider.state, LookupState::Done { results } if !results.is_empty())
    }));
    disc || code || number
}

/// The title-search step. Once any identifier has found something the search
/// is not needed, even while others are still looking.
pub(super) fn search_step(
    progress: &SearchProgress,
    identifiers_found_something: bool,
    context: &SignalsContext,
) -> SearchStepView {
    let providers = match (progress, &context.search.query) {
        (SearchProgress::NotAsked { reason }, _) => {
            return SearchStepView::NotAsked { reason: *reason }
        }
        (_, None) => return SearchStepView::NoTitle,
        (SearchProgress::Pending, Some(_)) if identifiers_found_something => {
            return SearchStepView::NotNeeded
        }
        (SearchProgress::Pending, Some(query)) => {
            return SearchStepView::Waiting {
                album: query.album.clone(),
                artist: query.artist.clone(),
            }
        }
        (SearchProgress::Skipped, Some(_)) => return SearchStepView::NotNeeded,
        (SearchProgress::Lookups { providers }, Some(_)) => providers,
    };
    let query = context
        .search
        .query
        .as_ref()
        .expect("a search runs only on a query");
    SearchStepView::Searched {
        album: query.album.clone(),
        artist: query.artist.clone(),
        cells: providers
            .iter()
            .map(|provider| ProviderCell {
                source: provider.source,
                lookup: match &provider.state {
                    LookupState::LookingUp => LookupView::LookingUp,
                    LookupState::Done { results } => found_or_no_match(results),
                    LookupState::Failed { failure } => LookupView::Failed {
                        failure: failure.clone(),
                    },
                },
            })
            .collect(),
    }
}

/// The disc-ID step: the value from the context, its lookup from the pipe.
pub(super) fn disc_id_step(progress: &DiscidProgress, context: &SignalsContext) -> DiscIdStepView {
    let disc_id = match &context.disc.signal {
        DiscIdSignal::Computed { disc_id, .. } => disc_id.clone(),
        DiscIdSignal::Absent { .. } => {
            return match progress {
                DiscidProgress::Computing => DiscIdStepView::Reading,
                _ => DiscIdStepView::Absent,
            }
        }
        DiscIdSignal::NotCdAudio { sample_rate_hz, .. } => {
            return DiscIdStepView::NotCdAudio {
                sample_rate_hz: *sample_rate_hz,
            }
        }
        DiscIdSignal::Failed { failure, .. } => {
            return DiscIdStepView::ReadFailed {
                failure: failure.clone(),
            }
        }
    };
    let lookup = match progress {
        // The track count reaches a surface through the terminal state.
        DiscidProgress::Computing | DiscidProgress::LookingUp => LookupView::LookingUp,
        DiscidProgress::Done { results, .. } => found_or_no_match(results),
        DiscidProgress::NotAsked { reason, .. } => LookupView::NotAsked { reason: *reason },
        DiscidProgress::Skipped { .. } => {
            unreachable!("only a disc ID that was never computed is skipped")
        }
        DiscidProgress::Failed { failure, .. } => LookupView::Failed {
            failure: failure.clone(),
        },
    };
    DiscIdStepView::Read { disc_id, lookup }
}

pub(super) fn barcode_step(
    progress: &BarcodeProgress,
    context: &SignalsContext,
    scanning: bool,
) -> BarcodeStepView {
    let row = |code: String, excluded: bool, cells: Vec<ProviderCell>| SignalValueRow {
        value: code,
        excluded,
        cells,
    };
    match progress {
        // The codes are asked once they settle, so every cell waits.
        BarcodeProgress::Scanning => BarcodeStepView::Rows {
            scanning: true,
            rows: context
                .barcode
                .code_values()
                .into_iter()
                .map(|code| row(code, false, uniform_cells(context, LookupView::Queued)))
                .collect(),
        },
        BarcodeProgress::NoCodes => BarcodeStepView::NoCodes,
        BarcodeProgress::ScanFailed { failure } => BarcodeStepView::ScanFailed {
            failure: failure.clone(),
        },
        // Nothing was read: say so when cover art is left unread.
        BarcodeProgress::Skipped if matches!(context.artwork, ArtworkScan::Off) => {
            BarcodeStepView::CoverArtOff
        }
        BarcodeProgress::Skipped => BarcodeStepView::Absent,
        BarcodeProgress::NotAsked { codes, reason } => BarcodeStepView::Rows {
            scanning: false,
            rows: codes
                .iter()
                .map(|code| {
                    let excluded = context.barcode.excluded.contains(code);
                    row(
                        code.clone(),
                        excluded,
                        uniform_cells(context, LookupView::NotAsked { reason: *reason }),
                    )
                })
                .collect(),
        },
        // A code without a lookup is one the person left out.
        BarcodeProgress::Lookups { codes } => BarcodeStepView::Rows {
            scanning,
            rows: context
                .barcode
                .code_values()
                .into_iter()
                .map(|code| {
                    let cells = match codes.iter().find(|asked| asked.value == code) {
                        Some(lookup) => lookup_cells(lookup),
                        None => uniform_cells(
                            context,
                            LookupView::NotAsked {
                                reason: NotAskedReason::LeftOut,
                            },
                        ),
                    };
                    let excluded = context.barcode.excluded.contains(&code);
                    row(code, excluded, cells)
                })
                .collect(),
        },
    }
}

/// One identical cell per provider the run asks.
fn uniform_cells(context: &SignalsContext, lookup: LookupView) -> Vec<ProviderCell> {
    context
        .providers
        .iter()
        .map(|&source| ProviderCell {
            source,
            lookup: lookup.clone(),
        })
        .collect()
}

pub(super) fn catalog_step(
    progress: &CatalogProgress,
    context: &SignalsContext,
    scanning: bool,
) -> CatalogStepView {
    let numbers = &context.catalog.numbers;
    if numbers.is_empty() && !scanning {
        return if matches!(context.artwork, ArtworkScan::Off) {
            CatalogStepView::CoverArtOff
        } else {
            CatalogStepView::NoneFound
        };
    }
    let rows = progress.lookups().iter().map(catalog_row).collect();
    let candidates = numbers
        .iter()
        .filter(|value| !context.catalog.is_chosen(value))
        .map(|value| CatalogCandidateView {
            value: value.clone(),
        })
        .collect();
    CatalogStepView::Numbers {
        scanning,
        rows,
        candidates,
    }
}

/// One cell per provider asked about a value.
fn lookup_cells(lookup: &ValueLookup) -> Vec<ProviderCell> {
    lookup
        .providers
        .iter()
        .map(|provider| ProviderCell {
            source: provider.source,
            lookup: match &provider.state {
                LookupState::LookingUp => LookupView::LookingUp,
                LookupState::Done { results } => found_or_no_match(results),
                LookupState::Failed { failure } => LookupView::Failed {
                    failure: failure.clone(),
                },
            },
        })
        .collect()
}

fn catalog_row(lookup: &ValueLookup) -> SignalValueRow {
    SignalValueRow {
        value: lookup.value.clone(),
        // A catalog row exists only while its number is looked up.
        excluded: false,
        cells: lookup_cells(lookup),
    }
}

/// A settled lookup's cell: its releases in album cards, in the lookup's own
/// order, or no match.
fn found_or_no_match(results: &LookupResults) -> LookupView {
    if results.is_empty() {
        return LookupView::NoMatch;
    }
    let groups = group_results(crate::import::release_group::unranked(
        results.iter().map(|(result, _)| result.clone()).collect(),
    ));
    LookupView::Found {
        count: groups
            .iter()
            .map(|group| group.pressings().count() as u32)
            .sum(),
        groups,
    }
}
