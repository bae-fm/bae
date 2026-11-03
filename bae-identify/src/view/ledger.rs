//! Builds the run's ledger, one step at a time.

use super::*;
use std::collections::HashSet;

/// Whether any identifier has found a release yet.
pub(super) fn identifiers_found_something(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
) -> bool {
    let disc = discid.answers().iter().any(|results| !results.is_empty());
    let code = !barcode.results().is_empty();
    let number = catalog.lookups().iter().any(|lookup| !lookup.results().is_empty());
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
    let query = context.search.query.as_ref().expect("a search runs only on a query");
    SearchStepView::Searched {
        album: query.album.clone(),
        artist: query.artist.clone(),
        cells: providers
            .iter()
            .map(|provider| ProviderCell {
                source: provider.source,
                lookup: lookup_view(&provider.state),
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
    let row =
        |disc_id: &str, lookup: LookupView| DiscIdRow { disc_id: disc_id.to_string(), lookup };
    let discs = match progress {
        // The disc IDs as their lookups were started.
        DiscidProgress::Lookups { discs } => {
            discs.iter().map(|disc| row(&disc.disc_id, lookup_view(&disc.state))).collect()
        }
        // The track count reaches a surface through the terminal state.
        DiscidProgress::Computing => {
            discs.iter().map(|disc| row(&disc.disc_id, LookupView::LookingUp)).collect()
        }
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
        IsrcProgress::Done { results } => LookupView::answered(results),
        IsrcProgress::NotAsked { reason } => LookupView::NotAsked { reason: *reason },
        IsrcProgress::Failed { failure } => LookupView::Failed { failure: failure.clone() },
    };
    IsrcStepView::Read { isrcs: context.isrc.codes(), lookup }
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
                            LookupView::NotAsked { reason: NotAskedReason::LeftOut },
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
        .map(|&source| ProviderCell { source, lookup: lookup.clone() })
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
                    LookupView::NotAsked { reason: NotAskedReason::LeftOut },
                ),
            });
        }
    }
    let candidates: Vec<CatalogCandidateView> = catalog
        .numbers
        .iter()
        .filter(|value| !catalog.holds(value) && !catalog.is_struck_out(value))
        .map(|value| CatalogCandidateView { value: value.clone() })
        .collect();
    if rows.is_empty() && candidates.is_empty() && !scanning {
        return CatalogStepView::NoneFound;
    }
    CatalogStepView::Numbers { scanning, rows, candidates }
}

/// One cell per provider asked about a value.
fn lookup_cells(lookup: &ValueLookup) -> Vec<ProviderCell> {
    lookup
        .providers
        .iter()
        .map(|provider| ProviderCell {
            source: provider.source,
            lookup: lookup_view(&provider.state),
        })
        .collect()
}

/// One lookup's cell.
fn lookup_view(state: &LookupState) -> LookupView {
    match state {
        LookupState::LookingUp => LookupView::LookingUp,
        LookupState::Done { results } => LookupView::answered(results),
        LookupState::Failed { failure } => LookupView::Failed { failure: failure.clone() },
    }
}

/// A number's row: one cell per provider it was searched on, standing for
/// every search of it there — one per artist it was searched under.
fn catalog_row(lookup: &CatalogLookup) -> SignalValueRow {
    let mut sources: Vec<LookupCatalog> = Vec::new();
    for provider in lookup.searches.iter().flat_map(|search| &search.providers) {
        if !sources.contains(&provider.source) {
            sources.push(provider.source);
        }
    }
    let cells = sources
        .into_iter()
        .map(|source| {
            let states: Vec<&LookupState> = lookup
                .searches
                .iter()
                .flat_map(|search| &search.providers)
                .filter(|provider| provider.source == source)
                .map(|provider| &provider.state)
                .collect();
            ProviderCell { source, lookup: searches_view(&states) }
        })
        .collect();
    SignalValueRow {
        value: lookup.value.clone(),
        // A number in effect is searched; a struck-out one is not.
        excluded: false,
        cells,
    }
}

/// What one provider's searches of a number show together: looking up while
/// any is, every release they found, else the first failure, else no match.
fn searches_view(states: &[&LookupState]) -> LookupView {
    if states.iter().any(|state| matches!(state, LookupState::LookingUp)) {
        return LookupView::LookingUp;
    }
    let found = crate::combine::union_all(states.iter().filter_map(|state| match state {
        LookupState::Done { results } => Some(results),
        LookupState::LookingUp | LookupState::Failed { .. } => None,
    }));
    if !found.is_empty() {
        return LookupView::answered(&found);
    }
    let failure = states.iter().find_map(|state| match state {
        LookupState::Failed { failure } => Some(failure),
        LookupState::LookingUp | LookupState::Done { .. } => None,
    });
    match failure {
        Some(failure) => LookupView::Failed { failure: failure.clone() },
        None => LookupView::NoMatch,
    }
}

impl LookupView {
    /// A settled lookup's cell: the releases it found in album cards, in the
    /// lookup's own order, or no match.
    pub fn answered(results: &LookupResults) -> Self {
        if results.is_empty() {
            return LookupView::NoMatch;
        }
        let groups = group_results(crate::release_group::unranked(results.clone()), None);
        LookupView::Found {
            count: groups.iter().map(|group| group.pressings().count() as u32).sum(),
            groups,
        }
    }

    /// The releases a cell that found some lists, card by card and row by
    /// row: the order [`Self::answered`] lays them out in, which it lays out
    /// again as they stand. None for any other cell.
    pub fn releases(&self) -> Vec<&MetadataResult> {
        let LookupView::Found { groups, .. } = self else {
            return Vec::new();
        };
        groups
            .iter()
            .flat_map(|group| &group.sections)
            .flat_map(|section| section.pressings.iter().chain(&section.narrowed_out))
            .flat_map(|pressing| &pressing.releases)
            .collect()
    }
}

impl IdentifyRunView {
    /// Every release a lookup of the folder's disc IDs found: what the Disc
    /// ID agreement is read from, since no record states the disc IDs it
    /// carries.
    pub(crate) fn disc_id_releases(&self) -> HashSet<bae_catalogs::records::LookupRef> {
        let DiscIdStepView::Read { discs } = &self.disc_id else {
            return HashSet::new();
        };
        discs.iter().flat_map(|disc| disc.lookup.releases()).map(MetadataResult::release).collect()
    }

    /// Every lookup's cell, clue by clue, row by row and cell by cell.
    pub fn lookups(&self) -> Vec<&LookupView> {
        fn cells(cells: &[ProviderCell]) -> impl Iterator<Item = &LookupView> {
            cells.iter().map(|cell| &cell.lookup)
        }
        let mut lookups = Vec::new();
        if let DiscIdStepView::Read { discs } = &self.disc_id {
            lookups.extend(discs.iter().map(|disc| &disc.lookup));
        }
        if let BarcodeStepView::Rows { rows, .. } = &self.barcode {
            lookups.extend(rows.iter().flat_map(|row| cells(&row.cells)));
        }
        if let CatalogStepView::Numbers { rows, .. } = &self.catalog {
            lookups.extend(rows.iter().flat_map(|row| cells(&row.cells)));
        }
        if let IsrcStepView::Read { lookup, .. } = &self.isrc {
            lookups.push(lookup);
        }
        if let SearchStepView::Searched { cells: searched, .. } = &self.search {
            lookups.extend(cells(searched));
        }
        lookups
    }

    /// Every lookup's cell, as [`Self::lookups`] orders them, to change.
    fn lookups_mut(&mut self) -> Vec<&mut LookupView> {
        let mut lookups = Vec::new();
        if let DiscIdStepView::Read { discs } = &mut self.disc_id {
            lookups.extend(discs.iter_mut().map(|disc| &mut disc.lookup));
        }
        if let BarcodeStepView::Rows { rows, .. } = &mut self.barcode {
            lookups.extend(
                rows.iter_mut().flat_map(|row| row.cells.iter_mut().map(|cell| &mut cell.lookup)),
            );
        }
        if let CatalogStepView::Numbers { rows, .. } = &mut self.catalog {
            lookups.extend(
                rows.iter_mut().flat_map(|row| row.cells.iter_mut().map(|cell| &mut cell.lookup)),
            );
        }
        if let IsrcStepView::Read { lookup, .. } = &mut self.isrc {
            lookups.push(lookup);
        }
        if let SearchStepView::Searched { cells, .. } = &mut self.search {
            lookups.extend(cells.iter_mut().map(|cell| &mut cell.lookup));
        }
        lookups
    }

    /// The ledger with every release its cells list as the run last read
    /// it: the copy of it `answers` holds first, its documents and album
    /// links applied, which is the copy the run's findings hold. A cell then
    /// shows what the rows show, and a stored verdict, which keeps each
    /// release once, lays the ledger out as it was recorded.
    pub(crate) fn read_as(mut self, answers: &LookupAnswers) -> Self {
        let mut reads: std::collections::HashMap<(LookupCatalog, &str), &MetadataResult> =
            std::collections::HashMap::new();
        for result in answers.all() {
            reads.entry((result.source, result.release_id.as_str())).or_insert(result);
        }
        for lookup in self.lookups_mut() {
            let releases: Vec<MetadataResult> = lookup
                .releases()
                .into_iter()
                .map(|release| {
                    (*reads
                        .get(&(release.source, release.release_id.as_str()))
                        .expect("a cell lists releases its lookup returned to the run"))
                    .clone()
                })
                .collect();
            if !releases.is_empty() {
                *lookup = LookupView::answered(&releases);
            }
        }
        self
    }
}
