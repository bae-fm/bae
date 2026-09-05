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
//! `step` takes a state and an event and returns the next state plus the side
//! effects for the service to run. No I/O, no async, nothing outside itself.

use super::combine::{combine_results, CombineOutcome, ResultProvenance};
use super::toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
use crate::db::LibraryStatus;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::MetadataSource;
use crate::signals::{ArtworkScan, BarcodeSignal, LookupFailure, SignalOrigin, Signals};

/// A signal the user acted on in the toolbar. The disc ID and barcode are
/// checked by default and toggle off; the catalog is off until one of the
/// extracted numbers is chosen, and choosing another replaces it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SignalToggle {
    Disc,
    Barcode,
    Catalog(String),
}

/// One candidate's identify state.
///
/// Every state but `Idle` carries a [`SignalsContext`], so the toolbar projection
/// always has its signal values and the user can toggle or re-run from any settled
/// state.
#[derive(Clone, Debug, PartialEq)]
pub enum IdentifyState {
    Idle,

    /// Lookups in flight. Each signal progresses independently; `settle_if_ready`
    /// combines them into a terminal state once all three are settled. The
    /// catalog pipe rests at `Skipped` until a number is chosen — which is also
    /// what puts a settled state back here, with the other two standing back up
    /// from their stored results rather than running again.
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
        provenance: Vec<ResultProvenance>,
        context: SignalsContext,
    },

    NotFoundAnywhere {
        context: SignalsContext,
    },

    /// Nothing to look up: no disc-ID artifact (LOG/CUE) and no barcode source
    /// (artwork, CUE `CATALOG`). Distinct from `NotFoundAnywhere`, where signals
    /// ran and matched nothing — here none ran, so the UI offers manual search.
    ManualOnly {
        track_count: u32,
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
        provenance: Vec<ResultProvenance>,
        track_count: u32,
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
            | IdentifyState::NotFoundAnywhere { context }
            | IdentifyState::ManualOnly { context, .. }
            | IdentifyState::Failed { context, .. } => Some(context),
            IdentifyState::Idle => None,
        }
    }

    /// Whether the machine has stopped moving on its own: nothing is in flight,
    /// so only the user (a toggle, a re-run) can change it now.
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
            value: context.disc_id.discid_value(),
            origin: SignalOrigin::DiscToc,
            state,
            excluded: context.disc_excluded,
            options: Vec::new(),
        }
    }

    /// The badge shows the matched code, or the first one when nothing has matched
    /// yet, and takes its origin from that code.
    fn barcode_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let code = context
            .matched_barcode
            .as_ref()
            .and_then(|v| context.barcode_codes.iter().find(|c| &c.value == v))
            .or_else(|| context.barcode_codes.first());
        let state = match self {
            IdentifyState::Triangulating { barcode, .. } => barcode_progress_state(barcode),
            _ => barcode_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Barcode,
            value: code.map(|c| c.value.clone()),
            origin: code.map_or(SignalOrigin::Artwork, |c| c.origin),
            state,
            excluded: context.barcode_excluded,
            options: Vec::new(),
        }
    }

    /// One badge for the catalog, whatever the candidate turned up: the chosen
    /// number and how its lookup went, with every extracted number behind it as
    /// the list to choose from. Nothing chosen means nothing ran.
    fn catalog_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let chosen = context
            .chosen_catalog
            .as_ref()
            .and_then(|v| context.catalogs.iter().find(|c| &c.value == v));
        let state = match self {
            IdentifyState::Triangulating { catalog, .. } => catalog_progress_state(catalog),
            _ => catalog_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Catalog,
            value: chosen.map(|c| c.value.clone()),
            origin: chosen.map_or(SignalOrigin::CueSheet, |c| c.origin),
            state,
            excluded: false,
            options: context
                .catalogs
                .iter()
                .map(|c| SignalOption {
                    value: c.value.clone(),
                    origin: c.origin,
                    chosen: context.chosen_catalog.as_deref() == Some(c.value.as_str()),
                })
                .collect(),
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
    Started {
        providers: Vec<MetadataSource>,
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

    /// One provider answered about the chosen catalog number.
    CatalogLookupAnswered {
        source: MetadataSource,
        for_catalog: String,
        outcome: LookupOutcome,
    },

    /// The user checked or unchecked a signal. Unchecking the disc ID or the
    /// barcode re-combines over the rest; choosing a catalog number runs its
    /// lookup, and choosing the one already chosen clears it.
    SignalToggled {
        signal: SignalToggle,
    },

    /// The user asked to replay the lookups. The reducer resets to `Triangulating`
    /// and re-dispatches from the retained signals, keeping what the user
    /// checked. `providers` is re-read, so a provider configured since the last
    /// run joins it.
    ReRun {
        providers: Vec<MetadataSource>,
    },

    /// The user asked to re-ask only what failed. Every lookup that answered
    /// keeps its answer; a failed provider walks the barcodes again from the
    /// first, a failed catalog lookup is asked again, and a failed disc-ID
    /// lookup runs again when there is a disc ID to ask about.
    RetryFailed,
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

    // Toggle, re-run and retry all act on the carried `SignalsContext`, so
    // they're handled once here rather than per state. `ReRun` is ignored
    // during triangulation — those lookups are already in flight.
    match event {
        IdentifyEvent::SignalToggled { signal } if state.context().is_some() => {
            return apply_toggle(state, signal);
        }
        IdentifyEvent::ReRun { providers }
            if !matches!(state, IdentifyState::Triangulating { .. }) =>
        {
            if let Some(context) = state.context() {
                return rerun(context.clone(), providers);
            }
            return (state, vec![]);
        }
        IdentifyEvent::RetryFailed => return retry_failed(state),
        _ => {}
    }

    match (state, event) {
        (IdentifyState::Idle, IdentifyEvent::Started { providers }) => (
            IdentifyState::Triangulating {
                discid: DiscidProgress::Computing,
                barcode: BarcodeProgress::Scanning,
                catalog: CatalogProgress::Skipped,
                context: SignalsContext::empty(providers),
            },
            vec![],
        ),

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
        // The `for_catalog` guard drops a response for a number the user has
        // already moved off.
        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Lookups { mut lookups },
                context,
            },
            IdentifyEvent::CatalogLookupAnswered {
                source,
                for_catalog,
                outcome,
            },
        ) => {
            if context.chosen_catalog.as_deref() == Some(for_catalog.as_str()) {
                if let Some(lookup) = lookups
                    .iter_mut()
                    .find(|l| l.source == source && l.state == LookupState::LookingUp)
                {
                    lookup.state = match outcome {
                        Ok(results) => LookupState::Done { results },
                        Err(failure) => LookupState::Failed { failure },
                    };
                }
            }
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Lookups { lookups },
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
            code: Some(codes[index].clone()),
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
        Err(failure) => BarcodeLookupState::Failed { failure },
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
        (DiscidProgress::Computing, signal) => start_discid_progress(signal, &mut effects),
        // Past Computing: the lookup is in flight or settled.
        (discid, _) => discid,
    };

    let barcode = match (barcode, &signals.barcode) {
        (BarcodeProgress::Scanning, BarcodeSignal::Settled { codes }) => {
            start_barcode_progress(codes, true, None, &context.providers, &mut effects)
        }
        (BarcodeProgress::Scanning, BarcodeSignal::Absent) => BarcodeProgress::Skipped,
        (BarcodeProgress::Scanning, BarcodeSignal::Failed { failure, .. }) => {
            BarcodeProgress::ScanFailed {
                failure: failure.clone(),
            }
        }
        // Codes not settled yet, or already iterating/settled.
        (barcode, _) => barcode,
    };

    // A snapshot that no longer offers the chosen number clears the choice, so
    // the pipe waiting on it has nothing left to wait for.
    let catalog = if context.chosen_catalog.is_none() {
        CatalogProgress::Skipped
    } else {
        catalog
    };

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

    // Nothing had anything to run. Offer manual search rather than claim we
    // looked and found nothing.
    if matches!(discid, DiscidProgress::Skipped { .. })
        && matches!(barcode, BarcodeProgress::Skipped)
        && matches!(catalog, CatalogProgress::Skipped)
    {
        return (
            IdentifyState::ManualOnly {
                track_count,
                context,
            },
            vec![],
        );
    }

    (re_derive(context), vec![])
}

/// Apply a toolbar toggle.
///
/// Unchecking the disc ID or the barcode changes nothing that has to be
/// fetched, so it re-combines in place. Choosing a catalog number does: its
/// lookup has to run, which puts the state back in `Triangulating` with the
/// other two signals standing back up from what they already found.
///
/// Mid-`Triangulating` an exclusion is only recorded — the in-flight lookups
/// keep running, and the settle applies it.
fn apply_toggle(state: IdentifyState, signal: SignalToggle) -> (IdentifyState, Vec<Effect>) {
    let SignalToggle::Catalog(value) = signal else {
        let exclude_disc = matches!(signal, SignalToggle::Disc);
        return match state {
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                mut context,
            } => {
                toggle_exclusion(&mut context, exclude_disc);
                (
                    IdentifyState::Triangulating {
                        discid,
                        barcode,
                        catalog,
                        context,
                    },
                    vec![],
                )
            }
            IdentifyState::Found { mut context, .. }
            | IdentifyState::NotFoundAnywhere { mut context }
            | IdentifyState::ManualOnly { mut context, .. }
            | IdentifyState::Failed { mut context, .. } => {
                toggle_exclusion(&mut context, exclude_disc);
                (re_derive(context), vec![])
            }
            IdentifyState::Idle => (IdentifyState::Idle, vec![]),
        };
    };
    choose_catalog(state, value)
}

fn toggle_exclusion(context: &mut SignalsContext, disc: bool) {
    if disc {
        context.disc_excluded = !context.disc_excluded;
    } else {
        context.barcode_excluded = !context.barcode_excluded;
    }
}

/// The pipes a state carries into `Triangulating` when a new lookup starts
/// from it: the live ones mid-run, else the settled ones stood back up from
/// the context.
fn pipes_of(state: IdentifyState) -> Option<(DiscidProgress, BarcodeProgress, SignalsContext)> {
    match state {
        IdentifyState::Triangulating {
            discid,
            barcode,
            context,
            ..
        } => Some((discid, barcode, context)),
        IdentifyState::Found { context, .. }
        | IdentifyState::NotFoundAnywhere { context }
        | IdentifyState::ManualOnly { context, .. }
        | IdentifyState::Failed { context, .. } => Some((
            settled_discid_progress(&context),
            settled_barcode_progress(&context),
            context,
        )),
        IdentifyState::Idle => None,
    }
}

/// Check one extracted catalog number, or uncheck the one already checked.
/// At most one is ever chosen: choosing another replaces it.
fn choose_catalog(state: IdentifyState, value: String) -> (IdentifyState, Vec<Effect>) {
    let Some((discid, barcode, mut context)) = pipes_of(state) else {
        return (IdentifyState::Idle, vec![]);
    };

    context.clear_catalog_lookup();
    if context.chosen_catalog.as_deref() == Some(value.as_str()) {
        context.chosen_catalog = None;
        return settle_if_ready(IdentifyState::Triangulating {
            discid,
            barcode,
            catalog: CatalogProgress::Skipped,
            context,
        });
    }

    context.chosen_catalog = Some(value.clone());
    let mut effects = Vec::new();
    let catalog = start_catalog_progress(&value, &context.providers, &mut effects);
    (
        IdentifyState::Triangulating {
            discid,
            barcode,
            catalog,
            context,
        },
        effects,
    )
}

/// Re-ask exactly the lookups that failed, keeping every answer that landed.
/// Mid-run the failed walks restart in place; from a settled state the pipes
/// stand back up from the context first. A state with nothing failed is left
/// as it is.
fn retry_failed(state: IdentifyState) -> (IdentifyState, Vec<Effect>) {
    let catalog = match &state {
        IdentifyState::Triangulating { catalog, .. } => catalog.clone(),
        IdentifyState::Idle => return (IdentifyState::Idle, vec![]),
        settled => settled
            .context()
            .map(settled_catalog_progress)
            .unwrap_or(CatalogProgress::Skipped),
    };
    let Some((mut discid, mut barcode, context)) = pipes_of(state) else {
        return (IdentifyState::Idle, vec![]);
    };
    let mut catalog = catalog;

    let mut effects = Vec::new();
    retry_failed_discid_lookup(&mut discid, &context.disc_id, &mut effects);
    retry_failed_barcode_lookups(&mut barcode, &mut effects);
    if let Some(chosen) = &context.chosen_catalog {
        retry_failed_catalog_lookups(&mut catalog, chosen, &mut effects);
    }

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        context,
    };
    if effects.is_empty() {
        settle_if_ready(next)
    } else {
        (next, effects)
    }
}

/// Re-combine over the non-excluded signals and lift the outcome into a state. The
/// one combine path: triangulation settle, signal toggle, and re-run completion all
/// arrive here once the results are in the context.
///
/// Both sides empty — because the lookups found nothing, or because the user
/// excluded the signals that did — lands on `NotFoundAnywhere`.
fn re_derive(context: SignalsContext) -> IdentifyState {
    // Combine first, whatever failed: a provider that did not answer never
    // invalidates what the others found, and a failed state that hid those
    // matches would leave a person looking at an empty pane while one source
    // had the answer.
    let outcome = combine_results(
        context.active_discid_results(),
        context.active_barcode_results(),
        context.active_catalog_results(),
    );
    let (matches, library_statuses, provenance) = match outcome {
        CombineOutcome::Found {
            matches,
            library_statuses,
            provenance,
        } => (matches, library_statuses, provenance),
        CombineOutcome::NotFoundAnywhere => (Vec::new(), Vec::new(), Vec::new()),
    };
    let track_count = context.track_count;
    let failures = context.active_failures();
    if !failures.is_empty() {
        return IdentifyState::Failed {
            failures,
            matches,
            library_statuses,
            provenance,
            track_count,
            context,
        };
    }
    if matches.is_empty() {
        return IdentifyState::NotFoundAnywhere { context };
    }
    IdentifyState::Found {
        matches,
        library_statuses,
        track_count,
        provenance,
        context,
    }
}

/// Reset to `Triangulating` and re-dispatch the lookups from the retained
/// signals, keeping what the user checked.
fn rerun(
    mut context: SignalsContext,
    providers: Vec<MetadataSource>,
) -> (IdentifyState, Vec<Effect>) {
    // The prior results go; the new lookups replace them as they land.
    context.providers = providers;
    context.discid_results = Vec::new();
    context.barcode_results = Vec::new();
    context.discid_failure = None;
    context.barcode_failures.clear();
    context.matched_barcode = None;
    context.clear_catalog_lookup();

    let mut effects = Vec::new();

    let discid = start_discid_progress(&context.disc_id, &mut effects);
    let barcode = start_barcode_progress(
        &context.barcode_codes,
        context.had_barcode_source,
        context.barcode_scan_failure.as_ref(),
        &context.providers,
        &mut effects,
    );
    let catalog = match &context.chosen_catalog {
        Some(value) => start_catalog_progress(value, &context.providers, &mut effects),
        None => CatalogProgress::Skipped,
    };

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        context,
    };
    if effects.is_empty() {
        settle_if_ready(next)
    } else {
        (next, effects)
    }
}

mod context;
mod progress;

pub use context::SignalsContext;
use progress::{
    barcode_progress_state, barcode_settled_state, catalog_progress_state, catalog_settled_state,
    discid_progress_state, retry_failed_barcode_lookups, retry_failed_catalog_lookups,
    retry_failed_discid_lookup, settled_barcode_progress, settled_catalog_progress,
    settled_discid_progress, settled_identity_state, settled_track_count, start_barcode_progress,
    start_catalog_progress, start_discid_progress,
};
pub use progress::{
    BarcodeLookupState, BarcodeProgress, CatalogProgress, DiscidProgress, LookupResults,
    LookupState, ProviderBarcodeLookup, ProviderLookup,
};

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

/// `re_derive` for tests in sibling modules: the one path that turns a settled
/// context into a terminal state, so a test can build the state a real run
/// would reach rather than hand-assembling one.
#[cfg(test)]
pub(crate) fn re_derive_for_tests(context: SignalsContext) -> IdentifyState {
    re_derive(context)
}
