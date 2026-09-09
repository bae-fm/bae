//! Pure state machine for the identify pipeline.
//!
//! Triangulation: the disc-ID, barcode and catalog signals run in parallel,
//! each reporting progress live so the UI can render them side by side. The
//! barcode and catalog lookups ask every provider in the run, and each provider
//! answers for itself — MusicBrainz landing never waits on Discogs, and one
//! failing leaves the other's matches standing. Once every signal settles, the
//! reducer hands their results to `combine` and lands on `Found`,
//! `NotFoundAnywhere`, or `Failed`.
//!
//! Settling also records the run's ledger — the layout every surface has been
//! drawing while it ran — onto the terminal state, so what the run showed
//! survives the run.
//!
//! `step` takes a state and an event and returns the next state plus the side
//! effects for the service to run. No I/O, no async, nothing outside itself.

use super::combine::{combine_results, CombineOutcome, LookupProvenance, NarrowedOut};
use super::toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
use super::view::{run_view, IdentifyRunView};
use crate::db::LibraryStatus;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::{LookupChoices, MetadataSource};
use crate::signals::{ArtworkScan, BarcodeSignal, LookupFailure, SignalOrigin, Signals};

/// One candidate's identify state.
///
/// Every state but `Idle` carries a [`SignalsContext`], so the toolbar
/// projection always has its signal values.
///
/// Every settled state carries the ledger its run recorded as it ended — the
/// last frame the run showed, with the lookups that were still in flight
/// settled onto it. `None` when extraction handed the run nothing to lay out,
/// and for a state stood back up from a verdict whose row records none.
#[derive(Clone, Debug, PartialEq)]
pub enum IdentifyState {
    Idle,

    /// Lookups in flight. Each signal progresses independently; `settle_if_ready`
    /// combines them into a terminal state once all three are settled, and
    /// records the ledger they settled as. The catalog pipe rests at `Skipped`
    /// until a number is chosen.
    Triangulating {
        discid: DiscidProgress,
        barcode: BarcodeProgress,
        catalog: CatalogProgress,
        context: SignalsContext,
    },

    Found {
        matches: Vec<MetadataResult>,
        library_statuses: Vec<LibraryStatus>,
        track_count: u32,
        /// Per-match provenance (which signals produced/confirmed each row),
        /// index-aligned with `matches` — drives the per-row signal badges, and
        /// says which signal produced any given match.
        provenance: Vec<LookupProvenance>,
        /// The releases the signals' agreement left out of `matches`. Empty
        /// when nothing was narrowed.
        narrowed_out: NarrowedOut,
        ledger: Option<IdentifyRunView>,
        context: SignalsContext,
    },

    NotFoundAnywhere {
        ledger: Option<IdentifyRunView>,
        context: SignalsContext,
    },

    /// Nothing to look up: no disc-ID artifact (LOG/CUE) and no barcode source
    /// (artwork, CUE `CATALOG`). Distinct from `NotFoundAnywhere`, where signals
    /// ran and matched nothing — here none ran, so the UI offers manual search.
    ManualOnly {
        track_count: u32,
        ledger: Option<IdentifyRunView>,
        context: SignalsContext,
    },

    /// An automatic lookup failed, either in the live reducer or resumed from
    /// its stored verdict after the run ended.
    ///
    /// It carries whatever the surviving evidence combined to, which is
    /// usually not nothing: one provider failing on the barcode leaves the
    /// other provider's matches standing, and a person looking at the pane
    /// should see them rather than an empty result area. They are not a
    /// verdict — the failure is what stores — so a resumed failure has none.
    Failed {
        failures: Vec<super::IdentifyFailure>,
        matches: Vec<MetadataResult>,
        library_statuses: Vec<LibraryStatus>,
        provenance: Vec<LookupProvenance>,
        /// The releases the surviving signals' agreement left out of
        /// `matches`. Empty when nothing was narrowed, and for a failure
        /// resumed from its stored verdict.
        narrowed_out: NarrowedOut,
        track_count: u32,
        ledger: Option<IdentifyRunView>,
        context: SignalsContext,
    },
}

impl IdentifyState {
    /// The carried context; `None` only for `Idle`. One access path shared by
    /// `step`, the toolbar projection, and the signal actions.
    fn context(&self) -> Option<&SignalsContext> {
        match self {
            IdentifyState::Triangulating { context, .. }
            | IdentifyState::Found { context, .. }
            | IdentifyState::NotFoundAnywhere { context, .. }
            | IdentifyState::ManualOnly { context, .. }
            | IdentifyState::Failed { context, .. } => Some(context),
            IdentifyState::Idle => None,
        }
    }

    /// Whether the machine has stopped moving on its own: nothing is in
    /// flight, so this run has nothing left to do. The driver ends here, and
    /// what a person asks for next is a run of its own.
    ///
    /// A lookup failure is terminal too; conversion preserves it as a failed
    /// verdict rather than misclassifying its partial evidence.
    pub fn is_terminal(&self) -> bool {
        match self {
            IdentifyState::Found { .. }
            | IdentifyState::NotFoundAnywhere { .. }
            | IdentifyState::ManualOnly { .. }
            | IdentifyState::Failed { .. } => true,
            IdentifyState::Idle | IdentifyState::Triangulating { .. } => false,
        }
    }

    /// The badge list the UI renders: the disc ID, the barcode, and the
    /// catalog — three, whatever the candidate turned up. `Idle` has no
    /// toolbar.
    pub fn toolbar(&self) -> Vec<ToolbarSignal> {
        let Some(context) = self.context() else {
            return Vec::new();
        };
        vec![
            self.disc_badge(context),
            self.barcode_badge(context),
            self.catalog_badge(context),
        ]
    }

    /// State comes from the live `DiscidProgress` while triangulating, else from
    /// the context's settled results.
    fn disc_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let state = match self {
            IdentifyState::Triangulating { discid, .. } => discid_progress_state(discid),
            _ => settled_identity_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::DiscId,
            value: context.disc.signal.discid_value(),
            origin: SignalOrigin::DiscToc,
            state,
            excluded: context.disc.excluded,
            options: Vec::new(),
        }
    }

    /// The badge shows the matched code, or the first one when nothing has matched
    /// yet, and takes its origin from that code.
    fn barcode_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let code = context
            .barcode
            .matched
            .as_ref()
            .and_then(|v| context.barcode.codes.iter().find(|c| &c.value == v))
            .or_else(|| context.barcode.codes.first());
        let state = match self {
            IdentifyState::Triangulating { barcode, .. } => barcode_progress_state(barcode),
            _ => barcode_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Barcode,
            value: code.map(|c| c.value.clone()),
            origin: code.map_or(SignalOrigin::Artwork, |c| c.origin),
            state,
            excluded: context.barcode.excluded,
            options: Vec::new(),
        }
    }

    /// One badge for the catalog, whatever the candidate turned up: the first
    /// number chosen and how the chosen numbers' lookups went together, with
    /// every extracted number behind it as the list to choose from, each
    /// marked when it is chosen. Nothing chosen means nothing ran.
    fn catalog_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let first_chosen = context.catalog.chosen.first().and_then(|chosen| {
            context
                .catalog
                .numbers
                .iter()
                .find(|c| c.value == chosen.value)
        });
        let state = match self {
            IdentifyState::Triangulating { catalog, .. } => catalog_progress_state(catalog),
            _ => catalog_settled_state(context),
        };
        let mut options: Vec<SignalOption> = Vec::new();
        for number in &context.catalog.numbers {
            if options.iter().any(|option| option.value == number.value) {
                continue;
            }
            options.push(SignalOption {
                value: number.value.clone(),
                origin: number.origin,
                chosen: context.catalog.is_chosen(&number.value),
            });
        }
        ToolbarSignal {
            kind: SignalKind::Catalog,
            value: first_chosen.map(|c| c.value.clone()),
            origin: first_chosen.map_or(SignalOrigin::CueSheet, |c| c.origin),
            state,
            excluded: false,
            options,
        }
    }
}

/// One provider's answer to one lookup, with each match paired with its
/// library status.
pub type LookupOutcome = Result<LookupResults, LookupFailure>;

/// What feeds the reducer: the external triggers, plus the completions of the
/// lookup effects the service ran on the previous step.
#[derive(Debug, Clone)]
pub enum IdentifyEvent {
    /// Begin. Enters `Triangulating` and waits for the first `SignalsUpdated` —
    /// extraction owns scanning and OCR, not the reducer. `providers` is what
    /// this run asks: MusicBrainz, and Discogs when it is configured.
    ///
    /// `choices` is what the person decided this candidate's identification
    /// asks about, read from the candidate when the run started. It is the
    /// only way a choice enters a run: nothing changes one while the run is
    /// going, so a different decision is a different run.
    Started {
        providers: Vec<MetadataSource>,
        choices: LookupChoices,
    },
    Cancelled,

    /// The candidate's latest signals, and where the artwork pass feeding
    /// them has got to. The reducer dispatches the disc-ID lookup once the
    /// disc ID is `Computed` and the barcode lookups once the codes have
    /// `Settled`, and refreshes the catalog filter from every snapshot.
    /// Snapshots stream, so this is idempotent: each signal's progress guards
    /// its own lookup against being dispatched twice.
    SignalsUpdated {
        signals: Signals,
        artwork: ArtworkScan,
    },

    // ── DiscID lookup completion ────────────────────────────────────
    DiscidLookupCompleted {
        results: LookupResults,
        track_count: u32,
    },
    DiscidLookupFailed {
        failure: LookupFailure,
        track_count: u32,
    },

    /// One provider answered about one barcode: matches, none, or why not.
    /// The reducer moves that provider's walk on and leaves the others alone.
    BarcodeLookupAnswered {
        source: MetadataSource,
        for_barcode: String,
        outcome: LookupOutcome,
    },

    /// One provider answered about one chosen catalog number.
    CatalogLookupAnswered {
        source: MetadataSource,
        for_catalog: String,
        outcome: LookupOutcome,
    },
}

/// The side effects the service performs — the provider lookups, one per
/// provider. Each one finishing feeds an `IdentifyEvent` back into `step`.
/// Scanning, OCR, and disc-ID derivation belong to the extraction service, not
/// here.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    LookupDiscid {
        disc_id: String,
        track_count: u32,
    },
    LookupBarcode {
        source: MetadataSource,
        barcode: String,
    },
    LookupCatalog {
        source: MetadataSource,
        catalog: String,
    },
}

/// Drive the state machine one step. `Cancelled` always resets to `Idle`.
pub fn step(state: IdentifyState, event: IdentifyEvent) -> (IdentifyState, Vec<Effect>) {
    if matches!(event, IdentifyEvent::Cancelled) {
        return (IdentifyState::Idle, vec![]);
    }

    match (state, event) {
        (IdentifyState::Idle, IdentifyEvent::Started { providers, choices }) => {
            let context = SignalsContext::started(providers, choices);
            // The chosen numbers are the person's decision about this
            // candidate, not something read off a snapshot, so their lookups
            // go out with the run rather than waiting for extraction to offer
            // the numbers again. A number the settled snapshot no longer
            // offers loses its lookup then, in `apply_signals`.
            let mut effects = Vec::new();
            let catalog = start_catalog_progress(
                &context.catalog.chosen_values(),
                &context.providers,
                &mut effects,
            );
            (
                IdentifyState::Triangulating {
                    discid: DiscidProgress::Computing,
                    barcode: BarcodeProgress::Scanning,
                    catalog,
                    context,
                },
                effects,
            )
        }

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                context,
            },
            IdentifyEvent::SignalsUpdated { signals, artwork },
        ) => apply_signals(discid, barcode, catalog, context, signals, artwork),

        // ── DiscID lookup completion ───────────────────────────────────
        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::Computing | DiscidProgress::LookingUp,
                barcode,
                catalog,
                context,
            },
            IdentifyEvent::DiscidLookupFailed {
                failure,
                track_count,
            },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid: DiscidProgress::Failed {
                failure,
                track_count,
            },
            barcode,
            catalog,
            context,
        }),

        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::LookingUp,
                barcode,
                catalog,
                context,
            },
            IdentifyEvent::DiscidLookupCompleted {
                results,
                track_count,
            },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid: DiscidProgress::Done {
                results,
                track_count,
            },
            barcode,
            catalog,
            context,
        }),

        // ── One provider's barcode answer ──────────────────────────────
        // The provider's own walk moves on; the others are untouched. An
        // answer for a code the walk has already moved past is stale and
        // dropped.
        (
            IdentifyState::Triangulating {
                discid,
                barcode:
                    BarcodeProgress::Lookups {
                        codes,
                        mut providers,
                    },
                catalog,
                context,
            },
            IdentifyEvent::BarcodeLookupAnswered {
                source,
                for_barcode,
                outcome,
            },
        ) => {
            let mut effects = Vec::new();
            if let Some(provider) = providers.iter_mut().find(|p| p.source == source) {
                if let BarcodeLookupState::Trying { index } = provider.state {
                    if codes.get(index) == Some(&for_barcode) {
                        provider.state =
                            advance_barcode_walk(source, index, &codes, outcome, &mut effects);
                    }
                }
            }
            let next = IdentifyState::Triangulating {
                discid,
                barcode: BarcodeProgress::Lookups { codes, providers },
                catalog,
                context,
            };
            if effects.is_empty() {
                settle_if_ready(next)
            } else {
                (next, effects)
            }
        }

        // ── One provider's catalog answer ──────────────────────────────
        // The answer lands on the lookup of the number it was asked about. A
        // number the user has since taken out of the run has no lookup any
        // more, so its late answer lands nowhere.
        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Lookups { mut values },
                context,
            },
            IdentifyEvent::CatalogLookupAnswered {
                source,
                for_catalog,
                outcome,
            },
        ) => {
            let asked = values
                .iter_mut()
                .find(|lookup| lookup.value == for_catalog)
                .and_then(|lookup| {
                    lookup
                        .providers
                        .iter_mut()
                        .find(|l| l.source == source && l.state == LookupState::LookingUp)
                });
            if let Some(lookup) = asked {
                lookup.state = match outcome {
                    Ok(results) => LookupState::Done { results },
                    Err(failure) => LookupState::Failed { failure },
                };
            }
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Lookups { values },
                context,
            })
        }

        // An event Triangulating doesn't act on — a stale barcode response, say.
        (state @ IdentifyState::Triangulating { .. }, _) => (state, vec![]),

        // Any other (state, event) pair leaves the state alone.
        (state, _) => (state, vec![]),
    }
}

/// Where one provider's walk goes after answering about `codes[index]`: a
/// match ends it, a miss asks about the next code (or ends it exhausted), a
/// failure leaves it failed.
fn advance_barcode_walk(
    source: MetadataSource,
    index: usize,
    codes: &[String],
    outcome: LookupOutcome,
    effects: &mut Vec<Effect>,
) -> BarcodeLookupState {
    match outcome {
        Ok(results) if !results.is_empty() => BarcodeLookupState::Matched {
            code: codes[index].clone(),
            results,
        },
        Ok(_) => match codes.get(index + 1) {
            Some(next) => {
                effects.push(Effect::LookupBarcode {
                    source,
                    barcode: next.clone(),
                });
                BarcodeLookupState::Trying { index: index + 1 }
            }
            None => BarcodeLookupState::Exhausted,
        },
        Err(failure) => BarcodeLookupState::Failed { failure, index },
    }
}

/// Fold the latest snapshot into the two signals.
///
/// Idempotent under streaming, because each signal's own progress guards it: the
/// disc ID dispatches `LookupDiscid` only while still `Computing`, and the barcode
/// walks are started only while still `Scanning` *and* once the codes have
/// `Settled` — so every provider walks a complete, stable list. The catalog
/// filter refreshes from every snapshot.
fn apply_signals(
    discid: DiscidProgress,
    barcode: BarcodeProgress,
    catalog: CatalogProgress,
    mut context: SignalsContext,
    signals: Signals,
    artwork: ArtworkScan,
) -> (IdentifyState, Vec<Effect>) {
    let mut effects = Vec::new();
    context.refresh_inputs(&signals, artwork);

    let discid = match (discid, &signals.disc_id) {
        (DiscidProgress::Computing, signal) => start_discid_progress(
            signal,
            context.disc.excluded,
            &context.providers,
            &mut effects,
        ),
        // Past Computing: the lookup is in flight or settled.
        (discid, _) => discid,
    };

    let barcode = match (barcode, &signals.barcode) {
        (BarcodeProgress::Scanning, BarcodeSignal::Settled { .. }) => start_barcode_progress(
            context.barcode.code_values(),
            true,
            None,
            context.barcode.excluded,
            &context.providers,
            &mut effects,
        ),
        (BarcodeProgress::Scanning, BarcodeSignal::Absent) => BarcodeProgress::Skipped,
        (BarcodeProgress::Scanning, BarcodeSignal::Failed { failure, .. }) => {
            BarcodeProgress::ScanFailed {
                failure: failure.clone(),
            }
        }
        // Codes not settled yet, or already iterating/settled.
        (barcode, _) => barcode,
    };

    // A snapshot that no longer offers a chosen number drops the choice, so
    // the lookup waiting on it has nothing left to wait for.
    let catalog = catalog.keeping(|lookup| context.catalog.is_chosen(&lookup.value));

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        context,
    };

    // A dispatched lookup means nothing can have settled this step.
    if effects.is_empty() {
        settle_if_ready(next)
    } else {
        (next, effects)
    }
}

/// Once every signal has settled, record their results into the context and
/// combine into a terminal state. Until then, stay in `Triangulating`.
fn settle_if_ready(state: IdentifyState) -> (IdentifyState, Vec<Effect>) {
    let IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        mut context,
    } = state
    else {
        return (state, vec![]);
    };
    if !discid.is_settled() || !barcode.is_settled() || !catalog.is_settled() {
        return (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                context,
            },
            vec![],
        );
    }

    let track_count = settled_track_count(&discid);
    context.track_count = track_count;
    context.record_results(&discid, &barcode, &catalog);

    // The ledger this run showed, as its last frame showed it: the same
    // layout the driver has been publishing, with every lookup now settled.
    // It is computed here and nowhere else — every later reader shows what
    // was recorded rather than standing the pipes back up.
    let ledger = context
        .has_inputs()
        .then(|| run_view(&discid, &barcode, &catalog, &context));

    // Nothing had anything to run. Offer manual search rather than claim we
    // looked and found nothing.
    if matches!(discid, DiscidProgress::Skipped { .. })
        && matches!(barcode, BarcodeProgress::Skipped)
        && matches!(catalog, CatalogProgress::Skipped)
    {
        return (
            IdentifyState::ManualOnly {
                track_count,
                ledger,
                context,
            },
            vec![],
        );
    }

    (re_derive(context, ledger), vec![])
}

/// Re-combine over the non-excluded signals and lift the outcome into a state.
/// The one combine path: every way triangulation settles arrives here once the
/// results are in the context.
///
/// Both sides empty — because the lookups found nothing, or because the user
/// excluded the signals that did — lands on `NotFoundAnywhere`.
fn re_derive(context: SignalsContext, ledger: Option<IdentifyRunView>) -> IdentifyState {
    // Combine first, whatever failed: a provider that did not answer never
    // invalidates what the others found, and a failed state that hid those
    // matches would leave a person looking at an empty pane while one source
    // had the answer.
    let outcome = combine_results(
        context.disc.active_results(),
        context.barcode.active_results(),
        context.catalog.active_results(),
        &context.text,
    );
    let (matches, library_statuses, provenance, narrowed_out) = match outcome {
        CombineOutcome::Found {
            matches,
            library_statuses,
            provenance,
            narrowed_out,
        } => (matches, library_statuses, provenance, narrowed_out),
        CombineOutcome::NotFoundAnywhere => {
            (Vec::new(), Vec::new(), Vec::new(), NarrowedOut::default())
        }
    };
    let track_count = context.track_count;
    let failures = context.active_failures();
    if !failures.is_empty() {
        return IdentifyState::Failed {
            failures,
            matches,
            library_statuses,
            provenance,
            narrowed_out,
            track_count,
            ledger,
            context,
        };
    }
    if matches.is_empty() {
        return IdentifyState::NotFoundAnywhere { ledger, context };
    }
    IdentifyState::Found {
        matches,
        library_statuses,
        track_count,
        provenance,
        narrowed_out,
        ledger,
        context,
    }
}

mod context;
mod progress;

pub use context::{
    BarcodeEvidence, CatalogEvidence, ChosenCatalog, DiscIdEvidence, SignalsContext,
};
use progress::{
    barcode_progress_state, barcode_settled_state, catalog_progress_state, catalog_settled_state,
    discid_progress_state, settled_identity_state, settled_track_count, start_barcode_progress,
    start_catalog_progress, start_discid_progress,
};
pub use progress::{
    BarcodeLookupState, BarcodeProgress, CatalogLookup, CatalogProgress, DiscidProgress,
    LookupResults, LookupState, ProviderBarcodeLookup, ProviderLookup,
};

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

/// `re_derive` for tests in sibling modules: the one path that turns a settled
/// context into a terminal state, so a test can build the state a real run
/// would reach rather than hand-assembling one. It records no ledger — a test
/// that wants one drives the pipes through [`step`].
#[cfg(test)]
pub(crate) fn re_derive_for_tests(context: SignalsContext) -> IdentifyState {
    re_derive(context, None)
}

/// The terminal state these settled pipes land on, ledger and all — the one
/// path a run ends by, for tests in sibling modules that want the run
/// recorded as a real one would record it.
#[cfg(test)]
pub(crate) fn settle_for_tests(
    discid: DiscidProgress,
    barcode: BarcodeProgress,
    catalog: CatalogProgress,
    context: SignalsContext,
) -> IdentifyState {
    settle_if_ready(IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        context,
    })
    .0
}
