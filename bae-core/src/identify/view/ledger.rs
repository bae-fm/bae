//! Builds the run's ledger, one step at a time.

use super::*;

/// Whether any identifier has found a release yet.
pub(super) fn identifiers_found_something(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
) -> bool {
    let disc = discid.answers().iter().any(|results| !results.is_empty());
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

/// The disc-ID step: one row per disc ID with its lookup — the disc IDs as
/// their lookups were started once they were, else as the context read them.
pub(super) fn disc_id_step(progress: &DiscidProgress, context: &SignalsContext) -> DiscIdStepView {
    let discs = match &context.disc.signal {
        DiscIdSignal::Computed { discs } => discs,
        DiscIdSignal::Absent => {
            return match progress {
                DiscidProgress::Computing => DiscIdStepView::Reading,
                _ => DiscIdStepView::Absent,
            }
        }
        DiscIdSignal::NotCdAudio => return DiscIdStepView::NotCdAudio,
        DiscIdSignal::Failed { .. } => {
            unreachable!("a run whose Disc ID could not be derived ended as an error")
        }
    };
    let row = |disc_id: &str, lookup: LookupView| DiscIdRow {
        disc_id: disc_id.to_string(),
        lookup,
    };
    let discs = match progress {
        // The disc IDs as their lookups were started.
        DiscidProgress::Lookups { discs } => discs
            .iter()
            .map(|disc| {
                row(
                    &disc.disc_id,
                    match &disc.state {
                        LookupState::LookingUp => LookupView::LookingUp,
                        LookupState::Done { results } => found_or_no_match(results),
                        LookupState::Failed { failure } => LookupView::Failed {
                            failure: failure.clone(),
                        },
                    },
                )
            })
            .collect(),
        // The track count reaches a surface through the terminal state.
        DiscidProgress::Computing => discs
            .iter()
            .map(|disc| row(&disc.disc_id, LookupView::LookingUp))
            .collect(),
        DiscidProgress::NotAsked { reason } => discs
            .iter()
            .map(|disc| row(&disc.disc_id, LookupView::NotAsked { reason: *reason }))
            .collect(),
        DiscidProgress::Skipped => {
            unreachable!("only a Disc ID that was never computed is skipped")
        }
    };
    DiscIdStepView::Read { discs }
}

/// The ISRC step: the codes from the context, their lookup from the pipe.
pub(super) fn isrc_step(progress: &IsrcProgress, context: &SignalsContext) -> IsrcStepView {
    let lookup = match progress {
        IsrcProgress::Reading => return IsrcStepView::Reading,
        IsrcProgress::Skipped => return IsrcStepView::Absent,
        IsrcProgress::LookingUp => LookupView::LookingUp,
        IsrcProgress::Done { results } => found_or_no_match(results),
        IsrcProgress::NotAsked { reason } => LookupView::NotAsked { reason: *reason },
        IsrcProgress::Failed { failure } => LookupView::Failed {
            failure: failure.clone(),
        },
    };
    IsrcStepView::Read {
        isrcs: context.isrc.codes(),
        lookup,
    }
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
    let catalog = &context.catalog;
    let mut rows: Vec<SignalValueRow> = progress.lookups().iter().map(catalog_row).collect();
    // A number in effect not asked yet — confirmed by the answers in hand,
    // its search still to go out.
    for value in catalog.in_effect() {
        if !rows.iter().any(|row| row.value == value) {
            rows.push(SignalValueRow {
                value,
                excluded: false,
                cells: uniform_cells(context, LookupView::Queued),
            });
        }
    }
    // A struck-out number stays in the row, left out, so it can be put back.
    for value in &catalog.struck_out {
        if catalog.is_chosen(value) || context.text.prints_catalog(value) {
            rows.push(SignalValueRow {
                value: value.clone(),
                excluded: true,
                cells: uniform_cells(
                    context,
                    LookupView::NotAsked {
                        reason: NotAskedReason::LeftOut,
                    },
                ),
            });
        }
    }
    let candidates: Vec<CatalogCandidateView> = catalog
        .numbers
        .iter()
        .filter(|value| !catalog.holds(value) && !catalog.is_struck_out(value))
        .map(|value| CatalogCandidateView {
            value: value.clone(),
        })
        .collect();
    if rows.is_empty() && candidates.is_empty() && !scanning {
        return CatalogStepView::NoneFound;
    }
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
        // A number in effect is searched; a struck-out one is not.
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
    let groups = group_results(
        crate::import::release_group::unranked(
            results.iter().map(|(result, _)| result.clone()).collect(),
        ),
        None,
    );
    LookupView::Found {
        count: groups
            .iter()
            .map(|group| group.pressings().count() as u32)
            .sum(),
        groups,
    }
}
