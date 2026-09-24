//! The run as a ledger: one row per value extraction found, with where it
//! was found beside it and one cell per provider asked about it. Built while
//! the run goes, and recorded once as it ends.

use super::*;

/// Whether any identifier has already found a release, whether or not the
/// others have finished looking.
pub(super) fn identifiers_found_something(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
) -> bool {
    let disc = matches!(discid, DiscidProgress::Done { results, .. } if !results.is_empty());
    let code = matches!(barcode, BarcodeProgress::Lookups { providers, .. }
        if providers.iter().any(|provider| matches!(provider.state, BarcodeLookupState::Matched { .. })));
    let number = matches!(catalog, CatalogProgress::Lookups { values }
    if values.iter().flat_map(|lookup| &lookup.providers).any(|provider| {
        matches!(&provider.state, LookupState::Done { results } if !results.is_empty())
    }));
    disc || code || number
}

/// The title-search step: the words the run searched by, from the context,
/// and how far each provider's lookup of them has got, from the pipe.
///
/// A step that has not run says which of the two reasons applies: the
/// candidate's draft states no title, or there was a title and the
/// identifiers answered before it was needed. An identifier that has already
/// found something while the rest are still looking has answered too: the
/// search runs only when all of them find nothing, so it is not needed. Only
/// while nothing has been found yet is the step waiting on them.
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

/// The disc-ID step: what extraction read, from the context, and how far
/// MusicBrainz's lookup of it has got, from the pipe.
pub(super) fn disc_id_step(progress: &DiscidProgress, context: &SignalsContext) -> DiscIdStepView {
    let (disc_id, source_file) = match &context.disc.signal {
        DiscIdSignal::Computed {
            disc_id,
            source_file,
            ..
        } => (disc_id.clone(), source_file.clone()),
        DiscIdSignal::Absent { .. } => {
            return match progress {
                DiscidProgress::Computing => DiscIdStepView::Reading,
                _ => DiscIdStepView::Absent,
            }
        }
        DiscIdSignal::Failed { failure, .. } => {
            return DiscIdStepView::ReadFailed {
                failure: failure.clone(),
            }
        }
    };
    // Two things leave a read disc ID unasked, and they are not the same thing
    // to a person looking at it: they took it out of the run, or no provider
    // the run asks answers disc IDs at all.
    if let DiscidProgress::NotAsked { .. } = progress {
        let source = source_file.map(disc_id_file);
        return if context.disc.excluded {
            DiscIdStepView::LeftOut { disc_id, source }
        } else {
            DiscIdStepView::ReadNotAsked { disc_id, source }
        };
    }
    let lookup = match progress {
        // The track count is a settled-state concern — it reaches a surface
        // through the terminal state, not through progress.
        DiscidProgress::Computing | DiscidProgress::LookingUp => LookupView::LookingUp,
        DiscidProgress::Done { results, .. } => found_or_no_match(results),
        DiscidProgress::Skipped { .. } | DiscidProgress::NotAsked { .. } => {
            unreachable!("a computed disc ID is skipped only by the early return above")
        }
        DiscidProgress::Failed { failure, .. } => LookupView::Failed {
            failure: failure.clone(),
        },
    };
    DiscIdStepView::Read {
        disc_id,
        source: source_file.map(disc_id_file),
        lookup,
    }
}

/// The file a disc ID was read off, by the kind of artifact it is. A disc ID
/// is derived from a rip log or a cue sheet and nothing else, so a file that
/// is not a log is a sheet.
fn disc_id_file(file: String) -> DiscIdFile {
    let is_log = std::path::Path::new(&file)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("log"));
    DiscIdFile {
        kind: if is_log {
            DiscIdFileKind::Log
        } else {
            DiscIdFileKind::Cue
        },
        file,
    }
}

pub(super) fn barcode_step(
    progress: &BarcodeProgress,
    context: &SignalsContext,
    scanning: bool,
) -> BarcodeStepView {
    let row = |code: String, excluded: bool, cells: Vec<ProviderCell>| SignalValueRow {
        sources: sources_of(&context.barcode.codes, &code),
        value: code,
        excluded,
        cells,
    };
    match progress {
        // The walks start once the codes settle: every code read so far is a
        // row whose cells wait, and more rows may still come.
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
        BarcodeProgress::Skipped => BarcodeStepView::Absent,
        // Every code is the run's and nobody was asked about any of them: every
        // row stands with its cells saying so, rather than reading as a lookup
        // that found nothing.
        BarcodeProgress::NotAsked { codes } => BarcodeStepView::Rows {
            scanning: false,
            rows: codes
                .iter()
                .map(|code| {
                    row(
                        code.clone(),
                        true,
                        uniform_cells(context, LookupView::NotAsked),
                    )
                })
                .collect(),
        },
        // Every code the candidate carries is a row. The ones the run asks
        // about take their cells from where each walk has got to, at the code's
        // index among the asked ones; the rest were never asked, and the
        // person's own choices say which of them they left out.
        BarcodeProgress::Lookups { codes, providers } => BarcodeStepView::Rows {
            scanning,
            rows: context
                .barcode
                .code_values()
                .into_iter()
                .map(|code| {
                    let cells = match codes.iter().position(|asked| *asked == code) {
                        Some(index) => providers
                            .iter()
                            .map(|provider| ProviderCell {
                                source: provider.source,
                                lookup: barcode_cell(&provider.state, index, codes),
                            })
                            .collect(),
                        None => uniform_cells(context, LookupView::NotAsked),
                    };
                    let excluded = context.barcode.excluded.contains(&code);
                    row(code, excluded, cells)
                })
                .collect(),
        },
    }
}

/// One cell per provider the run asks, all saying the same thing: a row whose
/// codes are still queued, or one nobody was asked about.
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

/// One provider's cell for the code at `index`, from where its walk is. A
/// walk asks the codes in order and stops at the first match or failure, so
/// where it is says what it did with every code: the ones before it missed,
/// the one it is on it is asking about, and the ones after wait — or, once it
/// has stopped, were never needed.
fn barcode_cell(walk: &BarcodeLookupState, index: usize, codes: &[String]) -> LookupView {
    match walk {
        BarcodeLookupState::Trying { index: at } => match index.cmp(at) {
            std::cmp::Ordering::Less => LookupView::NoMatch,
            std::cmp::Ordering::Equal => LookupView::LookingUp,
            std::cmp::Ordering::Greater => LookupView::Queued,
        },
        BarcodeLookupState::Matched { code, results } => {
            let at = codes
                .iter()
                .position(|c| c == code)
                .expect("a walk matches one of the codes it asks");
            match index.cmp(&at) {
                std::cmp::Ordering::Less => LookupView::NoMatch,
                std::cmp::Ordering::Equal => found_or_no_match(results),
                std::cmp::Ordering::Greater => LookupView::NotAsked,
            }
        }
        BarcodeLookupState::Exhausted => LookupView::NoMatch,
        BarcodeLookupState::Failed { failure, index: at } => match index.cmp(at) {
            std::cmp::Ordering::Less => LookupView::NoMatch,
            std::cmp::Ordering::Equal => LookupView::Failed {
                failure: failure.clone(),
            },
            std::cmp::Ordering::Greater => LookupView::NotAsked,
        },
    }
}

pub(super) fn catalog_step(
    progress: &CatalogProgress,
    context: &SignalsContext,
    scanning: bool,
) -> CatalogStepView {
    let numbers = context.catalog.number_values();
    if numbers.is_empty() && !scanning {
        return CatalogStepView::NoneFound;
    }
    let rows = progress
        .lookups()
        .iter()
        .map(|lookup| catalog_row(lookup, context))
        .collect();
    let candidates = numbers
        .into_iter()
        .filter(|value| !context.catalog.is_chosen(value))
        .map(|value| CatalogCandidateView {
            sources: sources_of(&context.catalog.numbers, &value),
            value,
        })
        .collect();
    CatalogStepView::Numbers {
        scanning,
        rows,
        candidates,
    }
}

fn catalog_row(lookup: &CatalogLookup, context: &SignalsContext) -> SignalValueRow {
    SignalValueRow {
        value: lookup.value.clone(),
        sources: sources_of(&context.catalog.numbers, &lookup.value),
        // A catalog row exists only for a number the run looks up: taking one
        // out drops its row and leaves the number offered as a candidate.
        excluded: false,
        cells: lookup
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
            .collect(),
    }
}

/// Every place `value` was read, in the order it was read there.
fn sources_of(sightings: &[crate::signals::SourcedValue], value: &str) -> Vec<ValueSource> {
    sightings
        .iter()
        .filter(|sighting| sighting.value == value)
        .map(|sighting| ValueSource {
            origin: sighting.origin,
            file: sighting.origin_path.clone(),
            region: sighting.region,
        })
        .collect()
}

/// What a settled lookup turned up: its releases folded into album cards, or
/// nothing. One cell of the ledger — what this lookup alone saw, before
/// anything else narrowed it — so the rows are the lookup's own order.
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
